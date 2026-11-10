//! Swirls, the built-in theme, in the real engine under the real limits:
//! what its layers draw, what they ask for, what a frame costs at any
//! window size, and that every pixel of its paint is a colour the
//! readability tests have been through (`tests/css.rs`,
//! `swirls_texts_read_over_every_colour_of_the_field`).

use std::collections::HashSet;

use gmnb_theme::draw::{MaskMode, Op};
use gmnb_theme::events::{Cause, Event, EventKind, KeyKind, Page, Source as PressSource, Via};
use gmnb_theme::layer::{
    ChangeKind, DisplayValue, Drawn, Failure, Frame, GlyphBox, Glyphs, KeyCell, Keys, LayerKind,
    Role,
};
use gmnb_theme::theme::{self, Source, Theme};
use gmnb_theme::types::*;
use gmnb_theme::{Ctx, Limits, OptionValue};

const BANDS: [&str; 6] = ["party", "lava", "lagoon", "orchid", "absinthe", "marble"];
const GLASS: [&str; 3] = ["clear", "smoked", "panel"];

/// Swirls, with the limits a theme really has.
fn swirls() -> Theme {
    let theme = Theme::load(Source::built_in("swirls").unwrap(), Limits::default())
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(theme.notes.is_empty(), "{:#?}", theme.notes);
    theme
}

/// A `ctx` with the theme's options at their defaults, but for `options`.
fn ctx(theme: &Theme, options: &[(&str, OptionValue)]) -> Ctx {
    let saved = options
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    Ctx {
        dark: true,
        options: theme::complete_options(&theme.def.options, &saved),
        ..Ctx::default()
    }
}

/// Sets the theme's clock (what its fields' allowance of thirty passes a
/// second runs on) to the frame's own time, and draws.
fn draw_at(theme: &Theme, kind: LayerKind, c: &Ctx, f: &Frame) -> Drawn {
    let now = f.time;
    theme.api().set_clock(std::rc::Rc::new(move || 100.0 + now));
    ok(theme.draw(kind, c, f))
}

fn text(s: &str) -> OptionValue {
    OptionValue::Text(s.into())
}

fn ok(r: Result<Drawn, Failure>) -> Drawn {
    match r {
        Ok(d) => d,
        Err(Failure::Failed(e)) => panic!("{e}"),
        Err(Failure::Off) => panic!("the hook is off"),
    }
}

/// The processor time this thread has used, in milliseconds: what a
/// hook's time limit is of.
fn cpu_ms() -> f64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `clock_gettime` writes the one `timespec` it is given.
    let ok = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) } == 0;
    assert!(ok);
    ts.tv_sec as f64 * 1000.0 + ts.tv_nsec as f64 / 1e6
}

/// The mean and the largest of some times.
fn mean_and_worst(times: &[f64]) -> (f64, f64) {
    (
        times.iter().sum::<f64>() / times.len() as f64,
        times.iter().copied().fold(0.0, f64::max),
    )
}

/// A number key pressed by the pointer at (x, y).
fn press(id: u32, kind: KeyKind, x: f32, y: f32) -> Event {
    let mut e = Event::at(
        EventKind::Press,
        Page::Calculator,
        [x - 42.0, y - 27.0, 85.0, 55.0],
    );
    e.source = Some(PressSource::Key);
    e.via = Some(Via::Pointer);
    e.key = Some((Some(id), kind));
    e
}

/// A result or an error in a readout across the top of a `width` window.
fn outcome(kind: EventKind, width: f32) -> Event {
    let mut e = Event::at(kind, Page::Calculator, [8.0, 50.0, width - 16.0, 116.0]);
    e.cause = Some(Cause::Equals);
    e.via = Some(Via::Pointer);
    e
}

/// The one field a layer's frame drew.
fn field(d: &Drawn) -> (&gmnb_theme::field::FieldPicture, [f32; 4]) {
    let fields: Vec<_> = d
        .list
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Field { picture, rect } => Some((&**picture, *rect)),
            _ => None,
        })
        .collect();
    assert_eq!(fields.len(), 1, "{:?}", d.list.ops.len());
    fields[0]
}

