//! Glazed (apps/gmnb/themes/glazed/theme.luau) in the real engine, without
//! a toolkit: it loads and returns only what the schema knows, its four
//! layers draw what they should within their limits and stand still when
//! nothing happens, and everything its layers paint under or as text
//! reads (the readability tables of `css.rs` see a style's fills, not what
//! a layer draws: the awning under the header, the icing on the digits).

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::time::Instant;

use appcore::color::{TEXT_CONTRAST, UI_CONTRAST, contrast};
use gmnb_theme::draw::{MaskMode, Op};
use gmnb_theme::events::{Cause, Event, EventKind, KeyKind, Page, Source as PressSource, Via};
use gmnb_theme::layer::{
    ChangeKind, DisplayValue, Drawn, Failure, Frame, GlyphBox, Glyphs, KeyCell, Keys, LayerKind,
    Role,
};
use gmnb_theme::picture::{ImageId, PictureKind, Pixels};
use gmnb_theme::states::over;
use gmnb_theme::theme::{self, Source, Theme};
use gmnb_theme::types::*;
use gmnb_theme::{Ctx, Limits, OptionValue, Style};

const FLAVOURS: [&str; 5] = ["strawberry", "blueberry", "matcha", "lemon", "caramel"];

/// The header's height, and how much of it is the awning's stripes
/// (theme.luau, `HEADER` and `AWNING`).
const HEADER: f32 = 52.0;
const AWNING: f32 = 36.0;

fn glazed() -> Theme {
    let theme = Theme::load(Source::built_in("glazed").unwrap(), Limits::default())
        .unwrap_or_else(|e| panic!("Glazed didn't load: {e}"));
    assert!(theme.notes.is_empty(), "{:#?}", theme.notes);
    theme
}

/// `ctx` for a flavour, with the other options as given (else their
/// defaults: the doughnut, the falling sprinkles and the fountain all on).
fn ctx(theme: &Theme, dark: bool, flavour: &str, options: &[(&str, bool)]) -> Ctx {
    let mut saved = BTreeMap::from([(
        "flavour".to_string(),
        OptionValue::Text(flavour.to_string()),
    )]);
    for (id, on) in options {
        saved.insert(id.to_string(), OptionValue::Bool(*on));
    }
    Ctx {
        dark,
        options: theme::complete_options(&theme.def.options, &saved),
        ..Ctx::default()
    }
}

fn ok(r: Result<Drawn, Failure>) -> Drawn {
    match r {
        Ok(d) => d,
        Err(Failure::Failed(e)) => panic!("{e}"),
        Err(Failure::Off) => panic!("off"),
    }
}

/// A clock the test holds, for the allowance of the pictures the theme
/// fills: every frame drawn here is a second after the one before.
fn clock(theme: &Theme) -> impl Fn() + use<> {
    let now = Rc::new(std::cell::Cell::new(100.0));
    let read = now.clone();
    theme.api().set_clock(Rc::new(move || read.get()));
    move || now.set(now.get() + 1.0)
}

fn rgb3(c: Rgba) -> [f32; 3] {
    [c[0], c[1], c[2]]
}

/// The contrast of `ink` (which may be translucent) on `ground`.
fn on(ink: Rgba, ground: Rgba) -> f32 {
    contrast(rgb3(over(ink, ground)), rgb3(ground))
}

fn solid(paint: &Paint) -> Rgba {
    match paint {
        Paint::Color(c) => *c,
        other => panic!("not a colour: {other:?}"),
    }
}

fn median(mut times: Vec<f64>) -> f64 {
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

/// A calculator keypad's cells, 6 rows of 4, the equals key last.
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
                rect: [col as f32 * 90.0, row as f32 * 60.0, 85.0, 55.0],
                row,
                col,
                radius: 6.0,
                enabled: true,
                hovered: false,
                pressed: false,
                checked: false,
            });
        }
    }
    cells
}

fn keys(cells: &[KeyCell]) -> Keys<'_> {
    Keys {
        pad: 1,
        layout: 1,
        cells,
    }
}

/// `n` digits set against the end of a readout `width` wide, in Jersey 10
/// at `size`: their boxes as the app would lay them out, near enough.
fn digits(n: usize, width: f32, size: f32) -> Glyphs {
    let advance = size * 0.42;
    let x0 = width - 14.0 - n as f32 * advance;
    let line = size * 1500.0 / 1400.0;
    Glyphs {
        glyphs: (0..n)
            .map(|i| GlyphBox {
                x: x0 + i as f32 * advance,
                y: 40.0,
                w: advance - 2.0,
                h: line,
            })
            .collect(),
        size,
        rect: [x0, 40.0, n as f32 * advance, line],
    }
}

fn value<'a>(
    glyphs: &'a Glyphs,
    version: u64,
    error: bool,
    change: Option<(ChangeKind, f64)>,
    role: Role,
) -> DisplayValue<'a> {
    DisplayValue {
        readout: 1,
        version,
        text: "123456789",
        glyphs,
        band: [0.0, glyphs.rect[1] - 4.0, 344.0, glyphs.rect[3] + 8.0],
        error,
        align_end: role == Role::Calculator,
        role,
        change: change.map(|(kind, age)| (kind, age, 1)),
        previous: change.map(|_| glyphs),
    }
}

/// The picture of each kind the theme has, by what it is.
struct Pictures {
    walls: [ImageId; 2],
    hem: [ImageId; 3],
    falling: ImageId,
    icing: ImageId,
    buddy: ImageId,
}

