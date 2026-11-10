//! Animations: each format's frames composited as the format asks,
//! timelines read without decoding, hostile files, and the player with
//! its thread (docs/theming-media.md, "The animation player" and tests 5
//! and 8).

mod common;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::*;
use gif::DisposalMethod;
use gmnb_media::{AnimLimits, Bytes, Error, Format, Source, Stream, Timeline, open, probe};

fn limits() -> AnimLimits {
    AnimLimits::default()
}

fn stream(file: &[u8]) -> Stream {
    let info = probe(file).unwrap();
    Stream::open(
        Bytes::new(file.to_vec()),
        info.format,
        info.width,
        info.height,
    )
    .unwrap()
}

/// Frame `i` of a file as letters, and how many frames were decoded to
/// reach it.
fn frame(stream: &mut Stream, i: u32) -> (Vec<String>, u32) {
    let (pixels, decoded) = stream.seek(i).unwrap();
    (letters(&pixels, 4), decoded)
}

/// Green at half opacity over red, as every format's "over" makes it.
const MIXED: [u8; 4] = [127, 128, 0, 255];

// ---- The three formats, frame by frame ---------------------------------------------

/// Four frames on a 4 × 4 canvas that between them use every way a GIF
/// frame can end: kept, cleared, put back.
fn gif_file() -> Vec<u8> {
    let mut part = GifFrame::new(0, 0, 2, 2, 2).dispose(DisposalMethod::Previous);
    // Its last pixel is the transparent one: it leaves what is there.
    part.indices[3] = 4;
    part.transparent = Some(4);
    gif(
        4,
        4,
        &[
            GifFrame::new(0, 0, 4, 4, 0).delay(0),
            GifFrame::new(1, 1, 2, 2, 1)
                .dispose(DisposalMethod::Background)
                .delay(5),
            part.delay(20),
            GifFrame::new(2, 2, 2, 2, 3).delay(1),
        ],
    )
}

const GIF_FRAMES: [[&str; 4]; 4] = [
    ["RRRR", "RRRR", "RRRR", "RRRR"],
    ["RRRR", "RGGR", "RGGR", "RRRR"],
    // The green square was cleared; blue is drawn around its one
    // transparent pixel.
    ["BBRR", "B..R", "R..R", "RRRR"],
    // What the blue frame covered is put back as it was before it.
    ["RRRR", "R..R", "R.WW", "RRWW"],
];

#[test]
fn a_gif_is_composited_frame_by_frame_with_its_disposals() {
    let file = gif_file();
    let t = timeline(&file).unwrap();
    assert_eq!(
        (t.format, t.width, t.height, t.frames()),
        (Format::Gif, 4, 4, 4)
    );
    // Hundredths of a second; under 20 ms plays as 100 ms.
    let starts: Vec<f64> = (0..4).map(|i| t.start(i)).collect();
    for (got, want) in starts.iter().zip([0.0, 0.1, 0.15, 0.35]) {
        assert!((got - want).abs() < 1e-9, "{starts:?}");
    }
    assert!((t.duration - 0.45).abs() < 1e-9);
    // One frame asks for what it covered to be put back: 2 × 2 pixels.
    assert_eq!(t.restore, 16);

    let mut s = stream(&file);
    for (i, want) in GIF_FRAMES.iter().enumerate() {
        assert_eq!(
            frame(&mut s, i as u32),
            (want.map(String::from).to_vec(), 1),
            "frame {i}"
        );
    }
    // The loop wraps to the first frame, on a clear canvas.
    assert_eq!(frame(&mut s, 0).0, GIF_FRAMES[0]);
    // Further ahead: the frames between are decoded and dropped.
    assert_eq!(
        frame(&mut s, 3),
        (GIF_FRAMES[3].map(String::from).to_vec(), 3)
    );
    // Behind its place: the file is started again.
    assert_eq!(
        frame(&mut s, 2),
        (GIF_FRAMES[2].map(String::from).to_vec(), 3)
    );
    // Where it is: nothing is decoded.
    assert_eq!(
        frame(&mut s, 2),
        (GIF_FRAMES[2].map(String::from).to_vec(), 0)
    );
    // Past the end: the file holds no such frame.
    assert!(matches!(s.seek(4), Err(Error::Broken(_))));
    // And it goes on after that.
    assert_eq!(frame(&mut s, 1).0, GIF_FRAMES[1]);
}

#[test]
fn a_gif_frame_may_bring_its_own_palette_and_reach_past_the_canvas() {
    let mut own = GifFrame::new(2, 2, 4, 4, 1);
    // Index 1 is white in this frame's palette.
    own.palette = Some(vec![0, 0, 0, 255, 255, 255, 0, 0, 0, 0, 0, 0]);
    let file = gif(4, 4, &[GifFrame::new(0, 0, 4, 4, 2), own]);
    let mut s = stream(&file);
    assert_eq!(frame(&mut s, 0).0, ["BBBB"; 4]);
    // The part of it that is on the canvas is drawn.
    assert_eq!(frame(&mut s, 1).0, ["BBBB", "BBBB", "BBWW", "BBWW"]);
}

