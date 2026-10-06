//! Elementary functions on decorated intervals.
//!
//! Each function encloses the mathematically exact image of its input box,
//! with the app's conventions (`functions.rs`): division and logarithms are
//! undefined at 0, arccot has range (0, π), trig functions take their
//! argument in the current [`TrigUnit`], inverse trig functions return one.
//!
//! Point values come from CORE-MATH (correctly rounded), widened by an ulp
//! each side unless exact; monotone pieces and critical points give the
//! image of an interval. A box that leaves a function's domain is
//! restricted to it and decorated [`Dec::Trv`].

use super::arith::{Interval, mul_down, mul_up, widen};
use super::dec::{Dec, DecInterval};
use crate::functions::TrigUnit;
use core_math as cm;
use std::f64::consts::{FRAC_PI_2, PI};

const INF: f64 = f64::INFINITY;

/// π, enclosed (the double π is below it).
pub fn pi() -> Interval {
    Interval::new(PI, PI.next_up())
}

/// e, enclosed (the double e is below it).
pub fn e() -> Interval {
    Interval::new(std::f64::consts::E, std::f64::consts::E.next_up())
}

/// π/2, enclosed.
fn half_pi() -> Interval {
    Interval::new(FRAC_PI_2, FRAC_PI_2.next_up())
}

// ------------------------------------------------------------- arithmetic

fn ge0(a: &DecInterval) -> bool {
    a.gt0() || (!a.is_empty() && a.lo() >= 0.0)
}

fn le0(a: &DecInterval) -> bool {
    a.lt0() || (!a.is_empty() && a.hi() <= 0.0)
}

pub fn add(a: &DecInterval, b: &DecInterval) -> DecInterval {
    let r = DecInterval::result(a.iv + b.iv, Dec::Com, &[a, b]);
    r.signs(
        (a.gt0() && ge0(b)) || (ge0(a) && b.gt0()),
        (a.lt0() && le0(b)) || (le0(a) && b.lt0()),
    )
}

pub fn neg(a: &DecInterval) -> DecInterval {
    let mut r = DecInterval::result(-a.iv, Dec::Com, &[a]);
    r.pos = a.neg;
    r.neg = a.pos;
    r.normalized()
}

pub fn sub(a: &DecInterval, b: &DecInterval) -> DecInterval {
    add(a, &neg(b))
}

pub fn mul(a: &DecInterval, b: &DecInterval) -> DecInterval {
    let r = DecInterval::result(a.iv * b.iv, Dec::Com, &[a, b]);
    r.signs(
        (a.gt0() && b.gt0()) || (a.lt0() && b.lt0()),
        (a.gt0() && b.lt0()) || (a.lt0() && b.gt0()),
    )
}

pub fn sqr(a: &DecInterval) -> DecInterval {
    let r = DecInterval::result(a.iv.sqr(), Dec::Com, &[a]);
    r.signs(a.ne0(), false)
}

/// a/b, undefined where b = 0.
pub fn div(a: &DecInterval, b: &DecInterval) -> DecInterval {
    let local = if b.ne0() || !b.iv.contains_zero() {
        Dec::Com
    } else {
        Dec::Trv
    };
    let r = DecInterval::result(a.iv / b.iv, local, &[a, b]);
    r.signs(
        (a.gt0() && b.gt0()) || (a.lt0() && b.lt0()),
        (a.gt0() && b.lt0()) || (a.lt0() && b.gt0()),
    )
}

pub fn recip(a: &DecInterval) -> DecInterval {
    div(&DecInterval::point(1.0), a)
}

/// The part of `x` inside `dom`, and whether that left anything out.
fn restrict(x: &DecInterval, dom: Interval) -> (Interval, bool) {
    let r = x.iv.intersect(dom);
    (r, !x.iv.subset(dom))
}

/// A result over the part of `x` in the function's domain: `Trv` if `x`
/// left it, else `local`.
fn on_domain(iv: Interval, left: bool, local: Dec, x: &DecInterval) -> DecInterval {
    DecInterval::result(iv, if left { Dec::Trv } else { local }, &[x])
}

/// Enclosure of a correctly rounded result `r` of a non-exact operation.
fn cr(r: f64) -> Interval {
    widen(r, -INF, INF)
}

/// Monotone increasing `f` (point enclosures `pe`) on `[lo, hi]`, with
/// the limits at infinite ends given.
fn inc(x: Interval, pe: impl Fn(f64) -> Interval, at_neg_inf: f64, at_pos_inf: f64) -> Interval {
    if x.is_empty() {
        return x;
    }
    let lo = if x.lo() == -INF {
        at_neg_inf
    } else {
        pe(x.lo()).lo()
    };
    let hi = if x.hi() == INF {
        at_pos_inf
    } else {
        pe(x.hi()).hi()
    };
    Interval::new(lo, hi)
}

/// Monotone decreasing.
fn dec(x: Interval, pe: impl Fn(f64) -> Interval, at_neg_inf: f64, at_pos_inf: f64) -> Interval {
    if x.is_empty() {
        return x;
    }
    let lo = if x.hi() == INF {
        at_pos_inf
    } else {
        pe(x.hi()).lo()
    };
    let hi = if x.lo() == -INF {
        at_neg_inf
    } else {
        pe(x.lo()).hi()
    };
    Interval::new(lo, hi)
}

// ------------------------------------------------------- exp, logarithms

fn pe_exp(x: f64) -> Interval {
    if x == 0.0 {
        return Interval::point(1.0);
    }
    widen(cm::exp(x), 0.0, INF)
}

pub fn exp(x: &DecInterval) -> DecInterval {
    let r = DecInterval::result(inc(x.iv, pe_exp, 0.0, INF), Dec::Com, &[x]);
    r.signs(true, false)
}

fn pe_ln(x: f64) -> Interval {
    if x == 1.0 {
        return Interval::point(0.0);
    }
    cr(cm::log(x))
}

/// Natural logarithm, undefined for x ≤ 0 (including 0).
pub fn ln(x: &DecInterval) -> DecInterval {
    let pos = Interval::new(0.0, INF);
    let (d, left) = restrict(x, pos);
    let undefined_at_0 = d.lo() == 0.0 && !x.gt0();
    let lo_dec = if left || undefined_at_0 {
        Dec::Trv
    } else {
        Dec::Com
    };
    let iv = if d.is_empty() || (d.hi() == 0.0) {
        Interval::EMPTY
    } else {
        let lo = if d.lo() == 0.0 {
            -INF
        } else {
            pe_ln(d.lo()).lo()
        };
        let hi = if d.hi() == INF {
            INF
        } else {
            pe_ln(d.hi()).hi()
        };
        Interval::new(lo, hi)
    };
    DecInterval::result(iv, lo_dec, &[x])
}

fn pe_log10(x: f64) -> Interval {
    // Exact powers of ten that doubles hold.
    let r = cm::log10(x);
    if r == r.trunc() && (0.0..=22.0).contains(&r) && 10f64.powi(r as i32) == x {
        return Interval::point(r);
    }
    cr(r)
}

/// Base-10 logarithm, undefined for x ≤ 0.
pub fn log10(x: &DecInterval) -> DecInterval {
    let (d, left) = restrict(x, Interval::new(0.0, INF));
    let undefined_at_0 = d.lo() == 0.0 && !x.gt0();
    let local = if left || undefined_at_0 {
        Dec::Trv
    } else {
        Dec::Com
    };
    let iv = if d.is_empty() || d.hi() == 0.0 {
        Interval::EMPTY
    } else {
        let lo = if d.lo() == 0.0 {
            -INF
        } else {
            pe_log10(d.lo()).lo()
        };
        let hi = if d.hi() == INF {
            INF
        } else {
            pe_log10(d.hi()).hi()
        };
        Interval::new(lo, hi)
    };
    DecInterval::result(iv, local, &[x])
}

fn pe_log1p(x: f64) -> Interval {
    if x == 0.0 {
        return Interval::point(0.0);
    }
    cr(cm::log1p(x))
}

/// ln(1 + x) for x > −1.
pub fn log1p(x: &DecInterval) -> DecInterval {
    let (d, left) = restrict(x, Interval::new(-1.0, INF));
    let at_m1 = d.lo() == -1.0;
    let iv = if d.is_empty() || d.hi() == -1.0 {
        Interval::EMPTY
    } else {
        let lo = if at_m1 { -INF } else { pe_log1p(d.lo()).lo() };
        let hi = if d.hi() == INF {
            INF
        } else {
            pe_log1p(d.hi()).hi()
        };
        Interval::new(lo, hi)
    };
    DecInterval::result(iv, if left || at_m1 { Dec::Trv } else { Dec::Com }, &[x])
}

