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
        // Down to a float or two: a root at 10⁹ is found to 10⁻⁷, not 4·10⁻⁷.
        let tol = 0.5 * f64::EPSILON * b.abs() + 1e-300;
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

/// Bisects between a point where `f` is defined and one where it is NaN;
/// returns the defined-side point closest to the transition. ±∞ counts as
/// defined: it is a value that overflowed.
pub(crate) fn bisect_defined(f: &mut dyn FnMut(f64) -> f64, mut good: f64, mut bad: f64) -> f64 {
    for _ in 0..200 {
        let m = 0.5 * (good + bad);
        if m == good || m == bad {
            break;
        }
        if f(m).is_nan() {
            bad = m;
        } else {
            good = m;
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
pub(crate) fn sequence_limit(v: &[f64]) -> SeqLimit {
    sequence_limit_err(v).0
}

/// Aitken's Δ² extrapolation of three successive values.
fn aitken(a: f64, b: f64, c: f64) -> f64 {
    let denom = (c - b) - (b - a);
    if denom == 0.0 {
        c
    } else {
        c - (c - b) * (c - b) / denom
    }
}

/// [`sequence_limit`] with an estimate of how far a finite limit may be
/// from the true one (∞ when there's none).
///
/// - A geometric approach, steps of one sign shrinking by one ratio r < 1
///   (every power tail x^p + L with p < 0 shrinks by 10^p per decade, however
///   close to 1 that is), has its limit given by Aitken's Δ². The ratios must
///   agree to a small part of their distance from 1, so a slow power tail is
///   never taken for growth, nor growth for one.
/// - Unbounded growth: steps of one sign that don't shrink at all (ln x moves
///   by ln 10 per decade; x^p with p > 0 and e^x by growing steps), or
///   overflow after growing.
/// - Otherwise convergence only when the steps shrink steadily to a tiny
///   value. Anything else (1/ln x and ln ln x creep, their steps shrinking
///   like 1/k) is unknown rather than guessed.
pub(crate) fn sequence_limit_err(v: &[f64]) -> (SeqLimit, f64) {
    let noise: Vec<f64> = v.iter().map(|x| 64.0 * f64::EPSILON * x.abs()).collect();
    sequence_limit_noisy(v, &noise)
}

/// f at each x, with how far each value may be off: f's change across the
/// floats next to x (the sample point is itself rounded: 1 + 10⁻¹³ is off by
/// 0.2% of 10⁻¹³) plus the rounding of the value.
pub(crate) fn sample(f: &mut dyn FnMut(f64) -> f64, xs: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut v = Vec::with_capacity(xs.len());
    let mut noise = Vec::with_capacity(xs.len());
    for &x in xs {
        let y = f(x);
        let (a, b) = (f(x.next_down()), f(x.next_up()));
        let mut n = 64.0 * f64::EPSILON * y.abs();
        for z in [a, b] {
            if z.is_finite() && y.is_finite() {
                n += (z - y).abs();
            }
        }
        v.push(y);
        noise.push(n);
    }
    (v, noise)
}

/// [`sequence_limit_err`] with each value's own uncertainty `noise_k`.
pub(crate) fn sequence_limit_noisy(v: &[f64], noise_in: &[f64]) -> (SeqLimit, f64) {
    const NONE: f64 = f64::INFINITY;
    // Undefined or infinite values before any finite one are the far
    // samples (another pole on the way to this one, 10⁶ away from a pole at
    // 10⁷): the limit is about the finite run that follows.
    let start = v.iter().position(|x| x.is_finite()).unwrap_or(v.len());
    if start > 0 && start < v.len() {
        let rest = noise_in.get(start..).unwrap_or(&[]);
        return sequence_limit_noisy(&v[start..], rest);
    }
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
                    let l = if v[i] > 0.0 {
                        SeqLimit::PosInf
                    } else {
                        SeqLimit::NegInf
                    };
                    return (l, 0.0);
                }
            }
            // An undefined value on the way (another pole crossed before
            // 10⁶ on the way out from 1/(√(x/10⁶) − 1)): the limit is about
            // the last finite run, if there is one beyond the trailing
            // undefined values (the point itself, reached by rounding).
            let mut end = v.len();
            while end > 0 && v[end - 1].is_nan() {
                end -= 1;
            }
            let from = v[..end]
                .iter()
                .rposition(|x| !x.is_finite())
                .map_or(0, |p| p + 1);
            if from > 0 && end - from >= 4 {
                let rest = noise_in.get(from..end).unwrap_or(&[]);
                return sequence_limit_noisy(&v[from..end], rest);
            }
            if i < 4 {
                return (SeqLimit::Unknown, NONE);
            }
            &v[..i]
        }
        None => v,
    };
    let n = fin.len();
    if n < 4 {
        return (SeqLimit::Unknown, NONE);
    }
    let t0 = n.saturating_sub(6);
    let tail = &fin[t0..];
    let tnoise: Vec<f64> = (t0..n)
        .map(|i| noise_in.get(i).copied().unwrap_or(0.0))
        .collect();
    let noise = tnoise.iter().fold(0.0f64, |m, &x| m.max(x));
    let steps: Vec<f64> = tail.windows(2).map(|w| w[1] - w[0]).collect();
    // How far each step may be off.
    let step_noise: Vec<f64> = tnoise.windows(2).map(|w| w[0] + w[1]).collect();
    let one_way = steps.iter().all(|&d| d > 0.0) || steps.iter().all(|&d| d < 0.0);
    if one_way {
        let ratios: Vec<f64> = steps.windows(2).map(|w| w[1] / w[0]).collect();
        // How far each ratio may be off through the steps' uncertainty.
        let slack: Vec<f64> = (0..ratios.len())
            .map(|i| {
                let (a, b) = (steps[i].abs(), steps[i + 1].abs());
                (step_noise[i + 1] + ratios[i].abs() * step_noise[i]) / a.min(b).max(1e-300)
                    + 1e-300
            })
            .collect();
        let r = ratios[ratios.len() - 1];
        let spread = ratios.iter().fold(0.0f64, |m, q| m.max((q - r).abs()));
        let fuzz = slack.iter().fold(0.0f64, |m, &q| m.max(q));
        // 1. Geometric. The slack is a worst case; ratios that agree among
        // themselves far better than it (x^−0.0001 + 10⁶: within 10⁻⁶ while
        // 1 − r = 2.3·10⁻⁴) show the noise is smaller, and that agreement
        // is the evidence.
        let consistent = spread <= 0.05 * (1.0 - r);
        if r > 0.0
            && 1.0 - r > 1e-9
            && (1.0 - r > 4.0 * fuzz || consistent)
            && ratios
                .iter()
                .zip(&slack)
                .all(|(q, sl)| (q - r).abs() <= 1e-3 * (1.0 - r) + sl)
        {
            let k = tail.len();
            let acc1 = aitken(tail[k - 3], tail[k - 2], tail[k - 1]);
            let acc2 = aitken(tail[k - 4], tail[k - 3], tail[k - 2]);
            if acc1.is_finite() && acc2.is_finite() {
                let rest = (tail[k - 1] - acc1).abs();
                let jitter = if 1.0 - r > 4.0 * fuzz {
                    spread + fuzz
                } else {
                    spread
                };
                let err = (acc1 - acc2).abs() + jitter / (1.0 - r) * rest + noise / (1.0 - r);
                return (SeqLimit::Converges(acc1), err);
            }
        }
        // 2. Steps that don't shrink, or settle on a size that isn't 0
        // (log₁₀(x − 5·10⁶) moves by ever closer to 1 per decade): unbounded,
        // in their direction. A power tail's steps head for 0 instead.
        let k = steps.len();
        let settles = aitken(steps[k - 3], steps[k - 2], steps[k - 1]);
        // Aitken on the steps means something only if their second
        // difference stands out of their noise.
        let second = ((steps[k - 1] - steps[k - 2]) - (steps[k - 2] - steps[k - 3])).abs();
        let resolved =
            second > 4.0 * (step_noise[k - 1] + 2.0 * step_noise[k - 2] + step_noise[k - 3]);
        let settled = settles.is_finite()
            && (settles > 0.0) == (steps[0] > 0.0)
            && (settles - steps[k - 1]).abs() <= 1e-2 * steps[k - 1].abs()
            && (resolved
                || (steps[k - 1] - steps[k - 2]).abs() <= step_noise[k - 1] + step_noise[k - 2]);
        // Steps measured shrinking, if only within their noise (x^−0.0001 +
        // 10⁶ shrinks by 0.023% per decade under rounding of 10⁶): neither
        // growth nor a limit can be told, so unknown, not ±∞.
        let shrinking = ratios.iter().all(|q| *q < 1.0 - 1e-9)
            && steps
                .iter()
                .zip(&step_noise)
                .all(|(d, n)| d.abs() > 4.0 * n);
        if !settled && shrinking {
            return (SeqLimit::Unknown, NONE);
        }
        if settled
            || ratios
                .iter()
                .zip(&slack)
                .all(|(q, sl)| *q >= 1.0 - 1e-9 - sl)
                && ratios.iter().all(|q| *q > 0.0)
        {
            let l = if steps[0] > 0.0 {
                SeqLimit::PosInf
            } else {
                SeqLimit::NegInf
            };
            return (l, 0.0);
        }
    }
    // 3. Convergence: pick the index with the smallest increment, requiring
    // the increments to shrink for a few steps before it.
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
        return (SeqLimit::Unknown, NONE);
    };
    let l = fin[k + 1];
    // Settled against how far the sequence has moved, not against its size
    // or 1: x^−0.0001 − 10⁶ still moves by 2·10⁻⁴ per decade, 10⁻¹²/ln x by
    // 10⁻¹⁵, and neither has arrived.
    // Or within the values' own noise: f(10⁸) − 10⁸ is rounding beyond
    // 10⁻⁸ when f = 1/acsch(x) ≈ x + 1/(6x).
    let spread = fin.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - fin.iter().copied().fold(f64::INFINITY, f64::min);
    let nk = noise_in.get(k + 1).copied().unwrap_or(0.0);
    // (Noise that is all there is, as in sin(x²) far out, settles nothing.)
    if d[k] > 1e-6 * spread && (d[k] > 4.0 * nk || nk > 1e-3 * spread) {
        return (SeqLimit::Unknown, NONE);
    }
    // Aitken Δ² on (v_{k-1}, v_k, v_{k+1}).
    let (a, b, c) = (fin[k - 1], fin[k], fin[k + 1]);
    let mut lim = l;
    let acc = aitken(a, b, c);
    if acc.is_finite() && (acc - l).abs() <= 10.0 * d[k] + 1e-300 {
        lim = acc;
    }
    let all = fin.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    (
        SeqLimit::Converges(lim),
        10.0 * d[k] + 64.0 * f64::EPSILON * all + 4.0 * nk,
    )
}