fn apng_frames() -> Vec<ApngFrame> {
    let mut part = ApngFrame::new(0, 0, 2, 2, BLUE).dispose(2).delay(1, 5);
    part.rgba[12..16].copy_from_slice(&CLEAR);
    vec![
        // A denominator of 0 is 100; a delay of 0 plays as 100 ms.
        ApngFrame::new(0, 0, 4, 4, RED).delay(0, 0),
        ApngFrame::new(1, 1, 2, 2, [0, 255, 0, 128])
            .blend(1)
            .dispose(1)
            .delay(1, 20),
        part,
        ApngFrame::new(2, 2, 2, 2, WHITE).blend(1).delay(1, 100),
    ]
}

const APNG_FRAMES: [[&str; 4]; 4] = [
    ["RRRR", "RRRR", "RRRR", "RRRR"],
    // Half-clear green blended over the red.
    ["RRRR", "R??R", "R??R", "RRRR"],
    // "Source": the frame replaces what is there, its clear pixel too.
    ["BBRR", "B..R", "R..R", "RRRR"],
    ["RRRR", "R..R", "R.WW", "RRWW"],
];

#[test]
fn an_animated_png_is_composited_with_its_blends_and_disposals() {
    // The same frames, with the file's first image a frame and with one
    // that is only for programs that don't animate.
    for default in [None, Some(solid(4, 4, BLUE))] {
        let file = apng(4, 4, default.as_deref(), &apng_frames(), None);
        let t = timeline(&file).unwrap();
        assert_eq!((t.format, t.frames()), (Format::Png, 4));
        for (i, want) in [0.0, 0.1, 0.15, 0.35].iter().enumerate() {
            assert!((t.start(i as u32) - want).abs() < 1e-9, "start {i}");
        }
        assert!((t.duration - 0.45).abs() < 1e-9);
        assert_eq!(t.restore, 16);

        let mut s = stream(&file);
        for (i, want) in APNG_FRAMES.iter().enumerate() {
            let (pixels, decoded) = s.seek(i as u32).unwrap();
            assert_eq!(letters(&pixels, 4), *want, "frame {i}");
            assert_eq!(decoded, 1);
            if i == 1 {
                assert_eq!(pixel(&pixels, 4, 1, 1), MIXED);
            }
        }
        // Round again, and backwards.
        assert_eq!(frame(&mut s, 0).0, APNG_FRAMES[0]);
        assert_eq!(
            frame(&mut s, 2),
            (APNG_FRAMES[2].map(String::from).to_vec(), 2)
        );
        assert_eq!(
            frame(&mut s, 1),
            (APNG_FRAMES[1].map(String::from).to_vec(), 2)
        );
        assert!(matches!(s.seek(4), Err(Error::Broken(_))));
    }
}

#[test]
fn an_animated_pngs_first_frame_cant_ask_for_the_previous_picture() {
    // "Previous" on the first frame means "background": the frame is
    // cleared, since nothing came before it.
    let frames = [
        ApngFrame::new(0, 0, 2, 2, RED).dispose(2),
        ApngFrame::new(2, 2, 2, 2, GREEN),
    ];
    let mut s = stream(&apng(4, 4, None, &frames, None));
    assert_eq!(frame(&mut s, 0).0, ["RR..", "RR..", "....", "...."]);
    assert_eq!(frame(&mut s, 1).0, ["....", "....", "..GG", "..GG"]);
}

fn webp_file() -> Vec<u8> {
    let mut part = WebpFrame::new(0, 0, 2, 2, BLUE).replace().millis(200);
    part.rgba[12..16].copy_from_slice(&CLEAR);
    webp_anim(
        4,
        4,
        &[
            WebpFrame::new(0, 0, 4, 4, RED).replace().millis(0),
            WebpFrame::new(2, 2, 2, 2, [0, 255, 0, 128])
                .dispose()
                .millis(50),
            part,
            WebpFrame::new(2, 2, 2, 2, WHITE).millis(10),
            // Clear blended on: nothing changes. Clear replacing: nothing
            // is left.
            WebpFrame::new(0, 0, 4, 4, CLEAR),
            WebpFrame::new(0, 0, 4, 4, CLEAR).replace(),
        ],
    )
}

const WEBP_FRAMES: [[&str; 4]; 6] = [
    ["RRRR", "RRRR", "RRRR", "RRRR"],
    ["RRRR", "RRRR", "RR??", "RR??"],
    // The green frame asked to be disposed of: its rectangle is clear.
    ["BBRR", "B.RR", "RR..", "RR.."],
    ["BBRR", "B.RR", "RRWW", "RRWW"],
    ["BBRR", "B.RR", "RRWW", "RRWW"],
    ["....", "....", "....", "...."],
];

