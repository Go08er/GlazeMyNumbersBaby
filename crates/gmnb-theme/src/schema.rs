//! The schema: one table of every path a style has (docs/theming.md,
//! "Reference: every element"), what kind of value sits there, and how its
//! base value derives from the tokens.
//!
//! # Adding a surface
//!
//! A row is `row(path, kind, base)`: the base is a function of the
//! [`Scope`] (the merged tokens, `ctx`, and the rows resolved before it).
//! Everything else follows from the kind: the checks, the merge with what
//! a theme returns, and the custom properties written
//! ([`crate::css::emit`]). Rows marked `todo(path, kind)` are accepted (a
//! theme returning them is checked, not warned about) but have no base
//! value yet: they resolve to nothing and emit nothing. To take one over,
//! give it a base, and read its properties in `apps/gmnb/src/style.css`
//! (`tools/theming/control_css.py` writes a control's rules) or from
//! [`crate::Style`] in Rust.
//!
//! Base values come from `appcore::metrics` and the tokens, never from
//! numbers copied here: DGMNB draws with the same constants.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use appcore::metrics;

use crate::ThemeValue;
use crate::check::{self, Notes, join};
use crate::tokens::{Ctx, Tokens};
use crate::types::*;

/// The kind of value at a path: how it is checked, merged and written.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Bool,
    /// Pixels, within a range.
    Px(f64, f64),
    /// A plain number, within a range.
    Num(f64, f64),
    /// Milliseconds, 0 to 5000.
    Ms,
    Color,
    Paint,
    Fill,
    /// A box shadow list.
    Shadows,
    /// A text shadow list (no spread, never inset).
    TextShadows,
    Radius,
    Spacing,
    Font,
    Transition,
    State,
    Icon,
    FocusRing,
    Text,
    Surface,
    Control,
    /// A key class: a control without `checked` (a toggle that is on uses
    /// `keys.on`).
    KeyControl,
    Field,
    /// One of these words.
    Word(&'static [&'static str]),
    /// A list of these words.
    Words(&'static [&'static str]),
    /// `{ dash, gap }`.
    Dash,
    /// Pixels, or this word.
    PxOr(&'static str, f64, f64),
    /// A colour, or this word.
    ColorOr(&'static str),
    /// A border.
    Border,
    /// The kind, or `false` for none.
    Opt(&'static Kind),
    /// A table of named parts.
    Record(&'static [(&'static str, Kind)]),
}

/// A row's base value.
#[derive(Clone, Copy)]
pub enum Base {
    /// Not taken over yet: see the module docs.
    Todo,
    Is(fn(&Scope) -> Value),
}

pub struct Row {
    pub path: &'static str,
    pub kind: Kind,
    pub base: Base,
}

const fn row(path: &'static str, kind: Kind, base: fn(&Scope) -> Value) -> Row {
    Row {
        path,
        kind,
        base: Base::Is(base),
    }
}

const fn todo(path: &'static str, kind: Kind) -> Row {
    Row {
        path,
        kind,
        base: Base::Todo,
    }
}

/// What a base value is worked out from.
pub struct Scope<'a> {
    /// `T`: the tokens, the theme's merged over the base ones.
    pub t: &'a Tokens,
    pub ctx: &'a Ctx,
    /// The rows above this one, resolved (the theme's values merged in).
    pub done: &'a BTreeMap<&'static str, Value>,
}

impl Scope<'_> {
    fn get(&self, path: &str) -> &Value {
        self.done.get(path).unwrap_or(&Value::Absent)
    }

    fn state(&self, path: &str) -> State {
        match self.get(path) {
            Value::State(s) => s.clone(),
            _ => State::default(),
        }
    }

    fn px(&self, path: &str) -> f32 {
        match self.get(path) {
            Value::Number(n) => *n as f32,
            _ => 0.0,
        }
    }

    /// The scrim under text on a see-through window, for the merged
    /// colours.
    fn text_scrim(&self) -> Rgba {
        self.t.colors.text_scrim(self.ctx.opacity())
    }

    /// The halo around text on a see-through window; none at full opacity.
    fn halo(&self) -> Vec<Shadow> {
        if !self.ctx.see_through() {
            return Vec::new();
        }
        let ring = |blur: f32, a: f32| Shadow {
            x: 0.0,
            y: 0.0,
            blur,
            spread: 0.0,
            color: alpha(self.t.colors.bg, a),
            inset: false,
        };
        vec![ring(3.0, 0.9), ring(10.0, 0.75)]
    }

    /// The reference's Control before a row says otherwise: nothing drawn
    /// but its text in `font`, with the base states.
    fn control(&self, font: &Font) -> Control {
        let c = &self.t.colors;
        Control {
            background: Fill::none(),
            color: c.fg,
            border: None,
            shadow: Vec::new(),
            radius: Radius::all(self.t.shape.radius),
            font: font.clone(),
            padding: Spacing::default(),
            margin: Spacing::default(),
            min_width: 0.0,
            min_height: 0.0,
            opacity: 1.0,
            icon: Icon {
                size: metrics::controls::ICON_BUTTON_ICON,
                stroke: None,
                color: None,
            },
            transition: Transition::default(),
            hover: State::overlay(c.hover),
            active: State::overlay(c.press),
            checked: State::default(),
            disabled: State {
                color: Some(c.fg_faint),
                ..State::default()
            },
        }
    }

    /// A resolved control above this row (`icon_button` for the swap and
    /// the graph's buttons): a theme's changes to it carry over.
    fn control_at(&self, path: &str) -> Control {
        match self.get(path) {
            Value::Control(c) => (**c).clone(),
            _ => self.control(&self.t.text.body),
        }
    }

    /// `icon_button` at `size` × `size`.
    fn icon_button(&self, size: f32) -> Value {
        Value::Control(Box::new(Control {
            min_width: size,
            min_height: size,
            ..self.control_at("icon_button")
        }))
    }

    /// Text in `font` and `color` straight on the backdrop: it has the
    /// window's halo (none at full opacity), and no margin.
    fn text(&self, font: &Font, color: Rgba) -> Value {
        self.text_with(font, color, "window.text_halo")
    }

    /// Text on something raised (a popover, a card): the raised halo.
    fn raised_text(&self, font: &Font, color: Rgba) -> Value {
        self.text_with(font, color, "window.raised_text_halo")
    }

    fn text_with(&self, font: &Font, color: Rgba, halo: &str) -> Value {
        let shadow = match self.get(halo) {
            Value::Shadows(s) => s.clone(),
            _ => Vec::new(),
        };
        Value::Text(Text {
            color,
            font: font.clone(),
            shadow,
            margin: Spacing::default(),
        })
    }

    /// `font` at another size and weight (a page's own text style).
    fn sized(&self, style: metrics::TextStyle) -> Font {
        Font {
            size: style.size,
            weight: style.weight,
            ..self.t.text.body.clone()
        }
    }

    /// Nothing drawn: no background, border, radius or padding.
    fn bare(&self) -> Surface {
        Surface {
            background: Fill::none(),
            border: None,
            shadow: Vec::new(),
            radius: Radius::all(0.0),
            padding: Spacing::default(),
            margin: Spacing::default(),
        }
    }

    /// A raised card's soft shadow: DGMNB's three translucent rings.
    fn rings(&self) -> Vec<Shadow> {
        metrics::shape::POPOVER_SHADOW
            .iter()
            .map(|&(spread, a)| Shadow {
                x: 0.0,
                y: 0.0,
                blur: 0.0,
                spread,
                color: alpha(BLACK, a),
                inset: false,
            })
            .collect()
    }

    fn line(&self) -> Option<Border> {
        Some(Border {
            width: metrics::shape::BORDER,
            color: self.t.colors.border,
        })
    }

    /// A key class: `keys`' shared values, with its own fill, text colour
    /// and font.
    fn key(&self, background: Rgba, color: Rgba, font: Font) -> Value {
        let radius = match self.get("keys.radius") {
            Value::Radius(r) => *r,
            _ => Radius::all(self.t.shape.radius),
        };
        let padding = match self.get("keys.padding") {
            Value::Spacing(s) => *s,
            _ => Spacing::default(),
        };
        let transition = match self.get("keys.transition") {
            Value::Transition(t) => t.clone(),
            _ => Transition::default(),
        };
        let icon = match self.get("keys.icon") {
            Value::Icon(i) => *i,
            _ => Icon {
                size: metrics::controls::KEY_ICON,
                stroke: None,
                color: None,
            },
        };
        Value::Control(Box::new(Control {
            background: Fill::color(background),
            color,
            radius,
            padding,
            min_width: self.px("keys.min_width"),
            min_height: self.px("keys.min_height"),
            icon,
            transition,
            // A class's own state replaces the shared one for that class.
            hover: self.state("keys.hover"),
            active: self.state("keys.active"),
            disabled: self.state("keys.disabled"),
            // A toggle that is on.
            checked: self.state("keys.on"),
            ..self.control(&font)
        }))
    }
}