/// log_b(x) = ln x / ln b, undefined unless b > 0, b ≠ 1, x > 0.
pub fn log_base(b: &DecInterval, x: &DecInterval) -> DecInterval {
    div(&ln(x), &ln(b))
}

// -------------------------------------------------------- roots, powers

/// √x, undefined for x < 0.
pub fn sqrt(x: &DecInterval) -> DecInterval {
    let (d, left) = restrict(x, Interval::new(0.0, INF));
    on_domain(d.sqrt(), left, Dec::Com, x).signs(x.gt0(), false)
}

fn pe_cbrt(x: f64) -> Interval {
    let r = cm::cbrt(x);
    // Exact for perfect cubes (checked on magnitudes: directed rounding
    // through a negative factor would turn a bound around).
    let (a, ax) = (r.abs(), x.abs());
    let c_lo = mul_down(mul_down(a, a), a);
    let c_hi = mul_up(mul_up(a, a), a);
    if r.is_finite() && c_lo == ax && c_hi == ax {
        return Interval::point(r);
    }
    cr(r)
}

/// Real cube root.
pub fn cbrt(x: &DecInterval) -> DecInterval {
    let r = DecInterval::result(inc(x.iv, pe_cbrt, -INF, INF), Dec::Com, &[x]);
    r.signs(x.gt0(), x.lt0())
}

/// x^n for a double x and an integer n, correctly rounded and widened.
fn pe_pown(x: f64, n: i32) -> Interval {
    match n {
        0 => return Interval::point(1.0),
        1 => return Interval::point(x),
        2 => return Interval::point(x).sqr(),
        _ => {}
    }
    if x == 0.0 {
        return if n > 0 {
            Interval::point(0.0)
        } else {
            Interval::EMPTY
        };
    }
    if x == 1.0 || (x == -1.0) {
        return Interval::point(if x < 0.0 && n % 2 != 0 { -1.0 } else { 1.0 });
    }
    cr(cm::pow(x, n as f64))
}

/// xⁿ for an integer n: 0 to a power ≤ 0 undefined (0⁰ included, as on
/// the TI-84 Plus CE and in the graphing evaluator, `functions::pow_int`).
pub fn powi(x: &DecInterval, n: i32) -> DecInterval {
    if n == 0 {
        let iv = x.iv;
        if iv.is_empty() || (iv.lo() == 0.0 && iv.hi() == 0.0) {
            return DecInterval::result(Interval::EMPTY, Dec::Trv, &[x]);
        }
        // 1 wherever x ≠ 0; a box that may hold 0 isn't defined throughout.
        let local = if iv.contains_zero() && !x.ne0() {
            Dec::Trv
        } else {
            Dec::Com
        };
        return DecInterval::result(Interval::point(1.0), local, &[x]).signs(true, false);
    }
    if n == 1 {
        return *x;
    }
    if n == 2 {
        return sqr(x);
    }
    let even = n % 2 == 0;
    let iv = x.iv;
    if iv.is_empty() {
        return DecInterval::result(iv, Dec::Trv, &[x]);
    }
    if n > 0 {
        let r = if even {
            if iv.contains_zero() {
                let (_, ma) = iv.mig_mag();
                Interval::new(0.0, if ma == INF { INF } else { pe_pown(ma, n).hi() })
            } else if iv.lo() > 0.0 {
                inc(iv, |v| pe_pown(v, n), 0.0, INF)
            } else {
                dec(iv, |v| pe_pown(v, n), INF, 0.0)
            }
        } else {
            inc(iv, |v| pe_pown(v, n), -INF, INF)
        };
        let res = DecInterval::result(r, Dec::Com, &[x]);
        return if even {
            res.signs(x.ne0(), false)
        } else {
            res.signs(x.gt0(), x.lt0())
        };
    }
    // n < 0: 1/x^|n| on each side of 0, running off to ±∞ toward 0.
    let pe = |v: f64| pe_pown(v, n);
    let mut r = Interval::EMPTY;
    let pos = iv.intersect(Interval::new(0.0, INF));
    if !pos.is_empty() && pos.hi() > 0.0 {
        // Decreasing in x.
        let lo = if pos.hi() == INF {
            0.0
        } else {
            pe(pos.hi()).lo()
        };
        let hi = if pos.lo() == 0.0 {
            INF
        } else {
            pe(pos.lo()).hi()
        };
        r = r.hull(Interval::new(lo, hi));
    }
    let negp = iv.intersect(Interval::new(-INF, 0.0));
    if !negp.is_empty() && negp.lo() < 0.0 {
        let piece = if even {
            // Increasing toward 0.
            let lo = if negp.lo() == -INF {
                0.0
            } else {
                pe(negp.lo()).lo()
            };
            let hi = if negp.hi() == 0.0 {
                INF
            } else {
                pe(negp.hi()).hi()
            };
            Interval::new(lo, hi)
        } else {
            // Decreasing toward 0 (to −∞).
            let lo = if negp.hi() == 0.0 {
                -INF
            } else {
                pe(negp.hi()).lo()
            };
            let hi = if negp.lo() == -INF {
                0.0
            } else {
                pe(negp.lo()).hi()
            };
            Interval::new(lo, hi)
        };
        r = r.hull(piece);
    }
    let defined_everywhere = !iv.contains_zero() || x.ne0();
    let res = DecInterval::result(
        r,
        if defined_everywhere {
            Dec::Com
        } else {
            Dec::Trv
        },
        &[x],
    );
    if even {
        res.signs(true, false)
    } else {
        res.signs(x.gt0(), x.lt0())
    }
}

/// e^t for a double-double t, as r·2ⁿ: `(m, n)` with m within about one
/// ulp of the true mantissa (CORE-MATH's exponential of the reduced hi
/// part, corrected by the lo part).
fn dd_exp(t: crate::dd::Dd) -> f64 {
    let n = (t.hi / std::f64::consts::LN_2).round();
    let r = t.add(crate::dd::LN2.mul_f(n).neg());
    let m = cm::exp(r.hi) * (1.0 + r.lo);
    let n = n.clamp(-2200.0, 2200.0) as i32;
    let h = n / 2;
    m * 2f64.powi(h) * 2f64.powi(n - h)
}

/// a^(p/q) for a double a > 0: from a double-double exponent, widened by
/// two ulps each side (the result is good to about one).
fn pe_powrat_pos(a: f64, p: i32, q: i32) -> Interval {
    if a == 1.0 {
        return Interval::point(1.0);
    }
    if p == 1 && q == 2 {
        return Interval::new(super::arith::sqrt_down(a), super::arith::sqrt_up(a));
    }
    if p == 1 && q == 3 {
        return pe_cbrt(a);
    }
    let t = crate::dd::ln(a).mul_f(p as f64).div_f(q as f64);
    if t.hi > 710.0 {
        return Interval::new(f64::MAX, INF);
    }
    if t.hi < -746.0 {
        return Interval::new(0.0, f64::from_bits(1));
    }
    let r = dd_exp(t);
    if !r.is_finite() {
        return Interval::new(f64::MAX, INF);
    }
    Interval::new(r.next_down().next_down().max(0.0), r.next_up().next_up())
}

/// x^(p/q) for q > 1 (lowest terms), real-root semantics: a negative x is
/// allowed when q is odd, (−8)^(1/3) = −2; 0 to a negative power is
/// undefined.
pub fn pow_rational(x: &DecInterval, p: i32, q: i32) -> DecInterval {
    debug_assert!(q > 1);
    if p == 0 {
        return powi(x, 0);
    }
    let odd_q = q % 2 != 0;
    let iv = x.iv;
    if iv.is_empty() {
        return DecInterval::result(iv, Dec::Trv, &[x]);
    }
    // Magnitude on a ≥ 0: increasing for p > 0, decreasing for p < 0.
    let mag = |a: Interval| -> Interval {
        if a.is_empty() {
            return a;
        }
        let pe = |v: f64| {
            if v == 0.0 {
                if p > 0 {
                    Interval::point(0.0)
                } else {
                    Interval::new(f64::MAX, INF)
                }
            } else {
                pe_powrat_pos(v, p, q)
            }
        };
        if p > 0 {
            inc(a, pe, 0.0, INF)
        } else {
            let lo = if a.hi() == INF { 0.0 } else { pe(a.hi()).lo() };
            let hi = if a.lo() == 0.0 { INF } else { pe(a.lo()).hi() };
            Interval::new(lo, hi)
        }
    };
    let pos_part = iv.intersect(Interval::new(0.0, INF));
    let neg_part = iv.intersect(Interval::new(-INF, 0.0));
    let mut r = Interval::EMPTY;
    if !pos_part.is_empty() && !(p < 0 && pos_part.hi() == 0.0) {
        r = r.hull(mag(pos_part));
    }
    let mut left = false;
    if !neg_part.is_empty() && neg_part.lo() < 0.0 {
        if odd_q {
            let m = mag(-neg_part);
            r = r.hull(if p % 2 != 0 { -m } else { m });
        } else {
            left = true;
        }
    }
    let at_zero_undefined = p < 0 && iv.contains_zero() && !x.ne0();
    let local = if left || at_zero_undefined {
        Dec::Trv
    } else {
        Dec::Com
    };
    let res = DecInterval::result(r, local, &[x]);
    let odd_result = odd_q && p % 2 != 0;
    if odd_result {
        res.signs(x.gt0(), x.lt0())
    } else {
        res.signs(x.ne0() && (odd_q || x.gt0()), false)
    }
}

