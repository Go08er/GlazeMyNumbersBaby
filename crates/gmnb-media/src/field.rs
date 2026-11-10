//! Fields (docs/theming-media.md, "The field pass, exactly"): a coarse
//! grid of numbers a theme computed, interpolated for every pixel of a
//! picture and only then coloured, through a ramp. The smooth part is
//! computed coarsely and the sharp part per pixel, so band edges stay
//! sharp.
//!
//! The pass is the contract's, step for step, in 32-bit floats: the same
//! field and ramp give the same bytes on every machine. Nothing here is
//! left to the compiler's taste (no fused multiply-add, no reordering),
//! and the square root and the rounding to whole numbers are exact
//! operations.

/// Entries per band in a ramp's table.
pub const ENTRIES: usize = 256;
/// The most pixels one pass computes.
pub const MAX_PIXELS: u32 = 262_144;
/// A ramp's colours.
pub const MIN_COLORS: usize = 2;
pub const MAX_COLORS: usize = 16;
/// A field's points along each side, and in all.
pub const MIN_SIDE: usize = 2;
pub const MAX_SIDE: usize = 256;
pub const MAX_POINTS: usize = 16_384;

/// The colours a field is drawn in: for each band, 256 colours from the
/// band's own to the next one's, blended between the ramp's two edges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ramp {
    /// After the last colour comes the first again.
    cycle: bool,
    /// How many colours.
    colors: usize,
    /// `bands × ENTRIES` colours, premultiplied RGBA8.
    table: Vec<[u8; 4]>,
}

impl Ramp {
    /// A ramp of 2 to 16 `colors` (each red, green, blue, opacity, 0 to
    /// 1, not premultiplied). Within a band the colour is the band's own
    /// up to the fraction `edge[0]`, the next one's from `edge[1]` on,
    /// and a smooth step between (`0 ≤ edge[0] ≤ edge[1] ≤ 1`).
    ///
    /// Entry `k` of band `b` is the colour at the fraction
    /// `(k + 0.5) / 256`: `t = clamp((f − e0) / (e1 − e0), 0, 1)` (with
    /// `e0 = e1`: 0 below it, 1 from it on), `s = t² (3 − 2t)`, and
    /// `a + (b − a) × s` on each sRGB-encoded channel and the opacity;
    /// stored premultiplied, each channel `floor(c × 255 + 0.5)`.
    pub fn new(colors: &[[f32; 4]], edge: [f32; 2], cycle: bool) -> Option<Ramp> {
        let n = colors.len();
        let [e0, e1] = edge;
        let sound = (MIN_COLORS..=MAX_COLORS).contains(&n)
            && (0.0..=1.0).contains(&e0)
            && (0.0..=1.0).contains(&e1)
            && e0 <= e1
            && colors.iter().flatten().all(|c| (0.0..=1.0).contains(c));
        if !sound {
            return None;
        }
        let bands = if cycle { n } else { n - 1 };
        let mut table = Vec::with_capacity(bands * ENTRIES);
        for band in 0..bands {
            let (a, b) = (colors[band], colors[(band + 1) % n]);
            for k in 0..ENTRIES {
                let f = (k as f32 + 0.5) / ENTRIES as f32;
                let t = if e0 == e1 {
                    if f < e0 { 0.0 } else { 1.0 }
                } else {
                    ((f - e0) / (e1 - e0)).clamp(0.0, 1.0)
                };
                let s = t * t * (3.0 - 2.0 * t);
                let mix = |i: usize| a[i] + (b[i] - a[i]) * s;
                let alpha = mix(3);
                let byte = |c: f32| (c * 255.0 + 0.5).floor() as u8;
                table.push([
                    byte(mix(0) * alpha),
                    byte(mix(1) * alpha),
                    byte(mix(2) * alpha),
                    byte(alpha),
                ]);
            }
        }
        Some(Ramp {
            cycle,
            colors: n,
            table,
        })
    }

    pub fn cycle(&self) -> bool {
        self.cycle
    }

    /// How many colours the ramp was made of.
    pub fn colors(&self) -> usize {
        self.colors
    }

    /// The colour at `position`, as the pass finds it: premultiplied
    /// RGBA8.
    pub fn at(&self, position: f32) -> [u8; 4] {
        self.table[index(position, self.colors as f32, self.cycle)]
    }
}

