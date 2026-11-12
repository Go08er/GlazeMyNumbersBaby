//! What the built-in themes' frames cost to draw, at their dearest, over
//! every page's layers, every palette (System and Freestyle too), light and
//! dark, opaque and see-through, from 300 x 400 to 3840 x 2160, their
//! animations at their busiest (held keys, a result, an error, a new
//! option, the stir) and under reduced motion, and Settings' preview tiles:
//! no frame is refused for being over its allowance, no hook is struck or
//! switched off, no note or log line. `frame_cost` is a share of what a
//! frame of its layer may cost (1 is the cap); the dearest of each theme is
//! printed (`-- --nocapture`).

mod common;

use std::collections::BTreeMap;

use gmnb_theme::events::{Cause, Event, EventKind, KeyKind, Page, Source as From, Via};
use gmnb_theme::layer::{
    ChangeKind, DisplayValue, Failure, Frame, GlyphBox, Glyphs, KeyCell, Keys, LayerKind, Role,
};
use gmnb_theme::theme::{self, OptionDef, OptionKind, Source, Theme};
use gmnb_theme::{Ctx, OptionValue};

fn built_in(id: &str) -> Theme {
    Theme::load(Source::built_in(id).unwrap(), common::limits()).unwrap()
}

fn text(s: &str) -> OptionValue {
    OptionValue::Text(s.into())
}

/// Windows from the least to the largest.
const WINDOWS: [(f32, f32); 7] = [
    (300.0, 400.0),
    (400.0, 300.0),
    (760.0, 640.0),
    (1280.0, 800.0),
    (1920.0, 1080.0),
    (2560.0, 1440.0),
    (3840.0, 2160.0),
];

/// The sizes at which the whole of a busy second is drawn.
fn timeline_at(w: f32) -> bool {
    [300.0, 760.0, 1920.0, 3840.0].contains(&w)
}

/// The layer's size in a window of `w` x `h`, and where the window is in the
/// layer's coordinates.
fn layer_box(kind: LayerKind, w: f32, h: f32) -> ((f32, f32), [f32; 4]) {
    match kind {
        LayerKind::Background | LayerKind::Overlay => ((w, h), [0.0, 0.0, w, h]),
        LayerKind::Keypad => {
            let top = (h * 0.38).min(260.0);
            ((w, h - top), [0.0, -top, w, h])
        }
        LayerKind::Display => {
            let top = 30.0f32.min(h / 8.0);
            let band = (h * 0.22).clamp(60.0, 180.0);
            ((w, band), [0.0, -top, w, h])
        }
    }
}

fn grid(rows: i32, cols: i32, w: f32, h: f32, state: usize) -> Vec<KeyCell> {
    let mut cells = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let kind = match (row, col) {
                (r, c) if r == rows - 1 && c == cols - 1 => KeyKind::Equals,
                (_, c) if c == cols - 1 => KeyKind::Operator,
                (0, _) => KeyKind::Func,
                (1, 0) => KeyKind::Toggle,
                (1, 1) => KeyKind::Other,
                _ => KeyKind::Number,
            };
            let (cw, ch) = (w / cols as f32, h / rows as f32);
            let i = (row * cols + col) as usize;
            cells.push(KeyCell {
                id: Some(i as u32),
                kind,
                rect: [col as f32 * cw, row as f32 * ch, cw * 0.97, ch * 0.97],
                row,
                col,
                radius: 8.0,
                enabled: state == 0 || i % 7 != 3,
                hovered: state == 1 && i % 5 == 1,
                pressed: state == 1 && i % 9 == 2,
                checked: state == 2 && i.is_multiple_of(4),
            });
        }
    }
    cells
}

fn glyphs_of(n: usize, w: f32, h: f32) -> Glyphs {
    let size = (h * 0.5).clamp(8.0, 75.0);
    let adv = if n == 0 {
        0.0
    } else {
        (w / n as f32).min(size * 0.55)
    };
    let x0 = w - adv * n as f32;
    Glyphs {
        glyphs: (0..n)
            .map(|i| GlyphBox {
                x: x0 + i as f32 * adv,
                y: h * 0.2,
                w: adv,
                h: h * 0.6,
            })
            .collect(),
        size,
        rect: [x0, h * 0.2, adv * n as f32, h * 0.6],
    }
}

