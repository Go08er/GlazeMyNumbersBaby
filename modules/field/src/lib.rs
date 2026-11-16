//! Swirls' paint (`apps/gmnb/themes/swirls/paint.wasm`): the field pass of
//! docs/theming-media.md ("The field pass, exactly") and the ramp's table,
//! as `gmnb_media::field` and `gmnb.ramp` made them in GMNB until 0.4, step
//! for step in 32-bit floats. A coarse grid of numbers the theme worked
//! out, glided between two snapshots, is interpolated for every pixel of a
//! picture and only then coloured, through a ramp: the smooth part is
//! computed coarsely and the sharp part (the bands' edges) per pixel.
//!
//! It is `no_std`, has no dependencies and imports nothing; its memory is
//! laid out once, statically. It is built with `simd128` (and nothing
//! relaxed) by `modules/build.sh`, the same bytes from any checkout.
//!
//! # How a theme uses it
//!
//! The module has [`SLOTS`] slots, each a field of its own: its two
//! snapshots, its ramp, and the picture it last made. A theme gives each
//! of the fields it draws a slot, so that what it wrote for one is never
//! overwritten by another.
//!
//! | Export | What |
//! |---|---|
//! | `at(slot, part)` | Where in memory a slot's `part` is: 0 the snapshot, 1 the snapshot after it (each `cols × rows × channels` 32-bit floats), 2 the ramp's colours (red, green, blue, opacity, 32-bit floats, not premultiplied), 3 its picture. 0 for no such slot or part. |
//! | `ramp(slot, n, e0, e1, cycle)` | The slot's ramp, from the `n` colours written at part 2: `gmnb.ramp { colors, edge = { e0, e1 }, cycle }`'s table. 1, or 0 if it can't be made. |
//! | `size(w, h, quality)` | The picture's size for a rectangle of `w × h` at `quality` (0.1 to 1): `width × 2²⁰ + height`. |
//! | `render(slot, cols, rows, channels, glide, mix, scale, offset, width, height)` | The slot's picture, `width × height`, premultiplied RGBA at part 3: the snapshot, or with `glide` 1 the snapshot `mix` of the way to the one after it, coloured through the slot's ramp at `value × scale + offset`. 1; 0 if the slot has no ramp (the module is fresh: see below); 2 if the field or the size isn't one the pass takes. |
//!
//! **A fresh module has no ramp.** A module stopped at its deadline starts
//! afresh, its memory as on load, and the theme isn't told: `render`
//! answering 0 is how it finds out. It then writes everything again (every
//! slot's ramp and snapshots) and asks once more.

#![no_std]

// (Square roots are SIMD's: the scalar ones aren't in `core` on stable
// Rust. Each lane is the IEEE square root, as the scalar is.)
#[cfg(not(target_feature = "simd128"))]
compile_error!("the module is built with simd128: modules/build.sh");

use core::arch::wasm32::{f32x4_extract_lane, f32x4_splat, f32x4_sqrt};
use core::arch::wasm32::{f64x2_extract_lane, f64x2_splat, f64x2_sqrt};
use core::cell::UnsafeCell;

/// Entries per band in a ramp's table.
const ENTRIES: usize = 256;
/// The most pixels one pass computes.
const MAX_PIXELS: usize = 262_144;
/// The widest picture (a module picture's side).
const MAX_WIDTH: usize = 16_384;
/// A ramp's colours.
const MIN_COLORS: usize = 2;
const MAX_COLORS: usize = 16;
/// A field's points along each side, and in all.
const MIN_SIDE: usize = 2;
const MAX_SIDE: usize = 256;
const MAX_POINTS: usize = 16_384;
/// Fields the module holds at once.
pub const SLOTS: usize = 8;

/// One field: what the theme wrote, the ramp made of it, and the picture.
#[repr(C, align(16))]
struct Slot {
    data: [f32; MAX_POINTS * 2],
    next: [f32; MAX_POINTS * 2],
    colors: [[f32; 4]; MAX_COLORS],
    /// `bands × ENTRIES` colours, premultiplied RGBA8.
    table: [[u8; 4]; MAX_COLORS * ENTRIES],
    picture: [[u8; 4]; MAX_PIXELS],
    /// How many colours the ramp was made of: 0, none (a fresh module).
    colors_made: u32,
    cycle: bool,
}

