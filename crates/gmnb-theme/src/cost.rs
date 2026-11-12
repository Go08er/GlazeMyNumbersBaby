//! What a layer's frame costs the toolkit to draw, estimated as it is
//! recorded (docs/theming.md, "Limits": "Drawing per frame").
//!
//! Every cap on what a frame holds (4096 calls, 16 384 path segments, a
//! blur of 100 px) left frames that took the toolkit tens of seconds, or
//! gigabytes: 1365 blurs of the whole window, or 2000 circles over it on
//! the GL renderer, each of which holds a window-sized mask. So each call
//! is also charged what drawing it costs, by how much of the layer it
//! covers (its box, through the transform in force, cut to what can be
//! seen) and what kind of call it is; the effects (`push_blur`,
//! `push_shadow`, `push_mask`, `push_blend`, `push_opacity`) are charged
//! for the box around what was drawn inside them, when they are popped.
//!
//! The weights are what each kind of call was measured to cost, over the
//! whole of a 760 × 640 window, on the software renderer (processor time)
//! and on GL (memory held while the frame is drawn), as points of a
//! frame's allowance: 1000 points are 100 ms of the first or 256 MB of the
//! second, whichever a call costs more of. A layer has 1000 points for
//! each 760 × 640 of what it is charged over (its own area; a keypad's or a
//! readout's, the window's too), and never fewer than 1000, so a frame
//! of the whole window at that size costs at most about 100 ms or 256 MB;
//! a larger window, more.

use crate::draw::{BlendMode, Filter, LineCap, MaskMode, Op, Rect, Stroke};
use crate::types::{GradientKind, Paint, Shadow};

/// The area the weights are of: the window they were measured in.
pub const REFERENCE_AREA: f64 = 760.0 * 640.0;
/// A frame's allowance for each [`REFERENCE_AREA`] of its layer.
pub const POINTS: f64 = 1000.0;

/// Points for covering [`REFERENCE_AREA`] with a paint: a colour, or a
/// gradient. The software renderer's radial and conic gradients are 40 and
/// 100 times a colour's (4 stops, an ellipse: 5.2 ms over the window; 3
/// stops of a conic: 10.5 ms); a linear one's grows with its stops (2: 0.2
/// ms; 16: 3 ms), and past 6 the GL renderer draws one through a
/// picture of its own (3.3 MB over the window).
fn paint(p: &Paint) -> f64 {
    match p {
        Paint::Color(_) => 1.3,
        Paint::Gradient(g) => {
            let stops = g.colors.len().max(2) as f64;
            match g.kind {
                GradientKind::Linear => {
                    let gl = if stops > 6.0 { 13.0 } else { 0.0 };
                    let along = (2.2 + 2.5 * (stops - 2.0)).max(gl);
                    // Across the axes the toolkit can't draw it by rows or
                    // columns, whatever its stops (the probe, over the window:
                    // 2 stops at 30 degrees 6.5 ms, as 16; at 90, 0.5 ms
                    // translucent and 0.08 opaque).
                    if angled(g.angle) {
                        along.max(ANGLED)
                    } else if g.colors.iter().any(|c| c[3] < 1.0) {
                        along.max(TRANSLUCENT_LINEAR)
                    } else {
                        along
                    }
                }
                GradientKind::Radial => 52.0,
                GradientKind::Conic => 130.0,
            }
        }
    }
}

/// A linear gradient that runs across the axes, over the window, at least;
/// and one along them with a colour that isn't opaque.
const ANGLED: f64 = 65.0;
const TRANSLUCENT_LINEAR: f64 = 5.0;

/// Whether a linear gradient's angle (CSS degrees) is off the axes.
fn angled(degrees: f32) -> bool {
    let off = f64::from(degrees).rem_euclid(90.0);
    off > 0.01 && off < 89.99
}