#[test]
fn an_animated_webp_is_composited_with_its_blends_and_disposals() {
    let file = webp_file();
    let t = timeline(&file).unwrap();
    assert_eq!(
        (t.format, t.width, t.height, t.frames()),
        (Format::Webp, 4, 4, 6)
    );
    // Milliseconds; 0 and 10 play as 100.
    for (i, want) in [0.0, 0.1, 0.15, 0.35, 0.45, 0.55].iter().enumerate() {
        assert!((t.start(i as u32) - want).abs() < 1e-9, "start {i}");
    }
    assert!((t.duration - 0.65).abs() < 1e-9);
    let mut s = stream(&file);
    for (i, want) in WEBP_FRAMES.iter().enumerate() {
        let (pixels, decoded) = s.seek(i as u32).unwrap();
        assert_eq!(letters(&pixels, 4), *want, "frame {i}");
        assert_eq!(decoded, 1);
        if i == 1 {
            let mixed = pixel(&pixels, 4, 2, 2);
            assert_eq!(mixed, MIXED);
        }
    }
    assert_eq!(frame(&mut s, 0).0, WEBP_FRAMES[0]);
    assert_eq!(
        frame(&mut s, 3),
        (WEBP_FRAMES[3].map(String::from).to_vec(), 3)
    );
    assert_eq!(
        frame(&mut s, 2),
        (WEBP_FRAMES[2].map(String::from).to_vec(), 3)
    );
    assert!(matches!(s.seek(6), Err(Error::Broken(_))));
}

#[test]
fn a_lossy_animated_webp_plays() {
    let file = fixture("lossy-anim.webp");
    let t = timeline(&file).unwrap();
    assert_eq!((t.width, t.height, t.frames()), (32, 24, 3));
    assert!((t.duration - 0.3).abs() < 1e-9);
    let mut s = stream(&file);
    for (i, want) in [[200u8, 50, 20], [20, 200, 50], [20, 50, 200]]
        .iter()
        .enumerate()
    {
        let (pixels, _) = s.seek(i as u32).unwrap();
        assert_eq!(pixels.len(), 32 * 24 * 4);
        let p = pixel(&pixels, 32, 16, 12);
        let close = p[..3].iter().zip(want).all(|(g, w)| g.abs_diff(*w) <= 16);
        assert!(close && p[3] == 255, "frame {i}: {p:?}");
    }
}

#[test]
fn animations_by_other_programs_encoders_play() {
    // A GIF by ImageMagick: red, green, blue, shown 0.1 s, 0.2 s, 0.3 s.
    let file = fixture("anim.gif");
    let t = timeline(&file).unwrap();
    assert_eq!(
        (t.format, t.width, t.height, t.frames()),
        (Format::Gif, 8, 6, 3)
    );
    for (i, want) in [0.0, 0.1, 0.3].iter().enumerate() {
        assert!((t.start(i as u32) - want).abs() < 1e-9, "start {i}");
    }
    assert!((t.duration - 0.6).abs() < 1e-9);
    let colours = [[200u8, 50, 20, 255], [20, 200, 50, 255], [20, 50, 200, 255]];
    let mut s = stream(&file);
    for (i, want) in colours.iter().enumerate() {
        let (pixels, _) = s.seek(i as u32).unwrap();
        assert_eq!(pixel(&pixels, 8, 4, 3), *want, "frame {i}");
    }
    // An animated PNG by ffmpeg: the same three, a tenth of a second each.
    let file = fixture("anim.png");
    let t = timeline(&file).unwrap();
    assert_eq!(
        (t.format, t.width, t.height, t.frames()),
        (Format::Png, 8, 6, 3)
    );
    assert!((t.duration - 0.3).abs() < 1e-9);
    let mut s = stream(&file);
    for (i, want) in colours.iter().enumerate() {
        let (pixels, _) = s.seek(i as u32).unwrap();
        assert_eq!(pixel(&pixels, 8, 4, 3), *want, "frame {i}");
    }
}

#[test]
fn a_still_is_an_animation_of_one_frame_and_a_jpeg_is_none() {
    let four = [RED, GREEN, BLUE, WHITE].concat();
    let sixteen = four.repeat(4);
    for file in [
        rgba_png(4, 4, &sixteen),
        webp(4, 4, &sixteen),
        gif(4, 4, &[GifFrame::new(0, 0, 4, 4, 2)]),
    ] {
        let t = timeline(&file).unwrap();
        assert_eq!((t.frames(), t.duration, t.moves()), (1, 0.0, false));
        assert_eq!(t.next_due(5.0), None);
        let mut s = stream(&file);
        let (pixels, _) = s.seek(0).unwrap();
        assert_eq!(pixels.len(), 64);
        assert!(s.seek(1).is_err());
    }
    assert_eq!(
        timeline(&fixture("baseline.jpg")),
        Err(Error::NotAnimation(Format::Jpeg))
    );
    assert_eq!(timeline(b"neither"), Err(Error::Unknown));
}

