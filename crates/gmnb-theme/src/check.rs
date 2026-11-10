//! Checks: what a script returned ([`ThemeValue`]) into the typed values of
//! [`crate::types`].
//!
//! Nothing here fails. A value of the wrong type or an unknown key is a
//! [`Note`] and the base value stays; a number out of range is clamped,
//! with a note. Values that merge (fonts, text, surfaces, controls) are
//! checked onto their base, field by field; lists, states and transitions
//! are taken whole (docs/theming.md, "Resolving a style").

use std::collections::BTreeMap;

use crate::ThemeValue;
use crate::types::*;

/// Something a theme's author should know: where, and what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// The style path (`keys.number.font.size`), or a place in the theme
    /// table (`options[2].default`).
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

/// The notes of one check, capped ([`Notes::MAX`]): the first are kept and
/// the rest counted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notes {
    pub list: Vec<Note>,
    /// Notes left out, past the cap.
    pub dropped: usize,
}

impl Notes {
    pub const MAX: usize = 200;

    pub fn push(&mut self, path: &str, message: impl Into<String>) {
        if self.list.len() >= Notes::MAX {
            self.dropped += 1;
            return;
        }
        self.list.push(Note {
            path: path.to_string(),
            message: message.into(),
        });
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    fn wrong(&mut self, path: &str, want: &str, got: &ThemeValue) {
        self.push(path, format!("expected {want}, got {}", describe(got)));
    }
}

fn describe(v: &ThemeValue) -> String {
    match v {
        ThemeValue::Nil => "nil".into(),
        ThemeValue::Bool(b) => b.to_string(),
        ThemeValue::Int(i) => i.to_string(),
        ThemeValue::Number(n) => n.to_string(),
        ThemeValue::String(s) => {
            let mut short: String = s.chars().take(40).collect();
            if short.len() < s.len() {
                short.push('…');
            }
            format!("{short:?}")
        }
        ThemeValue::Array(_) => "a list".into(),
        ThemeValue::Map(_) => "a table".into(),
        ThemeValue::Function(_) => "a function".into(),
    }
}

pub(crate) fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn index(path: &str, i: usize) -> String {
    format!("{path}[{}]", i + 1)
}

/// A list: an array, or the empty table (which converts to an empty map).
pub fn list(v: &ThemeValue) -> Option<&[ThemeValue]> {
    match v {
        ThemeValue::Array(a) => Some(a),
        ThemeValue::Map(m) if m.is_empty() => Some(&[]),
        _ => None,
    }
}

fn map<'a>(
    v: &'a ThemeValue,
    path: &str,
    want: &str,
    notes: &mut Notes,
) -> Option<&'a BTreeMap<String, ThemeValue>> {
    match v {
        ThemeValue::Map(m) => Some(m),
        other => {
            notes.wrong(path, want, other);
            None
        }
    }
}

/// Notes every key of `m` that is in neither list.
fn unknown_keys(
    m: &BTreeMap<String, ThemeValue>,
    path: &str,
    known: &[&str],
    also: &[&str],
    notes: &mut Notes,
) {
    for key in m.keys() {
        if !known.contains(&key.as_str()) && !also.contains(&key.as_str()) {
            notes.push(&join(path, key), "unknown key");
        }
    }
}

// ---- Scalars ---------------------------------------------------------------

/// A number, clamped to `min..=max` (with a note).
pub fn number(v: &ThemeValue, path: &str, min: f64, max: f64, notes: &mut Notes) -> Option<f64> {
    let Some(n) = v.as_f64() else {
        notes.wrong(path, "a number", v);
        return None;
    };
    if n < min || n > max {
        let to = n.clamp(min, max);
        notes.push(path, format!("{n} is out of range; using {to}"));
        return Some(to);
    }
    Some(n)
}

fn num32(v: &ThemeValue, path: &str, min: f64, max: f64, notes: &mut Notes) -> Option<f32> {
    number(v, path, min, max, notes).map(|n| n as f32)
}

pub fn boolean(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<bool> {
    match v {
        ThemeValue::Bool(b) => Some(*b),
        other => {
            notes.wrong(path, "true or false", other);
            None
        }
    }
}

/// One of `words`.
pub fn word(
    v: &ThemeValue,
    path: &str,
    words: &'static [&'static str],
    notes: &mut Notes,
) -> Option<&'static str> {
    let found = v
        .as_str()
        .and_then(|s| words.iter().find(|w| **w == s).copied());
    if found.is_none() {
        notes.wrong(path, &format!("one of {}", quoted(words)), v);
    }
    found
}

