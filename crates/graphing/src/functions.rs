//! Real-valued implementations of the built-in functions.
//!
//! Everything works on the real number field (like the original graphing
//! engine's `EvalNumberField::Real`): out-of-domain inputs produce NaN and
//! poles produce ±∞, which the plotter treats as gaps.
//!
//! The elementary functions underneath are CORE-MATH's (`core_math`):
//! correctly rounded for every double, with no hardware-specific paths, so
//! a plot, a trace and the analysis panel compute the same bits on every
//! machine. Functions it lacks (sec, csc, cot, the reciprocal hyperbolics
//! and their inverses, degree and grad trigonometry) are built from its
//! functions here.

use std::f64::consts::PI;

use core_math as cm;

use crate::big::{Nat, Round};
use crate::dd::Dd;

/// Angle unit used by trigonometric functions
/// (`Graphing::EvalTrigUnitMode`). Hyperbolic functions are not affected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TrigUnit {
    /// Period of sin is 2π (default).
    #[default]
    Radians,
    /// Period of sin is 360.
    Degrees,
    /// Period of sin is 400.
    Grads,
}

impl TrigUnit {
    /// Size of a full turn in this unit.
    pub fn full_turn(self) -> f64 {
        match self {
            TrigUnit::Radians => 2.0 * PI,
            TrigUnit::Degrees => 360.0,
            TrigUnit::Grads => 400.0,
        }
    }

    /// Radians per unit.
    pub fn to_radians_factor(self) -> f64 {
        2.0 * PI / self.full_turn()
    }

    /// Integer code used by the original (`GraphingSettingsViewModel`:
    /// 1 = radians, 2 = degrees, 3 = gradians).
    pub fn code(self) -> i32 {
        match self {
            TrigUnit::Radians => 1,
            TrigUnit::Degrees => 2,
            TrigUnit::Grads => 3,
        }
    }

    /// Inverse of [`TrigUnit::code`].
    pub fn from_code(code: i32) -> Option<TrigUnit> {
        match code {
            1 => Some(TrigUnit::Radians),
            2 => Some(TrigUnit::Degrees),
            3 => Some(TrigUnit::Grads),
            _ => None,
        }
    }
}

/// sin and cos of an angle in the given unit, each rounded to a double. For
/// degrees/grads, exact quarter turns give exact 0/±1 so that e.g.
/// `tan(90)` is a pole and `sin(180)` is exactly 0.
#[inline]
pub fn sin_cos(x: f64, unit: TrigUnit) -> (f64, f64) {
    let (s, c, _, _) = sin_cos_zeros(x, unit);
    (s, c)
}

/// [`sin_cos`], and whether the sine and the cosine are exactly zero
/// rather than values too small for a double: only then is a reciprocal
/// undefined; otherwise it overflows to ±∞ (csc of 10⁻³²³ degrees).
#[inline]
fn sin_cos_zeros(x: f64, unit: TrigUnit) -> (f64, f64, bool, bool) {
    match unit {
        TrigUnit::Radians => {
            // No double but 0 is a multiple of π/2.
            let (s, c) = cm::sincos(x);
            (s, c, x == 0.0, false)
        }
        _ => {
            if !x.is_finite() {
                return (f64::NAN, f64::NAN, false, false);
            }
            // Reduced as |x| mod a turn, which `%` does exactly, and the
            // sign put back (sin is odd, cos even): reducing a tiny negative
            // x into [0, turn) would round it up to a whole turn, a zero sine.
            // Then to the nearest quarter turn, also exactly (|d| ≤ ⅛ turn,
            // on r's grid): converting r itself to radians would round away
            // the small sine of an angle just short of 180° or the cosine
            // near 90° (sin 179.99999999999997° is 4.96·10⁻¹⁶, not 5.67).
            let turn = unit.full_turn();
            let quarter = turn / 4.0;
            let half = turn / 2.0;
            let r = x.abs() % turn;
            // (r ≥ 0: adding ½ and truncating rounds, without a libm call.)
            let qi = (r / quarter + 0.5) as i64;
            let d = r - qi as f64 * quarter;
            let odd = qi % 2 == 1;
            let (s, c, sz, cz) = if d == 0.0 {
                let (s, c) = match qi % 4 {
                    0 => (0.0, 1.0),
                    1 => (1.0, 0.0),
                    2 => (0.0, -1.0),
                    _ => (-1.0, 0.0),
                };
                (s, c, !odd, odd)
            } else {
                // In half turns (d/half, one rounding; π itself is never
                // rounded), then sinπ and cosπ, correctly rounded. Below the
                // normal doubles d/half has lost precision (it is 0 for the
                // smallest angles), so there in radians instead, the sine
                // being the angle itself. (An angle so small that its sine
                // underflows gives ±0, with the angle's sign; not an exact
                // zero.)
                let h = d / half;
                let (sd, cd) = if h.abs() >= f64::MIN_POSITIVE {
                    (cm::sinpi(h), cm::cospi(h))
                } else {
                    cm::sincos(d * unit.to_radians_factor())
                };
                // A quarter turn swaps them (and negates the new cosine), a
                // half turn negates both.
                let (s, c) = if odd { (cd, -sd) } else { (sd, cd) };
                let (s, c) = if qi & 2 != 0 { (-s, -c) } else { (s, c) };
                (s, c, false, false)
            };
            if x < 0.0 {
                (-s, c, sz, cz)
            } else {
                (s, c, sz, cz)
            }
        }
    }
}

/// a/b where b is a sine or cosine: undefined where b is exactly 0, and
/// ±∞ where b is only a value too small for a double.
#[inline]
fn over(a: f64, b: f64, exact_zero: bool) -> f64 {
    if b == 0.0 && !exact_zero {
        a / b
    } else {
        div(a, b)
    }
}

#[inline]
pub fn sin_u(x: f64, unit: TrigUnit) -> f64 {
    if unit == TrigUnit::Radians {
        cm::sin(x)
    } else {
        sin_cos(x, unit).0
    }
}

#[inline]
pub fn cos_u(x: f64, unit: TrigUnit) -> f64 {
    if unit == TrigUnit::Radians {
        cm::cos(x)
    } else {
        sin_cos(x, unit).1
    }
}

#[inline]
pub fn tan_u(x: f64, unit: TrigUnit) -> f64 {
    if unit == TrigUnit::Radians {
        cm::tan(x)
    } else {
        let (s, c, _, cz) = sin_cos_zeros(x, unit);
        over(s, c, cz)
    }
}

/// Division where dividing by exactly zero is undefined (NaN) rather than
/// ±∞. An infinite intermediate would otherwise carry on as if the
/// expression were defined there: atan(1/0) = π/2, e^(−1/0²) = 0.
/// (Overflow of a finite value still gives ±∞: 1/(1+e^1000) is 0.)
#[inline]
pub fn div(a: f64, b: f64) -> f64 {
    if b == 0.0 { f64::NAN } else { a / b }
}

/// ln, with ln 0 undefined (NaN) rather than −∞, for the same reason:
/// 1/ln(0) would otherwise be a finite −0.
#[inline]
pub fn ln(x: f64) -> f64 {
    if x == 0.0 { f64::NAN } else { cm::log(x) }
}

#[inline]
pub fn log10(x: f64) -> f64 {
    if x == 0.0 { f64::NAN } else { cm::log10(x) }
}