// ---- Hostile files -----------------------------------------------------------------

#[test]
fn a_small_file_of_very_many_frames_is_refused_by_its_count() {
    // 3000 one-pixel frames in under 100 KiB: decoded whole it would be
    // nothing, but as an animation's frames it is over the cap.
    let frames: Vec<GifFrame> = (0..3000u32)
        .map(|i| GifFrame::new(0, 0, 1, 1, (i % 4) as u8))
        .collect();
    let file = gif(1, 1, &frames);
    assert!(file.len() < 100_000, "{}", file.len());
    let started = Instant::now();
    assert_eq!(timeline(&file), Err(Error::Frames { max: 2048 }));
    assert!(started.elapsed() < Duration::from_millis(500));
    // At the cap it is taken; one more is not.
    let file = gif(1, 1, &frames[..2048]);
    assert_eq!(timeline(&file).unwrap().frames(), 2048);
    let file = gif(1, 1, &frames[..2049]);
    assert_eq!(timeline(&file), Err(Error::Frames { max: 2048 }));
    // Every format counts.
    let two = AnimLimits {
        frames: 2,
        ..limits()
    };
    for file in [
        gif_file(),
        apng(4, 4, None, &apng_frames(), None),
        webp_file(),
    ] {
        assert_eq!(open(&Bytes::new(file), &two), Err(Error::Frames { max: 2 }));
    }
    // A PNG announcing more frames than any file may have.
    let file = apng(4, 4, None, &apng_frames(), Some(4_000_000_000));
    assert_eq!(timeline(&file), Err(Error::Frames { max: 2048 }));
}

#[test]
fn a_canvas_over_the_cap_is_refused_from_the_header() {
    let mut huge = gif_file();
    huge[6..8].copy_from_slice(&60_000u16.to_le_bytes());
    huge[8..10].copy_from_slice(&60_000u16.to_le_bytes());
    assert_eq!(
        timeline(&huge),
        Err(Error::Size {
            width: 60_000,
            height: 60_000,
            max: 1024
        })
    );
    let mut wide = webp_file();
    wide[24..27].copy_from_slice(&1024u32.to_le_bytes()[..3]);
    assert!(matches!(
        timeline(&wide),
        Err(Error::Size { width: 1025, .. })
    ));
    let mut tall = apng(4, 4, None, &apng_frames(), None);
    tall[20..24].copy_from_slice(&1025u32.to_be_bytes());
    assert!(matches!(
        timeline(&tall),
        Err(Error::Size { height: 1025, .. })
    ));
}

#[test]
fn a_file_that_holds_other_than_it_announces_is_refused() {
    // More frames announced than held, and fewer.
    for announced in [5, 3, 0] {
        let file = apng(4, 4, None, &apng_frames(), Some(announced));
        assert!(
            matches!(timeline(&file), Err(Error::Broken(_))),
            "{announced}"
        );
    }
    // A WebP frame larger than the picture: found when it is played.
    let mut file = webp_file();
    let anmf = file.windows(4).position(|w| w == b"ANMF").unwrap();
    file[anmf + 8 + 6..anmf + 8 + 9].copy_from_slice(&600u32.to_le_bytes()[..3]);
    if timeline(&file).is_ok() {
        assert!(matches!(stream(&file).seek(0), Err(Error::Broken(_))));
    }
    // An animated WebP cut short in its last frame.
    let file = webp_file();
    let cut = &file[..file.len() - 9];
    if let Ok(t) = timeline(cut) {
        let last = t.frames() - 1;
        let mut s = stream(cut);
        // Whatever it counted, each frame decodes or is refused.
        let _ = s.seek(last);
    }
    // A truncated GIF: its frames can't be counted.
    let file = gif_file();
    assert!(timeline(&file[..file.len() / 2]).is_err());
}

#[test]
fn malformed_animations_never_panic_past_the_shield() {
    let mut rng = Rng(0x2545_f491);
    let files = [
        gif_file(),
        apng(4, 4, None, &apng_frames(), None),
        apng(4, 4, Some(&solid(4, 4, BLUE)), &apng_frames(), None),
        webp_file(),
        fixture("lossy-anim.webp"),
    ];
    let (mut played, mut refused) = (0, 0);
    for file in &files {
        for _ in 0..300 {
            let mut bad = file.clone();
            for _ in 0..1 + rng.below(3) {
                let at = rng.below(bad.len());
                bad[at] = rng.next() as u8;
            }
            let bytes = Bytes::new(bad);
            let Ok(t) = open(&bytes, &limits()) else {
                refused += 1;
                continue;
            };
            assert!(t.width <= 1024 && t.height <= 1024 && t.frames() <= 2048);
            let Ok(mut s) = Stream::open(bytes, t.format, t.width, t.height) else {
                refused += 1;
                continue;
            };
            // Every frame, round the loop and back to the first.
            let size = t.width as usize * t.height as usize * 4;
            let frames = (0..t.frames().min(8)).chain([0]);
            let whole = frames
                .map(|i| gmnb_media::shielded(|| s.seek(i)))
                .inspect(|r| {
                    if let Err(Error::Panicked(what)) = r {
                        panic!("a decoder panicked: {what}");
                    }
                })
                .all(|r| r.is_ok_and(|(pixels, _)| pixels.len() == size));
            if whole {
                played += 1;
            } else {
                refused += 1;
            }
        }
    }
    assert!(
        played > 0 && refused > 0,
        "{played} played, {refused} refused"
    );
}

