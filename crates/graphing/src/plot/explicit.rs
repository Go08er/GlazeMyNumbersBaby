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
//! The range is classified in two passes: first its structure (where f is
//! continuous, undefined or beyond the band, and where it breaks: order-0
//! enclosures only), then the shape of the continuous parts (smooth,
//! steep, peak). The breaks — poles, holes, jumps, domain edges — so come
//! before any refinement in the budget's order.
//!
//! Nothing is joined that isn't proven continuous, and no chord strays
//! more than the tolerance from the curve. No vertex lies outside f's
//! enclosure at its point (with the literals' exact decimals): a point
//! value outside it is moved into it where the enclosure is within the
//! tolerance, else the curve breaks there (`push_vertex`).
//!
//! A continuous box the budget leaves unrefined is still joined (it's
//! proven continuous), through point samples a seed spacing apart; a box
//! whose structure the budget (or a cancel) leaves unknown falls back to
//! the heuristic sampler below, which breaks rather than joins any jump
//! it can't examine, and draws its point values unchecked, as a dense
//! column's stroke does. Each sets `has_missing_data`.
//!
//! The budget ([`PlotOptions::max_work`]) is a deterministic count of
//! estimated cost, never a time: an evaluation is charged by the size of
//! f ([`Program::cost`], [`IntervalFn::cost`]), an interval one many
//! times a point one.
//!
//! Without an interval form (a bare [`Program`], as in this module's
//! tests) the heuristic sampler is used throughout.

use super::{Cancel, IntervalFn, PlotOptions, Polyline, axis_point, signed_area};
use crate::compile::{Input, Program};
use crate::equation::Axis;
use crate::interval::{Dec, DecInterval, derivs_valid};
use crate::viewport::Viewport;

/// Width of the first boxes the certified sampler classifies (pixels).
const CHUNK_PX: f64 = 32.0;

/// Narrowest box examined (pixels): a discontinuity is located to within
/// this, and a curve ends this close to its domain's edge.
pub(crate) const FLOOR_PX: f64 = 1e-5;

/// Most interval evaluations spent finding one sub-pixel box's extreme
/// values (each way).
const EXTREME_EVALS: usize = 96;

/// Most points per pixel a smooth box may ask for before it is split to
/// get a tighter curvature bound.
const MAX_PTS_PER_PX: f64 = 8.0;

/// Each first box may spend this many times its even share of the
/// budget: a pathological one (an endless oscillation) can't starve the
/// rest of the curve.
const CHUNK_SHARE: usize = 4;

/// Share of the budget the first pass (the structure) may spend; the rest
/// is kept for the shapes of the continuous parts.
const STRUCTURE_SHARE: f64 = 0.75;

/// Extra budget, as a share of the whole, for telling jumps from steep
/// parts with point samples where the structure is unknown.
const FALLBACK_SHARE: f64 = 0.25;

/// Most interval evaluations the first pass spends below one pixel column
/// that isn't proven continuous: about two features (a pole, a hole, a
/// domain edge) located to [`FLOOR_PX`]. A column with more (`tan(30x)`
/// at ±1000: 25 poles a pixel) is drawn as a stroke down the column
/// through its sampled values ([`Kind::Dense`]).
const COLUMN_EVALS: usize = 96;

/// [`COLUMN_EVALS`] for a column right after a dense one: dense poles come
/// in runs (`tan 100x` at ±1000 has one run across the whole view), and
/// spending the full allowance on each column before giving up would use
/// the budget up halfway across, leaving the rest to the fallback.
const DENSE_RUN_EVALS: usize = 20;

/// Point samples across a dense column.
const DENSE_SAMPLES: usize = 16;

/// A run of unproven boxes wider than this (pixels) is no hole.
const HOLE_RUN_PX: f64 = 1e-3;

