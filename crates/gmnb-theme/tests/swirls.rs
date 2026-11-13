//! Swirls, the built-in theme, in the real engine under the real limits
//! (four times the frame's unless `GMNB_REAL_LIMITS` is set: `limits`):
//! what its layers draw, what they ask for, what a frame costs at any
//! window size, and that every pixel of its paint is a colour the
//! readability tests have been through (`tests/css.rs`,
//! `swirls_texts_read_over_every_colour_of_the_field`).

mod common;

use std::collections::HashSet;

use gmnb_theme::draw::{MaskMode, Op};
use gmnb_theme::events::{Cause, Event, EventKind, KeyKind, Page, Source as PressSource, Via};
use gmnb_theme::layer::{
    ChangeKind, DisplayValue, Drawn, Failure, Frame, GlyphBox, Glyphs, KeyCell, Keys, LayerKind,
    Role,
};
use gmnb_theme::theme::{self, Source, Theme};
use gmnb_theme::types::*;
use gmnb_theme::{Ctx, Limits, OptionValue, Style};

const BANDS: [&str; 8] = [
    "party", "lava", "lagoon", "orchid", "absinthe", "marble", "system", "custom",
];
const GLASS: [&str; 3] = ["clear", "smoked", "panel"];

/// The limits a theme really has when `GMNB_REAL_LIMITS` is set (run so
/// with `--release` on a quiet machine, to hold the real budget). Otherwise
/// four times the frame's limit: these tests run wherever the code is
/// built (CI's runners, the packages' builders, unoptimised builds), and a
/// slow machine there is not a slow theme. Four times still catches a
/// frame gone badly wrong.
fn limits() -> Limits {
    let real = Limits::default();
    if std::env::var_os("GMNB_REAL_LIMITS").is_some() {
        real
    } else {
        Limits {
            frame: real.frame * 4,
            ..real
        }
    }
}

