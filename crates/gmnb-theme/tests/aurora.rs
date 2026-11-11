//! Aurora, the built-in theme, in the real engine: what its background
//! draws, standing, drifting and see-through; what a frame costs at any
//! window size; and that every pixel of it is a colour the readability
//! tests have been through (`tests/css.rs`,
//! `auroras_texts_read_over_every_colour_of_the_sky`).

mod common;

use gmnb_theme::draw::{DisplayList, MaskMode, Op, Rect};
use gmnb_theme::events::{Cause, Event, EventKind, KeyKind, Page, Source as PressSource, Via};
use gmnb_theme::layer::{Drawn, Failure, Frame, LayerKind};
use gmnb_theme::theme::{self, Source, Theme};
use gmnb_theme::types::*;
use gmnb_theme::{Ctx, OptionValue};

const PALETTES: [&str; 7] = [
    "aurora", "ember", "abyss", "orchard", "graphite", "system", "custom",
];

/// Aurora: GMNB's own copy, or for looking at a changed one without
/// building, the folder `AURORA_DIR` names.
fn aurora() -> Theme {
    let source = match std::env::var_os("AURORA_DIR") {
        Some(dir) => Source::folder("aurora", std::path::Path::new(&dir)),
        None => Source::built_in("aurora").unwrap(),
    };
    let theme = Theme::load(source, common::limits()).unwrap_or_else(|e| panic!("{e}"));
    assert!(theme.notes.is_empty(), "{:#?}", theme.notes);
    theme
}

fn text(s: &str) -> OptionValue {
    OptionValue::Text(s.into())
}

/// A `ctx` with the theme's options at their defaults, but for `options`.
fn ctx(theme: &Theme, dark: bool, options: &[(&str, OptionValue)]) -> Ctx {
    let saved = options
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
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
        Err(Failure::Off) => panic!("the hook is off"),
    }
}

/// Sets the theme's clock (what its field's allowance of thirty passes a
/// second runs on) to `now`, and draws the background.
fn draw_at(theme: &Theme, c: &Ctx, f: &Frame, now: f64) -> Drawn {
    theme.api().set_clock(std::rc::Rc::new(move || 100.0 + now));
    ok(theme.draw(LayerKind::Background, c, f))
}

/// A key pressed by the pointer at (x, y).
fn press(id: u32, kind: KeyKind, x: f32, y: f32) -> Event {
    let mut e = Event::at(
        EventKind::Press,
        Page::Calculator,
        [x - 40.0, y - 25.0, 80.0, 50.0],
    );
    e.source = Some(PressSource::Key);
    e.via = Some(Via::Pointer);
    e.key = Some((Some(id), kind));
    e.x = x;
    e.y = y;
    e
}

/// A result or an error in a readout across the top of a `width` window.
fn outcome(kind: EventKind, width: f32) -> Event {
    let mut e = Event::at(kind, Page::Calculator, [8.0, 50.0, width - 16.0, 116.0]);
    e.cause = Some(Cause::Equals);
    e.via = Some(Via::Pointer);
    e
}

