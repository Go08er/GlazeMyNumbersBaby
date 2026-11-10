//! The `gmnb` library (docs/theming.md, "The `gmnb` library"): what a theme
//! script can call. Registered before the sandbox closes
//! ([`crate::Engine::with_globals`]), so the table and everything in it is
//! read-only to scripts.
//!
//! Colour maths is `appcore::color`, the same code both apps use, in
//! 32-bit floats. Functions take any Color (a hex string or an `Rgba`
//! table) and return `Rgba` tables.
//!
//! The drawing constructors (`gmnb.path`, `gmnb.svg_path`, `gmnb.image`,
//! `gmnb.noise`) belong to the draw API: [`register`] takes the function
//! that adds them, and [`placeholder_drawing`] adds stand-ins that only
//! let a theme load.

use appcore::color::{self, Rgb};
use appcore::palette::Palette;
use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::check::parse_hex;
use crate::types::{Rgba, TRANSPARENT};

/// The API version the app speaks.
pub const API: i64 = 1;

fn bad(what: &str, problem: impl std::fmt::Display) -> mlua::Error {
    mlua::Error::runtime(format!("gmnb.{what}: {problem}"))
}

fn finite(what: &str, n: f64) -> mlua::Result<f32> {
    if n.is_finite() {
        Ok(n as f32)
    } else {
        Err(bad(what, "a number that isn't finite"))
    }
}

fn unit(what: &str, n: f64) -> mlua::Result<f32> {
    Ok(finite(what, n)?.clamp(0.0, 1.0))
}

fn number(what: &str, v: &Value) -> mlua::Result<f32> {
    match v {
        Value::Integer(i) => Ok(*i as f32),
        Value::Number(n) => finite(what, *n),
        other => Err(bad(
            what,
            format!("expected a number, got {}", other.type_name()),
        )),
    }
}

/// A Color argument: a hex string, or `{ r, g, b, a }` (read raw: no
/// script code runs).
fn color_arg(what: &str, v: &Value) -> mlua::Result<Rgba> {
    match v {
        Value::String(s) => {
            let text = s.to_str()?;
            parse_hex(&text).ok_or_else(|| {
                bad(
                    what,
                    "a colour is \"#rrggbb\", \"#rrggbbaa\" or { r, g, b, a }",
                )
            })
        }
        Value::Table(t) => {
            let part = |key: &str, default: Option<f32>| -> mlua::Result<f32> {
                match t.raw_get::<Value>(key)? {
                    Value::Nil => {
                        default.ok_or_else(|| bad(what, format!("a colour table needs `{key}`")))
                    }
                    v => Ok(number(what, &v)?.clamp(0.0, 1.0)),
                }
            };
            Ok([
                part("r", None)?,
                part("g", None)?,
                part("b", None)?,
                part("a", Some(1.0))?,
            ])
        }
        other => Err(bad(
            what,
            format!("expected a colour, got {}", other.type_name()),
        )),
    }
}

fn rgb(c: Rgba) -> Rgb {
    [c[0], c[1], c[2]]
}

fn with_alpha(c: Rgb, a: f32) -> Rgba {
    [c[0], c[1], c[2], a]
}

fn color_table(lua: &Lua, c: Rgba) -> mlua::Result<Table> {
    let t = lua.create_table_with_capacity(0, 4)?;
    t.raw_set("r", c[0])?;
    t.raw_set("g", c[1])?;
    t.raw_set("b", c[2])?;
    t.raw_set("a", c[3])?;
    Ok(t)
}

fn color_list(what: &str, v: &Value, min: usize, max: usize) -> mlua::Result<Vec<Rgba>> {
    let Value::Table(t) = v else {
        return Err(bad(what, "expected a list of colours"));
    };
    let n = t.raw_len();
    if n < min || n > max {
        return Err(bad(what, format!("{n} colours; expected {min} to {max}")));
    }
    (1..=n)
        .map(|i| color_arg(what, &t.raw_get::<Value>(i)?))
        .collect()
}

/// The base colours as a table (`gmnb.base`, `ctx.base.colors`).
pub fn colors_table(lua: &Lua, colors: &crate::tokens::Colors) -> mlua::Result<Table> {
    let t = lua.create_table_with_capacity(0, 14)?;
    for (name, c) in colors.named() {
        t.raw_set(name, color_table(lua, c)?)?;
    }
    let series = lua.create_table_with_capacity(6, 0)?;
    for (i, c) in colors.series.iter().enumerate() {
        series.raw_seti(i + 1, color_table(lua, *c)?)?;
    }
    t.raw_set("series", series)?;
    Ok(t)
}