// ---- The player --------------------------------------------------------------------

#[test]
fn the_player_shows_frames_in_order_and_round_the_loop() {
    let file = gif_file();
    let (mut p, wake) = player();
    add(&mut p, 7, &file);
    // The first frame is there at once, decoded by the caller: no thread.
    let first = p.frame(7, 0, 0.0).unwrap();
    assert_eq!(letters(&first.pixels, 4), GIF_FRAMES[0]);
    assert_eq!((first.index, first.width, first.height), (0, 4, 4));
    assert!(!p.stats().thread);

    // Frame 1 is neither shown nor ahead: asked for, the first drawn
    // meanwhile.
    let seen = wake.count();
    assert_eq!(p.frame(7, 1, 0.1).unwrap().index, 0);
    assert!(p.stats().thread);
    wake.after(seen);
    // It arrived: whoever drew the animation draws again, and gets it.
    assert_eq!(p.poll(), [7]);
    let got = p.frame(7, 1, 0.1).unwrap();
    assert_eq!(
        (got.index, letters(&got.pixels, 4)),
        (1, GIF_FRAMES[1].map(String::from).to_vec())
    );

    // The frame after it was asked for at once: by the time it is due it
    // is ahead, and drawing it asks for the next. No draw waits again.
    for pass in 0..3 {
        for i in [2u32, 3, 0, 1] {
            let changed = settle(&mut p, &wake);
            assert!(
                changed.is_empty(),
                "pass {pass}, before frame {i}: {changed:?}"
            );
            let got = p.frame(7, i, 1.0).unwrap();
            assert_eq!(got.index, i, "pass {pass}");
            assert_eq!(
                letters(&got.pixels, 4),
                GIF_FRAMES[i as usize],
                "pass {pass}, frame {i}"
            );
        }
    }
    // An unknown animation, and a frame past the end (the last is held).
    assert!(p.frame(8, 0, 0.0).is_none());
    settle(&mut p, &wake);
    p.frame(7, 99, 2.0);
    settle(&mut p, &wake);
    assert_eq!(p.frame(7, 99, 2.0).unwrap().index, 3);
    assert!(p.take_notes().is_empty());
}

#[test]
fn every_format_plays_through_the_player_as_it_decodes_alone() {
    for (file, want) in [
        (gif_file(), GIF_FRAMES.to_vec()),
        (apng(4, 4, None, &apng_frames(), None), APNG_FRAMES.to_vec()),
        (webp_file(), WEBP_FRAMES.to_vec()),
    ] {
        let (mut p, wake) = player();
        let t = add(&mut p, 1, &file);
        for round in 0..2 {
            for i in 0..t.frames() {
                p.frame(1, i, 0.0);
                settle(&mut p, &wake);
                let got = p.frame(1, i, 0.0).unwrap();
                assert_eq!(got.index, i);
                assert_eq!(
                    letters(&got.pixels, 4),
                    want[i as usize],
                    "round {round}, frame {i}"
                );
            }
        }
        // Skipping, and going back.
        for i in [3, 1, 2, 0, 3] {
            p.frame(1, i, 0.0);
            settle(&mut p, &wake);
            let got = p.frame(1, i, 0.0).unwrap();
            assert_eq!(letters(&got.pixels, 4), want[i as usize], "frame {i}");
        }
    }
}

#[test]
fn at_rest_the_thread_is_never_woken() {
    let (mut p, wake) = player();
    add(&mut p, 1, &gif_file());
    // Playing: frames are asked for and the thread works.
    for i in [1, 2, 3, 0, 1, 2] {
        p.frame(1, i, 0.0);
        settle(&mut p, &wake);
    }
    let playing = p.stats();
    assert!(playing.thread && playing.wakeups > 0 && playing.decoded > 0);
    // The window settles: the picture is replayed (the same frame wanted
    // again and again) and nothing new is asked for.
    settle(&mut p, &wake);
    let before = p.stats();
    for _ in 0..200 {
        assert_eq!(p.frame(1, 2, 9.0).unwrap().index, 2);
        assert!(p.first(1).is_some());
        assert!(p.poll().is_empty());
        std::thread::sleep(Duration::from_millis(2));
    }
    let after = p.stats();
    assert_eq!(
        after.wakeups, before.wakeups,
        "the thread was woken at rest"
    );
    assert_eq!(after.decoded, before.decoded);
    assert_eq!(after.busy, before.busy);
    assert!(!after.ended);
}

