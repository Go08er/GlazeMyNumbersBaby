//! Still pictures through the app's own decoders: good files of every
//! kind, and hostile ones (docs/theming-media.md, "Decoding stills" and
//! test 7).

mod common;

use common::*;
use gmnb_media::{Bytes, Error, Format, Rgba, decode_still, probe, shielded, sniff};

/// The side a theme's still may have.
const SIDE: u32 = 2048;

fn still(bytes: &[u8]) -> Result<Rgba, Error> {
    decode_still(&Bytes::new(bytes.to_vec()), SIDE)
}

fn near(got: [u8; 4], want: [u8; 4], by: u8) -> bool {
    got.iter().zip(&want).all(|(g, w)| g.abs_diff(*w) <= by)
}

#[test]
fn a_png_of_every_kind_decodes_to_premultiplied_rgba() {
    use png::{BitDepth, ColorType};
    // Colour with opacity: premultiplied, rounded to the nearest.
    let four = [RED, [0, 255, 0, 128], BLUE, CLEAR].concat();
    let want = [RED, [0, 128, 0, 128], BLUE, CLEAR].concat();
    let got = still(&rgba_png(2, 2, &four)).unwrap();
    assert_eq!((got.width, got.height), (2, 2));
    assert_eq!(got.pixels, want);
    // Colour without: opaque.
    let rgb = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    let got = still(&png(2, 2, ColorType::Rgb, BitDepth::Eight, &rgb, None)).unwrap();
    assert_eq!(got.pixels, [RED, GREEN, BLUE, WHITE].concat());
    // Grey, and grey with opacity.
    let got = still(&png(
        2,
        2,
        ColorType::Grayscale,
        BitDepth::Eight,
        &[0, 85, 170, 255],
        None,
    ))
    .unwrap();
    let grey = |v: u8| [v, v, v, 255];
    assert_eq!(
        got.pixels,
        [grey(0), grey(85), grey(170), grey(255)].concat()
    );
    let got = still(&png(
        2,
        2,
        ColorType::GrayscaleAlpha,
        BitDepth::Eight,
        &[200, 255, 100, 128, 50, 0, 255, 255],
        None,
    ))
    .unwrap();
    assert_eq!(
        got.pixels,
        [grey(200), [50, 50, 50, 128], CLEAR, grey(255)].concat()
    );
    // A palette with transparency, at two bits a pixel.
    let plte = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    let trns = [255, 128, 255, 0];
    let packed = [0b0001_0000, 0b1011_0000];
    let got = still(&png(
        2,
        2,
        ColorType::Indexed,
        BitDepth::Two,
        &packed,
        Some((&plte, &trns)),
    ))
    .unwrap();
    assert_eq!(got.pixels, want);
    // Sixteen bits a channel are reduced to eight.
    let deep: Vec<u8> = four.iter().flat_map(|c| [*c, 0x7f]).collect();
    let got = still(&png(2, 2, ColorType::Rgba, BitDepth::Sixteen, &deep, None)).unwrap();
    assert_eq!(got.pixels, want);
}

#[test]
fn jpeg_files_decode_baseline_progressive_and_grey() {
    for name in ["baseline.jpg", "progressive.jpg"] {
        let got = still(&fixture(name)).unwrap();
        assert_eq!((got.width, got.height), (16, 12), "{name}");
        let (left, right) = (pixel(&got.pixels, 16, 2, 6), pixel(&got.pixels, 16, 13, 6));
        assert!(near(left, [200, 50, 20, 255], 14), "{name}: {left:?}");
        assert!(near(right, [20, 50, 200, 255], 14), "{name}: {right:?}");
    }
    let got = still(&fixture("gray.jpg")).unwrap();
    assert_eq!((got.width, got.height), (16, 12));
    let (left, right) = (pixel(&got.pixels, 16, 2, 6), pixel(&got.pixels, 16, 13, 6));
    for p in [left, right] {
        assert!(p[0] == p[1] && p[1] == p[2] && p[3] == 255, "{p:?}");
    }
    assert!(left[0] > right[0] + 15, "{left:?} {right:?}");
}

#[test]
fn webp_files_decode_lossless_and_lossy_with_and_without_opacity() {
    let four = [RED, [0, 255, 0, 128], BLUE, CLEAR].concat();
    let got = still(&webp(2, 2, &four)).unwrap();
    assert_eq!((got.width, got.height), (2, 2));
    assert_eq!(got.pixels, [RED, [0, 128, 0, 128], BLUE, CLEAR].concat());
    // No opacity in the file: opaque.
    let rgb = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    let got = still(&webp_rgb(2, 2, &rgb)).unwrap();
    assert_eq!(got.pixels, [RED, GREEN, BLUE, WHITE].concat());
    // Lossy: near enough.
    let got = still(&fixture("lossy.webp")).unwrap();
    assert_eq!((got.width, got.height), (16, 12));
    let (left, right) = (pixel(&got.pixels, 16, 2, 6), pixel(&got.pixels, 16, 13, 6));
    assert!(near(left, [200, 50, 20, 255], 16), "{left:?}");
    assert!(near(right, [20, 50, 200, 255], 16), "{right:?}");
    // Lossy with its opacity apart: the right half is clear.
    let got = still(&fixture("lossy-alpha.webp")).unwrap();
    let (left, right) = (pixel(&got.pixels, 16, 2, 6), pixel(&got.pixels, 16, 13, 6));
    assert!(near(left, [200, 50, 20, 255], 16), "{left:?}");
    assert_eq!(right, CLEAR);
}