/// A path, a circle or a stroke: the GL renderer makes a mask of it, as
/// large as what it covers (3.3 MB for the window; with a gradient, 6.1
/// MB), and paints through it.
fn masked(p: &Paint) -> f64 {
    match p {
        Paint::Color(_) => 12.7 + paint(p),
        gradient => 24.0 + paint(gradient),
    }
}
/// Text and glyphs: shaped and drawn (with a gradient, through a mask).
const TEXT: f64 = 7.0;
const TEXT_MASK: f64 = 16.0;
/// A picture scaled into its rectangle (a still, a frame of an
/// animation, a field's picture); a picture repeated.
const IMAGE: f64 = 8.2;
/// And for each [`REFERENCE_AREA`] of the picture's own pixels, however
/// small it is drawn: the software renderer reads them all to scale it
/// (a 2048 x 2048 still over a 760 x 640 window: 37 ms smooth and clean,
/// 14 ms smooth or hard; drawn 76 x 64, 8 ms).
fn source(filter: Filter) -> f64 {
    match filter {
        Filter::Trilinear => 43.0,
        Filter::Linear | Filter::Nearest => 16.0,
    }
}
const TILE: f64 = 5.5;
/// And for each [`REFERENCE_AREA`] of the picture's own pixels, however
/// little of the layer it covers: the software renderer makes a surface of
/// the whole picture each time it is repeated (50 tiles of 10 × 10 px of a
/// 2048 × 2048 still: 190 ms, for nothing the area charged; 200: 750 ms).
const TILE_SOURCE: f64 = 7.0;
/// And each tile call that is seen, however small (the probe: a 16 or 128
/// px picture in 10 x 10 rects, 0.1 to 0.4 ms a call).
const TILE_CALL: f64 = 4.0;
/// Points for each dash of a dashed stroke, up to [`DASH_UNITS`] of them in
/// a frame: a butt-capped dash is one unit, a round- or square-capped one
/// [`DASH_CAPPED`] (the software renderer, in parallel dashes of a stroke
/// 1.5 px wide: 0.1 to 0.2 us for a butt-capped dash up to 230 000 of them
/// in a frame of 1920 x 1080 and 0.3 us at 520 000 in 3840 x 2160, 13 MB;
/// 1.9 to 2 us for a round-capped one at 83 000 and 520 000, 100 MB; a
/// graph paper's grid of 40 000 dotted lines dashed 1 on, 2 off, each
/// crossing the others, 5 ms; in dashed lines that cross, 65 000 butt-capped
/// in 100 lines took 56 ms of a 760 x 640 frame, 16 000 round-capped 55 ms).
/// Past the units each costs [`DASH_OVER`], the weight the dashes had
/// before they were measured at those counts, for the toolkit's stroker
/// makes a shape of every dash, and the shapes of a path that crosses
/// itself cost it more than their number (the software renderer, 100 000
/// dashes of a zigzag of 2000 lines: 0.9 s; 560 000 of 400: 2 s; 1 120 000
/// of 800: 25 s and 460 MB; round caps, 1 400 000 of parallel lines: 15 s
/// and 2.1 GB). The units are not by the size of the layer, so that no
/// window is allowed more dashes than were measured; the points are not by
/// what can be seen, for dashes beyond the layer cost as much.
const DASH: f64 = 0.015;
const DASH_CAPPED: f64 = 3.0;
const DASH_UNITS: f64 = 262_144.0;
const DASH_OVER: f64 = 0.2;
/// And for each pixel of width of a stroke of round or square caps, for each
/// dash: 1.8 us a dash at 1.5 px, 8.6 at 20, 70 at 100, 340 at 600 (round
/// caps, 15 000 parallel dashes in a 760 x 640 frame).
const DASH_WIDE: f64 = 0.01;
/// Points for each unit of a path's strain (`path::strain`: a pair of its
/// lines whose boxes meet is one, and a pair that crosses four). The
/// tessellator spends time on each crossing: 16 million of them, the lines
/// of a star of 8000 corners, took the software renderer 12 s stroked and
/// 1.5 s filled, 0.75 µs and 0.1 µs each. (A fan of 8000 thick lines from
/// one point, which cross nowhere, took it 1.9 s; 3000 arches of one span
/// and as many short lines through their crowns, 3.2 s.)
const STRAIN_STROKE: f64 = 0.002;
const STRAIN_FILL: f64 = 0.00035;
const BORDER: f64 = 1.3;
/// `draw:shadow`, over the whole layer.
const BOX_SHADOW: f64 = 4.3;
/// And for each one seen at all, whatever it covers (8 to 22 us measured
/// for a shadow of no blur, 4 x 4 to 50 x 50 px).
const SHADOW_CALL: f64 = 0.15;
/// What the software renderer spends on a blurred box shadow (`probe`, in
/// `apps/gmnb/src/layers/costprobe.rs`; 100 us a point), by the shadow's
/// blur, its box and its corners, in the layer's pixels. It makes a surface
/// of the shadow and blurs it, and how much of it depends on which of three
/// ways it goes about it:
///
/// - an inset shadow, always: it costs by its blur alone, 0.1 ms at 3, 0.2
///   at 10, 0.9 at 30, 2.3 at 50, 7 at 100 in a box of 200 px or more (a
///   fraction of that in a box much smaller than the blur: 4 x 4 px blurred
///   by 100, 0.9 ms), and by the square of its corners' radius ([`CORNERS`]);
/// - an outset one whose box is more than its corners and its blur (a
///   radius and one and a half blurs less than half its shorter side): it
///   draws the four corners and stretches the sides, 0.1 to 0.8 ms
///   whatever its size (a panel of 740 x 600 px at a blur of 30, 0.1);
/// - an outset one of a box the blur or the corners swallow (a key's, a
///   pill, a small box, a round one): it blurs the whole of it, as it does
///   an inset one, and the more for what it covers (2 ms for 50 x 50 px at a
///   blur of 30, 5.8 for 4 x 4 at 100, 12 for 200 x 200 at 100, 18 for
///   740 x 740 at 100).
///
/// A blur of 3 or more px: below it the toolkit skips the blur.
fn blur_cost(blur: f64) -> f64 {
    if blur >= 2.5 {
        0.5 + 0.035 * blur.powf(1.75)
    } else if blur > 0.0 {
        0.3
    } else {
        0.0
    }
}

