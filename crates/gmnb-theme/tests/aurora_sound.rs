//! Aurora's sounds (apps/gmnb/themes/aurora/theme.luau, "8. Sound"): they
//! load, every event plays what the theme means it to, its `sound`
//! function stays inside its 2 ms, and its bank is small.
//!
//! The events are made and decided by the stand-in for the app that the
//! render example uses, so what is tested here is what
//! `tools/theming/sound_check.py` measures.

#[path = "../../gmnb-sound/examples/support/stand_in.rs"]
mod stand_in;

use std::time::Duration;

use gmnb_sound::{Effect, EventName, Play, Recipe, SoundId, SoundSource, limits, prepare_all};
use gmnb_theme::events::{Action, EventKind, KeyKind, Page, Source as From};
use gmnb_theme::sound::{Choice, HOOK_TIME};
use gmnb_theme::theme::{Source, Theme};
use gmnb_theme::{Limits, OptionValue};
use stand_in::{By, StandIn};

// The sounds, by the order the theme makes them in.
const TICK_A: SoundId = SoundId(1);
const TICK_B: SoundId = SoundId(2);
const TOCK: SoundId = SoundId(3);
const TAP: SoundId = SoundId(4);
const SET: SoundId = SoundId(5);
const FLIP: SoundId = SoundId(6);
const BACK: SoundId = SoundId(7);
const SWEEP: SoundId = SoundId(8);
const SWELL: SoundId = SoundId(9);
const THUD: SoundId = SoundId(10);
const BELL_GLASS: SoundId = SoundId(11);
const BELL_WARM: SoundId = SoundId(12);
const BELL_BOWL: SoundId = SoundId(13);
const SOUNDS: usize = 13;

const PALETTES: [&str; 7] = [
    "aurora", "ember", "abyss", "orchard", "graphite", "system", "custom",
];

fn aurora() -> Theme {
    Theme::load(Source::built_in("aurora").unwrap(), Limits::default())
        .unwrap_or_else(|e| panic!("Aurora didn't load: {e}"))
}

fn recipes(theme: &Theme) -> Vec<Recipe> {
    theme
        .sounds()
        .sounds
        .into_iter()
        .map(|s| match s {
            SoundSource::Synth(r) => r,
            other => panic!("not a recipe: {other:?}"),
        })
        .collect()
}

/// A note's frequency over another's: the speed that plays a sound made
/// on `home` at `note`.
fn speed(note: &str, home: &str) -> f32 {
    (gmnb_sound::note_name(note).unwrap() / gmnb_sound::note_name(home).unwrap()) as f32
}

fn near(a: f32, b: f32, within: f32) -> bool {
    (a - b).abs() <= within
}

/// The processor time this thread has used.
fn cpu() -> Duration {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `clock_gettime` writes the one `timespec` it is given.
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) } == 0;
    assert!(ok);
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

fn digit(id: u32, across: f32) -> gmnb_theme::events::Event {
    stand_in::key(id, KeyKind::Number, None, across)
}

fn equals() -> gmnb_theme::events::Event {
    stand_in::key(30, KeyKind::Equals, None, 0.875)
}

fn operator() -> gmnb_theme::events::Event {
    stand_in::key(20, KeyKind::Operator, None, 0.875)
}

fn result() -> gmnb_theme::events::Event {
    stand_in::outcome(EventKind::Result, Page::Calculator)
}

/// Sets the palette as the app does: `ctx` first, then the event.
fn choose(app: &mut StandIn, palette: &str) -> (Vec<Play>, By) {
    let value = OptionValue::Text(palette.to_string());
    app.set_option("palette", value.clone());
    app.decide(&stand_in::option("palette", value))
}

// ---- They load -----------------------------------------------------------------

