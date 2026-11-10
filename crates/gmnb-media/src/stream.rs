//! An animation decoded as it is shown: one frame after another, each
//! composited onto the canvas as its format asks (docs/theming-media.md,
//! "The animation player", compositing). Forward only: a frame behind
//! the stream's place starts the file again.
//!
//! What a stream holds between frames is one canvas and, for a file whose
//! frames ask for the previous picture back, the part of the canvas to
//! put back. A frame's own pixels are freed before the finished frame is
//! copied out.
//!
//! - **GIF**: the frame's rectangle is drawn, transparent pixels left as
//!   they are; then it is disposed of as it says (kept; made transparent;
//!   put back as it was before the frame).
//! - **APNG**: `blend_op` source or over; `dispose_op` none, background
//!   (transparent) or previous.
//! - **WebP**: each frame's own picture is decoded by `image-webp` and
//!   composited here, blended or not as its flags say; a frame that asks
//!   to be disposed of leaves its rectangle transparent. (`image-webp`
//!   0.2.4's own compositing ignores that request unless it is given a
//!   background colour, and with one clears the wrong bytes under a frame
//!   that has no alpha.)
//!
//! # A WebP picture says its size twice
//!
//! Once in the file's header (or a frame's), and once in the picture's
//! own bitstream. `image-webp` 0.2.4 decodes a lossy picture at the size
//! its bitstream says, allocating for it, and compares with the header
//! only afterwards; its memory limit covers metadata alone. A file with a
//! 16 × 16 header around a bitstream claiming 16 383 × 16 383 would pass
//! every check made on the header (measured: 384 MiB asked for by a file
//! of 126 bytes). So no bitstream reaches the decoder here until its own
//! size has been read and found to be the size already approved
//! ([`webp_picture`]).
//!
//! What is left open: a lossy bitstream also says how long each of its
//! partitions is, in 24 bits, and the decoder allocates a partition
//! before finding that the file doesn't hold it. How many there are is
//! inside the compressed header, out of this crate's reach. One such
//! allocation is alive at a time: a spoiled lossy WebP can make the
//! decoder ask for up to 16 MiB it never touches, for a moment.

use std::io::Cursor;

use crate::sniff::Format;
use crate::still::{decoder_room, expand_rgb, png_decoder, png_row, premultiply};
use crate::{Bytes, Error, broken};

/// A part of the canvas: x, y, width, height, inside it.
type Part = [usize; 4];

/// What the frame last drawn asks for once it has been shown.
#[derive(Default)]
enum After {
    /// Leave it.
    #[default]
    Keep,
    /// Make its part transparent.
    Clear(Part),
    /// Put back what was there before it.
    Restore(Part, Vec<u8>),
}

struct Gif {
    decoder: gif::Decoder<Cursor<Bytes>>,
    /// The file's own palette, for frames that bring none.
    global: Vec<u8>,
}

struct Apng {
    reader: png::Reader<Cursor<Bytes>>,
    animated: bool,
    /// The file's first image is the animation's first frame (an `fcTL`
    /// comes before its `IDAT`); else it is only for programs that don't
    /// animate, and is skipped.
    first_is_frame: bool,
}

/// One frame of an animated WebP, as its `ANMF` chunk describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WebpFrame {
    /// Where in the file the frame's own chunks are (`ALPH`, `VP8 `,
    /// `VP8L`).
    data: std::ops::Range<usize>,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    /// Milliseconds.
    pub(crate) duration: u32,
    /// Blended onto what is there (else it replaces it).
    blend: bool,
    /// Its rectangle is made transparent once it has been shown.
    dispose: bool,
}

enum Webp {
    /// A still picture: an animation of one frame.
    Still,
    Frames(Vec<WebpFrame>),
}

fn le24(bytes: &[u8], at: usize) -> usize {
    usize::from(bytes[at]) | usize::from(bytes[at + 1]) << 8 | usize::from(bytes[at + 2]) << 16
}

/// A RIFF file's chunks within `bytes[from..to]`, as (name, where its
/// data is), for as long as they are whole.
fn riff_chunks(
    bytes: &[u8],
    from: usize,
    to: usize,
) -> impl Iterator<Item = ([u8; 4], std::ops::Range<usize>)> + '_ {
    let mut at = from;
    let to = to.min(bytes.len());
    std::iter::from_fn(move || {
        let head = bytes.get(at..at.checked_add(8)?)?;
        let name = [head[0], head[1], head[2], head[3]];
        let size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let body = at + 8;
        let end = body.checked_add(size)?;
        if end > to {
            return None;
        }
        // Chunks are padded to an even length.
        at = end + (size & 1);
        Some((name, body..end))
    })
}