const PLACEMENT_KEYS: [&str; 6] = ["angle", "from", "to", "at", "center", "radius_y"];

/// `gmnb.linear`, `gmnb.radial`, `gmnb.conic`: a gradient table from a
/// spec, its colours as `Rgba` and its stops filled in. (What a style
/// does with it is checked again when the style is.)
fn gradient(lua: &Lua, kind: &'static str, spec: Value) -> mlua::Result<Table> {
    let Value::Table(spec) = spec else {
        return Err(bad(kind, "expected a table { colors = { … }, … }"));
    };
    let colors = color_list(kind, &spec.raw_get::<Value>("colors")?, 2, 16)?;
    let n = colors.len();
    let stops: Vec<f32> = match spec.raw_get::<Value>("stops")? {
        Value::Nil => (0..n).map(|i| i as f32 / (n - 1) as f32).collect(),
        Value::Table(given) => {
            if given.raw_len() != n {
                return Err(bad(kind, format!("expected {n} stops, one per colour")));
            }
            let mut stops: Vec<f32> = Vec::with_capacity(n);
            for i in 1..=n {
                let s = number(kind, &given.raw_get::<Value>(i)?)?.clamp(0.0, 1.0);
                if stops.last().is_some_and(|&last| s < last) {
                    return Err(bad(kind, "stops must never decrease"));
                }
                stops.push(s);
            }
            stops
        }
        _ => return Err(bad(kind, "`stops` is a list of numbers")),
    };
    let out = lua.create_table_with_capacity(0, 8)?;
    out.raw_set("kind", kind)?;
    let list = lua.create_table_with_capacity(n, 0)?;
    for (i, c) in colors.iter().enumerate() {
        list.raw_seti(i + 1, color_table(lua, *c)?)?;
    }
    out.raw_set("colors", list)?;
    out.raw_set("stops", stops)?;
    for key in PLACEMENT_KEYS {
        match spec.raw_get::<Value>(key)? {
            Value::Nil => {}
            Value::Integer(i) => out.raw_set(key, i)?,
            Value::Number(x) => out.raw_set(key, f64::from(finite(kind, x)?))?,
            Value::Table(point) => {
                if point.raw_len() != 2 {
                    return Err(bad(kind, format!("`{key}` is a point {{ x, y }}")));
                }
                let x = number(kind, &point.raw_get::<Value>(1)?)?;
                let y = number(kind, &point.raw_get::<Value>(2)?)?;
                out.raw_set(key, vec![x, y])?;
            }
            _ => return Err(bad(kind, format!("`{key}` is a number or a point"))),
        }
    }
    match spec.raw_get::<Value>("radius")? {
        Value::Nil => {}
        Value::Integer(i) => out.raw_set("radius", i)?,
        Value::Number(x) => out.raw_set("radius", f64::from(finite(kind, x)?))?,
        Value::String(s) => {
            let name = s.to_str()?;
            let known = crate::types::Extent::ALL.iter().find(|(n, _)| *n == &*name);
            match known {
                Some((n, _)) => out.raw_set("radius", *n)?,
                None => return Err(bad(kind, "`radius` is a number or a CSS extent keyword")),
            }
        }
        _ => return Err(bad(kind, "`radius` is a number or a CSS extent keyword")),
    }
    Ok(out)
}

/// The easing functions, in Luau: calling them costs a script no trip to
/// the host.
const EASE: &str = "
return {
    linear = function(p) return p end,
    in_quad = function(p) return p * p end,
    out_quad = function(p) return 1 - (1 - p) * (1 - p) end,
    in_out_quad = function(p)
        if p < 0.5 then return 2 * p * p end
        return 1 - (-2 * p + 2) ^ 2 / 2
    end,
    in_cubic = function(p) return p * p * p end,
    out_cubic = function(p) return 1 - (1 - p) ^ 3 end,
    in_out_cubic = function(p)
        if p < 0.5 then return 4 * p * p * p end
        return 1 - (-2 * p + 2) ^ 3 / 2
    end,
    out_quart = function(p) return 1 - (1 - p) ^ 4 end,
    out_back = function(p) return 1 + 2.70158 * (p - 1) ^ 3 + 1.70158 * (p - 1) ^ 2 end,
}";