/// The user chose another palette.
fn chosen(palette: &str, w: f32, h: f32) -> Event {
    let mut e = Event::at(EventKind::Option, Page::Settings, [0.0, 0.0, w, h]);
    e.option = Some(("palette".into(), text(palette)));
    e
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

// ---- A plain painter for what the background draws ---------------------------------

/// A picture being painted: premultiplied red, green, blue and opacity.
struct Layer {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

impl Layer {
    fn new(w: usize, h: usize) -> Layer {
        Layer {
            w,
            h,
            px: vec![[0.0; 4]; w * h],
        }
    }

    /// `colour` (not premultiplied) over the pixel at (x, y).
    fn put(&mut self, x: usize, y: usize, colour: Rgba) {
        let p = &mut self.px[y * self.w + x];
        let a = colour[3];
        for c in 0..3 {
            p[c] = colour[c] * a + p[c] * (1.0 - a);
        }
        p[3] = a + p[3] * (1.0 - a);
    }

    fn rows(&self, rect: Rect) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let x0 = rect[0].max(0.0).round() as usize;
        let y0 = rect[1].max(0.0).round() as usize;
        let x1 = ((rect[0] + rect[2]).round().max(0.0) as usize).min(self.w);
        let y1 = ((rect[1] + rect[3]).round().max(0.0) as usize).min(self.h);
        (x0.min(x1)..x1, y0.min(y1)..y1)
    }
}

/// A paint's colour at the middle of pixel (x, y) of a rectangle.
fn colour_at(paint: &Paint, x: usize, y: usize) -> Rgba {
    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
    match paint {
        Paint::Color(c) => *c,
        Paint::Gradient(g) => match g.kind {
            GradientKind::Linear => {
                let (from, to) = (g.pixels.from.unwrap(), g.pixels.to.unwrap());
                let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
                let len = dx * dx + dy * dy;
                let t = ((px - from[0]) * dx + (py - from[1]) * dy) / len.max(1e-9);
                g.at_stop(t.clamp(0.0, 1.0))
            }
            GradientKind::Radial => {
                let c = g.pixels.center.unwrap();
                let rx = g.pixels.radius.unwrap();
                let ry = g.pixels.radius_y.unwrap_or(rx);
                let (dx, dy) = ((px - c[0]) / rx.max(1e-9), (py - c[1]) / ry.max(1e-9));
                g.at_stop((dx * dx + dy * dy).sqrt().clamp(0.0, 1.0))
            }
            GradientKind::Conic => panic!("the background draws no conic gradient"),
        },
    }
}

/// What the background's display list puts on a `w × h` window, a pixel
/// at a time, premultiplied: its rectangles, its field and what it masks,
/// over `under` if there is something under it (the sky). (The grain is
/// left out: a twentieth of white or black on a pixel here and there.)
fn painted(list: &DisplayList, under: Option<&Paint>, w: usize, h: usize) -> Vec<[f32; 4]> {
    // The picture under, the mask once it is drawn, and what is being
    // drawn now.
    let mut stack: Vec<(Layer, Option<Layer>)> = Vec::new();
    let mut now = Layer::new(w, h);
    if let Some(paint) = under {
        for y in 0..h {
            for x in 0..w {
                now.put(x, y, colour_at(paint, x, y));
            }
        }
    }
    // The grain's own group, when it is thinned: left out with it.
    let mut grain = 0;
    for op in &list.ops {
        match op {
            Op::Rect { rect, paint } => {
                let (xs, ys) = now.rows(*rect);
                for y in ys {
                    for x in xs.clone() {
                        now.put(x, y, colour_at(paint, x, y));
                    }
                }
            }
            Op::Field { picture, rect } => {
                let (xs, ys) = now.rows(*rect);
                let (pw, ph) = (picture.width as usize, picture.height as usize);
                let texel = |i: usize, j: usize| -> [f32; 4] {
                    let at = (j.min(ph - 1) * pw + i.min(pw - 1)) * 4;
                    let p = &picture.pixels[at..at + 4];
                    [0, 1, 2, 3].map(|c| p[c] as f32 / 255.0)
                };
                for y in ys {
                    let fy = ((y as f32 + 0.5 - rect[1]) / rect[3] * ph as f32 - 0.5).max(0.0);
                    let (j, ty) = (fy as usize, fy.fract());
                    for x in xs.clone() {
                        let fx = ((x as f32 + 0.5 - rect[0]) / rect[2] * pw as f32 - 0.5).max(0.0);
                        let (i, tx) = (fx as usize, fx.fract());
                        let (a, b, c, d) = (
                            texel(i, j),
                            texel(i + 1, j),
                            texel(i, j + 1),
                            texel(i + 1, j + 1),
                        );
                        let p = [0, 1, 2, 3].map(|k| {
                            let top = a[k] + (b[k] - a[k]) * tx;
                            let bottom = c[k] + (d[k] - c[k]) * tx;
                            top + (bottom - top) * ty
                        });
                        let under = &mut now.px[y * w + x];
                        for k in 0..4 {
                            under[k] = p[k] + under[k] * (1.0 - p[3]);
                        }
                    }
                }
            }
            Op::PushMask(MaskMode::Alpha) => {
                stack.push((std::mem::replace(&mut now, Layer::new(w, h)), None));
            }
            Op::PushOpacity(_) => grain += 1,
            Op::Pop if grain > 0 => grain -= 1,
            Op::Pop => {
                let (under, mask) = stack.pop().expect("a pop with nothing pushed");
                match mask {
                    // The mask is drawn: what shows through it comes next.
                    None => {
                        let mask = std::mem::replace(&mut now, Layer::new(w, h));
                        stack.push((under, Some(mask)));
                    }
                    Some(mask) => {
                        let shown = std::mem::replace(&mut now, under);
                        for ((p, s), m) in now.px.iter_mut().zip(&shown.px).zip(&mask.px) {
                            let a = s[3] * m[3];
                            for c in 0..3 {
                                p[c] = s[c] * m[3] + p[c] * (1.0 - a);
                            }
                            p[3] = a + p[3] * (1.0 - a);
                        }
                    }
                }
            }
            Op::Tile { .. } => {}
            other => panic!("the background draws no {other:?}"),
        }
    }
    assert!(stack.is_empty() && grain == 0);
    now.px
}

// ---- What the background is made of ---------------------------------------------------

/// What one frame of the background is made of, each part checked to be
/// what the readability tests take it for (`tests/css.rs`,
/// `auroras_sky_colours`): the stars, small squares of one colour, more
/// or less opaque; one mask, holding the aurora's field and the rays of
/// whatever just happened, all of it white and nothing but; and showing
/// through the mask the lights, a gradient of six colours straight down
/// the window, all as strong as each other. Then the grain. Nothing else:
/// the sky under it all is the window's own background.
struct Made {
    /// The lights, and how strong they are drawn (1 on an opaque window).
    lights: Gradient,
    strength: f32,
    /// The stars' colour, and how opaque the most opaque of them is.
    star: Option<Rgba>,
    stars: usize,
    /// The field's picture and where it is drawn.
    field: (std::rc::Rc<gmnb_theme::field::FieldPicture>, Rect),
    /// How many rays of light are in the mask with it.
    rays: usize,
    /// How much of the grain is drawn (1 on an opaque window).
    grain: f32,
}

fn made_of(list: &DisplayList, w: f32, h: f32, what: &str) -> Made {
    let whole = [0.0, 0.0, w, h];
    let mut ops = list.ops.iter().peekable();
    let mut star = None::<Rgba>;
    let mut stars = 0;
    while let Some(Op::Rect {
        rect,
        paint: Paint::Color(c),
    }) = ops.peek()
    {
        // A speck: under three pixels, and in the window or on its edge.
        assert!(
            rect[2] == rect[3] && (1.0..=2.8).contains(&rect[2]),
            "{what}: a star of {rect:?}"
        );
        assert!(
            rect[0] > -3.0 && rect[1] > -3.0 && rect[0] < w + 3.0 && rect[1] < h + 3.0,
            "{what}: a star at {rect:?}"
        );
        assert!(
            star.is_none_or(|s| s[..3] == c[..3]),
            "{what}: stars of two colours"
        );
        assert!((0.0..=1.0).contains(&c[3]));
        if star.is_none_or(|s| c[3] > s[3]) {
            star = Some(*c);
        }
        stars += 1;
        ops.next();
    }
    assert!(stars <= 160, "{what}: {stars} stars");
    assert!(
        matches!(ops.next(), Some(Op::PushMask(MaskMode::Alpha))),
        "{what}: the mask after the stars"
    );
    let field = match ops.next() {
        Some(Op::Field { picture, rect }) => (picture.clone(), *rect),
        other => panic!("{what}: the field first in the mask, not {other:?}"),
    };
    // The field's picture is white, more or less opaque: premultiplied,
    // its four parts are one number.
    assert!(
        field
            .0
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[0] == p[3] && p[1] == p[3] && p[2] == p[3]),
        "{what}: the field's picture is not white"
    );
    assert!(
        field.1[0] == 0.0 && field.1[1] == 0.0 && field.1[2] == w && field.1[3] <= h,
        "{what}: the field at {:?}",
        field.1
    );
    let mut rays = 0;
    loop {
        match ops.next() {
            Some(Op::Pop) => break,
            Some(Op::Rect {
                paint: Paint::Gradient(g),
                ..
            }) if g.kind == GradientKind::Radial
                && g.colors.iter().all(|c| c[..3] == [1.0, 1.0, 1.0]) =>
            {
                rays += 1
            }
            other => panic!("{what}: in the mask, {other:?}"),
        }
    }
    let lights = match ops.next() {
        Some(Op::Rect {
            rect,
            paint: Paint::Gradient(g),
        }) if *rect == whole
            && g.colors.len() == 6
            && g.kind == GradientKind::Linear
            && g.pixels.from == Some([0.0, 0.0])
            && g.pixels.to == Some([0.0, h]) =>
        {
            g.clone()
        }
        other => panic!("{what}: the lights through the mask, not {other:?}"),
    };
    let strength = lights.colors[0][3];
    assert!(
        lights.colors.iter().all(|c| c[3] == strength),
        "{what}: lights of different strengths"
    );
    assert!(matches!(ops.next(), Some(Op::Pop)), "{what}");
    let grain = match ops.next() {
        Some(Op::PushOpacity(a)) => {
            let a = *a;
            assert!(
                matches!(ops.next(), Some(Op::Tile { rect, .. }) if *rect == whole),
                "{what}: the grain"
            );
            assert!(matches!(ops.next(), Some(Op::Pop)), "{what}");
            a
        }
        Some(Op::Tile { rect, .. }) if *rect == whole => 1.0,
        other => panic!("{what}: the grain last, not {other:?}"),
    };
    assert!(ops.next().is_none(), "{what}: something after the grain");
    Made {
        lights,
        strength,
        star,
        stars,
        field,
        rays,
        grain,
    }
}