/// Swirls, with the limits a theme really has (`limits`).
fn swirls() -> Theme {
    let theme = Theme::load(Source::built_in("swirls").unwrap(), limits())
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

/// Whether a layer asked to be drawn again when its next thirtieth of a
/// second begins, having been drawn at `time`.
/// (Never a whole thirtieth away: that would be a plain timer of the
/// app's, which is late for the screen's frame half the time. And a
/// frame half a millisecond early belongs to the thirtieth it came for.)
fn next_thirtieth(again: Option<f64>, time: f64) -> bool {
    let next = ((time * 30.0 + 0.03).floor() + 1.0) / 30.0;
    let wait = (next - time).clamp(0.001, 1.0 / 30.0 - 0.001);
    again.is_some_and(|asked| (asked - wait).abs() < 1e-9)
        && gmnb_theme::sched::Clock::new().end(0.0, again, RUN) == gmnb_theme::sched::Wait::Frame
}

const RUN: gmnb_theme::sched::Conditions = gmnb_theme::sched::Conditions {
    mapped: true,
    active: true,
    motion: true,
    settled: false,
};

/// The app's own clock (`sched`) driven by what the background returns,
/// on a screen of `hertz` whose frames come `early` seconds before they
/// are due: how many times the layer is drawn in ten seconds, and how
/// many of those are new pictures, and how many times the picture was made
/// coarser on the way (a machine too busy: the paint then stands for as
/// much as a fifth of a second while its two snapshots are made again).
fn drawn_in_ten_seconds(hertz: f64, early: f64) -> (usize, usize, usize) {
    use gmnb_theme::sched::{Clock, Fire};
    let theme = swirls();
    let c = ctx(&theme, &[]);
    let mut clock = Clock::new();
    let (mut draws, mut pictures, mut coarser) = (0, HashSet::new(), 0);
    let mut width = None;
    let mut draw = |clock: &mut Clock, now: f64| {
        let timing = clock.begin(now, RUN);
        let f = Frame {
            time: timing.time,
            dt: timing.dt,
            ..Frame::sized(760.0, 640.0)
        };
        let d = draw_at(&theme, LayerKind::Background, &c, &f);
        assert_eq!(d.retry, None, "{now}");
        draws += 1;
        pictures.insert(field(&d).0.serial);
        let now_width = field(&d).0.width;
        if width.is_some_and(|w| now_width < w) {
            coarser += 1;
        }
        width = Some(now_width);
        // (A plain timer is never asked for: `next_thirtieth` has that.)
        assert_eq!(clock.end(now, d.again, RUN), gmnb_theme::sched::Wait::Frame);
    };
    draw(&mut clock, 0.0);
    let frames = (10.0 * hertz) as usize;
    for n in 1..=frames {
        let now = n as f64 / hertz - early;
        if clock.fire(now, RUN) == Fire::Draw {
            draw(&mut clock, now);
        }
    }
    (draws, pictures.len(), coarser)
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
    // One field over the whole window, at half size, and again when the
    // next thirtieth of a second begins.
    let first = draw_at(&theme, LayerKind::Background, &flowing, &f);
    let (picture, rect) = field(&first);
    assert_eq!(rect, [0.0, 0.0, 760.0, 640.0]);
    // (Half size: 380 × 320. A machine over three times slower than the
    // one the theme was written on gets a coarser picture, by as much
    // as it is slower: `keep`, in the theme.)
    let nominal = (picture.width, picture.height) == (380, 320);
    let thinned = picture.width < 380 && (picture.width * 320 / 380).abs_diff(picture.height) <= 1;
    assert!(nominal || thinned, "{} x {}", picture.width, picture.height);
    if thinned {
        println!(
            "this machine is slow: Swirls thinned its picture to {} x {}",
            picture.width, picture.height
        );
    }
    assert_eq!(first.list.ops.len(), 1);
    assert!(next_thirtieth(first.again, 0.0), "{:?}", first.again);
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
    assert!(next_thirtieth(again.again, 0.012), "{:?}", again.again);
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
            assert!(next_thirtieth(last.as_ref().unwrap().again, t), "{i}");
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
    // Its busiest frames cost well within what a frame may to draw.
    let busiest = theme.api().busiest_frame();
    println!(
        "swirls (flowing): the busiest frame cost {:.1}% of what one may",
        busiest * 100.0
    );
    assert!(busiest > 0.0 && busiest < 0.25, "{busiest}");
}

/// The paint on the app's own clock: thirty pictures a second on a 60 Hz
/// screen and no frame drawn for nothing, whether the screen's frames
/// come on time or the half millisecond early the clock allows; and
/// close to thirty on screens whose frames don't divide a thirtieth.
#[test]
fn the_paint_is_drawn_thirty_times_a_second_on_the_apps_clock() {
    for early in [0.0, 0.0003, 0.00049] {
        let (draws, pictures, coarser) = drawn_in_ten_seconds(60.0, early);
        assert!((299..=301).contains(&draws), "60 Hz, {early}: {draws}");
        // (The first fifth of a second is one picture: one snapshot. So is
        // the fifth after the paint was made coarser on a busy machine.)
        assert!(
            pictures + 5 * coarser >= draws - 6,
            "60 Hz, {early}: {pictures} of {draws}"
        );
    }
    for hertz in [50.0, 75.0, 120.0, 144.0, 165.0, 240.0] {
        let (draws, pictures, coarser) = drawn_in_ten_seconds(hertz, 0.0002);
        assert!((240..=301).contains(&draws), "{hertz} Hz: {draws}");
        assert!(
            pictures + 5 * coarser >= draws - 6,
            "{hertz} Hz: {pictures} of {draws}"
        );
    }
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
                seen.extend(field(&d).0.pixels.as_chunks::<4>().0);
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
                seen.extend(picture.pixels.as_chunks::<4>().0);
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

/// The sets of one's own paint in the ramp of the bands their style names,
/// whatever the accent or the colours picked, and follow a new one on the
/// theme that drew the last: System on a desktop of each of a few accents
/// and Freestyle from a few picks, one after another on one theme, its
/// paint, its figures and its tile each in the ramp of the moment. (A
/// theme holds the colours of eight ramps at once; these are more.)
#[test]
fn own_colours_paint_in_their_own_ramp() {
    let mut theme = swirls();
    let option = theme
        .def
        .options
        .iter()
        .find(|o| o.id == "bands")
        .unwrap()
        .clone();
    let mut cases = Vec::new();
    for accent in [
        [0.2, 0.4, 0.9],
        [0.1, 0.8, 0.3],
        [1.0, 0.9, 0.0],
        [0.9, 0.1, 0.8],
        [0.5, 0.5, 0.5],
    ] {
        let mut c = ctx(&theme, &[("bands", text("system"))]);
        c.accent = Some(accent);
        cases.push((format!("system {accent:?}"), "system", c));
    }
    for (first, second, dark) in [
        ("#ff0000", "#00ffff", "#000000"),
        ("#000000", "#000000", "#ffffff"),
        ("#ffff00", "#ff00ff", "#00ff00"),
        ("#808080", "#7c4dff", "#3a2014"),
    ] {
        let c = ctx(
            &theme,
            &[
                ("bands", text("custom")),
                ("paint_first", text(first)),
                ("paint_second", text(second)),
                ("paint_dark", text(dark)),
            ],
        );
        cases.push((format!("freestyle {first} {second} {dark}"), "custom", c));
    }
    let mut tables = HashSet::new();
    // (The clock runs on from case to case: the fields' allowance is kept by it.)
    for (k, (name, choice, c)) in cases.iter().enumerate() {
        let table = table(&mut theme, c);
        tables.insert(table.clone());
        let all: HashSet<[u8; 4]> = table.iter().copied().collect();
        let first: HashSet<[u8; 4]> = table[..256].iter().copied().collect();
        let mut seen = HashSet::new();
        for i in 0..20 {
            let f = Frame {
                time: k as f64 * 10.0 + i as f64 / 6.0,
                dt: 1.0 / 6.0,
                ..Frame::sized(360.0, 600.0)
            };
            let d = draw_at(&theme, LayerKind::Background, c, &f);
            seen.extend(field(&d).0.pixels.as_chunks::<4>().0);
        }
        let strays: Vec<_> = seen.difference(&all).take(5).collect();
        assert!(strays.is_empty(), "{name}: not of the ramp: {strays:?}");
        let (now, old) = (glyphs(9, 80.0), glyphs(4, 250.0));
        let f = Frame {
            time: k as f64 * 10.0 + 5.0,
            value: Some(value(&now, &old, Role::Calculator, false, None)),
            ..Frame::sized(428.0, 116.0)
        };
        let d = draw_at(&theme, LayerKind::Display, c, &f);
        let figures: HashSet<[u8; 4]> = field(&d)
            .0
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .collect();
        let strays: Vec<_> = figures.difference(&first).take(5).collect();
        assert!(
            strays.is_empty(),
            "{name}: figures not of the first band: {strays:?}"
        );
        let tile = theme.preview(&option, choice, c).unwrap();
        let tiled: HashSet<[u8; 4]> = tile
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Field { picture, .. } => Some(picture.pixels.as_chunks::<4>().0.to_vec()),
                _ => None,
            })
            .flatten()
            .collect();
        let strays: Vec<_> = tiled.difference(&all).take(5).collect();
        assert!(
            strays.is_empty(),
            "{name}: the tile not of the ramp: {strays:?}"
        );
    }
    // Every case its own bands.
    assert_eq!(tables.len(), cases.len());
    let notes = theme.take_draw_notes();
    assert!(notes.is_empty(), "{notes:?}");
}