/// What a pass works in, shared by the slots (one call at a time).
#[repr(C, align(16))]
struct Scratch {
    values: [f32; MAX_POINTS * 2],
    left: [u32; MAX_WIDTH],
    across: [f32; MAX_WIDTH],
    row: [f32; MAX_SIDE * 2],
    value: [f32; MAX_WIDTH],
}

// (Only ever the statics' first value: no copy of it is made at run time.)
#[allow(clippy::large_const_arrays)]
const EMPTY: Slot = Slot {
    data: [0.0; MAX_POINTS * 2],
    next: [0.0; MAX_POINTS * 2],
    colors: [[0.0; 4]; MAX_COLORS],
    table: [[0; 4]; MAX_COLORS * ENTRIES],
    picture: [[0; 4]; MAX_PIXELS],
    colors_made: 0,
    cycle: false,
};

/// The module's memory: one thread, one call at a time, so each export
/// borrows it once.
struct Memory<T>(UnsafeCell<T>);

// SAFETY: a module runs on one thread, one call at a time.
unsafe impl<T> Sync for Memory<T> {}

static FIELDS: Memory<[Slot; SLOTS]> = Memory(UnsafeCell::new([EMPTY; SLOTS]));
static SCRATCH: Memory<Scratch> = Memory(UnsafeCell::new(Scratch {
    values: [0.0; MAX_POINTS * 2],
    left: [0; MAX_WIDTH],
    across: [0.0; MAX_WIDTH],
    row: [0.0; MAX_SIDE * 2],
    value: [0.0; MAX_WIDTH],
}));

fn slot(n: u32) -> Option<&'static mut Slot> {
    // SAFETY: one call at a time; each export takes one slot.
    let fields = unsafe { &mut *FIELDS.0.get() };
    fields.get_mut(n as usize)
}

fn scratch() -> &'static mut Scratch {
    // SAFETY: one call at a time; only `render` takes it.
    unsafe { &mut *SCRATCH.0.get() }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

// ---- Float arithmetic, exactly as the pass's -----------------------------------

#[inline(always)]
fn abs(x: f32) -> f32 {
    x.abs()
}

#[inline(always)]
fn copysign(x: f32, sign: f32) -> f32 {
    x.copysign(sign)
}

/// The IEEE square root (`f32::sqrt`).
#[inline(always)]
fn sqrt(x: f32) -> f32 {
    f32x4_extract_lane::<0>(f32x4_sqrt(f32x4_splat(x)))
}

/// The same of an `f64`.
fn sqrt64(x: f64) -> f64 {
    f64x2_extract_lane::<0>(f64x2_sqrt(f64x2_splat(x)))
}

/// Rounded to the nearest whole number, halves away from zero (Rust's
/// `f32::round`).
fn round(x: f32) -> f32 {
    // Towards zero, exactly (`floor` of the size, with the sign back).
    let t = copysign(floor(abs(x)), x);
    // (Below 2²³ in size the difference is exact; above, x is whole.)
    if abs(x - t) >= 0.5 {
        t + copysign(1.0, x)
    } else {
        t
    }
}

/// The largest whole number not above `x`, exactly, in float arithmetic
/// alone: `gmnb_media::field`'s, which works a row four at a time.
#[inline(always)]
fn floor(x: f32) -> f32 {
    // Below 2²³ in size, adding 2²³ (of x's sign) leaves no room for a
    // fraction: the sum is x rounded to a whole number, and taking 2²³
    // away again is exact.
    const WHOLE: f32 = 8_388_608.0;
    let shift = copysign(WHOLE, x);
    let near = (x + shift) - shift;
    let down = if near > x { near - 1.0 } else { near };
    // From 2²³ on every float is a whole number already.
    if abs(x) < WHOLE { down } else { x }
}

/// Steps 5 to 7: where in a ramp of `n` colours the position `p` falls,
/// as an entry of its table (a whole number, still a float).
#[inline(always)]
fn entry(p: f32, n: f32, cycle: bool) -> f32 {
    // A position that is not finite counts as 0.
    let p = if p.is_finite() { p } else { 0.0 };
    let (p, bands) = if cycle {
        (p - n * floor(p / n), n)
    } else {
        (p.max(0.0).min(n - 1.0), n - 1.0)
    };
    let band = floor(p).max(0.0).min(bands - 1.0);
    let entry = floor((p - band) * ENTRIES as f32)
        .max(0.0)
        .min(ENTRIES as f32 - 1.0);
    band * ENTRIES as f32 + entry
}

