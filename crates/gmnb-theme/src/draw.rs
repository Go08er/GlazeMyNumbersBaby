//! The draw API (docs/theming.md, "The draw API"): what a draw hook calls,
//! recorded into a bounded display list of typed operations. Nothing here
//! knows the toolkit: the app replays a [`DisplayList`] into its snapshot,
//! and tests read it as data.
//!
//! One [`Api`] belongs to one theme. Its `draw` table's functions record
//! into the [`Recorder`] they share, which only takes calls while a hook
//! runs ([`Api::begin`] to [`Api::finish`]).
//!
//! # What goes wrong
//!
//! - An argument of the wrong type or shape is a Luau error at the call,
//!   with the theme's line: the hook fails unless it catches it.
//! - Going over a cap of [`DrawLimits`] is an error too, and it also marks
//!   the frame: whatever the hook does after (catching the error
//!   included), [`Api::finish`] fails and nothing is drawn.
//! - Numbers that aren't finite are refused; opacities, colour parts and
//!   gradient stops out of range are brought into range without a word.
//! - Pushes and saves left open are closed, and noted once.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use mlua::{Function, Lua, LuaString, Table, Value};

use crate::ThemeValue;
use crate::check::{self, Notes, parse_hex};
use crate::field::{FieldPicture, Fields};
use crate::path::{self, Path};
use crate::picture::{Canvas, ImageId, PictureKind, Pictures};
use crate::types::*;

/// The draw API's caps (docs/theming.md, "Limits").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawLimits {
    /// Draw calls per `draw`.
    pub ops: usize,
    /// Pushes open at once (a mask or a blend counts twice: it takes two
    /// pops).
    pub pushes: usize,
    /// Saves open at once.
    pub saves: usize,
    /// Path segments per frame, over all paths filled or stroked.
    pub path_segments: usize,
    /// Text per `draw:text` or `draw:text_size` call, in bytes.
    pub text_bytes: usize,
    /// Text calls per frame.
    pub text_calls: usize,
    /// A blur's radius and a shadow's blur, in pixels.
    pub blur: u32,
    /// An image file, in bytes.
    pub image_file: u64,
    /// An image's width and height, decoded.
    pub image_side: u32,
    /// Images a theme may load.
    pub images: usize,
    /// All of a theme's images decoded (four bytes a pixel), in bytes.
    pub image_bytes: u64,
    /// A noise picture's side.
    pub noise_side: u32,
    /// Noise pictures a theme may make.
    pub noises: usize,
    /// An animation file, in bytes.
    pub animation_file: u64,
    /// An animation's canvas: its width and height.
    pub animation_side: u32,
    /// An animation's frames.
    pub animation_frames: u32,
    /// Animations a theme may load.
    pub animations: usize,
    /// A picture the theme fills (`gmnb.pixels`): its width and height.
    pub canvas_side: u32,
    /// Pictures a theme may make to fill.
    pub canvases: usize,
    /// Fields per `draw`.
    pub fields: usize,
    /// Ramps a theme may make.
    pub ramps: usize,
}

impl Default for DrawLimits {
    fn default() -> Self {
        DrawLimits {
            ops: 4096,
            pushes: 32,
            saves: 32,
            path_segments: 16_384,
            text_bytes: 256,
            text_calls: 64,
            blur: 100,
            image_file: 4 << 20,
            image_side: 2048,
            images: 16,
            image_bytes: 32 << 20,
            noise_side: 512,
            noises: 8,
            animation_file: 8 << 20,
            animation_side: 1024,
            animation_frames: 2048,
            animations: 4,
            canvas_side: 512,
            canvases: 4,
            fields: 2,
            ramps: 8,
        }
    }
}

/// `x, y, width, height`.
pub type Rect = [f32; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    Winding,
    EvenOdd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

/// A stroke (`GskStroke`).
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    /// Dash lengths, on and off in turn; empty: a solid line.
    pub dash: Vec<f32>,
    pub dash_offset: f32,
}

/// How a picture drawn at another size than its own is sampled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    /// Smooth, and clean when drawn much smaller. A still's default.
    Trilinear,
    Linear,
    /// Hard pixels.
    Nearest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskMode {
    Alpha,
    InvertedAlpha,
    Luminance,
    InvertedLuminance,
}