fn quoted(words: &[&str]) -> String {
    words
        .iter()
        .map(|w| format!("\"{w}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

// ---- Colours and paints ----------------------------------------------------

/// `#rgb`, `#rrggbb` or `#rrggbbaa`.
pub fn parse_hex(s: &str) -> Option<Rgba> {
    let digits = s.strip_prefix('#')?;
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let d: Vec<f32> = digits
        .bytes()
        .map(|b| (b as char).to_digit(16).unwrap_or(0) as f32)
        .collect();
    let pair = |i: usize| (d[i] * 16.0 + d[i + 1]) / 255.0;
    match d.len() {
        3 => Some([d[0] * 17.0 / 255.0, d[1] * 17.0 / 255.0, d[2] * 17.0 / 255.0, 1.0]),
        6 => Some([pair(0), pair(2), pair(4), 1.0]),
        8 => Some([pair(0), pair(2), pair(4), pair(6)]),
        _ => None,
    }
}

/// A Color: a hex string, or `{ r, g, b, a }` with each part 0 to 1.
pub fn color(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<Rgba> {
    match v {
        ThemeValue::String(s) => {
            let c = parse_hex(s);
            if c.is_none() {
                notes.wrong(path, "a colour (\"#rrggbb\", \"#rrggbbaa\" or { r, g, b, a })", v);
            }
            c
        }
        ThemeValue::Map(m) if m.contains_key("r") => {
            unknown_keys(m, path, &["r", "g", "b", "a"], &[], notes);
            let mut part = |key: &str, default: Option<f32>| match m.get(key) {
                Some(v) => num32(v, &join(path, key), 0.0, 1.0, notes),
                None => {
                    if default.is_none() {
                        notes.push(path, format!("a colour table needs `{key}`"));
                    }
                    default
                }
            };
            let (r, g, b, a) = (
                part("r", None),
                part("g", None),
                part("b", None),
                part("a", Some(1.0)),
            );
            Some([r?, g?, b?, a?])
        }
        other => {
            notes.wrong(
                path,
                "a colour (\"#rrggbb\", \"#rrggbbaa\" or { r, g, b, a })",
                other,
            );
            None
        }
    }
}

fn point(v: &ThemeValue, path: &str, min: f64, max: f64, notes: &mut Notes) -> Option<[f32; 2]> {
    match list(v) {
        Some([x, y]) => Some([
            num32(x, &index(path, 0), min, max, notes)?,
            num32(y, &index(path, 1), min, max, notes)?,
        ]),
        _ => {
            notes.wrong(path, "a point { x, y }", v);
            None
        }
    }
}

const PX: f64 = 1.0e5;

/// A gradient table, as `gmnb.linear`, `gmnb.radial` and `gmnb.conic`
/// make them. `pixels`: whether placement in pixels is taken (draw hooks)
/// or refused (a style).
pub fn gradient(v: &ThemeValue, path: &str, pixels: bool, notes: &mut Notes) -> Option<Gradient> {
    let m = map(v, path, "a gradient", notes)?;
    unknown_keys(
        m,
        path,
        &[
            "kind", "colors", "stops", "angle", "from", "to", "at", "center", "radius", "radius_y",
        ],
        &[],
        notes,
    );
    let kind = match m.get("kind").and_then(ThemeValue::as_str) {
        Some("linear") => GradientKind::Linear,
        Some("radial") => GradientKind::Radial,
        Some("conic") => GradientKind::Conic,
        _ => {
            notes.push(
                &join(path, "kind"),
                "expected \"linear\", \"radial\" or \"conic\"",
            );
            return None;
        }
    };
    let cpath = join(path, "colors");
    let Some(listed) = m.get("colors").and_then(list) else {
        notes.push(&cpath, "a gradient needs a list of 2 to 16 colours");
        return None;
    };
    if !(2..=16).contains(&listed.len()) {
        notes.push(&cpath, format!("{} colours; a gradient takes 2 to 16", listed.len()));
        return None;
    }
    let mut colors = Vec::with_capacity(listed.len());
    for (i, c) in listed.iter().enumerate() {
        colors.push(color(c, &index(&cpath, i), notes)?);
    }
    let n = colors.len();
    let stops = match m.get("stops") {
        None => (0..n).map(|i| i as f32 / (n - 1) as f32).collect(),
        Some(v) => {
            let spath = join(path, "stops");
            let Some(given) = list(v).filter(|l| l.len() == n) else {
                notes.push(&spath, format!("expected {n} stops, one per colour"));
                return None;
            };
            let mut stops: Vec<f32> = Vec::with_capacity(n);
            for (i, s) in given.iter().enumerate() {
                let s = num32(s, &index(&spath, i), 0.0, 1.0, notes)?;
                if stops.last().is_some_and(|&last| s < last) {
                    notes.push(&index(&spath, i), "stops must never decrease");
                    return None;
                }
                stops.push(s);
            }
            stops
        }
    };
    let mut g = Gradient {
        kind,
        colors,
        stops,
        angle: if kind == GradientKind::Linear { 180.0 } else { 0.0 },
        at: [0.5, 0.5],
        extent: Extent::FarthestCorner,
        pixels: Pixels::default(),
    };
    if let Some(v) = m.get("angle") {
        g.angle = num32(v, &join(path, "angle"), -36000.0, 36000.0, notes)?;
    }
    if let Some(v) = m.get("at") {
        g.at = point(v, &join(path, "at"), -10.0, 10.0, notes)?;
    }
    for (key, slot) in [
        ("from", &mut g.pixels.from),
        ("to", &mut g.pixels.to),
        ("center", &mut g.pixels.center),
    ] {
        if let Some(v) = m.get(key) {
            *slot = Some(point(v, &join(path, key), -PX, PX, notes)?);
        }
    }
    match m.get("radius") {
        None => {}
        Some(ThemeValue::String(s)) => match Extent::ALL.iter().find(|(n, _)| n == s) {
            Some((_, e)) => g.extent = *e,
            None => {
                notes.push(
                    &join(path, "radius"),
                    "expected a number, \"closest-side\", \"farthest-side\", \"closest-corner\" or \"farthest-corner\"",
                );
                return None;
            }
        },
        Some(v) => g.pixels.radius = Some(num32(v, &join(path, "radius"), 0.0, PX, notes)?),
    }
    if let Some(v) = m.get("radius_y") {
        g.pixels.radius_y = Some(num32(v, &join(path, "radius_y"), 0.0, PX, notes)?);
    }
    if !pixels && !g.pixels.is_empty() {
        notes.push(
            path,
            "a gradient placed in pixels (from, to, center, a radius in pixels) only works in draw hooks; use angle or at",
        );
        return None;
    }
    Some(g)
}

/// A Paint: a colour or a gradient.
pub fn paint(v: &ThemeValue, path: &str, pixels: bool, notes: &mut Notes) -> Option<Paint> {
    match v {
        ThemeValue::Map(m) if m.contains_key("kind") || m.contains_key("colors") => {
            gradient(v, path, pixels, notes).map(Paint::Gradient)
        }
        _ => color(v, path, notes).map(Paint::Color),
    }
}

/// A Fill: one paint, or up to four layered, topmost first.
pub fn fill(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<Fill> {
    match list(v) {
        Some(layers) => {
            if layers.len() > Fill::MAX {
                notes.push(
                    path,
                    format!("{} layers; a fill takes up to {}", layers.len(), Fill::MAX),
                );
                return None;
            }
            let mut out = Vec::with_capacity(layers.len());
            for (i, layer) in layers.iter().enumerate() {
                out.push(paint(layer, &index(path, i), false, notes)?);
            }
            Some(Fill(out))
        }
        None => paint(v, path, false, notes).map(|p| Fill(vec![p])),
    }
}

// ---- Shadows, borders, radii, spacing ---------------------------------------

pub const MAX_SHADOWS: usize = 8;
/// The contract's cap on a blur radius, in pixels.
pub const MAX_BLUR: f64 = 100.0;

pub fn shadow(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<Shadow> {
    let m = map(v, path, "a shadow { x, y, blur, spread, color, inset }", notes)?;
    unknown_keys(m, path, &["x", "y", "blur", "spread", "color", "inset"], &[], notes);
    let Some(c) = m.get("color") else {
        notes.push(path, "a shadow needs `color`");
        return None;
    };
    let mut s = Shadow {
        x: 0.0,
        y: 0.0,
        blur: 0.0,
        spread: 0.0,
        color: color(c, &join(path, "color"), notes)?,
        inset: false,
    };
    for (key, slot, min, max) in [
        ("x", &mut s.x, -1000.0, 1000.0),
        ("y", &mut s.y, -1000.0, 1000.0),
        ("blur", &mut s.blur, 0.0, MAX_BLUR),
        ("spread", &mut s.spread, -1000.0, 1000.0),
    ] {
        if let Some(v) = m.get(key) {
            *slot = num32(v, &join(path, key), min, max, notes)?;
        }
    }
    if let Some(v) = m.get("inset") {
        s.inset = boolean(v, &join(path, "inset"), notes)?;
    }
    Some(s)
}

/// A shadow list: at most [`MAX_SHADOWS`], topmost first.
pub fn shadows(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<Vec<Shadow>> {
    let Some(items) = list(v) else {
        notes.wrong(path, "a list of shadows", v);
        return None;
    };
    if items.len() > MAX_SHADOWS {
        notes.push(
            path,
            format!("{} shadows; a list takes up to {MAX_SHADOWS}", items.len()),
        );
        return None;
    }
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        out.push(shadow(item, &index(path, i), notes)?);
    }
    Some(out)
}

/// A Border, or `false` (`Some(None)`): no border.
pub fn border(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<Option<Border>> {
    if *v == ThemeValue::Bool(false) {
        return Some(None);
    }
    let m = map(v, path, "a border { width, color } or false", notes)?;
    unknown_keys(m, path, &["width", "color"], &[], notes);
    let (Some(w), Some(c)) = (m.get("width"), m.get("color")) else {
        notes.push(path, "a border needs `width` and `color`");
        return None;
    };
    Some(Some(Border {
        width: num32(w, &join(path, "width"), 0.0, 64.0, notes)?,
        color: color(c, &join(path, "color"), notes)?,
    }))
}

const MAX_RADIUS: f64 = 9999.0;
const MAX_SPACE: f64 = 4096.0;

/// A Radius: a number, or four.
pub fn radius(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<Radius> {
    if v.as_f64().is_some() {
        return num32(v, path, 0.0, MAX_RADIUS, notes).map(Radius::all);
    }
    match list(v) {
        Some(four) if four.len() == 4 => {
            let mut r = [0.0; 4];
            for (i, c) in four.iter().enumerate() {
                r[i] = num32(c, &index(path, i), 0.0, MAX_RADIUS, notes)?;
            }
            Some(Radius(r))
        }
        _ => {
            notes.wrong(path, "a radius (a number, or four)", v);
            None
        }
    }
}

/// A Spacing: a number, `{ vertical, horizontal }` or `{ top, right,
/// bottom, left }`.
pub fn spacing(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<Spacing> {
    if v.as_f64().is_some() {
        return num32(v, path, -MAX_SPACE, MAX_SPACE, notes).map(Spacing::all);
    }
    let parts = list(v).filter(|l| l.len() == 2 || l.len() == 4);
    let Some(parts) = parts else {
        notes.wrong(path, "a spacing (a number, two or four)", v);
        return None;
    };
    let mut n = [0.0; 4];
    for (i, p) in parts.iter().enumerate() {
        n[i] = num32(p, &index(path, i), -MAX_SPACE, MAX_SPACE, notes)?;
    }
    Some(if parts.len() == 2 {
        Spacing::vh(n[0], n[1])
    } else {
        Spacing::trbl(n[0], n[1], n[2], n[3])
    })
}

// ---- Fonts -----------------------------------------------------------------

pub const MAX_FAMILIES: usize = 8;

/// A family name that can be written into the stylesheet as a quoted
/// string: not empty, no control characters, not absurdly long.
pub fn family_ok(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
}

/// Font families: one name, or a list to fall back through.
pub fn families(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<Vec<String>> {
    let names: Vec<&ThemeValue> = match v {
        ThemeValue::String(_) => vec![v],
        other => match list(other) {
            Some(l) if !l.is_empty() && l.len() <= MAX_FAMILIES => l.iter().collect(),
            _ => {
                notes.wrong(path, "a font family name, or a list of up to 8", v);
                return None;
            }
        },
    };
    let mut out = Vec::with_capacity(names.len());
    for (i, name) in names.iter().enumerate() {
        match name.as_str() {
            Some(s) if family_ok(s) => out.push(s.to_string()),
            _ => {
                notes.wrong(&index(path, i), "a font family name", name);
                return None;
            }
        }
    }
    Some(out)
}

/// An OpenType tag: four ASCII letters or digits.
fn tag(s: &str) -> bool {
    s.len() == 4 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// `"tnum"`, or `"ss01=0"` to switch one off.
fn feature(s: &str) -> Option<(String, u32)> {
    let (name, value) = match s.split_once('=') {
        Some((name, value)) => (name, value.parse::<u32>().ok().filter(|v| *v <= 99)?),
        None => (s, 1),
    };
    tag(name).then(|| (name.to_string(), value))
}

/// A Font, onto `base`: every field is optional.
pub fn font(v: &ThemeValue, base: &Font, path: &str, notes: &mut Notes) -> Font {
    let mut f = base.clone();
    let Some(m) = map(v, path, "a font { family, size, weight, … }", notes) else {
        return f;
    };
    unknown_keys(
        m,
        path,
        &[
            "family",
            "size",
            "weight",
            "italic",
            "spacing",
            "features",
            "variations",
        ],
        &[],
        notes,
    );
    if let Some(v) = m.get("family")
        && let Some(names) = families(v, &join(path, "family"), notes)
    {
        f.family = names;
    }
    if let Some(v) = m.get("size")
        && let Some(n) = num32(v, &join(path, "size"), 1.0, 512.0, notes)
    {
        f.size = n;
    }
    if let Some(v) = m.get("weight")
        && let Some(n) = num32(v, &join(path, "weight"), 1.0, 1000.0, notes)
    {
        f.weight = n;
    }
    if let Some(v) = m.get("italic")
        && let Some(b) = boolean(v, &join(path, "italic"), notes)
    {
        f.italic = b;
    }
    if let Some(v) = m.get("spacing")
        && let Some(n) = num32(v, &join(path, "spacing"), -0.5, 2.0, notes)
    {
        f.spacing = n;
    }
    if let Some(v) = m.get("features") {
        let fpath = join(path, "features");
        let parsed = list(v).filter(|l| l.len() <= 32).and_then(|l| {
            l.iter()
                .map(|t| t.as_str().and_then(feature))
                .collect::<Option<Vec<_>>>()
        });
        match parsed {
            Some(features) => f.features = features,
            None => notes.wrong(
                &fpath,
                "a list of OpenType features (\"tnum\", \"ss01=0\")",
                v,
            ),
        }
    }
    if let Some(v) = m.get("variations") {
        let vpath = join(path, "variations");
        match v {
            ThemeValue::Map(axes) if axes.len() <= 16 && axes.keys().all(|k| tag(k)) => {
                let mut out = Vec::with_capacity(axes.len());
                let mut ok = true;
                for (axis, value) in axes {
                    match num32(value, &join(&vpath, axis), -1.0e5, 1.0e5, notes) {
                        Some(n) => out.push((axis.clone(), n)),
                        None => ok = false,
                    }
                }
                if ok {
                    f.variations = out;
                }
            }
            other => notes.wrong(&vpath, "a table of axes ({ opsz = 24 })", other),
        }
    }
    f
}

// ---- Transitions -------------------------------------------------------------

pub const MAX_MS: f64 = 5000.0;

fn ease(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<Ease> {
    if let Some(s) = v.as_str() {
        let found = Ease::NAMED.iter().find(|(n, _)| *n == s).map(|(_, e)| *e);
        if found.is_none() {
            notes.wrong(path, "an ease (\"ease-out\", \"spring\", … or four numbers)", v);
        }
        return found;
    }
    match list(v) {
        Some(four) if four.len() == 4 => {
            let mut b = [0.0; 4];
            for (i, n) in four.iter().enumerate() {
                // The x of a control point stays within the curve's span.
                let (min, max) = if i % 2 == 0 { (0.0, 1.0) } else { (-5.0, 5.0) };
                b[i] = num32(n, &index(path, i), min, max, notes)?;
            }
            Some(Ease::Bezier(b))
        }
        _ => {
            notes.wrong(path, "an ease (\"ease-out\", \"spring\", … or four numbers)", v);
            None
        }
    }
}

/// A Transition: property → milliseconds, or `{ ms, ease }`.
pub fn transition(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<Transition> {
    let m = map(v, path, "a transition { background = 160, … }", notes)?;
    let mut out = Vec::new();
    for (key, value) in m {
        let kpath = join(path, key);
        let Some((_, prop)) = TransitionProp::ALL.iter().find(|(n, _)| n == key) else {
            notes.push(&kpath, "unknown key");
            continue;
        };
        let (ms, e) = match value {
            ThemeValue::Map(d) => {
                unknown_keys(d, &kpath, &["ms", "ease"], &[], notes);
                let Some(ms) = d.get("ms") else {
                    notes.push(&kpath, "a duration needs `ms`");
                    return None;
                };
                let e = match d.get("ease") {
                    Some(e) => ease(e, &join(&kpath, "ease"), notes)?,
                    None => Ease::EaseOut,
                };
                (num32(ms, &join(&kpath, "ms"), 0.0, MAX_MS, notes)?, e)
            }
            other => (num32(other, &kpath, 0.0, MAX_MS, notes)?, Ease::EaseOut),
        };
        out.push((*prop, ms, e));
    }
    out.sort_by_key(|(p, _, _)| *p);
    Some(Transition(out))
}

// ---- Icons, focus rings, states ----------------------------------------------

/// An Icon, onto `base`.
pub fn icon(v: &ThemeValue, base: Icon, path: &str, notes: &mut Notes) -> Icon {
    let mut icon = base;
    let Some(m) = map(v, path, "an icon { size, stroke, color }", notes) else {
        return icon;
    };
    unknown_keys(m, path, &["size", "stroke", "color"], &[], notes);
    if let Some(v) = m.get("size")
        && let Some(n) = num32(v, &join(path, "size"), 1.0, 512.0, notes)
    {
        icon.size = n;
    }
    if let Some(v) = m.get("stroke")
        && let Some(n) = num32(v, &join(path, "stroke"), 0.0, 64.0, notes)
    {
        icon.stroke = Some(n);
    }
    if let Some(v) = m.get("color")
        && let Some(c) = color(v, &join(path, "color"), notes)
    {
        icon.color = Some(c);
    }
    icon
}

/// A FocusRing, onto `base`.
pub fn focus_ring(v: &ThemeValue, base: FocusRing, path: &str, notes: &mut Notes) -> FocusRing {
    let mut ring = base;
    let Some(m) = map(v, path, "a focus ring { width, color, offset, radius }", notes) else {
        return ring;
    };
    unknown_keys(m, path, &["width", "color", "offset", "radius"], &[], notes);
    if let Some(v) = m.get("width")
        && let Some(n) = num32(v, &join(path, "width"), 0.0, 64.0, notes)
    {
        ring.width = n;
    }
    if let Some(v) = m.get("color")
        && let Some(c) = color(v, &join(path, "color"), notes)
    {
        ring.color = c;
    }
    if let Some(v) = m.get("offset")
        && let Some(n) = num32(v, &join(path, "offset"), -64.0, 64.0, notes)
    {
        ring.offset = n;
    }
    if let Some(v) = m.get("radius")
        && let Some(n) = num32(v, &join(path, "radius"), 0.0, MAX_RADIUS, notes)
    {
        ring.radius = Some(n);
    }
    ring
}

const STATE_KEYS: [&str; 14] = [
    "background",
    "overlay",
    "color",
    "fade",
    "border",
    "shadow",
    "opacity",
    "brightness",
    "saturation",
    "scale",
    "rotate",
    "font",
    "icon",
    "transition",
];

/// A State. It is taken whole: what it leaves out stays as at rest, not as
/// the base look's state had it. A part of the wrong type is noted and left
/// out.
pub fn state(v: &ThemeValue, path: &str, notes: &mut Notes) -> Option<State> {
    let m = map(v, path, "a state { background, overlay, color, … }", notes)?;
    unknown_keys(m, path, &STATE_KEYS, &[], notes);
    let mut s = State::default();
    let at = |key: &str| join(path, key);
    if let Some(v) = m.get("background") {
        s.background = fill(v, &at("background"), notes);
    }
    if let Some(v) = m.get("overlay") {
        s.overlay = color(v, &at("overlay"), notes);
    }
    if let Some(v) = m.get("color") {
        s.color = color(v, &at("color"), notes);
    }
    if let Some(v) = m.get("border") {
        s.border = border(v, &at("border"), notes);
    }
    if let Some(v) = m.get("shadow") {
        s.shadow = shadows(v, &at("shadow"), notes);
    }
    for (key, slot, min, max) in [
        ("fade", &mut s.fade, 0.0, 1.0),
        ("opacity", &mut s.opacity, 0.0, 1.0),
        ("brightness", &mut s.brightness, 0.0, 4.0),
        ("saturation", &mut s.saturation, 0.0, 4.0),
        ("scale", &mut s.scale, 0.0, 4.0),
        ("rotate", &mut s.rotate, -3600.0, 3600.0),
    ] {
        if let Some(v) = m.get(key) {
            *slot = num32(v, &at(key), min, max, notes);
        }
    }
    // Only `weight` is read of a state's font, and only `color` of its icon.
    if let Some(v) = m.get("font") {
        let probe = Font::new(&[], 1.0, f32::NAN);
        let w = font(v, &probe, &at("font"), notes).weight;
        s.font_weight = (!w.is_nan()).then_some(w);
    }
    if let Some(v) = m.get("icon") {
        let probe = Icon {
            size: 1.0,
            stroke: None,
            color: None,
        };
        s.icon_color = icon(v, probe, &at("icon"), notes).color;
    }
    if let Some(v) = m.get("transition") {
        s.transition = transition(v, &at("transition"), notes);
    }
    Some(s)
}

// ---- Text, surfaces, controls, fields ------------------------------------------

/// A Text, onto `base`. `also`: keys of `v` that are other rows' business.
pub fn text(v: &ThemeValue, base: &Text, path: &str, also: &[&str], notes: &mut Notes) -> Text {
    let mut t = base.clone();
    let Some(m) = map(v, path, "a text style { color, font, shadow, margin }", notes) else {
        return t;
    };
    unknown_keys(m, path, &["color", "font", "shadow", "margin"], also, notes);
    if let Some(v) = m.get("color")
        && let Some(c) = color(v, &join(path, "color"), notes)
    {
        t.color = c;
    }
    if let Some(v) = m.get("font") {
        t.font = font(v, &t.font, &join(path, "font"), notes);
    }
    if let Some(v) = m.get("shadow")
        && let Some(s) = shadows(v, &join(path, "shadow"), notes)
    {
        t.shadow = s;
    }
    if let Some(v) = m.get("margin")
        && let Some(s) = spacing(v, &join(path, "margin"), notes)
    {
        t.margin = s;
    }
    t
}

const SURFACE_KEYS: [&str; 6] = ["background", "border", "shadow", "radius", "padding", "margin"];

/// What a surface, a control and a field share, checked onto the parts
/// given.
#[allow(clippy::too_many_arguments)]
fn boxed(
    m: &BTreeMap<String, ThemeValue>,
    path: &str,
    notes: &mut Notes,
    background: &mut Fill,
    border_: &mut Option<Border>,
    shadow_: &mut Vec<Shadow>,
    radius_: &mut Radius,
    padding: &mut Spacing,
    margin: Option<&mut Spacing>,
) {
    if let Some(v) = m.get("background")
        && let Some(f) = fill(v, &join(path, "background"), notes)
    {
        *background = f;
    }
    if let Some(v) = m.get("border")
        && let Some(b) = border(v, &join(path, "border"), notes)
    {
        *border_ = b;
    }
    if let Some(v) = m.get("shadow")
        && let Some(s) = shadows(v, &join(path, "shadow"), notes)
    {
        *shadow_ = s;
    }
    if let Some(v) = m.get("radius")
        && let Some(r) = radius(v, &join(path, "radius"), notes)
    {
        *radius_ = r;
    }
    if let Some(v) = m.get("padding")
        && let Some(s) = spacing(v, &join(path, "padding"), notes)
    {
        *padding = s;
    }
    if let Some(margin) = margin
        && let Some(v) = m.get("margin")
        && let Some(s) = spacing(v, &join(path, "margin"), notes)
    {
        *margin = s;
    }
}

/// A Surface, onto `base`.
pub fn surface(
    v: &ThemeValue,
    base: &Surface,
    path: &str,
    also: &[&str],
    notes: &mut Notes,
) -> Surface {
    let mut s = base.clone();
    let Some(m) = map(v, path, "a surface { background, border, shadow, … }", notes) else {
        return s;
    };
    unknown_keys(m, path, &SURFACE_KEYS, also, notes);
    boxed(
        m,
        path,
        notes,
        &mut s.background,
        &mut s.border,
        &mut s.shadow,
        &mut s.radius,
        &mut s.padding,
        Some(&mut s.margin),
    );
    s
}

const CONTROL_KEYS: [&str; 17] = [
    "background",
    "color",
    "border",
    "shadow",
    "radius",
    "font",
    "padding",
    "margin",
    "min_width",
    "min_height",
    "opacity",
    "icon",
    "transition",
    "hover",
    "active",
    "checked",
    "disabled",
];

/// A Control, onto `base`. `checked`: whether the element has that state
/// (the key classes don't: a toggle that is on uses `keys.on`).
pub fn control(
    v: &ThemeValue,
    base: &Control,
    path: &str,
    also: &[&str],
    checked: bool,
    notes: &mut Notes,
) -> Control {
    let mut c = base.clone();
    let Some(m) = map(v, path, "a control { background, color, font, … }", notes) else {
        return c;
    };
    unknown_keys(m, path, &CONTROL_KEYS, also, notes);
    boxed(
        m,
        path,
        notes,
        &mut c.background,
        &mut c.border,
        &mut c.shadow,
        &mut c.radius,
        &mut c.padding,
        Some(&mut c.margin),
    );
    if let Some(v) = m.get("color")
        && let Some(col) = color(v, &join(path, "color"), notes)
    {
        c.color = col;
    }
    if let Some(v) = m.get("font") {
        c.font = font(v, &c.font, &join(path, "font"), notes);
    }
    for (key, slot, max) in [
        ("min_width", &mut c.min_width, MAX_SPACE),
        ("min_height", &mut c.min_height, MAX_SPACE),
        ("opacity", &mut c.opacity, 1.0),
    ] {
        if let Some(v) = m.get(key)
            && let Some(n) = num32(v, &join(path, key), 0.0, max, notes)
        {
            *slot = n;
        }
    }
    if let Some(v) = m.get("icon") {
        c.icon = icon(v, c.icon, &join(path, "icon"), notes);
    }
    if let Some(v) = m.get("transition")
        && let Some(t) = transition(v, &join(path, "transition"), notes)
    {
        c.transition = t;
    }
    for (key, slot) in [
        ("hover", &mut c.hover),
        ("active", &mut c.active),
        ("disabled", &mut c.disabled),
    ] {
        if let Some(v) = m.get(key)
            && let Some(s) = state(v, &join(path, key), notes)
        {
            *slot = s;
        }
    }
    if let Some(v) = m.get("checked") {
        if !checked {
            notes.push(
                &join(path, "checked"),
                "this element has no `checked` state (a toggle key that is on uses `keys.on`)",
            );
        } else if let Some(s) = state(v, &join(path, "checked"), notes) {
            c.checked = s;
        }
    }
    c
}

const FIELD_KEYS: [&str; 13] = [
    "background",
    "color",
    "border",
    "shadow",
    "radius",
    "font",
    "padding",
    "min_height",
    "placeholder",
    "selection",
    "caret",
    "focus",
    "error",
];

/// A text field's style, onto `base`.
pub fn field(v: &ThemeValue, base: &Field, path: &str, also: &[&str], notes: &mut Notes) -> Field {
    let mut f = base.clone();
    let Some(m) = map(v, path, "a field { background, color, border, … }", notes) else {
        return f;
    };
    unknown_keys(m, path, &FIELD_KEYS, also, notes);
    boxed(
        m,
        path,
        notes,
        &mut f.background,
        &mut f.border,
        &mut f.shadow,
        &mut f.radius,
        &mut f.padding,
        None,
    );
    for (key, slot) in [
        ("color", &mut f.color),
        ("placeholder", &mut f.placeholder),
        ("selection", &mut f.selection),
        ("caret", &mut f.caret),
    ] {
        if let Some(v) = m.get(key)
            && let Some(c) = color(v, &join(path, key), notes)
        {
            *slot = c;
        }
    }
    if let Some(v) = m.get("font") {
        f.font = font(v, &f.font, &join(path, "font"), notes);
    }
    if let Some(v) = m.get("min_height")
        && let Some(n) = num32(v, &join(path, "min_height"), 0.0, MAX_SPACE, notes)
    {
        f.min_height = n;
    }
    for (key, slot) in [("focus", &mut f.focus), ("error", &mut f.error)] {
        if let Some(v) = m.get(key)
            && let Some(s) = state(v, &join(path, key), notes)
        {
            *slot = s;
        }
    }
    f
}
