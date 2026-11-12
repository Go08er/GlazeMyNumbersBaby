//! Paths (`gmnb.path()`, `gmnb.svg_path()`): what `draw:fill` and
//! `draw:stroke` take.
//!
//! To a script a path is a table with builder methods. The table holds
//! the path as plain numbers, written by the methods in Luau itself, so
//! building one costs no trips to the app and its memory is the theme's
//! own. The app reads the numbers when the path is drawn ([`read`]), and
//! checks them then.

use std::rc::Rc;

use mlua::{Lua, Table, Value};

use crate::types::Radius;

/// One builder call.
#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    /// The control point, then the end.
    QuadTo(f32, f32, f32, f32),
    /// Two control points, then the end.
    CurveTo(f32, f32, f32, f32, f32, f32),
    Close,
    Rect(f32, f32, f32, f32),
    RoundedRect(f32, f32, f32, f32, Radius),
    /// Centre and radius.
    Circle(f32, f32, f32),
    /// SVG path data (`gsk_path_parse`), checked by [`svg_segments`].
    Svg(Rc<str>),
}

/// A path as it was when it was drawn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub segments: Vec<Segment>,
}

// The numbers a path table holds: an operation's code, then its
// arguments.
const MOVE: i64 = 1;
const LINE: i64 = 2;
const QUAD: i64 = 3;
const CURVE: i64 = 4;
const CLOSE: i64 = 5;
const RECT: i64 = 6;
const ROUNDED: i64 = 7;
const CIRCLE: i64 = 8;
const SVG: i64 = 9;

/// The builder, in Luau: each method appends its code and arguments and
/// returns the path. Kept in step with the codes above.
const BUILDER: &str = r#"
local P = {}
P.__index = P
local function bad(name)
	error("path:" .. name .. ": expected numbers", 3)
end
function P.move_to(self, x, y)
	if type(x) ~= "number" or type(y) ~= "number" then bad("move_to") end
	local n = #self
	self[n + 1], self[n + 2], self[n + 3] = 1, x, y
	return self
end
function P.line_to(self, x, y)
	if type(x) ~= "number" or type(y) ~= "number" then bad("line_to") end
	local n = #self
	self[n + 1], self[n + 2], self[n + 3] = 2, x, y
	return self
end
function P.quad_to(self, cx, cy, x, y)
	if type(cx) ~= "number" or type(cy) ~= "number" or type(x) ~= "number" or type(y) ~= "number" then
		bad("quad_to")
	end
	local n = #self
	self[n + 1], self[n + 2], self[n + 3], self[n + 4], self[n + 5] = 3, cx, cy, x, y
	return self
end
function P.curve_to(self, ax, ay, bx, by, x, y)
	if
		type(ax) ~= "number" or type(ay) ~= "number" or type(bx) ~= "number"
		or type(by) ~= "number" or type(x) ~= "number" or type(y) ~= "number"
	then
		bad("curve_to")
	end
	local n = #self
	self[n + 1], self[n + 2], self[n + 3], self[n + 4], self[n + 5], self[n + 6], self[n + 7] = 4, ax, ay, bx, by, x, y
	return self
