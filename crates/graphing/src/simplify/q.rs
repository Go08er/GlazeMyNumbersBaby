//! Exact rationals for the simplifier: literals as typed, constants folded
//! without rounding, periods as rational multiples of π (or of 360° and
//! 400 grads). `i128` parts, every operation checked: an overflow gives
//! `None`, never a wrong value.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use crate::interval::Interval;

/// An exact rational `n/d` in lowest terms with `d > 0`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Q {
    n: i128,
    d: i128,
}

fn gcd(mut a: i128, mut b: i128) -> i128 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

// add, sub, mul, div and neg return `None` on overflow (or a zero
// divisor), which the operator traits can't express.
#[allow(clippy::should_implement_trait)]
impl Q {
    /// 0.
    pub const ZERO: Q = Q { n: 0, d: 1 };
    /// 1.
    pub const ONE: Q = Q { n: 1, d: 1 };

    /// `n/d` in lowest terms; `None` for `d = 0` or an overflow.
    pub fn new(n: i128, d: i128) -> Option<Q> {
        if d == 0 {
            return None;
        }
        let g = gcd(n, d).max(1);
        let (mut n, mut d) = (n / g, d / g);
        if d < 0 {
            n = n.checked_neg()?;
            d = d.checked_neg()?;
        }
        Some(Q { n, d })
    }

    /// The integer `n`.
    pub const fn int(n: i128) -> Q {
        Q { n, d: 1 }
    }

    /// Numerator.
    pub fn numer(self) -> i128 {
        self.n
    }

    /// Denominator (> 0).
    pub fn denom(self) -> i128 {
        self.d
    }

    /// True for an integer.
    pub fn is_int(self) -> bool {
        self.d == 1
    }

    /// True for 0.
    pub fn is_zero(self) -> bool {
        self.n == 0
    }

    /// The value as an integer, if it is one and fits an `i64`.
    pub fn as_int(self) -> Option<i64> {
        if self.d == 1 {
            i64::try_from(self.n).ok()
        } else {
            None
        }
    }

    /// Sign: −1, 0 or 1.
    pub fn signum(self) -> i32 {
        self.n.signum() as i32
    }

    /// a + b.
    pub fn add(self, o: Q) -> Option<Q> {
        let g = gcd(self.d, o.d).max(1);
        let (da, db) = (self.d / g, o.d / g);
        let n = self.n.checked_mul(db)?.checked_add(o.n.checked_mul(da)?)?;
        Q::new(n, self.d.checked_mul(db)?)
    }

    /// a − b.
    pub fn sub(self, o: Q) -> Option<Q> {
        self.add(o.neg()?)
    }

    /// a · b.
    pub fn mul(self, o: Q) -> Option<Q> {
        let g1 = gcd(self.n, o.d).max(1);
        let g2 = gcd(o.n, self.d).max(1);
        Q::new(
            (self.n / g1).checked_mul(o.n / g2)?,
            (self.d / g2).checked_mul(o.d / g1)?,
        )
    }

    /// −a.
    pub fn neg(self) -> Option<Q> {
        Some(Q {
            n: self.n.checked_neg()?,
            d: self.d,
        })
    }

    /// 1/a (`None` for 0).
    pub fn recip(self) -> Option<Q> {
        Q::new(self.d, self.n)
    }

    /// a / b (`None` for b = 0).
    pub fn div(self, o: Q) -> Option<Q> {
        self.mul(o.recip()?)
    }

    /// aᵏ for an integer k (`None` for 0 to a power ≤ 0, as in graphing).
    pub fn powi(self, k: i64) -> Option<Q> {
        if k == 0 {
            return if self.is_zero() { None } else { Some(Q::ONE) };
        }
        let base = if k < 0 { self.recip()? } else { self };
        let mut e = k.unsigned_abs();
        let mut acc = Q::ONE;
        let mut b = base;
        while e > 0 {
            if e & 1 == 1 {
                acc = acc.mul(b)?;
            }
            e >>= 1;
            if e > 0 {
                b = b.mul(b)?;
            }
        }
        Some(acc)
    }

    /// |a|.
    pub fn abs(self) -> Option<Q> {
        if self.n < 0 { self.neg() } else { Some(self) }
    }

    /// The least common multiple of two positive rationals:
    /// lcm(a/b, c/d) = lcm(a, c) / gcd(b, d).
    pub fn lcm(self, o: Q) -> Option<Q> {
        if self.n <= 0 || o.n <= 0 {
            return None;
        }
        let g = gcd(self.n, o.n).max(1);
        let l = (self.n / g).checked_mul(o.n)?;
        Q::new(l, gcd(self.d, o.d).max(1))
    }

