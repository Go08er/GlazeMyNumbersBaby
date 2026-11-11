//! The drawing layers without a toolkit: the draw API and its display
//! list, every cap hit on purpose, the frame a hook is told, `place`,
//! previews, pictures, and the built-in themes' layers in the real engine
//! (docs/theming.md, "Drawing" and "Limits").

mod common;

use std::rc::Rc;
use std::time::{Duration, Instant};

use gmnb_theme::draw::{self, BlendMode, FillRule, LineCap, LineJoin, MaskMode, Op};
use gmnb_theme::events::{Event, EventKind, KeyKind, Page, Source as PressSource, Via};
use gmnb_theme::layer::{
    ChangeKind, DisplayValue, Drawn, Failure, Frame, GlyphBox, Glyphs, KeyCell, Keys, LayerKind,
    Placement, Role,
};
use gmnb_theme::path::Segment;
use gmnb_theme::picture::{ImageId, NoiseSpec, PictureKind, Pixels};
use gmnb_theme::theme::{OptionKind, Source, Theme};
use gmnb_theme::types::*;
use gmnb_theme::{Ctx, Limits, OptionValue, TextMeasure, ThemeError};

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
    let dir = std::env::temp_dir().join(format!("gmnb-draw-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Dir(dir)
}

fn load_in(d: &Dir, source: &str, limits: Limits) -> Theme {
    std::fs::write(d.0.join("theme.luau"), source).unwrap();
    Theme::load(Source::folder("t", &d.0), limits).unwrap()
}

fn load(name: &str, source: &str) -> (Theme, Dir) {
    let d = dir(name);
    let theme = load_in(&d, source, patient());
    (theme, d)
}

/// A theme whose background layer's `draw` is `body` (with `draw` and
/// `frame` in scope), after `top` at the file's top level.
fn background(name: &str, top: &str, body: &str) -> (Theme, Dir) {
    let source = format!(
        "{top}\nreturn {{ api = 1, name = 'T', layers = {{ background = {{ draw = function(draw, frame)\n{body}\nend }} }} }}"
    );
    load(name, &source)
}

fn frame() -> Frame<'static> {
    Frame::sized(400.0, 300.0)
}

fn run(theme: &Theme) -> Result<Drawn, Failure> {
    theme.draw(LayerKind::Background, &Ctx::default(), &frame())
}

fn ops(theme: &Theme) -> Vec<Op> {
    run(theme).unwrap().list.ops
}

fn message(failure: Failure) -> String {
    match failure {
        Failure::Failed(e) => e.to_string(),
        Failure::Off => "off".into(),
    }
}

fn color(hex: &str) -> Paint {
    Paint::Color(gmnb_theme::check::parse_hex(hex).unwrap())
}

// ---- The calls -----------------------------------------------------------------

/// `draw:tile` reads one option, `filter` (hard pixels on a screen of
/// more pixels than the window's), and is smooth without it.
#[test]
fn a_tile_takes_its_filter_and_nothing_else() {
    let (theme, _d) = background(
        "tile-filter",
        "local grain = gmnb.noise({ size = 8 })",
        r#"
        draw:tile(grain, 0, 0, 40, 30, { filter = "nearest", time = 3, loop = false })
        draw:tile(grain, 0, 0, 40, 30, {})
        draw:tile(grain, 0, 0, 40, 30)
        "#,
    );
    let filters: Vec<draw::Filter> = ops(&theme)
        .iter()
        .filter_map(|op| match op {
            Op::Tile { filter, .. } => Some(*filter),
            _ => None,
        })
        .collect();
    use draw::Filter::{Linear, Nearest};
    assert_eq!(filters, [Nearest, Linear, Linear]);
    let (theme, _d) = background(
        "tile-bad-filter",
        "local grain = gmnb.noise({ size = 8 })",
        r#"draw:tile(grain, 0, 0, 40, 30, { filter = "blurry" })"#,
    );
    assert!(run(&theme).is_err());
}

#[test]
fn every_call_records_its_operation() {
    let (theme, _d) = background(
        "calls",
        "local grain = gmnb.noise({ size = 8 })",
        r##"
        draw:rect(1, 2, 30, 40, "#ff0000")
        draw:rounded_rect(1, 2, 30, 40, { 1, 2, 3, 4 }, { r = 0, g = 1, b = 0, a = 0.5 })
        draw:circle(50, 60, 7, gmnb.linear({ angle = 90, colors = { "#000000", "#ffffff" } }))
        local p = gmnb.path():move_to(0, 0):line_to(10, 0):quad_to(15, 5, 10, 10)
            :curve_to(8, 12, 2, 12, 0, 10):close():rect(1, 1, 2, 2):rounded_rect(0, 0, 9, 9, 3):circle(5, 5, 2)
        draw:fill(p, "#0000ff", "even-odd")
        draw:stroke(gmnb.svg_path("M6 12.5h12"),
            { width = 2, cap = "round", join = "bevel", dash = { 4, 2 }, dash_offset = 1 }, "#00ff00")
        draw:stroke(p, 1.5, "#00ff00")
        draw:border(0, 0, 20, 20, 4, 2, "#123456")
        draw:shadow(0, 0, 20, 20, 4, { y = 2, blur = 6, color = "#00000080" })
        draw:shadow(0, 0, 20, 20, 0, { inset = true, spread = 1, color = "#ffffff" })
        draw:text("12", 5, 6, { size = 20, weight = 700 }, "#ffffff")
        draw:image(grain, 0, 0, 16, 16)
        draw:tile(grain, 0, 0, 400, 300)
        draw:push_clip(0, 0, 10, 10)
        draw:push_clip(0, 0, 10, 10, 3)
        draw:push_opacity(0.5)
        draw:push_blur(4)
        draw:push_shadow({ { blur = 3, color = "#000000" } })
        draw:pop() draw:pop() draw:pop() draw:pop() draw:pop()
        draw:push_mask("luminance")
        draw:rect(0, 0, 1, 1, "#ffffff")
        draw:pop()
        draw:rect(0, 0, 1, 1, "#ff0000")
        draw:pop()
        draw:push_blend("screen")
        draw:pop()
        draw:pop()
        draw:save()
        draw:translate(3, 4)
        draw:scale(2)
        draw:scale(2, 3)
        draw:rotate(45)
        draw:restore()
        return 0.5
        "##,
    );
    let drawn = run(&theme).unwrap();
    assert_eq!(drawn.again, Some(0.5));
    let ops = drawn.list.ops;
    let rect = [1.0, 2.0, 30.0, 40.0];
    assert_eq!(
        ops[0],
        Op::Rect {
            rect,
            paint: color("#ff0000")
        }
    );
    assert_eq!(
        ops[1],
        Op::RoundedRect {
            rect,
            radius: Radius([1.0, 2.0, 3.0, 4.0]),
            paint: Paint::Color([0.0, 1.0, 0.0, 0.5]),
        }
    );
    let Op::Circle {
        center,
        radius,
        paint: Paint::Gradient(g),
    } = &ops[2]
    else {
        panic!("{:?}", ops[2]);
    };
    assert_eq!((*center, *radius), ([50.0, 60.0], 7.0));
    assert_eq!((g.kind, g.angle), (GradientKind::Linear, 90.0));
    assert_eq!(g.colors, [[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]]);
    assert_eq!(g.stops, [0.0, 1.0]);
    let Op::Fill { path, paint, rule } = &ops[3] else {
        panic!("{:?}", ops[3]);
    };
    assert_eq!((paint, *rule), (&color("#0000ff"), FillRule::EvenOdd));
    assert_eq!(
        path.segments,
        [
            Segment::MoveTo(0.0, 0.0),
            Segment::LineTo(10.0, 0.0),
            Segment::QuadTo(15.0, 5.0, 10.0, 10.0),
            Segment::CurveTo(8.0, 12.0, 2.0, 12.0, 0.0, 10.0),
            Segment::Close,
            Segment::Rect(1.0, 1.0, 2.0, 2.0),
            Segment::RoundedRect(0.0, 0.0, 9.0, 9.0, Radius::all(3.0)),
            Segment::Circle(5.0, 5.0, 2.0),
        ]
    );
    let Op::Stroke { path, stroke, .. } = &ops[4] else {
        panic!("{:?}", ops[4]);
    };
    assert_eq!(path.segments, [Segment::Svg(Rc::from("M6 12.5h12"))]);
    assert_eq!(
        *stroke,
        draw::Stroke {
            width: 2.0,
            cap: LineCap::Round,
            join: LineJoin::Bevel,
            dash: vec![4.0, 2.0],
            dash_offset: 1.0,
        }
    );
    assert!(
        matches!(&ops[5], Op::Stroke { stroke, .. } if stroke.width == 1.5 && stroke.dash.is_empty())
    );
    assert_eq!(
        ops[6],
        Op::Border {
            rect: [0.0, 0.0, 20.0, 20.0],
            radius: Radius::all(4.0),
            width: 2.0,
            color: gmnb_theme::check::parse_hex("#123456").unwrap(),
        }
    );
    assert!(
        matches!(&ops[7], Op::Shadow { shadow, .. } if !shadow.inset && shadow.blur == 6.0 && shadow.y == 2.0)
    );
    assert!(matches!(&ops[8], Op::Shadow { shadow, .. } if shadow.inset && shadow.spread == 1.0));
    let Op::Text { text, at, font, .. } = &ops[9] else {
        panic!("{:?}", ops[9]);
    };
    assert_eq!((text.as_str(), *at), ("12", [5.0, 6.0]));
    assert_eq!((font.size, font.weight), (20.0, 700.0));
    assert_eq!(
        ops[10],
        Op::Image {
            image: ImageId(0),
            rect: [0.0, 0.0, 16.0, 16.0],
            filter: draw::Filter::Trilinear,
        }
    );
    assert_eq!(
        ops[11],
        Op::Tile {
            image: ImageId(0),
            rect: [0.0, 0.0, 400.0, 300.0],
            filter: draw::Filter::Linear,
        }
    );
    assert_eq!(
        ops[12..22],
        [
            Op::PushClip {
                rect: [0.0, 0.0, 10.0, 10.0],
                radius: None
            },
            Op::PushClip {
                rect: [0.0, 0.0, 10.0, 10.0],
                radius: Some(Radius::all(3.0))
            },
            Op::PushOpacity(0.5),
            Op::PushBlur(4.0),
            Op::PushShadow(vec![Shadow {
                x: 0.0,
                y: 0.0,
                blur: 3.0,
                spread: 0.0,
                color: [0.0, 0.0, 0.0, 1.0],
                inset: false
            }]),
            Op::Pop,
            Op::Pop,
            Op::Pop,
            Op::Pop,
            Op::Pop,
        ]
    );
    assert_eq!(ops[22], Op::PushMask(MaskMode::Luminance));
    assert_eq!((&ops[24], &ops[26]), (&Op::Pop, &Op::Pop));
    assert_eq!(
        ops[27..],
        [
            Op::PushBlend(BlendMode::Screen),
            Op::Pop,
            Op::Pop,
            Op::Save,
            Op::Translate(3.0, 4.0),
            Op::Scale(2.0, 2.0),
            Op::Scale(2.0, 3.0),
            Op::Rotate(45.0),
            Op::Restore,
        ]
    );
    assert!(theme.take_draw_notes().is_empty());
}