#[test]
fn reduced_motion_and_stills_never_start_the_thread() {
    let (mut p, wake) = player();
    add(&mut p, 1, &gif_file());
    add(&mut p, 2, &rgba_png(4, 4, &solid(4, 4, GREEN)));
    let made = wake.count();
    for _ in 0..50 {
        // Reduced motion, a preview, a tile: the first frame.
        assert_eq!(p.first(1).unwrap().index, 0);
        assert_eq!(p.frame(1, 0, 3.0).unwrap().index, 0);
        // A still has one frame, whichever is asked for.
        assert_eq!(p.frame(2, 0, 3.0).unwrap().index, 0);
        assert_eq!(p.frame(2, 5, 3.0).unwrap().index, 0);
        assert!(p.poll().is_empty());
    }
    let stats = p.stats();
    assert!(!stats.thread, "{stats:?}");
    assert_eq!((stats.wakeups, stats.decoded), (0, 0));
    assert_eq!(wake.count(), made);
    assert_eq!(p.slow(3.0), 1.0);
}

#[test]
fn the_first_frame_stands_by_while_another_plays() {
    // A preview tile draws the first frame while a layer plays the
    // animation: the tile's frame costs nothing and moves nothing.
    let (mut p, wake) = player();
    add(&mut p, 1, &gif_file());
    p.frame(1, 2, 0.0);
    settle(&mut p, &wake);
    let before = p.stats();
    for _ in 0..20 {
        assert_eq!(letters(&p.first(1).unwrap().pixels, 4), GIF_FRAMES[0]);
        assert_eq!(p.frame(1, 2, 0.0).unwrap().index, 2);
    }
    assert_eq!(p.stats(), before);
}

