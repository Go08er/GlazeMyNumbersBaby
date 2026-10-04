//! Closed intervals of doubles with outward rounding, and the basic
//! arithmetic on them.
//!
//! Every operation returns an interval containing the exact result for
//! every real in its inputs. Results are rounded outward with
//! `next_down`/`next_up` of the correctly rounded (round-to-nearest) value,
//! and kept a point where the operation is exact (TwoSum for sums; an
//! `f64::mul_add` residual for products, quotients and square roots, which
//! is exact with or without a hardware FMA unit). No rounding-mode
//! switching, no CPU features assumed.

/// A closed interval `[lo, hi]` of reals, or the empty set.
///
/// Endpoints may be infinite (an unbounded interval); an interval never
/// contains ±∞ itself. The empty interval is `lo = +∞, hi = −∞`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    lo: f64,
    hi: f64,
}

/// The smallest normal magnitude at which a product's or quotient's
/// rounding error is itself exactly representable (2⁻⁹⁶⁹ = 2^(emin + p)).
const EXACT_MIN: f64 = 2.004168360008973e-292;
/// Beyond this magnitude a residual's intermediate could overflow.
const EXACT_MAX: f64 = 1.0e306;

impl Interval {
    /// ∅.
    pub const EMPTY: Interval = Interval {
        lo: f64::INFINITY,
        hi: f64::NEG_INFINITY,
    };
    /// ℝ.
    pub const ENTIRE: Interval = Interval {
        lo: f64::NEG_INFINITY,
        hi: f64::INFINITY,
    };

    /// `[lo, hi]`; empty if `lo > hi` or either is NaN, or if it would
    /// contain no real (`[+∞, +∞]`).
    pub fn new(lo: f64, hi: f64) -> Interval {
        if lo <= hi && lo < f64::INFINITY && hi > f64::NEG_INFINITY {
            Interval {
                lo: if lo == 0.0 { 0.0 } else { lo },
                hi: if hi == 0.0 { 0.0 } else { hi },
            }
        } else {
            Interval::EMPTY
        }
    }

    /// `[x, x]` (empty for a non-finite x).
    pub fn point(x: f64) -> Interval {
        if x.is_finite() {
            Interval::new(x, x)
        } else {
            Interval::EMPTY
        }
    }

    /// The doubles on either side of `x`: an enclosure of any real that
    /// rounds to `x`.
    pub fn around(x: f64) -> Interval {
        if x.is_nan() {
            Interval::EMPTY
        } else {
            Interval::new(x.next_down(), x.next_up())
        }
    }

    pub fn lo(self) -> f64 {
        self.lo
    }

    pub fn hi(self) -> f64 {
        self.hi
    }

    pub fn is_empty(self) -> bool {
        self.lo > self.hi
    }

    pub fn is_entire(self) -> bool {
        self.lo == f64::NEG_INFINITY && self.hi == f64::INFINITY
    }

    pub fn is_bounded(self) -> bool {
        self.is_empty() || (self.lo.is_finite() && self.hi.is_finite())
    }

    pub fn is_point(self) -> bool {
        self.lo == self.hi
    }

    pub fn contains(self, x: f64) -> bool {
        self.lo <= x && x <= self.hi
    }

    /// True if every element is > 0.
    pub fn gt0(self) -> bool {
        !self.is_empty() && self.lo > 0.0
    }

    /// True if every element is < 0.
    pub fn lt0(self) -> bool {
        !self.is_empty() && self.hi < 0.0
    }

    pub fn contains_zero(self) -> bool {
        self.contains(0.0)
    }

    /// A midpoint (finite where possible).
    pub fn mid(self) -> f64 {
        match (self.lo.is_finite(), self.hi.is_finite()) {
            (true, true) => {
                let m = self.lo + (self.hi - self.lo) / 2.0;
                if m.is_finite() {
                    m
                } else {
                    self.lo / 2.0 + self.hi / 2.0
                }
            }
            (true, false) => {
                if self.lo >= 0.0 {
                    if self.lo == 0.0 { 1.0 } else { self.lo * 2.0 }
                } else {
                    0.0
                }
            }
            (false, true) => {
                if self.hi <= 0.0 {
                    if self.hi == 0.0 { -1.0 } else { self.hi * 2.0 }
                } else {
                    0.0
                }
            }
            (false, false) => 0.0,
        }
    }

