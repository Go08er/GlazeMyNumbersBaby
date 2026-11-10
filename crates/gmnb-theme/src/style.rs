//! The resolved style (docs/theming.md, "Resolving a style"): `T0`, the
//! theme's tokens merged over it, `E0` worked out from those, the theme's
//! elements merged over that and checked, giving a typed [`Style`].

use std::collections::BTreeMap;

use crate::ThemeValue;
use crate::check::{Notes, join};
use crate::schema::{self, Base, Node, ROWS, Scope};
use crate::tokens::{Ctx, TOKEN_TABLES, Tokens};
use crate::types::*;

/// A checked style: the tokens, and every element's value by its path.
/// Widgets that draw themselves read it; the stylesheet's custom
/// properties are written from it ([`crate::css::properties`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub dark: bool,
    /// The Background opacity it was resolved for, 0.1 to 1.
    pub opacity: f32,
    pub tokens: Tokens,
    values: BTreeMap<&'static str, Value>,
}

/// A style, and what its theme's author should know.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub style: Style,
    pub notes: Notes,
}

impl Style {
    /// The base look for `ctx`: what a theme returning `{}` resolves to.
    pub fn base(ctx: &Ctx) -> Style {
        resolve(ctx, &ThemeValue::Map(BTreeMap::new())).style
    }

    /// The value at a path of the schema; `None` for a path that isn't
    /// one, or whose row has no base value yet.
    pub fn get(&self, path: &str) -> Option<&Value> {
        self.values.get(path)
    }

    /// Every resolved value, by path.
    pub fn values(&self) -> impl Iterator<Item = (&'static str, &Value)> {
        self.values.iter().map(|(k, v)| (*k, v))
    }

    /// A number (pixels, milliseconds or plain); 0 if there is none.
    pub fn number(&self, path: &str) -> f32 {
        match self.get(path) {
            Some(Value::Number(n)) => *n as f32,
            _ => 0.0,
        }
    }

    pub fn flag(&self, path: &str) -> bool {
        matches!(self.get(path), Some(Value::Bool(true)))
    }

    pub fn color(&self, path: &str) -> Rgba {
        match self.get(path) {
            Some(Value::Color(c)) => *c,
            _ => TRANSPARENT,
        }
    }

    pub fn fill(&self, path: &str) -> Fill {
        match self.get(path) {
            Some(Value::Fill(f)) => f.clone(),
            _ => Fill::none(),
        }
    }

    pub fn radius(&self, path: &str) -> Radius {
        match self.get(path) {
            Some(Value::Radius(r)) => *r,
            _ => Radius::all(0.0),
        }
    }

    pub fn shadows(&self, path: &str) -> &[Shadow] {
        match self.get(path) {
            Some(Value::Shadows(s)) => s,
            _ => &[],
        }
    }

    pub fn control(&self, path: &str) -> Option<&Control> {
        match self.get(path) {
            Some(Value::Control(c)) => Some(c),
            _ => None,
        }
    }

    pub fn paint(&self, path: &str) -> Option<&Paint> {
        match self.get(path) {
            Some(Value::Paint(p)) => Some(p),
            _ => None,
        }
    }

    pub fn font(&self, path: &str) -> Option<&Font> {
        match self.get(path) {
            Some(Value::Font(f)) => Some(f),
            _ => None,
        }
    }

    pub fn text(&self, path: &str) -> Option<&Text> {
        match self.get(path) {
            Some(Value::Text(t)) => Some(t),
            _ => None,
        }
    }

    /// One part of a table of named parts (`display.sizes`, `standard`).
    pub fn part(&self, path: &str, name: &str) -> Option<&Value> {
        match self.get(path) {
            Some(Value::Record(parts)) => parts.get(name),
            _ => None,
        }
    }

    pub fn icon(&self, path: &str) -> Option<Icon> {
        match self.get(path) {
            Some(Value::Icon(i)) => Some(*i),
            _ => None,
        }
    }

    /// The toolkit-drawn families the theme handed back (`native`).
    pub fn native(&self) -> &[&'static str] {
        match self.get("native") {
            Some(Value::Words(w)) => w,
            _ => &[],
        }
    }

    /// The line width of an icon drawn at `size` pixels: `icons.stroke` on
    /// the 24-unit grid, scaled by `icons.taper`. Taper 0, the base look's,
    /// is DGMNB's line: it scales the grid's line with the icon, and then
    /// once more, by the same factor (`ui.rs` `icon`: its stroke width is
    /// in the path's units, which the icon's transform scales).
    pub fn icon_stroke(&self, size: f32) -> f32 {
        let stroke = self.number("icons.stroke");
        let taper = self.number("icons.taper");
        let k = size / appcore::metrics::shape::ICON_GRID;
        if taper == 0.0 {
            stroke * k * k
        } else {
            stroke * (taper + (1.0 - taper) * k)
        }
    }
}

/// What a theme gave at `path`, if anything.
fn lookup<'a>(style: &'a ThemeValue, path: &str) -> Option<&'a ThemeValue> {
    let mut v = style;
    for part in path.split('.') {
        v = v.get(part)?;
    }
    Some(v)
}

/// Resolves what a theme's `style` returned (`{}` for no theme) for `ctx`.
/// Nothing a theme returns makes this fail: what is wrong is noted and
/// its base value stays.
pub fn resolve(ctx: &Ctx, theme: &ThemeValue) -> Resolved {
    let mut notes = Notes::default();
    let empty = ThemeValue::Map(BTreeMap::new());
    let theme = match theme {
        ThemeValue::Map(_) => theme,
        other => {
            notes.push(
                "style",
                format!(
                    "expected `style` to return a table, got {}",
                    other.type_name()
                ),
            );
            &empty
        }
    };
    let tokens = Tokens::merged(ctx, theme, &mut notes);
    let mut values: BTreeMap<&'static str, Value> = BTreeMap::new();
    for row in ROWS {
        let given = lookup(theme, row.path);
        let also = schema::child_names(row.path);
        match row.base {
            Base::Is(base) => {
                let base = base(&Scope {
                    t: &tokens,
                    ctx,
                    done: &values,
                });
                let value = match given {
                    Some(v) => schema::check(&row.kind, v, &base, row.path, &also, &mut notes),
                    None => base,
                };
                values.insert(row.path, value);
            }
            // Accepted and checked, but not resolved yet.
            Base::Todo => {
                if let Some(v) = given {
                    let onto = schema::neutral(&row.kind);
                    schema::check(&row.kind, v, &onto, row.path, &also, &mut notes);
                }
            }
        }
    }
    unknown(schema::tree(), theme, "", &mut notes);
    Resolved {
        style: Style {
            dark: ctx.dark,
            opacity: ctx.opacity(),
            tokens,
            values,
        },
        notes,
    }
}

/// Notes the keys no row accounts for, under the nodes that are only
/// parents (a row's own check notes the keys of its value).
fn unknown(node: &Node, given: &ThemeValue, path: &str, notes: &mut Notes) {
    if node.children.is_empty() {
        return;
    }
    let ThemeValue::Map(m) = given else {
        if node.row.is_none() {
            notes.push(path, format!("expected a table, got {}", given.type_name()));
        }
        return;
    };
    for (key, value) in m {
        match node.children.get(key.as_str()) {
            Some(child) => unknown(child, value, &join(path, key), notes),
            None if node.row.is_some() => {}
            None if path.is_empty() && TOKEN_TABLES.contains(&key.as_str()) => {}
            None => notes.push(&join(path, key), "unknown key"),
        }
    }
}
