//! Swirls' sounds (apps/gmnb/themes/swirls/theme.luau, "8. Sound"): they
//! load, every event plays what the theme means it to, a result rings the
//! notes that were typed, its `sound` function stays inside its 2 ms, and
//! its bank is small.
//!
//! The events are made by the stand-in for the app that the render
//! example uses, and decided by the app's own code
//! (`gmnb_theme::sound::decide`), so what is tested here is what
//! `tools/theming/sound_check.py` measures and what the app plays.

#[path = "../../gmnb-sound/examples/support/stand_in.rs"]
mod stand_in;

mod common;

use std::time::Duration;

use gmnb_sound::{Effect, EventName, Play, Recipe, SoundId, SoundSource, limits, prepare_all};
use gmnb_theme::OptionValue;
use gmnb_theme::events::{Action, Cause, Event, EventKind, KeyKind, Page, Source as From};
use gmnb_theme::sound::{Choice, HOOK_TIME};
use gmnb_theme::theme::{Source, Theme};
use stand_in::{By, StandIn};

// The sounds, by the order the theme makes them in.
const DROP_A: SoundId = SoundId(1);
const DROP_D: SoundId = SoundId(2);
const PLOP: SoundId = SoundId(3);
const BEAD: SoundId = SoundId(4);
const PLUNGE: SoundId = SoundId(5);
const PAIR: SoundId = SoundId(6);
const BACK: SoundId = SoundId(7);
const WIPE: SoundId = SoundId(8);
const RING: SoundId = SoundId(9);
const GLUG: SoundId = SoundId(10);
const STIRRED: SoundId = SoundId(11);
const SOUNDS: usize = 11;

/// The scale the number keys fall on: each note, the drop that plays it
/// and the note that drop is made on.
const SCALE: [(&str, SoundId, &str); 7] = [
    ("F#5", DROP_A, "A5"),
    ("A5", DROP_A, "A5"),
    ("B5", DROP_A, "A5"),
    ("D6", DROP_D, "D6"),
    ("E6", DROP_D, "D6"),
    ("F#6", DROP_D, "D6"),
    ("A6", DROP_D, "D6"),
];

/// What each set of bands pours in as: notes of the scale, from 0.
const POURS: [(&str, &[usize]); 8] = [
    ("party", &[1, 3, 4, 6]),
    ("lava", &[0, 1, 3, 5]),
    ("lagoon", &[1, 2, 4, 6]),
    ("orchid", &[0, 2, 3, 5]),
    ("absinthe", &[2, 3, 5, 6]),
    ("marble", &[1, 3, 6]),
    // Fourths, F#5 B5 E6 A6; and D major's chord, D6 F#6 A6.
    ("system", &[0, 2, 4, 6]),
    ("custom", &[3, 5, 6]),
];

