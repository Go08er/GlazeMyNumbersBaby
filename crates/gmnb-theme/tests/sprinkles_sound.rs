//! Sprinkles' sounds (apps/gmnb/themes/sprinkles/theme.luau, "9. Sound"): they
//! load, every event plays what the theme means it to (number keys the
//! notes of the flavour's scale, a result after = the flavour's jingle),
//! nothing is longer than the sound chapter allows in any flavour, its
//! `sound` function stays inside its 2 ms, and its bank is small.
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
use gmnb_theme::events::{Action, Cause, EventKind, KeyKind, Page, Source as From};
use gmnb_theme::sound::{Choice, HOOK_TIME};
use gmnb_theme::theme::{Source, Theme};
use stand_in::{By, StandIn};

// The sounds, by the order the theme makes them in.
const BLIP_A: SoundId = SoundId(1);
const BLIP_B: SoundId = SoundId(2);
const BOP: SoundId = SoundId(3);
const TAP: SoundId = SoundId(4);
const SET: SoundId = SoundId(5);
const FLIP: SoundId = SoundId(6);
const BACK: SoundId = SoundId(7);
const SWEEP: SoundId = SoundId(8);
const JAR: SoundId = SoundId(9);
const NOTE: SoundId = SoundId(10);
const PATTER: SoundId = SoundId(11);
const WOMP: SoundId = SoundId(12);
const DOOR: SoundId = SoundId(13);
const SOUNDS: usize = 13;

/// A flavour's notes: its five for the number keys, its jingle's four,
/// the note its operators play and its home note.
struct Flavour {
    id: &'static str,
    keys: [&'static str; 5],
    jingle: [&'static str; 4],
    low: &'static str,
    home: &'static str,
}

const FLAVOURS: [Flavour; 5] = [
    Flavour {
        id: "strawberry",
        keys: ["C5", "D5", "E5", "G5", "A5"],
        jingle: ["C5", "E5", "G5", "C6"],
        low: "G4",
        home: "C5",
    },
    Flavour {
        id: "blueberry",
        keys: ["A4", "C5", "D5", "E5", "G5"],
        jingle: ["A4", "C5", "E5", "A5"],
        low: "E4",
        home: "A4",
    },
    Flavour {
        id: "matcha",
        keys: ["D5", "Eb5", "G5", "A5", "C6"],
        jingle: ["D5", "G5", "A5", "D6"],
        low: "A4",
        home: "D5",
    },
    Flavour {
        id: "lemon",
        keys: ["D5", "E5", "F#5", "A5", "B5"],
        jingle: ["D5", "F#5", "A5", "D6"],
        low: "A4",
        home: "D5",
    },
    Flavour {
        id: "caramel",
        keys: ["F4", "G4", "A4", "C5", "D5"],
        jingle: ["F4", "A4", "C5", "F5"],
        low: "F4",
        home: "F4",
    },
];

fn sprinkles() -> Theme {
    Theme::load(Source::built_in("sprinkles").unwrap(), common::limits())
        .unwrap_or_else(|e| panic!("Sprinkles didn't load: {e}"))
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

type Event = gmnb_theme::events::Event;

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

/// Sets the flavour as the app does: `ctx` first, then the event.
fn choose(app: &mut StandIn, flavour: &str) -> (Vec<Play>, By) {
    let value = OptionValue::Text(flavour.to_string());
    app.set_option("flavour", value.clone());
    app.decide(&stand_in::option("flavour", value))
}

// ---- They load -----------------------------------------------------------------

#[test]
fn sprinkles_sounds_load() {
    let theme = sprinkles();
    assert!(theme.notes.is_empty(), "{:?}", theme.notes);
    let sounds = theme.sounds();
    assert!(sounds.hook && sounds.any());
    assert_eq!(sounds.sounds.len(), SOUNDS);
    assert_eq!(theme.sounds_declared(), SOUNDS);
    let notes = theme.take_sound_notes();
    assert!(notes.is_empty(), "{notes:?}");

    // Every one is a recipe that renders, at no more than the gain a
    // built-in theme may have; and none is a file.
    let made = prepare_all(&sounds.sounds);
    for (i, (recipe, made)) in recipes(&theme).iter().zip(&made).enumerate() {
        let samples = made
            .as_ref()
            .unwrap_or_else(|e| panic!("sound {}: {e}", i + 1));
        assert!(samples.len() >= 2, "sound {} is silent", i + 1);
        assert!(recipe.gain <= 0.25, "sound {}: gain {}", i + 1, recipe.gain);
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak <= 8_192, "sound {}", i + 1);
        // Chip-tune: every one has something under it taken out (the
        // thump of a square wave switched on), so none carries an offset.
        assert!(
            recipe
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Filter(_))),
            "sound {}",
            i + 1
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
    let theme = sprinkles();
    let sounds = theme.sounds();
    let recipes = recipes(&theme);

    // What bounds the work of making it, whatever the machine is doing.
    // An echo is rendered for the sound's whole `length`, or for 4 s if
    // it has none: each of Sprinkles' two has one.
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
            assert!(length <= 1.0, "sound {}: {length} s of echo", i + 1);
        }
    }
    assert_eq!(rooms, 2, "the jingle's voice and the door's bell");
    assert!(
        layer_seconds <= limits::LAYER_SECONDS / 20.0,
        "{layer_seconds} layer-seconds: over a twentieth of what a theme may ask for"
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
        "Sprinkles' bank: {SOUNDS} sounds, {layer_seconds:.2} layer-seconds, {bytes} bytes, made in {:.1} ms of processor time",
        took.as_secs_f64() * 1000.0
    );
    // Processor time, so a busy machine doesn't fail this; a build
    // without optimisation is allowed ten times as long.
    let allowed = if cfg!(debug_assertions) { 1500 } else { 150 };
    assert!(took < Duration::from_millis(allowed), "{took:?}");
}