end
function P.close(self)
	self[#self + 1] = 5
	return self
end
function P.rect(self, x, y, w, h)
	if type(x) ~= "number" or type(y) ~= "number" or type(w) ~= "number" or type(h) ~= "number" then
		bad("rect")
	end
	local n = #self
	self[n + 1], self[n + 2], self[n + 3], self[n + 4], self[n + 5] = 6, x, y, w, h
	return self
end
function P.rounded_rect(self, x, y, w, h, r)
	local a, b, c, d
	if type(r) == "table" then
		a, b, c, d = r[1], r[2], r[3], r[4]
	else
		a, b, c, d = r, r, r, r
	end
	if
		type(x) ~= "number" or type(y) ~= "number" or type(w) ~= "number" or type(h) ~= "number"
		or type(a) ~= "number" or type(b) ~= "number" or type(c) ~= "number" or type(d) ~= "number"
	then
		bad("rounded_rect")
	end
	local n = #self
	self[n + 1], self[n + 2], self[n + 3], self[n + 4], self[n + 5] = 7, x, y, w, h
	self[n + 6], self[n + 7], self[n + 8], self[n + 9] = a, b, c, d
	return self
end
function P.circle(self, x, y, r)
	if type(x) ~= "number" or type(y) ~= "number" or type(r) ~= "number" then bad("circle") end
	local n = #self
	self[n + 1], self[n + 2], self[n + 3], self[n + 4] = 8, x, y, r
	return self
end
return P, function()
	return setmetatable({}, P)
end
"#;

/// Adds `gmnb.path` and `gmnb.svg_path`.
pub(crate) fn register(lua: &Lua, gmnb: &Table) -> mlua::Result<()> {
    let (methods, new): (Table, mlua::Function) = lua
        .load(BUILDER)
        .set_name("=gmnb.path")
        .set_mode(mlua::chunk::ChunkMode::Text)
        .eval()?;
    // The methods are shared by every path: scripts can't change them.
    methods.set_readonly(true);
    gmnb.raw_set("path", new)?;
    gmnb.raw_set(
        "svg_path",
        lua.create_function(move |lua, d: Value| {
            let Value::String(d) = d else {
                return Err(mlua::Error::runtime(
                    "gmnb.svg_path: expected SVG path data",
                ));
            };
            let text = d.to_str()?;
            svg_segments(&text).map_err(|e| mlua::Error::runtime(format!("gmnb.svg_path: {e}")))?;
            let path = lua.create_table_with_capacity(2, 0)?;
            path.raw_seti(1, SVG)?;
            path.raw_seti(2, d.clone())?;
            path.set_metatable(Some(methods.clone()))?;
            Ok(path)
        })?,
    )?;
    Ok(())
}

/// Why a path can't be drawn.
pub(crate) enum Problem {
    /// More segments than the frame has room for.
    TooLong,
    Bad(String),
}

/// Reads a path table of at most `room` segments: the path, and how many
/// segments it counts for (SVG data counts for each of its commands).
pub(crate) fn read(v: &Value, room: usize) -> Result<(Path, usize), Problem> {
    const NOT_A_PATH: &str = "not a path: it holds something a builder didn't put there";
    let bad = |m: &str| Problem::Bad(m.to_string());
    let Value::Table(t) = v else {
        return Err(bad("expected a path (gmnb.path(), gmnb.svg_path())"));
    };
    // A segment is at most nine numbers: a longer table can't fit.
    if t.raw_len() / 9 > room {
        return Err(Problem::TooLong);
    }
    let values: Vec<Value> = t
        .sequence_values::<Value>()
        .collect::<mlua::Result<_>>()
        .map_err(|e| Problem::Bad(e.to_string()))?;
    let mut at = 0;
    let mut segments = Vec::new();
    let mut used = 0usize;
    while at < values.len() {
        let code = match &values[at] {
            Value::Integer(i) => *i,
            Value::Number(n) if n.fract() == 0.0 => *n as i64,
            _ => return Err(bad(NOT_A_PATH)),
        };
        at += 1;
        let mut n = |what: &str| -> Result<f32, Problem> {
            let x = match values.get(at) {
                Some(Value::Integer(i)) => *i as f64,
                Some(Value::Number(n)) => *n,
                Some(_) => return Err(Problem::Bad(format!("a path's {what} needs numbers"))),
                None => return Err(Problem::Bad(format!("a path cut short in its {what}"))),
            };
            at += 1;
            if !x.is_finite() {
                return Err(Problem::Bad(format!(
                    "a path's {what} has a number that isn't finite"
                )));
            }
            Ok(x.clamp(-1.0e7, 1.0e7) as f32)
        };
        let (segment, cost) = match code {
            MOVE => (Segment::MoveTo(n("move_to")?, n("move_to")?), 1),
            LINE => (Segment::LineTo(n("line_to")?, n("line_to")?), 1),
            QUAD => {
                let w = "quad_to";
                (Segment::QuadTo(n(w)?, n(w)?, n(w)?, n(w)?), 1)
            }
            CURVE => {
                let w = "curve_to";
                (
                    Segment::CurveTo(n(w)?, n(w)?, n(w)?, n(w)?, n(w)?, n(w)?),
                    1,
                )
            }
            CLOSE => (Segment::Close, 1),
            RECT => {
                let w = "rect";
                (
                    Segment::Rect(n(w)?, n(w)?, n(w)?.max(0.0), n(w)?.max(0.0)),
                    1,
                )
            }
            ROUNDED => {
                let w = "rounded_rect";
                let (x, y, wd, h) = (n(w)?, n(w)?, n(w)?.max(0.0), n(w)?.max(0.0));
                let r = [
                    n(w)?.clamp(0.0, MAX_RADIUS),
                    n(w)?.clamp(0.0, MAX_RADIUS),
                    n(w)?.clamp(0.0, MAX_RADIUS),
                    n(w)?.clamp(0.0, MAX_RADIUS),
                ];
                (Segment::RoundedRect(x, y, wd, h, Radius(r)), 1)
            }
            CIRCLE => {
                let w = "circle";
                (
                    Segment::Circle(n(w)?, n(w)?, n(w)?.clamp(0.0, MAX_RADIUS)),
                    1,
                )
            }
            SVG => {
                let Some(Value::String(d)) = values.get(at) else {
                    return Err(bad("not a path: its SVG data is missing"));
                };
                at += 1;
                let Ok(text) = d.to_str() else {
                    return Err(bad("SVG path data that isn't UTF-8"));
                };
                let cost = svg_segments(&text).map_err(Problem::Bad)?;
                (Segment::Svg(Rc::from(&*text)), cost)
            }
            _ => return Err(bad(NOT_A_PATH)),
        };
        used += cost;
        if used > room {
            return Err(Problem::TooLong);
        }
        // A circle or a rounded rectangle reaching far from the origin is
        // given as the curves it is made of.
        match segment {
            Segment::Circle(x, y, r) if !near(&[x - r, x + r, y - r, y + r]) => {
                segments.extend(circle_curves(x, y, r));
            }
            Segment::RoundedRect(x, y, w, h, r) if !near(&[x, x + w, y, y + h]) => {
                segments.extend(rounded_curves(x, y, w, h, &r));
            }
            segment => segments.push(segment),
        }
    }
    Ok((Path { segments }, used))
}

/// How far from the origin, in a path's own units, the points of a circle
/// or a rounded rectangle may be for the toolkit's own shape to be drawn.
/// Its circles and rounded corners are worked out in single precision to
/// half a unit, and cost it more the further out they are: twenty circles
/// of radius 10^6 about (10^6, 10^6) took a second (each 55 ms on the
/// software renderer, 117 ms on GL), where about the window they took
/// nothing. Further out a shape is given as cubic curves instead, which
/// cost no more there than anywhere.
pub const CONTOUR_REACH: f32 = 1.0e5;

/// How far along its tangent a cubic curve's control point is put for a
/// quarter circle of radius 1.
const KAPPA: f32 = 0.552_284_8;

fn near(coords: &[f32]) -> bool {
    coords.iter().all(|c| c.abs() <= CONTOUR_REACH)
}

/// A circle as four cubic curves, clockwise from its right, as the
/// toolkit's own goes.
pub fn circle_curves(cx: f32, cy: f32, r: f32) -> Vec<Segment> {
    let k = r * KAPPA;
    vec![
        Segment::MoveTo(cx + r, cy),
        Segment::CurveTo(cx + r, cy + k, cx + k, cy + r, cx, cy + r),
        Segment::CurveTo(cx - k, cy + r, cx - r, cy + k, cx - r, cy),
        Segment::CurveTo(cx - r, cy - k, cx - k, cy - r, cx, cy - r),
        Segment::CurveTo(cx + k, cy - r, cx + r, cy - k, cx + r, cy),
        Segment::Close,
    ]
}

/// A rounded rectangle as lines and cubic curves, clockwise from its top
/// left, its corners (top left, top right, bottom right, bottom left)
/// scaled down together when they are too large for it, as the toolkit
/// does.
pub fn rounded_curves(x: f32, y: f32, w: f32, h: f32, radius: &Radius) -> Vec<Segment> {
    let [tl, tr, br, bl] = radius.0;
    let fit = |len: f32, a: f32, b: f32| if a + b > len { len / (a + b) } else { 1.0 };
    let f = fit(w, tl, tr)
        .min(fit(w, bl, br))
        .min(fit(h, tl, bl))
        .min(fit(h, tr, br));
    let [tl, tr, br, bl] = [tl * f, tr * f, br * f, bl * f];
    let (r, b) = (x + w, y + h);
    vec![
        Segment::MoveTo(x + tl, y),
        Segment::LineTo(r - tr, y),
        Segment::CurveTo(r - tr + tr * KAPPA, y, r, y + tr - tr * KAPPA, r, y + tr),
        Segment::LineTo(r, b - br),
        Segment::CurveTo(r, b - br + br * KAPPA, r - br + br * KAPPA, b, r - br, b),
        Segment::LineTo(x + bl, b),
        Segment::CurveTo(x + bl - bl * KAPPA, b, x, b - bl + bl * KAPPA, x, b - bl),
        Segment::LineTo(x, y + tl),
        Segment::CurveTo(x, y + tl - tl * KAPPA, x + tl - tl * KAPPA, y, x + tl, y),
        Segment::Close,
    ]
}

/// How far from the origin SVG path data may reach, and how large an
/// arc's radius may be: far beyond any window.
pub const SVG_REACH: f64 = 1.0e6;

/// The largest radius of a circle or of a rounded rectangle's corner in a
/// path (and of `draw:circle`): the toolkit's circles and corners cost it
/// more the larger they are, in the path's own units (a tenth of a second
/// each at 10^7, measured, where 10^6 cost nothing more than 10^3).
pub const MAX_RADIUS: f32 = 1.0e6;

/// Checks SVG path data and counts its segments: the commands of the SVG
/// path grammar (`MmLlHhVvCcSsQqTtAaZz`), each with as many numbers as it
/// takes, repeated or not, and no point further than [`SVG_REACH`] from
/// the origin (counting every relative step) nor an arc's radius larger.
/// What passes here is what the toolkit's parser is given; anything else
/// is refused before it gets there.
pub fn svg_segments(d: &str) -> Result<usize, String> {
    svg_shape(d).map(|shape| shape.0)
}

/// [`svg_segments`], and how far from the origin a point of the path can
/// be, at most.
pub fn svg_reach(d: &str) -> Result<(usize, f64), String> {
    svg_shape(d).map(|(count, reach, _)| (count, reach))
}

/// Where SVG path data's points go, followed command by command: the
/// current point, where the subpath started, the control point a smooth
/// curve reflects, and the box around every point and control point (a
/// curve lies within its control points; an arc within twice its radius
/// of its ends).
#[derive(Default)]
struct Pen {
    at: [f64; 2],
    start: [f64; 2],
    /// The last cubic's second control point, and the last quadratic's.
    cubic: Option<[f64; 2]>,
    quad: Option<[f64; 2]>,
    bounds: Option<[f64; 4]>,
    /// The length of the path's points and control points joined in
    /// order, plus an arc's whole ellipse: at least how long the path is.
    length: f64,
    /// The lines those are joined by (an arc: the box around where it
    /// can go), if they are wanted ([`svg_edges`]).
    edges: Option<Vec<[f64; 4]>>,
}

impl Pen {
    /// The path goes on from the current point along `via` to `to`.
    fn run(&mut self, via: &[[f64; 2]], to: [f64; 2]) {
        let mut from = self.at;
        for p in via.iter().chain([&to]) {
            self.length += (p[0] - from[0]).hypot(p[1] - from[1]);
            if let Some(edges) = self.edges.as_mut().filter(|e| e.len() < MAX_EDGES) {
                edges.push([from[0], from[1], p[0], p[1]]);
            }
            from = *p;
        }
    }

    fn mark(&mut self, [x, y]: [f64; 2], grow: f64) {
        let b = self.bounds.get_or_insert([x, y, x, y]);
        *b = [
            b[0].min(x - grow),
            b[1].min(y - grow),
            b[2].max(x + grow),
            b[3].max(y + grow),
        ];
    }

    /// One set of numbers of `command` (its arguments, flags as 0 or 1).
    ///
    /// `Err`: an arc whose radii, grown to reach from one end to the other
    /// as the SVG grammar has it, are past [`SVG_REACH`]. Else, for an arc,
    /// the largest radius it is drawn with.
    fn go(&mut self, command: u8, v: &[f64; 7], first_of_move: bool) -> Result<f64, String> {
        let rel = command.is_ascii_lowercase();
        let [x0, y0] = self.at;
        let pt = |x: f64, y: f64| if rel { [x0 + x, y0 + y] } else { [x, y] };
        let (mut cubic, mut quad) = (None, None);
        let mut grown = 0.0;
        let end = match command.to_ascii_lowercase() {
            b'm' => {
                let p = pt(v[0], v[1]);
                if first_of_move {
                    self.start = p;
                }
                p
            }
            b'l' => {
                let end = pt(v[0], v[1]);
                self.run(&[], end);
                end
            }
            b'h' => {
                let end = [if rel { x0 + v[0] } else { v[0] }, y0];
                self.run(&[], end);
                end
            }
            b'v' => {
                let end = [x0, if rel { y0 + v[0] } else { v[0] }];
                self.run(&[], end);
                end
            }
            b'c' => {
                let (c1, c2) = (pt(v[0], v[1]), pt(v[2], v[3]));
                self.mark(c1, 0.0);
                self.mark(c2, 0.0);
                cubic = Some(c2);
                let end = pt(v[4], v[5]);
                self.run(&[c1, c2], end);
                end
            }
            b's' => {
                let c1 = self
                    .cubic
                    .map_or(self.at, |c| [2.0 * x0 - c[0], 2.0 * y0 - c[1]]);
                let c2 = pt(v[0], v[1]);
                self.mark(c1, 0.0);
                self.mark(c2, 0.0);
                cubic = Some(c2);
                let end = pt(v[2], v[3]);
                self.run(&[c1, c2], end);
                end
            }
            b'q' => {
                let c = pt(v[0], v[1]);
                self.mark(c, 0.0);
                quad = Some(c);
                let end = pt(v[2], v[3]);
                self.run(&[c], end);
                end
            }
            b't' => {
                let c = self
                    .quad
                    .map_or(self.at, |c| [2.0 * x0 - c[0], 2.0 * y0 - c[1]]);
                self.mark(c, 0.0);
                quad = Some(c);
                let end = pt(v[0], v[1]);
                self.run(&[c], end);
                end
            }
            _ => {
                // An arc: its radii, grown to reach from one end to the
                // other if they are too small (F.6.6: both by the root of
                // how far the ends are out of the ellipse, so a long thin
                // one is grown to a very long one).
                let end = pt(v[5], v[6]);
                let chord = ((end[0] - x0).powi(2) + (end[1] - y0).powi(2)).sqrt();
                let (mut rx, mut ry) = (v[0].abs(), v[1].abs());
                if rx > 0.0 && ry > 0.0 {
                    let (sin, cos) = v[2].to_radians().sin_cos();
                    let (dx, dy) = ((x0 - end[0]) / 2.0, (y0 - end[1]) / 2.0);
                    let (x1, y1) = (cos * dx + sin * dy, cos * dy - sin * dx);
                    let lambda = (x1 / rx).powi(2) + (y1 / ry).powi(2);
                    if lambda > 1.0 {
                        let by = lambda.sqrt();
                        (rx, ry) = (rx * by, ry * by);
                    }
                }
                let r = rx.max(ry).max(chord / 2.0);
                if r > SVG_REACH {
                    return Err(format!(
                        "an arc whose radius, grown to reach its two ends, is past {SVG_REACH:e}"
                    ));
                }
                grown = r;
                // An arc is part of its ellipse, which is no longer than
                // a circle of its larger radius's twice round.
                self.length += std::f64::consts::TAU * r;
                if let Some(edges) = self.edges.as_mut().filter(|e| e.len() < MAX_EDGES) {
                    let b = [
                        x0.min(end[0]) - r,
                        y0.min(end[1]) - r,
                        x0.max(end[0]) + r,
                        y0.max(end[1]) + r,
                    ];
                    edges.extend(box_edges(b));
                }
                self.mark(self.at, 2.0 * r);
                self.mark(end, 2.0 * r);
                end
            }
        };
        self.mark(self.at, 0.0);
        self.mark(end, 0.0);
        self.at = end;
        self.cubic = cubic;
        self.quad = quad;
        Ok(grown)
    }

    fn close(&mut self) {
        self.run(&[], self.start);
        self.at = self.start;
        self.cubic = None;
        self.quad = None;
    }
}

/// [`svg_reach`], and the box around the path's points: `(x0, y0, x1, y1)`.
pub fn svg_shape(d: &str) -> Result<(usize, f64, Option<[f64; 4]>), String> {
    svg_walk(d, false).map(|(count, reach, pen)| (count, reach, pen.bounds))
}

/// How long SVG path data is, at most (its points and control points
/// joined in order, an arc counted as its whole ellipse): what a dashed
/// stroke of it is charged by.
pub fn svg_length(d: &str) -> Result<f64, String> {
    svg_walk(d, false).map(|(_, _, pen)| pen.length)
}

/// The lines SVG path data is joined of, for [`crossings`]: each
/// curve's control points joined, an arc as the box around where it can go.
fn svg_edges(d: &str) -> Vec<[f64; 4]> {
    svg_walk(d, true)
        .ok()
        .and_then(|(_, _, pen)| pen.edges)
        .unwrap_or_default()
}

fn svg_walk(d: &str, edges: bool) -> Result<(usize, f64, Pen), String> {
    let mut pen = Pen {
        edges: edges.then(Vec::new),
        ..Pen::default()
    };
    let b = d.as_bytes();
    let mut i = 0;
    let mut count = 0usize;
    let mut first = true;
    let skip = |i: &mut usize, comma: bool| {
        let mut seen = false;
        while *i < b.len() {
            match b[*i] {
                b' ' | b'\t' | b'\n' | b'\r' | 0x0c => *i += 1,
                b',' if comma && !seen => {
                    seen = true;
                    *i += 1;
                }
                _ => break,
            }
        }
    };
    // One number: sign, digits, a fraction, an exponent.
    let num = |i: &mut usize| -> Option<f64> {
        let start = *i;
        if *i < b.len() && (b[*i] == b'+' || b[*i] == b'-') {
            *i += 1;
        }
        let mut digits = 0;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
            digits += 1;
        }
        if *i < b.len() && b[*i] == b'.' {
            *i += 1;
            while *i < b.len() && b[*i].is_ascii_digit() {
                *i += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            *i = start;
            return None;
        }
        if *i < b.len() && (b[*i] == b'e' || b[*i] == b'E') {
            let mark = *i;
            *i += 1;
            if *i < b.len() && (b[*i] == b'+' || b[*i] == b'-') {
                *i += 1;
            }
            let mut exp = 0;
            while *i < b.len() && b[*i].is_ascii_digit() {
                *i += 1;
                exp += 1;
            }
            if exp == 0 {
                *i = mark;
            }
        }
        // A number the toolkit would read as infinite is no number.
        let n = d[start..*i].parse::<f32>().ok().filter(|n| n.is_finite())?;
        Some(f64::from(n))
    };
    // An arc's flag: one `0` or `1`, which may run into what follows.
    let flag = |i: &mut usize| -> bool {
        if *i < b.len() && (b[*i] == b'0' || b[*i] == b'1') {
            *i += 1;
            true
        } else {
            false
        }
    };
    // How far from the origin a point of the path can be, at most: the
    // largest absolute coordinate, and every relative step added up (a
    // control point's on top of that), kept within [`SVG_REACH`]; and an
    // arc's radii within it too. (The toolkit's arcs, and the arcs a
    // radius too small for its two ends is grown to, cost it more the
    // larger they are: 10^10 hung it; and a point near a float's
    // largest made its GL renderer abort.)
    let mut absolute = 0.0f64;
    let mut steps = 0.0f64;
    // The furthest, of all of them (an arc's points can be its radius
    // from its centre, which is its radius from its ends).
    let mut furthest = 0.0f64;
    skip(&mut i, false);
    while i < b.len() {
        let command = b[i];
        let arity = match command.to_ascii_lowercase() {
            b'm' | b'l' | b't' => 2,
            b'h' | b'v' => 1,
            b'c' => 6,
            b's' | b'q' => 4,
            b'a' => 7,
            b'z' => 0,
            _ => {
                return Err(format!(
                    "unexpected {:?} at byte {}",
                    d[i..].chars().next().unwrap_or(' '),
                    i + 1
                ));
            }
        };
        if first && !command.eq_ignore_ascii_case(&b'm') {
            return Err("path data must start with M or m".into());
        }
        first = false;
        i += 1;
        if arity == 0 {
            count += 1;
            pen.close();
            skip(&mut i, false);
            continue;
        }
        let mut sets = 0;
        loop {
            skip(&mut i, sets > 0);
            // Another set of numbers repeats the command; anything else
            // ends it.
            if sets > 0 && (i >= b.len() || b[i].is_ascii_alphabetic()) {
                break;
            }
            let mut vals = [0.0f64; 7];
            for (k, val) in vals.iter_mut().enumerate().take(arity) {
                if k > 0 {
                    skip(&mut i, true);
                }
                let at = i;
                let needs = || {
                    format!(
                        "{:?} needs {arity} numbers (at byte {})",
                        command as char,
                        at + 1
                    )
                };
                if arity == 7 && (k == 3 || k == 4) {
                    if !flag(&mut i) {
                        return Err(needs());
                    }
                    continue;
                }
                *val = num(&mut i).ok_or_else(needs)?;
                let n = val.abs();
                let relative = command.is_ascii_lowercase();
                // What the number is: an arc's radius or its rotation, a
                // control point's coordinate, or the end's.
                let (radius, control) = match (command.to_ascii_lowercase(), k) {
                    (b'a', 0 | 1) => (true, false),
                    (b'a', 2) => continue,
                    (b'c', 0..=3) | (b's' | b'q', 0 | 1) => (false, true),
                    _ => (false, false),
                };
                let reach = if radius {
                    n
                } else if !relative {
                    absolute = absolute.max(n);
                    absolute + steps
                } else if control {
                    absolute + steps + n
                } else {
                    steps += n;
                    absolute + steps
                };
                if reach > SVG_REACH {
                    let what = if radius { "an arc's radius" } else { "a point" };
                    return Err(format!("{what} past {SVG_REACH:e} (at byte {})", at + 1));
                }
                furthest = furthest.max(if radius {
                    absolute + steps + 2.0 * n
                } else {
                    reach
                });
            }
            let grown = pen.go(command, &vals, sets == 0)?;
            furthest = furthest.max(absolute + steps + 2.0 * grown);
            sets += 1;
            count += 1;
        }
    }
    if count == 0 {
        return Err("empty path data".into());
    }
    Ok((count, furthest, pen))
}

/// The box around a path's points and control points, in its own units:
/// `(x0, y0, x1, y1)`. `None`: it has none.
pub fn bounds(path: &Path) -> Option<[f32; 4]> {
    let mut b: Option<[f32; 4]> = None;
    let mut add = |x0: f32, y0: f32, x1: f32, y1: f32| {
        let n = b.get_or_insert([x0, y0, x1, y1]);
        *n = [n[0].min(x0), n[1].min(y0), n[2].max(x1), n[3].max(y1)];
    };
    for s in &path.segments {
        match s {
            Segment::MoveTo(x, y) | Segment::LineTo(x, y) => add(*x, *y, *x, *y),
            Segment::QuadTo(a, c, x, y) => {
                add(*a, *c, *a, *c);
                add(*x, *y, *x, *y);
            }
            Segment::CurveTo(a, c, e, f, x, y) => {
                add(*a, *c, *a, *c);
                add(*e, *f, *e, *f);
                add(*x, *y, *x, *y);
            }
            Segment::Close => {}
            Segment::Rect(x, y, w, h) | Segment::RoundedRect(x, y, w, h, _) => {
                add(*x, *y, x + w, y + h);
            }
            Segment::Circle(x, y, r) => add(x - r, y - r, x + r, y + r),
            Segment::Svg(d) => {
                if let Ok((_, _, Some([x0, y0, x1, y1]))) = svg_shape(d) {
                    add(x0 as f32, y0 as f32, x1 as f32, y1 as f32);
                }
            }
        }
    }
    b
}

/// How long a path is, at most, in its own units: its points and control
/// points joined in order (a curve is no longer than the lines through
/// its control points), a circle or an arc as its whole ellipse. A dashed
/// stroke's dashes are as many as this is long over the dash's period.
pub fn length(path: &Path) -> f64 {
    let (mut at, mut start) = ([0.0f64; 2], [0.0f64; 2]);
    let mut total = 0.0;
    let mut go = |at: &mut [f64; 2], p: [f32; 2]| {
        let p = p.map(f64::from);
        total += (p[0] - at[0]).hypot(p[1] - at[1]);
        *at = p;
    };
    let mut sum = 0.0;
    for s in &path.segments {
        match s {
            Segment::MoveTo(x, y) => {
                at = [f64::from(*x), f64::from(*y)];
                start = at;
            }
            Segment::LineTo(x, y) => go(&mut at, [*x, *y]),
            Segment::QuadTo(a, b, x, y) => {
                go(&mut at, [*a, *b]);
                go(&mut at, [*x, *y]);
            }
            Segment::CurveTo(a, b, c, d, x, y) => {
                go(&mut at, [*a, *b]);
                go(&mut at, [*c, *d]);
                go(&mut at, [*x, *y]);
            }
            Segment::Close => {
                go(&mut at, [start[0] as f32, start[1] as f32]);
            }
            Segment::Rect(_, _, w, h) | Segment::RoundedRect(_, _, w, h, _) => {
                sum += 2.0 * f64::from(*w + *h);
            }
            Segment::Circle(_, _, r) => sum += std::f64::consts::TAU * f64::from(*r),
            Segment::Svg(d) => sum += svg_length(d).unwrap_or(0.0),
        }
    }
    total + sum
}

/// The most edges one path is taken to have when its crossings are
/// counted (more are left out).
const MAX_EDGES: usize = 70_000;

/// The four sides of a box `(x0, y0, x1, y1)`.
fn box_edges([x0, y0, x1, y1]: [f64; 4]) -> [[f64; 4]; 4] {
    [
        [x0, y0, x1, y0],
        [x1, y0, x1, y1],
        [x1, y1, x0, y1],
        [x0, y1, x0, y0],
    ]
}

/// The lines a path is joined of: a line is itself, a curve its control
/// points joined, a rectangle, a rounded one or a circle the box around it,
/// SVG data as [`svg_edges`] has it.
fn edges(path: &Path) -> Vec<[f64; 4]> {
    let (mut at, mut start) = ([0.0f64; 2], [0.0f64; 2]);
    let mut out: Vec<[f64; 4]> = Vec::new();
    fn to(out: &mut Vec<[f64; 4]>, at: &mut [f64; 2], p: [f32; 2]) {
        let p = p.map(f64::from);
        out.push([at[0], at[1], p[0], p[1]]);
        *at = p;
    }
    let mut boxes: Vec<[f64; 4]> = Vec::new();
    let mut svg: Vec<[f64; 4]> = Vec::new();
    for s in &path.segments {
        match s {
            Segment::MoveTo(x, y) => {
                at = [f64::from(*x), f64::from(*y)];
                start = at;
            }
            Segment::LineTo(x, y) => to(&mut out, &mut at, [*x, *y]),
            Segment::QuadTo(a, b, x, y) => {
                to(&mut out, &mut at, [*a, *b]);
                to(&mut out, &mut at, [*x, *y]);
            }
            Segment::CurveTo(a, b, c, d, x, y) => {
                to(&mut out, &mut at, [*a, *b]);
                to(&mut out, &mut at, [*c, *d]);
                to(&mut out, &mut at, [*x, *y]);
            }
            Segment::Close => {
                out.push([at[0], at[1], start[0], start[1]]);
                at = start;
            }
            Segment::Rect(x, y, w, h) | Segment::RoundedRect(x, y, w, h, _) => {
                let [x, y, w, h] = [*x, *y, *w, *h].map(f64::from);
                boxes.extend(box_edges([x, y, x + w, y + h]));
            }
            Segment::Circle(x, y, r) => {
                let [x, y, r] = [*x, *y, *r].map(f64::from);
                boxes.extend(box_edges([x - r, y - r, x + r, y + r]));
            }
            Segment::Svg(d) => svg.extend(svg_edges(d)),
        }
        if out.len() + boxes.len() + svg.len() > MAX_EDGES {
            break;
        }
    }
    out.extend(boxes);
    out.extend(svg);
    out
}

/// Steps of the sweep [`crossings`] makes, at most: a path whose edges
/// need more is counted as the worst.
const SWEEP_STEPS: usize = 4_000_000;

/// How many pairs of a path's edges cross (`edges`: [`edges`]'s), counted
/// to `limit` and no further (then `limit` is what it says). A pair is
/// counted when each edge's ends lie on either side of the other: lines
/// that only touch, as the neighbours of a polyline do, are not. Which
/// pairs to try is found by a sweep over the edges' boxes in x, within
/// [`SWEEP_STEPS`]; a path that needs more than that is given as having
/// `limit`.
///
/// The toolkit's tessellator spends time for each crossing of a path, and
/// a path of 16 000 lines that all cross is 64 million of them: a stroke
/// of one took it two minutes, a fill three seconds.
pub fn crossings(path: &Path, limit: f64) -> f64 {
    let edges = edges(path);
    count_crossings(&edges, limit)
}

fn count_crossings(edges: &[[f64; 4]], limit: f64) -> f64 {
    // A line with no length (`close` at the point the path is at) crosses
    // nothing.
    let lines: Vec<[f64; 4]> = edges
        .iter()
        .copied()
        .filter(|e| e[0] != e[2] || e[1] != e[3])
        .collect();
    let edges = &lines[..];
    // A path of a few lines has few crossings, and they are cheap.
    if edges.len() < 16 {
        return 0.0;
    }
    // (x0, x1, y0, y1) of each edge's box.
    let boxes: Vec<[f64; 4]> = edges
        .iter()
        .map(|e| {
            [
                e[0].min(e[2]),
                e[0].max(e[2]),
                e[1].min(e[3]),
                e[1].max(e[3]),
            ]
        })
        .collect();
    let mut order: Vec<usize> = (0..edges.len()).collect();
    order.sort_by(|a, b| boxes[*a][0].total_cmp(&boxes[*b][0]));
    let side =
        |e: &[f64; 4], x: f64, y: f64| (e[2] - e[0]) * (y - e[1]) - (e[3] - e[1]) * (x - e[0]);
    let (mut active, mut found, mut steps): (Vec<usize>, f64, usize) = (Vec::new(), 0.0, 0);
    for i in order {
        let b = boxes[i];
        active.retain(|j| boxes[*j][1] >= b[0]);
        steps += active.len() + 1;
        if steps > SWEEP_STEPS {
            return limit;
        }
        for j in &active {
            let c = boxes[*j];
            if c[2] > b[3] || c[3] < b[2] {
                continue;
            }
            let (e, f) = (&edges[i], &edges[*j]);
            let (s1, s2) = (side(e, f[0], f[1]), side(e, f[2], f[3]));
            let (s3, s4) = (side(f, e[0], e[1]), side(f, e[2], e[3]));
            if s1 * s2 < 0.0 && s3 * s4 < 0.0 {
                found += 1.0;
                if found >= limit {
                    return limit;
                }
            }
        }
        active.push(i);
    }
    found
}

/// How far from the origin, in its own units, a point of the path can be,
/// at most.
pub fn reach(path: &Path) -> f32 {
    let far = |c: &[f32]| c.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    path.segments
        .iter()
        .map(|s| match s {
            Segment::MoveTo(x, y) | Segment::LineTo(x, y) => far(&[*x, *y]),
            Segment::QuadTo(a, b, x, y) => far(&[*a, *b, *x, *y]),
            Segment::CurveTo(a, b, c, d, x, y) => far(&[*a, *b, *c, *d, *x, *y]),
            Segment::Close => 0.0,
            Segment::Rect(x, y, w, h) | Segment::RoundedRect(x, y, w, h, _) => {
                far(&[*x, *y, x + w, y + h])
            }
            Segment::Circle(x, y, r) => far(&[*x, *y]) + r,
            Segment::Svg(d) => svg_reach(d).map_or(SVG_REACH, |(_, r)| r) as f32,
        })
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::{Path, Segment, count_crossings, length, svg_length, svg_segments};

    /// Pairs of lines that cross, and only those: a grid of 20 by 20 lines
    /// has 400; parallel lines, a polyline's neighbours and lines that
    /// only touch have none; and counting stops at the limit.
    #[test]
    fn crossings_are_counted_to_a_limit() {
        let mut grid = Vec::new();
        for i in 0..20 {
            let i = f64::from(i);
            grid.push([-1.0, i, 21.0, i]);
            grid.push([i, -1.0, i, 21.0]);
        }
        assert_eq!(count_crossings(&grid, 1e9), 400.0);
        assert_eq!(count_crossings(&grid, 50.0), 50.0);
        let parallel: Vec<[f64; 4]> = (0..40)
            .map(|i| [0.0, f64::from(i), 9.0, f64::from(i) + 5.0])
            .collect();
        assert_eq!(count_crossings(&parallel, 1e9), 0.0);
        let polyline: Vec<[f64; 4]> = (0..40)
            .map(|i| {
                [
                    f64::from(i),
                    f64::from(i % 2),
                    f64::from(i + 1),
                    f64::from((i + 1) % 2),
                ]
            })
            .collect();
        assert_eq!(count_crossings(&polyline, 1e9), 0.0);
        // A few lines are not counted at all, nor are lines of no length
        // (16 384 `close`s of a path at its start).
        assert_eq!(count_crossings(&grid[..10], 1e9), 0.0);
        assert_eq!(
            count_crossings(&vec![[3.0, 4.0, 3.0, 4.0]; 16384], 10.0),
            0.0
        );
    }

    /// How long a path is, at most (a dashed stroke is charged by it): the
    /// points and control points joined in order, a closed path back to
    /// its start, a circle or an arc as its whole.
    #[test]
    fn a_path_is_never_longer_than_its_length() {
        let near = |a: f64, b: f64| (a - b).abs() < 1e-3;
        assert!(near(
            svg_length("M0 0 L3 4 h5 v-12z").unwrap(),
            5.0 + 5.0 + 12.0 + 8.0f64.hypot(8.0)
        ));
        // A relative step is the step's length; a curve its control polygon.
        assert!(near(svg_length("m1 1 l3 4").unwrap(), 5.0));
        assert!(near(
            svg_length("M0 0 C 0 3 4 3 4 0").unwrap(),
            3.0 + 4.0 + 3.0
        ));
        assert!(near(svg_length("M0 0 Q 0 3 4 0").unwrap(), 3.0 + 5.0));
        // An arc: a circle of its radius twice round at most.
        assert!(svg_length("M0 0 A 10 10 0 0 1 20 0").unwrap() >= std::f64::consts::PI * 10.0);
        assert!(svg_length("M0 0").unwrap() == 0.0);
        let path = Path {
            segments: vec![
                Segment::MoveTo(0.0, 0.0),
                Segment::LineTo(3.0, 4.0),
                Segment::Close,
                Segment::Rect(0.0, 0.0, 10.0, 20.0),
                Segment::Circle(0.0, 0.0, 1.0),
                Segment::Svg("M0 0 L0 7".into()),
            ],
        };
        assert!(near(
            length(&path),
            5.0 + 5.0 + 60.0 + std::f64::consts::TAU + 7.0
        ));
    }

    #[test]
    fn svg_data_is_checked_and_counted() {
        assert_eq!(svg_segments("M6 12.5h12"), Ok(2));
        assert_eq!(svg_segments("M0,0 L10,0 10,10 0,10z"), Ok(5));
        assert_eq!(svg_segments("m1 1c1 2 3 4 5 6 7 8 9 10 11 12Z"), Ok(4));
        // Flags run into the numbers after them.
        assert_eq!(svg_segments("M10 10a5 5 0 01-3.5 8.5"), Ok(2));
        assert_eq!(svg_segments("M1-2.5e1.5.5"), Ok(2));
        assert!(svg_segments("").is_err());
        assert!(svg_segments("L1 1").is_err());
        assert!(svg_segments("M1").is_err());
        assert!(svg_segments("M1 1 url(x)").is_err());
        assert!(svg_segments("M1 1 L 1e999 2").is_err());
        assert!(svg_segments("M10 10a5 5 0 2 1 3 3").is_err());
    }

    /// Each number finite was not enough: an arc of radius 10^12 hung the
    /// toolkit on every renderer, and points near a float's largest (or
    /// relative steps that add up past it) made its GL renderer abort.
    #[test]
    fn svg_data_reaches_no_further_than_the_toolkit_draws_cheaply() {
        // Within reach: drawn.
        assert_eq!(svg_segments("M-1e6 0 L1e6 0"), Ok(2));
        assert_eq!(svg_segments("M0 0 A1e6 1e6 0 1 1 10 10"), Ok(2));
        assert_eq!(svg_segments("m0 0 l5e5 0 5e5 0"), Ok(3));
        assert_eq!(svg_segments("M0 0 c1e6 0 1e6 1e6 0 1e6"), Ok(2));
        // An arc's rotation is an angle: any finite one.
        assert_eq!(svg_segments("M0 0 A5 5 1e30 0 0 10 10"), Ok(2));
        // Past it: refused.
        for far in [
            "M0 0 A1e12 1e12 0 1 1 10 10",
            "M0 0 a3 3e38 0 1 1 10 10",
            "M-3e38 -3e38 L3e38 -3e38 L3e38 3e38 Z",
            "M0 0 L1.5e6 0",
            "M0 0 C2e6 0 0 0 1 1",
            // Relative steps, each within reach, adding up past it.
            "m0 0 l6e5 0 6e5 0",
            "m0 0 h4e5 v4e5 h4e5",
            // An absolute point, then relative steps from it.
            "M9e5 0 l2e5 0",
            // A relative control point beyond where the path has got to.
            "M9e5 0 q2e5 0 0 0",
        ] {
            let e = svg_segments(far).unwrap_err();
            assert!(e.contains("past"), "{far}: {e}");
        }
        let long = format!("m0 0{}", " l3e38 3e38".repeat(8));
        assert!(svg_segments(&long).is_err());
    }

    /// A radius too small for an arc's two ends is grown to reach them, both
    /// radii by the root of how far the ends are out of the ellipse: a long
    /// thin ellipse, its short side across the chord, is grown to a very long
    /// one, however small its numbers. (R2-E-1: `A 1e6 1e-27 90 1 0 300 10`
    /// hung the toolkit's software renderer past a minute.)
    #[test]
    fn an_arc_grown_past_reach_is_refused() {
        for far in [
            "M10 10 A1000000 0.001 90 0 0 300 10",
            "M10 10 A1000000 0.000000000000000000000000001 90 1 0 300 10",
            "M10 10 a1 1e-9 135 0 1 300 300",
        ] {
            let e = svg_segments(far).unwrap_err();
            assert!(e.contains("past"), "{far}: {e}");
        }
        // The same arcs turned so that the long side is along the chord, and
        // radii grown a little, are drawn.
        assert_eq!(svg_segments("M10 10 A1000000 0.001 0 0 0 300 10"), Ok(2));
        assert_eq!(svg_segments("M0 0 A5 5 0 0 0 10 10"), Ok(2));
        assert_eq!(svg_segments("M10 10 a1 1e-9 45 0 1 300 300"), Ok(2));
        assert_eq!(svg_segments("M0 0 A1e-30 1e-30 0 0 0 10 10"), Ok(2));
        assert_eq!(svg_segments("M0 0 A0 1e-30 90 0 0 3e5 10"), Ok(2));
    }
}