/// On a see-through window the paint thins itself: the layer is drawn at
/// full strength (`fade = false`), in no group, through a ramp whose dark
/// band clears first (gone from 0.5 down) while the bright ones keep two
/// fifths of their colour at 0.1; every pixel it draws is of that ramp.
/// At full opacity it is the bands' own ramp. The figures stay solid, in
/// the bands' own first band. And each twentieth is a little further on
/// than the one before: the slider moves through the looks smoothly.
#[test]
fn the_paint_thins_itself_on_a_see_through_window() {
    let theme = swirls();
    assert!(theme.def.layers["background"].unfaded);
    let mut c = ctx(&theme, &[]);
    let solid = table(&mut swirls(), &c);
    let first: HashSet<[u8; 4]> = solid[..256].iter().copied().collect();
    let mut last: Option<(f32, f32)> = None;
    for step in (2..=20).rev() {
        c.opacity = step as f32 / 20.0;
        let k = 20 - step;
        let f = Frame {
            time: k as f64,
            ..Frame::sized(360.0, 600.0)
        };
        let d = draw_at(&theme, LayerKind::Background, &c, &f);
        assert!(!d.list.ops.iter().any(|op| matches!(op, Op::PushOpacity(_))));
        let (picture, _) = field(&d);
        let ramp = &picture.ramp;
        let table: HashSet<[u8; 4]> = (0..3 * 256)
            .map(|k| ramp.at((k as f32 + 0.5) / 256.0))
            .collect();
        let strays: Vec<_> = picture
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| !table.contains(*p))
            .take(5)
            .collect();
        assert!(
            strays.is_empty(),
            "{}: not of its ramp: {strays:?}",
            c.opacity
        );
        // The bands' own opacity: the first's, and the dark one's, each where
        // it is alone.
        let bright = ramp.at(0.5 / 256.0)[3] as f32 / 255.0;
        let dark = ramp.at(2.0 + 0.5 / 256.0)[3] as f32 / 255.0;
        if c.opacity == 1.0 {
            let own: Vec<[u8; 4]> = (0..3 * 256)
                .map(|k| ramp.at((k as f32 + 0.5) / 256.0))
                .collect();
            assert_eq!(own, solid);
        }
        if c.opacity <= 0.5 {
            assert_eq!(dark, 0.0, "{}", c.opacity);
        }
        assert!(bright >= 0.39, "{}: {bright}", c.opacity);
        if let Some((was_bright, was_dark)) = last {
            assert!(bright <= was_bright && dark <= was_dark, "{}", c.opacity);
        }
        last = Some((bright, dark));
        // The figures: solid, of the bands' own first band.
        let (now, old) = (glyphs(9, 80.0), glyphs(4, 250.0));
        let f = Frame {
            time: k as f64 + 0.5,
            value: Some(value(&now, &old, Role::Calculator, false, None)),
            ..Frame::sized(428.0, 116.0)
        };
        let d = draw_at(&theme, LayerKind::Display, &c, &f);
        let figures: HashSet<[u8; 4]> = field(&d)
            .0
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .collect();
        let strays: Vec<_> = figures.difference(&first).take(5).collect();
        assert!(strays.is_empty(), "{}: figures {strays:?}", c.opacity);
    }
    // What the thinned paint costs to draw on a 4K screen: within what a
    // frame may, as the paint at full opacity.
    for (k, opacity) in [0.5, 0.15].into_iter().enumerate() {
        c.opacity = opacity;
        let f = Frame {
            time: 40.0 + k as f64,
            ..Frame::sized(3840.0, 2160.0)
        };
        draw_at(&theme, LayerKind::Background, &c, &f);
    }
    let busiest = theme.api().busiest_frame();
    assert!(busiest > 0.0 && busiest < 0.25, "{busiest}");
    let notes = theme.take_draw_notes();
    assert!(notes.is_empty(), "{notes:?}");
}