// ---- Every event --------------------------------------------------------------

#[test]
fn keys_and_buttons_play_what_the_table_says() {
    let theme = sprinkles();
    let mut app = StandIn::new(&theme);
    let mut one = |event: &Event| {
        let (plays, by) = app.decide(event);
        assert_eq!(plays.len(), 1, "{event:?}");
        (plays[0], by)
    };
    let key = |kind, action| stand_in::key(7, kind, action, 0.25);

    for (event, name, sound) in [
        (key(KeyKind::Func, None), EventName::Func, TAP),
        (key(KeyKind::Other, None), EventName::Func, TAP),
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
            JAR,
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
        (stand_in::mode(Page::Graphing), EventName::Mode, DOOR),
        (
            stand_in::outcome(EventKind::Error, Page::Calculator),
            EventName::Error,
            WOMP,
        ),
    ] {
        let (play, by) = one(&event);
        assert_eq!(by, By::Table(name), "{event:?}");
        assert_eq!(play.sound, sound, "{event:?}");
        // None of these is played slower than this: the chapter's
        // lengths are measured at the slowest.
        assert!(play.params.rate >= 0.94, "{event:?}");
    }

    // Where a key is across the window is where it sounds, at 60% width.
    let (left, _) = one(&stand_in::key(7, KeyKind::Func, None, 0.0));
    let (right, _) = one(&stand_in::key(7, KeyKind::Func, None, 1.0));
    assert!(near(left.params.pan, -0.6, 1e-6) && near(right.params.pan, 0.6, 1e-6));
    // The error waits for the number's first shake of its head.
    let (error, _) = one(&stand_in::outcome(EventKind::Error, Page::Calculator));
    assert!(near(error.params.delay, 0.03, 1e-6));
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

#[test]
fn number_keys_play_the_flavours_scale() {
    let theme = sprinkles();
    let mut app = StandIn::new(&theme);
    for flavour in &FLAVOURS {
        choose(&mut app, flavour.id);
        let notes: Vec<f32> = flavour.keys.iter().map(|n| speed(n, "C5")).collect();
        let mut heard = [false; 5];
        let mut last: Option<Play> = None;
        for press in 0..80u32 {
            let id = press % 10 + 130;
            let across = (press % 4) as f32 / 3.0;
            let (plays, by) = app.decide(&digit(id, across));
            assert_eq!((plays.len(), by), (1, By::Function));
            let play = plays[0];
            assert!(play.sound == BLIP_A || play.sound == BLIP_B);
            // The key's own note, exactly: a tune, not a cloud.
            let own = (id * 7 % 5) as usize;
            heard[own] = true;
            assert!(
                near(play.params.rate, notes[own], 1e-5),
                "{}, key {id}",
                flavour.id
            );
            assert!((0.88..=1.0).contains(&play.params.volume));
            assert!(near(play.params.pan, (2.0 * across - 1.0) * 0.6, 1e-5));
            assert_eq!(play.params.delay, 0.0);
            // Never the same sound twice running: the other take, and
            // another loudness.
            if let Some(last) = last {
                assert_ne!(last.sound, play.sound);
                assert_ne!(last.params.volume, play.params.volume);
            }
            last = Some(play);
        }
        // Ten keys, five notes: every note of the scale is some key's.
        assert_eq!(heard, [true; 5], "{}", flavour.id);

        // The operators play the flavour's low note, the equals key its
        // home note.
        let (plays, by) = app.decide(&operator());
        assert_eq!((plays.len(), by), (1, By::Function));
        assert_eq!(plays[0].sound, BOP);
        assert!(near(plays[0].params.rate, speed(flavour.low, "G4"), 1e-5));
        let (plays, by) = app.decide(&equals());
        assert_eq!((plays.len(), by), (1, By::Function));
        assert_eq!(plays[0].sound, SET);
        assert!(near(plays[0].params.rate, speed(flavour.home, "C5"), 1e-5));
    }

    // A key the calculator doesn't number plays the first note; and a
    // flavour that isn't one (a saved value from another version) sounds
    // as strawberry.
    choose(&mut app, "nonsense");
    let unnumbered = Event {
        key: Some((None, KeyKind::Number)),
        ..digit(0, 0.5)
    };
    let play = app.decide(&unnumbered).0[0];
    assert!(near(play.params.rate, 1.0, 1e-5));
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

#[test]
fn a_result_jingles_only_after_equals() {
    let theme = sprinkles();
    let mut app = StandIn::new(&theme);
    for flavour in &FLAVOURS {
        choose(&mut app, flavour.id);
        let jingle: Vec<f32> = flavour.jingle.iter().map(|n| speed(n, "E5")).collect();
        let home = speed(flavour.home, "C5");

        // 12 + 34 =: the operator's running total is silent; the equals
        // key's result is the jingle, three notes rising, left to
        // right, 80 ms apart, and the patter of sprinkles after them.
        app.decide(&digit(131, 0.1));
        app.decide(&operator());
        assert_eq!(
            app.decide(&result_after(Cause::Operator)),
            (vec![], By::Function)
        );
        app.decide(&digit(133, 0.1));
        app.decide(&equals());
        let (plays, by) = app.decide(&result());
        assert_eq!((plays.len(), by), (4, By::Function), "{}", flavour.id);
        for (i, play) in plays[..3].iter().enumerate() {
            assert_eq!(play.sound, NOTE);
            assert!(near(play.params.rate, jingle[i], 1e-5), "{}", flavour.id);
            // The first comes 90 ms after the key's own two notes began:
            // as they end.
            assert!(near(play.params.delay, 0.09 + 0.08 * i as f32, 1e-6));
        }
        assert!(plays[0].params.pan < 0.0 && plays[2].params.pan > 0.0);
        assert_eq!(plays[3].sound, PATTER);
        assert!(near(plays[3].params.rate, home, 1e-5));
        // The sprinkles land on the number 0.24 to 0.69 s after the
        // result (theme.luau, `POUR` and `SPRINKLE_TIME`): the patter
        // starts among them.
        assert!((0.24..0.5).contains(&plays[3].params.delay));

        // = again and again repeats the operation: the jingle's top
        // note and a quieter patter, each time.
        for _ in 0..3 {
            app.decide(&equals());
            let (soft, _) = app.decide(&result());
            assert_eq!(soft.len(), 2);
            assert_eq!((soft[0].sound, soft[1].sound), (NOTE, PATTER));
            assert!(near(soft[0].params.rate, jingle[3], 1e-5));
            assert!(soft[0].params.volume <= 0.6 && soft[1].params.volume <= 0.7);
        }
        // Anything else pressed, and = jingles again.
        app.decide(&digit(134, 0.4));
        app.decide(&equals());
        assert_eq!(app.decide(&result()).0.len(), 4);

        // A function's result (a sine, a square root) is one note.
        app.decide(&stand_in::key(40, KeyKind::Func, None, 0.3));
        let (soft, by) = app.decide(&result_after(Cause::Function));
        assert_eq!((soft.len(), by), (1, By::Function));
        assert_eq!(soft[0].sound, NOTE);
        assert!(near(soft[0].params.rate, jingle[2], 1e-5) && soft[0].params.volume <= 0.6);

        // What a digit, a clear or a memory button puts in the readout
        // is no result to sing about.
        for cause in [Cause::Digit, Cause::Clear, Cause::Backspace, Cause::Memory] {
            assert_eq!(app.decide(&result_after(cause)), (vec![], By::Function));
        }
    }

    // An error is the table's, and after it = jingles afresh.
    app.decide(&equals());
    app.decide(&result());
    app.decide(&equals());
    let (womp, by) = app.decide(&stand_in::outcome(EventKind::Error, Page::Calculator));
    assert_eq!((womp[0].sound, by), (WOMP, By::Table(EventName::Error)));
    app.decide(&equals());
    assert_eq!(app.decide(&result()).0.len(), 4);

    // The converter gives a result with every digit, and the date page
    // with every date: neither is a sound.
    for page in [Page::Converter, Page::Date] {
        let (plays, by) = app.decide(&stand_in::outcome(EventKind::Result, page));
        assert_eq!((plays.len(), by), (0, By::Nobody));
    }
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

#[test]
fn a_new_flavour_plays_its_jingle() {
    let theme = sprinkles();
    let mut app = StandIn::new(&theme);
    for flavour in &FLAVOURS {
        let (plays, by) = choose(&mut app, flavour.id);
        assert_eq!((plays.len(), by), (4, By::Function), "{}", flavour.id);
        for (i, (play, note)) in plays.iter().zip(flavour.jingle).enumerate() {
            assert_eq!(play.sound, NOTE, "{}", flavour.id);
            assert!(
                near(play.params.rate, speed(note, "E5"), 1e-5),
                "{} {note}",
                flavour.id
            );
            assert!(near(play.params.delay, 0.08 * i as f32, 1e-6));
        }
        // Left to right.
        assert!(plays[0].params.pan < plays[3].params.pan);
        // The jingle begins on the flavour's home note and ends an
        // octave above it; every key's note is in the octave between or
        // just beside it.
        assert_eq!(&flavour.jingle[0][..1], &flavour.home[..1]);
        assert_eq!(&flavour.jingle[3][..1], &flavour.home[..1]);
    }
    // A flavour that isn't one sounds as strawberry.
    let (plays, _) = choose(&mut app, "nonsense");
    assert!(near(plays[1].params.rate, speed("E5", "E5"), 1e-5));

    // The other options make no sound.
    for id in ["buddy", "living", "fountain"] {
        let value = OptionValue::Bool(false);
        app.set_option(id, value.clone());
        let (plays, by) = app.decide(&stand_in::option(id, value));
        assert_eq!((plays.len(), by), (0, By::Nobody), "{id}");
    }
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

// ---- The chapter's lengths, in every flavour -----------------------------------

/// `sound_check.py` measures the flavour the theme starts in. The notes
/// of the others are the same sounds at other speeds, and a slower sound
/// is a longer one: caramel's lowest key plays the blip at two thirds of
/// its speed. So the lengths are held here for every flavour, from the
/// sounds as rendered (which end 60 dB down, as the script measures).
#[test]
fn nothing_is_too_long_in_any_flavour() {
    let theme = sprinkles();
    let made = prepare_all(&theme.sounds().sounds);
    let frames: Vec<usize> = made.iter().map(|m| m.as_ref().unwrap().len() / 2).collect();
    // How long after its event a play ends, in seconds.
    let ends = |play: &Play| {
        let p = play.params;
        // Nothing the function returns is out of a play's range.
        assert_eq!(p, p.clamped());
        let (slow, fast) = limits::PLAY_RATE;
        assert!(p.rate >= slow && p.rate <= fast, "{p:?}");
        p.delay + frames[play.sound.index().unwrap()] as f32 / 48_000.0 / p.rate
    };
    let mut app = StandIn::new(&theme);
    let mut longest_key = 0.0f32;
    for flavour in &FLAVOURS {
        // The jingle itself: under the two seconds nothing needs more than.
        let (jingle, _) = choose(&mut app, flavour.id);
        let longest = jingle.iter().map(&ends).fold(0.0, f32::max);
        assert!(
            longest <= 1.6,
            "{}: its jingle lasts {longest} s",
            flavour.id
        );
        // A number key and an operator: 80 ms at most, on every key.
        for id in 0..64 {
            let play = app.decide(&digit(id, 0.5)).0[0];
            let length = ends(&play);
            longest_key = longest_key.max(length);
            assert!(length <= 0.08, "{}, key {id}: {length} s", flavour.id);
        }
        let length = ends(&app.decide(&operator()).0[0]);
        assert!(
            length <= 0.08,
            "{}: an operator lasts {length} s",
            flavour.id
        );
        // The equals key: a quarter of a second at most.
        let length = ends(&app.decide(&equals()).0[0]);
        assert!(length <= 0.25, "{}: = lasts {length} s", flavour.id);
        // The jingle after =, and = pressed again.
        for _ in 0..4 {
            let (plays, _) = app.decide(&result());
            let longest = plays.iter().map(&ends).fold(0.0, f32::max);
            assert!(
                longest <= 1.75,
                "{}: a result lasts {longest} s",
                flavour.id
            );
            app.decide(&equals());
        }
    }
    println!(
        "Sprinkles' longest number key, in any flavour: {:.0} ms",
        longest_key * 1000.0
    );
    assert!(app.notes.is_empty(), "{:?}", app.notes);
}

// ---- The function's 2 ms -----------------------------------------------------------

#[test]
fn the_sound_function_stays_inside_its_time() {
    assert_eq!(HOOK_TIME, Duration::from_millis(2));
    let theme = sprinkles();
    let hook = theme
        .def
        .sound
        .clone()
        .expect("Sprinkles has a sound function");
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
    for round in 0..60u32 {
        for flavour in &FLAVOURS {
            let value = OptionValue::Text(flavour.id.to_string());
            app.set_option("flavour", value.clone());
            let jingle = ask(&mut app, &stand_in::option("flavour", value));
            assert!(matches!(jingle, Choice::Plays(p) if p.len() == 4));
            for id in 0..12 {
                ask(&mut app, &digit(round * 12 + id, id as f32 / 11.0));
            }
            assert!(matches!(ask(&mut app, &operator()), Choice::Plays(p) if p.len() == 1));
            assert_eq!(
                ask(&mut app, &result_after(Cause::Operator)),
                Choice::Silence
            );
            assert!(matches!(ask(&mut app, &equals()), Choice::Plays(p) if p.len() == 1));
            assert!(matches!(ask(&mut app, &result()), Choice::Plays(p) if p.len() == 4));
            ask(&mut app, &equals());
            assert!(matches!(ask(&mut app, &result()), Choice::Plays(p) if p.len() == 2));
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
        "Sprinkles' sound function: {calls} calls, {:.1} us each, {:.1} us at worst (it has 2000)",
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