#[test]
fn gradients_are_placed_in_pixels_or_by_fractions() {
    let (theme, _d) = background(
        "gradients",
        "",
        r##"
        draw:rect(0, 0, 9, 9, gmnb.linear({ from = { 0, 1 }, to = { 2, 3 }, colors = { "#000000", "#ffffff" } }))
        draw:rect(0, 0, 9, 9, gmnb.radial({ center = { 4, 5 }, radius = 6, radius_y = 7,
            colors = { "#000000", "#ff0000", "#ffffff" }, stops = { 0, 0.25, 1 } }))
        draw:rect(0, 0, 9, 9, gmnb.radial({ at = { 0.25, 0.75 }, radius = "closest-side", colors = { "#000000", "#ffffff" } }))
        draw:rect(0, 0, 9, 9, gmnb.conic({ center = { 4, 5 }, angle = 30, colors = { "#000000", "#ffffff" } }))
        -- Written by hand, with its colours any way a colour can be given.
        draw:rect(0, 0, 9, 9, { kind = "linear", colors = { "#000", { r = 1, g = 1, b = 1 } } })
        "##,
    );
    let ops = ops(&theme);
    let g = |i: usize| match &ops[i] {
        Op::Rect {
            paint: Paint::Gradient(g),
            ..
        } => g.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        (g(0).pixels.from, g(0).pixels.to),
        (Some([0.0, 1.0]), Some([2.0, 3.0]))
    );
    assert_eq!(
        (g(1).pixels.center, g(1).pixels.radius, g(1).pixels.radius_y),
        (Some([4.0, 5.0]), Some(6.0), Some(7.0))
    );
    assert_eq!(g(1).stops, [0.0, 0.25, 1.0]);
    assert_eq!((g(2).at, g(2).extent), ([0.25, 0.75], Extent::ClosestSide));
    assert!(g(2).pixels.is_empty());
    assert_eq!(
        (g(3).kind, g(3).angle, g(3).pixels.center),
        (GradientKind::Conic, 30.0, Some([4.0, 5.0]))
    );
    assert_eq!(g(4).colors, [[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]]);
    assert_eq!((g(4).angle, g(4).stops.clone()), (180.0, vec![0.0, 1.0]));
}

