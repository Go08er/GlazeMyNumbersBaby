//! A lossless WebP's codes, counted before the decoder builds them
//! (docs/theming-media.md, "A WebP says its size twice"): a file of one
//! pixel whose bitstream declares 65 536 groups of Huffman codes, each
//! code with a table of 1024 entries, made `image-webp` hold 1301 MiB
//! for two seconds.
//!
//! One test, so that nothing else allocates while it counts.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Bits written least significant first, as VP8L reads them.
#[derive(Default)]
struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn put(&mut self, value: u32, bits: u32) {
        self.acc |= u64::from(value) << self.n;
        self.n += bits;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// A Huffman code of `len` bits: its first bit is read first.
    fn code(&mut self, code: u32, len: u32) {
        for i in (0..len).rev() {
            self.put((code >> i) & 1, 1);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        // Room for the reader's look-ahead.
        self.out.extend([0; 8]);
        self.out
    }
}

/// A simple code of one symbol.
fn single(b: &mut Bits, symbol: u32) {
    b.put(1, 1);
    b.put(0, 1);
    if symbol < 2 {
        b.put(0, 1);
        b.put(symbol, 1);
    } else {
        b.put(1, 1);
        b.put(symbol, 8);
    }
}

/// A normal code for symbols 0 to 10 of lengths 1 to 9, 10 and 10: a
/// table of 1024 entries in the decoder.
fn deep(b: &mut Bits) {
    b.put(0, 1);
    // 14 code-length code lengths, in the format's order: 17, 18, 0, 1,
    // 2, 3, 4, 5, 16, 6, 7, 8, 9, 10.
    b.put(14 - 4, 4);
    for len in [0, 0, 0, 3, 3, 3, 3, 3, 0, 3, 4, 4, 4, 4] {
        b.put(len, 3);
    }
    // Eleven code lengths are read.
    b.put(1, 1);
    b.put(1, 3);
    b.put(11 - 2, 4);
    // The code-length code: 1..6 are 000..101, 7..10 are 1100..1111.
    for len in [1u32, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10] {
        if len <= 6 {
            b.code(len - 1, 3);
        } else {
            b.code(12 + len - 7, 4);
        }
    }
}

/// A lossless bitstream of `width × height` (`header`: with the header a
/// `VP8L` chunk starts with; without, as in an `ALPH` chunk) whose
/// entropy image names group `groups − 1` everywhere, with every group's
/// five codes `deep`.
fn lossless(groups: u32, width: u32, height: u32, header: bool) -> Vec<u8> {
    let mut b = Bits::default();
    if header {
        b.put(0x2f, 8);
        b.put(width - 1, 14);
        b.put(height - 1, 14);
        b.put(0, 1);
        b.put(0, 3);
    }
    // No transform, no colour cache.
    b.put(0, 1);
    b.put(0, 1);
    // An entropy image of 4 × 4 blocks, its codes simple: red and green
    // say which group.
    b.put(1, 1);
    b.put(0, 3);
    b.put(0, 1);
    let last = groups - 1;
    single(&mut b, last & 0xff);
    single(&mut b, last >> 8);
    single(&mut b, 0);
    single(&mut b, 0);
    single(&mut b, 0);
    for _ in 0..groups {
        for _ in 0..5 {
            deep(&mut b);
        }
    }
    // The pixels: green, red, blue, alpha all symbol 0 (code `0`).
    for _ in 0..width * height {
        b.put(0, 4);
    }
    b.finish()
}

fn wrapped(name: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = name.to_vec();
    chunk.extend((data.len() as u32).to_le_bytes());
    chunk.extend(data);
    if data.len() % 2 == 1 {
        chunk.push(0);
    }
    chunk
}

fn riff(chunks: &[Vec<u8>]) -> Vec<u8> {
    let body = chunks.concat();
    let mut file = b"RIFF".to_vec();
    file.extend((body.len() as u32 + 4).to_le_bytes());
    file.extend(b"WEBP");
    file.extend(body);
    file
}

/// A lossless WebP of one pixel.
fn bomb(groups: u32) -> Vec<u8> {
    riff(&[wrapped(b"VP8L", &lossless(groups, 1, 1, true))])
}

/// Decodes `file` as a still: what came of it, and the most held.
fn held(file: Vec<u8>) -> (Result<(u32, u32), Error>, usize) {
    let bytes = Bytes::new(file);
    let start = LIVE.load(Ordering::Relaxed);
    PEAK.store(start, Ordering::Relaxed);
    let result = decode_still(&bytes, 2048).map(|r| (r.width, r.height));
    (result, PEAK.load(Ordering::Relaxed) - start)
}

fn refused_for_its_codes(result: &Result<(u32, u32), Error>) -> bool {
    matches!(result, Err(Error::Broken(why)) if why.contains("codes"))
}

#[test]
fn a_lossless_webps_codes_are_counted_before_they_are_built() {
    // 65 536 groups in a file under the 4 MiB a picture may be: refused,
    // with nothing built.
    let file = bomb(65_536);
    assert!(file.len() < 4 << 20, "{}", file.len());
    let (result, peak) = held(file);
    assert!(refused_for_its_codes(&result), "{result:?}");
    assert!(peak < 4 << 20, "{} KiB held", peak >> 10);

    // As many as fit in the room the codes are given: the picture, and
    // never more held than was counted.
    let (result, peak) = held(bomb(700));
    assert_eq!(result, Ok((1, 1)));
    assert!(peak > 8 << 20, "{} KiB held", peak >> 10);
    assert!(peak < 17 << 20, "{} KiB held", peak >> 10);

    // The same codes as the picture of a lossy file's opacity, compressed
    // losslessly in its ALPH chunk: counted too.
    let lossy = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lossy.webp"),
    )
    .unwrap();
    let info = gmnb_media::probe(&lossy).unwrap();
    assert_eq!(&lossy[12..16], b"VP8 ");
    let vp8 = &lossy[20..20 + u32::from_le_bytes(lossy[16..20].try_into().unwrap()) as usize];
    let mut vp8x = vec![0x10, 0, 0, 0];
    vp8x.extend(&(info.width - 1).to_le_bytes()[..3]);
    vp8x.extend(&(info.height - 1).to_le_bytes()[..3]);
    let with_alpha = |alpha: &[u8]| {
        riff(&[
            wrapped(b"VP8X", &vp8x),
            wrapped(b"ALPH", alpha),
            wrapped(b"VP8 ", vp8),
        ])
    };
    let mut alpha = vec![1u8];
    alpha.extend(lossless(65_536, info.width, info.height, false));
    let (result, peak) = held(with_alpha(&alpha));
    assert!(refused_for_its_codes(&result), "{result:?}");
    assert!(peak < 8 << 20, "{} KiB held", peak >> 10);
    // And with a few groups, the picture.
    let mut alpha = vec![1u8];
    alpha.extend(lossless(3, info.width, info.height, false));
    let (result, _) = held(with_alpha(&alpha));
    assert_eq!(result, Ok((info.width, info.height)));
}