/// b^e for an exponent that doesn't depend on x (a constant, or a slider):
/// a negative base only to an integer power, 0 only to a positive power
/// (0⁰ is undefined, as on the TI-84 Plus CE: see `docs/ti-conventions.md`),
/// and an undefined operand stays undefined: IEEE makes 1^NaN and NaN^0
/// equal 1, which would turn 1^(1/0) or (1/0)^0 into a defined value.
#[inline]
pub fn pow(b: f64, e: f64) -> f64 {
    if b.is_nan() || e.is_nan() || (b == 0.0 && e <= 0.0) {
        f64::NAN
    } else {
        cm::pow(b, e)
    }
}

/// b^e for a constant exponent exactly an odd integer that its double `e`
/// isn't (past 2⁵³ every double is even: a typed 9007199254740993 is held
/// as 2⁵³): sign(b)·|b|^e, the odd power's sign kept (review 14,
/// R14-M-02: (−1)^9007199254740993 came out +1). Its magnitude is |b|
/// to the double, as for any power.
#[inline]
pub(crate) fn pow_odd(b: f64, e: f64) -> f64 {
    let m = pow(b.abs(), e);
    if b < 0.0 { -m } else { m }
}

/// b^e for a constant exponent beyond the doubles whose exact value isn't
/// carried (3^20000, too long to fold): whether it is an integer, and odd
/// or even, isn't known, so a negative base gives no value (NaN, which
/// the program marks unknown, never undefined); any other base as [`pow`].
#[inline]
pub(crate) fn pow_beyond(b: f64, e: f64) -> f64 {
    if b < 0.0 { f64::NAN } else { pow(b, e) }
}

/// b^e for an exponent that depends on x (x^x, 2^x, (x−1)^(x+1)): defined
/// only for a positive base, or a zero base to a positive power, as in the
/// TI-84 Plus CE's Real mode and IEEE 1788's pow. (−2)^x is undefined even
/// at whole x, rather than defined on scattered points.
#[inline]
pub fn pow_var(b: f64, e: f64) -> f64 {
    if b > 0.0 {
        if e.is_nan() { f64::NAN } else { cm::pow(b, e) }
    } else if b == 0.0 && e > 0.0 {
        0.0
    } else {
        f64::NAN
    }
}

/// [`pow`] for an integer exponent (0⁰ undefined, as for [`pow`]). A square
/// is one multiplication (correctly rounded) and a cube two (within 2 ulps,
/// as it always was: the most common power in a plot stays a pair of
/// multiplications, and the analysis's numbers near a cancelling hole such
/// as (x³ − 1)/(x − 1)'s stay what they were); any higher power is
/// CORE-MATH's pow, correctly rounded, where repeated squaring rounded at
/// every step. A negative power is 1/bⁿ, one rounding more: the compiler
/// turns a/bⁿ into a·b⁻ⁿ (see `compile::lower`), and this keeps 1/x² bit
/// for bit the division it was written as, so the analysis's reference
/// evaluation of the written expression agrees with the compiled one.
#[inline]
pub fn pow_int(b: f64, n: i32) -> f64 {
    if b.is_nan() || (b == 0.0 && n <= 0) {
        return f64::NAN;
    }
    let m = n.unsigned_abs();
    let p = match m {
        1 => b,
        2 => b * b,
        3 => b * b * b,
        _ => cm::pow(b, f64::from(m)),
    };
    if n >= 0 {
        return p;
    }
    // bⁿ past the largest double: 1/∞ is 0, and the power may be a
    // subnormal double yet ((9007199254740993¹⁰)⁻² is 8.1·10⁻³²⁰, and was
    // 0 where the reference had it right: found by `tests/crossfuzz.rs`).
    // pow has it, correctly rounded.
    if p.is_infinite() && b.is_finite() {
        return cm::pow(b, f64::from(n));
    }
    1.0 / p
}

/// atanh, undefined (NaN rather than ±∞) at its poles ±1.
#[inline]
pub fn atanh(x: f64) -> f64 {
    if x.abs() == 1.0 {
        return f64::NAN;
    }
    cm::atanh(x)
}

/// The plain hyperbolic functions and their inverses, the exponential and
/// the cube root: CORE-MATH's, correctly rounded.
#[inline]
pub fn sinh(x: f64) -> f64 {
    cm::sinh(x)
}

#[inline]
pub fn cosh(x: f64) -> f64 {
    cm::cosh(x)
}

#[inline]
pub fn tanh(x: f64) -> f64 {
    cm::tanh(x)
}

#[inline]
pub fn asinh(x: f64) -> f64 {
    cm::asinh(x)
}

#[inline]
pub fn acosh(x: f64) -> f64 {
    cm::acosh(x)
}

#[inline]
pub fn exp(x: f64) -> f64 {
    cm::exp(x)
}

#[inline]
pub fn cbrt(x: f64) -> f64 {
    cm::cbrt(x)
}

#[inline]
pub fn cot_u(x: f64, unit: TrigUnit) -> f64 {
    let (s, c, sz, _) = sin_cos_zeros(x, unit);
    over(c, s, sz)
}

#[inline]
pub fn sec_u(x: f64, unit: TrigUnit) -> f64 {
    let (_, c, _, cz) = sin_cos_zeros(x, unit);
    over(1.0, c, cz)
}

#[inline]
pub fn csc_u(x: f64, unit: TrigUnit) -> f64 {
    let (s, _, sz, _) = sin_cos_zeros(x, unit);
    over(1.0, s, sz)
}

/// An inverse trigonometric result in `unit`: in radians the correctly
/// rounded `rad`; in degrees or grads `half` (the same angle in half turns,
/// correctly rounded) times the unit's half turn: one more rounding, and π
/// never rounded on the way. Below the normal doubles the half turns have
/// lost precision (asinπ(5·10⁻³²⁴) is 0), so there it is the radians over
/// the unit's size instead (two roundings).
#[inline]
fn angle(unit: TrigUnit, rad: impl FnOnce() -> f64, half: impl FnOnce() -> f64) -> f64 {
    if unit == TrigUnit::Radians {
        return rad();
    }
    let h = half();
    if h.abs() >= f64::MIN_POSITIVE || h.is_nan() {
        h * (unit.full_turn() / 2.0)
    } else {
        rad() / unit.to_radians_factor()
    }
}

#[inline]
pub fn asin_u(x: f64, unit: TrigUnit) -> f64 {
    angle(unit, || cm::asin(x), || cm::asinpi(x))
}

#[inline]
pub fn acos_u(x: f64, unit: TrigUnit) -> f64 {
    angle(unit, || cm::acos(x), || cm::acospi(x))
}

#[inline]
pub fn atan_u(x: f64, unit: TrigUnit) -> f64 {
    angle(unit, || cm::atan(x), || cm::atanpi(x))
}

/// √(x² − 1) for |x| ≥ 1, as √((|x| − 1)(|x| + 1)): near ±1 that keeps the
/// small difference exact (|x| − 1 is exact there), where 1/x or x² − 1
/// would round it away. NaN for |x| < 1.
#[inline]
fn sqrt_x2_minus_1(x: f64) -> f64 {
    let a = x.abs();
    ((a - 1.0) * (a + 1.0)).sqrt()
}

/// arcsec(x) = arccos(1/x), range [0, π]. Near ±1, where arccos is steep
/// and 1/x's rounding would be magnified, as arctan √(x² − 1) instead.
#[inline]
pub fn asec_u(x: f64, unit: TrigUnit) -> f64 {
    if x.abs() >= 2.0 || x.is_nan() {
        angle(unit, || cm::acos(1.0 / x), || cm::acospi(1.0 / x))
    } else {
        let s = sqrt_x2_minus_1(x);
        angle(
            unit,
            || {
                let t = cm::atan(s);
                if x < 0.0 { PI - t } else { t }
            },
            || {
                let t = cm::atanpi(s);
                if x < 0.0 { 1.0 - t } else { t }
            },
        )
    }
}

