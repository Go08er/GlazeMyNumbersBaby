//! Pictures: what `draw:image` and `draw:tile` take.
//!
//! | Made by | What |
//! |---|---|
//! | `gmnb.image(file)` | A still: a PNG, JPEG, WebP or GIF file of the theme's folder |
//! | `gmnb.noise(spec)` | A square of speckles, by a fixed recipe |
//! | `gmnb.animation(file)` | A GIF, an animated PNG or an animated WebP |
//! | `gmnb.pixels(w, h)` | A picture the theme fills from a Luau `buffer` (`:set`) |
//!
//! To a script a picture is a small read-only table: its size, and the
//! number the app knows it by. Asking for the same picture again gives
//! the same one.
//!
//! **Nothing is decoded while a theme's code runs.** Making a picture
//! reads its file and looks at its header (docs/theming-media.md,
//! "Decoding stills"): what kind it is, by its first bytes, and how big.
//! An animation's frames are counted, none decoded. The pixels are made
//! when the app first draws the picture ([`Pictures::still`],
//! [`Pictures::animation`]), after the hook that drew it has returned:
//! decoding is never inside a call's time limit, and a theme that is only
//! listed, or loads pictures it doesn't draw, decodes nothing.
//!
//! A picture that can't be had (not there, not a picture, too big, one
//! too many) is an empty one, of no size, which draws nothing; the
//! theme's author is told in a note. A file whose header is sound and
//! whose pixels turn out not to be keeps its size and draws nothing,
//! with a note when that is found. Either way the file has used one of
//! the count, and is not tried again.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gmnb_media::{AnimLimits, Bucket, Bytes, Rgba as Decoded, Timeline};
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
/// More kinds may come, so a reader must have an arm for the ones it
/// doesn't know.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum PictureKind {
    /// A PNG, JPEG, WebP or GIF file of the theme's folder, by its path
    /// in it: a still (of a file that moves, its first picture).
    File(String),
    Noise(NoiseSpec),
    /// A GIF, animated PNG or animated WebP file of the theme's folder,
    /// and when each of its frames starts.
    Animation(String, Arc<Timeline>),
    /// A picture the theme fills from a buffer ([`Pictures::canvas`]).
    Canvas,
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
    /// A file's bytes, to decode. (No longer given: a still comes
    /// decoded.)
    Encoded(Vec<u8>),
    /// RGBA8, premultiplied, `width × 4` bytes a row.
    Rgba8 {
        width: u32,
        height: u32,
        bytes: Vec<u8>,
    },
}

/// A picture the theme fills (`gmnb.pixels`), as it stands.
#[derive(Clone, Debug, PartialEq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// RGBA8, **not** premultiplied, `width × 4` bytes a row.
    pub pixels: Bytes,
    /// Goes up each time the theme changes the picture: a texture made
    /// of an earlier version is out of date.
    pub version: u64,
}

struct CanvasState {
    canvas: Canvas,
    /// How often it may change: 30 times a second, two in hand.
    allowance: Bucket,
}

/// How an animation's own clock stands (docs/theming-media.md, "plays
/// slower"): how far behind the layer's clock it has fallen.
#[derive(Clone, Copy, Debug, Default)]
struct Playhead {
    /// Seconds behind the clock of the layer that plays it.
    lag: f64,
    /// That clock when the animation was last drawn on it.
    last: Option<f64>,
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
    /// Animation files tried, good or bad.
    animations_tried: usize,
    /// Bytes all the theme's pictures are counted as, decoded.
    decoded: u64,
    noises: usize,
    canvases: HashMap<ImageId, CanvasState>,
    playheads: HashMap<ImageId, Playhead>,
    /// Pictures whose pixels turned out not to be there: not tried again.
    spoiled: Vec<ImageId>,
    notes: Vec<String>,
}