/// The table of the ramp Swirls makes for the bands a style names (the
/// same recipe as `tests/css.rs` goes through): `bands × 256` colours.
fn table(theme: &mut Theme, c: &Ctx) -> Vec<[u8; 4]> {
    let style = theme.style(c).unwrap().style.clone();
    let Some(Paint::Gradient(g)) = style.fill("window.background").0.first().cloned() else {
        panic!("Swirls' window.background is a gradient of its three bands");
    };
    let [a, ground, b] = g.colors[..] else {
        panic!("three bands, not {}", g.colors.len());
    };
    let ramp = gmnb_media::Ramp::new(&[a, b, ground], [0.08, 0.92], true).unwrap();
    (0..3 * 256)
        .map(|k| ramp.at((k as f32 + 0.5) / 256.0))
        .collect()
}

fn glyphs(n: usize, x0: f32) -> Glyphs {
    Glyphs {
        glyphs: (0..n)
            .map(|i| GlyphBox {
                x: x0 + i as f32 * 36.0,
                y: 44.0,
                w: 34.0,
                h: 60.0,
            })
            .collect(),
        size: 62.0,
        rect: [x0, 44.0, n as f32 * 36.0, 60.0],
    }
}

/// A value of nine figures. (Each call is a new version of it: the app
/// hands a hook a value's table again only when that changed.)
fn value<'a>(
    now: &'a Glyphs,
    old: &'a Glyphs,
    role: Role,
    error: bool,
    change: Option<(ChangeKind, f64)>,
) -> DisplayValue<'a> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static VERSION: AtomicU64 = AtomicU64::new(1);
    DisplayValue {
        readout: role as u64 + 1,
        version: VERSION.fetch_add(1, Ordering::Relaxed),
        text: "123456789",
        glyphs: now,
        band: [0.0, 40.0, 428.0, 68.0],
        error,
        align_end: true,
        role,
        change: change.map(|(kind, age)| (kind, age, 9)),
        previous: change.map(|_| old),
    }
}

/// What the background asks for, flowing and still.
#[test]
fn the_paint_flows_while_it_may_and_stands_when_it_may_not() {
    let theme = swirls();
    assert_eq!(
        theme
            .hooks()
            .iter()
            .map(|h| h.0.as_str())
            .collect::<Vec<_>>(),
        [
            "background.draw",
            "display.draw",
            "keypad.draw",
            "keypad.place"
        ]
    );
    let flowing = ctx(&theme, &[]);
    let f = Frame::sized(760.0, 640.0);
    // One field over the whole window, at half size, and again in a
    // thirtieth of a second: the app keeps a plain timer for that.
    let first = draw_at(&theme, LayerKind::Background, &flowing, &f);
    let (picture, rect) = field(&first);
    assert_eq!(rect, [0.0, 0.0, 760.0, 640.0]);
    assert_eq!((picture.width, picture.height), (380, 320));
    assert_eq!(first.list.ops.len(), 1);
    assert_eq!(first.again, Some(1.0 / 30.0));
    assert_eq!(first.retry, None);
    // Drawn again within the same thirtieth (an event came, the style
    // changed): the picture already made, at no cost, and the rest of
    // the wait.
    let serial = picture.serial;
    let soon = Frame {
        time: 0.012,
        dt: 0.012,
        ..Frame::sized(760.0, 640.0)
    };
    let again = draw_at(&theme, LayerKind::Background, &flowing, &soon);
    assert_eq!(field(&again).0.serial, serial);
    let wait = again.again.unwrap();
    assert!((wait - (1.0 / 30.0 - 0.012)).abs() < 1e-6, "{wait}");
    // Thirty pictures a second and no more: a second of frames a
    // sixtieth apart makes thirty. (From the first fifth of a second on:
    // until its second snapshot is made the paint stands.)
    let mut serials = HashSet::new();
    for i in 12..72 {
        let t = i as f64 / 60.0;
        let f = Frame {
            time: t,
            dt: 1.0 / 60.0,
            ..Frame::sized(760.0, 640.0)
        };
        let d = draw_at(&theme, LayerKind::Background, &flowing, &f);
        assert_eq!(d.retry, None, "{t}");
        serials.insert(field(&d).0.serial);
    }
    assert_eq!(serials.len(), 30);

    // Under reduced motion: a still picture, and no more frames.
    let quiet = Frame {
        motion: false,
        ..Frame::sized(760.0, 640.0)
    };
    let still = draw_at(&theme, LayerKind::Background, &flowing, &quiet);
    assert_eq!(still.again, None);
    assert_eq!(still.list.ops.len(), 1);
    let serial = field(&still).0.serial;
    let still = draw_at(&theme, LayerKind::Background, &flowing, &quiet);
    assert_eq!(field(&still).0.serial, serial);

    // Held still by its own option: nothing is asked for at rest. A press
    // stirs it all the same, for as long as the stir lasts.
    let held = ctx(&theme, &[("speed", OptionValue::Number(0.0))]);
    let theme = swirls();
    let f = Frame::sized(760.0, 640.0);
    assert_eq!(
        draw_at(&theme, LayerKind::Background, &held, &f).again,
        None
    );
    let key = press(5, KeyKind::Number, 110.0, 300.0);
    let mut last = None;
    for i in 0..60 {
        let t = 1.0 + i as f64 / 30.0;
        let f = Frame {
            time: t,
            dt: 1.0 / 30.0,
            events: if i < 39 {
                vec![(i as f64 / 30.0, &key)]
            } else {
                Vec::new()
            },
            ..Frame::sized(760.0, 640.0)
        };
        last = Some(draw_at(&theme, LayerKind::Background, &held, &f));
        if i < 30 {
            assert_eq!(last.as_ref().unwrap().again, Some(1.0 / 30.0), "{i}");
        }
    }
    assert_eq!(last.unwrap().again, None);
    // And with the stirring off nothing moves it.
    let calm = ctx(
        &theme,
        &[
            ("speed", OptionValue::Number(0.0)),
            ("stir", OptionValue::Bool(false)),
        ],
    );
    let theme = swirls();
    let f = Frame {
        time: 0.5,
        events: vec![(0.1, &key)],
        ..Frame::sized(760.0, 640.0)
    };
    assert_eq!(
        draw_at(&theme, LayerKind::Background, &calm, &f).again,
        None
    );
    assert!(
        theme
            .hooks()
            .iter()
            .all(|(_, strikes, off)| *strikes == 0 && !off)
    );
}

