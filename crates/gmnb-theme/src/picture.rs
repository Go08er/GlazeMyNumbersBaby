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
use std::rc::{Rc, Weak};
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

/// A picture the theme fills (`gmnb.pixels`, or a module's), as it
/// stood at one version. A draw of it holds the version it drew
/// ([`crate::draw::Op::Image`]'s `canvas`), so that what was drawn is what
/// is shown, whatever the theme does with the picture afterwards.
#[derive(Clone, Debug, PartialEq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// RGBA8, `width × 4` bytes a row: not premultiplied (`gmnb.pixels`),
    /// or premultiplied (a module's: [`Canvas::premultiplied`]).
    pub pixels: Bytes,
    /// Goes up each time the theme changes the picture: a texture made
    /// of an earlier version is of that version.
    pub version: u64,
    /// The pixels are premultiplied, each colour no more than the
    /// opacity: a module's picture (`picture:take`).
    pub premultiplied: bool,
}

/// Earlier versions of the theme's pictures that draws still hold, at
/// most: past that, a change is refused (as past the image total).
pub const EARLIER_HELD: usize = 1024;

struct CanvasState {
    /// `gmnb.pixels`': the version standing, shared with each draw of it.
    /// A module's: the version a layer that has never taken it draws (the
    /// last taken anywhere but in a preview).
    canvas: Rc<Canvas>,
    /// `gmnb.pixels`': how often it may change, 30 times a second, two in
    /// hand. (A module picture's allowances are its takers'.)
    allowance: Bucket,
    /// A module's picture (`module:picture`), whose size each `take`
    /// sets; else `gmnb.pixels`'.
    module: bool,
    /// A module's picture is kept per layer, as a field is: each that has
    /// taken it, its own allowance and the version it took last. At most
    /// [`TAKERS`]; past that, the one used longest ago is let go.
    takers: HashMap<Taker, Took>,
    /// The versions made, for the next one's number.
    versions: u64,
    /// The clock `takers` is kept by: a tick for each take or draw.
    used: u64,
}

/// Who takes a module's picture, and so whose allowance a take spends and
/// whose last version it sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Taker {
    /// A window layer, in the place it is drawn (this readout's, that
    /// keypad's): the draw's own `layer`, as fields' are kept by.
    Layer(u64),
    /// A preview's tile: no allowance, and nothing it takes changes what a
    /// window layer draws. Held for that tile only: let go as the next
    /// tile is begun ([`Pictures::let_go_of_tiles`]).
    Preview,
    /// Any other hook (`style`, `sound`, an action...): an allowance of its
    /// own, and, as a layer's take does, the version a layer that has
    /// never taken the picture draws.
    Outside,
}

/// One taker's hold on a module's picture.
struct Took {
    allowance: Bucket,
    /// What it took last: what its draws show.
    last: Rc<Canvas>,
    /// When it was last used, on its picture's clock.
    used: u64,
}

/// Takers of a module's picture kept at once (as `Fields` keeps layers).
pub const TAKERS: usize = 32;

/// Why a module's picture can't be `width × height`, if it can't: 1 to
/// [`DrawLimits::module_picture_side`] a side, and no more than
/// [`DrawLimits::module_picture_pixels`] pixels (any shape).
pub(crate) fn module_size(limits: &DrawLimits, width: u64, height: u64) -> Result<(), String> {
    let side = u64::from(limits.module_picture_side);
    let pixels = u64::from(limits.module_picture_pixels);
    let fits = (1..=side).contains(&width)
        && (1..=side).contains(&height)
        && width.checked_mul(height).is_some_and(|n| n <= pixels);
    if fits {
        Ok(())
    } else {
        Err(format!(
            "{width}×{height}; a module's picture is at most {pixels} pixels, 1 to {side} a side"
        ))
    }
}

/// `bytes` (RGBA) premultiplied as a module's picture is: each colour
/// clamped to the pixel's opacity.
pub(crate) fn premultiplied(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    for p in out.as_chunks_mut::<4>().0 {
        let a = p[3];
        p[0] = p[0].min(a);
        p[1] = p[1].min(a);
        p[2] = p[2].min(a);
    }
    out
}

