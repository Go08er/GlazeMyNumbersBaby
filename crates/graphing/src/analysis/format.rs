//! Formatting of analysis results: "nice" recognition of numbers that are
//! (numerically indistinguishable from) integers, simple fractions, rational
//! multiples of π or e, or simple surds; intervals; sets; periodic families.

/// Typographic minus sign used in analysis strings.
pub const MINUS: &str = "−";

/// A recognised closed form of a number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Nice {
    /// p/q (q ≥ 1).
    Rational(i64, i64),
    /// (p/q)·π.
    Pi(i64, i64),
    /// (p/q)·e.
    E(i64, i64),
    /// p/(q·e).
    InvE(i64, i64),
    /// sign·(s·√r)/q.
    Surd { neg: bool, s: i64, r: i64, q: i64 },
    /// No closed form found.
    Decimal(f64),
}

const REL_TOL: f64 = 1e-9;

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

fn rational(v: f64, max_den: i64, max_num: f64, tol: f64) -> Option<(i64, i64)> {
    let scale = v.abs().max(1.0);
    for q in 1..=max_den {
        let p = (v * q as f64).round();
        if p.abs() > max_num {
            return None;
        }
        // A fraction with a numerator in the millions (30994059/7) is no
        // closed form anyone would recognise; whole numbers can be large.
        if q > 1 && p.abs() > 1e6 {
            return None;
        }
        if (v - p / q as f64).abs() <= tol * scale {
            let g = gcd(p as i64, q).max(1);
            return Some((p as i64 / g, q / g));
        }
    }
    None
}

impl Nice {
    /// Recognises `v` for display: within 1e-9 of it relative to its own
    /// size, and no more than 10⁻⁶ off. So 999999998.7 is not shown as
    /// the integer 999999999, nor 10⁻¹⁰ as 0 (values reaching display are
    /// already cleared of noise).
    pub fn of(v: f64) -> Nice {
        let a = v.abs();
        Nice::with_tol(
            v,
            if a < 1.0 {
                REL_TOL * a
            } else {
                REL_TOL.min(1e-6 / a)
            },
        )
    }

    /// Recognises `v` with a relative tolerance.
    pub fn with_tol(v: f64, tol: f64) -> Nice {
        if !v.is_finite() {
            return Nice::Decimal(v);
        }
        if let Some((p, q)) = rational(v, 16, 1e15, tol) {
            return Nice::Rational(p, q);
        }
        // Larger denominators only for a very tight fit.
        if let Some((p, q)) = rational(v, 64, 1e9, tol * 1e-3) {
            return Nice::Rational(p, q);
        }
        if v.abs() < 1e6 {
            if let Some((p, q)) = rational(v / std::f64::consts::PI, 12, 48.0, tol)
                && p != 0
            {
                return Nice::Pi(p, q);
            }
            if let Some((p, q)) = rational(v * v, 16, 1e5, tol)
                && p > 0
            {
                // √(p/q) = √(p·q)/q, then pull square factors out.
                let mut r = p * q;
                let mut s = 1;
                let mut f = 2;
                while f * f <= r {
                    while r % (f * f) == 0 {
                        r /= f * f;
                        s *= f;
                    }
                    f += 1;
                }
                if r > 1 && r <= 1000 {
                    let g = gcd(s, q).max(1);
                    return Nice::Surd {
                        neg: v < 0.0,
                        s: s / g,
                        r,
                        q: q / g,
                    };
                }
            }
            if let Some((p, q)) = rational(v / std::f64::consts::E, 4, 12.0, tol)
                && p != 0
            {
                return Nice::E(p, q);
            }
            if let Some((p, q)) = rational(v * std::f64::consts::E, 4, 12.0, tol)
                && p != 0
            {
                return Nice::InvE(p, q);
            }
        }
        Nice::Decimal(v)
    }

