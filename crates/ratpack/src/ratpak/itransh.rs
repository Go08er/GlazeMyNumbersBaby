// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//-----------------------------------------------------------------------------
//  Package Title  ratpak
//  File           itransh.c
//  Copyright      (C) 1995-97 Microsoft
//  Date           01-16-95
//
//
//  Description
//
//    Contains inverse hyperbolic sin, cos, and tan functions.
//
//-----------------------------------------------------------------------------

use super::basex::mulnumx;
use super::exp::lograt_;
use super::rat::{addrat_, divrat, mulrat, rootrat, subrat_};
use super::support::{rat_gt, rat_lt};
use super::{CalcResult, Ctx, Rat, create_taylor, destroy_taylor, inc, small_enough_rat};
use crate::CALC_E_DOMAIN;

//-----------------------------------------------------------------------------
//
//  FUNCTION: asinhrat
//
//  ARGUMENTS:  x PRAT representation of number to take the inverse
//    hyperbolic sine of
//  RETURN: asinh of x in PRAT form.
//
//  EXPLANATION: This uses Taylor series
//
//    n
//   ___                                                   2 2
//   \  ]                                           -(2j+1) X
//    \   thisterm  ; where thisterm   = thisterm  * ---------
//    /           j                 j+1          j   (2j+2)*(2j+3)
//   /__]
//   j=0
//
//   thisterm  = X ;  and stop when thisterm < precision used.
//           0                              n
//
//   For abs(x) < .85, and
//
//   asinh(x) = log(x+sqrt(x^2+1))
//
//   For abs(x) >= .85
//
//-----------------------------------------------------------------------------

pub(crate) fn asinhrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    let mut neg_pt_eight_five = ctx.pt_eight_five.clone();
    neg_pt_eight_five.pp.sign = neg_pt_eight_five.pp.sign.wrapping_mul(-1);
    if rat_gt(ctx, px, &ctx.pt_eight_five, precision)?
        || rat_lt(ctx, px, &neg_pt_eight_five, precision)?
    {
        let mut ptmp = px.clone();
        mulrat(ctx, &mut ptmp, px, precision)?;
        addrat_(ctx, &mut ptmp, &ctx.rat_one, precision)?;
        rootrat(ctx, &mut ptmp, &ctx.rat_two, radix, precision)?;
        addrat_(ctx, px, &ptmp, precision)?;
        lograt_(ctx, px, precision)?;
    } else {
        let (mut xx, _) = create_taylor(ctx, px, precision)?;
        xx.pp.sign = xx.pp.sign.wrapping_mul(-1);

        let mut pret = px.clone();
        let mut thisterm = px.clone();

        let mut n2 = ctx.num_one.clone();

        loop {
            // NEXTTERM(xx, MULNUM(n2) MULNUM(n2) INC(n2) DIVNUM(n2) INC(n2) DIVNUM(n2), precision);
            mulrat(ctx, &mut thisterm, &xx, precision)?;
            mulnumx(&mut thisterm.pp, &n2)?;
            mulnumx(&mut thisterm.pp, &n2)?;
            inc(ctx, &mut n2)?;
            mulnumx(&mut thisterm.pq, &n2)?;
            inc(ctx, &mut n2)?;
            mulnumx(&mut thisterm.pq, &n2)?;
            addrat_(ctx, &mut pret, &thisterm, precision)?;
            if small_enough_rat(ctx, &thisterm, precision) {
                break;
            }
        }

        destroy_taylor(ctx, px, pret, precision);
    }
    Ok(())
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: acoshrat
//
//  ARGUMENTS:  x PRAT representation of number to take the inverse
//    hyperbolic cose of
//  RETURN: acosh of x in PRAT form.
//
//  EXPLANATION: This uses
//
//   acosh(x)=ln(x+sqrt(x^2-1))
//
//   For x >= 1
//
//-----------------------------------------------------------------------------

pub(crate) fn acoshrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    if rat_lt(ctx, px, &ctx.rat_one, precision)? {
        return Err(CALC_E_DOMAIN);
    }
    let mut ptmp = px.clone();
    mulrat(ctx, &mut ptmp, px, precision)?;
    subrat_(ctx, &mut ptmp, &ctx.rat_one, precision)?;
    rootrat(ctx, &mut ptmp, &ctx.rat_two, radix, precision)?;
    addrat_(ctx, px, &ptmp, precision)?;
    lograt_(ctx, px, precision)
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: atanhrat
//
//  ARGUMENTS:  x PRAT representation of number to take the inverse
//              hyperbolic tangent of
//
//  RETURN: atanh of x in PRAT form.
//
//  EXPLANATION: This uses
//
//             1     x+1
//  atanh(x) = -*ln(----)
//             2     x-1
//
//-----------------------------------------------------------------------------

pub(crate) fn atanhrat(ctx: &Ctx, px: &mut Rat, precision: i32) -> CalcResult<()> {
    let mut ptmp = px.clone();
    subrat_(ctx, &mut ptmp, &ctx.rat_one, precision)?;
    addrat_(ctx, px, &ctx.rat_one, precision)?;
    divrat(ctx, px, &ptmp, precision)?;
    px.pp.sign = px.pp.sign.wrapping_mul(-1);
    lograt_(ctx, px, precision)?;
    divrat(ctx, px, &ctx.rat_two, precision)
}