/// The events of a busy second in a layer of `w` x `h`: a key held down
/// (a press every thirtieth of a second), a result, an error, an option
/// chosen, a mode change, the Settings page shown. Each with the time it
/// was born.
fn busy_events(w: f32, h: f32, cells: &[KeyCell], opt: &str) -> Vec<(f64, Event)> {
    let mut out = Vec::new();
    for i in 0..28 {
        let cell = &cells[(i * 5) % cells.len().max(1)];
        let mut e = Event::at(EventKind::Press, Page::Calculator, cell.rect);
        e.source = Some(From::Key);
        e.via = Some(Via::Pointer);
        e.key = Some((cell.id, cell.kind));
        e.cause = Some(Cause::Digit);
        out.push((i as f64 / 30.0, e));
    }
    let mut result = Event::at(
        EventKind::Result,
        Page::Calculator,
        [8.0, 0.0, w - 16.0, h.min(116.0)],
    );
    result.cause = Some(Cause::Equals);
    result.via = Some(Via::Pointer);
    out.push((0.35, result));
    let mut error = Event::at(
        EventKind::Error,
        Page::Calculator,
        [8.0, 0.0, w - 16.0, h.min(116.0)],
    );
    error.cause = Some(Cause::Equals);
    out.push((0.5, error));
    let mut option = Event::at(EventKind::Option, Page::Settings, [0.0, 0.0, w, h]);
    option.option = Some((opt.into(), text("system")));
    out.push((0.55, option));
    out.push((
        0.6,
        Event::at(EventKind::Mode, Page::Calculator, [0.0, 0.0, w, h]),
    ));
    out.push((
        0.65,
        Event::at(EventKind::Show, Page::Converter, [0.0, 0.0, w, h]),
    ));
    out
}

/// Choice options with every value, toggles all on and all off, sliders at
/// their ends, colours from a short list under Freestyle.
fn option_sets(theme: &Theme) -> Vec<BTreeMap<String, OptionValue>> {
    let defs = &theme.def.options;
    let mut sets = Vec::new();
    let colours: Vec<&OptionDef> = defs
        .iter()
        .filter(|o| matches!(o.kind, OptionKind::Color))
        .collect();
    let palette: Vec<String> = defs
        .iter()
        .find_map(|o| match &o.kind {
            OptionKind::Choice { choices, .. }
                if o.id == "palette" || o.id == "flavour" || o.id == "bands" =>
            {
                Some(choices.iter().map(|c| c.id.clone()).collect())
            }
            _ => None,
        })
        .unwrap();
    let key = ["palette", "flavour", "bands"]
        .into_iter()
        .find(|k| defs.iter().any(|o| &o.id == k))
        .unwrap();
    let swatches = [
        "#000000", "#ffffff", "#ffff00", "#0000ff", "#ff2d95", "#808080",
    ];
    for id in &palette {
        for end in 0..2 {
            let mut base = BTreeMap::new();
            for o in defs {
                match &o.kind {
                    OptionKind::Toggle => {
                        base.insert(o.id.clone(), OptionValue::Bool(end == 0));
                    }
                    OptionKind::Slider { min, max, .. } => {
                        base.insert(
                            o.id.clone(),
                            OptionValue::Number(if end == 0 { *max } else { *min }),
                        );
                    }
                    OptionKind::Choice { choices, .. } if o.id != key => {
                        let c = &choices[(end * 2) % choices.len()];
                        base.insert(o.id.clone(), text(&c.id));
                    }
                    _ => {}
                }
            }
            base.insert(key.to_string(), text(id));
            if id == "custom" {
                for (n, sw) in swatches.iter().enumerate() {
                    let mut s = base.clone();
                    for (k, c) in colours.iter().enumerate() {
                        s.insert(c.id.clone(), text(swatches[(n + k * 2) % swatches.len()]));
                    }
                    let _ = sw;
                    sets.push(s);
                }
            } else {
                sets.push(base);
            }
        }
    }
    // Every value of every other choice, once, under the first palette.
    for o in defs {
        if o.id == key {
            continue;
        }
        if let OptionKind::Choice { choices, .. } = &o.kind {
            for c in choices {
                let mut s = BTreeMap::new();
                s.insert(o.id.clone(), text(&c.id));
                sets.push(s);
            }
        }
    }
    sets
}

#[derive(Default)]
struct Worst {
    cost: f64,
    at: String,
}

impl Worst {
    fn see(&mut self, cost: f64, at: impl FnOnce() -> String) {
        if cost > self.cost {
            self.cost = cost;
            self.at = at();
        }
    }
}

