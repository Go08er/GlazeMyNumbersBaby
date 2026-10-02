// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//---------------------------------------------------------------------------
//  Package Title  ratpak
//  File           num.c
//  Copyright      (C) 1995-99 Microsoft
//  Date           01-16-95
//
//
//  Description
//
//     Contains routines for and, or, xor, not and other support
//
//---------------------------------------------------------------------------

use super::basex::mulnumx;
use super::conv::{createnum, ratpowi32, rattoi32};
use super::num::remnum;
use super::rat::{addrat_, divrat, mulrat, zerrat};
use super::support::{intrat, rat_gt, rat_lt};
use super::{BASEX, CalcResult, Ctx, Number, Rat, renormalize, sign};
use crate::{CALC_E_DOMAIN, CALC_E_INDEFINITE};

pub(crate) fn lshrat(
    ctx: &Ctx,
    pa: &mut Rat,
    b: &Rat,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    intrat(ctx, pa, radix, precision)?;
    if !super::num::zernum(&pa.pp) {
        // If input is zero we're done.
        if rat_gt(ctx, b, &ctx.rat_max_exp, precision)? {
            // Don't attempt lsh of anything big
            return Err(CALC_E_DOMAIN);
        }
        let intb = rattoi32(ctx, b, radix, precision)?;
        let mut pwr = ctx.rat_two.clone();
        ratpowi32(ctx, &mut pwr, intb, precision)?;
        mulrat(ctx, pa, &pwr, precision)?;
    }
    Ok(())
}

