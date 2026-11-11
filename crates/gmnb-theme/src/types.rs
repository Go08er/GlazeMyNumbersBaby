//! The typed values of the theme API (docs/theming.md, "Values"): what a
//! checked style is made of, and what the draw API's paints are.
//!
//! Everything here is plain data: no Luau, no toolkit. [`crate::check`]
//! makes these from what a script returned; [`crate::css`] writes them as
//! CSS.

use std::collections::BTreeMap;

/// Straight (non-premultiplied) RGBA, each 0..1 (`appcore::palette::Rgba`).
pub type Rgba = [f32; 4];

pub const TRANSPARENT: Rgba = [0.0, 0.0, 0.0, 0.0];
pub const WHITE: Rgba = [1.0, 1.0, 1.0, 1.0];
pub const BLACK: Rgba = [0.0, 0.0, 0.0, 1.0];

/// `c` with its opacity multiplied by `a`.
pub fn alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * a]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientKind {
    Linear,
    Radial,
    Conic,
}

/// How far a radial gradient placed by `at` reaches (CSS's keywords).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extent {
    ClosestSide,
    FarthestSide,
    ClosestCorner,
    FarthestCorner,
}

impl Extent {
    pub const ALL: [(&'static str, Extent); 4] = [
        ("closest-side", Extent::ClosestSide),
        ("farthest-side", Extent::FarthestSide),
        ("closest-corner", Extent::ClosestCorner),
        ("farthest-corner", Extent::FarthestCorner),
    ];

    pub fn name(self) -> &'static str {
        Extent::ALL
            .iter()
            .find(|(_, e)| *e == self)
            .map_or("farthest-corner", |(n, _)| n)
    }
}

/// A gradient placed in pixels: draw hooks only (a style refuses it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pixels {
    /// Linear.
    pub from: Option<[f32; 2]>,
    pub to: Option<[f32; 2]>,
    /// Radial and conic.
    pub center: Option<[f32; 2]>,
    /// Radial.
    pub radius: Option<f32>,
    pub radius_y: Option<f32>,
}

impl Pixels {
    pub fn is_empty(&self) -> bool {
        *self == Pixels::default()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    /// 2 to 16.
    pub colors: Vec<Rgba>,
    /// One per colour, 0 to 1, never decreasing.
    pub stops: Vec<f32>,
    /// Linear: CSS degrees (0 up, 90 right, 180 down). Conic: where it
    /// starts.
    pub angle: f32,
    /// Radial and conic: the centre as fractions of the box.
    pub at: [f32; 2],
    /// Radial, placed by `at`.
    pub extent: Extent,
    pub pixels: Pixels,
}

/// A colour or a gradient.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Color(Rgba),
    Gradient(Gradient),
}

impl Paint {
    /// The colours a text on this paint must be read against: a colour's
    /// own, or a gradient's two ends and its middle.
    pub fn samples(&self) -> Vec<Rgba> {
        match self {
            Paint::Color(c) => vec![*c],
            Paint::Gradient(g) => {
                let (Some(&a), Some(&b)) = (g.colors.first(), g.colors.last()) else {
                    return Vec::new();
                };
                let mid = g.at_stop(0.5);
                vec![a, mid, b]
            }
        }
    }
}

impl Gradient {
    /// The colour at `t` (0 to 1) along the gradient.
    pub fn at_stop(&self, t: f32) -> Rgba {
        let n = self.colors.len().min(self.stops.len());
        if n == 0 {
            return TRANSPARENT;
        }
        if t <= self.stops[0] {
            return self.colors[0];
        }
        for i in 1..n {
            let (s0, s1) = (self.stops[i - 1], self.stops[i]);
            if t <= s1 {
                let k = if s1 > s0 { (t - s0) / (s1 - s0) } else { 1.0 };
                let (a, b) = (self.colors[i - 1], self.colors[i]);
                return [0, 1, 2, 3].map(|c| a[c] + (b[c] - a[c]) * k);
            }
        }
        self.colors[n - 1]
    }
}

/// Up to [`Fill::MAX`] paints, topmost first. Empty: nothing is painted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fill(pub Vec<Paint>);

impl Fill {
    pub const MAX: usize = 4;

    pub fn none() -> Fill {
        Fill(Vec::new())
    }

