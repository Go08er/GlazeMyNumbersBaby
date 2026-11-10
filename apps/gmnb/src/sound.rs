//! Theme sounds in the app (docs/theming-media.md, "From event to sound",
//! "Settings", "Environment hooks").
//!
//! [`Voice`] is the app's one sound engine and what it knows of the theme
//! loaded: the theme's sounds (a [`Bank`]) and who chooses among them (a
//! [`Resolver`]). It hears each event the layers are told of, in window
//! coordinates (under reduced motion too), and turns it into plays. It is
//! plain data: the hub (`crate::theme::Hub`) holds it and says when the
//! window is in use; the tests below drive it without a window.
//!
//! **Nothing exists in the base look.** The engine makes its thread, and
//! connects to the sound server, only when a theme with sounds (or a
//! `sound` function) is loaded while *Theme sounds* is on and the desktop
//! allows event sounds. The base look, a theme without sounds, the switch
//! off or the desktop's event sounds off: no thread, no socket.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gmnb_sound::{
    Bank, EventName, Play, Played, Resolver, Settings, Sink, SoundEngine, Stats, Status, Trigger,
};
use gmnb_theme::events::{Event, EventKind};
use gmnb_theme::sound::{answers, decide};
use gmnb_theme::theme::Theme;
use gmnb_theme::{Ctx, Note};

/// The least time the window has been on screen before anything sounds:
/// nothing sounds while the app starts or restores what it was showing.
pub const SETTLING: Duration = Duration::from_millis(500);

/// What a theme with sounds holds while it is loaded.
struct Loaded {
    /// Its name, for Settings.
    name: String,
    bank: Bank,
    resolver: Resolver,
    /// The sounds it had made the last time the engine was told: a
    /// `sound` function may make more.
    declared: usize,
}

/// Why an event made no sound, or what it played.
#[derive(Debug, PartialEq)]
pub enum Heard {
    /// Sound does not answer it: not a press, result, error, mode or
    /// option, or operated through the accessibility interface.
    NotAnswered,
    /// No theme with sounds, sound switched off, or no sound system.
    Silent(&'static str),
    /// The theme's `sound` said `false`, or its table has nothing (or
    /// `false`) for the event.
    Nothing(Option<EventName>),
    /// The same name sounded less than 30 ms ago.
    TooSoon(Option<EventName>),
    /// These plays, and what the engine made of each.
    Played(Option<EventName>, Vec<(Play, Played)>),
}

/// What the theme's `sound` function did for one event, besides what it
/// chose.
#[derive(Debug, Default, PartialEq)]
pub struct Asked {
    /// It failed (or ran out of time): why.
    pub failed: Option<String>,
    /// It has been switched off, by this failure.
    pub switched_off: bool,
}

/// The app's theme sounds.
pub struct Voice {
    engine: SoundEngine,
    loaded: Option<Loaded>,
    /// The theme loaded has no sounds and no `sound` function (or there
    /// is none): Settings says so.
    none: bool,
    /// Settings' switch, and the desktop's event sounds.
    switch: bool,
    desktop: bool,
    /// Sound is off for this run whatever Settings says
    /// (`GMNB_SOUND=0`), or for a screenshot.
    run_off: Option<&'static str>,
    seed: u32,
}

impl Voice {
    /// Creates nothing: no thread, no socket.
    pub fn new(settings: Settings, switch: bool, desktop: bool) -> Voice {
        // The tests never reach the session's sound server: they name a
        // socket of their own, or the null sink.
        #[cfg(test)]
        assert_ne!(
            settings.sink,
            Sink::Session,
            "a test asked for the session's sound server"
        );
        let run_off = (settings.sink == Sink::Off).then_some("GMNB_SOUND=0");
        let settings = Settings {
            enabled: switch && desktop,
            ..settings
        };
        Voice {
            engine: SoundEngine::new(settings),
            loaded: None,
            none: true,
            switch,
            desktop,
            run_off,
            seed: seed(),
        }
    }

    /// Off for this run for another reason than `GMNB_SOUND=0` (a
    /// screenshot): what Settings shows is what it would be.
    pub fn quiet_run(mut self) -> Voice {
        self.run_off = None;
        self
    }

