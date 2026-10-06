//! Extended-range values, for telling a double's overflow and underflow
//! apart from a genuinely undefined point.
//!
//! A double holds magnitudes from about 10⁻³²⁴ to 10³⁰⁸. Beyond them a
//! defined value rounds to 0 or ±∞, and an operation on that rounding can
//! give NaN, which the evaluator otherwise keeps for "undefined": e^x/e^x is
//! ∞/∞ for x > 709.8, 0/e^x is 0/0 for x < −745.2, ln(e^x) is ln 0 and
//! (e^x)^−1 is 0^−1 there. A [`Wide`] keeps its binary exponent apart from
//! its mantissa, so those stay ordinary numbers, and an exact zero is only
//! ever a real one. Programs are evaluated this way only where the double
//! result is NaN (see `Program::eval`), and constants are folded this way.
//!
//! Where both operands and the result are ordinary doubles, every operation
//! rounds exactly as the double one does.

use crate::compile::{Fn1, Fn2};
use crate::dd;
use crate::functions as fns;
use core_math as cm;
use std::cmp::Ordering;
use std::f64::consts::{LN_2, LN_10};

/// ln 2 − `LN_2`, for exact argument reduction.
const LN2_LO: f64 = 2.319_046_813_846_299_6e-17;

/// 2⁵³: up to it every integer is a double, so a binary exponent is exact.
/// Beyond it a value's exponent is itself rounded and its mantissa
/// meaningless: such a value is "lost" (see [`Wide::lost`]). It is still
/// certainly beyond a double's range, but nothing that would bring it back
/// into range (a logarithm, a root, a ratio of two of them) can be known.
const TWO53: f64 = 9_007_199_254_740_992.0;

/// Exponents within which m·2^e is an ordinary double, with room for a
/// simple result of it to stay one.
const NORMAL: f64 = 1000.0;

/// A real number in extended range, or why there isn't one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Wide {
    /// m·2^e with 1 ≤ |m| < 2 and e an integer, or m = ±0 and e = 0. An
    /// infinite e is a magnitude beyond even this range (e^(e^1000), or its
    /// reciprocal), whose sign is still known.
    Val(f64, f64),
    /// Undefined in the reals: 1/0, ln 0, √−1.
    Undef,
    /// A value this can't resolve (two magnitudes beyond the range that may
    /// cancel, the sine of a number too large for a double): not known to
    /// be defined, nor known not to be.
    Unknown,
}

/// 2^k for an integer k in [−1022, 1023].
fn pow2(k: i32) -> f64 {
    f64::from_bits(((k + 1023) as u64) << 52)
}

/// v = m·2^e with 1 ≤ |m| < 2, for a finite nonzero v.
fn frexp(v: f64) -> (f64, f64) {
    let (v, adj) = if v.abs() < f64::MIN_POSITIVE {
        (v * pow2(64), -64)
    } else {
        (v, 0)
    };
    let bits = v.to_bits();
    let e = ((bits >> 52) & 0x7ff) as i32 - 1023;
    let m = f64::from_bits((bits & !(0x7ff << 52)) | (1023 << 52));
    (m, f64::from(e + adj))
}

/// m·2^e as a double, rounded once (to 0 or ±∞ beyond its range).
fn ldexp(m: f64, e: f64) -> f64 {
    if e > 1023.0 {
        m * f64::INFINITY
    } else if e >= -1022.0 {
        m * pow2(e as i32)
    } else if e >= -1100.0 {
        // Subnormal: scaled exactly first, rounded by the second product.
        m * pow2(e as i32 + 600) * pow2(-600)
    } else {
        m * 0.0
    }
}

/// `r` as a value if it is an ordinary double that can't be the overflow or
/// underflow of a value beyond the range.
fn safe(r: f64) -> Option<Wide> {
    (r.is_finite() && r != 0.0 && r.abs() >= pow2(-1000) && r.abs() <= pow2(1000))
        .then(|| Wide::new(r))
}

/// The result of an operation on operands that aren't both values:
/// undefined if either is undefined.
fn other(a: Wide, b: Wide) -> Wide {
    if a == Wide::Undef || b == Wide::Undef {
        Wide::Undef
    } else {
        Wide::Unknown
    }
}

/// ln|v| for v = m·2^e ≠ 0, as a double (±∞ only for an infinite e).
fn ln_abs(m: f64, e: f64) -> f64 {
    if e.abs() <= NORMAL {
        cm::log(ldexp(m.abs(), e))
    } else {
        ln_dd(m, e).hi
    }
}