/// b^e for a general exponent (IEEE 1788 `pow`, the TI rule for an
/// exponent that varies): defined for b > 0, and for b = 0 with e > 0.
pub fn pow(b: &DecInterval, e: &DecInterval) -> DecInterval {
    let (bd, left) = restrict(b, Interval::new(0.0, INF));
    // 0^e for e ≤ 0 is undefined (TI-84 Plus CE; `functions::pow`).
    let only_zero_base = bd.lo() == 0.0 && bd.hi() == 0.0;
    if bd.is_empty() || e.is_empty() || (only_zero_base && e.hi() <= 0.0) {
        return DecInterval::result(Interval::EMPTY, Dec::Trv, &[b, e]);
    }
    let corner = |bv: f64, ev: f64| -> Interval {
        if bv == 0.0 {
            // 0^e: 0 for e > 0; the limit as b → 0⁺ otherwise.
            if ev > 0.0 {
                Interval::point(0.0)
            } else if ev == 0.0 {
                Interval::point(1.0)
            } else {
                Interval::new(f64::MAX, INF)
            }
        } else if bv == INF {
            if ev > 0.0 {
                Interval::new(f64::MAX, INF)
            } else if ev < 0.0 {
                Interval::point(0.0)
            } else {
                Interval::point(1.0)
            }
        } else if ev == INF || ev == -INF {
            let up = (bv > 1.0) == (ev > 0.0);
            if bv == 1.0 {
                Interval::point(1.0)
            } else if up {
                Interval::new(f64::MAX, INF)
            } else {
                Interval::point(0.0)
            }
        } else if bv == 1.0 || ev == 0.0 {
            Interval::point(1.0)
        } else if ev == 1.0 {
            Interval::point(bv)
        } else {
            widen(cm::pow(bv, ev), 0.0, INF)
        }
    };
    // b^e is monotone in each argument on the box: extremes at corners.
    let mut r = Interval::EMPTY;
    for bv in [bd.lo(), bd.hi()] {
        for ev in [e.lo(), e.hi()] {
            r = r.hull(corner(bv, ev));
        }
    }
    // Where b's range starts at 0 and e can be ≤ 0, 0^e is undefined.
    let zero_bad = bd.lo() == 0.0 && !b.gt0() && e.lo() <= 0.0;
    let local = if left || zero_bad { Dec::Trv } else { Dec::Com };
    let res = DecInterval::result(r, local, &[b, e]);
    res.signs(b.gt0(), false)
}

/// root(x, n): the n-th root, real for odd integer n.
pub fn root(x: &DecInterval, n: &DecInterval) -> DecInterval {
    let nv = n.iv;
    if nv.is_point() && nv.lo() == nv.lo().trunc() && nv.lo().abs() < 1e6 && nv.lo() != 0.0 {
        let k = nv.lo() as i32;
        let r = if k == 1 {
            *x
        } else if k == -1 {
            recip(x)
        } else if k > 0 {
            pow_rational(x, 1, k)
        } else {
            pow_rational(x, -1, -k)
        };
        return r.cap(n.dec);
    }
    // A point degree is that number: an odd integer (only below 2⁵³:
    // every double beyond is even) takes a negative x too. (An infinite
    // one is no degree.)
    if nv.is_point() && nv.lo() != 0.0 {
        if !nv.lo().is_finite() {
            return DecInterval::result(Interval::EMPTY, Dec::Trv, &[x, n]);
        }
        return root_of_parity(x, n, crate::functions::is_odd_integer(nv.lo()));
    }
    // A general degree: x ≥ 0 (x > 0 for a negative degree).
    let mut r = pow(x, &recip(n));
    // Unless the degree may be an odd integer, which takes a negative x
    // too (root(x, x) is −1 at −1): there, −|x|^(1/n), maybe.
    if x.lo() < 0.0 && may_hold_odd(nv) {
        let mirrored = DecInterval::new((-x.iv).intersect(Interval::new(0.0, INF)));
        let m = neg(&pow(&mirrored, &recip(n)));
        if !m.is_empty() {
            r = DecInterval::result(r.iv.hull(m.iv), Dec::Trv, &[x, n]);
        } else {
            r = r.cap(Dec::Trv);
        }
    }
    if n.iv.contains_zero() && !n.ne0() {
        r.cap(Dec::Trv)
    } else {
        r
    }
}

/// Whether the real interval `n` holds an odd integer. Beyond 2⁵³ every
/// double is even, but the numbers between two of them aren't: the box
/// [2⁵³, 2⁵³ + 2] holds the odd 2⁵³ + 1 (review 13, R13-M-04: such a box
/// was taken to hold none, and root(−8, n) over it was empty). So does
/// any box past −10³⁰⁰ that is more than a point, and one reaching −∞
/// (review 14, R14-M-04: the lower end was clipped to −10³⁰⁰, which put
/// the first candidate above the box [−10³⁰², −10³⁰¹]).
pub(crate) fn may_hold_odd(n: Interval) -> bool {
    if n.is_empty() {
        return false;
    }
    const BIG: f64 = 9007199254740992.0;
    // The least integer in the box (−∞ for a box reaching −∞).
    let first = n.lo().ceil();
    if first.abs() >= BIG {
        // An even integer (or −∞), the next number above it odd: any
        // double above `first` is at least one more, and any real number
        // above −∞ has an odd integer below it.
        return n.hi() > first;
    }
    // first + 1 ≤ 2⁵³ is exact.
    let odd = if first.rem_euclid(2.0) == 1.0 {
        first
    } else {
        first + 1.0
    };
    odd <= n.hi()
}

/// root(x, n) for a degree known exactly to be an odd integer (`odd`) or
/// not (an even integer, or no integer), whatever its enclosure `n` shows:
/// a typed 9007199254740993 is odd, though enclosed by 2⁵³ and the double
/// after it. Odd: sign(x)·|x|^(1/n), x ≠ 0 for n < 0. Otherwise x ≥ 0
/// (x > 0 for n < 0), as for a degree that varies.
pub fn root_of_parity(x: &DecInterval, n: &DecInterval, odd: bool) -> DecInterval {
    let e = recip(n);
    let r = if !odd || x.lo() >= 0.0 || x.gt0() {
        pow(x, &e)
    } else if x.hi() <= 0.0 || x.lt0() {
        neg(&pow(&neg(x), &e))
    } else {
        // Both signs: each half, x = 0 in both.
        let half = |iv: Interval| pow(&DecInterval::with_dec(iv, x.dec), &e);
        let p = half(x.iv.intersect(Interval::new(0.0, INF)));
        let m = neg(&half((-x.iv).intersect(Interval::new(0.0, INF))));
        let dec = p.dec.min(m.dec);
        DecInterval::result(p.iv.hull(m.iv), dec, &[x, n])
    };
    let r = if odd { r.signs(x.gt0(), x.lt0()) } else { r };
    if n.iv.contains_zero() && !n.ne0() {
        r.cap(Dec::Trv)
    } else {
        r
    }
}

// ------------------------------------------------------------------ trig

