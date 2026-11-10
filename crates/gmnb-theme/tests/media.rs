//! Pictures that move, pictures a theme fills, and fields, as a theme
//! reaches them (docs/theming-media.md, "Pictures"): what the `gmnb` and
//! `draw` calls record, every cap hit on purpose, an animation's frames
//! under a clock the test holds, and the swirl demonstration.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gmnb_theme::draw::{Filter, Op};
use gmnb_theme::layer::{Drawn, Failure, Frame, LayerKind};
use gmnb_theme::media::field as pass;
use gmnb_theme::picture::{ImageId, PictureKind};
use gmnb_theme::theme::{OptionKind, Source, Theme};
use gmnb_theme::{Ctx, Limits, OptionValue, ThemeError};

/// Limits whose deadlines can't be what fails a test that isn't about
/// time: the machine these run on is shared.
fn patient() -> Limits {
    Limits {
        load: Duration::from_secs(20),
        style: Duration::from_secs(20),
        frame: Duration::from_secs(20),
        place: Duration::from_secs(20),
        preview: Duration::from_secs(20),
        ..Limits::default()
    }
}

struct Dir(std::path::PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(name: &str) -> Dir {
    let dir = std::env::temp_dir().join(format!("gmnb-media-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Dir(dir)
}

impl Dir {
    fn put(&self, name: &str, bytes: &[u8]) -> &Dir {
        std::fs::write(self.0.join(name), bytes).unwrap();
        self
    }

    fn load(&self, source: &str) -> Theme {
        std::fs::write(self.0.join("theme.luau"), source).unwrap();
        match Theme::load(Source::folder("t", &self.0), patient()) {
            Ok(theme) => theme,
            Err(e) => panic!("{e}"),
        }
    }

    /// A theme whose background layer's `draw` is `body` (with `draw`
    /// and `frame` in scope), after `top` at the file's top level.
    fn background(&self, top: &str, body: &str) -> Theme {
        self.load(&format!(
            "{top}\nreturn {{ api = 1, name = 'T', layers = {{ background = {{ draw = function(draw, frame)\n{body}\nend }} }} }}"
        ))
    }
}

/// A picture by another program's encoder: the media crate's fixtures.
fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../gmnb-media/tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A GIF written by hand: frames of one pixel (red, blue, red, …) at the
/// corner of a `w × h` canvas, each shown `delays[i]` hundredths of a
/// second.
fn tiny_gif(w: u16, h: u16, delays: &[u16]) -> Vec<u8> {
    let mut out = b"GIF89a".to_vec();
    out.extend(w.to_le_bytes());
    out.extend(h.to_le_bytes());
    // A table of two colours for the whole file: red, blue.
    out.extend([0x80, 0, 0, 255, 0, 0, 0, 0, 255]);
    for (i, delay) in delays.iter().enumerate() {
        // How the frame is shown: left in place, for this long.
        out.extend([0x21, 0xf9, 4, 0]);
        out.extend(delay.to_le_bytes());
        out.extend([0, 0]);
        // Where: one pixel at the corner.
        out.extend([0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
        // That pixel, compressed: "clear, the colour, end" in 3-bit codes.
        out.extend([2, 2, if i % 2 == 0 { 0x44 } else { 0x4c }, 1, 0]);
    }
    out.push(0x3b);
    out
}

/// A PNG's first bytes for a picture of this size: enough for a theme to
/// be told a size.
fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
    out.extend(width.to_be_bytes());
    out.extend(height.to_be_bytes());
    out.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    out
}

fn at(time: f64) -> Frame<'static> {
    Frame {
        time,
        ..Frame::sized(400.0, 300.0)
    }
}

fn run(theme: &Theme, frame: &Frame) -> Result<Drawn, Failure> {
    theme.draw(LayerKind::Background, &Ctx::default(), frame)
}

fn drawn(theme: &Theme, time: f64) -> Drawn {
    match run(theme, &at(time)) {
        Ok(drawn) => drawn,
        Err(e) => panic!("{e:?}"),
    }
}

fn notes(theme: &Theme) -> Vec<String> {
    theme
        .take_draw_notes()
        .into_iter()
        .map(|n| n.message)
        .collect()
}

/// A clock the test holds, for the allowances.
fn clock(theme: &Theme, start: f64) -> Rc<Cell<f64>> {
    let now = Rc::new(Cell::new(start));
    let read = now.clone();
    theme.api().set_clock(Rc::new(move || read.get()));
    now
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn close(a: Option<f64>, b: f64) -> bool {
    a.is_some_and(|a| (a - b).abs() < 1e-9)
}

// ---- Stills ------------------------------------------------------------------------

#[test]
fn stills_of_all_four_kinds_load_by_their_header_and_decode_when_drawn() {
    let d = dir("stills");
    d.put("a.png", &fixture("still.png"))
        .put("b.jpg", &fixture("baseline.jpg"))
        .put("c.webp", &fixture("lossy.webp"))
        .put("d.gif", &fixture("anim.gif"))
        // Named for what it isn't: a file is what its bytes say.
        .put("e.png", &fixture("progressive.jpg"));
    let theme = d.background(
        r##"
        local a, b, c = gmnb.image("a.png"), gmnb.image("b.jpg"), gmnb.image("c.webp")
        local g, e = gmnb.image("d.gif"), gmnb.image("e.png")
        for _, p in { a, b, c, e } do assert(p.width == 16 and p.height == 12) end
        assert(g.width == 8 and g.height == 6 and g.frames == nil)
        assert(a == gmnb.image("a.png") and a ~= b)
        "##,
        r##"
        draw:image(a, 0, 0, 16, 12)
        draw:image(b, 0, 0, 160, 120, { filter = "nearest" })
        draw:image(c, 0, 0, 16, 12, { filter = "linear", time = 3, loop = false })
        draw:image(g, 0, 0, 8, 6)
        draw:tile(e, 0, 0, 100, 100)
        "##,
    );
    assert!(notes(&theme).is_empty());
    let d0 = drawn(&theme, 0.0);
    // Stills: smooth and clean when drawn smaller, unless told otherwise.
    // What an animation would make of `time` means nothing to them.
    let filters: Vec<Filter> = d0
        .list
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Image { filter, .. } => Some(*filter),
            _ => None,
        })
        .collect();
    assert_eq!(
        filters,
        [
            Filter::Trilinear,
            Filter::Nearest,
            Filter::Linear,
            Filter::Trilinear
        ]
    );
    assert!(matches!(
        d0.list.ops[4],
        Op::Tile {
            image: ImageId(4),
            ..
        }
    ));
    // A still asks for no other frame.
    assert_eq!((d0.again, d0.due), (None, None));
    // The pixels, when the app comes to draw them: premultiplied RGBA of
    // the size the theme was told.
    for (i, (w, h)) in [(16, 12), (16, 12), (16, 12), (8, 6), (16, 12)]
        .iter()
        .enumerate()
    {
        let still = theme.api().still(ImageId(i as u32)).expect("pixels");
        assert_eq!((still.width, still.height), (*w, *h), "picture {i}");
        assert_eq!(still.pixels.len(), (w * h * 4) as usize);
        assert!(matches!(
            theme.api().kind(ImageId(i as u32)),
            Some(PictureKind::File(_))
        ));
    }
    // A GIF as a still is its first frame.
    let gif = theme.api().still(ImageId(3)).unwrap();
    assert_eq!(gif.pixels[..4], [200, 50, 20, 255]);
    assert!(notes(&theme).is_empty());
}

#[test]
fn hostile_stills_draw_nothing_and_are_told_once() {
    let d = dir("hostile");
    let good = fixture("still.png");
    // A header that claims 60 000 × 60 000.
    let mut huge = good.clone();
    huge[16..20].copy_from_slice(&60_000u32.to_be_bytes());
    huge[20..24].copy_from_slice(&60_000u32.to_be_bytes());
    // The wrong first byte.
    let mut magic = good.clone();
    magic[0] = b'X';
    // A WebP whose header says 16 × 12 and whose picture says
    // 16 383 × 16 383.
    let mut liar = fixture("lossy-alpha.webp");
    let chunk = liar.windows(4).position(|w| w == b"VP8 ").unwrap();
    liar[chunk + 14..chunk + 18].copy_from_slice(&[0xff, 0x3f, 0xff, 0x3f]);
    d.put("cut.png", &good[..good.len() / 2])
        .put("huge.png", &huge)
        .put("magic.png", &magic)
        .put("liar.webp", &liar)
        .put("cut.webp", &fixture("lossy.webp")[..40])
        .put("text.gif", b"GIF89a is how it starts and that is all");
    let theme = d.background(
        r##"
        local cut, liar = gmnb.image("cut.png"), gmnb.image("liar.webp")
        -- Their headers are sound: they have a size.
        assert(cut.width == 16 and cut.height == 12 and liar.width == 16)
        -- These are refused from their first bytes or their header.
        for _, file in { "huge.png", "magic.png", "cut.webp", "text.gif" } do
            local p = gmnb.image(file)
            assert(p.width == 0 and p.height == 0, file)
        end
        "##,
        "draw:image(cut, 0, 0, 16, 12) draw:image(liar, 0, 0, 16, 12) draw:image(gmnb.image('huge.png'), 0, 0, 9, 9)",
    );
    let told = notes(&theme);
    for part in [
        "huge.png is 60000×60000",
        "magic.png is not a PNG, JPEG, WebP or GIF picture",
        "cut.webp can't be decoded: the WebP file is cut short",
        "text.gif",
    ] {
        assert!(told.iter().any(|n| n.contains(part)), "{part}: {told:?}");
    }
    assert_eq!(theme.api().pictures().len(), 2);
    // Drawing records the two that have a size; the refused one, nothing.
    assert_eq!(drawn(&theme, 0.0).list.ops.len(), 2);
    assert!(notes(&theme).is_empty());
    // Decoded when the app draws them: neither has pixels. Each keeps its
    // size, draws nothing, and is noted once; it is not tried again.
    for _ in 0..3 {
        assert!(theme.api().still(ImageId(0)).is_none());
        assert!(theme.api().still(ImageId(1)).is_none());
    }
    let told = notes(&theme);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(told[0].contains("cut.png can't be decoded") && told[0].contains("draws nothing"));
    assert!(told[1].contains("liar.webp can't be decoded") && told[1].contains("size"));
    assert!(notes(&theme).is_empty());
}

#[test]
fn a_file_that_fails_has_used_one_of_the_count() {
    // However a theme's hooks are written, the app tries at most sixteen
    // image files, good or bad.
    let d = dir("count");
    d.put("ok.png", &fixture("still.png"));
    let theme = d.background(
        "",
        r##"
        for i = 1, 40 do gmnb.image("missing" .. i .. ".png") end
        local late = gmnb.image("ok.png")
        return late.width
        "##,
    );
    for _ in 0..3 {
        // Sixteen were looked for; the good one came too late.
        assert_eq!(drawn(&theme, 0.0).again, Some(0.0));
    }
    let told = notes(&theme);
    assert_eq!(
        told.iter().filter(|n| n.contains("missing")).count(),
        32,
        "{told:?}"
    );
    assert!(
        told.iter().any(|n| n.contains("may load 16 images")),
        "{told:?}"
    );
    assert!(theme.api().pictures().is_empty());
}

// ---- Animations --------------------------------------------------------------------

#[test]
fn animations_load_with_their_frames_counted_and_none_decoded() {
    let d = dir("animations");
    d.put("a.gif", &fixture("anim.gif"))
        .put("b.png", &fixture("anim.png"))
        .put("c.webp", &fixture("lossy-anim.webp"))
        .put("still.png", &fixture("still.png"));
    let theme = d.background(
        r##"
        local a, b, c = gmnb.animation("a.gif"), gmnb.animation("b.png"), gmnb.animation("c.webp")
        assert(a.width == 8 and a.height == 6 and a.frames == 3 and math.abs(a.duration - 0.6) < 1e-9)
        assert(b.width == 8 and b.frames == 3 and math.abs(b.duration - 0.3) < 1e-9)
        assert(c.width == 32 and c.height == 24 and c.frames == 3 and math.abs(c.duration - 0.3) < 1e-9)
        assert(a == gmnb.animation("a.gif") and a ~= gmnb.image("a.gif"))
        assert(not pcall(function() a.frames = 9 end))
        -- A still is an animation of one frame.
        local s = gmnb.animation("still.png")
        assert(s.width == 16 and s.frames == 1 and s.duration == 0)
        "##,
        "draw:image(a, 0, 0, 80, 60) draw:image(s, 0, 0, 16, 12)",
    );
    assert!(notes(&theme).is_empty());
    assert!(matches!(
        theme.api().kind(ImageId(0)),
        Some(PictureKind::Animation(file, timeline)) if file == "a.gif" && timeline.frames() == 3
    ));
    // What the app's player is given: the file, and when its frames
    // start.
    let (bytes, timeline) = theme.api().animation(ImageId(2)).unwrap();
    assert_eq!(bytes.len(), fixture("lossy-anim.webp").len());
    assert_eq!(
        (timeline.width, timeline.height, timeline.frames()),
        (32, 24, 3)
    );
    // A still is no animation to the player, and an animation no still.
    assert!(theme.api().animation(ImageId(3)).is_none());
    assert!(theme.api().still(ImageId(0)).is_none());
    // Counted against the image total by what playing holds: four frames
    // and what its format's decoder keeps.
    let frame = |w: u64, h: u64| w * h * 4;
    let want = frame(8, 6) * 17 / 4          // a GIF: 4¼
        + frame(8, 6) * 6                    // a PNG: 6
        + frame(32, 24) * 5                  // a WebP: 5
        + frame(8, 6)                        // the GIF again, as a still
        + frame(16, 12) * 6; // a still PNG as an animation
    assert_eq!(theme.api().pictures().decoded_bytes(), want);
}

#[test]
fn animations_are_capped_in_number_size_frames_and_weight() {
    let d = dir("animcaps");
    for i in 1..=5 {
        d.put(&format!("{i}.gif"), &tiny_gif(4, 4, &[10, 10]));
    }
    let theme = d.load(
        r##"
        for i = 1, 4 do assert(gmnb.animation(i .. ".gif").frames == 2) end
        local fifth = gmnb.animation("5.gif")
        assert(fifth.width == 0 and fifth.frames == nil)
        return { api = 1, name = "T" }
        "##,
    );
    let told = notes(&theme);
    assert!(told[0].contains("may load 4 animations"), "{told:?}");

    let d = dir("animcaps2");
    let mut heavy = tiny_gif(4, 4, &[10, 10]);
    heavy.resize((8 << 20) + 1, 0);
    d.put("wide.gif", &tiny_gif(1025, 4, &[10, 10]))
        .put("long.gif", &tiny_gif(1, 1, &[5; 2049]))
        .put("heavy.gif", &heavy)
        .put("photo.jpg", &fixture("baseline.jpg"));
    let theme = d.load(
        r##"
        for _, file in { "wide.gif", "long.gif", "heavy.gif", "photo.jpg" } do
            local a = gmnb.animation(file)
            assert(a.width == 0 and a.height == 0, file)
        end
        -- Each used one of the four: there is none left.
        assert(gmnb.animation("none.gif").width == 0)
        return { api = 1, name = "T" }
        "##,
    );
    let told = notes(&theme);
    for part in [
        "wide.gif is 1025×4; an animation is at most 1024×1024",
        "long.gif has more than 2048 frames",
        "heavy.gif is 8388609 bytes",
        "photo.jpg is a JPEG file; an animation is a GIF, PNG or WebP file",
        "may load 4 animations",
    ] {
        assert!(told.iter().any(|n| n.contains(part)), "{part}: {told:?}");
    }

    // The image total: a 1024 × 1024 GIF counts as 17 MiB of the 32.
    let d = dir("animcaps3");
    d.put("big1.gif", &tiny_gif(1024, 1024, &[10, 10]))
        .put("big2.gif", &tiny_gif(1024, 1024, &[10, 10]))
        .put("small.gif", &tiny_gif(512, 512, &[10, 10]));
    let theme = d.load(
        r##"
        assert(gmnb.animation("big1.gif").width == 1024)
        assert(gmnb.animation("big2.gif").width == 0)
        assert(gmnb.animation("small.gif").width == 512)
        return { api = 1, name = "T" }
        "##,
    );
    let told = notes(&theme);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("big2.gif") && told[0].contains("over 32 MiB decoded"));
    assert_eq!(
        theme.api().pictures().decoded_bytes(),
        (1024 * 1024 * 4 + 512 * 512 * 4) * 17 / 4
    );
}

/// The frame of the animation drawn first in a display list.
fn frame_of(d: &Drawn) -> (u32, bool, Filter) {
    match d.list.ops.first() {
        Some(Op::Animation {
            frame,
            still,
            filter,
            ..
        }) => (*frame, *still, *filter),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_animation_plays_on_its_layers_clock_and_asks_for_its_next_frame() {
    // Three frames shown 0.1 s, 0.2 s and 0.3 s: they start at 0, 0.1 and
    // 0.3, and a pass takes 0.6 s.
    let d = dir("clock");
    d.put("a.gif", &fixture("anim.gif"));
    let theme = d.background(
        r#"local a = gmnb.animation("a.gif")"#,
        "draw:image(a, 0, 0, frame.width, frame.height)",
    );
    for (time, frame, due) in [
        (0.0, 0, 0.1),
        (0.05, 0, 0.05),
        (0.1, 1, 0.2),
        (0.25, 1, 0.05),
        (0.31, 2, 0.29),
        // A frame is never asked for within a thirtieth of a second.
        (0.59, 2, 1.0 / 30.0),
        // The loop wraps to the first frame.
        (0.61, 0, 0.09),
        // Drawn a millisecond before a frame starts (a timer of whole
        // milliseconds woke the layer for it): that frame, and the one
        // after it due when it is due.
        (0.0993, 1, 0.2007),
        (0.2992, 2, 0.3008),
        (0.5992, 0, 0.1008),
        // Three milliseconds before is too early.
        (0.097, 0, 1.0 / 30.0),
        (0.75, 1, 0.15),
        (60.0 * 0.6 + 0.35, 2, 0.25),
    ] {
        let d = drawn(&theme, time);
        assert_eq!(frame_of(&d), (frame, false, Filter::Linear), "at {time}");
        // `draw` returned nothing; the animation keeps the layer going.
        assert_eq!(d.again, None, "at {time}");
        assert!(
            d.due.is_some_and(|got| (got - due).abs() < 1e-6),
            "at {time}: due in {:?}, not {due}",
            d.due
        );
    }
}

#[test]
fn what_draw_returns_stays_its_own_beside_an_animations_next_frame() {
    let d = dir("returns");
    d.put("a.gif", &fixture("anim.gif"));
    let theme = d.background(
        r#"local a = gmnb.animation("a.gif")"#,
        "draw:image(a, 0, 0, 8, 6) return frame.width / 1000",
    );
    // The app draws again at the sooner of the two; neither is folded
    // into the other (a readout's change ends when `draw` returns nil).
    let d0 = drawn(&theme, 0.0);
    assert!(close(d0.again, 0.4) && close(d0.due, 0.1), "{d0:?}");
    // A failed frame asks for nothing.
    let theme = d.background(
        r#"local a = gmnb.animation("a.gif")"#,
        "draw:image(a, 0, 0, 8, 6) error('no')",
    );
    assert!(run(&theme, &at(0.0)).is_err());
    let theme = d.background(
        r#"local a = gmnb.animation("a.gif")"#,
        "if frame.time < 1 then draw:image(a, 0, 0, 8, 6) end",
    );
    assert!(drawn(&theme, 0.0).due.is_some());
    // A frame that draws no animation leaves nothing due from the last.
    assert_eq!(drawn(&theme, 2.0).due, None);
}

#[test]
fn a_themes_own_time_picks_the_frame_and_schedules_nothing() {
    let d = dir("owntime");
    d.put("a.gif", &fixture("anim.gif"));
    let theme = d.background(
        r#"local a = gmnb.animation("a.gif")"#,
        r##"
        local t = frame.time
        draw:image(a, 0, 0, 8, 6, { time = t })
        draw:image(a, 0, 0, 8, 6, { time = t, loop = false })
        draw:image(a, 0, 0, 8, 6, { time = t, loop = true, filter = "nearest" })
        "##,
    );
    for (time, wrapped, held) in [
        (0.0, 0, 0),
        (0.35, 2, 2),
        (0.75, 1, 2),
        (-0.2, 2, 0),
        (99.05, 0, 2),
    ] {
        let d = drawn(&theme, time);
        let frames: Vec<(u32, Filter)> = d
            .list
            .ops
            .iter()
            .map(|op| match op {
                Op::Animation {
                    frame,
                    filter,
                    still: false,
                    ..
                } => (*frame, *filter),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            frames,
            [
                (wrapped, Filter::Linear),
                (held, Filter::Linear),
                (wrapped, Filter::Nearest)
            ],
            "at {time}"
        );
        // The theme's clock is the theme's to keep going.
        assert_eq!((d.again, d.due), (None, None));
    }
    // What isn't a time or a yes-or-no is an error at that line.
    for options in [
        "{ time = 'x' }",
        "{ time = 0/0 }",
        "{ loop = 1 }",
        "{ filter = 'sharp' }",
        "7",
    ] {
        let theme = d.background(
            r#"local a = gmnb.animation("a.gif")"#,
            &format!("draw:image(a, 0, 0, 8, 6, {options})"),
        );
        assert!(run(&theme, &at(0.0)).is_err(), "{options}");
    }
}

#[test]
fn nothing_moves_under_reduced_motion_or_in_a_preview_or_a_tile() {
    let d = dir("still");
    d.put("a.gif", &fixture("anim.gif"));
    let theme = d.load(
        r##"
        local a = gmnb.animation("a.gif")
        local function show(draw, frame)
            draw:image(a, 0, 0, 8, 6)
            draw:image(a, 0, 0, 8, 6, { time = 0.35 })
            draw:tile(a, 0, 0, 80, 60)
            return 0
        end
        return { api = 1, name = "T",
            options = { { id = "o", type = "choice", label = "O", default = "x",
                choices = { { id = "x", label = "X" } },
                preview = function(draw, frame, choice) show(draw, frame) end } },
            layers = { background = { draw = show } } }
        "##,
    );
    // Reduced motion: the first frame, whatever the clock or the theme's
    // own `time` says, and no frame asked for.
    let still = Frame {
        time: 0.35,
        motion: false,
        ..Frame::sized(400.0, 300.0)
    };
    let d0 = run(&theme, &still).unwrap();
    let frames = |ops: &[Op]| -> Vec<(u32, bool)> {
        ops.iter()
            .filter_map(|op| match op {
                Op::Animation { frame, still, .. } => Some((*frame, *still)),
                _ => None,
            })
            .collect()
    };
    assert_eq!(frames(&d0.list.ops), [(0, true), (0, true)]);
    assert_eq!(d0.due, None);
    // Of an animation a tile is the first frame.
    assert!(matches!(
        d0.list.ops[2],
        Op::Tile {
            image: ImageId(0),
            ..
        }
    ));
    // With motion: frame 2 by the clock and by `time`, and the next due.
    let moving = drawn(&theme, 0.35);
    assert_eq!(frames(&moving.list.ops), [(2, false), (2, false)]);
    assert!(moving.due.is_some());
    // A preview's tile is a still picture.
    let option = theme.def.options[0].clone();
    assert!(matches!(option.kind, OptionKind::Choice { .. }));
    let tile = theme.preview(&option, "x", &Ctx::default()).unwrap();
    assert_eq!(frames(&tile.ops), [(0, true), (0, true)]);
}

#[test]
fn an_animation_the_app_cant_keep_up_with_plays_slower() {
    // Eight frames of a quarter of a second each (a delay a float holds
    // exactly, so that a frame's start is no rounding's to decide).
    let d = dir("slow");
    d.put("a.gif", &tiny_gif(4, 4, &[25; 8]));
    let theme = d.background(
        r#"local a = gmnb.animation("a.gif")"#,
        "draw:image(a, 0, 0, 4, 4)",
    );
    let quarter = |i: u32| f64::from(i) / 4.0;
    // Keeping up: the position is the layer's clock, exactly.
    for i in 0..=8 {
        let d = drawn(&theme, quarter(i));
        assert_eq!(frame_of(&d).0, i % 8, "at {i}/4");
        assert_eq!(d.due, Some(0.25));
    }
    // The player finds it can decode half as fast as the file asks: from
    // here the animation advances half a frame for each quarter of a
    // second, and its next frame is due twice as far off as its file
    // says.
    theme.api().set_slow(2.0);
    let mut seen = Vec::new();
    for i in 9..=16 {
        let d = drawn(&theme, quarter(i));
        seen.push(frame_of(&d).0);
        assert!(d.due == Some(0.25) || d.due == Some(0.5), "{:?}", d.due);
    }
    // (At 2 s it showed frame 0.) No frame is skipped; each stays twice
    // as long.
    assert_eq!(seen, [0, 1, 1, 2, 2, 3, 3, 4]);
    // Able to keep up again: it goes on from where it is at full speed,
    // it doesn't jump to where the clock is.
    theme.api().set_slow(1.0);
    let mut seen = Vec::new();
    for i in 17..=20 {
        seen.push(frame_of(&drawn(&theme, quarter(i))).0);
    }
    assert_eq!(seen, [5, 6, 7, 0]);
    // Nonsense is "keeping up".
    theme.api().set_slow(f64::NAN);
    assert_eq!(frame_of(&drawn(&theme, quarter(21))).0, 1);
}

// ---- Pictures a theme fills ----------------------------------------------------------

#[test]
fn a_theme_fills_a_picture_from_a_buffer() {
    let d = dir("pixels");
    let theme = d.background(
        r##"
        local canvas = gmnb.pixels(4, 2)
        assert(canvas.width == 4 and canvas.height == 2 and type(canvas.set) == "function")
        -- The same size is the same picture.
        assert(canvas == gmnb.pixels(4, 2) and canvas ~= gmnb.pixels(2, 4))
        assert(not pcall(function() canvas.width = 9 end))
        local buf = buffer.create(32)
        for i = 0, 7 do buffer.writeu32(buf, i * 4, 0x80000000 + i * 0x010203) end
        local spare, sets = buffer.create(32), 0
        "##,
        r##"
        draw:image(canvas, 0, 0, frame.width, frame.height)
        draw:image(canvas, 0, 0, 40, 20, { filter = "nearest" })
        draw:tile(canvas, 0, 0, 80, 80)
        local taken = canvas:set(buf)
        -- Something else to set the next time.
        sets += 1
        buffer.writeu8(spare, 0, sets)
        buf = spare
        return if taken then 1 else 2
        "##,
    );
    let now = clock(&theme, 100.0);
    // It starts transparent.
    let before = theme.api().canvas(ImageId(0)).unwrap();
    assert_eq!((before.width, before.height, before.version), (4, 2, 0));
    assert!(before.pixels.iter().all(|b| *b == 0));
    let d0 = drawn(&theme, 0.0);
    assert_eq!(d0.again, Some(1.0));
    // Smooth by default; hard pixels on request; tiled at its own size.
    assert!(matches!(
        d0.list.ops[0],
        Op::Image {
            image: ImageId(0),
            filter: Filter::Linear,
            ..
        }
    ));
    assert!(matches!(
        d0.list.ops[1],
        Op::Image {
            filter: Filter::Nearest,
            ..
        }
    ));
    assert!(matches!(
        d0.list.ops[2],
        Op::Tile {
            image: ImageId(0),
            ..
        }
    ));
    // The bytes as written: red, green, blue, opacity, not premultiplied.
    let after = theme.api().canvas(ImageId(0)).unwrap();
    assert_eq!(after.version, 1);
    for (i, p) in after.pixels.chunks(4).enumerate() {
        let i = i as u8;
        assert_eq!(p, [i * 3, i * 2, i, 0x80], "pixel {i}");
    }
    assert!(matches!(
        theme.api().kind(ImageId(0)),
        Some(PictureKind::Canvas)
    ));
    assert!(theme.api().still(ImageId(0)).is_none());

    // Thirty changes a second, two in hand: a third at the same moment
    // is refused and the picture stays.
    let second = drawn(&theme, 0.0);
    assert_eq!((second.again, second.retry), (Some(1.0), None));
    let third = drawn(&theme, 0.0);
    assert_eq!(third.again, Some(2.0));
    assert_eq!(theme.api().canvas(ImageId(0)).unwrap().version, 2);
    // The layer is owed a frame when the picture can be changed again, a
    // thirtieth of a second on: what stands is never left out of date.
    assert!(close(third.retry, 1.0 / 30.0), "{:?}", third.retry);
    now.set(100.0 + 0.9 / 30.0);
    assert_eq!(drawn(&theme, 0.0).again, Some(2.0));
    now.set(100.0 + 1.1 / 30.0);
    assert_eq!(drawn(&theme, 0.0).again, Some(1.0));
    assert_eq!(theme.api().canvas(ImageId(0)).unwrap().version, 3);
    // A steady thirty a second is never refused.
    for i in 1..=90 {
        now.set(200.0 + f64::from(i) / 30.0);
        assert_eq!(drawn(&theme, 0.0).again, Some(1.0), "frame {i}");
    }
    assert!(notes(&theme).is_empty());
}

#[test]
fn pictures_to_fill_are_capped_in_size_number_and_changes() {
    let d = dir("pixelcaps");
    d.put("big1.png", &png_header(2048, 2048))
        .put("big2.png", &png_header(2048, 2000));
    let theme = d.background(
        r##"
        local small = buffer.create(8)
        -- Over 512 × 512: no picture, and a `set` that takes nothing.
        local wide = gmnb.pixels(513, 2)
        assert(wide.width == 0 and wide.height == 0 and wide:set(small) == false)
        local made = {}
        for h = 1, 5 do made[h] = gmnb.pixels(1, h) end
        -- The fifth is one too many.
        assert(made[4].height == 4 and made[5].height == 0 and made[5]:set(small) == false)
        assert(gmnb.pixels(512, 512).width == 0)
        -- What isn't a size is an error.
        for _, size in { { 0, 4 }, { 2.5, 4 }, { 4, -1 }, { "4", 4 }, { 4 } } do
            assert(not pcall(gmnb.pixels, size[1], size[2]))
        end
        local two = made[2]
        local draws = 0
        "##,
        r##"
        -- A buffer of the wrong size changes nothing.
        assert(two:set(buffer.create(12)) == false)
        draws += 1
        buffer.writeu8(small, 0, draws)
        assert(two:set(small) == true)
        -- What it already holds is no change: taken, and nothing spent.
        for _ = 1, 5 do assert(two:set(small) == true) end
        -- What isn't a buffer is an error.
        assert(not pcall(two.set, two, "pixels"))
        assert(not pcall(two.set, two))
        -- Called on what is no picture, it takes nothing.
        assert(two.set({}, small) == false)
        "##,
    );
    let told = notes(&theme);
    for part in [
        "513×2; a picture the theme fills is at most 512×512",
        "may make 4 pictures to fill",
    ] {
        assert!(told.iter().any(|n| n.contains(part)), "{part}: {told:?}");
    }
    assert_eq!(theme.api().pictures().len(), 4);
    assert_eq!(theme.api().pictures().decoded_bytes(), (1 + 2 + 3 + 4) * 4);
    let now = clock(&theme, 7.0);
    for i in 0..3 {
        now.set(7.0 + f64::from(i));
        drawn(&theme, 0.0);
    }
    // Said once, however often the frame is drawn.
    let told = notes(&theme);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(
        told[0].contains("a buffer of 12 bytes; a 1×2 picture takes 8"),
        "{told:?}"
    );
    assert_eq!(theme.api().canvas(ImageId(1)).unwrap().version, 3);

    // The image total holds for them too.
    let theme = d.load(
        r##"
        assert(gmnb.image("big1.png").width == 2048 and gmnb.image("big2.png").width == 2048)
        -- 31.6 MiB are counted: 100 KiB more fit, a mebibyte doesn't.
        assert(gmnb.pixels(160, 160).width == 160)
        assert(gmnb.pixels(512, 512).width == 0)
        return { api = 1, name = "T" }
        "##,
    );
    let told = notes(&theme);
    assert!(
        told.iter().any(|n| n.contains("over 32 MiB decoded")),
        "{told:?}"
    );
}

// ---- Ramps and fields ----------------------------------------------------------------

const RAMP: &str = r##"gmnb.ramp({
    colors = { { r = 1, g = 0.811765, b = 0.160784 }, { r = 0.054902, g = 0.839216, b = 0.941176 },
        { r = 0.003922, g = 0.023529, b = 0.035294 } },
    edge = { 0.08, 0.92 }, cycle = true })"##;

/// The field of the media crate's stored hash, written by Luau: 9 × 7
/// points of two numbers, each one a float holds exactly.
const GOLDEN_FIELD: &str = r##"
local buf = buffer.create(9 * 7 * 8)
local o = 0
for j = 0, 6 do
    for i = 0, 8 do
        buffer.writef32(buf, o, ((i * 37 + j * 11) % 17) / 4 - 2)
        buffer.writef32(buf, o + 4, ((i * 5 + j * 29) % 13) / 8 - 0.75)
        o += 8
    end
end
"##;

fn golden_values() -> Vec<f32> {
    let mut data = Vec::new();
    for j in 0..7 {
        for i in 0..9 {
            data.push(((i * 37 + j * 11) % 17) as f32 / 4.0 - 2.0);
            data.push(((i * 5 + j * 29) % 13) as f32 / 8.0 - 0.75);
        }
    }
    data
}

fn pictures(d: &Drawn) -> Vec<(u64, u32, u32, u64)> {
    d.list
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Field { picture, .. } => Some((
                picture.serial,
                picture.width,
                picture.height,
                fnv(&picture.pixels),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn a_ramp_is_declared_once_and_read_back() {
    let d = dir("ramps");
    let theme = d.background(
        r##"
        local spec = { colors = { "#ffcf29", "#0ed6f0", gmnb.rgb(0, 0, 0, 0.5) }, edge = { 0.08, 0.92 }, cycle = true }
        local ramp = gmnb.ramp(spec)
        assert(#ramp.colors == 3 and ramp.cycle == true and ramp.edge[1] < 0.081 and ramp.edge[2] > 0.919)
        assert(math.abs(ramp.colors[1].g - 207 / 255) < 1e-6 and ramp.colors[3].a == 0.5)
        -- Asked for again, the same ramp.
        assert(ramp == gmnb.ramp(spec))
        assert(not pcall(function() ramp.cycle = false end))
        assert(not pcall(function() ramp.colors[1].r = 0 end))
        -- A gradient map by default: the whole band blends, nothing repeats.
        local map = gmnb.ramp({ colors = { "#000000", "#ffffff" } })
        assert(map.cycle == false and map.edge[1] == 0 and map.edge[2] == 1 and map ~= ramp)
        -- What isn't a ramp is an error at that line.
        for _, bad in {
            {}, { colors = { "#000000" } }, { colors = "#000000" }, { colors = { "#000000", "red" } },
            { colors = { "#000000", "#ffffff" }, edge = { 0.6, 0.4 } },
            { colors = { "#000000", "#ffffff" }, edge = { 0.5 } },
            { colors = { "#000000", "#ffffff" }, edge = "soft" },
            { colors = { "#000000", "#ffffff" }, cycle = 1 },
        } do
            assert(not pcall(gmnb.ramp, bad))
        end
        local many = {}
        for i = 1, 17 do many[i] = "#102030" end
        assert(not pcall(gmnb.ramp, { colors = many }))
        -- Eight ramps; the ninth colours nothing.
        local made = { ramp, map }
        for i = 3, 9 do made[i] = gmnb.ramp({ colors = { "#000000", gmnb.rgb(i / 9, 0, 0) } }) end
        local buf = buffer.create(2 * 2 * 4)
        "##,
        r##"
        draw:field(0, 0, 40, 40, { data = buf, cols = 2, rows = 2, channels = 1, ramp = made[8] })
        draw:field(0, 0, 40, 40, { data = buf, cols = 2, rows = 2, channels = 1, ramp = made[9] })
        "##,
    );
    let told = notes(&theme);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("may make 8 ramps"), "{told:?}");
    clock(&theme, 1.0);
    assert_eq!(pictures(&drawn(&theme, 0.0)).len(), 1);
}

#[test]
fn a_field_from_a_buffer_is_coloured_as_the_pass_colours_it() {
    let d = dir("field");
    let theme = d.background(
        &format!(
            "local ramp = {RAMP}\n{GOLDEN_FIELD}
            local later = buffer.create(9 * 7 * 8)
            for o = 0, 9 * 7 * 8 - 4, 4 do buffer.writef32(later, o, buffer.readf32(buf, o) * 0.5 + 1) end"
        ),
        r##"
        if frame.time == 0 then
            draw:field(0, 0, 61, 47, { data = buf, cols = 9, rows = 7, channels = 2, ramp = ramp,
                scale = 1.25, offset = 0.125, quality = 1 })
        elseif frame.time == 1 then
            -- A glide a quarter of the way to a second snapshot, at half size.
            draw:field(10, 20, 400, 300, { data = buf, next = later, mix = 0.25, cols = 9, rows = 7,
                channels = 2, ramp = ramp })
        elseif frame.time == 2 then
            -- The same numbers as one channel of twice the rows; quality
            -- below its least.
            draw:field(0, 0, 400, 300, { data = buf, cols = 9, rows = 14, channels = 1, ramp = ramp,
                quality = 0.01 })
            draw:field(0, 0, 2000, 2000, { data = buf, cols = 9, rows = 14, channels = 1, ramp = ramp,
                quality = 1, mix = 0.7 })
        end
        "##,
    );
    let now = clock(&theme, 10.0);
    // The same bytes as the media crate's stored hash: what a theme
    // writes into a buffer is what the pass reads.
    let d0 = drawn(&theme, 0.0);
    let Some(Op::Field { picture, rect }) = d0.list.ops.first() else {
        panic!("{:?}", d0.list.ops);
    };
    assert_eq!((picture.width, picture.height), (61, 47));
    assert_eq!(*rect, [0.0, 0.0, 61.0, 47.0]);
    assert_eq!(
        fnv(&picture.pixels),
        0x6604_19d7_81bb_1ab4,
        "{:#x}",
        fnv(&picture.pixels)
    );
    assert_eq!((d0.again, d0.due), (None, None));

    // The glide is on the numbers, before the colours.
    now.set(11.0);
    let d1 = drawn(&theme, 1.0);
    let Some(Op::Field { picture, rect }) = d1.list.ops.first() else {
        panic!("{:?}", d1.list.ops);
    };
    assert_eq!((picture.width, picture.height), (200, 150));
    assert_eq!(*rect, [10.0, 20.0, 400.0, 300.0]);
    let a = golden_values();
    let glided: Vec<f32> = a.iter().map(|v| v + ((v * 0.5 + 1.0) - v) * 0.25).collect();
    let ramp = pass::Ramp::new(
        &[
            [1.0, 0.811765, 0.160784, 1.0],
            [0.054902, 0.839216, 0.941176, 1.0],
            [0.003922, 0.023529, 0.035294, 1.0],
        ],
        [0.08, 0.92],
        true,
    )
    .unwrap();
    let grid = pass::Field {
        data: &glided,
        cols: 9,
        rows: 7,
        channels: 2,
    };
    let want = pass::render(&grid, &ramp, 1.0, 0.0, 200, 150).unwrap();
    assert!(picture.pixels[..] == want[..]);

    // The size computed: quality times the rectangle, at least a tenth,
    // and never more than 262 144 pixels.
    now.set(12.0);
    let sizes: Vec<(u32, u32)> = pictures(&drawn(&theme, 2.0))
        .iter()
        .map(|p| (p.1, p.2))
        .collect();
    assert_eq!(sizes, [(40, 30), (512, 512)]);
    assert!(notes(&theme).is_empty());
}

#[test]
fn a_field_of_the_wrong_shape_draws_nothing_or_fails_as_the_contract_says() {
    let d = dir("fieldcaps");
    let top = format!("local ramp = {RAMP}\nlocal buf = buffer.create(4 * 4 * 8)");
    // A buffer of the wrong size: the call does nothing; a note, once.
    let theme = d.background(
        &top,
        r##"
        draw:field(0, 0, 40, 40, { data = buf, cols = 4, rows = 5, channels = 2, ramp = ramp })
        draw:field(0, 0, 40, 40, { data = buf, next = buffer.create(8), cols = 4, rows = 4, channels = 2, ramp = ramp })
        "##,
    );
    for _ in 0..3 {
        assert!(drawn(&theme, 0.0).list.ops.is_empty());
    }
    let told = notes(&theme);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(
        told[0].contains("`data` holds 128 bytes; 4 × 5 points of 2 take 160"),
        "{told:?}"
    );
    assert!(told[1].contains("`next` holds 8 bytes"), "{told:?}");

    // A rectangle of no size draws nothing, without a word.
    let theme = d.background(
        &top,
        "draw:field(0, 0, 0, 40, { data = buf, cols = 4, rows = 4, channels = 2, ramp = ramp })",
    );
    assert!(drawn(&theme, 0.0).list.ops.is_empty());
    assert!(notes(&theme).is_empty());

    // Two fields a draw; a third is over the cap, and the frame is
    // dropped whatever the hook does about the error.
    let field =
        "draw:field(0, 0, 40, 40, { data = buf, cols = 4, rows = 4, channels = 2, ramp = ramp })";
    let theme = d.background(&top, &format!("{field} {field}"));
    assert_eq!(pictures(&drawn(&theme, 0.0)).len(), 2);
    let theme = d.background(
        &top,
        &format!("{field} {field} pcall(function() {field} end)"),
    );
    match run(&theme, &at(0.0)) {
        Err(Failure::Failed(ThemeError::Limit(cap))) => {
            assert!(cap.contains("more than 2 fields"), "{cap}");
        }
        other => panic!("{other:?}"),
    }

    // What isn't a field is an error at that line.
    for spec in [
        "{ data = buf, cols = 1, rows = 4, channels = 2, ramp = ramp }",
        "{ data = buf, cols = 257, rows = 4, channels = 2, ramp = ramp }",
        "{ data = buf, cols = 2.5, rows = 4, channels = 2, ramp = ramp }",
        "{ data = buf, cols = 200, rows = 100, channels = 1, ramp = ramp }",
        "{ data = buf, cols = 4, rows = 4, channels = 3, ramp = ramp }",
        "{ data = buf, cols = 4, rows = 4, ramp = ramp }",
        "{ data = buf, cols = 4, rows = 4, channels = 2 }",
        "{ data = buf, cols = 4, rows = 4, channels = 2, ramp = { colors = {} , id = 99 } }",
        "{ data = 'numbers', cols = 4, rows = 4, channels = 2, ramp = ramp }",
        "{ data = buf, next = 7, cols = 4, rows = 4, channels = 2, ramp = ramp }",
        "{ data = buf, cols = 4, rows = 4, channels = 2, ramp = ramp, scale = 0/0 }",
        "{ data = buf, cols = 4, rows = 4, channels = 2, ramp = ramp, quality = 'high' }",
        "nil",
    ] {
        let theme = d.background(&top, &format!("draw:field(0, 0, 40, 40, {spec})"));
        assert!(run(&theme, &at(0.0)).is_err(), "{spec}");
    }
    // A rectangle that isn't made of numbers, too.
    let theme = d.background(
        &top,
        "draw:field(0, 0/0, 40, 40, { data = buf, cols = 4, rows = 4, channels = 2, ramp = ramp })",
    );
    assert!(run(&theme, &at(0.0)).is_err());
}

#[test]
fn a_layers_fields_are_coloured_thirty_times_a_second_at_most() {
    let d = dir("fieldrate");
    let top = format!(
        "local ramp = {RAMP}\nlocal buf = buffer.create(4 * 4 * 8)
        local function fill(v) for o = 0, 4 * 4 * 8 - 4, 4 do buffer.writef32(buf, o, v) end end"
    );
    let field =
        "draw:field(0, 0, 40, 40, { data = buf, cols = 4, rows = 4, channels = 2, ramp = ramp })";
    let theme = d.background(&top, &format!("fill(frame.time) {field}"));
    let now = clock(&theme, 50.0);
    let frame = |time: f64| {
        let d = drawn(&theme, time);
        (pictures(&d)[0].0, d.retry)
    };
    let serial = |time: f64| frame(time).0;
    // Two in hand; a third pass at the same moment draws the picture the
    // field made last, again, whatever the buffer now holds.
    let (first, second) = (frame(0.0), frame(0.25));
    assert_ne!(first.0, second.0);
    assert_eq!((first.1, second.1), (None, None));
    let second = second.0;
    // And the layer is owed a frame a thirtieth of a second on, when
    // there is a pass again: a picture that stands is never out of date.
    let refused = frame(0.5);
    assert_eq!(refused.0, second);
    assert!(close(refused.1, 1.0 / 30.0), "{:?}", refused.1);
    assert_eq!(serial(0.75), second);
    now.set(50.0 + 0.5 / 30.0);
    assert!(close(frame(0.8).1, 0.5 / 30.0));
    // A thirtieth of a second later there is a pass again.
    now.set(50.0 + 1.1 / 30.0);
    let third = serial(1.0);
    assert_ne!(third, second);
    assert_eq!(serial(1.25), third);
    // A steady thirty a second, the numbers new each time: a new picture
    // every frame.
    let mut last = third;
    for i in 1..=60 {
        now.set(60.0 + f64::from(i) / 30.0);
        let next = serial(10.0 + f64::from(i));
        assert_ne!(next, last, "frame {i}");
        last = next;
    }

    // Two fields in one draw share the layer's thirty: they take turns,
    // each coloured fifteen times a second, neither starved.
    let theme = d.background(&top, &format!("fill(frame.time) {field} {field}"));
    let now = clock(&theme, 50.0);
    let mut fresh = [0, 0];
    let mut last = [0, 0];
    for i in 1..=120 {
        now.set(50.0 + f64::from(i) / 30.0);
        let made = pictures(&drawn(&theme, f64::from(i)));
        assert_eq!(made.len(), 2, "frame {i}");
        for slot in 0..2 {
            if made[slot].0 != last[slot] {
                fresh[slot] += 1;
                last[slot] = made[slot].0;
            }
        }
    }
    assert!(
        (58..=63).contains(&fresh[0]) && (58..=63).contains(&fresh[1]),
        "{fresh:?}"
    );
    // A field that has no picture yet and no pass to make one draws
    // nothing this once, and is owed a frame.
    let theme = d.background(
        &top,
        &format!("fill(frame.time) {field} if frame.time > 1 then {field} end"),
    );
    clock(&theme, 50.0);
    assert_eq!(pictures(&drawn(&theme, 0.0)).len(), 1);
    assert_eq!(pictures(&drawn(&theme, 1.0)).len(), 1);
    let short = drawn(&theme, 2.0);
    assert_eq!(pictures(&short).len(), 1);
    assert!(short.retry.is_some());

    // A field asked for with the numbers, the ramp and the size of its
    // last picture is that picture: nothing is coloured and nothing is
    // spent, however often and however fast the layer is drawn.
    let theme = d.background(
        &top,
        &format!("fill(math.floor(frame.time)) {field} {field}"),
    );
    clock(&theme, 50.0);
    let first = drawn(&theme, 0.0);
    let made = pictures(&first);
    assert_eq!(made.len(), 2);
    for _ in 0..50 {
        let again = drawn(&theme, 0.5);
        assert_eq!(pictures(&again), made);
        assert_eq!(again.retry, None);
    }
    // Both changed, with nothing in hand: both are refused and the layer
    // is owed a frame. A thirtieth of a second on there is one pass: the
    // owed frame makes one field good and another frame is owed. The
    // second makes the other good, the first field's new picture
    // standing as it is, at no cost; and then nothing is owed.
    let changed = drawn(&theme, 1.0);
    assert_eq!(pictures(&changed), made);
    assert!(changed.retry.is_some());
    let now = clock(&theme, 50.0 + 1.01 / 30.0);
    let owed = drawn(&theme, 1.0);
    let half = pictures(&owed);
    assert!(owed.retry.is_some());
    assert!(
        (half[0].0 != made[0].0) != (half[1].0 != made[1].0),
        "{half:?}"
    );
    now.set(50.0 + 2.02 / 30.0);
    let owed = drawn(&theme, 1.0);
    let after = pictures(&owed);
    assert_eq!(owed.retry, None, "{after:?}");
    assert!(
        after[0].0 != made[0].0 && after[1].0 != made[1].0,
        "{after:?}"
    );
    assert!(after[0] == half[0] || after[1] == half[1], "{after:?}");
    // And then it stands: no frame is owed for ever.
    for i in 0..20 {
        now.set(60.0 + f64::from(i));
        let still = drawn(&theme, 1.0);
        assert_eq!((pictures(&still), still.retry), (after.clone(), None));
    }
    // One of the two stops being drawn: the other has every pass again.
    let theme = d.background(
        &top,
        &format!("fill(frame.time) {field} if frame.time < 30 then {field} end"),
    );
    let now = clock(&theme, 50.0);
    let mut fresh = 0;
    let mut last = 0;
    for i in 1..=90 {
        now.set(50.0 + f64::from(i) / 30.0);
        let made = pictures(&drawn(&theme, f64::from(i)));
        if i > 40 && made[0].0 != last {
            fresh += 1;
        }
        last = made[0].0;
    }
    assert!(fresh >= 49, "{fresh} of 50 frames were coloured");
}

#[test]
fn a_previews_fields_are_always_coloured() {
    // Tiles are still pictures, drawn once each when Settings opens: six
    // of them at the same moment, each with a field.
    let d = dir("fieldtiles");
    let theme = d.load(&format!(
        r##"
        local ramp = {RAMP}
        local buf = buffer.create(2 * 2 * 4)
        local ids = {{ "a", "b", "c", "d", "e", "f" }}
        local choices = {{}}
        for i, id in ids do choices[i] = {{ id = id, label = id }} end
        return {{ api = 1, name = "T",
            options = {{ {{ id = "o", type = "choice", label = "O", default = "a", choices = choices,
                preview = function(draw, frame, choice)
                    buffer.writef32(buf, 0, string.byte(choice))
                    draw:field(0, 0, frame.width, frame.height,
                        {{ data = buf, cols = 2, rows = 2, channels = 1, ramp = ramp, quality = 1 }})
                end }} }} }}
        "##
    ));
    clock(&theme, 3.0);
    let option = theme.def.options[0].clone();
    let mut seen = std::collections::HashSet::new();
    for choice in ["a", "b", "c", "d", "e", "f"] {
        let tile = theme.preview(&option, choice, &Ctx::default()).unwrap();
        let [Op::Field { picture, rect }] = &tile.ops[..] else {
            panic!("{choice}: {:?}", tile.ops);
        };
        assert_eq!(*rect, [0.0, 0.0, 96.0, 56.0]);
        assert_eq!((picture.width, picture.height), (96, 56));
        assert!(seen.insert(fnv(&picture.pixels)), "{choice} repeats a tile");
    }
}

// ---- The swirl demonstration -----------------------------------------------------------

const W: usize = 760;
const H: usize = 640;

fn swirls() -> Theme {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/themes/swirls");
    // The limits a theme really has.
    match Theme::load(Source::folder("swirls", &dir), Limits::default()) {
        Ok(theme) => theme,
        Err(e) => panic!("{e}"),
    }
}

fn swirl_ctx(grid: &str, place: &str) -> Ctx {
    let mut ctx = Ctx::default();
    ctx.options
        .insert("grid".into(), OptionValue::Text(grid.into()));
    ctx.options
        .insert("where".into(), OptionValue::Text(place.into()));
    ctx
}

fn swirl_frame(time: f64) -> Frame<'static> {
    Frame {
        time,
        ..Frame::sized(W as f32, H as f32)
    }
}

/// The ColorMix shader per pixel at time 0 (the picture the page draws),
/// through the same ramp.
fn exact() -> Vec<u8> {
    let ramp = pass::Ramp::new(
        &[
            [1.0, 207.0 / 255.0, 41.0 / 255.0, 1.0],
            [14.0 / 255.0, 214.0 / 255.0, 240.0 / 255.0, 1.0],
            [1.0 / 255.0, 6.0 / 255.0, 9.0 / 255.0, 1.0],
        ],
        [0.08, 0.92],
        true,
    )
    .unwrap();
    let (px, py) = (1.31f32, 4.27f32);
    let (sx, sy) = (0.825 * W as f32 / H as f32, 1.65f32);
    let mut out = Vec::with_capacity(W * H * 4);
    for y in 0..H {
        let v0 = (1.0 - 2.0 * (y as f32 + 0.5) / H as f32) * sy;
        for x in 0..W {
            let (mut u, mut v) = ((2.0 * (x as f32 + 0.5) / W as f32 - 1.0) * sx, v0);
            let (mut u2, mut v2) = (u + v, u + v);
            for _ in 0..3 {
                let len = (u * u + v * v).sqrt();
                u2 += u + len;
                v2 += v + len;
                u += 0.5 * (px + v2 * 0.2).cos();
                v += 0.5 * (py + u2).sin();
                let d = (u + v).cos() - (u * 0.7 - v).sin();
                u -= d;
                v -= d;
            }
            out.extend_from_slice(&ramp.at((u * u + v * v).sqrt() * 1.25));
        }
    }
    out
}

/// A picture scaled up smoothly to the window's size, pixel centres
/// aligned: what drawing a smaller texture with the linear filter does.
fn upscale(src: &[u8], sw: usize, sh: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(W * H * 4);
    for y in 0..H {
        let fy = ((y as f32 + 0.5) * sh as f32 / H as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let j = (fy as usize).min(sh.saturating_sub(2));
        let (ty, j1) = (fy - j as f32, (j + 1).min(sh - 1));
        for x in 0..W {
            let fx = ((x as f32 + 0.5) * sw as f32 / W as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let i = (fx as usize).min(sw.saturating_sub(2));
            let (tx, i1) = (fx - i as f32, (i + 1).min(sw - 1));
            for c in 0..4 {
                let at = |i: usize, j: usize| f32::from(src[(j * sw + i) * 4 + c]);
                let top = at(i, j) + (at(i1, j) - at(i, j)) * tx;
                let bottom = at(i, j1) + (at(i1, j1) - at(i, j1)) * tx;
                out.push((top + (bottom - top) * ty + 0.5) as u8);
            }
        }
    }
    out
}

/// The mean difference of a channel (of 255), and the share of pixels
/// with a channel off by more than 32.
fn differ(a: &[u8], b: &[u8]) -> (f64, f64) {
    let (mut sum, mut far) = (0u64, 0u64);
    for (p, q) in a.chunks(4).zip(b.chunks(4)) {
        let mut worst = 0;
        for c in 0..3 {
            let d = p[c].abs_diff(q[c]);
            sum += u64::from(d);
            worst = worst.max(d);
        }
        far += u64::from(worst > 32);
    }
    let pixels = (a.len() / 4) as f64;
    (sum as f64 / (pixels * 3.0), far as f64 * 100.0 / pixels)
}

#[test]
fn the_swirl_demonstration_draws_the_pages_picture_from_a_coarse_field() {
    let truth = exact();
    for (grid, mean_most, far_most) in [("12", 1.4, 0.4), ("16", 2.3, 1.3)] {
        let theme = swirls();
        assert!(theme.notes.list.is_empty(), "{:?}", theme.notes.list);
        let ctx = swirl_ctx(grid, "background");
        let d0 = theme
            .draw(LayerKind::Background, &ctx, &swirl_frame(0.0))
            .unwrap_or_else(|e| panic!("{e:?}"));
        // One field over the whole window, coloured at half its size,
        // and asked for again thirty times a second.
        let [Op::Field { picture, rect }] = &d0.list.ops[..] else {
            panic!("{:?}", d0.list.ops);
        };
        assert_eq!(*rect, [0.0, 0.0, 760.0, 640.0]);
        assert_eq!((picture.width, picture.height), (380, 320));
        assert!(close(d0.again, 1.0 / 30.0));
        // Against the shader worked out per pixel: as near as the
        // designer measured (1.13 / 0.18% at 12 px, 1.91 / 0.87% at 16).
        let shown = upscale(&picture.pixels, 380, 320);
        let (mean, far) = differ(&shown, &truth);
        eprintln!(
            "the swirl from a field every {grid} px: mean error {mean:.2}, {far:.2}% off by more than 32"
        );
        assert!(
            mean < mean_most && far < far_most,
            "{grid} px: {mean:.2}, {far:.2}%"
        );
        // The other place draws nothing.
        let over = theme
            .draw(LayerKind::Overlay, &ctx, &swirl_frame(0.0))
            .unwrap();
        assert!(over.list.ops.is_empty() && over.again.is_none());
        assert!(notes(&theme).is_empty());
    }
    // Over everything instead: the same picture from the other layer.
    let theme = swirls();
    let ctx = swirl_ctx("12", "overlay");
    assert!(
        theme
            .draw(LayerKind::Background, &ctx, &swirl_frame(0.0))
            .unwrap()
            .list
            .ops
            .is_empty()
    );
    assert_eq!(
        theme
            .draw(LayerKind::Overlay, &ctx, &swirl_frame(0.0))
            .unwrap()
            .list
            .ops
            .len(),
        1
    );
}

#[test]
fn the_swirl_computes_its_field_ten_times_a_second_and_glides_between() {
    let theme = swirls();
    let now = clock(&theme, 1000.0);
    let ctx = swirl_ctx("12", "background");
    // A second at thirty frames a second, under the limits a theme has
    // (8 ms of processor time a frame).
    let mut seen = std::collections::HashSet::new();
    for i in 0..=30 {
        now.set(1000.0 + f64::from(i) / 30.0);
        let d = theme
            .draw(
                LayerKind::Background,
                &ctx,
                &swirl_frame(f64::from(i) / 30.0),
            )
            .unwrap_or_else(|e| panic!("frame {i}: {e:?}"));
        let [Op::Field { picture, .. }] = &d.list.ops[..] else {
            panic!("{:?}", d.list.ops);
        };
        // Every frame is a new picture: the glide moves between field
        // updates.
        assert!(
            seen.insert(fnv(&picture.pixels)),
            "frame {i} repeats a picture"
        );
    }
    // Reduced motion: one still picture, the same as the first frame's.
    let still = Frame {
        motion: false,
        ..swirl_frame(0.0)
    };
    now.set(2000.0);
    let d = theme.draw(LayerKind::Background, &ctx, &still).unwrap();
    assert_eq!(d.again, None);
    assert_eq!(d.list.ops.len(), 1);
    assert_eq!(theme.hooks().iter().map(|h| h.1).sum::<u32>(), 0);
}

/// What the swirl costs a frame, in the real engine under the real
/// limits. Not a test of anything: run it for the figures, in the release
/// profile, on a quiet machine.
/// `cargo test --release -p gmnb-theme --test media -- --ignored --nocapture`
#[test]
#[ignore = "a measurement, not a test"]
fn what_the_swirl_costs() {
    for grid in ["16", "12"] {
        let theme = swirls();
        let now = clock(&theme, 0.0);
        let ctx = swirl_ctx(grid, "background");
        // Thirty frames a second for twenty seconds. Every third frame a
        // new tenth of a second begins and one snapshot of the field is
        // computed; the other two only glide.
        let (mut update, mut glide) = (Vec::new(), Vec::new());
        for i in 3..600 {
            now.set(f64::from(i) / 30.0);
            let frame = swirl_frame(f64::from(i) / 30.0);
            let cpu = gmnb_theme::media::thread_time().unwrap();
            theme.draw(LayerKind::Background, &ctx, &frame).unwrap();
            let ms = (gmnb_theme::media::thread_time().unwrap() - cpu).as_secs_f64() * 1e3;
            if i % 3 == 0 {
                update.push(ms)
            } else {
                glide.push(ms)
            }
        }
        let stats = |v: &mut Vec<f64>| {
            v.sort_by(f64::total_cmp);
            (v[0], v[v.len() / 2], v[v.len() * 95 / 100])
        };
        let (u, g) = (stats(&mut update), stats(&mut glide));
        eprintln!(
            "a point every {grid} px, 760×640, processor ms a frame (min median p95): a frame that only \
             glides {:.3} {:.3} {:.3}; a frame that computes a field {:.3} {:.3} {:.3}; the field itself \
             {:.3} (median)",
            g.0,
            g.1,
            g.2,
            u.0,
            u.1,
            u.2,
            u.1 - g.1
        );
    }
}