/// ln|v| for v = m·2^e ≠ 0, in double-double (±∞ for an infinite e).
fn ln_dd(m: f64, e: f64) -> dd::Dd {
    if e.is_finite() {
        dd::ln(m.abs()).add(dd::LN2.mul_f(e))
    } else {
        dd::Dd::of(e)
    }
}

/// |m·2^e|^t for m ≠ 0, through logarithms: e^(t·ln|m·2^e|), with that
/// exponent in double-double (see [`crate::dd`]) so the result keeps a
/// double's precision however far out of range it is, up to 2⁵³ binades.
/// Beyond, only its size is kept (a lost value); and a lost base gives
/// nothing that would land back within 2⁵³ binades.
fn pow_mag(m: f64, e: f64, t: dd::Dd) -> Wide {
    let p = t.hi * e;
    if !p.is_finite() || p.abs() >= TWO53 || !e.is_finite() || e.abs() > TWO53 {
        let n = p + t.hi * cm::log2(m.abs());
        return if n.is_nan() {
            Wide::Unknown
        } else if n.is_infinite() || n.abs() > TWO53 {
            Wide::Val(1.0, n.round())
        } else {
            Wide::Unknown
        };
    }
    let l = dd::ln(m.abs()).add(dd::LN2.mul_f(e));
    let (r, n) = dd::exp(l.mul(t));
    Wide::norm(r, n)
}

/// `out`, unless one of `inputs` is lost (beyond 2⁵³ binades, see
/// [`TWO53`]) and `out` is not: a result back within reach would carry the
/// lost value's meaningless mantissa or exponent, so it is unknown.
fn unless_lost(inputs: &[Wide], out: Wide) -> Wide {
    if inputs.iter().any(|w| w.lost()) && !out.lost() && !out.beyond() && !out.is_zero() {
        Wide::Unknown
    } else {
        out
    }
}

impl Wide {
    pub(crate) const ONE: Wide = Wide::Val(1.0, 0.0);
    const HUGE: Wide = Wide::Val(1.0, f64::INFINITY);

    /// A double as a value: NaN is undefined, and ±∞ a magnitude beyond
    /// the range (a double only reaches it by overflowing).
    pub(crate) fn new(v: f64) -> Wide {
        if v.is_nan() {
            Wide::Undef
        } else if v == 0.0 {
            Wide::Val(v, 0.0)
        } else if v.is_infinite() {
            Wide::Val(v.signum(), f64::INFINITY)
        } else {
            let (m, e) = frexp(v);
            Wide::Val(m, e)
        }
    }

    /// m·2^e normalised. A NaN exponent (∞ − ∞) can't be resolved.
    fn norm(m: f64, e: f64) -> Wide {
        if m == 0.0 {
            return Wide::Val(m, 0.0);
        }
        if m.is_nan() || e.is_nan() {
            return Wide::Unknown;
        }
        if m.is_infinite() {
            return Wide::Val(m.signum(), f64::INFINITY);
        }
        let (m, k) = frexp(m);
        Wide::Val(m, e + k)
    }

    /// The nearest double: 0 or ±∞ beyond its range, NaN unless a value.
    pub(crate) fn to_f64(self) -> f64 {
        match self {
            Wide::Val(m, e) => ldexp(m, e),
            _ => f64::NAN,
        }
    }

    /// True for an exact zero (never the underflow of a nonzero value).
    pub(crate) fn is_zero(self) -> bool {
        matches!(self, Wide::Val(m, _) if m == 0.0)
    }

    /// A value beyond 2⁵³ binades: its size is known to be beyond a
    /// double, but its exponent is rounded and its mantissa made up.
    pub(crate) fn lost(self) -> bool {
        matches!(self, Wide::Val(m, e) if m != 0.0 && e.is_finite() && e.abs() > TWO53)
    }

    /// A magnitude beyond even the extended range (an infinite exponent).
    fn beyond(self) -> bool {
        matches!(self, Wide::Val(m, e) if m != 0.0 && e.is_infinite())
    }

    /// The double, when it is an ordinary one with room to spare.
    fn normal(self) -> Option<f64> {
        match self {
            Wide::Val(m, e) if m == 0.0 || e.abs() <= NORMAL => Some(ldexp(m, e)),
            _ => None,
        }
    }

    pub(crate) fn neg(self) -> Wide {
        match self {
            Wide::Val(m, e) => Wide::Val(-m, e),
            w => w,
        }
    }

