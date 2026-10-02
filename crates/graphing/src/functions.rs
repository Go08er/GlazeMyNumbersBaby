//! Real-valued implementations of the built-in functions.
//!
//! Everything works on the real number field (like the original graphing
//! engine's `EvalNumberField::Real`): out-of-domain inputs produce NaN and
//! poles produce ±∞, which the plotter treats as gaps.

use std::f64::consts::PI;

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

/// sin and cos of an angle in the given unit. For degrees/grads, exact
/// quarter turns give exact 0/±1 so that e.g. `tan(90)` is a pole and
/// `sin(180)` is exactly 0.
#[inline]
pub fn sin_cos(x: f64, unit: TrigUnit) -> (f64, f64) {
    match unit {
        TrigUnit::Radians => x.sin_cos(),
        _ => {
            if !x.is_finite() {
                return (f64::NAN, f64::NAN);
            }
            let turn = unit.full_turn();
            let r = x.rem_euclid(turn);
            let quarter = turn / 4.0;
            let q = r / quarter;
            if q == q.trunc() {
                return match q as i64 {
                    0 => (0.0, 1.0),
                    1 => (1.0, 0.0),
                    2 => (0.0, -1.0),
                    _ => (-1.0, 0.0),
                };
            }
            (r * unit.to_radians_factor()).sin_cos()
        }
    }
}

#[inline]
pub fn sin_u(x: f64, unit: TrigUnit) -> f64 {
    if unit == TrigUnit::Radians {
        x.sin()
    } else {
        sin_cos(x, unit).0
    }
}

#[inline]
pub fn cos_u(x: f64, unit: TrigUnit) -> f64 {
    if unit == TrigUnit::Radians {
        x.cos()
    } else {
        sin_cos(x, unit).1
    }
}

#[inline]
pub fn tan_u(x: f64, unit: TrigUnit) -> f64 {
    if unit == TrigUnit::Radians {
        x.tan()
    } else {
        let (s, c) = sin_cos(x, unit);
        if c == 0.0 {
            f64::INFINITY.copysign(s)
        } else {
            s / c
        }
    }
}

#[inline]
pub fn cot_u(x: f64, unit: TrigUnit) -> f64 {
    let (s, c) = sin_cos(x, unit);
    if s == 0.0 {
        f64::INFINITY.copysign(c)
    } else {
        c / s
    }
}

#[inline]
pub fn sec_u(x: f64, unit: TrigUnit) -> f64 {
    1.0 / cos_u(x, unit)
}

#[inline]
pub fn csc_u(x: f64, unit: TrigUnit) -> f64 {
    1.0 / sin_u(x, unit)
}

#[inline]
fn from_rad(r: f64, unit: TrigUnit) -> f64 {
    if unit == TrigUnit::Radians {
        r
    } else {
        r / unit.to_radians_factor()
    }
}

#[inline]
pub fn asin_u(x: f64, unit: TrigUnit) -> f64 {
    from_rad(x.asin(), unit)
}

#[inline]
pub fn acos_u(x: f64, unit: TrigUnit) -> f64 {
    from_rad(x.acos(), unit)
}

#[inline]
pub fn atan_u(x: f64, unit: TrigUnit) -> f64 {
    from_rad(x.atan(), unit)
}

/// arcsec(x) = arccos(1/x).
#[inline]
pub fn asec_u(x: f64, unit: TrigUnit) -> f64 {
    from_rad((1.0 / x).acos(), unit)
}

/// arccsc(x) = arcsin(1/x).
#[inline]
pub fn acsc_u(x: f64, unit: TrigUnit) -> f64 {
    from_rad((1.0 / x).asin(), unit)
}

/// arccot(x) = π/2 − arctan(x) (continuous, range (0, π)).
#[inline]
pub fn acot_u(x: f64, unit: TrigUnit) -> f64 {
    from_rad(PI / 2.0 - x.atan(), unit)
}

#[inline]
pub fn sech(x: f64) -> f64 {
    1.0 / x.cosh()
}

#[inline]
pub fn csch(x: f64) -> f64 {
    1.0 / x.sinh()
}

#[inline]
pub fn coth(x: f64) -> f64 {
    // Not cosh/sinh: both overflow for |x| ≳ 710 and the ratio becomes NaN.
    1.0 / x.tanh()
}