/// arccsc(x) = arcsin(1/x), range [−π/2, π/2]. Near ±1, as
/// arctan(1/√(x² − 1)) instead (see [`asec_u`]).
#[inline]
pub fn acsc_u(x: f64, unit: TrigUnit) -> f64 {
    if x.abs() >= 2.0 || x.is_nan() {
        angle(unit, || cm::asin(1.0 / x), || cm::asinpi(1.0 / x))
    } else {
        let r = 1.0 / sqrt_x2_minus_1(x);
        angle(
            unit,
            || cm::atan(r).copysign(x),
            || cm::atanpi(r).copysign(x),
        )
    }
}

/// arccot(x), continuous with range (0, π): arctan(1/x) for x > 0 and
/// π + arctan(1/x) for x < 0. Not π/2 − arctan(x), which cancels to 0 for
/// large x (arccot(10¹⁵) is 10⁻¹⁵, not 0.888·10⁻¹⁵).
#[inline]
pub fn acot_u(x: f64, unit: TrigUnit) -> f64 {
    if x > 0.0 {
        angle(unit, || cm::atan(1.0 / x), || cm::atanpi(1.0 / x))
    } else if x < 0.0 {
        angle(
            unit,
            || PI + cm::atan(1.0 / x),
            || 1.0 + cm::atanpi(1.0 / x),
        )
    } else if x == 0.0 {
        angle(unit, || PI / 2.0, || 0.5)
    } else {
        f64::NAN
    }
}

/// sech x = 2e^−|x| / (1 + e^−2|x|): no cosh to overflow, so it stays
/// right (subnormal) out to |x| ≈ 745 instead of becoming 0 past 710.
#[inline]
pub fn sech(x: f64) -> f64 {
    let a = x.abs();
    if a < 1.0 {
        1.0 / cm::cosh(x)
    } else {
        let e = cm::exp(-a);
        2.0 * e / (1.0 + e * e)
    }
}

/// csch x, as 2e^−|x| / (1 − e^−2|x|) beyond |x| = 1 (no sinh to
/// overflow; see [`sech`]). Undefined at 0.
#[inline]
pub fn csch(x: f64) -> f64 {
    let a = x.abs();
    if a < 1.0 {
        div(1.0, cm::sinh(x))
    } else {
        // (e ≤ 1/e here, so 1 − e² doesn't cancel.)
        let e = cm::exp(-a);
        (2.0 * e / (1.0 - e * e)).copysign(x)
    }
}

#[inline]
pub fn coth(x: f64) -> f64 {
    // Not cosh/sinh: both overflow for |x| ≳ 710 and the ratio becomes NaN.
    div(1.0, cm::tanh(x))
}

/// arsech x = ln((1 + √(1 − x²)) / x) on (0, 1], undefined elsewhere:
/// computed as ln(1 + √((1 − x)(1 + x))) − ln x, so neither 1/x overflows
/// near 0 (arsech 10⁻³¹⁰ is 714.5, not ∞) nor its rounding is magnified
/// near 1, where the result is small.
#[inline]
pub fn asech(x: f64) -> f64 {
    if !(x > 0.0 && x <= 1.0) {
        return f64::NAN;
    }
    let ln_x = if x > 0.5 {
        cm::log1p(x - 1.0)
    } else {
        cm::log(x)
    };
    cm::log1p(((1.0 - x) * (1.0 + x)).sqrt()) - ln_x
}

/// arcsch x = arsinh(1/x), undefined at 0. For |x| ≤ 1 as
/// ±(ln(1 + √(1 + x²)) − ln|x|), so 1/x can't overflow near 0.
#[inline]
pub fn acsch(x: f64) -> f64 {
    if x == 0.0 || x.is_nan() {
        return f64::NAN;
    }
    let a = x.abs();
    if a > 1.0 {
        cm::asinh(1.0 / x)
    } else {
        (cm::log1p((1.0 + a * a).sqrt()) - cm::log(a)).copysign(x)
    }
}

/// arcoth x = ½ ln((x + 1)/(x − 1)) for |x| > 1, undefined at ±1 and
/// between. As ±½ ln(1 + 2/(|x| − 1)): |x| − 1 is exact near 1, and the
/// logarithm stays accurate (≈ 1/x) far out, where atanh(1/x) would
/// magnify 1/x's rounding near ±1.
#[inline]
pub fn acoth(x: f64) -> f64 {
    let a = x.abs();
    if a.is_nan() || a <= 1.0 {
        return f64::NAN;
    }
    (0.5 * cm::log1p(2.0 / (a - 1.0))).copysign(x)
}

/// `root(x, n)`: real n-th root. Odd integer n accepts negative x.
#[inline]
pub fn root(x: f64, n: f64) -> f64 {
    // A negative degree at 0 is 1/0: undefined.
    if n == 0.0 || !n.is_finite() || x.is_nan() || (x == 0.0 && n < 0.0) {
        return f64::NAN;
    }
    if n == 3.0 {
        return cm::cbrt(x);
    }
    if n == 2.0 {
        return x.sqrt();
    }
    // Another whole degree: the power 1/n as [`pow_rational`] takes it
    // (the same real root: a negative x for an odd n only), which doesn't
    // let 1/n's rounding through. Through `pow` alone that rounding is
    // magnified by |ln x|/n: root(x⁵, 5) at 10¹⁷ was 14 ulps off and
    // root(−x², 5) at 10¹⁰⁰ 40, outside f's enclosure (found by
    // `tests/crossfuzz.rs`).
    if n == n.trunc() && n.abs() <= f64::from(i32::MAX) {
        return pow_rational(x, if n > 0.0 { 1 } else { -1 }, n.abs() as i32);
    }
    if is_odd_integer(n) {
        let r = cm::pow(x.abs(), 1.0 / n);
        return if x < 0.0 { -r } else { r };
    }
    if x < 0.0 {
        return f64::NAN;
    }
    cm::pow(x, 1.0 / n)
}

/// Logarithm of `x` in base `b` (`log(b, x)`).
#[inline]
pub fn log_base(b: f64, x: f64) -> f64 {
    if b <= 0.0 || b == 1.0 || x == 0.0 {
        return f64::NAN;
    }
    if b == 10.0 {
        return cm::log10(x);
    }
    if b == 2.0 {
        return cm::log2(x);
    }
    cm::log(x) / cm::log(b)
}

/// Floored modulo: `a - b·floor(a/b)`; the result has the sign of `b`.
#[inline]
pub fn modulo(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        return f64::NAN;
    }
    let r = a % b;
    if r != 0.0 && (r < 0.0) != (b < 0.0) {
        r + b
    } else {
        r
    }
}

/// Sign function (0 at 0).
#[inline]
pub fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else if x == 0.0 {
        0.0
    } else {
        f64::NAN
    }
}

/// Round half away from zero.
#[inline]
pub fn round(x: f64) -> f64 {
    x.round()
}

/// The gamma function Γ(x), correctly rounded (CORE-MATH). Poles (0, −1,
/// −2, …) give NaN.
pub fn gamma(x: f64) -> f64 {
    if x.is_nan() || (x <= 0.0 && x == x.trunc()) {
        return f64::NAN;
    }
    cm::tgamma(x)
}

/// `n!` extended to the reals as Γ(n+1).
#[inline]
pub fn factorial(n: f64) -> f64 {
    gamma(n + 1.0)
}