/// Limit of f(x) as x → +∞ (`sign > 0`) or −∞.
#[cfg(test)]
fn limit_at_infinity(f: &mut dyn FnMut(f64) -> f64, sign: f64) -> SeqLimit {
    limit_at_infinity_beyond_err(f, sign, 0.0).0
}

/// Limit at ±∞ of a function defined on (or a piece starting at) `from`,
/// with an error estimate for a finite limit (see [`sequence_limit_err`]):
/// sampled at powers of 10 well past it, skipping any where f isn't yet
/// defined (log(x − 5·10⁶) at x = 10).
pub(crate) fn limit_at_infinity_beyond_err(
    f: &mut dyn FnMut(f64) -> f64,
    sign: f64,
    from: f64,
) -> (SeqLimit, f64) {
    let start = if from.is_finite() {
        2.0 * from.abs()
    } else {
        0.0
    };
    let first = (start.max(10.0).log10().ceil() as i32).max(1);
    let xs: Vec<f64> = (first..first + 17).map(|k| sign * 10f64.powi(k)).collect();
    let (v, noise) = sample(f, &xs);
    let defined = v.iter().position(|x| !x.is_nan()).unwrap_or(v.len());
    sequence_limit_noisy(&v[defined..], &noise[defined..])
}

/// Whether |f| diverges approaching `c` from the side `side` (±1).
/// Returns the sign of the divergence.
pub(crate) fn diverges_near(f: &mut dyn FnMut(f64) -> f64, c: f64, side: f64) -> Option<f64> {
    diverges_within(f, c, side, f64::INFINITY)
}

