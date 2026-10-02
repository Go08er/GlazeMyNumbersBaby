//! Marching squares for implicit relations `F(x, y) = 0` and inequality
//! regions `F(x, y) < 0`.
//!
//! The lattice is aligned to world multiples of the cell size (stable while
//! panning). Coarse blocks of `implicit_block²` cells are evaluated at their
//! corners and centre; blocks where the sign changes (and their neighbours)
//! are refined to the fine lattice. Crossing points are computed per
//! lattice edge from node values only, so neighbouring cells agree exactly
//! and segments can be joined into polylines by edge identity.

use super::{Cancel, PlotOptions, Point, Polyline};
use crate::compile::{Input, Program};
use crate::viewport::Viewport;
use std::collections::{HashMap, HashSet};

pub(crate) struct ContourResult {
    pub curves: Vec<Polyline>,
    pub fill: Vec<Polyline>,
    pub missing: bool,
}

/// Global edge identity: horizontal edge from node (i, j) to (i+1, j) has
/// dir 0, vertical edge from (i, j) to (i, j+1) has dir 1.
type EdgeKey = (i64, i64, u8);

/// Most fine cells in the lattice, whatever the window size.
const MAX_FINE_CELLS: f64 = 4.0e6;

struct Lattice<'a> {
    f: &'a Program,
    hx: f64,
    hy: f64,
}

impl Lattice<'_> {
    #[inline]
    fn x(&self, i: i64) -> f64 {
        i as f64 * self.hx
    }

    #[inline]
    fn y(&self, j: i64) -> f64 {
        j as f64 * self.hy
    }

    /// Zero crossing on the lattice edge between nodes `p` and `q` (values
    /// of opposite sign). A few bisection steps tell zeros from poles and
    /// jumps: near a zero |F| shrinks with the bracket, near a pole it grows
    /// and across a jump it stays put. Returns `None` for non-zeros.
    fn crossing(&self, p: (f64, f64), vp: f64, q: (f64, f64), vq: f64) -> Option<Point> {
        let at = |t: f64| Point::new(p.0 + t * (q.0 - p.0), p.1 + t * (q.1 - p.1));
        let m0 = vp.abs().max(vq.abs());
        let (mut a, mut fa, mut b, mut fb) = (0.0, vp, 1.0, vq);
        for _ in 0..6 {
            let m = 0.5 * (a + b);
            let pm = at(m);
            let fm = self.f.eval(pm.x, pm.y);
            if !fm.is_finite() {
                return None;
            }
            if fm == 0.0 {
                return Some(pm);
            }
            if (fm < 0.0) == (fa < 0.0) {
                a = m;
                fa = fm;
            } else {
                b = m;
                fb = fm;
            }
        }
        if fa.abs().max(fb.abs()) > 0.25 * m0 {
            return None;
        }
        let t = a + (b - a) * fa / (fa - fb);
        Some(at(if t.is_finite() { t } else { 0.5 * (a + b) }))
    }
}

/// The coarse stage of [`contour`]: lattice, block corner and centre
/// values, and the blocks to refine (where the sign changes, dilated).
struct Coarse<'a> {
    lat: Lattice<'a>,
    i0: i64,
    j0: i64,
    nbx_u: usize,
    nby_u: usize,
    corners: Vec<f64>,
    centres: Vec<f64>,
    marked: Vec<bool>,
}

/// Coarsest fine-cell size the refinement budget may push the lattice to.
const MAX_CELL_PX: f64 = 32.0;