#[inline]
pub fn asech(x: f64) -> f64 {
    (1.0 / x).acosh()
}

#[inline]
pub fn acsch(x: f64) -> f64 {
    (1.0 / x).asinh()
}

#[inline]
pub fn acoth(x: f64) -> f64 {
    (1.0 / x).atanh()
}

/// `root(x, n)`: real n-th root. Odd integer n accepts negative x.
#[inline]
pub fn root(x: f64, n: f64) -> f64 {
    if n == 0.0 || !n.is_finite() {
        return f64::NAN;
    }
    if is_odd_integer(n) {
        if n == 3.0 {
            return x.cbrt();
        }
        let r = x.abs().powf(1.0 / n);
        return if x < 0.0 { -r } else { r };
    }
    if x < 0.0 {
        return f64::NAN;
    }
    if n == 2.0 {
        return x.sqrt();
    }
    x.powf(1.0 / n)
}

/// Logarithm of `x` in base `b` (`log(b, x)`).
#[inline]
pub fn log_base(b: f64, x: f64) -> f64 {
    if b <= 0.0 || b == 1.0 {
        return f64::NAN;
    }
    if b == 10.0 {
        return x.log10();
    }
    if b == 2.0 {
        return x.log2();
    }
    x.ln() / b.ln()
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

const LANCZOS_G: f64 = 7.0;
const LANCZOS: [f64; 9] = [
    0.999_999_999_999_809_9,
    676.520_368_121_885_1,
    -1_259.139_216_722_402_8,
    771.323_428_777_653_1,
    -176.615_029_162_140_6,
    12.507_343_278_686_905,
    -0.138_571_095_265_720_12,
    9.984_369_578_019_572e-6,
    1.505_632_735_149_311_6e-7,
];

/// The gamma function Γ(x). Poles (0, −1, −2, …) give NaN.
pub fn gamma(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x == x.trunc() {
        if x <= 0.0 {
            return f64::NAN;
        }
        if x <= 171.0 {
            // Exact for integers (up to f64 rounding).
            let mut r = 1.0;
            let mut k = 2.0;
            while k < x {
                r *= k;
                k += 1.0;
            }
            return r;
        }
        return f64::INFINITY;
    }
    if x < 0.5 {
        // Reflection formula.
        let s = (PI * x).sin();
        return PI / (s * gamma(1.0 - x));
    }
    if x > 171.7 {
        return f64::INFINITY;
    }
    let x = x - 1.0;
    let mut a = LANCZOS[0];
    let t = x + LANCZOS_G + 0.5;
    for (i, c) in LANCZOS.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    // t^(x+½) overflows before Γ does: split it around e^(−t).
    let half = t.powf(0.5 * (x + 0.5));
    (2.0 * PI).sqrt() * half * ((-t).exp() * half) * a
}

/// ln Γ(x) for x > 0, computed in log space so it never overflows (Γ
/// itself does beyond x ≈ 171.6).
fn ln_gamma(x: f64) -> f64 {
    if x < 0.5 {
        // Reflection; sin(πx) > 0 on (0, ½).
        return (PI / (PI * x).sin()).ln() - ln_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = LANCZOS[0];
    let t = x + LANCZOS_G + 0.5;
    for (i, c) in LANCZOS.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
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
fn is_odd_integer(n: f64) -> bool {
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
    (ln_gamma(n + 1.0) - ln_gamma(r + 1.0) - ln_gamma(n - r + 1.0)).exp()
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
    (ln_gamma(n + 1.0) - ln_gamma(n - r + 1.0)).exp()
}

/// `b^(p/q)` with real-root semantics: a negative base is allowed when q is
/// odd (e.g. `(-8)^(1/3) = -2`, `(-8)^(2/3) = 4`).
#[inline]
pub fn pow_rational(b: f64, p: i32, q: i32) -> f64 {
    if b >= 0.0 || q % 2 == 0 {
        if q == 2 && p == 1 {
            return b.sqrt();
        }
        return b.powf(p as f64 / q as f64);
    }
    let m = if q == 3 {
        (-b).cbrt().powi(p)
    } else {
        (-b).powf(p as f64 / q as f64)
    };
    if p % 2 == 0 { m } else { -m }
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
        assert!(tan_u(90.0, TrigUnit::Degrees).is_infinite());
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