    /// The theme was loaded (`None`: the base look, or it was unloaded).
    /// What was loaded before is let go: what is sounding fades out. A
    /// theme with sounds is handed to the engine, which renders them on
    /// its thread (and connects to the sound server) if sound is on.
    ///
    /// Returns what the theme's author should know of its sounds so far.
    pub fn load(&mut self, theme: Option<&Theme>) -> Vec<Note> {
        self.loaded = None;
        let Some(theme) = theme else {
            self.none = true;
            self.engine.unload();
            return Vec::new();
        };
        let notes = theme.take_sound_notes();
        let sounds = theme.sounds();
        self.none = !sounds.any();
        if self.none {
            self.engine.unload();
            return notes;
        }
        let bank = self.engine.load(&sounds);
        self.seed = self.seed.wrapping_mul(0x9e37_79b9).wrapping_add(1) | 1;
        self.loaded = Some(Loaded {
            name: theme.def.meta.name.clone(),
            bank,
            resolver: Resolver::new(sounds.table, self.seed),
            declared: sounds.sounds.len(),
        });
        notes
    }

    /// Whether events are wanted: a theme with sounds is loaded and sound
    /// is on.
    pub fn listening(&self) -> bool {
        self.loaded.is_some() && self.engine.is_enabled() && self.run_off.is_none()
    }

    /// Settings' switch.
    pub fn set_switch(&mut self, on: bool) {
        self.switch = on;
        self.engine.set_enabled(self.switch && self.desktop);
    }

    pub fn switch(&self) -> bool {
        self.switch
    }

    /// The desktop's event sounds (`gtk-enable-event-sounds`).
    pub fn set_desktop(&mut self, on: bool) {
        self.desktop = on;
        self.engine.set_enabled(self.switch && self.desktop);
    }

    /// The Settings volume, 0 to 1.
    pub fn set_volume(&mut self, volume: f32) {
        self.engine.set_volume(volume);
    }

    /// One event, in window coordinates, from a window `width` wide, at
    /// `now`: what it plays ("From event to sound"). The caller has
    /// checked that the window is in use and has been on screen for
    /// [`SETTLING`].
    pub fn hear(
        &mut self,
        theme: Option<&Theme>,
        ctx: &Ctx,
        event: &Event,
        width: f32,
        now: Instant,
    ) -> (Heard, Asked) {
        let mut asked = Asked::default();
        if !answers(event) {
            return (Heard::NotAnswered, asked);
        }
        let (Some(loaded), Some(theme)) = (self.loaded.as_mut(), theme) else {
            return (Heard::Silent("the theme has no sounds"), asked);
        };
        if self.run_off.is_some() || !self.engine.is_enabled() {
            return (Heard::Silent("sound is off"), asked);
        }
        if !self.engine.available() {
            return (Heard::Silent("no sound system"), asked);
        }
        // The theme's own choice first, then its table.
        let decided = decide(theme, &mut loaded.resolver, event, width, ctx);
        asked.failed = decided.failed.as_ref().map(ToString::to_string);
        asked.switched_off = decided.switched_off;
        // A sound it made just now joins the bank (it plays once it is
        // rendered: this time it may not).
        let declared = theme.sounds_declared();
        if declared > loaded.declared {
            self.engine.extend(&loaded.bank, &theme.sounds().sounds);
            loaded.declared = declared;
        }
        let (name, plays) = (decided.name, decided.plays);
        if plays.is_empty() {
            return (Heard::Nothing(name), asked);
        }
        if loaded.resolver.too_soon(name, now) {
            return (Heard::TooSoon(name), asked);
        }
        let played = plays
            .into_iter()
            .map(|p| (p, self.engine.play(&loaded.bank, p.sound, p.params)))
            .collect();
        (Heard::Played(name, played), asked)
    }

    /// Settings' volume was let go: the theme's `digit` sound, once.
    pub fn preview(&mut self) -> Option<Played> {
        let loaded = self.loaded.as_mut()?;
        if self.run_off.is_some() || !self.engine.available() {
            return None;
        }
        let play = loaded
            .resolver
            .resolve(EventName::Digit, Trigger::default())?;
        Some(self.engine.play(&loaded.bank, play.sound, play.params))
    }