#[test]
fn auroras_sounds_load() {
    let theme = aurora();
    assert!(theme.notes.is_empty(), "{:?}", theme.notes);
    let sounds = theme.sounds();
    assert!(sounds.hook && sounds.any());
    assert_eq!(sounds.sounds.len(), SOUNDS);
    assert_eq!(theme.sounds_declared(), SOUNDS);
    let notes = theme.take_sound_notes();
    assert!(notes.is_empty(), "{notes:?}");

    // Every one is a recipe that renders, at no more than the gain a
    // built-in theme may have.
    let made = prepare_all(&sounds.sounds);
    for (i, (recipe, made)) in recipes(&theme).iter().zip(&made).enumerate() {
        let samples = made
            .as_ref()
            .unwrap_or_else(|e| panic!("sound {}: {e}", i + 1));
        assert!(samples.len() >= 2, "sound {} is silent", i + 1);
        assert!(recipe.gain <= 0.5, "sound {}: gain {}", i + 1, recipe.gain);
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak <= 16_384, "sound {}", i + 1);
    }

    // Every event the app has a name for has an entry of its own.
    for name in EventName::ALL {
        let own = sounds.table.get(name).is_some();
        let expected = !matches!(name, EventName::Key | EventName::Press);
        assert_eq!(own, expected, "{}", name.name());
    }
}

// ---- The bank ---------------------------------------------------------------------

#[test]
fn the_bank_is_small_and_quick_to_make() {
    let theme = aurora();
    let sounds = theme.sounds();
    let recipes = recipes(&theme);

    // What bounds the work of making it, whatever the machine is doing.
    // A room (a reverb or a delay) is rendered for the sound's whole
    // `length`, or for 4 s if it has none: each of Aurora's has one.
    let mut layer_seconds = 0.0;
    let mut rooms = 0;
    for (i, recipe) in recipes.iter().enumerate() {
        layer_seconds += recipe.layer_seconds();
        let room = recipe
            .effects
            .iter()
            .any(|e| matches!(e, Effect::Reverb { .. } | Effect::Delay { .. }));
        if room {
            rooms += 1;
            let length = recipe.length.unwrap_or(limits::SECONDS);
            assert!(length <= 1.5, "sound {}: {length} s in a room", i + 1);
        }
    }
    assert_eq!(rooms, 4, "the three bells and the swell");
    assert!(
        layer_seconds <= limits::LAYER_SECONDS / 5.0,
        "{layer_seconds} layer-seconds: over a fifth of what a theme may ask for"
    );

    let started = cpu();
    let made = prepare_all(&sounds.sounds);
    let took = cpu() - started;
    let bytes: usize = made.iter().flatten().map(|s| s.len() * 2).sum();
    assert!(
        bytes <= limits::TOTAL_BYTES / 5,
        "{bytes} bytes: over a fifth of what a theme may hold"
    );
    println!(
        "Aurora's bank: {SOUNDS} sounds, {layer_seconds:.2} layer-seconds, {bytes} bytes, made in {:.1} ms of processor time",
        took.as_secs_f64() * 1000.0
    );
    // The chapter's cost: 9.3 ms per layer-second at worst, in a release
    // build. Processor time, so a busy machine doesn't fail this; a build
    // without optimisation is allowed ten times as long.
    let allowed = if cfg!(debug_assertions) { 1500 } else { 150 };
    assert!(took < Duration::from_millis(allowed), "{took:?}");
}

// ---- Every event --------------------------------------------------------------

