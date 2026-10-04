//! Sampling of explicit curves.
//!
//! With the curve's interval form ([`IntervalFn`]) the sampler proves what
//! it draws. The range is first split into boxes, each classified with
//! interval arithmetic:
//!
//! * **off**: f is defined on the box and its enclosure lies wholly above
//!   or below the band around the viewport, so nothing there is visible;
//! * **smooth**: f is defined and continuous with |f″| ≤ M, so chords of
//!   width w ≤ √(8·tol/M) stay within `tol` (the tolerance, in pixels) of
//!   the curve, and points that far apart are drawn;
//! * **steep**: a box at most a pixel wide on which f is continuous and
//!   stays within its end values (± tol): the chord across it is within a
//!   pixel of the curve in every direction;
//! * **peak**: a box at most a pixel wide on which f is continuous but
//!   leaves its end values (a spike narrower than a pixel, an oscillation):
//!   its lowest and highest values are found to within `tol` by branch and
//!   bound, and the stroke runs through them, down that pixel's column;
//! * **gap**: f is not proven continuous on a box narrower than
//!   [`FLOOR_PX`]: a pole, a jump, a domain edge or a hole. The curve
//!   breaks there; where f is proven undefined at one number inside and
//!   the curve meets itself across, the point is a hole (`holes`).
//!
//! Nothing is joined that isn't proven continuous, and no chord strays
//! more than the tolerance from the curve. Boxes the evaluation budget
//! (or a cancel) leaves unclassified fall back to the heuristic sampler
//! below and set `exhausted`.
//!
//! Without an interval form (a bare [`Program`], as in this module's
//! tests) the heuristic sampler is used throughout.

use super::{Cancel, IntervalFn, PlotOptions, Polyline, axis_point, signed_area};
use crate::compile::{Input, Program};
use crate::equation::Axis;
use crate::interval::{Dec, derivs_valid};
use crate::viewport::Viewport;

/// Width of the first boxes the certified sampler classifies (pixels).
const CHUNK_PX: f64 = 32.0;

/// Narrowest box examined (pixels): a discontinuity is located to within
/// this, and a curve ends this close to its domain's edge.
pub(crate) const FLOOR_PX: f64 = 1e-5;

/// Most interval evaluations spent finding one sub-pixel box's extreme
/// values (each way).
const EXTREME_EVALS: usize = 48;

/// Most points per pixel a smooth box may ask for before it is split to
/// get a tighter curvature bound.
const MAX_PTS_PER_PX: f64 = 8.0;

/// Each first box may spend this many times its even share of the
/// budget: a pathological one (an endless oscillation) can't starve the
/// rest of the curve.
const CHUNK_SHARE: usize = 4;

/// Budget charged for one interval evaluation of order 0 and of order 2,
/// in point evaluations (an interval Taylor evaluation costs about that
/// many plain ones).
const COST_IV0: usize = 16;
const COST_IV2: usize = 48;

/// What the certified sampler proved about a box.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    /// Defined, enclosure wholly beyond the band: invisible.
    Off,
    /// Defined and continuous; chords of this width are within tolerance.
    Smooth(f64),
    /// At most a pixel wide, continuous, within its end values.
    Steep,
    /// At most a pixel wide and continuous, reaching beyond its end values:
    /// drawn through its lowest and highest points `(x, y)` (found to
    /// within the tolerance), a stroke down the pixel's column.
    Peak((f64, f64), (f64, f64)),
    /// Continuous, shape not resolved within the budget.
    Continuous,
    /// Not proven continuous at the narrowest width.
    Gap,
    /// Undefined everywhere on the box.
    Undefined,
    /// Not classified (budget or cancel): sampled heuristically.
    Unresolved,
}

/// A jump between neighbouring leaf samples larger than this (pixels) is
/// examined to decide between "steep but continuous" and "discontinuous".
const JUMP_PX: f64 = 2.0;

