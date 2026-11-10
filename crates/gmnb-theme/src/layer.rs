//! Layers (docs/theming.md, "Drawing"): what a frame is told, and running
//! a layer's hooks.
//!
//! The app describes a frame as plain data ([`Frame`]); [`Frames`] turns
//! it into the table a hook reads, refilling in place what it made
//! before: a frame's own fields every time, a keypad's cells only when
//! its layout changes, a readout's value only when the value does. All of
//! it is the host's ([`crate::HostTable`]): outside the theme's heap cap,
//! read-only to scripts.

use std::collections::HashMap;

use mlua::{Lua, Table, Value};

use crate::draw::{DisplayList, Rect};
use crate::events::{Event, KeyKind};
use crate::tokens::OptionValue;
use crate::{Engine, HostTable, ThemeError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LayerKind {
    Background,
    Keypad,
    Display,
    Overlay,
}

impl LayerKind {
    pub const ALL: [LayerKind; 4] = [
        LayerKind::Background,
        LayerKind::Keypad,
        LayerKind::Display,
        LayerKind::Overlay,
    ];

    /// Its name in a theme's `layers` table.
    pub fn name(self) -> &'static str {
        match self {
            LayerKind::Background => "background",
            LayerKind::Keypad => "keypad",
            LayerKind::Display => "display",
            LayerKind::Overlay => "overlay",
        }
    }
}

/// One cell of a keypad: a key, or another widget that sits in its grid.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyCell {
    /// The key's number; `None` on cells that aren't calculator keys.
    pub id: Option<u32>,
    pub kind: KeyKind,
    pub rect: Rect,
    pub row: i32,
    pub col: i32,
    /// Its corner radius, from the style.
    pub radius: f32,
    pub enabled: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub checked: bool,
}

/// A keypad's cells.
#[derive(Clone, Copy, Debug)]
pub struct Keys<'a> {
    /// Which keypad: the same number for the same keypad every frame.
    pub pad: u64,
    /// Changes whenever a cell's place, kind, id or radius does (not its
    /// state): until then the cells' tables are kept.
    pub layout: u64,
    pub cells: &'a [KeyCell],
}

/// Where one character of a readout's value is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A value as the app laid it out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Glyphs {
    pub glyphs: Vec<GlyphBox>,
    /// The fitted font size.
    pub size: f32,
    /// The box around the whole value.
    pub rect: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Typing,
    Result,
    Replace,
    Error,
}

impl ChangeKind {
    pub fn name(self) -> &'static str {
        match self {
            ChangeKind::Typing => "typing",
            ChangeKind::Result => "result",
            ChangeKind::Replace => "replace",
            ChangeKind::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Calculator,
    Converter,
    Date,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Calculator => "calculator",
            Role::Converter => "converter",
            Role::Date => "date",
        }
    }
}

/// A readout's value, for the display layer.
#[derive(Clone, Copy, Debug)]
pub struct DisplayValue<'a> {
    /// Which readout: the same number for the same readout every frame.
    pub readout: u64,
    /// Changes whenever anything here but the change's age does.
    pub version: u64,
    pub text: &'a str,
    pub glyphs: &'a Glyphs,
    /// The strip the value sits in, as wide as the readout.
    pub band: Rect,
    pub error: bool,
    /// Set against the readout's end (the calculator) or its start.
    pub align_end: bool,
    pub role: Role,
    /// While a change is being shown: its kind, its age in seconds, and
    /// the first glyph that differs from the old value (from 1).
    pub change: Option<(ChangeKind, f64, u32)>,
    /// The old value, for as long as `change` is there.
    pub previous: Option<&'a Glyphs>,
}

/// What a frame is told.
#[derive(Clone, Debug)]
pub struct Frame<'a> {
    pub width: f32,
    pub height: f32,
    pub time: f64,
    pub dt: f64,
    pub motion: bool,
    pub settled: bool,
    pub active: bool,
    /// For layers that asked for the pointer, while it is over them.
    pub pointer: Option<(f32, f32)>,
    /// Each event's age and the event, newest last.
    pub events: Vec<(f64, &'a Event)>,
    pub keys: Option<Keys<'a>>,
    pub value: Option<DisplayValue<'a>>,
}