/// The frames of an animated WebP, by its `ANMF` chunks; at most `most`
/// (`None` if there are more).
pub(crate) fn webp_frames(bytes: &[u8], most: usize) -> Option<Vec<WebpFrame>> {
    let mut frames = Vec::new();
    for (name, data) in riff_chunks(bytes, 12, bytes.len()) {
        if &name != b"ANMF" || data.len() < 16 {
            continue;
        }
        if frames.len() >= most {
            return None;
        }
        let at = data.start;
        // The offsets are stored halved, the sizes less one.
        frames.push(WebpFrame {
            x: le24(bytes, at) * 2,
            y: le24(bytes, at + 3) * 2,
            width: le24(bytes, at + 6) + 1,
            height: le24(bytes, at + 9) + 1,
            duration: le24(bytes, at + 12) as u32,
            blend: bytes[at + 15] & 0x02 == 0,
            dispose: bytes[at + 15] & 0x01 != 0,
            data: at + 16..data.end,
        });
    }
    Some(frames)
}

/// The size a WebP picture's own bitstream says it has.
fn bitstream_size(name: &[u8; 4], data: &[u8]) -> Option<(usize, usize)> {
    match name {
        // Lossy: a 3-byte frame tag, the start code, then 14 bits each
        // (the two bits above them are a scale nothing honours).
        b"VP8 " => {
            let head = data.get(..10)?;
            if head[3..6] != [0x9d, 0x01, 0x2a] {
                return None;
            }
            // The tag: a key frame (a picture by itself), and how long
            // the first partition is, which the chunk must hold.
            let tag = u32::from(head[0]) | u32::from(head[1]) << 8 | u32::from(head[2]) << 16;
            if tag & 1 != 0 || (tag >> 5) as usize > data.len() - 10 {
                return None;
            }
            let side =
                |at: usize| usize::from(u16::from_le_bytes([head[at], head[at + 1]]) & 0x3fff);
            Some((side(6), side(8)))
        }
        // Lossless: a signature byte, then 14 bits each, less one.
        b"VP8L" => {
            let head = data.get(..5)?;
            if head[0] != 0x2f {
                return None;
            }
            let bits = u32::from_le_bytes([head[1], head[2], head[3], head[4]]);
            Some((
                (bits & 0x3fff) as usize + 1,
                ((bits >> 14) & 0x3fff) as usize + 1,
            ))
        }
        _ => None,
    }
}

