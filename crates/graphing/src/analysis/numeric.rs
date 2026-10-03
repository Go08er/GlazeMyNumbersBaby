//! Numeric building blocks for function analysis: root finding, limits,
//! divergence tests and sample grids.

/// Brent's method on a bracket with `fa`, `fb` of opposite signs (or one of
/// them zero). Converges to a point where the sign changes, which for a
/// continuous function is a root.
pub(crate) fn brent(
    f: &mut dyn FnMut(f64) -> f64,
    mut a: f64,
    mut fa: f64,
    mut b: f64,
    mut fb: f64,
) -> f64 {
    if fa == 0.0 {
        return a;
    }
    if fb == 0.0 {
        return b;
    }
    let mut c = a;
    let mut fc = fa;
    let mut d = b - a;
    let mut e = d;
    for _ in 0..200 {
        if (fb > 0.0) == (fc > 0.0) {
            c = a;
            fc = fa;
            d = b - a;
            e = d;
        }
        if fc.abs() < fb.abs() {
            a = b;
            b = c;
            c = a;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let tol = 2.0 * f64::EPSILON * b.abs() + 1e-300;
        let m = 0.5 * (c - b);
        if m.abs() <= tol || fb == 0.0 {
            return b;
        }
        if e.abs() >= tol && fa.abs() > fb.abs() {
            let s = fb / fa;
            let (mut p, mut q);
            if a == c {
                p = 2.0 * m * s;
                q = 1.0 - s;
            } else {
                let qq = fa / fc;
                let r = fb / fc;
                p = s * (2.0 * m * qq * (qq - r) - (b - a) * (r - 1.0));
                q = (qq - 1.0) * (r - 1.0) * (s - 1.0);
            }
            if p > 0.0 {
                q = -q;
            } else {
                p = -p;
            }
            if 2.0 * p < (3.0 * m * q - (tol * q).abs()).min((e * q).abs()) {
                e = d;
                d = p / q;
            } else {
                d = m;
                e = d;
            }
        } else {
            d = m;
            e = d;
        }
        a = b;
        fa = fb;
        b += if d.abs() > tol { d } else { tol.copysign(m) };
        fb = f(b);
        if fb.is_nan() {
            // Fall back to bisection of the remaining bracket.
            return bisect_sign(f, a, fa, c, fc);
        }
    }
    b
}

/// Plain bisection on the sign (robust for discontinuous functions).
pub(crate) fn bisect_sign(
    f: &mut dyn FnMut(f64) -> f64,
    mut a: f64,
    mut fa: f64,
    mut b: f64,
    _fb: f64,
) -> f64 {
    for _ in 0..200 {
        let m = 0.5 * (a + b);
        if m == a || m == b {
            break;
        }
        let fm = f(m);
        if fm == 0.0 {
            return m;
        }
        if fm.is_nan() {
            b = m;
            continue;
        }
        if (fm > 0.0) == (fa > 0.0) {
            a = m;
            fa = fm;
        } else {
            b = m;
        }
    }
    0.5 * (a + b)
}

/// Bisects between a point where `f` is finite and one where it is not;
/// returns the finite-side point closest to the transition.
pub(crate) fn bisect_finite(f: &mut dyn FnMut(f64) -> f64, mut good: f64, mut bad: f64) -> f64 {
    for _ in 0..200 {
        let m = 0.5 * (good + bad);
        if m == good || m == bad {
            break;
        }
        if f(m).is_finite() {
            good = m;
        } else {
            bad = m;
        }
    }
    good
}

/// Result of examining a sequence of values that should converge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SeqLimit {
    Converges(f64),
    PosInf,
    NegInf,
    Unknown,
}