fn pictures(theme: &Theme) -> Pictures {
    let mut files = BTreeMap::new();
    let mut canvases = Vec::new();
    for i in 0..16 {
        let id = ImageId(i);
        match theme.api().kind(id) {
            Some(PictureKind::File(name) | PictureKind::Animation(name, _)) => {
                files.insert(name, id);
            }
            Some(PictureKind::Canvas) => canvases.push(id),
            _ => {}
        }
    }
    let file = |name: &str| *files.get(name).unwrap_or_else(|| panic!("no {name}"));
    assert_eq!(files.len(), 6, "{files:?}");
    assert_eq!(canvases.len(), 2);
    let size = |id: ImageId| {
        let c = theme.api().canvas(id).unwrap();
        (c.width, c.height)
    };
    assert_eq!(
        (size(canvases[0]), size(canvases[1])),
        ((256, 28), (24, 24))
    );
    Pictures {
        walls: [file("images/wall-light.png"), file("images/wall-dark.png")],
        hem: [
            file("images/scallop-odd.png"),
            file("images/scallop-even.png"),
            file("images/scallop-line.png"),
        ],
        falling: file("images/sprinkles.gif"),
        icing: canvases[0],
        buddy: canvases[1],
    }
}

/// Every colour a decoded still or a filled picture holds, see-through
/// pixels left out.
fn colours_of(theme: &Theme, id: ImageId) -> BTreeSet<[u8; 4]> {
    let bytes = match theme.api().canvas(id) {
        Some(canvas) => canvas.pixels.to_vec(),
        None => match theme.api().pictures().pixels(id) {
            Some(Pixels::Rgba8 { bytes, .. }) => bytes,
            _ => panic!("picture {} has no pixels", id.0),
        },
    };
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] != 0)
        .copied()
        .collect()
}

fn rgba(p: [u8; 4]) -> Rgba {
    [
        p[0] as f32 / 255.0,
        p[1] as f32 / 255.0,
        p[2] as f32 / 255.0,
        p[3] as f32 / 255.0,
    ]
}

// ---- It loads -------------------------------------------------------------------------

/// Glazed loads in the real engine, its `style` runs for every flavour,
/// light and dark, at three opacities, and every key it returns is one
/// the schema knows, of the right type: no notes at all.
#[test]
fn glazed_loads_and_returns_only_known_keys() {
    let mut theme = glazed();
    assert_eq!(theme.def.meta.name, "Glazed");
    assert_eq!(theme.def.fonts, ["fonts/Jersey10-Regular.ttf"]);
    assert_eq!(theme.credits()[0].0, "Jersey 10 typeface");
    assert!(theme.credits()[0].1.contains("SIL Open Font License"));
    assert!(theme.credits()[0].1.contains("Soft Type Project Authors"));
    assert_eq!(theme.font_files().len(), 1);
    let ids: Vec<&str> = theme.def.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(
        ids,
        ["flavour", "buddy", "living", "fountain", "motion_note"]
    );
    for name in ["background", "keypad", "display", "overlay"] {
        assert!(theme.def.layers[name].draw.is_some(), "{name}");
    }
    assert!(theme.def.layers["keypad"].place.is_some());
    assert!(!theme.def.layers["keypad"].pointer);
    for flavour in FLAVOURS {
        for dark in [false, true] {
            for opacity in [1.0, 0.5, 0.1] {
                let mut c = ctx(&theme, dark, flavour, &[]);
                c.opacity = opacity;
                let r = theme
                    .style(&c)
                    .unwrap_or_else(|e| panic!("{flavour} {dark} {opacity}: {e}"));
                assert!(
                    r.notes.is_empty(),
                    "{flavour} {dark} {opacity}: {:#?}",
                    r.notes
                );
                let s = &r.style;
                assert_eq!(s.radius("keys.radius"), Radius::all(6.0));
                assert_eq!(s.number("header.height"), HEADER);
                assert!(s.native().is_empty());
                let eq = s.control("keys.equals").unwrap();
                assert_eq!(eq.font.family[0], "Jersey 10");
                // Whatever carries text is solid at any opacity: the
                // desktop shows between things, never under them.
                for path in ["plain.background", "display.background"] {
                    let fill = s.fill(path);
                    assert!(
                        fill.samples_over([0.0, 0.0, 0.0, 0.0])
                            .iter()
                            .all(|c| c[3] >= 0.999),
                        "{path}"
                    );
                }
                // The awning is the wall's picture and thins with it; on
                // a see-through window the header has a card of its own.
                let header = s.fill("header.background");
                let shows = header.samples_over([0.0, 0.0, 0.0, 0.0])[0][3];
                assert_eq!(shows, if opacity < 1.0 { 1.0 } else { 0.0 });
            }
        }
    }
    assert!(theme.take_log().is_empty());
}

// ---- Its layers ------------------------------------------------------------------------