/// The picture held by the chunks in `bytes[within]` (`ALPH`, `VP8 `,
/// `VP8L`; others are passed over), which is to be `width × height`:
/// RGBA8, not premultiplied.
///
/// The chunks are handed to the decoder as the still WebP file they
/// amount to, and only once the bitstream's own size has been read and
/// is `width × height`: the decoder allocates by that size, not by the
/// header's.
pub(crate) fn webp_picture(
    bytes: &[u8],
    within: std::ops::Range<usize>,
    width: usize,
    height: usize,
) -> Result<Vec<u8>, Error> {
    let chunks: Vec<_> = riff_chunks(bytes, within.start, within.end)
        .filter(|(name, _)| matches!(name, b"ALPH" | b"VP8 " | b"VP8L"))
        .collect();
    let mut pictures = chunks.iter().filter(|(name, _)| name != b"ALPH");
    let (Some((name, data)), None) = (pictures.next(), pictures.next()) else {
        return Err(broken("not one picture where one was expected"));
    };
    if bitstream_size(name, &bytes[data.clone()]) != Some((width, height)) {
        return Err(broken("a picture isn't the size its header says"));
    }
    // A lossless picture (or a lossless picture of opacity) says how many
    // groups of codes it has only inside its bitstream, and the decoder
    // builds them all before a pixel: they are counted first.
    let size = (width as u16, height as u16);
    for (name, data) in &chunks {
        let held = match name {
            b"VP8L" => crate::vp8l::codes_held(&bytes[data.clone()], None),
            // Its first byte says how it is compressed; then the picture.
            b"ALPH" => match bytes[data.clone()].split_first() {
                Some((info, picture)) if info & 3 == 1 => {
                    crate::vp8l::codes_held(picture, Some(size))
                }
                _ => continue,
            },
            _ => continue,
        };
        match held {
            Ok(_) => {}
            Err(crate::vp8l::Refused::TooBig) => {
                return Err(broken(format_args!(
                    "a lossless picture whose codes would take more than {} MiB to hold",
                    crate::vp8l::ROOM >> 20
                )));
            }
            Err(crate::vp8l::Refused::Unreadable) => {
                return Err(broken("a lossless picture that can't be read"));
            }
        }
    }
    let mut file = Vec::with_capacity(within.len() + 32);
    file.extend_from_slice(b"RIFF\0\0\0\0WEBP");
    if chunks.iter().any(|(name, _)| name == b"ALPH") {
        // Opacity apart from the colours: the extended form, which says
        // so in its header (flag 0x10) with the picture's size less one.
        file.extend_from_slice(b"VP8X\x0a\0\0\0\x10\0\0\0");
        file.extend_from_slice(&((width - 1) as u32).to_le_bytes()[..3]);
        file.extend_from_slice(&((height - 1) as u32).to_le_bytes()[..3]);
    }
    for (name, data) in chunks {
        file.extend_from_slice(&name);
        file.extend_from_slice(&(data.len() as u32).to_le_bytes());
        file.extend_from_slice(&bytes[data.clone()]);
        if data.len() % 2 == 1 {
            file.push(0);
        }
    }
    let size = (file.len() - 8) as u32;
    file[4..8].copy_from_slice(&size.to_le_bytes());
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(file)).map_err(broken)?;
    let (w, h) = decoder.dimensions();
    if (w as usize, h as usize) != (width, height) || decoder.is_animated() {
        return Err(broken("a picture isn't the size its header says"));
    }
    let mut out = vec![0; width * height * 4];
    let size = decoder
        .output_buffer_size()
        .filter(|size| *size <= out.len())
        .ok_or_else(|| broken("too large"))?;
    decoder.read_image(&mut out[..size]).map_err(broken)?;
    if !decoder.has_alpha() {
        expand_rgb(&mut out);
    }
    Ok(out)
}

enum State {
    Gif(Box<Gif>),
    Apng(Box<Apng>),
    Webp(Box<Webp>),
}

/// One animation file being decoded.
pub struct Stream {
    bytes: Bytes,
    format: Format,
    width: usize,
    height: usize,
    /// RGBA8, not premultiplied; transparent at the start of each pass.
    canvas: Vec<u8>,
    /// The frame the canvas shows; `None` before the first of a pass.
    place: Option<u32>,
    state: Option<State>,
    after: After,
}

fn clip(x: usize, y: usize, w: usize, h: usize, width: usize, height: usize) -> Part {
    let (x, y) = (x.min(width), y.min(height));
    [x, y, w.min(width - x), h.min(height - y)]
}

/// A copy of a part of a canvas of `stride` bytes a row, to put back
/// later.
fn save(canvas: &[u8], stride: usize, [x, y, w, h]: Part) -> Vec<u8> {
    let mut saved = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        saved.extend_from_slice(&canvas[row * stride + x * 4..row * stride + (x + w) * 4]);
    }
    saved
}

/// `src` over `dst`, both RGBA8 and not premultiplied.
fn over(dst: &mut [u8; 4], src: &[u8; 4]) {
    match src[3] {
        0 => {}
        255 => *dst = *src,
        sa => {
            let (sa, da) = (u32::from(sa), u32::from(dst[3]));
            // Opacity times 255: the source's, and what shows of the
            // destination beneath it.
            let under = da * (255 - sa);
            let total = sa * 255 + under;
            for c in 0..3 {
                let mixed = u32::from(src[c]) * sa * 255 + u32::from(dst[c]) * under;
                dst[c] = ((mixed + total / 2) / total) as u8;
            }
            dst[3] = ((total + 127) / 255) as u8;
        }
    }
}

impl Stream {
    /// A stream at the start of `bytes`, a file of kind `format` whose
    /// canvas is `width × height` (as [`crate::probe`] read them). Nothing
    /// is decoded yet.
    pub fn open(bytes: Bytes, format: Format, width: u32, height: u32) -> Result<Stream, Error> {
        if format == Format::Jpeg {
            return Err(Error::NotAnimation(format));
        }
        Ok(Stream {
            bytes,
            format,
            width: width as usize,
            height: height as usize,
            canvas: Vec::new(),
            place: None,
            state: None,
            after: After::Keep,
        })
    }