    pub fn color(c: Rgba) -> Fill {
        Fill(vec![Paint::Color(c)])
    }

    /// The fill as one colour, if that is all it is (an empty fill is
    /// transparent).
    pub fn as_color(&self) -> Option<Rgba> {
        match self.0.as_slice() {
            [] => Some(TRANSPARENT),
            [Paint::Color(c)] => Some(*c),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Rgba,
    pub inset: bool,
}

/// Drawn inside the element's edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub width: f32,
    pub color: Rgba,
}

/// Top-left, top-right, bottom-right, bottom-left. 999 or more is fully
/// round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radius(pub [f32; 4]);

impl Radius {
    pub fn all(r: f32) -> Radius {
        Radius([r; 4])
    }

    /// One number for code that rounds all corners alike (the largest).
    pub fn uniform(&self) -> f32 {
        self.0.iter().copied().fold(0.0, f32::max)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spacing {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Spacing {
    pub fn all(v: f32) -> Spacing {
        Spacing::trbl(v, v, v, v)
    }

    /// `{ vertical, horizontal }`.
    pub fn vh(v: f32, h: f32) -> Spacing {
        Spacing::trbl(v, h, v, h)
    }

    pub fn trbl(top: f32, right: f32, bottom: f32, left: f32) -> Spacing {
        Spacing {
            top,
            right,
            bottom,
            left,
        }
    }
}

/// The bundled Inter's family name (`fc-scan`), not "Inter".
pub const INTER: &str = "Inter Variable";
/// Inter's optical size axis, which the app sets to the size in pixels
/// kept within this range (DGMNB's rule).
pub const OPSZ: &str = "opsz";
pub const OPSZ_RANGE: (f32, f32) = (14.0, 32.0);

/// A resolved font: every field present.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    /// Family names to fall back through. Never empty once resolved.
    pub family: Vec<String>,
    /// Pixels.
    pub size: f32,
    /// 1 to 1000.
    pub weight: f32,
    pub italic: bool,
    /// Letter spacing in em.
    pub spacing: f32,
    /// OpenType features: a four-character tag and its value (1: on).
    pub features: Vec<(String, u32)>,
    /// A variable font's axes: a four-character tag and its value.
    pub variations: Vec<(String, f32)>,
}

impl Font {
    pub fn new(family: &[String], size: f32, weight: f32) -> Font {
        Font {
            family: family.to_vec(),
            size,
            weight,
            italic: false,
            spacing: 0.0,
            features: Vec::new(),
            variations: Vec::new(),
        }
    }

    pub fn with_features(mut self, tags: &[&str]) -> Font {
        self.features = tags.iter().map(|t| (t.to_string(), 1)).collect();
        self
    }

    /// The axes to set: the font's own, and for the bundled Inter its
    /// optical size (the size in pixels, within [`OPSZ_RANGE`]) unless
    /// the font names one.
    pub fn effective_variations(&self) -> Vec<(String, f32)> {
        let mut v = self.variations.clone();
        let inter = self.family.first().is_some_and(|f| f == INTER);
        if inter && !v.iter().any(|(tag, _)| tag == OPSZ) {
            v.push((
                OPSZ.to_string(),
                self.size.clamp(OPSZ_RANGE.0, OPSZ_RANGE.1),
            ));
        }
        v
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Overshoots a little: `cubic-bezier(0.34, 1.56, 0.64, 1)`.
    Spring,
    Bezier([f32; 4]),
}

impl Ease {
    pub const NAMED: [(&'static str, Ease); 6] = [
        ("linear", Ease::Linear),
        ("ease", Ease::Ease),
        ("ease-in", Ease::EaseIn),
        ("ease-out", Ease::EaseOut),
        ("ease-in-out", Ease::EaseInOut),
        ("spring", Ease::Spring),
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TransitionProp {
    Background,
    Color,
    Shadow,
    Transform,
    Filter,
    Opacity,
}

impl TransitionProp {
    pub const ALL: [(&'static str, TransitionProp); 6] = [
        ("background", TransitionProp::Background),
        ("color", TransitionProp::Color),
        ("shadow", TransitionProp::Shadow),
        ("transform", TransitionProp::Transform),
        ("filter", TransitionProp::Filter),
        ("opacity", TransitionProp::Opacity),
    ];
}

/// Which properties ease when an element changes state. Empty: none do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transition(pub Vec<(TransitionProp, f32, Ease)>);

/// The app's icons are lines on a 24-unit grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Icon {
    /// The drawn size, in pixels.
    pub size: f32,
    /// The line's width in pixels; `None`: from `icons.stroke` and
    /// `icons.taper`.
    pub stroke: Option<f32>,
    /// `None`: the control's own colour.
    pub color: Option<Rgba>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusRing {
    pub width: f32,
    pub color: Rgba,
    /// The gap to the element's edge (negative: inside).
    pub offset: f32,
    /// `None`: the ring's corners follow the control's.
    pub radius: Option<f32>,
    /// A line along both sides of the ring, around a keypad's keys: a
    /// ring that may lie on its own colour stays seen.
    pub edge: Option<Border>,
}

/// How a control differs while in a state. What is `None` stays as it is
/// at rest. A theme's state replaces the base state whole.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub background: Option<Fill>,
    /// A colour laid over whatever the background is.
    pub overlay: Option<Rgba>,
    pub color: Option<Rgba>,
    /// Text and icon at this opacity, the background unchanged.
    pub fade: Option<f32>,
    /// `Some(None)`: the border is removed.
    pub border: Option<Option<Border>>,
    pub shadow: Option<Vec<Shadow>>,
    pub opacity: Option<f32>,
    pub brightness: Option<f32>,
    pub saturation: Option<f32>,
    pub scale: Option<f32>,
    /// Degrees.
    pub rotate: Option<f32>,
    pub font_weight: Option<f32>,
    pub icon_color: Option<Rgba>,
    /// The transition into this state.
    pub transition: Option<Transition>,
}

impl State {
    pub fn overlay(c: Rgba) -> State {
        State {
            overlay: Some(c),
            ..State::default()
        }
    }

    pub fn filled(background: Rgba, color: Rgba) -> State {
        State {
            background: Some(Fill::color(background)),
            color: Some(color),
            ..State::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub color: Rgba,
    pub font: Font,
    /// A text shadow list.
    pub shadow: Vec<Shadow>,
    pub margin: Spacing,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub background: Fill,
    pub border: Option<Border>,
    pub shadow: Vec<Shadow>,
    pub radius: Radius,
    pub padding: Spacing,
    pub margin: Spacing,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    pub background: Fill,
    pub color: Rgba,
    pub border: Option<Border>,
    pub shadow: Vec<Shadow>,
    pub radius: Radius,
    pub font: Font,
    pub padding: Spacing,
    pub margin: Spacing,
    pub min_width: f32,
    pub min_height: f32,
    pub opacity: f32,
    pub icon: Icon,
    pub transition: Transition,
    pub hover: State,
    pub active: State,
    pub checked: State,
    pub disabled: State,
}

/// A text field.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub background: Fill,
    pub color: Rgba,
    pub border: Option<Border>,
    pub shadow: Vec<Shadow>,
    pub radius: Radius,
    pub font: Font,
    pub padding: Spacing,
    pub min_height: f32,
    pub placeholder: Rgba,
    pub selection: Rgba,
    pub caret: Rgba,
    pub focus: State,
    pub error: State,
}

/// One resolved value of the style, of whichever kind its row of the
/// schema says ([`crate::schema::Kind`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Number(f64),
    Color(Rgba),
    Paint(Paint),
    Fill(Fill),
    Shadows(Vec<Shadow>),
    Radius(Radius),
    Spacing(Spacing),
    Font(Font),
    Transition(Transition),
    State(State),
    Icon(Icon),
    FocusRing(FocusRing),
    Text(Text),
    Surface(Surface),
    Control(Box<Control>),
    Field(Box<Field>),
    /// One of a fixed set of words, or a list of them.
    Word(&'static str),
    Words(Vec<&'static str>),
    /// Font family names.
    Families(Vec<String>),
    Colors(Vec<Rgba>),
    Numbers(Vec<f64>),
    /// A border, or anything else that can be absent (`false` in a theme).
    Border(Border),
    Record(BTreeMap<&'static str, Value>),
    /// Cleared by the theme with `false`, or absent in the base look.
    Absent,
}