/// sin and cos of a double angle in `unit`, enclosed. Degrees and grads
/// are reduced exactly to the nearest quarter turn first (so sin 180° is
/// exactly 0), then evaluated with sinpi/cospi.
pub(crate) fn pe_sin_cos(x: f64, unit: TrigUnit) -> (Interval, Interval) {
    if unit == TrigUnit::Radians {
        if x == 0.0 {
            return (Interval::point(0.0), Interval::point(1.0));
        }
        let (s, c) = cm::sincos(x);
        return (widen(s, -1.0, 1.0), widen(c, -1.0, 1.0));
    }
    let turn = unit.full_turn();
    let quarter = turn / 4.0;
    let half = turn / 2.0;
    let r = x.abs() % turn;
    let qi = (r / quarter + 0.5) as i64;
    let d = r - qi as f64 * quarter;
    let (sd, cd) = if d == 0.0 {
        (Interval::point(0.0), Interval::point(1.0))
    } else {
        // t = d/half exactly enclosed; sinpi rises and cospi falls in |t|
        // on [0, ¼].
        let t0 = d / half;
        let t = if d.abs() > 1e-290 {
            let res = (-t0).mul_add(half, d);
            if res == 0.0 {
                Interval::point(t0)
            } else if res > 0.0 {
                Interval::new(t0, t0.next_up())
            } else {
                Interval::new(t0.next_down(), t0)
            }
        } else {
            Interval::around(t0)
        };
        let s = Interval::new(
            cm::sinpi(t.lo()).next_down().max(-1.0),
            cm::sinpi(t.hi()).next_up().min(1.0),
        );
        let ta = t.abs();
        let c = Interval::new(
            cm::cospi(ta.hi()).next_down().max(-1.0),
            cm::cospi(ta.lo()).next_up().min(1.0),
        );
        (s, c)
    };
    let (s, c) = match qi % 4 {
        0 => (sd, cd),
        1 => (cd, -sd),
        2 => (-sd, -cd),
        _ => (-cd, sd),
    };
    if x < 0.0 { (-s, c) } else { (s, c) }
}

/// sin and cos of one double angle: enclosures and, when decided, their
/// exact signs (in radians those of the correctly rounded values: no double
/// but 0 is a multiple of π/2, and rounding to nearest keeps a nonzero
/// value's sign; in degrees and grads those of the exact-reduction
/// enclosures).
#[derive(Clone, Copy)]
struct Ep {
    s: Interval,
    c: Interval,
    signs: Option<(i8, i8)>,
}

fn endpoint(x: f64, unit: TrigUnit) -> Ep {
    let sg = |v: f64| -> i8 {
        if v > 0.0 {
            1
        } else if v < 0.0 {
            -1
        } else {
            0
        }
    };
    if unit == TrigUnit::Radians {
        if x == 0.0 {
            return Ep {
                s: Interval::point(0.0),
                c: Interval::point(1.0),
                signs: Some((0, 1)),
            };
        }
        let (s, c) = cm::sincos(x);
        return Ep {
            s: widen(s, -1.0, 1.0),
            c: widen(c, -1.0, 1.0),
            signs: Some((sg(s), sg(c))),
        };
    }
    let (s, c) = pe_sin_cos(x, unit);
    let decide = |i: Interval| -> Option<i8> {
        if i.gt0() {
            Some(1)
        } else if i.lt0() {
            Some(-1)
        } else if i.is_point() {
            Some(0)
        } else {
            None
        }
    };
    let signs = match (decide(s), decide(c)) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    };
    Ep { s, c, signs }
}

/// Whether `[lo, hi]` (finite) is at least half a turn wide, judged
/// conservatively.
fn half_turn_or_more(x: Interval, unit: TrigUnit) -> bool {
    let w = x.width();
    match unit {
        TrigUnit::Radians => w >= PI,
        u => w >= u.full_turn() / 2.0,
    }
}

/// The images of `iv` under sin and under cos, from shared end values.
fn sin_cos_ivs(iv: Interval, unit: TrigUnit) -> (Interval, Interval) {
    let full = Interval::new(-1.0, 1.0);
    if !iv.is_bounded() {
        return (full, full);
    }
    let turn = match unit {
        TrigUnit::Radians => 2.0 * PI,
        u => u.full_turn(),
    };
    if iv.width() >= turn {
        return (full, full);
    }
    if half_turn_or_more(iv, unit) {
        let m = iv.mid();
        // Two adjacent doubles a half turn or more apart (beyond 2⁵³ or
        // so): no double between to split at.
        if !(iv.lo() < m && m < iv.hi()) {
            return (full, full);
        }
        let (sa, ca) = sin_cos_ivs(Interval::new(iv.lo(), m), unit);
        let (sb, cb) = sin_cos_ivs(Interval::new(m, iv.hi()), unit);
        return (sa.hull(sb), ca.hull(cb));
    }
    let el = endpoint(iv.lo(), unit);
    let mut s = el.s;
    let mut c = el.c;
    if iv.is_point() {
        return (s, c);
    }
    let eh = endpoint(iv.hi(), unit);
    s = s.hull(eh.s);
    c = c.hull(eh.c);
    // Under half a turn: at most one critical point of each, inside exactly
    // when the derivative (cos for sin, −sin for cos) changes sign.
    let (Some((sl, cl)), Some((sh, ch))) = (el.signs, eh.signs) else {
        return (full, full);
    };
    if cl > 0 && ch < 0 {
        s = Interval::new(s.lo(), 1.0);
    } else if cl < 0 && ch > 0 {
        s = Interval::new(-1.0, s.hi());
    }
    if sl < 0 && sh > 0 {
        c = Interval::new(c.lo(), 1.0);
    } else if sl > 0 && sh < 0 {
        c = Interval::new(-1.0, c.hi());
    }
    (s.intersect(full), c.intersect(full))
}

/// sin and cos of the same argument, sharing their work.
pub fn sin_cos(x: &DecInterval, unit: TrigUnit) -> (DecInterval, DecInterval) {
    if x.is_empty() {
        let e = DecInterval::result(x.iv, Dec::Trv, &[x]);
        return (e, e);
    }
    let (s, c) = sin_cos_ivs(x.iv, unit);
    let (sd, cd) = (
        DecInterval::result(s, Dec::Com, &[x]),
        DecInterval::result(c, Dec::Com, &[x]),
    );
    // Strict signs, when both ends have the same one and the box is under
    // half a turn (no zero inside: sin and cos have simple zeros half a
    // turn apart). The enclosure may still reach 0 (sin of a subnormal
    // rounds to one), the value never does: csc and cot there stay defined.
    let iv = x.iv;
    if iv.is_bounded() && !half_turn_or_more(iv, unit) {
        let (el, eh) = (endpoint(iv.lo(), unit), endpoint(iv.hi(), unit));
        if let (Some((sl, cl)), Some((sh, ch))) = (el.signs, eh.signs) {
            let fact = |a: i8, b: i8| (a == b && a > 0, a == b && a < 0);
            let (sp, sn) = fact(sl, sh);
            let (cp, cn) = fact(cl, ch);
            return (sd.signs(sp, sn), cd.signs(cp, cn));
        }
    }
    (sd, cd)
}

pub fn sin(x: &DecInterval, unit: TrigUnit) -> DecInterval {
    sin_cos(x, unit).0
}

pub fn cos(x: &DecInterval, unit: TrigUnit) -> DecInterval {
    sin_cos(x, unit).1
}

/// True if `iv` may contain a zero of sin (`sine`) or cos: a pole of
/// csc/cot or of tan/sec.
fn may_hit_zero(iv: Interval, unit: TrigUnit, sine: bool) -> bool {
    if !iv.is_bounded() || half_turn_or_more(iv, unit) {
        return true;
    }
    // Under half a turn, a zero inside means a sign change; a zero at an end
    // is exact (degrees) or the end is 0 (radians, sine).
    match (endpoint(iv.lo(), unit).signs, endpoint(iv.hi(), unit).signs) {
        (Some((sl, cl)), Some((sh, ch))) => {
            let (a, b) = if sine { (sl, sh) } else { (cl, ch) };
            !(a != 0 && a == b)
        }
        _ => true,
    }
}

pub fn tan(x: &DecInterval, unit: TrigUnit) -> DecInterval {
    let iv = x.iv;
    if iv.is_empty() {
        return DecInterval::result(iv, Dec::Trv, &[x]);
    }
    if may_hit_zero(iv, unit, false) {
        return DecInterval::result(Interval::ENTIRE, Dec::Trv, &[x]);
    }
    let pe = |v: f64| -> Interval {
        if unit == TrigUnit::Radians {
            if v == 0.0 {
                Interval::point(0.0)
            } else {
                cr(cm::tan(v))
            }
        } else {
            let (s, c) = pe_sin_cos(v, unit);
            s / c
        }
    };
    DecInterval::result(inc(iv, pe, -INF, INF), Dec::Com, &[x])
}

