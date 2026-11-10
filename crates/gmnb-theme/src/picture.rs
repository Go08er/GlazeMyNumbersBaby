//! Pictures (`gmnb.image()`, `gmnb.noise()`): what `draw:image` and
//! `draw:tile` take.
//!
//! To a script a picture is a small read-only table: its size, and the
//! number the app knows it by. Asking for the same picture again gives
//! the same one. Nothing is decoded or generated here until the app
//! wants the pixels ([`Pictures::pixels`]): making a picture costs a
//! theme's load only a look at the file's header.
//!
//! A picture that can't be had (not there, too big, one too many) is an
//! empty one, of no size, which draws nothing; the theme's author is told
//! in a note.

use std::rc::Rc;

use mlua::{Lua, Table, Value};

use crate::check::parse_hex;
use crate::draw::{DrawLimits, Shared};
use crate::theme::Source;
use crate::types::Rgba;

/// Which of a theme's pictures: its place in the theme's [`Pictures`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageId(pub u32);

/// What `gmnb.noise` was asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct NoiseSpec {
    /// The square's side, in pixels.
    pub size: u32,
    pub seed: u32,
    pub color: Rgba,
    pub opacity: f32,
}

/// The seed `gmnb.noise` uses when given none.
pub const NOISE_SEED: u32 = 0x9e37_79b9;

impl NoiseSpec {
    /// The picture: `size` × `size` RGBA8, premultiplied, row by row. Per
    /// pixel a 32-bit xorshift (`s ^= s << 13; s ^= s >> 17; s ^= s << 5`,
    /// from `seed`) gives `n = (s & 0xff) / 255`; the pixel is `color` at
    /// alpha `n × opacity × color's own`, each part cut down to 8 bits.
    /// This recipe is the contract's and never changes.
    pub fn pixels(&self) -> Vec<u8> {
        let count = self.size as usize * self.size as usize;
        let mut out = Vec::with_capacity(count * 4);
        let mut s = self.seed;
        let [r, g, b, a0] = self.color;
        for _ in 0..count {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let n = (s & 0xff) as f32 / 255.0;
            let a = n * self.opacity * a0;
            out.extend_from_slice(&[
                (r * a * 255.0) as u8,
                (g * a * 255.0) as u8,
                (b * a * 255.0) as u8,
                (a * 255.0) as u8,
            ]);
        }
        out
    }
}

/// Where a picture's pixels come from.
///
/// More kinds will come (moving pictures, pictures the app fills in), so
/// a reader must have an arm for the ones it doesn't know.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum PictureKind {
    /// A PNG or JPEG file of the theme's folder, by its path in it.
    File(String),
    Noise(NoiseSpec),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub kind: PictureKind,
    /// In pixels.
    pub width: u32,
    pub height: u32,
}

/// A picture's pixels, for the app to make a texture of.
#[non_exhaustive]
pub enum Pixels {
    /// A PNG or JPEG file's bytes, to decode.
    Encoded(Vec<u8>),
    /// RGBA8, premultiplied, `width × 4` bytes a row.
    Rgba8 {
        width: u32,
        height: u32,
        bytes: Vec<u8>,
    },
}

/// One theme's pictures.
pub struct Pictures {
    source: Source,
    limits: DrawLimits,
    list: Vec<Picture>,
    /// What each was asked for by, to give the same one again.
    asked: Vec<(String, ImageId)>,
    /// Requests that failed: asked again, they fail without another look.
    failed: Vec<String>,
    /// Image files tried, good or bad: each has used one of the count.
    tried: usize,
    /// Bytes all the image files take decoded.
    decoded: u64,
    noises: usize,
    notes: Vec<String>,
}

