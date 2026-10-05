//! Exact naturals of any size, for values a double can't carry exactly on
//! the way to its result: the counting functions' exact integers (n!!,
//! nCr, nPr), enclosed or rounded once.

use std::cmp::Ordering;

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

#[cfg(test)]
mod tests {
    use super::*;

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