/// The size of a PNG, JPEG, WebP or GIF picture, from its header.
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    gmnb_media::probe(bytes)
        .ok()
        .map(|info| (info.width, info.height))
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
            animations_tried: 0,
            decoded: 0,
            noises: 0,
            canvases: HashMap::new(),
            playheads: HashMap::new(),
            spoiled: Vec::new(),
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

    /// Bytes the theme's pictures are counted as against "Images: total
    /// decoded".
    pub fn decoded_bytes(&self) -> u64 {
        self.decoded
    }

    /// A still's pixels, decoded now: an image file's (read again, and
    /// decoded by the app's own decoders), or the noise, generated now.
    /// `None`: not a still, or its file has gone or doesn't decode. (For
    /// the app, [`Pictures::still`] also notes why and doesn't try
    /// twice.)
    pub fn pixels(&self, id: ImageId) -> Option<Pixels> {
        let decoded = self.decode(id)?.ok()?;
        Some(Pixels::Rgba8 {
            width: decoded.width,
            height: decoded.height,
            bytes: decoded.pixels,
        })
    }

    fn decode(&self, id: ImageId) -> Option<Result<Decoded, String>> {
        let picture = self.get(id)?;
        match &picture.kind {
            PictureKind::File(path) => Some(
                self.source
                    .read(path, self.limits.image_file)
                    .and_then(|bytes| {
                        gmnb_media::decode_still(&Bytes::new(bytes), self.limits.image_side)
                            .map_err(|e| format!("{path} {e}"))
                    })
                    // The size the theme was told is the size it gets.
                    .and_then(|decoded| {
                        if (decoded.width, decoded.height) == (picture.width, picture.height) {
                            Ok(decoded)
                        } else {
                            Err(format!("{path} changed while the theme was loaded"))
                        }
                    }),
            ),
            PictureKind::Noise(spec) => Some(Ok(Decoded {
                width: spec.size,
                height: spec.size,
                pixels: spec.pixels(),
            })),
            _ => None,
        }
    }

    /// A still's pixels for the app's texture: RGBA8, premultiplied.
    /// Decoded now, on the caller's thread, which is never inside one of
    /// the theme's calls. A file that doesn't decode is noted and not
    /// tried again: the picture keeps its size and draws nothing.
    pub fn still(&mut self, id: ImageId) -> Option<Decoded> {
        if self.spoiled.contains(&id) {
            return None;
        }
        match self.decode(id)? {
            Ok(decoded) => Some(decoded),
            Err(why) => {
                self.spoil(id, format!("gmnb.image: {why}; it draws nothing"));
                None
            }
        }
    }

    /// An animation's file and timeline, for the app's player. The file
    /// is read again now. `None`: not an animation, or its file has gone
    /// (noted; not tried again).
    pub fn animation(&mut self, id: ImageId) -> Option<(Bytes, Arc<Timeline>)> {
        if self.spoiled.contains(&id) {
            return None;
        }
        let PictureKind::Animation(path, timeline) = &self.get(id)?.kind else {
            return None;
        };
        let (path, timeline) = (path.clone(), timeline.clone());
        match self.source.read(&path, self.limits.animation_file) {
            Ok(bytes) => Some((Bytes::new(bytes), timeline)),
            Err(e) => {
                self.spoil(id, format!("gmnb.animation: {e}; it draws nothing"));
                None
            }
        }
    }

    /// The app found that a picture's pixels can't be had after all (an
    /// animation whose first frame doesn't decode): noted once, and not
    /// asked for again.
    pub fn spoil(&mut self, id: ImageId, note: String) {
        if !self.spoiled.contains(&id) {
            self.spoiled.push(id);
            self.note(note);
        }
    }

    /// A note from the app about the theme's pictures (an animation that
    /// stopped, animations too heavy to play at full speed).
    pub fn note(&mut self, note: String) {
        if self.notes.len() < 32 && !self.notes.contains(&note) {
            self.notes.push(note);
        }
    }

    /// A picture the theme fills, as it stands.
    pub fn canvas(&self, id: ImageId) -> Option<&Canvas> {
        self.canvases.get(&id).map(|c| &c.canvas)
    }

    pub(crate) fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// Where an animation played on a layer's clock is, at `time` on
    /// that clock: `time` itself, less what it has fallen behind while
    /// the app could not decode it at full speed (`slow` times slower;
    /// 1: it keeps up, and the position is the clock's exactly).
    pub(crate) fn position(&mut self, id: ImageId, time: f64, slow: f64) -> f64 {
        let head = self.playheads.entry(id).or_default();
        if slow > 1.0
            && let Some(last) = head.last
        {
            // Another layer's clock, or a clock put back, moves nothing.
            let passed = (time - last).clamp(0.0, crate::sched::MAX_DT);
            head.lag += passed * (1.0 - 1.0 / slow);
        }
        head.last = Some(time);
        if head.lag == 0.0 {
            time
        } else {
            time - head.lag
        }
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

    /// Counts `bytes` against the theme's image total, if there is room.
    fn charge(&mut self, bytes: u64) -> bool {
        if self.decoded + bytes > self.limits.image_bytes {
            return false;
        }
        self.decoded += bytes;
        true
    }

    fn over_total(&self, what: &str, file: &str) -> String {
        format!(
            "gmnb.{what}: {file}: the theme's pictures are over {} MiB decoded; left out",
            self.limits.image_bytes >> 20
        )
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
        // What it is, by its first bytes, and how big, by its header.
        let info = match gmnb_media::probe(&bytes) {
            Ok(info) => info,
            Err(gmnb_media::Error::Unknown) => {
                let note = format!(
                    "gmnb.image: {file} is not a PNG, JPEG, WebP or GIF picture (by its first bytes)"
                );
                return self.refuse(key, note);
            }
            Err(e) => return self.refuse(key, format!("gmnb.image: {file} {e}")),
        };
        let (width, height) = (info.width, info.height);
        if width == 0 || height == 0 || width > limits.image_side || height > limits.image_side {
            let note = format!(
                "gmnb.image: {file} is {width}×{height}; a picture is at most {0}×{0}",
                limits.image_side
            );
            return self.refuse(key, note);
        }
        if !self.charge(u64::from(width) * u64::from(height) * 4) {
            let note = self.over_total("image", file);
            return self.refuse(key, note);
        }
        self.add(
            key,
            Picture {
                kind: PictureKind::File(file.to_string()),
                width,
                height,
            },
        )
    }

    /// `gmnb.animation(file)`.
    fn animation_file(&mut self, file: &str) -> Option<ImageId> {
        let key = format!("animation:{file}");
        if let Some(known) = self.known(&key) {
            return known;
        }
        let limits = self.limits.clone();
        if self.animations_tried >= limits.animations {
            let note = format!(
                "gmnb.animation: {file}: a theme may load {} animations; left out",
                limits.animations
            );
            return self.refuse(key, note);
        }
        self.animations_tried += 1;
        let bytes = match self.source.read(file, limits.animation_file) {
            Ok(bytes) => Bytes::new(bytes),
            Err(e) => return self.refuse(key, format!("gmnb.animation: {e}")),
        };
        // Its frames are counted and their delays read; none is decoded.
        let caps = AnimLimits {
            side: limits.animation_side,
            frames: limits.animation_frames,
        };
        let timeline = match gmnb_media::open(&bytes, &caps) {
            Ok(timeline) => timeline,
            Err(gmnb_media::Error::Size { width, height, max }) => {
                let note = format!(
                    "gmnb.animation: {file} is {width}×{height}; an animation is at most {max}×{max}"
                );
                return self.refuse(key, note);
            }
            Err(gmnb_media::Error::Frames { max }) => {
                let note = format!("gmnb.animation: {file} has more than {max} frames; left out");
                return self.refuse(key, note);
            }
            Err(gmnb_media::Error::Unknown) => {
                let note = format!(
                    "gmnb.animation: {file} is not a GIF, PNG or WebP file (by its first bytes)"
                );
                return self.refuse(key, note);
            }
            Err(e @ gmnb_media::Error::NotAnimation(_)) => {
                return self.refuse(key, format!("gmnb.animation: {file} is {e}"));
            }
            Err(e) => return self.refuse(key, format!("gmnb.animation: {file} {e}")),
        };
        if !self.charge(timeline.charge()) {
            let note = self.over_total("animation", file);
            return self.refuse(key, note);
        }
        let (width, height) = (timeline.width, timeline.height);
        self.add(
            key,
            Picture {
                kind: PictureKind::Animation(file.to_string(), Arc::new(timeline)),
                width,
                height,
            },
        )
    }

    /// `gmnb.pixels(width, height)`.
    fn pixels_picture(&mut self, width: u32, height: u32) -> Option<ImageId> {
        let key = format!("pixels:{width}x{height}");
        if let Some(known) = self.known(&key) {
            return known;
        }
        let side = self.limits.canvas_side;
        if width > side || height > side {
            let note = format!(
                "gmnb.pixels: {width}×{height}; a picture the theme fills is at most {side}×{side}"
            );
            return self.refuse(key, note);
        }
        if self.canvases.len() >= self.limits.canvases {
            let note = format!(
                "gmnb.pixels: a theme may make {} pictures to fill; left out",
                self.limits.canvases
            );
            return self.refuse(key, note);
        }
        let bytes = width as usize * height as usize * 4;
        if !self.charge(bytes as u64) {
            let note = self.over_total("pixels", &format!("{width}×{height}"));
            return self.refuse(key, note);
        }
        let id = self.add(
            key,
            Picture {
                kind: PictureKind::Canvas,
                width,
                height,
            },
        )?;
        self.canvases.insert(
            id,
            CanvasState {
                canvas: Canvas {
                    width,
                    height,
                    // It starts transparent.
                    pixels: Bytes::new(vec![0; bytes]),
                    version: 0,
                },
                allowance: Bucket::new(),
            },
        );
        Some(id)
    }

    /// `canvas:set(buffer)` at `now`: the picture takes `bytes`, if they
    /// are as many as it has and it wasn't changed too recently.
    fn set_canvas(&mut self, id: ImageId, bytes: Vec<u8>, now: f64) -> Result<bool, String> {
        let Some(state) = self.canvases.get_mut(&id) else {
            return Ok(false);
        };
        let c = &mut state.canvas;
        let want = c.width as usize * c.height as usize * 4;
        if bytes.len() != want {
            return Err(format!(
                "gmnb.pixels: set was given a buffer of {} bytes; a {}×{} picture takes {want}",
                bytes.len(),
                c.width,
                c.height
            ));
        }
        if !state.allowance.take(now) {
            return Ok(false);
        }
        c.pixels = Bytes::new(bytes);
        c.version += 1;
        Ok(true)
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

/// Adds `gmnb.image`, `gmnb.noise`, `gmnb.animation` and `gmnb.pixels`.
/// Each picture's table is made once and kept in `tables`, a table of the
/// registry's, by its number.
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

    // `canvas:set(buffer)`: one function for every picture a theme
    // fills; which one, it reads off the table it is called on.
    let s = shared.clone();
    let set = lua.create_function(move |_, (canvas, data): (Value, Value)| {
        let what = "pixels";
        let id = match &canvas {
            Value::Table(t) => t.raw_get::<Value>("id")?,
            _ => Value::Nil,
        };
        let Value::Buffer(data) = data else {
            return Err(bad(what, "set takes a buffer of the picture's pixels"));
        };
        let id = match id {
            // An empty picture (over a limit) takes nothing.
            Value::Nil => return Ok(false),
            v => number(what, &v)?,
        };
        if id.fract() != 0.0 || id < 1.0 || id > f64::from(u32::MAX) {
            return Err(bad(
                what,
                "set was called on something that isn't a picture",
            ));
        }
        let now = s.now();
        let done = s
            .pictures
            .borrow_mut()
            .set_canvas(ImageId(id as u32 - 1), data.to_vec(), now);
        match done {
            Ok(done) => Ok(done),
            // The wrong size: the call does nothing, and says so once.
            Err(note) => {
                s.pictures.borrow_mut().note(note);
                Ok(false)
            }
        }
    })?;
    // What `gmnb.pixels` gives when it can't give a picture: no size, and
    // a `set` that takes nothing.
    let no_canvas = lua.create_table_with_capacity(0, 3)?;
    no_canvas.raw_set("width", 0)?;
    no_canvas.raw_set("height", 0)?;
    no_canvas.raw_set("set", set.clone())?;
    no_canvas.set_readonly(true);
    let no_canvas = Rc::new(lua.create_registry_value(no_canvas)?);
    let set = Rc::new(lua.create_registry_value(set)?);

    let handle =
        move |lua: &Lua, tables: &mlua::RegistryKey, shared: &Shared, id: Option<ImageId>| {
            let tables: Table = lua.registry_value(tables)?;
            let Some(id) = id else {
                return tables.raw_get::<Table>(0);
            };
            let at = id.0 as usize + 1;
            if let Value::Table(t) = tables.raw_get::<Value>(at)? {
                return Ok(t);
            }
            let pictures = shared.pictures.borrow();
            let picture = pictures.get(id);
            let (w, h) = picture.map_or((0, 0), |p| (p.width, p.height));
            let t = lua.create_table_with_capacity(0, 5)?;
            t.raw_set("width", w)?;
            t.raw_set("height", h)?;
            t.raw_set("id", at)?;
            match picture.map(|p| &p.kind) {
                Some(PictureKind::Animation(_, timeline)) => {
                    t.raw_set("frames", timeline.frames())?;
                    t.raw_set("duration", timeline.duration)?;
                }
                Some(PictureKind::Canvas) => {
                    t.raw_set("set", lua.registry_value::<mlua::Function>(&set)?)?;
                }
                _ => {}
            }
            t.set_readonly(true);
            tables.raw_seti(at, t.clone())?;
            Ok(t)
        };
    let handle = Rc::new(handle);

    for (name, animated) in [("image", false), ("animation", true)] {
        let (s, tb, handle) = (shared.clone(), tables.clone(), handle.clone());
        gmnb.raw_set(
            name,
            lua.create_function(move |lua, file: Value| {
                let Value::String(file) = file else {
                    return Err(bad(name, "expected a file's path in the theme's folder"));
                };
                let file = file.to_str()?;
                let id = if animated {
                    s.pictures.borrow_mut().animation_file(&file)
                } else {
                    s.pictures.borrow_mut().image(&file)
                };
                handle(lua, &tb, &s, id)
            })?,
        )?;
    }
    let (s, tb, h) = (shared.clone(), tables.clone(), handle.clone());
    gmnb.raw_set(
        "pixels",
        lua.create_function(move |lua, (width, height): (Value, Value)| {
            let what = "pixels";
            let side = |v: &Value| -> mlua::Result<u32> {
                let n = number(what, v)?;
                if n.fract() != 0.0 || !(1.0..=65_536.0).contains(&n) {
                    return Err(bad(
                        what,
                        "a picture's width and height are whole numbers of pixels, 1 or more",
                    ));
                }
                Ok(n as u32)
            };
            let (width, height) = (side(&width)?, side(&height)?);
            // (Asked in a statement of its own: the pictures are free
            // again before the table is made.)
            let id = s.pictures.borrow_mut().pixels_picture(width, height);
            match id {
                None => lua.registry_value::<Table>(&no_canvas),
                id => h(lua, &tb, &s, id),
            }
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
