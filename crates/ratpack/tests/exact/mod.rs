// Rust port: GMNB contributors.

//! Exact arithmetic for the tests of how a number is written out: whole
//! numbers of any length, with only what checking a rounding takes
//! (products, sums and comparisons; no division, and no floats).
//!
//! [`check_rounded`] says whether a text in scientific notation is a
//! rational rounded to a given number of significant digits, half away
//! from zero, which is how ratpack rounds what it writes.

#![allow(dead_code)]

use std::cmp::Ordering;

use ratpack::{Number, Rational};

/// A whole number, zero or more: limbs of base 10⁹, least significant
/// first, no zero limbs at the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Big(Vec<u32>);

const LIMB: u64 = 1_000_000_000;

impl Big {
    pub fn from(value: u64) -> Big {
        let mut limbs = Vec::new();
        let mut rest = value;
        while rest > 0 {
            limbs.push((rest % LIMB) as u32);
            rest /= LIMB;
        }
        Big(limbs)
    }

    pub fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    /// `self × factor + addend`.
    pub fn mul_add(&self, factor: u64, addend: u64) -> Big {
        assert!(factor < 1 << 32 && addend < 1 << 32);
        let mut limbs = Vec::with_capacity(self.0.len() + 2);
        let mut carry = addend;
        for &limb in &self.0 {
            let v = u64::from(limb) * factor + carry;
            limbs.push((v % LIMB) as u32);
            carry = v / LIMB;
        }
        while carry > 0 {
            limbs.push((carry % LIMB) as u32);
            carry /= LIMB;
        }
        Big(limbs)
    }

    pub fn add(&self, other: &Big) -> Big {
        let mut limbs = Vec::with_capacity(self.0.len().max(other.0.len()) + 1);
        let mut carry = 0u64;
        for i in 0..self.0.len().max(other.0.len()) {
            let v = u64::from(self.0.get(i).copied().unwrap_or(0))
                + u64::from(other.0.get(i).copied().unwrap_or(0))
                + carry;
            limbs.push((v % LIMB) as u32);
            carry = v / LIMB;
        }
        if carry > 0 {
            limbs.push(carry as u32);
        }
        Big(limbs)
    }

    /// `self − other`, which must not be negative.
    pub fn sub(&self, other: &Big) -> Big {
        assert!(*self >= *other);
        let mut limbs = Vec::with_capacity(self.0.len());
        let mut borrow = 0i64;
        for i in 0..self.0.len() {
            let mut v =
                i64::from(self.0[i]) - i64::from(other.0.get(i).copied().unwrap_or(0)) - borrow;
            borrow = i64::from(v < 0);
            if v < 0 {
                v += LIMB as i64;
            }
            limbs.push(v as u32);
        }
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
        Big(limbs)
    }

    pub fn mul(&self, other: &Big) -> Big {
        if self.is_zero() || other.is_zero() {
            return Big(Vec::new());
        }
        let mut limbs = vec![0u64; self.0.len() + other.0.len() + 1];
        for (i, &a) in self.0.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in other.0.iter().enumerate() {
                let v = limbs[i + j] + u64::from(a) * u64::from(b) + carry;
                limbs[i + j] = v % LIMB;
                carry = v / LIMB;
            }
            let mut k = i + other.0.len();
            while carry > 0 {
                let v = limbs[k] + carry;
                limbs[k] = v % LIMB;
                carry = v / LIMB;
                k += 1;
            }
        }
        let mut limbs: Vec<u32> = limbs.into_iter().map(|l| l as u32).collect();
        while limbs.last() == Some(&0) {
            limbs.pop();
        }
        Big(limbs)
    }

    /// `base` to the power `exponent`.
    pub fn pow(base: u32, exponent: u32) -> Big {
        let mut result = Big::from(1);
        let mut square = Big::from(u64::from(base));
        let mut rest = exponent;
        while rest > 0 {
            if rest & 1 == 1 {
                result = result.mul(&square);
            }
            rest >>= 1;
            if rest > 0 {
                square = square.mul(&square);
            }
        }
        result
    }

    /// The magnitude of a ratpack number that is whole (as the two parts of
    /// a rational are): digits of base 2³¹, least significant first, above
    /// `exp` zero ones.
    pub fn from_number(number: &Number) -> Big {
        assert!(number.exp() >= 0, "{number:?}");
        let mut value = Big(Vec::new());
        for &digit in number.mantissa().iter().rev() {
            value = value.mul_add(1 << 31, u64::from(digit));
        }
        for _ in 0..number.exp() {
            value = value.mul_add(1 << 31, 0);
        }
        value
    }

    /// The digits of `text` in `radix` (0–9, then A–Z) as a number.
    pub fn from_digits(text: &str, radix: u32) -> Big {
        let mut value = Big(Vec::new());
        for c in text.chars() {
            let digit = c
                .to_digit(radix)
                .unwrap_or_else(|| panic!("digit {c:?} in {text:?}"));
            value = value.mul_add(u64::from(radix), u64::from(digit));
        }
        value
    }
}