    /// The frame last decoded; `None` before the first of a pass.
    pub fn place(&self) -> Option<u32> {
        self.place
    }

    fn canvas_bytes(&self) -> usize {
        self.width * self.height * 4
    }

    /// Starts the file again: a new decoder, a transparent canvas.
    fn rewind(&mut self) {
        self.state = None;
        self.place = None;
        self.after = After::Keep;
    }

    /// The decoder, made when a pass starts.
    fn start(&mut self) -> Result<(), Error> {
        let room = decoder_room(self.width as u32, self.height as u32);
        let cursor = Cursor::new(self.bytes.clone());
        let state = match self.format {
            Format::Gif => {
                let mut options = gif::DecodeOptions::new();
                // Palette indices, a byte a pixel: the colours are looked
                // up as the frame is drawn onto the canvas.
                options.set_color_output(gif::ColorOutput::Indexed);
                let limit =
                    std::num::NonZeroU64::new(room as u64).unwrap_or(std::num::NonZeroU64::MIN);
                options.set_memory_limit(gif::MemoryLimit::Bytes(limit));
                let decoder = options.read_info(cursor).map_err(broken)?;
                if (usize::from(decoder.width()), usize::from(decoder.height()))
                    != (self.width, self.height)
                {
                    return Err(broken("the picture isn't the size its header says"));
                }
                let global = decoder.global_palette().unwrap_or(&[]).to_vec();
                State::Gif(Box::new(Gif { decoder, global }))
            }
            Format::Png => {
                let reader = png_decoder(self.bytes.clone(), room)
                    .read_info()
                    .map_err(broken)?;
                let info = reader.info();
                if (info.width as usize, info.height as usize) != (self.width, self.height) {
                    return Err(broken("the picture isn't the size its header says"));
                }
                State::Apng(Box::new(Apng {
                    animated: info.animation_control.is_some(),
                    first_is_frame: info.frame_control.is_some(),
                    reader,
                }))
            }
            Format::Webp => {
                let info = crate::sniff::probe(&self.bytes)?;
                if (info.width as usize, info.height as usize) != (self.width, self.height) {
                    return Err(broken("the picture isn't the size its header says"));
                }
                State::Webp(Box::new(if info.animated {
                    let frames = webp_frames(&self.bytes, usize::MAX).unwrap_or_default();
                    // A frame is never larger than the picture: what it
                    // takes to decode is bounded by the canvas.
                    if frames
                        .iter()
                        .any(|f| f.width > self.width || f.height > self.height)
                    {
                        return Err(broken("a frame is larger than the picture"));
                    }
                    Webp::Frames(frames)
                } else {
                    Webp::Still
                }))
            }
            Format::Jpeg => return Err(Error::NotAnimation(self.format)),
        };
        self.canvas.clear();
        self.canvas.resize(self.canvas_bytes(), 0);
        self.state = Some(state);
        self.place = None;
        self.after = After::Keep;
        Ok(())
    }

    /// Does what the frame last drawn asked for once shown.
    fn dispose(&mut self) {
        let stride = self.width * 4;
        match std::mem::take(&mut self.after) {
            After::Keep => {}
            After::Clear([x, y, w, h]) => {
                for row in y..y + h {
                    self.canvas[row * stride + x * 4..row * stride + (x + w) * 4].fill(0);
                }
            }
            After::Restore([x, y, w, h], saved) => {
                for (i, row) in (y..y + h).enumerate() {
                    self.canvas[row * stride + x * 4..row * stride + (x + w) * 4]
                        .copy_from_slice(&saved[i * w * 4..(i + 1) * w * 4]);
                }
            }
        }
    }