/// `n!!` for integers n ≥ −1 (NaN otherwise): the exact integer, rounded
/// once.
pub fn double_factorial(n: f64) -> f64 {
    match double_factorial_exact(n) {
        None => f64::NAN,
        Some(Count::Exact(v)) => v.to_f64(Round::Nearest),
        Some(Count::Huge) => f64::INFINITY,
    }
}

/// An exact count (n!!, nCr, nPr of whole numbers), or one beyond the
/// doubles (above 2¹⁰²⁵: +∞ once rounded).
pub(crate) enum Count {
    Exact(Nat),
    Huge,
}

/// 2⁵³: products of whole numbers below it are exact in doubles.
const EXACT: f64 = 9_007_199_254_740_992.0;

/// The bits beyond which a count is +∞ in doubles.
const HUGE_BITS: u64 = 1026;

/// `n!!` exactly, for integers n ≥ −1 (`None` otherwise). From 301 on it
/// is beyond the doubles (301!! > 10³⁰⁹; 300!! ≈ 8·10³⁰⁷ is not).
pub(crate) fn double_factorial_exact(n: f64) -> Option<Count> {
    if n != n.trunc() || n < -1.0 {
        return None;
    }
    if n > 300.0 {
        return Some(Count::Huge);
    }
    // In doubles while the product is exact, then in whole numbers.
    let mut r = 1.0;
    let mut k = n;
    while k > 1.0 && r * k < EXACT {
        r *= k;
        k -= 2.0;
    }
    let mut big = Nat::from_u64(r as u64);
    while k > 1.0 {
        big = big.mul_small(k as u32);
        k -= 2.0;
    }
    Some(Count::Exact(big))
}

/// nCr (or nPr, `perm`) exactly, for a whole n ≥ 0 and a whole r (0 when
/// r < 0 or r > n); `None` for other arguments (Γ's generalisation).
pub(crate) fn count_exact(n: f64, r: f64, perm: bool) -> Option<Count> {
    if !(n.is_finite() && r.is_finite() && n == n.trunc() && r == r.trunc() && n >= 0.0) {
        return None;
    }
    if r < 0.0 || r > n {
        return Some(Count::Exact(Nat::zero()));
    }
    let nat = |v: f64| Nat::from_f64(v).expect("a whole number ≥ 0");
    if perm {
        // n(n − 1)…(n − r + 1): factors ≥ 2 until the last, so a long
        // product leaves the doubles within about 1100 of them.
        let (mut acc, mut i) = (1.0, 0.0);
        while i < r && n < EXACT && acc * (n - i) < EXACT {
            acc *= n - i;
            i += 1.0;
        }
        let mut big = Nat::from_u64(acc as u64);
        let top = nat(n);
        while i < r {
            big = big.mul(&top.sub(&nat(i)));
            if big.bits() > HUGE_BITS {
                return Some(Count::Huge);
            }
            i += 1.0;
        }
        return Some(Count::Exact(big));
    }
    // C(n, r) = C(n, n − r): the smaller r. (n − r is exact where it is the
    // smaller: r and n within a factor 2.)
    let r = r.min(n - r);
    // acc = C(n − r + i, i), increasing in i (C(n, i) ≥ 2^i for i ≤ n/2):
    // multiplied, then divided exactly.
    let (mut acc, mut i) = (1.0, 1.0);
    while i <= r && n < EXACT && acc * (n - r + i) < EXACT {
        acc = acc * (n - r + i) / i;
        i += 1.0;
    }
    let mut big = Nat::from_u64(acc as u64);
    let base = nat(n).sub(&nat(r));
    while i <= r {
        // i ≤ about 1100 here: past that the count is beyond the doubles.
        let (q, rem) = big.mul(&base.add(&nat(i))).div_rem_small(i as u32);
        debug_assert_eq!(rem, 0);
        big = q;
        if big.bits() > HUGE_BITS {
            return Some(Count::Huge);
        }
        i += 1.0;
    }
    Some(Count::Exact(big))
}

/// nCr (or nPr, `perm`) exactly for whole numbers 0 ≤ r ≤ n of any size,
/// such as a typed 9007199254740993 that no double holds (review 14,
/// R14-M-03: rounded first, C(2⁵³ + 1, 1) − 2⁵³ was 0, not 1); `None` for
/// a count longer than `big::MAX_BITS` bits (and so beyond the doubles).
pub(crate) fn count_exact_big(n: &Nat, r: &Nat, perm: bool) -> Option<Nat> {
    let cap = crate::big::MAX_BITS;
    let small = |v: &Nat| -> Option<u64> {
        let (lo, hi) = v.enclose();
        (hi < EXACT && lo == hi).then_some(lo as u64)
    };
    if perm {
        // n(n − 1)…(n − r + 1): r factors, each ≥ 1 and all but the last
        // ≥ 2, so at least 2^(r − 1).
        let r = small(r).filter(|&r| r <= cap + 1)?;
        let mut acc = Nat::from_u64(1);
        for i in 0..r {
            acc = acc.mul(&n.sub(&Nat::from_u64(i)));
            if acc.bits() > cap {
                return None;
            }
        }
        return Some(acc);
    }
    // C(n, r) = C(n, n − r): the smaller, r′ (C(n, r′) ≥ 2^r′).
    let rest = n.sub(r);
    let r = if rest < *r { rest } else { r.clone() };
    let r = small(&r).filter(|&r| r <= cap)?;
    // acc = C(n − r′ + i, i), increasing in i: multiplied, then divided
    // exactly.
    let base = n.sub(&Nat::from_u64(r));
    let mut acc = Nat::from_u64(1);
    for i in 1..=r {
        let (q, rem) = acc
            .mul(&base.add(&Nat::from_u64(i)))
            .div_rem_small(i as u32);
        debug_assert_eq!(rem, 0);
        acc = q;
        if acc.bits() > cap {
            return None;
        }
    }
    Some(acc)
}

/// True for odd integers. Every `f64` at or beyond 2^53 is an even integer
/// (a saturating `as i64` cast would wrongly report `i64::MAX`, i.e. odd).
pub(crate) fn is_odd_integer(n: f64) -> bool {
    n == n.trunc() && n.abs() < 9_007_199_254_740_992.0 && n % 2.0 != 0.0
}

/// The most factors [`falling`] multiplies: the interval form's bound
/// (`interval::elem::small_count`).
pub(crate) const SMALL_COUNT: f64 = 60.0;

/// nPr(n, k) = n(n − 1)…(n − k + 1), and nCr(n, k) that over k!, for a
/// whole 0 ≤ k ≤ [`SMALL_COUNT`] and an n that is no whole number ≥ 0
/// (those are counted exactly): what Γ's quotient is there, as the interval
/// form has it (`interval::elem::ncr_npr`, the same operations in the same
/// order, so the value lies in its enclosure). `None` for another r. A
/// negative integer n is undefined (Γ(n + 1) at a pole: a hole in the
/// polynomial).
///
/// Γ's quotient itself loses everything here: in doubles n − k + 1 rounds
/// onto a pole beside a whole n (nCr(1 − 2⁻⁵³, 2) was undefined), Γ of a
/// large negative n underflows (nCr(−172.5, 1) was off in its twelfth
/// digit), and past where Γ overflows the difference of its logarithms
/// keeps no digit (nCr(5.6·10¹⁴ + 1/16, 2) was 2.15 times too large): the
/// tester `tests/crossfuzz.rs` found each against the interval form.
fn falling(n: f64, r: f64, perm: bool) -> Option<f64> {
    if !(r == r.trunc() && (0.0..=SMALL_COUNT).contains(&r)) {
        return None;
    }
    if n == n.trunc() && n < 0.0 {
        return Some(f64::NAN);
    }
    let k = r as u32;
    let mut acc = 1.0;
    for i in 0..k {
        acc *= n - f64::from(i);
    }
    if perm {
        return Some(acc);
    }
    let mut f = 1.0;
    for i in 2..=k {
        f *= f64::from(i);
    }
    if acc.is_finite() {
        return Some(acc / f);
    }
    // The product left the doubles before k! brought it back: divided as
    // it goes (|C(n, i)| grows with i this far out).
    let mut acc = 1.0;
    for i in 0..k {
        acc = acc * (n - f64::from(i)) / f64::from(i + 1);
    }
    Some(acc)
}

