//! The keyboard's focus ring is never hidden (docs/theming.md, readability
//! rule 5, "The focus ring"). A ring narrower than [`LEAST_WIDTH`], or
//! one that reaches less than [`LEAST_CONTRAST`] on what it lies on (with
//! its edge, where that is drawn), is the base look's ring instead, with
//! a note. A theme can't hide it with `width = 0`, a colour it has made
//! transparent, or the colour of what is under it.
//!
//! What a ring lies on is judged from the style, at the window's own
//! colours: outside the control, what is around it (for `focus`, the
//! window's background, which a keypad's keys lie on); inside it, the
//! control's own fill. A ring left to the toolkit (`native` lists
//! `focus`) is judged as libadwaita draws it ([`native_ring`]): if that
//! doesn't read, the app's ring is drawn instead.

use std::collections::BTreeMap;

use appcore::color::{UI_CONTRAST, contrast};
use appcore::metrics;

use crate::check::Notes;
use crate::states::over;
use crate::tokens::Tokens;
use crate::types::*;

/// The least width of a ring, in pixels.
pub const LEAST_WIDTH: f32 = 1.0;
/// The least contrast of a ring (or its edge) on what it lies on: WCAG's
/// for a part of the interface.
pub const LEAST_CONTRAST: f32 = UI_CONTRAST;

/// How far apart a ring and its edge must be to read on any colour at
/// all: on a colour between them, the contrasts of the two multiply to
/// theirs, so one of them reaches 3 when they are 9 apart.
pub const EDGE_APART: f32 = UI_CONTRAST * UI_CONTRAST;

/// The rings of the style, and where each is drawn.
pub const RINGS: [&str; 3] = ["focus", "date.calendar.focus", "converter.value.focus"];

/// The page's surfaces a control may sit on, each over the window's
/// background: the ring of `focus` lies on them outside the control.
pub const SURFACES: [&str; 5] = [
    "plain.background",
    "header.background",
    "nav.background",
    "panel",
    // The narrow window's History and Memory (R5-A-1).
    "panel.sheet",
];

/// The ring libadwaita draws itself: its accent colour at half opacity,
/// 2 px wide, just inside the control's edge.
pub fn native_ring(accent: Rgba) -> FocusRing {
    FocusRing {
        width: 2.0,
        color: alpha(accent, 0.5),
        offset: 0.0,
        radius: None,
        edge: None,
    }
}

/// The worst contrast `ring` has on any of `grounds` (opaque colours it
/// may lie on): on each, the better of the ring's own and its edge's
/// (`with_edge`: where the edge is drawn, around a keypad's keys).
pub fn worst(ring: &FocusRing, grounds: &[Rgba], with_edge: bool) -> f32 {
    let on = |c: Rgba, g: Rgba| contrast(rgb3(over(c, g)), rgb3(g));
    grounds
        .iter()
        .map(|g| {
            let edge = ring
                .edge
                .filter(|e| with_edge && e.width >= LEAST_WIDTH)
                .map_or(0.0, |e| on(e.color, *g));
            on(ring.color, *g).max(edge)
        })
        .fold(f32::INFINITY, f32::min)
}

/// What a ring lies on, at the window's own colours: `outside` the
/// control and `inside` it. Each list holds opaque colours.
#[derive(Clone, Debug, Default)]
pub struct Grounds {
    pub outside: Vec<Rgba>,
    pub inside: Vec<Rgba>,
    /// Whether the ring's edge is drawn there.
    pub edge: bool,
    /// The page's surfaces a control (not a key) may sit on, which a ring
    /// outside it lies on, the ring alone (`focus` only).
    pub surfaces: Vec<Rgba>,
}

impl Grounds {
    /// The colours `ring` lies on: those outside where its band is
    /// outside the control, those inside where it is inside.
    pub fn under(&self, ring: &FocusRing) -> Vec<Rgba> {
        let mut out = Vec::new();
        if ring.offset > 0.0 {
            out.extend(&self.outside);
        }
        if ring.offset - ring.width < 0.0 {
            out.extend(&self.inside);
        }
        out
    }
}

