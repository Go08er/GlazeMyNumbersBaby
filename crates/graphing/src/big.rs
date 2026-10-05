//! Exact naturals and rationals of any size up to a cap, for values a
//! double can't carry exactly on the way to its result: the counting
//! functions' exact integers (n!!, nCr, nPr), enclosed or rounded once, and
//! the compiler's folding of literal arithmetic (`10^17·(0.1 + 0.2 − 0.3)`
//! is exactly 0, not 10^17 times the doubles' residue).

use std::cmp::Ordering;

use crate::wide::Wide;

/// The most bits a rational's numerator or denominator may have: an
/// operation whose result would be longer is refused (`None`), never
/// truncated. (10^±4900 fit; the doubles end near 2^±1075.)
pub(crate) const MAX_BITS: u64 = 1 << 14;

/// A natural number: little-endian 32-bit limbs, no high zero limb.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Nat(Vec<u32>);

impl Nat {
    pub(crate) fn zero() -> Nat {
        Nat(Vec::new())
    }

    pub(crate) fn from_u64(v: u64) -> Nat {
        let mut n = Nat(vec![v as u32, (v >> 32) as u32]);
        n.trim();
        n
    }

    fn trim(&mut self) {
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    /// The number of bits (0 for 0).
    pub(crate) fn bits(&self) -> u64 {
        match self.0.last() {
            None => 0,
            Some(&top) => (self.0.len() as u64 - 1) * 32 + u64::from(32 - top.leading_zeros()),
        }
    }

    /// A double that is a whole number ≥ 0, exactly.
    pub(crate) fn from_f64(v: f64) -> Option<Nat> {
        if !(v >= 0.0 && v.is_finite() && v == v.trunc()) {
            return None;
        }
        if v < 18_446_744_073_709_551_616.0 {
            return Some(Nat::from_u64(v as u64));
        }
        // v ≥ 2⁶⁴: its 53-bit mantissa shifted up.
        let bits = v.to_bits();
        let e = ((bits >> 52) & 0x7ff) - 1075;
        let m = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
        Some(Nat::from_u64(m).shl(e))
    }

    /// The decimal digits `s` (ASCII digits only; `None` beyond the cap).
    fn from_digits(s: &str) -> Option<Nat> {
        let mut n = Nat::zero();
        for chunk in s.as_bytes().chunks(9) {
            let mut v = 0u32;
            for &c in chunk {
                if !c.is_ascii_digit() {
                    return None;
                }
                v = v * 10 + u32::from(c - b'0');
            }
            n = n
                .mul_small(10u32.pow(chunk.len() as u32))
                .add(&Nat::from_u64(u64::from(v)));
            if n.bits() > MAX_BITS {
                return None;
            }
        }
        Some(n)
    }

    pub(crate) fn shl(&self, k: u64) -> Nat {
        if self.is_zero() {
            return Nat::zero();
        }
        let (limbs, bits) = ((k / 32) as usize, (k % 32) as u32);
        let mut out = vec![0u32; limbs];
        if bits == 0 {
            out.extend_from_slice(&self.0);
        } else {
            let mut carry = 0u32;
            for &l in &self.0 {
                out.push((l << bits) | carry);
                carry = l >> (32 - bits);
            }
            if carry != 0 {
                out.push(carry);
            }
        }
        Nat(out)
    }

    pub(crate) fn add(&self, o: &Nat) -> Nat {
        let (a, b) = if self.0.len() >= o.0.len() {
            (self, o)
        } else {
            (o, self)
        };
        let mut out = Vec::with_capacity(a.0.len() + 1);
        let mut carry = 0u64;
        for (i, &l) in a.0.iter().enumerate() {
            let s = u64::from(l) + u64::from(b.0.get(i).copied().unwrap_or(0)) + carry;
            out.push(s as u32);
            carry = s >> 32;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        Nat(out)
    }

    /// self − o, for self ≥ o.
    pub(crate) fn sub(&self, o: &Nat) -> Nat {
        debug_assert!(*self >= *o);
        let mut out = Vec::with_capacity(self.0.len());
        let mut borrow = 0i64;
        for (i, &l) in self.0.iter().enumerate() {
            let mut d = i64::from(l) - i64::from(o.0.get(i).copied().unwrap_or(0)) - borrow;
            borrow = 0;
            if d < 0 {
                d += 1 << 32;
                borrow = 1;
            }
            out.push(d as u32);
        }
        let mut n = Nat(out);
        n.trim();
        n
    }

    pub(crate) fn mul(&self, o: &Nat) -> Nat {
        if self.is_zero() || o.is_zero() {
            return Nat::zero();
        }
        let mut out = vec![0u32; self.0.len() + o.0.len()];
        for (i, &a) in self.0.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in o.0.iter().enumerate() {
                let t = u64::from(a) * u64::from(b) + u64::from(out[i + j]) + carry;
                out[i + j] = t as u32;
                carry = t >> 32;
            }
            let mut k = i + o.0.len();
            while carry != 0 {
                let t = u64::from(out[k]) + carry;
                out[k] = t as u32;
                carry = t >> 32;
                k += 1;
            }
        }
        let mut n = Nat(out);
        n.trim();
        n
    }

    pub(crate) fn mul_small(&self, m: u32) -> Nat {
        self.mul(&Nat::from_u64(u64::from(m)))
    }

    /// (self div d, self mod d), d > 0.
    pub(crate) fn div_rem_small(&self, d: u32) -> (Nat, u32) {
        let d = u64::from(d);
        let mut out = vec![0u32; self.0.len()];
        let mut rem = 0u64;
        for i in (0..self.0.len()).rev() {
            let cur = (rem << 32) | u64::from(self.0[i]);
            out[i] = (cur / d) as u32;
            rem = cur % d;
        }
        let mut n = Nat(out);
        n.trim();
        (n, rem as u32)
    }

    /// The leading bits of self/d (both > 0): `(q, e, inexact)` with
    /// self/d = (q + δ)·2^e, 2⁶³ ≤ q < 2⁶⁴, 0 ≤ δ < 1 and `inexact` iff
    /// δ > 0.
    fn leading(&self, d: &Nat) -> (u64, i64, bool) {
        debug_assert!(!self.is_zero() && !d.is_zero());
        // (n·2^s)/d lies in [2⁶³, 2⁶⁵).
        let s = 64 + d.bits() as i64 - self.bits() as i64;
        let (mut rem, den) = if s >= 0 {
            (self.shl(s as u64), d.clone())
        } else {
            (self.clone(), d.shl((-s) as u64))
        };
        let mut q: u128 = 0;
        for b in (0..=64u64).rev() {
            let t = den.shl(b);
            if rem >= t {
                rem = rem.sub(&t);
                q |= 1 << b;
            }
        }
        let mut inexact = !rem.is_zero();
        let mut e = -s;
        if q >= 1 << 64 {
            inexact |= q & 1 == 1;
            q >>= 1;
            e += 1;
        }
        (q as u64, e, inexact)
    }

    /// The double nearest (or below, or above) the number.
    pub(crate) fn to_f64(&self, mode: Round) -> f64 {
        if self.is_zero() {
            return 0.0;
        }
        let (q, e, inexact) = self.leading(&Nat::from_u64(1));
        round_f64(false, q, e, inexact, mode)
    }

    /// The tightest enclosure by doubles: [below, above] (a point when the
    /// number is a double).
    pub(crate) fn enclose(&self) -> (f64, f64) {
        (self.to_f64(Round::Down), self.to_f64(Round::Up))
    }
}

impl PartialOrd for Nat {
    fn partial_cmp(&self, o: &Nat) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Nat {
    fn cmp(&self, o: &Nat) -> Ordering {
        self.0
            .len()
            .cmp(&o.0.len())
            .then_with(|| self.0.iter().rev().cmp(o.0.iter().rev()))
    }
}

/// How to round to a double.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Round {
    /// To nearest, ties to even.
    Nearest,
    /// Toward −∞.
    Down,
    /// Toward +∞.
    Up,
}

/// ±(q + δ)·2^e (2⁶³ ≤ q < 2⁶⁴, `inexact` iff δ > 0) rounded to a double.
fn round_f64(neg: bool, q: u64, e: i64, inexact: bool, mode: Round) -> f64 {
    // Away from 0 or toward it, for the magnitude.
    let away = match mode {
        Round::Nearest => None,
        Round::Down => Some(neg),
        Round::Up => Some(!neg),
    };
    let mut top = e + 63;
    // Bits below a double's last place: 11 for a normal result, more in
    // the subnormal range.
    let drop = if top >= -1022 { 11 } else { 11 + (-1022 - top) };
    let (mut m, up) = if drop >= 65 {
        // Below half the least subnormal.
        (0u64, away == Some(true))
    } else {
        let drop = drop as u32;
        let m = if drop == 64 { 0 } else { q >> drop };
        let rem = if drop == 64 {
            q
        } else {
            q & ((1u64 << drop) - 1)
        };
        let half = 1u64 << (drop - 1);
        let up = match away {
            None => rem > half || (rem == half && (inexact || m & 1 == 1)),
            Some(a) => a && (rem != 0 || inexact),
        };
        (m, up)
    };
    if up {
        m += 1;
    }
    let bits = if top >= -1022 {
        if m == 1 << 53 {
            m >>= 1;
            top += 1;
        }
        if top > 1023 {
            let v = if away == Some(false) {
                f64::MAX
            } else {
                f64::INFINITY
            };
            return if neg { -v } else { v };
        }
        (((top + 1023) as u64) << 52) | (m & ((1u64 << 52) - 1))
    } else {
        // Subnormal (a carry into bit 52 is the least normal, as it should).
        m
    };
    let v = f64::from_bits(bits);
    if neg { -v } else { v }
}

/// An exact rational ±n/d (d > 0; not necessarily in lowest terms).
#[derive(Clone, Debug)]
pub(crate) struct Rat {
    neg: bool,
    n: Nat,
    d: Nat,
}

fn capped(r: Rat) -> Option<Rat> {
    (r.n.bits() <= MAX_BITS && r.d.bits() <= MAX_BITS).then_some(r)
}

impl Rat {
    pub(crate) fn int(v: u64) -> Rat {
        Rat::nat(Nat::from_u64(v))
    }