/// Every frame of the background, in every palette, light and dark,
/// standing, drifting, and with every kind of pulse alive at once, on an
/// opaque window and a see-through one, at two window sizes, is made of
/// what `Made` says and nothing else. And so every pixel it puts on the
/// window, painted here plainly, is one of the palette's lights at that
/// height, more or less of it and never more than its strength, or a
/// star: over the sky, the colours the readability tests put under every
/// text.
#[test]
fn the_background_is_lights_and_stars_and_nothing_else() {
    for (w, h) in [(760.0f32, 640.0f32), (360.0, 600.0)] {
        let presses: Vec<Event> = (0..6)
            .map(|i| {
                press(
                    i,
                    if i == 5 {
                        KeyKind::Equals
                    } else {
                        KeyKind::Number
                    },
                    w * (0.15 + 0.14 * i as f32),
                    h * (0.5 + 0.08 * i as f32),
                )
            })
            .collect();
        let result = outcome(EventKind::Result, w);
        let error = outcome(EventKind::Error, w);
        for palette in PALETTES {
            for (dark, opacity) in [(true, 1.0), (false, 1.0), (true, 0.4), (false, 0.15)] {
                let theme = aurora();
                let mut c = ctx(&theme, dark, &[("palette", text(palette))]);
                c.opacity = opacity;
                let option = chosen(palette, w, h);
                let what = |frame: &str| format!("{palette} dark={dark} {opacity} {w}x{h} {frame}");
                // Standing.
                let still = Frame {
                    motion: false,
                    ..Frame::sized(w, h)
                };
                let d = draw_at(&theme, &c, &still, 0.0);
                let standing = made_of(&d.list, w, h, &what("standing"));
                assert_eq!(standing.rays, 0);
                assert_eq!(standing.stars > 0, dark, "{}", what("standing"));
                assert_eq!(standing.strength == 1.0, opacity == 1.0);
                // Drifting for two seconds, with every kind of pulse.
                let mut most_rays = 0;
                for i in 0..30 {
                    let t = i as f64 / 15.0;
                    let mut events: Vec<(f64, &Event)> = Vec::new();
                    if i >= 6 {
                        let age = t - 0.4;
                        for (n, e) in presses.iter().enumerate() {
                            if age >= n as f64 * 0.07 {
                                events.push((age - n as f64 * 0.07, e));
                            }
                        }
                        events.push((age, &result));
                        events.push((age * 0.5, &error));
                        events.push((age, &option));
                    }
                    let f = Frame {
                        time: t,
                        dt: if i == 0 { 0.0 } else { 1.0 / 15.0 },
                        events,
                        ..Frame::sized(w, h)
                    };
                    let d = draw_at(&theme, &c, &f, t);
                    let name = what(&format!("frame {i}"));
                    let drawn = made_of(&d.list, w, h, &name);
                    most_rays = most_rays.max(drawn.rays);
                    // The same lights and stars in every frame.
                    assert_eq!(drawn.lights, standing.lights, "{name}");
                    assert_eq!(
                        (drawn.star, drawn.stars, drawn.grain),
                        (standing.star, standing.stars, standing.grain),
                        "{name}"
                    );
                    // (Painting is slow: two frames, one of them with
                    // every pulse up; in the larger window, of one palette.)
                    if (i != 3 && i != 17) || (w > 400.0 && palette != "aurora") {
                        continue;
                    }
                    // Painted without its stars, each pixel is the light at
                    // its height, as much of it as the mask lets through.
                    let mut bare = d.list.clone();
                    bare.ops.retain(|op| {
                        !matches!(
                            op,
                            Op::Rect {
                                paint: Paint::Color(_),
                                ..
                            }
                        )
                    });
                    let px = painted(&bare, None, w as usize, h as usize);
                    for y in 0..h as usize {
                        let light = drawn.lights.at_stop((y as f32 + 0.5) / h);
                        for x in 0..w as usize {
                            let p = px[y * w as usize + x];
                            assert!(
                                p[3] <= drawn.strength + 1e-4
                                    && (0..3).all(|c| (p[c] - light[c] * p[3]).abs() < 2e-3),
                                "{name}: the pixel at {x}, {y} is {p:?}, the light there {light:?}"
                            );
                        }
                    }
                }
                // (Six keys, the readout twice over, four rays of a new
                // palette: some of them are two lights.)
                assert!(most_rays >= 12, "{}: {most_rays} rays", what("pulses"));
                let notes = theme.take_draw_notes();
                assert!(notes.is_empty(), "{}: {notes:?}", what("notes"));
            }
        }
    }
}