impl Frame<'_> {
    /// A still frame of this size with nothing in it.
    pub fn sized(width: f32, height: f32) -> Frame<'static> {
        Frame {
            width,
            height,
            time: 0.0,
            dt: 0.0,
            motion: true,
            settled: false,
            active: true,
            pointer: None,
            events: Vec::new(),
            keys: None,
            value: None,
        }
    }
}

/// How `place` moves one cell of a keypad: scaled about its centre, moved
/// and faded. The picture only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
    /// 0: the cell isn't drawn.
    pub opacity: f32,
}

/// What a layer's hooks made of a frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawn {
    pub list: DisplayList,
    /// What `draw` returned: `None` for nil, or seconds.
    pub again: Option<f64>,
    /// Keypad: one per cell, in the cells' order; `None` leaves the cell
    /// alone. Empty if `place` didn't run.
    pub places: Vec<Option<Placement>>,
    /// Seconds until something the layer drew is due to change by itself
    /// (an animation's next frame), whatever `draw` returned: the layer
    /// is drawn again at the sooner of the two. `None`: nothing is.
    pub due: Option<f64>,
}

/// Why a layer drew nothing.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// The hook failed (an error, the time limit, a cap): nothing is
    /// drawn this frame.
    Failed(ThemeError),
    /// The hook has been switched off (three failures, or one that ran
    /// four times its limit): nothing is drawn until the theme is
    /// reloaded.
    Off,
}

fn rect_table(lua: &Lua, r: Rect) -> mlua::Result<Table> {
    let t = lua.create_table_with_capacity(0, 4)?;
    t.raw_set("x", r[0])?;
    t.raw_set("y", r[1])?;
    t.raw_set("w", r[2])?;
    t.raw_set("h", r[3])?;
    t.set_readonly(true);
    Ok(t)
}

fn frozen(t: Table) -> Table {
    t.set_readonly(true);
    t
}

/// The table kept in `slot`, made the first time.
fn kept(
    slot: &mut Option<Table>,
    make: impl FnOnce() -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    if let Some(t) = slot {
        return Ok(t.clone());
    }
    let t = make()?;
    *slot = Some(t.clone());
    Ok(t)
}

struct PadTables {
    layout: u64,
    list: Table,
    cells: Vec<Table>,
    /// enabled, hovered, pressed, checked, as last written.
    states: Vec<[bool; 4]>,
}

struct ValueTables {
    version: u64,
    value: Table,
    change: Option<Table>,
}

/// The tables frames are told through, kept between frames.
pub(crate) struct Frames {
    frame: HostTable,
    pointer: Option<Table>,
    no_events: Option<Table>,
    pads: HashMap<u64, PadTables>,
    values: HashMap<u64, ValueTables>,
}

/// Keypads and readouts whose tables are kept at once.
const KEPT: usize = 16;

impl Frames {
    pub(crate) fn new(engine: &Engine) -> Result<Frames, ThemeError> {
        Ok(Frames {
            frame: engine.host_table(0, 12)?,
            pointer: None,
            no_events: None,
            pads: HashMap::new(),
            values: HashMap::new(),
        })
    }

    pub(crate) fn table(&self) -> &Table {
        self.frame.table()
    }

    /// The table of cell `i` of keypad `pad`, as last filled.
    pub(crate) fn cell(&self, pad: u64, i: usize) -> Option<&Table> {
        self.pads.get(&pad).and_then(|p| p.cells.get(i))
    }

    /// Refills the frame table for `frame`; `ctx` is the theme's filled
    /// `ctx` table.
    pub(crate) fn fill(
        &mut self,
        engine: &Engine,
        frame: &Frame,
        ctx: &Table,
    ) -> Result<(), ThemeError> {
        if self.pads.len() > KEPT {
            self.pads.clear();
        }
        if self.values.len() > KEPT {
            self.values.clear();
        }
        let lua = engine.lua();
        let Frames {
            frame: host,
            pointer,
            no_events,
            pads,
            values,
        } = self;
        engine.fill(host, |t| {
            t.raw_set("width", frame.width)?;
            t.raw_set("height", frame.height)?;
            t.raw_set("time", frame.time)?;
            t.raw_set("dt", frame.dt)?;
            t.raw_set("motion", frame.motion)?;
            t.raw_set("settled", frame.settled)?;
            t.raw_set("active", frame.active)?;
            t.raw_set("ctx", ctx)?;
            match frame.pointer {
                None => t.raw_set("pointer", Value::Nil)?,
                Some((x, y)) => {
                    let p = kept(pointer, || lua.create_table_with_capacity(0, 2))?;
                    p.set_readonly(false);
                    p.raw_set("x", x)?;
                    p.raw_set("y", y)?;
                    p.set_readonly(true);
                    t.raw_set("pointer", p)?;
                }
            }
            if frame.events.is_empty() {
                t.raw_set(
                    "events",
                    kept(no_events, || lua.create_table().map(frozen))?,
                )?;
            } else {
                let list = lua.create_table_with_capacity(frame.events.len(), 0)?;
                for (i, (age, event)) in frame.events.iter().enumerate() {
                    list.raw_seti(i + 1, event_table(lua, *age, event)?)?;
                }
                t.raw_set("events", frozen(list))?;
            }
            match &frame.keys {
                None => t.raw_set("keys", Value::Nil)?,
                Some(keys) => t.raw_set("keys", pad_tables(lua, pads, keys)?)?,
            }
            match &frame.value {
                None => t.raw_set("value", Value::Nil)?,
                Some(value) => t.raw_set("value", value_tables(lua, values, value)?)?,
            }
            Ok(())
        })
    }
}

