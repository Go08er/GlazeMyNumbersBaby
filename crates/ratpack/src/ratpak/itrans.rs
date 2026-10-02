// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//-----------------------------------------------------------------------------
//  Package Title  ratpak
//  File           itrans.c
//  Copyright      (C) 1995-96 Microsoft
//  Date           01-16-95
//
//
//  Description
//
//     Contains inverse sin, cos, tan functions for rationals
//
//-----------------------------------------------------------------------------

use super::basex::mulnumx;
use super::conv::i32tonum;
use super::rat::{addrat_, divrat, mulrat, rootrat, subrat_};
use super::support::{rat_equ, rat_ge, rat_gt, rat_le};
use super::{
    BASEX, CalcResult, Ctx, Rat, create_taylor, destroy_taylor, inc, sign, small_enough_rat,
};
use crate::{AngleType, CALC_E_DOMAIN};

pub(crate) fn ascalerat(
    ctx: &Ctx,
    pa: &mut Rat,
    angletype: AngleType,
    precision: i32,
) -> CalcResult<()> {
    match angletype {
        AngleType::Radians => {}
        AngleType::Degrees => {
            divrat(ctx, pa, &ctx.two_pi, precision)?;
            mulrat(ctx, pa, &ctx.rat_360, precision)?;
        }
        AngleType::Gradians => {
            divrat(ctx, pa, &ctx.two_pi, precision)?;
            mulrat(ctx, pa, &ctx.rat_400, precision)?;
        }
    }
    Ok(())
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: asinrat, _asinrat
//
//  ARGUMENTS: x PRAT representation of number to take the inverse
//    sine of
//  RETURN: asin  of x in PRAT form.
//
//  EXPLANATION: This uses Taylor series
//
//    n
//   ___                                                   2 2
//   \  ]                                            (2j+1) X
//    \   thisterm  ; where thisterm   = thisterm  * ---------
//    /           j                 j+1          j   (2j+2)*(2j+3)
//   /__]
//   j=0
//
//   thisterm  = X ;  and stop when thisterm < precision used.
//           0                              n
//
//   If abs(x) > 0.85 then an alternate form is used
//      pi/2-sgn(x)*asin(sqrt(1-x^2)
//
//-----------------------------------------------------------------------------

/// `_asinrat`
fn asinrat_(ctx: &Ctx, px: &mut Rat, precision: i32) -> CalcResult<()> {
    let (xx, _) = create_taylor(ctx, px, precision)?;
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
    Ok(())
}

pub(crate) fn asinanglerat(
    ctx: &Ctx,
    pa: &mut Rat,
    angletype: AngleType,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    asinrat(ctx, pa, radix, precision)?;
    ascalerat(ctx, pa, angletype, precision)
}

pub(crate) fn asinrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    let sgn = sign(px);

    px.pp.sign = 1;
    px.pq.sign = 1;

    // Avoid the really bad part of the asin curve near +/-1.
    let mut phack = px.clone();
    subrat_(ctx, &mut phack, &ctx.rat_one, precision)?;
    // Since *px might be epsilon near zero we must set it to zero.
    if rat_le(ctx, &phack, &ctx.rat_smallest, precision)?
        && rat_ge(ctx, &phack, &ctx.rat_negsmallest, precision)?
    {
        *px = ctx.pi_over_two.clone();
    } else if rat_gt(ctx, px, &ctx.pt_eight_five, precision)? {
        if rat_gt(ctx, px, &ctx.rat_one, precision)? {
            subrat_(ctx, px, &ctx.rat_one, precision)?;
            if rat_gt(ctx, px, &ctx.rat_smallest, precision)? {
                return Err(CALC_E_DOMAIN);
            } else {
                *px = ctx.rat_one.clone();
            }
        }
        let pret = px.clone();
        mulrat(ctx, px, &pret, precision)?;
        px.pp.sign = px.pp.sign.wrapping_mul(-1);
        addrat_(ctx, px, &ctx.rat_one, precision)?;
        rootrat(ctx, px, &ctx.rat_two, radix, precision)?;
        asinrat_(ctx, px, precision)?;
        px.pp.sign = px.pp.sign.wrapping_mul(-1);
        addrat_(ctx, px, &ctx.pi_over_two, precision)?;
    } else {
        asinrat_(ctx, px, precision)?;
    }
    px.pp.sign = sgn;
    px.pq.sign = 1;
    Ok(())
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: acosrat, _acosrat
//
//  ARGUMENTS: x PRAT representation of number to take the inverse
//    cosine of
//  RETURN: acos  of x in PRAT form.
//
//  EXPLANATION: This uses Taylor series
//
//    n
//   ___                                                   2 2
//   \  ]                                            (2j+1) X
//    \   thisterm  ; where thisterm   = thisterm  * ---------
//    /           j                 j+1          j   (2j+2)*(2j+3)
//   /__]
//   j=0
//
//   thisterm  = 1 ;  and stop when thisterm < precision used.
//           0                              n
//
//   In this case pi/2-asin(x) is used.  At least for now _acosrat isn't
//      called.
//
//-----------------------------------------------------------------------------

pub(crate) fn acosanglerat(
    ctx: &Ctx,
    pa: &mut Rat,
    angletype: AngleType,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    acosrat(ctx, pa, radix, precision)?;
    ascalerat(ctx, pa, angletype, precision)
}

/// `_acosrat` (unused in the C++ as well)
#[allow(dead_code)]
fn acosrat_(ctx: &Ctx, px: &mut Rat, precision: i32) -> CalcResult<()> {
    let (xx, mut pret) = create_taylor(ctx, px, precision)?;

    let mut thisterm = Rat {
        pp: i32tonum(1, BASEX),
        pq: i32tonum(1, BASEX),
    };

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
    Ok(())
}

pub(crate) fn acosrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    let sgn = sign(px);

    px.pp.sign = 1;
    px.pq.sign = 1;

    if rat_equ(ctx, px, &ctx.rat_one, precision)? {
        if sgn == -1 {
            *px = ctx.pi.clone();
        } else {
            *px = ctx.rat_zero.clone();
        }
    } else {
        px.pp.sign = sgn;
        asinrat(ctx, px, radix, precision)?;
        px.pp.sign = px.pp.sign.wrapping_mul(-1);
        addrat_(ctx, px, &ctx.pi_over_two, precision)?;
    }
    Ok(())
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: atanrat, _atanrat
//
//  ARGUMENTS: x PRAT representation of number to take the inverse
//              hyperbolic tangent of
//
//  RETURN: atanh of x in PRAT form.
//
//  EXPLANATION: This uses Taylor series
//
//    n
//   ___                                                   2
//   \  ]                                            (2j)*X (-1^j)
//    \   thisterm  ; where thisterm   = thisterm  * ---------
//    /           j                 j+1          j   (2j+2)
//   /__]
//   j=0
//
//   thisterm  = X ;  and stop when thisterm < precision used.
//           0                              n
//
//   If abs(x) > 0.85 then an alternate form is used
//      asin(x/sqrt(q+x^2))
//
//   And if abs(x) > 2.0 then this form is used.
//
//   pi/2 - atan(1/x)
//
//-----------------------------------------------------------------------------

pub(crate) fn atananglerat(
    ctx: &Ctx,
    pa: &mut Rat,
    angletype: AngleType,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    atanrat(ctx, pa, radix, precision)?;
    ascalerat(ctx, pa, angletype, precision)
}

/// `_atanrat`
fn atanrat_(ctx: &Ctx, px: &mut Rat, precision: i32) -> CalcResult<()> {
    let (mut xx, _) = create_taylor(ctx, px, precision)?;

    let mut pret = px.clone();
    let mut thisterm = px.clone();

    let mut n2 = ctx.num_one.clone();

    xx.pp.sign = xx.pp.sign.wrapping_mul(-1);

    loop {
        // NEXTTERM(xx, MULNUM(n2) INC(n2) INC(n2) DIVNUM(n2), precision);
        mulrat(ctx, &mut thisterm, &xx, precision)?;
        mulnumx(&mut thisterm.pp, &n2)?;
        inc(ctx, &mut n2)?;
        inc(ctx, &mut n2)?;
        mulnumx(&mut thisterm.pq, &n2)?;
        addrat_(ctx, &mut pret, &thisterm, precision)?;
        if small_enough_rat(ctx, &thisterm, precision) {
            break;
        }
    }

    destroy_taylor(ctx, px, pret, precision);
    Ok(())
}

pub(crate) fn atanrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    let sgn = sign(px);

    px.pp.sign = 1;
    px.pq.sign = 1;

    if rat_gt(ctx, px, &ctx.pt_eight_five, precision)? {
        if rat_gt(ctx, px, &ctx.rat_two, precision)? {
            px.pp.sign = sgn;
            px.pq.sign = 1;
            let mut tmpx = ctx.rat_one.clone();
            divrat(ctx, &mut tmpx, px, precision)?;
            atanrat_(ctx, &mut tmpx, precision)?;
            tmpx.pp.sign = sgn;
            tmpx.pq.sign = 1;
            *px = ctx.pi_over_two.clone();
            subrat_(ctx, px, &tmpx, precision)?;
        } else {
            px.pp.sign = sgn;
            let mut tmpx = px.clone();
            mulrat(ctx, &mut tmpx, px, precision)?;
            addrat_(ctx, &mut tmpx, &ctx.rat_one, precision)?;
            rootrat(ctx, &mut tmpx, &ctx.rat_two, radix, precision)?;
            divrat(ctx, px, &tmpx, precision)?;
            asinrat(ctx, px, radix, precision)?;
            px.pp.sign = sgn;
            px.pq.sign = 1;
        }
    } else {
        px.pp.sign = sgn;
        px.pq.sign = 1;
        atanrat_(ctx, px, precision)?;
    }
    if rat_gt(ctx, px, &ctx.pi_over_two, precision)? {
        subrat_(ctx, px, &ctx.pi, precision)?;
    }
    Ok(())
}