    /// Decodes the next frame of the pass onto the canvas. Past the last
    /// frame it is an error: the file holds fewer frames than were
    /// counted.
    pub fn advance(&mut self) -> Result<(), Error> {
        if self.state.is_none() {
            self.start()?;
        }
        self.dispose();
        let first = self.place.is_none();
        let (width, height) = (self.width, self.height);
        let stride = width * 4;
        // What the frame asks for after it, decided before it is drawn:
        // "previous" needs a copy of what it covers.
        let Some(state) = self.state.as_mut() else {
            return Err(broken("no decoder"));
        };
        match state {
            State::Gif(gif) => {
                let Gif { decoder, global } = &mut **gif;
                let Some(frame) = decoder.read_next_frame().map_err(broken)? else {
                    return Err(broken("the file holds fewer frames than it has delays for"));
                };
                let (fw, fh) = (usize::from(frame.width), usize::from(frame.height));
                let part = clip(
                    usize::from(frame.left),
                    usize::from(frame.top),
                    fw,
                    fh,
                    width,
                    height,
                );
                let [x, y, w, h] = part;
                let palette = frame.palette.as_deref().unwrap_or(global);
                if frame.buffer.len() < fw * fh {
                    return Err(broken("a frame is cut short"));
                }
                let saved = (frame.dispose == gif::DisposalMethod::Previous)
                    .then(|| save(&self.canvas, stride, part));
                for row in 0..h {
                    let line = &frame.buffer[row * fw..row * fw + w];
                    let out = &mut self.canvas
                        [(y + row) * stride + x * 4..(y + row) * stride + (x + w) * 4];
                    for (o, index) in out.as_chunks_mut::<4>().0.iter_mut().zip(line) {
                        // A transparent pixel leaves what is there.
                        if frame.transparent == Some(*index) {
                            continue;
                        }
                        let at = usize::from(*index) * 3;
                        if let Some(rgb) = palette.get(at..at + 3) {
                            *o = [rgb[0], rgb[1], rgb[2], 255];
                        }
                    }
                }
                self.after = match (frame.dispose, saved) {
                    (gif::DisposalMethod::Background, _) => After::Clear(part),
                    (gif::DisposalMethod::Previous, Some(saved)) => After::Restore(part, saved),
                    _ => After::Keep,
                };
            }
            State::Apng(apng) => {
                let control = if !apng.animated {
                    None
                } else if first && apng.first_is_frame {
                    apng.reader.info().frame_control
                } else {
                    // Moves on to the next frame's data, past the first
                    // image if that isn't a frame.
                    Some(*apng.reader.next_frame_info().map_err(broken)?)
                };
                let mut buf = vec![0; apng.reader.output_buffer_size()];
                let frame = apng.reader.next_frame(&mut buf).map_err(broken)?;
                if frame.bit_depth != png::BitDepth::Eight {
                    return Err(broken("a bit depth the decoder left in place"));
                }
                let (fw, fh) = (frame.width as usize, frame.height as usize);
                let (fx, fy) =
                    control.map_or((0, 0), |c| (c.x_offset as usize, c.y_offset as usize));
                let part = clip(fx, fy, fw, fh, width, height);
                let [x, y, w, h] = part;
                let blend = control.is_some_and(|c| c.blend_op == png::BlendOp::Over);
                // For the first frame, "previous" means "background".
                let dispose = match control.map(|c| c.dispose_op) {
                    Some(png::DisposeOp::Previous) if first => Some(png::DisposeOp::Background),
                    other => other,
                };
                let saved = (dispose == Some(png::DisposeOp::Previous))
                    .then(|| save(&self.canvas, stride, part));
                let mut rgba = vec![0; fw * 4];
                for row in 0..h {
                    let line = buf
                        .get(row * frame.line_size..(row + 1) * frame.line_size)
                        .ok_or_else(|| broken("a frame is cut short"))?;
                    png_row(frame.color_type, line, &mut rgba)?;
                    let out = &mut self.canvas
                        [(y + row) * stride + x * 4..(y + row) * stride + (x + w) * 4];
                    if blend {
                        let pixels = out.as_chunks_mut::<4>().0.iter_mut();
                        for (o, i) in pixels.zip(rgba.as_chunks::<4>().0) {
                            over(o, i);
                        }
                    } else {
                        out.copy_from_slice(&rgba[..w * 4]);
                    }
                }
                self.after = match (dispose, saved) {
                    (Some(png::DisposeOp::Background), _) => After::Clear(part),
                    (Some(png::DisposeOp::Previous), Some(saved)) => After::Restore(part, saved),
                    _ => After::Keep,
                };
            }
            State::Webp(webp) => match &**webp {
                Webp::Still => {
                    if !first {
                        return Err(broken("the file holds one picture"));
                    }
                    // The picture is the canvas: no second one is held
                    // while it is decoded.
                    self.canvas = Vec::new();
                    self.canvas = webp_picture(&self.bytes, 12..self.bytes.len(), width, height)?;
                }
                Webp::Frames(frames) => {
                    let next = self.place.map_or(0, |p| p as usize + 1);
                    let Some(frame) = frames.get(next) else {
                        return Err(broken("the file holds fewer frames than it has delays for"));
                    };
                    let picture =
                        webp_picture(&self.bytes, frame.data.clone(), frame.width, frame.height)?;
                    let part = clip(frame.x, frame.y, frame.width, frame.height, width, height);
                    let [x, y, w, h] = part;
                    for row in 0..h {
                        let line = &picture[row * frame.width * 4..(row * frame.width + w) * 4];
                        let out = &mut self.canvas
                            [(y + row) * stride + x * 4..(y + row) * stride + (x + w) * 4];
                        if frame.blend {
                            let pixels = out.as_chunks_mut::<4>().0.iter_mut();
                            for (o, i) in pixels.zip(line.as_chunks::<4>().0) {
                                over(o, i);
                            }
                        } else {
                            out.copy_from_slice(line);
                        }
                    }
                    if frame.dispose {
                        self.after = After::Clear(part);
                    }
                }
            },
        }
        self.place = Some(self.place.map_or(0, |p| p + 1));
        Ok(())
    }

