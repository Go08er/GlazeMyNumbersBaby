//! The theme hub: the one place that knows the active theme and the
//! resolved style, writes it to the CSS providers, and tells subscribers.
//!
//! What a style is worked out for (`ctx`) is gathered here: light or dark
//! from libadwaita, the desktop's accent colour from the Settings portal,
//! the Background opacity, the theme's options, reduced motion and the
//! desktop's font. The base look needs no theme: no Luau machine exists
//! then. Nothing here runs at rest: no timer and no tick callback.
//!
//! The hub also holds the theme's sounds (`crate::sound::Voice`): it hands
//! the engine each theme loaded, hears each event the layers are told of,
//! and is what Settings' Sound rows ask and tell.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use gmnb_theme::draw::DisplayList;
use gmnb_theme::events::{Event, EventKind, Page};
use gmnb_theme::layer::{Drawn, Failure, Frame, LayerKind};
use gmnb_theme::theme::{self as themes, Action, FontFile, OptionDef, Origin, Source, Theme};
use gmnb_theme::{Ctx, Limits, Note, OptionValue, Style, TextMeasure, Update, Writer, css, schema};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use crate::layers::{Happened, Layers};
use crate::sound::{Asked, Heard, Voice};

pub use appcore::color::{parse_hex, to_hex};

pub fn rgba(c: [f32; 3], a: f32) -> gdk::RGBA {
    gdk::RGBA::new(c[0], c[1], c[2], a)
}

// ---------------------------------------------------------------------------
// The hub.
// ---------------------------------------------------------------------------

/// Returns false once it should be dropped (its widget is gone).
type StyleSubscriber = Box<dyn Fn(&Rc<Style>) -> bool>;
/// Told what Settings says of sound now; false once its widget is gone.
type SoundSubscriber = Box<dyn Fn(&str) -> bool>;
/// How the user is told something (a toast).
type Tell = Box<dyn Fn(&str)>;

/// What the hub starts with.
pub struct Config {
    /// The theme to use: an id, or "" for the base look.
    pub theme: String,
    /// What is saved of each theme's options.
    pub saved_options: BTreeMap<String, BTreeMap<String, OptionValue>>,
    /// Options for this run only (`GMNB_THEME_OPTIONS`, `GMNB_PALETTE`):
    /// option id and its value as text.
    pub run_options: Vec<(String, String)>,
    pub opacity: f32,
    /// "Use the desktop's accent colour".
    pub system_accent: bool,
    /// `GMNB_ACCENT`: what the desktop's accent is taken to be (`None`
    /// inside: none).
    pub accent: Option<Option<[f32; 3]>>,
    /// `GMNB_THEME_DIR`.
    pub dev_dir: Option<PathBuf>,
    /// Settings' *Theme sounds* and its volume (0 to 1).
    pub sound: bool,
    pub sound_volume: f32,
}

/// One theme of Settings' list.
pub struct Listed {
    /// "" for the base look.
    pub id: String,
    pub name: String,
    pub description: String,
    /// The user's folder replaces a built-in theme of the same id.
    pub your_copy: bool,
    /// Why it doesn't load, if it doesn't.
    pub error: Option<String>,
}

/// Counters for the gates (`GMNB_THEME_STATS`): what theming costs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    /// Luau machines alive (0 in the base look).
    pub machines: u32,
    /// Times `style(ctx)` ran.
    pub style_calls: u32,
    /// Loads of the main provider, and of the live one.
    pub main_loads: u32,
    pub live_loads: u32,
    /// Properties in each.
    pub main_properties: usize,
    pub live_properties: usize,
    /// What the drawing layers have cost.
    pub layers: crate::layers::Stats,
    /// Textures made of the theme's pictures.
    pub textures: usize,
    /// What the theme's animations have cost: the media thread.
    pub media: crate::layers::replay::MediaStats,
    /// What the sound engine has done, and whether its thread exists.
    pub sound: gmnb_sound::Stats,
    pub sound_thread: bool,
    /// Connections to a sound server this process has attempted.
    pub sound_connections: u64,
}

pub struct Hub {
    theme: RefCell<Option<Theme>>,
    /// The theme chosen: "" for the base look. It may not be loaded (not
    /// installed, or failed).
    active: RefCell<String>,
    /// Why the chosen theme isn't the one showing.
    error: RefCell<Option<String>>,
    /// What the theme's author should know, latest last.
    notes: RefCell<Vec<Note>>,
    /// `style` has failed and the user has been told.
    style_failed: Cell<bool>,
    /// The loaded theme's `style` has worked at least once.
    styled: Cell<bool>,
    options: RefCell<BTreeMap<String, BTreeMap<String, OptionValue>>>,
    run_options: RefCell<Vec<(String, String)>>,
    opacity: Cell<f32>,
    system_accent: Cell<bool>,
    accent_override: Option<Option<[f32; 3]>>,
    /// The desktop's accent as its portal reports it (read lazily, kept
    /// fresh by a portal watch). The inner `None`: it shares none.
    portal_accent: Cell<Option<Option<[f32; 3]>>>,
    portal_watch: RefCell<Option<gio::SignalSubscription>>,
    dev_dir: Option<PathBuf>,
    style: RefCell<Rc<Style>>,
    writer: RefCell<Writer>,
    main: gtk::CssProvider,
    live: gtk::CssProvider,
    windows: RefCell<Vec<glib::WeakRef<gtk::Window>>>,
    fonts: RefCell<HashSet<PathBuf>>,
    style_subscribers: RefCell<Vec<StyleSubscriber>>,
    /// How the user is told of a theme that failed (a toast).
    tell: RefCell<Option<Tell>>,
    /// What was to be told before there was a way to tell it.
    untold: RefCell<Vec<String>>,
    stats: Cell<Stats>,
    /// The drawing layers' runtime.
    layers: Rc<Layers>,
    /// The `ctx` the style was last worked out for: what frames are told.
    frame_ctx: RefCell<Ctx>,
    /// Measures `draw:text_size`, once a window is there to measure in.
    measure: RefCell<Option<Rc<dyn TextMeasure>>>,
    /// The layers whose hook was switched off, and the user told.
    told_off: RefCell<HashSet<LayerKind>>,
    /// The theme's sounds.
    sound: RefCell<Voice>,
    /// Since when the window has been on screen.
    on_screen: Cell<Option<Instant>>,
    /// Settings' Sound rows, told when what they say changes.
    sound_subscribers: RefCell<Vec<SoundSubscriber>>,
    /// The theme's `sound` was switched off, and the user told.
    told_sound_off: Cell<bool>,
    /// `GMNB_SOUND_TRACE`: a line on stderr for each event sound hears.
    trace: bool,
}

