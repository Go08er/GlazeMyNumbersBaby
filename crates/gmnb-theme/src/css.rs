//! The generated style: typed values written as CSS (docs/theming.md, "How
//! style reaches the app").
//!
//! Every value is written from typed data: numbers with their units,
//! colours as `rgb()`/`rgba()`, family names as quoted strings with `"`
//! and `\` escaped (names with control characters were refused by the
//! checks), tags and keywords from fixed sets. Nothing a theme returned
//! is ever copied into the stylesheet as text.

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::schema::{Kind, ROWS};
use crate::states::{COMBOS, FIELD_COMBOS, Finished};
use crate::style::Style;
use crate::types::*;

/// The prefix of the app's custom properties.
pub const PREFIX: &str = "--gmnb-";

/// The custom property of a style path (and a suffix, if the value at the
/// path expands into several): `keys.number.min_height` →
/// `--gmnb-keys-number-min-height`.
pub fn property(path: &str, suffix: &str) -> String {
    let mut name = String::with_capacity(PREFIX.len() + path.len() + suffix.len() + 1);
    name.push_str(PREFIX);
    for c in path.chars() {
        name.push(if c == '.' || c == '_' { '-' } else { c });
    }
    if !suffix.is_empty() {
        name.push('-');
        name.push_str(suffix);
    }
    name
}

// ---- Values ------------------------------------------------------------------