    /// Numeric value.
    pub fn value(&self) -> f64 {
        match *self {
            Nice::Rational(p, q) => p as f64 / q as f64,
            Nice::Pi(p, q) => p as f64 * std::f64::consts::PI / q as f64,
            Nice::E(p, q) => p as f64 * std::f64::consts::E / q as f64,
            Nice::InvE(p, q) => p as f64 / (q as f64 * std::f64::consts::E),
            Nice::Surd { neg, s, r, q } => {
                let v = s as f64 * (r as f64).sqrt() / q as f64;
                if neg { -v } else { v }
            }
            Nice::Decimal(v) => v,
        }
    }

    /// `k`-multiple form for periodic families, e.g. `2kπ`, `kπ/2`, `360k`.
    pub fn times_k(&self) -> String {
        let sym = |p: i64, q: i64, s: &str| -> String {
            let sign = if p < 0 { MINUS } else { "" };
            let p = p.abs();
            let num = if p == 1 {
                format!("k{s}")
            } else {
                format!("{p}k{s}")
            };
            if q == 1 {
                format!("{sign}{num}")
            } else {
                format!("{sign}{num}/{q}")
            }
        };
        match *self {
            Nice::Rational(p, q) => sym(p, q, ""),
            Nice::Pi(p, q) => sym(p, q, "π"),
            Nice::E(p, q) => sym(p, q, "e"),
            _ => format!("{}k", self),
        }
    }
}

impl std::fmt::Display for Nice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let frac =
            |f: &mut std::fmt::Formatter<'_>, p: i64, q: i64, sym: &str| -> std::fmt::Result {
                let sign = if p < 0 { MINUS } else { "" };
                let p = p.abs();
                let num = if sym.is_empty() {
                    p.to_string()
                } else if p == 1 {
                    sym.to_string()
                } else {
                    format!("{p}{sym}")
                };
                if q == 1 {
                    write!(f, "{sign}{num}")
                } else {
                    write!(f, "{sign}{num}/{q}")
                }
            };
        match *self {
            Nice::Rational(p, q) => frac(f, p, q, ""),
            Nice::Pi(p, q) => frac(f, p, q, "π"),
            Nice::E(p, q) => frac(f, p, q, "e"),
            Nice::InvE(p, q) => {
                let sign = if p < 0 { MINUS } else { "" };
                if q == 1 {
                    write!(f, "{sign}{}/e", p.abs())
                } else {
                    write!(f, "{sign}{}/({q}e)", p.abs())
                }
            }
            Nice::Surd { neg, s, r, q } => {
                let sign = if neg { MINUS } else { "" };
                let coef = if s == 1 { String::new() } else { s.to_string() };
                if q == 1 {
                    write!(f, "{sign}{coef}√{r}")
                } else {
                    write!(f, "{sign}{coef}√{r}/{q}")
                }
            }
            Nice::Decimal(v) => write!(f, "{}", format_decimal(v)),
        }
    }
}

/// Formats a value with recognition of closed forms (default tolerance).
pub fn format_number(v: f64) -> String {
    Nice::of(v).to_string()
}

/// Formats a value with a custom relative tolerance for recognition.
pub fn format_number_tol(v: f64, tol: f64) -> String {
    Nice::with_tol(v, tol).to_string()
}

pub(crate) fn superscript(n: i32) -> String {
    n.to_string()
        .chars()
        .map(|c| match c {
            '-' => '⁻',
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            _ => '⁹',
        })
        .collect()
}

/// Formats `v` like [`format_number`], with as many more significant
/// digits as it takes to tell it apart from each of `others`: a zero 10⁻⁶
/// beside a pole at 2 shows as 1.999999, not as 2.
pub fn format_number_apart(v: f64, others: &[f64]) -> String {
    let s = format_number(v);
    if !others.iter().any(|&o| looks_same(v, o)) {
        return s;
    }
    let mut out = String::new();
    for sig in 7..=17 {
        out = format_decimal_digits(v, sig);
        if others
            .iter()
            .all(|&o| o == v || format_decimal_digits(o, sig) != out)
        {
            break;
        }
    }
    out
}

/// Distinct values that read the same, as closed forms or as decimals
/// (π/2 and 1.5708 both).
fn looks_same(v: f64, o: f64) -> bool {
    let shown = format_number(v);
    o != v && o.is_finite() && (format_number(o) == shown || format_decimal(o) == shown)
}