thread_local! {
    /// The window's hub, for widgets that draw a layer: they are made by
    /// pages that needn't know.
    static CURRENT: RefCell<std::rc::Weak<Hub>> = const { RefCell::new(std::rc::Weak::new()) };
}

/// The app's name for the base look in Settings.
pub const BASE_NAME: &str = "GMNB";

/// `$XDG_DATA_HOME/gmnb/themes`: where the user's themes live.
pub fn user_dir() -> PathBuf {
    glib::user_data_dir().join(crate::DATA_DIR).join("themes")
}

impl Hub {
    pub fn new(config: Config) -> Rc<Self> {
        let provider = |priority: u32| {
            let p = gtk::CssProvider::new();
            p.connect_parsing_error(|_, section, err| {
                eprintln!("css error at {}: {err}", section.to_str());
            });
            gtk::style_context_add_provider_for_display(
                &gdk::Display::default().expect("display"),
                &p,
                priority,
            );
            p
        };
        let ctx = Ctx::default();
        // The desktop's own switch for event sounds.
        let desktop_sounds =
            gtk::Settings::default().is_none_or(|s| s.is_gtk_enable_event_sounds());
        let mut voice = Voice::new(
            crate::sound::engine_settings(config.sound_volume),
            config.sound,
            desktop_sounds,
        );
        if std::env::var_os("GMNB_SCREENSHOT").is_some() {
            voice = voice.quiet_run();
        }
        let hub = Rc::new(Hub {
            theme: RefCell::new(None),
            active: RefCell::new(String::new()),
            error: RefCell::new(None),
            notes: RefCell::default(),
            style_failed: Cell::new(false),
            styled: Cell::new(false),
            options: RefCell::new(config.saved_options),
            run_options: RefCell::new(config.run_options),
            opacity: Cell::new(config.opacity),
            system_accent: Cell::new(config.system_accent),
            accent_override: config.accent,
            portal_accent: Cell::new(None),
            portal_watch: RefCell::new(None),
            dev_dir: config.dev_dir,
            style: RefCell::new(Rc::new(Style::base(&ctx))),
            writer: RefCell::new(Writer::new(crate::STYLESHEET)),
            // Above USER: shells like Noctalia/matugen write
            // ~/.config/gtk-4.0/gtk.css. The stylesheet is at USER + 1.
            main: provider(gtk::STYLE_PROVIDER_PRIORITY_USER + 2),
            live: provider(gtk::STYLE_PROVIDER_PRIORITY_USER + 3),
            windows: RefCell::default(),
            fonts: RefCell::default(),
            style_subscribers: RefCell::default(),
            tell: RefCell::new(None),
            untold: RefCell::default(),
            stats: Cell::default(),
            layers: Layers::new(),
            frame_ctx: RefCell::new(ctx.clone()),
            measure: RefCell::new(None),
            told_off: RefCell::default(),
            sound: RefCell::new(voice),
            on_screen: Cell::new(None),
            sound_subscribers: RefCell::default(),
            told_sound_off: Cell::new(false),
            trace: std::env::var_os("GMNB_SOUND_TRACE").is_some(),
        });
        hub.layers.set_hub(&hub);
        {
            // Sound hears every event the layers are told of, in the
            // window's coordinates (while a theme with sounds is on).
            let weak = Rc::downgrade(&hub);
            hub.layers.listen(move |event, width| {
                if let Some(hub) = weak.upgrade() {
                    hub.hear_event(event, width);
                }
            });
        }
        CURRENT.with(|c| c.replace(Rc::downgrade(&hub)));
        let sm = adw::StyleManager::default();
        let weak = Rc::downgrade(&hub);
        sm.connect_dark_notify(move |_| {
            if let Some(hub) = weak.upgrade() {
                hub.apply(false);
            }
        });
        let weak = Rc::downgrade(&hub);
        sm.connect_accent_color_rgba_notify(move |_| {
            if let Some(hub) = weak.upgrade() {
                hub.refresh_portal_accent();
            }
        });
        if let Some(settings) = gtk::Settings::default() {
            // Reduced motion and the desktop's font are part of `ctx`.
            let weak = Rc::downgrade(&hub);
            settings.connect_gtk_enable_animations_notify(move |_| {
                if let Some(hub) = weak.upgrade() {
                    hub.apply(false);
                }
            });
            let weak = Rc::downgrade(&hub);
            settings.connect_gtk_font_name_notify(move |_| {
                if let Some(hub) = weak.upgrade() {
                    hub.apply(false);
                }
            });
            // The desktop turning its event sounds off turns GMNB's off.
            let weak = Rc::downgrade(&hub);
            settings.connect_gtk_enable_event_sounds_notify(move |s| {
                if let Some(hub) = weak.upgrade() {
                    hub.sound
                        .borrow_mut()
                        .set_desktop(s.is_gtk_enable_event_sounds());
                    hub.sound_changed();
                }
            });
        }
        hub.load(&config.theme);
        hub
    }

    /// The window's hub, if there is one.
    pub fn current() -> Option<Rc<Hub>> {
        CURRENT.with(|c| c.borrow().upgrade())
    }

    /// The drawing layers' runtime.
    pub fn layers(&self) -> &Rc<Layers> {
        &self.layers
    }

    // ---- ctx ---------------------------------------------------------------