fn color(c: Rgba) -> Value {
    Value::Color(c)
}

fn px(n: f32) -> Value {
    Value::Number(f64::from(n))
}

/// One number for each of the calculator's modes a readout is sized for.
fn modes(standard: f32, scientific: f32, programmer: f32, compact: f32) -> Value {
    Value::Record(
        [
            ("standard", px(standard)),
            ("scientific", px(scientific)),
            ("programmer", px(programmer)),
            ("compact", px(compact)),
        ]
        .into(),
    )
}

fn spacing(top: f32, right: f32, bottom: f32, left: f32) -> Value {
    Value::Spacing(Spacing::trbl(top, right, bottom, left))
}

fn record(parts: &[(&'static str, Value)]) -> Value {
    Value::Record(parts.iter().cloned().collect())
}

// ---- Kinds the rows share --------------------------------------------------

const SIZE: Kind = Kind::Px(0.0, 4096.0);
const FONT_SIZE: Kind = Kind::Px(1.0, 512.0);
const UNIT: Kind = Kind::Num(0.0, 1.0);
const REVEAL: Kind = Kind::Word(&["always", "hover"]);
const HEIGHT: Kind = Kind::PxOr("auto", 0.0, 4096.0);
const SIZE_FONT: Kind = Kind::Record(&[("size", FONT_SIZE), ("font", Kind::Font)]);
const SLIDER_TRACK: Kind =
    Kind::Record(&[("height", SIZE), ("color", Kind::Color), ("radius", SIZE)]);
const SLIDER_KNOB: Kind = Kind::Record(&[
    ("size", SIZE),
    ("color", Kind::Color),
    ("halo", Kind::Color),
    ("halo_size", SIZE),
]);
const SWITCH: Kind = Kind::Record(&[
    ("width", SIZE),
    ("height", SIZE),
    ("knob_size", SIZE),
    ("off", Kind::Color),
    ("on", Kind::Color),
    ("knob", Kind::Color),
    ("knob_on", Kind::Color),
]);
const TOAST: Kind = Kind::Record(&[
    ("background", Kind::Fill),
    ("color", Kind::Color),
    ("radius", Kind::Radius),
    ("font", Kind::Font),
    ("min_height", SIZE),
    ("padding", Kind::Spacing),
]);
const TOOLTIP: Kind = Kind::Record(&[
    ("background", Kind::Fill),
    ("color", Kind::Color),
    ("radius", Kind::Radius),
    ("font", Kind::Font),
    ("padding", Kind::Spacing),
]);
const KEYS_PAD: Kind = Kind::Record(&[
    ("gap", SIZE),
    ("radius", Kind::Radius),
    ("min_height", SIZE),
    ("font", Kind::Font),
]);
/// A radius that rounds a control's ends fully (CSS shrinks it to half the
/// shorter side).
const ROUND: f32 = 999.0;

/// The toolkit's own widget families a theme can hand back (`native`).
pub const NATIVE_FAMILIES: [&str; 11] = [
    "segmented",
    "field",
    "slider",
    "switch",
    "scrollbar",
    "menu",
    "toast",
    "tooltip",
    "button",
    "card",
    "focus",
];

/// The calculator's modes a readout is sized for.
const MODES_HEIGHT: Kind = Kind::Record(&[
    ("standard", HEIGHT),
    ("scientific", HEIGHT),
    ("programmer", HEIGHT),
    ("compact", HEIGHT),
]);
const MODES_SIZE: Kind = Kind::Record(&[
    ("standard", FONT_SIZE),
    ("scientific", FONT_SIZE),
    ("programmer", FONT_SIZE),
    ("compact", FONT_SIZE),
]);

// ---- The table ---------------------------------------------------------------

/// Every path of a style, in the order they are resolved: a row may read
/// the rows above it.
pub static ROWS: &[Row] = &[
    // -- Tokens that derive nothing: icons, motion, layout.
    row("icons.stroke", Kind::Num(0.0, 64.0), |_| {
        px(metrics::shape::ICON_STROKE)
    }),
    row("icons.taper", UNIT, |_| px(0.0)),
    row("motion.page", Kind::Ms, |_| px(0.0)),
    row("motion.mode", Kind::Ms, |_| px(0.0)),
    row("motion.panel", Kind::Ms, |_| px(0.0)),
    row("motion.flip", Kind::Ms, |_| px(0.0)),
    row("motion.graph_view", Kind::Ms, |_| px(0.0)),
    row("motion.graph_side", Kind::Ms, |_| px(0.0)),
    row("motion.date", Kind::Ms, |_| px(0.0)),
    row("motion.history_row", Kind::Ms, |_| px(0.0)),
    row("motion.equation_row", Kind::Ms, |_| px(0.0)),
    row("motion.icon_draw", Kind::Ms, |_| px(0.0)),
    row("motion.graph_draw", Kind::Ms, |_| px(0.0)),
    row("motion.graph_zoom", Kind::Ms, |_| px(0.0)),
    row("layout.calculator.margin", Kind::Spacing, |_| {
        use metrics::calc::{MARGIN_X, MARGIN_Y};
        spacing(MARGIN_Y, MARGIN_X, MARGIN_Y, MARGIN_X)
    }),
    row("layout.calculator.gap", SIZE, |_| px(0.0)),
    row("layout.calculator.panel_width", SIZE, |_| {
        px(metrics::calc::PANEL_W - metrics::calc::PANEL_MARGIN_LEFT)
    }),
    row("layout.calculator.panel_margin", Kind::Spacing, |_| {
        use metrics::calc::{PANEL_MARGIN_LEFT, PANEL_MARGIN_Y};
        spacing(PANEL_MARGIN_Y, 0.0, PANEL_MARGIN_Y, PANEL_MARGIN_LEFT)
    }),
    row("layout.converter.margin", Kind::Spacing, |_| {
        use metrics::converter::{MARGIN_X, MARGIN_Y};
        spacing(MARGIN_Y, MARGIN_X, MARGIN_Y, MARGIN_X)
    }),
    row("layout.converter.gap", SIZE, |_| {
        px(metrics::converter::NARROW_GAP)
    }),
    row("layout.converter.gap_wide", SIZE, |_| {
        px(metrics::converter::WIDE_GAP)
    }),
    row("layout.converter.keypad_width", SIZE, |_| {
        px(metrics::converter::KEYPAD_W)
    }),
    row("layout.date.margin", Kind::Spacing, |_| {
        use metrics::date::{MARGIN_X, MARGIN_Y};
        spacing(MARGIN_Y, MARGIN_X, MARGIN_Y, MARGIN_X)
    }),
    row("layout.date.gap", SIZE, |_| {
        px(metrics::date::STEP_FROM_TO - metrics::date::FIELD_BLOCK_H)
    }),
    row("layout.date.max_width", SIZE, |_| {
        px(metrics::date::MAX_WIDTH)
    }),
    row("layout.graph.margin", Kind::Spacing, |_| {
        use metrics::graph::{MARGIN_X, MARGIN_Y};
        spacing(MARGIN_Y, MARGIN_X, MARGIN_Y, MARGIN_X)
    }),
    row("layout.graph.gap", SIZE, |_| px(metrics::graph::GAP)),
    row("layout.graph.side_width", SIZE, |_| {
        px(metrics::graph::SIDE_W)
    }),
    row("layout.graph.pad_height", SIZE, |_| {
        px(metrics::graph::PAD_H)
    }),
    // -- The toolkit's own widgets.
    row("native", Kind::Words(&NATIVE_FAMILIES), |_| {
        Value::Words(Vec::new())
    }),
    row("stock.window", Kind::Color, |s| color(s.t.colors.bg)),
    row("stock.view", Kind::Color, |s| color(s.t.colors.surface)),
    row("stock.headerbar", Kind::Color, |s| color(s.t.colors.bg)),
    row("stock.sidebar", Kind::Color, |s| color(s.t.colors.bg)),
    row("stock.dialog", Kind::Color, |s| color(s.t.colors.bg)),
    row("stock.popover", Kind::Color, |s| color(s.t.colors.surface)),
    row("stock.card", Kind::Color, |s| color(s.t.colors.surface)),
    row("stock.fg", Kind::Color, |s| color(s.t.colors.fg)),
    row("stock.accent", Kind::Color, |s| {
        color(s.t.colors.accent_text)
    }),
    row("stock.accent_bg", Kind::Color, |s| color(s.t.colors.accent)),
    row("stock.accent_fg", Kind::Color, |s| {
        color(s.t.colors.on_accent)
    }),
    row("stock.destructive", Kind::Color, |s| {
        color(s.t.colors.danger)
    }),
    row("stock.error", Kind::Color, |s| color(s.t.colors.danger)),
    // -- Window.
    row("window.background", Kind::Fill, |s| {
        Value::Fill(Fill::color(s.t.colors.bg))
    }),
    row("window.color", Kind::Color, |s| color(s.t.colors.fg)),
    row("window.font", Kind::Font, |s| {
        Value::Font(s.t.text.body.clone())
    }),
    row("window.text_halo", Kind::TextShadows, |s| {
        Value::Shadows(s.halo())
    }),
    row("window.raised_text_halo", Kind::TextShadows, |_| {
        Value::Shadows(Vec::new())
    }),
    row("plain.background", Kind::Fill, |s| {
        Value::Fill(Fill::color(s.text_scrim()))
    }),
    row("plain.radius", Kind::Radius, |s| {
        Value::Radius(Radius::all(s.t.shape.card_radius))
    }),
    row("scrim", Kind::Color, |_| {
        color(alpha(BLACK, metrics::shape::SCRIM_ALPHA))
    }),
    row("focus", Kind::FocusRing, |s| {
        Value::FocusRing(FocusRing {
            width: metrics::focus::WIDTH,
            color: s.t.colors.accent_text,
            offset: metrics::focus::OFFSET,
            radius: None,
        })
    }),
    // -- Header and window buttons.
    row("header.background", Kind::Fill, |s| {
        Value::Fill(Fill::color(s.text_scrim()))
    }),
    todo("header.height", SIZE),
    todo("header.title", Kind::Text),
    todo("header.button", Kind::Control),
    todo("header.controls", Kind::Control),
    todo("header.close", Kind::Control),
    // -- Navigation sidebar.
    todo("nav.background", Kind::Fill),
    todo("nav.shadow", Kind::Shadows),
    todo("nav.width", SIZE),
    todo("nav.title", Kind::Text),
    todo("nav.show_title", Kind::Bool),
    todo("nav.header", Kind::Text),
    todo("nav.uppercase_headers", Kind::Bool),
    todo("nav.row", Kind::Control),
    todo("nav.settings", Kind::Control),
    // -- Shared controls.
    row("icon_button", Kind::Control, |s| {
        let c = &s.t.colors;
        let size = metrics::controls::ICON_BUTTON;
        Value::Control(Box::new(Control {
            min_width: size,
            min_height: size,
            radius: Radius::all(ROUND),
            checked: State::filled(
                alpha(c.accent, metrics::controls::ICON_BUTTON_ON_ALPHA),
                c.accent_text,
            ),
            ..s.control(&s.t.text.body)
        }))
    }),
    todo("text_button", Kind::Control),
    todo("text_button_gap", SIZE),
    todo("flyout_button", Kind::Control),
    row("button", Kind::Control, |s| {
        let c = &s.t.colors;
        Value::Control(Box::new(Control {
            background: Fill::color(c.surface2),
            min_height: metrics::controls::TEXT_BUTTON_H,
            padding: Spacing::vh(0.0, metrics::controls::BUTTON_PAD_X),
            checked: State::filled(c.accent, c.on_accent),
            ..s.control(&s.t.text.body)
        }))
    }),
    row("segmented.gap", SIZE, |_| {
        px(metrics::controls::SEGMENTED_GAP)
    }),
    row("segmented.item", Kind::Control, |s| {
        let c = &s.t.colors;
        Value::Control(Box::new(Control {
            background: Fill::color(c.surface2),
            min_height: metrics::controls::SEGMENTED_H,
            // DGMNB keeps a button's label this far from its sides.
            padding: Spacing::vh(0.0, metrics::controls::BUTTON_PAD_X),
            checked: State::filled(c.accent, c.on_accent),
            ..s.control(&s.t.text.small)
        }))
    }),
    row("field", Kind::Field, |s| {
        let c = &s.t.colors;
        let ring = |color: Rgba| State {
            border: Some(Some(Border {
                width: metrics::shape::FIELD_BORDER_ACTIVE,
                color,
            })),
            ..State::default()
        };
        Value::Field(Box::new(Field {
            background: Fill::color(c.surface),
            color: c.fg,
            border: s.line(),
            shadow: Vec::new(),
            radius: Radius::all(metrics::shape::FIELD_RADIUS),
            font: s.t.text.body.clone(),
            padding: Spacing::vh(0.0, metrics::controls::FIELD_PAD_X),
            min_height: metrics::controls::FIELD_H,
            placeholder: c.fg_faint,
            selection: alpha(c.accent, metrics::controls::SELECTION_ALPHA),
            caret: c.fg,
            focus: ring(c.accent_text),
            error: ring(c.danger),
        }))
    }),
    row("row", Kind::Control, |s| {
        let c = &s.t.colors;
        Value::Control(Box::new(Control {
            radius: Radius::all(s.t.shape.row_radius),
            min_height: metrics::controls::ROW_H,
            checked: State {
                background: Some(Fill::color(alpha(
                    c.accent,
                    metrics::controls::ROW_CHECKED_ALPHA,
                ))),
                ..State::default()
            },
            ..s.control(&s.t.text.body)
        }))
    }),
    row("section", Kind::Text, |s| {
        s.raised_text(&s.t.text.caption, s.t.colors.fg_dim)
    }),
    row("slider.track", SLIDER_TRACK, |s| {
        use metrics::controls::{SLIDER_TRACK_H, SLIDER_TRACK_RADIUS};
        record(&[
            ("height", px(SLIDER_TRACK_H)),
            ("color", color(s.t.colors.border)),
            ("radius", px(SLIDER_TRACK_RADIUS)),
        ])
    }),
    row("slider.fill", Kind::Color, |s| color(s.t.colors.accent)),
    row("slider.knob", SLIDER_KNOB, |s| {
        use metrics::controls::{SLIDER_HALO_ALPHA, SLIDER_HALO_RADIUS, SLIDER_KNOB_RADIUS};
        let a = s.t.colors.accent;
        record(&[
            ("size", px(SLIDER_KNOB_RADIUS * 2.0)),
            ("color", color(a)),
            ("halo", color(alpha(a, SLIDER_HALO_ALPHA))),
            ("halo_size", px(SLIDER_HALO_RADIUS * 2.0)),
        ])
    }),
    row("switch", SWITCH, |s| {
        use metrics::controls::{SWITCH_H, SWITCH_KNOB_RADIUS, SWITCH_W};
        let c = &s.t.colors;
        record(&[
            ("width", px(SWITCH_W)),
            ("height", px(SWITCH_H)),
            ("knob_size", px(SWITCH_KNOB_RADIUS * 2.0)),
            ("off", color(c.border)),
            ("on", color(c.accent)),
            ("knob", color(c.surface)),
            ("knob_on", color(c.on_accent)),
        ])
    }),
    todo(
        "scrollbar",
        Kind::Record(&[("width", SIZE), ("color", Kind::Color), ("radius", SIZE)]),
    ),
    row("card", Kind::Surface, |s| {
        Value::Surface(Surface {
            background: Fill::color(s.t.colors.surface),
            radius: Radius::all(s.t.shape.card_radius),
            ..s.bare()
        })
    }),
    // The headings over Settings' and About's cards (DGMNB's STRONG).
    row("card.heading", Kind::Text, |s| {
        s.raised_text(&s.t.text.strong, s.t.colors.fg)
    }),
    row("popover", Kind::Surface, |s| {
        Value::Surface(Surface {
            background: Fill::color(s.t.colors.surface),
            border: s.line(),
            shadow: s.rings(),
            radius: Radius::all(s.t.shape.card_radius),
            padding: Spacing::all(metrics::controls::LIST_POPUP_PAD),
            margin: Spacing::default(),
        })
    }),
    row("menu", Kind::Surface, |s| {
        let popover = match s.get("popover") {
            Value::Surface(p) => p.clone(),
            _ => s.bare(),
        };
        Value::Surface(Surface {
            radius: Radius::all(metrics::shape::MENU_RADIUS),
            ..popover
        })
    }),
    row("toast", TOAST, |s| {
        use metrics::toast;
        let c = &s.t.colors;
        record(&[
            (
                "background",
                Value::Fill(Fill::color(alpha(c.fg, toast::ALPHA))),
            ),
            ("color", color(c.bg)),
            (
                "radius",
                Value::Radius(Radius::all(metrics::shape::TOAST_RADIUS)),
            ),
            ("font", Value::Font(s.t.text.body.clone())),
            ("min_height", px(toast::MIN_HEIGHT)),
            ("padding", spacing(0.0, toast::PAD_X, 0.0, toast::PAD_X)),
        ])
    }),
    row("tooltip", TOOLTIP, |s| {
        let c = &s.t.colors;
        record(&[
            (
                "background",
                Value::Fill(Fill::color(alpha(c.fg, metrics::toast::ALPHA))),
            ),
            ("color", color(c.bg)),
            ("radius", Value::Radius(Radius::all(s.t.shape.row_radius))),
            ("font", Value::Font(s.t.text.small.clone())),
            ("padding", spacing(4.0, 8.0, 4.0, 8.0)),
        ])
    }),
    // -- Display (the readout).
    row("display.background", Kind::Fill, |s| {
        Value::Fill(Fill::color(s.text_scrim()))
    }),
    row("display.radius", Kind::Radius, |s| {
        Value::Radius(Radius::all(s.t.shape.card_radius))
    }),
    row("display.padding_x", SIZE, |_| {
        px(metrics::calc::DISPLAY_PAD_X)
    }),
    row("display.heights", MODES_HEIGHT, |_| {
        use metrics::calc::*;
        modes(
            DISPLAY_H_STANDARD,
            DISPLAY_H_SCIENTIFIC,
            DISPLAY_H_PROGRAMMER,
            DISPLAY_H_COMPACT,
        )
    }),
    row("display.sizes", MODES_SIZE, |_| {
        let (full, compact) = (
            metrics::text::DISPLAY_VALUE.size,
            metrics::text::DISPLAY_VALUE_COMPACT.size,
        );
        modes(full, full, full, compact)
    }),
    row("display.min_size", FONT_SIZE, |_| {
        px(metrics::text::DISPLAY_MIN_SIZE)
    }),
    row("display.value.font", Kind::Font, |s| {
        let value = metrics::text::DISPLAY_VALUE;
        Value::Font(
            Font::new(&s.t.fonts.display, value.size, value.weight).with_features(&["tnum"]),
        )
    }),
    row("display.value.color", Kind::Paint, |s| {
        Value::Paint(Paint::Color(s.t.colors.fg))
    }),
    row("display.value.error_color", Kind::Paint, |s| {
        Value::Paint(Paint::Color(s.t.colors.fg))
    }),
    row("display.value.error_scale", Kind::Num(0.1, 4.0), |_| {
        px(1.0)
    }),
    row(
        "display.value.glow",
        Kind::Opt(&Kind::Record(&[
            ("color", Kind::Color),
            ("blur", Kind::Num(0.0, 4.0)),
        ])),
        |_| Value::Absent,
    ),
    row("display.expression", Kind::Text, |s| {
        Value::Text(Text {
            color: s.t.colors.fg_dim,
            font: s.t.text.small.clone(),
            shadow: Vec::new(),
            margin: Spacing::default(),
        })
    }),
    row("display.expression_min_size", FONT_SIZE, |_| {
        px(metrics::text::EXPRESSION_MIN_SIZE)
    }),
    row("display.halo", Kind::Shadows, |s| Value::Shadows(s.halo())),
    // -- Keys. The shared values first: the classes are built from them.
    row("keys.gap", SIZE, |_| px(metrics::calc::KEY_GAP)),
    row("keys.gap_compact", SIZE, |_| {
        px(metrics::calc::KEY_GAP_COMPACT)
    }),
    row("keys.radius", Kind::Radius, |s| {
        Value::Radius(Radius::all(s.t.shape.radius))
    }),
    row("keys.min_width", SIZE, |_| px(0.0)),
    row("keys.min_height", SIZE, |_| px(0.0)),
    row("keys.padding", Kind::Spacing, |_| {
        Value::Spacing(Spacing::default())
    }),
    row("keys.transition", Kind::Transition, |_| {
        Value::Transition(Transition::default())
    }),
    row("keys.icon", Kind::Icon, |_| {
        Value::Icon(Icon {
            size: metrics::controls::KEY_ICON,
            stroke: None,
            color: None,
        })
    }),
    row("keys.hover", Kind::State, |s| {
        Value::State(State::overlay(s.t.colors.hover))
    }),
    row("keys.active", Kind::State, |s| {
        Value::State(State::overlay(s.t.colors.press))
    }),
    row("keys.disabled", Kind::State, |_| {
        Value::State(State {
            fade: Some(metrics::controls::DISABLED_ALPHA),
            ..State::default()
        })
    }),
    row("keys.on", Kind::State, |s| {
        Value::State(State::filled(s.t.colors.accent, s.t.colors.on_accent))
    }),
    row("keys.number", Kind::KeyControl, |s| {
        let mut font = s.t.text.key.clone();
        font.size += metrics::text::NUMBER_KEY_SIZE_ADD;
        font.weight = metrics::text::NUMBER_KEY_WEIGHT;
        s.key(s.t.colors.surface, s.t.colors.fg, font)
    }),
    row("keys.func", Kind::KeyControl, |s| {
        s.key(s.t.colors.surface2, s.t.colors.fg, s.t.text.key.clone())
    }),
    row("keys.operator", Kind::KeyControl, |s| {
        s.key(
            s.t.colors.surface2,
            s.t.colors.accent_text,
            s.t.text.key.clone(),
        )
    }),
    row("keys.equals", Kind::KeyControl, |s| {
        s.key(
            s.t.colors.accent,
            s.t.colors.on_accent,
            s.t.text.key.clone(),
        )
    }),
    row("keys.pad", KEYS_PAD, |s| {
        // The pad's keys are set in their classes' own fonts; a theme's
        // font here (anything other than this) is used for all of them.
        let font = s.control_at("keys.func").font;
        record(&[
            ("gap", px(metrics::graph::PAD_GAP)),
            ("radius", Value::Radius(Radius::all(s.t.shape.radius))),
            ("min_height", px(0.0)),
            ("font", Value::Font(font)),
        ])
    }),
    // -- History and memory panel.
    todo("panel", Kind::Surface),
    todo("panel.sheet", Kind::Surface),
    row("panel.tabs.stretch", Kind::Bool, |_| Value::Bool(true)),
    row("panel.tabs.gap", SIZE, |_| px(0.0)),
    row("panel.tabs.item", Kind::Control, |s| {
        let c = &s.t.colors;
        Value::Control(Box::new(Control {
            min_height: metrics::panel::TAB_H,
            padding: Spacing::vh(0.0, metrics::controls::BUTTON_PAD_X),
            checked: State::filled(c.accent, c.on_accent),
            ..s.control(&s.t.text.strong)
        }))
    }),
    todo("panel.title", Kind::Text),
    todo("panel.row", Kind::Control),
    todo("panel.expression", Kind::Text),
    todo("panel.result", Kind::Text),
    todo("panel.delete", Kind::Control),
    todo("panel.delete_reveal", REVEAL),
    todo("panel.memory_button", Kind::Control),
    todo("panel.memory_reveal", REVEAL),
    todo("panel.empty", Kind::Text),
    // -- Programmer.
    todo("programmer.radix.row", Kind::Control),
    todo("programmer.radix.name", Kind::Text),
    todo("programmer.radix.name_checked", Kind::Color),
    todo("programmer.radix.value", Kind::Text),
    todo("programmer.bit", Kind::Control),
    todo("programmer.bit_index", Kind::Text),
    // -- Date page.
    row("date.label", Kind::Text, |s| {
        s.text(&s.t.text.caption, s.t.colors.fg_dim)
    }),
    row("date.field", Kind::Control, |s| {
        let c = &s.t.colors;
        Value::Control(Box::new(Control {
            background: Fill::color(c.surface2),
            min_height: metrics::date::FIELD_H,
            padding: Spacing::vh(0.0, metrics::controls::BUTTON_PAD_X),
            // Lit while its calendar is open.
            checked: State::filled(c.accent, c.on_accent),
            ..s.control(&s.t.text.body)
        }))
    }),
    row("date.result_panel", Kind::Surface, |s| {
        Value::Surface(s.bare())
    }),
    row("date.result", SIZE_FONT, |s| {
        let font = s.sized(metrics::text::DATE_RESULT);
        record(&[("size", px(font.size)), ("font", Value::Font(font))])
    }),
    row("date.add_result", SIZE_FONT, |s| {
        let font = s.sized(metrics::text::DATE_RESULT);
        record(&[("size", px(font.size)), ("font", Value::Font(font))])
    }),
    row("date.subresult", Kind::Text, |s| {
        s.text(&s.t.text.body, s.t.colors.fg_dim)
    }),
    row("date.calendar.title", Kind::Text, |s| {
        s.raised_text(&s.t.text.strong, s.t.colors.fg)
    }),
    row("date.calendar.weekday", Kind::Text, |s| {
        s.raised_text(&s.t.text.caption, s.t.colors.fg_dim)
    }),
    row("date.calendar.day", Kind::Control, |s| {
        let c = &s.t.colors;
        Value::Control(Box::new(Control {
            radius: Radius::all(ROUND),
            checked: State::filled(c.accent, c.on_accent),
            ..s.control(&s.t.text.small)
        }))
    }),
    row("date.calendar.other_month", Kind::Color, |s| {
        color(s.t.colors.fg_faint)
    }),
    row("date.calendar.today", Kind::Opt(&Kind::Border), |s| {
        Value::Border(Border {
            width: metrics::date::TODAY_BORDER,
            color: s.t.colors.accent_text,
        })
    }),
    row("date.calendar.focus", Kind::FocusRing, |s| {
        s.get("focus").clone()
    }),
    // -- Converter.
    row("converter.field", Kind::Control, |s| {
        Value::Control(Box::new(Control {
            radius: Radius::all(0.0),
            // DGMNB draws nothing under the pointer or pressed.
            hover: State::default(),
            active: State::default(),
            ..s.control(&s.t.text.body)
        }))
    }),
    row(
        "converter.marker",
        Kind::Opt(&Kind::Record(&[
            ("color", Kind::Color),
            ("width", SIZE),
            ("height", SIZE),
        ])),
        |s| {
            use metrics::converter::{MARKER_H, MARKER_W};
            record(&[
                ("color", color(s.t.colors.accent)),
                ("width", px(MARKER_W)),
                ("height", px(MARKER_H)),
            ])
        },
    ),
    row(
        "converter.value",
        Kind::Record(&[
            ("size", FONT_SIZE),
            ("font", Kind::Font),
            ("checked_weight", Kind::Num(1.0, 1000.0)),
            ("focus", Kind::FocusRing),
        ]),
        |s| {
            use metrics::text::{CONVERTER_VALUE, CONVERTER_VALUE_CHECKED_WEIGHT};
            let font = Font {
                family: s.t.fonts.display.clone(),
                ..s.sized(CONVERTER_VALUE)
            }
            .with_features(&["tnum"]);
            record(&[
                ("size", px(font.size)),
                ("font", Value::Font(font)),
                ("checked_weight", px(CONVERTER_VALUE_CHECKED_WEIGHT)),
                ("focus", s.get("focus").clone()),
            ])
        },
    ),
    row("converter.symbol", Kind::Text, |s| {
        s.text(&s.sized(metrics::text::CONVERTER_SYMBOL), s.t.colors.fg_dim)
    }),
    row("converter.unit", Kind::Control, |s| {
        let c = &s.t.colors;
        Value::Control(Box::new(Control {
            background: Fill::color(c.surface2),
            min_height: metrics::converter::UNIT_H,
            padding: Spacing::vh(0.0, metrics::controls::BUTTON_PAD_X),
            // Lit while its list is open.
            checked: State::filled(c.accent, c.on_accent),
            ..s.control(&s.t.text.body)
        }))
    }),
    row("converter.swap", Kind::Control, |s| {
        s.icon_button(metrics::controls::ICON_BUTTON)
    }),
    row("converter.caption", Kind::Text, |s| {
        s.text(&s.t.text.caption, s.t.colors.fg_dim)
    }),
    row("converter.whimsy", Kind::State, |_| {
        Value::State(State::default())
    }),
    // A chip's `checked` is the whimsy (the playful comparison): the
    // stylesheet reads it as the chip's checked state.
    row("converter.chip", Kind::Control, |s| {
        use metrics::converter::{CHIP_H, CHIP_PAD_X};
        Value::Control(Box::new(Control {
            background: Fill::color(s.t.colors.surface2),
            radius: Radius::all(metrics::shape::CHIP_RADIUS),
            min_height: CHIP_H,
            padding: Spacing::vh(0.0, CHIP_PAD_X),
            // A label: nothing under the pointer or pressed.
            hover: State::default(),
            active: State::default(),
            checked: s.state("converter.whimsy"),
            ..s.control(&s.t.text.small)
        }))
    }),
    row("converter.whimsy_icon", Kind::Bool, |_| Value::Bool(false)),
    row("converter.ratio", Kind::Text, |s| {
        s.text(&s.t.text.small, s.t.colors.fg)
    }),
    row("converter.timestamp", Kind::Text, |s| {
        s.text(&s.t.text.small, s.t.colors.fg_dim)
    }),
    row("converter.link", Kind::Control, |s| {
        // "Update rates": a plain button in DGMNB's small text.
        Value::Control(Box::new(Control {
            padding: Spacing::vh(0.0, metrics::controls::BUTTON_PAD_X),
            ..s.control(&s.t.text.small)
        }))
    }),
    // -- Graphing.
    row("graph.side", Kind::Surface, |s| Value::Surface(s.bare())),
    row("graph.divider", Kind::Opt(&Kind::Color), |s| {
        color(s.t.colors.border)
    }),
    row("graph.row", Kind::Surface, |s| {
        Value::Surface(Surface {
            padding: Spacing::vh(metrics::graph::ROW_FIELD_Y, 0.0),
            ..s.bare()
        })
    }),
    row("graph.row_error", Kind::State, |_| {
        Value::State(State::default())
    }),
    row("graph.entry", Kind::Field, |s| match s.get("field") {
        Value::Field(f) => Value::Field(Box::new(Field {
            min_height: metrics::graph::ROW_FIELD_H,
            ..(**f).clone()
        })),
        other => other.clone(),
    }),
    row("graph.row_button", Kind::Control, |s| {
        s.icon_button(metrics::graph::ROW_BUTTON)
    }),
    row("graph.error", Kind::Text, |s| {
        s.text(&s.t.text.caption, s.t.colors.danger)
    }),
    row("graph.swatch.size", SIZE, |_| {
        px(metrics::graph::SWATCH_RADIUS * 2.0)
    }),
    row("graph.swatch.hidden_opacity", UNIT, |_| {
        px(metrics::graph::SWATCH_HIDDEN_ALPHA)
    }),
    row(
        "graph.swatch.glow",
        Kind::Opt(&Kind::Record(&[
            ("blur", Kind::Px(0.0, 100.0)),
            ("spread", Kind::Px(-1000.0, 1000.0)),
        ])),
        |_| Value::Absent,
    ),
    row("graph.swatch.active_scale", Kind::Num(0.0, 4.0), |_| {
        px(1.0)
    }),
    row("graph.swatch.transition", Kind::Transition, |_| {
        Value::Transition(Transition::default())
    }),
    row("graph.add", Kind::Control, |s| {
        Value::Control(Box::new(Control {
            min_height: metrics::graph::ADD_H,
            padding: Spacing::vh(0.0, metrics::controls::BUTTON_PAD_X),
            ..s.control(&s.t.text.body)
        }))
    }),
    row("graph.variables", Kind::Text, |s| {
        s.text(&s.t.text.strong, s.t.colors.fg)
    }),
    row("graph.variable", Kind::Text, |s| {
        s.text(&s.sized(metrics::text::GRAPH_VARIABLE), s.t.colors.fg)
    }),
    row("graph.analysis.title", Kind::Text, |s| {
        s.text(&s.sized(metrics::text::ANALYSIS_TITLE), s.t.colors.fg)
    }),
    row("graph.analysis.card", Kind::Surface, |s| {
        Value::Surface(Surface {
            border: s.line(),
            radius: Radius::all(metrics::shape::ANALYSIS_CARD_RADIUS),
            padding: Spacing::vh(
                metrics::graph::ANALYSIS_CARD_BOTTOM,
                metrics::graph::ANALYSIS_CARD_X,
            ),
            ..s.bare()
        })
    }),
    row("graph.analysis.card_title", Kind::Text, |s| {
        s.text(&s.t.text.caption, s.t.colors.fg_dim)
    }),
    row("graph.analysis.value", Kind::Text, |s| {
        s.text(&s.t.text.body, s.t.colors.fg)
    }),
    row("graph.analysis.text", Kind::Text, |s| {
        s.text(&s.t.text.small, s.t.colors.fg)
    }),
    row("graph.analysis.note", Kind::Text, |s| {
        s.text(&s.t.text.caption, s.t.colors.fg_dim)
    }),
    row("graph.toolbar", Kind::Surface, |s| {
        use metrics::graph::{
            TOOLBAR_BUTTON, TOOLBAR_FILL_ALPHA, TOOLBAR_PAD_X, TOOLBAR_PAD_Y, TOOLBAR_STEP,
        };
        // The buttons are 34 apart by 36: the last one's 2 are left below
        // it, with the padding.
        let bottom = TOOLBAR_PAD_Y + TOOLBAR_STEP - TOOLBAR_BUTTON;
        Value::Surface(Surface {
            background: Fill::color(alpha(s.t.colors.surface2, TOOLBAR_FILL_ALPHA)),
            radius: Radius::all(metrics::shape::TOOLBAR_RADIUS),
            padding: Spacing::trbl(TOOLBAR_PAD_Y, TOOLBAR_PAD_X, bottom, TOOLBAR_PAD_X),
            ..s.bare()
        })
    }),
    row("graph.toolbar_button", Kind::Control, |s| {
        s.icon_button(metrics::graph::TOOLBAR_BUTTON)
    }),
    row("graph.canvas.background", Kind::Paint, |s| {
        Value::Paint(Paint::Color(s.t.colors.surface))
    }),
    row("graph.canvas.radius", SIZE, |_| {
        px(metrics::graph::CANVAS_RADIUS)
    }),
    row("graph.canvas.border", Kind::Opt(&Kind::Border), |s| {
        Value::Border(Border {
            width: metrics::shape::BORDER,
            color: s.t.colors.border,
        })
    }),
    row("graph.canvas.grid_minor", Kind::Color, |s| {
        let (dark, light) = metrics::graph::GRID_MINOR_ALPHA;
        color(alpha(s.t.colors.fg, if s.ctx.dark { dark } else { light }))
    }),
    row("graph.canvas.grid_major", Kind::Color, |s| {
        let (dark, light) = metrics::graph::GRID_MAJOR_ALPHA;
        color(alpha(s.t.colors.fg, if s.ctx.dark { dark } else { light }))
    }),
    row(
        "graph.canvas.axis",
        Kind::Record(&[("color", Kind::Color), ("width", Kind::Px(0.0, 64.0))]),
        |s| {
            use metrics::graph::{AXIS_ALPHA, AXIS_WIDTH};
            record(&[
                ("color", color(alpha(s.t.colors.fg, AXIS_ALPHA))),
                ("width", px(AXIS_WIDTH)),
            ])
        },
    ),
    row("graph.canvas.label", Kind::Text, |s| {
        s.text(&s.sized(metrics::text::CANVAS_LABEL), s.t.colors.fg_dim)
    }),
    row("graph.canvas.fill_opacity", UNIT, |_| {
        px(metrics::graph::FILL_OPACITY)
    }),
    row(
        "graph.canvas.glow",
        Kind::Opt(&Kind::Record(&[
            ("blur", Kind::Px(0.0, 100.0)),
            ("width", Kind::Num(0.0, 64.0)),
            ("opacity", UNIT),
        ])),
        |_| Value::Absent,
    ),
    row("graph.canvas.trace.hair_opacity", UNIT, |_| {
        px(metrics::graph::HAIR_OPACITY)
    }),
    row(
        "graph.canvas.trace.hair_dash",
        Kind::Opt(&Kind::Dash),
        |_| Value::Absent,
    ),
    row(
        "graph.canvas.trace.halo",
        Kind::Opt(&Kind::Record(&[
            ("grow", Kind::Px(0.0, 64.0)),
            ("color", Kind::ColorOr("series")),
            ("opacity", UNIT),
        ])),
        |s| {
            record(&[
                ("grow", px(metrics::graph::TRACE_HALO_GROW)),
                ("color", color(s.t.colors.surface)),
                ("opacity", px(1.0)),
            ])
        },
    ),
    row("graph.canvas.trace.ring", Kind::Opt(&Kind::Border), |_| {
        Value::Absent
    }),
    row(
        "graph.canvas.trace.bubble",
        Kind::Record(&[
            ("background", Kind::Color),
            ("color", Kind::Color),
            ("radius", SIZE),
            ("font", Kind::Font),
            ("border", Kind::Opt(&Kind::Border)),
            ("shadow", Kind::Shadows),
        ]),
        |s| {
            let c = &s.t.colors;
            record(&[
                ("background", color(c.surface)),
                ("color", color(c.fg)),
                ("radius", px(metrics::shape::BUBBLE_RADIUS)),
                ("font", Value::Font(s.t.text.small.clone())),
                (
                    "border",
                    Value::Border(Border {
                        width: metrics::shape::BORDER,
                        color: c.border,
                    }),
                ),
                ("shadow", Value::Shadows(s.rings())),
            ])
        },
    ),
];

// ---- The tree ----------------------------------------------------------------

/// The rows by path: a node may be a row, have rows under it, or both
/// (`panel` is a surface, and `panel.row` a control).
#[derive(Default)]
pub struct Node {
    pub row: Option<&'static Row>,
    pub children: BTreeMap<&'static str, Node>,
}

pub fn tree() -> &'static Node {
    static TREE: OnceLock<Node> = OnceLock::new();
    TREE.get_or_init(|| {
        let mut root = Node::default();
        for row in ROWS {
            let mut node = &mut root;
            for part in row.path.split('.') {
                node = node.children.entry(part).or_default();
            }
            node.row = Some(row);
        }
        root
    })
}

/// The node at `path`, if the schema has one.
pub fn node(path: &str) -> Option<&'static Node> {
    let mut node = tree();
    for part in path.split('.') {
        node = node.children.get(part)?;
    }
    Some(node)
}