#[test]
fn of_a_file_that_moves_a_still_is_its_first_picture() {
    // A GIF: its first frame on its canvas; what the frame doesn't cover
    // is clear.
    let file = gif(
        4,
        4,
        &[GifFrame::new(1, 1, 2, 2, 1), GifFrame::new(0, 0, 4, 4, 0)],
    );
    let got = still(&file).unwrap();
    assert_eq!(letters(&got.pixels, 4), ["....", ".GG.", ".GG.", "...."]);
    // An animated PNG: the picture it holds for programs that don't
    // animate.
    let frames = [ApngFrame::new(0, 0, 4, 4, RED)];
    let file = apng(4, 4, Some(&solid(4, 4, BLUE)), &frames, None);
    let got = still(&file).unwrap();
    assert_eq!(letters(&got.pixels, 4), ["BBBB"; 4]);
    // An animated WebP: its first frame.
    let got = still(&fixture("lossy-anim.webp")).unwrap();
    assert_eq!((got.width, got.height), (32, 24));
    let p = pixel(&got.pixels, 32, 16, 12);
    assert!(near(p, [200, 50, 20, 255], 16), "{p:?}");
}

#[test]
fn a_file_is_taken_for_what_its_bytes_say_not_its_name() {
    // The four kinds are told apart by content alone: nothing here has a
    // name at all.
    for (name, bytes) in good() {
        let want = match name.rsplit('.').next() {
            Some("png") => Format::Png,
            Some("jpg") => Format::Jpeg,
            Some("gif") => Format::Gif,
            _ => Format::Webp,
        };
        assert_eq!(sniff(&bytes), Some(want), "{name}");
        assert!(still(&bytes).is_ok(), "{name}");
        // The wrong magic: none of the four, whatever follows.
        let mut other = bytes.clone();
        other[0] ^= 0x55;
        assert_eq!(still(&other), Err(Error::Unknown), "{name}");
    }
    // Another kind's first bytes in front of a file: refused as broken,
    // never handed to the decoder its own bytes would have named.
    let mut lying = b"\x89PNG\r\n\x1a\n".to_vec();
    lying.extend(gif(4, 4, &[GifFrame::new(0, 0, 4, 4, 0)]));
    assert!(matches!(still(&lying), Err(Error::Broken(_))));
    for other in [&b"<svg/>"[..], b"BM\x36\0\0\0", b"\0\0\x01\0", b""] {
        assert_eq!(still(other), Err(Error::Unknown));
    }
}

#[test]
fn a_truncated_file_is_refused() {
    for (name, bytes) in good() {
        // Cut anywhere: never a panic, and a picture only if it is of
        // the size the header says.
        let step = (bytes.len() / 97).max(1);
        for cut in (0..bytes.len()).step_by(step) {
            if let Ok(picture) = still(&bytes[..cut]) {
                let info = probe(&bytes).unwrap();
                assert_eq!(
                    (picture.width, picture.height),
                    (info.width, info.height),
                    "{name} cut at {cut}"
                );
            }
        }
        // Cut in half: refused.
        let half = still(&bytes[..bytes.len() / 2]);
        assert!(half.is_err(), "{name} cut in half decoded");
    }
}

#[test]
fn a_header_claiming_a_huge_picture_is_refused_before_anything_is_decoded() {
    for (name, file) in claiming(60_000) {
        let started = std::time::Instant::now();
        assert_eq!(
            still(&file),
            Err(Error::Size {
                width: 60_000,
                height: 60_000,
                max: SIDE
            }),
            "{name}"
        );
        // No 14 GiB picture was attempted.
        assert!(started.elapsed().as_millis() < 500, "{name}");
    }
    // A lossless WebP's header holds 14 bits a side: 16 384 at most.
    let mut file = webp(2, 2, &solid(2, 2, RED));
    file[21..25].copy_from_slice(&(16_383u32 | 16_383 << 14).to_le_bytes());
    assert!(matches!(
        still(&file),
        Err(Error::Size {
            width: 16_384,
            height: 16_384,
            ..
        })
    ));
    // One pixel over the cap, and of no size at all.
    for (name, file) in claiming(SIDE + 1) {
        assert!(matches!(still(&file), Err(Error::Size { .. })), "{name}");
    }
    let mut none = rgba_png(2, 2, &solid(2, 2, RED));
    none[16..20].copy_from_slice(&0u32.to_be_bytes());
    assert!(matches!(still(&none), Err(Error::Size { width: 0, .. })));
    // A small canvas whose one frame claims to be huge: the decoder's own
    // limit refuses it.
    let mut file = gif(4, 4, &[GifFrame::new(0, 0, 4, 4, 0)]);
    let descriptor = file
        .iter()
        .rposition(|b| *b == 0x2c)
        .expect("an image descriptor");
    file[descriptor + 5..descriptor + 7].copy_from_slice(&60_000u16.to_le_bytes());
    file[descriptor + 7..descriptor + 9].copy_from_slice(&60_000u16.to_le_bytes());
    assert!(matches!(still(&file), Err(Error::Broken(_))));
}