/// Number of combinations `nCr(n, r)` (generalised through Γ).
pub fn ncr(n: f64, r: f64) -> f64 {
    count(n, r, false)
}

/// Number of permutations `nPr(n, r)` (generalised through Γ).
pub fn npr(n: f64, r: f64) -> f64 {
    count(n, r, true)
}

/// nCr (nPr, `perm`): whole numbers counted exactly, a small whole r as the
/// polynomial in n, and else Γ's quotient: as it is in doubles where that
/// is right to a few ulps ([`gamma_direct`]), and in extended range where
/// it isn't ([`gamma_count`]).
fn count(n: f64, r: f64, perm: bool) -> f64 {
    if !n.is_finite() || !r.is_finite() {
        return f64::NAN;
    }
    // Whole numbers: the exact count, rounded once.
    match count_exact(n, r, perm) {
        Some(Count::Exact(v)) => return v.to_f64(Round::Nearest),
        Some(Count::Huge) => return f64::INFINITY,
        None => {}
    }
    if let Some(v) = falling(n, r, perm) {
        return v;
    }
    if let Some(v) = gamma_direct(n, r, perm) {
        return v;
    }
    match gamma_count(n, r, perm) {
        Some((m, e)) => crate::wide::ldexp(m, e),
        None => f64::NAN,
    }
}

/// nCr (nPr, `perm`) of two doubles where [`count`] has it as 0, a
/// subnormal or ±∞: its own value, m·2^e with 1 ≤ |m| < 2 (e = ±∞ for a
/// magnitude beyond even that range), and (0, 0) only for a count that is
/// 0 (C(2, 3)); `None` at a pole of Γ. Within an ulp or two of m, for the
/// evaluations that hold values beyond the doubles (`wide::apply2`, the
/// reference evaluator): a count past them inside a larger expression was
/// taken as its double, so a/nCr(x, 3) and ln nCr(x, 2) beside 0, where
/// the count underflows to 0, were undefined, and nPr(x, 62)/x⁶² near
/// −5.6·10¹⁴, both overflowing, was +∞ for about 1 (the first two found
/// by `tests/crossfuzz.rs`).
pub(crate) fn count_beyond(n: f64, r: f64, perm: bool) -> Option<(f64, f64)> {
    if !n.is_finite() || !r.is_finite() {
        return None;
    }
    let whole = |v: f64| v == v.trunc();
    if whole(n) && whole(r) && n >= 0.0 {
        // Counted exactly in doubles, so 0 only outside 0 ≤ r ≤ n; past
        // them, through Γ (no argument of it below 1 here).
        if r < 0.0 || r > n {
            return Some((0.0, 0.0));
        }
        return gamma_count(n, r, perm);
    }
    if whole(r) && (0.0..=SMALL_COUNT).contains(&r) {
        // The polynomial ([`falling`]); a whole n < 0 is a hole in it.
        if whole(n) {
            return None;
        }
        return Some(falling_far(n, r as u32, perm));
    }
    gamma_count(n, r, perm)
}

/// n! (Γ(n + 1)) of a double where [`factorial`] has it as 0, a subnormal
/// or +∞, as [`count_beyond`] has a count: nPr(n, n). Within an ulp or two
/// (`wide::apply1` took e^(ln Γ(n + 1)) with ln Γ a double, about 13
/// digits: x!/(x·(x − 1)!) at 10⁶ was 1.00000000000017).
pub(crate) fn factorial_beyond(n: f64) -> Option<(f64, f64)> {
    gamma_count(n, n, true)
}

/// n!! of a double where [`double_factorial`] has it as +∞ (a whole n past
/// 300): 2^k·k! for n = 2k, and (2k + 1)!/(2^k·k!) = nPr(2k + 1, k + 1)/2^k
/// for n = 2k + 1. `None` for any other n.
pub(crate) fn double_factorial_beyond(n: f64) -> Option<(f64, f64)> {
    if !(n.is_finite() && n == n.trunc() && n > 300.0) {
        return None;
    }
    // (Exact: every double from 2⁵³ on is even.)
    let k = (n / 2.0).floor();
    if n % 2.0 == 0.0 {
        let (m, e) = gamma_count(k, k, true)?;
        Some((m, e + k))
    } else {
        let (m, e) = gamma_count(n, k + 1.0, true)?;
        Some((m, e - k))
    }
}

/// [`falling`]'s n(n − 1)…(n − k + 1) (over k! for nCr) for an n no whole
/// number, as m·2^e: each factor n − i exact (a double-double), each
/// product in double-double with its binary exponent kept apart, so
/// nothing overflows or underflows, and m within about half an ulp.
fn falling_far(n: f64, k: u32, perm: bool) -> (f64, f64) {
    // x = m·2^s with 1 ≤ |m.hi| < 2, for x.hi finite and nonzero. (A
    // nonzero x.lo is within half an ulp of a normal x.hi, so its scaling
    // is in range.)
    let split = |x: Dd| -> (Dd, f64) {
        let (hi, s) = crate::wide::frexp(x.hi);
        let lo = if x.lo == 0.0 {
            0.0
        } else {
            crate::wide::ldexp(x.lo, -s)
        };
        (Dd { hi, lo }, s)
    };
    let (mut m, mut e) = (Dd::of(1.0), 0.0);
    for i in 0..k {
        let (x, s) = split(Dd::of(n).add(Dd::of(-f64::from(i))));
        let (p, t) = split(m.mul(x));
        (m, e) = (p, e + s + t);
    }
    if !perm {
        let mut f = Dd::of(1.0);
        for i in 2..=k {
            f = f.mul_f(f64::from(i));
        }
        let (f, s) = split(f);
        let (q, t) = split(m.div(f));
        (m, e) = (q, e - s + t);
    }
    (m.hi, e)
}

/// a + b exactly in doubles?
fn sum_is_exact(a: f64, b: f64) -> bool {
    let s = a + b;
    let bb = s - a;
    (a - (s - bb)) + (b - bb) == 0.0
}

/// Γ(n + 1)/Γ(n − r + 1) (over Γ(r + 1) for nCr) in doubles, as it always
/// was, where that is right to a few ulps: each argument of Γ exact in
/// doubles (n − r + 1 rounded is another argument: nPr(169.3, 0.7) was 297
/// ulps off) and each Γ, product and quotient a normal double (each then
/// correctly rounded or rounded once: 2½ ulps at most). `None` elsewhere.
fn gamma_direct(n: f64, r: f64, perm: bool) -> Option<f64> {
    let rest = n - r;
    if !(sum_is_exact(n, 1.0)
        && sum_is_exact(n, -r)
        && sum_is_exact(rest, 1.0)
        && (perm || sum_is_exact(r, 1.0)))
    {
        return None;
    }
    let (top, below) = (factorial(n), factorial(rest));
    if !(top.is_normal() && below.is_normal()) {
        return None;
    }
    let v = if perm {
        top / below
    } else {
        let k = factorial(r);
        let d = k * below;
        if !(k.is_normal() && d.is_normal()) {
            return None;
        }
        top / d
    };
    v.is_normal().then_some(v)
}

