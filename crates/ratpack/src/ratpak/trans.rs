// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//----------------------------------------------------------------------------
//  File           trans.c
//  Copyright      (C) 1995-96 Microsoft
//  Date           01-16-95
//
//
//  Description
//
//     Contains sin, cos and tan for rationals
//
//----------------------------------------------------------------------------

use super::basex::mulnumx;
use super::conv::i32tonum;
use super::rat::{addrat_, divrat, mulrat, subrat_, zerrat};
use super::support::{inbetween, rat_ge, rat_gt, rat_le, scale, scale2pi};
use super::{CalcResult, Ctx, Rat, create_taylor, destroy_taylor, inc, small_enough_rat};
use crate::{AngleType, CALC_E_DOMAIN};

pub(crate) fn scalerat(
    ctx: &Ctx,
    pa: &mut Rat,
    angletype: AngleType,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    match angletype {
        AngleType::Radians => scale2pi(ctx, pa, radix, precision),
        AngleType::Degrees => scale(ctx, pa, &ctx.rat_360, radix, precision),
        AngleType::Gradians => scale(ctx, pa, &ctx.rat_400, radix, precision),
    }
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: sinrat, _sinrat
//
//  ARGUMENTS:  x PRAT representation of number to take the sine of
//
//  RETURN: sin of x in PRAT form.
//
//  EXPLANATION: This uses Taylor series
//
//    n
//   ___          2j+1
//   \  ]   j    X
//    \   -1  * ---------
//    /          (2j+1)!
//   /__]
//   j=0
//          or,
//    n
//   ___                                                 2
//   \  ]                                              -X
//    \   thisterm  ; where thisterm   = thisterm  * ---------
//    /           j                 j+1          j   (2j)*(2j+1)
//   /__]
//   j=0
//
//   thisterm  = X ;  and stop when thisterm < precision used.
//           0                              n
//
//-----------------------------------------------------------------------------

/// `_sinrat`
pub(crate) fn sinrat_(ctx: &Ctx, px: &mut Rat, precision: i32) -> CalcResult<()> {
    let (mut xx, _) = create_taylor(ctx, px, precision)?;

    let mut pret = px.clone();
    let mut thisterm = px.clone();

    let mut n2 = ctx.num_one.clone();
    xx.pp.sign = xx.pp.sign.wrapping_mul(-1);

    loop {
        // NEXTTERM(xx, INC(n2) DIVNUM(n2) INC(n2) DIVNUM(n2), precision);
        mulrat(ctx, &mut thisterm, &xx, precision)?;
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

    // Since *px might be epsilon above 1 or below -1, due to TRIMIT we need
    // this trick here.
    inbetween(ctx, px, &ctx.rat_one, precision)?;

    // Since *px might be epsilon near zero we must set it to zero.
    if rat_le(ctx, px, &ctx.rat_smallest, precision)?
        && rat_ge(ctx, px, &ctx.rat_negsmallest, precision)?
    {
        *px = ctx.rat_zero.clone();
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn sinrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    scale2pi(ctx, px, radix, precision)?;
    sinrat_(ctx, px, precision)
}

pub(crate) fn sinanglerat(
    ctx: &Ctx,
    pa: &mut Rat,
    angletype: AngleType,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    scalerat(ctx, pa, angletype, radix, precision)?;
    match angletype {
        AngleType::Degrees => {
            if rat_gt(ctx, pa, &ctx.rat_180, precision)? {
                subrat_(ctx, pa, &ctx.rat_360, precision)?;
            }
            divrat(ctx, pa, &ctx.rat_180, precision)?;
            mulrat(ctx, pa, &ctx.pi, precision)?;
        }
        AngleType::Gradians => {
            if rat_gt(ctx, pa, &ctx.rat_200, precision)? {
                subrat_(ctx, pa, &ctx.rat_400, precision)?;
            }
            divrat(ctx, pa, &ctx.rat_200, precision)?;
            mulrat(ctx, pa, &ctx.pi, precision)?;
        }
        AngleType::Radians => {}
    }
    sinrat_(ctx, pa, precision)
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: cosrat, _cosrat
//
//  ARGUMENTS:  x PRAT representation of number to take the cosine of
//
//  RETURN: cosine of x in PRAT form.
//
//  EXPLANATION: This uses Taylor series
//
//    n
//   ___    2j   j
//   \  ]  X   -1
//    \   ---------
//    /    (2j)!
//   /__]
//   j=0
//          or,
//    n
//   ___                                                 2
//   \  ]                                              -X
//    \   thisterm  ; where thisterm   = thisterm  * ---------
//    /           j                 j+1          j   (2j)*(2j+1)
//   /__]
//   j=0
//
//   thisterm  = 1 ;  and stop when thisterm < precision used.
//           0                              n
//
//-----------------------------------------------------------------------------

/// `_cosrat`
pub(crate) fn cosrat_(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    let (mut xx, _) = create_taylor(ctx, px, precision)?;

    let mut pret = Rat {
        pp: i32tonum(1, radix),
        pq: i32tonum(1, radix),
    };

    let mut thisterm = pret.clone();

    let mut n2 = i32tonum(0, radix);
    xx.pp.sign = xx.pp.sign.wrapping_mul(-1);

    loop {
        // NEXTTERM(xx, INC(n2) DIVNUM(n2) INC(n2) DIVNUM(n2), precision);
        mulrat(ctx, &mut thisterm, &xx, precision)?;
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
    // Since *px might be epsilon above 1 or below -1, due to TRIMIT we need
    // this trick here.
    inbetween(ctx, px, &ctx.rat_one, precision)?;
    // Since *px might be epsilon near zero we must set it to zero.
    if rat_le(ctx, px, &ctx.rat_smallest, precision)?
        && rat_ge(ctx, px, &ctx.rat_negsmallest, precision)?
    {
        *px = ctx.rat_zero.clone();
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn cosrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    scale2pi(ctx, px, radix, precision)?;
    cosrat_(ctx, px, radix, precision)
}

pub(crate) fn cosanglerat(
    ctx: &Ctx,
    pa: &mut Rat,
    angletype: AngleType,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    scalerat(ctx, pa, angletype, radix, precision)?;
    match angletype {
        AngleType::Degrees => {
            if rat_gt(ctx, pa, &ctx.rat_180, precision)? {
                let mut ptmp = ctx.rat_360.clone();
                subrat_(ctx, &mut ptmp, pa, precision)?;
                *pa = ptmp;
            }
            divrat(ctx, pa, &ctx.rat_180, precision)?;
            mulrat(ctx, pa, &ctx.pi, precision)?;
        }
        AngleType::Gradians => {
            if rat_gt(ctx, pa, &ctx.rat_200, precision)? {
                let mut ptmp = ctx.rat_400.clone();
                subrat_(ctx, &mut ptmp, pa, precision)?;
                *pa = ptmp;
            }
            divrat(ctx, pa, &ctx.rat_200, precision)?;
            mulrat(ctx, pa, &ctx.pi, precision)?;
        }
        AngleType::Radians => {}
    }
    cosrat_(ctx, pa, radix, precision)
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: tanrat, _tanrat
//
//  ARGUMENTS:  x PRAT representation of number to take the tangent of
//
//  RETURN: tan     of x in PRAT form.
//
//  EXPLANATION: This uses sinrat and cosrat
//
//-----------------------------------------------------------------------------

/// `_tanrat`
pub(crate) fn tanrat_(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    let mut ptmp = px.clone();
    sinrat_(ctx, px, precision)?;
    cosrat_(ctx, &mut ptmp, radix, precision)?;
    if zerrat(&ptmp) {
        return Err(CALC_E_DOMAIN);
    }
    divrat(ctx, px, &ptmp, precision)
}

#[allow(dead_code)]
pub(crate) fn tanrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    scale2pi(ctx, px, radix, precision)?;
    tanrat_(ctx, px, radix, precision)
}

pub(crate) fn tananglerat(
    ctx: &Ctx,
    pa: &mut Rat,
    angletype: AngleType,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    scalerat(ctx, pa, angletype, radix, precision)?;
    match angletype {
        AngleType::Degrees => {
            if rat_gt(ctx, pa, &ctx.rat_180, precision)? {
                subrat_(ctx, pa, &ctx.rat_180, precision)?;
            }
            divrat(ctx, pa, &ctx.rat_180, precision)?;
            mulrat(ctx, pa, &ctx.pi, precision)?;
        }
        AngleType::Gradians => {
            if rat_gt(ctx, pa, &ctx.rat_200, precision)? {
                subrat_(ctx, pa, &ctx.rat_200, precision)?;
            }
            divrat(ctx, pa, &ctx.rat_200, precision)?;
            mulrat(ctx, pa, &ctx.pi, precision)?;
        }
        AngleType::Radians => {}
    }
    tanrat_(ctx, pa, radix, precision)
}