fn coarse_pass<'a>(f: &'a Program, vp: &Viewport, b: i64, cell_px: f64) -> Option<Coarse<'a>> {
    let lat = Lattice {
        f,
        hx: cell_px * vp.x_per_px(),
        hy: cell_px * vp.y_per_px(),
    };
    let i0 = (vp.x_min / lat.hx).floor() as i64 - 1;
    let j0 = (vp.y_min / lat.hy).floor() as i64 - 1;
    let i1 = (vp.x_max / lat.hx).ceil() as i64 + 1;
    let j1 = (vp.y_max / lat.hy).ceil() as i64 + 1;
    let nbx = ((i1 - i0) + b - 1) / b;
    let nby = ((j1 - j0) + b - 1) / b;
    if nbx <= 0 || nby <= 0 || nbx * nby > 10_000_000 {
        return None;
    }
    let (nbx_u, nby_u) = (nbx as usize, nby as usize);

    // Coarse corner values, row-major (nbx+1) × (nby+1).
    let cw = nbx_u + 1;
    let mut corners = vec![0.0; cw * (nby_u + 1)];
    let xs: Vec<f64> = (0..=nbx).map(|bx| lat.x(i0 + bx * b)).collect();
    for by in 0..=nby {
        let y = lat.y(j0 + by * b);
        let row = by as usize * cw;
        f.eval_batch(
            Input::Slice(&xs),
            Input::Scalar(y),
            &mut corners[row..row + cw],
        );
    }
    // Block centres.
    let mut centres = vec![0.0; nbx_u * nby_u];
    let cxs: Vec<f64> = (0..nbx)
        .map(|bx| lat.x(i0 + bx * b) + 0.5 * b as f64 * lat.hx)
        .collect();
    for by in 0..nby {
        let y = lat.y(j0 + by * b) + 0.5 * b as f64 * lat.hy;
        let row = by as usize * nbx_u;
        f.eval_batch(
            Input::Slice(&cxs),
            Input::Scalar(y),
            &mut centres[row..row + nbx_u],
        );
    }

    let block_vals = |bx: usize, by: usize| -> [f64; 5] {
        [
            corners[by * cw + bx],
            corners[by * cw + bx + 1],
            corners[(by + 1) * cw + bx + 1],
            corners[(by + 1) * cw + bx],
            centres[by * nbx_u + bx],
        ]
    };
    let mixed = |v: &[f64; 5]| {
        let mut neg = false;
        let mut pos = false;
        let mut nan = false;
        for &x in v {
            if x.is_nan() {
                nan = true;
            } else if x < 0.0 {
                neg = true;
            } else if x > 0.0 {
                pos = true;
            } else {
                return true;
            }
        }
        (neg && pos) || (nan && (neg || pos))
    };
    let mut marked = vec![false; nbx_u * nby_u];
    for by in 0..nby_u {
        for bx in 0..nbx_u {
            if mixed(&block_vals(bx, by)) {
                // Dilate by one block so curves that cross a block edge twice
                // between coarse nodes are still found.
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let (nx, ny) = (bx as i64 + dx, by as i64 + dy);
                        if nx >= 0 && ny >= 0 && nx < nbx && ny < nby {
                            marked[ny as usize * nbx_u + nx as usize] = true;
                        }
                    }
                }
            }
        }
    }
    Some(Coarse {
        lat,
        i0,
        j0,
        nbx_u,
        nby_u,
        corners,
        centres,
        marked,
    })
}