    /// The canvas as it is: RGBA8, not premultiplied.
    pub fn take_canvas(self) -> Vec<u8> {
        self.canvas
    }

    /// Decodes on to frame `index` and gives it, premultiplied, with how
    /// many frames were decoded on the way. The frames between are
    /// decoded and dropped; a frame at or behind the stream's place
    /// starts the file again.
    pub fn seek(&mut self, index: u32) -> Result<(Vec<u8>, u32), Error> {
        let result = self.seek_inner(index);
        if result.is_err() {
            // Whatever state the failure left is not gone on with.
            self.rewind();
        }
        result
    }

    fn seek_inner(&mut self, index: u32) -> Result<(Vec<u8>, u32), Error> {
        if self.place.is_some_and(|p| p > index) {
            self.rewind();
        }
        let mut decoded = 0;
        while self.place != Some(index) {
            self.advance()?;
            decoded += 1;
        }
        let mut out = self.canvas.clone();
        premultiply(&mut out);
        Ok((out, decoded))
    }

    /// One frame on the way to frame `index` ([`Stream::seek`] a frame at
    /// a time): `Some` with the frame, premultiplied, and how many frames
    /// this step decoded (0 or 1) once there; `None` after decoding a
    /// frame on the way.
    pub fn step(&mut self, index: u32) -> Result<Option<(Vec<u8>, u32)>, Error> {
        if self.place.is_some_and(|p| p > index) {
            self.rewind();
        }
        let mut decoded = 0;
        if self.place != Some(index) {
            if let Err(e) = self.advance() {
                self.rewind();
                return Err(e);
            }
            decoded = 1;
            if self.place != Some(index) {
                return Ok(None);
            }
        }
        let mut out = self.canvas.clone();
        premultiply(&mut out);
        Ok(Some((out, decoded)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_is_source_over_on_straight_alpha() {
        // Opaque source replaces; a clear one leaves.
        let mut d = [10, 20, 30, 40];
        over(&mut d, &[1, 2, 3, 255]);
        assert_eq!(d, [1, 2, 3, 255]);
        over(&mut d, &[9, 9, 9, 0]);
        assert_eq!(d, [1, 2, 3, 255]);
        // Half red over opaque blue: half each, opaque.
        let mut d = [0, 0, 255, 255];
        over(&mut d, &[255, 0, 0, 128]);
        assert_eq!(d, [128, 0, 127, 255]);
        // Onto nothing: the source as it is.
        let mut d = [0, 0, 0, 0];
        over(&mut d, &[200, 100, 50, 128]);
        assert_eq!(d, [200, 100, 50, 128]);
        // Half over half: three quarters, the colour weighted 2 to 1.
        let mut d = [0, 0, 0, 128];
        over(&mut d, &[255, 255, 255, 128]);
        assert_eq!(d[3], 192);
        assert!((168..=171).contains(&d[0]), "{d:?}");
    }

    #[test]
    fn a_part_is_cut_to_the_canvas() {
        assert_eq!(clip(2, 3, 4, 5, 10, 10), [2, 3, 4, 5]);
        assert_eq!(clip(8, 9, 4, 5, 10, 10), [8, 9, 2, 1]);
        assert_eq!(clip(20, 20, 4, 5, 10, 10), [10, 10, 0, 0]);
    }
}