/// 1/cos.
pub fn sec(x: &DecInterval, unit: TrigUnit) -> DecInterval {
    if !x.is_empty() && may_hit_zero(x.iv, unit, false) {
        return DecInterval::result(recip(&cos(x, unit)).iv, Dec::Trv, &[x]);
    }
    recip(&cos(x, unit))
}

/// 1/sin.
pub fn csc(x: &DecInterval, unit: TrigUnit) -> DecInterval {
    if !x.is_empty() && may_hit_zero(x.iv, unit, true) {
        return DecInterval::result(recip(&sin(x, unit)).iv, Dec::Trv, &[x]);
    }
    recip(&sin(x, unit))
}

/// cos/sin: decreasing between its poles.
pub fn cot(x: &DecInterval, unit: TrigUnit) -> DecInterval {
    let iv = x.iv;
    if iv.is_empty() {
        return DecInterval::result(iv, Dec::Trv, &[x]);
    }
    if may_hit_zero(iv, unit, true) {
        return DecInterval::result(Interval::ENTIRE, Dec::Trv, &[x]);
    }
    let pe = |v: f64| -> Interval {
        let (s, c) = pe_sin_cos(v, unit);
        c / s
    };
    DecInterval::result(dec(iv, pe, INF, -INF), Dec::Com, &[x])
}

// ---------------------------------------------------- inverse trig

/// Radians → the unit, for a radian enclosure.
fn from_rad(r: Interval, unit: TrigUnit) -> Interval {
    match unit {
        TrigUnit::Radians => r,
        u => r * (Interval::point(u.full_turn() / 2.0) / pi()),
    }
}

/// A result in half-turns (asinpi etc.) → the unit.
fn from_halfturns(h: Interval, unit: TrigUnit) -> Interval {
    match unit {
        TrigUnit::Radians => h * pi(),
        u => h * Interval::point(u.full_turn() / 2.0),
    }
}

fn pe_asin(v: f64, unit: TrigUnit) -> Interval {
    if v == 0.0 {
        return Interval::point(0.0);
    }
    match unit {
        TrigUnit::Radians => cr(cm::asin(v)),
        u => from_halfturns(cr(cm::asinpi(v)), u),
    }
}

fn pe_acos(v: f64, unit: TrigUnit) -> Interval {
    if v == 1.0 {
        return Interval::point(0.0);
    }
    match unit {
        TrigUnit::Radians => cr(cm::acos(v)),
        u => from_halfturns(cr(cm::acospi(v)), u),
    }
}

fn pe_atan(v: f64, unit: TrigUnit) -> Interval {
    if v == 0.0 {
        return Interval::point(0.0);
    }
    if v.is_infinite() {
        let q = match unit {
            TrigUnit::Radians => half_pi(),
            u => Interval::point(u.full_turn() / 4.0),
        };
        return if v > 0.0 { q } else { -q };
    }
    match unit {
        TrigUnit::Radians => cr(cm::atan(v)),
        u => from_halfturns(cr(cm::atanpi(v)), u),
    }
}

pub fn asin(x: &DecInterval, unit: TrigUnit) -> DecInterval {
    let (d, left) = restrict(x, Interval::new(-1.0, 1.0));
    let iv = inc(d, |v| pe_asin(v, unit), 0.0, 0.0);
    on_domain(iv, left, Dec::Com, x).signs(x.gt0(), x.lt0())
}

pub fn acos(x: &DecInterval, unit: TrigUnit) -> DecInterval {
    let (d, left) = restrict(x, Interval::new(-1.0, 1.0));
    let iv = dec(d, |v| pe_acos(v, unit), 0.0, 0.0);
    on_domain(iv, left, Dec::Com, x)
}

pub fn atan(x: &DecInterval, unit: TrigUnit) -> DecInterval {
    let iv = inc(
        x.iv,
        |v| pe_atan(v, unit),
        pe_atan(-INF, unit).lo(),
        pe_atan(INF, unit).hi(),
    );
    DecInterval::result(iv, Dec::Com, &[x]).signs(x.gt0(), x.lt0())
}

/// arcsec on one sign piece, as the app computes it: arccos(1/x) for |x| ≥
/// 2, arctan √(x² − 1) (π − that for x < 0) near ±1.
fn pe_asec(v: f64, unit: TrigUnit) -> Interval {
    if v == 1.0 {
        return Interval::point(0.0);
    }
    if v == -1.0 {
        return match unit {
            TrigUnit::Radians => pi(),
            u => Interval::point(u.full_turn() / 2.0),
        };
    }
    let x = DecInterval::point(v);
    let r = if v.abs() >= 2.0 {
        acos(&recip(&x), TrigUnit::Radians).iv
    } else {
        let a = Interval::point(v.abs());
        let s = ((a - Interval::point(1.0)) * (a + Interval::point(1.0))).sqrt();
        let t = atan(&DecInterval::new(s), TrigUnit::Radians).iv;
        if v < 0.0 { pi() - t } else { t }
    };
    from_rad(r, unit)
}

/// arcsec: increasing on x ≤ −1 and on x ≥ 1, undefined between.
pub fn asec(x: &DecInterval, unit: TrigUnit) -> DecInterval {
    asec_or_acsc(x, unit, true)
}

fn pe_acsc(v: f64, unit: TrigUnit) -> Interval {
    if v.abs() == 1.0 {
        let q = match unit {
            TrigUnit::Radians => half_pi(),
            u => Interval::point(u.full_turn() / 4.0),
        };
        return if v < 0.0 { -q } else { q };
    }
    let x = DecInterval::point(v);
    let r = if v.abs() >= 2.0 {
        asin(&recip(&x), TrigUnit::Radians).iv
    } else {
        let a = Interval::point(v.abs());
        let s = ((a - Interval::point(1.0)) * (a + Interval::point(1.0))).sqrt();
        let t = atan(&recip(&DecInterval::new(s)), TrigUnit::Radians).iv;
        if v < 0.0 { -t } else { t }
    };
    from_rad(r, unit)
}

/// arccsc: decreasing on x ≤ −1 and on x ≥ 1, undefined between.
pub fn acsc(x: &DecInterval, unit: TrigUnit) -> DecInterval {
    asec_or_acsc(x, unit, false)
}

fn asec_or_acsc(x: &DecInterval, unit: TrigUnit, sec: bool) -> DecInterval {
    let iv = x.iv;
    let neg = iv.intersect(Interval::new(-INF, -1.0));
    let pos = iv.intersect(Interval::new(1.0, INF));
    let left = !iv.subset(Interval::new(-INF, -1.0)) && !iv.subset(Interval::new(1.0, INF));
    let pe = |v: f64| {
        if sec {
            pe_asec(v, unit)
        } else {
            pe_acsc(v, unit)
        }
    };
    // Limits at ±∞: arcsec → π/2 (a quarter turn), arccsc → 0.
    let quarter = match unit {
        TrigUnit::Radians => half_pi(),
        u => Interval::point(u.full_turn() / 4.0),
    };
    let piece = |p: Interval| -> Interval {
        if p.is_empty() {
            return p;
        }
        let at = |v: f64| -> Interval {
            if v.is_infinite() {
                if sec { quarter } else { Interval::point(0.0) }
            } else {
                pe(v)
            }
        };
        if sec {
            Interval::new(at(p.lo()).lo(), at(p.hi()).hi())
        } else {
            Interval::new(at(p.hi()).lo(), at(p.lo()).hi())
        }
    };
    let r = piece(neg).hull(piece(pos));
    let res = on_domain(r, left, Dec::Com, x);
    // Two pieces with a gap between is defined but not continuous: but an
    // input box spanning (−1, 1) already left the domain.
    if sec {
        res
    } else {
        res.signs(x.gt0(), x.lt0())
    }
}

fn pe_acot(v: f64, unit: TrigUnit) -> Interval {
    if v == 0.0 {
        return match unit {
            TrigUnit::Radians => half_pi(),
            u => Interval::point(u.full_turn() / 4.0),
        };
    }
    let inv = Interval::point(1.0) / Interval::point(v);
    let a = atan(&DecInterval::new(inv), unit).iv;
    if v > 0.0 {
        a
    } else {
        let h = match unit {
            TrigUnit::Radians => pi(),
            u => Interval::point(u.full_turn() / 2.0),
        };
        h + a
    }
}