fn sweep(id: &str) {
    let mut theme = built_in(id);
    let mut bad: Vec<String> = Vec::new();
    let mut worst: BTreeMap<&str, Worst> = BTreeMap::new();
    let mut by_window: BTreeMap<(u32, u32), f64> = BTreeMap::new();
    let mut frames = 0usize;
    let clock = std::rc::Rc::new(std::cell::Cell::new(100.0f64));
    let read = clock.clone();
    theme.api().set_clock(std::rc::Rc::new(move || read.get()));
    let sets = option_sets(&theme);
    let key = ["palette", "flavour", "bands"]
        .into_iter()
        .find(|k| theme.def.options.iter().any(|o| &o.id == k))
        .unwrap();
    for (si, set) in sets.iter().enumerate() {
        for dark in [false, true] {
            for (oi, opacity) in [1.0f32, 0.5, 0.1].into_iter().enumerate() {
                // (Every set at full opacity; the see-through ones in
                // alternation, both dark and light.)
                if oi > 0 && (si + oi + usize::from(dark)) % 3 != 0 {
                    continue;
                }
                let ctx = Ctx {
                    dark,
                    opacity,
                    accent: (si % 2 == 0).then_some([0.1, 0.5, 0.9]),
                    options: theme::complete_options(&theme.def.options, set),
                    ..Ctx::default()
                };
                if let Err(e) = theme.style(&ctx) {
                    bad.push(format!("{id}: style {e}"));
                    continue;
                }
                let _ = theme.take_log();
                for (w, h) in WINDOWS {
                    for kind in LayerKind::ALL {
                        if theme.layer(kind).is_none() {
                            continue;
                        }
                        let ((lw, lh), win) = layer_box(kind, w, h);
                        let pads: &[(i32, i32)] = match kind {
                            LayerKind::Keypad => &[(6, 4), (8, 5)],
                            _ => &[(6, 4)],
                        };
                        for &(rows, cols) in pads {
                            let cells = grid(rows, cols, lw, lh, si % 3);
                            let born = busy_events(lw, lh, &cells, key);
                            let full = timeline_at(w) && (si + usize::from(dark)) % 2 == 0;
                            let mut times: Vec<f64> = vec![0.0, 0.05];
                            if full {
                                times.extend((0..45).map(|i| 0.1 + i as f64 / 30.0));
                                times.extend([2.0, 4.9, 5.2, 30.0]);
                            } else {
                                times.extend([0.2, 0.5, 0.7, 1.5]);
                            }
                            for (n, motion) in [true, true, false].into_iter().enumerate() {
                                if n == 1 && !full {
                                    continue;
                                }
                                for (k, &t) in times.iter().enumerate() {
                                    if !motion && k > 2 {
                                        break;
                                    }
                                    let t = t + 60.0 * (n as f64);
                                    clock.set(100.0 + t);
                                    let evs: Vec<(f64, &Event)> = if motion {
                                        born.iter()
                                            .filter(|(b, _)| *b + 60.0 * (n as f64) <= t)
                                            .map(|(b, e)| (t - b - 60.0 * (n as f64), e))
                                            .filter(|(age, _)| *age <= 5.0)
                                            .collect()
                                    } else {
                                        Vec::new()
                                    };
                                    let mut frame = Frame::sized(lw, lh);
                                    frame.time = t;
                                    frame.dt = 1.0 / 30.0;
                                    frame.motion = motion;
                                    frame.events = evs;
                                    frame.window = Some(win);
                                    frame.pointer = Some((lw * 0.5, lh * 0.5));
                                    let glyphs;
                                    let prev;
                                    let text_of =
                                        ["0", "1234567.89", "-0.000000000000001e+29", "ERROR"];
                                    match kind {
                                        LayerKind::Keypad => {
                                            frame.keys = Some(Keys {
                                                pad: rows as u64,
                                                layout: rows as u64,
                                                cells: &cells,
                                            });
                                        }
                                        LayerKind::Display => {
                                            let s = text_of[(k + si) % 4];
                                            glyphs = glyphs_of(s.len(), lw, lh);
                                            prev = glyphs_of(3, lw, lh);
                                            let change = match (k + si) % 5 {
                                                0 => None,
                                                1 => Some((ChangeKind::Typing, t % 0.3, 1)),
                                                2 => Some((ChangeKind::Result, t % 0.6, 1)),
                                                3 => Some((ChangeKind::Replace, t % 0.6, 3)),
                                                _ => Some((ChangeKind::Error, t % 0.6, 0)),
                                            };
                                            frame.value = Some(DisplayValue {
                                                readout: (k % 3) as u64,
                                                version: k as u64 + 1000 * (si as u64),
                                                text: s,
                                                glyphs: &glyphs,
                                                band: [0.0, 0.0, lw, lh],
                                                error: s == "ERROR",
                                                align_end: true,
                                                role: [
                                                    Role::Calculator,
                                                    Role::Converter,
                                                    Role::Date,
                                                ][k % 3],
                                                change,
                                                previous: change.map(|_| &prev),
                                            });
                                        }
                                        _ => {}
                                    }
                                    let name = || {
                                        format!(
                                            "{id} {kind:?} set#{si}={set:?} dark={dark} op={opacity} motion={motion} {w}x{h} pad {rows}x{cols} t={t:.2}"
                                        )
                                    };
                                    frames += 1;
                                    match theme.draw(kind, &ctx, &frame) {
                                        Ok(d) => {
                                            let cost = theme.api().frame_cost();
                                            worst
                                                .entry(match kind {
                                                    LayerKind::Background => "background",
                                                    LayerKind::Keypad => "keypad",
                                                    LayerKind::Display => "display",
                                                    LayerKind::Overlay => "overlay",
                                                })
                                                .or_default()
                                                .see(cost, name);
                                            let e =
                                                by_window.entry((w as u32, h as u32)).or_default();
                                            *e = e.max(cost);
                                            if cost >= 1.0 {
                                                bad.push(format!("over: {cost} {}", name()));
                                            }
                                            let _ = d;
                                        }
                                        Err(Failure::Failed(e)) => {
                                            bad.push(format!("{}: failed: {e}", name()));
                                            theme = built_in(id);
                                            let read = clock.clone();
                                            theme
                                                .api()
                                                .set_clock(std::rc::Rc::new(move || read.get()));
                                            let _ = theme.style(&ctx);
                                        }
                                        Err(Failure::Off) => {
                                            bad.push(format!("{}: off", name()));
                                        }
                                    }
                                    let notes = theme.take_draw_notes();
                                    if !notes.is_empty() {
                                        bad.push(format!("{}: notes {notes:?}", name()));
                                    }
                                    let log = theme.take_log();
                                    if !log.is_empty() {
                                        bad.push(format!("{}: log {log:?}", name()));
                                    }
                                    if bad.len() > 40 {
                                        panic!("{id}:\n{}", bad.join("\n"));
                                    }
                                }
                            }
                        }
                    }
                }
                if !theme
                    .hooks()
                    .iter()
                    .all(|(_, strikes, off)| *strikes == 0 && !*off)
                {
                    bad.push(format!("{id} set#{si}: hooks {:?}", theme.hooks()));
                    theme = built_in(id);
                    let read = clock.clone();
                    theme.api().set_clock(std::rc::Rc::new(move || read.get()));
                }
            }
        }
    }
    // Settings' preview tiles, every choice of every option that has one.
    let mut tiles = Worst::default();
    for set in sets.iter().step_by(3) {
        for dark in [false, true] {
            let ctx = Ctx {
                dark,
                options: theme::complete_options(&theme.def.options, set),
                ..Ctx::default()
            };
            let _ = theme.style(&ctx);
            let defs = theme.def.options.clone();
            for o in &defs {
                if let OptionKind::Choice {
                    choices,
                    preview: Some(_),
                } = &o.kind
                {
                    for c in choices {
                        match theme.preview(o, &c.id, &ctx) {
                            Ok(_) => {
                                let cost = theme.api().frame_cost();
                                tiles.see(cost, || {
                                    format!("{id} {}={} dark={dark} {set:?}", o.id, c.id)
                                });
                            }
                            Err(e) => bad.push(format!("{id} tile {}={}: {e}", o.id, c.id)),
                        }
                    }
                }
            }
        }
    }
    let _ = theme.take_draw_notes();
    println!("== {id}: {frames} frames, {} option sets", sets.len());
    for (layer, w) in &worst {
        println!("  {layer}: dearest {:.2}% at {}", w.cost * 100.0, w.at);
    }
    for ((w, h), c) in &by_window {
        println!("  window {w}x{h}: dearest {:.2}%", c * 100.0);
    }
    println!(
        "  preview tile: dearest {:.2}% ({})",
        tiles.cost * 100.0,
        tiles.at
    );
    println!(
        "  busiest_frame {:.2}%",
        theme.api().busiest_frame() * 100.0
    );
    assert!(bad.is_empty(), "{id}:\n{}", bad.join("\n"));
}

#[test]
#[ignore = "a long sweep (five minutes): cargo test --release -p gmnb-theme --test frame_budget -- --ignored --nocapture"]
fn aurora_frames_cost_what_they_may() {
    sweep("aurora");
}

#[test]
#[ignore = "a long sweep (five minutes): cargo test --release -p gmnb-theme --test frame_budget -- --ignored --nocapture"]
fn sprinkles_frames_cost_what_they_may() {
    sweep("sprinkles");
}

#[test]
#[ignore = "a long sweep (five minutes): cargo test --release -p gmnb-theme --test frame_budget -- --ignored --nocapture"]
fn swirls_frames_cost_what_they_may() {
    sweep("swirls");
}