/// The glass goes from an opaque window's to a see-through one's over a
/// short range (the veil, from 1 to 0.7), not at once: in every set of
/// bands under every glass, the panes' strength at each 64th of opacity
/// (a scheme is made for each) is within a little of the one before.
#[test]
fn the_glass_moves_smoothly_with_the_slider() {
    // (What a style costs is the app's budget, held in the running app
    // and measured here; this test runs wherever the code is built, with
    // four times the time.)
    let mut theme = Theme::load(Source::built_in("swirls").unwrap(), common::limits()).unwrap();
    let mut worst = (0.0f32, String::new());
    let mut slowest = 0.0f64;
    for bands in BANDS {
        for glass in GLASS {
            let mut c = ctx(&theme, &[("bands", text(bands)), ("glass", text(glass))]);
            let mut last: Option<f32> = None;
            for step in (7..=64).rev() {
                c.opacity = step as f32 / 64.0;
                let started = cpu_ms();
                let style = theme.style(&c).unwrap().style.clone();
                slowest = slowest.max(cpu_ms() - started);
                let pane = style.fill("plain.background").samples_over([0.0; 4])[0][3];
                if let Some(was) = last {
                    let jump = (pane - was).abs();
                    if jump > worst.0 {
                        worst = (jump, format!("{bands} {glass} at {step}/64"));
                    }
                }
                last = Some(pane);
            }
        }
    }
    println!(
        "swirls: the glass's biggest step between 64ths: {:.3} ({}); the most processor time a style took {slowest:.1} ms",
        worst.0, worst.1
    );
    assert!(worst.0 <= 0.05, "{} jumps {:.3}", worst.1, worst.0);
}

/// The background at every size a window may have, from the least to a
/// 4K screen: the grid stays within its caps, a frame within its time
/// (first frame, a new snapshot, and a frame with the paint disturbed
/// eight times over), and nothing is refused.
#[test]
fn the_background_stays_within_its_limits_at_any_size() {
    let limit = limits().frame.as_secs_f64() * 1000.0;
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
            assert!(next_thirtieth(d.again, t), "{w}x{h} at {t}");
            // Never more pixels to colour than a 760 × 640 window has at
            // half size, and a little over for the rounding.
            let (picture, _) = field(&d);
            assert!(
                picture.width * picture.height <= 161_000,
                "{w}x{h}: {} x {}",
                picture.width,
                picture.height
            );
            times.push(ms);
        }
        let (mean, worst) = mean_and_worst(&times);
        report.push(format!("{w}x{h}: mean {mean:.2} ms, worst {worst:.2} ms"));
        // The limit is 8 ms, and a frame is held to three quarters of it
        // in an optimised build (an unoptimised one: to all of it; and
        // without `GMNB_REAL_LIMITS`, of the four times it has).
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
        // Its busiest frames cost well within what a frame may to draw.
        let busiest = theme.api().busiest_frame();
        report.push(format!(
            "  the busiest frame cost {:.1}% of what one may",
            busiest * 100.0
        ));
        assert!(busiest > 0.0 && busiest < 0.25, "{w}x{h}: {busiest}");
    }
    println!(
        "swirls, the background stirred eight times over and rippling, by window size \
         (processor time; the limit is {limit} ms):\n  {}",
        report.join("\n  ")
    );
}