// ---- The exports -------------------------------------------------------------------

/// Where slot `n`'s part is in memory: 0 the snapshot, 1 the one after
/// it, 2 the ramp's colours, 3 the picture. 0: no such slot or part.
#[unsafe(no_mangle)]
pub extern "C" fn at(n: u32, part: u32) -> u32 {
    let Some(s) = slot(n) else { return 0 };
    match part {
        0 => s.data.as_ptr() as u32,
        1 => s.next.as_ptr() as u32,
        2 => s.colors.as_ptr() as u32,
        3 => s.picture.as_ptr() as u32,
        _ => 0,
    }
}

/// Slot `n`'s ramp, of the `colors` colours written to it (each red,
/// green, blue, opacity, 0 to 1, not premultiplied): within a band the
/// colour is the band's own up to the fraction `e0`, the next one's from
/// `e1` on, and a smooth step between; with `cycle`, after the last comes
/// the first again. As `gmnb.ramp` took them, colours and edges are first
/// held to 0 to 1. 1 if it was made; 0 (and no ramp) if it can't be.
///
/// Entry `k` of band `b` is the colour at the fraction `(k + 0.5) / 256`:
/// `t = clamp((f − e0) / (e1 − e0), 0, 1)` (with `e0 = e1`: 0 below it, 1
/// from it on), `s = t² (3 − 2t)`, and `a + (b − a) × s` on each channel
/// and the opacity; stored premultiplied, each `floor(c × 255 + 0.5)`.
#[unsafe(no_mangle)]
pub extern "C" fn ramp(n: u32, colors: u32, e0: f32, e1: f32, cycle: u32) -> u32 {
    let Some(s) = slot(n) else { return 0 };
    s.colors_made = 0;
    let count = colors as usize;
    let (e0, e1) = (e0.clamp(0.0, 1.0), e1.clamp(0.0, 1.0));
    if !(MIN_COLORS..=MAX_COLORS).contains(&count) || e0.is_nan() || e1.is_nan() || e0 > e1 {
        return 0;
    }
    for c in s.colors[..count].iter_mut().flatten() {
        *c = c.clamp(0.0, 1.0);
    }
    if s.colors[..count].iter().flatten().any(|c| c.is_nan()) {
        return 0;
    }
    let cycle = cycle != 0;
    let bands = if cycle { count } else { count - 1 };
    for band in 0..bands {
        let (a, b) = (s.colors[band], s.colors[(band + 1) % count]);
        for k in 0..ENTRIES {
            let f = (k as f32 + 0.5) / ENTRIES as f32;
            let t = if e0 == e1 {
                if f < e0 { 0.0 } else { 1.0 }
            } else {
                ((f - e0) / (e1 - e0)).clamp(0.0, 1.0)
            };
            let step = t * t * (3.0 - 2.0 * t);
            let mix = |i: usize| a[i] + (b[i] - a[i]) * step;
            let alpha = mix(3);
            let byte = |c: f32| floor(c * 255.0 + 0.5) as u8;
            s.table[band * ENTRIES + k] = [
                byte(mix(0) * alpha),
                byte(mix(1) * alpha),
                byte(mix(2) * alpha),
                byte(alpha),
            ];
        }
    }
    s.colors_made = colors;
    s.cycle = cycle;
    1
}

/// Step 1: how many pixels are computed for a rectangle of `w × h` at
/// `quality` (held to 0.1 to 1). `W = max(1, round(w × quality))`, `H`
/// likewise; if `W × H` is over 262 144, both are multiplied by
/// `√(262 144 / (W × H))` and rounded down. Returned as `W × 2²⁰ + H`.
#[unsafe(no_mangle)]
pub extern "C" fn size(w: f32, h: f32, quality: f32) -> f64 {
    let (w, h) = output_size(w, h, quality.clamp(0.1, 1.0), MAX_PIXELS as u32);
    f64::from(w) * 1_048_576.0 + f64::from(h)
}