    /// A natural number.
    pub(crate) fn nat(n: Nat) -> Rat {
        Rat {
            neg: false,
            n,
            d: Nat::from_u64(1),
        }
    }

    /// A finite double, exactly.
    pub(crate) fn from_f64(v: f64) -> Option<Rat> {
        if !v.is_finite() {
            return None;
        }
        if v == 0.0 {
            return Some(Rat::int(0));
        }
        let bits = v.to_bits();
        let exp = ((bits >> 52) & 0x7ff) as i64;
        let frac = bits & ((1u64 << 52) - 1);
        let (m, e) = if exp == 0 {
            (frac, -1074)
        } else {
            (frac | (1u64 << 52), exp - 1075)
        };
        let (n, d) = if e >= 0 {
            (Nat::from_u64(m).shl(e as u64), Nat::from_u64(1))
        } else {
            (Nat::from_u64(m), Nat::from_u64(1).shl((-e) as u64))
        };
        Some(Rat { neg: v < 0.0, n, d })
    }

    /// The decimal `digits` (digits and at most one `.`), exactly.
    pub(crate) fn from_decimal(digits: &str) -> Option<Rat> {
        let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
        let frac = frac.trim_end_matches('0');
        let all = format!("{int}{frac}");
        let n = Nat::from_digits(all.trim_start_matches('0'))?;
        let d = Nat::from_digits(&format!("1{}", "0".repeat(frac.len())))?;
        capped(Rat { neg: false, n, d })
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.n.is_zero()
    }