/// Stand-ins for the draw API's constructors, so that a theme which makes
/// its paths and pictures as it loads does load: a path whose methods do
/// nothing, and pictures that are only a size.
pub fn placeholder_drawing(lua: &Lua, gmnb: &Table) -> mlua::Result<()> {
    let path = lua.create_function(|lua, _: MultiValue| {
        let p = lua.create_table()?;
        let chain = lua.create_function(|_, args: MultiValue| Ok(args.into_iter().next()))?;
        for name in [
            "move_to",
            "line_to",
            "quad_to",
            "curve_to",
            "close",
            "rect",
            "rounded_rect",
            "circle",
        ] {
            p.raw_set(name, chain.clone())?;
        }
        Ok(p)
    })?;
    gmnb.raw_set("path", path.clone())?;
    gmnb.raw_set("svg_path", path)?;
    let picture = |lua: &Lua, size: f64| -> mlua::Result<Table> {
        let t = lua.create_table()?;
        t.raw_set("width", size)?;
        t.raw_set("height", size)?;
        Ok(t)
    };
    gmnb.raw_set(
        "image",
        lua.create_function(move |lua, _: MultiValue| picture(lua, 0.0))?,
    )?;
    gmnb.raw_set(
        "noise",
        lua.create_function(move |lua, spec: Value| {
            let size = match &spec {
                Value::Table(t) => match t.raw_get::<Value>("size")? {
                    Value::Nil => 128.0,
                    v => number("noise", &v)?.clamp(1.0, 512.0),
                },
                _ => 128.0,
            };
            picture(lua, f64::from(size))
        })?,
    )?;
    Ok(())
}

