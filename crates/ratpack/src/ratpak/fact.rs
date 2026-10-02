// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//-----------------------------------------------------------------------------
//  Package Title  ratpak
//  File           fact.c
//  Copyright      (C) 1995-96 Microsoft
//  Date           01-16-95
//
//
//  Description
//
//     Contains fact(orial) and supporting _gamma functions.
//
//-----------------------------------------------------------------------------

use super::basex::mulnumx;
use super::conv::{i32tonum, i32torat, rattoi32};
use super::exp::{exprat, lograt_, powratcomp};
use super::rat::{addrat_, divrat, fracrat, mulrat, subrat_, zerrat};
use super::support::{intrat, rat_gt, rat_lt, rat_neq};
use super::{BASEX, CalcResult, Ctx, Rat, absrat, inc, logratradix, sign};
use crate::{CALC_E_DOMAIN, CALC_E_OVERFLOW};

/// `NEGATE(x)`
fn negate(x: &mut Rat) {
    x.pp.sign = x.pp.sign.wrapping_mul(-1);
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: factrat, _gamma, gamma
//
//  ARGUMENTS:  x PRAT representation of number to take the sine of
//
//  RETURN: factorial of x in PRAT form.
//
//  EXPLANATION: This uses Taylor series
//
//      n
//     ___    2j
//   n \  ]  A       1          A
//  A   \   -----[ ---- - ---------------]
//      /   (2j)!  n+2j   (n+2j+1)(2j+1)
//     /__]
//     j=0
//
//                        / oo
//                        |    n-1 -x     __
//  This was derived from |   x   e  dx = |
//                        |               | (n) { = (n-1)! for +integers}
//                        / 0
//
//  It can be shown that the above series is within precision if A is chosen
//  big enough.
//                          A    n  precision
//  Based on the relation ne  = A 10            A was chosen as
//
//             precision
//  A = ln(Base         /n)+1
//  A += n*ln(A)  This is close enough for precision > base and n < 1.5
//
//-----------------------------------------------------------------------------

fn gamma(ctx: &Ctx, pn: &mut Rat, radix: u32, mut precision: i32) -> CalcResult<()> {
    // Set up constants and initial conditions
    let mut ratprec = i32torat(precision);

    // Find the best 'A' for convergence to the required precision.
    let mut a = i32torat(radix as i32);
    lograt_(ctx, &mut a, precision)?;
    mulrat(ctx, &mut a, &ratprec, precision)?;

    // Really is -ln(n)+1, but -ln(n) will be < 1
    // if we scale n between 0.5 and 1.5
    addrat_(ctx, &mut a, &ctx.rat_two, precision)?;
    let mut tmp = a.clone();
    lograt_(ctx, &mut tmp, precision)?;
    mulrat(ctx, &mut tmp, pn, precision)?;
    addrat_(ctx, &mut a, &tmp, precision)?;
    addrat_(ctx, &mut a, &ctx.rat_one, precision)?;

    // Calculate the necessary bump in precision and up the precision.
    // The following code is equivalent to
    // precision += ln(exp(a)*pow(a,n+1.5))-ln(radix));
    tmp = pn.clone();
    let mut one_pt_five = i32torat(3);
    divrat(ctx, &mut one_pt_five, &ctx.rat_two, precision)?;
    addrat_(ctx, &mut tmp, &one_pt_five, precision)?;
    let mut term = a.clone();
    powratcomp(ctx, &mut term, &tmp, radix, precision)?;
    tmp = a.clone();
    exprat(ctx, &mut tmp, radix, precision)?;
    mulrat(ctx, &mut term, &tmp, precision)?;
    lograt_(ctx, &mut term, precision)?;
    let rat_radix = i32torat(radix as i32);
    tmp = rat_radix.clone();
    lograt_(ctx, &mut tmp, precision)?;
    subrat_(ctx, &mut term, &tmp, precision)?;
    precision = precision.wrapping_add(rattoi32(ctx, &term, radix, precision)?);

    // Set up initial terms for series, refer to series in above comment block.
    let mut factorial = ctx.rat_one.clone(); // Start factorial out with one
    let mut count = i32tonum(0, BASEX);

    let mut mpy = a.clone();
    powratcomp(ctx, &mut mpy, pn, radix, precision)?;
    // a2=a^2
    let mut a2 = a.clone();
    mulrat(ctx, &mut a2, &a, precision)?;

    // sum=(1/n)-(a/(n+1))
    let mut sum = ctx.rat_one.clone();
    divrat(ctx, &mut sum, pn, precision)?;
    tmp = pn.clone();
    addrat_(ctx, &mut tmp, &ctx.rat_one, precision)?;
    term = a.clone();
    divrat(ctx, &mut term, &tmp, precision)?;
    subrat_(ctx, &mut sum, &term, precision)?;

    let mut err = rat_radix.clone();
    negate(&mut ratprec);
    powratcomp(ctx, &mut err, &ratprec, radix, precision)?;
    divrat(ctx, &mut err, &rat_radix, precision)?;

    // Just get something not tiny in term
    term = ctx.rat_two.clone();

    // Loop until precision is reached, or asked to halt.
    while !zerrat(&term) && rat_gt(ctx, &term, &err, precision)? {
        addrat_(ctx, pn, &ctx.rat_two, precision)?;

        // WARNING: mixing numbers and  rationals here.
        // for speed and efficiency.
        inc(ctx, &mut count)?;
        mulnumx(&mut factorial.pp, &count)?;
        inc(ctx, &mut count)?;
        mulnumx(&mut factorial.pp, &count)?;

        divrat(ctx, &mut factorial, &a2, precision)?;

        tmp = pn.clone();
        addrat_(ctx, &mut tmp, &ctx.rat_one, precision)?;
        term = Rat {
            pp: count.clone(),
            pq: ctx.num_one.clone(),
        };
        addrat_(ctx, &mut term, &ctx.rat_one, precision)?;
        mulrat(ctx, &mut term, &tmp, precision)?;
        tmp = a.clone();
        divrat(ctx, &mut tmp, &term, precision)?;

        term = ctx.rat_one.clone();
        divrat(ctx, &mut term, pn, precision)?;
        subrat_(ctx, &mut term, &tmp, precision)?;

        divrat(ctx, &mut term, &factorial, precision)?;
        addrat_(ctx, &mut sum, &term, precision)?;
        absrat(&mut term);
    }

    // Multiply by factor.
    mulrat(ctx, &mut sum, &mpy, precision)?;

    *pn = sum;
    Ok(())
}

pub(crate) fn factrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    if rat_gt(ctx, px, &ctx.rat_max_fact, precision)?
        || rat_lt(ctx, px, &ctx.rat_min_fact, precision)?
    {
        // Don't attempt factorial of anything too large or small.
        return Err(CALC_E_OVERFLOW);
    }

    let mut fact = ctx.rat_one.clone();

    let mut neg_rat_one = ctx.rat_one.clone();
    neg_rat_one.pp.sign = neg_rat_one.pp.sign.wrapping_mul(-1);

    let mut frac = px.clone();
    fracrat(ctx, &mut frac, radix, precision)?;

    // Check for negative integers and throw an error.
    if (zerrat(&frac) || (logratradix(ctx, &frac) <= precision.wrapping_neg())) && (sign(px) == -1)
    {
        return Err(CALC_E_DOMAIN);
    }
    while rat_gt(ctx, px, &ctx.rat_zero, precision)?
        && (logratradix(ctx, px) > precision.wrapping_neg())
    {
        mulrat(ctx, &mut fact, px, precision)?;
        subrat_(ctx, px, &ctx.rat_one, precision)?;
    }

    // Added to make numbers 'close enough' to integers use integer factorial.
    if logratradix(ctx, px) <= precision.wrapping_neg() {
        *px = ctx.rat_zero.clone();
        intrat(ctx, &mut fact, radix, precision)?;
    }

    while rat_lt(ctx, px, &neg_rat_one, precision)? {
        addrat_(ctx, px, &ctx.rat_one, precision)?;
        divrat(ctx, &mut fact, px, precision)?;
    }

    if rat_neq(ctx, px, &ctx.rat_zero, precision)? {
        addrat_(ctx, px, &ctx.rat_one, precision)?;
        gamma(ctx, px, radix, precision)?;
        mulrat(ctx, px, &fact, precision)?;
    } else {
        *px = fact;
    }
    Ok(())
}