/// arccot, range (0, π): continuous and decreasing on ℝ.
pub fn acot(x: &DecInterval, unit: TrigUnit) -> DecInterval {
    let h = match unit {
        TrigUnit::Radians => pi(),
        u => Interval::point(u.full_turn() / 2.0),
    };
    let iv = dec(x.iv, |v| pe_acot(v, unit), h.hi(), 0.0);
    DecInterval::result(iv, Dec::Com, &[x]).signs(true, false)
}

// ------------------------------------------------------------- hyperbolic

fn pe_sinh(v: f64) -> Interval {
    if v == 0.0 {
        Interval::point(0.0)
    } else {
        cr(cm::sinh(v))
    }
}

fn pe_cosh(v: f64) -> Interval {
    if v == 0.0 {
        Interval::point(1.0)
    } else {
        widen(cm::cosh(v), 1.0, INF)
    }
}

fn pe_tanh(v: f64) -> Interval {
    if v == 0.0 {
        Interval::point(0.0)
    } else {
        widen(cm::tanh(v), -1.0, 1.0)
    }
}

pub fn sinh(x: &DecInterval) -> DecInterval {
    DecInterval::result(inc(x.iv, pe_sinh, -INF, INF), Dec::Com, &[x]).signs(x.gt0(), x.lt0())
}

/// Even, least (1) at 0.
fn even_min_at_zero(
    iv: Interval,
    pe: impl Fn(f64) -> Interval,
    at_inf: f64,
    at_zero: f64,
) -> Interval {
    if iv.is_empty() {
        return iv;
    }
    let a = iv.abs();
    let lo = if a.lo() == 0.0 {
        at_zero
    } else {
        pe(a.lo()).lo()
    };
    let hi = if a.hi() == INF {
        at_inf
    } else {
        pe(a.hi()).hi()
    };
    Interval::new(lo, hi)
}

pub fn cosh(x: &DecInterval) -> DecInterval {
    DecInterval::result(even_min_at_zero(x.iv, pe_cosh, INF, 1.0), Dec::Com, &[x])
        .signs(true, false)
}

pub fn tanh(x: &DecInterval) -> DecInterval {
    DecInterval::result(inc(x.iv, pe_tanh, -1.0, 1.0), Dec::Com, &[x]).signs(x.gt0(), x.lt0())
}

/// sech v = 2e^−|v| / (1 + e^−2|v|) beyond |v| = 1 (no overflow), 1/cosh
/// within.
fn pe_sech(v: f64) -> Interval {
    let a = v.abs();
    if a < 1.0 {
        return Interval::point(1.0) / pe_cosh(a);
    }
    let e = pe_exp(-a);
    (Interval::point(2.0) * e) / (Interval::point(1.0) + e.sqr())
}

/// Even, greatest (1) at 0, decreasing in |x|.
pub fn sech(x: &DecInterval) -> DecInterval {
    let iv = x.iv;
    let r = if iv.is_empty() {
        iv
    } else {
        let a = iv.abs();
        let hi = if a.lo() == 0.0 {
            1.0
        } else {
            pe_sech(a.lo()).hi()
        };
        let lo = if a.hi() == INF {
            0.0
        } else {
            pe_sech(a.hi()).lo()
        };
        Interval::new(lo.max(0.0), hi.min(1.0))
    };
    DecInterval::result(r, Dec::Com, &[x]).signs(true, false)
}

/// csch v, from 2e^−|v| / (1 − e^−2|v|) beyond |v| = 1.
fn pe_csch(v: f64) -> Interval {
    let a = v.abs();
    let m = if a < 1.0 {
        Interval::point(1.0) / pe_sinh(a)
    } else {
        let e = pe_exp(-a);
        (Interval::point(2.0) * e) / (Interval::point(1.0) - e.sqr())
    };
    if v < 0.0 { -m } else { m }
}

/// Undefined at 0, decreasing on each side.
pub fn csch(x: &DecInterval) -> DecInterval {
    two_sided_decreasing(x, pe_csch, (0.0, -INF), (INF, 0.0))
}

fn pe_coth(v: f64) -> Interval {
    Interval::point(1.0) / pe_tanh(v)
}

/// Undefined at 0, decreasing on each side (|coth| > 1).
pub fn coth(x: &DecInterval) -> DecInterval {
    two_sided_decreasing(x, pe_coth, (-1.0, -INF), (INF, 1.0))
}

/// A function undefined at 0 and decreasing on (−∞, 0) and (0, ∞), with
/// limits `(at −∞, at 0⁻)` and `(at 0⁺, at +∞)`.
fn two_sided_decreasing(
    x: &DecInterval,
    pe: impl Fn(f64) -> Interval,
    neg_lim: (f64, f64),
    pos_lim: (f64, f64),
) -> DecInterval {
    let iv = x.iv;
    let mut r = Interval::EMPTY;
    let pos = iv.intersect(Interval::new(0.0, INF));
    if !pos.is_empty() && pos.hi() > 0.0 {
        let hi = if pos.lo() == 0.0 {
            pos_lim.0
        } else {
            pe(pos.lo()).hi()
        };
        let lo = if pos.hi() == INF {
            pos_lim.1
        } else {
            pe(pos.hi()).lo()
        };
        r = r.hull(Interval::new(lo, hi));
    }
    let neg = iv.intersect(Interval::new(-INF, 0.0));
    if !neg.is_empty() && neg.lo() < 0.0 {
        let hi = if neg.lo() == -INF {
            neg_lim.0
        } else {
            pe(neg.lo()).hi()
        };
        let lo = if neg.hi() == 0.0 {
            neg_lim.1
        } else {
            pe(neg.hi()).lo()
        };
        r = r.hull(Interval::new(lo, hi));
    }
    let defined = !iv.contains_zero() || x.ne0();
    DecInterval::result(r, if defined { Dec::Com } else { Dec::Trv }, &[x]).signs(x.gt0(), x.lt0())
}

pub fn asinh(x: &DecInterval) -> DecInterval {
    let pe = |v: f64| {
        if v == 0.0 {
            Interval::point(0.0)
        } else {
            cr(cm::asinh(v))
        }
    };
    DecInterval::result(inc(x.iv, pe, -INF, INF), Dec::Com, &[x]).signs(x.gt0(), x.lt0())
}

/// Defined for x ≥ 1.
pub fn acosh(x: &DecInterval) -> DecInterval {
    let (d, left) = restrict(x, Interval::new(1.0, INF));
    let pe = |v: f64| {
        if v == 1.0 {
            Interval::point(0.0)
        } else {
            widen(cm::acosh(v), 0.0, INF)
        }
    };
    on_domain(inc(d, pe, 0.0, INF), left, Dec::Com, x)
}

/// Defined on (−1, 1), unbounded toward its poles.
pub fn atanh(x: &DecInterval) -> DecInterval {
    let (d, left) = restrict(x, Interval::new(-1.0, 1.0));
    let at_pole = d.lo() == -1.0 || d.hi() == 1.0;
    let pe = |v: f64| {
        if v == 0.0 {
            Interval::point(0.0)
        } else if v == 1.0 {
            Interval::new(f64::MAX, INF)
        } else if v == -1.0 {
            Interval::new(-INF, -f64::MAX)
        } else {
            cr(cm::atanh(v))
        }
    };
    let iv = if d.is_empty() || (d.lo() == 1.0) || (d.hi() == -1.0) {
        Interval::EMPTY
    } else {
        let lo = if d.lo() == -1.0 {
            -INF
        } else {
            pe(d.lo()).lo()
        };
        let hi = if d.hi() == 1.0 { INF } else { pe(d.hi()).hi() };
        Interval::new(lo, hi)
    };
    let local = if left || at_pole { Dec::Trv } else { Dec::Com };
    DecInterval::result(iv, local, &[x]).signs(x.gt0(), x.lt0())
}

/// arsech v = ln(1 + √((1 − v)(1 + v))) − ln v on (0, 1].
fn pe_asech(v: f64) -> Interval {
    let x = DecInterval::point(v);
    let one = DecInterval::point(1.0);
    let s = sqrt(&mul(&sub(&one, &x), &add(&one, &x)));
    let ln_x = if v > 0.5 {
        log1p(&sub(&x, &one))
    } else {
        ln(&x)
    };
    sub(&log1p(&s), &ln_x).iv
}

/// Defined on (0, 1], decreasing, unbounded toward 0.
pub fn asech(x: &DecInterval) -> DecInterval {
    let (d, left) = restrict(x, Interval::new(0.0, 1.0));
    let at0 = d.lo() == 0.0;
    let iv = if d.is_empty() || d.hi() == 0.0 {
        Interval::EMPTY
    } else {
        let lo = pe_asech(d.hi()).lo().max(0.0);
        let hi = if at0 { INF } else { pe_asech(d.lo()).hi() };
        Interval::new(lo, hi)
    };
    let local = if left || at0 { Dec::Trv } else { Dec::Com };
    DecInterval::result(iv, local, &[x])
}