    /// An upper bound on `hi − lo`.
    pub fn width(self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            sub_up(self.hi, self.lo)
        }
    }

    /// The smallest interval containing both.
    pub fn hull(self, o: Interval) -> Interval {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return self;
        }
        Interval::new(self.lo.min(o.lo), self.hi.max(o.hi))
    }

    pub fn intersect(self, o: Interval) -> Interval {
        Interval::new(self.lo.max(o.lo), self.hi.min(o.hi))
    }

    pub fn subset(self, o: Interval) -> bool {
        self.is_empty() || (o.lo <= self.lo && self.hi <= o.hi)
    }

    /// Absolute value.
    pub fn abs(self) -> Interval {
        if self.is_empty() {
            self
        } else if self.lo >= 0.0 {
            self
        } else if self.hi <= 0.0 {
            -self
        } else {
            Interval::new(0.0, (-self.lo).max(self.hi))
        }
    }

    /// The smallest and largest |x|.
    pub fn mig_mag(self) -> (f64, f64) {
        let a = self.abs();
        (a.lo, a.hi)
    }

    pub fn sqr(self) -> Interval {
        if self.is_empty() {
            return self;
        }
        let (mi, ma) = self.mig_mag();
        Interval::new(mul_down(mi, mi).max(0.0), mul_up(ma, ma))
    }

    pub fn sqrt(self) -> Interval {
        let d = self.intersect(Interval::new(0.0, f64::INFINITY));
        if d.is_empty() {
            return d;
        }
        Interval::new(sqrt_down(d.lo), sqrt_up(d.hi))
    }

    /// 1/x (empty where x is only 0; the hull of the defined values where
    /// 0 is inside).
    pub fn recip(self) -> Interval {
        Interval::point(1.0) / self
    }

    /// The left and right parts of a division by an interval containing 0.
    fn div_by_straddle(self, b: Interval) -> Interval {
        let a = self;
        if a.lo == 0.0 && a.hi == 0.0 {
            return Interval::point(0.0);
        }
        if b.lo == 0.0 {
            // y in (0, b.hi]
            if a.lo >= 0.0 {
                Interval::new(div_down(a.lo, b.hi), f64::INFINITY)
            } else if a.hi <= 0.0 {
                Interval::new(f64::NEG_INFINITY, div_up(a.hi, b.hi))
            } else {
                Interval::ENTIRE
            }
        } else if b.hi == 0.0 {
            // y in [b.lo, 0)
            if a.lo >= 0.0 {
                Interval::new(f64::NEG_INFINITY, div_up(a.lo, b.lo))
            } else if a.hi <= 0.0 {
                Interval::new(div_down(a.hi, b.lo), f64::INFINITY)
            } else {
                Interval::ENTIRE
            }
        } else {
            Interval::ENTIRE
        }
    }
}

impl std::ops::Neg for Interval {
    type Output = Interval;
    fn neg(self) -> Interval {
        if self.is_empty() {
            self
        } else {
            Interval::new(-self.hi, -self.lo)
        }
    }
}

impl std::ops::Add for Interval {
    type Output = Interval;
    fn add(self, o: Interval) -> Interval {
        if self.is_empty() || o.is_empty() {
            return Interval::EMPTY;
        }
        Interval::new(add_down(self.lo, o.lo), add_up(self.hi, o.hi))
    }
}

impl std::ops::Sub for Interval {
    type Output = Interval;
    fn sub(self, o: Interval) -> Interval {
        self + (-o)
    }
}

impl std::ops::Mul for Interval {
    type Output = Interval;
    fn mul(self, o: Interval) -> Interval {
        if self.is_empty() || o.is_empty() {
            return Interval::EMPTY;
        }
        let (a, b) = (self, o);
        let lo = mul_down(a.lo, b.lo)
            .min(mul_down(a.lo, b.hi))
            .min(mul_down(a.hi, b.lo))
            .min(mul_down(a.hi, b.hi));
        let hi = mul_up(a.lo, b.lo)
            .max(mul_up(a.lo, b.hi))
            .max(mul_up(a.hi, b.lo))
            .max(mul_up(a.hi, b.hi));
        Interval::new(lo, hi)
    }
}

impl std::ops::Div for Interval {
    type Output = Interval;
    /// The hull of `{a/b : a ∈ self, b ∈ o, b ≠ 0}`.
    fn div(self, o: Interval) -> Interval {
        if self.is_empty() || o.is_empty() || (o.lo == 0.0 && o.hi == 0.0) {
            return Interval::EMPTY;
        }
        if o.contains_zero() {
            return self.div_by_straddle(o);
        }
        let (a, b) = (self, o);
        let lo = div_down(a.lo, b.lo)
            .min(div_down(a.lo, b.hi))
            .min(div_down(a.hi, b.lo))
            .min(div_down(a.hi, b.hi));
        let hi = div_up(a.lo, b.lo)
            .max(div_up(a.lo, b.hi))
            .max(div_up(a.hi, b.lo))
            .max(div_up(a.hi, b.hi));
        Interval::new(lo, hi)
    }
}

