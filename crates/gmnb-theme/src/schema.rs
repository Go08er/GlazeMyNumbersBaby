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

fn spacing(top: f32, right: f32, bottom: f32, left: f32) -> Value {
    Value::Spacing(Spacing::trbl(top, right, bottom, left))
}

// ---- Kinds the rows share --------------------------------------------------

const SIZE: Kind = Kind::Px(0.0, 4096.0);
const FONT_SIZE: Kind = Kind::Px(1.0, 512.0);
const UNIT: Kind = Kind::Num(0.0, 1.0);
const REVEAL: Kind = Kind::Word(&["always", "hover"]);
const HEIGHT: Kind = Kind::PxOr("auto", 0.0, 4096.0);
const SIZE_FONT: Kind = Kind::Record(&[("size", FONT_SIZE), ("font", Kind::Font)]);

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
    todo("icon_button", Kind::Control),
    todo("text_button", Kind::Control),
    todo("text_button_gap", SIZE),
    todo("flyout_button", Kind::Control),
    todo("button", Kind::Control),
    row("segmented.gap", SIZE, |_| {
        px(metrics::controls::SEGMENTED_GAP)
    }),
    row("segmented.item", Kind::Control, |s| {
        let c = &s.t.colors;
        Value::Control(Box::new(Control {
            background: Fill::color(c.surface2),
            min_height: metrics::controls::SEGMENTED_H,
            checked: State::filled(c.accent, c.on_accent),
            ..s.control(&s.t.text.small)
        }))
    }),
    todo("field", Kind::Field),
    todo("row", Kind::Control),
    todo("section", Kind::Text),
    todo(
        "slider.track",
        Kind::Record(&[("height", SIZE), ("color", Kind::Color), ("radius", SIZE)]),
    ),
    todo("slider.fill", Kind::Color),
    todo(
        "slider.knob",
        Kind::Record(&[
            ("size", SIZE),
            ("color", Kind::Color),
            ("halo", Kind::Color),
            ("halo_size", SIZE),
        ]),
    ),
    todo(
        "switch",
        Kind::Record(&[
            ("width", SIZE),
            ("height", SIZE),
            ("off", Kind::Color),
            ("on", Kind::Color),
            ("knob", Kind::Color),
            ("knob_on", Kind::Color),
        ]),
    ),
    todo(
        "scrollbar",
        Kind::Record(&[("width", SIZE), ("color", Kind::Color), ("radius", SIZE)]),
    ),
    todo("card", Kind::Surface),
    todo("popover", Kind::Surface),
    todo("menu", Kind::Surface),
    todo(
        "toast",
        Kind::Record(&[
            ("background", Kind::Fill),
            ("color", Kind::Color),
            ("radius", Kind::Radius),
            ("font", Kind::Font),
            ("min_height", SIZE),
            ("padding", Kind::Spacing),
        ]),
    ),
    todo(
        "tooltip",
        Kind::Record(&[
            ("background", Kind::Fill),
            ("color", Kind::Color),
            ("radius", Kind::Radius),
            ("font", Kind::Font),
            ("padding", Kind::Spacing),
        ]),
    ),
    // -- Display (the readout).
    row("display.background", Kind::Fill, |s| {
        Value::Fill(Fill::color(s.text_scrim()))
    }),
    row("display.radius", Kind::Radius, |s| {
        Value::Radius(Radius::all(s.t.shape.card_radius))
    }),
    todo("display.padding_x", SIZE),
    todo("display.heights", MODES_HEIGHT),
    todo("display.sizes", MODES_SIZE),
    todo("display.min_size", FONT_SIZE),
    todo("display.value.font", Kind::Font),
    todo("display.value.color", Kind::Paint),
    todo("display.value.error_color", Kind::Paint),
    todo("display.value.error_scale", Kind::Num(0.1, 4.0)),
    todo(
        "display.value.glow",
        Kind::Opt(&Kind::Record(&[
            ("color", Kind::Color),
            ("blur", Kind::Num(0.0, 4.0)),
        ])),
    ),
    todo("display.expression", Kind::Text),
    todo("display.expression_min_size", FONT_SIZE),
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
    todo(
        "keys.pad",
        Kind::Record(&[
            ("gap", SIZE),
            ("radius", Kind::Radius),
            ("min_height", SIZE),
            ("font", Kind::Font),
        ]),
    ),
    // -- History and memory panel.
    todo("panel", Kind::Surface),
    todo("panel.sheet", Kind::Surface),
    row("panel.tabs.stretch", Kind::Bool, |_| Value::Bool(true)),
    row("panel.tabs.gap", SIZE, |_| px(0.0)),
    row("panel.tabs.item", Kind::Control, |s| {
        let c = &s.t.colors;
        Value::Control(Box::new(Control {
            min_height: metrics::panel::TAB_H,
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
    todo("date.label", Kind::Text),
    todo("date.field", Kind::Control),
    todo("date.result_panel", Kind::Surface),
    todo("date.result", SIZE_FONT),
    todo("date.add_result", SIZE_FONT),
    todo("date.subresult", Kind::Text),
    todo("date.calendar.title", Kind::Text),
    todo("date.calendar.weekday", Kind::Text),
    todo("date.calendar.day", Kind::Control),
    todo("date.calendar.other_month", Kind::Color),
    todo("date.calendar.today", Kind::Opt(&Kind::Border)),
    todo("date.calendar.focus", Kind::FocusRing),
    // -- Converter.
    todo("converter.field", Kind::Control),
    todo(
        "converter.marker",
        Kind::Opt(&Kind::Record(&[
            ("color", Kind::Color),
            ("width", SIZE),
            ("height", SIZE),
        ])),
    ),
    todo(
        "converter.value",
        Kind::Record(&[
            ("size", FONT_SIZE),
            ("font", Kind::Font),
            ("checked_weight", Kind::Num(1.0, 1000.0)),
            ("focus", Kind::FocusRing),
        ]),
    ),
    todo("converter.symbol", Kind::Text),
    todo("converter.unit", Kind::Control),
    todo("converter.swap", Kind::Control),
    todo("converter.caption", Kind::Text),
    todo("converter.chip", Kind::Control),
    todo("converter.whimsy", Kind::State),
    todo("converter.whimsy_icon", Kind::Bool),
    todo("converter.ratio", Kind::Text),
    todo("converter.timestamp", Kind::Text),
    todo("converter.link", Kind::Control),
    // -- Graphing.
    todo("graph.side", Kind::Surface),
    todo("graph.divider", Kind::Opt(&Kind::Color)),
    todo("graph.row", Kind::Surface),
    todo("graph.row_error", Kind::State),
    todo("graph.entry", Kind::Field),
    todo("graph.row_button", Kind::Control),
    todo("graph.error", Kind::Text),
    todo("graph.swatch.size", SIZE),
    todo("graph.swatch.hidden_opacity", UNIT),
    todo(
        "graph.swatch.glow",
        Kind::Opt(&Kind::Record(&[
            ("blur", Kind::Px(0.0, 100.0)),
            ("spread", Kind::Px(-1000.0, 1000.0)),
        ])),
    ),
    todo("graph.swatch.active_scale", Kind::Num(0.0, 4.0)),
    todo("graph.swatch.transition", Kind::Transition),
    todo("graph.add", Kind::Control),
    todo("graph.variable", Kind::Text),
    todo("graph.analysis.title", Kind::Text),
    todo("graph.analysis.card", Kind::Surface),
    todo("graph.analysis.card_title", Kind::Text),
    todo("graph.analysis.value", Kind::Text),
    todo("graph.analysis.text", Kind::Text),
    todo("graph.analysis.note", Kind::Text),
    todo("graph.toolbar", Kind::Surface),
    todo("graph.toolbar_button", Kind::Control),
    todo("graph.canvas.background", Kind::Paint),
    todo("graph.canvas.radius", SIZE),
    todo("graph.canvas.border", Kind::Opt(&Kind::Border)),
    todo("graph.canvas.grid_minor", Kind::Color),
    todo("graph.canvas.grid_major", Kind::Color),
    todo(
        "graph.canvas.axis",
        Kind::Record(&[("color", Kind::Color), ("width", Kind::Px(0.0, 64.0))]),
    ),
    todo("graph.canvas.label", Kind::Text),
    todo("graph.canvas.fill_opacity", UNIT),
    todo(
        "graph.canvas.glow",
        Kind::Opt(&Kind::Record(&[
            ("blur", Kind::Px(0.0, 100.0)),
            ("width", Kind::Num(0.0, 64.0)),
            ("opacity", UNIT),
        ])),
    ),
    todo("graph.canvas.trace.hair_opacity", UNIT),
    todo("graph.canvas.trace.hair_dash", Kind::Opt(&Kind::Dash)),
    todo(
        "graph.canvas.trace.halo",
        Kind::Opt(&Kind::Record(&[
            ("grow", Kind::Px(0.0, 64.0)),
            ("color", Kind::ColorOr("series")),
            ("opacity", UNIT),
        ])),
    ),
    todo("graph.canvas.trace.ring", Kind::Opt(&Kind::Border)),
    todo(
        "graph.canvas.trace.bubble",
        Kind::Record(&[
            ("background", Kind::Color),
            ("color", Kind::Color),
            ("radius", SIZE),
            ("font", Kind::Font),
            ("border", Kind::Opt(&Kind::Border)),
            ("shadow", Kind::Shadows),
        ]),
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
