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
    if n < 0 { 1.0 / p } else { p }
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
    if is_odd_integer(n) {
        if n == 3.0 {
            return cm::cbrt(x);
        }
        let r = cm::pow(x.abs(), 1.0 / n);
        return if x < 0.0 { -r } else { r };
    }
    if x < 0.0 {
        return f64::NAN;
    }
    if n == 2.0 {
        return x.sqrt();
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

/// ln Γ(x), correctly rounded (CORE-MATH), where Γ(x) > 0; NaN where it is
/// negative or at a pole. It never overflows where Γ itself does (beyond
/// x ≈ 171.6).
pub(crate) fn ln_gamma(x: f64) -> f64 {
    if x.is_nan() || (x <= 0.0 && (x == x.trunc() || x.floor() % 2.0 != 0.0)) {
        return f64::NAN;
    }
    cm::lgamma(x)
}

/// `n!` extended to the reals as Γ(n+1).
#[inline]
pub fn factorial(n: f64) -> f64 {
    gamma(n + 1.0)
}

/// `n!!` for integers n ≥ −1 (NaN otherwise).
pub fn double_factorial(n: f64) -> f64 {
    if n != n.trunc() || n < -1.0 {
        return f64::NAN;
    }
    if n > 300.0 {
        return f64::INFINITY;
    }
    let mut r = 1.0;
    let mut k = n;
    while k > 1.0 {
        r *= k;
        k -= 2.0;
    }
    r
}

/// True for odd integers. Every `f64` at or beyond 2^53 is an even integer
/// (a saturating `as i64` cast would wrongly report `i64::MAX`, i.e. odd).
pub(crate) fn is_odd_integer(n: f64) -> bool {
    n == n.trunc() && n.abs() < 9_007_199_254_740_992.0 && n % 2.0 != 0.0
}

/// Number of combinations `nCr(n, r)` (generalised through Γ).
pub fn ncr(n: f64, r: f64) -> f64 {
    if !n.is_finite() || !r.is_finite() {
        return f64::NAN;
    }
    if n == n.trunc() && r == r.trunc() && n >= 0.0 {
        if r < 0.0 || r > n {
            return 0.0;
        }
        let r = r.min(n - r);
        let mut acc = 1.0;
        let mut i = 1.0;
        while i <= r {
            // acc = C(n−r+i−1, i−1). Multiplying first keeps small results
            // exact; when that product alone overflows, divide first, since
            // the coefficient itself may still fit (nCr(1021, 510) ≈ 5.6e305).
            let wide = acc * (n - r + i);
            acc = if wide.is_finite() {
                wide / i
            } else {
                acc / i * (n - r + i)
            };
            // C(n, r) ≥ 2^r here, so a long loop overflows within ~1100
            // steps; stop there instead of iterating up to r (≤ 10^19…).
            if !acc.is_finite() {
                return f64::INFINITY;
            }
            i += 1.0;
        }
        return acc.round();
    }
    let direct = factorial(n) / (factorial(r) * factorial(n - r));
    if direct.is_finite() || n + 1.0 <= 0.0 || r + 1.0 <= 0.0 || n - r + 1.0 <= 0.0 {
        return direct;
    }
    // Γ(n+1) overflows long before the quotient does.
    cm::exp(ln_gamma(n + 1.0) - ln_gamma(r + 1.0) - ln_gamma(n - r + 1.0))
}

/// Number of permutations `nPr(n, r)` (generalised through Γ).
pub fn npr(n: f64, r: f64) -> f64 {
    if !n.is_finite() || !r.is_finite() {
        return f64::NAN;
    }
    if n == n.trunc() && r == r.trunc() && n >= 0.0 {
        if r < 0.0 || r > n {
            return 0.0;
        }
        let mut acc = 1.0;
        let mut i = 0.0;
        while i < r {
            acc *= n - i;
            // Factors are ≥ 2 until the last one, so this overflows quickly
            // whenever the loop would be long.
            if !acc.is_finite() {
                return f64::INFINITY;
            }
            i += 1.0;
        }
        return acc;
    }
    let direct = factorial(n) / factorial(n - r);
    if direct.is_finite() || n + 1.0 <= 0.0 || n - r + 1.0 <= 0.0 {
        return direct;
    }
    cm::exp(ln_gamma(n + 1.0) - ln_gamma(n - r + 1.0))
}

/// `b^(p/q)` with real-root semantics: a negative base is allowed when q is
/// odd (e.g. `(-8)^(1/3) = -2`, `(-8)^(2/3) = 4`).
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
    // p/q as a double is rounded (1/3 by 2⁻⁵⁶), and pow, though itself
    // correctly rounded, magnifies that by |ln a·p/q|. Up to |ln a·p/q| ≈ 4
    // that costs at most about 2 ulps and pow is used (|ln a| judged by a's
    // binary exponent, no logarithm); beyond, see [`root_power`].
    let t = p as f64 / q as f64;
    let binades = ((a.to_bits() >> 52) & 0x7ff) as i64 - 1023;
    let m = if !(a > 0.0 && a.is_finite()) || (binades.unsigned_abs() as f64 + 1.0) * t.abs() <= 5.7
    {
        cm::pow(a, t)
    } else {
        root_power(a, p, q)
    };
    if neg && p % 2 != 0 { -m } else { m }
}

/// a^(p/q) for a > 0 where powf's rounded p/q would cost ulps (x^(1/3) at
/// 10⁴⁵ was 17 ulps off): square and cube roots of small powers through
/// sqrt/cbrt, the rest with the exponent in double-double.
#[cold]
#[inline(never)]
fn root_power(a: f64, p: i32, q: i32) -> f64 {
    if p.abs() <= 3 && (q == 2 || q == 3) {
        let r = if q == 2 { a.sqrt() } else { cm::cbrt(a) };
        return match p {
            1 => r,
            2 => r * r,
            3 => r * r * r,
            -1 => 1.0 / r,
            -2 => 1.0 / (r * r),
            -3 => 1.0 / (r * r * r),
            _ => 1.0,
        };
    }
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