#[test]
fn the_thread_ends_with_its_player() {
    let (mut p, wake) = player();
    add(&mut p, 1, &gif_file());
    p.frame(1, 1, 0.0);
    settle(&mut p, &wake);
    let watch = p.watch();
    assert!(watch.stats().thread && !watch.stats().ended);
    drop(p);
    let started = Instant::now();
    while !watch.stats().ended {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the thread lives on"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    // A player that never played had no thread to end.
    let (p, _) = player();
    let watch = p.watch();
    drop(p);
    std::thread::sleep(Duration::from_millis(30));
    assert!(!watch.stats().thread && !watch.stats().ended);
}

/// A picture with something in it for a decoder to do: `side × side`,
/// each frame different.
fn busy_frames(side: u32, count: u32) -> Vec<ApngFrame> {
    let mut rng = Rng(0x1234_5678);
    (0..count)
        .map(|_| {
            let mut f = ApngFrame::new(0, 0, side, side, RED).delay(1, 100);
            for p in f.rgba.as_chunks_mut::<4>().0 {
                // Runs of a colour, so that it compresses a little.
                if rng.below(4) == 0 {
                    let c = rng.next();
                    *p = [c as u8, (c >> 8) as u8, (c >> 16) as u8, 255];
                } else {
                    *p = [rng.0 as u8, 80, 160, 255];
                }
            }
            f
        })
        .collect()
}

#[test]
fn the_thread_takes_a_quarter_of_a_core_at_most() {
    let file = apng(192, 192, None, &busy_frames(192, 6), None);
    let (mut p, wake) = player();
    let t = add(&mut p, 1, &file);
    // As fast as frames can be had, for a while.
    let started = Instant::now();
    let mut i = 0;
    let mut shown = 0u64;
    while started.elapsed() < Duration::from_millis(1200) {
        i = t.after(i);
        let seen = wake.count();
        if p.frame(1, i, started.elapsed().as_secs_f64())
            .unwrap()
            .index
            != i
        {
            wake.after(seen);
            p.poll();
        }
        shown += 1;
    }
    let elapsed = started.elapsed();
    let stats = p.stats();
    assert!(
        shown > 3 && stats.decoded >= shown / 2,
        "{shown} shown, {stats:?}"
    );
    // Processor time against the clock on the wall: a quarter, and the
    // little the first frame was let off.
    let share = stats.busy.as_secs_f64() / elapsed.as_secs_f64();
    assert!(share <= 0.27, "{:.1}% of a core ({stats:?})", share * 100.0);
    // It rested between frames. (How long is not asserted: a rest the
    // thread didn't need, because the next frame was asked for late, is
    // not counted.)
    assert!(stats.rested > Duration::ZERO, "{stats:?}");
    assert!(p.slow(elapsed.as_secs_f64()) >= 1.0);
}

#[test]
fn the_frames_on_the_way_to_a_far_one_are_held_to_a_quarter_core_too() {
    // A theme's own clock can ask for any frame: one far ahead, or one
    // behind (the file is started again). Every frame on the way is
    // decoded, and the thread rests after each as after any other.
    let file = apng(192, 192, None, &busy_frames(192, 60), None);
    let (mut p, wake) = player();
    let t = add(&mut p, 1, &file);
    let last = t.frames() - 1;
    let started = Instant::now();
    let seen = wake.count();
    p.frame(1, last, 0.0);
    wake.after(seen);
    let elapsed = started.elapsed();
    p.poll();
    assert_eq!(p.frame(1, last, 0.0).unwrap().index, last);
    let stats = p.stats();
    // (The thread decodes the first frame too: it starts the file anew.)
    assert_eq!(stats.decoded, u64::from(last) + 1, "{stats:?}");
    let share = stats.busy.as_secs_f64() / elapsed.as_secs_f64();
    assert!(
        share <= 0.3,
        "{:.0}% of a core for {} frames on the way ({stats:?})",
        share * 100.0,
        last
    );
}

#[test]
fn a_thread_on_its_way_to_a_far_frame_ends_with_its_player() {
    // A theme unloaded while the thread decodes its way to a frame far
    // ahead: it stops there, not at the frame.
    let file = apng(192, 192, None, &busy_frames(192, 400), None);
    let (mut p, _wake) = player();
    let t = add(&mut p, 1, &file);
    p.frame(1, t.frames() - 1, 0.0);
    std::thread::sleep(Duration::from_millis(30));
    let watch = p.watch();
    assert!(watch.stats().thread && !watch.stats().ended);
    let at_drop = watch.stats();
    drop(p);
    let dropped = Instant::now();
    while !watch.stats().ended {
        std::thread::sleep(Duration::from_millis(1));
    }
    let after = watch.stats();
    // A frame or two more at most (one may be under way).
    assert!(
        after.decoded <= at_drop.decoded + 2 && dropped.elapsed() < Duration::from_millis(250),
        "{} frames decoded and {:?} after its player went ({at_drop:?}, then {after:?})",
        after.decoded - at_drop.decoded,
        dropped.elapsed()
    );
}

/// A source that takes `cost` of processor time a frame.
struct Heavy(Duration);

impl Source for Heavy {
    fn seek(&mut self, _index: u32) -> Result<(Vec<u8>, u32), Error> {
        let started = gmnb_media::thread_time().expect("a thread's processor time");
        let mut x = 1u64;
        while gmnb_media::thread_time().unwrap() - started < self.0 {
            for i in 0..2000u64 {
                x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(i);
            }
        }
        Ok((vec![x as u8 | 1; 64], 1))
    }
}

#[test]
fn an_animation_too_heavy_for_a_quarter_core_is_told_how_much_slower_to_play() {
    // 10 ms of processor time a frame, a frame every 20 ms: half a core
    // at full speed, twice the thread's share.
    let (mut p, wake) = player();
    let t = Arc::new(Timeline::of_delays(Format::Gif, 4, 4, &[0.02; 40]));
    p.open_with(
        1,
        t.clone(),
        vec![0; 64],
        Box::new(|| Ok(Box::new(Heavy(Duration::from_millis(10))) as Box<dyn Source>)),
    );
    // Nothing has been measured yet: full speed.
    assert_eq!(p.slow(0.0), 1.0);
    let started = Instant::now();
    // A few frames are no measure (a file's first frames cost more than
    // the rest): it is believed after two dozen.
    for i in 1..=6 {
        p.frame(1, i, started.elapsed().as_secs_f64());
        settle(&mut p, &wake);
    }
    assert_eq!(p.slow(started.elapsed().as_secs_f64()), 1.0);
    assert!(p.take_notes().is_empty());
    for i in 7..=30 {
        p.frame(1, i, started.elapsed().as_secs_f64());
        settle(&mut p, &wake);
    }
    let now = started.elapsed().as_secs_f64();
    let slow = p.slow(now);
    assert!((1.7..=2.6).contains(&slow), "{slow}");
    // Said once.
    let notes = p.take_notes();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("play slower"), "{notes:?}");
    p.slow(now);
    assert!(p.take_notes().is_empty());
    // The thread kept to its quarter meanwhile.
    let stats = p.stats();
    let share = stats.busy.as_secs_f64() / started.elapsed().as_secs_f64();
    assert!(share <= 0.3, "{:.1}% of a core ({stats:?})", share * 100.0);
    // Once it is no longer played it no longer counts.
    assert_eq!(p.slow(now + 5.0), 1.0);
}

/// A source that gives a frame of `index` bytes until `bad`, where it
/// fails in the given way; it counts its calls.
struct Failing {
    bad: u32,
    panics: bool,
    calls: Arc<Mutex<u32>>,
}