    /// What the style is worked out for now.
    pub fn ctx(&self) -> Ctx {
        let settings = gtk::Settings::default();
        Ctx {
            dark: adw::StyleManager::default().is_dark(),
            accent: self.accent(),
            opacity: self.opacity.get(),
            options: self.option_values(),
            motion: settings
                .as_ref()
                .is_none_or(|s| s.is_gtk_enable_animations()),
            font: desktop_font(settings.as_ref()),
        }
    }

    /// The desktop's accent colour exactly as its portal reports it;
    /// `None` if it shares none or the user turned its use off.
    fn accent(&self) -> Option<[f32; 3]> {
        if let Some(fixed) = self.accent_override {
            return fixed;
        }
        if !self.system_accent.get() {
            return None;
        }
        if let Some(known) = self.portal_accent.get() {
            return known;
        }
        let read = read_portal_accent();
        self.portal_accent.set(Some(read));
        self.watch_portal();
        read
    }

    fn refresh_portal_accent(&self) {
        self.portal_accent.set(None);
        if self.accent_override.is_none() && self.system_accent.get() {
            self.apply(false);
        }
    }

    /// Live-follow accent changes (e.g. Noctalia re-theming from a wallpaper).
    fn watch_portal(&self) {
        if self.portal_watch.borrow().is_some() {
            return;
        }
        let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
            return;
        };
        // The callback can't hold the hub, so it nudges it through the
        // thread's own slot instead.
        let sub = conn.subscribe_to_signal(
            Some("org.freedesktop.portal.Desktop"),
            Some("org.freedesktop.portal.Settings"),
            Some("SettingChanged"),
            Some("/org/freedesktop/portal/desktop"),
            Some("org.freedesktop.appearance"),
            gio::DBusSignalFlags::NONE,
            |signal| {
                // Signal bodies aren't type-checked: a malformed one is
                // ignored, not indexed into.
                let key = signal.parameters.try_child_value(1);
                if key.as_ref().and_then(|k| k.str()) == Some("accent-color") {
                    PORTAL_ACCENT_CHANGED.with(|f| {
                        if let Some(f) = f.borrow().as_ref() {
                            f();
                        }
                    });
                }
            },
        );
        self.portal_watch.replace(Some(sub));
    }

    /// "Use the desktop's accent colour".
    pub fn set_system_accent(&self, on: bool) {
        if self.system_accent.replace(on) != on {
            self.apply(false);
        }
    }

    /// The Background opacity. `drag`: one step of a slider being dragged
    /// (the few properties that change go to the live provider; call
    /// [`Hub::settle`] when it rests).
    pub fn set_opacity(&self, opacity: f32, drag: bool) {
        self.opacity.set(opacity);
        self.apply(drag);
    }

    // ---- The theme ------------------------------------------------------------

    fn sources(&self) -> Vec<Source> {
        themes::discover(Some(&user_dir()), self.dev_dir.as_deref())
    }

    /// What Settings lists: the base look first, then the built-in themes,
    /// then the user's. Each theme's file is run in a throwaway sandbox
    /// for its name.
    pub fn list(&self) -> Vec<Listed> {
        let mut out = vec![Listed {
            id: String::new(),
            name: BASE_NAME.to_string(),
            description: "The base look".to_string(),
            your_copy: false,
            error: None,
        }];
        for l in themes::list(&self.sources(), &Limits::default()) {
            let (name, description, error) = match l.meta {
                Ok(meta) => (meta.name, meta.description.unwrap_or_default(), None),
                Err(e) => (l.id.clone(), String::new(), Some(e)),
            };
            out.push(Listed {
                id: l.id,
                name,
                description,
                your_copy: l.origin == Origin::UserCopy,
                error,
            });
        }
        out
    }

    /// What the installed themes credit (About → Legal): a title and a
    /// text each.
    pub fn credits(&self) -> Vec<(String, String)> {
        credits(&self.sources())
    }

    /// The theme chosen ("" for the base look), loaded or not.
    pub fn active(&self) -> String {
        self.active.borrow().clone()
    }

    /// Why the chosen theme isn't the one showing, if it isn't.
    pub fn error(&self) -> Option<String> {
        self.error.borrow().clone()
    }

    /// What the theme's author should know: the latest 20.
    pub fn notes(&self) -> Vec<Note> {
        // What the sound engine found as it made the theme's sounds.
        let made = self.sound.borrow().take_notes();
        for note in made {
            self.note(&note.path, note.message);
        }
        let notes = self.notes.borrow();
        notes[notes.len().saturating_sub(20)..].to_vec()
    }

    /// How the user is told that a theme failed.
    /// What was to be told before then (a theme that failed as GMNB
    /// started) is told now.
    pub fn set_tell(&self, tell: impl Fn(&str) + 'static) {
        for message in self.untold.take() {
            tell(&message);
        }
        self.tell.replace(Some(Box::new(tell)));
    }

    fn tell(&self, message: &str) {
        eprintln!("gmnb: {message}");
        match self.tell.borrow().as_ref() {
            Some(tell) => tell(message),
            None => self.untold.borrow_mut().push(message.to_string()),
        }
    }

    /// Chooses a theme ("" or "base": the base look) and shows it.
    pub fn set_theme(&self, id: &str) {
        self.load(id);
    }

    /// Runs the chosen theme's file again (Settings' Reload).
    pub fn reload(&self) {
        let id = self.active();
        self.load(&id);
    }

    fn load(&self, id: &str) {
        let id = if id == themes::BASE_ID { "" } else { id };
        *self.active.borrow_mut() = id.to_string();
        self.error.replace(None);
        self.notes.borrow_mut().clear();
        self.style_failed.set(false);
        self.styled.set(false);
        // Unload first: one machine at a time.
        self.theme.replace(None);
        if !id.is_empty() {
            match self.sources().into_iter().find(|s| s.id == id) {
                None => {
                    // Kept as the choice: the theme returns when its
                    // folder does.
                    self.error
                        .replace(Some(format!("The theme “{id}” isn't installed")));
                }
                Some(source) => match Theme::load(source, Limits::default()) {
                    Ok(theme) => {
                        self.register_fonts(&theme);
                        self.notes.borrow_mut().extend(theme.notes.list.clone());
                        if let Some(measure) = self.measure.borrow().as_ref() {
                            theme.api().set_measure(measure.clone());
                        }
                        self.theme.replace(Some(theme));
                    }
                    Err(e) => {
                        self.error.replace(Some(e.to_string()));
                        self.tell(&format!("Theme “{id}” couldn't be loaded"));
                    }
                },
            }
        }
        // Nothing of the theme that was keeps running; the layers of the
        // one that is start afresh, and its sounds are made.
        self.told_off.borrow_mut().clear();
        self.layers.theme_changed();
        self.sounds_changed();
        self.apply(false);
    }

    /// The theme was loaded or unloaded: the sound engine is handed its
    /// sounds (or lets the last theme's go).
    fn sounds_changed(&self) {
        self.told_sound_off.set(false);
        let notes = {
            let theme = self.theme.borrow();
            self.sound.borrow_mut().load(theme.as_ref())
        };
        for note in notes {
            self.note(&note.path, note.message);
        }
        self.sound_changed();
    }

    // ---- Sound -------------------------------------------------------------------

    /// What sound does changed (the theme, the switch, the desktop's
    /// setting, or the engine found the sound system or none): events are
    /// heard only while there is something to hear them, and Settings
    /// says what is so.
    pub fn sound_changed(&self) {
        let (listening, subtitle) = {
            let voice = self.sound.borrow();
            (voice.listening(), voice.subtitle())
        };
        self.layers.set_heard(listening);
        let subs = std::mem::take(&mut *self.sound_subscribers.borrow_mut());
        let mut keep: Vec<SoundSubscriber> = subs.into_iter().filter(|f| f(&subtitle)).collect();
        keep.append(&mut self.sound_subscribers.borrow_mut());
        *self.sound_subscribers.borrow_mut() = keep;
    }

    /// Call `f` with what Settings says of sound, now and whenever it
    /// changes, while `widget` is alive.
    pub fn subscribe_sound<W: IsA<glib::Object>>(
        &self,
        widget: &W,
        f: impl Fn(&W, &str) + 'static,
    ) {
        f(widget, &self.sound.borrow().subtitle());
        let weak = widget.downgrade();
        self.sound_subscribers
            .borrow_mut()
            .push(Box::new(move |text| match weak.upgrade() {
                Some(w) => {
                    f(&w, text);
                    true
                }
                None => false,
            }));
    }

    /// Settings' *Theme sounds*.
    pub fn sound_switch(&self) -> bool {
        self.sound.borrow().switch()
    }

    pub fn set_sound(&self, on: bool) {
        self.sound.borrow_mut().set_switch(on);
        self.sound_changed();
    }

    /// Settings' volume, 0 to 1.
    pub fn set_sound_volume(&self, volume: f32) {
        self.sound.borrow_mut().set_volume(volume);
    }

    /// The volume was let go: the theme's `digit` sound, once.
    pub fn preview_sound(&self) {
        let played = self.sound.borrow_mut().preview();
        if self.trace {
            eprintln!("sound: the volume let go: {played:?}");
        }
    }

    /// The window came on screen, or left it.
    fn on_screen(&self, shown: bool) {
        self.on_screen.set(shown.then(Instant::now));
    }

    /// One event, in the window's coordinates: what it plays ("From
    /// event to sound"). Nothing while the window is not the one in use
    /// or has been on screen for less than half a second.
    fn hear_event(&self, event: &Event, width: f32) {
        let start = Instant::now();
        let settled = self
            .on_screen
            .get()
            .is_some_and(|t| start.saturating_duration_since(t) >= crate::sound::SETTLING);
        let (heard, asked) = if !self.layers.active() {
            (Heard::Silent("the window is not in use"), Asked::default())
        } else if !settled {
            (
                Heard::Silent("the window has only just come on screen"),
                Asked::default(),
            )
        } else {
            // (A theme being restyled is not to be asked anything.)
            let (Ok(theme), Ok(ctx)) = (self.theme.try_borrow(), self.frame_ctx.try_borrow())
            else {
                return;
            };
            self.sound
                .borrow_mut()
                .hear(theme.as_ref(), &ctx, event, width, start)
        };
        if self.trace {
            eprintln!(
                "{}",
                crate::sound::trace_line(event, &heard, &asked, start.elapsed())
            );
        }
        if matches!(heard, Heard::NotAnswered | Heard::Silent(_)) {
            return;
        }
        // What the theme's `sound` noted, did wrong, or came to.
        let (notes, name, dead) = self.with_theme(|t| match t {
            Some(t) => (t.take_sound_notes(), t.def.meta.name.clone(), t.is_dead()),
            None => (Vec::new(), String::new(), false),
        });
        for note in notes {
            self.note(&note.path, note.message);
        }
        if let Some(e) = &asked.failed {
            eprintln!("gmnb: sound: {e}");
            self.note("sound", e.clone());
        }
        if dead {
            self.layers.after_frame(|| {
                if let Some(hub) = Hub::current() {
                    hub.unload_dead();
                }
            });
        } else if asked.switched_off && !self.told_sound_off.replace(true) {
            self.tell(&format!(
                "Theme “{name}”: its sound function failed and was switched off"
            ));
        }
    }

    /// Adds the theme's font files to the app's font map (they are known
    /// by the family names inside them). A built-in theme's are written to
    /// the cache directory first.
    fn register_fonts(&self, theme: &Theme) {
        for file in theme.font_files() {
            let path = match file {
                FontFile::Path(path) => path,
                FontFile::Bytes(name, bytes) => {
                    let name = name.rsplit('/').next().unwrap_or(name);
                    match crate::cache_font(&format!("{}-{name}", theme.source.id), bytes) {
                        Some(path) => path,
                        None => continue,
                    }
                }
            };
            if self.fonts.borrow_mut().insert(path.clone()) {
                crate::add_font_file(&path);
            }
        }
    }

    /// Runs `f` with the loaded theme, if one is.
    pub fn with_theme<R>(&self, f: impl FnOnce(Option<&Theme>) -> R) -> R {
        f(self.theme.borrow().as_ref())
    }

    /// The loaded theme's options, as Settings renders them.
    pub fn option_defs(&self) -> Vec<OptionDef> {
        self.with_theme(|t| t.map_or_else(Vec::new, |t| t.def.options.clone()))
    }

    /// The loaded theme's option values: the run's, else the saved ones,
    /// else the defaults. Always complete.
    pub fn option_values(&self) -> BTreeMap<String, OptionValue> {
        let theme = self.theme.borrow();
        let Some(theme) = theme.as_ref() else {
            return BTreeMap::new();
        };
        let mut saved = self
            .options
            .borrow()
            .get(&theme.source.id)
            .cloned()
            .unwrap_or_default();
        for (id, text) in self.run_options.borrow().iter() {
            if let Some(def) = theme.def.options.iter().find(|o| o.id == *id) {
                let value = match text.as_str() {
                    "true" => OptionValue::Bool(true),
                    "false" => OptionValue::Bool(false),
                    t => match (&def.kind, t.parse::<f64>()) {
                        (themes::OptionKind::Slider { .. }, Ok(n)) => OptionValue::Number(n),
                        _ => OptionValue::Text(t.to_string()),
                    },
                };
                saved.insert(id.clone(), value);
            }
        }
        themes::complete_options(&theme.def.options, &saved)
    }

    /// Changes one of the loaded theme's options (the caller saves it).
    /// `drag`: as for [`Hub::set_opacity`].
    pub fn set_option(&self, id: &str, value: OptionValue, drag: bool) {
        let Some(theme) = self.with_theme(|t| t.map(|t| t.source.id.clone())) else {
            return;
        };
        self.options
            .borrow_mut()
            .entry(theme)
            .or_default()
            .insert(id.to_string(), value);
        // What the environment asked of this option stands only until
        // the user changes it.
        self.run_options.borrow_mut().retain(|(o, _)| o != id);
        let told = self.option_values().get(id).cloned();
        self.apply(drag);
        // After the style: a layer that hears of it sees the new `ctx`.
        if let Some(value) = told {
            let mut happened = Happened::new(EventKind::Option, Page::Settings, None);
            happened.option = Some((id.to_string(), value));
            self.layers.emit(&happened);
        }
    }

    /// The `ctx` frames and Settings' tiles are drawn for: the one the
    /// style was last worked out for.
    pub fn frame_ctx(&self) -> Ctx {
        self.frame_ctx.borrow().clone()
    }

    /// Whether Settings shows `option` now.
    pub fn option_visible(&self, option: &OptionDef) -> bool {
        let ctx = self.ctx();
        self.with_theme(|t| t.is_none_or(|t| t.visible(option, &ctx)))
    }

    /// Runs an option's action: the options it changes.
    pub fn run_action(&self, action: &Action) -> BTreeMap<String, OptionValue> {
        let ctx = self.ctx();
        self.with_theme(|t| t.and_then(|t| t.run_action(action, &ctx).ok()))
            .unwrap_or_default()
    }

    // ---- The style --------------------------------------------------------------

    pub fn style(&self) -> Rc<Style> {
        self.style.borrow().clone()
    }

    /// Works the style out again for the current `ctx`, and if it changed,
    /// writes it and tells the subscribers.
    fn apply(&self, drag: bool) {
        let ctx = self.ctx();
        let ctx_changed = *self.frame_ctx.borrow() != ctx;
        if ctx_changed {
            self.frame_ctx.replace(ctx.clone());
        }
        let mut stats = self.stats.get();
        let mut unload = None;
        let resolved = {
            let mut theme = self.theme.borrow_mut();
            match theme.as_mut() {
                None => Some(gmnb_theme::Resolved {
                    style: Style::base(&ctx),
                    notes: Default::default(),
                }),
                Some(theme) => {
                    stats.style_calls += 1;
                    let result = theme.style(&ctx).cloned();
                    for line in theme.take_log() {
                        eprintln!("gmnb theme {}: {line}", theme.source.id);
                        self.notes.borrow_mut().push(Note {
                            path: "print".into(),
                            message: line,
                        });
                    }
                    match result {
                        Ok(resolved) => {
                            self.styled.set(true);
                            Some(resolved)
                        }
                        Err(e) => {
                            if theme.is_dead() {
                                unload = Some(e.to_string());
                            } else {
                                self.notes.borrow_mut().push(Note {
                                    path: "style".into(),
                                    message: e.to_string(),
                                });
                                if !self.style_failed.replace(true) {
                                    self.tell(&format!(
                                        "Theme “{}” has a problem: {e}",
                                        theme.def.meta.name
                                    ));
                                }
                            }
                            // The theme's last style that worked stands;
                            // if none has, the base look (not the style
                            // of the theme before it).
                            (!self.styled.get()).then(|| gmnb_theme::Resolved {
                                style: Style::base(&ctx),
                                notes: Default::default(),
                            })
                        }
                    }
                }
            }
        };
        self.stats.set(stats);
        if let Some(why) = unload {
            // The memory limit: the theme is unloaded.
            let name = self.active();
            self.theme.replace(None);
            self.error.replace(Some(why));
            self.layers.theme_changed();
            self.sounds_changed();
            self.tell(&format!(
                "Theme “{name}” was unloaded: it ran out of memory"
            ));
            return self.apply(false);
        }
        // Frames are told `ctx` too: one that changed redraws the layers,
        // whether or not the style did.
        if ctx_changed {
            self.layers.ctx_changed();
        }
        // A `style` that failed leaves the last one that worked standing
        // (the base look, if none did).
        let Some(resolved) = resolved else { return };
        {
            let mut notes = self.notes.borrow_mut();
            for note in resolved.notes.list {
                if !notes.contains(&note) {
                    notes.push(note);
                }
            }
            let extra = notes.len().saturating_sub(200);
            notes.drain(..extra);
        }
        let style = Rc::new(resolved.style);
        let changed = **self.style.borrow() != *style;
        self.style.replace(style.clone());
        if changed || self.stats.get().main_loads == 0 {
            self.write(&style, drag);
            self.sync_windows(&style);
            self.notify(&style);
        }
    }

    /// Writes the style to the providers: the whole of it to the main one,
    /// or, for a step of a drag that changed few properties, those to the
    /// live one.
    fn write(&self, style: &Style, drag: bool) {
        let (preamble, stock) = css::stock(style);
        let mut properties = css::properties(style);
        properties.extend(stock);
        let mut writer = self.writer.borrow_mut();
        let mut stats = self.stats.get();
        match writer.update(&preamble, &properties, drag) {
            Update::Unchanged => {}
            Update::Live(text) => {
                self.live.load_from_string(&text);
                stats.live_loads += 1;
            }
            Update::Main(text) => {
                self.main.load_from_string(&text);
                self.live.load_from_string("");
                stats.main_loads += 1;
            }
        }
        stats.main_properties = writer.main_len();
        stats.live_properties = writer.live_len();
        self.stats.set(stats);
    }

    /// Folds what a drag left in the live provider back into the main
    /// one. Called when the dragged setting is saved, a moment after its
    /// last step (`prefs::persist_at_rest`): theming keeps no timer of
    /// its own.
    pub fn settle(&self) {
        let folded = self.writer.borrow_mut().fold();
        if let Some(text) = folded {
            self.main.load_from_string(&text);
            self.live.load_from_string("");
            let mut stats = self.stats.get();
            stats.main_loads += 1;
            stats.main_properties = self.writer.borrow().main_len();
            stats.live_properties = 0;
            self.stats.set(stats);
        }
    }

    /// A window the style applies to: it gets a class for each family of
    /// the toolkit's widgets the app draws itself (`own-switch`), which
    /// the stylesheet's rules for that family sit under. A theme hands
    /// families back with `native`.
    pub fn attach(&self, window: &impl IsA<gtk::Window>) {
        let window = window.clone().upcast::<gtk::Window>();
        self.windows.borrow_mut().push(window.downgrade());
        // Nothing sounds until the window has been on screen a moment.
        if window.is_mapped() {
            self.on_screen(true);
        }
        window.connect_map(|_| {
            if let Some(hub) = Hub::current() {
                hub.on_screen(true);
            }
        });
        window.connect_unmap(|_| {
            if let Some(hub) = Hub::current() {
                hub.on_screen(false);
            }
        });
        self.sync_window(&window, &self.style());
        // `draw:text_size` measures as the window lays text out.
        if self.measure.borrow().is_none() {
            let measure = crate::layers::measure(&window);
            self.with_theme(|t| {
                if let Some(t) = t {
                    t.api().set_measure(measure.clone());
                }
            });
            self.measure.replace(Some(measure));
        }
    }

    // ---- The layers ---------------------------------------------------------------

    /// Runs the hooks of the theme's layer `kind` for one frame.
    pub fn draw_layer(&self, kind: LayerKind, frame: &Frame) -> Result<Drawn, Failure> {
        let ctx = self.frame_ctx.borrow();
        let theme = self.theme.borrow();
        let Some(theme) = theme.as_ref() else {
            return Err(Failure::Off);
        };
        let start = std::time::Instant::now();
        let result = theme.draw(kind, &ctx, frame);
        self.layers.hook_took(kind, start.elapsed());
        self.hear(theme);
        result
    }

    /// The tile of one choice of an option with a `preview`.
    pub fn preview(&self, option: &OptionDef, choice: &str) -> Option<DisplayList> {
        let ctx = self.frame_ctx.borrow();
        let theme = self.theme.borrow();
        let theme = theme.as_ref()?;
        let result = theme.preview(option, choice, &ctx);
        self.hear(theme);
        match result {
            Ok(list) => Some(list),
            Err(e) => {
                self.note("preview", e.to_string());
                None
            }
        }
    }

    fn note(&self, path: &str, message: String) {
        let note = Note {
            path: path.into(),
            message,
        };
        let mut notes = self.notes.borrow_mut();
        if !notes.contains(&note) {
            notes.push(note);
        }
        let extra = notes.len().saturating_sub(200);
        notes.drain(..extra);
    }

    /// Takes in what the theme printed and what its drawing noted.
    fn hear(&self, theme: &Theme) {
        for line in theme.take_log() {
            eprintln!("gmnb theme {}: {line}", theme.source.id);
            self.note("print", line);
        }
        for note in theme.take_draw_notes() {
            eprintln!("gmnb theme {}: {note}", theme.source.id);
            self.note(&note.path, note.message);
        }
    }

    /// A layer's hook failed. The frame draws nothing; the author gets a
    /// note. A hook that has been switched off is said so to the user,
    /// once; a theme that hit its memory limit is unloaded. Both happen
    /// after the frame being painted: this is called from a `snapshot`.
    pub fn layer_failed(&self, kind: LayerKind, failure: Failure) {
        if let Failure::Failed(e) = &failure {
            eprintln!("gmnb: layers.{}.draw: {e}", kind.name());
            self.note(&format!("layers.{}.draw", kind.name()), e.to_string());
        }
        let (dead, off, name) = self.with_theme(|t| match t {
            None => (false, false, String::new()),
            Some(t) => (t.is_dead(), !t.draws(kind), t.def.meta.name.clone()),
        });
        self.layers.sync_drawn();
        if dead {
            self.layers.after_frame(|| {
                if let Some(hub) = Hub::current() {
                    hub.unload_dead();
                }
            });
        } else if off && self.told_off.borrow_mut().insert(kind) {
            let message = format!(
                "Theme “{name}”: its {} drawing failed and was switched off",
                kind.name()
            );
            self.layers.after_frame(move || {
                if let Some(hub) = Hub::current() {
                    hub.tell(&message);
                }
            });
        }
    }

    /// Unloads a theme that hit its memory limit while drawing.
    fn unload_dead(&self) {
        if !self.with_theme(|t| t.is_some_and(|t| t.is_dead())) {
            return;
        }
        let name = self.active();
        self.theme.replace(None);
        self.error
            .replace(Some("the theme ran out of memory".into()));
        self.layers.theme_changed();
        self.sounds_changed();
        self.tell(&format!(
            "Theme “{name}” was unloaded: it ran out of memory"
        ));
        self.apply(false);
    }

    /// Each hook of the loaded theme's layers: its name, its failures and
    /// whether it was switched off.
    pub fn hooks(&self) -> Vec<(String, u32, bool)> {
        self.with_theme(|t| t.map_or_else(Vec::new, |t| t.hooks()))
    }

    fn sync_window(&self, window: &gtk::Window, style: &Style) {
        let native = style.native();
        for family in schema::NATIVE_FAMILIES {
            let class = format!("own-{family}");
            if native.contains(&family) {
                window.remove_css_class(&class);
            } else {
                window.add_css_class(&class);
            }
        }
    }

    fn sync_windows(&self, style: &Style) {
        let windows: Vec<gtk::Window> = {
            let mut all = self.windows.borrow_mut();
            all.retain(|w| w.upgrade().is_some());
            all.iter().filter_map(|w| w.upgrade()).collect()
        };
        for window in windows {
            self.sync_window(&window, style);
        }
    }

    fn notify(&self, style: &Rc<Style>) {
        // Collect first: a subscriber may itself subscribe or open UI.
        let subs = std::mem::take(&mut *self.style_subscribers.borrow_mut());
        let mut keep: Vec<StyleSubscriber> = subs.into_iter().filter(|f| f(style)).collect();
        keep.append(&mut self.style_subscribers.borrow_mut());
        *self.style_subscribers.borrow_mut() = keep;
    }

    /// Call `f` now and whenever the style changes, while `widget` is
    /// alive.
    pub fn subscribe_style<W: IsA<glib::Object>>(
        &self,
        widget: &W,
        f: impl Fn(&W, &Rc<Style>) + 'static,
    ) {
        f(widget, &self.style());
        let weak = widget.downgrade();
        self.style_subscribers
            .borrow_mut()
            .push(Box::new(move |s| match weak.upgrade() {
                Some(w) => {
                    f(&w, s);
                    true
                }
                None => false,
            }));
    }

    pub fn stats(&self) -> Stats {
        let mut stats = self.stats.get();
        stats.machines = u32::from(self.theme.borrow().is_some());
        stats.layers = self.layers.stats();
        stats.textures = self.layers.cache.textures();
        stats.media = self.layers.cache.media();
        let voice = self.sound.borrow();
        stats.sound = voice.stats();
        stats.sound_thread = voice.has_thread();
        stats.sound_connections = gmnb_sound::connection_attempts();
        stats
    }

    /// What the sound side is doing, in a line (`GMNB_THEME_STATS`).
    pub fn sound_report(&self) -> String {
        let voice = self.sound.borrow();
        let s = voice.stats();
        format!(
            "status {:?}, thread {}, threads {}, connections {}, queued {}, started {}, skipped {}, \
             wakes {}, uncorks {}, corks {}; {}",
            voice.status(),
            if voice.has_thread() { "yes" } else { "no" },
            s.threads,
            gmnb_sound::connection_attempts(),
            s.queued,
            s.started,
            s.skipped,
            s.wakes,
            s.uncorks,
            s.corks,
            voice.subtitle()
        )
    }
}