#[test]
fn keys_and_buttons_play_what_the_table_says() {
    let theme = aurora();
    let mut app = StandIn::new(&theme);
    let mut one = |event: &gmnb_theme::events::Event| {
        let (plays, by) = app.decide(event);
        assert_eq!(plays.len(), 1, "{event:?}");
        (plays[0], by)
    };
    let key = |kind, action| stand_in::key(7, kind, action, 0.25);

    for (event, name, sound) in [
        (key(KeyKind::Operator, None), EventName::Operator, TOCK),
        (key(KeyKind::Func, None), EventName::Func, TAP),
        (key(KeyKind::Other, None), EventName::Func, TAP),
        (key(KeyKind::Equals, None), EventName::Equals, SET),
        (key(KeyKind::Toggle, None), EventName::Toggle, FLIP),
        (
            key(KeyKind::Func, Some(Action::Clear)),
            EventName::Clear,
            SWEEP,
        ),
        (
            key(KeyKind::Other, Some(Action::Backspace)),
            EventName::Backspace,
            BACK,
        ),
        // A number key that clears or deletes is no digit.
        (
            key(KeyKind::Number, Some(Action::Backspace)),
            EventName::Backspace,
            BACK,
        ),
        (
            stand_in::button(From::Button, Some(Action::Memory), Page::Calculator, 0.25),
            EventName::Memory,
            TOCK,
        ),
        (
            stand_in::button(From::Button, None, Page::Calculator, 0.25),
            EventName::Button,
            TAP,
        ),
        (
            stand_in::button(From::Swap, None, Page::Converter, 0.5),
            EventName::Button,
            TAP,
        ),
        (
            stand_in::button(From::Date, None, Page::Date, 0.5),
            EventName::Button,
            TAP,
        ),
        (stand_in::mode(Page::Graphing), EventName::Mode, SWELL),
        (
            stand_in::outcome(EventKind::Error, Page::Calculator),
            EventName::Error,
            THUD,
        ),
    ] {
        let (play, by) = one(&event);
        assert_eq!(by, By::Table(name), "{event:?}");
        assert_eq!(play.sound, sound, "{event:?}");
        // No key sound is played slower than this: the 80 ms rule is
        // measured at the slowest.
        assert!(play.params.rate >= 0.95, "{event:?}");
    }

    // Where a key is across the window is where it sounds, at 60% width.
    let (left, _) = one(&stand_in::key(7, KeyKind::Operator, None, 0.0));
    let (right, _) = one(&stand_in::key(7, KeyKind::Operator, None, 1.0));
    assert!(near(left.params.pan, -0.6, 1e-6) && near(right.params.pan, 0.6, 1e-6));
    // The memory row's tock is a fifth up; the error waits for the
    // readout's first swing.
    let (memory, _) = one(&stand_in::button(
        From::Button,
        Some(Action::Memory),
        Page::Calculator,
        0.5,
    ));
    assert!(near(memory.params.rate, 1.498, 1e-6));
    let (error, _) = one(&stand_in::outcome(EventKind::Error, Page::Calculator));
    assert!(near(error.params.delay, 0.034, 1e-6));
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

#[test]
fn number_keys_ring_the_palettes_chord() {
    let theme = aurora();
    let mut app = StandIn::new(&theme);
    // Aurora's keys: C#6, E6, G#6 and A6, on a tick made at E6.
    let notes: Vec<f32> = ["C#6", "E6", "G#6", "A6"]
        .iter()
        .map(|n| speed(n, "E6"))
        .collect();
    let mut heard = [false; 4];
    let mut last: Option<Play> = None;
    for press in 0..80u32 {
        let id = press % 10 + 130;
        let across = (press % 4) as f32 / 3.0;
        let (plays, by) = app.decide(&digit(id, across));
        assert_eq!((plays.len(), by), (1, By::Function));
        let play = plays[0];
        assert!(play.sound == TICK_A || play.sound == TICK_B);
        // The key's own note (the choice the background makes of a
        // blob's colour: `id * 7 % 4`), a tenth of a semitone around it.
        let own = (id * 7 % 4) as usize;
        heard[own] = true;
        let off = play.params.rate / notes[own];
        assert!((0.994..=1.0061).contains(&off), "key {id}: {off}");
        assert!((0.86..=1.0).contains(&play.params.volume));
        assert!(near(play.params.pan, (2.0 * across - 1.0) * 0.6, 1e-5));
        assert_eq!(play.params.delay, 0.0);
        // Never the same sound twice running: the other take, and
        // another pitch and loudness.
        if let Some(last) = last {
            assert_ne!(last.sound, play.sound);
            assert_ne!(last.params.volume, play.params.volume);
        }
        last = Some(play);
    }
    assert_eq!(heard, [true; 4]);

    // One key, twenty times: twenty different sounds, one note.
    let mut seen: Vec<(SoundId, u32, u32)> = Vec::new();
    for _ in 0..20 {
        let play = app.decide(&digit(135, 0.5)).0[0];
        let this = (
            play.sound,
            play.params.rate.to_bits(),
            play.params.volume.to_bits(),
        );
        assert!(!seen.contains(&this));
        seen.push(this);
        assert!((0.994..=1.0061).contains(&(play.params.rate / notes[(135 * 7 % 4) as usize])));
    }

    // The converter's keypad chooses as its pulses do (`id * 5 % 4`),
    // and a key the calculator doesn't number rings the first note.
    let converter = gmnb_theme::events::Event {
        page: Page::Converter,
        ..digit(131, 0.5)
    };
    let play = app.decide(&converter).0[0];
    assert!(near(
        play.params.rate / notes[(131 * 5 % 4) as usize],
        1.0,
        0.0061
    ));
    let unnumbered = gmnb_theme::events::Event {
        key: Some((None, KeyKind::Number)),
        ..digit(0, 0.5)
    };
    let play = app.decide(&unnumbered).0[0];
    assert!(near(play.params.rate / notes[0], 1.0, 0.0061));

    // Another palette, another chord: Abyss's keys are D6, E6, G6, A6.
    assert!(!choose(&mut app, "abyss").0.is_empty());
    let abyss: Vec<f32> = ["D6", "E6", "G6", "A6"]
        .iter()
        .map(|n| speed(n, "E6"))
        .collect();
    for id in 130..140u32 {
        let play = app.decide(&digit(id, 0.5)).0[0];
        assert!(near(
            play.params.rate / abyss[(id * 7 % 4) as usize],
            1.0,
            0.0061
        ));
    }
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

#[test]
fn a_result_chimes_only_after_equals() {
    let theme = aurora();
    let mut app = StandIn::new(&theme);
    let (e6, a6) = (speed("E6", "A5"), speed("A6", "A5"));

    // 12 + 34 =: the operator's result is silent, the equals key's is the
    // chime, E6 then A6, left then right, 140 ms apart.
    assert_eq!(app.decide(&digit(131, 0.1)).1, By::Function);
    assert_eq!(app.decide(&operator()).0[0].sound, TOCK);
    assert_eq!(app.decide(&result()), (vec![], By::Function));
    app.decide(&digit(133, 0.1));
    assert_eq!(app.decide(&equals()).0[0].sound, SET);
    let (chime, by) = app.decide(&result());
    assert_eq!(by, By::Function);
    assert_eq!(chime.len(), 2);
    assert!(chime.iter().all(|p| p.sound == BELL_GLASS));
    assert!(near(chime[0].params.rate, e6, 1e-5) && near(chime[1].params.rate, a6, 1e-5));
    assert!(chime[0].params.pan < 0.0 && chime[1].params.pan > 0.0);
    // The first comes 40 to 80 ms after the key's own sound (the
    // chapter's "equals and result arrive together").
    assert!(near(chime[0].params.delay, 0.05, 1e-6) && near(chime[1].params.delay, 0.19, 1e-6));

    // = again and again repeats the operation: one soft note each time,
    // up the chord (A5, C#6, E6, G#6, and round).
    for step in 0..6 {
        app.decide(&equals());
        let (soft, _) = app.decide(&result());
        assert_eq!(soft.len(), 1, "= number {}", step + 2);
        let note = ["A5", "C#6", "E6", "G#6"][step % 4];
        assert!(near(soft[0].params.rate, speed(note, "A5"), 1e-5));
        assert!(soft[0].params.volume <= 0.5);
    }
    // Anything else pressed, and = chimes again.
    app.decide(&digit(134, 0.4));
    app.decide(&equals());
    assert_eq!(app.decide(&result()).0.len(), 2);

    // A function's result (sin, a square root) is one soft note.
    app.decide(&stand_in::key(40, KeyKind::Func, None, 0.3));
    let (soft, by) = app.decide(&result());
    assert_eq!((soft.len(), by), (1, By::Function));
    assert!(near(soft[0].params.rate, a6, 1e-5) && soft[0].params.volume <= 0.5);

    // An error is the table's thud, and after it = chimes afresh.
    app.decide(&equals());
    app.decide(&result());
    app.decide(&equals());
    let (thud, by) = app.decide(&stand_in::outcome(EventKind::Error, Page::Calculator));
    assert_eq!((thud[0].sound, by), (THUD, By::Table(EventName::Error)));
    app.decide(&equals());
    assert_eq!(app.decide(&result()).0.len(), 2);

    // The converter gives a result with every digit, and the date page
    // with every date: neither is a sound.
    for page in [Page::Converter, Page::Date] {
        let (plays, by) = app.decide(&stand_in::outcome(EventKind::Result, page));
        assert_eq!((plays.len(), by), (0, By::Nobody));
    }

    // Each palette's chime is two of its own notes on its own bell.
    for (palette, bell, low, high) in [
        ("ember", BELL_WARM, "C#6", "F#6"),
        ("abyss", BELL_BOWL, "G5", "D6"),
        ("orchard", BELL_GLASS, "D6", "F#6"),
        ("graphite", BELL_BOWL, "A5", "A6"),
        ("system", BELL_GLASS, "B5", "E6"),
        ("custom", BELL_GLASS, "B5", "E6"),
        ("aurora", BELL_GLASS, "E6", "A6"),
    ] {
        choose(&mut app, palette);
        app.decide(&digit(132, 0.6));
        app.decide(&equals());
        let (chime, _) = app.decide(&result());
        assert_eq!(chime.len(), 2, "{palette}");
        assert!(chime.iter().all(|p| p.sound == bell), "{palette}");
        assert!(
            near(chime[0].params.rate, speed(low, "A5"), 1e-5),
            "{palette}"
        );
        assert!(
            near(chime[1].params.rate, speed(high, "A5"), 1e-5),
            "{palette}"
        );
    }
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

#[test]
fn a_new_palette_plays_its_chord() {
    let theme = aurora();
    let mut app = StandIn::new(&theme);
    for (palette, bell, chord) in [
        ("aurora", BELL_GLASS, ["A5", "C#6", "E6", "G#6"]),
        ("ember", BELL_WARM, ["F#5", "A5", "C#6", "E6"]),
        ("abyss", BELL_BOWL, ["G5", "D6", "G6", "A6"]),
        ("orchard", BELL_GLASS, ["A5", "B5", "D6", "F#6"]),
        ("graphite", BELL_BOWL, ["A5", "D6", "E6", "A6"]),
        ("system", BELL_GLASS, ["A5", "B5", "E6", "F#6"]),
        ("custom", BELL_GLASS, ["A5", "B5", "E6", "F#6"]),
    ] {
        let (plays, by) = choose(&mut app, palette);
        assert_eq!((plays.len(), by), (4, By::Function), "{palette}");
        for (i, (play, note)) in plays.iter().zip(chord).enumerate() {
            assert_eq!(play.sound, bell, "{palette}");
            assert!(
                near(play.params.rate, speed(note, "A5"), 1e-5),
                "{palette} {note}"
            );
            // Left to right, where the background's four pulses rise.
            let x = 0.2 + 0.2 * i as f32;
            assert!(near(play.params.pan, (2.0 * x - 1.0) * 0.6, 1e-5));
            assert!(near(play.params.delay, 0.09 * i as f32, 1e-6));
        }
        // Every chord has the A that the sounds on A and E need.
        assert!(chord.iter().any(|n| n.starts_with('A')), "{palette}");
    }
    // A palette that isn't one (a saved value from another version)
    // sounds as Aurora.
    let (plays, _) = choose(&mut app, "nonsense");
    assert_eq!(plays[1].sound, BELL_GLASS);
    assert!(near(plays[1].params.rate, speed("C#6", "A5"), 1e-5));
    app.decide(&equals());
    assert_eq!(app.decide(&result()).0.len(), 2);

    // The other options make no sound: dragging through the colour
    // chooser changes one many times a second.
    for (id, value) in [
        ("living", OptionValue::Bool(false)),
        ("primary", OptionValue::Text("#ff0000".into())),
        ("secondary", OptionValue::Text("#00ff00".into())),
    ] {
        app.set_option(id, value.clone());
        let (plays, by) = app.decide(&stand_in::option(id, value));
        assert_eq!((plays.len(), by), (0, By::Nobody), "{id}");
    }
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

// ---- The chapter's lengths, in every palette -----------------------------------

#[test]
fn nothing_is_too_long_in_any_palette() {
    let theme = aurora();
    let made = prepare_all(&theme.sounds().sounds);
    let frames: Vec<usize> = made.iter().map(|m| m.as_ref().unwrap().len() / 2).collect();
    // How long after its event a play ends, in seconds.
    let ends = |play: &Play| {
        let p = play.params;
        // Nothing the function returns is out of a play's range.
        assert_eq!(p, p.clamped());
        let (slow, fast) = limits::PLAY_RATE;
        assert!(p.rate > slow && p.rate <= fast, "{p:?}");
        p.delay + frames[play.sound.index().unwrap()] as f32 / 48_000.0 / p.rate
    };
    let mut app = StandIn::new(&theme);
    for palette in PALETTES {
        // The chord itself: under the two seconds nothing needs more than.
        let (chord, _) = choose(&mut app, palette);
        let longest = chord.iter().map(&ends).fold(0.0, f32::max);
        assert!(longest <= 1.6, "{palette}: its chord lasts {longest} s");
        // A number key: 80 ms at most, on every key.
        for id in 0..64 {
            let play = app.decide(&digit(id, 0.5)).0[0];
            let length = ends(&play);
            assert!(length <= 0.07, "{palette}, key {id}: {length} s");
        }
        // The chime, and the soft notes of = pressed again.
        for _ in 0..6 {
            app.decide(&equals());
            let (plays, _) = app.decide(&result());
            let longest = plays.iter().map(&ends).fold(0.0, f32::max);
            assert!(longest <= 1.75, "{palette}: a result lasts {longest} s");
        }
    }
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

// ---- The function's 2 ms -----------------------------------------------------------

#[test]
fn the_sound_function_stays_inside_its_time() {
    assert_eq!(HOOK_TIME, Duration::from_millis(2));
    let theme = aurora();
    let hook = theme
        .def
        .sound
        .clone()
        .expect("Aurora has a sound function");
    let mut app = StandIn::new(&theme);
    // `ask` gives the function the 2 ms the app does, of processor time: a
    // call that ran out of it comes back as an error, and three of those
    // would switch the function off.
    let mut worst = Duration::ZERO;
    let mut total = Duration::ZERO;
    let mut calls = 0u32;
    let mut ask = |app: &mut StandIn, event: &gmnb_theme::events::Event| {
        let started = cpu();
        let answer = app.ask(event);
        let took = cpu() - started;
        worst = worst.max(took);
        total += took;
        calls += 1;
        match answer {
            Some(Ok(choice)) => choice,
            other => panic!("{event:?}: {other:?}"),
        }
    };

    // The first call of all, with nothing warmed up.
    assert!(matches!(ask(&mut app, &digit(131, 0.2)), Choice::Plays(p) if p.len() == 1));
    for round in 0..40u32 {
        for palette in PALETTES {
            let value = OptionValue::Text(palette.to_string());
            app.set_option("palette", value.clone());
            let chord = ask(&mut app, &stand_in::option("palette", value));
            assert!(matches!(chord, Choice::Plays(p) if p.len() == 4));
            for id in 0..12 {
                ask(&mut app, &digit(round * 12 + id, id as f32 / 11.0));
            }
            assert_eq!(ask(&mut app, &operator()), Choice::Table);
            assert_eq!(ask(&mut app, &result()), Choice::Silence);
            assert_eq!(ask(&mut app, &equals()), Choice::Table);
            assert!(matches!(ask(&mut app, &result()), Choice::Plays(p) if p.len() == 2));
            ask(&mut app, &equals());
            assert!(matches!(ask(&mut app, &result()), Choice::Plays(p) if p.len() == 1));
            for name in EventName::ALL {
                if let Some(event) = stand_in::named(name, round, 0.5) {
                    ask(&mut app, &event);
                }
            }
            let living = OptionValue::Bool(round % 2 == 0);
            assert_eq!(
                ask(&mut app, &stand_in::option("living", living)),
                Choice::Table
            );
            let elsewhere = stand_in::outcome(EventKind::Result, Page::Converter);
            assert_eq!(ask(&mut app, &elsewhere), Choice::Table);
        }
    }
    println!(
        "Aurora's sound function: {calls} calls, {:.1} us each, {:.1} us at worst (it has 2000)",
        total.as_secs_f64() * 1e6 / f64::from(calls),
        worst.as_secs_f64() * 1e6
    );
    assert!(calls > 8000);
    // Not one failure, so it was never near being switched off; and it
    // made no sound of its own on the way (one made there would miss its
    // event).
    assert_eq!((hook.strikes(), hook.is_disabled()), (0, false));
    assert_eq!(theme.sounds_declared(), SOUNDS);
    assert!(app.notes.is_empty(), "{:?}", app.notes);
    assert!(theme.take_sound_notes().is_empty());
    // And with room to spare: on average, under a twentieth of its time
    // (the worst call is only printed: one page fault can be all of it).
    assert!(total / calls < HOOK_TIME / 20, "{:?}", total / calls);
}