fn event_table(lua: &Lua, age: f64, e: &Event) -> mlua::Result<Table> {
    let t = lua.create_table_with_capacity(0, 11)?;
    t.raw_set("kind", e.kind.name())?;
    t.raw_set("age", age)?;
    t.raw_set("x", e.x)?;
    t.raw_set("y", e.y)?;
    t.raw_set("rect", rect_table(lua, e.rect)?)?;
    t.raw_set("page", e.page.name())?;
    if let Some(source) = e.source {
        t.raw_set("source", source.name())?;
    }
    if let Some(via) = e.via {
        t.raw_set("via", via.name())?;
    }
    if let Some(action) = e.action {
        t.raw_set("action", action.name())?;
    }
    if let Some((id, kind)) = e.key {
        let key = lua.create_table_with_capacity(0, 2)?;
        if let Some(id) = id {
            key.raw_set("id", id)?;
        }
        key.raw_set("kind", kind.name())?;
        t.raw_set("key", frozen(key))?;
    }
    if let Some((id, value)) = &e.option {
        t.raw_set("id", id.as_str())?;
        match value {
            OptionValue::Text(s) => t.raw_set("value", s.as_str())?,
            OptionValue::Number(n) => t.raw_set("value", *n)?,
            OptionValue::Bool(b) => t.raw_set("value", *b)?,
        }
    }
    Ok(frozen(t))
}

const STATES: [&str; 4] = ["enabled", "hovered", "pressed", "checked"];

fn states(cell: &KeyCell) -> [bool; 4] {
    [cell.enabled, cell.hovered, cell.pressed, cell.checked]
}

/// The `frame.keys` list for a keypad: made anew when its layout
/// changed, else only the states that changed are written.
fn pad_tables(lua: &Lua, pads: &mut HashMap<u64, PadTables>, keys: &Keys) -> mlua::Result<Table> {
    if let Some(pad) = pads.get_mut(&keys.pad)
        && pad.layout == keys.layout
        && pad.cells.len() == keys.cells.len()
    {
        for (i, cell) in keys.cells.iter().enumerate() {
            let now = states(cell);
            if pad.states[i] != now {
                let t = &pad.cells[i];
                t.set_readonly(false);
                for (k, name) in STATES.iter().enumerate() {
                    if pad.states[i][k] != now[k] {
                        t.raw_set(*name, now[k])?;
                    }
                }
                t.set_readonly(true);
                pad.states[i] = now;
            }
        }
        return Ok(pad.list.clone());
    }
    let list = lua.create_table_with_capacity(keys.cells.len(), 0)?;
    let mut cells = Vec::with_capacity(keys.cells.len());
    for (i, cell) in keys.cells.iter().enumerate() {
        let t = lua.create_table_with_capacity(0, 10)?;
        if let Some(id) = cell.id {
            t.raw_set("id", id)?;
        }
        t.raw_set("kind", cell.kind.name())?;
        t.raw_set("rect", rect_table(lua, cell.rect)?)?;
        t.raw_set("row", cell.row)?;
        t.raw_set("col", cell.col)?;
        t.raw_set("radius", cell.radius)?;
        for (name, on) in STATES.iter().zip(states(cell)) {
            t.raw_set(*name, on)?;
        }
        t.set_readonly(true);
        list.raw_seti(i + 1, t.clone())?;
        cells.push(t);
    }
    list.set_readonly(true);
    pads.insert(
        keys.pad,
        PadTables {
            layout: keys.layout,
            list: list.clone(),
            cells,
            states: keys.cells.iter().map(states).collect(),
        },
    );
    Ok(list)
}