/// What the themes of `sources` (`themes::discover`'s) credit, for
/// About → Legal: a title and a text each. The built-in themes' always,
/// first: their fonts and pictures are in GMNB itself, whether or not a
/// folder of the user's replaces the theme ("your copy"); then the
/// user's themes' own.
fn credits(sources: &[Source]) -> Vec<(String, String)> {
    let built_in = themes::BUILT_IN
        .iter()
        .filter_map(|b| Source::built_in(b.id));
    let users = sources
        .iter()
        .filter(|s| s.origin != Origin::BuiltIn)
        .cloned();
    let mut out: Vec<(String, String)> = Vec::new();
    for theme in built_in
        .chain(users)
        .filter_map(|source| Theme::load(source, Limits::default()).ok())
    {
        for credit in theme.credits() {
            if !out.contains(&credit) {
                out.push(credit);
            }
        }
    }
    out
}

/// The desktop's interface font: its family, and its size in pixels.
fn desktop_font(settings: Option<&gtk::Settings>) -> (String, f32) {
    let fallback = ("sans-serif".to_string(), 15.0);
    let Some(settings) = settings else {
        return fallback;
    };
    let Some(name) = settings.gtk_font_name() else {
        return fallback;
    };
    let desc = pango::FontDescription::from_string(&name);
    let Some(family) = desc.family().filter(|f| !f.is_empty()) else {
        return fallback;
    };
    // A size in points is scaled by the desktop's resolution (in 1/1024
    // dots an inch; unset: 96).
    let size = desc.size() as f32 / pango::SCALE as f32;
    let px = if desc.is_size_absolute() {
        size
    } else {
        let dpi = settings.gtk_xft_dpi();
        let dpi = if dpi > 0 { dpi as f32 / 1024.0 } else { 96.0 };
        size * dpi / 72.0
    };
    let px = if px.is_finite() && px > 0.0 {
        px
    } else {
        fallback.1
    };
    (family.to_string(), (px * 100.0).round() / 100.0)
}