fn output_size(w: f32, h: f32, quality: f32, max_pixels: u32) -> (u32, u32) {
    let side = |s: f32| {
        let n = round(s * quality);
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
        let mut n = sqrt64((side * max) as f64 / other as f64) as u128;
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

/// Steps 2 to 7, into slot `n`'s picture: its field of `cols × rows`
/// points of `channels` numbers (1: the number; 2: the length of the
/// pair), the snapshot or (`glide` 1) the snapshot `mix` of the way to the
/// one after it, a number that isn't finite counting as 0; interpolated
/// for each pixel of `width × height`, rows first, its corners on the
/// picture's corners; at `value × scale + offset` in the ramp. 1; 0: the
/// slot has no ramp; 2: not a field or a size the pass takes.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "C" fn render(
    n: u32,
    cols: u32,
    rows: u32,
    channels: u32,
    glide: u32,
    mix: f32,
    scale: f32,
    offset: f32,
    width: u32,
    height: u32,
) -> u32 {
    let Some(s) = slot(n) else { return 2 };
    if s.colors_made == 0 {
        return 0;
    }
    let (cols, rows, channels) = (cols as usize, rows as usize, channels as usize);
    let (width, height) = (width as usize, height as usize);
    let sound = (MIN_SIDE..=MAX_SIDE).contains(&cols)
        && (MIN_SIDE..=MAX_SIDE).contains(&rows)
        && cols * rows <= MAX_POINTS
        && (1..=2).contains(&channels)
        && (1..=MAX_WIDTH).contains(&width)
        && height >= 1
        && width * height <= MAX_PIXELS;
    if !sound {
        return 2;
    }
    let scratch = scratch();
    // The numbers, after the glide.
    let count = cols * rows * channels;
    let sane = |v: f32| if v.is_finite() { v } else { 0.0 };
    let values = &mut scratch.values[..count];
    if glide != 0 {
        let mix = mix.clamp(0.0, 1.0);
        for ((v, a), b) in values.iter_mut().zip(&s.data).zip(&s.next) {
            *v = sane(a + (b - a) * mix);
        }
    } else {
        for (v, a) in values.iter_mut().zip(&s.data) {
            *v = sane(*a);
        }
    }
    // Step 3, along a row: which two points a pixel lies between, and how
    // far from the first to the second.
    let left = &mut scratch.left[..width];
    let across = &mut scratch.across[..width];
    for (x, (l, t)) in left.iter_mut().zip(across.iter_mut()).enumerate() {
        let fx = (x as f32 + 0.5) / width as f32 * (cols - 1) as f32;
        let i = (fx as usize).min(cols - 2);
        *l = (i * channels) as u32;
        *t = fx - i as f32;
    }
    let per_row = cols * channels;
    let row = &mut scratch.row[..per_row];
    let value = &mut scratch.value[..width];
    let (colors, cycle) = (s.colors_made as f32, s.cycle);
    let table = &s.table[..];
    for (y, line) in s.picture[..width * height]
        .chunks_exact_mut(width)
        .enumerate()
    {
        // Rows first: the two rows of points the pixel row lies between.
        let fy = (y as f32 + 0.5) / height as f32 * (rows - 1) as f32;
        let j = (fy as usize).min(rows - 2);
        let down = fy - j as f32;
        let top = &values[j * per_row..(j + 1) * per_row];
        let bottom = &values[(j + 1) * per_row..(j + 2) * per_row];
        for ((r, a), b) in row.iter_mut().zip(top).zip(bottom) {
            *r = a + (b - a) * down;
        }
        // Then along the row, and step 4: the value.
        if channels == 2 {
            for ((v, i), t) in value.iter_mut().zip(left.iter()).zip(across.iter()) {
                let i = *i as usize;
                let p = &row[i..i + 4];
                let u = p[0] + (p[2] - p[0]) * t;
                let w = p[1] + (p[3] - p[1]) * t;
                *v = sqrt(u * u + w * w);
            }
        } else {
            for ((v, i), t) in value.iter_mut().zip(left.iter()).zip(across.iter()) {
                let i = *i as usize;
                let p = &row[i..i + 2];
                *v = p[0] + (p[1] - p[0]) * t;
            }
        }
        // Steps 5 to 7: the position, its entry of the table, and the
        // colour there. (Two loops for the two kinds of ramp, so that
        // neither carries the other's choice through every pixel.)
        if cycle {
            for v in value.iter_mut() {
                *v = entry(*v * scale + offset, colors, true);
            }
        } else {
            for v in value.iter_mut() {
                *v = entry(*v * scale + offset, colors, false);
            }
        }
        for (pixel, e) in line.iter_mut().zip(value.iter()) {
            *pixel = table[*e as i32 as usize];
        }
    }
    1
}