/// ½·ln 2π and ln π to about 106 bits.
const HALF_LN_2PI: Dd = Dd {
    hi: 0.9189385332046728,
    lo: -3.8782941580672414e-17,
};
const LN_PI: Dd = Dd {
    hi: 1.1447298858494002,
    lo: 1.0265951162707826e-17,
};

/// From here on ln Γ is Stirling's series ([`stirling_tail`]).
const STIRLING_FROM: f64 = 16.0;

/// An argument of Γ beyond which ln Γ of it alone (above 2²⁰⁰·138) is
/// only a sign: a count that holds one is beyond the extended range.
const FAR_ARGUMENT: f64 = 1.606_938_044_258_990_3e60;

/// The largest |ln| of a count whose mantissa is kept (2⁴⁷): ln Γ in
/// double-double is within about 2⁻¹⁰⁰ of itself, an ulp of the count's
/// mantissa here. Past it a count is only a size, beyond the extended
/// range (its binary exponent would near 2⁵³, where `Wide` keeps no
/// mantissa either).
const FAR_LOG: f64 = 140_737_488_355_328.0;

/// a·n + b·r + c for a, b, c ∈ {−1, 0, 1}, as a double-double (exact but
/// for a rounding at 2⁻¹⁰⁶ of it).
fn linear(n: f64, r: f64, (a, b, c): (f64, f64, f64)) -> Dd {
    Dd::of(a * n).add(Dd::of(b * r)).add(Dd::of(c))
}

/// ln x for a double-double x > 0.
fn ln_dd(x: Dd) -> Dd {
    crate::dd::ln(x.hi).add(Dd::of(x.lo / x.hi))
}

/// s·ln(1 + δ/q) for double-doubles with |δ/q| ≤ ¼, to about 2⁻¹⁰⁰ of
/// itself.
fn times_ln_ratio(s: Dd, delta: Dd, q: Dd) -> Dd {
    let t = delta.div(q);
    if t.hi.abs() < 9.5e-7 {
        // ln(1 + t) = 2·atanh(y), y = t/(2 + t): 2y(1 + y²/3 + y⁴/5), the
        // next term below 2⁻¹²⁰ of it; s·2y taken as (s/q)·δ·2/(2 + t),
        // so that nothing is subnormal however large q is.
        let w = Dd::of(2.0).add(t);
        let y = t.div(w);
        let z = y.mul(y);
        let series = Dd::of(1.0).add(z.div_f(3.0)).add(Dd::of(z.hi * z.hi / 5.0));
        return s.div(q).mul(delta).mul_f(2.0).div(w).mul(series);
    }
    // 1 + t = h + l: ln h + ln(1 + l/h), |l/h| ≤ 2⁻⁵³. (h + l holds t to
    // 2⁻¹⁰⁶ absolute, 2⁻⁸⁶ of t here.)
    let u = Dd::of(1.0).add(t);
    let c = Dd::of(u.lo).div_f(u.hi);
    s.mul(crate::dd::ln(u.hi).add(c).add(Dd::of(-0.5 * c.hi * c.hi)))
}

/// ln Γ(x) − ((x − ½)·ln x − x + ½·ln 2π) for x ≥ [`STIRLING_FROM`]:
/// Stirling's series to B₁₆, within 3·10⁻²⁰.
fn stirling_tail(x: f64) -> f64 {
    let z = 1.0 / (x * x);
    let s: f64 = -3617.0 / 122_400.0;
    let s = s.mul_add(z, 1.0 / 156.0);
    let s = s.mul_add(z, -691.0 / 360_360.0);
    let s = s.mul_add(z, 1.0 / 1188.0);
    let s = s.mul_add(z, -1.0 / 1680.0);
    let s = s.mul_add(z, 1.0 / 1260.0);
    let s = s.mul_add(z, -1.0 / 360.0);
    let s = s.mul_add(z, 1.0 / 12.0);
    s / x
}

/// ln Γ(x) for x ≥ [`STIRLING_FROM`], to about 2⁻¹⁰⁰ of its size.
fn ln_gamma_stirling(x: Dd) -> Dd {
    x.add(Dd::of(-0.5))
        .mul(ln_dd(x))
        .add(x.neg())
        .add(HALF_LN_2PI)
        .add(Dd::of(stirling_tail(x.hi)))
}

/// ln Γ(x) for a double-double x ≥ ½, to about 2⁻¹⁰⁰ of its size; `None`
/// past [`FAR_ARGUMENT`].
fn ln_gamma_dd(x: Dd) -> Option<Dd> {
    if x.hi > FAR_ARGUMENT {
        return None;
    }
    if x.hi >= STIRLING_FROM {
        return Some(ln_gamma_stirling(x));
    }
    // Γ(x) = Γ(x + m)/(x(x + 1)…(x + m − 1)).
    let one = Dd::of(1.0);
    let mut product = x;
    let mut y = x.add(one);
    while y.hi < STIRLING_FROM {
        product = product.mul(y);
        y = y.add(one);
    }
    Some(ln_gamma_stirling(y).add(ln_dd(product).neg()))
}

/// ln Γ(p) − ln Γ(q) for double-doubles p, q ≥ ½ apart by δ = p − q
/// (given exactly). Two large arguments close together are taken as one
/// ratio, whose logarithm is about δ·ln q however large q is: through
/// Stirling, (p − ½)·ln(1 + δ/q) + δ·(ln q − 1) and the difference of the
/// series. `None` where a lone ln Γ is past [`FAR_ARGUMENT`].
fn ln_gamma_ratio(p: Dd, q: Dd, delta: Dd) -> Option<Dd> {
    if p.hi >= STIRLING_FROM && q.hi >= STIRLING_FROM && delta.hi.abs() <= 0.25 * q.hi {
        return Some(
            times_ln_ratio(p.add(Dd::of(-0.5)), delta, q)
                .add(delta.mul(ln_dd(q).add(Dd::of(-1.0))))
                .add(Dd::of(stirling_tail(p.hi) - stirling_tail(q.hi))),
        );
    }
    Some(ln_gamma_dd(p)?.add(ln_gamma_dd(q)?.neg()))
}

/// |sin πd| for 0 < |d| ≤ ½ as m·2^e, 1 ≤ m < 2 (πd itself below 2⁻⁹⁰⁰,
/// to 2⁻¹⁸⁰⁰ of it).
fn sinpi_scaled(d: f64) -> (f64, f64) {
    let d = d.abs();
    if d < 1.2e-271 {
        let (m, e) = crate::wide::frexp(d);
        let (m, k) = crate::wide::frexp(PI * m);
        return (m, e + k);
    }
    crate::wide::frexp(cm::sinpi(d))
}

