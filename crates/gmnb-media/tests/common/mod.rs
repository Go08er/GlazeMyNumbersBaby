//! Files made for the tests: PNG, animated PNG, GIF and lossless WebP
//! (still and animated) written here, so that every field of every frame
//! is the test's to choose (hostile ones too). JPEG and lossy WebP, which
//! nothing here can write, are a few small files in `fixtures/`.
#![allow(dead_code)]

use std::borrow::Cow;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use gmnb_media::{AnimLimits, Bytes, Error, Player, Timeline, open};

pub const RED: [u8; 4] = [255, 0, 0, 255];
pub const GREEN: [u8; 4] = [0, 255, 0, 255];
pub const BLUE: [u8; 4] = [0, 0, 255, 255];
pub const WHITE: [u8; 4] = [255, 255, 255, 255];
pub const CLEAR: [u8; 4] = [0, 0, 0, 0];

/// `w × h` pixels of one colour.
pub fn solid(w: u32, h: u32, color: [u8; 4]) -> Vec<u8> {
    color.repeat((w * h) as usize)
}

/// A picture as rows of letters: `R`, `G`, `B`, `W` for the four opaque
/// colours, `.` for a clear pixel, `?` for anything else.
pub fn letters(pixels: &[u8], width: usize) -> Vec<String> {
    let letter = |p: &[u8; 4]| match *p {
        RED => 'R',
        GREEN => 'G',
        BLUE => 'B',
        WHITE => 'W',
        CLEAR => '.',
        _ => '?',
    };
    pixels
        .as_chunks::<4>()
        .0
        .chunks(width)
        .map(|row| row.iter().map(letter).collect())
        .collect()
}

/// The pixel at `(x, y)` of a picture `width` wide.
pub fn pixel(pixels: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let at = (y * width + x) * 4;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

pub fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A 32-bit xorshift: the tests' only randomness, the same every run.
pub struct Rng(pub u32);

impl Rng {
    pub fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    pub fn below(&mut self, n: usize) -> usize {
        self.next() as usize % n.max(1)
    }
}

// ---- PNG -------------------------------------------------------------------------

/// A PNG file of `data` as the encoder takes it for that colour type and
/// depth.
pub fn png(
    w: u32,
    h: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
    palette: Option<(&[u8], &[u8])>,
) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w, h);
        encoder.set_color(color);
        encoder.set_depth(depth);
        if let Some((plte, trns)) = palette {
            encoder.set_palette(plte.to_vec());
            if !trns.is_empty() {
                encoder.set_trns(trns.to_vec());
            }
        }
        let mut writer = encoder.write_header().expect("a PNG header");
        writer.write_image_data(data).expect("PNG data");
    }
    out
}

pub fn rgba_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    png(w, h, png::ColorType::Rgba, png::BitDepth::Eight, rgba, None)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for b in bytes {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

pub fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut summed = kind.to_vec();
    summed.extend_from_slice(data);
    out.extend_from_slice(&crc32(&summed).to_be_bytes());
    out
}

/// The compressed picture of a PNG file: its `IDAT` chunks' data, joined.
fn idat(file: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut at = 8;
    while at + 8 <= file.len() {
        let len = u32::from_be_bytes([file[at], file[at + 1], file[at + 2], file[at + 3]]) as usize;
        if &file[at + 4..at + 8] == b"IDAT" {
            out.extend_from_slice(&file[at + 8..at + 8 + len]);
        }
        at += 12 + len;
    }
    out
}

/// One frame of an animated PNG: its place, its delay as a fraction of a
/// second, how it is disposed of (0 none, 1 background, 2 previous) and
/// blended (0 source, 1 over), and its RGBA pixels.
#[derive(Clone)]
pub struct ApngFrame {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub delay: (u16, u16),
    pub dispose: u8,
    pub blend: u8,
    pub rgba: Vec<u8>,
}

impl ApngFrame {
    pub fn new(x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) -> ApngFrame {
        ApngFrame {
            x,
            y,
            w,
            h,
            delay: (1, 10),
            dispose: 0,
            blend: 0,
            rgba: solid(w, h, color),
        }
    }

    pub fn dispose(mut self, op: u8) -> ApngFrame {
        self.dispose = op;
        self
    }

    pub fn blend(mut self, op: u8) -> ApngFrame {
        self.blend = op;
        self
    }

    pub fn delay(mut self, num: u16, den: u16) -> ApngFrame {
        self.delay = (num, den);
        self
    }
}