/// On a see-through window the sky clears and the aurora stays. The layer
/// is not faded by the app (`fade = false`): it thins itself. Its lights
/// are all there on an opaque window and seven tenths or more of them at
/// any opacity, so that they hang over the desktop; its stars and its
/// grain belong to the sky and go with it, the stars sooner. And nothing
/// jumps on the way: a hundredth of opacity changes each by a hundredth
/// or less.
#[test]
fn on_a_see_through_window_the_sky_clears_and_the_lights_stay() {
    let (w, h) = (760.0, 640.0);
    let theme = aurora();
    assert!(theme.def.layers["background"].unfaded);
    let still = Frame {
        motion: false,
        ..Frame::sized(w, h)
    };
    for dark in [true, false] {
        let mut c = ctx(&theme, dark, &[]);
        let opaque = made_of(&draw_at(&theme, &c, &still, 0.0).list, w, h, "opaque");
        assert_eq!((opaque.strength, opaque.grain), (1.0, 1.0));
        let most = |m: &Made| m.star.map_or(0.0, |s| s[3]);
        assert_eq!(most(&opaque) > 0.9, dark);
        let mut last = (1.0f32, most(&opaque), 1.0f32);
        // (Down to a tenth: the setting goes no lower.)
        for step in (10..100).rev() {
            let opacity = step as f32 / 100.0;
            c.opacity = opacity;
            let d = draw_at(&theme, &c, &still, 0.0);
            let m = made_of(&d.list, w, h, &format!("dark={dark} {opacity}"));
            // The same aurora at every opacity: the field's picture is the
            // one already made, and the lights are the same colours.
            assert_eq!(m.field.0.serial, opaque.field.0.serial);
            for (a, b) in m.lights.colors.iter().zip(&opaque.lights.colors) {
                assert_eq!(a[..3], b[..3]);
            }
            let now = (m.strength, most(&m), m.grain);
            assert!((0.7..1.0).contains(&m.strength), "{opacity}: {now:?}");
            assert!(
                m.grain <= opacity + 1e-6 && m.star.is_none_or(|s| s[3] <= opacity),
                "{opacity}: {now:?}"
            );
            for (was, is) in [(last.0, now.0), (last.1, now.1), (last.2, now.2)] {
                assert!(
                    is <= was + 1e-6 && was - is <= 0.0201,
                    "dark={dark} {opacity}: {last:?} to {now:?}"
                );
            }
            last = now;
        }
        // All but clear: the lights are still there, and the stars are out.
        assert!(
            last.0 >= 0.7 && last.1 < 0.011 && last.2 < 0.101,
            "{last:?}"
        );
    }
}