#[test]
fn a_picture_is_sampled_as_its_options_say() {
    let (theme, _d) = background(
        "filter",
        "local grain = gmnb.noise({ size = 8 })",
        r##"
        draw:image(grain, 0, 0, 16, 16, { filter = "nearest" })
        draw:image(grain, 0, 0, 16, 16, { filter = "linear", time = 1.5, loop = false })
        draw:image(grain, 0, 0, 16, 16, {})
        "##,
    );
    let filters: Vec<draw::Filter> = ops(&theme)
        .iter()
        .map(|op| match op {
            Op::Image { filter, .. } => *filter,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        filters,
        [
            draw::Filter::Nearest,
            draw::Filter::Linear,
            draw::Filter::Trilinear
        ]
    );
    let (theme, _d) = background(
        "badfilter",
        "local grain = gmnb.noise({ size = 8 })",
        "draw:image(grain, 0, 0, 16, 16, { filter = 'cubic' })",
    );
    assert!(message(run(&theme).unwrap_err()).contains("expected one of"));
}

#[test]
fn what_a_hook_returns_is_when_it_runs_again() {
    for (body, again) in [
        ("return nil", None),
        ("", None),
        ("return 0", Some(0.0)),
        ("return 1 / 15", Some(1.0 / 15.0)),
        // Anything else is "this picture stands".
        ("return true", None),
        ("return 'soon'", None),
    ] {
        let (theme, _d) = background("again", "", body);
        assert_eq!(run(&theme).unwrap().again, again, "{body}");
    }
}

// ---- Arguments -------------------------------------------------------------------

#[test]
fn wrong_arguments_are_errors_with_the_call_named() {
    for (call, expect) in [
        ("draw:rect(0, 0, 1, 1, '#zzz')", "draw:rect: a colour is"),
        ("draw:rect(0, 0, 1, 1)", "draw:rect: expected a paint"),
        ("draw:rect(0, 0, 1, 1, { r = 1 })", "needs r, g and b"),
        ("draw:rect(0/0, 0, 1, 1, '#fff')", "isn't finite"),
        (
            "draw:rect(0, 0, 1, 1, { kind = 'spiral', colors = {} })",
            "linear, radial or conic",
        ),
        (
            "draw:rect(0, 0, 1, 1, { kind = 'linear', colors = { '#fff' } })",
            "2 to 16",
        ),
        (
            "draw:rect(0, 0, 1, 1, { kind = 'linear', colors = { '#fff', '#000' }, stops = { 1, 0 } })",
            "never decrease",
        ),
        (
            "draw:rect(0, 0, 1, 1, { kind = 'linear', colors = { '#fff', '#000' }, stops = { 0 } })",
            "one per colour",
        ),
        (
            "draw:rounded_rect(0, 0, 1, 1, { 1, 2 }, '#fff')",
            "a number, or four",
        ),
        ("draw:fill({ 99 }, '#fff')", "not a path"),
        ("draw:fill('M0 0', '#fff')", "expected a path"),
        ("draw:fill(gmnb.path(), '#fff', 'odd')", "expected one of"),
        ("draw:stroke(gmnb.path(), 0, '#fff')", "must be above 0"),
        (
            "draw:stroke(gmnb.path(), { width = 1, cap = 'flat' }, '#fff')",
            "expected one of",
        ),
        ("draw:shadow(0, 0, 1, 1, 0, { blur = 2 })", "needs `color`"),
        ("draw:push_shadow({})", "0 shadows"),
        ("draw:push_mask('negative')", "expected one of"),
        ("draw:push_blend()", "expected one of"),
        ("draw:text(12, 0, 0, nil, '#fff')", "expected text"),
        ("draw:image(7, 0, 0, 1, 1)", "expected a picture"),
        (
            "draw:image({ id = 5 }, 0, 0, 1, 1)",
            "not a picture of this theme",
        ),
        (
            "draw:glyph({ index = 1 }, '#fff')",
            "isn't one of the value's 0",
        ),
        ("draw:pop()", "nothing is pushed"),
        ("draw:restore()", "nothing is saved"),
        ("draw:save() draw:pop()", "restore it first"),
        ("draw:push_opacity(1) draw:restore()", "pop it first"),
        ("gmnb.path():line_to(1)", "path:line_to: expected numbers"),
        ("gmnb.svg_path('M0 0 Q1')", "gmnb.svg_path"),
        ("gmnb.noise({ seed = -1 })", "gmnb.noise: `seed`"),
        ("gmnb.noise({ size = 0 })", "gmnb.noise: `size`"),
    ] {
        let (theme, _d) = background("args", "", call);
        let failure = message(run(&theme).unwrap_err());
        assert!(failure.contains(expect), "{call}: {failure}");
        // The theme's own line is in the message.
        assert!(failure.contains("theme.luau"), "{call}: {failure}");
    }
}

#[test]
fn numbers_out_of_range_are_brought_into_range() {
    let (theme, _d) = background(
        "clamp",
        "",
        r##"
        draw:rect(0, 0, -5, 1e30, { r = 2, g = -1, b = 0.5, a = 7 })
        draw:push_opacity(3)
        draw:pop()
        draw:circle(0, 0, -4, "#fff")
        "##,
    );
    let ops = ops(&theme);
    assert_eq!(
        ops[0],
        Op::Rect {
            rect: [0.0, 0.0, 0.0, 1.0e7],
            paint: Paint::Color([1.0, 0.0, 0.5, 1.0]),
        }
    );
    assert_eq!(ops[1], Op::PushOpacity(1.0));
    assert!(matches!(ops[3], Op::Circle { radius, .. } if radius == 0.0));
}

/// Scales within the coordinates' range multiply: six of 10^7 are past a
/// float's, and the toolkit's software renderer, given that and a blur,
/// stopped drawing the whole window (Settings too). Past 10^14 together
/// a scale is refused; a save or a push gives back what it held.
#[test]
fn scales_one_inside_another_stay_within_what_can_be_drawn() {
    let (theme, _d) = background(
        "zoom",
        "",
        r##"
        draw:save()
        for _ = 1, 6 do draw:scale(1e7, 1e7) end
        draw:push_blur(4)
        draw:rect(0, 0, 1, 1, "#ff0000")
        draw:pop()
        draw:restore()
        "##,
    );
    let failed = message(run(&theme).unwrap_err());
    assert!(failed.contains("draw:scale"), "{failed}");
    // Two are 10^14: drawn. Each inside its own save, or its own push,
    // or one of a mask's two parts, any number of times: drawn.
    let (theme, _d) = background(
        "zoom-kept",
        "",
        r##"
        draw:save()
        draw:scale(1e7)
        draw:scale(-1e7, 0.5)
        draw:rect(0, 0, 1e-7, 1e-7, "#ff0000")
        draw:restore()
        for _ = 1, 20 do
            draw:save()
            draw:scale(1e7, 1e7)
            draw:restore()
            draw:push_opacity(0.5)
            draw:scale(1e7, 1e7)
            draw:pop()
            draw:push_mask()
            draw:scale(1e7, 1e7)
            draw:scale(1e7, 1e7)
            draw:pop()
            draw:scale(1e7, 1e7)
            draw:scale(1e7, 1e7)
            draw:pop()
        end
        draw:scale(1e7, 1e-3)
        draw:scale(0.5, 1e7)
        draw:scale(0.5)
        "##,
    );
    let ops = ops(&theme);
    assert_eq!(ops[1], Op::Scale(1.0e7, 1.0e7));
    assert_eq!(ops[2], Op::Scale(-1.0e7, 0.5));
    assert_eq!(*ops.last().unwrap(), Op::Scale(0.5, 0.5));
    // A scale that shrinks takes nothing back: the toolkit folds scales
    // that follow one another into one, and six of 10^7 after four of
    // 10^-7 would be past a float's range by themselves.
    let (theme, _d) = background(
        "zoom-folded",
        "",
        r##"
        for _ = 1, 4 do draw:scale(1e-7) end
        draw:translate(1, 1)
        for _ = 1, 6 do draw:scale(1e7) end
        draw:push_blur(4)
        draw:rect(0, 0, 1, 1, "#ff0000")
        draw:pop()
        "##,
    );
    let failed = message(run(&theme).unwrap_err());
    assert!(failed.contains("draw:scale"), "{failed}");
    // Whatever the scales, nothing is drawn more than 10^8 pixels from
    // the layer's origin: given a picture 10^11 across to shadow (a scale
    // of 10^4 and a rect of 10^7), the software renderer crashed.
    for (body, drawn) in [
        ("draw:scale(10) draw:rect(-1e7, 0, 1, 1e7, '#fff')", true),
        ("draw:scale(100) draw:rect(-1e7, 0, 1, 1, '#fff')", false),
        (
            "draw:scale(10) draw:push_shadow({ { blur = 4, color = '#000' } }) draw:rect(5e6, 0, 1e7, 1, '#fff') draw:pop()",
            false,
        ),
        ("draw:scale(1e4) draw:circle(0, 0, 1e5, '#fff')", false),
        ("draw:scale(1e4) draw:circle(9e3, 0, 1e3, '#fff')", true),
        (
            "draw:scale(1e3) draw:fill(gmnb.svg_path('M0 0 L1e6 0 L0 1 Z'), '#fff')",
            false,
        ),
        (
            "draw:scale(1e3) draw:fill(gmnb.svg_path('M0 0 L24 0 L0 24 Z'), '#fff')",
            true,
        ),
        (
            "draw:scale(1e3) draw:stroke(gmnb.path():move_to(0, 0):line_to(9e4, 0), 1, '#fff')",
            true,
        ),
        (
            "draw:scale(1e3) draw:stroke(gmnb.path():move_to(0, 0):line_to(1e5, 1), 2, '#fff')",
            false,
        ),
        (
            "draw:scale(1e4) draw:text('8', 1e4, 0, { size = 20 }, '#fff')",
            false,
        ),
        (
            "draw:scale(1e5) draw:shadow(0, 0, 10, 10, 0, { x = 1e3, blur = 8, color = '#000' })",
            false,
        ),
    ] {
        let (theme, _d) = background("reach", "", body);
        let result = run(&theme);
        assert_eq!(result.is_ok(), drawn, "{body}: {result:?}");
        if let Err(failure) = result {
            assert!(message(failure).contains("10^8"), "{body}");
        }
    }
    // A shadow's blur is within the cap with the scales in force: blurred
    // by a million pixels or so (a scale of 10^6 and a blur of 4, on a
    // screen of twice the pixels), the shadow of a 1 × 1 rect crashed the
    // software renderer.
    for (body, drawn) in [
        (
            "draw:push_shadow({ { blur = 100, color = '#000' } }) draw:pop()",
            true,
        ),
        (
            "draw:scale(2) draw:push_shadow({ { blur = 50, color = '#000' } }) draw:pop()",
            true,
        ),
        (
            "draw:scale(1e4) draw:push_shadow({ { blur = 100, color = '#000' } }) draw:pop()",
            false,
        ),
        (
            "draw:scale(1e6) draw:push_shadow({ { x = 2, blur = 4, color = '#000' } }) draw:pop()",
            false,
        ),
        (
            "draw:save() draw:scale(1e6) draw:restore() draw:push_shadow({ { blur = 4, color = '#000' } }) draw:pop()",
            true,
        ),
    ] {
        let (theme, _d) = background("shadow-zoom", "", body);
        let result = run(&theme);
        assert_eq!(result.is_ok(), drawn, "{body}: {result:?}");
        if let Err(failure) = result {
            assert!(
                message(failure).contains("with the scales in force"),
                "{body}"
            );
        }
    }
    // And one more after those three is refused, by itself: a hook that
    // catches the error draws what it drew.
    let (theme, _d) = background(
        "zoom-caught",
        "",
        r##"
        draw:scale(1e7)
        draw:scale(1e7)
        local ok, why = pcall(draw.scale, draw, 1.5)
        assert(not ok and string.find(tostring(why), "draw:scale"), tostring(why))
        draw:rect(0, 0, 1e-7, 1e-7, "#ff0000")
        "##,
    );
    assert_eq!(ops_count(&theme), 3);
}

/// A circle, or a path's rounded corner, of radius 10^7 cost the toolkit
/// a tenth of a second to draw, each (a path may hold 16 384), and so did
/// one of radius 10^6 a million pixels out: the toolkit works its circles
/// and corners out in single precision, to half a unit. Radii are taken up
/// to 10^6, and a circle or a rounded rectangle reaching past 10^5 is
/// given as the cubic curves it is made of, which cost no more out there.
/// (SVG data reaches no further than 10^6: its own test, in `path.rs`.)
#[test]
fn circles_and_rounded_corners_far_out_are_drawn_as_curves() {
    let (theme, _d) = background(
        "radius",
        r##"local p = gmnb.path():circle(1, 2, 50):circle(1, 2, 1e7)
            :rounded_rect(0, 0, 4e7, 4e7, { 1e7, 5, 2e6, 0 }):rounded_rect(1, 2, 30, 40, 99)"##,
        r##"
        draw:circle(3, 4, 10, "#ffffff")
        draw:circle(3, 4, 1e7, "#ffffff")
        draw:circle(1e6, 0, 1, "#ffffff")
        draw:fill(p, "#ffffff")
        local ok, why = pcall(gmnb.svg_path, "M0 0 A1e12 1e12 0 1 1 10 10 Z")
        assert(not ok and string.find(tostring(why), "past"), tostring(why))
        "##,
    );
    let ops = ops(&theme);
    // About the window: the toolkit's own circle.
    assert!(
        matches!(ops[0], Op::Circle { center: [3.0, 4.0], radius, .. } if radius == 10.0),
        "{:?}",
        ops[0]
    );
    // Far out, or far reaching: curves, the radius taken up to 10^6.
    let curves = |op: &Op| match op {
        Op::Fill { path, .. } => path.segments.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        curves(&ops[1]),
        gmnb_theme::path::circle_curves(3.0, 4.0, 1.0e6)
    );
    assert_eq!(
        curves(&ops[2]),
        gmnb_theme::path::circle_curves(1.0e6, 0.0, 1.0)
    );
    let k = 0.552_284_8f32;
    assert_eq!(
        curves(&ops[2])[..2],
        [
            Segment::MoveTo(1.0e6 + 1.0, 0.0),
            Segment::CurveTo(1.0e6 + 1.0, k, 1.0e6 + k, 1.0, 1.0e6, 1.0),
        ]
    );
    let path = curves(&ops[3]);
    assert_eq!(path[0], Segment::Circle(1.0, 2.0, 50.0));
    assert_eq!(
        path[1..7],
        gmnb_theme::path::circle_curves(1.0, 2.0, 1.0e6)[..]
    );
    // The rounded rectangle, coordinates taken up to 10^7, its corners
    // to 10^6, clockwise from its top left.
    let rounded =
        gmnb_theme::path::rounded_curves(0.0, 0.0, 1.0e7, 1.0e7, &Radius([1.0e6, 5.0, 1.0e6, 0.0]));
    assert_eq!(path[7..17], rounded[..]);
    assert_eq!(rounded[0], Segment::MoveTo(1.0e6, 0.0));
    assert_eq!(rounded[1], Segment::LineTo(1.0e7 - 5.0, 0.0));
    assert_eq!(rounded[3], Segment::LineTo(1.0e7, 1.0e7 - 1.0e6));
    assert_eq!(rounded[5], Segment::LineTo(0.0, 1.0e7));
    assert_eq!(rounded[7], Segment::LineTo(0.0, 1.0e6));
    // A small one stays the toolkit's, its corners as given (the toolkit
    // scales them down to fit).
    assert_eq!(
        path[17],
        Segment::RoundedRect(1.0, 2.0, 30.0, 40.0, Radius([99.0; 4]))
    );
    assert_eq!(path.len(), 18);
    // Corners too large for the rectangle are scaled down together.
    let fitted = gmnb_theme::path::rounded_curves(0.0, 0.0, 10.0, 40.0, &Radius([10.0; 4]));
    assert_eq!(fitted[0], Segment::MoveTo(5.0, 0.0));
    assert_eq!(fitted[1], Segment::LineTo(5.0, 0.0));
    assert_eq!(fitted[3], Segment::LineTo(10.0, 35.0));
}

fn ops_count(theme: &Theme) -> usize {
    ops(theme).len()
}

#[test]
fn the_draw_table_only_works_inside_a_hook_and_cannot_be_changed() {
    let (mut theme, _d) = load(
        "outside",
        r##"
        local kept
        return { api = 1, name = "T",
            style = function(ctx)
                if kept then kept:rect(0, 0, 1, 1, "#fff") end
                return {}
            end,
            layers = { background = { draw = function(draw, frame)
                kept = draw
                local ok = pcall(function() draw.rect = nil end)
                local ok2 = pcall(function() frame.width = 1 end)
                local ok3 = pcall(function() frame.ctx.dark = true end)
                if ok or ok2 or ok3 then error("writable") end
            end } },
        }"##,
    );
    assert!(theme.style(&Ctx::default()).is_ok());
    assert!(run(&theme).is_ok());
    let dark = Ctx {
        dark: true,
        ..Ctx::default()
    };
    let e = theme.style(&dark).unwrap_err().to_string();
    assert!(
        e.contains("can only be called while a draw hook runs"),
        "{e}"
    );
}

// ---- Limits -------------------------------------------------------------------------

/// A frame over `cap` is dropped (also when the hook catches the error),
/// and the hook is switched off after three.
fn dropped(name: &str, top: &str, body: &str, cap: &str) {
    let (theme, _d) = background(name, top, body);
    for round in 1..=3 {
        match run(&theme) {
            Err(Failure::Failed(e)) => {
                let text = e.to_string();
                assert!(text.contains(cap), "{name} round {round}: {text}");
            }
            other => panic!("{name} round {round}: {other:?}"),
        }
    }
    assert_eq!(run(&theme), Err(Failure::Off), "{name}");
    assert!(!theme.draws(LayerKind::Background));
    assert_eq!(theme.hooks(), [("background.draw".to_string(), 3, true)]);

    // Caught by the hook itself, the frame is dropped all the same: the
    // hook returns as if nothing had happened...
    let caught = format!("pcall(function()\n{body}\nend)");
    let (theme, _d) = background(name, top, &caught);
    match run(&theme) {
        Err(Failure::Failed(ThemeError::Limit(text))) => assert!(text.contains(cap), "{text}"),
        other => panic!("{name}, caught: {other:?}"),
    }
    assert_eq!(theme.hooks()[0].1, 1);
    // ... or goes on drawing, and is told again.
    let caught = format!("pcall(function()\n{body}\nend)\ndraw:rect(0, 0, 1, 1, '#fff')");
    let (theme, _d) = background(name, top, &caught);
    match run(&theme) {
        Err(Failure::Failed(e)) => assert!(e.to_string().contains(cap), "{e}"),
        other => panic!("{name}, caught and drawn on: {other:?}"),
    }
}

#[test]
fn a_frame_over_a_cap_is_dropped_and_three_switch_the_hook_off() {
    dropped(
        "ops",
        "",
        "for i = 1, 4097 do draw:rect(0, 0, 1, 1, '#fff') end",
        "more than 4096 draw calls",
    );
    dropped(
        "pushes",
        "",
        "for i = 1, 33 do draw:push_opacity(0.9) end",
        "more than 32 pushes",
    );
    // A mask is two pushes.
    dropped(
        "masks",
        "",
        "for i = 1, 17 do draw:push_mask() end",
        "more than 32 pushes",
    );
    dropped(
        "saves",
        "",
        "for i = 1, 33 do draw:save() end",
        "more than 32 saves",
    );
    dropped(
        "segments",
        "local p = gmnb.path() for i = 1, 5000 do p:line_to(i, i) end",
        "for i = 1, 4 do draw:fill(p, '#fff') end",
        "more than 16384 path segments",
    );
    dropped(
        "onepath",
        "local p = gmnb.path() for i = 1, 16385 do p:close() end",
        "draw:stroke(p, 1, '#fff')",
        "more than 16384 path segments",
    );
    dropped(
        "svgsegments",
        "local p = gmnb.svg_path('M0 0' .. string.rep(' 1 1', 900))",
        "for i = 1, 19 do draw:fill(p, '#fff') end",
        "more than 16384 path segments",
    );
    dropped(
        "textbytes",
        "",
        "draw:text(string.rep('x', 257), 0, 0, nil, '#fff')",
        "text of more than 256 bytes",
    );
    dropped(
        "textsize",
        "",
        "draw:text_size(string.rep('x', 257), nil)",
        "text of more than 256 bytes",
    );
    dropped(
        "textcalls",
        "",
        "for i = 1, 33 do draw:text('x', 0, 0, nil, '#fff') draw:text_size('x', nil) end",
        "more than 64 text calls",
    );
    dropped(
        "blur",
        "",
        "draw:push_blur(100.5)",
        "a blur of more than 100 px",
    );
    dropped(
        "shadowblur",
        "",
        "draw:shadow(0, 0, 9, 9, 0, { blur = 101, color = '#000' })",
        "a shadow blurred by more than 100 px",
    );
    dropped(
        "pushshadowblur",
        "",
        "draw:push_shadow({ { blur = 2, color = '#000' }, { blur = 400, color = '#000' } })",
        "a shadow blurred by more than 100 px",
    );
}

/// What a frame costs to draw is capped (docs/theming.md, "Drawing per
/// frame"): each call is charged by what it covers of the layer and its
/// kind, and a 760 × 640 layer's frame has 1000 points, about 100 ms of
/// the software renderer's time or 256 MB of the GL renderer's memory.
/// Within every other cap, 1365 blurs of the whole window took over ten
/// seconds a frame, and 2000 circles over it held 4 GB on GL.
#[test]
fn what_a_frame_costs_to_draw_is_capped() {
    let window = Frame::sized(760.0, 640.0);
    let cost = |body: &str, f: &Frame| -> Result<f64, String> {
        let (theme, _d) = background("cost", "", body);
        let result = theme.draw(LayerKind::Background, &Ctx::default(), f);
        match result {
            Ok(_) => Ok(theme.api().frame_cost()),
            Err(e) => Err(message(e)),
        }
    };
    let blurs = |n: u32| {
        format!(
            "for _ = 1, {n} do draw:push_blur(100) draw:rect(0, 0, frame.width, frame.height, '#fff') draw:pop() end"
        )
    };
    // Three blurs of the whole window: drawn, most of what a frame may.
    let three = cost(&blurs(3), &window).unwrap();
    assert!((0.85..1.0).contains(&three), "{three}");
    // A fourth, or the 1365 the other caps let through: the frame is
    // dropped, the error saying why.
    for n in [4, 1365] {
        let e = cost(&blurs(n), &window).unwrap_err();
        assert!(e.contains("what one may to draw"), "{e}");
    }
    // Circles over the whole window (a mask each on GL): 71, not 72.
    let circles = |n: u32| format!("for _ = 1, {n} do draw:circle(380, 320, 600, '#ffffff10') end");
    assert!(cost(&circles(71), &window).is_ok());
    assert!(cost(&circles(72), &window).is_err());
    // Small ones cost what they cover: 2000 of radius 3, a fraction.
    let small = cost(
        "for i = 1, 2000 do draw:circle(i % 700, i % 600, 3, '#ffffff') end",
        &window,
    )
    .unwrap();
    assert!(small < 0.1, "{small}");
    // An effect is charged for what is drawn inside it (a blur's grown by
    // three times its radius): blurring a small picture a little is cheap;
    // inside a small clip, a large one is too.
    let blurred_small = cost(
        "for _ = 1, 40 do draw:push_blur(4) draw:rect(10, 10, 40, 40, '#fff') draw:pop() end",
        &window,
    )
    .unwrap();
    assert!(blurred_small < 0.5, "{blurred_small}");
    let clipped = cost(
        "draw:push_clip(0, 0, 76, 64) for _ = 1, 40 do draw:push_blur(100) draw:rect(0, 0, 760, 640, '#fff') draw:pop() end draw:pop()",
        &window,
    )
    .unwrap();
    assert!(clipped < 0.5, "{clipped}");
    // Through the transforms in force: a tenth of the window scaled by
    // ten is the window.
    assert_eq!(
        cost("draw:scale(10) for _ = 1, 3 do draw:push_blur(100) draw:rect(0, 0, 76, 64, '#fff') draw:pop() end", &window)
            .map(|c| (c * 100.0).round()),
        cost(&blurs(3), &window).map(|c| (c * 100.0).round())
    );
    assert!(
        cost("draw:scale(10) for _ = 1, 4 do draw:push_blur(100) draw:rect(0, 0, 76, 64, '#fff') draw:pop() end", &window)
            .is_err()
    );
    // What can't be seen costs nothing: off the layer.
    let off = cost(
        "for _ = 1, 100 do draw:push_blur(100) draw:rect(2000, 2000, 760, 640, '#fff') draw:pop() end",
        &window,
    )
    .unwrap();
    assert_eq!(off, 0.0);
    // A gradient costs its kind: twenty radial ones over the window are
    // over (the software renderer's radial gradient is 40 times a colour).
    let radial = |n: u32| {
        format!(
            "local g = gmnb.radial({{ colors = {{ '#ff0000', '#0000ff' }} }}) for _ = 1, {n} do draw:rect(0, 0, frame.width, frame.height, g) end"
        )
    };
    assert!(cost(&radial(18), &window).is_ok());
    assert!(cost(&radial(20), &window).is_err());
    // A layer has as many points for each 760 × 640 of its area, and
    // never fewer than a whole window's: over a quarter of the window,
    // four times as many (a small layer spends little: its sweeps in
    // sweep.rs, at 0 × 0 and 1 × 400, were refused when it had a quarter);
    // over twice the window, as many.
    let quarter = Frame::sized(380.0, 320.0);
    assert!(cost(&radial(72), &quarter).is_ok());
    assert!(cost(&radial(80), &quarter).is_err());
    let double = Frame::sized(1520.0, 640.0);
    assert!(cost(&radial(18), &double).is_ok());
    assert!(cost(&radial(20), &double).is_err());
    // What lies just off the layer is charged when a blur or a shadow
    // brings it into view: blurred, or moved by its offset.
    let off_blur = |n: u32| {
        format!(
            "for _ = 1, {n} do draw:push_blur(100) draw:rect(frame.width + 1, 0, frame.width, frame.height, '#fff') draw:pop() end"
        )
    };
    assert!(cost(&off_blur(8), &window).is_ok());
    assert!(cost(&off_blur(1365), &window).is_err());
    let off_shadow = "for _ = 1, 10 do draw:push_shadow({ { x = -1000, blur = 100, color = '#000' } }) draw:rect(frame.width + 1, 0, frame.width, frame.height, '#fff') draw:pop() end";
    assert!(cost(off_shadow, &window).is_err());
    // A blend is charged for all that can be seen, whatever is in it (the
    // software renderer blends whole layers): tiny ones as large ones.
    let tiny_blends = |n: u32| {
        format!(
            "for _ = 1, {n} do draw:push_blend('screen') draw:rect(10, 10, 1, 1, '#fff') draw:pop() draw:rect(10, 10, 1, 1, '#fff') draw:pop() end"
        )
    };
    assert!(cost(&tiny_blends(20), &window).is_ok());
    assert!(cost(&tiny_blends(21), &window).is_err());
    // A picture costs its own pixels however small it is drawn: here the
    // noise, 512 x 512, drawn 10 x 10 (a picture's pixels are what the
    // software renderer reads to scale it).
    let small_pictures = |n: u32| {
        format!(
            "local p = gmnb.noise({{ size = 512 }}) for _ = 1, {n} do draw:image(p, 0, 0, 10, 10) end"
        )
    };
    let each = cost(&small_pictures(1), &window).unwrap();
    assert!((0.02..0.03).contains(&each), "{each}");
    assert!(cost(&small_pictures(1000), &window).is_err());
    // A keypad's or a readout's layer is charged where the window shows
    // what it draws, beyond itself too, and has points for the window it
    // can draw over (here 2320 x 1380, about six and a half windows of
    // 760 x 640): blurs beyond the keypad but in the window cost each about
    // a whole window's worth of points; five are drawn, eight are not; the
    // same eight off the window cost nothing.
    let pad = Frame {
        window: Some([-400.0, -300.0, 1920.0, 1080.0]),
        ..Frame::sized(355.0, 355.0)
    };
    let keypad_blurs = |n: u32, x: u32| {
        format!(
            "return {{ api = 1, name = 'T', layers = {{ keypad = {{ draw = function(draw, frame) for _ = 1, {n} do draw:push_blur(100) draw:rect({x}, 0, 700, 700, '#fff') draw:pop() end end }} }} }}"
        )
    };
    for (n, x, drawn) in [(5, 800, true), (8, 800, false), (8, 4000, true)] {
        let (theme, _d) = load("cost-pad", &keypad_blurs(n, x));
        let result = theme.draw(LayerKind::Keypad, &Ctx::default(), &pad);
        assert_eq!(
            result.is_ok(),
            drawn,
            "{n} blurs at {x}: {:?}",
            result.map(|d| d.list.ops.len())
        );
    }
    // A frame that leaves an effect open is charged for it all the same.
    let e = cost(
        "for _ = 1, 3 do draw:push_blur(100) draw:rect(0, 0, frame.width, frame.height, '#fff') draw:pop() end draw:push_blur(100) draw:rect(0, 0, frame.width, frame.height, '#fff')",
        &window,
    )
    .unwrap_err();
    assert!(e.contains("what one may to draw"), "{e}");
}

#[test]
fn frames_at_the_caps_are_drawn() {
    let (theme, _d) = background(
        "atcaps",
        "local p = gmnb.path() for i = 1, 16384 do p:close() end",
        r##"
        draw:fill(p, "#fff")
        for i = 1, 32 do draw:push_opacity(0.9) end
        for i = 1, 32 do draw:pop() end
        for i = 1, 32 do draw:save() end
        for i = 1, 32 do draw:restore() end
        for i = 1, 64 do draw:text(string.rep("x", 256), 0, 0, nil, "#fff") end
        draw:push_blur(100)
        draw:pop()
        for i = 1, 4096 - 195 do draw:rect(0, 0, 1, 1, "#fff") end
        "##,
    );
    assert_eq!(ops(&theme).len(), 4096);
}

#[test]
fn what_is_left_open_is_closed_and_reported_once() {
    let (theme, _d) = background(
        "open",
        "",
        "draw:save() draw:push_opacity(0.5) draw:push_mask() draw:rect(0, 0, 1, 1, '#fff')",
    );
    let list = ops(&theme);
    assert_eq!(
        list[4..],
        [Op::Pop, Op::Pop, Op::Pop, Op::Restore],
        "{list:?}"
    );
    assert!(theme.api().was_unbalanced());
    let notes = theme.take_draw_notes();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0]
            .message
            .contains("3 pushes and 1 saves were left open")
    );
    // The next frame is closed too, and says nothing more.
    assert_eq!(ops(&theme).len(), 8);
    assert!(theme.take_draw_notes().is_empty());
}

