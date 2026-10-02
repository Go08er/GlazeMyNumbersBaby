// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CEngine/Rational.cpp`.

use std::cmp::Ordering;

use crate::ratpak::conv::{i32torat, rat_to_string, rattoui64, ui32torat};
use crate::ratpak::logic::{andrat, lshrat, orrat, remrat, rshrat, xorrat};
use crate::ratpak::rat::{addrat, divrat, mulrat, subrat};
use crate::ratpak::support::{rat_equ, rat_lt};
use crate::ratpak::{Ctx, Rat, with_ctx};
use crate::{CalcResult, Number, NumberFormat, RATIONAL_BASE, RATIONAL_PRECISION};

/// Mirrors `CalcEngine::Rational`. Value semantics; cheap enough to clone.
///
/// Comparisons (`PartialEq`/`PartialOrd`) follow `rat_equ`/`rat_lt` at
/// `RATIONAL_PRECISION`, NOT structural equality. `<=` is `!(a > b)` and
/// `>=` is `!(a < b)`, exactly as in the C++. The fallible `try_*` forms
/// surface the (allocation-only) errors those comparisons can raise; the
/// operator forms treat such an error as "false".
#[derive(Clone, Debug)]
pub struct Rational {
    p: Number,
    q: Number,
}

impl Default for Rational {
    /// `Rational()` — 0/1.
    fn default() -> Self {
        Rational {
            p: Number::default(),
            q: Number::new(1, 0, vec![1]),
        }
    }
}

fn binop(
    lhs: &Rational,
    rhs: &Rational,
    f: impl FnOnce(&Ctx, &mut Rat, &Rat) -> CalcResult<()>,
) -> CalcResult<Rational> {
    let mut lhs_rat = lhs.to_rat();
    let rhs_rat = rhs.to_rat();
    with_ctx(|ctx| f(ctx, &mut lhs_rat, &rhs_rat))?;
    Ok(Rational::from_rat(lhs_rat))
}

impl Rational {
    /// `Rational(Number const& n)`. Note that, like the C++, a positive
    /// exponent of `n` is dropped (only a negative one is moved into `q`).
    pub fn from_number(n: Number) -> Self {
        let mut q_exp: i32 = 0;
        if n.exp < 0 {
            q_exp = q_exp.wrapping_sub(n.exp);
        }

        Rational {
            p: Number::new(n.sign, 0, n.mant),
            q: Number::new(1, q_exp, vec![1]),
        }
    }

    /// `Rational(Number const& p, Number const& q)`
    pub fn from_pq(p: Number, q: Number) -> Self {
        Rational { p, q }
    }

    /// `Rational(PRAT)`
    pub(crate) fn from_rat(r: Rat) -> Self {
        Rational { p: r.pp, q: r.pq }
    }

    /// `ToPRAT()`
    pub(crate) fn to_rat(&self) -> Rat {
        Rat {
            pp: self.p.clone(),
            pq: self.q.clone(),
        }
    }

    pub fn p(&self) -> &Number {
        &self.p
    }
    pub fn q(&self) -> &Number {
        &self.q
    }

    /// `operator+` (`addrat`, which snaps tiny residuals to zero).
    pub fn add(&self, rhs: &Rational) -> CalcResult<Rational> {
        binop(self, rhs, |ctx, a, b| addrat(ctx, a, b, RATIONAL_PRECISION))
    }
    /// `operator-` (`subrat`, which snaps tiny residuals to zero).
    pub fn sub(&self, rhs: &Rational) -> CalcResult<Rational> {
        binop(self, rhs, |ctx, a, b| subrat(ctx, a, b, RATIONAL_PRECISION))
    }
    /// `operator*`
    pub fn mul(&self, rhs: &Rational) -> CalcResult<Rational> {
        binop(self, rhs, |ctx, a, b| mulrat(ctx, a, b, RATIONAL_PRECISION))
    }
    /// `operator/`
    pub fn div(&self, rhs: &Rational) -> CalcResult<Rational> {
        binop(self, rhs, |ctx, a, b| divrat(ctx, a, b, RATIONAL_PRECISION))
    }
    /// C++ `operator%` (remrat semantics — NOT `RationalMath::Mod`).
    /// The sign of a result will match the sign of `self`.
    pub fn rem(&self, rhs: &Rational) -> CalcResult<Rational> {
        binop(self, rhs, |_, a, b| remrat(a, b))
    }
    /// `operator<<`
    pub fn shl(&self, rhs: &Rational) -> CalcResult<Rational> {
        binop(self, rhs, |ctx, a, b| {
            lshrat(ctx, a, b, RATIONAL_BASE, RATIONAL_PRECISION)
        })
    }
    /// `operator>>`
    pub fn shr(&self, rhs: &Rational) -> CalcResult<Rational> {
        binop(self, rhs, |ctx, a, b| {
            rshrat(ctx, a, b, RATIONAL_BASE, RATIONAL_PRECISION)
        })
    }
    /// `operator&`
    pub fn bitand(&self, rhs: &Rational) -> CalcResult<Rational> {
        binop(self, rhs, |ctx, a, b| {
            andrat(ctx, a, b, RATIONAL_BASE, RATIONAL_PRECISION)
        })
    }
    /// `operator|`
    pub fn bitor(&self, rhs: &Rational) -> CalcResult<Rational> {
        binop(self, rhs, |ctx, a, b| {
            orrat(ctx, a, b, RATIONAL_BASE, RATIONAL_PRECISION)
        })
    }
    /// `operator^`
    pub fn bitxor(&self, rhs: &Rational) -> CalcResult<Rational> {
        binop(self, rhs, |ctx, a, b| {
            xorrat(ctx, a, b, RATIONAL_BASE, RATIONAL_PRECISION)
        })
    }