/// What the background asks for, and when its picture changes: standing
/// when the option is off or motion isn't allowed; fifteen pictures a
/// second while it drifts, however often it is drawn; and never a field
/// coloured for nothing.
#[test]
fn the_aurora_drifts_while_it_may_and_stands_when_it_may_not() {
    let (w, h) = (760.0, 640.0);
    let theme = aurora();
    let living = ctx(&theme, true, &[]);
    let standing = ctx(&theme, true, &[("living", OptionValue::Bool(false))]);
    let serial = |d: &Drawn| made_of(&d.list, w, h, "a frame").field.0.serial;

    // The option off: one picture, and nothing asked for.
    let f = Frame::sized(w, h);
    let first = draw_at(&theme, &standing, &f, 0.0);
    assert_eq!(first.again, None);
    let f = Frame {
        time: 5.0,
        dt: 0.25,
        ..Frame::sized(w, h)
    };
    let later = draw_at(&theme, &standing, &f, 5.0);
    assert_eq!((later.again, later.retry), (None, None));
    assert_eq!(serial(&later), serial(&first));
    // A pulse plays thirty times a second, over the picture that stands.
    let key = press(5, KeyKind::Number, 170.0, 380.0);
    let f = Frame {
        time: 5.1,
        dt: 0.1,
        events: vec![(0.1, &key)],
        ..Frame::sized(w, h)
    };
    let pulsing = draw_at(&theme, &standing, &f, 5.1);
    assert_eq!(pulsing.again, Some(0.032));
    // (Under a thirtieth: the app waits for it on the screen's own
    // frames, not on a timer that is out of step with them.)
    assert_eq!(
        gmnb_theme::sched::Clock::new().end(
            0.0,
            pulsing.again,
            gmnb_theme::sched::Conditions {
                mapped: true,
                active: true,
                motion: true,
                settled: false,
            }
        ),
        gmnb_theme::sched::Wait::Frame
    );
    assert_eq!(serial(&pulsing), serial(&first));

    // Drifting, drawn sixty times a second for ten seconds (as while
    // pulses play): a new picture each fifteenth of a second and no more,
    // none refused, and between pulses a fifteenth of a second asked for.
    let mut pictures = std::collections::HashSet::new();
    let mut last = serial(&first);
    let mut changes = Vec::new();
    for i in 0..600 {
        let t = 10.0 + i as f64 / 60.0;
        let f = Frame {
            time: t,
            dt: 1.0 / 60.0,
            ..Frame::sized(w, h)
        };
        let d = draw_at(&theme, &living, &f, t);
        assert_eq!(d.again, Some(0.066), "{i}");
        assert_eq!(d.retry, None, "{i}");
        let s = serial(&d);
        if s != last {
            changes.push(i);
            last = s;
        }
        pictures.insert(s);
    }
    assert!(
        (140..=151).contains(&pictures.len()),
        "{} pictures in ten seconds",
        pictures.len()
    );
    // (Never two frames running: a fifteenth is four frames of sixty.)
    assert!(changes.windows(2).all(|p| p[1] - p[0] >= 3), "{changes:?}");

    // Motion not allowed: the picture it had, and it stands.
    let quiet = Frame {
        motion: false,
        ..Frame::sized(w, h)
    };
    let a = draw_at(&theme, &living, &quiet, 30.0);
    let b = draw_at(&theme, &living, &quiet, 31.0);
    assert_eq!(serial(&a), serial(&b));
    assert_eq!(b.retry, None);

    // A theme just loaded, standing still: there is an aurora in its
    // picture. A good part of the field is lit, and a good part is dark.
    let again = aurora();
    let c = ctx(&again, true, &[]);
    let p = draw_at(&again, &c, &quiet, 0.0);
    let p = made_of(&p.list, w, h, "still").field.0;
    let lit = p
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| px[3] > 96)
        .count();
    let share = lit as f64 / (p.width * p.height) as f64;
    assert!((0.08..0.6).contains(&share), "{share}");
}

