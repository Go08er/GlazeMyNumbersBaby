//! Ogg Vorbis files with a few bytes changed in their pages (each page's
//! checksum made right again, so that the damage reaches the decoder):
//! whatever the file holds, decoding it ends soon and does not abort. Set
//! `FUZZ_ITERS` and `FUZZ_SEED` for a longer run.

use gmnb_sound::file::decode;
use std::time::Instant;

const FILES: [&[u8]; 4] = [
    include_bytes!("data/tone-mono-44100.ogg"),
    include_bytes!("data/tone-stereo-48000.ogg"),
    include_bytes!("data/long-mono-8000.ogg"),
    include_bytes!("data/three-channels.ogg"),
];

fn crc(data: &[u8]) -> u32 {
    let mut c = 0u32;
    for &b in data {
        c ^= u32::from(b) << 24;
        for _ in 0..8 {
            c = if c & 0x8000_0000 != 0 {
                (c << 1) ^ 0x04c1_1db7
            } else {
                c << 1
            };
        }
    }
    c
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// (start, end) of each page.
fn pages(b: &[u8]) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    let mut at = 0;
    while at + 27 <= b.len() && &b[at..at + 4] == b"OggS" {
        let n = b[at + 26] as usize;
        if at + 27 + n > b.len() {
            break;
        }
        let body: usize = b[at + 27..at + 27 + n].iter().map(|&x| x as usize).sum();
        let end = at + 27 + n + body;
        if end > b.len() {
            break;
        }
        v.push((at, end));
        at = end;
    }
    v
}

fn fix(b: &mut [u8], start: usize, end: usize) {
    b[start + 22..start + 26].fill(0);
    let c = crc(&b[start..end]);
    b[start + 22..start + 26].copy_from_slice(&c.to_le_bytes());
}

#[test]
fn mutated_vorbis() {
    let iters: usize = std::env::var("FUZZ_ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let seed: u64 = std::env::var("FUZZ_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ seed);
    let mut worst = 0.0f64;
    let mut oks = 0;
    for it in 0..iters {
        let src = FILES[rng.below(FILES.len())];
        let pg = pages(src);
        let mut b = src.to_vec();
        let n = 1 + rng.below(4);
        let mut touched = Vec::new();
        for _ in 0..n {
            // Mostly the header pages.
            let p = if rng.below(3) < 2 {
                rng.below(pg.len().min(3))
            } else {
                rng.below(pg.len())
            };
            let (s, e) = pg[p];
            let from = if rng.below(8) == 0 { s + 4 } else { s + 27 };
            let at = from + rng.below(e - from);
            match rng.below(4) {
                0 => b[at] ^= 1 << rng.below(8),
                1 => b[at] = rng.next() as u8,
                2 => b[at] = 0xff,
                _ => b[at] = 0,
            }
            touched.push(p);
        }
        for p in touched {
            let (s, e) = pg[p];
            fix(&mut b, s, e);
        }
        let t = Instant::now();
        let r = decode(&b, Some(0.5));
        let el = t.elapsed().as_secs_f64();
        if r.is_ok() {
            oks += 1;
        }
        if el > worst {
            worst = el;
        }
        if el > 2.0 {
            std::fs::write(
                std::env::temp_dir().join(format!("gmnb-slow-{seed}-{it}.ogg")),
                &b,
            )
            .unwrap();
            panic!("slow decode {el}s (iter {it})");
        }
    }
    eprintln!("{iters} mutations, {oks} decoded, worst {worst:.3}s");
}
