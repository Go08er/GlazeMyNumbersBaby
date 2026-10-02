// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//-----------------------------------------------------------------------------
//  Package Title  ratpak
//  File           rat.c
//  Copyright      (C) 1995-96 Microsoft
//  Date           01-16-95
//
//
//  Description
//
//  Contains mul, div, add, and other support functions for rationals.
//
//-----------------------------------------------------------------------------

use super::basex::{divnumx, mulnumx, mulnumx_self};
use super::conv::{flatrat, gcd};
use super::exp::powrat;
use super::num::{addnum, addnum_s, equnum, remnum, zernum};
use super::support::{rat_lt, trimit};
use super::{BASEX, CalcResult, Ctx, Rat, absrat, renormalize};
use crate::{CALC_E_DIVIDEBYZERO, CALC_E_INDEFINITE};

//-----------------------------------------------------------------------------
//
//    FUNCTION: gcdrat
//
//    DESCRIPTION: Divides p and q in rational by the G.C.D.
//    of both.  It was hoped this would speed up some
//    calculations, and until the above trimming was done it
//    did, but after trimming gcdratting, only slows things
//    down.
//
//-----------------------------------------------------------------------------

#[allow(dead_code)]
pub(crate) fn gcdrat(ctx: &Ctx, pa: &mut Rat, precision: i32) -> CalcResult<()> {
    let pgcd = gcd(&pa.pp, &pa.pq)?;

    if !zernum(&pgcd) {
        divnumx(ctx, &mut pa.pp, &pgcd, precision)?;
        divnumx(ctx, &mut pa.pq, &pgcd, precision)?;
    }

    renormalize(pa);
    Ok(())
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: fracrat
//
//    DESCRIPTION: Does the rational equivalent of frac(*pa);
//
//-----------------------------------------------------------------------------

pub(crate) fn fracrat(ctx: &Ctx, pa: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    // Only do the flatrat operation if number is nonzero.
    // and only if the bottom part is not one.
    if !zernum(&pa.pp) && !equnum(&pa.pq, &ctx.num_one) {
        flatrat(ctx, pa, radix, precision)?;
    }

    remnum(&mut pa.pp, &pa.pq, BASEX)?;

    // Get *pa back in the integer over integer form.
    renormalize(pa);
    Ok(())
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: mulrat
//
//    DESCRIPTION: Does the rational equivalent of *pa *= b.
//    Assumes radix is the radix of both numbers.
//
//-----------------------------------------------------------------------------

pub(crate) fn mulrat(ctx: &Ctx, pa: &mut Rat, b: &Rat, precision: i32) -> CalcResult<()> {
    // Only do the multiply if it isn't zero.
    if !zernum(&pa.pp) {
        mulnumx(&mut pa.pp, &b.pp)?;
        mulnumx(&mut pa.pq, &b.pq)?;
        trimit(ctx, pa, precision);
    } else {
        // If it is zero, blast a one in the denominator.
        pa.pq.clone_from(&ctx.num_one);
    }
    Ok(())
}

/// `mulrat(pa, *pa, precision)`: the aliasing (squaring) call.
pub(crate) fn mulrat_self(ctx: &Ctx, pa: &mut Rat, precision: i32) -> CalcResult<()> {
    if !zernum(&pa.pp) {
        mulnumx_self(&mut pa.pp)?;
        mulnumx_self(&mut pa.pq)?;
        trimit(ctx, pa, precision);
    } else {
        pa.pq.clone_from(&ctx.num_one);
    }
    Ok(())
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: divrat
//
//    DESCRIPTION: Does the rational equivalent of *pa /= b.
//    Assumes radix is the radix of both numbers.
//
//-----------------------------------------------------------------------------

pub(crate) fn divrat(ctx: &Ctx, pa: &mut Rat, b: &Rat, precision: i32) -> CalcResult<()> {
    if !zernum(&pa.pp) {
        // Only do the divide if the top isn't zero.
        mulnumx(&mut pa.pp, &b.pq)?;
        mulnumx(&mut pa.pq, &b.pp)?;

        if zernum(&pa.pq) {
            // raise an exception if the bottom is 0.
            return Err(CALC_E_DIVIDEBYZERO);
        }
        trimit(ctx, pa, precision);
    } else {
        // Top is zero.
        if zerrat(b) {
            // If bottom is zero
            // 0 / 0 is indefinite, raise an exception.
            return Err(CALC_E_INDEFINITE);
        } else {
            // 0/x make a unique 0.
            pa.pq.clone_from(&ctx.num_one);
        }
    }
    Ok(())
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: subrat, _subrat
//
//    DESCRIPTION: Does the rational equivalent of *pa -= b.
//    Assumes base is internal throughout.
//
//    subrat does snapping to zero after subtraction. All ratpak internal
//    should use _subrat by default.
//
//-----------------------------------------------------------------------------

pub(crate) fn subrat(ctx: &Ctx, pa: &mut Rat, b: &Rat, precision: i32) -> CalcResult<()> {
    let a = pa.clone();

    subrat_(ctx, pa, b, precision)?;

    snaprat(ctx, pa, &a, Some(b), precision)
}

/// `_subrat`
pub(crate) fn subrat_(ctx: &Ctx, pa: &mut Rat, b: &Rat, precision: i32) -> CalcResult<()> {
    // b->pp->sign *= -1; _addrat(pa, b, precision); b->pp->sign *= -1;
    addrat_signed(ctx, pa, b, -1, precision)
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: addrat, _addrat
//
//    DESCRIPTION: Does the rational equivalent of *pa += b.
//    Assumes base is internal throughout.
//
//    addrat does snapping to zero after addition. All ratpak internal should
//    use _addrat by default.
//
//-----------------------------------------------------------------------------

pub(crate) fn addrat(ctx: &Ctx, pa: &mut Rat, b: &Rat, precision: i32) -> CalcResult<()> {
    let a = pa.clone();

    addrat_(ctx, pa, b, precision)?;

    snaprat(ctx, pa, &a, Some(b), precision)
}

/// `_addrat`
pub(crate) fn addrat_(ctx: &Ctx, pa: &mut Rat, b: &Rat, precision: i32) -> CalcResult<()> {
    addrat_signed(ctx, pa, b, 1, precision)
}

/// `_addrat` where `b->pp->sign` is multiplied by `bmul` for the duration of
/// the call (the C++ callers flip `b->pp->sign` around `_addrat`). Every use
/// of the sign inside is multiplicative, so the flip is applied to the
/// products directly.
///
/// (The C++ equal-denominator branch also normalises `b`'s signs in place;
/// that never changes `b`'s value and no caller depends on its
/// representation afterwards.)
pub(crate) fn addrat_signed(
    ctx: &Ctx,
    pa: &mut Rat,
    b: &Rat,
    bmul: i32,
    precision: i32,
) -> CalcResult<()> {
    if equnum(&pa.pq, &b.pq) {
        // Very special case, q's match.,
        // make sure signs are involved in the calculation
        // we have to do this since the optimization here is only
        // working with the top half of the rationals.
        pa.pp.sign = pa.pp.sign.wrapping_mul(pa.pq.sign);
        pa.pq.sign = 1;
        let bsign = bmul.wrapping_mul(b.pp.sign).wrapping_mul(b.pq.sign);
        addnum_s(&mut pa.pp, &b.pp, bsign, BASEX)?;
    } else {
        // Usual case q's aren't the same.
        let mut bot = pa.pq.clone();
        mulnumx(&mut bot, &b.pq)?;
        mulnumx(&mut pa.pp, &b.pq)?;
        mulnumx(&mut pa.pq, &b.pp)?;
        pa.pq.sign = pa.pq.sign.wrapping_mul(bmul);
        addnum(&mut pa.pp, &pa.pq, BASEX)?;
        pa.pq = bot;
        trimit(ctx, pa, precision);

        // Get rid of negative zeros here.
        pa.pp.sign = pa.pp.sign.wrapping_mul(pa.pq.sign);
        pa.pq.sign = 1;
    }
    Ok(())
}

/// `_addrat(pa, *pa, precision)`: the aliasing call.
pub(crate) fn addrat_self(ctx: &Ctx, pa: &mut Rat, precision: i32) -> CalcResult<()> {
    let b = pa.clone();
    addrat_(ctx, pa, &b, precision)
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: rootrat
//
//  PARAMETERS: y prat representation of number to take the root of
//              n prat representation of the root to take.
//
//  RETURN: bth root of a in rat form.
//
//  EXPLANATION: This is now a stub function to powrat().
//
//-----------------------------------------------------------------------------

pub(crate) fn rootrat(
    ctx: &Ctx,
    py: &mut Rat,
    n: &Rat,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    // Initialize 1/n
    let mut oneovern = ctx.rat_one.clone();
    divrat(ctx, &mut oneovern, n, precision)?;

    powrat(ctx, py, &oneovern, radix, precision)
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: zerrat
//
//    DESCRIPTION: Returns true if input is zero.
//    False otherwise.
//
//-----------------------------------------------------------------------------

pub(crate) fn zerrat(a: &Rat) -> bool {
    zernum(&a.pp)
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: _snaprat
//
//    ARGUMENTS: r prat to potentially snap to zero
//               a, b prats for comparison.
//               b is optional and can be null for unary operations.
//
//    DESCRIPTION: If |pr| is magnitude smaller than |a| or |b| beyond
//    precision, snap pr to 0. This is to address issues with exposing tiny
//    residuals to the user in calculations that should yield zero.
//
//    Example: let rat a = sqrt(2.25), rat b = 1.5. r = a - b should be zero.
//    However, rat a is an approximation of sqrt(2.25) and very close to 1.5,
//    but not exactly 1.5. The result r is a tiny residual close to zero, but
//    not zero. _snaprat can be used to check if r is small enough compared to
//    a or b, and snap it to zero if so. Without this, users may see unexpected
//    tiny values in results that should be zero.
//
//    log(a) where a is very close to 1 is another example. The result should be
//    zero.
//
//    trimit also removes digits but it's for a different reason.
//
//    Notice that trigonometric functions sinrat/cosrat have specifically
//    adjusted for approximation errors.
//
//-----------------------------------------------------------------------------

pub(crate) fn snaprat(
    ctx: &Ctx,
    pr: &mut Rat,
    a: &Rat,
    b: Option<&Rat>,
    precision: i32,
) -> CalcResult<()> {
    let mut threshold = match b {
        None => {
            let mut t = a.clone();
            absrat(&mut t);
            t
        }
        Some(b) => {
            let mut abs_a = a.clone();
            let mut abs_b = b.clone();
            absrat(&mut abs_a);
            absrat(&mut abs_b);

            if rat_lt(ctx, &abs_a, &abs_b, precision)? {
                abs_b
            } else {
                abs_a
            }
        }
    };
    mulrat(ctx, &mut threshold, &ctx.rat_smallest, precision)?;

    let mut abs_r = pr.clone();
    absrat(&mut abs_r);

    // if absResult < threshold => snap to zero
    if rat_lt(ctx, &abs_r, &threshold, precision)? {
        *pr = ctx.rat_zero.clone();
    }
    Ok(())
}