/// An inset shadow's blur costs less in a box much smaller than it: by the
/// box's shorter side over twice the blur, in the layer's pixels (30 x 30
/// px at a blur of 50: 0.1 of it; 100 x 100: 0.5; 200 x 200 and more: all).
fn inset_share(side: f64, blur: f64) -> f64 {
    let x = side / (2.0 * blur.max(1.0));
    if x >= 1.0 {
        1.0
    } else if x <= 0.5 {
        0.15
    } else {
        0.15 + 0.85 * (x - 0.5) / 0.5
    }
}

/// The corners of a shadow blurred as a whole: the toolkit blurs the whole
/// of a shadow whose corners are large, and what that takes goes as the
/// square of their radius (the largest, at most half the box's shorter
/// side). Measured over boxes of 740 x 740 px, inset: 0.7 ms for a radius
/// of 20, 2.2 for 100, 4.3 for 185, 6.5 for 259, 10.8 for 333, 14 for 366.
const CORNERS: f64 = 0.001;
/// An outset shadow blurred as a whole, for each pixel of its box (740 x 740
/// at a blur of 100: 7 ms more than the blur alone).
const WHOLE: f64 = 0.00013;
/// Points for an outset shadow that the toolkit draws by its corners:
/// 0.1 ms, 0.2 for a radius of 50 and a blur of 100.
const SLICED: f64 = 1.0;
const SLICED_RADIUS: f64 = 0.022;
const SLICED_BLUR: f64 = 0.07;

/// The points for one box shadow: its blur and its corners, over a box of
/// `w` x `h` px, corners of `radius` (cut to what the box leaves them),
/// all in the layer's pixels.
fn box_shadow(inset: bool, blur: f64, radius: f64, w: f64, h: f64, spread: f64) -> f64 {
    let side = w.min(h);
    let radius = radius.min(side / 2.0);
    if blur < 2.5 {
        return blur_cost(blur);
    }
    if inset {
        return blur_cost(blur) * inset_share(side, blur) + CORNERS * radius * radius;
    }
    let half = side / 2.0 + spread.max(0.0);
    if radius + 1.5 * blur < 0.95 * half {
        return SLICED + SLICED_RADIUS * radius + SLICED_BLUR * blur;
    }
    blur_cost(blur) + CORNERS * radius * radius + WHOLE * w * h
}
/// And for each rounded rectangle, by its largest corner radius (at most
/// half its shorter side, in the layer's pixels) however little of the
/// layer it covers: the toolkit makes a mask of its corners for each, and
/// more the larger they are. 400 of 200 × 200 px with a gradient took 67 ms
/// at a radius of 100, 31 at 30, 9 at none (0.17 ms each at 100: 700 ms of
/// the 4096 calls a frame has, for the 0.8 the area charged).
const CORNER: f64 = 0.02;
/// What each open clip that is not a plain box (rounded, or turned) adds to
/// the cost of what is drawn inside, as a share of it: the software
/// renderer takes each as a mask of its own, and a rectangle over the
/// window drawn inside 30 of them took 8 times what it takes inside one
/// (30 turned, rounded clips and 600 window-sized rectangles: 800 ms for
/// the 76 ms the area charged).
const CLIP_DEPTH: f64 = 0.25;
/// Fonts a frame may set text in (or measure it in) before each further
/// one is charged: the toolkit finds, instances and loads a font for each
/// size, weight and variation the first time it is asked for it, 1.4 to 4.5
/// ms, however little the text covers (64 calls with a weight each: 290 ms
/// a frame for a theme whose weights move; the area charged nothing).
const FREE_FONTS: usize = 8;
/// Points for each font past [`FREE_FONTS`] (4.5 ms).
const FONT: f64 = 45.0;
/// Points for each character the frame sets in a font that it hasn't set in
/// that font at that scale and turn yet, however little of the layer it
/// covers: the toolkit makes the glyph for it, 30 µs a glyph measured, a
/// frame's cost where its fonts move (64 texts of 85 different CJK
/// characters, in sizes that change each frame: 170 ms for the 2.4% the
/// area charged; the same text at 64 scales, 69 ms).
const GLYPH: f64 = 0.35;
/// Points for each script the frame sets text of in a font that the toolkit
/// has to find a font for (below), 2 ms measured: a text of a dozen scripts
/// in each of 8 sizes took 100 ms in the 8 fonts that are free.
const SCRIPT: f64 = 20.0;