/// Limit of `v_k` (values at geometrically spaced arguments, e.g. f(10^k)).
/// Divergence: the tail grows monotonically without the increments
/// shrinking (catches logarithmic growth) or overflows. Convergence: the
/// increments shrink steadily down to a tiny value; the limit is refined
/// with Aitken's Δ² extrapolation.
pub(crate) fn sequence_limit(v: &[f64]) -> SeqLimit {
    // Overflow to ±∞ after finite growth.
    let first_nonfinite = v.iter().position(|x| !x.is_finite());
    let fin: &[f64] = match first_nonfinite {
        Some(i) => {
            if v[i].is_infinite()
                && i >= 1
                && v[i..]
                    .iter()
                    .all(|x| x.is_infinite() && x.signum() == v[i].signum())
            {
                let tail = &v[..i];
                let growing =
                    tail.len() < 2 || tail[tail.len() - 1].abs() > tail[tail.len() - 2].abs();
                if growing && tail[tail.len() - 1].signum() == v[i].signum() {
                    return if v[i] > 0.0 {
                        SeqLimit::PosInf
                    } else {
                        SeqLimit::NegInf
                    };
                }
            }
            if i < 4 {
                return SeqLimit::Unknown;
            }
            &v[..i]
        }
        None => v,
    };
    let n = fin.len();
    if n < 4 {
        return SeqLimit::Unknown;
    }
    // Divergence test on the last 6 values.
    let tail = &fin[n.saturating_sub(6)..];
    let same_sign = tail
        .iter()
        .all(|x| x.signum() == tail[0].signum() && *x != 0.0);
    let growing = tail.windows(2).all(|w| w[1].abs() > w[0].abs());
    if same_sign && growing {
        let incs: Vec<f64> = tail.windows(2).map(|w| w[1].abs() - w[0].abs()).collect();
        let not_shrinking = incs.windows(2).all(|w| w[1] >= 0.5 * w[0]);
        if not_shrinking {
            return if tail[0] > 0.0 {
                SeqLimit::PosInf
            } else {
                SeqLimit::NegInf
            };
        }
    }
    // Steady growth through zero (ln x − 30 at x = 10^k): steps of one
    // sign that don't shrink (slow convergence like 1/ln x shrinks them
    // faster than this), ending on the far side of 0 and moving away.
    let steps: Vec<f64> = tail.windows(2).map(|w| w[1] - w[0]).collect();
    let up = steps.iter().all(|&d| d > 0.0);
    let down = steps.iter().all(|&d| d < 0.0);
    let steady = steps.windows(2).all(|w| w[1].abs() >= 0.9 * w[0].abs());
    let (last, before) = (tail[tail.len() - 1], tail[tail.len() - 2]);
    // Steps that don't shrink at all (ln x − 40 moves by ln 10 per decade
    // on either side of 0) are unbounded growth wherever they are.
    let constant = steps
        .windows(2)
        .all(|w| (w[1].abs() - w[0].abs()).abs() <= 1e-3 * w[0].abs());
    if (up || down) && constant && steps[0] != 0.0 {
        return if up {
            SeqLimit::PosInf
        } else {
            SeqLimit::NegInf
        };
    }
    if steady && (up && last > 0.0 || down && last < 0.0) && last.abs() > before.abs() {
        return if up {
            SeqLimit::PosInf
        } else {
            SeqLimit::NegInf
        };
    }
    // A geometric approach, steps shrinking by one constant ratio (every
    // power tail: x^−0.1 shrinks by 10^−0.1 per decade), has its limit given
    // exactly by Aitken's Δ² even while the steps are still large.
    let signed: Vec<f64> = tail.windows(2).map(|w| w[1] - w[0]).collect();
    let ratios: Vec<f64> = signed.windows(2).map(|w| w[1] / w[0]).collect();
    let r = ratios[ratios.len() - 1];
    if ratios.len() >= 3 && r > 0.0 && r < 0.99 && ratios.iter().all(|q| (q - r).abs() <= 1e-6 * r)
    {
        let (a, b, c) = (
            tail[tail.len() - 3],
            tail[tail.len() - 2],
            tail[tail.len() - 1],
        );
        let denom = (c - b) - (b - a);
        if denom != 0.0 {
            let acc = c - (c - b) * (c - b) / denom;
            if acc.is_finite() {
                return SeqLimit::Converges(acc);
            }
        }
    }
    // Convergence: pick the index with the smallest increment, requiring the
    // increments to shrink for a few steps before it.
    let d: Vec<f64> = fin.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
    // Prefer indices reached by a clean geometric decrease (rounding noise
    // breaks the pattern), falling back to plain monotone decrease.
    let pick = |ratio: f64| {
        let mut best: Option<usize> = None;
        for k in 2..d.len() {
            if d[k] <= ratio * d[k - 1] && d[k - 1] <= ratio * d[k - 2] {
                match best {
                    Some(b) if d[b] <= d[k] => {}
                    _ => best = Some(k),
                }
            }
        }
        best
    };
    let best = pick(0.5).or_else(|| pick(1.0));
    let Some(k) = best else {
        return SeqLimit::Unknown;
    };
    let l = fin[k + 1];
    let scale = l.abs().max(1.0);
    if d[k] > 1e-6 * scale {
        return SeqLimit::Unknown;
    }
    // Aitken Δ² on (v_{k-1}, v_k, v_{k+1}).
    let (a, b, c) = (fin[k - 1], fin[k], fin[k + 1]);
    let denom = (c - b) - (b - a);
    let mut lim = l;
    if denom != 0.0 {
        let acc = c - (c - b) * (c - b) / denom;
        if acc.is_finite() && (acc - l).abs() <= 10.0 * d[k] + 1e-300 {
            lim = acc;
        }
    }
    SeqLimit::Converges(lim)
}

