//! Fields and ramps (docs/theming-media.md, "Fields: swirls, plasma,
//! contours"): `gmnb.ramp` and `draw:field`.
//!
//! A theme computes a coarse grid of numbers; the app interpolates it
//! for every pixel of a picture and only then looks the colour up, in a
//! ramp the theme declared. The pass itself is `gmnb_media::field`'s,
//! which is the contract's, step for step. It runs inside the `draw`
//! call that asked for it, on that call's time: it is part of drawing.
//!
//! **How often.** A layer's fields are coloured 30 times a second at
//! most, two in hand ([`gmnb_media::Bucket`]). A call past that draws the
//! picture that field made last, again, and the layer is drawn once more
//! when the allowance has a pass to give, so that a picture that stands
//! (a theme that doesn't animate, reduced motion) is never left out of
//! date. When a `draw` has two fields and the allowance is short, they
//! take turns: the one that went without last time goes first. A
//! preview's tile is a still picture drawn once: its fields are always
//! coloured.
//!
//! **What isn't a pass.** A field asked for with the same numbers, ramp
//! and size as its last picture is that picture: nothing is coloured and
//! nothing spent. So a layer that is drawn again for another reason (an
//! event, the pointer, the frame it was owed) pays only for the fields
//! that changed.
//!
//! **Ramps.** A theme may make as many as it likes (a theme whose
//! colours follow the desktop's accent, or colours the user picks, makes
//! a new one for each), but the tables of only `Limits::ramps` are held
//! at once: making or drawing one past that lets go of the one used
//! longest ago, and a ramp let go is made again, the same, from its own
//! table when it is next drawn (a few microseconds).

use std::collections::HashMap;
use std::rc::Rc;

use gmnb_media::field::{self, MAX_COLORS, MAX_PIXELS, MAX_POINTS, MAX_SIDE, MIN_COLORS, MIN_SIDE};
use gmnb_media::{Bucket, Bytes, Ramp};
use mlua::{Lua, Table, Value};

use crate::draw::{self, Op, Shared};
use crate::library::color_arg;

/// What one pass made: a picture to be scaled over the rectangle the
/// field was drawn in.
#[derive(Clone, PartialEq)]
pub struct FieldPicture {
    /// A number no other picture of this theme has: what a texture made
    /// of it is known by.
    pub serial: u64,
    pub width: u32,
    pub height: u32,
    /// RGBA8, premultiplied, `width × 4` bytes a row.
    pub pixels: Bytes,
    /// The ramp it was coloured through: every pixel is one of its
    /// table's colours, or a blend of neighbours as the picture is
    /// scaled. (For the tests that hold what is drawn to what reads.)
    pub ramp: Rc<Ramp>,
}

impl std::fmt::Debug for FieldPicture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // (The ramp's table is a few thousand colours: left out.)
        f.debug_struct("FieldPicture")
            .field("serial", &self.serial)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("pixels", &self.pixels)
            .finish()
    }
}

/// One layer's allowance, and which of its fields went without last.
#[derive(Default)]
struct Turn {
    allowance: Bucket,
    /// How many of the layer's draws have had a field.
    draw: u64,
    /// A field of this layer that asked and was refused, and in which
    /// draw: in the next draw it goes first.
    waiting: Option<(usize, u64)>,
}

/// A theme's ramps, and what its layers' fields made last.
#[derive(Default)]
pub(crate) struct Fields {
    /// The ramps whose colours are held: at most `Limits::ramps`, by
    /// number, each with when it was last made or drawn. A ramp let go is
    /// made again from its own table when it is next drawn.
    held: Vec<(u64, Rc<Ramp>, u64)>,
    /// How many ramps have been made: the number the last one got.
    made: u64,
    /// The clock `held` is kept by: a tick for each ramp made or drawn.
    used: u64,
    /// What the last ramps were asked for by (their colours, edges and
    /// cycle), and each one's table's place among the ramps' tables: the
    /// same ramp asked for again is the same table.
    asked: Vec<(String, usize)>,
    /// How many tables have been made.
    tables: usize,
    serial: u64,
    turns: HashMap<u64, Turn>,
    /// By layer and by which field of its `draw` (the first, the second):
    /// the picture it made last, and what that was made of.
    last: HashMap<(u64, usize), (Asked, Rc<FieldPicture>)>,
}

