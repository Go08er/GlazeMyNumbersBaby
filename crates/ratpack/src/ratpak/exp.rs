// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//-----------------------------------------------------------------------------
//  Package Title  ratpak
//  File           exp.c
//  Copyright      (C) 1995-96 Microsoft
//  Date           01-16-95
//
//
//  Description
//
//     Contains exp, and log functions for rationals
//
//-----------------------------------------------------------------------------

use super::basex::mulnumx;
use super::conv::{i32tonum, i32torat, ratpowi32, rattoi32};
use super::num::{addnum, addnum_s};
use super::rat::{addrat_, addrat_self, divrat, fracrat, mulrat, snaprat, subrat_, zerrat};
use super::support::{intrat, rat_equ, rat_gt, rat_le, rat_lt, trimit};
use super::{
    BASEX, BASEXPWR, CalcResult, Ctx, Rat, inc, lograt2, sign, small_enough_rat, trimtop,
    zero_over_zero,
};
use crate::CALC_E_DOMAIN;

//-----------------------------------------------------------------------------
//
//  FUNCTION: exprat
//
//  ARGUMENTS: x PRAT representation of number to exponentiate
//
//  RETURN: exp  of x in PRAT form.
//
//  EXPLANATION: This uses Taylor series
//
//    n
//   ___
//   \  ]                                               X
//    \   thisterm  ; where thisterm   = thisterm  * ---------
//    /           j                 j+1          j      j+1
//   /__]
//   j=0
//
//   thisterm  = X ;  and stop when thisterm < precision used.
//           0                              n
//
//-----------------------------------------------------------------------------

/// `_exprat`
pub(crate) fn exprat_(ctx: &Ctx, px: &mut Rat, precision: i32) -> CalcResult<()> {
    // CREATETAYLOR(): xx (= x*x) is never used by this series, so it is not
    // computed.
    let mut pret = zero_over_zero();

    addnum(&mut pret.pp, &ctx.num_one, BASEX)?;
    addnum(&mut pret.pq, &ctx.num_one, BASEX)?;
    let mut thisterm = pret.clone();

    let mut n2 = i32tonum(0, BASEX);

    loop {
        // NEXTTERM(*px, INC(n2) DIVNUM(n2), precision);
        mulrat(ctx, &mut thisterm, px, precision)?;
        inc(ctx, &mut n2)?;
        mulnumx(&mut thisterm.pq, &n2)?;
        addrat_(ctx, &mut pret, &thisterm, precision)?;
        if small_enough_rat(ctx, &thisterm, precision) {
            break;
        }
    }

    // DESTROYTAYLOR();
    trimit(ctx, &mut pret, precision);
    *px = pret;
    Ok(())
}

pub(crate) fn exprat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    if rat_gt(ctx, px, &ctx.rat_max_exp, precision)?
        || rat_lt(ctx, px, &ctx.rat_min_exp, precision)?
    {
        // Don't attempt exp of anything large.
        return Err(CALC_E_DOMAIN);
    }

    let mut pwr = ctx.rat_exp.clone();
    let mut pint = px.clone();

    intrat(ctx, &mut pint, radix, precision)?;

    let intpwr = rattoi32(ctx, &pint, radix, precision)?;
    ratpowi32(ctx, &mut pwr, intpwr, precision)?;

    subrat_(ctx, px, &pint, precision)?;

    // It just so happens to be an integral power of e.
    if rat_gt(ctx, px, &ctx.rat_negsmallest, precision)?
        && rat_lt(ctx, px, &ctx.rat_smallest, precision)?
    {
        *px = pwr;
    } else {
        exprat_(ctx, px, precision)?;
        mulrat(ctx, px, &pwr, precision)?;
    }
    Ok(())
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: lograt, _lograt, __lograt
//
//  ARGUMENTS: x PRAT representation of number to logarithim
//
//  RETURN: log  of x in PRAT form.
//
//  EXPLANATION: This uses Taylor series
//
//    n
//   ___
//   \  ]                                             j*(1-X)
//    \   thisterm  ; where thisterm   = thisterm  * ---------
//    /           j                 j+1          j      j+1
//   /__]
//   j=0
//
//   thisterm  = X ;  and stop when thisterm < precision used.
//           0                              n
//
//   Number is scaled between one and e_to_one_half prior to taking the
//   log. This is to keep execution time from exploding.
//
//   lograt tries to snap to zero. Use _lograt inside ratpak by default.
//   __lograt is part of _lograt private implementation and should not be used.
//
//-----------------------------------------------------------------------------