#[test]
fn a_hook_that_fails_or_runs_out_of_time_draws_nothing_and_strikes() {
    let (theme, _d) = background("fails", "", "draw:rect(0, 0, 1, 1, '#fff') error('no')");
    for _ in 0..3 {
        assert!(matches!(
            run(&theme),
            Err(Failure::Failed(ThemeError::Runtime { .. }))
        ));
    }
    assert_eq!(run(&theme), Err(Failure::Off));

    let d = dir("slow");
    let limits = Limits {
        frame: Duration::from_millis(20),
        ..patient()
    };
    let theme = load_in(
        &d,
        "return { api = 1, name = 'T', layers = { background = { draw = function(draw) while true do end end } } }",
        limits,
    );
    for _ in 0..3 {
        assert!(matches!(
            run(&theme),
            Err(Failure::Failed(ThemeError::Timeout(_)))
        ));
    }
    assert_eq!(run(&theme), Err(Failure::Off));

    // A theme without the layer, or without a draw hook in it.
    let (theme, _d) = load(
        "nolayer",
        "return { api = 1, name = 'T', layers = { keypad = {} } }",
    );
    assert_eq!(run(&theme), Err(Failure::Off));
    assert_eq!(
        theme.draw(LayerKind::Keypad, &Ctx::default(), &frame()),
        Err(Failure::Off)
    );
    assert!(theme.hooks().is_empty());
}