/// Whether `bytes`, premultiplied as a module's picture is, are `held`.
fn same_premultiplied(held: &[u8], bytes: &[u8]) -> bool {
    held.len() == bytes.len()
        && held
            .as_chunks::<4>()
            .0
            .iter()
            .zip(bytes.as_chunks::<4>().0)
            .all(|(h, b)| {
                let a = b[3];
                *h == [b[0].min(a), b[1].min(a), b[2].min(a), a]
            })
}

/// The versions a picture's state holds: the one standing (or a layer
/// that never took a module's draws) and each taker's last.
fn versions(state: &CanvasState) -> Vec<&Rc<Canvas>> {
    let mut all = vec![&state.canvas];
    all.extend(state.takers.values().map(|t| &t.last));
    all
}

/// The versions a picture's state holds, each once, shared.
fn distinct(state: &CanvasState) -> Vec<Rc<Canvas>> {
    let mut out: Vec<Rc<Canvas>> = Vec::new();
    for c in versions(state) {
        if !out.iter().any(|o| Rc::ptr_eq(o, c)) {
            out.push(c.clone());
        }
    }
    out
}

/// The version taken last of those a picture's state holds.
fn latest(state: &CanvasState) -> &Rc<Canvas> {
    versions(state)
        .into_iter()
        .max_by_key(|c| c.version)
        .unwrap_or(&state.canvas)
}

/// The bytes of `all`, each version once.
fn distinct_bytes(all: &[&Rc<Canvas>]) -> u64 {
    let mut seen: Vec<*const Canvas> = Vec::new();
    let mut bytes = 0;
    for c in all {
        if !seen.contains(&Rc::as_ptr(c)) {
            seen.push(Rc::as_ptr(c));
            bytes += c.pixels.len() as u64;
        }
    }
    bytes
}

/// What the bytes a picture's state holds come to.
fn held(state: &CanvasState) -> u64 {
    distinct_bytes(&versions(state))
}

/// Of the versions a picture's state holds, those not among `after` that a
/// draw holds too (more holds than the state's own): their bytes, and how
/// many.
fn leaving(state: &CanvasState, after: &[&Rc<Canvas>]) -> (u64, usize) {
    let now = versions(state);
    let mut seen: Vec<*const Canvas> = Vec::new();
    let (mut bytes, mut n) = (0, 0);
    for c in &now {
        let ptr = Rc::as_ptr(c);
        if seen.contains(&ptr) || after.iter().any(|a| Rc::as_ptr(a) == ptr) {
            continue;
        }
        seen.push(ptr);
        let own = now.iter().filter(|o| Rc::as_ptr(o) == ptr).count();
        if Rc::strong_count(c) > own {
            bytes += c.pixels.len() as u64;
            n += 1;
        }
    }
    (bytes, n)
}

/// Whether, of a picture's state (or what it would be: `fallback` and
/// `takers`), the tile's version is held by the tile alone: an earlier
/// version once the next tile is begun, if a draw holds it then, and so
/// counted as one already (`Pictures::tiles_alone`).
fn tile_alone<'a>(
    fallback: &Rc<Canvas>,
    takers: impl Iterator<Item = (Taker, &'a Rc<Canvas>)>,
) -> bool {
    let takers: Vec<_> = takers.collect();
    takers.iter().any(|(k, tile)| {
        *k == Taker::Preview
            && !Rc::ptr_eq(tile, fallback)
            && !takers
                .iter()
                .any(|(o, v)| *o != Taker::Preview && Rc::ptr_eq(v, tile))
    })
}

/// The same, of a picture's state as it stands.
fn tile_alone_now(state: &CanvasState) -> bool {
    tile_alone(
        &state.canvas,
        state.takers.iter().map(|(k, t)| (*k, &t.last)),
    )
}

/// Why a module picture didn't take what it was given.
pub(crate) enum Refused {
    /// Too soon after the last change: it can be taken in this many
    /// seconds.
    TooSoon(f64),
    /// Over a cap: the note for the theme's author.
    Over(String),
    /// The image total is taken up by earlier versions draws still hold:
    /// the note, and the layer is owed a frame (they go as frames are
    /// drawn).
    Full(String),
}