/// What a pass is made of: the numbers after the glide, their shape, the
/// ramp and how a value becomes a position. Asked for again unchanged,
/// the picture is the one already made.
struct Asked {
    values: Vec<f32>,
    shape: (usize, usize, usize),
    /// The ramp's number: a ramp made again is the same ramp.
    ramp_id: u64,
    ramp: Rc<Ramp>,
    /// As bits: the same number, not merely an equal one.
    scale: u32,
    offset: u32,
}

impl PartialEq for Asked {
    fn eq(&self, other: &Asked) -> bool {
        self.shape == other.shape
            && self.scale == other.scale
            && self.offset == other.offset
            && self.ramp_id == other.ramp_id
            && self.values.len() == other.values.len()
            && self
                .values
                .iter()
                .zip(&other.values)
                .all(|(a, b)| a.to_bits() == b.to_bits())
    }
}

/// Layers and fields whose last pictures are kept at once.
const KEPT: usize = 32;

fn bad(what: &str, problem: impl std::fmt::Display) -> mlua::Error {
    mlua::Error::runtime(format!("{what}: {problem}"))
}

fn finite(what: &str, v: &Value) -> mlua::Result<f64> {
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

/// Specs whose ramp `gmnb.ramp` hands out again rather than make anew.
const ASKED: usize = 64;

/// Holds `ramp` as ramp number `id`, the newest: at most `limit` are
/// held, and the one made or drawn longest ago is let go to make room.
fn hold(fields: &mut Fields, limit: usize, id: u64, ramp: Rc<Ramp>) {
    fields.used += 1;
    let now = fields.used;
    fields.held.retain(|(held, _, _)| *held != id);
    while !fields.held.is_empty() && fields.held.len() >= limit.max(1) {
        let oldest = (0..fields.held.len())
            .min_by_key(|&i| fields.held[i].2)
            .unwrap_or(0);
        fields.held.swap_remove(oldest);
    }
    fields.held.push((id, ramp, now));
}

/// The colours ramp number `id` is drawn in: the ones held, else made
/// again from its table (`colors`, `edge` and `cycle`, as `gmnb.ramp`
/// wrote them), and then held.
fn ramp_of(shared: &Shared, what: &str, id: u64, t: &Table) -> mlua::Result<Rc<Ramp>> {
    let limit = shared.recorder.borrow().limits().ramps;
    let mut fields = shared.fields.borrow_mut();
    let not_one = || bad(what, "`ramp` is not a ramp of this theme");
    if id == 0 || id > fields.made {
        return Err(not_one());
    }
    fields.used += 1;
    let now = fields.used;
    if let Some(held) = fields.held.iter_mut().find(|(held, _, _)| *held == id) {
        held.2 = now;
        return Ok(held.1.clone());
    }
    let colors = match t.raw_get::<Value>("colors")? {
        // The table is the theme's to forge (it need only carry a ramp's
        // number): what it holds is read only if it is a ramp's size.
        Value::Table(list) => {
            let n = list.raw_len();
            if !(MIN_COLORS..=MAX_COLORS).contains(&n) {
                return Err(not_one());
            }
            (1..=n)
                .map(|i| color_arg("ramp", &list.raw_get::<Value>(i)?))
                .collect::<mlua::Result<Vec<_>>>()?
        }
        _ => return Err(not_one()),
    };
    let edge = match t.raw_get::<Value>("edge")? {
        Value::Table(e) => {
            let at = |i: usize| -> mlua::Result<f32> {
                Ok(finite(what, &e.raw_get::<Value>(i)?)? as f32)
            };
            [at(1)?, at(2)?]
        }
        _ => return Err(not_one()),
    };
    let cycle = matches!(t.raw_get::<Value>("cycle")?, Value::Boolean(true));
    let ramp = Rc::new(Ramp::new(&colors, edge, cycle).ok_or_else(not_one)?);
    hold(&mut fields, limit, id, ramp.clone());
    Ok(ramp)
}

/// Adds `gmnb.ramp`. Each ramp's table is made once, and the last few
/// asked for are kept in a table of the registry's to be handed out again.
pub(crate) fn register(lua: &Lua, gmnb: &Table, shared: &Rc<Shared>) -> mlua::Result<()> {
    let tables = Rc::new(lua.create_registry_value(lua.create_table()?)?);
    let s = shared.clone();
    gmnb.raw_set(
        "ramp",
        lua.create_function(move |lua, spec: Value| {
            let what = "gmnb.ramp";
            let Value::Table(spec) = spec else {
                return Err(bad(what, "expected { colors, edge, cycle }"));
            };
            let colors = match spec.raw_get::<Value>("colors")? {
                Value::Table(list) => {
                    let n = list.raw_len();
                    if !(MIN_COLORS..=MAX_COLORS).contains(&n) {
                        return Err(bad(
                            what,
                            format!("{n} colours; a ramp takes {MIN_COLORS} to {MAX_COLORS}"),
                        ));
                    }
                    (1..=n)
                        .map(|i| color_arg("ramp", &list.raw_get::<Value>(i)?))
                        .collect::<mlua::Result<Vec<_>>>()?
                }
                _ => return Err(bad(what, "`colors` is a list of 2 to 16 colours")),
            };
            let edge = match spec.raw_get::<Value>("edge")? {
                Value::Nil => [0.0, 1.0],
                Value::Table(t) if t.raw_len() == 2 => {
                    let at = |i: usize| -> mlua::Result<f32> {
                        Ok(finite(what, &t.raw_get::<Value>(i)?)?.clamp(0.0, 1.0) as f32)
                    };
                    let edge = [at(1)?, at(2)?];
                    if edge[0] > edge[1] {
                        return Err(bad(
                            what,
                            "`edge` is where the blend starts, then where it ends",
                        ));
                    }
                    edge
                }
                _ => return Err(bad(what, "`edge` is two numbers from 0 to 1")),
            };
            let cycle = match spec.raw_get::<Value>("cycle")? {
                Value::Nil => false,
                Value::Boolean(b) => b,
                _ => return Err(bad(what, "`cycle` is true or false")),
            };
            let tables: Table = lua.registry_value(&tables)?;
            let key = format!("{colors:?}{edge:?}{cycle}");
            let mut fields = s.fields.borrow_mut();
            if let Some((_, at)) = fields.asked.iter().find(|(k, _)| *k == key) {
                return tables.raw_get::<Table>(*at);
            }
            // The checks above are the ramp's own.
            let ramp = Ramp::new(&colors, edge, cycle)
                .ok_or_else(|| bad(what, "a ramp that can't be made"))?;
            // What the theme reads back: its colours as the app took
            // them, and the number the app knows the ramp by.
            let t = lua.create_table_with_capacity(0, 4)?;
            let list = lua.create_table_with_capacity(colors.len(), 0)?;
            for (i, c) in colors.iter().enumerate() {
                let color = lua.create_table_with_capacity(0, 4)?;
                for (name, part) in ["r", "g", "b", "a"].iter().zip(c) {
                    color.raw_set(*name, *part)?;
                }
                color.set_readonly(true);
                list.raw_seti(i + 1, color)?;
            }
            list.set_readonly(true);
            t.raw_set("colors", list)?;
            let edges = lua.create_sequence_from(edge)?;
            edges.set_readonly(true);
            t.raw_set("edge", edges)?;
            t.raw_set("cycle", cycle)?;
            let limit = s.recorder.borrow().limits().ramps;
            fields.made += 1;
            let id = fields.made;
            hold(&mut fields, limit, id, Rc::new(ramp));
            t.raw_set("id", id)?;
            t.set_readonly(true);
            // Kept to be handed out again for the same spec: the last 64
            // asked for. An older one is the theme's to keep (drawn
            // again, it is made again) or to let go.
            fields.tables += 1;
            let at = fields.tables;
            tables.raw_seti(at, t.clone())?;
            fields.asked.push((key, at));
            if fields.asked.len() > ASKED {
                let (_, gone) = fields.asked.remove(0);
                tables.raw_seti(gone, Value::Nil)?;
            }
            Ok(t)
        })?,
    )?;
    Ok(())
}

/// `draw:field(x, y, w, h, spec)`.
pub(crate) fn draw_field(
    shared: &Shared,
    (x, y, w, h): (f64, f64, f64, f64),
    spec: Value,
) -> mlua::Result<()> {
    let what = "draw:field";
    let name = "field";
    let Value::Table(spec) = spec else {
        return Err(bad(
            what,
            "expected { data, cols, rows, channels, ramp, … }",
        ));
    };
    let rect = draw::rect(name, x, y, w, h)?;
    shared.recorder.borrow().ready(name)?;
    let whole = |key: &str, least: usize, most: usize| -> mlua::Result<usize> {
        let n = finite(what, &spec.raw_get::<Value>(key)?)?;
        if n.fract() != 0.0 || n < least as f64 || n > most as f64 {
            return Err(bad(
                what,
                format!("`{key}` is a whole number from {least} to {most}"),
            ));
        }
        Ok(n as usize)
    };
    let cols = whole("cols", MIN_SIDE, MAX_SIDE)?;
    let rows = whole("rows", MIN_SIDE, MAX_SIDE)?;
    let channels = whole("channels", 1, 2)?;
    if cols * rows > MAX_POINTS {
        return Err(bad(
            what,
            format!("{cols} × {rows} points; a field has at most {MAX_POINTS}"),
        ));
    }
    let number = |key: &str, default: f64| -> mlua::Result<f64> {
        match spec.raw_get::<Value>(key)? {
            Value::Nil => Ok(default),
            v => finite(what, &v),
        }
    };
    let scale = number("scale", 1.0)? as f32;
    let offset = number("offset", 0.0)? as f32;
    let mix = number("mix", 0.0)?.clamp(0.0, 1.0) as f32;
    let quality = number("quality", 0.5)?.clamp(0.1, 1.0) as f32;
    let buffer = |key: &str, needed: bool| -> mlua::Result<Option<mlua::Buffer>> {
        match spec.raw_get::<Value>(key)? {
            Value::Buffer(b) => Ok(Some(b)),
            Value::Nil if !needed => Ok(None),
            _ => Err(bad(
                what,
                format!("`{key}` is a buffer of 32-bit floats (buffer.writef32)"),
            )),
        }
    };
    let data = buffer("data", true)?;
    let next = buffer("next", false)?;
    // The ramp: a table `gmnb.ramp` made, known by its number.
    let (ramp_id, ramp) = match spec.raw_get::<Value>("ramp")? {
        Value::Table(t) => {
            let n = match t.raw_get::<Value>("id")? {
                Value::Nil => return Err(bad(what, "`ramp` is not a ramp of this theme")),
                v => finite(what, &v)?,
            };
            if n.fract() != 0.0 || n < 1.0 {
                return Err(bad(what, "`ramp` is not a ramp of this theme"));
            }
            (n as u64, ramp_of(shared, what, n as u64, &t)?)
        }
        _ => return Err(bad(what, "`ramp` is a ramp made by gmnb.ramp")),
    };

    // Which field of this draw it is: there may be two.
    let (layer, slot, still) = {
        let mut rec = shared.recorder.borrow_mut();
        let cap = rec.limits().fields;
        if rec.fields >= cap {
            return Err(rec.over(name, format!("more than {cap} fields in one frame")));
        }
        rec.fields += 1;
        (rec.layer, rec.fields - 1, rec.still())
    };
    // A buffer of the wrong size: the call does nothing, and says so.
    let need = cols * rows * channels * 4;
    let held = data.as_ref().map_or(0, |b| b.len());
    let wrong = if held != need {
        Some(("data", held))
    } else {
        next.as_ref()
            .map(|b| b.len())
            .filter(|len| *len != need)
            .map(|len| ("next", len))
    };
    if let Some((which, held)) = wrong {
        shared.recorder.borrow_mut().note(format!(
            "draw:field: `{which}` holds {held} bytes; {cols} × {rows} points of {channels} \
             take {need}"
        ));
        return Ok(());
    }
    let Some(data) = data else {
        return Ok(());
    };
    if rect[2] <= 0.0 || rect[3] <= 0.0 {
        return Ok(());
    }

    // The numbers the pass would colour, and the size it would make.
    let values = field::values(&data.to_vec(), next.map(|b| b.to_vec()).as_deref(), mix);
    let (width, height) = field::output_size(rect[2], rect[3], quality, MAX_PIXELS);
    let asked = Asked {
        values,
        shape: (cols, rows, channels),
        ramp_id,
        ramp,
        scale: scale.to_bits(),
        offset: offset.to_bits(),
    };

    let now = shared.now();
    let mut fields = shared.fields.borrow_mut();
    if fields.turns.len() > KEPT || fields.last.len() > KEPT {
        fields.turns.clear();
        fields.last.clear();
    }
    let turn = fields.turns.entry(layer).or_default();
    if slot == 0 {
        turn.draw += 1;
    }
    // A field that hasn't been drawn since it was refused waits no more.
    if turn.waiting.is_some_and(|(_, at)| at + 1 < turn.draw) {
        turn.waiting = None;
    }
    // The same numbers through the same ramp at the same size as this
    // field's last picture: that picture is this one. Nothing is coloured
    // and nothing spent. (A layer drawn again for another reason, or to
    // make good a field that was refused, costs its other fields
    // nothing.)
    let same = fields
        .last
        .get(&(layer, slot))
        .is_some_and(|(made, picture)| {
            *made == asked && (picture.width, picture.height) == (width, height)
        });
    let turn = fields.turns.entry(layer).or_default();
    let (granted, retry) = if same {
        if turn.waiting.is_some_and(|(waiting, _)| waiting == slot) {
            turn.waiting = None;
        }
        (false, None)
    } else {
        // Its turn, if the allowance has a pass to give: a field that
        // went without is served before one that didn't.
        let yields = turn
            .waiting
            .is_some_and(|(other, at)| other != slot && at < turn.draw);
        // (A preview's tile is drawn once and stands: it is always
        // coloured.)
        let granted = still || (!yields && turn.allowance.take(now));
        if granted {
            if turn.waiting.is_some_and(|(waiting, _)| waiting == slot) {
                turn.waiting = None;
            }
        } else if !yields && turn.waiting.is_none() {
            turn.waiting = Some((slot, turn.draw));
        }
        // Refused: the layer is drawn once more when there is a pass to
        // be had, whatever its hook returns.
        let retry = (!granted).then(|| turn.allowance.wait(now).max(1.0 / 60.0));
        (granted, retry)
    };
    let picture = if granted {
        let grid = field::Field {
            data: &asked.values,
            cols,
            rows,
            channels,
        };
        let Some(pixels) = field::render(
            &grid,
            &asked.ramp,
            scale,
            offset,
            width as usize,
            height as usize,
        ) else {
            return Ok(());
        };
        fields.serial += 1;
        let picture = Rc::new(FieldPicture {
            serial: fields.serial,
            width,
            height,
            pixels: Bytes::new(pixels),
            ramp: asked.ramp.clone(),
        });
        fields.last.insert((layer, slot), (asked, picture.clone()));
        Some(picture)
    } else {
        // The same picture, or too soon for another: the picture this
        // field made last, again. (None yet: nothing this once.)
        fields
            .last
            .get(&(layer, slot))
            .map(|(_, picture)| picture.clone())
    };
    drop(fields);
    let mut rec = shared.recorder.borrow_mut();
    if let Some(wait) = retry {
        rec.retry(wait);
    }
    match picture {
        Some(picture) => rec.push(name, Op::Field { picture, rect }),
        None => Ok(()),
    }
}