/// What a frame costs in a 760 × 640 window, as the app runs the layers:
/// the background and the readout thirty times a second each, flowing at
/// its own pace, and while somebody types (a key every quarter of a
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
    // The limit is 8 ms a frame, and no frame comes near it: no frame
    // carries a whole snapshot but the first (the next is worked out a
    // few rows a frame). Flowing, a frame is a tenth of the limit on
    // average on the machine this was written on, and with six stirs and
    // a ripple alive at once a quarter; here they are held to a quarter
    // and a half, for a machine that is busy or slower.
    let limit = limits().frame.as_secs_f64() * 1000.0;
    for (times, share) in [
        (&flowing, 4.0),
        (&typing, 2.0),
        (&flowing_readout, 8.0),
        (&typing_readout, 8.0),
        (&keypad, 8.0),
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

/// A window dragged larger and smaller, a new size every frame for
/// twenty seconds' worth of frames: the buffers of each size are let go
/// as the next are made, and the theme stays far inside its 8 MiB.
#[test]
fn a_window_being_resized_keeps_its_memory() {
    let theme = swirls();
    let c = ctx(&theme, &[]);
    let key = press(5, KeyKind::Number, 110.0, 300.0);
    let mut most = 0usize;
    for i in 0..1200u32 {
        let t = i as f64 / 60.0;
        // 300 to 1500 wide and back, 400 to 1000 high, out of step.
        let swing = |period: f64| 0.5 - 0.5 * (t * std::f64::consts::TAU / period).cos();
        let (w, h) = (
            (300.0 + 1200.0 * swing(7.0)).round() as f32,
            (400.0 + 600.0 * swing(4.6)).round() as f32,
        );
        let f = Frame {
            time: t,
            dt: 1.0 / 60.0,
            events: vec![(t % 1.0, &key)],
            ..Frame::sized(w, h)
        };
        let d = draw_at(&theme, LayerKind::Background, &c, &f);
        assert_eq!(field(&d).1, [0.0, 0.0, w, h]);
        most = most.max(theme.engine().used_memory());
    }
    println!(
        "swirls, resized every frame: {} KiB of the theme's 8 MiB at most",
        most / 1024
    );
    assert!(most < 4 << 20, "{most} bytes");
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
    assert!(next_thirtieth(rest.again, 0.0), "{:?}", rest.again);
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
    assert!(next_thirtieth(done.again, 2.0), "{:?}", done.again);
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
    // Its busiest frames cost well within what a frame may to draw.
    let busiest = theme.api().busiest_frame();
    println!(
        "swirls (readout): the busiest frame cost {:.1}% of what one may",
        busiest * 100.0
    );
    assert!(busiest > 0.0 && busiest < 0.25, "{busiest}");
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
        [
            "bands",
            "system_note",
            "paint_first",
            "paint_second",
            "paint_dark",
            "glass",
            "speed",
            "stir",
            "digits",
            "motion_note"
        ]
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
    // Eight sets, eight pictures.
    assert_eq!(pictures.len(), 8);
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
    assert!(credits[0].1.contains("src/animations/ColorMix.zig"));
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
    // A saved value that is no set of bands is the default.
    let odd = ctx(&theme, &[("bands", text("nonsense"))]);
    assert!(theme.style(&odd).unwrap().notes.is_empty());
}

/// Every page fits the least window, 300 × 400, as in the base look
/// (docs/theming.md, open point 2). A key or a bit with a least size of
/// its own made the window grow there (Standard to 300 × 472, Programmer
/// to 300 × 566, Graphing to 348 × 400): the keys, the bits and the
/// glyph keys keep none, as the base's keep none, and take what the
/// window gives them. (The graph pad's 32 px rows fit: measured in the
/// app at 300 × 400.)
///
/// And a readout the app shrinks to fit its width keeps the optical size
/// its figures have at their largest. Inter's `opsz` follows the size
/// unless a font names it, and a smaller optical size is a wider cut: a
/// line shrunk for its width came out longer than it, and the date page's
/// "4 months, 1 week, 1 day" was cut off in a 360-pixel window.
#[test]
fn every_page_fits_the_least_window() {
    let mut theme = swirls();
    let base = Style::base(&Ctx::default());
    for opacity in [1.0, 0.5] {
        for glass in GLASS {
            let mut c = ctx(&theme, &[("glass", text(glass))]);
            c.opacity = opacity;
            let style = theme.style(&c).unwrap().style.clone();
            for path in ["keys.min_width", "keys.min_height"] {
                assert!(
                    style.number(path) <= base.number(path),
                    "{path}: {} (the base's: {})",
                    style.number(path),
                    base.number(path)
                );
            }
            for path in [
                "keys.number",
                "keys.func",
                "keys.operator",
                "keys.equals",
                "programmer.bit",
            ] {
                let (ours, theirs) = (style.control(path).unwrap(), base.control(path).unwrap());
                assert!(
                    ours.min_width <= theirs.min_width && ours.min_height <= theirs.min_height,
                    "{path}: {} × {} (the base's: {} × {})",
                    ours.min_width,
                    ours.min_height,
                    theirs.min_width,
                    theirs.min_height
                );
            }
            let Some(Value::Record(pad)) = style.get("keys.pad") else {
                panic!("no keys.pad");
            };
            assert_eq!(pad.get("min_height"), Some(&Value::Number(32.0)));

            // The readouts, at their largest: the calculator's (the
            // standard page's size), the date page's two, the converter's.
            let sized = |path: &str| -> Font {
                let Some(Value::Record(r)) = style.get(path) else {
                    panic!("no {path}");
                };
                let (Some(Value::Number(size)), Some(Value::Font(font))) =
                    (r.get("size"), r.get("font"))
                else {
                    panic!("{path}: no size and font");
                };
                Font {
                    size: *size as f32,
                    ..font.clone()
                }
            };
            let Some(Value::Number(largest)) = style.part("display.sizes", "standard") else {
                panic!("no display.sizes.standard");
            };
            let calculator = Font {
                size: *largest as f32,
                ..style.font("display.value.font").unwrap().clone()
            };
            for (what, font) in [
                ("the calculator's readout", calculator),
                ("date.result", sized("date.result")),
                ("date.add_result", sized("date.add_result")),
                ("converter.value", sized("converter.value")),
            ] {
                assert_eq!(
                    font.family.first().map(String::as_str),
                    Some(INTER),
                    "{what}"
                );
                let opsz: Vec<f32> = font
                    .variations
                    .iter()
                    .filter(|(tag, _)| tag == OPSZ)
                    .map(|(_, v)| *v)
                    .collect();
                // What the app would set at the largest size, held there.
                let want = font.size.clamp(OPSZ_RANGE.0, OPSZ_RANGE.1);
                assert_eq!(opsz, [want], "{what}: {:?}", font.variations);
                let shrunk = Font { size: 16.0, ..font };
                assert!(
                    shrunk
                        .effective_variations()
                        .contains(&(OPSZ.to_string(), want)),
                    "{what}"
                );
            }
        }
    }
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
        for px in picture.pixels.as_chunks::<4>().0 {
            ppm.extend_from_slice(&px[..3]);
        }
        std::fs::write(out.join(format!("f{i:03}.ppm")), ppm).unwrap();
    }
}