    /// What Settings says under *Theme sounds*.
    pub fn subtitle(&self) -> String {
        if let Some(why) = self.run_off {
            return format!("Turned off for this run ({why})");
        }
        if !self.desktop {
            return "Turned off by the desktop's sound settings".into();
        }
        let Some(loaded) = &self.loaded else {
            return "This theme has no sounds".into();
        };
        if self.engine.status() == Status::Unavailable {
            return "No sound system found".into();
        }
        format!("‘{}’ has sounds", loaded.name)
    }

    /// What the engine has done (`GMNB_THEME_STATS`, the tests).
    pub fn stats(&self) -> Stats {
        self.engine.stats()
    }

    pub fn status(&self) -> Status {
        self.engine.status()
    }

    /// Whether the engine's thread exists.
    pub fn has_thread(&self) -> bool {
        self.engine.has_thread()
    }

    /// What the engine found wrong with the theme's sounds as it made
    /// them, since the last call.
    pub fn take_notes(&self) -> Vec<Note> {
        let Some(loaded) = &self.loaded else {
            return Vec::new();
        };
        loaded
            .bank
            .take_notes()
            .into_iter()
            .map(|(id, message)| Note {
                path: format!("sound[{}]", id.0),
                message,
            })
            .collect()
    }

    /// The bank of the theme loaded, while there is one (the tests).
    #[cfg(test)]
    fn bank(&self) -> Option<&Bank> {
        self.loaded.as_ref().map(|l| &l.bank)
    }
}

/// A seed for the theme's random choices.
fn seed() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    nanos ^ std::process::id().rotate_left(16) | 1
}

/// The engine's settings, as the app makes them: the session's sound
/// server, unless the environment says otherwise (`GMNB_SOUND=0` or
/// `null`, `GMNB_SOUND_DUMP`); nothing under `GMNB_SCREENSHOT`. Each
/// change of the engine's state is told to the hub, on the main thread
/// (Settings follows it).
pub fn engine_settings(volume: f32) -> Settings {
    let mut settings = Settings::from_env(true, volume);
    if std::env::var_os("GMNB_SCREENSHOT").is_some() {
        settings.sink = Sink::Off;
    }
    settings.on_status = Some(Arc::new(|_| {
        // From the sound thread: once per change of state, never at rest.
        gtk::glib::MainContext::default().invoke(|| {
            if let Some(hub) = crate::theme::Hub::current() {
                hub.sound_changed();
            }
        });
    }));
    settings
}