fn swirls() -> Theme {
    Theme::load(Source::built_in("swirls").unwrap(), common::limits())
        .unwrap_or_else(|e| panic!("Swirls didn't load: {e}"))
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

/// The speed of the ring (it is made on A5) for a note of the scale.
fn ring(note: usize) -> f32 {
    speed(SCALE[note].0, "A5")
}

/// Which note of the scale a key falls on.
fn note_of(id: u32) -> usize {
    (id * 3 % 7) as usize
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

fn digit(id: u32, across: f32) -> Event {
    stand_in::key(id, KeyKind::Number, None, across)
}

fn equals() -> Event {
    stand_in::key(30, KeyKind::Equals, None, 0.875)
}

fn operator() -> Event {
    stand_in::key(20, KeyKind::Operator, None, 0.875)
}

fn result() -> Event {
    stand_in::outcome(EventKind::Result, Page::Calculator)
}

fn result_after(cause: Cause) -> Event {
    stand_in::outcome_after(EventKind::Result, Page::Calculator, cause)
}

/// Sets the bands as the app does: `ctx` first, then the event.
fn choose(app: &mut StandIn, bands: &str) -> (Vec<Play>, By) {
    let value = OptionValue::Text(bands.to_string());
    app.set_option("bands", value.clone());
    app.decide(&stand_in::option("bands", value))
}

// ---- They load -----------------------------------------------------------------

#[test]
fn swirls_sounds_load() {
    let theme = swirls();
    assert!(theme.notes.is_empty(), "{:?}", theme.notes);
    let sounds = theme.sounds();
    assert!(sounds.hook && sounds.any());
    assert_eq!(sounds.sounds.len(), SOUNDS);
    assert_eq!(theme.sounds_declared(), SOUNDS);
    let notes = theme.take_sound_notes();
    assert!(notes.is_empty(), "{notes:?}");

    // Every one is a recipe that renders, at no more than the gain a
    // built-in theme may have, and with nothing under 120 Hz let through:
    // each ends in a high-pass, or is high-passed before its room.
    let made = prepare_all(&sounds.sounds);
    for (i, (recipe, made)) in recipes(&theme).iter().zip(&made).enumerate() {
        let samples = made
            .as_ref()
            .unwrap_or_else(|e| panic!("sound {}: {e}", i + 1));
        assert!(samples.len() >= 2, "sound {} is silent", i + 1);
        assert!(recipe.gain <= 0.5, "sound {}: gain {}", i + 1, recipe.gain);
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak <= 16_384, "sound {}", i + 1);
        assert!(
            matches!(recipe.effects.first(), Some(Effect::Filter(_))),
            "sound {}: {:?}",
            i + 1,
            recipe.effects.first()
        );
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
    let theme = swirls();
    let sounds = theme.sounds();
    let recipes = recipes(&theme);

    // What bounds the work of making it, whatever the machine is doing.
    // A room (a reverb or a delay) is rendered for the sound's whole
    // `length`, or for 4 s if it has none: each of Swirls' has one.
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
    assert_eq!(rooms, 2, "the ring and the stir");
    assert!(
        layer_seconds <= limits::LAYER_SECONDS / 10.0,
        "{layer_seconds} layer-seconds: over a tenth of what a theme may ask for"
    );

    let started = cpu();
    let made = prepare_all(&sounds.sounds);
    let took = cpu() - started;
    let bytes: usize = made.iter().flatten().map(|s| s.len() * 2).sum();
    assert!(
        bytes <= limits::TOTAL_BYTES / 10,
        "{bytes} bytes: over a tenth of what a theme may hold"
    );
    println!(
        "Swirls' bank: {SOUNDS} sounds, {layer_seconds:.2} layer-seconds, {bytes} bytes, made in {:.1} ms of processor time",
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
    let theme = swirls();
    let mut app = StandIn::new(&theme);
    let mut one = |event: &Event| {
        let (plays, by) = app.decide(event);
        assert_eq!(plays.len(), 1, "{event:?}");
        (plays[0], by)
    };
    let key = |kind, action| stand_in::key(7, kind, action, 0.25);

    for (event, name, sound) in [
        (key(KeyKind::Operator, None), EventName::Operator, PLOP),
        (key(KeyKind::Func, None), EventName::Func, BEAD),
        (key(KeyKind::Other, None), EventName::Func, BEAD),
        (key(KeyKind::Equals, None), EventName::Equals, PLUNGE),
        (key(KeyKind::Toggle, None), EventName::Toggle, PAIR),
        (
            key(KeyKind::Func, Some(Action::Clear)),
            EventName::Clear,
            WIPE,
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
            PLOP,
        ),
        (
            stand_in::button(From::Button, None, Page::Calculator, 0.25),
            EventName::Button,
            BEAD,
        ),
        (
            stand_in::button(From::Swap, None, Page::Converter, 0.5),
            EventName::Button,
            BEAD,
        ),
        (
            stand_in::button(From::Date, None, Page::Date, 0.5),
            EventName::Button,
            BEAD,
        ),
        (stand_in::mode(Page::Graphing), EventName::Mode, STIRRED),
        (
            stand_in::outcome(EventKind::Error, Page::Calculator),
            EventName::Error,
            GLUG,
        ),
    ] {
        let (play, by) = one(&event);
        assert_eq!(by, By::Table(name), "{event:?}");
        assert_eq!(play.sound, sound, "{event:?}");
        // No key sound is played slower than this: the 80 ms rule is
        // measured at the slowest.
        assert!(play.params.rate >= 0.92, "{event:?}");
    }

    // Where a key is across the window is where it sounds, at 60% width.
    let (left, _) = one(&stand_in::key(7, KeyKind::Operator, None, 0.0));
    let (right, _) = one(&stand_in::key(7, KeyKind::Operator, None, 1.0));
    assert!(near(left.params.pan, -0.6, 1e-6) && near(right.params.pan, 0.6, 1e-6));
    // The memory row's drop is the operators' a fourth up; the error
    // waits for the readout's first swing.
    let (memory, _) = one(&stand_in::button(
        From::Button,
        Some(Action::Memory),
        Page::Calculator,
        0.5,
    ));
    assert!(near(memory.params.rate, 1.335, 1e-6));
    let (error, _) = one(&stand_in::outcome(EventKind::Error, Page::Calculator));
    assert!(near(error.params.delay, 0.03, 1e-6));
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

#[test]
fn number_keys_fall_on_the_scale() {
    let theme = swirls();
    let mut app = StandIn::new(&theme);
    let mut heard = [false; 7];
    let mut last: Option<Play> = None;
    for press in 0..80u32 {
        let id = press % 10 + 130;
        let across = (press % 4) as f32 / 3.0;
        let (plays, by) = app.decide(&digit(id, across));
        assert_eq!((plays.len(), by), (1, By::Function));
        let play = plays[0];
        // The key's own note, on the drop that plays it, a tenth of a
        // semitone around it.
        let (note, drop, home) = SCALE[note_of(id)];
        heard[note_of(id)] = true;
        assert_eq!(play.sound, drop, "key {id}");
        let off = play.params.rate / speed(note, home);
        assert!((0.994..=1.0061).contains(&off), "key {id}: {off}");
        assert!((0.86..=1.0).contains(&play.params.volume));
        assert!(near(play.params.pan, (2.0 * across - 1.0) * 0.6, 1e-5));
        assert_eq!(play.params.delay, 0.0);
        // Never the same sound twice running: another pitch and loudness.
        if let Some(last) = last {
            assert_ne!(last.params.volume, play.params.volume);
            assert_ne!(last.params.rate, play.params.rate);
        }
        last = Some(play);
    }
    assert_eq!(heard, [true; 7]);
    // Keys next to each other are a third or more apart, never a second:
    // a run up the keypad is a tune, not a scale.
    for id in 130..139u32 {
        let (a, b) = (note_of(id) as i32, note_of(id + 1) as i32);
        assert!((a - b).abs() >= 3, "keys {id} and {}", id + 1);
    }

    // One key, twenty times: twenty different sounds, one note.
    let mut seen: Vec<(u32, u32)> = Vec::new();
    for _ in 0..20 {
        let play = app.decide(&digit(135, 0.5)).0[0];
        let this = (play.params.rate.to_bits(), play.params.volume.to_bits());
        assert!(!seen.contains(&this));
        seen.push(this);
        let (note, _, home) = SCALE[note_of(135)];
        assert!((0.994..=1.0061).contains(&(play.params.rate / speed(note, home))));
    }

    // A key the calculator doesn't number falls on the first note, and
    // the converter's keypad is the same instrument.
    let unnumbered = Event {
        key: Some((None, KeyKind::Number)),
        ..digit(0, 0.5)
    };
    let play = app.decide(&unnumbered).0[0];
    assert!(near(play.params.rate / speed("F#5", "A5"), 1.0, 0.0061));
    let converter = Event {
        page: Page::Converter,
        ..digit(131, 0.5)
    };
    let play = app.decide(&converter).0[0];
    let (note, drop, home) = SCALE[note_of(131)];
    assert_eq!(play.sound, drop);
    assert!(near(play.params.rate / speed(note, home), 1.0, 0.0061));
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

#[test]
fn a_result_rings_what_was_typed() {
    let theme = swirls();
    let mut app = StandIn::new(&theme);
    let rings = |app: &mut StandIn| {
        let (plays, by) = app.decide(&result());
        assert_eq!(by, By::Function);
        assert!(plays.iter().all(|p| p.sound == RING));
        plays
    };
    // What three rings are: the notes given, lowest first, left to right,
    // a tenth of a second apart, each a little louder than the last, the
    // first just after the equals key's own drop.
    let check = |plays: &[Play], notes: &[usize]| {
        assert_eq!(plays.len(), notes.len());
        for (i, (play, note)) in plays.iter().zip(notes).enumerate() {
            assert!(
                near(play.params.rate, ring(*note), 1e-5),
                "ring {i}: {} is not {}",
                play.params.rate,
                SCALE[*note].0
            );
            assert!(near(play.params.delay, 0.06 + 0.1 * i as f32, 1e-6));
            assert!(near(play.params.volume, 0.72 + 0.1 * i as f32, 1e-6));
        }
        if plays.len() == 3 {
            assert!(plays[0].params.pan < 0.0 && plays[2].params.pan > 0.0);
            assert_eq!(plays[1].params.pan, 0.0);
        }
    };

    // 1 2 + 3 4 =: the operator's running total is silent (its own drop
    // said it), and the equals key's result rings the last three notes
    // typed, lowest first.
    let keys = [131u32, 132, 133, 134];
    app.decide(&digit(keys[0], 0.1));
    app.decide(&digit(keys[1], 0.3));
    assert_eq!(app.decide(&operator()).0[0].sound, PLOP);
    assert_eq!(
        app.decide(&result_after(Cause::Operator)),
        (vec![], By::Nobody)
    );
    app.decide(&digit(keys[2], 0.5));
    app.decide(&digit(keys[3], 0.7));
    assert_eq!(app.decide(&equals()).0[0].sound, PLUNGE);
    let mut typed: Vec<usize> = keys[1..].iter().map(|id| note_of(*id)).collect();
    typed.sort();
    check(&rings(&mut app), &typed);

    // = again repeats the operation, with nothing typed: the same rings,
    // each a step up the scale, and round.
    for _ in 0..3 {
        app.decide(&equals());
        typed = typed.iter().map(|n| (n + 1) % 7).collect();
        typed.sort();
        check(&rings(&mut app), &typed);
    }

    // One key, pressed many times: one ring, not three of one note. Two
    // keys: two.
    for _ in 0..5 {
        app.decide(&digit(135, 0.5));
    }
    app.decide(&equals());
    check(&rings(&mut app), &[note_of(135)]);
    app.decide(&digit(135, 0.5));
    app.decide(&digit(136, 0.5));
    app.decide(&digit(135, 0.5));
    app.decide(&equals());
    let mut two = vec![note_of(135), note_of(136)];
    two.sort();
    check(&rings(&mut app), &two);

    // What is deleted is not rung: backspace takes the last note back,
    // and C all of them (the rings are then the last ones, a step up).
    for id in [131, 132, 133, 134] {
        app.decide(&digit(id, 0.5));
    }
    app.decide(&stand_in::key(
        50,
        KeyKind::Other,
        Some(Action::Backspace),
        0.9,
    ));
    app.decide(&equals());
    let mut kept: Vec<usize> = [131u32, 132, 133].iter().map(|id| note_of(*id)).collect();
    kept.sort();
    check(&rings(&mut app), &kept);
    app.decide(&digit(137, 0.5));
    app.decide(&stand_in::key(51, KeyKind::Func, Some(Action::Clear), 0.5));
    app.decide(&equals());
    let mut up: Vec<usize> = kept.iter().map(|n| (n + 1) % 7).collect();
    up.sort();
    check(&rings(&mut app), &up);

    // A function's result (sin, a square root) is one ring on the last
    // note typed, or on D6 if none was.
    app.decide(&digit(138, 0.5));
    app.decide(&stand_in::key(40, KeyKind::Func, None, 0.3));
    let (one, by) = app.decide(&result_after(Cause::Function));
    assert_eq!((one.len(), by), (1, By::Function));
    assert_eq!(one[0].sound, RING);
    assert!(near(one[0].params.rate, ring(note_of(138)), 1e-5));
    assert!(near(one[0].params.delay, 0.06, 1e-6));
    app.decide(&equals());
    app.decide(&result());
    app.decide(&stand_in::key(40, KeyKind::Func, None, 0.3));
    let (one, _) = app.decide(&result_after(Cause::Function));
    assert!(near(one[0].params.rate, ring(3), 1e-5));

    // An error is the table's glug.
    app.decide(&equals());
    let (glug, by) = app.decide(&stand_in::outcome(EventKind::Error, Page::Calculator));
    assert_eq!((glug[0].sound, by), (GLUG, By::Table(EventName::Error)));

    // The converter gives a result with every digit, and the date page
    // with every date: neither is a sound. Nor is the value a digit shows.
    for page in [Page::Converter, Page::Date] {
        let (plays, by) = app.decide(&stand_in::outcome(EventKind::Result, page));
        assert_eq!((plays.len(), by), (0, By::Nobody));
    }
    assert_eq!(
        app.decide(&result_after(Cause::Digit)),
        (vec![], By::Nobody)
    );
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

#[test]
fn new_paint_pours_in_on_its_own_notes() {
    let theme = swirls();
    let mut app = StandIn::new(&theme);
    for (bands, notes) in POURS {
        let (plays, by) = choose(&mut app, bands);
        assert_eq!((plays.len(), by), (notes.len(), By::Function), "{bands}");
        for (i, (play, note)) in plays.iter().zip(notes).enumerate() {
            assert_eq!(play.sound, RING, "{bands}");
            assert!(near(play.params.rate, ring(*note), 1e-5), "{bands} {i}");
            assert!(near(play.params.delay, 0.09 * i as f32, 1e-6));
            // Left to right.
            let x = i as f32 / (notes.len() - 1) as f32;
            assert!(near(play.params.pan, (2.0 * x - 1.0) * 0.5, 1e-5));
        }
        // Up the scale.
        assert!(notes.windows(2).all(|w| w[0] < w[1]), "{bands}");
    }
    // Bands that aren't a set (a saved value from another version) pour
    // as the page's own.
    let (plays, _) = choose(&mut app, "nonsense");
    assert_eq!(plays.len(), 4);
    assert!(near(plays[1].params.rate, ring(3), 1e-5));

    // The other options make no sound: the pace is a slider, dragged, and
    // so is a colour in the chooser.
    for (id, value) in [
        ("paint_first", OptionValue::Text("#ff0000".into())),
        ("paint_second", OptionValue::Text("#00ffff".into())),
        ("paint_dark", OptionValue::Text("#000000".into())),
        ("glass", OptionValue::Text("clear".into())),
        ("speed", OptionValue::Number(2.0)),
        ("stir", OptionValue::Bool(false)),
        ("digits", OptionValue::Bool(false)),
    ] {
        app.set_option(id, value.clone());
        let (plays, by) = app.decide(&stand_in::option(id, value));
        assert_eq!((plays.len(), by), (0, By::Nobody), "{id}");
    }
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

// ---- The chapter's lengths ----------------------------------------------------------

#[test]
fn nothing_is_too_long() {
    let theme = swirls();
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
    // A number key: under 80 ms, on every key. (The lowest note is the
    // longest: its drop is played at 0.84.)
    let mut longest = 0.0f32;
    for id in 0..64 {
        let play = app.decide(&digit(id, 0.5)).0[0];
        longest = longest.max(ends(&play));
    }
    assert!(longest <= 0.078, "a number key lasts {longest} s");
    // The rings of a result, whatever was typed, and of = again.
    for round in 0..40u32 {
        for id in 0..(round % 5) {
            app.decide(&digit(round * 7 + id, 0.5));
        }
        app.decide(&equals());
        let (plays, _) = app.decide(&result());
        assert!((1..=3).contains(&plays.len()), "{}", plays.len());
        let longest = plays.iter().map(&ends).fold(0.0, f32::max);
        assert!(longest <= 1.75, "a result lasts {longest} s");
    }
    // Each set's pour.
    for (bands, _) in POURS {
        let (pour, _) = choose(&mut app, bands);
        let longest = pour.iter().map(&ends).fold(0.0, f32::max);
        assert!(longest <= 1.75, "{bands}: its pour lasts {longest} s");
    }
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

// ---- The function's 2 ms -----------------------------------------------------------

#[test]
fn the_sound_function_stays_inside_its_time() {
    assert_eq!(HOOK_TIME, Duration::from_millis(2));
    let theme = swirls();
    let hook = theme
        .def
        .sound
        .clone()
        .expect("Swirls has a sound function");
    let mut app = StandIn::new(&theme);
    // `ask` gives the function the 2 ms the app does, of processor time: a
    // call that ran out of it comes back as an error, and three of those
    // would switch the function off.
    let mut worst = Duration::ZERO;
    let mut total = Duration::ZERO;
    let mut calls = 0u32;
    let mut ask = |app: &mut StandIn, event: &Event| {
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
        for (bands, notes) in POURS {
            let value = OptionValue::Text(bands.to_string());
            app.set_option("bands", value.clone());
            let pour = ask(&mut app, &stand_in::option("bands", value));
            assert!(matches!(pour, Choice::Plays(p) if p.len() == notes.len()));
            for id in 0..12 {
                ask(&mut app, &digit(round * 12 + id, id as f32 / 11.0));
            }
            assert_eq!(ask(&mut app, &operator()), Choice::Table);
            assert_eq!(ask(&mut app, &result_after(Cause::Operator)), Choice::Table);
            assert_eq!(ask(&mut app, &equals()), Choice::Table);
            assert!(matches!(ask(&mut app, &result()), Choice::Plays(p) if p.len() == 3));
            ask(&mut app, &equals());
            assert!(matches!(ask(&mut app, &result()), Choice::Plays(p) if p.len() == 3));
            for name in EventName::ALL {
                if let Some(event) = stand_in::named(name, round, 0.5) {
                    ask(&mut app, &event);
                }
            }
            let stir = OptionValue::Bool(round % 2 == 0);
            assert_eq!(
                ask(&mut app, &stand_in::option("stir", stir)),
                Choice::Table
            );
            let elsewhere = stand_in::outcome(EventKind::Result, Page::Converter);
            assert_eq!(ask(&mut app, &elsewhere), Choice::Table);
        }
    }
    println!(
        "Swirls' sound function: {calls} calls, {:.1} us each, {:.1} us at worst (it has 2000)",
        total.as_secs_f64() * 1e6 / f64::from(calls),
        worst.as_secs_f64() * 1e6
    );
    assert!(calls > 6000);
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
