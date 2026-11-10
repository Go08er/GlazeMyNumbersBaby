//! What decoding and playing hold in memory, counted: every allocation
//! of this test's process goes through a counting allocator, the
//! decoders' own included (docs/theming-media.md, "The animation player",
//! and test 7).
//!
//! One test, so that nothing else allocates while it counts.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use common::*;
use gmnb_media::{Bytes, Error, decode_still};

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(by: usize) {
    let now = LIVE.fetch_add(by, Ordering::Relaxed) + by;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

// SAFETY: every call is handed on to the system's allocator unchanged;
// this only counts the sizes that went by.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's contract is the system allocator's.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            grew(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as above.
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            grew(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        // SAFETY: as above.
        unsafe { System.dealloc(p, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: as above.
        let q = unsafe { System.realloc(p, layout, size) };
        if !q.is_null() {
            if size >= layout.size() {
                grew(size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - size, Ordering::Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Runs `f`: the most it held at once beyond what was held before it,
/// and what `during` (called by `f` at a moment of its choosing) saw.
struct Held {
    start: usize,
}

impl Held {
    fn begin() -> Held {
        let start = LIVE.load(Ordering::Relaxed);
        PEAK.store(start, Ordering::Relaxed);
        Held { start }
    }

    fn now(&self) -> usize {
        LIVE.load(Ordering::Relaxed).saturating_sub(self.start)
    }

    fn peak(&self) -> usize {
        PEAK.load(Ordering::Relaxed).saturating_sub(self.start)
    }
}

fn kib(bytes: usize) -> String {
    format!("{:.0} KiB", bytes as f64 / 1024.0)
}

/// Pixels with something in them for a decoder to do.
fn busy(rng: &mut Rng, w: u32, h: u32) -> Vec<u8> {
    let mut out = solid(w, h, RED);
    for p in out.as_chunks_mut::<4>().0 {
        if rng.below(3) == 0 {
            let c = rng.next();
            *p = [c as u8, (c >> 8) as u8, (c >> 16) as u8, 255];
        } else {
            *p = [rng.0 as u8, 80, 160, 200];
        }
    }
    out
}

const SIDE: u32 = 256;
const FRAME: usize = (SIDE * SIDE * 4) as usize;

/// Six-frame animations of 256 × 256 in each format, one frame of each a
/// part that asks for the previous picture back where the format can.
fn animations() -> Vec<(&'static str, Vec<u8>)> {
    let mut rng = Rng(0x0bad_5eed);
    let part = SIDE / 2;
    let mut apng_frames = Vec::new();
    let mut gif_frames = Vec::new();
    let mut webp_frames = Vec::new();
    for i in 0..6 {
        let whole = i != 3;
        let (x, y, w, h) = if whole {
            (0, 0, SIDE, SIDE)
        } else {
            (64, 64, part, part)
        };
        let mut a = ApngFrame::new(x, y, w, h, RED).blend(1);
        a.rgba = busy(&mut rng, w, h);
        let mut g = GifFrame::new(x as u16, y as u16, w as u16, h as u16, 0);
        for index in &mut g.indices {
            *index = rng.below(4) as u8;
        }
        let mut wp = WebpFrame::new(x, y, w, h, RED);
        wp.rgba = busy(&mut rng, w, h);
        if !whole {
            a = a.dispose(2);
            g = g.dispose(gif::DisposalMethod::Previous);
            wp = wp.dispose();
        }
        apng_frames.push(a);
        gif_frames.push(g);
        webp_frames.push(wp);
    }
    // A GIF's frames may be larger than its canvas (only what lies on it
    // is shown), and its decoder holds a whole frame as palette indices:
    // here each is eight times the canvas's pixels.
    let big = (SIDE as f64 * 8f64.sqrt()) as u16;
    let oversized: Vec<GifFrame> = (0..4)
        .map(|i| {
            let mut f = GifFrame::new(0, 0, big, big, 0);
            for index in &mut f.indices {
                *index = rng.below(4) as u8 + i % 2;
            }
            f
        })
        .collect();
    vec![
        ("GIF", gif(SIDE as u16, SIDE as u16, &gif_frames)),
        ("APNG", apng(SIDE, SIDE, None, &apng_frames, None)),
        ("WebP", webp_anim(SIDE, SIDE, &webp_frames)),
        (
            "GIF, frames larger than the canvas",
            gif(SIDE as u16, SIDE as u16, &oversized),
        ),
    ]
}

#[test]
fn what_decoding_and_playing_hold_is_within_what_is_charged() {
    // ---- Playing: four frames an animation, and what a frame asks to
    // have put back.
    for (name, file) in animations() {
        let t = timeline(&file).unwrap();
        let charged = t.charge() as usize;
        let bytes = Bytes::new(file);
        let held = Held::begin();
        let (mut p, wake) = player();
        p.open(1, bytes.clone(), std::sync::Arc::new(t.clone()))
            .unwrap();
        // The first frame alone: what reduced motion costs.
        let still = held.now();
        let mut steady = 0;
        for _ in 0..3 {
            for i in (1..t.frames()).chain([0]) {
                p.frame(1, i, 0.0);
                settle(&mut p, &wake);
                assert_eq!(p.frame(1, i, 0.0).unwrap().index, i, "{name}");
                steady = steady.max(held.now());
            }
        }
        let peak = held.peak();
        eprintln!(
            "{name} {SIDE}×{SIDE}: frame {}; first frame alone {}; between frames {} ({:.2} frames); \
             at most {} ({:.2} frames); charged {} (restore {})",
            kib(FRAME),
            kib(still),
            kib(steady),
            steady as f64 / FRAME as f64,
            kib(peak),
            peak as f64 / FRAME as f64,
            kib(charged),
            kib(t.restore as usize),
        );
        // A still animation holds its first frame and nothing else.
        assert!(still < FRAME + (64 << 10), "{name}: {still}");
        // Playing never holds more than is charged: the first frame, the
        // one shown, the one ahead and the canvas, what is to be put
        // back, and what the format's decoder keeps.
        assert!(
            steady <= peak && peak <= charged,
            "{name}: {peak} of {charged}"
        );
        // And never less than the four frames that are the player's: the
        // charge is not far above what is held.
        assert!(steady >= 4 * FRAME, "{name}: {steady}");
        assert!(charged <= peak + FRAME, "{name}: {peak} of {charged}");
        // The thread holds its canvas until it has ended: the next count
        // starts after that.
        let watch = p.watch();
        drop(p);
        while watch.stats().thread && !watch.stats().ended {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(held.now() < 4096, "{name}: {} left behind", held.now());
    }

    // ---- A still at 1024 × 1024: the decoder and the picture together.
    let side = 1024;
    let decoded = (side * side * 4) as usize;
    let mut rng = Rng(0x5eed_0001);
    let pixels = busy(&mut rng, side, side);
    let indices: Vec<u8> = (0..side * side).map(|_| rng.below(4) as u8).collect();
    let mut whole = GifFrame::new(0, 0, side as u16, side as u16, 0);
    whole.indices = indices;
    for (name, file) in [
        ("PNG", rgba_png(side, side, &pixels)),
        ("WebP (lossless)", webp(side, side, &pixels)),
        ("GIF", gif(side as u16, side as u16, &[whole])),
    ] {
        let bytes = Bytes::new(file);
        let held = Held::begin();
        let picture = decode_still(&bytes, 2048).unwrap();
        let peak = held.peak();
        assert_eq!(picture.pixels.len(), decoded);
        eprintln!(
            "{name} still {side}×{side}: decoded {}; at most {} held ({:.2} times)",
            kib(decoded),
            kib(peak),
            peak as f64 / decoded as f64
        );
        // The picture, and for a WebP its file once more (this one
        // hardly compresses).
        assert!(peak <= decoded * 7 / 4, "{name}: {peak}");
    }
    // The designer's largest test pictures, if they are at hand (they are
    // not part of the repository): 2048 × 2048, a JPEG among them.
    if let Some(dir) = std::env::var_os("GMNB_MEDIA_TESTMEDIA") {
        for name in ["still.png", "still.jpg"] {
            let Ok(file) = std::fs::read(std::path::Path::new(&dir).join(name)) else {
                continue;
            };
            let bytes = Bytes::new(file);
            let held = Held::begin();
            let started = std::time::Instant::now();
            let picture = decode_still(&bytes, 2048).unwrap();
            let took = started.elapsed();
            let (peak, decoded) = (held.peak(), picture.pixels.len());
            eprintln!(
                "{name} {}×{}: decoded {}; at most {} held ({:.2} times); {:.1} ms",
                picture.width,
                picture.height,
                kib(decoded),
                kib(peak),
                peak as f64 / decoded as f64,
                took.as_secs_f64() * 1000.0
            );
        }
    }

    // ---- A header that claims 60 000 × 60 000: nothing is allocated for
    // the picture.
    for (name, file) in claiming(60_000) {
        let bytes = Bytes::new(file);
        let held = Held::begin();
        assert!(matches!(
            decode_still(&bytes, 2048),
            Err(Error::Size { .. })
        ));
        assert!(matches!(
            gmnb_media::open(&bytes, &gmnb_media::AnimLimits::default()),
            Err(Error::Size { .. } | Error::NotAnimation(_))
        ));
        let peak = held.peak();
        assert!(peak < 4096, "{name}: {peak} bytes for a refused header");
    }

    // ---- A WebP whose picture claims 16 383 × 16 383 inside a small
    // header: refused before the decoder is given it.
    for (name, file, _) in webp_liars() {
        let bytes = Bytes::new(file);
        let held = Held::begin();
        assert!(matches!(decode_still(&bytes, 2048), Err(Error::Broken(_))));
        let peak = held.peak();
        assert!(
            peak < (64 << 10),
            "{name}: {peak} bytes for a lying picture"
        );
    }

    // ---- Malformed files, with pictures capped at 256 × 256: whatever a
    // spoiled header claims, a decoder never holds more than the cap
    // allows it.
    let cap = 256u32;
    let most = (cap * cap * 4) as usize;
    let mut rng = Rng(0x9e37_79b9);
    let mut worst = (0, "");
    let mut worst_lossy = (0, "");
    for (name, file) in good() {
        for _ in 0..300 {
            let mut bad = file.clone();
            for _ in 0..1 + rng.below(3) {
                let at = rng.below(bad.len());
                bad[at] = rng.next() as u8;
            }
            let bytes = Bytes::new(bad);
            let held = Held::begin();
            let _ = decode_still(&bytes, cap);
            let peak = held.peak();
            // A lossy WebP's partitions are apart: see below.
            let which = if name.starts_with("lossy") {
                &mut worst_lossy
            } else {
                &mut worst
            };
            if peak > which.0 {
                *which = (peak, name);
            }
        }
    }
    eprintln!(
        "malformed files under a {cap}×{cap} cap ({} a picture): at most {} held, by {};          lossy WebP: {}, by {}",
        kib(most),
        kib(worst.0),
        worst.1,
        kib(worst_lossy.0),
        worst_lossy.1
    );
    // The picture, the decoder's working copy of it, and its tables.
    assert!(worst.0 <= most * 3 + (1 << 20), "{worst:?}");
    // A lossy WebP says how long its partitions are in 24 bits, and its
    // decoder allocates one before finding the file doesn't hold it: up
    // to 16 MiB, never touched, one at a time.
    assert!(worst_lossy.0 <= most * 3 + (17 << 20), "{worst_lossy:?}");
}