/// The background at every size a window may have, from the least to a
/// 4K screen and two shapes no window has: the field stays within its
/// caps, a frame within its time (the first, which works out a whole
/// snapshot; those that drift; and frames with eight rays up), and
/// nothing is refused.
#[test]
fn the_background_stays_within_its_limits_at_any_size() {
    let limit = common::limits().frame.as_secs_f64() * 1000.0;
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
        let mut theme = aurora();
        let c = ctx(&theme, true, &[]);
        // (As in the app: `style` has worked the palette out.)
        theme.style(&c).unwrap();
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
        let option = chosen("ember", w, h);
        let (mut first, mut worst, mut sum) = (0.0, 0.0f64, 0.0);
        let frames = 60;
        for i in 0..frames {
            let t = i as f64 / 15.0;
            let mut events: Vec<(f64, &Event)> = Vec::new();
            if (15..32).contains(&i) {
                for (n, e) in presses.iter().enumerate() {
                    let age = t - 1.0 - n as f64 * 0.05;
                    if age >= 0.0 {
                        events.push((age, e));
                    }
                }
                events.push((t - 1.0, &result));
                events.push((t - 1.0, &option));
            }
            let f = Frame {
                time: t,
                dt: if i == 0 { 0.0 } else { 1.0 / 15.0 },
                events,
                ..Frame::sized(w, h)
            };
            let started = cpu_ms();
            let d = draw_at(&theme, &c, &f, t);
            let ms = cpu_ms() - started;
            assert_eq!(d.retry, None, "{w}x{h} at {t}");
            let drawn = made_of(&d.list, w, h, &format!("{w}x{h} at {t}"));
            // Never more pixels to colour than a 760 × 640 window's aurora
            // has at two thirds size, and a little over for the rounding.
            let picture = &drawn.field.0;
            assert!(
                picture.width * picture.height <= 172_000,
                "{w}x{h}: {} x {}",
                picture.width,
                picture.height
            );
            if i == 0 {
                first = ms;
            } else {
                worst = worst.max(ms);
                sum += ms;
            }
        }
        report.push(format!(
            "{w}x{h}: the first frame {first:.2} ms, then mean {:.2} ms, worst {worst:.2} ms",
            sum / (frames - 1) as f64
        ));
        // The limit is 8 ms, and a frame is held to three quarters of it
        // in an optimised build (an unoptimised one: to all of it; and
        // without `GMNB_REAL_LIMITS`, of the four times it has).
        let most = if cfg!(debug_assertions) {
            limit
        } else {
            limit * 0.75
        };
        assert!(
            first.max(worst) < most,
            "{w}x{h}: a frame took {:.2} ms",
            first.max(worst)
        );
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
        // Its busiest frames cost well within what a frame may to draw:
        // under a third of it in a window of any real shape. (In a strip
        // a few rays cover everything, and a ray is charged by what it
        // covers: under a half.)
        let busiest = theme.api().busiest_frame();
        report.push(format!(
            "  the busiest frame cost {:.1}% of what one may",
            busiest * 100.0
        ));
        let strip = w > h * 4.0 || h > w * 4.0;
        let most = if strip { 0.5 } else { 0.3 };
        assert!(busiest > 0.0 && busiest < most, "{w}x{h}: {busiest}");
    }
    println!(
        "aurora, the background drifting, with eight rays, a result and a new palette, by \
         window size (processor time; the limit is {limit} ms):\n  {}",
        report.join("\n  ")
    );
}