impl MaskMode {
    pub const ALL: [(&'static str, MaskMode); 4] = [
        ("alpha", MaskMode::Alpha),
        ("inverted-alpha", MaskMode::InvertedAlpha),
        ("luminance", MaskMode::Luminance),
        ("inverted-luminance", MaskMode::InvertedLuminance),
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Color,
    Hue,
    Saturation,
    Luminosity,
}

impl BlendMode {
    pub const ALL: [(&'static str, BlendMode); 15] = [
        ("multiply", BlendMode::Multiply),
        ("screen", BlendMode::Screen),
        ("overlay", BlendMode::Overlay),
        ("darken", BlendMode::Darken),
        ("lighten", BlendMode::Lighten),
        ("color-dodge", BlendMode::ColorDodge),
        ("color-burn", BlendMode::ColorBurn),
        ("hard-light", BlendMode::HardLight),
        ("soft-light", BlendMode::SoftLight),
        ("difference", BlendMode::Difference),
        ("exclusion", BlendMode::Exclusion),
        ("color", BlendMode::Color),
        ("hue", BlendMode::Hue),
        ("saturation", BlendMode::Saturation),
        ("luminosity", BlendMode::Luminosity),
    ];
}

/// One recorded draw call. Each maps onto one GtkSnapshot call or a fixed
/// few (the contract's table); the app's replay is the only reader that
/// needs the toolkit.
///
/// More kinds may come, so a reader must have an arm for the ones it
/// doesn't know.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Op {
    Rect {
        rect: Rect,
        paint: Paint,
    },
    RoundedRect {
        rect: Rect,
        radius: Radius,
        paint: Paint,
    },
    Circle {
        center: [f32; 2],
        radius: f32,
        paint: Paint,
    },
    Fill {
        path: Rc<Path>,
        paint: Paint,
        rule: FillRule,
    },
    Stroke {
        path: Rc<Path>,
        stroke: Stroke,
        paint: Paint,
    },
    Border {
        rect: Rect,
        radius: Radius,
        width: f32,
        color: Rgba,
    },
    Shadow {
        rect: Rect,
        radius: Radius,
        shadow: Shadow,
    },
    /// `at` is the text's top-left.
    Text {
        text: String,
        at: [f32; 2],
        font: Font,
        paint: Paint,
    },
    /// One laid-out character of the display's value (`index` from 0), or
    /// of the value before it (`previous`), at its place.
    Glyph {
        previous: bool,
        index: u32,
        paint: Paint,
    },
    /// The picture scaled into `rect`.
    Image {
        image: ImageId,
        rect: Rect,
        filter: Filter,
    },
    /// The picture repeated at its own size over `rect` (of an animation,
    /// its first frame).
    Tile {
        image: ImageId,
        rect: Rect,
    },
    /// One frame of an animation scaled into `rect`. `still`: nothing
    /// moves (reduced motion, a preview), and `frame` is the first; the
    /// app shows it without disturbing an animation that plays elsewhere.
    Animation {
        image: ImageId,
        /// Which frame, from 0.
        frame: u32,
        still: bool,
        rect: Rect,
        filter: Filter,
    },
    /// A field's picture scaled smoothly over `rect`.
    Field {
        picture: Rc<FieldPicture>,
        rect: Rect,
    },
    PushClip {
        rect: Rect,
        radius: Option<Radius>,
    },
    PushOpacity(f32),
    PushBlur(f32),
    PushShadow(Vec<Shadow>),
    /// Two parts, each ended by a [`Op::Pop`]: the mask, then what shows
    /// through it.
    PushMask(MaskMode),
    /// Two parts, each ended by a [`Op::Pop`]: what is underneath, then
    /// what is blended onto it.
    PushBlend(BlendMode),
    Pop,
    Save,
    Restore,
    Translate(f32, f32),
    Scale(f32, f32),
    /// Degrees.
    Rotate(f32),
}

/// What one `draw` call drew: balanced (every push popped, every save
/// restored, in order), within the caps, every number finite.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    pub ops: Vec<Op>,
}

impl DisplayList {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// Measures text for `draw:text_size`. The app's is Pango; without one
/// ([`Api::set_measure`]) a size is estimated from the font's size alone.
pub trait TextMeasure {
    /// The width and height of `text` set in `font`, in pixels.
    fn measure(&self, text: &str, font: &Font) -> (f32, f32);
}

/// Half an em a character, 1.2 em a line: what `draw:text_size` answers
/// when nothing can measure (tests, a theme being listed).
pub fn estimate(text: &str, font: &Font) -> (f32, f32) {
    (
        text.chars().count() as f32 * font.size * 0.5,
        font.size * 1.2,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Open {
    Push,
    Save,
}

/// What the `draw` table's functions record into.
pub struct Recorder {
    limits: DrawLimits,
    /// A hook is running.
    active: bool,
    ops: Vec<Op>,
    open: Vec<Open>,
    pushes: usize,
    saves: usize,
    segments: usize,
    text_calls: usize,
    /// The cap this frame went over: the frame is dropped.
    over: Option<String>,
    /// How many glyphs the display's value has, and the one before it.
    glyphs: [u32; 2],
    /// What `draw:text` fonts merge onto.
    base_font: Font,
    notes: Vec<String>,
    /// The frame's place on its layer's clock, and whether anything may
    /// move: what an animation drawn without a `time` plays by.
    time: f64,
    motion: bool,
    /// A still picture is being drawn (a preview).
    still: bool,
    /// Which layer, in which place, is drawing.
    pub(crate) layer: u64,
    /// Fields drawn so far this frame.
    pub(crate) fields: usize,
    /// Seconds until an animation drawn this frame shows its next frame.
    due: Option<f64>,
    /// Seconds until a picture this frame was refused (a field coloured,
    /// a picture changed, too soon after the last) can be made.
    retry: Option<f64>,
}

impl Recorder {
    fn new(limits: DrawLimits) -> Recorder {
        Recorder {
            limits,
            active: false,
            ops: Vec::new(),
            open: Vec::new(),
            pushes: 0,
            saves: 0,
            segments: 0,
            text_calls: 0,
            over: None,
            glyphs: [0, 0],
            base_font: Font::new(&[INTER.to_string()], 15.0, 400.0),
            notes: Vec::new(),
            time: 0.0,
            motion: true,
            still: false,
            layer: 0,
            fields: 0,
            due: None,
            retry: None,
        }
    }

    /// A hook is drawing a still picture (a preview's tile).
    pub(crate) fn still(&self) -> bool {
        self.still
    }

    /// A picture was refused for being asked for too soon, and can be
    /// had in `wait` seconds: if a hook is drawing, its layer is to be
    /// drawn once more then, so that what stands on screen is never a
    /// picture the theme has since replaced.
    pub(crate) fn retry(&mut self, wait: f64) {
        if self.active {
            self.retry = Some(self.retry.map_or(wait, |r| r.min(wait)));
        }
    }

    pub(crate) fn limits(&self) -> &DrawLimits {
        &self.limits
    }

    /// A call may record: a hook is running and the frame is within its
    /// caps.
    pub(crate) fn ready(&self, what: &str) -> mlua::Result<()> {
        if !self.active {
            return Err(bad(what, "can only be called while a draw hook runs"));
        }
        match &self.over {
            Some(cap) => Err(mlua::Error::runtime(cap.clone())),
            None => Ok(()),
        }
    }

    /// Going over a cap: an error, and the frame is dropped whatever the
    /// hook does next.
    pub(crate) fn over(&mut self, what: &str, cap: impl std::fmt::Display) -> mlua::Error {
        let message = format!("draw:{what}: {cap}");
        self.over.get_or_insert_with(|| message.clone());
        mlua::Error::runtime(message)
    }

    pub(crate) fn push(&mut self, what: &str, op: Op) -> mlua::Result<()> {
        self.ready(what)?;
        if self.ops.len() >= self.limits.ops {
            return Err(self.over(
                what,
                format!("more than {} draw calls in one frame", self.limits.ops),
            ));
        }
        self.ops.push(op);
        Ok(())
    }

    /// Opens `parts` pushes (a mask or a blend has two).
    fn open_push(&mut self, what: &str, op: Op, parts: usize) -> mlua::Result<()> {
        self.ready(what)?;
        if self.pushes + parts > self.limits.pushes {
            return Err(self.over(
                what,
                format!("more than {} pushes open at once", self.limits.pushes),
            ));
        }
        self.push(what, op)?;
        self.pushes += parts;
        self.open.extend(std::iter::repeat_n(Open::Push, parts));
        Ok(())
    }

    pub(crate) fn note(&mut self, message: String) {
        if self.notes.len() < 32 && !self.notes.contains(&message) {
            self.notes.push(message);
        }
    }
}

fn bad(what: &str, problem: impl std::fmt::Display) -> mlua::Error {
    mlua::Error::runtime(format!("draw:{what}: {problem}"))
}

/// The largest coordinate taken: far beyond any window, small enough that
/// the renderers' arithmetic stays exact.
const FAR: f64 = 1.0e7;

fn coord(what: &str, n: f64) -> mlua::Result<f32> {
    if n.is_finite() {
        Ok(n.clamp(-FAR, FAR) as f32)
    } else {
        Err(bad(what, "a number that isn't finite"))
    }
}

pub(crate) fn rect(what: &str, x: f64, y: f64, w: f64, h: f64) -> mlua::Result<Rect> {
    Ok([
        coord(what, x)?,
        coord(what, y)?,
        coord(what, w)?.max(0.0),
        coord(what, h)?.max(0.0),
    ])
}

fn number(what: &str, v: &Value) -> mlua::Result<f64> {
    match v {
        Value::Integer(i) => Ok(*i as f64),
        Value::Number(n) if n.is_finite() => Ok(*n),
        Value::Number(_) => Err(bad(what, "a number that isn't finite")),
        other => Err(bad(
            what,
            format!("expected a number, got {}", other.type_name()),
        )),
    }
}

/// The field names read on every call, made once: reading a table by a
/// string made on the spot costs an allocation under the heap cap.
pub(crate) struct Keys {
    r: LuaString,
    g: LuaString,
    b: LuaString,
    a: LuaString,
    kind: LuaString,
    index: LuaString,
    previous: LuaString,
    id: LuaString,
}

impl Keys {
    fn new(lua: &Lua) -> mlua::Result<Keys> {
        Ok(Keys {
            r: lua.create_string("r")?,
            g: lua.create_string("g")?,
            b: lua.create_string("b")?,
            a: lua.create_string("a")?,
            kind: lua.create_string("kind")?,
            index: lua.create_string("index")?,
            previous: lua.create_string("previous")?,
            id: lua.create_string("id")?,
        })
    }
}

fn unit(what: &str, v: &Value) -> mlua::Result<f32> {
    Ok(number(what, v)?.clamp(0.0, 1.0) as f32)
}

/// A Color: a hex string, or `{ r, g, b, a }`.
fn color(keys: &Keys, what: &str, v: &Value) -> mlua::Result<Rgba> {
    match v {
        Value::String(s) => parse_hex(&s.to_str()?).ok_or_else(|| {
            bad(
                what,
                "a colour is \"#rrggbb\", \"#rrggbbaa\" or { r, g, b, a }",
            )
        }),
        Value::Table(t) => color_table(keys, what, t),
        other => Err(bad(
            what,
            format!("expected a colour, got {}", other.type_name()),
        )),
    }
}

fn color_table(keys: &Keys, what: &str, t: &Table) -> mlua::Result<Rgba> {
    let part = |key: &LuaString| t.raw_get::<Value>(key);
    let (r, g, b) = (part(&keys.r)?, part(&keys.g)?, part(&keys.b)?);
    if r.is_nil() || g.is_nil() || b.is_nil() {
        return Err(bad(what, "a colour table needs r, g and b"));
    }
    let a = match part(&keys.a)? {
        Value::Nil => 1.0,
        a => unit(what, &a)?,
    };
    Ok([unit(what, &r)?, unit(what, &g)?, unit(what, &b)?, a])
}

fn point(what: &str, v: &Value) -> mlua::Result<[f32; 2]> {
    let Value::Table(t) = v else {
        return Err(bad(what, "a point is { x, y }"));
    };
    Ok([
        coord(what, number(what, &t.raw_get::<Value>(1)?)?)?,
        coord(what, number(what, &t.raw_get::<Value>(2)?)?)?,
    ])
}

/// A gradient table, as `gmnb.linear`, `gmnb.radial` and `gmnb.conic`
/// make them (or written by hand). Placement in pixels is taken.
fn gradient(keys: &Keys, what: &str, t: &Table, kind: &Value) -> mlua::Result<Gradient> {
    let kind = match kind {
        Value::String(s) => match &*s.as_bytes() {
            b"linear" => GradientKind::Linear,
            b"radial" => GradientKind::Radial,
            b"conic" => GradientKind::Conic,
            _ => return Err(bad(what, "a gradient's kind is linear, radial or conic")),
        },
        _ => return Err(bad(what, "a gradient's kind is linear, radial or conic")),
    };
    let mut g = Gradient {
        kind,
        colors: Vec::new(),
        stops: Vec::new(),
        angle: if kind == GradientKind::Linear {
            180.0
        } else {
            0.0
        },
        at: [0.5, 0.5],
        extent: Extent::FarthestCorner,
        pixels: Pixels::default(),
    };
    let mut problem = None;
    t.for_each::<Value, Value>(|k, v| {
        let Value::String(k) = k else {
            return Ok(());
        };
        let found: mlua::Result<()> = (|| {
            match &*k.as_bytes() {
                b"colors" => {
                    let Value::Table(list) = &v else {
                        return Err(bad(what, "a gradient needs a list of 2 to 16 colours"));
                    };
                    let n = list.raw_len();
                    if !(2..=16).contains(&n) {
                        return Err(bad(what, format!("{n} colours; a gradient takes 2 to 16")));
                    }
                    for i in 1..=n {
                        g.colors
                            .push(color(keys, what, &list.raw_get::<Value>(i)?)?);
                    }
                }
                b"stops" => {
                    let Value::Table(list) = &v else {
                        return Err(bad(what, "a gradient's stops are a list of numbers"));
                    };
                    for i in 1..=list.raw_len() {
                        let s = unit(what, &list.raw_get::<Value>(i)?)?;
                        if g.stops.last().is_some_and(|last| s < *last) {
                            return Err(bad(what, "a gradient's stops must never decrease"));
                        }
                        g.stops.push(s);
                    }
                }
                b"angle" => g.angle = coord(what, number(what, &v)?)?,
                b"at" => g.at = point(what, &v)?,
                b"from" => g.pixels.from = Some(point(what, &v)?),
                b"to" => g.pixels.to = Some(point(what, &v)?),
                b"center" => g.pixels.center = Some(point(what, &v)?),
                b"radius" => match &v {
                    Value::String(s) => {
                        let name = s.to_str()?;
                        match Extent::ALL.iter().find(|(n, _)| *n == &*name) {
                            Some((_, e)) => g.extent = *e,
                            None => {
                                return Err(bad(
                                    what,
                                    "a gradient's radius is a number or a CSS extent keyword",
                                ));
                            }
                        }
                    }
                    v => g.pixels.radius = Some(coord(what, number(what, v)?)?.max(0.0)),
                },
                b"radius_y" => {
                    g.pixels.radius_y = Some(coord(what, number(what, &v)?)?.max(0.0));
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(e) = found {
            problem.get_or_insert(e);
        }
        Ok(())
    })?;
    if let Some(e) = problem {
        return Err(e);
    }
    let n = g.colors.len();
    if n < 2 {
        return Err(bad(what, "a gradient needs a list of 2 to 16 colours"));
    }
    if g.stops.is_empty() {
        g.stops = (0..n).map(|i| i as f32 / (n - 1) as f32).collect();
    } else if g.stops.len() != n {
        return Err(bad(
            what,
            format!("a gradient of {n} colours needs {n} stops, one per colour"),
        ));
    }
    Ok(g)
}

/// A Paint: a colour or a gradient.
fn paint(keys: &Keys, what: &str, v: &Value) -> mlua::Result<Paint> {
    match v {
        Value::Table(t) => match t.raw_get::<Value>(&keys.kind)? {
            Value::Nil => color_table(keys, what, t).map(Paint::Color),
            kind => gradient(keys, what, t, &kind).map(Paint::Gradient),
        },
        Value::Nil => Err(bad(what, "expected a paint (a colour or a gradient)")),
        v => color(keys, what, v).map(Paint::Color),
    }
}

/// A Radius: a number, or four.
fn radius(what: &str, v: &Value) -> mlua::Result<Radius> {
    match v {
        Value::Table(t) => {
            if t.raw_len() != 4 {
                return Err(bad(what, "a radius is a number, or four"));
            }
            let mut r = [0.0; 4];
            for (i, slot) in r.iter_mut().enumerate() {
                *slot = coord(what, number(what, &t.raw_get::<Value>(i + 1)?)?)?.max(0.0);
            }
            Ok(Radius(r))
        }
        v => Ok(Radius::all(coord(what, number(what, v)?)?.max(0.0))),
    }
}

/// A plain value tree from a Luau value, for the arguments the style
/// checks already know how to read (fonts, shadows): small and shallow,
/// read raw, functions refused.
fn plain(what: &str, v: &Value, depth: usize, room: &mut usize) -> mlua::Result<ThemeValue> {
    if *room == 0 || depth > 4 {
        return Err(bad(what, "a table too big or too deep"));
    }
    *room -= 1;
    Ok(match v {
        Value::Nil => ThemeValue::Nil,
        Value::Boolean(b) => ThemeValue::Bool(*b),
        Value::Integer(i) => ThemeValue::Int(*i),
        Value::Number(n) if n.is_finite() => ThemeValue::Number(*n),
        Value::Number(_) => return Err(bad(what, "a number that isn't finite")),
        Value::String(s) => ThemeValue::String(s.to_str()?.to_string()),
        Value::Table(t) => {
            let n = t.raw_len();
            if n > 0 {
                let mut list = Vec::with_capacity(n.min(64));
                for i in 1..=n {
                    list.push(plain(what, &t.raw_get::<Value>(i)?, depth + 1, room)?);
                }
                ThemeValue::Array(list)
            } else {
                let mut map = std::collections::BTreeMap::new();
                let mut problem = None;
                t.for_each::<Value, Value>(|k, v| {
                    let converted = match &k {
                        Value::String(k) => k
                            .to_str()
                            .map(|k| k.to_string())
                            .and_then(|k| plain(what, &v, depth + 1, room).map(|v| (k, v))),
                        _ => Err(bad(what, "a table with keys that aren't names")),
                    };
                    match converted {
                        Ok((k, v)) => {
                            map.insert(k, v);
                        }
                        Err(e) => {
                            problem.get_or_insert(e);
                        }
                    }
                    Ok(())
                })?;
                if let Some(e) = problem {
                    return Err(e);
                }
                ThemeValue::Map(map)
            }
        }
        other => {
            return Err(bad(
                what,
                format!("a {} where a value was expected", other.type_name()),
            ));
        }
    })
}

/// The first note of a check, as the call's error.
fn refused(what: &str, notes: &Notes, fallback: &str) -> mlua::Error {
    match notes.list.first() {
        Some(note) => bad(what, note),
        None => bad(what, fallback),
    }
}

/// A Shadow table; its blur within the cap.
fn shadow(rec: &mut Recorder, what: &str, v: &Value) -> mlua::Result<Shadow> {
    let tree = plain(what, v, 0, &mut 64)?;
    let blur = tree.get("blur").and_then(ThemeValue::as_f64).unwrap_or(0.0);
    if blur > f64::from(rec.limits.blur) {
        return Err(rec.over(
            what,
            format!("a shadow blurred by more than {} px", rec.limits.blur),
        ));
    }
    let mut notes = Notes::default();
    check::shadow(&tree, "shadow", &mut notes)
        .ok_or_else(|| refused(what, &notes, "expected a shadow { x, y, blur, color }"))
}

fn word<T: Copy>(what: &str, v: &Value, words: &[(&'static str, T)]) -> mlua::Result<T> {
    let found = match v {
        Value::String(s) => {
            let s = s.as_bytes();
            words
                .iter()
                .find(|(w, _)| w.as_bytes() == &*s)
                .map(|(_, t)| *t)
        }
        _ => None,
    };
    found.ok_or_else(|| {
        let names: Vec<String> = words.iter().map(|(w, _)| format!("\"{w}\"")).collect();
        bad(what, format!("expected one of {}", names.join(", ")))
    })
}

/// A stroke: a width, or `{ width, cap, join, dash, dash_offset }`.
fn stroke(what: &str, v: &Value) -> mlua::Result<Stroke> {
    let mut s = Stroke {
        width: 1.0,
        cap: LineCap::Butt,
        join: LineJoin::Miter,
        dash: Vec::new(),
        dash_offset: 0.0,
    };
    let width = |v: &Value| -> mlua::Result<f32> {
        let w = coord(what, number(what, v)?)?;
        if w <= 0.0 {
            return Err(bad(what, "a stroke's width must be above 0"));
        }
        Ok(w.min(1000.0))
    };
    match v {
        Value::Table(t) => {
            s.width = width(&t.raw_get::<Value>("width")?)?;
            match t.raw_get::<Value>("cap")? {
                Value::Nil => {}
                v => {
                    s.cap = word(
                        what,
                        &v,
                        &[
                            ("butt", LineCap::Butt),
                            ("round", LineCap::Round),
                            ("square", LineCap::Square),
                        ],
                    )?;
                }
            }
            match t.raw_get::<Value>("join")? {
                Value::Nil => {}
                v => {
                    s.join = word(
                        what,
                        &v,
                        &[
                            ("miter", LineJoin::Miter),
                            ("round", LineJoin::Round),
                            ("bevel", LineJoin::Bevel),
                        ],
                    )?;
                }
            }
            match t.raw_get::<Value>("dash")? {
                Value::Nil => {}
                Value::Table(list) => {
                    let n = list.raw_len();
                    if n > 16 {
                        return Err(bad(what, "a dash takes up to 16 lengths"));
                    }
                    for i in 1..=n {
                        let d = coord(what, number(what, &list.raw_get::<Value>(i)?)?)?;
                        s.dash.push(d.max(0.0));
                    }
                    // GSK refuses a dash that never draws or never moves on.
                    if s.dash.iter().sum::<f32>() <= 0.0 {
                        s.dash.clear();
                    }
                }
                _ => return Err(bad(what, "a dash is a list of lengths")),
            }
            match t.raw_get::<Value>("dash_offset")? {
                Value::Nil => {}
                v => s.dash_offset = coord(what, number(what, &v)?)?,
            }
        }
        v => s.width = width(v)?,
    }
    Ok(s)
}

/// What a theme's draw calls share: the recorder, the theme's pictures and
/// what measures text.
pub(crate) struct Shared {
    pub(crate) recorder: RefCell<Recorder>,
    pub(crate) pictures: RefCell<Pictures>,
    pub(crate) fields: RefCell<Fields>,
    /// Seconds on a clock that only goes forward: what the allowances of
    /// pictures that change and of fields are counted by.
    clock: RefCell<Rc<dyn Fn() -> f64>>,
    /// How much slower than their files say the theme's animations play,
    /// because the app can't decode them faster within its share (1:
    /// they keep up).
    slow: Cell<f64>,
    measure: RefCell<Option<Rc<dyn TextMeasure>>>,
    /// Messages already handed out: each is reported once.
    told: RefCell<HashSet<String>>,
    /// Whether the last frame recorded left pushes or saves open.
    unbalanced: Cell<bool>,
}

impl Shared {
    pub(crate) fn now(&self) -> f64 {
        let clock = self.clock.borrow().clone();
        clock()
    }
}

/// Seconds since the first time anyone asked: the clock allowances are
/// counted by unless the app (or a test) sets another.
fn monotonic() -> f64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
}

/// What a hook's frame is, as far as the draw calls need to know.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Begin {
    /// How many characters the display's value has, and the value before
    /// it (0 outside the display layer).
    pub glyphs: [u32; 2],
    /// `frame.time`: seconds the layer has spent animating.
    pub time: f64,
    /// `frame.motion`.
    pub motion: bool,
    /// A still picture is drawn (a preview's tile): animations show their
    /// first frame.
    pub still: bool,
    /// A number for the layer in the place it is drawn (this keypad's,
    /// that readout's): what a field's allowance and last picture are
    /// kept by.
    pub layer: u64,
}

impl Default for Begin {
    fn default() -> Self {
        Begin {
            glyphs: [0, 0],
            time: 0.0,
            motion: true,
            still: false,
            layer: 0,
        }
    }
}

/// One theme's draw API: its `draw` table, and what was recorded.
pub struct Api {
    shared: Rc<Shared>,
    table: Table,
}

impl Api {
    /// Makes the `draw` table and adds the drawing constructors
    /// (`gmnb.path`, `gmnb.svg_path`, `gmnb.image`, `gmnb.noise`,
    /// `gmnb.animation`, `gmnb.pixels`, `gmnb.ramp`) to `gmnb`. Called
    /// while the engine's globals are registered.
    pub(crate) fn register(
        lua: &Lua,
        gmnb: &Table,
        limits: DrawLimits,
        pictures: Pictures,
    ) -> mlua::Result<Api> {
        let shared = Rc::new(Shared {
            recorder: RefCell::new(Recorder::new(limits)),
            pictures: RefCell::new(pictures),
            fields: RefCell::default(),
            clock: RefCell::new(Rc::new(monotonic)),
            slow: Cell::new(1.0),
            measure: RefCell::new(None),
            told: RefCell::default(),
            unbalanced: Cell::new(false),
        });
        path::register(lua, gmnb)?;
        crate::picture::register(lua, gmnb, &shared)?;
        crate::field::register(lua, gmnb, &shared)?;
        let table = draw_table(lua, &shared)?;
        Ok(Api { shared, table })
    }

    /// The `draw` argument of a hook.
    pub fn table(&self) -> &Table {
        &self.table
    }

    /// What `draw:text_size` measures with.
    pub fn set_measure(&self, measure: Rc<dyn TextMeasure>) {
        self.shared.measure.replace(Some(measure));
    }

    /// What `draw:text` fonts merge onto: the style's body text.
    pub fn set_base_font(&self, font: &Font) {
        let mut rec = self.shared.recorder.borrow_mut();
        if rec.base_font != *font {
            rec.base_font = font.clone();
        }
    }

    /// The clock the allowances of `canvas:set` and `draw:field` are
    /// counted by: seconds, only ever forward.
    pub fn set_clock(&self, clock: Rc<dyn Fn() -> f64>) {
        self.shared.clock.replace(clock);
    }

    /// How much slower than their files say the theme's animations are to
    /// play from now on (1 or more): what the app's player measured it
    /// can decode within its share of a processor core.
    pub fn set_slow(&self, slow: f64) {
        self.shared
            .slow
            .set(if slow.is_finite() { slow.max(1.0) } else { 1.0 });
    }

    /// A hook is about to run: the recorder is emptied and takes calls.
    pub fn begin(&self, frame: &Begin) {
        let Begin {
            glyphs,
            time,
            motion,
            still,
            layer,
        } = *frame;
        let mut rec = self.shared.recorder.borrow_mut();
        rec.time = time;
        rec.motion = motion;
        rec.still = still;
        rec.layer = layer;
        rec.fields = 0;
        rec.due = None;
        rec.retry = None;
        rec.active = true;
        rec.ops.clear();
        rec.open.clear();
        rec.pushes = 0;
        rec.saves = 0;
        rec.segments = 0;
        rec.text_calls = 0;
        rec.over = None;
        rec.glyphs = glyphs;
        rec.notes.clear();
    }

    /// The hook has returned: what it drew, with whatever it left open
    /// closed; or the cap it went over (the frame is dropped). Either way
    /// the recorder takes no more calls.
    pub fn finish(&self) -> Result<DisplayList, String> {
        let mut rec = self.shared.recorder.borrow_mut();
        rec.active = false;
        if let Some(cap) = rec.over.take() {
            rec.ops.clear();
            rec.open.clear();
            return Err(cap);
        }
        let left = rec.open.len();
        self.shared.unbalanced.set(left > 0);
        if left > 0 {
            let (pushes, saves) = (rec.pushes, rec.saves);
            while let Some(open) = rec.open.pop() {
                rec.ops.push(match open {
                    Open::Push => Op::Pop,
                    Open::Save => Op::Restore,
                });
            }
            rec.note(format!(
                "{pushes} pushes and {saves} saves were left open when draw returned; they were closed"
            ));
        }
        Ok(DisplayList {
            ops: std::mem::take(&mut rec.ops),
        })
    }

    /// Seconds until an animation the finished frame drew on its layer's
    /// clock shows its next frame: the layer is drawn again then, as if
    /// `draw` had returned that. `None`: no animation was drawn so.
    pub fn take_due(&self) -> Option<f64> {
        self.shared.recorder.borrow_mut().due.take()
    }

    /// Seconds until a picture the finished frame was refused can be made
    /// (`draw:field` or `canvas:set` past its allowance): the layer is to
    /// be drawn once more then, whether or not anything may move. `None`:
    /// nothing was refused.
    pub fn take_retry(&self) -> Option<f64> {
        self.shared.recorder.borrow_mut().retry.take()
    }

    /// Abandons a frame (the hook failed): nothing is kept.
    pub fn abandon(&self) {
        let mut rec = self.shared.recorder.borrow_mut();
        rec.active = false;
        rec.due = None;
        rec.retry = None;
        rec.over = None;
        rec.ops.clear();
        rec.open.clear();
    }

    /// Whether the last finished frame had pushes or saves left open.
    pub fn was_unbalanced(&self) -> bool {
        self.shared.unbalanced.get()
    }

    /// What the theme's author should know of the calls since
    /// [`Api::begin`] (and of pictures that didn't load): each message
    /// once in the theme's life.
    pub fn take_notes(&self) -> Vec<String> {
        let mut notes = std::mem::take(&mut self.shared.recorder.borrow_mut().notes);
        notes.extend(self.shared.pictures.borrow_mut().take_notes());
        let mut told = self.shared.told.borrow_mut();
        notes.retain(|n| told.len() < 256 && told.insert(n.clone()));
        notes
    }

    /// The theme's pictures, for the app to make textures of.
    pub fn pictures(&self) -> std::cell::Ref<'_, Pictures> {
        self.shared.pictures.borrow()
    }

    /// What kind of picture `id` is.
    pub fn kind(&self, id: ImageId) -> Option<PictureKind> {
        self.shared
            .pictures
            .borrow()
            .get(id)
            .map(|p| p.kind.clone())
    }

    /// A still's pixels, decoded now ([`Pictures::still`]): call it
    /// outside the theme's hooks.
    pub fn still(&self, id: ImageId) -> Option<gmnb_media::Rgba> {
        self.shared.pictures.borrow_mut().still(id)
    }

    /// An animation's file and timeline ([`Pictures::animation`]).
    pub fn animation(
        &self,
        id: ImageId,
    ) -> Option<(gmnb_media::Bytes, std::sync::Arc<gmnb_media::Timeline>)> {
        self.shared.pictures.borrow_mut().animation(id)
    }

    /// A picture the theme fills, as it stands.
    pub fn canvas(&self, id: ImageId) -> Option<Canvas> {
        self.shared.pictures.borrow().canvas(id).cloned()
    }

    /// The app found a picture's pixels can't be had ([`Pictures::spoil`]).
    pub fn spoil(&self, id: ImageId, note: String) {
        self.shared.pictures.borrow_mut().spoil(id, note);
    }

    /// A note from the app about the theme's pictures.
    pub fn note(&self, note: String) {
        self.shared.pictures.borrow_mut().note(note);
    }
}

fn draw_table(lua: &Lua, shared: &Rc<Shared>) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let keys = Rc::new(Keys::new(lua)?);
    let set = |name: &'static str, f: Function| t.raw_set(name, f);

    // ---- Shapes ----------------------------------------------------------
    let (s, k) = (shared.clone(), keys.clone());
    set(
        "rect",
        lua.create_function(
            move |_, (_, x, y, w, h, p): (Value, f64, f64, f64, f64, Value)| {
                let op = Op::Rect {
                    rect: rect("rect", x, y, w, h)?,
                    paint: paint(&k, "rect", &p)?,
                };
                s.recorder.borrow_mut().push("rect", op)
            },
        )?,
    )?;
    let (s, k) = (shared.clone(), keys.clone());
    set(
        "rounded_rect",
        lua.create_function(
            move |_, (_, x, y, w, h, r, p): (Value, f64, f64, f64, f64, Value, Value)| {
                let what = "rounded_rect";
                let op = Op::RoundedRect {
                    rect: rect(what, x, y, w, h)?,
                    radius: radius(what, &r)?,
                    paint: paint(&k, what, &p)?,
                };
                s.recorder.borrow_mut().push(what, op)
            },
        )?,
    )?;
    let (s, k) = (shared.clone(), keys.clone());
    set(
        "circle",
        lua.create_function(move |_, (_, x, y, r, p): (Value, f64, f64, f64, Value)| {
            let what = "circle";
            let op = Op::Circle {
                center: [coord(what, x)?, coord(what, y)?],
                radius: coord(what, r)?.max(0.0),
                paint: paint(&k, what, &p)?,
            };
            s.recorder.borrow_mut().push(what, op)
        })?,
    )?;
    let (s, k) = (shared.clone(), keys.clone());
    set(
        "fill",
        lua.create_function(move |_, (_, path, p, rule): (Value, Value, Value, Value)| {
            let what = "fill";
            s.recorder.borrow().ready(what)?;
            let rule = match &rule {
                Value::Nil => FillRule::Winding,
                v => word(
                    what,
                    v,
                    &[
                        ("winding", FillRule::Winding),
                        ("even-odd", FillRule::EvenOdd),
                    ],
                )?,
            };
            let paint = paint(&k, what, &p)?;
            let path = take_path(&s, what, &path)?;
            s.recorder
                .borrow_mut()
                .push(what, Op::Fill { path, paint, rule })
        })?,
    )?;
    let (s, k) = (shared.clone(), keys.clone());
    set(
        "stroke",
        lua.create_function(move |_, (_, path, line, p): (Value, Value, Value, Value)| {
            let what = "stroke";
            s.recorder.borrow().ready(what)?;
            let stroke = stroke(what, &line)?;
            let paint = paint(&k, what, &p)?;
            let path = take_path(&s, what, &path)?;
            s.recorder.borrow_mut().push(
                what,
                Op::Stroke {
                    path,
                    stroke,
                    paint,
                },
            )
        })?,
    )?;
    let (s, k) = (shared.clone(), keys.clone());
    set(
        "border",
        lua.create_function(
            move |_, (_, x, y, w, h, r, width, c): (Value, f64, f64, f64, f64, Value, f64, Value)| {
                let what = "border";
                let op = Op::Border {
                    rect: rect(what, x, y, w, h)?,
                    radius: radius(what, &r)?,
                    width: coord(what, width)?.clamp(0.0, 1000.0),
                    color: color(&k, what, &c)?,
                };
                s.recorder.borrow_mut().push(what, op)
            },
        )?,
    )?;
    let s = shared.clone();
    set(
        "shadow",
        lua.create_function(
            move |_, (_, x, y, w, h, r, sh): (Value, f64, f64, f64, f64, Value, Value)| {
                let what = "shadow";
                let mut rec = s.recorder.borrow_mut();
                rec.ready(what)?;
                let op = Op::Shadow {
                    rect: rect(what, x, y, w, h)?,
                    radius: radius(what, &r)?,
                    shadow: shadow(&mut rec, what, &sh)?,
                };
                rec.push(what, op)
            },
        )?,
    )?;

    // ---- Text and pictures -------------------------------------------------
    let (s, k) = (shared.clone(), keys.clone());
    set(
        "text",
        lua.create_function(
            move |_, (_, text, x, y, font, p): (Value, Value, f64, f64, Value, Value)| {
                let what = "text";
                let mut rec = s.recorder.borrow_mut();
                rec.ready(what)?;
                let (text, font) = text_and_font(&mut rec, what, &text, &font)?;
                let op = Op::Text {
                    text,
                    at: [coord(what, x)?, coord(what, y)?],
                    font,
                    paint: paint(&k, what, &p)?,
                };
                rec.push(what, op)
            },
        )?,
    )?;
    let s = shared.clone();
    set(
        "text_size",
        lua.create_function(move |_, (_, text, font): (Value, Value, Value)| {
            let what = "text_size";
            let (text, font) = {
                let mut rec = s.recorder.borrow_mut();
                rec.ready(what)?;
                text_and_font(&mut rec, what, &text, &font)?
            };
            let (w, h) = match s.measure.borrow().as_ref() {
                Some(m) => m.measure(&text, &font),
                None => estimate(&text, &font),
            };
            let fin = |n: f32| {
                if n.is_finite() {
                    f64::from(n.max(0.0))
                } else {
                    0.0
                }
            };
            Ok((fin(w), fin(h)))
        })?,
    )?;
    let (s, k) = (shared.clone(), keys.clone());
    set(
        "glyph",
        lua.create_function(move |_, (_, glyph, p): (Value, Value, Value)| {
            let what = "glyph";
            let Value::Table(glyph) = glyph else {
                return Err(bad(what, "expected a glyph of frame.value"));
            };
            let previous = matches!(glyph.raw_get::<Value>(&k.previous)?, Value::Boolean(true));
            let index = number(what, &glyph.raw_get::<Value>(&k.index)?)?;
            let mut rec = s.recorder.borrow_mut();
            rec.ready(what)?;
            let count = rec.glyphs[usize::from(previous)];
            if index.fract() != 0.0 || index < 1.0 || index > f64::from(count) {
                return Err(bad(
                    what,
                    format!("glyph {index} isn't one of the value's {count}"),
                ));
            }
            let op = Op::Glyph {
                previous,
                index: index as u32 - 1,
                paint: paint(&k, what, &p)?,
            };
            rec.push(what, op)
        })?,
    )?;
    for (name, tile) in [("image", false), ("tile", true)] {
        let (s, k) = (shared.clone(), keys.clone());
        set(
            name,
            lua.create_function(
                move |_,
                      (_, image, x, y, w, h, options): (
                    Value,
                    Value,
                    f64,
                    f64,
                    f64,
                    f64,
                    Value,
                )| {
                    let Value::Table(image) = image else {
                        return Err(bad(
                            name,
                            "expected a picture (gmnb.image, gmnb.noise, gmnb.animation, gmnb.pixels)",
                        ));
                    };
                    let rect = rect(name, x, y, w, h)?;
                    // `options`: how it is sampled, and for an animation
                    // where in it and whether it wraps. (`draw:tile`
                    // takes none.)
                    let options = match &options {
                        Value::Table(o) if !tile => Some(o),
                        Value::Nil | Value::Table(_) => None,
                        _ => {
                            return Err(bad(
                                name,
                                "options are a table { filter = …, time = …, loop = … }",
                            ));
                        }
                    };
                    let option = |key: &str| match options {
                        Some(o) => o.raw_get::<Value>(key),
                        None => Ok(Value::Nil),
                    };
                    let filter = match option("filter")? {
                        Value::Nil => None,
                        v => Some(word(
                            name,
                            &v,
                            &[
                                ("trilinear", Filter::Trilinear),
                                ("linear", Filter::Linear),
                                ("nearest", Filter::Nearest),
                            ],
                        )?),
                    };
                    let time = match option("time")? {
                        Value::Nil => None,
                        v => Some(number(name, &v)?),
                    };
                    let looped = match option("loop")? {
                        Value::Nil => true,
                        Value::Boolean(b) => b,
                        _ => return Err(bad(name, "`loop` is true or false")),
                    };
                    let mut rec = s.recorder.borrow_mut();
                    rec.ready(name)?;
                    // A picture that didn't load has no id: it draws nothing.
                    let id = match image.raw_get::<Value>(&k.id)? {
                        Value::Nil => return Ok(()),
                        v => number(name, &v)?,
                    };
                    let mut pictures = s.pictures.borrow_mut();
                    if id.fract() != 0.0 || id < 1.0 || id > pictures.len() as f64 {
                        return Err(bad(name, "not a picture of this theme"));
                    }
                    let image = ImageId(id as u32 - 1);
                    let kind = pictures.get(image).map(|p| p.kind.clone());
                    let op = match kind {
                        // Of an animation a tile is its first frame.
                        _ if tile => Op::Tile { image, rect },
                        Some(PictureKind::Animation(_, timeline)) => {
                            // Nothing moves under reduced motion or in a
                            // preview, whatever `time` says.
                            let still = !rec.motion || rec.still;
                            let frame = match time {
                                _ if still => 0,
                                // The theme's own clock: it asks for
                                // frames itself.
                                Some(time) => timeline.frame_at(time, looped),
                                // The layer's clock: the app sees to the
                                // next frame.
                                None => {
                                    let slow = s.slow.get();
                                    let position = pictures.position(image, rec.time, slow);
                                    if let Some(due) = timeline.next_due(position) {
                                        let due = due * slow;
                                        rec.due = Some(rec.due.map_or(due, |d| d.min(due)));
                                    }
                                    timeline.frame_at(position, true)
                                }
                            };
                            Op::Animation {
                                image,
                                frame,
                                still,
                                rect,
                                filter: filter.unwrap_or(Filter::Linear),
                            }
                        }
                        Some(PictureKind::Canvas) => Op::Image {
                            image,
                            rect,
                            filter: filter.unwrap_or(Filter::Linear),
                        },
                        _ => Op::Image {
                            image,
                            rect,
                            filter: filter.unwrap_or(Filter::Trilinear),
                        },
                    };
                    drop(pictures);
                    rec.push(name, op)
                },
            )?,
        )?;
    }
    let s = shared.clone();
    set(
        "field",
        lua.create_function(
            move |_, (_, x, y, w, h, spec): (Value, f64, f64, f64, f64, Value)| {
                crate::field::draw_field(&s, (x, y, w, h), spec)
            },
        )?,
    )?;

    // ---- Groups ----------------------------------------------------------------
    let s = shared.clone();
    set(
        "push_clip",
        lua.create_function(
            move |_, (_, x, y, w, h, r): (Value, f64, f64, f64, f64, Value)| {
                let what = "push_clip";
                let op = Op::PushClip {
                    rect: rect(what, x, y, w, h)?,
                    radius: match &r {
                        Value::Nil => None,
                        r => Some(radius(what, r)?),
                    },
                };
                s.recorder.borrow_mut().open_push(what, op, 1)
            },
        )?,
    )?;
    let s = shared.clone();
    set(
        "push_opacity",
        lua.create_function(move |_, (_, opacity): (Value, f64)| {
            let what = "push_opacity";
            let op = Op::PushOpacity(coord(what, opacity)?.clamp(0.0, 1.0));
            s.recorder.borrow_mut().open_push(what, op, 1)
        })?,
    )?;
    let s = shared.clone();
    set(
        "push_blur",
        lua.create_function(move |_, (_, r): (Value, f64)| {
            let what = "push_blur";
            let r = coord(what, r)?.max(0.0);
            let mut rec = s.recorder.borrow_mut();
            rec.ready(what)?;
            if r > rec.limits.blur as f32 {
                let cap = rec.limits.blur;
                return Err(rec.over(what, format!("a blur of more than {cap} px")));
            }
            rec.open_push(what, Op::PushBlur(r), 1)
        })?,
    )?;
    let s = shared.clone();
    set(
        "push_shadow",
        lua.create_function(move |_, (_, list): (Value, Value)| {
            let what = "push_shadow";
            let Value::Table(list) = list else {
                return Err(bad(what, "expected a list of shadows"));
            };
            let n = list.raw_len();
            if n == 0 || n > check::MAX_SHADOWS {
                return Err(bad(
                    what,
                    format!("{n} shadows; a list takes 1 to {}", check::MAX_SHADOWS),
                ));
            }
            let mut rec = s.recorder.borrow_mut();
            rec.ready(what)?;
            let mut shadows = Vec::with_capacity(n);
            for i in 1..=n {
                shadows.push(shadow(&mut rec, what, &list.raw_get::<Value>(i)?)?);
            }
            rec.open_push(what, Op::PushShadow(shadows), 1)
        })?,
    )?;
    let s = shared.clone();
    set(
        "push_mask",
        lua.create_function(move |_, (_, mode): (Value, Value)| {
            let what = "push_mask";
            let mode = match &mode {
                Value::Nil => MaskMode::Alpha,
                v => word(what, v, &MaskMode::ALL)?,
            };
            s.recorder
                .borrow_mut()
                .open_push(what, Op::PushMask(mode), 2)
        })?,
    )?;
    let s = shared.clone();
    set(
        "push_blend",
        lua.create_function(move |_, (_, mode): (Value, Value)| {
            let what = "push_blend";
            let mode = word(what, &mode, &BlendMode::ALL)?;
            s.recorder
                .borrow_mut()
                .open_push(what, Op::PushBlend(mode), 2)
        })?,
    )?;
    let s = shared.clone();
    set(
        "pop",
        lua.create_function(move |_, _: Value| {
            let what = "pop";
            let mut rec = s.recorder.borrow_mut();
            rec.ready(what)?;
            match rec.open.last() {
                None => return Err(bad(what, "nothing is pushed")),
                Some(Open::Save) => {
                    return Err(bad(
                        what,
                        "a save is open inside this push; restore it first",
                    ));
                }
                Some(Open::Push) => {}
            }
            rec.push(what, Op::Pop)?;
            rec.open.pop();
            rec.pushes -= 1;
            Ok(())
        })?,
    )?;

    // ---- Transforms ----------------------------------------------------------
    let s = shared.clone();
    set(
        "save",
        lua.create_function(move |_, _: Value| {
            let what = "save";
            let mut rec = s.recorder.borrow_mut();
            rec.ready(what)?;
            if rec.saves >= rec.limits.saves {
                let cap = rec.limits.saves;
                return Err(rec.over(what, format!("more than {cap} saves open at once")));
            }
            rec.push(what, Op::Save)?;
            rec.saves += 1;
            rec.open.push(Open::Save);
            Ok(())
        })?,
    )?;
    let s = shared.clone();
    set(
        "restore",
        lua.create_function(move |_, _: Value| {
            let what = "restore";
            let mut rec = s.recorder.borrow_mut();
            rec.ready(what)?;
            match rec.open.last() {
                None => return Err(bad(what, "nothing is saved")),
                Some(Open::Push) => {
                    return Err(bad(what, "a push is open inside this save; pop it first"));
                }
                Some(Open::Save) => {}
            }
            rec.push(what, Op::Restore)?;
            rec.open.pop();
            rec.saves -= 1;
            Ok(())
        })?,
    )?;
    let s = shared.clone();
    set(
        "translate",
        lua.create_function(move |_, (_, dx, dy): (Value, f64, f64)| {
            let what = "translate";
            let op = Op::Translate(coord(what, dx)?, coord(what, dy)?);
            s.recorder.borrow_mut().push(what, op)
        })?,
    )?;
    let s = shared.clone();
    set(
        "scale",
        lua.create_function(move |_, (_, sx, sy): (Value, f64, Option<f64>)| {
            let what = "scale";
            let op = Op::Scale(coord(what, sx)?, coord(what, sy.unwrap_or(sx))?);
            s.recorder.borrow_mut().push(what, op)
        })?,
    )?;
    let s = shared.clone();
    set(
        "rotate",
        lua.create_function(move |_, (_, degrees): (Value, f64)| {
            let what = "rotate";
            let op = Op::Rotate(coord(what, degrees)?);
            s.recorder.borrow_mut().push(what, op)
        })?,
    )?;

    t.set_readonly(true);
    Ok(t)
}

/// The text and font of a text call, within the text caps. The font
/// merges onto the style's body text; what is wrong in it is noted.
fn text_and_font(
    rec: &mut Recorder,
    what: &str,
    text: &Value,
    font: &Value,
) -> mlua::Result<(String, Font)> {
    let Value::String(text) = text else {
        return Err(bad(what, "expected text"));
    };
    let bytes = text.as_bytes();
    if bytes.len() > rec.limits.text_bytes {
        let cap = rec.limits.text_bytes;
        return Err(rec.over(what, format!("text of more than {cap} bytes")));
    }
    if rec.text_calls >= rec.limits.text_calls {
        let cap = rec.limits.text_calls;
        return Err(rec.over(what, format!("more than {cap} text calls in one frame")));
    }
    rec.text_calls += 1;
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Err(bad(what, "text that isn't UTF-8"));
    };
    if text.chars().any(|c| c == '\0') {
        return Err(bad(what, "text with a NUL in it"));
    }
    let font = match font {
        Value::Nil => rec.base_font.clone(),
        v => {
            let tree = plain(what, v, 0, &mut 128)?;
            let mut notes = Notes::default();
            let font = check::font(&tree, &rec.base_font, "font", &mut notes);
            for note in notes.list {
                rec.note(format!("draw:{what}: {note}"));
            }
            font
        }
    };
    Ok((text.to_string(), font))
}

/// The path of a fill or a stroke, its segments counted against the cap.
fn take_path(shared: &Shared, what: &str, v: &Value) -> mlua::Result<Rc<Path>> {
    let room = {
        let rec = shared.recorder.borrow();
        rec.limits.path_segments.saturating_sub(rec.segments)
    };
    match path::read(v, room) {
        Ok((path, segments)) => {
            shared.recorder.borrow_mut().segments += segments;
            Ok(Rc::new(path))
        }
        Err(path::Problem::TooLong) => {
            let mut rec = shared.recorder.borrow_mut();
            let cap = rec.limits.path_segments;
            Err(rec.over(what, format!("more than {cap} path segments in one frame")))
        }
        Err(path::Problem::Bad(message)) => Err(bad(what, message)),
    }
}