/// Every pixel the background draws, in every set of bands, flowing,
/// stirred, rippling and shuddering, at two window sizes, is one of the
/// 768 colours of that set's ramp; and every pixel of the painted
/// figures is one of the first band's 256. Those are the colours the
/// readability tests put under every text.
#[test]
fn every_pixel_of_the_paint_is_one_of_its_ramps_colours() {
    for bands in BANDS {
        let mut theme = swirls();
        let c = ctx(&theme, &[("bands", text(bands))]);
        let table = table(&mut theme, &c);
        let all: HashSet<[u8; 4]> = table.iter().copied().collect();
        let first: HashSet<[u8; 4]> = table[..256].iter().copied().collect();
        let key = press(5, KeyKind::Number, 110.0, 300.0);
        let equals = press(121, KeyKind::Equals, 300.0, 560.0);
        let result = outcome(EventKind::Result, 360.0);
        let error = outcome(EventKind::Error, 360.0);
        let mode = Event::at(EventKind::Mode, Page::Converter, [0.0, 46.0, 360.0, 554.0]);
        let mut seen = HashSet::new();
        for (w, h) in [(360.0, 600.0), (760.0, 640.0)] {
            for i in 0..75 {
                let t = i as f64 / 30.0;
                let mut events = Vec::new();
                for (at, e) in [
                    (0.2, &key),
                    (0.5, &equals),
                    (0.55, &result),
                    (1.2, &error),
                    (1.6, &mode),
                ] {
                    if t >= at && t - at < 1.3 {
                        events.push((t - at, e));
                    }
                }
                let f = Frame {
                    time: t,
                    dt: 1.0 / 30.0,
                    events,
                    ..Frame::sized(w, h)
                };
                let d = draw_at(&theme, LayerKind::Background, &c, &f);
                for px in field(&d).0.pixels.chunks_exact(4) {
                    seen.insert([px[0], px[1], px[2], px[3]]);
                }
            }
        }
        // (A good part of the table shows in two and a half seconds.)
        assert!(seen.len() > 300, "{bands}: {} colours", seen.len());
        let strays: Vec<_> = seen.difference(&all).take(5).collect();
        assert!(strays.is_empty(), "{bands}: not of the ramp: {strays:?}");

        // The figures: at rest, and through a result.
        let (now, old) = (glyphs(9, 80.0), glyphs(4, 250.0));
        let mut seen = HashSet::new();
        for role in [Role::Calculator, Role::Converter, Role::Date] {
            for i in 0..45 {
                let t = i as f64 / 30.0;
                let change = (i < 30).then_some((ChangeKind::Result, t));
                let f = Frame {
                    time: t,
                    dt: 1.0 / 30.0,
                    value: Some(value(&now, &old, role, false, change)),
                    ..Frame::sized(428.0, 116.0)
                };
                let d = draw_at(&theme, LayerKind::Display, &c, &f);
                let (picture, rect) = field(&d);
                assert_eq!(rect, [0.0, 40.0, 428.0, 68.0]);
                for px in picture.pixels.chunks_exact(4) {
                    seen.insert([px[0], px[1], px[2], px[3]]);
                }
            }
        }
        assert!(seen.len() > 100, "{bands}: {} colours", seen.len());
        let strays: Vec<_> = seen.difference(&first).take(5).collect();
        assert!(
            strays.is_empty(),
            "{bands}: figures not of the first band: {strays:?}"
        );
        let notes = theme.take_draw_notes();
        assert!(notes.is_empty(), "{notes:?}");
    }
}

