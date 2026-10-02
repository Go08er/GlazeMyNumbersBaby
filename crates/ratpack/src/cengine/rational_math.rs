// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CEngine/RationalMath.cpp` (`CalcEngine::RationalMath`).

use crate::ratpak::exp::{exprat, lograt, powrat};
use crate::ratpak::fact::factrat;
use crate::ratpak::itrans::{acosanglerat, asinanglerat, atananglerat};
use crate::ratpak::itransh::{acoshrat, asinhrat, atanhrat};
use crate::ratpak::logic::modrat;
use crate::ratpak::rat::fracrat;
use crate::ratpak::support::intrat;
use crate::ratpak::trans::{cosanglerat, sinanglerat, tananglerat};
use crate::ratpak::transh::{coshrat, sinhrat, tanhrat};
use crate::ratpak::{Ctx, Rat, with_ctx};
use crate::{AngleType, CalcResult, Number, RATIONAL_BASE, RATIONAL_PRECISION, Rational};

fn unary(rat: &Rational, f: impl FnOnce(&Ctx, &mut Rat) -> CalcResult<()>) -> CalcResult<Rational> {
    let mut prat = rat.to_rat();
    with_ctx(|ctx| f(ctx, &mut prat))?;
    Ok(Rational::from_rat(prat))
}

pub fn frac(r: &Rational) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        fracrat(ctx, x, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn integer(r: &Rational) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        intrat(ctx, x, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn pow(base: &Rational, pow: &Rational) -> CalcResult<Rational> {
    let pow_rat = pow.to_rat();
    unary(base, |ctx, x| {
        powrat(ctx, x, &pow_rat, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn root(base: &Rational, root: &Rational) -> CalcResult<Rational> {
    pow(base, &invert(root)?)
}

pub fn fact(r: &Rational) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        factrat(ctx, x, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn exp(r: &Rational) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        exprat(ctx, x, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn log(r: &Rational) -> CalcResult<Rational> {
    unary(r, |ctx, x| lograt(ctx, x, RATIONAL_PRECISION))
}

pub fn log10(r: &Rational) -> CalcResult<Rational> {
    log(r)?.div(&crate::ln_ten())
}

pub fn invert(r: &Rational) -> CalcResult<Rational> {
    Rational::from(1i32).div(r)
}

pub fn abs(r: &Rational) -> CalcResult<Rational> {
    Ok(Rational::from_pq(
        Number::new(1, r.p().exp(), r.p().mantissa().to_vec()),
        Number::new(1, r.q().exp(), r.q().mantissa().to_vec()),
    ))
}

pub fn sin(r: &Rational, a: AngleType) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        sinanglerat(ctx, x, a, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn cos(r: &Rational, a: AngleType) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        cosanglerat(ctx, x, a, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn tan(r: &Rational, a: AngleType) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        tananglerat(ctx, x, a, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn asin(r: &Rational, a: AngleType) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        asinanglerat(ctx, x, a, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn acos(r: &Rational, a: AngleType) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        acosanglerat(ctx, x, a, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn atan(r: &Rational, a: AngleType) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        atananglerat(ctx, x, a, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn sinh(r: &Rational) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        sinhrat(ctx, x, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn cosh(r: &Rational) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        coshrat(ctx, x, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn tanh(r: &Rational) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        tanhrat(ctx, x, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn asinh(r: &Rational) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        asinhrat(ctx, x, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn acosh(r: &Rational) -> CalcResult<Rational> {
    unary(r, |ctx, x| {
        acoshrat(ctx, x, RATIONAL_BASE, RATIONAL_PRECISION)
    })
}

pub fn atanh(r: &Rational) -> CalcResult<Rational> {
    unary(r, |ctx, x| atanhrat(ctx, x, RATIONAL_PRECISION))
}

/// `RationalMath::Mod`: calculate the modulus after division, the sign of
/// the result will match the sign of `b`.
pub fn modulo(a: &Rational, b: &Rational) -> CalcResult<Rational> {
    let pn = b.to_rat();
    unary(a, |ctx, x| modrat(ctx, x, &pn))
}
