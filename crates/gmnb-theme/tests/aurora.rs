//! Aurora, the built-in theme, in the real engine: what its background
//! draws, and that every pixel of it is a colour the readability tests
//! have been through (`tests/css.rs`,
//! `auroras_texts_read_over_every_colour_of_the_sky`).

mod common;

use gmnb_theme::draw::{DisplayList, MaskMode, Op, Rect};
use gmnb_theme::events::{Event, EventKind, KeyKind, Page, Source as PressSource, Via};
use gmnb_theme::layer::{Drawn, Failure, Frame, LayerKind};
use gmnb_theme::theme::{self, Source, Theme};
use gmnb_theme::types::*;
use gmnb_theme::{Ctx, OptionValue};

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

/// What the background's display list shows in a `w × h` window, a pixel
/// at a time: its rectangles, its field and what it masks. (The grain is
/// left out: a twentieth of white or black on a pixel here and there.)
fn painted(list: &DisplayList, w: usize, h: usize) -> Vec<[f32; 3]> {
    // The picture under, the mask once it is drawn, and what is being
    // drawn now.
    let mut stack: Vec<(Layer, Option<Layer>)> = Vec::new();
    let mut now = Layer::new(w, h);
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
    assert!(stack.is_empty());
    now.px.iter().map(|p| [p[0], p[1], p[2]]).collect()
}

/// For looking at: `AURORA_FRAMES=<folder>` writes the background alone
/// as PPM pictures: each palette still, light and dark, and the default
/// palette drifting while keys are pressed (`cargo test -p gmnb-theme
/// --test aurora -- --ignored frames`). `AURORA_DIR=<folder>` takes the
/// theme from there, `AURORA_SIZE=WxH` is the window's size.
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
    let write = |name: &str, px: &[[f32; 3]]| {
        let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
        for p in px {
            ppm.extend(p.map(|c| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8));
        }
        std::fs::write(out.join(name), ppm).unwrap();
    };
    let palettes = std::env::var("AURORA_PALETTES")
        .unwrap_or_else(|_| "aurora,ember,abyss,orchard,graphite,system,custom".into());
    for palette in palettes.split(',') {
        for dark in [true, false] {
            let theme = aurora();
            let c = ctx(&theme, dark, &[("palette", text(palette))]);
            let f = Frame {
                motion: false,
                ..Frame::sized(w as f32, h as f32)
            };
            let d = draw_at(&theme, &c, &f, 0.0);
            write(
                &format!("{palette}-{}.ppm", if dark { "dark" } else { "light" }),
                &painted(&d.list, w, h),
            );
            // The sky and the lights, as drawn: each colour and how light it is.
            for op in &d.list.ops {
                if let Op::Rect {
                    paint: Paint::Gradient(g),
                    ..
                } = op
                {
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
                    println!("{palette} dark={dark}: {}", colours.join(", "));
                }
            }
        }
    }
    let seconds: f64 = std::env::var("AURORA_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    if seconds > 0.0 {
        let theme = aurora();
        let c = ctx(&theme, true, &[]);
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
                write(&format!("f{i:04}.ppm"), &painted(&d.list, w, h));
            }
        }
    }
}