    /// `operator==` (`rat_equ` at `RATIONAL_PRECISION`).
    pub fn try_eq(&self, rhs: &Rational) -> CalcResult<bool> {
        let (l, r) = (self.to_rat(), rhs.to_rat());
        with_ctx(|ctx| rat_equ(ctx, &l, &r, RATIONAL_PRECISION))
    }
    /// `operator!=`
    pub fn try_ne(&self, rhs: &Rational) -> CalcResult<bool> {
        Ok(!self.try_eq(rhs)?)
    }
    /// `operator<` (`rat_lt` at `RATIONAL_PRECISION`).
    pub fn try_lt(&self, rhs: &Rational) -> CalcResult<bool> {
        let (l, r) = (self.to_rat(), rhs.to_rat());
        with_ctx(|ctx| rat_lt(ctx, &l, &r, RATIONAL_PRECISION))
    }
    /// `operator>`: `rhs < lhs`
    pub fn try_gt(&self, rhs: &Rational) -> CalcResult<bool> {
        rhs.try_lt(self)
    }
    /// `operator<=`: `!(lhs > rhs)`
    pub fn try_le(&self, rhs: &Rational) -> CalcResult<bool> {
        Ok(!self.try_gt(rhs)?)
    }
    /// `operator>=`: `!(lhs < rhs)`
    pub fn try_ge(&self, rhs: &Rational) -> CalcResult<bool> {
        Ok(!self.try_lt(rhs)?)
    }

    /// `Rational::ToString(radix, format, precision)`
    pub fn to_string_radix(
        &self,
        radix: u32,
        format: NumberFormat,
        precision: i32,
    ) -> CalcResult<String> {
        let rat = self.to_rat();
        with_ctx(|ctx| rat_to_string(ctx, &rat, format, radix, precision))
    }

    /// `Rational::ToUInt64_t()`
    pub fn to_u64(&self) -> CalcResult<u64> {
        let rat = self.to_rat();
        with_ctx(|ctx| rattoui64(ctx, &rat, RATIONAL_BASE, RATIONAL_PRECISION))
    }
}

impl From<i32> for Rational {
    /// `Rational(int32_t)`
    fn from(i: i32) -> Self {
        Rational::from_rat(i32torat(i))
    }
}
impl From<u32> for Rational {
    /// `Rational(uint32_t)`
    fn from(ui: u32) -> Self {
        Rational::from_rat(ui32torat(ui))
    }
}
impl From<u64> for Rational {
    /// `Rational(uint64_t)`: `(Rational{ hi } << 32) | lo`
    fn from(ui: u64) -> Self {
        let hi = ((ui >> 32) & 0xffff_ffff) as u32;
        let lo = ui as u32;

        // lshrat/orrat can only fail on allocation failure here.
        Rational::from(hi)
            .shl(&Rational::from(32i32))
            .and_then(|t| t.bitor(&Rational::from(lo)))
            .unwrap_or_else(|e| panic!("Rational::from(u64) failed: {e:#x}"))
    }
}

impl std::ops::Neg for &Rational {
    type Output = Rational;
    /// `Rational::operator-()`
    fn neg(self) -> Rational {
        Rational {
            p: Number::new(-self.p.sign, self.p.exp, self.p.mant.clone()),
            q: self.q.clone(),
        }
    }
}
impl std::ops::Neg for Rational {
    type Output = Rational;
    fn neg(self) -> Rational {
        -&self
    }
}

impl PartialEq for Rational {
    fn eq(&self, other: &Self) -> bool {
        self.try_eq(other).unwrap_or(false)
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.try_lt(other).ok()? {
            Some(Ordering::Less)
        } else if other.try_lt(self).ok()? {
            Some(Ordering::Greater)
        } else {
            Some(Ordering::Equal)
        }
    }
    fn lt(&self, other: &Self) -> bool {
        self.try_lt(other).unwrap_or(false)
    }
    fn gt(&self, other: &Self) -> bool {
        self.try_gt(other).unwrap_or(false)
    }
    fn le(&self, other: &Self) -> bool {
        self.try_le(other).unwrap_or(false)
    }
    fn ge(&self, other: &Self) -> bool {
        self.try_ge(other).unwrap_or(false)
    }
}