/// The four layers in the real engine, under the real limits: what they
/// draw at rest and mid-animation, what they ask for, and what a frame
/// costs. (The limits are of processor time, so a shared machine taking
/// the processor away for a while fails nothing here.)
#[test]
fn glazeds_layers_draw_within_their_limits() {
    let theme = glazed();
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
            "keypad.place",
            "overlay.draw"
        ]
    );
    let tick = clock(&theme);
    let pics = pictures(&theme);
    let frame = 1.0 / 30.0;

    // ---- Background: the wall, the sprinkles, the awning.
    for (dark, flavour) in [(false, "strawberry"), (true, "matcha")] {
        let quiet = ctx(&theme, dark, flavour, &[("living", false)]);
        let f = Frame::sized(760.0, 640.0);
        let drawn = ok(theme.draw(LayerKind::Background, &quiet, &f));
        // The wall over everything first; then the awning: its ground,
        // twelve stripes, and the hem's three masks, each a mask, the
        // picture, a pop, the colour and a pop.
        assert_eq!(drawn.list.ops.len(), 1 + 1 + 12 + 3 * 5, "{flavour}");
        let wall = pics.walls[dark as usize];
        assert!(matches!(
            drawn.list.ops[0],
            Op::Tile { image, rect, .. } if image == wall && rect == [0.0, 0.0, 760.0, 640.0]
        ));
        let hems: Vec<ImageId> = drawn
            .list
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Tile { image, rect, .. } if *rect == [0.0, AWNING, 760.0, HEADER - AWNING] => {
                    Some(*image)
                }
                _ => None,
            })
            .collect();
        assert_eq!(hems, pics.hem);
        // It stands: nothing asked for, nothing due.
        assert_eq!((drawn.again, drawn.due), (None, None));

        // Living: the loop side by side under the awning, and the app
        // told when its next frame is due (an eighth of a second).
        let living = ctx(&theme, dark, flavour, &[]);
        let drawn = ok(theme.draw(LayerKind::Background, &living, &f));
        let loops: Vec<&Op> = drawn
            .list
            .ops
            .iter()
            .filter(|op| matches!(op, Op::Animation { .. }))
            .collect();
        assert_eq!(loops.len(), 9);
        for op in &loops {
            let Op::Animation {
                image, still, rect, ..
            } = op
            else {
                unreachable!()
            };
            assert_eq!((*image, *still), (pics.falling, false));
            assert_eq!((rect[2], rect[3]), (256.0, 256.0));
            assert!(rect[1] >= AWNING);
        }
        assert_eq!(drawn.again, None);
        let due = drawn.due.expect("the loop's next frame");
        assert!((0.1..=0.13).contains(&due), "{due}");
        // Under reduced motion its first frame stands.
        let still = Frame {
            motion: false,
            ..f.clone()
        };
        let drawn = ok(theme.draw(LayerKind::Background, &living, &still));
        assert_eq!(drawn.due, None);
        assert!(
            drawn
                .list
                .ops
                .iter()
                .all(|op| !matches!(op, Op::Animation { still: false, .. }))
        );
    }
    let living = ctx(&theme, false, "strawberry", &[]);
    let f = Frame::sized(760.0, 640.0);
    let mut times = Vec::new();
    for _ in 0..200 {
        let t = Instant::now();
        ok(theme.draw(LayerKind::Background, &living, &f));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let background_ms = median(times);

    // ---- Overlay: the fountain, and a new flavour's shower.
    let readout = [8.0, 58.0, 344.0, 122.0];
    let equals_key = [270.0, 540.0, 82.0, 52.0];
    let mut press = Event::at(EventKind::Press, Page::Calculator, equals_key);
    press.source = Some(PressSource::Key);
    press.via = Some(Via::Pointer);
    press.key = Some((Some(23), KeyKind::Equals));
    let mut result = Event::at(EventKind::Result, Page::Calculator, readout);
    result.cause = Some(Cause::Equals);
    let mut total = result.clone();
    total.cause = Some(Cause::Operator);
    let mut flavour = Event::at(EventKind::Option, Page::Settings, [0.0, 0.0, 360.0, 600.0]);
    flavour.option = Some(("flavour".into(), OptionValue::Text("lemon".into())));
    let f = Frame::sized(360.0, 600.0);
    let nothing = ok(theme.draw(LayerKind::Overlay, &living, &f));
    assert_eq!((nothing.list.ops.len(), nothing.again), (0, None));
    let mut seen = Vec::new();
    let mut times = Vec::new();
    for step in 0..40 {
        let age = step as f64 * 0.03;
        let f = Frame {
            events: vec![(age + 0.01, &press), (age, &result)],
            ..Frame::sized(360.0, 600.0)
        };
        let t = Instant::now();
        let drawn = ok(theme.draw(LayerKind::Overlay, &living, &f));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        if age < 1.1 {
            assert_eq!(drawn.again, Some(frame), "{age}");
        } else {
            assert_eq!((drawn.again, drawn.list.ops.len()), (None, 0), "{age}");
        }
        for op in &drawn.list.ops {
            let Op::Rect { rect, .. } = op else {
                panic!("{op:?}")
            };
            // A sprinkle: three art pixels by one, on the art grid, and
            // never as high as the readout.
            assert!(
                [rect[2], rect[3]] == [6.0, 2.0] || [rect[2], rect[3]] == [2.0, 6.0],
                "{rect:?}"
            );
            assert!(rect[1] > readout[1] + readout[3], "{age}: {rect:?}");
        }
        seen.push(drawn.list.ops.len());
    }
    // Thirty are thrown; they are gone one by one.
    assert_eq!(seen[1], 30);
    assert!(seen.windows(2).all(|w| w[1] <= w[0]) && seen[30] > 0 && seen[30] < 30);
    let overlay_ms = median(times);
    // The same event is the same fountain from frame to frame: at the
    // same age, the same picture.
    let again = |age: f64| {
        let f = Frame {
            events: vec![(age + 0.01, &press), (age, &result)],
            ..Frame::sized(360.0, 600.0)
        };
        ok(theme.draw(LayerKind::Overlay, &living, &f)).list
    };
    assert_eq!(again(1.25), again(1.25));
    // The running total an operator shows throws nothing; nor does
    // anything with the fountain switched off.
    let f = Frame {
        events: vec![(0.2, &total)],
        ..Frame::sized(360.0, 600.0)
    };
    assert_eq!(
        ok(theme.draw(LayerKind::Overlay, &living, &f))
            .list
            .ops
            .len(),
        0
    );
    let off = ctx(&theme, false, "strawberry", &[("fountain", false)]);
    let f = Frame {
        events: vec![(0.21, &press), (0.2, &result), (0.3, &flavour)],
        ..Frame::sized(360.0, 600.0)
    };
    let drawn = ok(theme.draw(LayerKind::Overlay, &off, &f));
    assert_eq!((drawn.list.ops.len(), drawn.again), (0, None));
    // A new flavour: sprinkles from under the awning.
    let f = Frame {
        events: vec![(0.6, &flavour)],
        ..Frame::sized(360.0, 600.0)
    };
    let drawn = ok(theme.draw(LayerKind::Overlay, &living, &f));
    assert_eq!(drawn.again, Some(frame));
    assert!(drawn.list.ops.len() > 10);
    for op in &drawn.list.ops {
        assert!(matches!(op, Op::Rect { rect, .. } if rect[1] >= HEADER - 4.0));
    }

    // ---- Keypad: stacking, sinking, hopping, crumbs.
    let mut cells = keypad_cells();
    let pad = [0.0, 0.0, 355.0, 355.0];
    let show = Event::at(EventKind::Show, Page::Calculator, pad);
    let f = Frame {
        events: vec![(0.1, &show)],
        keys: Some(keys(&cells)),
        ..Frame::sized(355.0, 355.0)
    };
    let stacking = ok(theme.draw(LayerKind::Keypad, &living, &f));
    assert_eq!(stacking.again, Some(frame));
    assert_eq!(stacking.places.len(), 24);
    // The bottom row is on its way down; the top row hasn't started.
    assert!(matches!(stacking.places[20], Some(p) if p.dy < 0.0 && p.opacity == 1.0));
    assert_eq!(stacking.places[0].map(|p| p.opacity), Some(0.0));
    // Keys move a whole art pixel at a time.
    for p in stacking.places.iter().flatten() {
        assert_eq!((p.dx % 2.0, p.dy % 2.0, p.scale), (0.0, 0.0, 1.0));
    }
    let f = Frame {
        events: vec![(1.19, &show)],
        keys: Some(keys(&cells)),
        ..Frame::sized(355.0, 355.0)
    };
    let landed = ok(theme.draw(LayerKind::Keypad, &living, &f));
    assert_eq!(landed.again, None);
    assert!(landed.places.iter().all(Option::is_none));
    // At rest only the equals key's icing is drawn: a shine (two
    // rectangles) and three sprinkles, clear of its sign.
    assert_eq!(landed.list.ops.len(), 5);
    let key = cells[23].rect;
    let sign = [
        key[0] + key[2] / 2.0 - 12.0,
        key[1] + key[3] / 2.0 - 12.0,
        24.0,
        24.0,
    ];
    for op in &landed.list.ops {
        let Op::Rect { rect, .. } = op else {
            panic!("{op:?}")
        };
        let inside = rect[0] >= key[0]
            && rect[1] >= key[1]
            && rect[0] + rect[2] <= key[0] + key[2]
            && rect[1] + rect[3] <= key[1] + key[3];
        let over_sign = rect[0] < sign[0] + sign[2]
            && rect[0] + rect[2] > sign[0]
            && rect[1] < sign[1] + sign[3]
            && rect[1] + rect[3] > sign[1];
        assert!(inside && !over_sign, "{rect:?}");
    }
    // A key held down has sunk an art pixel, and its icing with it.
    cells[23].pressed = true;
    cells[9].pressed = true;
    let f = Frame {
        keys: Some(keys(&cells)),
        ..Frame::sized(355.0, 355.0)
    };
    let held = ok(theme.draw(LayerKind::Keypad, &living, &f));
    assert_eq!(held.again, None);
    assert_eq!(held.places[9].map(|p| (p.dx, p.dy)), Some((0.0, 2.0)));
    assert_eq!(held.places[23].map(|p| (p.dx, p.dy)), Some((0.0, 2.0)));
    assert_eq!(held.places.iter().flatten().count(), 2);
    let (Op::Rect { rect: up, .. }, Op::Rect { rect: down, .. }) =
        (&landed.list.ops[0], &held.list.ops[0])
    else {
        panic!()
    };
    assert_eq!(down[1], up[1] + 2.0);
    cells[23].pressed = false;
    cells[9].pressed = false;
    // Let go, it hops; and crumbs fly from where it was pressed.
    let mut tap = Event::at(EventKind::Press, Page::Calculator, cells[9].rect);
    tap.source = Some(PressSource::Key);
    tap.via = Some(Via::Pointer);
    tap.key = Some((Some(9), KeyKind::Number));
    let f = Frame {
        events: vec![(0.02, &tap)],
        keys: Some(keys(&cells)),
        ..Frame::sized(355.0, 355.0)
    };
    let hopping = ok(theme.draw(LayerKind::Keypad, &living, &f));
    assert_eq!(hopping.again, Some(frame));
    assert_eq!(hopping.places[9].map(|p| p.dy), Some(-4.0));
    assert_eq!(hopping.list.ops.len(), 5 + 5);
    // A key typed on the keyboard is shown held for 110 ms first.
    tap.via = Some(Via::Keyboard);
    let f = Frame {
        events: vec![(0.12, &tap)],
        keys: Some(keys(&cells)),
        ..Frame::sized(355.0, 355.0)
    };
    let typed = ok(theme.draw(LayerKind::Keypad, &living, &f));
    assert_eq!(typed.places[9].map(|p| p.dy), Some(-4.0));
    // A result sends a hop out from the equals key: it has left the
    // equals key by the time it reaches the far corner.
    let mut pad_press = press.clone();
    pad_press.rect = cells[23].rect;
    let mut pad_result = result.clone();
    pad_result.rect = [0.0, -150.0, 355.0, 122.0];
    let wave = |age: f64| {
        let f = Frame {
            events: vec![(age + 0.01, &pad_press), (age, &pad_result)],
            keys: Some(keys(&cells)),
            ..Frame::sized(355.0, 355.0)
        };
        ok(theme.draw(LayerKind::Keypad, &living, &f))
    };
    let near = wave(0.07);
    assert!(matches!(near.places[23], Some(p) if p.dy < 0.0));
    assert_eq!(near.places[0], None);
    let far = wave(0.42);
    assert!(matches!(far.places[0], Some(p) if p.dy < 0.0));
    assert_eq!(far.again, Some(frame));
    assert_eq!(wave(0.59).again, None);
    // An error shakes the whole pad.
    let error = Event::at(
        EventKind::Error,
        Page::Calculator,
        [0.0, -150.0, 355.0, 122.0],
    );
    let f = Frame {
        events: vec![(0.06, &error)],
        keys: Some(keys(&cells)),
        ..Frame::sized(355.0, 355.0)
    };
    let shaken = ok(theme.draw(LayerKind::Keypad, &living, &f));
    assert!(shaken.places.iter().all(|p| p.map(|p| p.dx) == Some(2.0)));
    // Under reduced motion nothing is placed and nothing asked for.
    let f = Frame {
        motion: false,
        keys: Some(keys(&cells)),
        ..Frame::sized(355.0, 355.0)
    };
    let still = ok(theme.draw(LayerKind::Keypad, &living, &f));
    assert!(still.places.is_empty());
    assert_eq!((still.again, still.list.ops.len()), (None, 5));
    let f = Frame {
        events: vec![(0.1, &show), (0.02, &tap), (0.05, &pad_result)],
        keys: Some(keys(&cells)),
        ..Frame::sized(355.0, 355.0)
    };
    let mut times = Vec::new();
    for _ in 0..200 {
        let t = Instant::now();
        ok(theme.draw(LayerKind::Keypad, &living, &f));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let keypad_ms = median(times);

    // ---- Display: plain digits, a result glazed, the doughnut.
    let now = digits(5, 344.0, 74.667);
    let display = |c: &Ctx, v: DisplayValue, events: Vec<(f64, &Event)>, time: f64| {
        tick();
        let f = Frame {
            value: Some(v),
            events,
            time,
            ..Frame::sized(344.0, 122.0)
        };
        ok(theme.draw(LayerKind::Display, c, &f))
    };
    let count = |d: &Drawn, what: fn(&Op) -> bool| d.list.ops.iter().filter(|op| what(op)).count();
    let glyph = |op: &Op| matches!(op, Op::Glyph { .. });
    let mask = |op: &Op| matches!(op, Op::PushMask(MaskMode::Alpha));
    let image_of = |d: &Drawn, id: ImageId| {
        d.list.ops.iter().find_map(|op| match op {
            Op::Image { image, rect, .. } if *image == id => Some(*rect),
            _ => None,
        })
    };
    let nothing_living = ctx(
        &theme,
        false,
        "strawberry",
        &[("living", false), ("buddy", true)],
    );
    // A number being typed: plain digits, once each, no icing; the
    // doughnut on the shelf; and with nothing living it stands.
    let typing = display(
        &nothing_living,
        value(
            &now,
            1,
            false,
            Some((ChangeKind::Typing, 2.9)),
            Role::Calculator,
        ),
        vec![],
        0.0,
    );
    assert_eq!((count(&typing, glyph), count(&typing, mask)), (5, 0));
    assert_eq!(typing.again, None);
    let buddy = image_of(&typing, pics.buddy).expect("the doughnut");
    assert_eq!((buddy[2], buddy[3]), (48.0, 48.0));
    assert!(
        buddy[0] + buddy[2] < now.rect[0],
        "it stands clear of the number"
    );
    assert!(
        typing
            .list
            .ops
            .iter()
            .any(|op| matches!(op, Op::Border { rect, .. } if *rect == [0.0, 0.0, 344.0, 122.0]))
    );
    // Living, it blinks now and then: the next frame is seconds away.
    let idle = display(
        &living,
        value(&now, 1, false, None, Role::Calculator),
        vec![],
        1.0,
    );
    assert!(
        matches!(idle.again, Some(t) if t > 3.0 && t < 3.3),
        "{:?}",
        idle.again
    );
    let blink = display(
        &living,
        value(&now, 1, false, None, Role::Calculator),
        vec![],
        4.25,
    );
    assert!(matches!(blink.again, Some(t) if t < 0.14));
    // A new digit drops into place.
    let dropping = display(
        &nothing_living,
        value(
            &now,
            2,
            false,
            Some((ChangeKind::Typing, 0.01)),
            Role::Calculator,
        ),
        vec![],
        0.0,
    );
    assert_eq!(dropping.again, Some(frame));
    assert!(dropping.list.ops.contains(&Op::Translate(0.0, -6.0)));
    // A result: the icing pours, through the digits' own shapes, and the
    // sprinkles land after it. Each frame of it asks for the next.
    // (Without the doughnut here, whose jump asks for frames of its own.)
    // The running total an operator shows is no such result: it stays
    // plain.
    let bare = ctx(
        &theme,
        false,
        "strawberry",
        &[("living", false), ("buddy", false)],
    );
    let running = display(
        &bare,
        value(
            &now,
            3,
            false,
            Some((ChangeKind::Result, 0.2)),
            Role::Calculator,
        ),
        vec![(0.2, &total)],
        0.0,
    );
    assert_eq!((count(&running, glyph), count(&running, mask)), (5, 0));
    assert_eq!(running.again, None);
    let mut filled = Vec::new();
    for (age, pouring) in [(0.05, true), (0.2, true), (0.5, true), (0.9, false)] {
        let d = display(
            &bare,
            value(
                &now,
                4,
                false,
                Some((ChangeKind::Result, age)),
                Role::Calculator,
            ),
            vec![(age, &result)],
            0.0,
        );
        assert_eq!(d.again, pouring.then_some(frame), "{age}");
        assert_eq!((count(&d, glyph), count(&d, mask)), (10, 1), "{age}");
        let icing = image_of(&d, pics.icing).expect("the icing");
        assert_eq!((icing[2], icing[3]), (512.0, 56.0));
        // On the art grid, and from just above the digits' tops.
        let top = now.rect[1] + (1125.0 - 750.0) / 1400.0 * now.size;
        assert_eq!((icing[0] % 2.0, icing[1] % 2.0), (0.0, 0.0));
        assert!((icing[1] - (top - 2.0)).abs() <= 1.0, "{} {top}", icing[1]);
        filled.push(colours_of(&theme, pics.icing).len());
        let canvas = theme.api().canvas(pics.icing).unwrap();
        let (pixels, _) = canvas.pixels.as_chunks::<4>();
        let iced = pixels.iter().filter(|p| p[3] != 0).count();
        filled.push(iced);
    }
    // More and more of it is iced, and the sprinkles' colours come last.
    assert!(filled[1] < filled[3] && filled[3] < filled[5] && filled[5] == filled[7]);
    assert!(filled[0] < filled[6], "{filled:?}");
    // It stays glazed once the change has played out...
    let glazed_rest = display(
        &nothing_living,
        value(&now, 3, false, None, Role::Calculator),
        vec![],
        0.0,
    );
    assert_eq!((count(&glazed_rest, mask), glazed_rest.again), (1, None));
    // ...until the next digit is typed, or an error says otherwise.
    let plain = display(
        &nothing_living,
        value(
            &now,
            4,
            false,
            Some((ChangeKind::Typing, 1.0)),
            Role::Calculator,
        ),
        vec![],
        0.0,
    );
    assert_eq!(count(&plain, mask), 0);
    let wrong = display(
        &nothing_living,
        value(
            &now,
            5,
            true,
            Some((ChangeKind::Error, 0.1)),
            Role::Calculator,
        ),
        vec![],
        0.0,
    );
    assert_eq!((count(&wrong, mask), wrong.again), (0, Some(frame)));
    assert!(
        wrong
            .list
            .ops
            .iter()
            .any(|op| matches!(op, Op::Translate(x, _) if *x != 0.0))
    );
    // Without motion nothing says what a number is: every one is glazed.
    tick();
    let f = Frame {
        value: Some(value(&now, 6, false, None, Role::Calculator)),
        motion: false,
        ..Frame::sized(344.0, 122.0)
    };
    let still = ok(theme.draw(LayerKind::Display, &living, &f));
    assert_eq!((count(&still, mask), still.again), (1, None));
    // The doughnut jumps for a result after =, and comes down again.
    let cheer = |age: f64| {
        display(
            &nothing_living,
            value(
                &now,
                7,
                false,
                Some((ChangeKind::Result, age)),
                Role::Calculator,
            ),
            vec![(age, &result)],
            0.0,
        )
    };
    let standing = image_of(&glazed_rest, pics.buddy).unwrap()[1];
    let flying = image_of(&cheer(0.27), pics.buddy).unwrap()[1];
    assert_eq!(flying, standing - 26.0);
    let landed = cheer(0.9);
    assert_eq!(image_of(&landed, pics.buddy).unwrap()[1], standing);
    assert!(matches!(landed.again, Some(t) if t > 0.1 && t < 0.15));
    assert_eq!(cheer(1.05).again, None);
    // As the number grows towards it, it ducks behind the shelf; with no
    // room at all it is gone. It never stands under a digit.
    for n in 5..=18 {
        let long = digits(n, 344.0, (344.0 - 28.0) / (n as f32 * 0.42));
        let long = if n <= 10 {
            digits(n, 344.0, 74.667)
        } else {
            long
        };
        let d = display(
            &nothing_living,
            value(&long, 10 + n as u64, false, None, Role::Calculator),
            vec![],
            0.0,
        );
        match image_of(&d, pics.buddy) {
            Some(b) if b[0] + b[2] > long.rect[0] - 10.0 => {
                // Ducking: the part of it above the shelf is no taller
                // than what there is no room for.
                let Some(Op::PushClip { rect: clip, .. }) = d
                    .list
                    .ops
                    .iter()
                    .rfind(|op| matches!(op, Op::PushClip { .. }))
                else {
                    panic!()
                };
                let shows = clip[1] + clip[3] - b[1];
                let room = long.rect[0] - 10.0 - 60.0;
                assert!(shows <= 48.0 + room + 2.0, "{n}: {shows} {room}");
                assert!(shows > 8.0, "{n}");
            }
            Some(_) => assert!(n <= 8, "{n}"),
            None => assert!(n >= 9, "{n}"),
        }
    }
    // The doughnut switched off: no picture of it.
    let alone = ctx(&theme, false, "strawberry", &[("buddy", false)]);
    let d = display(
        &alone,
        value(&now, 40, false, None, Role::Calculator),
        vec![],
        0.0,
    );
    assert_eq!(image_of(&d, pics.buddy), None);
    assert_eq!(d.again, None);
    // A page's line (a converter's value, a date's result): plain digits
    // and nothing else, whatever happens to it.
    for role in [Role::Converter, Role::Date] {
        let d = display(
            &living,
            value(&now, 50, false, Some((ChangeKind::Result, 0.5)), role),
            vec![],
            0.0,
        );
        assert_eq!((d.list.ops.len(), d.again), (5, None), "{role:?}");
    }
    let mut times = Vec::new();
    for i in 0..200 {
        let v = value(
            &now,
            60,
            false,
            Some((ChangeKind::Result, 0.1 + i as f64 * 0.002)),
            Role::Calculator,
        );
        tick();
        let f = Frame {
            value: Some(v),
            events: vec![(0.1 + i as f64 * 0.002, &result)],
            ..Frame::sized(344.0, 122.0)
        };
        let t = Instant::now();
        ok(theme.draw(LayerKind::Display, &living, &f));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let display_ms = median(times);
    // The dearest frame a readout has: one that works the icing's picture
    // out anew, all 7168 pixels of it (a step of the pour: a dozen in a
    // result, each made once).
    let mut times = Vec::new();
    let mut versions = BTreeSet::new();
    for i in 0..120 {
        let age = (i % 12) as f64 * 0.034;
        let v = value(
            &now,
            70,
            false,
            Some((ChangeKind::Result, age)),
            Role::Calculator,
        );
        tick();
        let f = Frame {
            value: Some(v),
            events: vec![(age, &result)],
            ..Frame::sized(344.0, 122.0)
        };
        let t = Instant::now();
        ok(theme.draw(LayerKind::Display, &living, &f));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        versions.insert(theme.api().canvas(pics.icing).unwrap().version);
    }
    assert!(versions.len() >= 110, "{}", versions.len());
    let icing_ms = median(times);

    // ---- The flavours' tiles.
    let option = theme
        .def
        .options
        .iter()
        .find(|o| o.id == "flavour")
        .unwrap();
    for choice in FLAVOURS {
        let tile = theme.preview(option, choice, &living).unwrap();
        assert!(tile.ops.len() > 100 && tile.ops.len() < 400, "{choice}");
    }

    println!(
        "glazed, Luau per frame (median of 200): background with the loop {background_ms:.3} ms, \
         fountain {overlay_ms:.3} ms, keypad stacking with a hop and crumbs {keypad_ms:.3} ms, \
         display mid-result {display_ms:.3} ms, and {icing_ms:.3} ms for a frame that makes the icing anew"
    );
    // The limit is 8 ms a frame; a frame is a small part of it.
    let limit = Limits::default().frame.as_secs_f64() * 1000.0;
    for ms in [background_ms, overlay_ms, keypad_ms, display_ms, icing_ms] {
        assert!(ms < limit / 4.0, "{ms}");
    }
    assert!(
        theme
            .hooks()
            .iter()
            .all(|(_, strikes, off)| *strikes == 0 && !off)
    );
    let notes = theme.take_draw_notes();
    assert!(notes.is_empty(), "{notes:?}");
    assert!(theme.take_log().is_empty());
}

// ---- What its layers paint reads ---------------------------------------------------------

/// The style of a flavour at full opacity.
fn style_of(theme: &mut Theme, c: &Ctx) -> Style {
    theme.style(c).unwrap().style.clone()
}

/// Readability rule 3 for what the layers paint, which the style's own
/// tables can't see. In every flavour, light and dark:
///
/// - the header's title and buttons on both of the awning's stripes
///   (the background layer draws them under a header with no fill);
/// - every colour a glazed number is painted in (the dough, the icing,
///   its shine and rim, each sprinkle) on the display case: large text,
///   and digits under 28 px, which are not, wear no icing;
/// - an error's colour there; a page's line in the window's own colour;
/// - the equals key's icing leaves the key's own colour under its sign.
#[test]
fn what_glazed_paints_reads() {
    let mut theme = glazed();
    let tick = clock(&theme);
    let pics = pictures(&theme);
    let mut worst_awning = f32::INFINITY;
    let mut worst_glaze = f32::INFINITY;
    // (A value's glyphs and size are told anew when its version changes:
    // every number drawn here has one of its own.)
    let mut version = 0;
    let mut next = || {
        version += 1;
        version
    };
    for flavour in FLAVOURS {
        for dark in [false, true] {
            let name = format!("{flavour} dark={dark}");
            let c = ctx(&theme, dark, flavour, &[]);
            let style = style_of(&mut theme, &c);

            // The awning: every rectangle the background layer paints
            // across the header, under the title and the buttons.
            let drawn = ok(theme.draw(LayerKind::Background, &c, &Frame::sized(760.0, 640.0)));
            let stripes: BTreeSet<[u8; 4]> = drawn
                .list
                .ops
                .iter()
                .filter_map(|op| match op {
                    Op::Rect { rect, paint } if rect[1] == 0.0 && rect[3] == AWNING => {
                        let c = solid(paint);
                        assert_eq!(c[3], 1.0);
                        Some(c.map(|v| (v * 255.0).round() as u8))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(stripes.len(), 2, "{name}");
            let Some(Value::Text(title)) = style.get("header.title") else {
                panic!()
            };
            let button = style.control("header.button").unwrap().finished(&[]);
            let controls = style.control("header.controls").unwrap().finished(&[]);
            for stripe in &stripes {
                for (what, ink) in [
                    ("title", title.color),
                    ("button", button.icon_color),
                    ("controls", controls.icon_color),
                ] {
                    let got = on(ink, rgba(*stripe));
                    worst_awning = worst_awning.min(got);
                    assert!(got >= TEXT_CONTRAST, "{name}: {what} on a stripe {got:.2}");
                }
            }

            // A glazed number, in a frame without motion (every number
            // is then glazed), at the sizes a readout has, down to the
            // least at which digits are iced.
            let case = style.fill("display.background").samples_over([0.0; 4])[0];
            assert_eq!(case[3], 1.0);
            for size in [74.667, 56.0, 28.0] {
                tick();
                let now = digits(5, 344.0, size);
                let f = Frame {
                    value: Some(value(&now, next(), false, None, Role::Calculator)),
                    motion: false,
                    ..Frame::sized(344.0, 122.0)
                };
                let d = ok(theme.draw(LayerKind::Display, &c, &f));
                // Outside the mask the digits are painted once, in the
                // dough's colour: the window's own text colour.
                let at = d
                    .list
                    .ops
                    .iter()
                    .position(|op| matches!(op, Op::PushMask(_)))
                    .unwrap();
                for op in &d.list.ops[..at] {
                    if let Op::Glyph { paint, .. } = op {
                        let got = on(solid(paint), case);
                        assert_eq!(solid(paint), style.color("window.color"));
                        assert!(got >= TEXT_CONTRAST, "{name}: dough {got:.2}");
                    }
                }
                // What shows through them: the icing's picture, every
                // pixel of it.
                let colours = colours_of(&theme, pics.icing);
                assert!(colours.len() >= 3, "{name}: {colours:?}");
                for p in colours {
                    assert_eq!(p[3], 255);
                    let got = on(rgba(p), case);
                    worst_glaze = worst_glaze.min(got);
                    assert!(got >= UI_CONTRAST, "{name} at {size}: icing {p:?} {got:.2}");
                }
            }
            // Smaller digits are not large text, and 3 is not enough for
            // them: they wear no icing, and are the text colour alone.
            for size in [27.0, 20.0] {
                tick();
                let small = digits(5, 344.0, size);
                let f = Frame {
                    value: Some(value(&small, next(), false, None, Role::Calculator)),
                    motion: false,
                    ..Frame::sized(344.0, 122.0)
                };
                let d = ok(theme.draw(LayerKind::Display, &c, &f));
                for op in &d.list.ops {
                    assert!(!matches!(op, Op::PushMask(_)), "{name} at {size}");
                    if let Op::Glyph { paint, .. } = op {
                        assert_eq!(solid(paint), style.color("window.color"));
                    }
                }
            }
            // An error, and a page's line.
            tick();
            let now = digits(5, 344.0, 37.3);
            let f = Frame {
                value: Some(value(&now, next(), true, None, Role::Calculator)),
                ..Frame::sized(344.0, 122.0)
            };
            let d = ok(theme.draw(LayerKind::Display, &c, &f));
            assert!(!d.list.ops.iter().any(|op| matches!(op, Op::PushMask(_))));
            for op in &d.list.ops {
                if let Op::Glyph { paint, .. } = op {
                    let got = on(solid(paint), case);
                    assert!(got >= TEXT_CONTRAST, "{name}: error {got:.2}");
                }
            }
            let plain = style.fill("plain.background").samples_over([0.0; 4])[0];
            let f = Frame {
                value: Some(value(&now, next(), false, None, Role::Converter)),
                ..Frame::sized(344.0, 60.0)
            };
            for op in &ok(theme.draw(LayerKind::Display, &c, &f)).list.ops {
                let Op::Glyph { paint, .. } = op else {
                    panic!("{op:?}")
                };
                assert!(on(solid(paint), plain) >= TEXT_CONTRAST, "{name}: a line");
            }
        }
    }
    println!(
        "glazed: the least contrast on the awning {worst_awning:.2}, of a glazed number {worst_glaze:.2}"
    );
}

/// What stands behind the page is the wall's still and nothing brighter
/// or darker than the style says: `window.background` is the wall's own
/// middle colour, so what the style's tables work out against it holds
/// for the picture. And the loop's sprinkles are six colours on nothing.
#[test]
fn the_wall_is_what_the_style_says() {
    let mut theme = glazed();
    let pics = pictures(&theme);
    for dark in [false, true] {
        let c = ctx(&theme, dark, "strawberry", &[]);
        let style = style_of(&mut theme, &c);
        let wall = style.fill("window.background").samples_over([0.0; 4])[0];
        let tiles = colours_of(&theme, pics.walls[dark as usize]);
        assert_eq!(tiles.len(), 4, "two tiles, the grout and the glint");
        for p in tiles {
            assert_eq!(p[3], 255);
            let got = on(rgba(p), wall);
            assert!(
                got < 1.3,
                "dark={dark}: a tile at {got:.2} from the wall's colour"
            );
        }
    }
    // The hem's masks are white and nothing else.
    for mask in pics.hem {
        assert_eq!(
            colours_of(&theme, mask),
            BTreeSet::from([[255, 255, 255, 255]])
        );
    }
    let Some(PictureKind::Animation(_, timeline)) = theme.api().kind(pics.falling) else {
        panic!()
    };
    // 128 frames, 0.12 s each: the loop is back where it began after
    // fifteen seconds.
    assert_eq!(timeline.frames(), 128);
    assert_eq!((timeline.width, timeline.height), (128, 128));
    assert!((timeline.duration - 15.36).abs() < 1e-6);
}