    /// The value as an integer, if it is one that fits an `i64`.
    pub(crate) fn as_int(&self) -> Option<i64> {
        let (q, whole) = self.n_div_d()?;
        if !whole {
            return None;
        }
        let v = i64::try_from(q).ok()?;
        Some(if self.neg { -v } else { v })
    }

    /// Whether the value is an integer.
    pub(crate) fn is_integer(&self) -> bool {
        if self.n.is_zero() || self.d == Nat::from_u64(1) {
            return true;
        }
        self.d <= self.n && self.n_mod_d_is_zero()
    }

    /// d | n, by long division (n and d are capped).
    fn n_mod_d_is_zero(&self) -> bool {
        let mut rem = self.n.clone();
        let db = self.d.bits();
        while rem >= self.d {
            let shift = rem.bits() - db;
            let mut t = self.d.shl(shift);
            if t > rem {
                t = self.d.shl(shift - 1);
            }
            rem = rem.sub(&t);
        }
        rem.is_zero()
    }

    /// (n div d, d | n) when the quotient fits a u64.
    fn n_div_d(&self) -> Option<(u64, bool)> {
        if self.n.is_zero() {
            return Some((0, true));
        }
        if self.n.bits() > self.d.bits() + 64 {
            return None;
        }
        let mut rem = self.n.clone();
        let mut q: u64 = 0;
        let db = self.d.bits();
        while rem >= self.d {
            let shift = rem.bits() - db;
            let (t, s) = {
                let t = self.d.shl(shift);
                if t > rem {
                    (self.d.shl(shift - 1), shift - 1)
                } else {
                    (t, shift)
                }
            };
            rem = rem.sub(&t);
            q = q.checked_add(1u64.checked_shl(s as u32)?)?;
        }
        Some((q, rem.is_zero()))
    }

    /// Whether the value is below 0.
    pub(crate) fn is_negative(&self) -> bool {
        self.neg && !self.is_zero()
    }