pub(crate) fn rshrat(
    ctx: &Ctx,
    pa: &mut Rat,
    b: &Rat,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    intrat(ctx, pa, radix, precision)?;
    if !super::num::zernum(&pa.pp) {
        // If input is zero we're done.
        if rat_lt(ctx, b, &ctx.rat_min_exp, precision)? {
            // Don't attempt rsh of anything big and negative.
            return Err(CALC_E_DOMAIN);
        }
        let intb = rattoi32(ctx, b, radix, precision)?;
        let mut pwr = ctx.rat_two.clone();
        ratpowi32(ctx, &mut pwr, intb, precision)?;
        divrat(ctx, pa, &pwr, precision)?;
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BoolFunc {
    And,
    Or,
    Xor,
}

pub(crate) fn andrat(
    ctx: &Ctx,
    pa: &mut Rat,
    b: &Rat,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    boolrat(ctx, pa, b, BoolFunc::And, radix, precision)
}

pub(crate) fn orrat(
    ctx: &Ctx,
    pa: &mut Rat,
    b: &Rat,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    boolrat(ctx, pa, b, BoolFunc::Or, radix, precision)
}

pub(crate) fn xorrat(
    ctx: &Ctx,
    pa: &mut Rat,
    b: &Rat,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    boolrat(ctx, pa, b, BoolFunc::Xor, radix, precision)
}

//---------------------------------------------------------------------------
//
//    FUNCTION: boolrat
//
//    DESCRIPTION: Does the rational equivalent of *pa op= b;
//
//---------------------------------------------------------------------------

fn boolrat(
    ctx: &Ctx,
    pa: &mut Rat,
    b: &Rat,
    func: BoolFunc,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    intrat(ctx, pa, radix, precision)?;
    let mut tmp = b.clone();
    intrat(ctx, &mut tmp, radix, precision)?;

    boolnum(&mut pa.pp, &tmp.pp, func)
}

//---------------------------------------------------------------------------
//
//    FUNCTION: boolnum
//
//    DESCRIPTION: Does the number equivalent of *pa &= b.
//    radix doesn't matter for logicals.
//    WARNING: Assumes numbers are unsigned.
//
//---------------------------------------------------------------------------

fn boolnum(pa: &mut Number, b: &Number, func: BoolFunc) -> CalcResult<()> {
    let a = &*pa;
    let mut cdigits = a
        .cdigit()
        .wrapping_add(a.exp)
        .max(b.cdigit().wrapping_add(b.exp))
        .wrapping_sub(a.exp.min(b.exp));
    let mut c = createnum(cdigits as u32)?;
    let c_exp = a.exp.min(b.exp);
    let mut mexp = c_exp;
    let c_cdigit_init = cdigits;
    let mut c_cdigit = cdigits;
    let mut ia = 0usize;
    let mut ib = 0usize;
    let mut ic = 0usize;
    while cdigits > 0 {
        let da = if mexp >= a.exp
            && cdigits.wrapping_add(a.exp).wrapping_sub(c_exp)
                > c_cdigit_init.wrapping_sub(a.cdigit())
        {
            let d = a.mant[ia];
            ia += 1;
            d
        } else {
            0
        };
        let db = if mexp >= b.exp
            && cdigits.wrapping_add(b.exp).wrapping_sub(c_exp)
                > c_cdigit_init.wrapping_sub(b.cdigit())
        {
            let d = b.mant[ib];
            ib += 1;
            d
        } else {
            0
        };
        c[ic] = match func {
            BoolFunc::And => da & db,
            BoolFunc::Or => da | db,
            BoolFunc::Xor => da ^ db,
        };
        ic += 1;
        cdigits -= 1;
        mexp = mexp.wrapping_add(1);
    }
    let c_sign = a.sign;
    while c_cdigit > 1 && c[(c_cdigit - 1) as usize] == 0 {
        c_cdigit -= 1;
    }
    c.truncate(c_cdigit.max(0) as usize);
    *pa = Number {
        sign: c_sign,
        exp: c_exp,
        mant: c,
    };
    Ok(())
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: remrat
//
//    DESCRIPTION: Calculate the remainder of *pa / b,
//                 equivalent of 'pa % b' in C/C++ and produces a result
//                 that is either zero or has the same sign as the dividend.
//
//-----------------------------------------------------------------------------

pub(crate) fn remrat(pa: &mut Rat, b: &Rat) -> CalcResult<()> {
    if zerrat(b) {
        return Err(CALC_E_INDEFINITE);
    }

    let mut tmp = b.clone();

    mulnumx(&mut pa.pp, &tmp.pq)?;
    mulnumx(&mut tmp.pp, &pa.pq)?;
    remnum(&mut pa.pp, &tmp.pp, BASEX)?;
    mulnumx(&mut pa.pq, &tmp.pq)?;

    // Get *pa back in the integer over integer form.
    renormalize(pa);
    Ok(())
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: modrat
//
//    DESCRIPTION: Calculate the remainder of *pa / b, with the sign of the result
//                 either zero or has the same sign as the divisor.
//    NOTE: When *pa or b are negative, the result won't be the same as
//          the C/C++ operator %, use remrat if it's the behavior you expect.
//
//-----------------------------------------------------------------------------

pub(crate) fn modrat(ctx: &Ctx, pa: &mut Rat, b: &Rat) -> CalcResult<()> {
    // contrary to remrat(X, 0) returning 0, modrat(X, 0) must return X
    if zerrat(b) {
        return Ok(());
    }

    let mut tmp = b.clone();

    let need_adjust = if sign(pa) == -1 {
        sign(b) == 1
    } else {
        sign(b) == -1
    };

    mulnumx(&mut pa.pp, &tmp.pq)?;
    mulnumx(&mut tmp.pp, &pa.pq)?;
    remnum(&mut pa.pp, &tmp.pp, BASEX)?;
    mulnumx(&mut pa.pq, &tmp.pq)?;

    if need_adjust && !zerrat(pa) {
        // C++ passes BASEX as the int32_t precision (i.e. INT_MIN).
        addrat_(ctx, pa, b, BASEX as i32)?;
    }

    // Get *pa back in the integer over integer form.
    renormalize(pa);
    Ok(())
}