/// The names of `path`'s children that are rows of their own (so a check
/// of `path`'s value leaves them alone).
pub fn child_names(path: &str) -> Vec<&'static str> {
    node(path).map_or_else(Vec::new, |n| n.children.keys().copied().collect())
}

// ---- Checking by kind ----------------------------------------------------------

fn neutral_font() -> Font {
    Font::new(&[], 15.0, 400.0)
}

fn neutral_control() -> Control {
    Control {
        background: Fill::none(),
        color: TRANSPARENT,
        border: None,
        shadow: Vec::new(),
        radius: Radius::all(0.0),
        font: neutral_font(),
        padding: Spacing::default(),
        margin: Spacing::default(),
        min_width: 0.0,
        min_height: 0.0,
        opacity: 1.0,
        icon: Icon {
            size: 18.0,
            stroke: None,
            color: None,
        },
        transition: Transition::default(),
        hover: State::default(),
        active: State::default(),
        checked: State::default(),
        disabled: State::default(),
    }
}

/// A value of `kind` with nothing in it: what a `todo` row's theme value
/// is checked onto, and what a part a theme left out of an optional table
/// is.
pub fn neutral(kind: &Kind) -> Value {
    match kind {
        Kind::Bool => Value::Bool(false),
        Kind::Px(min, _) | Kind::Num(min, _) | Kind::PxOr(_, min, _) => Value::Number(min.max(0.0)),
        Kind::Ms => Value::Number(0.0),
        Kind::Color | Kind::ColorOr(_) => Value::Color(TRANSPARENT),
        Kind::Paint => Value::Paint(Paint::Color(TRANSPARENT)),
        Kind::Fill => Value::Fill(Fill::none()),
        Kind::Shadows | Kind::TextShadows => Value::Shadows(Vec::new()),
        Kind::Radius => Value::Radius(Radius::all(0.0)),
        Kind::Spacing => Value::Spacing(Spacing::default()),
        Kind::Font => Value::Font(neutral_font()),
        Kind::Transition => Value::Transition(Transition::default()),
        Kind::State => Value::State(State::default()),
        Kind::Icon => Value::Icon(Icon {
            size: 18.0,
            stroke: None,
            color: None,
        }),
        Kind::FocusRing => Value::FocusRing(FocusRing {
            width: 0.0,
            color: TRANSPARENT,
            offset: 0.0,
            radius: None,
        }),
        Kind::Text => Value::Text(Text {
            color: TRANSPARENT,
            font: neutral_font(),
            shadow: Vec::new(),
            margin: Spacing::default(),
        }),
        Kind::Surface => Value::Surface(Surface {
            background: Fill::none(),
            border: None,
            shadow: Vec::new(),
            radius: Radius::all(0.0),
            padding: Spacing::default(),
            margin: Spacing::default(),
        }),
        Kind::Control | Kind::KeyControl => Value::Control(Box::new(neutral_control())),
        Kind::Field => {
            let c = neutral_control();
            Value::Field(Box::new(Field {
                background: c.background,
                color: c.color,
                border: None,
                shadow: Vec::new(),
                radius: c.radius,
                font: c.font,
                padding: c.padding,
                min_height: 0.0,
                placeholder: TRANSPARENT,
                selection: TRANSPARENT,
                caret: TRANSPARENT,
                focus: State::default(),
                error: State::default(),
            }))
        }
        Kind::Word(words) => Value::Word(words.first().copied().unwrap_or("")),
        Kind::Words(_) => Value::Words(Vec::new()),
        Kind::Dash => Value::Numbers(vec![0.0, 0.0]),
        Kind::Border => Value::Border(Border {
            width: 0.0,
            color: TRANSPARENT,
        }),
        Kind::Opt(_) => Value::Absent,
        Kind::Record(parts) => {
            Value::Record(parts.iter().map(|(name, k)| (*name, neutral(k))).collect())
        }
    }
}