thread_local! {
    static PORTAL_ACCENT_CHANGED: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Route portal accent changes to `hub` (main thread only).
pub fn follow_portal_accent(hub: &Rc<Hub>) {
    let weak = Rc::downgrade(hub);
    PORTAL_ACCENT_CHANGED.with(|f| {
        f.replace(Some(Box::new(move || {
            if let Some(hub) = weak.upgrade() {
                hub.refresh_portal_accent();
            }
        })));
    });
}

/// `org.freedesktop.appearance accent-color` from the Settings portal:
/// `(ddd)` in 0..1, or out of range when the desktop has none set.
fn read_portal_accent() -> Option<[f32; 3]> {
    let conn = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
    let reply = conn
        .call_sync(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
            "ReadOne",
            Some(&("org.freedesktop.appearance", "accent-color").to_variant()),
            Some(glib::VariantTy::new("(v)").expect("valid type")),
            gio::DBusCallFlags::NONE,
            500,
            gio::Cancellable::NONE,
        )
        .ok()?;
    let inner = reply.child_value(0).as_variant()?;
    let (r, g, b) = inner.get::<(f64, f64, f64)>()?;
    let ok = |v: f64| (0.0..=1.0).contains(&v);
    (ok(r) && ok(g) && ok(b)).then_some([r as f32, g as f32, b as f32])
}

/// `GMNB_ACCENT`: a hex colour, or `none`.
pub fn accent_from_env(value: &str) -> Option<Option<[f32; 3]>> {
    match value.trim() {
        "none" => Some(None),
        hex => parse_hex(hex).map(Some),
    }
}

/// `GMNB_THEME_OPTIONS='palette=ember;living=false'`: option ids and their
/// values as text.
pub fn options_from_env(value: &str) -> Vec<(String, String)> {
    value
        .split(';')
        .filter_map(|pair| {
            let (id, v) = pair.split_once('=')?;
            Some((id.trim().to_string(), v.trim().to_string()))
        })
        .filter(|(id, _)| !id.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environments_hooks_parse() {
        assert_eq!(accent_from_env("none"), Some(None));
        assert_eq!(accent_from_env("#ff0000"), Some(Some([1.0, 0.0, 0.0])));
        assert_eq!(accent_from_env("25d08d").map(|a| a.is_some()), Some(true));
        assert_eq!(accent_from_env("red"), None);
        assert_eq!(
            options_from_env("palette=ember; living = false;;bad;=x"),
            [
                ("palette".to_string(), "ember".to_string()),
                ("living".to_string(), "false".to_string())
            ]
        );
    }

    /// About → Legal credits what GMNB itself carries, the built-in
    /// themes' fonts and the swirl, whatever the user installs: a folder
    /// of theirs named after a built-in theme ("your copy") replaced its
    /// credits with its own, and so did one that fails to load. A theme of
    /// the user's adds its own.
    #[test]
    fn the_built_in_credits_stay_with_a_copy_of_the_theme() {
        let dir = std::env::temp_dir().join(format!("gmnb-credits-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (id, source) in [
            ("aurora", "return { api = 1, name = \"Mine\" }"),
            ("glazed", "this is not Luau"),
            (
                "mine",
                "return { api = 1, name = \"Mine\", credits = { { title = \"Me\", file = \"me.txt\" } } }",
            ),
        ] {
            std::fs::create_dir_all(dir.join(id)).unwrap();
            std::fs::write(dir.join(id).join("theme.luau"), source).unwrap();
        }
        std::fs::write(dir.join("mine/me.txt"), "mine").unwrap();
        let listed = credits(&themes::discover(Some(&dir), None));
        let _ = std::fs::remove_dir_all(&dir);
        let titles: Vec<&str> = listed.iter().map(|(title, _)| title.as_str()).collect();
        for title in [
            "Outfit typeface",
            "Jersey 10 typeface",
            "The swirl (ColorMix, from ly)",
            "Me",
        ] {
            assert!(titles.contains(&title), "{title} is not in {titles:?}");
        }
        assert_eq!(titles.len(), 4, "{titles:?}");
    }

    /// The function of `code` that holds its byte `at`: the nearest `fn`
    /// before it.
    fn function_at(code: &str, at: usize) -> &str {
        let from = code[..at].rfind("fn ").map_or(0, |i| i + 3);
        code[from..]
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()
            .unwrap_or("")
    }

    /// Theming costs nothing at rest: no timeout, no idle and no frame
    /// callback is installed by the hub, the backdrop, the readout, the
    /// replay or a preview's tile at all. The layers' runtime makes them
    /// in two places only: `arm`, for a hook that asked for another
    /// frame (never for one that returned nil: `gmnb_theme::sched`), and
    /// `after_frame`, once, for a hook that failed. The media thread
    /// reaches the main loop in one place too, `wake`, with a frame of an
    /// animation that a layer being drawn asked for. The keypad's one
    /// timer is the 110 ms flash of a key typed on the keyboard, and its
    /// one idle makes it short or not again once it has been laid out at
    /// a height on the other side of `keys.short.below` (a resize, never
    /// at rest).
    #[test]
    fn theming_installs_no_sources() {
        const CALLS: [&str; 5] = [
            "timeout_add",
            "idle_add",
            "add_tick_callback",
            "source_new",
            "invoke",
        ];
        for (name, source, allowed) in [
            ("theme.rs", include_str!("theme.rs"), &[][..]),
            (
                "widgets/backdrop.rs",
                include_str!("widgets/backdrop.rs"),
                &[],
            ),
            (
                "widgets/display.rs",
                include_str!("widgets/display.rs"),
                &[],
            ),
            ("layers/replay.rs", include_str!("layers/replay.rs"), &[]),
            ("sound.rs", include_str!("sound.rs"), &[]),
            ("layers/tile.rs", include_str!("layers/tile.rs"), &[]),
            (
                "layers/mod.rs",
                include_str!("layers/mod.rs"),
                &[
                    ("timeout_add", "arm"),
                    ("add_tick_callback", "arm"),
                    ("idle_add", "after_frame"),
                    ("invoke", "wake"),
                ],
            ),
            (
                "widgets/keypad.rs",
                include_str!("widgets/keypad.rs"),
                &[("timeout_add", "flash"), ("idle_add", "size_allocate")],
            ),
        ] {
            let code = source.split("#[cfg(test)]").next().unwrap_or("");
            for call in CALLS {
                // The calls themselves: a name followed by its arguments
                // or a longer name (`timeout_add_local_once(`).
                for (at, _) in code.match_indices(call) {
                    let rest = &code[at + call.len()..];
                    if !rest.starts_with('(') && !rest.starts_with('_') {
                        continue;
                    }
                    let inside = function_at(code, at);
                    assert!(
                        allowed.contains(&(call, inside)),
                        "{name} calls {call} in `{inside}`"
                    );
                }
            }
            for (call, inside) in allowed {
                let found = code
                    .match_indices(call)
                    .filter(|(at, _)| function_at(code, *at) == *inside)
                    .count();
                assert_eq!(found, 1, "{name}: {call} in `{inside}`");
            }
        }
    }
}