/// `__lograt`
fn lograt__(ctx: &Ctx, px: &mut Rat, precision: i32) -> CalcResult<()> {
    // CREATETAYLOR(): xx (= x*x) is never used by this series, so it is not
    // computed.

    // sub one from x
    // (*px)->pq->sign *= -1; addnum(&((*px)->pp), (*px)->pq, BASEX); (*px)->pq->sign *= -1;
    let qsign = px.pq.sign.wrapping_mul(-1);
    addnum_s(&mut px.pp, &px.pq, qsign, BASEX)?;

    let mut pret = px.clone();
    let mut thisterm = px.clone();

    let mut n2 = i32tonum(1, BASEX);
    px.pp.sign = px.pp.sign.wrapping_mul(-1);

    loop {
        // NEXTTERM(*px, MULNUM(n2) INC(n2) DIVNUM(n2), precision);
        mulrat(ctx, &mut thisterm, px, precision)?;
        mulnumx(&mut thisterm.pp, &n2)?;
        inc(ctx, &mut n2)?;
        mulnumx(&mut thisterm.pq, &n2)?;
        addrat_(ctx, &mut pret, &thisterm, precision)?;
        trimtop(ctx, px, precision);
        if small_enough_rat(ctx, &thisterm, precision) {
            break;
        }
    }

    // DESTROYTAYLOR();
    trimit(ctx, &mut pret, precision);
    *px = pret;
    Ok(())
}

/// `_lograt`
pub(crate) fn lograt_(ctx: &Ctx, px: &mut Rat, precision: i32) -> CalcResult<()> {
    // Check for someone taking the log of zero or a negative number.
    if rat_le(ctx, px, &ctx.rat_zero, precision)? {
        return Err(CALC_E_DOMAIN);
    }

    // Get number > 1, for scaling
    let fneglog = rat_lt(ctx, px, &ctx.rat_one, precision)?;
    if fneglog {
        std::mem::swap(&mut px.pp, &mut px.pq);
    }

    // Scale the number within BASEX factor of 1, for the large scale.
    // log(x*2^(BASEXPWR*k)) = BASEXPWR*k*log(2)+log(x)
    let mut pwr; // pwr is the large scaling factor.
    if lograt2(px) > 1 {
        let intpwr = lograt2(px) - 1;
        px.pq.exp = px.pq.exp.wrapping_add(intpwr);
        pwr = i32torat((intpwr as u32).wrapping_mul(BASEXPWR) as i32);
        mulrat(ctx, &mut pwr, &ctx.ln_two, precision)?;
        // ln(x+e)-ln(x) looks close to e when x is close to one using some
        // expansions.  This means we can trim past precision digits+1.
        trimtop(ctx, px, precision);
    } else {
        pwr = ctx.rat_zero.clone();
    }

    // offset is the incremental scaling factor.
    let mut offset = ctx.rat_zero.clone();
    // Scale the number between 1 and e_to_one_half, for the small scale.
    while rat_gt(ctx, px, &ctx.e_to_one_half, precision)? {
        divrat(ctx, px, &ctx.e_to_one_half, precision)?;
        addrat_(ctx, &mut offset, &ctx.rat_one, precision)?;
    }

    lograt__(ctx, px, precision)?;

    // Add the large and small scaling factors, take into account
    // small scaling was done in e_to_one_half chunks.
    divrat(ctx, &mut offset, &ctx.rat_two, precision)?;
    addrat_(ctx, &mut pwr, &offset, precision)?;

    // And add the resulting scaling factor to the answer.
    addrat_(ctx, px, &pwr, precision)?;

    trimit(ctx, px, precision);

    // If number started out < 1 rescale answer to negative.
    if fneglog {
        px.pp.sign = px.pp.sign.wrapping_mul(-1);
    }
    Ok(())
}

pub(crate) fn lograt(ctx: &Ctx, px: &mut Rat, precision: i32) -> CalcResult<()> {
    let a = px.clone();

    lograt_(ctx, px, precision)?;

    snaprat(ctx, px, &a, None, precision)
}

#[allow(dead_code)]
pub(crate) fn log10rat(ctx: &Ctx, px: &mut Rat, precision: i32) -> CalcResult<()> {
    lograt(ctx, px, precision)?;
    divrat(ctx, px, &ctx.ln_ten, precision)
}