    pub(crate) fn add(self, o: Wide) -> Wide {
        // A lost value far below a known one doesn't change it (e^(−10¹⁶)
        // + 1 is 1); otherwise a sum back in range depends on what was lost.
        if let (Wide::Val(a, ea), Wide::Val(b, eb)) = (self, o) {
            match (self.lost(), o.lost()) {
                (true, false) if b != 0.0 && ea < eb - 64.0 => return o,
                (false, true) if a != 0.0 && eb < ea - 64.0 => return self,
                _ => {}
            }
        }
        unless_lost(&[self, o], self.add_inner(o))
    }

    fn add_inner(self, o: Wide) -> Wide {
        let (Wide::Val(a, ea), Wide::Val(b, eb)) = (self, o) else {
            return other(self, o);
        };
        if a == 0.0 || b == 0.0 {
            return match (a == 0.0, b == 0.0) {
                (true, true) => Wide::Val(a + b, 0.0),
                (true, false) => o,
                _ => self,
            };
        }
        if ea.is_infinite() && ea == eb {
            // Two magnitudes beyond the range: the sum's size is known only
            // if they can't cancel.
            return if a.signum() == b.signum() {
                Wide::Val(a.signum(), ea)
            } else {
                Wide::Unknown
            };
        }
        let (hi, eh, lo, el) = if ea >= eb {
            (a, ea, b, eb)
        } else {
            (b, eb, a, ea)
        };
        let d = eh - el;
        if d > 64.0 {
            return Wide::Val(hi, eh);
        }
        Wide::norm(hi + lo * pow2(-(d as i32)), eh)
    }

    pub(crate) fn sub(self, o: Wide) -> Wide {
        self.add(o.neg())
    }

    pub(crate) fn mul(self, o: Wide) -> Wide {
        let (Wide::Val(a, ea), Wide::Val(b, eb)) = (self, o) else {
            return other(self, o);
        };
        if a == 0.0 || b == 0.0 {
            // Exactly 0, even beside a magnitude beyond the range.
            return Wide::Val(a * b, 0.0);
        }
        unless_lost(&[self, o], Wide::norm(a * b, ea + eb))
    }

    /// Division, undefined by an exact 0 (as `functions::div`).
    pub(crate) fn div(self, o: Wide) -> Wide {
        unless_lost(&[self, o], self.div_inner(o))
    }

    fn div_inner(self, o: Wide) -> Wide {
        match (self, o) {
            (_, Wide::Val(b, _)) if b == 0.0 => Wide::Undef,
            (Wide::Val(a, ea), Wide::Val(b, eb)) => {
                if a == 0.0 {
                    Wide::Val(a / b, 0.0)
                } else {
                    Wide::norm(a / b, ea - eb)
                }
            }
            _ => other(self, o),
        }
    }

    /// The order of two values; None if either isn't one or they can't be
    /// told apart.
    fn cmp(self, o: Wide) -> Option<Ordering> {
        match self.sub(o) {
            Wide::Val(m, _) => m.partial_cmp(&0.0),
            _ => None,
        }
    }

    pub(crate) fn exp(self) -> Wide {
        let Wide::Val(m, e) = self else {
            return self;
        };
        if m == 0.0 || e < -NORMAL {
            return Wide::ONE;
        }
        let v = ldexp(m, e);
        if (-708.0..=709.0).contains(&v) {
            return Wide::new(cm::exp(v));
        }
        let n = (v / LN_2).round();
        if n.is_nan() || n.abs() >= TWO53 {
            // Beyond 2⁵³ binades (or v beyond a double, and so n): only the
            // size is kept, a lost value.
            return Wide::Val(1.0, n);
        }
        let r = (-n).mul_add(LN_2, v);
        let r = (-n).mul_add(LN2_LO, r);
        Wide::norm(cm::exp(r), n)
    }

    /// ln, undefined at 0 (as `functions::ln`) and for negative numbers.
    pub(crate) fn ln(self) -> Wide {
        unless_lost(
            &[self],
            match self {
                Wide::Val(m, e) if m > 0.0 => Wide::new(ln_abs(m, e)),
                Wide::Val(..) => Wide::Undef,
                w => w,
            },
        )
    }

    pub(crate) fn log10(self) -> Wide {
        unless_lost(&[self], self.log10_inner())
    }

    fn log10_inner(self) -> Wide {
        match self {
            Wide::Val(m, e) if m > 0.0 => Wide::new(if e.abs() <= NORMAL {
                cm::log10(ldexp(m, e))
            } else {
                ln_abs(m, e) / LN_10
            }),
            Wide::Val(..) => Wide::Undef,
            w => w,
        }
    }

