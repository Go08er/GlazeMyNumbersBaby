//! The field pass (docs/theming-media.md, "The field pass, exactly", and
//! test 9): the same bytes as the chapter's steps written out plainly,
//! the same bytes on every machine (a stored hash), band edges that stay
//! sharp where colours computed coarsely blur, and what it costs.

mod common;

use common::Rng;
use gmnb_media::field::{self, Field, MAX_PIXELS, Ramp};

type Color = [f32; 4];

const GOLD: Color = [1.0, 0.811765, 0.160784, 1.0];
const CYAN: Color = [0.054902, 0.839216, 0.941176, 1.0];
const INK: Color = [0.003922, 0.023529, 0.035294, 1.0];

/// The chapter's steps 3 to 7, one pixel at a time, with the library's
/// own `floor` and no table: what the pass must equal byte for byte.
#[allow(clippy::too_many_arguments)]
fn by_the_book(
    f: &Field,
    colors: &[Color],
    edge: [f32; 2],
    cycle: bool,
    scale: f32,
    offset: f32,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let (cols, rows, channels) = (f.cols, f.rows, f.channels);
    let n = colors.len();
    let mut out = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            // Step 3.
            let fx = (x as f32 + 0.5) / width as f32 * (cols - 1) as f32;
            let fy = (y as f32 + 0.5) / height as f32 * (rows - 1) as f32;
            let i = (fx.floor() as usize).min(cols - 2);
            let j = (fy.floor() as usize).min(rows - 2);
            let (tx, ty) = (fx - i as f32, fy - j as f32);
            let at = |i: usize, j: usize, c: usize| f.data[(j * cols + i) * channels + c];
            let channel = |c: usize| {
                let left = at(i, j, c) + (at(i, j + 1, c) - at(i, j, c)) * ty;
                let right = at(i + 1, j, c) + (at(i + 1, j + 1, c) - at(i + 1, j, c)) * ty;
                left + (right - left) * tx
            };
            // Step 4.
            let value = if channels == 1 {
                channel(0)
            } else {
                let (u, v) = (channel(0), channel(1));
                (u * u + v * v).sqrt()
            };
            // Step 5.
            let mut p = value * scale + offset;
            if !p.is_finite() {
                p = 0.0;
            }
            let nf = n as f32;
            let (band, next) = if cycle {
                p -= nf * (p / nf).floor();
                let b = (p.floor().clamp(0.0, nf - 1.0)) as usize;
                (b, (b + 1) % n)
            } else {
                p = p.clamp(0.0, nf - 1.0);
                let b = (p.floor() as usize).min(n - 2);
                (b, b + 1)
            };
            let fraction = p - band as f32;
            // Step 7: the entry of the band's table.
            let entry = ((fraction * 256.0).floor() as i64).clamp(0, 255);
            let at_entry = (entry as f32 + 0.5) / 256.0;
            // Step 6.
            let [e0, e1] = edge;
            let t = if e0 == e1 {
                if at_entry < e0 { 0.0 } else { 1.0 }
            } else {
                ((at_entry - e0) / (e1 - e0)).clamp(0.0, 1.0)
            };
            let s = t * t * (3.0 - 2.0 * t);
            let (a, b) = (colors[band], colors[next]);
            let mix = |c: usize| a[c] + (b[c] - a[c]) * s;
            let alpha = mix(3);
            for c in 0..3 {
                out.push((mix(c) * alpha * 255.0 + 0.5).floor() as u8);
            }
            out.push((alpha * 255.0 + 0.5).floor() as u8);
        }
    }
    out
}

/// Numbers from −`reach` to `reach`, the same every run.
fn numbers(rng: &mut Rng, count: usize, reach: f32) -> Vec<f32> {
    (0..count)
        .map(|_| (rng.next() as f32 / u32::MAX as f32 * 2.0 - 1.0) * reach)
        .collect()
}