//
// return if the given x is even number. The assumption here is its denominator is 1 and we are testing the numerator is
// even or not
fn is_even(ctx: &Ctx, x: &Rat, radix: u32, precision: i32) -> CalcResult<bool> {
    let mut tmp = x.clone();
    divrat(ctx, &mut tmp, &ctx.rat_two, precision)?;
    fracrat(ctx, &mut tmp, radix, precision)?;
    addrat_self(ctx, &mut tmp, precision)?;
    subrat_(ctx, &mut tmp, &ctx.rat_one, precision)?;
    rat_lt(ctx, &tmp, &ctx.rat_zero, precision)
}

//---------------------------------------------------------------------------
//
//  FUNCTION: powrat
//
//  ARGUMENTS: PRAT *px, PRAT y, uint32_t radix, int32_t precision
//
//  RETURN: none, sets *px to *px to the y.
//
//  EXPLANATION: Calculates the power of both px and
//  handles special cases where px is a perfect root.
//  Assumes, all checking has been done on validity of numbers.
//
//---------------------------------------------------------------------------

pub(crate) fn powrat(
    ctx: &Ctx,
    px: &mut Rat,
    y: &Rat,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    // Handle cases where px or y is 0 by calling powratcomp directly
    if zerrat(px) || zerrat(y) {
        return powratcomp(ctx, px, y, radix, precision);
    }
    // When y is 1, return px
    if rat_equ(ctx, y, &ctx.rat_one, precision)? {
        return Ok(());
    }

    match pow_rat_numerator_denominator(ctx, px, y, radix, precision) {
        Ok(result) => {
            *px = result;
            Ok(())
        }
        Err(_) => {
            // If calculating the power using numerator/denominator
            // failed, fall back to the less accurate method of
            // passing in the original y
            powratcomp(ctx, px, y, radix, precision)
        }
    }
}

/// `powratNumeratorDenominator`. Returns the new value of `*px` instead of
/// assigning it, so that `*px` is untouched when it throws (the caller's
/// `catch (...)` then retries with the original value).
fn pow_rat_numerator_denominator(
    ctx: &Ctx,
    px: &Rat,
    y: &Rat,
    radix: u32,
    precision: i32,
) -> CalcResult<Rat> {
    // Prepare rationals
    let mut y_numerator = ctx.rat_zero.clone(); // yNumerator->pq is 1 one
    let mut y_denominator = ctx.rat_zero.clone(); // yDenominator->pq is 1 one
    y_numerator.pp = y.pp.clone();
    y_denominator.pp = y.pq.clone();

    // Calculate the following use the Powers of Powers rule:
    // px ^ (yNum/yDenom) == px ^ yNum ^ (1/yDenom)
    // 1. For px ^ yNum, we call powratcomp directly which will call ratpowi32
    //    and store the result in pxPowNum
    // 2. For pxPowNum ^ (1/yDenom), we call powratcomp
    // 3. Validate the result of 2 by adding/subtracting 0.5, flooring and call powratcomp with yDenom
    //    on the floored result.

    // 1. Initialize result.
    let mut px_pow = px.clone();

    // 2. Calculate pxPow = px ^ yNumerator
    // if yNumerator is not 1
    if !rat_equ(ctx, &y_numerator, &ctx.rat_one, precision)? {
        powratcomp(ctx, &mut px_pow, &y_numerator, radix, precision)?;
    }

    // 2. Calculate pxPowNumDenom = pxPowNum ^ (1/yDenominator),
    // if yDenominator is not 1
    if !rat_equ(ctx, &y_denominator, &ctx.rat_one, precision)? {
        // Calculate 1 over y
        let mut oneovery_denom = ctx.rat_one.clone();
        divrat(ctx, &mut oneovery_denom, &y_denominator, precision)?;

        // ##################################
        // Take the oneoveryDenom power
        // ##################################
        let mut original_result = px_pow.clone();
        powratcomp(ctx, &mut original_result, &oneovery_denom, radix, precision)?;

        // ##################################
        // Round the originalResult to roundedResult
        // ##################################
        let mut rounded_result = original_result.clone();
        if rounded_result.pp.sign == -1 {
            subrat_(ctx, &mut rounded_result, &ctx.rat_half, precision)?;
        } else {
            addrat_(ctx, &mut rounded_result, &ctx.rat_half, precision)?;
        }
        intrat(ctx, &mut rounded_result, radix, precision)?;

        // ##################################
        // Take the yDenom power of the roundedResult.
        // ##################################
        let mut rounded_power = rounded_result.clone();
        powratcomp(ctx, &mut rounded_power, &y_denominator, radix, precision)?;

        // ##################################
        // if roundedPower == px,
        // we found an exact power in roundedResult
        // ##################################
        if rat_equ(ctx, &rounded_power, &px_pow, precision)? {
            Ok(rounded_result)
        } else {
            Ok(original_result)
        }
    } else {
        Ok(px_pow)
    }
}