/// The script of a character that takes the toolkit a font of its own to
/// set (0: the font's own, or one that costs little: Greek, Cyrillic, the
/// Indic scripts, Thai, Hebrew, the arrows and the maths measured 0 to 0.5
/// ms a font). A number for the scripts that do, a family of them sharing a
/// font (the ideographs, or emoji): CJK and Hangul 1.1 ms a font, Arabic's
/// presentation forms, the symbols and dingbats, emoji and the historic
/// scripts of the planes past the first.
fn script(c: char) -> u32 {
    match u32::from(c) {
        // Ideographs, kana, bopomofo, fullwidth forms and the planes of
        // more of them.
        0x2E80..0xA000 | 0xF900..0xFB00 | 0xFF00..0xFFF0 | 0x20000..0x40000 => 1,
        // Hangul.
        0x1100..0x1200 | 0xAC00..0xD800 => 2,
        // Symbols and dingbats.
        0x2600..0x2800 => 3,
        // Arabic presentation forms.
        0xFB50..0xFE00 | 0xFE70..0xFF00 => 4,
        // Emoji and pictographs.
        0x1F000..0x20000 => 5,
        // The historic scripts, a block of five hundred and twelve each.
        c @ 0x10000..0x1F000 => 0x1000 + (c >> 9),
        _ => 0,
    }
}

/// A clip with rounded corners.
const ROUNDED_CLIP: f64 = 0.9;
/// The effects: what they make and hold beside what is drawn in them.
const OPACITY: f64 = 13.0;
/// A mask, by its mode (the whole window drawn through a mask of 4 × 4 px:
/// alpha 0.3 ms a mask, luminance 0.6, inverted luminance 0.5, inverted
/// alpha 15 to 19 ms; through a mask the size of the window, 5.7).
fn mask_weight(mode: MaskMode) -> f64 {
    match mode {
        MaskMode::InvertedAlpha => 190.0,
        _ => 26.0,
    }
}
/// A blend, by its mode (the whole window, four blends each: multiply 6.4
/// ms, screen 5.5, overlay 6.6, color-dodge 10, soft-light 9.8, color 10.5,
/// luminosity 11.5, saturation 13.6, hue 19).
fn blend_weight(mode: BlendMode) -> f64 {
    match mode {
        BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity => 190.0,
        BlendMode::ColorDodge | BlendMode::ColorBurn | BlendMode::SoftLight => 100.0,
        _ => 48.0,
    }
}

/// `push_blur`: the software renderer's blur costs about the same whatever
/// its radius (14 ms for the window at 1, 30 at 100); GL's grows with it.
fn blur(radius: f32) -> f64 {
    if radius > 0.0 {
        140.0 + 1.6 * f64::from(radius)
    } else {
        0.0
    }
}

/// `push_shadow`: each of its shadows.
fn shadow(s: &Shadow) -> f64 {
    if s.blur > 0.0 {
        71.0 + 0.87 * f64::from(s.blur)
    } else {
        13.0
    }
}

/// x' = a x + c y + e, y' = b x + d y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Affine([f64; 6]);

impl Affine {
    const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    fn translate(&mut self, dx: f64, dy: f64) {
        let [a, b, c, d, e, f] = self.0;
        self.0 = [a, b, c, d, e + a * dx + c * dy, f + b * dx + d * dy];
    }

    fn scale(&mut self, sx: f64, sy: f64) {
        let [a, b, c, d, e, f] = self.0;
        self.0 = [a * sx, b * sx, c * sy, d * sy, e, f];
    }

    fn rotate(&mut self, degrees: f64) {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let [a, b, c, d, e, f] = self.0;
        self.0 = [
            a * cos + c * sin,
            b * cos + d * sin,
            c * cos - a * sin,
            d * cos - b * sin,
            e,
            f,
        ];
    }

    /// Whether it turns or slants (a box stays a box through anything
    /// else, a quarter turn too).
    fn turned(&self) -> bool {
        let [a, b, c, d, ..] = self.0;
        let small = |x: f64, y: f64| x.abs() <= 1e-9 * (1.0 + y.abs());
        !((small(b, a) && small(c, d)) || (small(a, b) && small(d, c)))
    }

    /// How much it enlarges, at most: a blur's radius in the layer's
    /// pixels is its own times this.
    fn magnitude(&self) -> f64 {
        let [a, b, c, d, ..] = self.0;
        a.hypot(b).max(c.hypot(d))
    }