#[test]
fn a_webp_whose_picture_claims_another_size_than_its_header_is_refused_undecoded() {
    // The header passes every check; the picture inside says it is
    // 16 383 × 16 383. A decoder that goes by the picture's word would
    // allocate for it before noticing.
    for (name, file, animated) in webp_liars() {
        let started = std::time::Instant::now();
        let got = still(&file);
        assert!(
            matches!(&got, Err(Error::Broken(why)) if why.contains("size")),
            "{name}: {got:?}"
        );
        assert!(started.elapsed().as_millis() < 200, "{name}");
        if animated {
            // Its frames are counted without being decoded; played, the
            // frame is refused.
            let bytes = Bytes::new(file);
            let t = gmnb_media::open(&bytes, &gmnb_media::AnimLimits::default()).unwrap();
            let mut stream = gmnb_media::Stream::open(bytes, t.format, t.width, t.height).unwrap();
            assert!(matches!(stream.seek(0), Err(Error::Broken(_))), "{name}");
        }
    }
}

#[test]
fn a_decoder_that_panics_is_caught_and_the_file_refused() {
    // The panic's message is printed by the test harness's hook; the
    // caller sees an error.
    let caught: Result<(), Error> = shielded(|| panic!("index out of bounds in a decoder"));
    assert_eq!(
        caught,
        Err(Error::Panicked("index out of bounds in a decoder".into()))
    );
    let line = 7;
    let caught: Result<(), Error> = shielded(|| panic!("at line {line}"));
    assert_eq!(caught, Err(Error::Panicked("at line 7".into())));
    let caught: Result<(), Error> = shielded(|| std::panic::panic_any(42));
    assert_eq!(caught, Err(Error::Panicked("a panic".into())));
    // A long message is cut.
    let long = "x".repeat(5000);
    let Err(Error::Panicked(text)) = shielded(|| -> Result<(), Error> { panic!("{long}") }) else {
        panic!("not caught");
    };
    assert!(text.len() < 200);
    // What isn't a panic passes through.
    assert_eq!(shielded(|| Ok(3)), Ok(3));
    assert_eq!(
        shielded(|| Err::<(), _>(Error::Unknown)),
        Err(Error::Unknown)
    );
    assert!(format!("{}", Error::Panicked("boom".into())).contains("boom"));
}

#[test]
fn malformed_files_never_panic_past_the_shield() {
    // Every good file with a few bytes changed, many times over: each
    // gives a picture of the size its header says, or an error.
    let mut rng = Rng(0x9e37_79b9);
    let (mut ok, mut refused) = (0, 0);
    for (name, bytes) in good() {
        for _ in 0..400 {
            let mut bad = bytes.clone();
            for _ in 0..1 + rng.below(3) {
                let at = rng.below(bad.len());
                bad[at] = rng.next() as u8;
            }
            match still(&bad) {
                Ok(picture) => {
                    ok += 1;
                    assert_eq!(
                        picture.pixels.len(),
                        picture.width as usize * picture.height as usize * 4,
                        "{name}"
                    );
                    assert!(picture.width <= SIDE && picture.height <= SIDE, "{name}");
                }
                Err(Error::Panicked(what)) => panic!("{name}: a decoder panicked: {what}"),
                Err(_) => refused += 1,
            }
        }
    }
    assert!(ok > 0 && refused > 0, "{ok} decoded, {refused} refused");
}

#[test]
fn an_interlaced_palette_png_decodes_like_the_plain_one() {
    // The same picture twice, by another program's encoder: 32 bits a
    // pixel, and eight with a palette, interlaced.
    let plain = still(&fixture("still.png")).unwrap();
    let interlaced = still(&fixture("interlaced.png")).unwrap();
    assert_eq!((plain.width, plain.height), (16, 12));
    assert_eq!(plain, interlaced);
    assert_eq!(pixel(&plain.pixels, 16, 2, 6), [200, 50, 20, 255]);
    assert_eq!(pixel(&plain.pixels, 16, 13, 6), [20, 50, 200, 255]);
}