// ---- The slow-machine guard ---------------------------------------------------------

/// Swirls with its `os.clock` slow (0.2 ms a call) for its first
/// `slow_calls` calls (a machine that is busy for a moment): the guard
/// (`keep`) times the work of a new window size with it.
fn swirls_timed(name: &str, slow_calls: u32) -> Theme {
    swirls_clocked(name, 0, slow_calls, 0.0002)
}

/// Swirls with its `os.clock` replaced by one that runs `step` s a call
/// from call `from` up to call `until`, and a millionth of a second
/// otherwise: a machine that is busy for a while, from when to when.
fn swirls_clocked(name: &str, from: u32, until: u32, step: f64) -> Theme {
    swirls_advanced(
        name,
        &format!(
            "now += if ticks > {from} and ticks <= {until} then {step} else 0.000001"
        ),
    )
}

/// Swirls with its `os.clock` moved on, at each call, by `advance` (Lua
/// statements that add to `now`; `ticks` counts the calls, `seed` is its own).
fn swirls_advanced(name: &str, advance: &str) -> Theme {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    let dir = std::env::temp_dir().join(format!("gmnb-swirls-timed-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/gmnb/themes/swirls"),
        &dir,
    );
    let file = dir.join("theme.luau");
    let source = std::fs::read_to_string(&file).unwrap();
    let shim = format!(
        "local ticks, now, seed = 0, 0, 7\nlocal os = {{ clock = function(): number\n\tticks += 1\n\t{advance}\n\treturn now\nend }}\n"
    );
    let (first, rest) = source.split_once('\n').unwrap();
    assert_eq!(first, "--!strict");
    std::fs::write(&file, format!("{first}\n{shim}{rest}")).unwrap();
    let theme =
        Theme::load(Source::folder("swirls", &dir), limits()).unwrap_or_else(|e| panic!("{e}"));
    let _ = std::fs::remove_dir_all(&dir);
    theme
}