/// Estimated cost of one point evaluation and of interval evaluations of
/// order 0 and 2 of a function of cost `c` ([`Program::cost`] for points,
/// [`IntervalFn::cost`] for intervals), in units calibrated to about a
/// nanosecond each on x/x, tan x, sin(3x)·e^(−x/4)+x², x! and a 59-term
/// max of sin(kx)^k (within a factor of two). Fixed numbers: the budget
/// is counted, not timed.
fn costs(c: usize) -> (usize, usize, usize) {
    (4 + 4 * c, 500 + 70 * c, 1200 + 300 * c)
}

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
    /// Continuous with values in `[lo, hi]` (the first pass's verdict;
    /// the second pass refines it into the kinds above).
    Proven(f64, f64),
    /// Not proven continuous at the narrowest width.
    Gap,
    /// Undefined everywhere on the box.
    Undefined,
    /// Not classified (budget or cancel): sampled heuristically.
    Unresolved,
    /// At most a pixel wide, with more breaks than [`COLUMN_EVALS`]
    /// locates: a stroke down the column through its sampled values, not
    /// joined to either side.
    Dense,
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
    /// The work past which the box being classified is left unresolved
    /// (its first box's share of the budget).
    chunk_limit: usize,
    /// The work past which the first pass stops.
    pass_limit: usize,
    /// Work spent telling jumps from steep parts in unresolved boxes.
    fallback_work: usize,
    /// Interval evaluations spent in the column being structured (None
    /// outside one), and the most it may spend.
    column: Option<usize>,
    column_limit: usize,
    /// The last column structured was dense.
    dense_run: bool,
    /// Costs of a point and an order-0 and order-2 interval evaluation.
    pt_cost: usize,
    iv0_cost: usize,
    iv2_cost: usize,
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
    /// Work done so far (see [`costs`]).
    work: usize,
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
        let (pt_cost, _, _) = costs(f.cost());
        ExplicitSampler {
            f,
            iv: None,
            holes: Vec::new(),
            chunk_limit: usize::MAX,
            pass_limit: usize::MAX,
            fallback_work: 0,
            column: None,
            column_limit: COLUMN_EVALS,
            dense_run: false,
            pt_cost,
            iv0_cost: 0,
            iv2_cost: 0,
            partial: false,
            axis,
            t0,
            t1,
            t_px,
            d_px,
            band_lo: d0 - h,
            band_hi: d1 + h,
            opts: *opts,
            work: 0,
            exhausted: false,
            cancel: Cancel(None),
            pieces: Vec::new(),
            cur: Vec::new(),
        }
    }

    /// Proves joins and gaps with `iv` (f's interval form).
    pub(crate) fn set_interval(&mut self, iv: Option<&'a IntervalFn>) {
        self.iv = iv;
        if let Some(iv) = iv {
            (_, self.iv0_cost, self.iv2_cost) = costs(iv.cost());
        }
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
        let before = self.work;
        self.work += self.pt_cost;
        if self.work >= self.opts.max_work
            || (self.work >> 16 != before >> 16 && self.cancel.is_set())
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
        self.work += n * self.pt_cost;
        for i in 0..n.saturating_sub(1) {
            self.segment(ts[i], fs[i], ts[i + 1], fs[i + 1], 0);
        }
        self.break_piece();
    }

    /// Charges `n` work to the budget; false once it is spent (or the
    /// caller cancelled), or the box's or the pass's share is.
    fn charge(&mut self, n: usize) -> bool {
        self.work += n;
        if self.work >= self.opts.max_work || self.cancel.is_set() {
            self.exhausted = true;
        }
        !self.exhausted && self.work < self.chunk_limit && self.work < self.pass_limit
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
        // Each first box may spend `times` its even share of what the pass
        // has left: the structure first (an even share, so a view of dense
        // poles degrades alike from left to right), then the shapes
        // (CHUNK_SHARE times).
        let share = |limit: usize, work: usize, done: usize, times: usize| {
            let left = chunks.saturating_sub(done).max(1);
            work.saturating_add(limit.saturating_sub(work) / left * times)
        };
        self.pass_limit = (self.opts.max_work as f64 * STRUCTURE_SHARE) as usize;
        let mut firsts: Vec<Vec<(f64, f64, Kind)>> = Vec::with_capacity(chunks);
        let mut t = lo;
        while t < hi {
            let e = (t + chunk).min(hi);
            let e = if e <= t { hi } else { e };
            self.chunk_limit = share(self.pass_limit, self.work, firsts.len(), 1);
            let mut first = Vec::new();
            self.structure(iv, t, e, &mut first);
            firsts.push(first);
            t = e;
        }
        self.pass_limit = usize::MAX;
        let mut boxes: Vec<(f64, f64, Kind)> = Vec::new();
        for (k, first) in firsts.into_iter().enumerate() {
            self.chunk_limit = share(self.opts.max_work, self.work, k, CHUNK_SHARE);
            for (a, b, kind) in first {
                match kind {
                    Kind::Proven(flo, fhi) => self.shape(iv, a, b, flo, fhi, &mut boxes),
                    _ => boxes.push((a, b, kind)),
                }
            }
        }
        self.chunk_limit = usize::MAX;
        self.draw(iv, &boxes, h);
    }

    /// The narrowest box worth splitting around t (in t units).
    fn floor_at(&self, t: f64) -> f64 {
        (FLOOR_PX / self.t_px).max(4.0 * ulp(t))
    }

    /// First pass: splits [lo, hi] until each part is proven continuous
    /// (`Proven`), undefined, beyond the band, or a narrowest `Gap`.
    fn structure(&mut self, iv: &IntervalFn, lo: f64, hi: f64, out: &mut Vec<(f64, f64, Kind)>) {
        if let Some(n) = &mut self.column {
            *n += 1;
            if *n > self.column_limit {
                // Dropped: the whole column is dense.
                return;
            }
        }
        if !self.charge(self.iv0_cost) {
            if let Some(n) = &mut self.column {
                // Out of budget inside a column already known to break:
                // drawn as a dense one.
                *n = usize::MAX / 2;
                return;
            }
            out.push((lo, hi, Kind::Unresolved));
            return;
        }
        let f0 = iv.enclose(lo, hi);
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
            let m = 0.5 * (lo + hi);
            let narrowest = (hi - lo) <= self.floor_at(lo.abs().max(hi.abs()));
            if narrowest || m <= lo || m >= hi {
                out.push((lo, hi, Kind::Gap));
            } else if self.column.is_none() && (hi - lo) * self.t_px <= 1.0 {
                // A column: its breaks are located within COLUMN_EVALS
                // (fewer next to a dense one).
                let mark = out.len();
                self.column = Some(0);
                self.column_limit = if self.dense_run {
                    DENSE_RUN_EVALS
                } else {
                    COLUMN_EVALS
                };
                self.structure(iv, lo, m, out);
                self.structure(iv, m, hi, out);
                let n = self.column.take().unwrap_or(0);
                self.dense_run = n > self.column_limit;
                if self.dense_run {
                    out.truncate(mark);
                    out.push((lo, hi, Kind::Dense));
                }
            } else {
                self.structure(iv, lo, m, out);
                self.structure(iv, m, hi, out);
            }
            return;
        }
        out.push((lo, hi, Kind::Proven(f0.lo(), f0.hi())));
    }

    /// Second pass: how to draw [lo, hi], on which f is continuous with
    /// values in [flo, fhi]; split until decided.
    #[allow(clippy::too_many_arguments)]
    fn shape(
        &mut self,
        iv: &IntervalFn,
        lo: f64,
        hi: f64,
        flo: f64,
        fhi: f64,
        out: &mut Vec<(f64, f64, Kind)>,
    ) {
        if flo > self.band_hi || fhi < self.band_lo {
            out.push((lo, hi, Kind::Off));
            return;
        }
        // A curvature bound gives the chord width that keeps the drawn
        // line within tolerance. At most a pixel wide, the cheaper test
        // that the ends bound it comes first.
        let width_px = (hi - lo) * self.t_px;
        let tol_d = self.opts.tolerance_px / self.d_px;
        if width_px <= 1.0 && self.within_ends(iv, lo, hi, flo, fhi, tol_d) {
            out.push((lo, hi, Kind::Steep));
            return;
        }
        if self.charge(self.iv2_cost) {
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
            let hi_pt = self.extreme(iv, lo, hi, tol_d, 1.0);
            let lo_pt = self.extreme(iv, lo, hi, tol_d, -1.0);
            match (lo_pt, hi_pt) {
                (Some(a), Some(b)) => out.push((lo, hi, Kind::Peak(a, b))),
                _ => out.push((lo, hi, Kind::Continuous)),
            }
            return;
        }
        let m = 0.5 * (lo + hi);
        let narrowest = (hi - lo) <= self.floor_at(lo.abs().max(hi.abs()));
        if narrowest || m <= lo || m >= hi || !self.charge(2 * self.iv0_cost) {
            out.push((lo, hi, Kind::Continuous));
            return;
        }
        for (a, b) in [(lo, m), (m, hi)] {
            // Each half is continuous too; its own enclosure is tighter.
            let e = iv.enclose(a, b);
            let (elo, ehi) = (e.lo().max(flo), e.hi().min(fhi));
            if e.is_empty() || elo > ehi {
                self.shape(iv, a, b, flo, fhi, out);
            } else {
                self.shape(iv, a, b, elo, ehi, out);
            }
        }
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
            if spent > EXTREME_EVALS || !self.charge(self.iv0_cost) {
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
        if !self.charge(2 * self.iv0_cost) {
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
                    self.work += 2 * self.pt_cost;
                    for t in [lo, hi] {
                        let d = self.f.eval(t, 0.0);
                        self.push_vertex(iv, t, d);
                    }
                }
                Kind::Dense => {
                    self.partial = true;
                    self.break_piece();
                    let n = DENSE_SAMPLES;
                    self.work += (n + 1) * self.pt_cost;
                    let (mut a, mut b) = (f64::INFINITY, f64::NEG_INFINITY);
                    for k in 0..=n {
                        let d = self.f.eval(lo + (hi - lo) * k as f64 / n as f64, 0.0);
                        if d.is_finite() {
                            a = a.min(d);
                            b = b.max(d);
                        }
                    }
                    if a < b {
                        let t = 0.5 * (lo + hi);
                        self.pieces.push(vec![(t, a), (t, b)]);
                    }
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
                    self.work += ts.len() * self.pt_cost;
                    for (&t, &d) in ts.iter().zip(&fs) {
                        self.push_vertex(iv, t, d);
                    }
                }
                Kind::Peak(a, b) => {
                    let (p, q) = if a.0 <= b.0 { (a, b) } else { (b, a) };
                    self.work += 2 * self.pt_cost;
                    let (fa, fb) = (self.f.eval(lo, 0.0), self.f.eval(hi, 0.0));
                    for (t, d) in [(lo, fa), p, q, (hi, fb)] {
                        self.push_vertex(iv, t, d);
                    }
                }
                Kind::Steep | Kind::Continuous | Kind::Proven(..) => {
                    // A continuous box whose shape wasn't resolved is still
                    // joined (it is continuous), through point samples half
                    // a seed spacing apart.
                    let n = if kind == Kind::Steep {
                        1
                    } else {
                        self.partial = true;
                        ((2.0 * (hi - lo) / h).ceil() as usize).clamp(1, 1 << 20)
                    };
                    self.work += (n + 1) * self.pt_cost;
                    for k in 0..=n {
                        let t = if k == n {
                            hi
                        } else {
                            lo + (hi - lo) * k as f64 / n as f64
                        };
                        let d = self.f.eval(t, 0.0);
                        self.push_vertex(iv, t, d);
                    }
                }
                Kind::Gap | Kind::Undefined => {
                    // A run of gaps is one break; a hole if it is narrower
                    // than HOLE_RUN_PX with no box of it proven undefined,
                    // f is proven defined on both sides of it and meets
                    // itself across, and is proven undefined at a number
                    // inside.
                    let mut j = i;
                    let mut undefined = kind == Kind::Undefined;
                    while j + 1 < boxes.len()
                        && matches!(boxes[j + 1].2, Kind::Gap | Kind::Undefined)
                    {
                        j += 1;
                        undefined |= boxes[j].2 == Kind::Undefined;
                    }
                    let (g0, g1) = (lo, boxes[j].1);
                    let before = self.cur.last().copied();
                    self.break_piece();
                    let defined = |k: usize| {
                        boxes.get(k).is_some_and(|b| {
                            matches!(
                                b.2,
                                Kind::Smooth(_)
                                    | Kind::Steep
                                    | Kind::Peak(..)
                                    | Kind::Continuous
                                    | Kind::Off
                            )
                        })
                    };
                    let narrow = g1 - g0
                        <= (HOLE_RUN_PX / self.t_px).max(16.0 * ulp(g0.abs().max(g1.abs())));
                    if !undefined
                        && narrow
                        && i > 0
                        && defined(i - 1)
                        && defined(j + 1)
                        && before.is_some_and(|(bt, _)| bt == g0)
                        && let Some(p) = undefined_inside(iv, g0, g1)
                        && let Some(v) = self.removable(iv, p, (p - g0).max(g1 - p))
                    {
                        self.holes.push((p, v));
                    }
                    i = j;
                }
                Kind::Unresolved => {
                    // Not even its structure is known: sampled half a seed
                    // spacing apart, joined only where the heuristic
                    // sampler finds no jump (so no chord is a pixel wide).
                    self.partial = true;
                    let n = ((2.0 * (hi - lo) / h).ceil() as usize).clamp(1, 1 << 20);
                    let mut prev = (lo, self.eval(lo));
                    for k in 1..=n {
                        let t = if k == n {
                            hi
                        } else {
                            lo + (hi - lo) * k as f64 / n as f64
                        };
                        let d = self.eval(t);
                        self.segment(prev.0, prev.1, t, d, 0);
                        prev = (t, d);
                    }
                }
            }
            i += 1;
        }
        self.break_piece();
    }

    /// A value beyond the band made finite (an overflow, `sinh(10²⁰x)`):
    /// the stroke is clipped and fills clamp to the band.
    fn off_value_of(&self, d: f64) -> f64 {
        let h = self.band_hi - self.band_lo;
        if d.is_finite() {
            d
        } else if d > 0.0 {
            self.band_hi + h
        } else {
            self.band_lo - h
        }
    }

    /// The value of a hole at `p` (where f is undefined, within a run of
    /// unproven boxes reaching `r` either side of it), or None: f at p ± r
    /// must meet within the tolerance, and eight doubles either side of p
    /// may still be within the tolerance of their mean `v` and not much
    /// further from it than at p ± r. So a pole that outgrows the
    /// tolerance only that close in (`x!` at −13 at the default view:
    /// ±0.0005 a hundred-thousandth of a pixel away, ±10⁴ eight doubles
    /// away), or at least grows towards p (`x!` at −20: 10⁻¹¹, then
    /// 10⁻⁴), is no hole; nor is a factorial's pole where doubles see no
    /// growth at all ([`IntervalFn::pole_at`]: x! near −1000). Enclosures, not point values, close in, so a
    /// cancelling form (`(x³−8)/(x−2)`) isn't mistaken for growth.
    ///
    /// Nor is a jump, however small: the sides must meet, not merely come
    /// within the tolerance (R12-L-01: `sign(x)·(x/x)/1000` jumps by a
    /// fifteenth of a pixel). See [`sides_converge`].
    fn removable(&self, iv: &IntervalFn, p: f64, r: f64) -> Option<f64> {
        if iv.pole_at(p) {
            return None;
        }
        let tol = self.opts.tolerance_px / self.d_px;
        let (l, h) = (self.f.eval(p - r, 0.0), self.f.eval(p + r, 0.0));
        if !(l.is_finite() && h.is_finite() && (h - l).abs() <= tol) {
            return None;
        }
        let v = 0.5 * (l + h);
        let dist = |t: f64| {
            let e = iv.enclose(t, t);
            if e.is_empty() {
                f64::INFINITY
            } else {
                (e.lo() - v).max(v - e.hi()).max(0.0)
            }
        };
        let far = dist(p - r).max(dist(p + r));
        let (mut a, mut b) = (p, p);
        for _ in 0..8 {
            a = a.next_down();
            b = b.next_up();
        }
        ([a, b].into_iter().all(|near| {
            let d = dist(near);
            d <= tol && d <= 2.0 * far + 4.0 * ulp(v)
        }) && sides_converge(iv, p, r))
        .then_some(v)
    }

    /// Draws a vertex at t, on a box f is proven defined (and, but for an
    /// off box, continuous) on, where the point evaluator gives `d`: never
    /// outside f's enclosure at t ([`super::drawn_value`]: `d` where the
    /// enclosure holds it, else a value in it if it is within the
    /// tolerance), made finite beyond the band like
    /// [`Self::off_value_of`]. Where `d` lies outside a wider enclosure
    /// (or is NaN) the point is undecided: the curve breaks there, joined
    /// neither into it nor out of it, and the plot is partial. An
    /// interval evaluation, but for a box's first vertex (its
    /// neighbour's last, drawn already).
    fn push_vertex(&mut self, iv: &IntervalFn, t: f64, d: f64) {
        if self.cur.last().is_some_and(|&(last, _)| last == t) {
            return;
        }
        self.work += self.iv0_cost;
        let e = iv.enclose(t, t);
        let tol = self.opts.tolerance_px / self.d_px;
        match super::drawn_value(d, e.lo(), e.hi(), tol) {
            Some(v) => self.push(t, self.off_value_of(v)),
            None => {
                self.partial = true;
                self.break_piece();
            }
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
            if !out_same && jump > JUMP_PX && self.jump_or_unknown(a, fa, b, fb) {
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

    /// [`Self::is_jump`], or true (break, don't join) once the budget and
    /// the extra [`FALLBACK_SHARE`] for this are spent.
    fn jump_or_unknown(&mut self, a: f64, fa: f64, b: f64, fb: f64) -> bool {
        if !self.exhausted {
            return self.is_jump(a, fa, b, fb);
        }
        if self.fallback_work as f64 >= self.opts.max_work as f64 * FALLBACK_SHARE
            || self.cancel.is_set()
        {
            return true;
        }
        let w = self.work;
        let r = self.is_jump(a, fa, b, fb);
        self.fallback_work += self.work - w;
        r
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
            // pt + s·(t − pt) would cancel to nothing. Where the segment
            // crosses the band its dependent value is the band's edge, not
            // pd + s·(d − pd): for a line steeper than 10¹⁹ per pixel both
            // crossings round to the same s and the stroke would collapse
            // to a point instead of running down the pixel's column.
            let lerp = |s: f64, edge: f64| {
                let t = if s <= 0.5 {
                    pt + s * (t - pt)
                } else {
                    t - (1.0 - s) * (t - pt)
                };
                axis_point(self.axis, t, edge)
            };
            let (enter, leave) = if d > pd { (lo, hi) } else { (hi, lo) };
            if cur.is_empty() {
                cur.push(if s0 <= 0.0 {
                    axis_point(self.axis, pt, pd)
                } else {
                    lerp(s0, enter)
                });
            }
            cur.push(if s1 >= 1.0 {
                axis_point(self.axis, t, d)
            } else {
                lerp(s1, leave)
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
        // `clamp` panics on NaN bounds (a viewport that isn't sane).
        if !(self.band_lo.is_finite() && self.band_hi.is_finite() && self.band_lo <= self.band_hi) {
            return Vec::new();
        }
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

/// Most rungs of [`sides_converge`]'s ladder above the doubles next to p.
const CONVERGE_RUNGS: usize = 24;

/// Whether f's one-sided enclosures beside p converge to one value within
/// their own width, from `r` away down to the doubles next to p: f's
/// enclosures either side at p ± r, p ± r/8, p ± r/64, … (to
/// [`CONVERGE_RUNGS`] rungs, and at least four times further out than
/// the next), eight doubles out and at the nearest doubles where f is
/// defined. At each rung the sides overlap, or are apart by no more than
/// their own widths and twice the gap one rung out scaled down by the
/// distance (f's slope parting the values beside a hole closes the gap in
/// step with the distance: `1/x!` at −1 is −2.2·10⁻¹⁶ and 1.1·10⁻¹⁶ at the
/// nearest doubles, each enclosed to 10⁻³¹); a gap that stays wherever
/// the enclosures are narrower than it is a jump (`sign(x)·(x/x)/1000`;
/// `(x²−1)/(x−1) + sign(x−1)/1000`, whose enclosures widen past the jump
/// only within 10⁻¹³ of 1). Not a proof that the limits are equal, which
/// no enclosure at a double can give: a jump narrower than f's
/// enclosures beside p, or than its change across a few doubles there,
/// still passes.
fn sides_converge(iv: &IntervalFn, p: f64, r: f64) -> bool {
    let (mut a8, mut b8) = (p, p);
    // The nearest doubles either side where f is defined.
    let (mut left, mut right) = ((p, None), (p, None));
    for _ in 0..8 {
        a8 = a8.next_down();
        b8 = b8.next_up();
        if left.1.is_none() {
            left = (a8, Some(iv.enclose(a8, a8)).filter(|e| !e.is_empty()));
        }
        if right.1.is_none() {
            right = (b8, Some(iv.enclose(b8, b8)).filter(|e| !e.is_empty()));
        }
    }
    let ((a1, Some(left)), (b1, Some(right))) = (left, right) else {
        return false;
    };
    let d8 = (p - a8).max(b8 - p);
    let mut rungs = Vec::with_capacity(CONVERGE_RUNGS + 1);
    let mut d = r;
    while rungs.len() < CONVERGE_RUNGS && d > 4.0 * d8 {
        rungs.push((p - d, p + d));
        d /= 8.0;
    }
    rungs.push((a8, b8));
    // (Distance across, enclosures either side.)
    let mut sides: Vec<(f64, DecInterval, DecInterval)> = rungs
        .into_iter()
        .map(|(a, b)| (b - a, iv.enclose(a, a), iv.enclose(b, b)))
        .collect();
    sides.push((b1 - a1, left, right));
    if sides.iter().any(|(_, l, r)| l.is_empty() || r.is_empty()) {
        return false;
    }
    let gap = |l: &DecInterval, r: &DecInterval| (l.lo().max(r.lo()) - l.hi().min(r.hi())).max(0.0);
    sides.windows(2).all(|w| {
        let ((d0, l0, r0), (d1, l1, r1)) = (&w[0], &w[1]);
        let widths = (l1.hi() - l1.lo()) + (r1.hi() - r1.lo());
        gap(l1, r1) - widths <= 2.0 * (d1 / d0) * gap(l0, r0)
    })
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

    /// `f` sampled with `g`'s interval form (a point evaluator at odds
    /// with the function it stands for): the strokes, and whether the
    /// plot is partial.
    fn sample_with(f: &str, g: &str, v: &Viewport) -> (Vec<Polyline>, bool) {
        let p = compile_str(f, TrigUnit::Radians).unwrap();
        let text = format!("y={g}");
        let eq = crate::Equation::parse(&text).unwrap();
        let lits = crate::interval::Literals::of(&text, Default::default()).unwrap();
        let opts = crate::compile::CompileOptions::default();
        let iv = IntervalFn::new(eq.explicit().unwrap().1.clone(), lits, &opts);
        let mut s = ExplicitSampler::new(&p, Axis::X, v, &PlotOptions::default());
        s.set_interval(Some(&iv));
        s.run();
        (s.stroke_polylines(), s.exhausted())
    }

    /// R12-M-05: no vertex is drawn outside f's point enclosure. Where the
    /// point value is off and the enclosure is within the tolerance, the
    /// vertex moves into it; where the enclosure is wider, the point is
    /// undecided: nothing is drawn through it and the plot is partial.
    /// Within a wide enclosure the point value stands (all that's known:
    /// 10¹⁷·(0.1 + 0.2 − 0.3) is enclosed in [−11.1, 5.6]).
    #[test]
    fn vertices_stay_within_the_point_enclosure() {
        let v = vp();
        let decimals = "10^17*(0.1+0.2-0.3)+x";
        let (lines, partial) = sample_with("x+1", "x", &v);
        assert!(!lines.is_empty() && !partial);
        assert!(lines.iter().flatten().all(|q| q.y == q.x), "{lines:?}");

        let (lines, partial) = sample_with("x+20", decimals, &v);
        assert!(lines.is_empty() && partial, "{lines:?}");

        let (lines, partial) = sample_with("x+1", decimals, &v);
        assert!(!lines.is_empty() && !partial);
        assert!(
            lines.iter().flatten().all(|q| q.y == q.x + 1.0),
            "{lines:?}"
        );
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