/// arcsch v: arsinh(1/v) beyond |v| = 1, ±(ln(1 + √(1 + v²)) − ln|v|)
/// within.
fn pe_acsch(v: f64) -> Interval {
    let a = DecInterval::point(v.abs());
    let one = DecInterval::point(1.0);
    let m = if v.abs() > 1.0 {
        asinh(&recip(&a)).iv
    } else {
        sub(&log1p(&sqrt(&add(&one, &sqr(&a)))), &ln(&a)).iv
    };
    if v < 0.0 { -m } else { m }
}

pub fn acsch(x: &DecInterval) -> DecInterval {
    two_sided_decreasing(x, pe_acsch, (0.0, -INF), (INF, 0.0))
}

/// arcoth v = ±½ ln(1 + 2/(|v| − 1)) for |v| > 1.
fn pe_acoth(v: f64) -> Interval {
    let a = DecInterval::point(v.abs());
    let one = DecInterval::point(1.0);
    let m = mul(
        &DecInterval::point(0.5),
        &log1p(&div(&DecInterval::point(2.0), &sub(&a, &one))),
    )
    .iv;
    if v < 0.0 { -m } else { m }
}

/// Defined for |x| > 1, decreasing on each piece, unbounded toward ±1.
pub fn acoth(x: &DecInterval) -> DecInterval {
    let iv = x.iv;
    let mut r = Interval::EMPTY;
    let pos = iv.intersect(Interval::new(1.0, INF));
    if !pos.is_empty() && pos.hi() > 1.0 {
        let hi = if pos.lo() == 1.0 {
            INF
        } else {
            pe_acoth(pos.lo()).hi()
        };
        let lo = if pos.hi() == INF {
            0.0
        } else {
            pe_acoth(pos.hi()).lo()
        };
        r = r.hull(Interval::new(lo, hi));
    }
    let neg = iv.intersect(Interval::new(-INF, -1.0));
    if !neg.is_empty() && neg.lo() < -1.0 {
        let hi = if neg.lo() == -INF {
            -0.0
        } else {
            pe_acoth(neg.lo()).hi()
        };
        let lo = if neg.hi() == -1.0 {
            -INF
        } else {
            pe_acoth(neg.hi()).lo()
        };
        r = r.hull(Interval::new(lo, hi));
    }
    let inside = iv.subset(Interval::new(1.0, INF)) || iv.subset(Interval::new(-INF, -1.0));
    let at_pole = iv.contains(1.0) || iv.contains(-1.0);
    let local = if !inside || at_pole {
        Dec::Trv
    } else {
        Dec::Com
    };
    DecInterval::result(r, local, &[x]).signs(x.gt0(), x.lt0())
}

// ------------------------------------------------- piecewise functions

pub fn abs(x: &DecInterval) -> DecInterval {
    DecInterval::result(x.iv.abs(), Dec::Com, &[x]).signs(x.ne0(), false)
}

/// A non-decreasing step function `f` (floor, ceil, round, sign):
/// continuous on the box exactly when it takes one value there.
fn step(x: &DecInterval, f: impl Fn(f64) -> f64) -> DecInterval {
    let iv = x.iv;
    if iv.is_empty() {
        return DecInterval::result(iv, Dec::Trv, &[x]);
    }
    let (a, b) = (f(iv.lo()), f(iv.hi()));
    let r = Interval::new(a, b);
    let local = if a == b { Dec::Com } else { Dec::Def };
    DecInterval::result(r, local, &[x])
}

pub fn floor(x: &DecInterval) -> DecInterval {
    step(x, f64::floor)
}

pub fn ceil(x: &DecInterval) -> DecInterval {
    step(x, f64::ceil)
}

/// Round half away from zero.
pub fn round(x: &DecInterval) -> DecInterval {
    step(x, f64::round)
}

pub fn sign(x: &DecInterval) -> DecInterval {
    step(x, crate::functions::sign)
}

pub fn min(a: &DecInterval, b: &DecInterval) -> DecInterval {
    let r = Interval::new(a.lo().min(b.lo()), a.hi().min(b.hi()));
    DecInterval::result(
        if a.is_empty() || b.is_empty() {
            Interval::EMPTY
        } else {
            r
        },
        Dec::Com,
        &[a, b],
    )
    .signs(a.gt0() && b.gt0(), a.lt0() || b.lt0())
}

pub fn max(a: &DecInterval, b: &DecInterval) -> DecInterval {
    let r = Interval::new(a.lo().max(b.lo()), a.hi().max(b.hi()));
    DecInterval::result(
        if a.is_empty() || b.is_empty() {
            Interval::EMPTY
        } else {
            r
        },
        Dec::Com,
        &[a, b],
    )
    .signs(a.gt0() || b.gt0(), a.lt0() && b.lt0())
}

/// Floored modulo a − b·⌊a/b⌋, undefined where b = 0; it has b's sign and
/// |mod| < |b|.
pub fn modulo(a: &DecInterval, b: &DecInterval) -> DecInterval {
    let (ai, bi) = (a.iv, b.iv);
    if ai.is_empty() || bi.is_empty() || (bi.lo() == 0.0 && bi.hi() == 0.0) {
        return DecInterval::result(Interval::EMPTY, Dec::Trv, &[a, b]);
    }
    let range = Interval::new(bi.lo().min(0.0), bi.hi().max(0.0));
    if bi.contains_zero() && !b.ne0() {
        return DecInterval::result(range, Dec::Trv, &[a, b]);
    }
    if ai.is_point() && bi.is_point() && ai.lo().is_finite() && bi.lo().is_finite() {
        // `%` is exact; adding b back (when the signs differ) rounds once.
        let (x, y) = (ai.lo(), bi.lo());
        let r = x % y;
        let iv = if r == 0.0 || (r < 0.0) == (y < 0.0) {
            Interval::point(r)
        } else {
            Interval::point(r) + Interval::point(y)
        };
        return DecInterval::result(iv.intersect(range), Dec::Com, &[a, b]);
    }
    let k = floor(&div(a, b)).iv;
    if k.is_point() && k.lo().is_finite() {
        let r = ai - bi * Interval::point(k.lo());
        return DecInterval::result(r.intersect(range), Dec::Com, &[a, b]);
    }
    DecInterval::result(range, Dec::Def, &[a, b])
}

// ---------------------------------------------------------- factorials

/// Γ's minimum on (0, ∞) is at x₀ = 1.46163214496836234…: the bounds
/// below are either side of it, and its value 0.88560319441088870… is
/// above `GAMMA_MIN`.
const GAMMA_ARGMIN_LO: f64 = 1.461632144968362;
const GAMMA_ARGMIN_HI: f64 = 1.4616321449683625;
const GAMMA_MIN: f64 = 0.8856031944108886;

/// Γ at a double, enclosed (CORE-MATH's is correctly rounded).
fn gamma_pt(v: f64) -> Interval {
    cr(cm::tgamma(v))
}