/// An animated PNG, chunk by chunk. `default`: a first image that is not
/// a frame (for programs that don't animate). `announced`: what `acTL`
/// says the frame count is (`None`: the truth).
pub fn apng(
    width: u32,
    height: u32,
    default: Option<&[u8]>,
    frames: &[ApngFrame],
    announced: Option<u32>,
) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = width.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&height.to_be_bytes());
    // 8 bits, RGBA, the one compression and filter method, not interlaced.
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    out.extend(png_chunk(b"IHDR", &ihdr));
    let mut actl = announced
        .unwrap_or(frames.len() as u32)
        .to_be_bytes()
        .to_vec();
    actl.extend_from_slice(&0u32.to_be_bytes());
    out.extend(png_chunk(b"acTL", &actl));
    if let Some(rgba) = default {
        out.extend(png_chunk(b"IDAT", &idat(&rgba_png(width, height, rgba))));
    }
    let mut sequence = 0u32;
    for (i, f) in frames.iter().enumerate() {
        let mut fctl = sequence.to_be_bytes().to_vec();
        sequence += 1;
        for n in [f.w, f.h, f.x, f.y] {
            fctl.extend_from_slice(&n.to_be_bytes());
        }
        fctl.extend_from_slice(&f.delay.0.to_be_bytes());
        fctl.extend_from_slice(&f.delay.1.to_be_bytes());
        fctl.extend_from_slice(&[f.dispose, f.blend]);
        out.extend(png_chunk(b"fcTL", &fctl));
        let data = idat(&rgba_png(f.w, f.h, &f.rgba));
        if i == 0 && default.is_none() {
            out.extend(png_chunk(b"IDAT", &data));
        } else {
            let mut fdat = sequence.to_be_bytes().to_vec();
            sequence += 1;
            fdat.extend_from_slice(&data);
            out.extend(png_chunk(b"fdAT", &fdat));
        }
    }
    out.extend(png_chunk(b"IEND", &[]));
    out
}

// ---- GIF -------------------------------------------------------------------------

/// The tests' palette: red, green, blue, white, and four spare entries
/// (index 4 is the one frames call transparent).
pub const PALETTE: [u8; 24] = [
    255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 9, 9, 9, 8, 8, 8, 7, 7, 7, 6, 6, 6,
];

#[derive(Clone)]
pub struct GifFrame {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    /// Hundredths of a second.
    pub delay: u16,
    pub dispose: gif::DisposalMethod,
    pub transparent: Option<u8>,
    pub palette: Option<Vec<u8>>,
    /// One palette index a pixel.
    pub indices: Vec<u8>,
}

impl GifFrame {
    /// A frame of one palette index.
    pub fn new(x: u16, y: u16, w: u16, h: u16, index: u8) -> GifFrame {
        GifFrame {
            x,
            y,
            w,
            h,
            delay: 10,
            dispose: gif::DisposalMethod::Keep,
            transparent: None,
            palette: None,
            indices: vec![index; usize::from(w) * usize::from(h)],
        }
    }

    pub fn dispose(mut self, method: gif::DisposalMethod) -> GifFrame {
        self.dispose = method;
        self
    }

    pub fn delay(mut self, hundredths: u16) -> GifFrame {
        self.delay = hundredths;
        self
    }
}

pub fn gif(width: u16, height: u16, frames: &[GifFrame]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut out, width, height, &PALETTE).expect("a GIF");
        encoder
            .set_repeat(gif::Repeat::Infinite)
            .expect("a GIF's repeat");
        for f in frames {
            let frame = gif::Frame {
                left: f.x,
                top: f.y,
                width: f.w,
                height: f.h,
                delay: f.delay,
                dispose: f.dispose,
                transparent: f.transparent,
                palette: f.palette.clone(),
                buffer: Cow::Borrowed(&f.indices),
                ..gif::Frame::default()
            };
            encoder.write_frame(&frame).expect("a GIF frame");
        }
    }
    out
}

// ---- WebP ------------------------------------------------------------------------

/// A still lossless WebP of RGBA pixels.
pub fn webp(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    image_webp::WebPEncoder::new(&mut out)
        .encode(rgba, w, h, image_webp::ColorType::Rgba8)
        .expect("a WebP");
    out
}

/// A still lossless WebP of RGB pixels (no opacity in the file).
pub fn webp_rgb(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    image_webp::WebPEncoder::new(&mut out)
        .encode(rgb, w, h, image_webp::ColorType::Rgb8)
        .expect("a WebP");
    out
}

pub fn riff_chunk(name: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = name.to_vec();
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    out
}