// ---------------------------------------------------------------- endpoints

/// The exact error of `a + b = s` (TwoSum), or NaN on overflow.
#[inline]
fn two_sum_err(a: f64, b: f64, s: f64) -> f64 {
    let bb = s - a;
    (a - (s - bb)) + (b - bb)
}

/// a + b rounded toward −∞.
pub(crate) fn add_down(a: f64, b: f64) -> f64 {
    let s = a + b;
    if !s.is_finite() {
        if a.is_finite() && b.is_finite() && s > 0.0 {
            return f64::MAX;
        }
        return s;
    }
    if two_sum_err(a, b, s) < 0.0 {
        s.next_down()
    } else {
        s
    }
}

/// a + b rounded toward +∞.
pub(crate) fn add_up(a: f64, b: f64) -> f64 {
    let s = a + b;
    if !s.is_finite() {
        if a.is_finite() && b.is_finite() && s < 0.0 {
            return -f64::MAX;
        }
        return s;
    }
    if two_sum_err(a, b, s) > 0.0 {
        s.next_up()
    } else {
        s
    }
}

pub(crate) fn sub_up(a: f64, b: f64) -> f64 {
    add_up(a, -b)
}

/// Sign of the rounding error of the product `a·b = p` (p finite and
/// nonzero): the true product is p + err.
#[inline]
fn mul_err(a: f64, b: f64, p: f64) -> Option<f64> {
    let m = p.abs();
    if (EXACT_MIN..EXACT_MAX).contains(&m) {
        Some(a.mul_add(b, -p))
    } else {
        None
    }
}

/// a·b rounded toward −∞, with 0·∞ = 0 (interval endpoints).
pub(crate) fn mul_down(a: f64, b: f64) -> f64 {
    if a == 0.0 || b == 0.0 {
        return 0.0;
    }
    let p = a * b;
    if !p.is_finite() {
        if a.is_finite() && b.is_finite() && p > 0.0 {
            return f64::MAX;
        }
        return p;
    }
    match mul_err(a, b, p) {
        Some(e) if e >= 0.0 => p,
        _ => p.next_down(),
    }
}

/// a·b rounded toward +∞, with 0·∞ = 0.
pub(crate) fn mul_up(a: f64, b: f64) -> f64 {
    if a == 0.0 || b == 0.0 {
        return 0.0;
    }
    let p = a * b;
    if !p.is_finite() {
        if a.is_finite() && b.is_finite() && p < 0.0 {
            return -f64::MAX;
        }
        return p;
    }
    match mul_err(a, b, p) {
        Some(e) if e <= 0.0 => p,
        _ => p.next_up(),
    }
}

/// The sign of `a/b − q` for the rounded quotient q (finite, nonzero),
/// when it can be computed exactly.
#[inline]
fn div_err_sign(a: f64, b: f64, q: f64) -> Option<f64> {
    let (mq, ma) = (q.abs(), a.abs());
    if (EXACT_MIN..EXACT_MAX).contains(&mq) && (EXACT_MIN..EXACT_MAX).contains(&ma) {
        // r = a − q·b exactly; a/b − q = r/b.
        let r = (-q).mul_add(b, a);
        Some(if r == 0.0 {
            0.0
        } else {
            r.signum() * b.signum()
        })
    } else {
        None
    }
}

/// a/b rounded toward −∞ (b ≠ 0), with x/±∞ = 0 and ±∞/finite = ±∞.
pub(crate) fn div_down(a: f64, b: f64) -> f64 {
    if a == 0.0 {
        return 0.0;
    }
    if b.is_infinite() {
        return if a.is_infinite() {
            f64::NAN
        } else {
            (a.signum() * b.signum()) * 0.0
        };
    }
    let q = a / b;
    if !q.is_finite() {
        if a.is_finite() && q > 0.0 {
            return f64::MAX;
        }
        return q;
    }
    if q == 0.0 {
        // Underflow: the true quotient is nonzero.
        return if (a > 0.0) == (b > 0.0) {
            0.0
        } else {
            (-0.0f64).next_down()
        };
    }
    match div_err_sign(a, b, q) {
        Some(s) if s >= 0.0 => q,
        _ => q.next_down(),
    }
}