/// The background at every size a window may have, from the least to a
/// 4K screen: the grid stays within its caps, a frame within its time
/// (first frame, a new snapshot, and a frame with the paint disturbed
/// eight times over), and nothing is refused.
#[test]
fn the_background_stays_within_its_limits_at_any_size() {
    let limit = Limits::default().frame.as_secs_f64() * 1000.0;
    let mut report = Vec::new();
    for (w, h) in [
        (300.0, 400.0),
        (360.0, 600.0),
        (760.0, 640.0),
        (1280.0, 800.0),
        (1920.0, 1080.0),
        (3840.0, 2160.0),
        (3840.0, 160.0),
        (200.0, 2160.0),
    ] {
        let theme = swirls();
        let c = ctx(&theme, &[]);
        let presses: Vec<Event> = (0..8)
            .map(|i| {
                press(
                    i,
                    if i == 7 {
                        KeyKind::Equals
                    } else {
                        KeyKind::Number
                    },
                    w * (0.1 + 0.1 * i as f32),
                    h * (0.3 + 0.08 * i as f32),
                )
            })
            .collect();
        let result = outcome(EventKind::Result, w);
        let mut times = Vec::new();
        for i in 0..90 {
            let t = i as f64 / 30.0;
            let mut events: Vec<(f64, &Event)> = Vec::new();
            if (30..69).contains(&i) {
                for (n, e) in presses.iter().enumerate() {
                    let age = t - 1.0 - n as f64 * 0.05;
                    if age >= 0.0 {
                        events.push((age, e));
                    }
                }
                events.push((t - 1.0, &result));
            }
            let f = Frame {
                time: t,
                dt: 1.0 / 30.0,
                events,
                ..Frame::sized(w, h)
            };
            let started = cpu_ms();
            let d = draw_at(&theme, LayerKind::Background, &c, &f);
            let ms = cpu_ms() - started;
            assert_eq!(d.retry, None, "{w}x{h} at {t}");
            assert_eq!(d.again, Some(1.0 / 30.0), "{w}x{h} at {t}");
            let (picture, _) = field(&d);
            assert!(picture.width * picture.height <= 262_144);
            times.push(ms);
        }
        let (mean, worst) = mean_and_worst(&times);
        report.push(format!("{w}x{h}: mean {mean:.2} ms, worst {worst:.2} ms"));
        // The limit is 8 ms; an unoptimised build is given all of it.
        let most = if cfg!(debug_assertions) {
            limit
        } else {
            limit * 0.75
        };
        assert!(worst < most, "{w}x{h}: a frame took {worst:.2} ms");
        assert!(
            theme
                .hooks()
                .iter()
                .all(|(_, strikes, off)| *strikes == 0 && !off),
            "{w}x{h}: {:?}",
            theme.hooks()
        );
        let notes = theme.take_draw_notes();
        assert!(notes.is_empty(), "{w}x{h}: {notes:?}");
    }
    println!(
        "swirls, the background stirred eight times over and rippling, by window size \
         (processor time; the limit is {limit} ms):\n  {}",
        report.join("\n  ")
    );
}

