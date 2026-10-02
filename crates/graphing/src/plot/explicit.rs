//! Adaptive sampling of explicit curves with discontinuity detection.

use super::{Cancel, PlotOptions, Polyline, axis_point, signed_area};
use crate::compile::{Input, Program};
use crate::equation::Axis;
use crate::viewport::Viewport;

/// A jump between neighbouring leaf samples larger than this (pixels) is
/// examined to decide between "steep but continuous" and "discontinuous".
const JUMP_PX: f64 = 2.0;

pub(crate) struct ExplicitSampler<'a> {
    f: &'a Program,
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
        self.exhausted
    }

    /// Raw continuous pieces `(t, d)` (unclipped).
    #[cfg(test)]
    pub(crate) fn pieces(&self) -> &[Vec<(f64, f64)>] {
        &self.pieces
    }

    pub(crate) fn run(&mut self) {
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
            let lerp = |s: f64| axis_point(self.axis, pt + s * (t - pt), pd + s * (d - pd));
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