    /// Whether the value is 1 or −1.
    pub(crate) fn abs_is_one(&self) -> bool {
        self.n == self.d
    }

    /// Whether the value is an even integer.
    pub(crate) fn is_even(&self) -> bool {
        Rat {
            neg: false,
            n: self.n.clone(),
            d: self.d.shl(1),
        }
        .is_integer()
    }

    pub(crate) fn neg(mut self) -> Rat {
        self.neg = !self.neg;
        self
    }

    pub(crate) fn abs(mut self) -> Rat {
        self.neg = false;
        self
    }

    pub(crate) fn add(&self, o: &Rat) -> Option<Rat> {
        if self.n.bits() + o.d.bits() > MAX_BITS + 64 || o.n.bits() + self.d.bits() > MAX_BITS + 64
        {
            return None;
        }
        let a = self.n.mul(&o.d);
        let b = o.n.mul(&self.d);
        let d = self.d.mul(&o.d);
        let (neg, n) = if self.neg == o.neg {
            (self.neg, a.add(&b))
        } else if a >= b {
            (self.neg, a.sub(&b))
        } else {
            (o.neg, b.sub(&a))
        };
        capped(Rat { neg, n, d })
    }

    pub(crate) fn sub(&self, o: &Rat) -> Option<Rat> {
        self.add(&o.clone().neg())
    }

    pub(crate) fn mul(&self, o: &Rat) -> Option<Rat> {
        if self.n.bits() + o.n.bits() > MAX_BITS + 64 || self.d.bits() + o.d.bits() > MAX_BITS + 64
        {
            return None;
        }
        capped(Rat {
            neg: self.neg != o.neg,
            n: self.n.mul(&o.n),
            d: self.d.mul(&o.d),
        })
    }

    /// self/o (`None` for o = 0).
    pub(crate) fn div(&self, o: &Rat) -> Option<Rat> {
        if o.is_zero() {
            return None;
        }
        self.mul(&Rat {
            neg: o.neg,
            n: o.d.clone(),
            d: o.n.clone(),
        })
    }

    /// selfᵏ for an integer k (`None` for 0 to a power ≤ 0, or too long).
    pub(crate) fn powi(&self, k: i64) -> Option<Rat> {
        if k == 0 {
            return (!self.is_zero()).then(|| Rat::int(1));
        }
        let longest = self.n.bits().max(self.d.bits());
        if longest.saturating_mul(k.unsigned_abs()) > MAX_BITS + 64 {
            return None;
        }
        let base = if k < 0 {
            Rat::int(1).div(self)?
        } else {
            self.clone()
        };
        let mut e = k.unsigned_abs();
        let mut acc = Rat::int(1);
        let mut b = base;
        while e > 0 {
            if e & 1 == 1 {
                acc = acc.mul(&b)?;
            }
            e >>= 1;
            if e > 0 {
                b = b.mul(&b)?;
            }
        }
        Some(acc)
    }

    /// The double nearest (or below, or above) the value.
    pub(crate) fn to_f64(&self, mode: Round) -> f64 {
        if self.is_zero() {
            return 0.0;
        }
        let (q, e, inexact) = self.n.leading(&self.d);
        round_f64(self.neg, q, e, inexact, mode)
    }