/// Decimal formatting with 6 significant digits, trailing zeros trimmed;
/// `m×10ⁿ` for very large/small magnitudes; `∞` for infinities.
pub fn format_decimal(v: f64) -> String {
    format_decimal_digits(v, 6)
}

/// [`format_decimal`] with `sig` significant digits.
pub(crate) fn format_decimal_digits(v: f64, sig: i32) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 {
            "∞".into()
        } else {
            format!("{MINUS}∞")
        };
    }
    if v == 0.0 {
        return "0".into();
    }
    let sign = if v < 0.0 { MINUS } else { "" };
    let a = v.abs();
    let e = a.log10().floor() as i32;
    // Beyond 6 digits (telling points apart), whole numbers up to 10¹⁶
    // stay whole: 1000000000002, not 1.000000000002×10¹².
    if !(-5..9).contains(&e) && !(sig > 6 && (9..16).contains(&e)) {
        let digits = (sig - 1) as usize;
        // In two steps: 10⁻³²⁴ itself is below the doubles.
        let mut m = a / 10f64.powi(e / 2) / 10f64.powi(e - e / 2);
        let mut e = e;
        if format!("{m:.digits$}").starts_with("10") {
            m /= 10.0;
            e += 1;
        }
        let ms = trim(&format!("{m:.digits$}"));
        return format!("{sign}{ms}×10{}", superscript(e));
    }
    // `sig` significant digits, but large values keep a few decimals so
    // nearby points stay apart (a maximum at 2000000.5 between zeros at
    // 2000000 and 2000001).
    let decimals = if e >= 5 {
        (sig + 3 - e).max(0)
    } else {
        (sig - 1 - e).clamp(0, sig + 6)
    } as usize;
    let s = trim(&format!("{a:.decimals$}"));
    if s == "0" {
        "0".into()
    } else {
        format!("{sign}{s}")
    }
}

fn trim(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

/// A point `(x, y)`.
pub fn format_point(x: f64, y: f64) -> String {
    format!("({}, {})", format_number(x), format_number(y))
}

/// `rep + k·period` (`k ∈ ℤ` implied), e.g. `π/2 + kπ`, `kπ`, `−π/2 + 2kπ`.
pub fn format_family(rep: f64, period: f64) -> String {
    let p = Nice::of(period).times_k();
    let r = Nice::of(rep);
    if r.value() == 0.0 {
        return p;
    }
    format!("{} + {}", r, p)
}

/// [`format_family`], with the digits that tell `rep` apart from each of
/// `others` (other points, compared at their copies nearest `rep`): the
/// zeros of tan x − 10⁹ sit 10⁻⁹ before the poles at π/2 + kπ.
pub fn format_family_apart(rep: f64, period: f64, others: &[f64]) -> String {
    let s = format_family(rep, period);
    let near: Vec<f64> = others
        .iter()
        .map(|&o| o + ((rep - o) / period).round() * period)
        .collect();
    if !near.iter().any(|&o| looks_same(rep, o)) {
        return s;
    }
    format!(
        "{} + {}",
        format_number_apart(rep, &near),
        Nice::of(period).times_k()
    )
}

/// One end of an interval.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bound {
    /// Value (may be ±∞).
    pub value: f64,
    /// Whether the end point belongs to the interval.
    pub closed: bool,
}

/// An interval of the real line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    pub lo: Bound,
    pub hi: Bound,
}

impl Interval {
    /// The whole real line.
    pub fn all() -> Interval {
        Interval {
            lo: Bound {
                value: f64::NEG_INFINITY,
                closed: false,
            },
            hi: Bound {
                value: f64::INFINITY,
                closed: false,
            },
        }
    }

    /// Open interval.
    pub fn open(lo: f64, hi: f64) -> Interval {
        Interval {
            lo: Bound {
                value: lo,
                closed: false,
            },
            hi: Bound {
                value: hi,
                closed: false,
            },
        }
    }

    /// Closed interval (infinite ends are always open).
    pub fn closed(lo: f64, hi: f64) -> Interval {
        Interval {
            lo: Bound {
                value: lo,
                closed: lo.is_finite(),
            },
            hi: Bound {
                value: hi,
                closed: hi.is_finite(),
            },
        }
    }