impl Source for Failing {
    fn seek(&mut self, index: u32) -> Result<(Vec<u8>, u32), Error> {
        *self.calls.lock().unwrap() += 1;
        if index >= self.bad {
            if self.panics {
                panic!("a decoder's index out of bounds");
            }
            return Err(Error::Broken("the file ended".into()));
        }
        Ok((vec![index as u8; 4 * 4 * 4], 1))
    }
}

#[test]
fn an_animation_that_fails_stands_on_the_frame_it_reached_and_is_not_tried_again() {
    for panics in [false, true] {
        let (mut p, wake) = player();
        let calls = Arc::new(Mutex::new(0));
        let t = Arc::new(Timeline::of_delays(Format::Gif, 4, 4, &[0.1; 5]));
        let source = Failing {
            bad: 2,
            panics,
            calls: calls.clone(),
        };
        p.open_with(
            1,
            t,
            vec![0; 64],
            Box::new(move || Ok(Box::new(source) as Box<dyn Source>)),
        );
        // A good file beside it.
        add(&mut p, 2, &gif_file());

        p.frame(1, 1, 0.0);
        let changed = settle(&mut p, &wake);
        assert_eq!(changed, [1]);
        assert_eq!(p.frame(1, 1, 0.0).unwrap().pixels[0], 1);
        // Frame 2 was asked for ahead, and failed: one note.
        let notes = p.take_notes();
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("stopped at frame 2"), "{notes:?}");
        if panics {
            assert!(notes[0].contains("index out of bounds"), "{notes:?}");
        }
        // It stands on frame 1 whatever is wanted, and the thread is not
        // asked again: no retry, however often it is drawn.
        let tried = *calls.lock().unwrap();
        let before = p.stats();
        for i in [2, 3, 4, 0, 2] {
            assert_eq!(p.frame(1, i, 1.0).unwrap().index, 1);
            assert!(settle(&mut p, &wake).is_empty());
        }
        assert_eq!(*calls.lock().unwrap(), tried);
        assert_eq!(p.stats().wakeups, before.wakeups);
        assert!(p.take_notes().is_empty());
        assert!(!p.stats().ended, "the thread died of one file");

        // The other animation plays on, on the same thread.
        p.frame(2, 1, 1.0);
        assert_eq!(settle(&mut p, &wake), [2]);
        let got = p.frame(2, 1, 1.0).unwrap();
        assert_eq!(letters(&got.pixels, 4), GIF_FRAMES[1]);
    }
}

#[test]
fn a_file_whose_later_frames_are_broken_fails_when_they_are_reached() {
    // The timeline walk inflates nothing, so a frame whose data is
    // spoiled is found only when it is decoded.
    let mut file = apng(4, 4, None, &apng_frames(), None);
    let second = file
        .windows(4)
        .enumerate()
        .filter(|(_, w)| *w == b"fdAT")
        .map(|(at, _)| at)
        .nth(1)
        .expect("a third frame");
    for b in &mut file[second + 8..second + 16] {
        *b ^= 0xff;
    }
    let (mut p, wake) = player();
    let t = add(&mut p, 1, &file);
    assert_eq!(t.frames(), 4);
    p.frame(1, 1, 0.0);
    assert_eq!(settle(&mut p, &wake), [1]);
    assert_eq!(p.frame(1, 1, 0.0).unwrap().index, 1);
    // Frame 2 (asked for ahead) is the broken one.
    let notes = p.take_notes();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(p.frame(1, 2, 0.5).unwrap().index, 1);
    assert_eq!(p.frame(1, 3, 0.5).unwrap().index, 1);
    assert!(settle(&mut p, &wake).is_empty());
    // A source that can't even be made.
    let (mut p, wake) = player();
    let t = Arc::new(Timeline::of_delays(Format::Gif, 4, 4, &[0.1; 3]));
    p.open_with(
        9,
        t,
        vec![0; 64],
        Box::new(|| Err(Error::Broken("no such file".into()))),
    );
    p.frame(9, 1, 0.0);
    assert!(settle(&mut p, &wake).is_empty());
    assert_eq!(p.frame(9, 1, 0.0).unwrap().index, 0);
    assert_eq!(p.take_notes().len(), 1);
}

#[test]
fn a_closed_animation_is_forgotten() {
    let (mut p, wake) = player();
    add(&mut p, 1, &gif_file());
    p.frame(1, 2, 0.0);
    settle(&mut p, &wake);
    assert!(p.has(1));
    p.close(1);
    assert!(!p.has(1) && p.frame(1, 0, 0.0).is_none() && p.first(1).is_none());
    // Taken up again under the same number, it starts afresh.
    add(&mut p, 1, &webp_file());
    p.frame(1, 1, 0.0);
    settle(&mut p, &wake);
    assert_eq!(
        letters(&p.frame(1, 1, 0.0).unwrap().pixels, 4),
        WEBP_FRAMES[1]
    );
}