/// What a frame costs in a 760 × 640 window, as the app runs the layers:
/// the background and the readout thirty times a second each, flowing at
/// the page's pace, and while somebody types (a key every quarter of a
/// second, a result every six keys). Processor time: what the limit of
/// 8 ms is of.
#[test]
fn what_a_frame_costs() {
    let theme = swirls();
    let c = ctx(&theme, &[]);
    let (now, old) = (glyphs(9, 80.0), glyphs(4, 250.0));
    let keys: Vec<Event> = (0..6)
        .map(|i| {
            let kind = if i == 5 {
                KeyKind::Equals
            } else {
                KeyKind::Number
            };
            press(
                i,
                kind,
                60.0 + 109.0 * (i % 4) as f32,
                300.0 + 72.0 * (i % 5) as f32,
            )
        })
        .collect();
    let result = outcome(EventKind::Result, 444.0);
    // (`from`: the layers' clocks run on from one run to the next.)
    let run = |typing: bool, from: f64| -> (Vec<f64>, Vec<f64>) {
        let (mut background, mut readout) = (Vec::new(), Vec::new());
        for i in 0..360 {
            let t = i as f64 / 30.0;
            // A key every 0.25 s, each kept for 1.3 s; the sixth is the
            // equals key, and its result comes with it.
            let mut events: Vec<(f64, &Event)> = Vec::new();
            let mut change = None;
            if typing {
                let latest = (t / 0.25).floor() as i64;
                for n in (latest - 6).max(0)..=latest {
                    let age = t - n as f64 * 0.25;
                    if age < 1.3 {
                        events.push((age, &keys[n as usize % 6]));
                    }
                    if n % 6 == 5 && age < 2.5 {
                        events.push((age, &result));
                        if age < 1.0 {
                            change = Some((ChangeKind::Result, age));
                        }
                    }
                }
                if change.is_none() {
                    let age = t - latest as f64 * 0.25;
                    change = (age < 0.24).then_some((ChangeKind::Typing, age));
                }
            }
            let f = Frame {
                time: from + t,
                dt: 1.0 / 30.0,
                events,
                ..Frame::sized(760.0, 640.0)
            };
            let started = cpu_ms();
            let d = draw_at(&theme, LayerKind::Background, &c, &f);
            background.push(cpu_ms() - started);
            assert_eq!(d.retry, None);
            let f = Frame {
                time: from + t,
                dt: 1.0 / 30.0,
                value: Some(value(&now, &old, Role::Calculator, false, change)),
                ..Frame::sized(428.0, 116.0)
            };
            let started = cpu_ms();
            let d = draw_at(&theme, LayerKind::Display, &c, &f);
            readout.push(cpu_ms() - started);
            assert_eq!(d.retry, None);
        }
        (background, readout)
    };
    let (flowing, flowing_readout) = run(false, 0.0);
    let (typing, typing_readout) = run(true, 12.0);
    // The keypad: `place` for a keypad of 24 keys, swirling in.
    let cells = keypad_cells();
    let show = Event::at(EventKind::Show, Page::Calculator, [0.0, 0.0, 430.0, 426.0]);
    let mut keypad = Vec::new();
    for i in 0..30 {
        let f = Frame {
            events: vec![(i as f64 / 30.0, &show)],
            keys: Some(Keys {
                pad: 1,
                layout: 1,
                cells: &cells,
            }),
            ..Frame::sized(430.0, 426.0)
        };
        let started = cpu_ms();
        draw_at(&theme, LayerKind::Keypad, &c, &f);
        keypad.push(cpu_ms() - started);
    }
    let line = |what: &str, times: &[f64]| {
        let (mean, worst) = mean_and_worst(times);
        format!("{what}: mean {mean:.3} ms, worst {worst:.3} ms")
    };
    println!(
        "swirls, Luau and the app's colouring pass per frame (processor time, 360 frames each):\n  {}\n  {}\n  {}\n  {}\n  {}",
        line("background, flowing", &flowing),
        line("background, typing", &typing),
        line("readout, flowing", &flowing_readout),
        line("readout, typing", &typing_readout),
        line("keypad, swirling in (place included)", &keypad),
    );
    // The limit is 8 ms a frame. The worst frame is the one that makes a
    // new snapshot with the paint disturbed; flowing, a frame is a tenth
    // of the limit on average, and with six stirs and a ripple alive at
    // once, a third at most.
    let limit = Limits::default().frame.as_secs_f64() * 1000.0;
    for (times, share) in [
        (&flowing, 6.0),
        (&typing, 3.0),
        (&flowing_readout, 16.0),
        (&typing_readout, 16.0),
        (&keypad, 16.0),
    ] {
        let (mean, worst) = mean_and_worst(times);
        assert!(mean < limit / share && worst < limit, "{mean} {worst}");
    }
    assert!(
        theme
            .hooks()
            .iter()
            .all(|(_, strikes, off)| *strikes == 0 && !off)
    );
}