    /// True if `v` is inside.
    pub fn contains(&self, v: f64) -> bool {
        (v > self.lo.value || (self.lo.closed && v == self.lo.value))
            && (v < self.hi.value || (self.hi.closed && v == self.hi.value))
    }

    /// True for a single point `[a, a]`.
    pub fn is_point(&self) -> bool {
        self.lo.value == self.hi.value
    }

    /// `(a, b]`-style text.
    pub fn format(&self) -> String {
        self.format_with(&format_number)
    }

    /// `(a, b]`-style text, finite ends written by `num`.
    pub fn format_with(&self, num: &dyn Fn(f64) -> String) -> String {
        let l = if self.lo.closed { "[" } else { "(" };
        let r = if self.hi.closed { "]" } else { ")" };
        format!(
            "{l}{}, {}{r}",
            fmt_end(self.lo.value, num),
            fmt_end(self.hi.value, num)
        )
    }

    /// Interval text with each finite end shifted by `k·period`.
    pub fn format_periodic(&self, period: f64) -> String {
        let l = if self.lo.closed { "[" } else { "(" };
        let r = if self.hi.closed { "]" } else { ")" };
        format!(
            "{l}{}, {}{r}",
            format_family(self.lo.value, period),
            format_family(self.hi.value, period)
        )
    }
}

fn fmt_end(v: f64, num: &dyn Fn(f64) -> String) -> String {
    if v == f64::INFINITY {
        "∞".into()
    } else if v == f64::NEG_INFINITY {
        format!("{MINUS}∞")
    } else {
        num(v)
    }
}

/// Like [`format_number`], but never writes a nonzero value as 0: for
/// values already cleared of rounding noise, where a tiny bound (the
/// maximum of 1/(x² − 4·10⁸) at −2.5·10⁻⁹, or far smaller) is genuine.
pub fn format_nonzero(v: f64) -> String {
    let n = Nice::of(v);
    if v != 0.0 && n.value() == 0.0 {
        format_decimal(v)
    } else {
        n.to_string()
    }
}

/// Formats a union of disjoint, sorted intervals as `var ∈ …`:
/// `x ∈ ℝ`, `x ∈ ℝ \ {0}`, `y ∈ [0, ∞)`, `x ∈ (−∞, −1] ∪ [1, ∞)`, `y ∈ {5}`.
pub fn format_set(var: &str, parts: &[Interval]) -> String {
    format_set_with(var, parts, &format_number)
}

/// [`format_set`] with finite numbers written by `num`.
pub fn format_set_with(var: &str, parts: &[Interval], num: &dyn Fn(f64) -> String) -> String {
    if parts.is_empty() {
        return format!("{var} ∈ ∅");
    }
    let all_reals_but_points = parts
        .first()
        .is_some_and(|p| p.lo.value == f64::NEG_INFINITY)
        && parts.last().is_some_and(|p| p.hi.value == f64::INFINITY)
        && parts
            .windows(2)
            .all(|w| w[0].hi.value == w[1].lo.value && !w[0].hi.closed && !w[1].lo.closed);
    if all_reals_but_points {
        if parts.len() == 1 {
            return format!("{var} ∈ ℝ");
        }
        let pts: Vec<String> = parts[..parts.len() - 1]
            .iter()
            .map(|p| num(p.hi.value))
            .collect();
        return format!("{var} ∈ ℝ \\ {{{}}}", pts.join(", "));
    }
    if parts.iter().all(|p| p.is_point()) {
        let pts: Vec<String> = parts.iter().map(|p| num(p.lo.value)).collect();
        return format!("{var} ∈ {{{}}}", pts.join(", "));
    }
    let items: Vec<String> = parts
        .iter()
        .map(|p| {
            if p.is_point() {
                format!("{{{}}}", num(p.lo.value))
            } else {
                p.format_with(num)
            }
        })
        .collect();
    format!("{var} ∈ {}", items.join(" ∪ "))
}