fn glyph_list(lua: &Lua, glyphs: &Glyphs, previous: bool) -> mlua::Result<Table> {
    let list = lua.create_table_with_capacity(glyphs.glyphs.len(), 0)?;
    for (i, g) in glyphs.glyphs.iter().enumerate() {
        let t = lua.create_table_with_capacity(0, 6)?;
        t.raw_set("index", i + 1)?;
        t.raw_set("x", g.x)?;
        t.raw_set("y", g.y)?;
        t.raw_set("w", g.w)?;
        t.raw_set("h", g.h)?;
        if previous {
            t.raw_set("previous", true)?;
        }
        list.raw_seti(i + 1, frozen(t))?;
    }
    Ok(frozen(list))
}

/// The `frame.value` table for a readout: made anew when its version
/// changed, else only the change's age is written.
fn value_tables(
    lua: &Lua,
    values: &mut HashMap<u64, ValueTables>,
    v: &DisplayValue,
) -> mlua::Result<Table> {
    if let Some(known) = values.get(&v.readout)
        && known.version == v.version
    {
        if let (Some(change), Some((_, age, _))) = (&known.change, v.change) {
            change.set_readonly(false);
            change.raw_set("age", age)?;
            change.set_readonly(true);
        }
        return Ok(known.value.clone());
    }
    let t = lua.create_table_with_capacity(0, 10)?;
    t.raw_set("text", v.text)?;
    t.raw_set("glyphs", glyph_list(lua, v.glyphs, false)?)?;
    t.raw_set("size", v.glyphs.size)?;
    t.raw_set("rect", rect_table(lua, v.glyphs.rect)?)?;
    t.raw_set("band", rect_table(lua, v.band)?)?;
    t.raw_set("error", v.error)?;
    t.raw_set("align", if v.align_end { "end" } else { "start" })?;
    t.raw_set("role", v.role.name())?;
    let change = match v.change {
        None => None,
        Some((kind, age, from)) => {
            let c = lua.create_table_with_capacity(0, 3)?;
            c.raw_set("kind", kind.name())?;
            c.raw_set("age", age)?;
            c.raw_set("from", from)?;
            c.set_readonly(true);
            t.raw_set("change", c.clone())?;
            if let Some(old) = v.previous {
                let p = lua.create_table_with_capacity(0, 3)?;
                p.raw_set("glyphs", glyph_list(lua, old, true)?)?;
                p.raw_set("size", old.size)?;
                p.raw_set("rect", rect_table(lua, old.rect)?)?;
                t.raw_set("previous", frozen(p))?;
            }
            Some(c)
        }
    };
    t.set_readonly(true);
    values.insert(
        v.readout,
        ValueTables {
            version: v.version,
            value: t.clone(),
            change,
        },
    );
    Ok(t)
}

/// What a preview is told: `{ width, height, ctx }`.
pub(crate) fn preview_table(
    engine: &Engine,
    host: &HostTable,
    width: f32,
    height: f32,
    ctx: &Table,
) -> Result<(), ThemeError> {
    engine.fill(host, |t| {
        t.raw_set("width", width)?;
        t.raw_set("height", height)?;
        t.raw_set("ctx", ctx)?;
        Ok(())
    })
}

/// What `place` returned for one cell.
pub(crate) fn placement(v: &crate::ThemeValue) -> Option<Placement> {
    let crate::ThemeValue::Map(m) = v else {
        return None;
    };
    let get = |key: &str, default: f32| {
        m.get(key)
            .and_then(crate::ThemeValue::as_f64)
            .map_or(default, |n| n as f32)
    };
    Some(Placement {
        dx: get("dx", 0.0).clamp(-10_000.0, 10_000.0),
        dy: get("dy", 0.0).clamp(-10_000.0, 10_000.0),
        scale: get("scale", 1.0).clamp(0.0, 100.0),
        opacity: get("opacity", 1.0).clamp(0.0, 1.0),
    })
}

/// The tile a `choice` option's `preview` draws: 96 × 56.
pub const PREVIEW_SIZE: (f32, f32) = (96.0, 56.0);