/// What the window's background shows as: its colours over `bg` (the
/// `bg` token), or on a see-through window faded to `opacity` over a
/// `desktop`.
pub fn backdrop(
    get: impl Fn(&str) -> Option<Value>,
    bg: Rgba,
    desktop: Option<(Rgba, f32)>,
) -> Vec<Rgba> {
    let window = match get("window.background") {
        Some(Value::Fill(f)) => f,
        _ => Fill::color(bg),
    };
    match desktop {
        None => window.samples_over(bg),
        Some((desktop, opacity)) => faded(&window, opacity).samples_over(desktop),
    }
}

/// What the ring at `path` lies on, in a style whose values `get` reads,
/// over `backdrop` (what the window's background shows as, each colour
/// in turn).
pub fn grounds(
    get: impl Fn(&str) -> Option<Value>,
    tokens: &Tokens,
    path: &str,
    backdrop: &[Rgba],
) -> Grounds {
    let fill = |p: &str| match get(p) {
        Some(Value::Fill(f)) => Some(f),
        Some(Value::Surface(s)) => Some(s.background),
        Some(Value::Control(c)) => Some(c.background),
        _ => None,
    };
    // A fill over each of `under`.
    let over_all = |f: &Option<Fill>, under: &[Rgba]| -> Vec<Rgba> {
        match f {
            None => under.to_vec(),
            Some(f) => under.iter().flat_map(|u| f.samples_over(*u)).collect(),
        }
    };
    let backdrop = backdrop.to_vec();
    match path {
        "date.calendar.focus" => {
            let popover = over_all(&fill("popover"), &[tokens.colors.surface]);
            Grounds {
                inside: over_all(&fill("date.calendar.day"), &popover),
                outside: popover,
                edge: false,
                surfaces: Vec::new(),
            }
        }
        "converter.value.focus" => {
            let plain = over_all(&fill("plain.background"), &backdrop);
            Grounds {
                inside: over_all(&fill("converter.field"), &plain),
                outside: plain,
                edge: false,
                surfaces: Vec::new(),
            }
        }
        _ => {
            let mut inside = Vec::new();
            for key in ["keys.number", "keys.func", "keys.operator", "keys.equals"] {
                inside.extend(over_all(&fill(key), &backdrop));
            }
            let surfaces = SURFACES
                .iter()
                .flat_map(|s| over_all(&fill(s), &backdrop))
                .collect();
            Grounds {
                outside: backdrop,
                inside,
                edge: true,
                surfaces,
            }
        }
    }
}

/// `fill` at `opacity`, as the window's background is faded.
fn faded(fill: &Fill, opacity: f32) -> Fill {
    Fill(
        fill.0
            .iter()
            .map(|p| match p {
                Paint::Color(c) => Paint::Color(alpha(*c, c[3] * opacity)),
                Paint::Gradient(g) => {
                    let mut g = g.clone();
                    for c in &mut g.colors {
                        *c = alpha(*c, c[3] * opacity);
                    }
                    Paint::Gradient(g)
                }
            })
            .collect(),
    )
}