/// Limit of f(x) as x → +∞ (`sign > 0`) or −∞.
pub(crate) fn limit_at_infinity(f: &mut dyn FnMut(f64) -> f64, sign: f64) -> SeqLimit {
    limit_at_infinity_beyond(f, sign, 0.0)
}

/// Limit at ±∞ of a function defined on (or a piece starting at) `from`:
/// sampled at powers of 10 well past it, skipping any where f isn't yet
/// defined (log(x − 5·10⁶) at x = 10).
pub(crate) fn limit_at_infinity_beyond(
    f: &mut dyn FnMut(f64) -> f64,
    sign: f64,
    from: f64,
) -> SeqLimit {
    let start = if from.is_finite() {
        2.0 * from.abs()
    } else {
        0.0
    };
    let first = (start.max(10.0).log10().ceil() as i32).max(1);
    let v: Vec<f64> = (first..first + 17)
        .map(|k| f(sign * 10f64.powi(k)))
        .collect();
    let defined = v.iter().position(|x| !x.is_nan()).unwrap_or(v.len());
    sequence_limit(&v[defined..])
}

/// Whether |f| diverges approaching `c` from the side `side` (±1).
/// Returns the sign of the divergence.
pub(crate) fn diverges_near(f: &mut dyn FnMut(f64) -> f64, c: f64, side: f64) -> Option<f64> {
    let base = c.abs().max(1.0);
    let kmax = if c == 0.0 { 150 } else { 13 };
    let step = if c == 0.0 { 5 } else { 1 };
    let mut v = Vec::new();
    let mut k = 1;
    while k <= kmax {
        let x = c + side * base * 10f64.powi(-k);
        if x == c {
            break;
        }
        v.push(f(x));
        k += step;
    }
    match sequence_limit(&v) {
        SeqLimit::PosInf => Some(1.0),
        SeqLimit::NegInf => Some(-1.0),
        _ => None,
    }
}

/// One-sided limit of f at `c` (approximate): converged value, ±∞, or the
/// value very close to `c` if no clear convergence is seen.
pub(crate) fn one_sided_limit(f: &mut dyn FnMut(f64) -> f64, c: f64, side: f64) -> f64 {
    if let Some(s) = diverges_near(f, c, side) {
        return s * f64::INFINITY;
    }
    let base = c.abs().max(1.0);
    let kmax = if c == 0.0 { 40 } else { 12 };
    let v: Vec<f64> = (1..=kmax)
        .map(|k| f(c + side * base * 10f64.powi(-k)))
        .collect();
    match sequence_limit(&v) {
        SeqLimit::Converges(l) => l,
        _ => v
            .iter()
            .rev()
            .copied()
            .find(|x| x.is_finite())
            .unwrap_or(f64::NAN),
    }
}

/// Symmetric grid on [−r, r], dense near 0 (x = sinh(u), u uniform),
/// containing 0 and exactly mirrored values.
pub(crate) fn sinh_grid(r: f64, n_half: usize) -> Vec<f64> {
    let umax = r.asinh();
    let pos: Vec<f64> = (1..=n_half)
        .map(|i| (umax * i as f64 / n_half as f64).sinh())
        .collect();
    let mut xs = Vec::with_capacity(2 * n_half + 1);
    xs.extend(pos.iter().rev().map(|x| -x));
    xs.push(0.0);
    xs.extend(pos);
    xs
}