/// Registers the `gmnb` global. `drawing` adds the draw API's
/// constructors to it.
pub fn register(
    lua: &Lua,
    drawing: impl FnOnce(&Lua, &Table) -> mlua::Result<()>,
) -> mlua::Result<()> {
    let g = lua.create_table()?;
    g.raw_set("api", API)?;
    g.raw_set("white", color_table(lua, [1.0, 1.0, 1.0, 1.0])?)?;
    g.raw_set("black", color_table(lua, [0.0, 0.0, 0.0, 1.0])?)?;
    g.raw_set("transparent", color_table(lua, TRANSPARENT)?)?;
    g.raw_set("TEXT_CONTRAST", color::TEXT_CONTRAST)?;
    g.raw_set("UI_CONTRAST", color::UI_CONTRAST)?;

    let f = |name: &'static str, f: Function| g.raw_set(name, f);
    f(
        "hex",
        lua.create_function(|lua, text: Value| match &text {
            Value::String(_) => color_table(lua, color_arg("hex", &text)?),
            _ => Err(bad("hex", "expected a string")),
        })?,
    )?;
    f(
        "color",
        lua.create_function(|lua, c: Value| color_table(lua, color_arg("color", &c)?))?,
    )?;
    f(
        "rgb",
        lua.create_function(|lua, (r, g, b, a): (f64, f64, f64, Option<f64>)| {
            let c = [
                unit("rgb", r)?,
                unit("rgb", g)?,
                unit("rgb", b)?,
                unit("rgb", a.unwrap_or(1.0))?,
            ];
            color_table(lua, c)
        })?,
    )?;
    f(
        "hsl",
        lua.create_function(|lua, (h, s, l, a): (f64, f64, f64, Option<f64>)| {
            let c = color::from_hsl(finite("hsl", h)?, unit("hsl", s)?, unit("hsl", l)?);
            color_table(lua, with_alpha(c, unit("hsl", a.unwrap_or(1.0))?))
        })?,
    )?;
    f(
        "to_hsl",
        lua.create_function(|_, c: Value| {
            let (h, s, l) = color::to_hsl(rgb(color_arg("to_hsl", &c)?));
            Ok((h, s, l))
        })?,
    )?;
    f(
        "to_hex",
        lua.create_function(|_, c: Value| Ok(color::to_hex(rgb(color_arg("to_hex", &c)?))))?,
    )?;
    f(
        "mix",
        lua.create_function(|lua, (a, b, t): (Value, Value, f64)| {
            let (a, b, t) = (
                color_arg("mix", &a)?,
                color_arg("mix", &b)?,
                finite("mix", t)?,
            );
            color_table(lua, [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t))
        })?,
    )?;
    f(
        "alpha",
        lua.create_function(|lua, (c, a): (Value, f64)| {
            let c = color_arg("alpha", &c)?;
            color_table(
                lua,
                with_alpha(rgb(c), (c[3] * finite("alpha", a)?).clamp(0.0, 1.0)),
            )
        })?,
    )?;
    f(
        "over",
        lua.create_function(|lua, (top, bottom): (Value, Value)| {
            let c = color::over(color_arg("over", &top)?, color_arg("over", &bottom)?);
            color_table(lua, c)
        })?,
    )?;
    f(
        "complement",
        lua.create_function(|lua, c: Value| {
            let c = color_arg("complement", &c)?;
            color_table(lua, with_alpha(color::complement(rgb(c)), c[3]))
        })?,
    )?;
    f(
        "luminance",
        lua.create_function(|_, c: Value| Ok(color::luminance(rgb(color_arg("luminance", &c)?))))?,
    )?;
    f(
        "contrast",
        lua.create_function(|_, (a, b): (Value, Value)| {
            Ok(color::contrast(
                rgb(color_arg("contrast", &a)?),
                rgb(color_arg("contrast", &b)?),
            ))
        })?,
    )?;
    f(
        "worst_contrast",
        lua.create_function(|_, (c, against): (Value, Value)| {
            let c = rgb(color_arg("worst_contrast", &c)?);
            let list: Vec<Rgb> = color_list("worst_contrast", &against, 1, 64)?
                .into_iter()
                .map(rgb)
                .collect();
            Ok(color::worst_contrast(c, &list))
        })?,
    )?;
    f(
        "with_contrast",
        lua.create_function(|lua, (c, against, target): (Value, Value, f64)| {
            let c = color_arg("with_contrast", &c)?;
            let against = rgb(color_arg("with_contrast", &against)?);
            let target = finite("with_contrast", target)?.clamp(1.0, 21.0);
            color_table(
                lua,
                with_alpha(color::with_contrast(rgb(c), against, target), c[3]),
            )
        })?,
    )?;
    f(
        "gradient_for_text",
        lua.create_function(|lua, (text, a, b, target): (Value, Value, Value, f64)| {
            let what = "gradient_for_text";
            let text = rgb(color_arg(what, &text)?);
            let (a, b) = (color_arg(what, &a)?, color_arg(what, &b)?);
            let target = finite(what, target)?.clamp(1.0, 21.0);
            let (a2, b2) = color::gradient_for_text(text, rgb(a), rgb(b), target);
            Ok((
                color_table(lua, with_alpha(a2, a[3]))?,
                color_table(lua, with_alpha(b2, b[3]))?,
            ))
        })?,
    )?;
    f(
        "readable_gradient",
        lua.create_function(
            |lua, (a, b, light, dark, target): (Value, Value, Value, Value, f64)| {
                let what = "readable_gradient";
                let (a, b) = (color_arg(what, &a)?, color_arg(what, &b)?);
                let (light, dark) = (color_arg(what, &light)?, color_arg(what, &dark)?);
                let target = finite(what, target)?.clamp(1.0, 21.0);
                let (text, a2, b2) =
                    color::readable_gradient(rgb(a), rgb(b), rgb(light), rgb(dark), target);
                Ok((
                    color_table(lua, with_alpha(text, 1.0))?,
                    color_table(lua, with_alpha(a2, a[3]))?,
                    color_table(lua, with_alpha(b2, b[3]))?,
                ))
            },
        )?,
    )?;
    f(
        "base",
        lua.create_function(|lua, (dark, accent): (bool, Value)| {
            let accent = match &accent {
                Value::Nil => None,
                c => Some(rgb(color_arg("base", c)?)),
            };
            let colors = crate::tokens::Colors::from(&Palette::new(dark, accent));
            colors_table(lua, &colors)
        })?,
    )?;
    for kind in ["linear", "radial", "conic"] {
        f(
            kind,
            lua.create_function(move |lua, spec: Value| gradient(lua, kind, spec))?,
        )?;
    }
    g.raw_set(
        "ease",
        lua.load(EASE)
            .set_name("=gmnb.ease")
            .set_mode(mlua::chunk::ChunkMode::Text)
            .eval::<Table>()?,
    )?;
    // `gmnb.warn` goes where `print` does: the theme's log.
    let print: Function = lua.globals().raw_get("print")?;
    g.raw_set("warn", print)?;
    // `gmnb.synth`, `gmnb.sound`, `gmnb.note`.
    crate::sound::register(lua, &g)?;
    drawing(lua, &g)?;
    lua.globals().raw_set("gmnb", g)?;
    Ok(())
}