/// One frame of an animated WebP. Its place must be even (the file
/// stores it halved).
#[derive(Clone)]
pub struct WebpFrame {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub millis: u32,
    /// Blended onto what is there; else it replaces it.
    pub blend: bool,
    /// Its rectangle is cleared once shown.
    pub dispose: bool,
    pub rgba: Vec<u8>,
}

impl WebpFrame {
    pub fn new(x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) -> WebpFrame {
        WebpFrame {
            x,
            y,
            w,
            h,
            millis: 100,
            blend: true,
            dispose: false,
            rgba: solid(w, h, color),
        }
    }

    pub fn replace(mut self) -> WebpFrame {
        self.blend = false;
        self
    }

    pub fn dispose(mut self) -> WebpFrame {
        self.dispose = true;
        self
    }

    pub fn millis(mut self, millis: u32) -> WebpFrame {
        self.millis = millis;
        self
    }
}

fn le24(n: u32) -> [u8; 3] {
    let b = n.to_le_bytes();
    [b[0], b[1], b[2]]
}

/// An animated WebP: each frame a lossless picture in an `ANMF` chunk.
pub fn webp_anim(width: u32, height: u32, frames: &[WebpFrame]) -> Vec<u8> {
    // Animated, with opacity.
    let mut vp8x = vec![0x12, 0, 0, 0];
    vp8x.extend_from_slice(&le24(width - 1));
    vp8x.extend_from_slice(&le24(height - 1));
    let mut body = b"WEBP".to_vec();
    body.extend(riff_chunk(b"VP8X", &vp8x));
    // A background colour nobody honours, and "loop for ever".
    body.extend(riff_chunk(b"ANIM", &[0xff, 0xff, 0xff, 0xff, 0, 0]));
    for f in frames {
        let still = webp(f.w, f.h, &f.rgba);
        let len = u32::from_le_bytes([still[16], still[17], still[18], still[19]]) as usize;
        let mut anmf = Vec::new();
        for n in [f.x / 2, f.y / 2, f.w - 1, f.h - 1, f.millis] {
            anmf.extend_from_slice(&le24(n));
        }
        anmf.push(u8::from(f.dispose) | u8::from(!f.blend) << 1);
        anmf.extend(riff_chunk(b"VP8L", &still[20..20 + len]));
        body.extend(riff_chunk(b"ANMF", &anmf));
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

// ---- The tests' files ------------------------------------------------------------

/// Every good file of the tests, by name.
pub fn good() -> Vec<(&'static str, Vec<u8>)> {
    let four = [RED, [0, 255, 0, 128], BLUE, CLEAR].concat();
    vec![
        ("rgba.png", rgba_png(2, 2, &four)),
        (
            "anim.png",
            apng(
                4,
                4,
                None,
                &[
                    ApngFrame::new(0, 0, 4, 4, RED),
                    ApngFrame::new(1, 1, 2, 2, GREEN),
                ],
                None,
            ),
        ),
        (
            "two.gif",
            gif(
                4,
                4,
                &[GifFrame::new(0, 0, 4, 4, 0), GifFrame::new(1, 1, 2, 2, 1)],
            ),
        ),
        ("lossless.webp", webp(2, 2, &four)),
        (
            "anim.webp",
            webp_anim(
                4,
                4,
                &[
                    WebpFrame::new(0, 0, 4, 4, RED),
                    WebpFrame::new(2, 2, 2, 2, GREEN),
                ],
            ),
        ),
        ("baseline.jpg", fixture("baseline.jpg")),
        ("progressive.jpg", fixture("progressive.jpg")),
        ("gray.jpg", fixture("gray.jpg")),
        ("lossy.webp", fixture("lossy.webp")),
        ("lossy-alpha.webp", fixture("lossy-alpha.webp")),
        ("lossy-anim.webp", fixture("lossy-anim.webp")),
    ]
}

/// A file of each kind whose header claims a picture of `side × side`.
pub fn claiming(side: u32) -> Vec<(&'static str, Vec<u8>)> {
    let mut out = Vec::new();
    // PNG: the width and height of IHDR.
    let mut file = rgba_png(2, 2, &solid(2, 2, RED));
    file[16..20].copy_from_slice(&side.to_be_bytes());
    file[20..24].copy_from_slice(&side.to_be_bytes());
    out.push(("png", file));
    // GIF: the logical screen.
    let mut file = gif(4, 4, &[GifFrame::new(0, 0, 4, 4, 0)]);
    file[6..8].copy_from_slice(&(side as u16).to_le_bytes());
    file[8..10].copy_from_slice(&(side as u16).to_le_bytes());
    out.push(("gif", file));
    // JPEG: the frame header's height and width.
    let mut file = fixture("baseline.jpg");
    let sof = file
        .windows(2)
        .position(|w| w == [0xff, 0xc0])
        .expect("a frame header");
    file[sof + 5..sof + 7].copy_from_slice(&(side as u16).to_be_bytes());
    file[sof + 7..sof + 9].copy_from_slice(&(side as u16).to_be_bytes());
    out.push(("jpg", file));
    // WebP, extended: the canvas less one, 24 bits each.
    let mut file = webp_anim(4, 4, &[WebpFrame::new(0, 0, 4, 4, RED)]);
    file[24..27].copy_from_slice(&(side - 1).to_le_bytes()[..3]);
    file[27..30].copy_from_slice(&(side - 1).to_le_bytes()[..3]);
    out.push(("webp", file));
    out
}

/// WebP files whose header says one size and whose picture's own
/// bitstream another: 16 383 × 16 383 inside, a few pixels outside. Each
/// with whether it is an animation.
pub fn webp_liars() -> Vec<(&'static str, Vec<u8>, bool)> {
    // A lossy bitstream's size: 14 bits each, 6 bytes into the chunk.
    let lossy = |mut file: Vec<u8>| {
        let chunk = file
            .windows(4)
            .position(|w| w == b"VP8 ")
            .expect("a lossy picture");
        file[chunk + 14..chunk + 16].copy_from_slice(&0x3fffu16.to_le_bytes());
        file[chunk + 16..chunk + 18].copy_from_slice(&0x3fffu16.to_le_bytes());
        file
    };
    // A lossless one's: 14 bits each less one, after a signature byte.
    let lossless = |mut file: Vec<u8>| {
        let chunk = file
            .windows(4)
            .position(|w| w == b"VP8L")
            .expect("a lossless picture");
        let at = chunk + 9;
        let bits = u32::from_le_bytes([file[at], file[at + 1], file[at + 2], file[at + 3]]);
        let bits = (bits & 0xf000_0000) | 0x3fff | 0x3fff << 14;
        file[at..at + 4].copy_from_slice(&bits.to_le_bytes());
        file
    };
    vec![
        // An extended still: its canvas in one chunk, its picture in
        // another.
        ("lossy still", lossy(fixture("lossy-alpha.webp")), false),
        ("lossy frame", lossy(fixture("lossy-anim.webp")), true),
        (
            "lossless frame",
            lossless(webp_anim(4, 4, &[WebpFrame::new(0, 0, 4, 4, RED)])),
            true,
        ),
    ]
}

// ---- The player ------------------------------------------------------------------

/// An animation's timeline, under the caps a theme's animation has.
pub fn timeline(file: &[u8]) -> Result<Timeline, Error> {
    open(&Bytes::new(file.to_vec()), &AnimLimits::default())
}

/// Counts the thread's "a frame has arrived" calls, and waits for them.
#[derive(Default)]
pub struct Wake {
    count: Mutex<u64>,
    arrived: Condvar,
}

impl Wake {
    /// Waits until one more call than `seen` has been made.
    pub fn after(&self, seen: u64) -> u64 {
        let guard = self.count.lock().unwrap();
        let (guard, timeout) = self
            .arrived
            .wait_timeout_while(guard, Duration::from_secs(20), |n| *n <= seen)
            .unwrap();
        assert!(!timeout.timed_out(), "no frame arrived");
        *guard
    }

    pub fn count(&self) -> u64 {
        *self.count.lock().unwrap()
    }
}

pub fn player() -> (Player, Arc<Wake>) {
    let wake = Arc::new(Wake::default());
    let w = wake.clone();
    let player = Player::new(move || {
        *w.count.lock().unwrap() += 1;
        w.arrived.notify_all();
    });
    (player, wake)
}

pub fn add(player: &mut Player, key: u32, file: &[u8]) -> Arc<Timeline> {
    let t = Arc::new(timeline(file).unwrap());
    player
        .open(key, Bytes::new(file.to_vec()), t.clone())
        .unwrap();
    t
}

/// Waits until the thread has given everything it was asked for, taking
/// in what arrives: the animations whose frame shown changed.
pub fn settle(player: &mut Player, wake: &Wake) -> Vec<u32> {
    let mut changed = Vec::new();
    loop {
        let seen = wake.count();
        changed.extend(player.poll());
        if !player.pending() {
            return changed;
        }
        wake.after(seen);
    }
}