#[test]
fn the_heap_cap_holds_through_the_librarys_functions() {
    // The library's functions build their values with the cap lifted (a
    // speed-up). A theme that keeps what they return in a table it made
    // beforehand allocates nothing itself: the cap must stop it all the
    // same, and stopping it unloads the theme.
    let cap = 2 << 20;
    let limits = Limits {
        memory: cap,
        ..patient()
    };
    let d = dir("heapcap");
    let theme = load_in(
        &d,
        r##"
        local kept = table.create(40000)
        local sixteen = {}
        for i = 1, 16 do sixteen[i] = "#ff8800" end
        local spec = { colors = sixteen }
        return { api = 1, name = "T", layers = { background = { draw = function(draw, frame)
            for i = 1, #kept == 0 and 40000 or 0 do
                kept[i] = gmnb.linear(spec)
            end
        end } } }
        "##,
        limits,
    );
    assert!(matches!(
        run(&theme),
        Err(Failure::Failed(ThemeError::Memory(_)))
    ));
    assert!(theme.is_dead());
    // Over by no more than the one value the last call built.
    let used = theme.engine().used_memory();
    assert!(
        used <= cap + (64 << 10),
        "{used} bytes in a heap capped at {cap}"
    );
    // Caught by the theme, it is still stopped: nothing more can be made.
    let d = dir("heapcap-caught");
    let theme = load_in(
        &d,
        r##"
        local kept = table.create(40000)
        local spec = { colors = { "#ff8800", "#0088ff", "#ff8800", "#0088ff", "#ff8800", "#0088ff", "#ff8800", "#0088ff" } }
        local made = 0
        return { api = 1, name = "T", layers = { background = { draw = function(draw, frame)
            for i = 1, 40000 do
                local ok, g = pcall(gmnb.linear, spec)
                if not ok then break end
                made += 1
                kept[made] = g
            end
            draw:rect(0, 0, made, 1, "#ffffff")
        end } } }
        "##,
        Limits {
            memory: cap,
            ..patient()
        },
    );
    let _ = run(&theme);
    let used = theme.engine().used_memory();
    assert!(
        used <= cap + (64 << 10),
        "{used} bytes in a heap capped at {cap}"
    );
}

// ---- Determinism ----------------------------------------------------------------------

#[test]
fn the_same_frame_draws_the_same_list() {
    let body = r##"
        local w, h = frame.width, frame.height
        for i = 1, 20 do
            local t = frame.time * 0.3 + i
            draw:circle(w * (0.5 + 0.4 * math.sin(t)), h * (0.5 + 0.4 * math.cos(t * 1.3)), 10 + i,
                gmnb.radial({ center = { w / 2, h / 2 }, radius = 40 + i,
                    colors = { gmnb.hsl(i * 18, 0.8, 0.6), gmnb.alpha("#ffffff", 0) } }))
        end
        draw:fill(gmnb.path():move_to(0, 0):line_to(w, h * math.sin(frame.time)):close(), "#00000040")
        return 0
    "##;
    let (a, _da) = background("same-a", "", body);
    let (b, _db) = background("same-b", "", body);
    let mut f = frame();
    for step in 0..5 {
        f.time = step as f64 * 0.37;
        let first = a.draw(LayerKind::Background, &Ctx::default(), &f).unwrap();
        let again = a.draw(LayerKind::Background, &Ctx::default(), &f).unwrap();
        let other = b.draw(LayerKind::Background, &Ctx::default(), &f).unwrap();
        assert_eq!(first, again);
        assert_eq!(first, other);
        assert_eq!(first.list.ops.len(), 21);
    }
}

// ---- Paths ------------------------------------------------------------------------------

#[test]
fn a_path_is_read_as_it_is_when_it_is_drawn() {
    let (theme, _d) = background(
        "paths",
        "local kept = gmnb.path():move_to(1, 1)",
        r##"
        draw:fill(kept, "#fff")
        kept:line_to(2, 2)
        draw:fill(kept, "#fff")
        -- An SVG path can be added to like any other.
        draw:fill(gmnb.svg_path("M0 0L4 4"):close(), "#fff")
        "##,
    );
    let ops = ops(&theme);
    let segments = |i: usize| match &ops[i] {
        Op::Fill { path, .. } => path.segments.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(segments(0), [Segment::MoveTo(1.0, 1.0)]);
    assert_eq!(
        segments(1),
        [Segment::MoveTo(1.0, 1.0), Segment::LineTo(2.0, 2.0)]
    );
    assert_eq!(
        segments(2),
        [Segment::Svg(Rc::from("M0 0L4 4")), Segment::Close]
    );
}

// ---- Pictures -----------------------------------------------------------------------------

/// FNV-1a, to pin pixels without keeping them.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn noise_is_the_recipe_the_contract_gives() {
    // The recipe, written out again here.
    let reference = |size: u32, seed: u32, color: Rgba, opacity: f32| {
        let mut s = seed;
        let mut out = Vec::new();
        for _ in 0..size * size {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let a = (s & 0xff) as f32 / 255.0 * opacity * color[3];
            for c in [color[0], color[1], color[2], 1.0] {
                out.push((c * a * 255.0) as u8);
            }
        }
        out
    };
    let (theme, _d) = load(
        "noise",
        r##"
        local dark = gmnb.noise({ size = 192, seed = 0x9e3779b9, color = gmnb.white, opacity = 0.07 })
        local same = gmnb.noise({ size = 192, color = "#ffffff", opacity = 0.07 })
        local plain = gmnb.noise({})
        local tinted = gmnb.noise({ size = 4, seed = 1, color = { r = 1, g = 0.5, b = 0, a = 0.5 }, opacity = 1 })
        assert(dark == same and dark ~= plain)
        assert(dark.width == 192 and dark.height == 192 and plain.width == 128 and tinted.width == 4)
        assert(not pcall(function() dark.width = 1 end))
        return { api = 1, name = "T" }
        "##,
    );
    let pictures = theme.api().pictures();
    assert_eq!(pictures.len(), 3);
    let pixels = |i: u32| match pictures.pixels(ImageId(i)) {
        Some(Pixels::Rgba8 {
            width,
            height,
            bytes,
        }) => {
            assert_eq!(bytes.len(), (width * height * 4) as usize);
            (width, bytes)
        }
        _ => panic!("no pixels"),
    };
    let (w, grain) = pixels(0);
    assert_eq!(w, 192);
    assert_eq!(grain, reference(192, 0x9e37_79b9, [1.0; 4], 0.07));
    // 0.2's grain: white specks, alpha at most 17/255.
    assert!(grain.chunks(4).all(|p| p[0] == p[3] && p[3] <= 17));
    assert_eq!(fnv(&grain), 0x63ea_aef0_441c_93c5, "{:#x}", fnv(&grain));
    let (_, plain) = pixels(1);
    assert_eq!(plain, reference(128, 0x9e37_79b9, [1.0; 4], 1.0));
    assert_eq!(fnv(&plain), 0x82de_7c9d_ba75_bf95, "{:#x}", fnv(&plain));
    let (_, tinted) = pixels(2);
    assert_eq!(tinted, reference(4, 1, [1.0, 0.5, 0.0, 0.5], 1.0));
    assert_eq!(
        pictures.get(ImageId(2)).map(|p| p.kind.clone()),
        Some(PictureKind::Noise(NoiseSpec {
            size: 4,
            seed: 1,
            color: [1.0, 0.5, 0.0, 0.5],
            opacity: 1.0
        }))
    );
}

#[test]
fn noise_is_capped_in_size_and_number() {
    let (theme, _d) = background(
        "noisecaps",
        r##"
        local big = gmnb.noise({ size = 4096 })
        assert(big.width == 512)
        local made = {}
        for seed = 1, 9 do made[seed] = gmnb.noise({ size = 2, seed = seed }) end
        -- The ninth (with `big`, the tenth asked for) is empty.
        assert(made[7].width == 2 and made[8].width == 0 and made[9].width == 0)
        "##,
        "draw:tile(made[9], 0, 0, 9, 9) draw:tile(made[1], 0, 0, 9, 9)",
    );
    // The empty picture draws nothing.
    let ops = ops(&theme);
    assert_eq!(ops.len(), 1);
    assert_eq!(theme.api().pictures().len(), 8);
    let notes: Vec<String> = theme
        .take_draw_notes()
        .into_iter()
        .map(|n| n.message)
        .collect();
    assert!(
        notes.iter().any(|n| n.contains("at most 512×512")),
        "{notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("may make 8 noise pictures")),
        "{notes:?}"
    );
}

/// A PNG's first bytes for a picture of this size (no pixels: only the
/// header is looked at when a theme asks for it).
fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
    out.extend(width.to_be_bytes());
    out.extend(height.to_be_bytes());
    out.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    out
}

fn jpeg_header(width: u16, height: u16) -> Vec<u8> {
    let mut out = vec![0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0, 0xff, 0xc0, 0, 11, 8];
    out.extend(height.to_be_bytes());
    out.extend(width.to_be_bytes());
    out.extend([3, 0, 0, 0]);
    out
}