/// A number: at most three decimals, no trailing zeros, never `NaN`.
pub fn num(n: f32) -> String {
    if !n.is_finite() {
        return "0".into();
    }
    let rounded = (f64::from(n) * 1000.0).round() / 1000.0;
    let mut s = format!("{rounded:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" { "0".into() } else { s }
}

/// A length. (Zero has its unit too: `calc()` needs it.)
pub fn px(n: f32) -> String {
    format!("{}px", num(n))
}

pub fn color(c: Rgba) -> String {
    let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (r, g, b) = (ch(c[0]), ch(c[1]), ch(c[2]));
    let a = if c[3].is_finite() {
        c[3].clamp(0.0, 1.0)
    } else {
        1.0
    };
    if a >= 0.9995 {
        format!("rgb({r},{g},{b})")
    } else {
        format!("rgba({r},{g},{b},{})", num(a))
    }
}

fn percent(n: f32) -> String {
    format!("{}%", num(n * 100.0))
}

fn gradient(g: &Gradient) -> String {
    let mut s = match g.kind {
        GradientKind::Linear => format!("linear-gradient({}deg", num(g.angle)),
        GradientKind::Radial => format!(
            "radial-gradient({} at {} {}",
            g.extent.name(),
            percent(g.at[0]),
            percent(g.at[1])
        ),
        GradientKind::Conic => format!(
            "conic-gradient(from {}deg at {} {}",
            num(g.angle),
            percent(g.at[0]),
            percent(g.at[1])
        ),
    };
    for (c, stop) in g.colors.iter().zip(&g.stops) {
        let _ = write!(s, ", {} {}", color(*c), percent(*stop));
    }
    s.push(')');
    s
}

/// A paint as one `background-image` layer (a colour as a flat gradient).
fn image_layer(p: &Paint) -> String {
    match p {
        Paint::Color(c) => {
            let c = color(*c);
            format!("linear-gradient({c}, {c})")
        }
        Paint::Gradient(g) => gradient(g),
    }
}

/// A fill as `background-color` (the solid bottom layer, or
/// `transparent`) and `background-image` (the layers above it, or `none`).
/// `none` is never written inside a list: the toolkit then drops the
/// gradient beside it.
pub fn fill(f: &Fill) -> (String, String) {
    let (bottom, above) = match f.0.split_last() {
        Some((Paint::Color(c), above)) => (color(*c), above),
        _ => ("transparent".to_string(), f.0.as_slice()),
    };
    let layers: Vec<String> = above.iter().map(image_layer).collect();
    let image = if layers.is_empty() {
        "none".to_string()
    } else {
        layers.join(", ")
    };
    (bottom, image)
}

/// A border and a shadow list as one `box-shadow`: the border first, as
/// an inset ring, then the shadows; `none` if there is neither.
pub fn box_shadow(border: Option<&Border>, shadows: &[Shadow]) -> String {
    let mut parts = Vec::with_capacity(shadows.len() + 1);
    if let Some(b) = border.filter(|b| b.width > 0.0) {
        parts.push(format!("inset 0 0 0 {} {}", px(b.width), color(b.color)));
    }
    for s in shadows {
        parts.push(format!(
            "{}{} {} {} {} {}",
            if s.inset { "inset " } else { "" },
            px(s.x),
            px(s.y),
            px(s.blur),
            px(s.spread),
            color(s.color)
        ));
    }
    if parts.is_empty() {
        "none".into()
    } else {
        parts.join(", ")
    }
}

/// A shadow list as `text-shadow` (which has no spread and no inset).
pub fn text_shadow(shadows: &[Shadow]) -> String {
    let parts: Vec<String> = shadows
        .iter()
        .filter(|s| !s.inset)
        .map(|s| format!("{} {} {} {}", px(s.x), px(s.y), px(s.blur), color(s.color)))
        .collect();
    if parts.is_empty() {
        "none".into()
    } else {
        parts.join(", ")
    }
}

pub fn radius(r: &Radius) -> String {
    let [a, b, c, d] = r.0;
    if a == b && b == c && c == d {
        px(a)
    } else {
        format!("{} {} {} {}", px(a), px(b), px(c), px(d))
    }
}

pub fn spacing(s: &Spacing) -> String {
    format!(
        "{} {} {} {}",
        px(s.top),
        px(s.right),
        px(s.bottom),
        px(s.left)
    )
}

/// A family name as a quoted string. `None` for a name that can't be
/// written safely (control characters): the checks refuse those, and this
/// refuses them again.
pub fn quote_family(name: &str) -> Option<String> {
    if !crate::check::family_ok(name) {
        return None;
    }
    let mut s = String::with_capacity(name.len() + 2);
    s.push('"');
    for c in name.chars() {
        if c == '"' || c == '\\' {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('"');
    Some(s)
}

pub fn font_family(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().filter_map(|n| quote_family(n)).collect();
    if quoted.is_empty() {
        "\"sans-serif\"".into()
    } else {
        quoted.join(", ")
    }
}

/// A four-character OpenType tag, quoted; `None` if it isn't one.
fn quote_tag(tag: &str) -> Option<String> {
    (tag.len() == 4 && tag.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| format!("\"{tag}\""))
}

pub fn font_features(f: &Font) -> String {
    let parts: Vec<String> = f
        .features
        .iter()
        .filter_map(|(tag, on)| Some(format!("{} {}", quote_tag(tag)?, on.min(&99))))
        .collect();
    if parts.is_empty() {
        "normal".into()
    } else {
        parts.join(", ")
    }
}

pub fn font_variations(f: &Font) -> String {
    let parts: Vec<String> = f
        .effective_variations()
        .iter()
        .filter_map(|(tag, v)| Some(format!("{} {}", quote_tag(tag)?, num(*v))))
        .collect();
    if parts.is_empty() {
        "normal".into()
    } else {
        parts.join(", ")
    }
}

fn ease(e: Ease) -> String {
    match e {
        Ease::Linear => "linear".into(),
        Ease::Ease => "ease".into(),
        Ease::EaseIn => "ease-in".into(),
        Ease::EaseOut => "ease-out".into(),
        Ease::EaseInOut => "ease-in-out".into(),
        Ease::Spring => "cubic-bezier(0.34, 1.56, 0.64, 1)".into(),
        Ease::Bezier([a, b, c, d]) => {
            format!(
                "cubic-bezier({}, {}, {}, {})",
                num(a),
                num(b),
                num(c),
                num(d)
            )
        }
    }
}

/// A complete `transition` value, or `none`.
pub fn transition(t: &Transition) -> String {
    let mut parts = Vec::new();
    for (prop, ms, e) in &t.0 {
        let names: &[&str] = match prop {
            TransitionProp::Background => &["background-color", "background-image"],
            TransitionProp::Color => &["color"],
            TransitionProp::Shadow => &["box-shadow"],
            TransitionProp::Transform => &["transform"],
            TransitionProp::Filter => &["filter"],
            TransitionProp::Opacity => &["opacity"],
        };
        for name in names {
            parts.push(format!("{name} {}ms {}", num(*ms), ease(*e)));
        }
    }
    if parts.is_empty() {
        "none".into()
    } else {
        parts.join(", ")
    }
}

/// `brightness(b) saturate(s)`, or `none`.
pub fn filter(brightness: f32, saturation: f32) -> String {
    let mut parts = Vec::new();
    if brightness != 1.0 {
        parts.push(format!("brightness({})", num(brightness)));
    }
    if saturation != 1.0 {
        parts.push(format!("saturate({})", num(saturation)));
    }
    if parts.is_empty() {
        "none".into()
    } else {
        parts.join(" ")
    }
}

/// `scale(s) rotate(d)`, or `none`.
pub fn transform(scale: f32, rotate: f32) -> String {
    let mut parts = Vec::new();
    if scale != 1.0 {
        parts.push(format!("scale({})", num(scale)));
    }
    if rotate != 0.0 {
        parts.push(format!("rotate({}deg)", num(rotate)));
    }
    if parts.is_empty() {
        "none".into()
    } else {
        parts.join(" ")
    }
}

// ---- Properties -----------------------------------------------------------------

/// The custom properties of a whole style: every row with a base value,
/// expanded as its kind says. The writer ([`crate::writer`]) then keeps
/// the ones the stylesheet reads.
pub fn properties(style: &Style) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for row in ROWS {
        if let Some(value) = style.get(row.path) {
            emit(row.path, &row.kind, value, &mut out);
        }
    }
    out
}

fn put(out: &mut BTreeMap<String, String>, path: &str, suffix: &str, value: String) {
    out.insert(property(path, suffix), value);
}

fn emit_fill(out: &mut BTreeMap<String, String>, path: &str, lead: &str, f: &Fill) {
    let (c, image) = fill(f);
    put(out, path, &format!("{lead}color"), c);
    put(out, path, &format!("{lead}image"), image);
}

/// A font's properties, named after the element at `path` that it is the
/// font of.
fn emit_font(out: &mut BTreeMap<String, String>, path: &str, f: &Font) {
    put(out, path, "font-family", font_family(&f.family));
    put(out, path, "font-size", px(f.size));
    put(out, path, "font-weight", num(f.weight.round()));
    let style = if f.italic { "italic" } else { "normal" };
    put(out, path, "font-style", style.into());
    put(out, path, "letter-spacing", format!("{}em", num(f.spacing)));
    put(out, path, "font-features", font_features(f));
    put(out, path, "font-variations", font_variations(f));
}

/// The ten properties a state combination is written as (and a control at
/// rest, with no `lead`).
fn emit_finished(out: &mut BTreeMap<String, String>, path: &str, lead: &str, f: &Finished) {
    emit_fill(out, path, &format!("{lead}background-"), &f.background);
    put(out, path, &format!("{lead}color"), color(f.color));
    put(out, path, &format!("{lead}icon-color"), color(f.icon_color));
    put(
        out,
        path,
        &format!("{lead}shadow"),
        box_shadow(f.border.as_ref(), &f.shadow),
    );
    put(out, path, &format!("{lead}opacity"), num(f.opacity));
    put(
        out,
        path,
        &format!("{lead}filter"),
        filter(f.brightness, f.saturation),
    );
    put(
        out,
        path,
        &format!("{lead}transform"),
        transform(f.scale, f.rotate),
    );
    put(
        out,
        path,
        &format!("{lead}transition"),
        transition(&f.transition),
    );
    put(
        out,
        path,
        &format!("{lead}font-weight"),
        num(f.font_weight.round()),
    );
}

fn emit_control(out: &mut BTreeMap<String, String>, path: &str, c: &Control) {
    emit_finished(out, path, "", &c.finished(&[]));
    emit_font(out, path, &c.font);
    put(out, path, "radius", radius(&c.radius));
    put(out, path, "padding", spacing(&c.padding));
    put(out, path, "margin", spacing(&c.margin));
    put(out, path, "min-width", px(c.min_width));
    put(out, path, "min-height", px(c.min_height));
    // Every combination is always written, equal to the resting value
    // where the state changes nothing: the stylesheet names them without
    // a fallback.
    for (name, states) in COMBOS {
        emit_finished(out, path, &format!("{name}-"), &c.finished(states));
    }
}

/// A text field: its resting values as a control's, its placeholder,
/// selection and caret colours, and `focus`, `error` and `error-focus` in
/// place of a control's combinations.
fn emit_field(out: &mut BTreeMap<String, String>, path: &str, f: &Field) {
    emit_finished(out, path, "", &f.finished(&[]));
    emit_font(out, path, &f.font);
    put(out, path, "radius", radius(&f.radius));
    put(out, path, "padding", spacing(&f.padding));
    put(out, path, "min-height", px(f.min_height));
    put(out, path, "placeholder", color(f.placeholder));
    put(out, path, "selection", color(f.selection));
    put(out, path, "caret", color(f.caret));
    for (name, states) in FIELD_COMBOS {
        emit_finished(out, path, &format!("{name}-"), &f.finished(states));
    }
}

/// Writes the custom properties of `value`, of `kind`, at `path`. Values
/// only Rust reads (booleans, words, standalone states, icons, paints)
/// write nothing.
pub fn emit(path: &str, kind: &Kind, value: &Value, out: &mut BTreeMap<String, String>) {
    match (kind, value) {
        (Kind::Px(..), Value::Number(n)) => put(out, path, "", px(*n as f32)),
        (Kind::Num(..), Value::Number(n)) => put(out, path, "", num(*n as f32)),
        (Kind::Ms, Value::Number(n)) => put(out, path, "", format!("{}ms", num(*n as f32))),
        (_, Value::Color(c)) => put(out, path, "", color(*c)),
        (Kind::Opt(inner), Value::Absent) => match inner {
            Kind::Color => put(out, path, "", "transparent".into()),
            Kind::Border => put(out, path, "", "none".into()),
            _ => {}
        },
        (Kind::Opt(inner), present) => emit(path, inner, present, out),
        (_, Value::Fill(f)) => emit_fill(out, path, "", f),
        (Kind::TextShadows, Value::Shadows(s)) => put(out, path, "", text_shadow(s)),
        (_, Value::Shadows(s)) => put(out, path, "", box_shadow(None, s)),
        (_, Value::Border(b)) => put(out, path, "", box_shadow(Some(b), &[])),
        (_, Value::Radius(r)) => put(out, path, "", radius(r)),
        (_, Value::Spacing(s)) => put(out, path, "", spacing(s)),
        (_, Value::Font(f)) => {
            // `window.font` is the window's font: `--gmnb-window-font-size`.
            let element = path.strip_suffix(".font").unwrap_or(path);
            emit_font(out, element, f);
        }
        (_, Value::Transition(t)) => put(out, path, "", transition(t)),
        (_, Value::FocusRing(r)) => {
            put(out, path, "width", px(r.width));
            put(out, path, "color", color(r.color));
            put(out, path, "offset", px(r.offset));
        }
        (_, Value::Text(t)) => {
            put(out, path, "color", color(t.color));
            emit_font(out, path, &t.font);
            put(out, path, "text-shadow", text_shadow(&t.shadow));
            put(out, path, "margin", spacing(&t.margin));
        }
        (_, Value::Surface(s)) => {
            emit_fill(out, path, "background-", &s.background);
            put(
                out,
                path,
                "shadow",
                box_shadow(s.border.as_ref(), &s.shadow),
            );
            put(out, path, "radius", radius(&s.radius));
            put(out, path, "padding", spacing(&s.padding));
            put(out, path, "margin", spacing(&s.margin));
        }
        (_, Value::Control(c)) => emit_control(out, path, c),
        (_, Value::Field(f)) => emit_field(out, path, f),
        (Kind::Record(parts), Value::Record(fields)) => {
            for (name, k) in *parts {
                if let Some(v) = fields.get(name) {
                    emit(&format!("{path}.{name}"), k, v, out);
                }
            }
        }
        _ => {}
    }
}

// ---- The toolkit's own colours ------------------------------------------------

/// `stock`'s tokens and the libadwaita colours each becomes.
const STOCK: [(&str, &[&str]); 13] = [
    ("stock.window", &["window_bg_color", "overview_bg_color"]),
    ("stock.view", &["view_bg_color"]),
    (
        "stock.headerbar",
        &["headerbar_bg_color", "headerbar_backdrop_color"],
    ),
    (
        "stock.sidebar",
        &["sidebar_bg_color", "secondary_sidebar_bg_color"],
    ),
    ("stock.dialog", &["dialog_bg_color"]),
    ("stock.popover", &["popover_bg_color"]),
    ("stock.card", &["card_bg_color"]),
    (
        "stock.fg",
        &[
            "window_fg_color",
            "overview_fg_color",
            "view_fg_color",
            "headerbar_fg_color",
            "sidebar_fg_color",
            "secondary_sidebar_fg_color",
            "dialog_fg_color",
            "popover_fg_color",
            "card_fg_color",
        ],
    ),
    ("stock.accent", &["accent_color"]),
    ("stock.accent_bg", &["accent_bg_color"]),
    ("stock.accent_fg", &["accent_fg_color"]),
    (
        "stock.destructive",
        &["destructive_color", "destructive_bg_color"],
    ),
    ("stock.error", &["error_color", "error_bg_color"]),
];

/// The toolkit's own named colours, which the app defines in full so the
/// desktop's `gtk.css` cannot repaint it: as `@define-color` lines (for
/// the main provider's preamble) and as libadwaita's variables (to go
/// with the style's properties).
pub fn stock(style: &Style) -> (String, BTreeMap<String, String>) {
    let mut defines = String::new();
    let mut vars = BTreeMap::new();
    for (path, names) in STOCK {
        let value = color(style.color(path));
        for name in names {
            let _ = writeln!(defines, "@define-color {name} {value};");
            vars.insert(format!("--{}", name.replace('_', "-")), value.clone());
        }
    }
    (defines, vars)
}