pub(crate) struct ExplicitSampler<'a> {
    f: &'a Program,
    iv: Option<&'a IntervalFn>,
    /// Proven holes `(t, d)`: f undefined at t, the curve meeting itself
    /// across it at d.
    holes: Vec<(f64, f64)>,
    /// The evaluation count past which the box being classified is left
    /// unresolved (its first box's share of the budget).
    chunk_limit: usize,
    /// Some detail wasn't resolved (a box over its share of the budget, a
    /// continuous box finer than [`CONTINUOUS_FLOOR_PX`]).
    partial: bool,
    axis: Axis,
    t0: f64,
    t1: f64,
    /// Pixels per unit of the independent variable.
    t_px: f64,
    /// Pixels per unit of the dependent variable.
    d_px: f64,
    band_lo: f64,
    band_hi: f64,
    opts: PlotOptions,
    evals: usize,
    exhausted: bool,
    cancel: Cancel<'a>,
    pieces: Vec<Vec<(f64, f64)>>,
    cur: Vec<(f64, f64)>,
}

impl<'a> ExplicitSampler<'a> {
    pub(crate) fn new(
        f: &'a Program,
        axis: Axis,
        vp: &Viewport,
        opts: &PlotOptions,
    ) -> ExplicitSampler<'a> {
        let (t0, t1, t_px, d0, d1, d_px) = match axis {
            Axis::X => (
                vp.x_min,
                vp.x_max,
                vp.width / vp.x_span(),
                vp.y_min,
                vp.y_max,
                vp.height / vp.y_span(),
            ),
            Axis::Y => (
                vp.y_min,
                vp.y_max,
                vp.height / vp.y_span(),
                vp.x_min,
                vp.x_max,
                vp.width / vp.x_span(),
            ),
        };
        let h = d1 - d0;
        ExplicitSampler {
            f,
            iv: None,
            holes: Vec::new(),
            chunk_limit: usize::MAX,
            partial: false,
            axis,
            t0,
            t1,
            t_px,
            d_px,
            band_lo: d0 - h,
            band_hi: d1 + h,
            opts: *opts,
            evals: 0,
            exhausted: false,
            cancel: Cancel(None),
            pieces: Vec::new(),
            cur: Vec::new(),
        }
    }

    /// Proves joins and gaps with `iv` (f's interval form).
    pub(crate) fn set_interval(&mut self, iv: Option<&'a IntervalFn>) {
        self.iv = iv;
    }

    /// Proven holes, as points.
    pub(crate) fn hole_points(&self) -> Vec<super::Point> {
        self.holes
            .iter()
            .filter(|&&(_, d)| d >= self.band_lo && d <= self.band_hi)
            .map(|&(t, d)| axis_point(self.axis, t, d))
            .collect()
    }

    /// Stops refining (as if out of budget) once `cancel` is set.
    pub(crate) fn set_cancel(&mut self, cancel: Cancel<'a>) {
        self.cancel = cancel;
    }

    #[inline]
    fn eval(&mut self, t: f64) -> f64 {
        self.evals += 1;
        if self.evals >= self.opts.max_evals
            || (self.evals.is_multiple_of(1024) && self.cancel.is_set())
        {
            self.exhausted = true;
        }
        self.f.eval(t, 0.0)
    }

    pub(crate) fn exhausted(&self) -> bool {
        self.exhausted || self.partial
    }

    /// Raw continuous pieces `(t, d)` (unclipped).
    #[cfg(test)]
    pub(crate) fn pieces(&self) -> &[Vec<(f64, f64)>] {
        &self.pieces
    }

    pub(crate) fn run(&mut self) {
        if let Some(iv) = self.iv {
            self.run_certified(iv);
            return;
        }
        let h = self.opts.seed_px.max(0.05) / self.t_px;
        if !(h.is_finite() && h > 0.0) {
            return;
        }
        let k0 = (self.t0 / h).floor() - 1.0;
        let k1 = (self.t1 / h).ceil() + 1.0;
        let n = ((k1 - k0) as usize + 1).min(1_000_000);
        let ts: Vec<f64> = (0..n).map(|i| (k0 + i as f64) * h).collect();
        let mut fs = vec![0.0; n];
        self.f
            .eval_batch(Input::Slice(&ts), Input::Scalar(0.0), &mut fs);
        self.evals += n;
        for i in 0..n.saturating_sub(1) {
            self.segment(ts[i], fs[i], ts[i + 1], fs[i + 1], 0);
        }
        self.break_piece();
    }

    /// Charges `n` evaluations to the budget; false once it is spent (or
    /// the caller cancelled).
    fn charge(&mut self, n: usize) -> bool {
        self.evals += n;
        if self.evals >= self.opts.max_evals || self.cancel.is_set() {
            self.exhausted = true;
        }
        !self.exhausted && self.evals < self.chunk_limit
    }

    fn run_certified(&mut self, iv: &IntervalFn) {
        let h = self.opts.seed_px.max(0.05) / self.t_px;
        if !(h.is_finite() && h > 0.0) {
            return;
        }
        // One seed spacing past each edge, as the heuristic sampler does.
        let (lo, hi) = (self.t0 - h, self.t1 + h);
        let chunk = CHUNK_PX / self.t_px;
        let chunks = (((hi - lo) / chunk).ceil() as usize).max(1);
        let share = self.opts.max_evals / chunks * CHUNK_SHARE;
        let mut boxes: Vec<(f64, f64, Kind)> = Vec::new();
        let mut t = lo;
        while t < hi {
            let e = (t + chunk).min(hi);
            let e = if e <= t { hi } else { e };
            self.chunk_limit = self.evals.saturating_add(share);
            self.classify(iv, t, e, &mut boxes);
            if self.evals >= self.chunk_limit {
                self.partial = true;
            }
            t = e;
        }
        self.chunk_limit = usize::MAX;
        self.draw(iv, &boxes, h);
    }

    /// The narrowest box worth splitting around t (in t units).
    fn floor_at(&self, t: f64) -> f64 {
        (FLOOR_PX / self.t_px).max(4.0 * ulp(t))
    }

    /// Classifies [lo, hi], splitting it until each part is decided.
    fn classify(&mut self, iv: &IntervalFn, lo: f64, hi: f64, out: &mut Vec<(f64, f64, Kind)>) {
        if !self.charge(COST_IV0) {
            out.push((lo, hi, Kind::Unresolved));
            return;
        }
        let f0 = iv.enclose(lo, hi);
        let width_px = (hi - lo) * self.t_px;
        let narrowest = (hi - lo) <= self.floor_at(lo.abs().max(hi.abs()));
        if f0.is_empty() {
            out.push((lo, hi, Kind::Undefined));
            return;
        }
        if f0.dec >= Dec::Def && (f0.lo() > self.band_hi || f0.hi() < self.band_lo) {
            out.push((lo, hi, Kind::Off));
            return;
        }
        if f0.dec < Dec::Dac {
            // Not proven continuous (or defined) throughout.
            if narrowest || !self.split(iv, lo, hi, out) {
                out.push((lo, hi, Kind::Gap));
            }
            return;
        }
        // Continuous. A curvature bound gives the chord width that keeps
        // the drawn line within tolerance.
        let tol_d = self.opts.tolerance_px / self.d_px;
        if self.charge(COST_IV2) {
            let s = iv.series(lo, hi, 2);
            if derivs_valid(&s, 2) {
                let (_, m2) = s[2].iv.mig_mag();
                let d2 = 2.0 * m2;
                let step = if d2 > 0.0 {
                    (8.0 * tol_d / d2).sqrt()
                } else {
                    f64::INFINITY
                };
                if step > 0.0 && (hi - lo) / step <= (width_px * MAX_PTS_PER_PX).max(1.0) {
                    out.push((lo, hi, Kind::Smooth(step)));
                    return;
                }
            }
        }
        if width_px <= 1.0 {
            if self.within_ends(iv, lo, hi, f0.lo(), f0.hi(), tol_d) {
                out.push((lo, hi, Kind::Steep));
                return;
            }
            let hi_pt = self.extreme(iv, lo, hi, tol_d, 1.0);
            let lo_pt = self.extreme(iv, lo, hi, tol_d, -1.0);
            match (lo_pt, hi_pt) {
                (Some(a), Some(b)) => out.push((lo, hi, Kind::Peak(a, b))),
                _ => out.push((lo, hi, Kind::Continuous)),
            }
            return;
        }
        if narrowest || self.evals >= self.chunk_limit || !self.split(iv, lo, hi, out) {
            out.push((lo, hi, Kind::Continuous));
        }
    }

    /// Classifies the two halves of [lo, hi]; false if it can't be split.
    fn split(
        &mut self,
        iv: &IntervalFn,
        lo: f64,
        hi: f64,
        out: &mut Vec<(f64, f64, Kind)>,
    ) -> bool {
        let m = 0.5 * (lo + hi);
        if m <= lo || m >= hi {
            return false;
        }
        self.classify(iv, lo, m, out);
        self.classify(iv, m, hi, out);
        true
    }

    /// The point of [lo, hi] where `sign`·f is largest, to within `tol`:
    /// branch and bound on the enclosures, the best point value so far
    /// pruning boxes that can't beat it by `tol`. None if the budget runs
    /// out first.
    fn extreme(
        &mut self,
        iv: &IntervalFn,
        lo: f64,
        hi: f64,
        tol: f64,
        sign: f64,
    ) -> Option<(f64, f64)> {
        let mut best: Option<(f64, f64)> = None;
        let offer = |x: f64, best: &mut Option<(f64, f64)>| {
            let y = self.f.eval(x, 0.0);
            if y.is_finite() && best.is_none_or(|(_, b)| sign * y > sign * b) {
                *best = Some((x, y));
            }
        };
        for x in [lo, 0.5 * (lo + hi), hi] {
            offer(x, &mut best);
        }
        let mut stack = vec![(lo, hi)];
        let mut spent = 0;
        while let Some((a, b)) = stack.pop() {
            spent += 1;
            if spent > EXTREME_EVALS || !self.charge(COST_IV0) {
                return None;
            }
            let e = iv.enclose(a, b);
            if e.is_empty() {
                continue;
            }
            let bound = if sign > 0.0 { e.hi() } else { -e.lo() };
            let (_, bv) = best?;
            if bound <= sign * bv + tol {
                continue;
            }
            let m = 0.5 * (a + b);
            if m <= a || m >= b {
                continue;
            }
            offer(m, &mut best);
            stack.push((a, m));
            stack.push((m, b));
        }
        best
    }

    /// Whether the enclosure [flo, fhi] of f over [lo, hi] lies within the
    /// hull of f's values at the ends, widened by tol.
    fn within_ends(
        &mut self,
        iv: &IntervalFn,
        lo: f64,
        hi: f64,
        flo: f64,
        fhi: f64,
        tol: f64,
    ) -> bool {
        if !self.charge(2 * COST_IV0) {
            return false;
        }
        let a = iv.enclose(lo, lo);
        let b = iv.enclose(hi, hi);
        if a.is_empty() || b.is_empty() {
            return false;
        }
        flo >= a.lo().min(b.lo()) - tol && fhi <= a.hi().max(b.hi()) + tol
    }

    /// Draws the classified boxes, in order.
    fn draw(&mut self, iv: &IntervalFn, boxes: &[(f64, f64, Kind)], h: f64) {
        let mut i = 0;
        while i < boxes.len() {
            let (lo, hi, kind) = boxes[i];
            match kind {
                Kind::Off => {
                    let (fa, fb) = (self.off_value(lo), self.off_value(hi));
                    self.push(lo, fa);
                    self.push(hi, fb);
                }
                Kind::Smooth(step) => {
                    let step = step.min(h);
                    let n = (((hi - lo) / step).ceil() as usize).clamp(1, 1 << 20);
                    let ts: Vec<f64> = (0..=n)
                        .map(|k| {
                            if k == n {
                                hi
                            } else {
                                lo + (hi - lo) * k as f64 / n as f64
                            }
                        })
                        .collect();
                    let mut fs = vec![0.0; ts.len()];
                    self.f
                        .eval_batch(Input::Slice(&ts), Input::Scalar(0.0), &mut fs);
                    self.evals += ts.len();
                    for (&t, &d) in ts.iter().zip(&fs) {
                        if d.is_finite() {
                            self.push(t, d);
                        } else {
                            self.break_piece();
                        }
                    }
                }
                Kind::Peak(a, b) => {
                    let (p, q) = if a.0 <= b.0 { (a, b) } else { (b, a) };
                    for t in [lo, hi] {
                        if !self.f.eval(t, 0.0).is_finite() {
                            self.break_piece();
                        }
                    }
                    let (fa, fb) = (self.f.eval(lo, 0.0), self.f.eval(hi, 0.0));
                    for (t, d) in [(lo, fa), p, q, (hi, fb)] {
                        if d.is_finite() {
                            self.push(t, d);
                        }
                    }
                }
                Kind::Steep | Kind::Continuous => {
                    for t in [lo, hi] {
                        let d = self.f.eval(t, 0.0);
                        if d.is_finite() {
                            self.push(t, d);
                        } else {
                            self.break_piece();
                        }
                    }
                    if kind == Kind::Continuous {
                        // Joined (it is continuous), but its shape between
                        // the ends isn't resolved.
                        self.partial = true;
                    }
                }
                Kind::Gap | Kind::Undefined => {
                    // A run of gaps is one break; a hole if f is proven
                    // undefined at a number inside and meets itself across.
                    let mut j = i;
                    while j + 1 < boxes.len()
                        && matches!(boxes[j + 1].2, Kind::Gap | Kind::Undefined)
                    {
                        j += 1;
                    }
                    let (g0, g1) = (lo, boxes[j].1);
                    let before = self.cur.last().copied();
                    self.break_piece();
                    let after = self.f.eval(g1, 0.0);
                    if let Some((bt, bd)) = before
                        && bt == g0
                        && after.is_finite()
                        && (after - bd).abs() * self.d_px <= self.opts.tolerance_px
                        && let Some(p) = undefined_inside(iv, g0, g1)
                    {
                        self.holes.push((p, 0.5 * (bd + after)));
                    }
                    i = j;
                }
                Kind::Unresolved => {
                    let (fa, fb) = (self.f.eval(lo, 0.0), self.f.eval(hi, 0.0));
                    self.segment(lo, fa, hi, fb, 0);
                }
            }
            i += 1;
        }
        self.break_piece();
    }

    /// f's value at t on an off box, made finite (beyond the band) where it
    /// overflows: the stroke is clipped and fills clamp to the band.
    fn off_value(&self, t: f64) -> f64 {
        let d = self.f.eval(t, 0.0);
        let h = self.band_hi - self.band_lo;
        if d.is_finite() {
            d
        } else if d > 0.0 {
            self.band_hi + h
        } else {
            self.band_lo - h
        }
    }

    fn push(&mut self, t: f64, d: f64) {
        if let Some(last) = self.cur.last()
            && last.0 == t
        {
            return;
        }
        self.cur.push((t, d));
    }

    fn break_piece(&mut self) {
        if self.cur.len() >= 2 {
            self.pieces.push(std::mem::take(&mut self.cur));
        } else {
            self.cur.clear();
        }
    }

    fn segment(&mut self, a: f64, fa: f64, b: f64, fb: f64, depth: u32) {
        match (fa.is_finite(), fb.is_finite()) {
            (false, false) => self.break_piece(),
            (true, false) => {
                self.push(a, fa);
                let (e, fe) = self.find_edge(a, fa, b);
                if e != a {
                    self.finite(a, fa, e, fe, depth);
                }
                self.break_piece();
            }
            (false, true) => {
                self.break_piece();
                let (e, fe) = self.find_edge(b, fb, a);
                self.push(e, fe);
                if e != b {
                    self.finite(e, fe, b, fb, depth);
                }
            }
            (true, true) => {
                self.push(a, fa);
                self.finite(a, fa, b, fb, depth);
            }
        }
    }

    /// Locates the edge of the domain between a finite sample and a
    /// non-finite one; returns the finite point closest to the edge.
    fn find_edge(&mut self, good_t: f64, good_f: f64, bad_t: f64) -> (f64, f64) {
        let mut good = (good_t, good_f);
        let mut bad = bad_t;
        let min_w = (1e-6 / self.t_px).max(good_t.abs() * 4e-16);
        for _ in 0..64 {
            if (bad - good.0).abs() <= min_w {
                break;
            }
            let m = 0.5 * (good.0 + bad);
            if m == good.0 || m == bad {
                break;
            }
            let fm = self.eval(m);
            if fm.is_finite() {
                good = (m, fm);
            } else {
                bad = m;
            }
        }
        good
    }

    /// Both endpoints finite; the current piece ends at `(a, fa)`.
    /// Leaves the current piece ending at `(b, fb)`.
    fn finite(&mut self, a: f64, fa: f64, b: f64, fb: f64, depth: u32) {
        let jump = (fb - fa).abs() * self.d_px;
        let out_same =
            (fa > self.band_hi && fb > self.band_hi) || (fa < self.band_lo && fb < self.band_lo);
        if depth >= self.opts.max_depth || self.exhausted || out_same {
            if !out_same && !self.exhausted && jump > JUMP_PX && self.is_jump(a, fa, b, fb) {
                self.break_piece();
            }
            self.push(b, fb);
            return;
        }
        let m = 0.5 * (a + b);
        let fm = self.eval(m);
        if !fm.is_finite() {
            self.segment(a, fa, m, fm, depth + 1);
            self.segment(m, fm, b, fb, depth + 1);
            return;
        }
        let tol = self.opts.tolerance_px;
        let mut refine = (fm - 0.5 * (fa + fb)).abs() * self.d_px > tol;
        if !refine && jump > JUMP_PX {
            // The midpoint can sit on the chord by coincidence (e.g. sign(x)
            // sampled symmetrically about 0); probe the quarter points too.
            let q1 = a + 0.25 * (b - a);
            let q3 = a + 0.75 * (b - a);
            let f1 = self.eval(q1);
            let f3 = self.eval(q3);
            refine = !f1.is_finite()
                || !f3.is_finite()
                || (f1 - (0.75 * fa + 0.25 * fb)).abs() * self.d_px > tol
                || (f3 - (0.25 * fa + 0.75 * fb)).abs() * self.d_px > tol;
        }
        if refine {
            self.finite(a, fa, m, fm, depth + 1);
            self.finite(m, fm, b, fb, depth + 1);
        } else {
            self.push(b, fb);
        }
    }

    /// Decides whether the change between two close samples is a
    /// discontinuity: keeps halving towards the larger change; a continuous
    /// function's change shrinks below a pixel, a jump or pole's does not.
    fn is_jump(&mut self, mut a: f64, mut fa: f64, mut b: f64, mut fb: f64) -> bool {
        for _ in 0..80 {
            let m = 0.5 * (a + b);
            if m <= a || m >= b {
                break;
            }
            let fm = self.eval(m);
            if !fm.is_finite() {
                return true;
            }
            if (fm - fa).abs() >= (fb - fm).abs() {
                b = m;
                fb = fm;
            } else {
                a = m;
                fa = fm;
            }
            if (fb - fa).abs() * self.d_px < 0.5 {
                return false;
            }
        }
        true
    }

    fn clip_piece(&self, piece: &[(f64, f64)], out: &mut Vec<Polyline>) {
        let (lo, hi) = (self.band_lo, self.band_hi);
        let mut cur: Polyline = Vec::new();
        let inside = |d: f64| d >= lo && d <= hi;
        for i in 0..piece.len() {
            let (t, d) = piece[i];
            if i == 0 {
                if inside(d) {
                    cur.push(axis_point(self.axis, t, d));
                }
                continue;
            }
            let (pt, pd) = piece[i - 1];
            // Parametric range s ∈ [0, 1] of the segment inside [lo, hi].
            let (s0, s1) = if pd == d {
                if inside(d) { (0.0, 1.0) } else { (1.0, 0.0) }
            } else {
                let sl = (lo - pd) / (d - pd);
                let sh = (hi - pd) / (d - pd);
                let (a, b) = if sl < sh { (sl, sh) } else { (sh, sl) };
                (a.max(0.0), b.min(1.0))
            };
            if s0 > s1 {
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
                continue;
            }
            // From the nearer end: next to a pole one end can be 10³⁰ and
            // pd + s·(d − pd) would cancel to nothing.
            let lerp = |s: f64| {
                if s <= 0.5 {
                    axis_point(self.axis, pt + s * (t - pt), pd + s * (d - pd))
                } else {
                    let r = 1.0 - s;
                    axis_point(self.axis, t - r * (t - pt), d - r * (d - pd))
                }
            };
            if cur.is_empty() {
                cur.push(lerp(s0));
            }
            cur.push(if s1 >= 1.0 {
                axis_point(self.axis, t, d)
            } else {
                lerp(s1)
            });
            if s1 < 1.0 {
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
            }
        }
        if cur.len() >= 2 {
            out.push(cur);
        }
    }

    /// Curves to stroke, clipped to the band around the viewport.
    pub(crate) fn stroke_polylines(&self) -> Vec<Polyline> {
        let mut out = Vec::new();
        for p in &self.pieces {
            self.clip_piece(p, &mut out);
        }
        out
    }

    /// Fill polygons for `dependent > f` (`greater`) or `dependent < f`.
    pub(crate) fn fill_polygons(&self, greater: bool) -> Vec<Polyline> {
        let edge = if greater { self.band_hi } else { self.band_lo };
        let mut out = Vec::new();
        for p in &self.pieces {
            let (Some(first), Some(last)) = (p.first(), p.last()) else {
                continue;
            };
            let mut poly: Polyline = p
                .iter()
                .map(|&(t, d)| axis_point(self.axis, t, d.clamp(self.band_lo, self.band_hi)))
                .collect();
            poly.push(axis_point(self.axis, last.0, edge));
            poly.push(axis_point(self.axis, first.0, edge));
            let area = signed_area(&poly);
            if area.abs() <= 0.0 {
                continue;
            }
            if area < 0.0 {
                poly.reverse();
            }
            out.push(poly);
        }
        out
    }
}