    pub(crate) fn sqrt(self) -> Wide {
        unless_lost(&[self], self.sqrt_inner())
    }

    fn sqrt_inner(self) -> Wide {
        match self {
            Wide::Val(m, _) if m < 0.0 => Wide::Undef,
            Wide::Val(m, e) => {
                if m == 0.0 || e.abs() <= NORMAL {
                    return Wide::new(ldexp(m, e).sqrt());
                }
                // (Every double from 2⁵³ on is even.)
                if e.is_finite() && e % 2.0 != 0.0 {
                    Wide::norm((2.0 * m).sqrt(), (e - 1.0) / 2.0)
                } else {
                    Wide::norm(m.sqrt(), e / 2.0)
                }
            }
            w => w,
        }
    }

    pub(crate) fn cbrt(self) -> Wide {
        unless_lost(&[self], self.cbrt_inner())
    }

    fn cbrt_inner(self) -> Wide {
        match self {
            Wide::Val(m, e) => {
                if m == 0.0 || e.abs() <= NORMAL {
                    return Wide::new(cm::cbrt(ldexp(m, e)));
                }
                if e.abs() <= TWO53 {
                    let r = e.rem_euclid(3.0);
                    Wide::norm(cm::cbrt(m * f64::from(1 << r as i32)), (e - r) / 3.0)
                } else {
                    Wide::norm(cm::cbrt(m), e / 3.0)
                }
            }
            w => w,
        }
    }
}

/// `b^t` (as `functions::pow`: 0 only to a positive power, b⁰ = 1 for any
/// other b, a negative base only to an integer power).
pub(crate) fn pow(b: Wide, t: Wide) -> Wide {
    let (Wide::Val(bm, be), Wide::Val(tm, te)) = (b, t) else {
        return other(b, t);
    };
    if bm == 0.0 {
        return if tm <= 0.0 {
            Wide::Undef
        } else {
            Wide::Val(0.0, 0.0)
        };
    }
    if tm == 0.0 {
        return Wide::ONE;
    }
    if let (Some(bv), Some(tv)) = (b.normal(), t.normal()) {
        let r = fns::pow(bv, tv);
        if r.is_nan() {
            return Wide::Undef;
        }
        if let Some(w) = safe(r) {
            return w;
        }
    }
    let tv = ldexp(tm, te);
    let mut odd = false;
    if bm < 0.0 {
        // Integer powers only; every double from 2⁵³ on is an even integer.
        if te < 0.0 || (te < 53.0 && tv.fract() != 0.0) {
            return Wide::Undef;
        }
        odd = te < 53.0 && tv % 2.0 != 0.0;
    }
    if tv.is_infinite() {
        // A power beyond a double: |b| = 1 stays 1, anything else leaves
        // the range one way or the other.
        let lb = be + cm::log2(bm.abs());
        return if lb == 0.0 {
            Wide::ONE
        } else {
            Wide::Val(1.0, (lb * tm).signum() * f64::INFINITY)
        };
    }
    let r = pow_mag(bm, be, dd::Dd::of(tv));
    if odd { r.neg() } else { r }
}

/// `b^t` for a constant exponent exactly an odd integer that `t` isn't (as
/// `functions::pow_odd`): sign(b)·|b|^t.
pub(crate) fn pow_odd(b: Wide, t: Wide) -> Wide {
    match b {
        Wide::Val(bm, be) if bm < 0.0 => pow(Wide::Val(-bm, be), t).neg(),
        _ => pow(b, t),
    }
}

/// `b^t` for a constant exponent beyond the doubles whose exact value
/// isn't carried (as `functions::pow_beyond`): a negative base's power is
/// unknown, its parity (or integrality) not known.
pub(crate) fn pow_beyond(b: Wide, t: Wide) -> Wide {
    match (b, t) {
        (Wide::Undef, _) | (_, Wide::Undef) => Wide::Undef,
        (Wide::Val(bm, _), _) if bm < 0.0 => Wide::Unknown,
        _ => pow(b, t),
    }
}

/// `b^t` for an exponent that varies with x (as `functions::pow_var`:
/// positive bases only, or 0 to a positive power).
pub(crate) fn pow_var(b: Wide, t: Wide) -> Wide {
    match (b, t) {
        // Whatever the exponent is (or whether it is at all).
        (Wide::Val(bm, _), _) if bm < 0.0 => Wide::Undef,
        (Wide::Val(bm, _), Wide::Val(tm, _)) if bm == 0.0 => {
            if tm > 0.0 {
                Wide::Val(0.0, 0.0)
            } else {
                Wide::Undef
            }
        }
        _ => pow(b, t),
    }
}