/// What a frame of the background costs in a 760 × 640 window: to work
/// out (processor time, of the hook's 8 ms) and to draw (its share of
/// what a frame may cost), standing, drifting, see-through, and at its
/// busiest.
#[test]
fn what_a_frame_costs() {
    let (w, h) = (760.0, 640.0);
    let mut theme = aurora();
    let mut c = ctx(&theme, true, &[]);
    let share = |theme: &Theme| theme.api().frame_cost() * 100.0;
    // A frame drawn before the palette's `style` has ever run works the
    // palette out as well (the app runs `style` first: this is the worst
    // a frame can be handed).
    let started = cpu_ms();
    draw_at(&theme, &c, &Frame::sized(300.0, 400.0), 0.0);
    let cold = cpu_ms() - started;
    theme.style(&c).unwrap();
    // The first frame at a window size works out a whole snapshot.
    let started = cpu_ms();
    draw_at(&theme, &c, &Frame::sized(w, h), 0.0);
    let first = cpu_ms() - started;
    let standing = share(&theme);
    // Drifting: a quarter of a snapshot in four frames of five.
    let mut times = Vec::new();
    for i in 1..=300 {
        let t = i as f64 / 15.0;
        let f = Frame {
            time: t,
            dt: 1.0 / 15.0,
            ..Frame::sized(w, h)
        };
        let started = cpu_ms();
        draw_at(&theme, &c, &f, t);
        times.push(cpu_ms() - started);
    }
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let worst = times.iter().copied().fold(0.0, f64::max);
    // Its dearest frames: a result just after the equals key, and a new
    // palette's four rays at their tallest.
    let equals = press(121, KeyKind::Equals, 390.0, 600.0);
    let result = outcome(EventKind::Result, w);
    let option = chosen("ember", w, h);
    let mut dearest = (0.0f64, 0.0f64);
    for i in 0..40 {
        let age = 0.03 * i as f64;
        for (which, events) in [vec![(age, &equals), (age, &result)], vec![(age, &option)]]
            .into_iter()
            .enumerate()
        {
            let f = Frame {
                time: 21.0 + age,
                dt: 0.0,
                events,
                ..Frame::sized(w, h)
            };
            draw_at(&theme, &c, &f, 21.0 + age);
            if which == 0 {
                dearest.0 = dearest.0.max(share(&theme));
            } else {
                dearest.1 = dearest.1.max(share(&theme));
            }
        }
    }
    // See-through, the grain is thinned in a group of its own.
    c.opacity = 0.5;
    draw_at(&theme, &c, &Frame::sized(w, h), 30.0);
    let clear = share(&theme);
    println!(
        "aurora, the background in a 760 x 640 window (processor time): the first frame \
         {first:.2} ms ({cold:.2} ms for one that has the palette to work out too); drifting, \
         mean {mean:.3} ms, worst {worst:.3} ms. To draw, of what a frame may cost: standing \
         {standing:.1}%, see-through {clear:.1}%, a result after = {:.1}%, a new palette {:.1}%",
        dearest.0, dearest.1
    );
    let limit = common::limits().frame.as_secs_f64() * 1000.0;
    assert!(
        cold < limit && first < limit * 0.75 && worst < limit / 2.0,
        "{cold} {first} {worst}"
    );
    assert!(standing > 0.0 && standing < 10.0, "{standing}");
    assert!(clear > 0.0 && clear < 10.0, "{clear}");
    assert!(dearest.0 < 25.0 && dearest.1 < 25.0, "{dearest:?}");
    assert!(theme.api().busiest_frame() * 100.0 < 25.0);
}