    /// The value as an extended-range [`Wide`]: the nearest double while it
    /// is one (rounded once), else its 53 leading bits and its binary
    /// exponent, however far beyond the doubles.
    pub(crate) fn to_wide(&self) -> Wide {
        if self.is_zero() {
            return Wide::new(0.0);
        }
        let v = self.to_f64(Round::Nearest);
        if v != 0.0 && v.is_finite() {
            return Wide::new(v);
        }
        // Beyond the doubles: m·2^top with m rounded to 53 bits.
        let (q, e, inexact) = self.n.leading(&self.d);
        let mut top = e + 63;
        let mut m = q >> 11;
        let rem = q & ((1 << 11) - 1);
        if rem > 1 << 10 || (rem == 1 << 10 && (inexact || m & 1 == 1)) {
            m += 1;
        }
        if m == 1 << 53 {
            m >>= 1;
            top += 1;
        }
        let mant = m as f64 / (1u64 << 52) as f64;
        Wide::Val(if self.neg { -mant } else { mant }, top as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Rat {
        Rat::from_decimal(s).unwrap()
    }

    #[test]
    fn literal_arithmetic_is_exact() {
        let s = dec("0.1")
            .add(&dec("0.2"))
            .unwrap()
            .sub(&dec("0.3"))
            .unwrap();
        assert!(s.is_zero());
        let big = Rat::int(10).powi(17).unwrap().mul(&s).unwrap();
        assert_eq!(big.to_f64(Round::Nearest), 0.0);
        // 0.1 is below its double.
        assert_eq!(dec("0.1").to_f64(Round::Nearest), 0.1);
        assert_eq!(dec("0.1").to_f64(Round::Up), 0.1);
        assert_eq!(dec("0.1").to_f64(Round::Down), 0.1f64.next_down());
        assert_eq!(dec("1.0000000000000001").to_f64(Round::Nearest), 1.0);
        assert_eq!(
            dec("1.0000000000000001").to_f64(Round::Up),
            1.0f64.next_up()
        );
        assert!(!dec("1.0000000000000001").is_integer());
        assert!(dec("2.000").is_integer() && dec("0.1").add(&dec("0.9")).unwrap().is_integer());
        assert_eq!(dec("2.000").as_int(), Some(2));
        assert_eq!(dec("0.5").clone().neg().as_int(), None);
        assert_eq!(dec("7").neg().as_int(), Some(-7));
        // 1/3 rounds once.
        let third = Rat::int(1).div(&Rat::int(3)).unwrap();
        assert_eq!(third.to_f64(Round::Nearest), 1.0 / 3.0);
        assert_eq!(third.to_f64(Round::Up), third.to_f64(Round::Down).next_up());
        assert!(Rat::int(1).div(&Rat::int(0)).is_none());
    }

    #[test]
    fn rounding_at_the_ends_of_the_doubles() {
        let two = Rat::int(2);
        assert_eq!(two.powi(-1074).unwrap().to_f64(Round::Nearest), 5e-324);
        assert_eq!(two.powi(-1075).unwrap().to_f64(Round::Nearest), 0.0);
        assert_eq!(two.powi(-1075).unwrap().to_f64(Round::Up), 5e-324);
        assert_eq!(
            two.powi(-1022).unwrap().to_f64(Round::Nearest),
            f64::MIN_POSITIVE
        );
        assert_eq!(
            two.powi(1024).unwrap().to_f64(Round::Nearest),
            f64::INFINITY
        );
        assert_eq!(two.powi(1024).unwrap().to_f64(Round::Down), f64::MAX);
        let max = Rat::from_f64(f64::MAX).unwrap();
        assert_eq!(max.to_f64(Round::Nearest), f64::MAX);
        assert_eq!(max.neg().to_f64(Round::Up), -f64::MAX);
        // 10⁴⁰⁰·10⁻³⁹⁹ = 10, and 10⁴⁰⁰ stays a value beyond the doubles.
        let ten = Rat::int(10);
        let p = ten
            .powi(400)
            .unwrap()
            .mul(&ten.powi(-399).unwrap())
            .unwrap();
        assert_eq!(p.to_f64(Round::Nearest), 10.0);
        let w = ten.powi(400).unwrap().to_wide();
        assert!(w.to_f64().is_infinite() && !w.lost());
        assert_eq!(ten.powi(-400).unwrap().to_wide().to_f64(), 0.0);
        assert!(!ten.powi(-400).unwrap().to_wide().is_zero());
        // Beyond the cap: refused.
        assert!(ten.powi(100_000).is_none());
    }

    #[test]
    fn naturals() {
        let mut f = Nat::from_u64(1);
        for k in 1..=25u32 {
            f = f.mul_small(k);
        }
        // 25! = 15511210043330985984000000, between two doubles.
        let (lo, hi) = f.enclose();
        assert_eq!(hi, lo.next_up());
        assert!(lo <= 1.5511210043330986e25 && 1.5511210043330986e25 <= hi);
        assert_eq!(f.to_f64(Round::Nearest), 1.5511210043330986e25);
        assert_eq!(Nat::from_f64(1e300).unwrap().to_f64(Round::Nearest), 1e300);
        assert_eq!(Nat::from_f64(1e300).unwrap().enclose(), (1e300, 1e300));
        let (q, r) = Nat::from_u64(1000).div_rem_small(7);
        assert_eq!((q, r), (Nat::from_u64(142), 6));
        // 2¹⁰²⁴ is beyond the doubles.
        let big = Nat::from_u64(1).shl(1024);
        assert_eq!(big.to_f64(Round::Nearest), f64::INFINITY);
        assert_eq!(big.to_f64(Round::Down), f64::MAX);
        // 2⁵³ + 1 is a tie: to even.
        let tie = Nat::from_u64((1 << 53) + 1);
        assert_eq!(tie.to_f64(Round::Nearest), 9007199254740992.0);
        assert_eq!(tie.enclose(), (9007199254740992.0, 9007199254740994.0));
    }
}