/// nPr(n, r) = Γ(n + 1)/Γ(n − r + 1), and nCr(n, r) that over Γ(r + 1),
/// for any doubles n and r, as m·2^e (1 ≤ |m| < 2; e = ±∞ for a magnitude
/// beyond even that); `None` where an argument of Γ is at a pole.
///
/// Each argument is a·n + b·r + 1, held exactly (as a double-double, not
/// rounded onto or beside a pole). One below ½ is reflected: Γ(x) =
/// π/(sin πx · Γ(1 − x)), sin πx from n and r reduced modulo 2 exactly, so
/// a pole is where it is 0 and its neighbourhood keeps every digit. What is
/// left is Γ of arguments ≥ ½ to a power ±1: the largest is taken with the
/// largest on the other side of the fraction as one ratio
/// ([`ln_gamma_ratio`], two large arguments apart by an exact a·n + b·r +
/// c), the rest alone, all as logarithms in double-double. So nothing
/// overflows on the way, and the result is within a few ulps: each sine
/// and the exponential correctly rounded, the logarithm to about 2⁻⁹⁰.
///
/// Γ's quotient in doubles loses it all far out and next to the poles:
/// nPr(x, 62) was undefined at 10⁻⁹⁰ (x − 61 rounds onto −61), as were
/// nCr(7, 123456.7) (an overflow times an underflow) and nPr(2x, ½) far
/// below 0 (two underflows), and nPr(10¹⁵ + ½, ½) was 3.6 times too small
/// (the difference of two logarithms near 3·10¹⁶): the tester
/// `tests/crossfuzz.rs` found them against the interval form.
pub(crate) fn gamma_count(n: f64, r: f64, perm: bool) -> Option<(f64, f64)> {
    // (a, b, c) of each argument, and its power in the quotient.
    const TOP: (f64, f64, f64) = (1.0, 0.0, 1.0);
    const REST: (f64, f64, f64) = (1.0, -1.0, 1.0);
    const COUNT: (f64, f64, f64) = (0.0, 1.0, 1.0);
    let forms: &[((f64, f64, f64), i32)] = if perm {
        &[(TOP, 1), (REST, -1)]
    } else {
        &[(TOP, 1), (REST, -1), (COUNT, -1)]
    };
    // n and r modulo 2, exactly (fmod is exact).
    let (n2, r2) = (n % 2.0, r % 2.0);
    let mut sign = 1.0;
    let mut log = Dd::of(0.0);
    // The sines' product, m·2^e.
    let (mut m, mut e) = (1.0, 0.0);
    // The arguments ≥ ½: (a, b, c), the value, the power.
    let mut args = [((0.0, 0.0, 0.0), Dd::of(0.0), 0); 3];
    let mut len = 0;
    for &((a, b, c), power) in forms {
        let x = linear(n, r, (a, b, c));
        if x.hi >= 0.5 {
            args[len] = ((a, b, c), x, power);
            len += 1;
            continue;
        }
        // sin πx = −sin π(a·n + b·r) (c = 1), a·n + b·r ≡ s + t modulo 2
        // exactly, and that k + d with k whole and |d| ≤ ½ (s − k is
        // exact: |s| < 4).
        let st = Dd::of(a * n2).add(Dd::of(b * r2));
        let k = st.hi.round();
        let d = (st.hi - k) + st.lo;
        if d == 0.0 {
            // x is a whole number ≤ 0.
            return None;
        }
        // Γ(x) has the sign of sin πx = −(−1)^k·sin πd.
        if (d > 0.0) != (k % 2.0 != 0.0) {
            sign = -sign;
        }
        // Γ(x)^power = (π/|sin πx|)^power · Γ(1 − x)^−power.
        let (sm, se) = sinpi_scaled(d);
        if power > 0 {
            m /= sm;
            e -= se;
            log = log.add(LN_PI);
        } else {
            m *= sm;
            e += se;
            log = log.add(LN_PI.neg());
        }
        let (mm, me) = crate::wide::frexp(m);
        (m, e) = (mm, e + me);
        args[len] = ((-a, -b, 1.0 - c), linear(n, r, (-a, -b, 1.0 - c)), -power);
        len += 1;
    }
    let args = &args[..len];
    // The largest argument with the largest on the other side, as one
    // ratio; the rest alone.
    let largest = |of: &mut dyn Iterator<Item = usize>| {
        of.max_by(|&i, &j| args[i].1.hi.total_cmp(&args[j].1.hi))
    };
    let big = largest(&mut (0..args.len())).expect("an argument");
    let other = largest(&mut (0..args.len()).filter(|&i| args[i].2 != args[big].2));
    let mut far = false;
    if let Some(other) = other {
        let (up, down) = if args[big].2 > 0 {
            (big, other)
        } else {
            (other, big)
        };
        let ((a, b, c), p, _) = args[up];
        let ((a2, b2, c2), q, _) = args[down];
        let delta = linear(n, r, (a - a2, b - b2, c - c2));
        match ln_gamma_ratio(p, q, delta) {
            Some(l) => log = log.add(l),
            None => far = true,
        }
    }
    for (i, &(_, x, power)) in args.iter().enumerate() {
        if Some(i) == other || (i == big && other.is_some()) {
            continue;
        }
        match ln_gamma_dd(x) {
            Some(l) => log = log.add(if power > 0 { l } else { l.neg() }),
            None => far = true,
        }
    }
    if far || !log.hi.is_finite() {
        // Arguments past 2²⁰⁰ that no ratio brings together, or a ratio of
        // two past 10³⁰⁵ apart by a quarter of them or more: the count is
        // beyond the extended range either way, as the sign of
        // Σ ±x·(ln x − 1), each taken over the largest x, says.
        let top = args.iter().map(|a| a.1.hi).fold(0.0, f64::max);
        let s: f64 = args
            .iter()
            .map(|&(_, x, power)| f64::from(power) * (x.hi / top) * (cm::log(x.hi) - 1.0))
            .sum();
        return Some((
            sign,
            if s > 0.0 {
                f64::INFINITY
            } else {
                f64::NEG_INFINITY
            },
        ));
    }
    if log.hi.abs() > FAR_LOG {
        let far = if log.hi > 0.0 {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        };
        return Some((sign, far));
    }
    let (x, k) = crate::dd::exp(log);
    let (mm, me) = crate::wide::frexp(m * x);
    Some((sign * mm, e + k + me))
}

/// `b^(p/q)` with real-root semantics: a negative base is allowed when q is
/// odd (e.g. `(-8)^(1/3) = -2`, `(-8)^(2/3) = 4`). Within an ulp, and the
/// exact value wherever that is a double ([`ratio_power`]): `x^(1/3)` is
/// `cbrt(x)` and `root(x, 3)` to the bit.
#[inline]
pub fn pow_rational(b: f64, p: i32, q: i32) -> f64 {
    if b.is_nan() || (b == 0.0 && p <= 0) {
        return f64::NAN;
    }
    let neg = b < 0.0;
    if neg && q % 2 == 0 {
        return f64::NAN;
    }
    if q == 2 && p == 1 {
        return b.sqrt();
    }
    let a = b.abs();
    let m = if a > 0.0 && a.is_finite() && p != 0 {
        ratio_power(a, p, q)
    } else {
        cm::pow(a, p as f64 / q as f64)
    };
    if neg && p % 2 != 0 { -m } else { m }
}

/// p and q below this are held in 20 bits: q times half of a double's
/// mantissa is then exact.
const SMALL_PART: i32 = 1 << 20;