/// The spacing of doubles at |t|.
fn ulp(t: f64) -> f64 {
    let a = t.abs();
    if a.is_finite() {
        f64::from_bits(a.to_bits() + 1) - a
    } else {
        f64::MAX
    }
}

/// A number in [g0, g1] at which f is proven undefined: the shortest
/// decimal in the box (where a removable hole like x/x's sits), 0, then
/// the box's middle and ends.
fn undefined_inside(iv: &IntervalFn, g0: f64, g1: f64) -> Option<f64> {
    let mut candidates = Vec::new();
    let m = 0.5 * (g0 + g1);
    for digits in 0..17 {
        let c: f64 = format!("{m:.digits$e}").parse().unwrap_or(m);
        if c >= g0 && c <= g1 {
            candidates.push(c);
            break;
        }
    }
    if g0 <= 0.0 && g1 >= 0.0 {
        candidates.push(0.0);
    }
    candidates.extend([m, g0, g1]);
    candidates
        .into_iter()
        .find(|&p| iv.enclose(p, p).is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile_str;
    use crate::functions::TrigUnit;

    fn sample(src: &str, vp: &Viewport) -> (Vec<Polyline>, Vec<Vec<(f64, f64)>>) {
        let p = compile_str(src, TrigUnit::Radians).unwrap();
        let mut s = ExplicitSampler::new(&p, Axis::X, vp, &PlotOptions::default());
        s.run();
        (s.stroke_polylines(), s.pieces().to_vec())
    }

    fn vp() -> Viewport {
        Viewport::new(-10.0, 10.0, -10.0, 10.0, 800.0, 800.0)
    }

    /// No stroked segment may cross the visible area vertically by more
    /// than `max_px` pixels while spanning less than a pixel horizontally
    /// near the given x values.
    fn no_connector_near(lines: &[Polyline], v: &Viewport, xs: &[f64]) {
        for l in lines {
            for w in l.windows(2) {
                let dy_px = (w[1].y - w[0].y).abs() / v.y_per_px();
                let mid = 0.5 * (w[0].x + w[1].x);
                for x in xs {
                    if (mid - x).abs() < v.x_per_px() {
                        assert!(
                            dy_px < v.height,
                            "connector across x={x}: {:?} -> {:?}",
                            w[0],
                            w[1]
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn tan_breaks_at_asymptotes() {
        let v = vp();
        let (lines, _) = sample("tan(x)", &v);
        let poles: Vec<f64> = (-3..=2)
            .map(|k| std::f64::consts::FRAC_PI_2 + k as f64 * std::f64::consts::PI)
            .collect();
        no_connector_near(&lines, &v, &poles);
        // One branch per period visible: 7 branches in [-10, 10].
        assert_eq!(lines.len(), 7, "{}", lines.len());
    }

    #[test]
    fn reciprocal_breaks_at_zero() {
        let v = vp();
        let (lines, _) = sample("1/x", &v);
        assert_eq!(lines.len(), 2);
        no_connector_near(&lines, &v, &[0.0]);
        // Shifted so no sample lands exactly on the pole.
        let (lines, _) = sample("1/(x-0.0123)", &v);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn steep_continuous_stays_connected() {
        let v = vp();
        let (lines, _) = sample("arctan(1000000x)", &v);
        assert_eq!(lines.len(), 1);
        let (lines, _) = sample("x^(1/3)", &v);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn floor_breaks_at_integers() {
        let v = vp();
        let (lines, _) = sample("floor(x)", &v);
        // Sampling extends one pixel past each edge: [-10.025, 10.025] holds 22 steps.
        assert_eq!(lines.len(), 22, "{}", lines.len());
        let (lines, _) = sample("sign(x)", &v);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn sqrt_reaches_domain_edge() {
        let v = vp();
        let (lines, _) = sample("sqrt(x)", &v);
        assert_eq!(lines.len(), 1);
        let first = lines[0][0];
        assert!(first.x.abs() < 1e-6 && first.y.abs() < 1e-3, "{first:?}");
        let (lines, _) = sample("sqrt(4 - x^2)", &v);
        assert_eq!(lines.len(), 1);
        let l = &lines[0];
        assert!((l[0].x + 2.0).abs() < 1e-6 && l[0].y.abs() < 1e-2);
        assert!((l[l.len() - 1].x - 2.0).abs() < 1e-6);
    }

    #[test]
    fn output_is_finite_and_clipped() {
        let v = vp();
        for src in ["1/x", "tan(x)", "e^x", "ln(x)", "1/x^2", "x^10"] {
            let (lines, _) = sample(src, &v);
            for l in &lines {
                for p in l {
                    assert!(p.x.is_finite() && p.y.is_finite());
                    assert!(p.y >= -30.0 - 1e-9 && p.y <= 30.0 + 1e-9, "{src}: {p:?}");
                }
            }
        }
    }

    #[test]
    fn smooth_curve_is_accurate() {
        let v = vp();
        let (lines, _) = sample("sin(x)", &v);
        assert_eq!(lines.len(), 1);
        for w in lines[0].windows(2) {
            let mid = 0.5 * (w[0].x + w[1].x);
            let chord = 0.5 * (w[0].y + w[1].y);
            assert!((mid.sin() - chord).abs() / v.y_per_px() < 0.3);
        }
    }

    #[test]
    fn fill_below_line() {
        let v = vp();
        let p = compile_str("x", TrigUnit::Radians).unwrap();
        let mut s = ExplicitSampler::new(&p, Axis::X, &v, &PlotOptions::default());
        s.run();
        let polys = s.fill_polygons(false);
        assert_eq!(polys.len(), 1);
        assert!(signed_area(&polys[0]) > 0.0);
    }
}