/// a/b rounded toward +∞ (b ≠ 0).
pub(crate) fn div_up(a: f64, b: f64) -> f64 {
    if a == 0.0 {
        return 0.0;
    }
    if b.is_infinite() {
        return if a.is_infinite() {
            f64::NAN
        } else {
            (a.signum() * b.signum()) * 0.0
        };
    }
    let q = a / b;
    if !q.is_finite() {
        if a.is_finite() && q < 0.0 {
            return -f64::MAX;
        }
        return q;
    }
    if q == 0.0 {
        return if (a > 0.0) == (b > 0.0) {
            0.0f64.next_up()
        } else {
            0.0
        };
    }
    match div_err_sign(a, b, q) {
        Some(s) if s <= 0.0 => q,
        _ => q.next_up(),
    }
}

/// The sign of `√x − r` for the rounded root r, when exactly computable.
#[inline]
fn sqrt_err_sign(x: f64, r: f64) -> Option<f64> {
    if (EXACT_MIN..EXACT_MAX).contains(&x) {
        // x − r² exactly; √x − r has its sign.
        let d = (-r).mul_add(r, x);
        Some(if d == 0.0 { 0.0 } else { d.signum() })
    } else {
        None
    }
}

/// √x rounded toward −∞ (x ≥ 0).
pub(crate) fn sqrt_down(x: f64) -> f64 {
    if x == 0.0 || x.is_infinite() {
        return x;
    }
    let r = x.sqrt();
    match sqrt_err_sign(x, r) {
        Some(s) if s >= 0.0 => r,
        _ => r.next_down().max(0.0),
    }
}

/// √x rounded toward +∞ (x ≥ 0).
pub(crate) fn sqrt_up(x: f64) -> f64 {
    if x == 0.0 || x.is_infinite() {
        return x;
    }
    let r = x.sqrt();
    match sqrt_err_sign(x, r) {
        Some(s) if s <= 0.0 => r,
        _ => r.next_up(),
    }
}

/// An enclosure of a correctly rounded (to nearest) value of a
/// non-exact operation: the doubles on either side, clamped to `[min,
/// max]` (the operation's known range).
#[inline]
pub(crate) fn widen(r: f64, min: f64, max: f64) -> Interval {
    if r.is_nan() {
        return Interval::EMPTY;
    }
    Interval::new(r.next_down().max(min), r.next_up().min(max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_operations_stay_points() {
        let p = |x: f64| Interval::point(x);
        assert_eq!(p(0.5) + p(0.25), p(0.75));
        assert_eq!(p(3.0) * p(7.0), p(21.0));
        assert_eq!(p(1.0) / p(4.0), p(0.25));
        assert_eq!(p(9.0).sqrt(), p(3.0));
        // 0.1 + 0.2 is not 0.30000000000000004 exactly.
        let s = p(0.1) + p(0.2);
        assert!(s.lo() < s.hi() && s.contains(0.30000000000000004));
        let q = p(1.0) / p(3.0);
        assert_eq!(q.hi(), q.lo().next_up());
    }

    #[test]
    fn division_by_intervals_with_zero() {
        let i = Interval::new;
        assert!((i(1.0, 2.0) / i(0.0, 0.0)).is_empty());
        assert_eq!(i(1.0, 2.0) / i(0.0, 4.0), i(0.25, f64::INFINITY));
        assert_eq!(i(-2.0, -1.0) / i(0.0, 4.0), i(f64::NEG_INFINITY, -0.25));
        assert!((i(1.0, 2.0) / i(-1.0, 1.0)).is_entire());
        assert_eq!(i(0.0, 0.0) / i(-1.0, 1.0), Interval::point(0.0));
    }

    #[test]
    fn overflow_and_infinite_endpoints() {
        let big = Interval::point(f64::MAX);
        let s = big + big;
        assert_eq!((s.lo(), s.hi()), (f64::MAX, f64::INFINITY));
        let m = Interval::new(0.0, 1.0) * Interval::new(1.0, f64::INFINITY);
        assert_eq!((m.lo(), m.hi()), (0.0, f64::INFINITY));
        assert_eq!(
            Interval::point(0.0) * Interval::ENTIRE,
            Interval::point(0.0)
        );
        let tiny = Interval::point(1e-200) * Interval::point(1e-200);
        assert!(tiny.contains(0.0) && tiny.hi() > 0.0);
    }
}
