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
                    n(w)?.max(0.0),
                    n(w)?.max(0.0),
                    n(w)?.max(0.0),
                    n(w)?.max(0.0),
                ];
                (Segment::RoundedRect(x, y, wd, h, Radius(r)), 1)
            }
            CIRCLE => {
                let w = "circle";
                (Segment::Circle(n(w)?, n(w)?, n(w)?.max(0.0)), 1)
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
        segments.push(segment);
    }
    Ok((Path { segments }, used))
}

/// Checks SVG path data and counts its segments: the commands of the SVG
/// path grammar (`MmLlHhVvCcSsQqTtAaZz`), each with as many numbers as it
/// takes, repeated or not. What passes here is what the toolkit's parser
/// is given; anything else is refused before it gets there.
pub fn svg_segments(d: &str) -> Result<usize, String> {
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
    let num = |i: &mut usize| -> bool {
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
            return false;
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
        d[start..*i].parse::<f32>().is_ok_and(f32::is_finite)
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
            for k in 0..arity {
                if k > 0 {
                    skip(&mut i, true);
                }
                let ok = if arity == 7 && (k == 3 || k == 4) {
                    flag(&mut i)
                } else {
                    num(&mut i)
                };
                if !ok {
                    return Err(format!(
                        "{:?} needs {arity} numbers (at byte {})",
                        command as char,
                        i + 1
                    ));
                }
            }
            sets += 1;
            count += 1;
        }
    }
    if count == 0 {
        return Err("empty path data".into());
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::svg_segments;

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
}