/// `b^n` for an integer n (as `functions::pow_int`).
pub(crate) fn powi(b: Wide, n: i32) -> Wide {
    let Wide::Val(m, e) = b else {
        return b;
    };
    if m == 0.0 {
        return if n <= 0 {
            Wide::Undef
        } else {
            Wide::new(fns::pow_int(m, n))
        };
    }
    if n == 0 {
        return Wide::ONE;
    }
    if let Some(v) = b.normal()
        && let Some(w) = safe(fns::pow_int(v, n))
    {
        return w;
    }
    let r = pow_mag(m, e, dd::Dd::of(f64::from(n)));
    if m < 0.0 && n % 2 != 0 { r.neg() } else { r }
}

/// `b^(p/q)` with real-root semantics (as `functions::pow_rational`).
pub(crate) fn pow_rational(b: Wide, p: i32, q: i32) -> Wide {
    let Wide::Val(m, e) = b else {
        return b;
    };
    if m == 0.0 {
        return if p <= 0 {
            Wide::Undef
        } else {
            Wide::new(fns::pow_rational(m, p, q))
        };
    }
    if let Some(v) = b.normal() {
        let r = fns::pow_rational(v, p, q);
        if r.is_nan() {
            return Wide::Undef;
        }
        if let Some(w) = safe(r) {
            return w;
        }
    }
    if m < 0.0 && q % 2 == 0 {
        return Wide::Undef;
    }
    // (p/q itself in double-double: 1/3 as a double is off by 2⁻⁵⁶, which
    // times an exponent of hundreds is many ulps.)
    let r = pow_mag(m, e, dd::Dd::of(f64::from(p)).div_f(f64::from(q)));
    if m < 0.0 && p % 2 != 0 { r.neg() } else { r }
}

/// `b^(p/q)` for a written ratio past the small fast path (as
/// `functions::pow_ratio`): sign(b)ᵖ·|b|^(p/q), a negative b only for an
/// odd q, 0 only to a positive power; p/q in double-double, so the
/// magnitude keeps a double's precision however large p/q is.
pub(crate) fn pow_ratio(b: Wide, r: crate::compile::RatioPow) -> Wide {
    let Wide::Val(m, e) = b else {
        return b;
    };
    if m == 0.0 {
        return if r.neg {
            Wide::Undef
        } else {
            Wide::Val(0.0, 0.0)
        };
    }
    if m < 0.0 && !r.odd_q {
        return Wide::Undef;
    }
    let mag = if r.e.hi.is_infinite() {
        // p/q beyond a double: |b| = 1 stays 1, anything else leaves the
        // range one way or the other.
        let lb = e + cm::log2(m.abs());
        if lb == 0.0 {
            Wide::ONE
        } else {
            Wide::Val(1.0, (lb * r.e.hi).signum() * f64::INFINITY)
        }
    } else {
        pow_mag(m.abs(), e, r.e)
    };
    if m < 0.0 && r.odd_p { mag.neg() } else { mag }
}