    /// The box around a box `(x0, y0, x1, y1)`, mapped.
    fn bound(&self, [x0, y0, x1, y1]: [f64; 4]) -> [f64; 4] {
        let [a, b, c, d, e, f] = self.0;
        let mut out = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for (x, y) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
            let (u, v) = (a * x + c * y + e, b * x + d * y + f);
            out = [out[0].min(u), out[1].min(v), out[2].max(u), out[3].max(v)];
        }
        out
    }
}

fn union(a: Option<[f64; 4]>, b: [f64; 4]) -> [f64; 4] {
    match a {
        None => b,
        Some(a) => [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ],
    }
}

fn meet(a: [f64; 4], b: [f64; 4]) -> Option<[f64; 4]> {
    let m = [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ];
    (m[0] < m[2] && m[1] < m[3]).then_some(m)
}

fn area(b: Option<[f64; 4]>) -> f64 {
    b.map_or(0.0, |b| (b[2] - b[0]) * (b[3] - b[1]))
}

fn grow(b: [f64; 4], by: f64) -> [f64; 4] {
    [b[0] - by, b[1] - by, b[2] + by, b[3] + by]
}

/// A rectangle `(x, y, w, h)` as a box.
fn rect_box(r: &Rect) -> [f64; 4] {
    let [x, y, w, h] = r.map(f64::from);
    [x, y, x + w, y + h]
}

/// What a push or a save opened.
#[derive(Debug)]
struct Level {
    /// The transform when it opened: given back when it closes.
    m: Affine,
    /// Points for each [`REFERENCE_AREA`] of what is drawn inside it.
    weight: f64,
    /// How much further than what is drawn inside it it reaches (a
    /// shadow's offset and blur).
    reach: f64,
    /// Charged for the whole of what can be seen, whatever is drawn in it
    /// (a blend: the software renderer blends whole layers).
    whole: bool,
    /// A clip: what is drawn inside reaches no further (in the layer's
    /// coordinates), and its own area is what a rounded one costs.
    clip: Option<[f64; 4]>,
    /// A clip that is not a plain box on the layer: rounded, or turned.
    mask: bool,
    /// The box around what was drawn inside, in the layer's coordinates.
    content: Option<[f64; 4]>,
}

/// One frame's account.
#[derive(Debug)]
pub(crate) struct Meter {
    m: Affine,
    levels: Vec<Level>,
    /// What can be seen, in the layer's coordinates: the layer, or for a
    /// keypad or a readout (which may draw beyond itself) the layer grown
    /// by its size on every side.
    visible: [f64; 4],
    /// Where a readout's glyphs are, in the layer's coordinates.
    glyphs: [f64; 4],
    allowance: f64,
    spent: f64,
    /// The dashes of this frame's strokes so far, in [`DASH`] units.
    dash_units: f64,
    /// Points for the picture the next operation draws, by its own size.
    picture: f64,
    /// The fonts text has been set or measured in this frame.
    fonts: Vec<FontKey>,
    /// The characters set or measured in each of them (by its place in
    /// `fonts`) at each scale and turn, and their scripts.
    chars: std::collections::HashSet<(usize, char, [i32; 4])>,
    scripts: std::collections::HashSet<(usize, u32)>,
}

/// What the toolkit makes a font of: a family list, a size, a weight, a
/// slant and the variations.
type FontKey = (Vec<String>, u32, u32, bool, Vec<(String, u32)>);

impl Default for Meter {
    fn default() -> Self {
        Meter::new([0.0, 0.0], true, None, [0.0; 4], 1.0)
    }
}

impl Meter {
    /// A frame of a layer of `size` (`whole`: it covers its window; else
    /// the window is `window`, in the layer's coordinates, if known), its
    /// glyphs within `glyphs` (x, y, w, h), its allowance `scale` times
    /// the layer's own.
    pub(crate) fn new(
        size: [f32; 2],
        whole: bool,
        window: Option<[f32; 4]>,
        glyphs: [f32; 4],
        scale: f64,
    ) -> Meter {
        let (w, h) = (f64::from(size[0].max(1.0)), f64::from(size[1].max(1.0)));
        let visible = match (whole, window) {
            (true, _) => [0.0, 0.0, w, h],
            (false, Some(window)) => union(Some([0.0, 0.0, w, h]), rect_box(&window)),
            // Not known: the layer, grown by its size on every side.
            (false, None) => {
                let around = w.max(h);
                [-around, -around, w + around, h + around]
            }
        };
        Meter {
            m: Affine::IDENTITY,
            levels: Vec::new(),
            visible,
            glyphs: rect_box(&glyphs),
            // In proportion to what it is charged over (the layer; a
            // keypad's or a readout's, the window too), and never less than
            // a whole window's: a small layer (or one the toolkit hasn't
            // sized yet) spends little anyway, its calls charged only for
            // what it can show.
            allowance: POINTS * scale * area(Some(visible)).max(REFERENCE_AREA) / REFERENCE_AREA,
            spent: 0.0,
            dash_units: 0.0,
            picture: 0.0,
            fonts: Vec::new(),
            chars: Default::default(),
            scripts: Default::default(),
        }
    }