/// A take refused for the earlier versions still shown: as many as are
/// held.
fn too_many_earlier(width: u32, height: u32) -> Refused {
    Refused::Full(format!(
        "picture:take: {width}×{height}: {EARLIER_HELD} earlier versions of the theme's pictures \
         are still shown"
    ))
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
    /// Of those, what was charged once for good: stills, animations,
    /// pictures `gmnb.pixels` fills. (The rest is what module pictures
    /// hold now: [`Pictures::decoded_from_scratch`].)
    charged: u64,
    noises: usize,
    canvases: HashMap<ImageId, CanvasState>,
    /// Versions of those pictures since changed that draws may still
    /// hold (a layer's last frame, Settings' tiles): while one is alive it
    /// counts against the image total, with the pictures as they stand.
    earlier: Vec<Weak<Canvas>>,
    /// Module pictures made.
    module_pictures: usize,
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
            charged: 0,
            noises: 0,
            canvases: HashMap::new(),
            earlier: Vec::new(),
            module_pictures: 0,
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

    /// The same, worked out afresh: what was charged once for good, and
    /// the versions each module picture holds, each version once. The
    /// count [`Pictures::decoded_bytes`] keeps as pictures change is
    /// always this (checked as it changes, in a debug build).
    pub fn decoded_from_scratch(&self) -> u64 {
        self.charged
            + self
                .canvases
                .values()
                .filter(|c| c.module)
                .map(held)
                .sum::<u64>()
    }

    /// The count kept is the count worked out afresh.
    fn check(&self) {
        debug_assert_eq!(
            self.decoded,
            self.decoded_from_scratch(),
            "the image total kept is not what the pictures hold"
        );
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

    /// A picture the theme fills, as it stands: of a module's, the version
    /// taken last, anywhere (a preview's too).
    pub fn canvas(&self, id: ImageId) -> Option<&Canvas> {
        self.canvases.get(&id).map(|c| &**latest(c))
    }

    /// The same, shared.
    pub fn snapshot(&self, id: ImageId) -> Option<Rc<Canvas>> {
        self.canvases.get(&id).map(|c| latest(c).clone())
    }

    /// The version of a picture the theme fills that a draw by `taker`
    /// shows, shared: what the draw holds. Of a module's, the version that
    /// taker took last; one that has never taken it shows the version
    /// taken last in a window layer or outside any. (A draw keeps its
    /// taker's hold from being let go before others'.)
    pub(crate) fn shown(&mut self, id: ImageId, taker: Taker) -> Option<Rc<Canvas>> {
        let state = self.canvases.get_mut(&id)?;
        if !state.module {
            return Some(state.canvas.clone());
        }
        state.used += 1;
        let now = state.used;
        Some(match state.takers.get_mut(&taker) {
            Some(took) => {
                took.used = now;
                took.last.clone()
            }
            None => state.canvas.clone(),
        })
    }

    /// The size of a module picture's version that `taker` would draw.
    pub(crate) fn shown_size(&self, id: ImageId, taker: Taker) -> (u32, u32) {
        let Some(state) = self.canvases.get(&id) else {
            return (0, 0);
        };
        let c = match state.takers.get(&taker) {
            Some(took) if state.module => &took.last,
            _ => &state.canvas,
        };
        (c.width, c.height)
    }

    /// Bytes of earlier versions of the theme's pictures that draws still
    /// hold, and how many there are.
    pub fn earlier_bytes(&self) -> (u64, usize) {
        self.earlier
            .iter()
            .filter_map(Weak::upgrade)
            .fold((0, 0), |(bytes, n), c| {
                (bytes + c.pixels.len() as u64, n + 1)
            })
    }

    /// Why picture `id` can't become `wanted` bytes now, if it can't: the
    /// theme's pictures as they stand, and the earlier versions draws
    /// still hold (this one's too, once it is changed, if a draw holds
    /// it), are kept within the image total and [`EARLIER_HELD`].
    fn room_for(&mut self, id: ImageId, wanted: u64) -> Result<(), String> {
        // (Those no draw holds any more are forgotten.)
        self.earlier.retain(|e| e.strong_count() > 0);
        let (earlier, count) = self.earlier_bytes();
        let Some(state) = self.canvases.get(&id) else {
            return Ok(());
        };
        let held = state.canvas.pixels.len() as u64;
        // (The picture's own table holds no version: only draws do.)
        let drawn = Rc::strong_count(&state.canvas) > 1;
        let kept = if drawn { held } else { 0 };
        if self.decoded - held + wanted + earlier + kept > self.limits.image_bytes {
            return Err(format!(
                "the theme's pictures, with the earlier ones still shown, would be over {} MiB \
                 decoded",
                self.limits.image_bytes >> 20
            ));
        }
        // (A tile's version the tile alone holds is counted as one to
        // come.)
        if drawn && count + self.tiles_alone(id) >= EARLIER_HELD {
            return Err(format!(
                "{EARLIER_HELD} earlier versions of the theme's pictures are still shown"
            ));
        }
        Ok(())
    }

    /// Picture `id` becomes `canvas`: the version it was is kept track
    /// of while a draw holds it.
    fn replace(&mut self, id: ImageId, canvas: Canvas) {
        let Some(state) = self.canvases.get_mut(&id) else {
            return;
        };
        let old = std::mem::replace(&mut state.canvas, Rc::new(canvas));
        if Rc::strong_count(&old) > 1 {
            self.earlier.push(Rc::downgrade(&old));
        }
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
        self.charged += bytes;
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
        if self.canvases.values().filter(|c| !c.module).count() >= self.limits.canvases {
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
                canvas: Rc::new(Canvas {
                    width,
                    height,
                    // It starts transparent.
                    pixels: Bytes::new(vec![0; bytes]),
                    version: 0,
                    premultiplied: false,
                }),
                allowance: Bucket::new(),
                module: false,
                takers: HashMap::new(),
                versions: 0,
                used: 0,
            },
        );
        Some(id)
    }

    /// `module:picture()`: a new picture a module fills, of no size until
    /// its first `take`. Each call makes another, up to
    /// [`DrawLimits::module_pictures`].
    pub(crate) fn module_picture(&mut self) -> Option<ImageId> {
        if self.module_pictures >= self.limits.module_pictures {
            let note = format!(
                "module:picture: a theme's modules may fill {} pictures; left out",
                self.limits.module_pictures
            );
            self.note(note);
            return None;
        }
        self.module_pictures += 1;
        let id = self.add(
            format!("module-picture:{}", self.module_pictures),
            Picture {
                kind: PictureKind::Canvas,
                width: 0,
                height: 0,
            },
        )?;
        self.canvases.insert(
            id,
            CanvasState {
                canvas: Rc::new(Canvas {
                    width: 0,
                    height: 0,
                    pixels: Bytes::new(Vec::new()),
                    version: 0,
                    premultiplied: true,
                }),
                allowance: Bucket::new(),
                module: true,
                takers: HashMap::new(),
                versions: 0,
                used: 0,
            },
        );
        Some(id)
    }

    /// Seconds from `now` until module picture `id` would take a change
    /// from `taker`: 0 if it would now (and always in a preview; one that
    /// has never taken it has its whole allowance). Infinite: not a module
    /// picture.
    pub(crate) fn module_wait(&self, id: ImageId, taker: Taker, now: f64) -> f64 {
        match self.canvases.get(&id) {
            Some(state) if state.module => match (taker, state.takers.get(&taker)) {
                (Taker::Preview, _) | (_, None) => 0.0,
                (_, Some(took)) => took.allowance.wait(now),
            },
            _ => f64::INFINITY,
        }
    }

    /// Of the module pictures but `except`, how many hold a tile's version
    /// that the tile alone holds: earlier versions to come, counted with
    /// the earlier ones against [`EARLIER_HELD`], so that letting them go
    /// never takes the count past it.
    fn tiles_alone(&self, except: ImageId) -> usize {
        self.canvases
            .iter()
            .filter(|(id, state)| **id != except && state.module && tile_alone_now(state))
            .count()
    }

    /// A preview's tile is about to be drawn: each module picture lets go
    /// of the version the last tile took, so that a tile draws what it
    /// takes itself, or, taking nothing, what a layer that never took the
    /// picture draws; never another tile's. A version the last tile's draw
    /// still holds is counted as an earlier one while it does (it was
    /// counted as one to come: see [`Pictures::tiles_alone`]).
    pub(crate) fn let_go_of_tiles(&mut self) {
        let ids: Vec<ImageId> = self
            .canvases
            .iter()
            .filter(|(_, state)| state.module && state.takers.contains_key(&Taker::Preview))
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            let state = self.canvases.get_mut(&id).expect("listed above");
            let before = held(state);
            let old = distinct(state);
            state.takers.remove(&Taker::Preview);
            let after = distinct(state);
            self.decoded =
                self.decoded - before + distinct_bytes(&after.iter().collect::<Vec<_>>());
            for v in old {
                // (`old` holds one; the picture none, now.)
                if !after.iter().any(|a| Rc::ptr_eq(a, &v)) && Rc::strong_count(&v) > 1 {
                    self.earlier.push(Rc::downgrade(&v));
                }
            }
        }
        self.check();
    }

    /// `picture:take(module, offset, w, h)` by `taker` at `now`: for that
    /// taker, module picture `id` becomes the `width × height` picture of
    /// `bytes` (RGBA, premultiplied: each colour is clamped to the pixel's
    /// opacity as it is copied), if its allowance has a change to give (a
    /// preview's take is always given one, and changes nothing a window
    /// layer draws). The same bytes as that taker's last, at the same size,
    /// change nothing and spend nothing. Every version a taker holds is
    /// charged to the image total, once however many hold it, and the
    /// earlier versions draws still hold with them.
    pub(crate) fn take_module(
        &mut self,
        id: ImageId,
        taker: Taker,
        width: u32,
        height: u32,
        bytes: &[u8],
        now: f64,
    ) -> Result<(), Refused> {
        module_size(&self.limits, width.into(), height.into())
            .map_err(|why| Refused::Over(format!("picture:take: {why}")))?;
        let Some(state) = self.canvases.get_mut(&id).filter(|s| s.module) else {
            return Err(Refused::Over(
                "picture:take was called on something that isn't a module's picture".into(),
            ));
        };
        state.used += 1;
        let tick = state.used;
        let shown = match state.takers.get(&taker) {
            Some(took) => &took.last,
            None => &state.canvas,
        };
        if (shown.width, shown.height) == (width, height)
            && same_premultiplied(&shown.pixels, bytes)
        {
            // Nothing changes and nothing is spent; a taker that hadn't
            // taken it keeps the version it shows as its own.
            match state.takers.get_mut(&taker) {
                Some(took) => took.used = tick,
                None => self.add_taker(id, taker, width, height)?,
            }
            self.check();
            return Ok(());
        }
        let pixels = premultiplied(bytes);
        // (The same bytes as the version taken last elsewhere: that one,
        // held once.)
        let newest = latest(state);
        let new = if (newest.width, newest.height) == (width, height)
            && newest.pixels[..] == pixels[..]
        {
            newest.clone()
        } else {
            Rc::new(Canvas {
                width,
                height,
                pixels: Bytes::new(pixels),
                version: state.versions + 1,
                premultiplied: true,
            })
        };
        // What the picture would hold after, and what that frees.
        let state = &self.canvases[&id];
        let evict = (!state.takers.contains_key(&taker) && state.takers.len() >= TAKERS)
            .then(|| {
                state
                    .takers
                    .iter()
                    .min_by_key(|(_, t)| t.used)
                    .map(|(k, _)| *k)
            })
            .flatten();
        let before = held(state);
        let mut after: Vec<&Rc<Canvas>> = state
            .takers
            .iter()
            .filter(|(k, _)| **k != taker && Some(**k) != evict)
            .map(|(_, t)| &t.last)
            .collect();
        after.push(&new);
        let fallback_after = if taker == Taker::Preview {
            &state.canvas
        } else {
            &new
        };
        after.push(fallback_after);
        let after_bytes = distinct_bytes(&after);
        let tile_before = tile_alone_now(state);
        let tile_after = tile_alone(
            fallback_after,
            state
                .takers
                .iter()
                .filter(|(k, _)| **k != taker && Some(**k) != evict)
                .map(|(k, t)| (*k, &t.last))
                .chain(std::iter::once((taker, &new))),
        );
        // Versions the picture lets go of that draws still hold become
        // earlier ones, counted while they are.
        let (gone_bytes, gone) = leaving(state, &after);
        let total = self.decoded - before + after_bytes;
        if total > self.limits.image_bytes {
            return Err(Refused::Over(format!(
                "picture:take: {width}×{height}: the theme's pictures would be over {} MiB decoded",
                self.limits.image_bytes >> 20
            )));
        }
        self.earlier.retain(|e| e.strong_count() > 0);
        let (earlier, count) = self.earlier_bytes();
        if total + earlier + gone_bytes > self.limits.image_bytes {
            return Err(Refused::Full(format!(
                "picture:take: {width}×{height}: the theme's pictures, with the earlier ones still \
                 shown, would be over {} MiB decoded",
                self.limits.image_bytes >> 20
            )));
        }
        // (A tile's version the tile alone holds is counted as one to
        // come.)
        let tiles = self.tiles_alone(id);
        let was = count + tiles + usize::from(tile_before);
        let will = count + gone + tiles + usize::from(tile_after);
        if will > was && will > EARLIER_HELD {
            return Err(too_many_earlier(width, height));
        }
        let state = self.canvases.get_mut(&id).expect("looked up above");
        let mut allowance = match state.takers.get(&taker) {
            Some(took) => took.allowance,
            None => Bucket::new(),
        };
        if taker != Taker::Preview && !allowance.take(now) {
            return Err(Refused::TooSoon(allowance.wait(now)));
        }
        // Taken: the picture as it stands now, the versions it let go
        // counted while draws hold them.
        let old = distinct(state);
        if new.version > state.versions {
            state.versions = new.version;
        }
        if let Some(k) = evict {
            state.takers.remove(&k);
        }
        if taker != Taker::Preview {
            state.canvas = new.clone();
        }
        state.takers.insert(
            taker,
            Took {
                allowance,
                last: new,
                used: tick,
            },
        );
        let (fallback_w, fallback_h) = (state.canvas.width, state.canvas.height);
        let after = distinct(state);
        self.decoded = self.decoded - before + distinct_bytes(&after.iter().collect::<Vec<_>>());
        for v in old {
            // (`old` holds one; the picture none, now.)
            if !after.iter().any(|a| Rc::ptr_eq(a, &v)) && Rc::strong_count(&v) > 1 {
                self.earlier.push(Rc::downgrade(&v));
            }
        }
        if let Some(picture) = self.list.get_mut(id.0 as usize) {
            picture.width = fallback_w;
            picture.height = fallback_h;
        }
        self.check();
        Ok(())
    }

    /// `taker`, which hadn't taken module picture `id`, takes the bytes
    /// it shows already (`width × height`, the picture's fallback): it
    /// holds that version as its own, with its whole allowance, the one
    /// used longest ago let go if the picture keeps as many takers as it
    /// may. Refused (Full) only if that one's version, still drawn, would
    /// take the earlier ones past their number.
    fn add_taker(
        &mut self,
        id: ImageId,
        taker: Taker,
        width: u32,
        height: u32,
    ) -> Result<(), Refused> {
        self.earlier.retain(|e| e.strong_count() > 0);
        let count = self.earlier_bytes().1;
        let tiles = self.tiles_alone(id);
        let Some(state) = self.canvases.get(&id) else {
            return Ok(());
        };
        let evict = (state.takers.len() >= TAKERS)
            .then(|| {
                state
                    .takers
                    .iter()
                    .min_by_key(|(_, t)| t.used)
                    .map(|(k, _)| *k)
            })
            .flatten();
        let kept = || {
            state
                .takers
                .iter()
                .filter(|(k, _)| Some(**k) != evict)
                .map(|(k, t)| (*k, &t.last))
                .chain(std::iter::once((taker, &state.canvas)))
        };
        let after: Vec<&Rc<Canvas>> = kept()
            .map(|(_, v)| v)
            .chain(std::iter::once(&state.canvas))
            .collect();
        let (_, gone) = leaving(state, &after);
        let was = count + tiles + usize::from(tile_alone_now(state));
        let will = count + gone + tiles + usize::from(tile_alone(&state.canvas, kept()));
        if will > was && will > EARLIER_HELD {
            return Err(too_many_earlier(width, height));
        }
        let state = self.canvases.get_mut(&id).expect("looked up above");
        let before = held(state);
        let old = distinct(state);
        if let Some(k) = evict {
            state.takers.remove(&k);
        }
        let used = state.used;
        let last = state.canvas.clone();
        state.takers.insert(
            taker,
            Took {
                allowance: Bucket::new(),
                last,
                used,
            },
        );
        let after = distinct(state);
        self.decoded = self.decoded - before + distinct_bytes(&after.iter().collect::<Vec<_>>());
        for v in old {
            if !after.iter().any(|a| Rc::ptr_eq(a, &v)) && Rc::strong_count(&v) > 1 {
                self.earlier.push(Rc::downgrade(&v));
            }
        }
        self.check();
        Ok(())
    }

    /// `canvas:set(buffer)` at `now`: the picture takes `bytes`, if they
    /// are as many as it has and it wasn't changed too recently. `Ok` of
    /// `Err`: refused as too soon, and in how many seconds it would be
    /// taken. `Err`: refused, with a note for the theme's author: the
    /// wrong size (`false`), or the image total taken up by earlier
    /// versions draws still hold (`true`: the layer is owed a frame, as
    /// they go as frames are drawn).
    fn set_canvas(
        &mut self,
        id: ImageId,
        bytes: Vec<u8>,
        now: f64,
    ) -> Result<Result<(), f64>, (String, bool)> {
        let Some(state) = self.canvases.get(&id) else {
            return Ok(Err(f64::INFINITY));
        };
        // (`set` reads the picture off the table it is called on.) A
        // module's picture changes only by `take`: per layer, clamped,
        // counted as its layers hold it. No frame would take it: none is
        // owed.
        if state.module {
            return Err((
                "gmnb.pixels: set was called on a module's picture, which changes only by \
                 picture:take"
                    .into(),
                false,
            ));
        }
        let c = &state.canvas;
        let want = c.width as usize * c.height as usize * 4;
        if bytes.len() != want {
            return Err((
                format!(
                    "gmnb.pixels: set was given a buffer of {} bytes; a {}×{} picture takes \
                     {want}",
                    bytes.len(),
                    c.width,
                    c.height
                ),
                false,
            ));
        }
        // The bytes it holds already: nothing changes and nothing is
        // spent.
        if c.pixels[..] == bytes[..] {
            return Ok(Ok(()));
        }
        let (width, height) = (c.width, c.height);
        self.room_for(id, want as u64).map_err(|why| {
            (
                format!("gmnb.pixels: {width}×{height}: set was refused: {why}"),
                true,
            )
        })?;
        let state = self.canvases.get_mut(&id).expect("looked up above");
        if !state.allowance.take(now) {
            return Ok(Err(state.allowance.wait(now)));
        }
        let version = state.canvas.version + 1;
        self.replace(
            id,
            Canvas {
                width,
                height,
                pixels: Bytes::new(bytes),
                version,
                premultiplied: false,
            },
        );
        self.check();
        Ok(Ok(()))
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
            Ok(Ok(())) => Ok(true),
            // Too soon: the old picture stays. If a hook is drawing, its
            // layer is drawn once more when the picture can be changed.
            Ok(Err(wait)) => {
                if wait.is_finite() {
                    s.recorder.borrow_mut().retry(wait.max(1.0 / 60.0));
                }
                Ok(false)
            }
            // The wrong size: the call does nothing, and says so once.
            // Earlier versions still shown taking up the image total: the
            // same, and the layer is owed a frame (they go as frames are
            // drawn).
            Err((note, owed)) => {
                s.pictures.borrow_mut().note(note);
                if owed {
                    s.recorder.borrow_mut().retry(1.0 / 30.0);
                }
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