/// The width of the picture the background's field is coloured at: the
/// finer the grid, the wider.
fn field_width(theme: &Theme, w: f32, h: f32) -> u32 {
    let c = ctx(theme, &[]);
    let f = Frame {
        dt: 0.0,
        ..Frame::sized(w, h)
    };
    let d = draw_at(theme, LayerKind::Background, &c, &f);
    field(&d).0.width
}

/// A machine that is busy for a moment looks slow for that moment (the
/// clock is the clock on the wall): that must not make the paint coarser
/// for good; a machine that is slow does; and a size that then times
/// quick has the fine grid again.
#[test]
fn the_guard_is_not_fooled_by_a_moment_of_busy_and_lets_go_of_a_slow_one() {
    // The calls before a first timing ends: the two of the frame's own
    // (the guard's and the background's), the one that starts the timing,
    // then two for each of three batches. Slow for the first timing only.
    let fine = field_width(&swirls_timed("fast", 0), 760.0, 640.0);
    let moment = field_width(&swirls_timed("moment", 9), 760.0, 640.0);
    assert_eq!(moment, fine, "a moment of busy made the paint coarser");
    // Slow throughout: coarser.
    let theme = swirls_timed("slow", 16);
    let coarse = field_width(&theme, 760.0, 640.0);
    assert!(coarse < fine, "{coarse} {fine}");
    // And the next size that times quick: fine again.
    let again = field_width(&theme, 761.0, 640.0);
    assert!(again >= fine - 2, "{again} {fine}");
}

/// A machine that becomes busy after a window's size was timed (its first
/// frame was quick) must make the paint coarser, a frame at a time, rather
/// than have the hook run over its 8 ms until the engine takes it off: the
/// frames themselves tell (`background`). Here every call of the clock
/// after the first frame is 4 ms; the theme must not strike, and the grid
/// must be coarser after a handful of frames than it was.
#[test]
fn a_machine_that_becomes_busy_makes_the_paint_coarser_not_a_hook_that_is_switched_off() {
    // The first frame's calls: the guard's and the frame's, the one that
    // starts the timing, the timing's six, the guard's last.
    let theme = swirls_clocked("busy-later", 10, u32::MAX, 0.004);
    let c = ctx(&theme, &[]);
    let draw = |t: f64| {
        let f = Frame {
            time: t,
            dt: 1.0 / 30.0,
            ..Frame::sized(1280.0, 800.0)
        };
        let d = draw_at(&theme, LayerKind::Background, &c, &f);
        field(&d).0.width
    };
    let fine = draw(0.0);
    let mut coarse = fine;
    for i in 1..40 {
        coarse = draw(i as f64 / 30.0);
    }
    assert!(coarse < fine, "{coarse} {fine}");
    assert!(
        theme
            .hooks()
            .iter()
            .all(|(_, strikes, off)| *strikes == 0 && !off),
        "{:?}",
        theme.hooks()
    );
}

/// A machine that stays busy must not have the paint go finer and coarser
/// and finer again (R9-A-1): after the frames made it coarser, the timing of
/// the next picture, which is made of a few quick calls and can read quick
/// between slow frames, may not take it back to fine until a quiet spell has
/// gone by. Counted: the times the grid gets finer in 200 frames of a clock
/// that is held up throughout.
#[test]
fn a_machine_that_stays_busy_does_not_have_the_paint_pump_between_coarse_and_fine() {
    for stall in [0.003, 0.005, 0.008] {
        // After the first frame a third of the calls of the clock (by a fixed pseudo-random sequence) find the
        // process held up for `stall` s (the machine shared with another
        // process, whose turn it is): the frames are slow, and a batch of
        // the timing, which is two calls, is mostly quick.
        let theme = swirls_advanced(
            "pump",
            &format!(
                "seed = (seed * 16807) % 2147483647\n\tnow += if ticks > 10 and seed % 3 == 0 then {stall} else 0.0001"
            ),
        );
        let mean = stall;
        let c = ctx(&theme, &[]);
        let mut widths = Vec::new();
        for i in 0..200 {
            let f = Frame {
                time: i as f64 / 30.0,
                dt: 1.0 / 30.0,
                ..Frame::sized(1280.0, 800.0)
            };
            let d = draw_at(&theme, LayerKind::Background, &c, &f);
            widths.push(field(&d).0.width);
        }
        let finer = widths.windows(2).filter(|w| w[1] > w[0] + 1).count();
        println!("{mean}: {finer} finer, {widths:?}");
        assert!(finer <= 1, "{mean}: {finer} times finer: {widths:?}");
        assert!(theme.hooks().iter().all(|(_, s, off)| *s == 0 && !off));
    }
}