/// [`diverges_near`], sampling no further than `reach` from `c` (the width
/// of the piece it ends: (x − 1000)/ln(x − 1000) has a pole at 1001).
pub(crate) fn diverges_within(
    f: &mut dyn FnMut(f64) -> f64,
    c: f64,
    side: f64,
    reach: f64,
) -> Option<f64> {
    let base = c.abs().max(1.0).min(reach);
    let kmax = if c == 0.0 { 150 } else { 13 };
    let step = if c == 0.0 { 5 } else { 1 };
    let mut xs = Vec::new();
    let mut k = 1;
    while k <= kmax {
        let x = c + side * base * 10f64.powi(-k);
        if x == c {
            break;
        }
        xs.push(x);
        k += step;
    }
    let (v, noise) = sample(f, &xs);
    match sequence_limit_noisy(&v, &noise).0 {
        SeqLimit::PosInf => Some(1.0),
        SeqLimit::NegInf => Some(-1.0),
        _ => overflows_into(f, c, side, base, &v),
    }
}

/// Every sample towards `c` overflowed to the same ±∞ (1/x^400 at 0 is
/// beyond 10³⁰⁸ from x ≈ 0.15 in): diverging if, halving the distance
/// from `base`, f grows steadily into that overflow.
fn overflows_into(
    f: &mut dyn FnMut(f64) -> f64,
    c: f64,
    side: f64,
    base: f64,
    v: &[f64],
) -> Option<f64> {
    let sign = v.first()?.signum();
    if !v.iter().all(|y| y.is_infinite() && y.signum() == sign) {
        return None;
    }
    let mut run: Vec<f64> = Vec::new();
    let mut d = base;
    for _ in 0..1100 {
        let y = f(c + side * d);
        if y.is_infinite() && y.signum() == sign {
            let growing = run.len() >= 3
                && run.iter().all(|r| r.signum() == sign)
                && run.windows(2).all(|w| w[1].abs() > w[0].abs());
            return growing.then_some(sign);
        }
        if !y.is_finite() {
            return None;
        }
        run.push(y);
        d *= 0.5;
        if c + side * d == c {
            return None;
        }
    }
    None
}