pub(crate) fn contour(
    f: &Program,
    vp: &Viewport,
    opts: &PlotOptions,
    want_fill: bool,
    strict: bool,
    cancel: &Cancel<'_>,
) -> ContourResult {
    let mut cell_px = opts.implicit_cell_px.max(0.25);
    let cells = |c: f64| (vp.width / c) * (vp.height / c);
    while cells(cell_px) > MAX_FINE_CELLS {
        cell_px *= 1.5;
    }
    let b = opts.implicit_block.max(1) as i64;
    // Refining every block of a relation that oscillates everywhere costs
    // the whole fine lattice; past the budget, a coarser lattice is used.
    let coarse = loop {
        let Some(c) = coarse_pass(f, vp, b, cell_px) else {
            return ContourResult {
                curves: Vec::new(),
                fill: Vec::new(),
                missing: true,
            };
        };
        let refined = c.marked.iter().filter(|m| **m).count() as f64 * (b * b) as f64;
        if refined > opts.implicit_max_refined.max(1.0) && cell_px < MAX_CELL_PX {
            cell_px *= 1.5;
            continue;
        }
        break c;
    };
    let Coarse {
        lat,
        i0,
        j0,
        nbx_u,
        nby_u,
        corners,
        centres,
        marked,
    } = coarse;
    let cw = nbx_u + 1;
    let block_vals = |bx: usize, by: usize| -> [f64; 5] {
        [
            corners[by * cw + bx],
            corners[by * cw + bx + 1],
            corners[(by + 1) * cw + bx + 1],
            corners[(by + 1) * cw + bx],
            centres[by * nbx_u + bx],
        ]
    };

    let inside = |v: f64| if strict { v < 0.0 } else { v <= 0.0 };
    let mut segments: Vec<(EdgeKey, EdgeKey, Point, Point)> = Vec::new();
    let mut fill: Vec<Polyline> = Vec::new();

    // Unmarked blocks that are entirely inside: merged horizontal runs.
    if want_fill {
        for by in 0..nby_u {
            let mut run_start: Option<usize> = None;
            for bx in 0..=nbx_u {
                let full = bx < nbx_u
                    && !marked[by * nbx_u + bx]
                    && block_vals(bx, by).iter().all(|&v| inside(v));
                match (full, run_start) {
                    (true, None) => run_start = Some(bx),
                    (false, Some(s)) => {
                        let x0 = lat.x(i0 + s as i64 * b);
                        let x1 = lat.x(i0 + bx as i64 * b);
                        let y0 = lat.y(j0 + by as i64 * b);
                        let y1 = lat.y(j0 + (by as i64 + 1) * b);
                        fill.push(rect(x0, y0, x1, y1));
                        run_start = None;
                    }
                    _ => {}
                }
            }
        }
    }

    // Refine marked blocks.
    let n = (b + 1) as usize;
    let mut bxs = vec![0.0; n * n];
    let mut bys = vec![0.0; n * n];
    let mut vals = vec![0.0; n * n];
    let mut hcache: Vec<Option<Option<Point>>> = vec![None; n * n];
    let mut vcache: Vec<Option<Option<Point>>> = vec![None; n * n];
    let mut cancelled = false;
    'rows: for by in 0..nby_u {
        if cancel.is_set() {
            cancelled = true;
            break 'rows;
        }
        for bx in 0..nbx_u {
            if !marked[by * nbx_u + bx] {
                continue;
            }
            let bi = i0 + bx as i64 * b;
            let bj = j0 + by as i64 * b;
            for jj in 0..n {
                for ii in 0..n {
                    bxs[jj * n + ii] = lat.x(bi + ii as i64);
                    bys[jj * n + ii] = lat.y(bj + jj as i64);
                }
            }
            f.eval_batch(Input::Slice(&bxs), Input::Slice(&bys), &mut vals);
            hcache.iter_mut().for_each(|c| *c = None);
            vcache.iter_mut().for_each(|c| *c = None);
            let node = |ii: usize, jj: usize| (bxs[jj * n + ii], bys[jj * n + ii]);
            let mut hcross = |ii: usize, jj: usize| -> Option<Point> {
                let idx = jj * n + ii;
                if let Some(c) = hcache[idx] {
                    return c;
                }
                let (va, vb) = (vals[idx], vals[idx + 1]);
                let c = lat.crossing(node(ii, jj), va, node(ii + 1, jj), vb);
                hcache[idx] = Some(c);
                c
            };
            let mut vcross = |ii: usize, jj: usize| -> Option<Point> {
                let idx = jj * n + ii;
                if let Some(c) = vcache[idx] {
                    return c;
                }
                let (va, vb) = (vals[idx], vals[idx + n]);
                let c = lat.crossing(node(ii, jj), va, node(ii, jj + 1), vb);
                vcache[idx] = Some(c);
                c
            };
            let mut run_start: Option<usize> = None;
            for jj in 0..n - 1 {
                for ii in 0..n {
                    // Cell (ii, jj) with corners 00, 10, 11, 01 (counter-clockwise).
                    let in_range = ii < n - 1;
                    let (v00, v10, v11, v01) = if in_range {
                        (
                            vals[jj * n + ii],
                            vals[jj * n + ii + 1],
                            vals[(jj + 1) * n + ii + 1],
                            vals[(jj + 1) * n + ii],
                        )
                    } else {
                        (f64::NAN, f64::NAN, f64::NAN, f64::NAN)
                    };
                    let gi = bi + ii as i64;
                    let gj = bj + jj as i64;
                    let finite =
                        v00.is_finite() && v10.is_finite() && v11.is_finite() && v01.is_finite();
                    // --- contour ---
                    if in_range && finite {
                        let bits = (v00 < 0.0) as u8
                            | ((v10 < 0.0) as u8) << 1
                            | ((v11 < 0.0) as u8) << 2
                            | ((v01 < 0.0) as u8) << 3;
                        let e_bottom: EdgeKey = (gi, gj, 0);
                        let e_top: EdgeKey = (gi, gj + 1, 0);
                        let e_left: EdgeKey = (gi, gj, 1);
                        let e_right: EdgeKey = (gi + 1, gj, 1);
                        let mut seg =
                            |ka: EdgeKey, pa: Option<Point>, kb: EdgeKey, pb: Option<Point>| {
                                if let (Some(pa), Some(pb)) = (pa, pb) {
                                    segments.push((ka, kb, pa, pb));
                                }
                            };
                        match bits {
                            0 | 15 => {}
                            5 | 10 => {
                                let (cx, cy) = (lat.x(gi) + 0.5 * lat.hx, lat.y(gj) + 0.5 * lat.hy);
                                let centre_neg = f.eval(cx, cy) < 0.0;
                                let pb = hcross(ii, jj);
                                let pr = vcross(ii + 1, jj);
                                let pt = hcross(ii, jj + 1);
                                let pl = vcross(ii, jj);
                                // bits 5: 00 and 11 negative. Connected through the
                                // centre if the centre has their sign.
                                let centre_like_00 = centre_neg == (v00 < 0.0);
                                if centre_like_00 {
                                    seg(e_bottom, pb, e_right, pr);
                                    seg(e_top, pt, e_left, pl);
                                } else {
                                    seg(e_left, pl, e_bottom, pb);
                                    seg(e_right, pr, e_top, pt);
                                }
                            }
                            _ => {
                                let mut ends: [(EdgeKey, Option<Point>); 2] =
                                    [((0, 0, 0), None); 2];
                                let mut k = 0;
                                if (bits & 1 != 0) != (bits & 2 != 0) {
                                    ends[k] = (e_bottom, hcross(ii, jj));
                                    k += 1;
                                }
                                if (bits & 2 != 0) != (bits & 4 != 0) {
                                    ends[k] = (e_right, vcross(ii + 1, jj));
                                    k += 1;
                                }
                                if (bits & 4 != 0) != (bits & 8 != 0) && k < 2 {
                                    ends[k] = (e_top, hcross(ii, jj + 1));
                                    k += 1;
                                }
                                if (bits & 8 != 0) != (bits & 1 != 0) && k < 2 {
                                    ends[k] = (e_left, vcross(ii, jj));
                                    k += 1;
                                }
                                if k == 2 {
                                    seg(ends[0].0, ends[0].1, ends[1].0, ends[1].1);
                                }
                            }
                        }
                    }
                    // --- fill ---
                    if want_fill {
                        let ins = [inside(v00), inside(v10), inside(v11), inside(v01)];
                        let full = in_range && ins.iter().all(|&b| b);
                        match (full, run_start) {
                            (true, None) => run_start = Some(ii),
                            (false, Some(s)) => {
                                fill.push(rect(
                                    lat.x(bi + s as i64),
                                    lat.y(gj),
                                    lat.x(gi),
                                    lat.y(gj + 1),
                                ));
                                run_start = None;
                            }
                            _ => {}
                        }
                        if in_range && !full && ins.iter().any(|&b| b) {
                            let corners = [
                                (lat.x(gi), lat.y(gj)),
                                (lat.x(gi + 1), lat.y(gj)),
                                (lat.x(gi + 1), lat.y(gj + 1)),
                                (lat.x(gi), lat.y(gj + 1)),
                            ];
                            let vs = [v00, v10, v11, v01];
                            partial_cell(&lat, &corners, &vs, &ins, &mut fill);
                        }
                    }
                }
            }
        }
    }

    if cancelled {
        return ContourResult {
            curves: Vec::new(),
            fill: Vec::new(),
            missing: true,
        };
    }
    ContourResult {
        curves: join_segments(segments),
        fill,
        missing: false,
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Polyline {
    vec![
        Point::new(x0, y0),
        Point::new(x1, y0),
        Point::new(x1, y1),
        Point::new(x0, y1),
    ]
}

/// Boundary point between an inside corner and an outside one for fills.
fn fill_cross(lat: &Lattice<'_>, p: (f64, f64), vp: f64, q: (f64, f64), vq: f64) -> Point {
    if vp.is_finite() && vq.is_finite() && vp != vq {
        if let Some(c) = lat.crossing(p, vp, q, vq) {
            return c;
        }
        let t = (vp / (vp - vq)).clamp(0.0, 1.0);
        return Point::new(p.0 + t * (q.0 - p.0), p.1 + t * (q.1 - p.1));
    }
    Point::new(0.5 * (p.0 + q.0), 0.5 * (p.1 + q.1))
}

/// Polygon(s) for the inside part of a partially covered cell.
fn partial_cell(
    lat: &Lattice<'_>,
    c: &[(f64, f64); 4],
    v: &[f64; 4],
    ins: &[bool; 4],
    out: &mut Vec<Polyline>,
) {
    let saddle = ins[0] == ins[2] && ins[1] == ins[3] && ins[0] != ins[1];
    if saddle {
        let cx = 0.5 * (c[0].0 + c[2].0);
        let cy = 0.5 * (c[0].1 + c[2].1);
        let centre_in = {
            let fc = lat.f.eval(cx, cy);
            fc <= 0.0
        };
        if !centre_in {
            // Two separate corner triangles.
            for k in 0..4 {
                if ins[k] {
                    let prev = (k + 3) % 4;
                    let next = (k + 1) % 4;
                    let a = fill_cross(lat, c[k], v[k], c[next], v[next]);
                    let b = fill_cross(lat, c[prev], v[prev], c[k], v[k]);
                    out.push(vec![Point::new(c[k].0, c[k].1), a, b]);
                }
            }
            return;
        }
    }
    let mut poly = Vec::with_capacity(6);
    for k in 0..4 {
        let next = (k + 1) % 4;
        if ins[k] {
            poly.push(Point::new(c[k].0, c[k].1));
        }
        if ins[k] != ins[next] {
            poly.push(fill_cross(lat, c[k], v[k], c[next], v[next]));
        }
    }
    if poly.len() >= 3 {
        out.push(poly);
    }
}

/// Joins segments sharing lattice edges into polylines (closed loops end
/// with their first point repeated).
fn join_segments(segs: Vec<(EdgeKey, EdgeKey, Point, Point)>) -> Vec<Polyline> {
    let mut by_edge: HashMap<EdgeKey, Vec<usize>> = HashMap::with_capacity(segs.len() * 2);
    for (i, s) in segs.iter().enumerate() {
        by_edge.entry(s.0).or_default().push(i);
        by_edge.entry(s.1).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    let mut visited_keys: HashSet<EdgeKey> = HashSet::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let (ka, kb, pa, pb) = segs[start];
        let mut fwd = vec![pa, pb];
        let mut key = kb;
        visited_keys.clear();
        // Forward.
        while let Some(next) = by_edge
            .get(&key)
            .and_then(|v| v.iter().copied().find(|&i| !used[i]))
        {
            used[next] = true;
            let s = segs[next];
            let (nk, np) = if s.0 == key { (s.1, s.3) } else { (s.0, s.2) };
            fwd.push(np);
            key = nk;
        }
        let closed = key == ka;
        if !closed {
            // Backward from the start.
            let mut back = Vec::new();
            let mut key = ka;
            while let Some(next) = by_edge
                .get(&key)
                .and_then(|v| v.iter().copied().find(|&i| !used[i]))
            {
                used[next] = true;
                let s = segs[next];
                let (nk, np) = if s.0 == key { (s.1, s.3) } else { (s.0, s.2) };
                back.push(np);
                key = nk;
            }
            if !back.is_empty() {
                back.reverse();
                back.extend(fwd);
                fwd = back;
            }
        } else if let Some(&first) = fwd.first() {
            // Ensure the loop closes exactly.
            if let Some(last) = fwd.last_mut() {
                *last = first;
            }
        }
        if fwd.len() >= 2 {
            out.push(fwd);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile_str;
    use crate::functions::TrigUnit;
    use crate::plot::{total_area, total_length};

    fn vp() -> Viewport {
        Viewport::new(-10.0, 10.0, -10.0, 10.0, 800.0, 800.0)
    }

    #[test]
    fn circle_contour() {
        let f = compile_str("x^2 + y^2 - 25", TrigUnit::Radians).unwrap();
        let r = contour(
            &f,
            &vp(),
            &PlotOptions::default(),
            false,
            false,
            &Cancel(None),
        );
        assert_eq!(r.curves.len(), 1, "one closed loop");
        let c = &r.curves[0];
        assert_eq!(c.first(), c.last());
        let len = total_length(&r.curves);
        assert!((len - 10.0 * std::f64::consts::PI).abs() < 0.05, "{len}");
        for p in c {
            assert!(((p.x * p.x + p.y * p.y).sqrt() - 5.0).abs() < 1e-3);
        }
    }

    #[test]
    fn disc_fill_area() {
        let f = compile_str("x^2 + y^2 - 25", TrigUnit::Radians).unwrap();
        let r = contour(
            &f,
            &vp(),
            &PlotOptions::default(),
            true,
            true,
            &Cancel(None),
        );
        let area = total_area(&r.fill);
        assert!(
            (area - 25.0 * std::f64::consts::PI).abs() / (25.0 * std::f64::consts::PI) < 0.005,
            "{area}"
        );
        for p in &r.fill {
            assert!(super::super::signed_area(p) > 0.0, "counter-clockwise");
        }
    }

    #[test]
    fn poles_are_not_contours() {
        // tan(x·y) = 1 changes sign across its poles too.
        let f = compile_str("tan(x) - y", TrigUnit::Radians).unwrap();
        let r = contour(
            &f,
            &vp(),
            &PlotOptions::default(),
            false,
            false,
            &Cancel(None),
        );
        let v = vp();
        for l in &r.curves {
            for w in l.windows(2) {
                assert!(
                    (w[1].y - w[0].y).abs() < 1.0,
                    "no vertical pole segments: {:?}",
                    w
                );
            }
            let _ = v;
        }
        // y = floor(x) + 0.5 is a staircase of horizontal steps without risers.
        let f = compile_str("floor(x) - y + 0.5", TrigUnit::Radians).unwrap();
        let r = contour(
            &f,
            &vp(),
            &PlotOptions::default(),
            false,
            false,
            &Cancel(None),
        );
        assert!(r.curves.len() >= 20, "{}", r.curves.len());
        for l in &r.curves {
            for w in l.windows(2) {
                assert!((w[1].y - w[0].y).abs() < 1e-9, "riser {:?}", w);
            }
        }
    }

    #[test]
    fn lines_from_products() {
        // x^2 = y^2 is two crossing lines; measure inside the viewport only.
        let f = compile_str("x^2 - y^2", TrigUnit::Radians).unwrap();
        let r = contour(
            &f,
            &vp(),
            &PlotOptions::default(),
            false,
            false,
            &Cancel(None),
        );
        let mut len = 0.0;
        for l in &r.curves {
            for w in l.windows(2) {
                let (mx, my) = (0.5 * (w[0].x + w[1].x), 0.5 * (w[0].y + w[1].y));
                if mx.abs() <= 10.0 && my.abs() <= 10.0 {
                    len += ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt();
                }
                assert!((w[0].x.abs() - w[0].y.abs()).abs() < 1e-6);
            }
        }
        let expected = 2.0 * (20.0f64 * 20.0 * 2.0).sqrt();
        assert!(
            (len - expected).abs() / expected < 0.01,
            "{len} vs {expected}"
        );
    }
}