/// The largest whole number not above `x`, exactly, in float arithmetic
/// alone: no call (without SSE4.1 the library's `floor` is one), no
/// conversion to a whole-number type and no branch, so that a row of
/// pixels is worked four at a time.
#[inline(always)]
fn floor(x: f32) -> f32 {
    // Below 2²³ in size, adding 2²³ (of x's sign) leaves no room for a
    // fraction: the sum is x rounded to a whole number, and taking 2²³
    // away again is exact.
    const WHOLE: f32 = 8_388_608.0;
    let shift = WHOLE.copysign(x);
    let near = (x + shift) - shift;
    let down = if near > x { near - 1.0 } else { near };
    // From 2²³ on every float is a whole number already.
    if x.abs() < WHOLE { down } else { x }
}

/// Steps 5 to 7: where in a ramp of `n` colours the position `p` falls,
/// as an entry of its table (a whole number, still a float).
#[inline(always)]
fn slot(p: f32, n: f32, cycle: bool) -> f32 {
    // A position that is not finite counts as 0.
    let p = if p.is_finite() { p } else { 0.0 };
    let (p, bands) = if cycle {
        (p - n * floor(p / n), n)
    } else {
        (p.max(0.0).min(n - 1.0), n - 1.0)
    };
    // The band, and the fraction of the way through it. (A position so
    // large that the subtraction above lost it lands in the first or the
    // last band.)
    let band = floor(p).max(0.0).min(bands - 1.0);
    let entry = floor((p - band) * ENTRIES as f32)
        .max(0.0)
        .min(ENTRIES as f32 - 1.0);
    band * ENTRIES as f32 + entry
}

#[inline(always)]
fn index(p: f32, n: f32, cycle: bool) -> usize {
    slot(p, n, cycle) as i32 as usize
}

/// A field: `cols × rows` points of `channels` numbers each, row by row
/// from the top. Its corners sit on the corners of the picture.
#[derive(Clone, Copy, Debug)]
pub struct Field<'a> {
    pub data: &'a [f32],
    pub cols: usize,
    pub rows: usize,
    /// 1: a point's value is the number. 2: the length of the pair.
    pub channels: usize,
}

impl Field<'_> {
    /// Whether the field is one the pass takes: each side 2 to 256, at
    /// most 16 384 points, one or two channels, and as many numbers as
    /// that makes.
    pub fn sound(&self) -> bool {
        (MIN_SIDE..=MAX_SIDE).contains(&self.cols)
            && (MIN_SIDE..=MAX_SIDE).contains(&self.rows)
            && self.cols * self.rows <= MAX_POINTS
            && (1..=2).contains(&self.channels)
            && self.data.len() == self.cols * self.rows * self.channels
    }
}

/// A field's numbers from a theme's buffer: 32-bit floats, little-endian.
/// With `next`, a second snapshot of the same size, every value is
/// `data + (next − data) × mix`. A value that is not finite counts as 0.
pub fn values(data: &[u8], next: Option<&[u8]>, mix: f32) -> Vec<f32> {
    let read = |b: &[u8; 4]| f32::from_le_bytes(*b);
    let sane = |v: f32| if v.is_finite() { v } else { 0.0 };
    let data = data.as_chunks::<4>().0;
    match next {
        None => data.iter().map(|b| sane(read(b))).collect(),
        Some(next) => data
            .iter()
            .zip(next.as_chunks::<4>().0)
            .map(|(a, b)| {
                let (a, b) = (read(a), read(b));
                sane(a + (b - a) * mix)
            })
            .collect(),
    }
}

/// Step 1: how many pixels are computed for a rectangle of `w × h`.
/// `W = max(1, round(w × quality))`, `H` likewise; if `W × H` is over
/// `max_pixels`, both are multiplied by `√(max_pixels / (W × H))` and
/// rounded down.
pub fn output_size(w: f32, h: f32, quality: f32, max_pixels: u32) -> (u32, u32) {
    let side = |s: f32| {
        let n = (s * quality).round();
        // No picture is wider than a window many times over.
        if n.is_finite() {
            n.clamp(1.0, 1.0e7) as u128
        } else {
            1
        }
    };
    let (w, h) = (side(w), side(h));
    let max = u128::from(max_pixels.max(1));
    if w * h <= max {
        return (w as u32, h as u32);
    }
    // W × √(max / (W × H)) rounded down is the largest whole number whose
    // square is at most W × max / H: found in whole numbers, so that no
    // rounding of a square root decides a pixel.
    let scaled = |side: u128, other: u128| {
        let mut n = ((side * max) as f64 / other as f64).sqrt() as u128;
        while n * n * other > side * max {
            n -= 1;
        }
        while (n + 1) * (n + 1) * other <= side * max {
            n += 1;
        }
        n.max(1)
    };
    let (mut ow, mut oh) = (scaled(w, h), scaled(h, w));
    // A side kept at its least of one pixel: the other takes what is left.
    if ow * oh > max {
        if ow > oh {
            ow = (max / oh).max(1);
        } else {
            oh = (max / ow).max(1);
        }
    }
    (ow as u32, oh as u32)
}