/// a^(p/q) for a finite a > 0 and p ≠ 0 (p/q in lowest terms), within an
/// ulp, and exactly where it is a double.
///
/// p/q as a double is rounded (1/3 by 2⁻⁵⁶), and pow, though itself
/// correctly rounded, magnifies that by |ln a·p/q|: pow(1000, 1/3) is
/// 9.999999999999998, so ⌊x^(1/3)⌋ was 9 at 1000 and (x^(1/3))! had a
/// value at −1000, where the cube root is −10 and the factorial undefined
/// (found by `tests/crossfuzz.rs`); 10⁷^(1/7) was the same number, and
/// far from 1 the error was many ulps (x^(1/5) at 10⁴⁵: 17).
///
/// * q a power of two: p/q is a double and pow is correctly rounded.
/// * The cube root and its reciprocal: cbrt, correctly rounded itself.
/// * Otherwise pow at the double t nearest p/q, times a^(p/q − t) = 1 +
///   (p/q − t)·ln a to within its square (below 10⁻²⁶): half an ulp of
///   pow's and half of the sum's. When the true value V is a double, that
///   is V or a double next to it, and [`exact_beside`] says when it could
///   be the one next to it: then the double-double form
///   ([`root_power`]) decides, which is V itself whenever V is a double.
///   (As it does where pow's value has left the normal doubles.)
/// * A part past 20 bits (a root of a huge degree): pow alone near 1,
///   where |p/q − t|·ln a is below 10⁻¹⁹ and only 1 has a double for its
///   root; the double-double form far from it.
fn ratio_power(a: f64, p: i32, q: i32) -> f64 {
    let t = p as f64 / q as f64;
    if q & (q - 1) == 0 {
        return cm::pow(a, t);
    }
    if q == 3 && p.abs() == 1 {
        let r = cm::cbrt(a);
        return if p == 1 { r } else { 1.0 / r };
    }
    if q >= SMALL_PART || p.abs() >= SMALL_PART {
        let binades = ((a.to_bits() >> 52) & 0x7ff) as i64 - 1023;
        return if (binades.unsigned_abs() as f64 + 1.0) * t.abs() <= 5.7 {
            cm::pow(a, t)
        } else {
            root_power(a, p, q)
        };
    }
    let m = cm::pow(a, t);
    if !(m.is_finite() && m >= f64::MIN_POSITIVE) {
        return root_power(a, p, q);
    }
    // p − q·t exactly, t in two halves of at most 27 bits: each product
    // and each difference is a multiple of t's last place that 53 bits
    // hold.
    let (pf, qf) = (p as f64, q as f64);
    let th = f64::from_bits(t.to_bits() & !0x7ff_ffff);
    let rest = ((pf - qf * th) - qf * (t - th)) / qf;
    let v = m + m * (rest * cm::log(a));
    if exact_beside(a, v, p, q) {
        root_power(a, p, q)
    } else {
        v
    }
}

/// Whether a^(p/q) may be a double other than `v`, which is within an ulp
/// of it (a finite a > 0, p/q in lowest terms, q no power of two).
///
/// With a = s·2^e for an odd s, a^(p/q) is rational only for s = g^q and
/// q dividing e, and is then g^p·2^(pe/q). For g = 1, a is a power of two.
/// Otherwise g ≥ 3: for p < 0 that is no double, and g^q ≤ s < 2⁵³ leaves
/// none for q ≥ 34 (3³⁴ > 2⁵³). Else g^p has at most ⌊53·p/q⌋ + 1 bits:
/// for p < q a double so short has none next to it as short, so if `v`
/// isn't one and a neighbour is, the value may be that neighbour; and for
/// p > q, g^p < 2⁵³ leaves s = g^q at most ⌊53·q/p⌋ + 1 bits.
fn exact_beside(a: f64, v: f64, p: i32, q: i32) -> bool {
    // The bits from a double's first set bit to its last.
    let bits = |x: f64| -> u64 {
        let b = x.to_bits();
        let frac = b & ((1 << 52) - 1);
        let m = if (b >> 52) & 0x7ff == 0 {
            frac
        } else {
            frac | (1 << 52)
        };
        if m == 0 {
            0
        } else {
            u64::from(64 - m.leading_zeros() - m.trailing_zeros())
        }
    };
    if bits(a) == 1 {
        return true;
    }
    if p < 0 || q >= 34 {
        return false;
    }
    let (p, q) = (p as u64, q as u64);
    if p > q {
        return bits(a) <= 53 * q / p + 1;
    }
    let most = 53 * p / q + 1;
    bits(v) > most && (bits(v.next_up()) <= most || bits(v.next_down()) <= most)
}

/// `b^(p/q)` for an exponent written as a ratio of integers past
/// [`pow_rational`]'s (`compile::Ratio::small`), any size: sign(b)ᵖ·|b|^(p/q)
/// by the parities of p and q in lowest terms (a negative b only for an
/// odd q; 0 only to a positive power), p/q in double-double (review 15,
/// R15-M-01: x^(1000001/3) took the positive-base rule, undefined at −1).
pub(crate) fn pow_ratio(b: f64, r: &crate::compile::RatioPow) -> f64 {
    crate::wide::pow_ratio(crate::wide::Wide::new(b), *r).to_f64()
}

/// a^(p/q) for a finite a > 0 with the exponent in double-double. Within
/// an ulp, and the exact value whenever that is a double: ln a·p/q is
/// held to 2⁻¹⁰⁰, and of the reduced argument's low part, below 2⁻⁵⁵,
/// [`crate::dd::exp`] keeps nothing, so the mantissa is the correctly
/// rounded exponential of a number within 2⁻⁵⁵ of the true one's
/// logarithm: less than half a last place away.
#[cold]
#[inline(never)]
fn root_power(a: f64, p: i32, q: i32) -> f64 {
    let l = crate::dd::ln(a).mul_f(p as f64).div_f(q as f64);
    let (r, n) = crate::dd::exp(l);
    scale2(r, n)
}

/// r·2ⁿ for an integer-valued n, rounded once (to 0 or ±∞ beyond the
/// doubles).
fn scale2(r: f64, n: f64) -> f64 {
    let n = n.clamp(-2200.0, 2200.0) as i32;
    let h = n / 2;
    // Two halves: each is a power of two a double holds (or its overflow).
    r * 2f64.powi(h) * 2f64.powi(n - h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-12 * (1.0 + b.abs())
    }

    #[test]
    fn trig_units() {
        assert_eq!(sin_u(180.0, TrigUnit::Degrees), 0.0);
        assert_eq!(cos_u(90.0, TrigUnit::Degrees), 0.0);
        assert!(close(sin_u(30.0, TrigUnit::Degrees), 0.5));
        // A pole: undefined, not ±∞ (see `div`).
        assert!(tan_u(90.0, TrigUnit::Degrees).is_nan());
        assert_eq!(sin_u(100.0, TrigUnit::Grads), 1.0);
        assert!(close(asin_u(0.5, TrigUnit::Degrees), 30.0));
        assert!(close(acos_u(0.0, TrigUnit::Grads), 100.0));
        assert!(close(sin_u(-30.0, TrigUnit::Degrees), -0.5));
    }

    #[test]
    fn gamma_and_factorial() {
        assert_eq!(factorial(5.0), 120.0);
        assert_eq!(factorial(0.0), 1.0);
        assert!(close(factorial(0.5), PI.sqrt() / 2.0));
        assert!(close(gamma(0.5), PI.sqrt()));
        assert!(close(gamma(-0.5), -2.0 * PI.sqrt()));
        assert!(factorial(-1.0).is_nan());
        assert!(factorial(150.5).is_finite() && factorial(150.5) > 1e260);
        assert_eq!(double_factorial(7.0), 105.0);
        assert_eq!(ncr(5.0, 2.0), 10.0);
        assert_eq!(npr(5.0, 2.0), 20.0);
    }

    #[test]
    fn roots_and_powers() {
        assert!(close(pow_rational(-8.0, 1, 3), -2.0));
        assert!(close(pow_rational(-8.0, 2, 3), 4.0));
        assert!(pow_rational(-4.0, 1, 2).is_nan());
        assert!(close(root(-32.0, 5.0), -2.0));
        assert!(root(-16.0, 4.0).is_nan());
        assert!(close(log_base(2.0, 8.0), 3.0));
        assert_eq!(modulo(-1.0, 3.0), 2.0);
        assert_eq!(modulo(5.5, 2.0), 1.5);
    }
}