/// A built-in function of one argument (as `Fn1::apply`).
pub(crate) fn apply1(f: Fn1, w: Wide) -> Wide {
    use Fn1::*;
    let Wide::Val(m, e) = w else {
        return w;
    };
    // Below and above the range where f is computed as a double: near 0,
    // the functions that vanish there go as their slope times w, and their
    // reciprocals as its reciprocal; far out, the logarithmic ones grow as
    // ln 2|w|, the reciprocal ones as 1/w.
    let tiny = m != 0.0 && e < -NORMAL;
    let huge = e > NORMAL;
    let v = ldexp(m, e);
    let k = |u: crate::TrigUnit| Wide::new(u.to_radians_factor());
    // (Logarithmic in w: nothing to be known of a lost w.)
    let lost = w.lost();
    let ln2w = |s: f64| {
        if lost {
            Wide::Unknown
        } else {
            Wide::new(s * (LN_2 + ln_abs(m, e)))
        }
    };
    let ln2_w = |s: f64| {
        if lost {
            Wide::Unknown
        } else {
            Wide::new(s * (LN_2 - ln_abs(m, e)))
        }
    };
    match f {
        Sin(u) | Tan(u) if tiny => w.mul(k(u)),
        Csc(u) | Cot(u) if tiny => Wide::ONE.div(w.mul(k(u))),
        Asin(u) | Atan(u) if tiny => w.div(k(u)),
        Sinh | Tanh | Asinh | Atanh if tiny => w,
        Csch | Coth if tiny => Wide::ONE.div(w),
        Asech if tiny => {
            if m > 0.0 {
                ln2_w(1.0)
            } else {
                Wide::Undef
            }
        }
        Acsch if tiny => ln2_w(m.signum()),
        // Beyond a double, where in its period the argument falls is lost.
        Sin(_) | Cos(_) | Tan(_) | Sec(_) | Csc(_) | Cot(_) if v.is_infinite() => Wide::Unknown,
        Acsc(u) if huge => Wide::ONE.div(w).div(k(u)),
        Acot(u) if huge && m > 0.0 => Wide::ONE.div(w).div(k(u)),
        Acsch | Acoth if huge => Wide::ONE.div(w),
        Asinh if huge => ln2w(m.signum()),
        Acosh if huge && m > 0.0 => ln2w(1.0),
        Sqrt => w.sqrt(),
        Cbrt => w.cbrt(),
        Ln => w.ln(),
        Log10 => w.log10(),
        Exp => w.exp(),
        Abs => Wide::Val(m.abs(), e),
        Sign => Wide::new(fns::sign(m)),
        Floor if tiny => Wide::new(if m < 0.0 { -1.0 } else { 0.0 }),
        Ceil if tiny => Wide::new(if m > 0.0 { 1.0 } else { -0.0 }),
        Round if tiny => Wide::new(0.0 * m),
        // Already an integer.
        Floor | Ceil | Round if huge => w,
        Sinh | Cosh | Sech | Csch if v.abs() > 700.0 => {
            // e^|x| is beyond a double: sinh, cosh ≈ ±e^|x|/2, and their
            // reciprocals ≈ ±2e^−|x|.
            let a = Wide::Val(m.abs(), e);
            let r = if matches!(f, Sinh | Cosh) {
                a.exp().mul(Wide::new(0.5))
            } else {
                a.neg().exp().mul(Wide::new(2.0))
            };
            if matches!(f, Sinh | Csch) && m < 0.0 {
                r.neg()
            } else {
                r
            }
        }
        Factorial | DoubleFactorial => {
            if tiny {
                // Γ(1 + w) ≈ 1; w!! needs an integer.
                return if f == Factorial {
                    Wide::ONE
                } else {
                    Wide::Undef
                };
            }
            if huge {
                // An integer: a pole if negative, beyond the range if not.
                return if m < 0.0 { Wide::Undef } else { Wide::HUGE };
            }
            let r = f.apply(v);
            if r.is_nan() {
                return Wide::Undef;
            }
            if let Some(s) = safe(r) {
                return s;
            }
            match f {
                Factorial if v > 0.0 => Wide::new(fns::ln_gamma(v + 1.0)).exp(),
                _ if r == f64::INFINITY => Wide::HUGE,
                _ => Wide::Unknown,
            }
        }
        _ => Wide::new(f.apply(v)),
    }
}