/// A calculator keypad's cells, 6 rows of 4.
fn keypad_cells() -> Vec<KeyCell> {
    let mut cells = Vec::new();
    for row in 0..6 {
        for col in 0..4 {
            cells.push(KeyCell {
                id: Some((row * 4 + col) as u32),
                kind: match (row, col) {
                    (5, 3) => KeyKind::Equals,
                    (_, 3) => KeyKind::Operator,
                    (0 | 1, _) => KeyKind::Func,
                    _ => KeyKind::Number,
                },
                rect: [col as f32 * 109.0, row as f32 * 72.0, 103.0, 66.0],
                row,
                col,
                radius: 12.0,
                enabled: true,
                hovered: false,
                pressed: false,
                checked: false,
            });
        }
    }
    cells
}

/// The keypad's keys swirl into place when it comes into view, and then
/// are left alone; nothing is ever drawn over them.
#[test]
fn the_keys_swirl_in_and_nothing_is_drawn_over_them() {
    let theme = swirls();
    let c = ctx(&theme, &[]);
    let cells = keypad_cells();
    let show = Event::at(EventKind::Show, Page::Calculator, [0.0, 0.0, 430.0, 426.0]);
    let at = |age: f64| {
        let f = Frame {
            events: vec![(age, &show)],
            keys: Some(Keys {
                pad: 1,
                layout: 1,
                cells: &cells,
            }),
            ..Frame::sized(430.0, 426.0)
        };
        draw_at(&theme, LayerKind::Keypad, &c, &f)
    };
    let early = at(0.1);
    assert_eq!(early.again, Some(0.0));
    assert_eq!(early.list.ops.len(), 0);
    assert_eq!(early.places.len(), 24);
    // The keys by the middle are on their way, turned off their places;
    // a corner's hasn't started.
    let inner = early.places[9].expect("a key by the middle is moving");
    assert!(inner.opacity > 0.0 && inner.opacity < 1.0, "{inner:?}");
    assert!(inner.dx != 0.0 && inner.dy != 0.0, "{inner:?}");
    assert_eq!(early.places[0].map(|p| p.opacity), Some(0.0));
    let landed = at(1.2);
    assert_eq!((landed.again, landed.list.ops.len()), (None, 0));
    assert!(landed.places.iter().all(Option::is_none));
    // With no keypad newly shown there is nothing to do.
    let f = Frame {
        keys: Some(Keys {
            pad: 1,
            layout: 1,
            cells: &cells,
        }),
        ..Frame::sized(430.0, 426.0)
    };
    let rest = draw_at(&theme, LayerKind::Keypad, &c, &f);
    assert_eq!((rest.again, rest.list.ops.len()), (None, 0));
    assert!(rest.places.iter().all(Option::is_none));
}