/// Formats a periodic set given by its parts within one period.
pub fn format_periodic_set(var: &str, parts: &[Interval], excluded: &[f64], period: f64) -> String {
    if parts.is_empty() {
        // ℝ minus periodic points.
        let pts: Vec<String> = excluded.iter().map(|&e| format_family(e, period)).collect();
        return format!("{var} ∈ ℝ \\ {{{} | k ∈ ℤ}}", pts.join(", "));
    }
    let items: Vec<String> = parts.iter().map(|p| p.format_periodic(period)).collect();
    format!("{var} ∈ {}, k ∈ ℤ", items.join(" ∪ "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{E, PI};

    #[test]
    fn nice_numbers() {
        assert_eq!(format_number(2.0), "2");
        assert_eq!(format_number(-3.0), "−3");
        assert_eq!(format_number(0.5), "1/2");
        assert_eq!(format_number(-2.0 / 3.0), "−2/3");
        assert_eq!(format_number(PI), "π");
        assert_eq!(format_number(-PI / 2.0), "−π/2");
        assert_eq!(format_number(3.0 * PI / 4.0), "3π/4");
        assert_eq!(format_number(2.0 * PI), "2π");
        assert_eq!(format_number(2f64.sqrt()), "√2");
        assert_eq!(format_number(-(2f64.sqrt()) / 2.0), "−√2/2");
        assert_eq!(format_number(1.5 * 3f64.sqrt()), "3√3/2");
        assert_eq!(format_number(E), "e");
        assert_eq!(format_number(-1.0 / E), "−1/e");
        assert_eq!(format_number(19.0 / 36.0), "19/36");
        // Noise is cleared before display; what reaches it is a value
        // (the zero of x − 10⁻¹⁰, the period of sin(10¹²x)).
        assert_eq!(format_number(1e-17), "1×10⁻¹⁷");
        // R11-L-03: a decimal exponent whose power of ten is below the
        // doubles.
        assert_eq!(format_number(-f64::from_bits(1)), "−4.94066×10⁻³²⁴");
        assert_eq!(format_number(f64::MIN_POSITIVE), "2.22507×10⁻³⁰⁸");
        assert_eq!(format_number(1e-10), "1×10⁻¹⁰");
        assert_eq!(format_number(2.0 * PI * 1e-12), "6.28319×10⁻¹²");
        assert_eq!(format_number(999999998.7), "999999998.7");
        assert_eq!(format_number(1.23456789), "1.23457");
        assert_eq!(format_number(-0.000123456), "−0.000123456");
        assert_eq!(format_number(1.5e12), "1500000000000");
        assert_eq!(format_number(1.234567e-7), "1.23457×10⁻⁷");
        assert_eq!(format_number(1.2345678e15 + 1.0), "1.23457×10¹⁵");
    }

    #[test]
    fn families_and_sets() {
        assert_eq!(format_family(0.0, PI), "kπ");
        assert_eq!(format_family(PI / 2.0, PI), "π/2 + kπ");
        assert_eq!(format_family(-PI / 2.0, 2.0 * PI), "−π/2 + 2kπ");
        assert_eq!(format_family(90.0, 180.0), "90 + 180k");
        assert_eq!(format_set("x", &[Interval::all()]), "x ∈ ℝ");
        assert_eq!(
            format_set(
                "x",
                &[
                    Interval::open(f64::NEG_INFINITY, 0.0),
                    Interval::open(0.0, f64::INFINITY)
                ]
            ),
            "x ∈ ℝ \\ {0}"
        );
        assert_eq!(
            format_set("y", &[Interval::closed(0.0, f64::INFINITY)]),
            "y ∈ [0, ∞)"
        );
        assert_eq!(format_set("y", &[Interval::closed(5.0, 5.0)]), "y ∈ {5}");
        assert_eq!(
            format_set(
                "x",
                &[
                    Interval::closed(f64::NEG_INFINITY, -1.0),
                    Interval::closed(1.0, f64::INFINITY)
                ]
            ),
            "x ∈ (−∞, −1] ∪ [1, ∞)"
        );
        assert_eq!(
            format_periodic_set("x", &[], &[PI / 2.0], PI),
            "x ∈ ℝ \\ {π/2 + kπ | k ∈ ℤ}"
        );
    }
}