/// Keeps every ring of a style being resolved visible: a ring that would
/// hide the keyboard's focus is the base look's ring instead, and a ring
/// left to the toolkit that wouldn't read is the app's.
pub fn floor(tokens: &Tokens, values: &mut BTreeMap<&'static str, Value>, notes: &mut Notes) {
    let behind = backdrop(|p| values.get(p).cloned(), tokens.colors.bg, None);
    // The toolkit's own ring first: if it doesn't read, the app's is drawn.
    let native = matches!(values.get("native"), Some(Value::Words(w)) if w.contains(&"focus"));
    if native {
        let accent = match values.get("stock.accent") {
            Some(Value::Color(c)) => *c,
            _ => tokens.colors.accent_text,
        };
        let ring = native_ring(accent);
        let g = grounds(|p| values.get(p).cloned(), tokens, "focus", &behind);
        // Inside a key, and on a control of the header, the sidebar or a
        // panel, whose fill is the surface's where it is transparent
        // (R5-A-2: the app's ring was judged there, the toolkit's not).
        let got = worst(&ring, &g.under(&ring), false).min(worst(&ring, &g.surfaces, false));
        if got < LEAST_CONTRAST {
            if let Some(Value::Words(w)) = values.get_mut("native") {
                w.retain(|f| *f != "focus");
            }
            notes.push(
                "native",
                format!(
                    "the toolkit's focus ring (the accent at half opacity) reaches {got:.2}:1 on \
                     what it lies on, under {LEAST_CONTRAST}: the app draws `focus` instead"
                ),
            );
        }
    }
    for path in RINGS {
        let Some(ring) = ring_at(|p| values.get(p).cloned(), path) else {
            continue;
        };
        let grounds = grounds(|p| values.get(p).cloned(), tokens, path, &behind);
        let why = if ring.width < LEAST_WIDTH {
            format!("it is {} px wide, under {LEAST_WIDTH}", ring.width)
        } else {
            let mut got = worst(&ring, &grounds.under(&ring), grounds.edge);
            // On a control on one of the page's surfaces (the header, a
            // panel, the sidebar): the ring alone, with no edge. Outside
            // the control it lies on the surface; inside it, on the
            // control's fill, which is the surface where it is transparent
            // (R4-A-1: a ring inside its control was judged on the keys
            // only).
            got = got.min(worst(&ring, &grounds.surfaces, false));
            if got >= LEAST_CONTRAST {
                continue;
            }
            format!("it reaches {got:.2}:1 on what it lies on, under {LEAST_CONTRAST}")
        };
        let fallback = base_ring(tokens, &grounds);
        set_ring(values, path, fallback);
        notes.push(
            path,
            format!(
                "the ring would hide the keyboard's focus ({why}): the base look's ring is drawn \
                 instead"
            ),
        );
    }
}

/// The base look's ring, 2 px just outside the control, in the theme's
/// own `accent_text`, or its `fg` if that reads better there, or black or
/// white if neither reads.
fn base_ring(tokens: &Tokens, grounds: &Grounds) -> FocusRing {
    let ring = |color| FocusRing {
        width: metrics::focus::WIDTH,
        color,
        offset: metrics::focus::OFFSET,
        radius: None,
        edge: None,
    };
    let reads = |color| {
        let ring = ring(color);
        worst(&ring, &grounds.outside, false).min(worst(&ring, &grounds.surfaces, false))
    };
    let c = &tokens.colors;
    // Those that read on every ground; else (white on one, black on
    // another) a grey between them, the one that reads best on the worst.
    let greys = (0..=16).map(|i| {
        let v = i as f32 / 16.0;
        [v, v, v, 1.0]
    });
    let best = [c.accent_text, c.fg, BLACK, WHITE]
        .into_iter()
        .map(|color| (reads(color), color))
        .find(|(got, _)| *got >= LEAST_CONTRAST)
        .map(|(_, color)| color)
        .unwrap_or_else(|| {
            [BLACK, WHITE]
                .into_iter()
                .chain(greys)
                .map(|color| (reads(color), color))
                .fold((-1.0, BLACK), |a, b| if b.0 > a.0 { b } else { a })
                .1
        });
    ring(best)
}

/// The ring at `path` (a row, or a record's part), in a style whose
/// values `get` reads.
pub fn ring_at(get: impl Fn(&str) -> Option<Value>, path: &str) -> Option<FocusRing> {
    if let Some(Value::FocusRing(f)) = get(path) {
        return Some(f);
    }
    match get(path.strip_suffix(".focus")?) {
        Some(Value::Record(r)) => match r.get("focus") {
            Some(Value::FocusRing(f)) => Some(*f),
            _ => None,
        },
        _ => None,
    }
}

fn set_ring(values: &mut BTreeMap<&'static str, Value>, path: &str, ring: FocusRing) {
    if let Some(record) = path.strip_suffix(".focus")
        && let Some(Value::Record(r)) = values.get_mut(record)
    {
        r.insert("focus", Value::FocusRing(ring));
        return;
    }
    if let Some(slot) = values.get_mut(path) {
        *slot = Value::FocusRing(ring);
    }
}

fn rgb3(c: Rgba) -> [f32; 3] {
    [c[0], c[1], c[2]]
}