/// The readout: figures of paint behind a mask, plain ones for an error
/// or when the option says so, every kind of change, and what it costs.
#[test]
fn the_readouts_figures_are_paint_behind_a_mask() {
    let theme = swirls();
    let c = ctx(&theme, &[]);
    let (now, old) = (glyphs(9, 80.0), glyphs(4, 250.0));
    let draw = |c: &Ctx, time: f64, v: DisplayValue| {
        let f = Frame {
            time,
            dt: 1.0 / 30.0,
            value: Some(v),
            ..Frame::sized(428.0, 116.0)
        };
        draw_at(&theme, LayerKind::Display, c, &f)
    };
    let count = |d: &Drawn, previous: bool| {
        d.list
            .ops
            .iter()
            .filter(|op| matches!(op, Op::Glyph { previous: p, .. } if *p == previous))
            .count()
    };
    let masked = |d: &Drawn| {
        d.list
            .ops
            .iter()
            .any(|op| matches!(op, Op::PushMask(MaskMode::Alpha)))
    };
    let fields = |d: &Drawn| {
        d.list
            .ops
            .iter()
            .filter(|op| matches!(op, Op::Field { .. }))
            .count()
    };
    let border = |d: &Drawn| matches!(d.list.ops.first(), Some(Op::Border { .. }));
    // At rest: the panel's hairline, the nine glyphs as a mask, the slice
    // of the swirl behind them; and the paint flows.
    let rest = draw(&c, 0.0, value(&now, &old, Role::Calculator, false, None));
    assert!(border(&rest) && masked(&rest));
    assert_eq!((count(&rest, false), fields(&rest)), (9, 1));
    assert_eq!(rest.again, Some(1.0 / 30.0));
    assert_eq!(rest.retry, None);
    // A converter's value and a date's result are figures of paint too,
    // with no hairline of the readout's.
    for role in [Role::Converter, Role::Date] {
        let d = draw(&c, 0.0, value(&now, &old, role, false, None));
        assert!(!border(&d) && masked(&d), "{role:?}: {:?}", d.list.ops);
        assert_eq!(fields(&d), 1, "{role:?}");
    }
    // Every kind of change plays on every frame, and ends.
    for (kind, age) in [
        (ChangeKind::Typing, 0.1),
        (ChangeKind::Result, 0.3),
        (ChangeKind::Result, 0.7),
        (ChangeKind::Replace, 0.1),
        (ChangeKind::Error, 0.2),
    ] {
        let error = kind == ChangeKind::Error;
        let d = draw(
            &c,
            1.0,
            value(&now, &old, Role::Calculator, error, Some((kind, age))),
        );
        assert_eq!(d.again, Some(0.0), "{kind:?}");
        assert_eq!(
            count(&d, true),
            if kind == ChangeKind::Replace { 4 } else { 0 }
        );
        // An error is written plainly, in its own colour.
        assert_eq!(fields(&d), if error { 0 } else { 1 }, "{kind:?}");
    }
    let done = draw(
        &c,
        2.0,
        value(
            &now,
            &old,
            Role::Calculator,
            false,
            Some((ChangeKind::Typing, 2.0)),
        ),
    );
    assert_eq!(done.again, Some(1.0 / 30.0));
    // Under reduced motion, a still picture of the same paint.
    let f = Frame {
        motion: false,
        value: Some(value(&now, &old, Role::Calculator, false, None)),
        ..Frame::sized(428.0, 116.0)
    };
    let still = draw_at(&theme, LayerKind::Display, &c, &f);
    assert_eq!((still.again, fields(&still)), (None, 1));
    // Plain figures by the option: no field, and nothing asked for at rest.
    let plain = ctx(&theme, &[("digits", OptionValue::Bool(false))]);
    let d = draw(
        &plain,
        0.0,
        value(&now, &old, Role::Calculator, false, None),
    );
    assert_eq!((fields(&d), d.again), (0, None));
    assert!(masked(&d));
    // Held still, the figures stand too.
    let held = ctx(&theme, &[("speed", OptionValue::Number(0.0))]);
    let d = draw(&held, 0.0, value(&now, &old, Role::Calculator, false, None));
    assert_eq!((fields(&d), d.again), (1, None));

    assert!(
        theme
            .hooks()
            .iter()
            .all(|(_, strikes, off)| *strikes == 0 && !off)
    );
    let notes = theme.take_draw_notes();
    assert!(notes.is_empty(), "{notes:?}");
}