//---------------------------------------------------------------------------
//
//  FUNCTION: powratcomp
//
//  ARGUMENTS: PRAT *px, and PRAT y
//
//  RETURN: none, sets *px to *px to the y.
//
//  EXPLANATION: This uses x^y=e(y*ln(x)), or a more exact calculation where
//  y is an integer.
//  Assumes, all checking has been done on validity of numbers.
//
//---------------------------------------------------------------------------

pub(crate) fn powratcomp(
    ctx: &Ctx,
    px: &mut Rat,
    y: &Rat,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    let mut sgn = sign(px);

    // Take the absolute value
    px.pp.sign = 1;
    px.pq.sign = 1;

    if zerrat(px) {
        // *px is zero.
        if rat_lt(ctx, y, &ctx.rat_zero, precision)? {
            return Err(CALC_E_DOMAIN);
        } else if zerrat(y) {
            // *px and y are both zero, special case a 1 return.
            *px = ctx.rat_one.clone();
            // Ensure sign is positive.
            sgn = 1;
        }
    } else {
        let mut pxint = px.clone();
        subrat_(ctx, &mut pxint, &ctx.rat_one, precision)?;
        if rat_gt(ctx, &pxint, &ctx.rat_negsmallest, precision)?
            && rat_lt(ctx, &pxint, &ctx.rat_smallest, precision)?
            && (sgn == 1)
        {
            // *px is one, special case a 1 return.
            *px = ctx.rat_one.clone();
            // Ensure sign is positive.
            sgn = 1;
        } else {
            // Only do the exp if the number isn't zero or one
            let mut podd = y.clone();
            fracrat(ctx, &mut podd, radix, precision)?;
            if rat_gt(ctx, &podd, &ctx.rat_negsmallest, precision)?
                && rat_lt(ctx, &podd, &ctx.rat_smallest, precision)?
            {
                // If power is an integer let ratpowi32 deal with it.
                let mut iy = y.clone();
                subrat_(ctx, &mut iy, &podd, precision)?;
                let inty = rattoi32(ctx, &iy, radix, precision)?;

                let mut plnx = px.clone();
                lograt_(ctx, &mut plnx, precision)?;
                mulrat(ctx, &mut plnx, &iy, precision)?;
                if rat_gt(ctx, &plnx, &ctx.rat_max_exp, precision)?
                    || rat_lt(ctx, &plnx, &ctx.rat_min_exp, precision)?
                {
                    // Don't attempt exp of anything large or small.A
                    return Err(CALC_E_DOMAIN);
                }
                ratpowi32(ctx, px, inty, precision)?;
                if (inty & 1) == 0 {
                    sgn = 1;
                }
            } else {
                // power is a fraction
                if sgn == -1 {
                    // Need to throw an error if the exponent has an even denominator.
                    // As a first step, the numerator and denominator must be divided by 2 as many times as
                    //     possible, so that 2/6 is allowed.
                    // If the final numerator is still even, the end result should be positive.
                    let mut fbad_exponent = false;

                    // Get the numbers in arbitrary precision rational number format
                    let mut p_numerator = ctx.rat_zero.clone(); // pNumerator->pq is 1 one
                    let mut p_denominator = ctx.rat_zero.clone(); // pDenominator->pq is 1 one

                    p_numerator.pp = y.pp.clone();
                    p_numerator.pp.sign = 1;
                    p_denominator.pp = y.pq.clone();
                    p_denominator.pp.sign = 1;

                    // both Numerator & denominator is even
                    while is_even(ctx, &p_numerator, radix, precision)?
                        && is_even(ctx, &p_denominator, radix, precision)?
                    {
                        divrat(ctx, &mut p_numerator, &ctx.rat_two, precision)?;
                        divrat(ctx, &mut p_denominator, &ctx.rat_two, precision)?;
                    }
                    if is_even(ctx, &p_denominator, radix, precision)? {
                        // denominator is still even
                        fbad_exponent = true;
                    }
                    if is_even(ctx, &p_numerator, radix, precision)? {
                        // numerator is still even
                        sgn = 1;
                    }

                    if fbad_exponent {
                        return Err(CALC_E_DOMAIN);
                    }
                } else {
                    // If the exponent is not odd disregard the sign.
                    sgn = 1;
                }

                lograt_(ctx, px, precision)?;
                mulrat(ctx, px, y, precision)?;
                exprat(ctx, px, radix, precision)?;
            }
        }
    }
    px.pp.sign = px.pp.sign.wrapping_mul(sgn);
    Ok(())
}