    /// `text` is about to be set (or measured) in `font`: the first few
    /// fonts of a frame are free, each past them is charged, and so is each
    /// character the font hasn't had yet and each script past Latin. `Err`:
    /// the frame is past its allowance (by that share).
    pub(crate) fn text(&mut self, font: &crate::types::Font, text: &str) -> Result<(), f64> {
        let key: FontKey = (
            font.family.clone(),
            font.size.to_bits(),
            font.weight.to_bits(),
            font.italic,
            font.variations
                .iter()
                .map(|(tag, v)| (tag.clone(), v.to_bits()))
                .collect(),
        );
        let at = match self.fonts.iter().position(|k| *k == key) {
            Some(at) => at,
            None => {
                self.fonts.push(key);
                if self.fonts.len() > FREE_FONTS {
                    self.spent += FONT;
                }
                self.fonts.len() - 1
            }
        };
        // A glyph is made for each size it is drawn at on the screen: the
        // transform in force, to a thousandth.
        let [a, b, c, d, ..] = self.m.0;
        let at_scale = [a, b, c, d].map(|v| (v * 1000.0).round() as i32);
        for c in text.chars().filter(|c| !c.is_whitespace()) {
            if self.chars.insert((at, c, at_scale)) {
                self.spent += GLYPH;
                let script = script(c);
                if script != 0 && self.scripts.insert((at, script)) {
                    self.spent += SCRIPT;
                }
            }
        }
        if self.spent > self.allowance {
            return Err(self.spent / self.allowance);
        }
        Ok(())
    }

    /// The next operation draws a picture of `pixels` of its own, scaled
    /// with `filter`.
    pub(crate) fn picture(&mut self, pixels: f64, filter: Filter) {
        self.picture = source(filter) * pixels / REFERENCE_AREA;
    }

    /// The next operation repeats a picture of `pixels` of its own.
    pub(crate) fn tiled(&mut self, pixels: f64) {
        self.picture = TILE_SOURCE * pixels / REFERENCE_AREA;
    }

    /// What the frame has spent, as a share of its allowance.
    pub(crate) fn share(&self) -> f64 {
        self.spent / self.allowance
    }

    /// `local`, through the transform in force, cut to what the levels
    /// open let through and to what can be seen: the box, if any is left.
    fn seen(&self, local: [f64; 4]) -> Option<[f64; 4]> {
        // What lies within an open blur's or shadow's reach of what can be
        // seen is seen once blurred or moved.
        let reach: f64 = self.levels.iter().map(|l| l.reach).sum();
        let mut b = meet(self.m.bound(local), grow(self.visible, reach))?;
        for level in &self.levels {
            if let Some(clip) = level.clip {
                b = meet(b, clip)?;
            }
        }
        Some(b)
    }

    fn charge(&mut self, weight: f64, b: Option<[f64; 4]>) -> Result<(), f64> {
        // Inside clips that are masks, everything is drawn through each.
        let masks = self.levels.iter().filter(|l| l.mask).count() as f64;
        self.spent += weight * (1.0 + CLIP_DEPTH * masks) * area(b) / REFERENCE_AREA;
        if self.spent > self.allowance {
            return Err(self.spent / self.allowance);
        }
        Ok(())
    }