/// `v`, what a theme gave at `path`, checked as `kind` onto `base` (which
/// stays wherever `v` is wrong). `also`: keys of `v` that are rows of
/// their own.
pub fn check(
    kind: &Kind,
    v: &ThemeValue,
    base: &Value,
    path: &str,
    also: &[&str],
    notes: &mut Notes,
) -> Value {
    let keep = || base.clone();
    match kind {
        Kind::Bool => check::boolean(v, path, notes).map_or_else(keep, Value::Bool),
        Kind::Px(min, max) | Kind::Num(min, max) => {
            check::number(v, path, *min, *max, notes).map_or_else(keep, Value::Number)
        }
        Kind::Ms => {
            check::number(v, path, 0.0, check::MAX_MS, notes).map_or_else(keep, Value::Number)
        }
        Kind::Color => check::color(v, path, notes).map_or_else(keep, Value::Color),
        Kind::Paint => check::paint(v, path, false, notes).map_or_else(keep, Value::Paint),
        Kind::Fill => check::fill(v, path, notes).map_or_else(keep, Value::Fill),
        Kind::Shadows | Kind::TextShadows => {
            check::shadows(v, path, notes).map_or_else(keep, Value::Shadows)
        }
        Kind::Radius => check::radius(v, path, notes).map_or_else(keep, Value::Radius),
        Kind::Spacing => check::spacing(v, path, notes).map_or_else(keep, Value::Spacing),
        Kind::Transition => check::transition(v, path, notes).map_or_else(keep, Value::Transition),
        Kind::State => check::state(v, path, notes).map_or_else(keep, Value::State),
        Kind::Border => match check::border(v, path, notes) {
            Some(Some(b)) => Value::Border(b),
            Some(None) => Value::Absent,
            None => keep(),
        },
        Kind::Word(words) => check::word(v, path, words, notes).map_or_else(keep, Value::Word),
        Kind::Words(words) => {
            let Some(items) = check::list(v) else {
                notes.push(path, "expected a list of names");
                return keep();
            };
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match check::word(item, &format!("{path}[{}]", i + 1), words, notes) {
                    // An unknown family is skipped; the rest stand.
                    Some(w) if !out.contains(&w) => out.push(w),
                    _ => {}
                }
            }
            Value::Words(out)
        }
        Kind::Dash => match check::list(v) {
            Some([dash, gap]) => {
                let dash = check::number(dash, &format!("{path}[1]"), 0.0, 1000.0, notes);
                let gap = check::number(gap, &format!("{path}[2]"), 0.0, 1000.0, notes);
                match (dash, gap) {
                    (Some(d), Some(g)) => Value::Numbers(vec![d, g]),
                    _ => keep(),
                }
            }
            _ => {
                notes.push(path, "expected { dash, gap }");
                keep()
            }
        },
        Kind::PxOr(word, min, max) => match v.as_str() {
            Some(s) if s == *word => Value::Word(word),
            Some(_) => {
                notes.push(path, format!("expected a number or \"{word}\""));
                keep()
            }
            None => check::number(v, path, *min, *max, notes).map_or_else(keep, Value::Number),
        },
        Kind::ColorOr(word) => match v.as_str() {
            Some(s) if s == *word => Value::Word(word),
            _ => check::color(v, path, notes).map_or_else(keep, Value::Color),
        },
        Kind::Opt(inner) => {
            if *v == ThemeValue::Bool(false) {
                return Value::Absent;
            }
            let onto = match base {
                Value::Absent => neutral(inner),
                present => present.clone(),
            };
            // Wrong, and nothing to fall back on: stay absent.
            let before = notes.list.len() + notes.dropped;
            let checked = check(inner, v, &onto, path, also, notes);
            let noted = notes.list.len() + notes.dropped > before;
            if *base == Value::Absent && noted && checked == onto {
                Value::Absent
            } else {
                checked
            }
        }
        // The kinds that merge, part by part.
        Kind::Font => match base {
            Value::Font(b) => Value::Font(check::font(v, b, path, notes)),
            _ => keep(),
        },
        Kind::Icon => match base {
            Value::Icon(b) => Value::Icon(check::icon(v, *b, path, notes)),
            _ => keep(),
        },
        Kind::FocusRing => match base {
            Value::FocusRing(b) => Value::FocusRing(check::focus_ring(v, *b, path, notes)),
            _ => keep(),
        },
        Kind::Text => match base {
            Value::Text(b) => Value::Text(check::text(v, b, path, also, notes)),
            _ => keep(),
        },
        Kind::Surface => match base {
            Value::Surface(b) => Value::Surface(check::surface(v, b, path, also, notes)),
            _ => keep(),
        },
        Kind::Control | Kind::KeyControl => match base {
            Value::Control(b) => {
                let checked = matches!(kind, Kind::Control);
                Value::Control(Box::new(check::control(v, b, path, also, checked, notes)))
            }
            _ => keep(),
        },
        Kind::Field => match base {
            Value::Field(b) => Value::Field(Box::new(check::field(v, b, path, also, notes))),
            _ => keep(),
        },
        Kind::Record(parts) => {
            let (ThemeValue::Map(m), Value::Record(b)) = (v, base) else {
                notes.push(path, format!("expected a table, got {}", v.type_name()));
                return keep();
            };
            let mut out = b.clone();
            for (key, value) in m {
                let kpath = join(path, key);
                match parts.iter().find(|(name, _)| name == key) {
                    Some((name, k)) => {
                        let onto = b.get(name).cloned().unwrap_or_else(|| neutral(k));
                        out.insert(name, check(k, value, &onto, &kpath, &[], notes));
                    }
                    None if also.contains(&key.as_str()) => {}
                    None => notes.push(&kpath, "unknown key"),
                }
            }
            Value::Record(out)
        }
    }
}