#[test]
fn images_come_from_the_themes_folder_within_their_caps() {
    assert_eq!(
        gmnb_theme::picture::image_size(&png_header(640, 480)),
        Some((640, 480))
    );
    assert_eq!(
        gmnb_theme::picture::image_size(&jpeg_header(300, 200)),
        Some((300, 200))
    );
    // A header cut short has no size.
    assert_eq!(gmnb_theme::picture::image_size(b"GIF89a"), None);
    assert_eq!(
        gmnb_theme::picture::image_size(b"GIF89a\x40\x01\xf0\x00\x00\x00\x00"),
        Some((320, 240))
    );

    let d = dir("images");
    let images = d.0.join("images");
    std::fs::create_dir_all(&images).unwrap();
    std::fs::write(images.join("a.png"), png_header(64, 32)).unwrap();
    std::fs::write(images.join("b.jpg"), jpeg_header(300, 200)).unwrap();
    std::fs::write(images.join("wide.png"), png_header(2049, 10)).unwrap();
    std::fs::write(images.join("text.png"), b"not a picture").unwrap();
    let mut heavy = png_header(8, 8);
    heavy.resize((4 << 20) + 1, 0);
    std::fs::write(images.join("heavy.png"), heavy).unwrap();
    std::fs::write(d.0.join("outside.png"), png_header(4, 4)).unwrap();
    let theme = load_in(
        &d,
        r##"
        local a = gmnb.image("images/a.png")
        assert(a.width == 64 and a.height == 32 and a == gmnb.image("images/a.png"))
        local b = gmnb.image("images/b.jpg")
        assert(b.width == 300 and b.height == 200)
        for _, file in { "images/wide.png", "images/text.png", "images/heavy.png", "images/none.png",
            "../outside.png", "/etc/passwd", "images/../../x.png" } do
            local p = gmnb.image(file)
            assert(p.width == 0 and p.height == 0, file)
        end
        return { api = 1, name = "T", layers = { background = { draw = function(draw)
            draw:image(a, 0, 0, 64, 32)
            draw:image(gmnb.image("images/none.png"), 0, 0, 1, 1)
        end } } }
        "##,
        patient(),
    );
    assert_eq!(theme.api().pictures().len(), 2);
    assert_eq!(ops(&theme).len(), 1);
    let notes: Vec<String> = theme
        .take_draw_notes()
        .into_iter()
        .map(|n| n.message)
        .collect();
    for part in [
        "wide.png is 2049×10",
        "text.png is not a PNG, JPEG, WebP or GIF picture",
        "heavy.png is 4194305 bytes",
        "none.png",
        "outside.png",
        "/etc/passwd",
    ] {
        assert!(notes.iter().any(|n| n.contains(part)), "{part}: {notes:?}");
    }
    // Asked for again every frame, a missing picture is noted once.
    assert_eq!(notes.iter().filter(|n| n.contains("none.png")).count(), 1);

    // These files are headers and nothing more: enough for a theme to be
    // told a size, and no picture. When the app comes to draw one it has
    // no pixels: it keeps its size, draws nothing, and the author is told
    // once. It is not decoded twice.
    assert!(theme.api().pictures().pixels(ImageId(0)).is_none());
    assert!(theme.api().still(ImageId(0)).is_none());
    assert!(theme.api().still(ImageId(0)).is_none());
    assert_eq!(
        theme
            .api()
            .pictures()
            .get(ImageId(0))
            .map(|p| (p.width, p.height)),
        Some((64, 32))
    );
    let notes = theme.take_draw_notes();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].message.contains("a.png can't be decoded")
            && notes[0].message.contains("it draws nothing"),
        "{notes:?}"
    );

    // Sixteen images, and 32 MiB decoded in all.
    let d = dir("manyimages");
    for i in 1..=17 {
        std::fs::write(d.0.join(format!("{i}.png")), png_header(16, 16)).unwrap();
    }
    for i in 1..=3 {
        std::fs::write(d.0.join(format!("big{i}.png")), png_header(2048, 2048)).unwrap();
    }
    let theme = load_in(
        &d,
        r##"
        for i = 1, 16 do assert(gmnb.image(i .. ".png").width == 16) end
        assert(gmnb.image("17.png").width == 0)
        return { api = 1, name = "T" }
        "##,
        patient(),
    );
    assert_eq!(theme.api().pictures().len(), 16);
    let notes = theme.take_draw_notes();
    assert!(notes[0].message.contains("may load 16 images"), "{notes:?}");
    let theme = load_in(
        &d,
        r##"
        assert(gmnb.image("big1.png").width == 2048 and gmnb.image("big2.png").width == 2048)
        assert(gmnb.image("big3.png").width == 0)
        return { api = 1, name = "T" }
        "##,
        patient(),
    );
    let notes = theme.take_draw_notes();
    assert!(
        notes[0].message.contains("over 32 MiB decoded"),
        "{notes:?}"
    );
}

// ---- Text ------------------------------------------------------------------------------------

struct Fixed;

impl TextMeasure for Fixed {
    fn measure(&self, text: &str, font: &Font) -> (f32, f32) {
        (text.len() as f32 * 10.0, font.size + font.weight / 100.0)
    }
}

#[test]
fn text_is_measured_by_the_app_and_fonts_lie_on_the_body_text() {
    let (mut theme, _d) = load(
        "text",
        r##"
        return { api = 1, name = "T",
            style = function() return { fonts = { ui = "Serif" }, text = { body = { size = 17, weight = 450 } } } end,
            layers = { background = { draw = function(draw)
                local w, h = draw:text_size("abc", { size = 30 })
                draw:rect(0, 0, w, h, "#fff")
                draw:text("abc", 0, 0, { weight = 700, bogus = 1 }, "#fff")
                draw:text("abc", 0, 0, nil, "#fff")
            end } },
        }"##,
    );
    // Without a measurer: half an em a character, 1.2 em a line.
    let ops = ops(&theme);
    assert!(matches!(ops[0], Op::Rect { rect, .. } if rect == [0.0, 0.0, 45.0, 36.0]));
    theme.api().set_measure(Rc::new(Fixed));
    theme.style(&Ctx::default()).unwrap();
    let ops = ops_of(&theme);
    // Measured in the font given, laid onto the style's body text.
    assert!(
        matches!(ops[0], Op::Rect { rect, .. } if rect == [0.0, 0.0, 30.0, 34.5]),
        "{:?}",
        ops[0]
    );
    let font = |i: usize| match &ops[i] {
        Op::Text { font, .. } => font.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        (font(1).family.clone(), font(1).size, font(1).weight),
        (vec!["Serif".to_string()], 17.0, 700.0)
    );
    assert_eq!((font(2).size, font(2).weight), (17.0, 450.0));
    let notes = theme.take_draw_notes();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].message.contains("font.bogus"), "{notes:?}");
}

fn ops_of(theme: &Theme) -> Vec<Op> {
    ops(theme)
}

// ---- The frame ---------------------------------------------------------------------------------