/// Every tile Settings shows: a set of bands is its swirl through its
/// own ramp, a glass a key of it over the swirl.
#[test]
fn the_tiles_are_small_swirls() {
    let theme = swirls();
    let option = |id: &str| theme.def.options.iter().find(|o| o.id == id).unwrap();
    let ids: Vec<&str> = theme.def.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(
        ids,
        ["bands", "glass", "speed", "stir", "digits", "motion_note"]
    );
    let mut pictures = HashSet::new();
    for bands in BANDS {
        let c = ctx(&theme, &[("bands", text(bands))]);
        let tile = theme.preview(option("bands"), bands, &c).unwrap();
        let fields: Vec<_> = tile
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Field { picture, rect } => Some((picture, rect)),
                _ => None,
            })
            .collect();
        assert_eq!(fields.len(), 1, "{bands}");
        assert_eq!(*fields[0].1, [0.0, 0.0, 96.0, 56.0]);
        pictures.insert(fields[0].0.pixels.to_vec());
        for glass in GLASS {
            let tile = theme.preview(option("glass"), glass, &c).unwrap();
            assert!(tile.ops.iter().any(|op| matches!(op, Op::Field { .. })));
            assert!(tile.ops.iter().any(|op| matches!(op, Op::Text { .. })));
        }
    }
    // Six sets, six pictures.
    assert_eq!(pictures.len(), 6);
    let notes = theme.take_draw_notes();
    assert!(notes.is_empty(), "{notes:?}");
}

/// What the theme loads as: its name, its credits, no fonts of its own,
/// and a style with no notes for every set of bands under every glass.
#[test]
fn swirls_loads_and_returns_only_known_keys() {
    let mut theme = swirls();
    assert_eq!(theme.def.meta.name, "Swirls");
    assert!(theme.def.fonts.is_empty());
    let credits = theme.credits();
    assert_eq!(credits.len(), 1);
    assert!(credits[0].0.contains("ColorMix"));
    assert!(credits[0].1.contains("https://github.com/fairyglade/ly"));
    assert!(credits[0].1.contains("s0y b0y"));
    for opacity in [1.0, 0.5, 0.1] {
        for bands in BANDS {
            for glass in GLASS {
                let mut c = ctx(&theme, &[("bands", text(bands)), ("glass", text(glass))]);
                c.opacity = opacity;
                let r = theme.style(&c).unwrap();
                assert!(
                    r.notes.is_empty(),
                    "{bands} {glass} {opacity}: {:#?}",
                    r.notes
                );
            }
        }
    }
    // A saved value that is no set of bands is the page's own.
    let odd = ctx(&theme, &[("bands", text("nonsense"))]);
    assert!(theme.style(&odd).unwrap().notes.is_empty());
}

/// For looking at: `SWIRLS_FRAMES=<folder>` writes the background's
/// frames as PPM pictures while keys are pressed and a result arrives
/// (`cargo test -p gmnb-theme --test swirls -- --ignored frames`).
#[test]
#[ignore]
fn frames() {
    let Some(out) = std::env::var_os("SWIRLS_FRAMES") else {
        println!("SWIRLS_FRAMES is not set; nothing written");
        return;
    };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let theme = swirls();
    let bands = std::env::var("SWIRLS_BANDS").unwrap_or_else(|_| "party".into());
    let speed = std::env::var("SWIRLS_SPEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);
    let options = [
        ("bands", text(&bands)),
        ("speed", OptionValue::Number(speed)),
    ];
    let c = ctx(&theme, &options);
    let (w, h) = (760.0, 640.0);
    let key = press(5, KeyKind::Number, 170.0, 380.0);
    let other = press(8, KeyKind::Number, 280.0, 300.0);
    let equals = press(121, KeyKind::Equals, 390.0, 600.0);
    let result = outcome(EventKind::Result, 444.0);
    let error = outcome(EventKind::Error, 444.0);
    for i in 0..120 {
        let t = i as f64 / 30.0;
        let mut events = Vec::new();
        for (at, e) in [
            (0.2, &key),
            (0.5, &other),
            (1.0, &equals),
            (1.05, &result),
            (2.6, &error),
        ] {
            if t >= at && t - at < 1.6 {
                events.push((t - at, e));
            }
        }
        let f = Frame {
            time: t,
            dt: 1.0 / 30.0,
            events,
            ..Frame::sized(w, h)
        };
        let d = draw_at(&theme, LayerKind::Background, &c, &f);
        let (picture, _) = field(&d);
        let mut ppm = format!("P6\n{} {}\n255\n", picture.width, picture.height).into_bytes();
        for px in picture.pixels.chunks_exact(4) {
            ppm.extend_from_slice(&px[..3]);
        }
        std::fs::write(out.join(format!("f{i:03}.ppm")), ppm).unwrap();
    }
}