/// And when the machine is quiet again the paint is as fine as a quiet
/// machine's, not as fine as the timing of a new size happens to leave it
/// (R9-A-2): that timing only makes the grid finer when the frame would take
/// under half of what is afforded, so a grid made coarser by a busy spell,
/// timed again after the quiet spell, stayed 1.6 times coarser for good. The
/// clock here is busy (4 ms a call) for the first 400 calls, then takes `quiet`
/// s a call.
#[test]
fn when_the_machine_is_quiet_again_the_paint_is_as_fine_as_on_a_quiet_one() {
    for quiet in [0.00005, 0.0001, 0.00015, 0.0002] {
        let run = |busy: bool| {
            let theme = swirls_advanced(
                "recover",
                &format!(
                    "now += if ticks > 10 and ticks <= {} then 0.004 else {quiet}",
                    if busy { 400 } else { 0 }
                ),
            );
            let c = ctx(&theme, &[]);
            let mut width = 0;
            for i in 0..1200 {
                let f = Frame {
                    time: i as f64 / 30.0,
                    dt: 1.0 / 30.0,
                    ..Frame::sized(1600.0, 1000.0)
                };
                let d = draw_at(&theme, LayerKind::Background, &c, &f);
                width = field(&d).0.width;
            }
            width
        };
        let (after, fresh) = (run(true), run(false));
        println!("{quiet}: after a busy spell {after}, fresh {fresh}");
        assert_eq!(after, fresh, "{quiet}");
    }
}

// ---- An event that is still moving the paint when it goes ------------------------

/// How many pixels of the paint an event of `kind` moves, `age` seconds
/// after it, in a `width` x `height` window standing still (speed 0, so
/// the field itself is the same in every frame).
fn event_pixels(width: f32, height: f32, kind: EventKind, age: f64) -> usize {
    let theme = swirls();
    let c = ctx(&theme, &[("speed", OptionValue::Number(0.0))]);
    let at = |time: f64, events: Vec<(f64, &Event)>| {
        let f = Frame {
            time,
            dt: 1.0 / 30.0,
            events,
            ..Frame::sized(width, height)
        };
        let d = draw_at(&theme, LayerKind::Background, &c, &f);
        field(&d).0.pixels.clone()
    };
    let result = outcome(kind, width);
    let _ = at(10.0, vec![]);
    let with = at(10.5, vec![(age, &result)]);
    let without = at(11.0, vec![]);
    assert_eq!(with.len(), without.len());
    with.as_chunks::<4>()
        .0
        .iter()
        .zip(without.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count()
}

/// Every event is dropped when its time is up: a result's after 2.5
/// seconds, the others' after 1.3. What it did to the paint must have worn
/// out by then, or the picture jumps: a ripple in a big window (it takes
/// longer than 2.5 s to cross a maximised one), the slow turn of a page
/// change or a new colour (still some pixels of turn at 1.3 s).
#[test]
fn what_an_event_does_to_the_paint_has_worn_out_by_the_time_it_goes() {
    for (kind, life) in [
        (EventKind::Result, 2.5),
        (EventKind::Error, 1.3),
        (EventKind::Mode, 1.3),
        (EventKind::Option, 1.3),
    ] {
        for (w, h) in [(760.0, 640.0), (1920.0, 1080.0), (3840.0, 2160.0)] {
            // (It does move the paint while it lives.)
            let alive = event_pixels(w, h, kind, 0.5);
            assert!(alive > 100, "{kind:?} {w}x{h}: {alive} pixels at 0.5 s");
            // And a frame before the event is dropped it has all but gone:
            // a small share of what it moved.
            let last = event_pixels(w, h, kind, life - 0.04);
            assert!(
                last * 100 <= alive,
                "{kind:?} {w}x{h}: still moving {last} pixels when its event goes (at 0.5 s: {alive})"
            );
        }
    }
}