#[test]
fn a_frame_is_what_the_contract_says() {
    let (theme, _d) = load(
        "frame",
        r##"
        local function same(a, b, where)
            if a ~= b then error(where .. ": " .. tostring(a) .. " ~= " .. tostring(b), 2) end
        end
        return { api = 1, name = "T",
            options = { { id = "living", type = "toggle", default = true } },
            layers = { overlay = { draw = function(draw, frame)
                same(frame.width, 400, "width") same(frame.height, 300, "height")
                same(frame.time, 12.5, "time") same(frame.dt, 0.016, "dt")
                same(frame.motion, true, "motion") same(frame.settled, false, "settled")
                same(frame.active, true, "active")
                same(frame.pointer.x, 30, "pointer.x") same(frame.pointer.y, 40, "pointer.y")
                same(frame.ctx.dark, true, "ctx.dark") same(frame.ctx.options.living, false, "ctx.options")
                same(frame.keys, nil, "keys") same(frame.value, nil, "value")
                same(#frame.events, 3, "events")
                local press, result, option = frame.events[1], frame.events[2], frame.events[3]
                same(press.kind, "press", "kind") same(press.age, 0.5, "age")
                same(press.x, 11, "x") same(press.y, 12, "y")
                same(press.rect.x, 1, "rect.x") same(press.rect.y, 2, "rect.y")
                same(press.rect.w, 30, "rect.w") same(press.rect.h, 40, "rect.h")
                same(press.page, "converter", "page") same(press.source, "key", "source")
                same(press.via, "pointer", "via")
                same(press.key.id, 7, "key.id") same(press.key.kind, "equals", "key.kind")
                same(result.kind, "result", "kind") same(result.age, 0.25, "age")
                same(result.x, 16, "x") same(result.y, 22, "y")
                same(result.source, nil, "source") same(result.key, nil, "key")
                same(option.kind, "option", "kind") same(option.page, "settings", "page")
                same(option.id, "palette", "id") same(option.value, "ember", "value")
                same(press.action, "clear", "action") same(result.action, nil, "action")
                -- Told, not asked: the frame can't be written to.
                for _, t in { frame, frame.events, press, press.rect, press.key, frame.pointer, frame.ctx } do
                    if pcall(function() t.x = 1 end) then error("writable") end
                end
            end } },
        }"##,
    );
    let mut press = Event::at(EventKind::Press, Page::Converter, [1.0, 2.0, 30.0, 40.0]);
    (press.x, press.y) = (11.0, 12.0);
    press.source = Some(PressSource::Key);
    press.via = Some(Via::Pointer);
    press.action = Some(gmnb_theme::events::Action::Clear);
    press.key = Some((Some(7), KeyKind::Equals));
    let result = Event::at(EventKind::Result, Page::Calculator, [1.0, 2.0, 30.0, 40.0]);
    let mut option = Event::at(EventKind::Option, Page::Settings, [0.0; 4]);
    option.option = Some(("palette".into(), OptionValue::Text("ember".into())));
    let f = Frame {
        time: 12.5,
        dt: 0.016,
        pointer: Some((30.0, 40.0)),
        events: vec![(0.5, &press), (0.25, &result), (0.0, &option)],
        ..frame()
    };
    let ctx = Ctx {
        dark: true,
        options: [("living".to_string(), OptionValue::Bool(false))].into(),
        ..Ctx::default()
    };
    if let Err(e) = theme.draw(LayerKind::Overlay, &ctx, &f) {
        panic!("{}", message(e));
    }
}

fn cell(id: u32, row: i32, col: i32) -> KeyCell {
    KeyCell {
        id: Some(id),
        kind: KeyKind::Number,
        rect: [col as f32 * 50.0, row as f32 * 40.0, 46.0, 36.0],
        row,
        col,
        radius: 4.0,
        enabled: true,
        hovered: false,
        pressed: false,
        checked: false,
    }
}

#[test]
fn a_keypad_frame_lists_its_cells_and_place_moves_them() {
    let (theme, _d) = load(
        "keypad",
        r##"
        local seen = {}
        return { api = 1, name = "T", layers = { keypad = {
            place = function(key, frame)
                assert(frame.keys[key.row * 2 + key.col + 1] == key)
                if key.id == 1 then return nil end
                if key.id == 2 then return { dy = 16, opacity = 0.5 } end
                if key.id == 3 then return { dx = -4, scale = 0.9 } end
                return { opacity = 0 }
            end,
            draw = function(draw, frame)
                local keys = frame.keys
                assert(#keys == 4)
                local k = keys[2]
                assert(k.id == 2 and k.kind == "number" and k.row == 0 and k.col == 1 and k.radius == 4)
                assert(k.rect.x == 50 and k.rect.y == 0 and k.rect.w == 46 and k.rect.h == 36)
                assert(keys[4].id == nil and keys[4].kind == "other" and keys[4].enabled == false)
                -- The same tables every frame, while the layout stands.
                if seen[1] then assert(seen[1] == keys and seen[2] == k) end
                seen[1], seen[2] = keys, k
                for _, key in keys do
                    if key.hovered then draw:rect(key.rect.x, key.rect.y, 1, 1, "#ff0000") end
                    if key.pressed then draw:rect(key.rect.x, key.rect.y, 2, 2, "#00ff00") end
                    if key.checked then draw:rect(key.rect.x, key.rect.y, 3, 3, "#0000ff") end
                end
                assert(not pcall(function() k.hovered = true end))
                assert(not pcall(function() k.rect.x = 0 end))
            end,
        } } }"##,
    );
    let mut cells = vec![cell(1, 0, 0), cell(2, 0, 1), cell(3, 1, 0), cell(4, 1, 1)];
    cells[3].id = None;
    cells[3].kind = KeyKind::Other;
    cells[3].enabled = false;
    let draw = |cells: &[KeyCell], layout: u64, motion: bool| {
        let f = Frame {
            motion,
            keys: Some(Keys {
                pad: 1,
                layout,
                cells,
            }),
            ..frame()
        };
        match theme.draw(LayerKind::Keypad, &Ctx::default(), &f) {
            Ok(d) => d,
            Err(e) => panic!("{}", message(e)),
        }
    };
    let drawn = draw(&cells, 1, true);
    assert!(drawn.list.is_empty());
    assert_eq!(
        drawn.places,
        [
            None,
            Some(Placement {
                dx: 0.0,
                dy: 16.0,
                scale: 1.0,
                opacity: 0.5
            }),
            Some(Placement {
                dx: -4.0,
                dy: 0.0,
                scale: 0.9,
                opacity: 1.0
            }),
            Some(Placement {
                dx: 0.0,
                dy: 0.0,
                scale: 1.0,
                opacity: 0.0
            }),
        ]
    );
    // States change in place.
    cells[1].hovered = true;
    cells[2].pressed = true;
    cells[2].checked = true;
    let drawn = draw(&cells, 1, true);
    assert_eq!(drawn.list.ops.len(), 3);
    cells[1].hovered = false;
    assert_eq!(draw(&cells, 1, true).list.ops.len(), 2);
    // `place` is not called under reduced motion.
    assert!(draw(&cells, 1, false).places.is_empty());
}

#[test]
fn a_keypad_whose_layout_changes_gets_new_cells() {
    let (theme, _d) = load(
        "relayout",
        r##"
        return { api = 1, name = "T", layers = { keypad = { draw = function(draw, frame)
            for _, key in frame.keys do draw:rect(key.rect.x, key.rect.y, key.rect.w, key.rect.h, "#fff") end
        end } } }"##,
    );
    let mut cells = vec![cell(1, 0, 0), cell(2, 0, 1)];
    let rects = |cells: &[KeyCell], pad: u64, layout: u64| {
        let f = Frame {
            keys: Some(Keys { pad, layout, cells }),
            ..frame()
        };
        let drawn = theme.draw(LayerKind::Keypad, &Ctx::default(), &f).unwrap();
        drawn
            .list
            .ops
            .iter()
            .map(|op| match op {
                Op::Rect { rect, .. } => rect[0],
                _ => -1.0,
            })
            .collect::<Vec<f32>>()
    };
    assert_eq!(rects(&cells, 1, 1), [0.0, 50.0]);
    cells[1].rect[0] = 70.0;
    cells.push(cell(3, 0, 2));
    assert_eq!(rects(&cells, 1, 2), [0.0, 70.0, 100.0]);
    // Another keypad has its own cells.
    assert_eq!(rects(&[cell(9, 0, 3)], 2, 1), [150.0]);
    assert_eq!(rects(&cells, 1, 2), [0.0, 70.0, 100.0]);
}

#[test]
fn place_shares_one_time_limit_and_a_failure_leaves_the_keys_alone() {
    let d = dir("placeslow");
    let limits = Limits {
        place: Duration::from_millis(30),
        ..patient()
    };
    let theme = load_in(
        &d,
        r##"
        return { api = 1, name = "T", layers = { keypad = {
            place = function(key)
                local t = os.clock()
                while os.clock() - t < 0.02 do end
                return { dy = 1 }
            end,
            draw = function(draw) draw:rect(0, 0, 1, 1, "#fff") end,
        } } }"##,
        limits,
    );
    let cells: Vec<KeyCell> = (0..8).map(|i| cell(i, 0, i as i32)).collect();
    let f = Frame {
        keys: Some(Keys {
            pad: 1,
            layout: 1,
            cells: &cells,
        }),
        ..frame()
    };
    // Eight calls of 20 ms don't fit 30 ms: no key is moved, and the
    // layer is still drawn.
    let drawn = theme.draw(LayerKind::Keypad, &Ctx::default(), &f).unwrap();
    assert!(drawn.places.is_empty());
    assert_eq!(drawn.list.ops.len(), 1);
}

fn glyphs(n: usize, x0: f32) -> Glyphs {
    Glyphs {
        glyphs: (0..n)
            .map(|i| GlyphBox {
                x: x0 + i as f32 * 20.0,
                y: 30.0,
                w: 18.0,
                h: 40.0,
            })
            .collect(),
        size: 52.0,
        rect: [x0, 30.0, n as f32 * 20.0, 40.0],
    }
}

#[test]
fn a_display_frame_hands_over_the_value_and_its_glyphs() {
    let (theme, _d) = load(
        "display",
        r##"
        return { api = 1, name = "T", layers = { display = { draw = function(draw, frame)
            local v = frame.value
            assert(v.text == "123" and #v.glyphs == 3 and v.size == 52)
            assert(v.rect.x == 100 and v.rect.w == 60 and v.band.x == 0 and v.band.w == 400)
            assert(v.error == false and v.align == "end" and v.role == "calculator")
            local g = v.glyphs[2]
            assert(g.index == 2 and g.x == 120 and g.y == 30 and g.w == 18 and g.h == 40)
            for _, glyph in v.glyphs do draw:glyph(glyph, "#ffffff") end
            if v.change then
                assert(v.change.kind == "replace" and v.change.from == 2)
                assert(#v.previous.glyphs == 2 and v.previous.size == 52 and v.previous.rect.x == 10)
                draw:push_opacity(1 - v.change.age)
                for _, glyph in v.previous.glyphs do draw:glyph(glyph, gmnb.linear({ colors = { "#fff", "#000" } })) end
                draw:pop()
                return 0
            end
            assert(v.previous == nil)
        end } } }"##,
    );
    let (now, old) = (glyphs(3, 100.0), glyphs(2, 10.0));
    let value = |version: u64, change: Option<f64>| DisplayValue {
        readout: 5,
        version,
        text: "123",
        glyphs: &now,
        band: [0.0, 26.0, 400.0, 48.0],
        error: false,
        align_end: true,
        role: Role::Calculator,
        change: change.map(|age| (ChangeKind::Replace, age, 2)),
        previous: change.map(|_| &old),
    };
    let draw = |v: DisplayValue| {
        let f = Frame {
            value: Some(v),
            ..frame()
        };
        match theme.draw(LayerKind::Display, &Ctx::default(), &f) {
            Ok(d) => d,
            Err(e) => panic!("{}", message(e)),
        }
    };
    let rest = draw(value(1, None));
    assert_eq!(rest.again, None);
    assert_eq!(
        rest.list.ops,
        (0..3)
            .map(|index| Op::Glyph {
                previous: false,
                index,
                paint: color("#ffffff")
            })
            .collect::<Vec<_>>()
    );
    // A change: the old value's glyphs can be drawn too, and its age moves on.
    for age in [0.0, 0.25] {
        let changing = draw(value(2, Some(age)));
        assert_eq!(changing.again, Some(0.0));
        assert_eq!(changing.list.ops[3], Op::PushOpacity(1.0 - age as f32));
        assert!(matches!(
            &changing.list.ops[4],
            Op::Glyph {
                previous: true,
                index: 0,
                paint: Paint::Gradient(_)
            }
        ));
        assert_eq!(changing.list.ops.len(), 7);
    }
    // And when it is over, they are gone.
    assert_eq!(draw(value(3, None)).list.ops.len(), 3);
}

// ---- Previews -------------------------------------------------------------------------------------

#[test]
fn a_preview_draws_a_tile_for_one_choice() {
    let (theme, _d) = load(
        "preview",
        r##"
        return { api = 1, name = "T", options = {
            { id = "palette", type = "choice", choices = { { id = "a" }, { id = "b" } },
              preview = function(draw, frame, choice)
                  assert(frame.width == 96 and frame.height == 56 and frame.ctx.dark == true)
                  if choice == "b" then error("no b") end
                  draw:rounded_rect(0, 0, frame.width, frame.height, 12, "#ff0000")
                  draw:push_opacity(0.5)
              end },
            { id = "plain", type = "choice", choices = { { id = "a" } } },
        } }"##,
    );
    let ctx = Ctx {
        dark: true,
        ..Ctx::default()
    };
    let option = &theme.def.options[0];
    assert!(matches!(
        &option.kind,
        OptionKind::Choice {
            preview: Some(_),
            ..
        }
    ));
    let tile = theme.preview(option, "a", &ctx).unwrap();
    assert_eq!(
        tile.ops,
        [
            Op::RoundedRect {
                rect: [0.0, 0.0, 96.0, 56.0],
                radius: Radius::all(12.0),
                paint: color("#ff0000")
            },
            Op::PushOpacity(0.5),
            Op::Pop,
        ]
    );
    assert!(matches!(
        theme.preview(option, "b", &ctx),
        Err(ThemeError::Runtime { .. })
    ));
    assert!(matches!(
        theme.preview(&theme.def.options[1], "a", &ctx),
        Err(ThemeError::Unsupported(_))
    ));
}

// ---- The built-in themes ----------------------------------------------------------------------------

fn built_in(id: &str, limits: Limits) -> Theme {
    Theme::load(Source::built_in(id).unwrap(), limits).unwrap()
}

#[test]
fn a_theme_without_layers_has_none() {
    let (theme, _dir) = load("no-layers", "return { api = 1, name = 'Plain' }");
    assert!(theme.def.layers.is_empty());
    assert!(theme.hooks().is_empty());
    for kind in LayerKind::ALL {
        assert!(!theme.draws(kind));
        assert_eq!(
            theme.draw(kind, &Ctx::default(), &frame()),
            Err(Failure::Off)
        );
    }
    assert!(theme.api().pictures().is_empty());
}

fn aurora_ctx(dark: bool, palette: &str, living: bool) -> Ctx {
    Ctx {
        dark,
        options: [
            ("palette".to_string(), OptionValue::Text(palette.into())),
            ("primary".to_string(), OptionValue::Text("#7c4dff".into())),
            ("secondary".to_string(), OptionValue::Text("#ff6fb5".into())),
            ("living".to_string(), OptionValue::Bool(living)),
        ]
        .into(),
        ..Ctx::default()
    }
}

/// A calculator keypad's cells, 6 rows of 4.
fn keypad_cells() -> Vec<KeyCell> {
    let mut cells = Vec::new();
    for row in 0..6 {
        for col in 0..4 {
            let mut c = cell((row * 4 + col) as u32, row, col);
            c.rect = [col as f32 * 90.0, row as f32 * 60.0, 85.0, 55.0];
            c.radius = 14.0;
            c.kind = match (row, col) {
                (5, 3) => KeyKind::Equals,
                (_, 3) => KeyKind::Operator,
                (0 | 1, _) => KeyKind::Func,
                _ => KeyKind::Number,
            };
            cells.push(c);
        }
    }
    cells
}

fn median(mut times: Vec<f64>) -> f64 {
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

/// Aurora's three layers in the real engine, under the real limits: what
/// they draw at rest and mid-animation, what they ask for, and what a
/// frame costs. (The limits are of processor time, so a shared machine
/// taking the processor away for a while fails nothing here.)
#[test]
fn auroras_layers_draw_within_their_limits() {
    let theme = built_in("aurora", common::limits());
    assert_eq!(
        theme
            .hooks()
            .iter()
            .map(|h| h.0.as_str())
            .collect::<Vec<_>>(),
        [
            "background.draw",
            "display.draw",
            "keypad.draw",
            "keypad.place"
        ]
    );
    let notes = &theme.notes;
    assert!(notes.is_empty(), "{notes:?}");
    let ok = |r: Result<Drawn, Failure>| match r {
        Ok(d) => d,
        Err(e) => panic!("{}", message(e)),
    };

    // ---- Background.
    for (dark, palette) in [
        (true, "aurora"),
        (false, "ember"),
        (true, "system"),
        (false, "custom"),
    ] {
        let still = aurora_ctx(dark, palette, false);
        let f = Frame::sized(760.0, 640.0);
        let drawn = ok(theme.draw(LayerKind::Background, &still, &f));
        // Over the sky, which is the window's own background: its stars
        // in the dark (a window of this size has all 160), the aurora's
        // light as a mask (a field over the top four fifths of the
        // window), the lights that show through it and the grain; and it
        // stands.
        let stars = if dark { 160 } else { 0 };
        let ops = &drawn.list.ops;
        assert_eq!(ops.len(), stars + 6, "{palette}");
        assert_eq!(drawn.again, None);
        assert!(matches!(ops[stars], Op::PushMask(MaskMode::Alpha)));
        assert!(
            matches!(&ops[stars + 1], Op::Field { rect, .. } if *rect == [0.0, 0.0, 760.0, 512.0])
        );
        assert!(matches!(ops[stars + 2], Op::Pop));
        assert!(
            matches!(&ops[stars + 3], Op::Rect { rect, paint: Paint::Gradient(g) }
                if *rect == [0.0, 0.0, 760.0, 640.0] && g.colors.len() == 6)
        );
        assert!(
            matches!(ops[stars + 5], Op::Tile { rect, .. } if rect == [0.0, 0.0, 760.0, 640.0])
        );
        // Living: again in a fifteenth of a second.
        let living = aurora_ctx(dark, palette, true);
        let drawn = ok(theme.draw(LayerKind::Background, &living, &f));
        assert_eq!(drawn.again, Some(0.066));
        // Under reduced motion it is a still picture whatever it returns.
        let quiet = Frame {
            motion: false,
            ..f.clone()
        };
        assert_eq!(
            ok(theme.draw(LayerKind::Background, &living, &quiet))
                .list
                .ops
                .len(),
            stars + 6
        );
    }
    // (The dark Aurora from here on: 160 stars and six more.)
    let standing = 166;
    let living = aurora_ctx(true, "aurora", true);
    let mut press = Event::at(
        EventKind::Press,
        Page::Calculator,
        [10.0, 300.0, 85.0, 55.0],
    );
    press.source = Some(PressSource::Key);
    press.via = Some(Via::Pointer);
    press.key = Some((Some(14), KeyKind::Number));
    let result = Event::at(
        EventKind::Result,
        Page::Calculator,
        [8.0, 50.0, 744.0, 120.0],
    );
    let mut option = Event::at(EventKind::Option, Page::Settings, [0.0, 0.0, 760.0, 640.0]);
    option.option = Some(("palette".into(), OptionValue::Text("aurora".into())));
    let events: Vec<(f64, &Event)> = (0..13).map(|i| (0.05 * i as f64, &press)).collect();
    let mut f = Frame {
        events,
        dt: 1.0 / 60.0,
        ..Frame::sized(760.0, 640.0)
    };
    let pulsing = ok(theme.draw(LayerKind::Background, &living, &f));
    // A number key's pulse is one ray, drawn into the aurora's mask; and
    // while one plays, thirty pictures a second.
    assert_eq!(pulsing.list.ops.len(), standing + 13);
    assert_eq!(pulsing.again, Some(0.032));
    let mut times = Vec::new();
    for _ in 0..200 {
        let t = Instant::now();
        ok(theme.draw(LayerKind::Background, &living, &f));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let background_ms = median(times);
    // A result's is a ray and the light it spreads round it; a new
    // palette sends four rays, one after another.
    f.events = vec![(0.2, &result), (0.4, &option)];
    assert_eq!(
        ok(theme.draw(LayerKind::Background, &living, &f))
            .list
            .ops
            .len(),
        standing + 2 + 4
    );
    f.events = vec![(0.1, &option)];
    assert_eq!(
        ok(theme.draw(LayerKind::Background, &living, &f))
            .list
            .ops
            .len(),
        standing + 2
    );

    // ---- Keypad: the cascade, a ripple, the pointer's light.
    let mut cells = keypad_cells();
    let pad_rect = [0.0, 0.0, 355.0, 355.0];
    let show = Event::at(EventKind::Show, Page::Calculator, pad_rect);
    fn keys(cells: &[KeyCell]) -> Keys<'_> {
        Keys {
            pad: 1,
            layout: 1,
            cells,
        }
    }
    let f = Frame {
        events: vec![(0.1, &show)],
        keys: Some(keys(&cells)),
        ..Frame::sized(355.0, 355.0)
    };
    let cascading = ok(theme.draw(LayerKind::Keypad, &living, &f));
    assert_eq!(cascading.again, Some(0.0));
    assert_eq!(cascading.places.len(), 24);
    // The first key is on its way; the last hasn't started.
    assert!(
        matches!(cascading.places[0], Some(p) if p.opacity > 0.0 && p.opacity < 1.0 && p.dy > 0.0)
    );
    assert_eq!(cascading.places[23].map(|p| p.opacity), Some(0.0));
    let f = Frame {
        events: vec![(1.4, &show)],
        keys: Some(keys(&cells)),
        ..Frame::sized(355.0, 355.0)
    };
    let landed = ok(theme.draw(LayerKind::Keypad, &living, &f));
    assert_eq!((landed.again, landed.list.ops.len()), (None, 0));
    assert!(landed.places.iter().all(Option::is_none));
    cells[14].hovered = true;
    let f = Frame {
        events: vec![(0.2, &press)],
        pointer: Some((60.0, 330.0)),
        keys: Some(keys(&cells)),
        ..Frame::sized(355.0, 355.0)
    };
    let lit = ok(theme.draw(LayerKind::Keypad, &living, &f));
    assert_eq!(lit.again, Some(0.0));
    let strokes = lit
        .list
        .ops
        .iter()
        .filter(|op| matches!(op, Op::Stroke { .. }))
        .count();
    assert!(strokes >= 6, "{strokes}");
    let mut times = Vec::new();
    for _ in 0..200 {
        let t = Instant::now();
        ok(theme.draw(LayerKind::Keypad, &living, &f));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let keypad_ms = median(times);

    // ---- Display: at rest, and each change mid-way.
    let (now, old) = (glyphs(9, 500.0), glyphs(4, 600.0));
    let value = |version: u64, error: bool, change: Option<(ChangeKind, f64)>| DisplayValue {
        readout: 1,
        version,
        text: "123456789",
        glyphs: &now,
        band: [0.0, 26.0, 744.0, 48.0],
        error,
        align_end: true,
        role: Role::Calculator,
        change: change.map(|(kind, age)| (kind, age, 9)),
        previous: change.map(|_| &old),
    };
    let display = |ctx: &Ctx, v: DisplayValue| {
        let f = Frame {
            value: Some(v),
            ..Frame::sized(744.0, 120.0)
        };
        ok(theme.draw(LayerKind::Display, ctx, &f))
    };
    let rest = display(&living, value(1, false, None));
    assert_eq!(rest.again, None);
    let count = |d: &Drawn, previous: bool| {
        d.list
            .ops
            .iter()
            .filter(|op| matches!(op, Op::Glyph { previous: p, .. } if *p == previous))
            .count()
    };
    // In the dark: the glow's copy and the masked digits.
    assert_eq!(count(&rest, false), 18);
    assert!(rest.list.ops.iter().any(|op| matches!(op, Op::PushBlur(_))));
    assert!(
        rest.list
            .ops
            .iter()
            .any(|op| matches!(op, Op::PushMask(MaskMode::Alpha)))
    );
    let light = display(&aurora_ctx(false, "aurora", true), value(1, false, None));
    assert_eq!(count(&light, false), 9);
    let mut version = 1;
    for (kind, age) in [
        (ChangeKind::Typing, 0.1),
        (ChangeKind::Result, 0.3),
        (ChangeKind::Result, 0.7),
        (ChangeKind::Replace, 0.1),
        (ChangeKind::Error, 0.2),
    ] {
        version += 1;
        let d = display(
            &living,
            value(version, kind == ChangeKind::Error, Some((kind, age))),
        );
        assert_eq!(d.again, Some(0.0), "{kind:?}");
        assert_eq!(
            count(&d, true),
            if kind == ChangeKind::Replace { 4 } else { 0 }
        );
    }
    // A change that has played out: one more picture, at rest.
    let done = display(&living, value(20, false, Some((ChangeKind::Typing, 2.0))));
    assert_eq!(done.again, None);
    let mut times = Vec::new();
    for i in 0..200 {
        let v = value(
            30,
            false,
            Some((ChangeKind::Result, 0.3 + i as f64 * 0.001)),
        );
        let f = Frame {
            value: Some(v),
            ..Frame::sized(744.0, 120.0)
        };
        let t = Instant::now();
        ok(theme.draw(LayerKind::Display, &living, &f));
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let display_ms = median(times);

    // ---- The palette tiles.
    let option = theme
        .def
        .options
        .iter()
        .find(|o| o.id == "palette")
        .unwrap();
    for choice in [
        "aurora", "ember", "abyss", "orchard", "graphite", "system", "custom",
    ] {
        let tile = theme.preview(option, choice, &living).unwrap();
        assert_eq!(tile.ops.len(), 9, "{choice}");
    }

    println!(
        "aurora, Luau per frame (median of 200): background with 13 pulses {background_ms:.3} ms, \
         keypad with pointer and ripple {keypad_ms:.3} ms, display mid-result {display_ms:.3} ms"
    );
    // The limit is 8 ms a frame; a frame is a small part of it.
    let limit = Limits::default().frame.as_secs_f64() * 1000.0;
    assert!(background_ms < limit / 4.0 && keypad_ms < limit / 4.0 && display_ms < limit / 4.0);
    assert!(
        theme
            .hooks()
            .iter()
            .all(|(_, strikes, off)| *strikes == 0 && !off)
    );
    let notes = theme.take_draw_notes();
    assert!(notes.is_empty(), "{notes:?}");
    // Its busiest frames cost within what a frame may to draw. The
    // aurora is one field behind one mask, and two gradients that run
    // straight down: standing, its background is a sixteenth of what a
    // frame may; with thirteen rays up at once, the dearest frame here, a
    // fifth. (0.2's four blobs and two veils, radial gradients the
    // software renderer draws at 5 ms the window each, were a fifth
    // standing and over half at their dearest.)
    let busiest = theme.api().busiest_frame();
    println!(
        "aurora: the busiest frame cost {:.1}% of what one may",
        busiest * 100.0
    );
    assert!(busiest > 0.0 && busiest < 0.3, "{busiest}");
}