/// Γ over `v` and whether it is defined throughout (no pole inside).
///
/// Off the poles Γ is continuous and |Γ| log-convex on each branch, so
/// its largest magnitude on a box is at an end. The smallest is at an end
/// unless the box holds the branch's extremum: on (0, ∞) that is x₀,
/// enclosed above; on a negative branch the reflection formula
/// |Γ(x)| = π / (|sin πx| · Γ(1 − x)) bounds it below by
/// π / (max |sin πx| · max Γ(1 − x)), the second maximum again at an
/// end of the box (Γ is convex on (1, ∞)).
pub(crate) fn gamma_iv(v: Interval) -> (Interval, bool) {
    if v.is_empty() {
        return (v, false);
    }
    let (a, b) = (v.lo(), v.hi());
    if a.is_nan() || b.is_nan() {
        return (Interval::ENTIRE, false);
    }
    // A pole: a non-positive integer in [a, b].
    let k = a.ceil();
    if k <= 0.0 && k <= b {
        if a == b {
            return (Interval::EMPTY, false);
        }
        return (Interval::ENTIRE, false);
    }
    if a == b {
        return (gamma_pt(a), true);
    }
    if a > 0.0 {
        if b == INF {
            let lo = if a >= GAMMA_ARGMIN_HI {
                gamma_pt(a).lo()
            } else {
                GAMMA_MIN
            };
            return (Interval::new(lo, INF), true);
        }
        let (ga, gb) = (gamma_pt(a), gamma_pt(b));
        let hi = ga.hi().max(gb.hi());
        let lo = if b <= GAMMA_ARGMIN_LO {
            gb.lo()
        } else if a >= GAMMA_ARGMIN_HI {
            ga.lo()
        } else {
            GAMMA_MIN
        };
        return (Interval::new(lo.min(hi), hi), true);
    }
    if a == -INF {
        return (Interval::ENTIRE, false);
    }
    // [a, b] inside (k − 1, k) for an integer k ≤ 0 (|a| < 2⁵³: beyond,
    // every double is an integer).
    let (ga, gb) = (gamma_pt(a), gamma_pt(b));
    let mag = |i: Interval| {
        (
            i.lo().abs().min(i.hi().abs()),
            i.lo().abs().max(i.hi().abs()),
        )
    };
    let (_, ma) = mag(ga);
    let (_, mb) = mag(gb);
    let hi = ma.max(mb);
    // max |sin πx| on [a, b]: 1 if it holds the half-integer, else at an end.
    let mid = k - 0.5;
    let sin_max = if a <= mid && mid <= b {
        1.0
    } else {
        cm::sinpi(a)
            .abs()
            .max(cm::sinpi(b).abs())
            .next_up()
            .min(1.0)
    };
    // max Γ(1 − x) over [1 − b, 1 − a] ⊂ (1, ∞), its ends rounded out.
    let (u0, u1) = ((1.0 - b).next_down().max(1.0), (1.0 - a).next_up());
    let g_max = gamma_pt(u0).hi().max(gamma_pt(u1).hi());
    let lo = if g_max.is_finite() {
        let p = pi().lo();
        let den = mul_up(sin_max, g_max);
        if den > 0.0 && den.is_finite() {
            (p / den).next_down().max(0.0)
        } else {
            0.0
        }
    } else {
        0.0
    };
    let lo = lo.min(hi);
    // (−1, 0) negative, (−2, −1) positive, …
    let negative = (k as i64).rem_euclid(2) == 0;
    let iv = if negative {
        Interval::new(-hi, -lo)
    } else {
        Interval::new(lo, hi)
    };
    (iv, true)
}

/// n! = Γ(n + 1): exact for small natural numbers, else from [`gamma_iv`];
/// undefined at the negative integers.
pub fn factorial(x: &DecInterval) -> DecInterval {
    let iv = x.iv;
    // n + 1 exactly (−10⁻²⁰ + 1 rounds to 1, but (−10⁻²⁰)! isn't 0!).
    if iv.is_point() && (iv.lo() + 1.0) - 1.0 == iv.lo() {
        let v = iv.lo() + 1.0;
        if v == v.trunc() && v <= 0.0 {
            return DecInterval::result(Interval::EMPTY, Dec::Trv, &[x]);
        }
        if v == v.trunc() && v <= 23.0 {
            // Exact up to 22!.
            let mut r = 1.0;
            let mut k = 2.0;
            while k < v {
                r *= k;
                k += 1.0;
            }
            return DecInterval::result(Interval::point(r), Dec::Com, &[x]);
        }
    }
    if iv.is_empty() {
        return DecInterval::result(Interval::EMPTY, Dec::Trv, &[x]);
    }
    let (g, defined) = gamma_iv(iv + Interval::point(1.0));
    let r = DecInterval::result(g, if defined { Dec::Com } else { Dec::Trv }, &[x]);
    // Γ is never 0, and positive on (0, ∞).
    let pos = defined && g.lo() > 0.0;
    let neg = defined && g.hi() < 0.0;
    r.signs(pos, neg)
}

/// An exact count, enclosed by the doubles either side of it (a point
/// when it is one); beyond the doubles, [`f64::MAX`, +∞].
fn count_enclosure(c: crate::functions::Count) -> Interval {
    match c {
        crate::functions::Count::Exact(v) => {
            let (lo, hi) = v.enclose();
            Interval::new(lo, hi)
        }
        crate::functions::Count::Huge => Interval::new(f64::MAX, INF),
    }
}

/// n!! for an integer n ≥ −1, from a point argument only: the exact
/// integer, enclosed.
pub fn double_factorial(x: &DecInterval) -> DecInterval {
    let iv = x.iv;
    if iv.is_point() {
        let Some(c) = crate::functions::double_factorial_exact(iv.lo()) else {
            return DecInterval::result(Interval::EMPTY, Dec::Trv, &[x]);
        };
        // Defined only at isolated points: not continuous on any wider box.
        return DecInterval::result(count_enclosure(c), Dec::Com, &[x]);
    }
    DecInterval::result(Interval::ENTIRE, Dec::Trv, &[x])
}

/// nCr and nPr. Whole numbers n ≥ 0 and r, of any size, are counted
/// exactly, as the evaluator counts them (`functions::count_exact`: 0 for
/// r < 0 or r > n, a count past the doubles [`f64::MAX`, +∞]); other
/// arguments go through Γ.
pub fn ncr_npr(n: &DecInterval, r: &DecInterval, perm: bool) -> DecInterval {
    if n.iv.is_point()
        && r.iv.is_point()
        && let Some(c) = exact_count(n.iv.lo(), r.iv.lo(), perm)
    {
        return DecInterval::result(c, Dec::Com, &[n, r]);
    }
    if n.is_empty() || r.is_empty() {
        return DecInterval::result(Interval::EMPTY, Dec::Trv, &[n, r]);
    }
    if let Some(k) = small_count(r) {
        // nPr(n, k) = n(n − 1)…(n − k + 1), nCr(n, k) that over k!, for
        // every n but the negative integers (where `functions` has
        // Γ(n + 1) at a pole: undefined).
        let mut acc = Interval::point(1.0);
        for i in 0..k {
            acc = acc * (n.iv - Interval::point(i as f64));
        }
        if !perm {
            let mut f = Interval::point(1.0);
            for i in 2..=k {
                f = f * Interval::point(i as f64);
            }
            acc = acc / f;
        }
        let first = n.iv.lo().max(-1e300).ceil();
        let negative_integer = first <= n.iv.hi() && first <= -1.0;
        if negative_integer && n.iv.is_point() {
            // Undefined there, though the polynomial isn't: a hole.
            return DecInterval::result(Interval::EMPTY, Dec::Trv, &[n, r]);
        }
        return DecInterval::result(
            acc,
            if negative_integer { Dec::Trv } else { Dec::Com },
            &[n, r],
        );
    }
    // Γ(n+1) / (Γ(r+1)·Γ(n−r+1)) (nPr without Γ(r+1)), defined where no
    // Γ argument is at a pole.
    let one = Interval::point(1.0);
    let (gn, dn) = gamma_iv(n.iv + one);
    let (gd, dd) = gamma_iv(n.iv - r.iv + one);
    let (mut q, mut defined) = (gn / gd, dn && dd);
    if !perm {
        let (gr, dr) = gamma_iv(r.iv + one);
        q = q / gr;
        defined &= dr;
    }
    DecInterval::result(q, if defined { Dec::Com } else { Dec::Trv }, &[n, r])
}

/// nCr(n, r) (nPr, `perm`) of whole numbers n ≥ 0 and r, enclosed
/// ([`count_enclosure`]); `None` for other arguments. A count in big
/// integers costs microseconds, and one constant nCr(200, 100) is enclosed
/// over and over (every box of a certificate): the last few are kept.
fn exact_count(n: f64, r: f64, perm: bool) -> Option<Interval> {
    type Recent = std::cell::RefCell<Vec<((u64, u64, bool), Interval)>>;
    thread_local! {
        static RECENT: Recent = const { std::cell::RefCell::new(Vec::new()) };
    }
    let key = (n.to_bits(), r.to_bits(), perm);
    if let Some(iv) = RECENT.with(|c| c.borrow().iter().find(|e| e.0 == key).map(|e| e.1)) {
        return Some(iv);
    }
    let iv = count_enclosure(crate::functions::count_exact(n, r, perm)?);
    RECENT.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= 8 {
            c.remove(0);
        }
        c.push((key, iv));
    });
    Some(iv)
}

/// r as a count 0 ≤ k ≤ 60 when it is that integer exactly.
pub(crate) fn small_count(r: &DecInterval) -> Option<u32> {
    let v = r.iv.lo();
    (r.iv.is_point() && v == v.trunc() && (0.0..=60.0).contains(&v)).then_some(v as u32)
}