impl PartialOrd for Big {
    fn partial_cmp(&self, other: &Big) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Big {
    fn cmp(&self, other: &Big) -> Ordering {
        self.0
            .len()
            .cmp(&other.0.len())
            .then_with(|| self.0.iter().rev().cmp(other.0.iter().rev()))
    }
}

/// A number as ratpack writes it in scientific notation: `-d.ddde±x` in
/// radix 10, `-d.ddd^±x` in another (the exponent in the radix too).
#[derive(Debug)]
pub struct Scientific {
    pub negative: bool,
    /// The mantissa's digits, the point left out.
    pub digits: String,
    /// The power of the radix the first digit stands for.
    pub exponent: i64,
}

impl Scientific {
    pub fn parse(text: &str, radix: u32) -> Option<Scientific> {
        let (negative, rest) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (mantissa, exponent) = rest.split_once(if radix == 10 { 'e' } else { '^' })?;
        let (int, frac) = mantissa.split_once('.')?;
        if int.chars().count() != 1 {
            return None;
        }
        let (exponent_negative, exponent) = match exponent.split_at(1) {
            ("-", digits) => (true, digits),
            ("+", digits) => (false, digits),
            _ => return None,
        };
        let exponent = i64::from_str_radix(exponent, radix).ok()?;
        let digits = format!("{int}{frac}");
        digits
            .chars()
            .all(|c| c.is_digit(radix))
            .then_some(Scientific {
                negative,
                digits,
                exponent: if exponent_negative {
                    -exponent
                } else {
                    exponent
                },
            })
    }
}

/// Checks that `text` is `value` written in scientific notation in `radix`
/// with `digits` significant digits: rounded half away from zero at the
/// digit after them, the zeros at the end of the mantissa dropped. Exactly:
/// `value` is the quotient of two whole numbers, and nothing here rounds.
pub fn check_rounded(value: &Rational, text: &str, radix: u32, digits: u32) -> Result<(), String> {
    let written = Scientific::parse(text, radix)
        .ok_or_else(|| format!("{text:?} isn't scientific notation"))?;
    let p = Big::from_number(value.p());
    let q = Big::from_number(value.q());
    if p.is_zero() {
        return Err("the value is zero".into());
    }
    let negative = (value.p().sign() < 0) != (value.q().sign() < 0);
    if negative != written.negative {
        return Err(format!("{text:?} has the wrong sign"));
    }
    let n = written.digits.chars().count() as u32;
    if n > digits {
        return Err(format!("{text:?} has {n} digits, more than {digits}"));
    }
    if written.digits.starts_with('0') || (n > 1 && written.digits.ends_with('0')) {
        return Err(format!("{text:?} isn't normalized"));
    }
    // The mantissa as a whole number of `digits` digits, and the power of
    // the radix its last digit stands for.
    let mantissa = Big::from_digits(&written.digits, radix).mul(&Big::pow(radix, digits - n));
    let last = written.exponent - i64::from(digits - 1);
    // In units of a tenth (a `radix`th) of that last digit, twice the
    // value is `a / b`, and it must lie in
    // [2·radix·mantissa − radix, 2·radix·mantissa + radix): within half a
    // unit of the last digit, a tie going away from zero. Just below a
    // power of the radix the digits are a `radix`th as far apart, so there
    // the lower bound is 2·radix·mantissa − 1.
    let scale = last - 1;
    let power = Big::pow(radix, scale.unsigned_abs() as u32);
    let (a, b) = if scale >= 0 {
        (p.mul_add(2, 0), q.mul(&power))
    } else {
        (p.mul_add(2, 0).mul(&power), q)
    };
    let centre = mantissa.mul_add(2 * u64::from(radix), 0);
    let is_power = mantissa == Big::pow(radix, digits - 1);
    let below = Big::from(if is_power { 1 } else { u64::from(radix) });
    let lower = centre.sub(&below).mul(&b);
    let upper = centre.add(&Big::from(u64::from(radix))).mul(&b);
    if a < lower {
        return Err(format!(
            "{text:?} is above the value rounded to {digits} digits"
        ));
    }
    if a >= upper {
        return Err(format!(
            "{text:?} is below the value rounded to {digits} digits"
        ));
    }
    Ok(())
}

/// The power of `radix` the first significant digit of `value` (which isn't
/// zero) stands for: the largest `e` with `radix^e <= |value|`, found from
/// `near`, which must be within one of it.
pub fn leading_exponent(value: &Rational, radix: u32, near: i64) -> i64 {
    let p = Big::from_number(value.p());
    let q = Big::from_number(value.q());
    // Whether radix^e <= p / q.
    let at_least = |e: i64| {
        let power = Big::pow(radix, e.unsigned_abs() as u32);
        if e >= 0 {
            q.mul(&power) <= p
        } else {
            q <= p.mul(&power)
        }
    };
    (near - 1..=near + 1)
        .rev()
        .find(|&e| at_least(e))
        .filter(|&e| !at_least(e + 1))
        .unwrap_or_else(|| panic!("the exponent isn't within one of {near}"))
}
