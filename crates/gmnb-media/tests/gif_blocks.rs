//! A GIF's walk costs by the block: a file made of blocks of a byte or two
//! is refused before the decoder walks it (round 4, area E: 8 MiB of them
//! took the decoder 60 to 90 ms of a theme's call), and a real GIF of as
//! many frames as are allowed, or of one big picture, is not.

use gmnb_media::{AnimLimits, Bytes, open, thread_time};

/// The most an animation's file may be.
const MOST: usize = 8 << 20;

/// A GIF's header: 2 × 2, a global colour table of two.
fn head() -> Vec<u8> {
    let mut g = b"GIF89a".to_vec();
    g.extend([2, 0, 2, 0, 0x80, 0, 0]);
    g.extend([0, 0, 0, 255, 255, 255]);
    g
}

/// A frame of 0.1 s: its graphic control, its descriptor, and its data in
/// `blocks` sub-blocks of `size` bytes.
fn frame(g: &mut Vec<u8>, blocks: usize, size: u8) {
    g.extend([0x21, 0xF9, 4, 0, 10, 0, 0, 0]);
    g.extend([0x2C, 0, 0, 0, 0, 2, 0, 2, 0, 0]);
    g.push(2);
    for _ in 0..blocks {
        g.push(size);
        g.extend(std::iter::repeat_n(0x44, usize::from(size)));
    }
    g.push(0);
}

/// What `open` makes of the file, and the processor time it took.
fn opened(g: Vec<u8>) -> (Result<u32, String>, f64) {
    assert!(g.len() <= MOST);
    let bytes = Bytes::new(g);
    let t = thread_time().unwrap();
    let r = open(&bytes, &AnimLimits::default());
    let took = (thread_time().unwrap() - t).as_secs_f64();
    (r.map(|t| t.frames()).map_err(|e| e.to_string()), took)
}

#[test]
fn a_gif_of_tiny_blocks_is_refused_before_it_is_walked() {
    let filled = |start: Vec<u8>, block: &[u8], end: &[u8]| {
        let mut g = start;
        while g.len() + block.len() + end.len() + 32 < MOST {
            g.extend(block);
        }
        g.extend(end);
        frame(&mut g, 1, 1);
        g.push(0x3B);
        g
    };
    let mut first = head();
    frame(&mut first, 1, 1);
    let mut comment = first.clone();
    comment.extend([0x21, 0xFE]);
    let mut app = first.clone();
    app.extend([0x21, 0xFF, 11]);
    app.extend(b"XMP DataXMP");
    let mut data = head();
    data.extend([
        0x21, 0xF9, 4, 0, 10, 0, 0, 0, 0x2C, 0, 0, 0, 0, 2, 0, 2, 0, 0, 2,
    ]);
    for (name, g) in [
        (
            "comments",
            filled(first.clone(), &[0x21, 0xFE, 1, b'x', 0], &[]),
        ),
        ("a comment's sub-blocks", filled(comment, &[1, b'x'], &[0])),
        ("an application's sub-blocks", filled(app, &[1, b'x'], &[0])),
        ("a frame's data sub-blocks", filled(data, &[1, 0x44], &[0])),
        (
            "graphic controls",
            filled(head(), &[0x21, 0xF9, 4, 0, 10, 0, 0, 0], &[]),
        ),
        ("empty plain texts", filled(head(), &[0x21, 0x01, 0], &[])),
    ] {
        let (r, took) = opened(g);
        assert!(r.is_err(), "{name}: {r:?}");
        assert!(took < 0.02, "{name}: {:.1} ms", took * 1000.0);
    }
}

#[test]
fn a_real_gifs_blocks_are_not_too_many() {
    // As many frames as are allowed, each a pixel's change.
    let most = AnimLimits::default().frames as usize;
    let mut g = head();
    for _ in 0..most {
        frame(&mut g, 1, 2);
    }
    g.push(0x3B);
    assert_eq!(opened(g).0, Ok(most as u32));
    // One picture of the most bytes, in data sub-blocks of 255.
    let mut g = head();
    frame(&mut g, (MOST - 64) / 256, 255);
    g.push(0x3B);
    assert_eq!(opened(g).0, Ok(1));
    // And of 64, as an encoder might write them.
    let mut g = head();
    frame(&mut g, (MOST - 64) / 65, 64);
    g.push(0x3B);
    assert_eq!(opened(g).0, Ok(1));
}