/// The size of a PNG or JPEG, from its header.
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.starts_with(PNG) {
        // The IHDR chunk comes first: length, "IHDR", width, height.
        if bytes.len() < 24 || &bytes[12..16] != b"IHDR" {
            return None;
        }
        let be = |at: usize| {
            u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        return Some((be(16), be(20)));
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        // Walk the segments to the frame header (SOF0..SOF15, but the
        // tables DHT, JPG and DAC that share the range).
        let mut at = 2;
        while at + 4 <= bytes.len() {
            if bytes[at] != 0xff {
                return None;
            }
            let marker = bytes[at + 1];
            if marker == 0xff {
                at += 1;
                continue;
            }
            if (0xd0..=0xd9).contains(&marker) || marker == 0x01 {
                at += 2;
                continue;
            }
            let len = usize::from(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
            if len < 2 {
                return None;
            }
            let frame = (0xc0..=0xcf).contains(&marker) && ![0xc4, 0xc8, 0xcc].contains(&marker);
            if frame {
                if at + 9 > bytes.len() {
                    return None;
                }
                let h = u16::from_be_bytes([bytes[at + 5], bytes[at + 6]]);
                let w = u16::from_be_bytes([bytes[at + 7], bytes[at + 8]]);
                return Some((u32::from(w), u32::from(h)));
            }
            at += 2 + len;
        }
    }
    None
}

impl Pictures {
    pub(crate) fn new(source: Source, limits: DrawLimits) -> Pictures {
        Pictures {
            source,
            limits,
            list: Vec::new(),
            asked: Vec::new(),
            failed: Vec::new(),
            tried: 0,
            decoded: 0,
            noises: 0,
            notes: Vec::new(),
        }
    }

    /// How many pictures the theme has.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn get(&self, id: ImageId) -> Option<&Picture> {
        self.list.get(id.0 as usize)
    }

    /// The picture's pixels: an image file's bytes, read now, or the
    /// noise, generated now. `None` if the file has gone.
    pub fn pixels(&self, id: ImageId) -> Option<Pixels> {
        let picture = self.get(id)?;
        match &picture.kind {
            PictureKind::File(path) => self
                .source
                .read(path, self.limits.image_file)
                .ok()
                .map(Pixels::Encoded),
            PictureKind::Noise(spec) => Some(Pixels::Rgba8 {
                width: spec.size,
                height: spec.size,
                bytes: spec.pixels(),
            }),
        }
    }

    pub(crate) fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    fn known(&self, key: &str) -> Option<Option<ImageId>> {
        if let Some((_, id)) = self.asked.iter().find(|(k, _)| k == key) {
            return Some(Some(*id));
        }
        self.failed.iter().any(|k| k == key).then_some(None)
    }

    fn refuse(&mut self, key: String, note: String) -> Option<ImageId> {
        if self.failed.len() < 64 {
            self.failed.push(key);
        }
        if self.notes.len() < 32 {
            self.notes.push(note);
        }
        None
    }

    fn add(&mut self, key: String, picture: Picture) -> Option<ImageId> {
        let id = ImageId(self.list.len() as u32);
        self.list.push(picture);
        self.asked.push((key, id));
        Some(id)
    }

    /// `gmnb.image(file)`.
    fn image(&mut self, file: &str) -> Option<ImageId> {
        let key = format!("image:{file}");
        if let Some(known) = self.known(&key) {
            return known;
        }
        let limits = self.limits.clone();
        // A file that fails has used one of the count too: however a
        // theme's hooks are written, the app tries at most that many.
        if self.tried >= limits.images {
            let note = format!(
                "gmnb.image: {file}: a theme may load {} images; left out",
                limits.images
            );
            return self.refuse(key, note);
        }
        self.tried += 1;
        let bytes = match self.source.read(file, limits.image_file) {
            Ok(bytes) => bytes,
            Err(e) => return self.refuse(key, format!("gmnb.image: {e}")),
        };
        let Some((width, height)) = image_size(&bytes) else {
            let note = format!("gmnb.image: {file} is not a PNG or JPEG picture");
            return self.refuse(key, note);
        };
        if width == 0 || height == 0 || width > limits.image_side || height > limits.image_side {
            let note = format!(
                "gmnb.image: {file} is {width}×{height}; a picture is at most {0}×{0}",
                limits.image_side
            );
            return self.refuse(key, note);
        }
        let decoded = u64::from(width) * u64::from(height) * 4;
        if self.decoded + decoded > limits.image_bytes {
            let note = format!(
                "gmnb.image: {file}: the theme's pictures are over {} MiB decoded; left out",
                limits.image_bytes >> 20
            );
            return self.refuse(key, note);
        }
        self.decoded += decoded;
        self.add(
            key,
            Picture {
                kind: PictureKind::File(file.to_string()),
                width,
                height,
            },
        )
    }

    /// `gmnb.noise(spec)`.
    fn noise(&mut self, spec: NoiseSpec) -> Option<ImageId> {
        let key = format!(
            "noise:{}:{}:{:?}:{}",
            spec.size, spec.seed, spec.color, spec.opacity
        );
        if let Some(known) = self.known(&key) {
            return known;
        }
        if self.noises >= self.limits.noises {
            let note = format!(
                "gmnb.noise: a theme may make {} noise pictures; left out",
                self.limits.noises
            );
            return self.refuse(key, note);
        }
        self.noises += 1;
        let size = spec.size;
        self.add(
            key,
            Picture {
                kind: PictureKind::Noise(spec),
                width: size,
                height: size,
            },
        )
    }
}

fn bad(what: &str, problem: impl std::fmt::Display) -> mlua::Error {
    mlua::Error::runtime(format!("gmnb.{what}: {problem}"))
}

fn number(what: &str, v: &Value) -> mlua::Result<f64> {
    match v {
        Value::Integer(i) => Ok(*i as f64),
        Value::Number(n) if n.is_finite() => Ok(*n),
        _ => Err(bad(what, "expected a number")),
    }
}

