// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//-----------------------------------------------------------------------------
//  Package Title  ratpak
//  File           transh.c
//  Copyright      (C) 1995-96 Microsoft
//  Date           01-16-95
//
//
//  Description
//
//     Contains hyperbolic sin, cos, and tan for rationals.
//
//-----------------------------------------------------------------------------

use super::basex::mulnumx;
use super::conv::i32tonum;
use super::exp::exprat;
use super::rat::{addrat_, divrat, mulrat, subrat_};
use super::support::{rat_ge, rat_lt};
use super::{CalcResult, Ctx, Rat, create_taylor, destroy_taylor, inc, small_enough_rat};
use crate::CALC_E_DOMAIN;

fn is_valid_for_hyp_func(ctx: &Ctx, px: &Rat, precision: i32) -> CalcResult<bool> {
    let mut ptmp = ctx.rat_min_exp.clone();
    divrat(ctx, &mut ptmp, &ctx.rat_ten, precision)?;
    Ok(!rat_lt(ctx, px, &ptmp, precision)?)
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: sinhrat, _sinhrat
//
//  ARGUMENTS:  x PRAT representation of number to take the sine hyperbolic
//    of
//  RETURN: sinh of x in PRAT form.
//
//  EXPLANATION: This uses Taylor series
//
//    n
//   ___    2j+1
//   \  ]  X
//    \   ---------
//    /    (2j+1)!
//   /__]
//   j=0
//          or,
//    n
//   ___                                                 2
//   \  ]                                               X
//    \   thisterm  ; where thisterm   = thisterm  * ---------
//    /           j                 j+1          j   (2j)*(2j+1)
//   /__]
//   j=0
//
//   thisterm  = X ;  and stop when thisterm < precision used.
//           0                              n
//
//   if x is bigger than 1.0 (e^x-e^-x)/2 is used.
//
//-----------------------------------------------------------------------------

/// `_sinhrat`
fn sinhrat_(ctx: &Ctx, px: &mut Rat, precision: i32) -> CalcResult<()> {
    if !is_valid_for_hyp_func(ctx, px, precision)? {
        // Don't attempt exp of anything large or small
        return Err(CALC_E_DOMAIN);
    }

    let (xx, _) = create_taylor(ctx, px, precision)?;

    let mut pret = px.clone();
    let mut thisterm = pret.clone();

    let mut n2 = ctx.num_one.clone();

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
    Ok(())
}

pub(crate) fn sinhrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    if rat_ge(ctx, px, &ctx.rat_one, precision)? {
        let mut tmpx = px.clone();
        exprat(ctx, px, radix, precision)?;
        tmpx.pp.sign = tmpx.pp.sign.wrapping_mul(-1);
        exprat(ctx, &mut tmpx, radix, precision)?;
        subrat_(ctx, px, &tmpx, precision)?;
        divrat(ctx, px, &ctx.rat_two, precision)?;
    } else {
        sinhrat_(ctx, px, precision)?;
    }
    Ok(())
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: coshrat
//
//  ARGUMENTS:  x PRAT representation of number to take the cosine
//              hyperbolic of
//
//  RETURN: cosh  of x in PRAT form.
//
//  EXPLANATION: This uses Taylor series
//
//    n
//   ___    2j
//   \  ]  X
//    \   ---------
//    /    (2j)!
//   /__]
//   j=0
//          or,
//    n
//   ___                                                 2
//   \  ]                                               X
//    \   thisterm  ; where thisterm   = thisterm  * ---------
//    /           j                 j+1          j   (2j)*(2j+1)
//   /__]
//   j=0
//
//   thisterm  = 1 ;  and stop when thisterm < precision used.
//           0                              n
//
//   if x is bigger than 1.0 (e^x+e^-x)/2 is used.
//
//-----------------------------------------------------------------------------

/// `_coshrat`
fn coshrat_(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    if !is_valid_for_hyp_func(ctx, px, precision)? {
        // Don't attempt exp of anything large or small
        return Err(CALC_E_DOMAIN);
    }

    let (xx, _) = create_taylor(ctx, px, precision)?;

    let mut pret = Rat {
        pp: i32tonum(1, radix),
        pq: i32tonum(1, radix),
    };

    let mut thisterm = pret.clone();

    let mut n2 = i32tonum(0, radix);

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
    Ok(())
}

pub(crate) fn coshrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    px.pp.sign = 1;
    px.pq.sign = 1;
    if rat_ge(ctx, px, &ctx.rat_one, precision)? {
        let mut tmpx = px.clone();
        exprat(ctx, px, radix, precision)?;
        tmpx.pp.sign = tmpx.pp.sign.wrapping_mul(-1);
        exprat(ctx, &mut tmpx, radix, precision)?;
        addrat_(ctx, px, &tmpx, precision)?;
        divrat(ctx, px, &ctx.rat_two, precision)?;
    } else {
        coshrat_(ctx, px, radix, precision)?;
    }
    // Since *px might be epsilon below 1 due to TRIMIT
    // we need this trick here.
    if rat_lt(ctx, px, &ctx.rat_one, precision)? {
        *px = ctx.rat_one.clone();
    }
    Ok(())
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: tanhrat
//
//  ARGUMENTS:  x PRAT representation of number to take the tangent
//              hyperbolic of
//
//  RETURN: tanh    of x in PRAT form.
//
//  EXPLANATION: This uses sinhrat and coshrat
//
//-----------------------------------------------------------------------------

pub(crate) fn tanhrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    let mut ptmp = px.clone();
    sinhrat(ctx, px, radix, precision)?;
    coshrat(ctx, &mut ptmp, radix, precision)?;
    mulnumx(&mut px.pp, &ptmp.pq)?;
    mulnumx(&mut px.pq, &ptmp.pp)?;
    Ok(())
}