    /// The operation is recorded: its points, and what it opens or
    /// closes. `Err`: the frame is past its allowance (by that share).
    pub(crate) fn take(&mut self, op: &Op) -> Result<(), f64> {
        let glyphs = self.glyphs;
        // Points beside the area's, if what is drawn is seen at all.
        let mut extra = 0.0;
        let (weight, local): (f64, Option<[f64; 4]>) = match op {
            Op::Translate(dx, dy) => {
                self.m.translate(f64::from(*dx), f64::from(*dy));
                return Ok(());
            }
            Op::Scale(sx, sy) => {
                self.m.scale(f64::from(*sx), f64::from(*sy));
                return Ok(());
            }
            Op::Rotate(degrees) => {
                self.m.rotate(f64::from(*degrees));
                return Ok(());
            }
            Op::Save => {
                self.open(0.0, 0.0, None, false);
                return Ok(());
            }
            Op::Restore | Op::Pop => return self.close(),
            Op::PushClip { rect, radius } => {
                let clip = meet(self.m.bound(rect_box(rect)), self.visible);
                let weight = if radius.is_some() { ROUNDED_CLIP } else { 0.0 };
                // Nothing drawn inside reaches past it; a rounded one
                // costs its own area.
                self.charge(weight, clip.and_then(|c| self.seen_clip(c)))?;
                let mask = radius.is_some() || self.m.turned();
                self.open(0.0, 0.0, Some(clip.unwrap_or([0.0; 4])), false);
                if let Some(level) = self.levels.last_mut() {
                    level.mask = mask;
                }
                return Ok(());
            }
            Op::PushOpacity(_) => {
                self.open(OPACITY, 0.0, None, false);
                return Ok(());
            }
            Op::PushBlur(r) => {
                let reach = 3.0 * f64::from(*r) * self.m.magnitude();
                self.open(blur(*r), reach, None, false);
                return Ok(());
            }
            Op::PushShadow(list) => {
                let weight = list.iter().map(shadow).sum();
                let reach = list
                    .iter()
                    .map(|s| f64::from(s.x.abs().max(s.y.abs()) + s.spread.abs() + 3.0 * s.blur))
                    .fold(0.0, f64::max);
                self.open(weight, reach * self.m.magnitude(), None, false);
                return Ok(());
            }
            Op::PushMask(_) | Op::PushBlend(_) => {
                // Two parts, each ended by a pop: the effect is charged
                // when the second ends, for both.
                let blend = matches!(op, Op::PushBlend(_));
                let weight = match op {
                    Op::PushBlend(mode) => blend_weight(*mode),
                    Op::PushMask(mode) => mask_weight(*mode),
                    _ => 0.0,
                };
                self.open(weight, 0.0, None, blend);
                self.open(0.0, 0.0, None, false);
                return Ok(());
            }
            Op::Rect { rect, paint: p } => (paint(p), Some(rect_box(rect))),
            Op::RoundedRect {
                rect,
                radius,
                paint: p,
            } => {
                let [x0, y0, x1, y1] = rect_box(rect);
                let half = (x1 - x0).abs().min((y1 - y0).abs()) / 2.0;
                let corner = f64::from(radius.uniform()).min(half) * self.m.magnitude();
                extra = CORNER * corner;
                (paint(p) + 0.3, Some(rect_box(rect)))
            }
            Op::Circle {
                center,
                radius,
                paint: p,
            } => {
                let ([x, y], r) = (center.map(f64::from), f64::from(*radius));
                (masked(p), Some([x - r, y - r, x + r, y + r]))
            }
            Op::Fill { path, paint: p, .. } => {
                self.cross(path, 0.0, STRAIN_FILL)?;
                (
                    masked(p),
                    crate::path::bounds(path).map(|b| b.map(f64::from)),
                )
            }
            Op::Stroke {
                path,
                stroke,
                paint: p,
            } => {
                let n = dashes(stroke, crate::path::length(path));
                // A butt-capped dash is one unit at DASH points; another
                // cap's is DASH_CAPPED units, and its disc is drawn as wide
                // as the stroke, which costs the wider it is.
                let (each, price) = if matches!(stroke.cap, LineCap::Butt) {
                    (1.0, DASH)
                } else {
                    let wide = f64::from(stroke.width) * self.m.magnitude();
                    (DASH_CAPPED, DASH * DASH_CAPPED + DASH_WIDE * wide)
                };
                let units = each * n;
                let cheap = (DASH_UNITS - self.dash_units).clamp(0.0, units) / each;
                self.spent += price * cheap + DASH_OVER.max(price) * (n - cheap);
                self.dash_units += units;
                self.cross(path, f64::from(stroke.width) / 2.0, STRAIN_STROKE)?;
                (
                    masked(p),
                    crate::path::bounds(path)
                        .map(|b| grow(b.map(f64::from), f64::from(stroke.width))),
                )
            }
            Op::Border { rect, width, .. } => {
                (BORDER, Some(grow(rect_box(rect), f64::from(*width))))
            }
            Op::Shadow {
                rect,
                radius,
                shadow: s,
            } => {
                let b = grow(
                    rect_box(rect),
                    f64::from(s.x.abs().max(s.y.abs()) + s.spread.abs() + 3.0 * s.blur),
                );
                // Its blur, and its corners', in the layer's pixels, if it
                // is seen at all.
                if self.seen(b).is_some() {
                    // The toolkit blurs by the shadow's own blur whatever
                    // the scale shrinks it to (a shadow of 7400 x 6200 px
                    // blurred by 20 at a scale of 0.1: 5.8 ms, as one over
                    // the window with corners of 300), so a scale that
                    // shrinks is none; its corners are as large as it makes
                    // them.
                    let by = self.m.magnitude();
                    let blur = f64::from(s.blur) * by.max(1.0);
                    let [x0, y0, x1, y1] = rect_box(rect);
                    let (w, h) = ((x1 - x0).abs() * by, (y1 - y0).abs() * by);
                    let corner = f64::from(radius.uniform()) * by;
                    self.spent += SHADOW_CALL
                        + box_shadow(s.inset, blur, corner, w, h, f64::from(s.spread) * by);
                }
                (BOX_SHADOW, Some(b))
            }
            Op::Text {
                text,
                at,
                font,
                paint: p,
            } => {
                let [x, y] = at.map(f64::from);
                let size = f64::from(font.size);
                let w = size * (text.len() as f64 + 1.0);
                (text_weight(p), Some([x, y - size, x + w, y + 2.0 * size]))
            }
            Op::Glyph { paint: p, .. } => (text_weight(p), Some(glyphs)),
            Op::Image { rect, .. } | Op::Animation { rect, .. } => (IMAGE, Some(rect_box(rect))),
            Op::Field { rect, picture } => {
                let pixels = f64::from(picture.width) * f64::from(picture.height);
                self.picture(pixels, Filter::Linear);
                (IMAGE, Some(rect_box(rect)))
            }
            Op::Tile { rect, .. } => {
                // (Charged with the picture's own pixels, if it is seen.)
                self.picture += TILE_CALL;
                (TILE, Some(rect_box(rect)))
            }
        };
        let picture = std::mem::take(&mut self.picture);
        let Some(local) = local else {
            return Ok(());
        };
        let b = self.seen(local);
        // A picture drawn at all costs its own pixels too.
        if b.is_some() && picture > 0.0 {
            self.spent += picture;
        }
        if b.is_some() {
            self.spent += extra;
        }
        if let Some(b) = b
            && let Some(level) = self.levels.last_mut()
        {
            level.content = Some(union(level.content, b));
        }
        self.charge(weight, b)
    }