    /// The exact decimal `digits` (digits and at most one `.`), if it
    /// fits.
    pub fn from_decimal(digits: &str) -> Option<Q> {
        let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
        if !int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit()) {
            return None;
        }
        let frac = frac.trim_end_matches('0');
        let all = format!("{int}{frac}");
        let all = all.trim_start_matches('0');
        let n: i128 = if all.is_empty() { 0 } else { all.parse().ok()? };
        let d = 10i128.checked_pow(u32::try_from(frac.len()).ok()?)?;
        Q::new(n, d)
    }

    /// The double nearest the value (exact when it is one).
    pub fn to_f64(self) -> f64 {
        // n and d below 2⁵³ are exact, and one division rounds once.
        if self.n.unsigned_abs() < 1 << 53 && self.d < 1 << 53 {
            return self.n as f64 / self.d as f64;
        }
        self.interval().mid()
    }

    /// The exact double the value is, if it is one.
    pub fn exact_f64(self) -> Option<f64> {
        let v = self.to_f64();
        (v.is_finite() && Q::from_f64(v) == Some(self)).then_some(v)
    }

    /// The exact rational a finite double is (`None` if it doesn't fit).
    pub fn from_f64(v: f64) -> Option<Q> {
        if !v.is_finite() {
            return None;
        }
        if v == 0.0 {
            return Some(Q::ZERO);
        }
        let bits = v.to_bits();
        let sign = if bits >> 63 == 1 { -1i128 } else { 1 };
        let exp = ((bits >> 52) & 0x7ff) as i32;
        let frac = (bits & ((1u64 << 52) - 1)) as i128;
        let (m, e) = if exp == 0 {
            (frac, -1074)
        } else {
            (frac | (1i128 << 52), exp - 1075)
        };
        if e >= 0 {
            let m = m.checked_mul(1i128.checked_shl(e as u32)?)?;
            if e > 74 {
                return None;
            }
            Q::new(sign * m, 1)
        } else {
            if -e > 126 {
                return None;
            }
            Q::new(sign * m, 1i128 << (-e) as u32)
        }
    }

    /// An enclosure of the value.
    pub fn interval(self) -> Interval {
        let exact = |v: i128| (v.unsigned_abs() <= 1 << 53).then_some(v as f64);
        if let (Some(n), Some(d)) = (exact(self.n), exact(self.d)) {
            if d == 1.0 {
                return Interval::point(n);
            }
            let q = n / d;
            // The correctly rounded quotient is within half an ulp.
            if Q::from_f64(q) == Some(self) {
                return Interval::point(q);
            }
            return Interval::new(q.next_down(), q.next_up());
        }
        // Huge parts: n and d each rounded once (relative error ≤ 2⁻⁵³),
        // the quotient once more; two ulps either side cover it.
        let (n, d) = (self.n as f64, self.d as f64);
        let q = n / d;
        let w = |v: f64, k: i32| {
            let mut v = v;
            for _ in 0..k {
                v = v.next_down();
            }
            v
        };
        let u = |v: f64, k: i32| {
            let mut v = v;
            for _ in 0..k {
                v = v.next_up();
            }
            v
        };
        Interval::new(w(q, 3), u(q, 3))
    }
}

impl PartialOrd for Q {
    fn partial_cmp(&self, o: &Q) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Q {
    fn cmp(&self, o: &Q) -> Ordering {
        // n1/d1 vs n2/d2 with d > 0: compare n1·d2 and n2·d1, exactly.
        match (self.n.checked_mul(o.d), o.n.checked_mul(self.d)) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => self
                .to_f64()
                .partial_cmp(&o.to_f64())
                .unwrap_or(Ordering::Equal)
                .then(self.n.cmp(&o.n)),
        }
    }
}

impl fmt::Display for Q {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.d == 1 {
            write!(f, "{}", self.n)
        } else {
            write!(f, "{}/{}", self.n, self.d)
        }
    }
}

impl FromStr for Q {
    type Err = ();
    fn from_str(s: &str) -> Result<Q, ()> {
        let int = |t: &str| -> Result<i128, ()> {
            let digits = t.strip_prefix('-').unwrap_or(t);
            if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
                return Err(());
            }
            t.parse().map_err(|_| ())
        };
        match s.split_once('/') {
            Some((a, b)) => Q::new(int(a)?, int(b)?).ok_or(()),
            None => Ok(Q::int(int(s)?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_is_exact_or_refused() {
        let a = Q::new(1, 3).unwrap();
        let b = Q::new(1, 6).unwrap();
        assert_eq!(a.add(b), Q::new(1, 2));
        assert_eq!(a.mul(b), Q::new(1, 18));
        assert_eq!(Q::int(2).powi(-3), Q::new(1, 8));
        assert_eq!(Q::ZERO.powi(0), None);
        assert_eq!(Q::int(i128::MAX).add(Q::ONE), None);
        assert_eq!(
            Q::new(3, 2).unwrap().lcm(Q::new(5, 4).unwrap()),
            Q::new(15, 2)
        );
    }

    #[test]
    fn decimals_and_doubles() {
        assert_eq!(Q::from_decimal("0.000001"), Q::new(1, 1_000_000));
        assert_eq!(
            Q::from_decimal("1000000000000.0625"),
            Q::new(16_000_000_000_001, 16)
        );
        assert_eq!(Q::from_f64(0.5), Q::new(1, 2));
        assert_eq!(Q::from_f64(1e15), Some(Q::int(1_000_000_000_000_000)));
        assert!(Q::from_f64(0.1).unwrap() != Q::new(1, 10).unwrap());
        let i = Q::new(1, 10).unwrap().interval();
        assert!(i.contains(0.1) && i.lo() < 0.1 && i.hi() > 0.1 || i == Interval::point(0.1));
        assert_eq!("-7/3".parse::<Q>(), Ok(Q::new(-7, 3).unwrap()));
    }
}