/// `GMNB_SOUND_TRACE`: a line on stderr for each event sound hears.
pub fn trace_line(event: &Event, heard: &Heard, asked: &Asked, took: Duration) -> String {
    let mut what = event.kind.name().to_string();
    if let Some(via) = event.via {
        what = format!("{what} ({})", via.name());
    }
    if let Some(cause) = event.cause {
        what = format!("{what} after {}", cause.name());
    }
    if event.kind == EventKind::Press
        && let Some((id, _)) = event.key
    {
        what = format!("{what} key {}", id.map_or("-".into(), |id| id.to_string()));
    }
    let name = |n: &Option<EventName>| n.map_or("-", EventName::name);
    let outcome = match heard {
        Heard::NotAnswered => "not answered".to_string(),
        Heard::Silent(why) => format!("silent: {why}"),
        Heard::Nothing(n) => format!("{}: nothing", name(n)),
        Heard::TooSoon(n) => format!("{}: too soon", name(n)),
        Heard::Played(n, plays) => {
            let plays: Vec<String> = plays
                .iter()
                .map(|(p, played)| {
                    let q = p.params;
                    format!(
                        "#{} volume {:.2} rate {:.3} pan {:.2} delay {:.2} {played:?}",
                        p.sound.0, q.volume, q.rate, q.pan, q.delay
                    )
                })
                .collect();
            format!("{}: {}", name(n), plays.join(", "))
        }
    };
    let failed = asked
        .failed
        .as_ref()
        .map_or(String::new(), |e| format!(" (sound() failed: {e})"));
    format!(
        "sound: {what}: {outcome}{failed} [{:.3} ms]",
        took.as_secs_f64() * 1e3
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmnb_sound::standin::Server;
    use gmnb_sound::{SoundId, SoundState};
    use gmnb_theme::Limits;
    use gmnb_theme::events::{Action, Cause, KeyKind, Page, Source as From, Via};
    use gmnb_theme::theme::Source;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A folder of this test's own, gone when it is dropped.
    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Dir {
            static N: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "gmnb-sound-{}-{name}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Dir(dir)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Sounds for each kind of event, each told apart by its order:
    /// #1 the keys, #2 equals, #3 a result, #4 an error, #5 a change of
    /// mode, #6 a function key, #7 clear, #8 a memory button.
    const SOUNDS: &str = r#"
local function tone(note, decay)
	return gmnb.synth({ gain = 0.4, layers = { { freq = note, env = { attack = 0.001, decay = decay } } } })
end
local key, equals, result, err, mode = tone("A5", 0.03), tone("C5", 0.05), tone("E6", 0.2), tone("A2", 0.1), tone("D4", 0.1)
local func, clear, memory = tone("B5", 0.03), tone("F5", 0.04), tone("G5", 0.04)
return {
	api = 1, name = "Tones",
	sounds = {
		key = { sound = key, pick = "key", rate = { 0.9, 1.1 }, pan = "position" },
		equals = equals,
		result = { sound = result, delay = 0.05 },
		error = err,
		mode = mode,
		func = func,
		clear = clear,
		memory = memory,
		button = false,
	},
}"#;

    fn theme_in(dir: &Path, source: &str) -> Theme {
        let folder = dir.join("theme");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("theme.luau"), source).unwrap();
        let limits = Limits {
            load: Duration::from_secs(20),
            ..Limits::default()
        };
        Theme::load(Source::folder("t", &folder), limits).unwrap_or_else(|e| panic!("{e}"))
    }

    fn null() -> Settings {
        Settings {
            sink: Sink::Null,
            log: Some(Arc::new(|_| {})),
            ..Settings::default()
        }
    }

    /// The engine's settings for a stand-in server at `socket`.
    fn at(socket: &Path) -> Settings {
        Settings {
            sink: Sink::Socket {
                path: socket.to_path_buf(),
                cookie: vec![0; 256],
            },
            log: Some(Arc::new(|_| {})),
            ..Settings::default()
        }
    }

    fn until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + limit;
        while !done() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        true
    }

    /// Waits for the theme's sounds to be made and the output ready.
    fn ready(voice: &Voice) {
        let bank = voice.bank().expect("a bank");
        assert!(bank.wait_settled(Duration::from_secs(20)));
        assert!(until(Duration::from_secs(10), || voice.status() == Status::Ready));
    }

    fn key(id: u32, kind: KeyKind, via: Via, x: f32) -> Event {
        let mut e = Event::at(
            EventKind::Press,
            Page::Calculator,
            [x - 20.0, 300.0, 40.0, 40.0],
        );
        e.x = x;
        e.source = Some(From::Key);
        e.via = Some(via);
        e.key = Some((Some(id), kind));
        e
    }

    /// An event of `kind` from nowhere in particular; a result or an
    /// error as after = typed.
    fn event(kind: EventKind, page: Page) -> Event {
        let mut e = Event::at(kind, page, [0.0, 0.0, 360.0, 600.0]);
        if matches!(kind, EventKind::Result | EventKind::Error) {
            (e.cause, e.via) = (Some(Cause::Equals), Some(Via::Keyboard));
        }
        e
    }

    /// The sounds an event played.
    fn sounds(heard: &Heard) -> Vec<u32> {
        match heard {
            Heard::Played(_, plays) => plays
                .iter()
                .filter(|(_, played)| *played == Played::Queued)
                .map(|(p, _)| p.sound.0)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The base look, a theme without sounds, the switch off and the
    /// desktop's event sounds off create nothing: no thread, and no
    /// connection to the (stand-in) sound server. A theme with sounds
    /// connects once.
    #[test]
    fn nothing_is_created_unless_a_theme_with_sounds_is_on() {
        let dir = Dir::new("gates");
        let socket = dir.0.join("native");
        let server = Server::start(&socket);
        let plain = theme_in(&dir.0.join("plain"), "return { api = 1, name = \"Plain\" }");
        let tones = theme_in(&dir.0.join("tones"), SOUNDS);
        let ctx = Ctx::default();
        let press = key(137, KeyKind::Number, Via::Pointer, 100.0);
        let now = Instant::now() + Duration::from_secs(1);

        let mut voice = Voice::new(at(&socket), true, true);
        // The base look.
        voice.load(None);
        let heard = voice.hear(None, &ctx, &press, 360.0, now).0;
        assert_eq!(heard, Heard::Silent("the theme has no sounds"));
        assert_eq!(voice.subtitle(), "This theme has no sounds");
        // A theme without sounds.
        voice.load(Some(&plain));
        assert_eq!(voice.subtitle(), "This theme has no sounds");
        assert!(!voice.listening());
        std::thread::sleep(Duration::from_millis(100));
        assert!(!voice.has_thread());
        assert_eq!((voice.stats().threads, voice.stats().connects), (0, 0));
        assert_eq!(server.connections(), 0);

        drop(voice);

        // The switch off; the desktop's event sounds off: a theme with
        // sounds is loaded, and still nothing.
        let mut switched_off = Voice::new(at(&socket), false, true);
        switched_off.load(Some(&tones));
        let mut desktop_off = Voice::new(at(&socket), true, false);
        desktop_off.load(Some(&tones));
        assert_eq!(
            desktop_off.subtitle(),
            "Turned off by the desktop's sound settings"
        );
        for voice in [&mut switched_off, &mut desktop_off] {
            assert!(!voice.listening());
            let heard = voice.hear(Some(&tones), &ctx, &press, 360.0, now).0;
            assert_eq!(heard, Heard::Silent("sound is off"));
            assert_eq!(voice.preview(), None);
        }
        std::thread::sleep(Duration::from_millis(100));
        for voice in [&switched_off, &desktop_off] {
            assert!(!voice.has_thread());
            assert_eq!((voice.stats().threads, voice.stats().connects), (0, 0));
        }
        assert_eq!(server.connections(), 0);

        // Turned on: one thread, one connection, and the theme's sounds.
        let mut voice = desktop_off;
        voice.set_desktop(true);
        assert!(voice.listening());
        ready(&voice);
        assert_eq!(voice.subtitle(), "‘Tones’ has sounds");
        assert!(until(Duration::from_secs(5), || server.connections() == 1));
        assert_eq!((voice.stats().threads, voice.stats().connects), (1, 1));
        // Off again, then the base look: all let go.
        voice.set_switch(false);
        assert!(!voice.has_thread());
        voice.load(None);
        voice.set_switch(true);
        assert!(!voice.has_thread());
        drop(voice);
        drop(switched_off);
        assert_eq!(server.connections(), 1);
    }

    /// No sound system: Settings says so, nothing plays, and nothing
    /// reaches the theme.
    #[test]
    fn without_a_sound_system_settings_says_so() {
        let dir = Dir::new("nobody");
        let tones = theme_in(&dir.0, SOUNDS);
        let mut voice = Voice::new(at(&dir.0.join("pulse/native")), true, true);
        voice.load(Some(&tones));
        assert!(until(Duration::from_secs(10), || voice.status()
            == Status::Unavailable));
        assert_eq!(voice.subtitle(), "No sound system found");
        let press = key(137, KeyKind::Number, Via::Keyboard, 100.0);
        let heard = voice.hear(Some(&tones), &Ctx::default(), &press, 360.0, Instant::now());
        assert_eq!(heard.0, Heard::Silent("no sound system"));
        assert_eq!(voice.preview(), None);
        // `GMNB_SOUND=0`: off for the run, whatever Settings says.
        let off = Voice::new(
            Settings {
                sink: Sink::Off,
                ..null()
            },
            true,
            true,
        );
        assert_eq!(off.subtitle(), "Turned off for this run (GMNB_SOUND=0)");
        let shot = Voice::new(
            Settings {
                sink: Sink::Off,
                ..null()
            },
            true,
            true,
        )
        .quiet_run();
        assert_eq!(shot.subtitle(), "This theme has no sounds");
    }

    /// Each kind of event plays the sound designed for it; a typed key
    /// the one a clicked key plays; a press through the accessibility
    /// interface nothing.
    #[test]
    fn each_event_plays_its_sound() {
        let dir = Dir::new("events");
        let tones = theme_in(&dir.0, SOUNDS);
        let mut voice = Voice::new(null(), true, true);
        assert!(voice.load(Some(&tones)).is_empty());
        ready(&voice);
        let ctx = Ctx::default();
        let mut now = Instant::now();
        let mut hear = |voice: &mut Voice, e: &Event| {
            now += Duration::from_millis(100);
            voice.hear(Some(&tones), &ctx, e, 360.0, now).0
        };

        let clicked = hear(&mut voice, &key(137, KeyKind::Number, Via::Pointer, 100.0));
        let typed = hear(&mut voice, &key(137, KeyKind::Number, Via::Keyboard, 100.0));
        assert_eq!(sounds(&clicked), [1]);
        // The same key sounds the same, clicked or typed: its own pitch,
        // and its place.
        let (Heard::Played(_, a), Heard::Played(_, b)) = (&clicked, &typed) else {
            panic!("{clicked:?} {typed:?}")
        };
        assert_eq!(a[0].0, b[0].0);
        assert!((a[0].0.params.pan - (2.0 * 100.0 / 360.0 - 1.0) * 0.6).abs() < 1e-5);
        let other = hear(&mut voice, &key(138, KeyKind::Number, Via::Keyboard, 140.0));
        let Heard::Played(_, c) = &other else {
            panic!()
        };
        assert_ne!(c[0].0.params.rate, a[0].0.params.rate);

        assert_eq!(
            sounds(&hear(
                &mut voice,
                &key(121, KeyKind::Equals, Via::Keyboard, 300.0)
            )),
            [2]
        );
        let result = hear(&mut voice, &event(EventKind::Result, Page::Calculator));
        assert_eq!(sounds(&result), [3]);
        let Heard::Played(Some(EventName::Result), r) = &result else {
            panic!()
        };
        assert_eq!(r[0].0.params.delay, 0.05);
        assert_eq!(
            sounds(&hear(
                &mut voice,
                &event(EventKind::Error, Page::Calculator)
            )),
            [4]
        );
        assert_eq!(
            sounds(&hear(&mut voice, &event(EventKind::Mode, Page::Graphing))),
            [5]
        );
        assert_eq!(
            sounds(&hear(
                &mut voice,
                &key(100, KeyKind::Func, Via::Pointer, 50.0)
            )),
            [6]
        );
        let mut c = key(81, KeyKind::Func, Via::Pointer, 50.0);
        c.action = Some(Action::Clear);
        assert_eq!(sounds(&hear(&mut voice, &c)), [7]);
        let mut ms = event(EventKind::Press, Page::Calculator);
        (ms.source, ms.via, ms.action) =
            (Some(From::Button), Some(Via::Pointer), Some(Action::Memory));
        assert_eq!(sounds(&hear(&mut voice, &ms)), [8]);
        // `button = false`: a swap, a date picked, F-E.
        let mut swap = event(EventKind::Press, Page::Converter);
        (swap.source, swap.via) = (Some(From::Swap), Some(Via::Pointer));
        assert_eq!(
            hear(&mut voice, &swap),
            Heard::Nothing(Some(EventName::Button))
        );
        // A result on another page, an option: the table has nothing.
        let elsewhere = hear(&mut voice, &event(EventKind::Result, Page::Converter));
        assert_eq!(elsewhere, Heard::Nothing(None));
        // The running total an operator shows is not the end of a
        // calculation: 1 + 2 + 3 = chimes once, at =.
        let mut running = event(EventKind::Result, Page::Calculator);
        running.cause = Some(Cause::Operator);
        assert_eq!(hear(&mut voice, &running), Heard::Nothing(None));
        let mut function = event(EventKind::Result, Page::Calculator);
        function.cause = Some(Cause::Function);
        assert_eq!(sounds(&hear(&mut voice, &function)), [3]);
        // What follows an assistive press is silent too.
        let mut after = event(EventKind::Error, Page::Calculator);
        after.via = Some(Via::Assistive);
        assert_eq!(hear(&mut voice, &after), Heard::NotAnswered);

        // Through the accessibility interface: nothing.
        let assistive = hear(
            &mut voice,
            &key(137, KeyKind::Number, Via::Assistive, 100.0),
        );
        assert_eq!(assistive, Heard::NotAnswered);
        let shown = hear(&mut voice, &event(EventKind::Show, Page::Calculator));
        assert_eq!(shown, Heard::NotAnswered);

        // The same name again within 30 ms is dropped; after, it plays.
        let t = now + Duration::from_secs(1);
        let press = key(137, KeyKind::Number, Via::Keyboard, 100.0);
        assert_eq!(
            sounds(&voice.hear(Some(&tones), &ctx, &press, 360.0, t).0),
            [1]
        );
        let again = voice.hear(
            Some(&tones),
            &ctx,
            &press,
            360.0,
            t + Duration::from_millis(20),
        );
        assert_eq!(again.0, Heard::TooSoon(Some(EventName::Digit)));
        let later = voice.hear(
            Some(&tones),
            &ctx,
            &press,
            360.0,
            t + Duration::from_millis(31),
        );
        assert_eq!(sounds(&later.0), [1]);

        // The volume let go in Settings: the digit's sound.
        assert_eq!(voice.preview(), Some(Played::Queued));
        // Every play became a voice (or will: none was refused).
        assert!(until(Duration::from_secs(5), || {
            let s = voice.stats();
            s.started + s.skipped == s.queued
        }));
        assert_eq!(voice.stats().skipped, 0);
        assert!(voice.take_notes().is_empty());
    }

    /// The theme's `sound` function decides first; a sound it makes there
    /// joins the bank; when it fails, the table decides, and after three
    /// failures it is asked no more.
    #[test]
    fn the_sound_function_decides_first() {
        let dir = Dir::new("hook");
        let theme = theme_in(
            &dir.0,
            r#"
local click = gmnb.synth({ layers = { { freq = "A5", env = { decay = 0.02 } } } })
local chime = gmnb.synth({ layers = { { freq = "E6", env = { decay = 0.05 } } } })
local fails = 0
return {
	api = 1, name = "Hook",
	sounds = { key = click },
	sound = function(event, ctx)
		if event.kind == "result" and event.cause == "operator" then
			-- It hears every result: the running total's too.
			return chime
		elseif event.kind == "result" then
			return { chime, { sound = chime, delay = 0.08, pan = event.pan } }
		elseif event.kind == "mode" then
			return gmnb.synth({ layers = { { freq = "G6", env = { decay = 0.02 } } } })
		elseif event.kind == "error" then
			error("no")
		elseif event.kind == "option" then
			return false
		end
		return nil
	end,
}"#,
        );
        let mut voice = Voice::new(null(), true, true);
        voice.load(Some(&theme));
        ready(&voice);
        let ctx = Ctx::default();
        let mut now = Instant::now();
        let mut hear = |voice: &mut Voice, e: &Event| {
            now += Duration::from_millis(100);
            voice.hear(Some(&theme), &ctx, e, 360.0, now)
        };
        // Nil: the table.
        let (heard, asked) = hear(&mut voice, &key(137, KeyKind::Number, Via::Pointer, 10.0));
        assert_eq!((sounds(&heard), asked), (vec![1], Asked::default()));
        // Two plays at once, on any page.
        let mut result = event(EventKind::Result, Page::Converter);
        result.x = 360.0;
        let (heard, _) = hear(&mut voice, &result);
        let Heard::Played(_, plays) = &heard else {
            panic!("{heard:?}")
        };
        assert_eq!(plays.len(), 2);
        assert_eq!(
            (plays[1].0.params.delay, plays[1].0.params.pan),
            (0.08, 1.0)
        );
        // It hears the running total too, which the table would not play.
        let mut total = event(EventKind::Result, Page::Calculator);
        total.cause = Some(Cause::Operator);
        assert_eq!(sounds(&hear(&mut voice, &total).0), [2]);
        // False: nothing, though it has no name in the table.
        let (heard, _) = hear(&mut voice, &event(EventKind::Option, Page::Settings));
        assert_eq!(heard, Heard::Nothing(None));
        // Made inside it: told to the engine, and played once rendered.
        let (heard, _) = hear(&mut voice, &event(EventKind::Mode, Page::Date));
        let Heard::Played(_, made) = &heard else {
            panic!("{heard:?}")
        };
        assert_eq!(made[0].0.sound, SoundId(3));
        let bank = voice.bank().unwrap().clone();
        assert!(until(Duration::from_secs(10), || bank.state(SoundId(3))
            == SoundState::Ready));
        let (heard, _) = hear(&mut voice, &event(EventKind::Mode, Page::Date));
        assert_eq!(sounds(&heard), [3]);
        // It fails: the table decides (an error has no entry: nothing),
        // and the third failure switches it off.
        for i in 0..3 {
            let (heard, asked) = hear(&mut voice, &event(EventKind::Error, Page::Calculator));
            assert_eq!(heard, Heard::Nothing(Some(EventName::Error)));
            assert!(asked.failed.is_some());
            assert_eq!(asked.switched_off, i == 2);
        }
        assert!(!theme.asks_sound());
        let mut running = event(EventKind::Result, Page::Calculator);
        running.cause = Some(Cause::Operator);
        let (heard, _) = hear(&mut voice, &running);
        assert_eq!(heard, Heard::Nothing(None));
        let calculator = event(EventKind::Result, Page::Calculator);
        let (heard, asked) = hear(&mut voice, &calculator);
        assert_eq!(
            (heard, asked),
            (Heard::Nothing(Some(EventName::Result)), Asked::default())
        );
    }

    /// A press to its play takes well under a millisecond on the
    /// interface's thread; a press to its sound reaching the (stand-in)
    /// server, a few. (The figures are printed for the record.)
    #[test]
    fn a_press_is_quick_to_sound() {
        let dir = Dir::new("quick");
        let socket = dir.0.join("native");
        let server = Server::start(&socket);
        let tones = theme_in(&dir.0, SOUNDS);
        let mut voice = Voice::new(at(&socket), true, true);
        voice.load(Some(&tones));
        ready(&voice);
        let ctx = Ctx::default();
        let (mut calls, mut arrivals) = (Vec::new(), Vec::new());
        for i in 0..40u32 {
            // Each from a stream at rest (corked): the sound and 100 ms of
            // silence, then the cork.
            let corks = voice.stats().corks;
            let press = key(
                137 + i % 10,
                KeyKind::Number,
                Via::Keyboard,
                20.0 + i as f32,
            );
            let t = Instant::now();
            let (heard, _) = voice.hear(Some(&tones), &ctx, &press, 360.0, t);
            calls.push(t.elapsed());
            assert_eq!(sounds(&heard), [1], "{heard:?}");
            assert!(until(Duration::from_secs(5), || server
                .sound_after(t)
                .is_some()));
            arrivals.push(server.sound_after(t).unwrap() - t);
            assert!(until(Duration::from_secs(5), || voice.stats().corks > corks));
        }
        let ms = |mut v: Vec<Duration>| {
            v.sort();
            let at = |q: f64| v[((v.len() - 1) as f64 * q) as usize].as_secs_f64() * 1e3;
            (at(0.5), at(0.95), at(1.0))
        };
        let (c50, c95, cmax) = ms(calls);
        let (a50, a95, amax) = ms(arrivals);
        eprintln!(
            "press → play: p50 {c50:.3}, p95 {c95:.3}, max {cmax:.3} ms; \
             press → stand-in server: p50 {a50:.2}, p95 {a95:.2}, max {amax:.2} ms"
        );
        assert!(c50 < 1.0, "press → play: {c50} ms");
        assert!(a50 < 100.0, "press → server: {a50} ms");
    }
}