/// Uniform grid on [a, b] with n intervals.
pub(crate) fn uniform_grid(a: f64, b: f64, n: usize) -> Vec<f64> {
    (0..=n)
        .map(|i| {
            if i == n {
                b
            } else {
                a + (b - a) * i as f64 / n as f64
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brent_finds_roots() {
        let mut f = |x: f64| x * x - 2.0;
        let r = brent(&mut f, 0.0, -2.0, 2.0, 2.0);
        assert!((r - 2f64.sqrt()).abs() < 1e-15);
        let mut g = |x: f64| x.cos();
        let r = brent(&mut g, 1.0, 1f64.cos(), 2.0, 2f64.cos());
        assert!((r - std::f64::consts::FRAC_PI_2).abs() < 1e-15);
    }

    #[test]
    fn limits() {
        match limit_at_infinity(&mut |x: f64| 1.0 / x, 1.0) {
            SeqLimit::Converges(l) => assert!(l.abs() < 1e-15),
            other => panic!("{other:?}"),
        }
        match limit_at_infinity(&mut |x: f64| x.atan(), 1.0) {
            SeqLimit::Converges(l) => assert!((l - std::f64::consts::FRAC_PI_2).abs() < 1e-12),
            other => panic!("{other:?}"),
        }
        // Rounding of 1 + 1/x biases this classic limit by ~1e-7.
        match limit_at_infinity(&mut |x: f64| (1.0 + 1.0 / x).powf(x), 1.0) {
            SeqLimit::Converges(l) => assert!((l - std::f64::consts::E).abs() < 1e-6, "{l}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            limit_at_infinity(&mut |x: f64| x.exp(), 1.0),
            SeqLimit::PosInf
        );
        assert!(
            matches!(limit_at_infinity(&mut |x: f64| x.exp(), -1.0), SeqLimit::Converges(l) if l.abs() < 1e-15)
        );
        assert_eq!(
            limit_at_infinity(&mut |x: f64| x.ln(), 1.0),
            SeqLimit::PosInf
        );
        assert_eq!(
            limit_at_infinity(&mut |x: f64| -x * x, 1.0),
            SeqLimit::NegInf
        );
        assert_eq!(
            limit_at_infinity(&mut |x: f64| x.sin(), 1.0),
            SeqLimit::Unknown
        );
        assert_eq!(
            limit_at_infinity(&mut |x: f64| x * x.sin(), 1.0),
            SeqLimit::Unknown
        );
    }

    #[test]
    fn divergence_near_points() {
        assert_eq!(diverges_near(&mut |x: f64| 1.0 / x, 0.0, 1.0), Some(1.0));
        assert_eq!(diverges_near(&mut |x: f64| 1.0 / x, 0.0, -1.0), Some(-1.0));
        assert_eq!(diverges_near(&mut |x: f64| x.ln(), 0.0, 1.0), Some(-1.0));
        assert_eq!(
            diverges_near(&mut |x: f64| (x - 1.0).ln(), 1.0, 1.0),
            Some(-1.0)
        );
        let c = std::f64::consts::FRAC_PI_2;
        assert_eq!(diverges_near(&mut |x: f64| x.tan(), c, -1.0), Some(1.0));
        assert_eq!(diverges_near(&mut |x: f64| x.sin() / x, 0.0, 1.0), None);
        assert_eq!(
            diverges_near(&mut |x: f64| (1.0 / x).exp(), 0.0, 1.0),
            Some(1.0)
        );
        assert_eq!(
            diverges_near(&mut |x: f64| (1.0 / x).exp(), 0.0, -1.0),
            None
        );
        assert_eq!(
            diverges_near(&mut |x: f64| (x * x - 1.0) / (x - 1.0), 1.0, 1.0),
            None
        );
        assert!(
            (one_sided_limit(&mut |x: f64| (x * x - 1.0) / (x - 1.0), 1.0, 1.0) - 2.0).abs() < 1e-9
        );
    }

    #[test]
    fn grids() {
        let g = sinh_grid(1e6, 100);
        assert_eq!(g.len(), 201);
        assert_eq!(g[100], 0.0);
        assert_eq!(g[0], -g[200]);
        assert!((g[200] - 1e6).abs() < 1e-3);
    }
}