/// Steps 2 to 7: the field coloured through the ramp into a picture of
/// `width × height` pixels, premultiplied RGBA8, `width × 4` bytes a row.
/// A position is `value × scale + offset`.
///
/// `field` must be [`Field::sound`]; `None` if it isn't, or the picture
/// has no pixels.
pub fn render(
    field: &Field,
    ramp: &Ramp,
    scale: f32,
    offset: f32,
    width: usize,
    height: usize,
) -> Option<Vec<u8>> {
    if !field.sound() || width == 0 || height == 0 {
        return None;
    }
    let Field {
        data,
        cols,
        rows,
        channels,
    } = *field;
    // Step 3, along a row: which two points a pixel lies between, and how
    // far from the first to the second.
    let mut left = Vec::with_capacity(width);
    let mut across = Vec::with_capacity(width);
    for x in 0..width {
        let fx = (x as f32 + 0.5) / width as f32 * (cols - 1) as f32;
        let i = (fx as usize).min(cols - 2);
        left.push(i * channels);
        across.push(fx - i as f32);
    }
    let per_row = cols * channels;
    let mut row = vec![0.0f32; per_row];
    let mut value = vec![0.0f32; width];
    let mut out = vec![0u8; width * height * 4];
    let (n, cycle) = (ramp.colors as f32, ramp.cycle);
    let table = &ramp.table[..];
    for (y, line) in out.chunks_exact_mut(width * 4).enumerate() {
        // Rows first: the two rows of points the pixel row lies between.
        let fy = (y as f32 + 0.5) / height as f32 * (rows - 1) as f32;
        let j = (fy as usize).min(rows - 2);
        let down = fy - j as f32;
        let top = &data[j * per_row..(j + 1) * per_row];
        let bottom = &data[(j + 1) * per_row..(j + 2) * per_row];
        for ((r, a), b) in row.iter_mut().zip(top).zip(bottom) {
            *r = a + (b - a) * down;
        }
        // Then along the row, and step 4: the value.
        if channels == 2 {
            for ((v, i), t) in value.iter_mut().zip(&left).zip(&across) {
                let p = &row[*i..*i + 4];
                let u = p[0] + (p[2] - p[0]) * t;
                let w = p[1] + (p[3] - p[1]) * t;
                *v = (u * u + w * w).sqrt();
            }
        } else {
            for ((v, i), t) in value.iter_mut().zip(&left).zip(&across) {
                let p = &row[*i..*i + 2];
                *v = p[0] + (p[1] - p[0]) * t;
            }
        }
        // Steps 5 to 7: the position, its entry of the table, and the
        // colour there. (Two loops for the two kinds of ramp, so that
        // neither carries the other's choice through every pixel.)
        if cycle {
            for v in &mut value {
                *v = slot(*v * scale + offset, n, true);
            }
        } else {
            for v in &mut value {
                *v = slot(*v * scale + offset, n, false);
            }
        }
        for (pixel, entry) in line.as_chunks_mut::<4>().0.iter_mut().zip(&value) {
            *pixel = table[*entry as i32 as usize];
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLD: [f32; 4] = [1.0, 0.811765, 0.160784, 1.0];
    const CYAN: [f32; 4] = [0.054902, 0.839216, 0.941176, 1.0];
    const INK: [f32; 4] = [0.003922, 0.023529, 0.035294, 1.0];

    #[test]
    fn floor_is_exact() {
        for x in [
            0.0f32,
            -0.0,
            0.5,
            -0.5,
            1.0,
            -1.0,
            2.999_999_8,
            -2.000_000_2,
            8_388_607.5,
            -8_388_607.5,
            8_388_608.0,
            1.0e30,
            -1.0e30,
            f32::MAX,
        ] {
            assert_eq!(floor(x), x.floor(), "{x}");
        }
        let mut s = 0x9e37_79b9u32;
        for _ in 0..100_000 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let x = f32::from_bits(s);
            if x.is_finite() {
                assert_eq!(floor(x), x.floor(), "{x}");
            }
        }
    }

    #[test]
    fn a_ramp_takes_two_to_sixteen_colours_and_ordered_edges() {
        assert!(Ramp::new(&[GOLD], [0.0, 1.0], true).is_none());
        assert!(Ramp::new(&[GOLD; 17], [0.0, 1.0], true).is_none());
        assert!(Ramp::new(&[GOLD, CYAN], [0.6, 0.4], true).is_none());
        assert!(Ramp::new(&[GOLD, CYAN], [-0.1, 0.4], true).is_none());
        assert!(Ramp::new(&[GOLD, CYAN], [0.0, f32::NAN], true).is_none());
        assert!(Ramp::new(&[GOLD, [2.0, 0.0, 0.0, 1.0]], [0.0, 1.0], true).is_none());
        let cyclic = Ramp::new(&[GOLD, CYAN, INK], [0.08, 0.92], true).unwrap();
        assert_eq!(cyclic.table.len(), 3 * ENTRIES);
        let map = Ramp::new(&[GOLD, CYAN, INK], [0.0, 1.0], false).unwrap();
        assert_eq!(map.table.len(), 2 * ENTRIES);
        assert_eq!((map.colors(), map.cycle()), (3, false));
    }

    #[test]
    fn a_band_is_its_colour_to_the_first_edge_and_the_next_from_the_second() {
        let ramp = Ramp::new(&[GOLD, CYAN, INK], [0.08, 0.92], true).unwrap();
        let (gold, cyan, ink) = ([255, 207, 41, 255], [14, 214, 240, 255], [1, 6, 9, 255]);
        // Up to 8% of a band: the band's colour. From 92%: the next.
        assert_eq!(ramp.at(0.0), gold);
        assert_eq!(ramp.at(0.079), gold);
        assert_eq!(ramp.at(0.93), cyan);
        assert_eq!(ramp.at(1.0), cyan);
        assert_eq!(ramp.at(1.95), ink);
        assert_eq!(ramp.at(2.0), ink);
        // After the last colour comes the first again.
        assert_eq!(ramp.at(2.95), gold);
        assert_eq!(ramp.at(3.0), gold);
        assert_eq!(ramp.at(300.0 + 1.0), cyan);
        assert_eq!(ramp.at(-0.05), gold);
        assert_eq!(ramp.at(-1.0), ink);
        // Halfway through, halfway between.
        let mid = ramp.at(0.5);
        for (m, (a, b)) in mid.iter().zip(gold.iter().zip(&cyan)) {
            let half = (f32::from(*a) + f32::from(*b)) / 2.0;
            assert!((f32::from(*m) - half).abs() <= 1.5, "{mid:?}");
        }
        // Not a number, or no number at all: position 0.
        assert_eq!(ramp.at(f32::NAN), gold);
        assert_eq!(ramp.at(f32::INFINITY), gold);
        assert_eq!(ramp.at(f32::MAX), ramp.at(f32::MAX));
    }

    #[test]
    fn equal_edges_are_a_hard_edge_and_a_gradient_map_ends_at_its_last_colour() {
        let hard = Ramp::new(&[GOLD, INK], [0.5, 0.5], true).unwrap();
        assert_eq!(hard.at(0.499), [255, 207, 41, 255]);
        assert_eq!(hard.at(0.501), [1, 6, 9, 255]);
        assert_eq!(hard.at(1.499), [1, 6, 9, 255]);
        assert_eq!(hard.at(1.501), [255, 207, 41, 255]);
        // A gradient map: position 0 the first colour, n − 1 the last,
        // nothing repeats.
        let map = Ramp::new(
            &[[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]],
            [0.0, 1.0],
            false,
        )
        .unwrap();
        assert_eq!(map.at(-5.0), map.at(0.0));
        assert_eq!(map.at(0.0), [0, 0, 0, 255]);
        assert_eq!(map.at(1.0), [255, 255, 255, 255]);
        assert_eq!(map.at(7.0), [255, 255, 255, 255]);
        let mid = map.at(0.5)[0];
        assert!((126..=130).contains(&mid), "{mid}");
        // Opacity blends too, and the table is premultiplied.
        let fade = Ramp::new(
            &[[1.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 0.0]],
            [0.0, 1.0],
            false,
        )
        .unwrap();
        let half = fade.at(0.5);
        assert!((126..=130).contains(&half[3]), "{half:?}");
        assert_eq!(half[0], half[3]);
        assert_eq!(fade.at(1.0), [0, 0, 0, 0]);
    }

    #[test]
    fn the_size_computed_is_step_ones() {
        // quality × the rectangle, rounded; at least one pixel.
        assert_eq!(output_size(760.0, 640.0, 0.5, MAX_PIXELS), (380, 320));
        assert_eq!(output_size(760.0, 640.0, 1.0 / 3.0, MAX_PIXELS), (253, 213));
        assert_eq!(output_size(1.0, 1.0, 0.1, MAX_PIXELS), (1, 1));
        assert_eq!(output_size(0.0, 5.0, 1.0, MAX_PIXELS), (1, 5));
        // Over the cap: both sides by √(cap / (W × H)), rounded down.
        assert_eq!(output_size(760.0, 640.0, 1.0, MAX_PIXELS), (557, 469));
        assert_eq!(output_size(512.0, 512.0, 1.0, MAX_PIXELS), (512, 512));
        assert_eq!(output_size(2000.0, 2000.0, 1.0, MAX_PIXELS), (512, 512));
        let (w, h) = output_size(1.0e7, 3.0, 1.0, MAX_PIXELS);
        assert!(w * h <= MAX_PIXELS && h >= 1, "{w}×{h}");
        let (w, h) = output_size(f32::INFINITY, f32::NAN, 1.0, MAX_PIXELS);
        assert!(w * h <= MAX_PIXELS && w >= 1 && h >= 1, "{w}×{h}");
    }

    #[test]
    fn a_field_is_interpolated_before_it_is_coloured() {
        // One channel, four points: 0 at the left, 2 at the right. With a
        // hard-edged ramp of two colours the picture is two halves, each
        // of one colour, the edge exactly in the middle however few the
        // points.
        let data = [0.0, 2.0, 0.0, 2.0];
        let field = Field {
            data: &data,
            cols: 2,
            rows: 2,
            channels: 1,
        };
        let ramp = Ramp::new(&[GOLD, INK], [0.5, 0.5], false).unwrap();
        let out = render(&field, &ramp, 0.5, 0.0, 64, 4).unwrap();
        for (i, pixel) in out.as_chunks::<4>().0.iter().enumerate() {
            let x = i % 64;
            let want = if x < 32 {
                [255, 207, 41, 255]
            } else {
                [1, 6, 9, 255]
            };
            assert_eq!(*pixel, want, "x = {x}");
        }
        // Two channels: the length of the pair, the pair interpolated
        // first (the middle of (−1, 0) and (1, 0) is 0 long, not 1).
        let pair = [-1.0, 0.0, 1.0, 0.0, -1.0, 0.0, 1.0, 0.0];
        let field = Field {
            data: &pair,
            cols: 2,
            rows: 2,
            channels: 2,
        };
        let map = Ramp::new(
            &[[0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]],
            [0.0, 1.0],
            false,
        )
        .unwrap();
        let out = render(&field, &map, 1.0, 0.0, 65, 1).unwrap();
        assert!(out[32 * 4] <= 2, "the middle is {}", out[32 * 4]);
        assert!(out[0] >= 250 && out[64 * 4] >= 250);
        // What isn't a sound field gives no picture.
        let short = Field {
            data: &pair[..7],
            ..field
        };
        assert!(render(&short, &map, 1.0, 0.0, 8, 8).is_none());
        assert!(render(&field, &map, 1.0, 0.0, 0, 8).is_none());
    }

    #[test]
    fn values_glide_between_two_snapshots_and_leave_nothing_that_isnt_a_number() {
        let bytes = |v: &[f32]| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
        let a = bytes(&[0.0, 10.0, f32::NAN, f32::INFINITY]);
        let b = bytes(&[4.0, 20.0, 1.0, 1.0]);
        assert_eq!(values(&a, None, 0.0), [0.0, 10.0, 0.0, 0.0]);
        assert_eq!(values(&a, Some(&b), 0.25), [1.0, 12.5, 0.0, 0.0]);
        assert_eq!(values(&a, Some(&b), 1.0)[..2], [4.0, 20.0]);
    }
}