/// A built-in function of two arguments (as `Fn2::apply`).
pub(crate) fn apply2(f: Fn2, a: Wide, b: Wide) -> Wide {
    let (Wide::Val(am, ae), Wide::Val(bm, be)) = (a, b) else {
        return other(a, b);
    };
    let normal = a.normal().zip(b.normal());
    match f {
        Fn2::Root => {
            let Some(n) = b.normal() else {
                return Wide::Unknown;
            };
            if n == 0.0 {
                return Wide::Undef;
            }
            if am == 0.0 {
                return if n < 0.0 {
                    Wide::Undef
                } else {
                    Wide::Val(0.0, 0.0)
                };
            }
            if let Some(x) = a.normal() {
                let r = fns::root(x, n);
                if r.is_nan() {
                    return Wide::Undef;
                }
                if let Some(w) = safe(r) {
                    return w;
                }
            }
            let odd = fns::is_odd_integer(n);
            if am < 0.0 && !odd {
                return Wide::Undef;
            }
            let r = pow_mag(am, ae, dd::Dd::of(1.0).div_f(n));
            if am < 0.0 { r.neg() } else { r }
        }
        Fn2::LogBase => {
            if let Some((base, x)) = normal {
                return Wide::new(fns::log_base(base, x));
            }
            if am <= 0.0 || bm <= 0.0 || a == Wide::ONE {
                return Wide::Undef;
            }
            if a.lost() || b.lost() {
                return Wide::Unknown;
            }
            let (lx, lb) = (ln_dd(bm, be), ln_dd(am, ae));
            let r = if lx.hi.is_finite() && lb.hi.is_finite() {
                lx.div(lb).hi
            } else {
                lx.hi / lb.hi
            };
            if r.is_nan() {
                Wide::Unknown
            } else {
                Wide::new(r)
            }
        }
        Fn2::Mod => {
            if bm == 0.0 {
                return Wide::Undef;
            }
            if let Some((x, y)) = normal {
                return Wide::new(fns::modulo(x, y));
            }
            if am == 0.0 {
                return a;
            }
            // Only |a| < |b| is resolvable: a, or a + b to take b's sign.
            match Wide::Val(am.abs(), ae).cmp(Wide::Val(bm.abs(), be)) {
                Some(Ordering::Less) if am.signum() == bm.signum() => a,
                Some(Ordering::Less) => a.add(b),
                _ => Wide::Unknown,
            }
        }
        Fn2::NCr | Fn2::NPr => match normal {
            Some((x, y)) => Wide::new(f.apply(x, y)),
            None => Wide::Unknown,
        },
        Fn2::Min | Fn2::Max => match a.cmp(b) {
            Some(Ordering::Greater) => {
                if f == Fn2::Min {
                    b
                } else {
                    a
                }
            }
            Some(_) => {
                if f == Fn2::Min {
                    a
                } else {
                    b
                }
            }
            None => Wide::Unknown,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(v: f64) -> Wide {
        Wide::new(v)
    }

    #[test]
    fn round_trips_and_rounds_like_doubles() {
        for v in [1.0, -3.5, 1e-310, 5e-324, 1e308, -0.0, 0.1, 7.0 / 3.0] {
            assert_eq!(w(v).to_f64().to_bits(), v.to_bits(), "{v}");
        }
        let (a, b) = (0.1, 0.2);
        assert_eq!(w(a).add(w(b)).to_f64(), a + b);
        assert_eq!(w(a).sub(w(b)).to_f64(), a - b);
        assert_eq!(w(a).mul(w(b)).to_f64(), a * b);
        assert_eq!(w(a).div(w(b)).to_f64(), a / b);
        assert_eq!(w(1e-300).mul(w(1e-300)).mul(w(1e300)).to_f64(), 1e-300);
        assert!(w(3.0).sub(w(3.0)).is_zero());
    }

    #[test]
    fn beyond_the_range_values_stay_values() {
        let big = w(1000.0).exp();
        let small = w(-1000.0).exp();
        assert_eq!(big.to_f64(), f64::INFINITY);
        assert_eq!(small.to_f64(), 0.0);
        assert!(!small.is_zero());
        assert_eq!(big.div(big), Wide::ONE);
        assert_eq!(small.div(small), Wide::ONE);
        assert!((big.mul(small).to_f64() - 1.0).abs() < 1e-12);
        assert!(w(0.0).div(small).is_zero());
        assert!((small.ln().to_f64() + 1000.0).abs() < 1e-9);
        assert!((powi(small, -1).ln().to_f64() - 1000.0).abs() < 1e-9);
        assert!(
            (apply1(Fn1::Atan(crate::TrigUnit::Radians), powi(small, -1)).to_f64()
                - std::f64::consts::FRAC_PI_2)
                .abs()
                < 1e-15
        );
        assert!((pow(big, w(0.001)).to_f64() - 1f64.exp()).abs() < 1e-12);
        assert!((small.sqrt().ln().to_f64() + 500.0).abs() < 1e-9);
        assert!((small.cbrt().ln().to_f64() + 1000.0 / 3.0).abs() < 1e-9);
        // Beyond even the extended range, the sign is kept.
        let huger = big.exp();
        assert_eq!(huger.to_f64(), f64::INFINITY);
        assert_eq!(powi(huger, -1).to_f64(), 0.0);
        assert_eq!(huger.sub(huger), Wide::Unknown);
        assert_eq!(huger.div(huger), Wide::Unknown);
        assert_eq!(huger.add(huger).to_f64(), f64::INFINITY);
    }

    #[test]
    fn undefined_stays_undefined() {
        use crate::TrigUnit::Radians as R;
        let small = w(-1000.0).exp();
        let big = w(1000.0).exp();
        assert_eq!(w(1.0).div(w(0.0)), Wide::Undef);
        assert_eq!(w(0.0).ln(), Wide::Undef);
        assert_eq!(w(-1.0).sqrt(), Wide::Undef);
        assert_eq!(powi(w(0.0), -1), Wide::Undef);
        assert_eq!(pow(w(0.0), w(-0.5)), Wide::Undef);
        assert_eq!(pow(w(-2.0), w(0.5)), Wide::Undef);
        assert_eq!(pow(small.neg(), w(0.5)), Wide::Undef);
        assert_eq!(pow_rational(big.neg(), 1, 2), Wide::Undef);
        assert_eq!(apply1(Fn1::Acosh, small), Wide::Undef);
        assert_eq!(apply1(Fn1::Asin(R), big), Wide::Undef);
        assert_eq!(apply1(Fn1::Acos(R), big), Wide::Undef);
        assert_eq!(apply1(Fn1::Atanh, big), Wide::Undef);
        assert_eq!(apply1(Fn1::Asec(R), small), Wide::Undef);
        assert_eq!(apply1(Fn1::Acsc(R), small), Wide::Undef);
        assert_eq!(apply1(Fn1::Acoth, small), Wide::Undef);
        assert_eq!(apply1(Fn1::Atanh, w(1.0)), Wide::Undef);
        assert_eq!(apply1(Fn1::Ln, small.neg()), Wide::Undef);
        assert_eq!(apply1(Fn1::DoubleFactorial, small), Wide::Undef);
        assert_eq!(apply2(Fn2::Root, w(8.0), w(0.0)), Wide::Undef);
        assert_eq!(apply2(Fn2::Root, small.neg(), w(2.0)), Wide::Undef);
        assert_eq!(apply2(Fn2::LogBase, w(1.0), small), Wide::Undef);
        assert_eq!(apply2(Fn2::LogBase, w(-2.0), big), Wide::Undef);
        assert_eq!(apply2(Fn2::Mod, big, w(0.0)), Wide::Undef);
        assert_eq!(w(f64::NAN).add(big), Wide::Undef);
        assert_eq!(apply1(Fn1::Sin(R), big), Wide::Unknown);
    }

    #[test]
    fn functions_near_zero_and_far_out() {
        use crate::TrigUnit::{Degrees, Radians as R};
        let small = w(-1000.0).exp();
        let big = w(1000.0).exp();
        let ratio = |a: Wide, b: Wide| a.div(b).to_f64();
        assert!((ratio(apply1(Fn1::Sin(R), small), small) - 1.0).abs() < 1e-15);
        let deg = Degrees.to_radians_factor();
        assert!((ratio(apply1(Fn1::Tan(Degrees), small), small) - deg).abs() < 1e-15);
        assert!((ratio(apply1(Fn1::Csc(R), small), powi(small, -1)) - 1.0).abs() < 1e-15);
        assert_eq!(apply1(Fn1::Cos(R), small), Wide::ONE);
        assert_eq!(apply1(Fn1::Floor, small.neg()).to_f64(), -1.0);
        assert_eq!(apply1(Fn1::Ceil, small).to_f64(), 1.0);
        assert!((apply1(Fn1::Asinh, big).to_f64() - (1000.0 + LN_2)).abs() < 1e-9);
        assert!((apply1(Fn1::Acosh, big).to_f64() - (1000.0 + LN_2)).abs() < 1e-9);
        assert!((apply1(Fn1::Asech, small).to_f64() - (1000.0 + LN_2)).abs() < 1e-9);
        assert!((ratio(apply1(Fn1::Acsch, big), powi(big, -1)) - 1.0).abs() < 1e-12);
        assert!((ratio(apply1(Fn1::Cosh, w(1000.0)), big) - 0.5).abs() < 1e-12);
        assert!((ratio(apply1(Fn1::Sech, w(-1000.0)), small) - 2.0).abs() < 1e-12);
        assert!((apply1(Fn1::Factorial, w(200.0)).ln().to_f64() - 863.231_987).abs() < 1e-5);
        assert!((apply2(Fn2::LogBase, w(10.0), big).to_f64() - 1000.0 / LN_10).abs() < 1e-9);
        assert_eq!(apply2(Fn2::Max, small, w(0.0)), small);
        assert_eq!(apply2(Fn2::Min, big, w(3.0)).to_f64(), 3.0);
        assert_eq!(apply2(Fn2::Mod, small.neg(), w(3.0)).to_f64(), 3.0);
        assert!((apply2(Fn2::Root, big, w(3.0)).ln().to_f64() - 1000.0 / 3.0).abs() < 1e-9);
        assert!(
            (apply2(Fn2::Root, big.neg(), w(3.0)).neg().ln().to_f64() - 1000.0 / 3.0).abs() < 1e-9
        );
        assert_eq!(pow(w(-1.0), big), Wide::ONE);
        assert_eq!(pow(w(2.0), big).to_f64(), f64::INFINITY);
        assert_eq!(pow(w(0.5), big).to_f64(), 0.0);
        assert!(!pow(w(0.5), big).is_zero());
    }
}