/// One-sided limit of f at `c` from the side `side` (±1), for the range:
/// `Converges` with an error estimate, ±∞, or `Unknown` (with whether the
/// approach is monotone, i.e. creeps towards a limit the samples can't pin
/// down, such as 1/ln(2/x) at 0, rather than oscillating).
pub(crate) fn one_sided_limit_err(
    f: &mut dyn FnMut(f64) -> f64,
    c: f64,
    side: f64,
    reach: f64,
) -> (SeqLimit, f64, bool) {
    if let Some(s) = diverges_within(f, c, side, reach) {
        let l = if s > 0.0 {
            SeqLimit::PosInf
        } else {
            SeqLimit::NegInf
        };
        return (l, 0.0, false);
    }
    let base = c.abs().max(1.0).min(reach);
    let kmax = if c == 0.0 { 40 } else { 12 };
    let xs: Vec<f64> = (1..=kmax)
        .map(|k| c + side * base * 10f64.powi(-k))
        .take_while(|&x| x != c)
        .collect();
    let (v, noise) = sample(f, &xs);
    let (l, err) = sequence_limit_noisy(&v, &noise);
    let fin: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    let monotone = fin.len() > 2
        && (fin.windows(2).all(|w| w[1] >= w[0]) || fin.windows(2).all(|w| w[1] <= w[0]));
    (l, err, monotone)
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