    /// A path about to be filled or stroked (`half`: half the stroke's
    /// width): its strain charged at `price` a unit, counted as far as the
    /// frame has points for. `Err`: the frame is past its allowance (by
    /// that share).
    fn cross(&mut self, path: &crate::path::Path, half: f64, price: f64) -> Result<(), f64> {
        let room = ((self.allowance - self.spent) / price).max(0.0) + 1.0;
        self.spent += price * crate::path::strain(path, half, room);
        if self.spent > self.allowance {
            return Err(self.spent / self.allowance);
        }
        Ok(())
    }

    /// A clip's box, cut to the clips already open.
    fn seen_clip(&self, clip: [f64; 4]) -> Option<[f64; 4]> {
        let mut b = clip;
        for level in &self.levels {
            if let Some(c) = level.clip {
                b = meet(b, c)?;
            }
        }
        Some(b)
    }

    fn open(&mut self, weight: f64, reach: f64, clip: Option<[f64; 4]>, whole: bool) {
        self.levels.push(Level {
            m: self.m,
            weight,
            reach,
            whole,
            clip,
            mask: false,
            content: None,
        });
    }

    /// A pop or a restore: the level's effect charged for what was drawn
    /// in it, which then counts as drawn in the level around it.
    fn close(&mut self) -> Result<(), f64> {
        let Some(level) = self.levels.pop() else {
            return Ok(());
        };
        self.m = level.m;
        let Some(content) = level.content else {
            return Ok(());
        };
        let reached = if level.whole {
            Some(self.visible)
        } else {
            meet(grow(content, level.reach), self.visible)
        };
        let reached = match reached {
            Some(b) => self.seen_clip(b),
            None => None,
        };
        if let Some(b) = reached
            && let Some(outer) = self.levels.last_mut()
        {
            outer.content = Some(union(outer.content, b));
        }
        self.charge(level.weight, reached)
    }

    /// The frame is done: what is still open is closed.
    pub(crate) fn finish(&mut self) -> Result<(), f64> {
        while !self.levels.is_empty() {
            self.close()?;
        }
        Ok(())
    }
}

/// How many dashes a stroke of a path `length` long has, at most. The
/// pattern's numbers are on, off, on, off...; an odd list is gone through
/// twice, so that its numbers are on and off in turn. A dash of no length
/// is still a dash (a round cap makes a dot of it).
fn dashes(stroke: &Stroke, length: f64) -> f64 {
    let n = stroke.dash.len();
    if n == 0 {
        return 0.0;
    }
    let sum: f64 = stroke.dash.iter().map(|d| f64::from(*d)).sum();
    let (period, on) = if n.is_multiple_of(2) {
        (sum, n / 2)
    } else {
        (2.0 * sum, n)
    };
    if period > 0.0 {
        length / period * on as f64
    } else {
        0.0
    }
}

fn text_weight(p: &Paint) -> f64 {
    match p {
        Paint::Color(_) => TEXT,
        gradient => TEXT + TEXT_MASK + paint(gradient),
    }
}