/// Adds `gmnb.image` and `gmnb.noise`. Each picture's table is made once
/// and kept in `tables`, a table of the registry's, by its number.
pub(crate) fn register(lua: &Lua, gmnb: &Table, shared: &Rc<Shared>) -> mlua::Result<()> {
    // The pictures' tables, by id + 1; and the empty picture at 0.
    let tables = lua.create_table()?;
    let empty = lua.create_table_with_capacity(0, 2)?;
    empty.raw_set("width", 0)?;
    empty.raw_set("height", 0)?;
    empty.set_readonly(true);
    tables.raw_seti(0, empty)?;
    let tables = lua.create_registry_value(tables)?;
    let tables = Rc::new(tables);

    let handle = |lua: &Lua, tables: &mlua::RegistryKey, shared: &Shared, id: Option<ImageId>| {
        let tables: Table = lua.registry_value(tables)?;
        let Some(id) = id else {
            return tables.raw_get::<Table>(0);
        };
        let at = id.0 as usize + 1;
        if let Value::Table(t) = tables.raw_get::<Value>(at)? {
            return Ok(t);
        }
        let pictures = shared.pictures.borrow();
        let (w, h) = pictures.get(id).map_or((0, 0), |p| (p.width, p.height));
        let t = lua.create_table_with_capacity(0, 3)?;
        t.raw_set("width", w)?;
        t.raw_set("height", h)?;
        t.raw_set("id", at)?;
        t.set_readonly(true);
        tables.raw_seti(at, t.clone())?;
        Ok(t)
    };

    let (s, tb) = (shared.clone(), tables.clone());
    gmnb.raw_set(
        "image",
        lua.create_function(move |lua, file: Value| {
            let Value::String(file) = file else {
                return Err(bad("image", "expected a file's path in the theme's folder"));
            };
            let file = file.to_str()?;
            let id = s.pictures.borrow_mut().image(&file);
            handle(lua, &tb, &s, id)
        })?,
    )?;
    let (s, tb) = (shared.clone(), tables);
    gmnb.raw_set(
        "noise",
        lua.create_function(move |lua, spec: Value| {
            let what = "noise";
            let spec = match spec {
                Value::Table(t) => t,
                Value::Nil => lua.create_table()?,
                _ => return Err(bad(what, "expected { size, seed, color, opacity }")),
            };
            let side = s.pictures.borrow().limits.noise_side;
            let size = match spec.raw_get::<Value>("size")? {
                Value::Nil => 128,
                v => {
                    let n = number(what, &v)?;
                    if n.fract() != 0.0 || n < 1.0 {
                        return Err(bad(what, "`size` is a whole number of pixels, 1 or more"));
                    }
                    if n > f64::from(side) {
                        let mut pictures = s.pictures.borrow_mut();
                        let note = format!(
                            "gmnb.noise: a noise picture is at most {side}×{side}; {n} was asked for"
                        );
                        if pictures.notes.len() < 32 && !pictures.notes.contains(&note) {
                            pictures.notes.push(note);
                        }
                    }
                    n.min(f64::from(side)) as u32
                }
            };
            let seed = match spec.raw_get::<Value>("seed")? {
                Value::Nil => NOISE_SEED,
                v => {
                    let n = number(what, &v)?;
                    if n.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&n) {
                        return Err(bad(what, "`seed` is a whole number from 0 to 4294967295"));
                    }
                    n as u32
                }
            };
            let color = match spec.raw_get::<Value>("color")? {
                Value::Nil => [1.0, 1.0, 1.0, 1.0],
                Value::String(text) => parse_hex(&text.to_str()?)
                    .ok_or_else(|| bad(what, "`color` is \"#rrggbb\" or { r, g, b, a }"))?,
                Value::Table(c) => {
                    let part = |key: &str, default: Option<f64>| -> mlua::Result<f32> {
                        match (c.raw_get::<Value>(key)?, default) {
                            (Value::Nil, Some(d)) => Ok(d as f32),
                            (v, _) => Ok(number(what, &v)?.clamp(0.0, 1.0) as f32),
                        }
                    };
                    [
                        part("r", None)?,
                        part("g", None)?,
                        part("b", None)?,
                        part("a", Some(1.0))?,
                    ]
                }
                _ => return Err(bad(what, "`color` is \"#rrggbb\" or { r, g, b, a }")),
            };
            let opacity = match spec.raw_get::<Value>("opacity")? {
                Value::Nil => 1.0,
                v => number(what, &v)?.clamp(0.0, 1.0) as f32,
            };
            let id = s.pictures.borrow_mut().noise(NoiseSpec {
                size,
                seed,
                color,
                opacity,
            });
            handle(lua, &tb, &s, id)
        })?,
    )?;
    Ok(())
}