#[test]
fn the_pass_is_the_chapters_steps_byte_for_byte() {
    let mut rng = Rng(0x00c0_ffee);
    let palettes: [&[Color]; 4] = [
        &[GOLD, CYAN, INK],
        &[GOLD, INK],
        &[[0.0, 0.0, 0.0, 0.0], [1.0, 0.5, 0.25, 0.5], CYAN, GOLD, INK],
        &[GOLD; 16],
    ];
    let edges = [[0.0, 1.0], [0.08, 0.92], [0.5, 0.5], [0.0, 0.0], [1.0, 1.0]];
    let grids = [(2, 2), (9, 7), (65, 55), (256, 64), (3, 200)];
    let sizes = [(1, 1), (61, 47), (380, 320), (7, 300)];
    let mut compared = 0;
    for round in 0..120 {
        let (cols, rows) = grids[round % grids.len()];
        let channels = 1 + round % 2;
        let reach = [1.0, 4.0, 40.0, 1.0e6][round % 4];
        let data = numbers(&mut rng, cols * rows * channels, reach);
        let f = Field {
            data: &data,
            cols,
            rows,
            channels,
        };
        assert!(f.sound());
        let colors = palettes[rng.below(palettes.len())];
        let edge = edges[rng.below(edges.len())];
        let cycle = rng.below(2) == 0;
        let scale = [1.0, 1.25, -0.7, 0.001, 300.0][rng.below(5)];
        let offset = [0.0, 0.1, -2.5, 1.0e9][rng.below(4)];
        let (w, h) = sizes[rng.below(sizes.len())];
        let ramp = Ramp::new(colors, edge, cycle).unwrap();
        let got = field::render(&f, &ramp, scale, offset, w, h).unwrap();
        let want = by_the_book(&f, colors, edge, cycle, scale, offset, w, h);
        assert_eq!(got.len(), w * h * 4);
        if got != want {
            let at = got.iter().zip(&want).position(|(a, b)| a != b).unwrap() / 4;
            panic!(
                "round {round}: pixel ({}, {}) of {w}×{h} differs; {cols}×{rows}×{channels}, \
                 {} colours, edge {edge:?}, cycle {cycle}, scale {scale}, offset {offset}",
                at % w,
                at / w,
                colors.len()
            );
        }
        compared += got.len() / 4;
    }
    assert!(compared > 1_000_000, "{compared} pixels compared");
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A small field every number of which a float holds exactly, so that
/// nothing about it depends on how a machine parses or rounds.
fn golden_field() -> (Vec<f32>, usize, usize) {
    let (cols, rows) = (9, 7);
    let mut data = Vec::new();
    for j in 0..rows {
        for i in 0..cols {
            data.push(((i * 37 + j * 11) % 17) as f32 / 4.0 - 2.0);
            data.push(((i * 5 + j * 29) % 13) as f32 / 8.0 - 0.75);
        }
    }
    (data, cols, rows)
}

#[test]
fn the_pass_gives_the_same_bytes_on_every_machine() {
    let (data, cols, rows) = golden_field();
    let f = Field {
        data: &data,
        cols,
        rows,
        channels: 2,
    };
    let ramp = Ramp::new(&[GOLD, CYAN, INK], [0.08, 0.92], true).unwrap();
    let cyclic = field::render(&f, &ramp, 1.25, 0.125, 61, 47).unwrap();
    // One channel of it, as a gradient map with a hard edge.
    let one: Vec<f32> = data.iter().step_by(2).copied().collect();
    let f1 = Field {
        data: &one,
        cols,
        rows,
        channels: 1,
    };
    let map = Ramp::new(&[INK, GOLD, CYAN, [1.0, 1.0, 1.0, 0.5]], [0.5, 0.5], false).unwrap();
    let mapped = field::render(&f1, &map, 0.75, 1.5, 40, 30).unwrap();
    let (a, b) = (fnv(&cyclic), fnv(&mapped));
    // If these change, the pass no longer does what the chapter says it
    // does (or the chapter changed): themes' pictures change with it.
    assert_eq!(
        (a, b),
        (GOLDEN_CYCLIC, GOLDEN_MAPPED),
        "the hashes are {a:#018x} and {b:#018x}"
    );
}

const GOLDEN_CYCLIC: u64 = 0x6604_19d7_81bb_1ab4;
const GOLDEN_MAPPED: u64 = 0xe14f_a9f0_8926_6082;

// ---- The swirl: sharp bands ----------------------------------------------------------

const W: usize = 760;
const H: usize = 640;
const SCALE: f32 = 1.25;

/// The ColorMix shader's distorted coordinate (fairyglade/ly, WTFPL).
fn distort(mut u: f32, mut v: f32, px: f32, py: f32) -> (f32, f32) {
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
    (u, v)
}

const PHASE: (f32, f32) = (1.31, 4.27);
const STRETCH: (f32, f32) = (0.825 * W as f32 / H as f32, 1.65);

/// The shader per pixel: the picture the field pass is to resemble.
fn exact(ramp: &Ramp) -> Vec<u8> {
    let mut out = Vec::with_capacity(W * H * 4);
    for y in 0..H {
        let v = (1.0 - 2.0 * (y as f32 + 0.5) / H as f32) * STRETCH.1;
        for x in 0..W {
            let u = (2.0 * (x as f32 + 0.5) / W as f32 - 1.0) * STRETCH.0;
            let (a, b) = distort(u, v, PHASE.0, PHASE.1);
            out.extend_from_slice(&ramp.at((a * a + b * b).sqrt() * SCALE));
        }
    }
    out
}

/// The field a theme computes: the distorted coordinate at every point
/// of a grid `step` pixels apart, corners on the picture's corners.
fn swirl_field(step: usize) -> (Vec<f32>, usize, usize) {
    let (cols, rows) = (W.div_ceil(step) + 1, H.div_ceil(step) + 1);
    let mut data = Vec::with_capacity(cols * rows * 2);
    for j in 0..rows {
        let v = (1.0 - 2.0 * j as f32 / (rows - 1) as f32) * STRETCH.1;
        for i in 0..cols {
            let u = (2.0 * i as f32 / (cols - 1) as f32 - 1.0) * STRETCH.0;
            let (a, b) = distort(u, v, PHASE.0, PHASE.1);
            data.extend_from_slice(&[a, b]);
        }
    }
    (data, cols, rows)
}

/// The other way: the colours themselves computed one a grid cell, as a
/// theme would fill a `gmnb.pixels` picture.
fn swirl_colours(step: usize, ramp: &Ramp) -> (Vec<u8>, usize, usize) {
    let (cols, rows) = (W.div_ceil(step), H.div_ceil(step));
    let mut out = Vec::with_capacity(cols * rows * 4);
    for j in 0..rows {
        let v = (1.0 - 2.0 * (j as f32 + 0.5) / rows as f32) * STRETCH.1;
        for i in 0..cols {
            let u = (2.0 * (i as f32 + 0.5) / cols as f32 - 1.0) * STRETCH.0;
            let (a, b) = distort(u, v, PHASE.0, PHASE.1);
            out.extend_from_slice(&ramp.at((a * a + b * b).sqrt() * SCALE));
        }
    }
    (out, cols, rows)
}

/// A picture scaled up smoothly to the window's size, pixel centres
/// aligned: what drawing a smaller texture with the linear filter does.
fn upscale(src: &[u8], sw: usize, sh: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(W * H * 4);
    for y in 0..H {
        let fy = ((y as f32 + 0.5) * sh as f32 / H as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let j = (fy as usize).min(sh.saturating_sub(2));
        let ty = fy - j as f32;
        let j1 = (j + 1).min(sh - 1);
        for x in 0..W {
            let fx = ((x as f32 + 0.5) * sw as f32 / W as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let i = (fx as usize).min(sw.saturating_sub(2));
            let tx = fx - i as f32;
            let i1 = (i + 1).min(sw - 1);
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

/// How far a picture is from another: the mean difference of a channel
/// (of 255), and the share of pixels with a channel off by more than 32.
fn differ(a: &[u8], b: &[u8]) -> (f64, f64) {
    let (mut sum, mut far) = (0u64, 0u64);
    for (p, q) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
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

/// The share of pixels that are none of the ramp's three colours. With
/// a hard-edged ramp the shader has none: every one there is, is an edge
/// that was smeared.
fn blended(picture: &[u8]) -> f64 {
    let pure = [[255u8, 207, 41], [14, 214, 240], [1, 6, 9]];
    let between = picture
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| {
            !pure
                .iter()
                .any(|c| (0..3).all(|i| p[i].abs_diff(c[i]) <= 12))
        })
        .count();
    between as f64 * 100.0 / (picture.len() / 4) as f64
}

#[test]
fn a_field_keeps_band_edges_sharp_where_coarse_colours_blur() {
    // The swirl's own ramp, and the same colours with a hard edge.
    let ramp = Ramp::new(&[GOLD, CYAN, INK], [0.08, 0.92], true).unwrap();
    let hard = Ramp::new(&[GOLD, CYAN, INK], [0.5, 0.5], true).unwrap();
    let truth = exact(&ramp);
    assert_eq!(blended(&exact(&hard)), 0.0);
    let mut report = String::new();
    let mut fields = Vec::new();
    for step in [16, 12, 8] {
        let (data, cols, rows) = swirl_field(step);
        let f = Field {
            data: &data,
            cols,
            rows,
            channels: 2,
        };
        // Coloured by the pass at half the window's size, then scaled up.
        let (w, h) = field::output_size(W as f32, H as f32, 0.5, MAX_PIXELS);
        assert_eq!((w, h), (380, 320));
        let half = field::render(&f, &ramp, SCALE, 0.0, w as usize, h as usize).unwrap();
        let shown = upscale(&half, w as usize, h as usize);
        let (mean, far) = differ(&shown, &truth);
        // Hard-edged: every pixel the pass makes is one of the ramp's
        // colours, whatever the grid. Only scaling the picture up to the
        // window softens an edge, by a pixel or two.
        let crisp = field::render(&f, &hard, SCALE, 0.0, w as usize, h as usize).unwrap();
        assert_eq!(blended(&crisp), 0.0, "a field every {step} px");
        let smeared = blended(&upscale(&crisp, w as usize, h as usize));
        report += &format!(
            "a field every {step} px ({cols}×{rows}), coloured at half size: mean error {mean:.2}, \
             {far:.2}% of pixels off by more than 32; hard-edged, {smeared:.1}% of pixels on a \
             smeared edge\n"
        );
        fields.push((step, mean, far, smeared));
    }
    let mut colours = Vec::new();
    for step in [12, 8, 4] {
        let (small, cols, rows) = swirl_colours(step, &ramp);
        let shown = upscale(&small, cols, rows);
        let (mean, far) = differ(&shown, &truth);
        let (crisp, cols, rows) = swirl_colours(step, &hard);
        let smeared = blended(&upscale(&crisp, cols, rows));
        report += &format!(
            "colours every {step} px ({cols}×{rows}), scaled up: mean error {mean:.2}, {far:.2}% of \
             pixels off by more than 32; hard-edged, {smeared:.1}% of pixels on a smeared edge\n"
        );
        colours.push((step, mean, far, smeared));
    }
    eprint!("{report}");
    // The chapter's figures (the designer's, with the field from Luau):
    // 1.91 / 0.87% at 16 px, 1.13 / 0.18% at 12, 0.64 / 0.01% at 8;
    // colours 4.41 / 4.94% at 12, 2.38 / 2.34% at 8, 0.74 / 0.20% at 4.
    let [f16, f12, f8] = [fields[0], fields[1], fields[2]];
    let [c12, c8, c4] = [colours[0], colours[1], colours[2]];
    assert!(f16.1 < 2.3 && f16.2 < 1.3, "{report}");
    assert!(f12.1 < 1.4 && f12.2 < 0.4, "{report}");
    assert!(f8.1 < 0.9 && f8.2 < 0.1, "{report}");
    assert!(c12.1 > 3.5 && c12.2 > 3.5, "{report}");
    // At the same grid, a field is several times nearer the shader than
    // colours are, and its far-off pixels are a small fraction of theirs.
    assert!(f12.1 * 3.0 < c12.1 && f12.2 * 10.0 < c12.2, "{report}");
    assert!(f8.1 * 3.0 < c8.1 && f8.2 * 10.0 < c8.2, "{report}");
    // A field every 12 px does about what colours every 4 px do.
    assert!(f12.1 < c4.1 * 2.0 && f12.2 < c4.2 * 1.5, "{report}");
    // And the edges themselves, hard-edged: the same field, whatever its
    // grid, smears an edge over the pixel or two that scaling up costs;
    // colours every 12 px smear it over a whole cell.
    assert!((f16.3 - f12.3).abs() < f12.3 * 0.25, "{report}");
    assert!(c12.3 > f12.3 * 3.0, "{report}");
}

// ---- What it costs -------------------------------------------------------------------

fn timed(runs: usize, mut f: impl FnMut()) -> (f64, f64, f64) {
    for _ in 0..10 {
        f();
    }
    let mut ms: Vec<f64> = (0..runs)
        .map(|_| {
            let t = std::time::Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    ms.sort_by(f64::total_cmp);
    (ms[0], ms[runs / 2], ms[runs * 95 / 100])
}

/// The pass's cost for a 760 × 640 rectangle. Not a test of anything:
/// run it for the figures, in the release profile, on a quiet machine.
/// `cargo test --release -p gmnb-media --test field -- --ignored --nocapture`
#[test]
#[ignore = "a measurement, not a test"]
fn what_the_pass_costs() {
    let ramp = Ramp::new(&[GOLD, CYAN, INK], [0.08, 0.92], true).unwrap();
    for step in [16, 12] {
        let (data, cols, rows) = swirl_field(step);
        let f = Field {
            data: &data,
            cols,
            rows,
            channels: 2,
        };
        for (name, w, h) in [
            ("full (over the cap)", W, H),
            ("the most allowed", 557, 469),
            ("half", W / 2, H / 2),
            ("a third", W / 3, H / 3),
        ] {
            let (min, median, p95) = timed(200, || {
                std::hint::black_box(field::render(&f, &ramp, SCALE, 0.0, w, h));
            });
            eprintln!(
                "field every {step} px, {name}, {w}×{h}: min {min:.3}  median {median:.3}  p95 {p95:.3} ms \
                 ({:.1} ns a pixel)",
                median * 1e6 / (w * h) as f64
            );
        }
    }
    // One channel, for scale.
    let (data, cols, rows) = swirl_field(12);
    let one: Vec<f32> = data.iter().step_by(2).copied().collect();
    let f = Field {
        data: &one,
        cols,
        rows,
        channels: 1,
    };
    let (min, median, p95) = timed(200, || {
        std::hint::black_box(field::render(&f, &ramp, SCALE, 0.0, W / 2, H / 2));
    });
    eprintln!("one channel, half, 380×320: min {min:.3}  median {median:.3}  p95 {p95:.3} ms");
    // The numbers from a buffer, with a glide between two snapshots.
    let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
    let (min, median, p95) = timed(200, || {
        std::hint::black_box(field::values(&bytes, Some(&bytes), 0.4));
    });
    eprintln!(
        "reading {} numbers from two buffers and gliding: min {min:.3}  median {median:.3}  p95 {p95:.3} ms",
        data.len()
    );
}