/// A window dragged larger and smaller, a new size every frame for
/// twenty seconds' worth of frames: the buffers of each size are let go
/// as the next are made, and the theme stays far inside its 8 MiB.
#[test]
fn a_window_being_resized_keeps_its_memory() {
    let theme = aurora();
    let c = ctx(&theme, true, &[]);
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
        let d = draw_at(&theme, &c, &f, t);
        assert!(d.list.ops.iter().any(|op| matches!(op, Op::Field { .. })));
        most = most.max(theme.engine().used_memory());
    }
    println!(
        "aurora, resized every frame: {} KiB of the theme's 8 MiB at most",
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

/// For looking at: `AURORA_FRAMES=<folder>` writes the background alone,
/// over its sky, as PPM pictures: each palette still, light and dark, and
/// the default palette drifting while keys are pressed (`cargo test -p
/// gmnb-theme --test aurora -- --ignored frames`). `AURORA_DIR=<folder>`
/// takes the theme from there, `AURORA_SIZE=WxH` is the window's size,
/// `AURORA_OPACITY` the window's opacity (over a grey desktop).
#[test]
#[ignore]
fn frames() {
    let Some(out) = std::env::var_os("AURORA_FRAMES") else {
        println!("AURORA_FRAMES is not set; nothing written");
        return;
    };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let (w, h) = std::env::var("AURORA_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((760usize, 640usize));
    let opacity: f32 = std::env::var("AURORA_OPACITY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);
    let write = |name: &str, px: &[[f32; 4]]| {
        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
        for p in px {
            // Over a mid-grey desktop, where the window is see-through.
            ppm.extend((0..3).map(|c| {
                let v = p[c] + 0.5 * (1.0 - p[3]);
                (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
            }));
        }
        std::fs::write(out.join(name), ppm).unwrap();
    };
    // The sky: the window's own background, as thin as the window.
    let sky_of = |theme: &mut Theme, c: &Ctx| -> Paint {
        let style = theme.style(c).unwrap().style.clone();
        let Some(Paint::Gradient(mut g)) = style.fill("window.background").0.first().cloned()
        else {
            panic!("Aurora's window.background is the sky's gradient");
        };
        g.pixels.from = Some([0.0, 0.0]);
        g.pixels.to = Some([0.0, h as f32]);
        for colour in &mut g.colors {
            colour[3] *= c.opacity;
        }
        Paint::Gradient(g)
    };
    let palettes = std::env::var("AURORA_PALETTES").unwrap_or_else(|_| PALETTES.join(","));
    for palette in palettes.split(',') {
        for dark in [true, false] {
            let mut theme = aurora();
            let mut c = ctx(&theme, dark, &[("palette", text(palette))]);
            c.opacity = opacity;
            let sky = sky_of(&mut theme, &c);
            let f = Frame {
                motion: false,
                ..Frame::sized(w as f32, h as f32)
            };
            let d = draw_at(&theme, &c, &f, 0.0);
            write(
                &format!("{palette}-{}.ppm", if dark { "dark" } else { "light" }),
                &painted(&d.list, Some(&sky), w, h),
            );
            // The sky and the lights, as drawn: each colour and how light it is.
            let made = made_of(&d.list, w as f32, h as f32, palette);
            let Paint::Gradient(sky) = &sky else {
                unreachable!()
            };
            for (what, g) in [("sky", sky), ("lights", &made.lights)] {
                let colours: Vec<String> = g
                    .colors
                    .iter()
                    .map(|c| {
                        let rgb = [c[0], c[1], c[2]];
                        format!(
                            "{} {:.3}",
                            appcore::color::to_hex(rgb),
                            appcore::color::luminance(rgb)
                        )
                    })
                    .collect();
                println!("{palette} dark={dark} {what}: {}", colours.join(", "));
            }
        }
    }
    let seconds: f64 = std::env::var("AURORA_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    if seconds > 0.0 {
        let mut theme = aurora();
        let mut c = ctx(&theme, true, &[]);
        c.opacity = opacity;
        let sky = sky_of(&mut theme, &c);
        let key = press(5, KeyKind::Number, 170.0, 380.0);
        let equals = press(121, KeyKind::Equals, 390.0, 600.0);
        let frames = (seconds * 15.0) as usize;
        for i in 0..frames {
            let t = i as f64 / 15.0;
            let mut events = Vec::new();
            for (at, e) in [(1.0, &key), (2.0, &equals)] {
                if t >= at && t - at < 1.15 {
                    events.push((t - at, e));
                }
            }
            let f = Frame {
                time: t,
                dt: if i == 0 { 0.0 } else { 1.0 / 15.0 },
                events,
                ..Frame::sized(w as f32, h as f32)
            };
            let d = draw_at(&theme, &c, &f, t);
            if i % 15 == 0 || (15..48).contains(&i) {
                write(&format!("f{i:04}.ppm"), &painted(&d.list, Some(&sky), w, h));
            }
        }
    }
}
