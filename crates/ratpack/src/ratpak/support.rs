// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//----------------------------------------------------------------------------
//  Package Title  ratpak
//  File           support.c
//  Copyright      (C) 1995-96 Microsoft
//  Date           10-21-96
//
//
//  Description
//
//     Contains support functions for rationals and numbers.
//
//----------------------------------------------------------------------------

use std::fmt::Write as _;

use super::conv::{flatrat, i32torat, numpowi32, ratpowi32};
use super::exp::{exprat_, lograt_};
use super::itrans::asinrat;
use super::logic::remrat;
use super::num::{equnum, zernum};
use super::rat::{addrat_, addrat_signed, divrat, mulrat, subrat_};
use super::ratconst::{self, RawNumber};
use super::{BASEX, BASEXPWR, CalcResult, Ctx, Number, Rat, sign};

const RATIO_FOR_DECIMAL: i32 = 9;
const DECIMAL: i32 = 10;
const CALC_DECIMAL_DIGITS_DEFAULT: i32 = 32;

/// The radix / precision the calculator engine sets up first
/// (`CCalcEngine::InitialOneTimeOnlySetup` -> `ChangeBaseConstants(10, 32, 32)`).
pub(crate) const INITIAL_RADIX: u32 = 10;
pub(crate) const INITIAL_PRECISION: i32 = 32;

fn rawnum(r: &RawNumber) -> Number {
    debug_assert_eq!(r.cdigit as usize, r.mant.len());
    Number {
        sign: r.sign,
        exp: r.exp,
        mant: r.mant.to_vec(),
    }
}

fn rawrat(p: &RawNumber, q: &RawNumber) -> Rat {
    Rat {
        pp: rawnum(p),
        pq: rawnum(q),
    }
}

impl Ctx {
    /// The state of the ratpak globals once the calculator engine has made
    /// its first `ChangeConstants(10, 32)` call.
    ///
    /// The C++ globals start out null (and `g_ratio` 0), so using ratpak
    /// before `ChangeConstants` crashes; and a first `ChangeConstants` that
    /// takes the "compute" path never initialises `rat_byte`. Starting from
    /// the state the app always establishes first sidesteps both.
    pub(crate) fn initial() -> Ctx {
        use ratconst::*;
        let mut ctx = Ctx {
            g_ratio: 0,
            g_ftrueinfinite: false,
            decimal_separator: '.',
            cbitsofprecision: RATIO_FOR_DECIMAL * DECIMAL * CALC_DECIMAL_DIGITS_DEFAULT,
            num_one: rawnum(&INIT_NUM_ONE),
            num_two: rawnum(&INIT_NUM_TWO),
            num_five: rawnum(&INIT_NUM_FIVE),
            num_six: rawnum(&INIT_NUM_SIX),
            num_ten: rawnum(&INIT_NUM_TEN),
            ln_ten: rawrat(&INIT_P_LN_TEN, &INIT_Q_LN_TEN),
            ln_two: rawrat(&INIT_P_LN_TWO, &INIT_Q_LN_TWO),
            rat_zero: rawrat(&INIT_P_RAT_ZERO, &INIT_Q_RAT_ZERO),
            rat_neg_one: rawrat(&INIT_P_RAT_NEG_ONE, &INIT_Q_RAT_NEG_ONE),
            rat_one: rawrat(&INIT_P_RAT_ONE, &INIT_Q_RAT_ONE),
            rat_two: rawrat(&INIT_P_RAT_TWO, &INIT_Q_RAT_TWO),
            rat_six: rawrat(&INIT_P_RAT_SIX, &INIT_Q_RAT_SIX),
            rat_half: rawrat(&INIT_P_RAT_HALF, &INIT_Q_RAT_HALF),
            rat_ten: rawrat(&INIT_P_RAT_TEN, &INIT_Q_RAT_TEN),
            pt_eight_five: rawrat(&INIT_P_PT_EIGHT_FIVE, &INIT_Q_PT_EIGHT_FIVE),
            pi: rawrat(&INIT_P_PI, &INIT_Q_PI),
            pi_over_two: rawrat(&INIT_P_PI_OVER_TWO, &INIT_Q_PI_OVER_TWO),
            two_pi: rawrat(&INIT_P_TWO_PI, &INIT_Q_TWO_PI),
            one_pt_five_pi: rawrat(&INIT_P_ONE_PT_FIVE_PI, &INIT_Q_ONE_PT_FIVE_PI),
            e_to_one_half: rawrat(&INIT_P_E_TO_ONE_HALF, &INIT_Q_E_TO_ONE_HALF),
            rat_exp: rawrat(&INIT_P_RAT_EXP, &INIT_Q_RAT_EXP),
            rad_to_deg: rawrat(&INIT_P_RAD_TO_DEG, &INIT_Q_RAD_TO_DEG),
            rad_to_grad: rawrat(&INIT_P_RAD_TO_GRAD, &INIT_Q_RAD_TO_GRAD),
            rat_qword: rawrat(&INIT_P_RAT_QWORD, &INIT_Q_RAT_QWORD),
            rat_dword: rawrat(&INIT_P_RAT_DWORD, &INIT_Q_RAT_DWORD),
            rat_word: rawrat(&INIT_P_RAT_WORD, &INIT_Q_RAT_WORD),
            rat_byte: rawrat(&INIT_P_RAT_BYTE, &INIT_Q_RAT_BYTE),
            rat_360: rawrat(&INIT_P_RAT_360, &INIT_Q_RAT_360),
            rat_400: rawrat(&INIT_P_RAT_400, &INIT_Q_RAT_400),
            rat_180: rawrat(&INIT_P_RAT_180, &INIT_Q_RAT_180),
            rat_200: rawrat(&INIT_P_RAT_200, &INIT_Q_RAT_200),
            rat_nradix: i32torat(INITIAL_RADIX as i32),
            rat_smallest: rawrat(&INIT_P_RAT_SMALLEST, &INIT_Q_RAT_SMALLEST),
            rat_negsmallest: rawrat(&INIT_P_RAT_NEGSMALLEST, &INIT_Q_RAT_NEGSMALLEST),
            rat_max_exp: rawrat(&INIT_P_RAT_MAX_EXP, &INIT_Q_RAT_MAX_EXP),
            rat_min_exp: rawrat(&INIT_P_RAT_MIN_EXP, &INIT_Q_RAT_MIN_EXP),
            rat_max_fact: rawrat(&INIT_P_RAT_MAX_FACT, &INIT_Q_RAT_MAX_FACT),
            rat_min_fact: rawrat(&INIT_P_RAT_MIN_FACT, &INIT_Q_RAT_MIN_FACT),
            rat_max_i32: rawrat(&INIT_P_RAT_MAX_I32, &INIT_Q_RAT_MAX_I32),
            rat_min_i32: rawrat(&INIT_P_RAT_MIN_I32, &INIT_Q_RAT_MIN_I32),
        };
        change_constants(&mut ctx, INITIAL_RADIX, INITIAL_PRECISION)
            .expect("ChangeConstants(10, 32) cannot fail");
        ctx
    }
}

//----------------------------------------------------------------------------
//
//  FUNCTION: ChangeConstants
//
//  ARGUMENTS:  base changing to, and precision to use.
//
//  RETURN: None
//
//  SIDE EFFECTS: sets a mess of constants.
//
//----------------------------------------------------------------------------

pub(crate) fn change_constants(ctx: &mut Ctx, radix: u32, precision: i32) -> CalcResult<()> {
    // ratio is set to the number of digits in the current radix, you can get
    // in the internal BASEX radix, this is important for length calculations
    // in translating from radix to BASEX and back.

    ctx.g_ratio = (BASEXPWR as f64 / (radix as f64).log2()).ceil() as i32 - 1;

    ctx.rat_nradix = i32torat(radix as i32);

    // Check to see what we have to recalculate and what we don't
    if ctx.cbitsofprecision
        < ctx
            .g_ratio
            .wrapping_mul(radix as i32)
            .wrapping_mul(precision)
    {
        ctx.g_ftrueinfinite = false;

        // INIT_AND_DUMP_RAW_NUM_IF_NULL / INIT_AND_DUMP_RAW_RAT_IF_NULL for
        // num_one .. rat_min_fact: these globals are never null here because
        // the context is always initialised from ratconst (see Ctx::initial),
        // so those initialisations are no-ops.

        let mut smallest = ctx.rat_nradix.clone();
        ratpowi32(ctx, &mut smallest, precision.wrapping_neg(), precision)?;
        ctx.rat_smallest = smallest;
        ctx.rat_negsmallest = ctx.rat_smallest.clone();
        ctx.rat_negsmallest.pp.sign = -1;

        // rat_half and pt_eight_five: likewise never null.

        let mut qword = ctx.rat_two.clone();
        numpowi32(ctx, &mut qword.pp, 64, BASEX, precision)?;
        subrat_(ctx, &mut qword, &ctx.rat_one, precision)?;
        ctx.rat_qword = qword;

        let mut dword = ctx.rat_two.clone();
        numpowi32(ctx, &mut dword.pp, 32, BASEX, precision)?;
        subrat_(ctx, &mut dword, &ctx.rat_one, precision)?;
        ctx.rat_dword = dword;

        let mut max_i32 = ctx.rat_two.clone();
        numpowi32(ctx, &mut max_i32.pp, 31, BASEX, precision)?;
        let mut min_i32 = max_i32.clone();
        subrat_(ctx, &mut max_i32, &ctx.rat_one, precision)?; // rat_max_i32 = 2^31 -1
        ctx.rat_max_i32 = max_i32;

        min_i32.pp.sign = min_i32.pp.sign.wrapping_mul(-1); // rat_min_i32 = -2^31
        ctx.rat_min_i32 = min_i32;

        let mut min_exp = ctx.rat_max_exp.clone();
        min_exp.pp.sign = min_exp.pp.sign.wrapping_mul(-1);
        ctx.rat_min_exp = min_exp;

        ctx.cbitsofprecision = (ctx.g_ratio as u32)
            .wrapping_mul(radix)
            .wrapping_mul(precision as u32) as i32;

        // Apparently when dividing 180 by pi, another (internal) digit of
        // precision is needed.
        let extra_precision = precision.wrapping_add(ctx.g_ratio);
        let mut pi = ctx.rat_half.clone();
        asinrat(ctx, &mut pi, radix, extra_precision)?;
        mulrat(ctx, &mut pi, &ctx.rat_six, extra_precision)?;
        ctx.pi = pi;

        let mut two_pi = ctx.pi.clone();
        let mut pi_over_two = ctx.pi.clone();
        let mut one_pt_five_pi = ctx.pi.clone();
        addrat_(ctx, &mut two_pi, &ctx.pi, extra_precision)?;
        ctx.two_pi = two_pi;

        divrat(ctx, &mut pi_over_two, &ctx.rat_two, extra_precision)?;
        ctx.pi_over_two = pi_over_two;

        addrat_(ctx, &mut one_pt_five_pi, &ctx.pi_over_two, extra_precision)?;
        ctx.one_pt_five_pi = one_pt_five_pi;

        let mut e_to_one_half = ctx.rat_half.clone();
        exprat_(ctx, &mut e_to_one_half, extra_precision)?;
        ctx.e_to_one_half = e_to_one_half;

        let mut rat_exp = ctx.rat_one.clone();
        exprat_(ctx, &mut rat_exp, extra_precision)?;
        ctx.rat_exp = rat_exp;

        // WARNING: remember _lograt uses exponent constants calculated above...

        let mut ln_ten = ctx.rat_ten.clone();
        lograt_(ctx, &mut ln_ten, extra_precision)?;
        ctx.ln_ten = ln_ten;

        let mut ln_two = ctx.rat_two.clone();
        lograt_(ctx, &mut ln_two, extra_precision)?;
        ctx.ln_two = ln_two;

        let mut rad_to_deg = i32torat(180);
        divrat(ctx, &mut rad_to_deg, &ctx.pi, extra_precision)?;
        ctx.rad_to_deg = rad_to_deg;

        let mut rad_to_grad = i32torat(200);
        divrat(ctx, &mut rad_to_grad, &ctx.pi, extra_precision)?;
        ctx.rad_to_grad = rad_to_grad;
    } else {
        readconstants(ctx);

        let mut smallest = ctx.rat_nradix.clone();
        ratpowi32(ctx, &mut smallest, precision.wrapping_neg(), precision)?;
        ctx.rat_smallest = smallest;
        ctx.rat_negsmallest = ctx.rat_smallest.clone();
        ctx.rat_negsmallest.pp.sign = -1;
    }
    Ok(())
}

//----------------------------------------------------------------------------
//
//  FUNCTION: intrat
//
//  ARGUMENTS:  pointer to x PRAT representation of number
//
//  RETURN: no return value x PRAT is smashed with integral number
//
//----------------------------------------------------------------------------

pub(crate) fn intrat(ctx: &Ctx, px: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    // Only do the intrat operation if number is nonzero.
    // and only if the bottom part is not one.
    if !zernum(&px.pp) && !equnum(&px.pq, &ctx.num_one) {
        flatrat(ctx, px, radix, precision)?;

        // Subtract the fractional part of the rational
        let mut pret = px.clone();
        remrat(&mut pret, &ctx.rat_one)?;

        // Flatten pret in case it's not aligned with px after remrat operation
        if !equnum(&px.pq, &pret.pq) {
            flatrat(ctx, &mut pret, radix, precision)?;
        }

        subrat_(ctx, px, &pret, precision)?;

        // Simplify the value if possible to resolve rounding errors
        flatrat(ctx, px, radix, precision)?;
    }
    Ok(())
}

//---------------------------------------------------------------------------
//
//  FUNCTION: rat_equ
//
//  RETURN: true if equal false otherwise.
//
//---------------------------------------------------------------------------

pub(crate) fn rat_equ(ctx: &Ctx, a: &Rat, b: &Rat, precision: i32) -> CalcResult<bool> {
    let mut rattmp = a.clone();
    rattmp.pp.sign = rattmp.pp.sign.wrapping_mul(-1);
    addrat_(ctx, &mut rattmp, b, precision)?;
    Ok(zernum(&rattmp.pp))
}

//---------------------------------------------------------------------------
//
//  FUNCTION: rat_ge
//
//  RETURN: true if a is greater than or equal to b
//
//---------------------------------------------------------------------------

pub(crate) fn rat_ge(ctx: &Ctx, a: &Rat, b: &Rat, precision: i32) -> CalcResult<bool> {
    let mut rattmp = a.clone();
    addrat_signed(ctx, &mut rattmp, b, -1, precision)?;
    Ok(zernum(&rattmp.pp) || sign(&rattmp) == 1)
}

//---------------------------------------------------------------------------
//
//  FUNCTION: rat_gt
//
//  RETURN: true if a is greater than b
//
//---------------------------------------------------------------------------

pub(crate) fn rat_gt(ctx: &Ctx, a: &Rat, b: &Rat, precision: i32) -> CalcResult<bool> {
    let mut rattmp = a.clone();
    addrat_signed(ctx, &mut rattmp, b, -1, precision)?;
    Ok(!zernum(&rattmp.pp) && sign(&rattmp) == 1)
}

//---------------------------------------------------------------------------
//
//  FUNCTION: rat_le
//
//  RETURN: true if a is less than or equal to b
//
//---------------------------------------------------------------------------

pub(crate) fn rat_le(ctx: &Ctx, a: &Rat, b: &Rat, precision: i32) -> CalcResult<bool> {
    let mut rattmp = a.clone();
    addrat_signed(ctx, &mut rattmp, b, -1, precision)?;
    Ok(zernum(&rattmp.pp) || sign(&rattmp) == -1)
}

//---------------------------------------------------------------------------
//
//  FUNCTION: rat_lt
//
//  RETURN: true if a is less than b
//
//---------------------------------------------------------------------------

pub(crate) fn rat_lt(ctx: &Ctx, a: &Rat, b: &Rat, precision: i32) -> CalcResult<bool> {
    let mut rattmp = a.clone();
    addrat_signed(ctx, &mut rattmp, b, -1, precision)?;
    Ok(!zernum(&rattmp.pp) && sign(&rattmp) == -1)
}

//---------------------------------------------------------------------------
//
//  FUNCTION: rat_neq
//
//  RETURN: true if a is not equal to b
//
//---------------------------------------------------------------------------

pub(crate) fn rat_neq(ctx: &Ctx, a: &Rat, b: &Rat, precision: i32) -> CalcResult<bool> {
    let mut rattmp = a.clone();
    rattmp.pp.sign = rattmp.pp.sign.wrapping_mul(-1);
    addrat_(ctx, &mut rattmp, b, precision)?;
    Ok(!zernum(&rattmp.pp))
}

//---------------------------------------------------------------------------
//
//  function: scale
//
//  RETURN: no return, value x PRAT is smashed with a scaled number in the
//          range of the scalefact.
//
//---------------------------------------------------------------------------

pub(crate) fn scale(
    ctx: &Ctx,
    px: &mut Rat,
    scalefact: &Rat,
    radix: u32,
    mut precision: i32,
) -> CalcResult<()> {
    let mut pret = px.clone();

    // Logscale is a quick way to tell how much extra precision is needed for
    // scaling by scalefact.
    let logscale = ctx.g_ratio.wrapping_mul(
        pret.pp
            .cdigit()
            .wrapping_add(pret.pp.exp)
            .wrapping_sub(pret.pq.cdigit().wrapping_add(pret.pq.exp)),
    );
    if logscale > 0 {
        precision = precision.wrapping_add(logscale);
    }

    divrat(ctx, &mut pret, scalefact, precision)?;
    intrat(ctx, &mut pret, radix, precision)?;
    mulrat(ctx, &mut pret, scalefact, precision)?;
    pret.pp.sign = pret.pp.sign.wrapping_mul(-1);
    addrat_(ctx, px, &pret, precision)
}

//---------------------------------------------------------------------------
//
//  function: scale2pi
//
//  RETURN: no return, value x PRAT is smashed with a scaled number in the
//          range of 0..2pi
//
//---------------------------------------------------------------------------

pub(crate) fn scale2pi(ctx: &Ctx, px: &mut Rat, radix: u32, mut precision: i32) -> CalcResult<()> {
    let mut pret = px.clone();

    // Logscale is a quick way to tell how much extra precision is needed for
    // scaling by 2 pi.
    let logscale = ctx.g_ratio.wrapping_mul(
        pret.pp
            .cdigit()
            .wrapping_add(pret.pp.exp)
            .wrapping_sub(pret.pq.cdigit().wrapping_add(pret.pq.exp)),
    );
    let my_two_pi = if logscale > 0 {
        precision = precision.wrapping_add(logscale);
        let mut t = ctx.rat_half.clone();
        asinrat(ctx, &mut t, radix, precision)?;
        mulrat(ctx, &mut t, &ctx.rat_six, precision)?;
        mulrat(ctx, &mut t, &ctx.rat_two, precision)?;
        t
    } else {
        ctx.two_pi.clone()
    };

    divrat(ctx, &mut pret, &my_two_pi, precision)?;
    intrat(ctx, &mut pret, radix, precision)?;
    mulrat(ctx, &mut pret, &my_two_pi, precision)?;
    pret.pp.sign = pret.pp.sign.wrapping_mul(-1);
    addrat_(ctx, px, &pret, precision)
}

//---------------------------------------------------------------------------
//
//  FUNCTION: inbetween
//
//  ARGUMENTS:  PRAT *px, and PRAT range.
//
//  RETURN: none, changes *px to -/+range, if px is outside -range..+range
//
//---------------------------------------------------------------------------

pub(crate) fn inbetween(ctx: &Ctx, px: &mut Rat, range: &Rat, precision: i32) -> CalcResult<()> {
    if rat_gt(ctx, px, range, precision)? {
        *px = range.clone();
    } else {
        let mut neg_range = range.clone();
        neg_range.pp.sign = neg_range.pp.sign.wrapping_mul(-1);
        if rat_lt(ctx, px, &neg_range, precision)? {
            *px = neg_range;
        }
    }
    Ok(())
}

//---------------------------------------------------------------------------
//
//  FUNCTION: _dumprawrat
//
//  RETURN: none, prints the results of a dump of the internal structures
//          of a PRAT, suitable for READRAWRAT to stderr.
//
//---------------------------------------------------------------------------

#[allow(dead_code)]
pub(crate) fn dumprawrat(varname: &str, rat: &Rat, out: &mut String) {
    dumprawnum(varname, &rat.pp, out);
    dumprawnum(varname, &rat.pq, out);
}

//---------------------------------------------------------------------------
//
//  FUNCTION: _dumprawnum
//
//  RETURN: none, prints the results of a dump of the internal structures
//          of a PNUMBER, suitable for READRAWNUM to stderr.
//
//---------------------------------------------------------------------------

#[allow(dead_code)]
pub(crate) fn dumprawnum(varname: &str, num: &Number, out: &mut String) {
    let _ = writeln!(out, "NUMBER {varname} = {{");
    let _ = writeln!(out, "\t{},", num.sign);
    let _ = writeln!(out, "\t{},", num.cdigit());
    let _ = writeln!(out, "\t{},", num.exp);
    out.push_str("\t{ ");
    for d in &num.mant {
        let _ = write!(out, " {d},");
    }
    out.push_str("}\n");
    out.push_str("};\n");
}

fn readconstants(ctx: &mut Ctx) {
    use ratconst::*;
    ctx.num_one = rawnum(&INIT_NUM_ONE);
    ctx.num_two = rawnum(&INIT_NUM_TWO);
    ctx.num_five = rawnum(&INIT_NUM_FIVE);
    ctx.num_six = rawnum(&INIT_NUM_SIX);
    ctx.num_ten = rawnum(&INIT_NUM_TEN);
    ctx.pt_eight_five = rawrat(&INIT_P_PT_EIGHT_FIVE, &INIT_Q_PT_EIGHT_FIVE);
    ctx.rat_six = rawrat(&INIT_P_RAT_SIX, &INIT_Q_RAT_SIX);
    ctx.rat_two = rawrat(&INIT_P_RAT_TWO, &INIT_Q_RAT_TWO);
    ctx.rat_zero = rawrat(&INIT_P_RAT_ZERO, &INIT_Q_RAT_ZERO);
    ctx.rat_one = rawrat(&INIT_P_RAT_ONE, &INIT_Q_RAT_ONE);
    ctx.rat_neg_one = rawrat(&INIT_P_RAT_NEG_ONE, &INIT_Q_RAT_NEG_ONE);
    ctx.rat_half = rawrat(&INIT_P_RAT_HALF, &INIT_Q_RAT_HALF);
    ctx.rat_ten = rawrat(&INIT_P_RAT_TEN, &INIT_Q_RAT_TEN);
    ctx.pi = rawrat(&INIT_P_PI, &INIT_Q_PI);
    ctx.two_pi = rawrat(&INIT_P_TWO_PI, &INIT_Q_TWO_PI);
    ctx.pi_over_two = rawrat(&INIT_P_PI_OVER_TWO, &INIT_Q_PI_OVER_TWO);
    ctx.one_pt_five_pi = rawrat(&INIT_P_ONE_PT_FIVE_PI, &INIT_Q_ONE_PT_FIVE_PI);
    ctx.e_to_one_half = rawrat(&INIT_P_E_TO_ONE_HALF, &INIT_Q_E_TO_ONE_HALF);
    ctx.rat_exp = rawrat(&INIT_P_RAT_EXP, &INIT_Q_RAT_EXP);
    ctx.ln_ten = rawrat(&INIT_P_LN_TEN, &INIT_Q_LN_TEN);
    ctx.ln_two = rawrat(&INIT_P_LN_TWO, &INIT_Q_LN_TWO);
    ctx.rad_to_deg = rawrat(&INIT_P_RAD_TO_DEG, &INIT_Q_RAD_TO_DEG);
    ctx.rad_to_grad = rawrat(&INIT_P_RAD_TO_GRAD, &INIT_Q_RAD_TO_GRAD);
    ctx.rat_qword = rawrat(&INIT_P_RAT_QWORD, &INIT_Q_RAT_QWORD);
    ctx.rat_dword = rawrat(&INIT_P_RAT_DWORD, &INIT_Q_RAT_DWORD);
    ctx.rat_word = rawrat(&INIT_P_RAT_WORD, &INIT_Q_RAT_WORD);
    ctx.rat_byte = rawrat(&INIT_P_RAT_BYTE, &INIT_Q_RAT_BYTE);
    ctx.rat_360 = rawrat(&INIT_P_RAT_360, &INIT_Q_RAT_360);
    ctx.rat_400 = rawrat(&INIT_P_RAT_400, &INIT_Q_RAT_400);
    ctx.rat_180 = rawrat(&INIT_P_RAT_180, &INIT_Q_RAT_180);
    ctx.rat_200 = rawrat(&INIT_P_RAT_200, &INIT_Q_RAT_200);
    ctx.rat_smallest = rawrat(&INIT_P_RAT_SMALLEST, &INIT_Q_RAT_SMALLEST);
    ctx.rat_negsmallest = rawrat(&INIT_P_RAT_NEGSMALLEST, &INIT_Q_RAT_NEGSMALLEST);
    ctx.rat_max_exp = rawrat(&INIT_P_RAT_MAX_EXP, &INIT_Q_RAT_MAX_EXP);
    ctx.rat_min_exp = rawrat(&INIT_P_RAT_MIN_EXP, &INIT_Q_RAT_MIN_EXP);
    ctx.rat_max_fact = rawrat(&INIT_P_RAT_MAX_FACT, &INIT_Q_RAT_MAX_FACT);
    ctx.rat_min_fact = rawrat(&INIT_P_RAT_MIN_FACT, &INIT_Q_RAT_MIN_FACT);
    ctx.rat_min_i32 = rawrat(&INIT_P_RAT_MIN_I32, &INIT_Q_RAT_MIN_I32);
    ctx.rat_max_i32 = rawrat(&INIT_P_RAT_MAX_I32, &INIT_Q_RAT_MAX_I32);
}

//---------------------------------------------------------------------------
//
//  FUNCTION: trimit
//
//  ARGUMENTS:  PRAT *px, int32_t precision
//
//
//  DESCRIPTION: Chops off digits from rational numbers to avoid time
//  explosions in calculations of functions using series.
//  It can be shown that it is enough to only keep the first n digits
//  of the largest of p or q in the rational p over q form, and of course
//  scale the smaller by the same number of digits.  This will give you
//  n-1 digits of accuracy.  This dramatically speeds up calculations
//  involving hundreds of digits or more.
//  The last part of this trim dealing with exponents never affects accuracy
//
//  RETURN: none, modifies the pointed to PRAT
//
//---------------------------------------------------------------------------

pub(crate) fn trimit(ctx: &Ctx, px: &mut Rat, precision: i32) {
    if !ctx.g_ftrueinfinite {
        let pp = &mut px.pp;
        let pq = &mut px.pq;
        let mut trim = ctx
            .g_ratio
            .wrapping_mul(
                pp.cdigit()
                    .wrapping_add(pp.exp)
                    .min(pq.cdigit().wrapping_add(pq.exp))
                    .wrapping_sub(1),
            )
            .wrapping_sub(precision);
        if trim > ctx.g_ratio {
            trim /= ctx.g_ratio;

            if trim <= pp.exp {
                pp.exp -= trim;
            } else {
                super::drop_low_digits(pp, trim.wrapping_sub(pp.exp) as usize);
                pp.exp = 0;
            }

            if trim <= pq.exp {
                pq.exp -= trim;
            } else {
                super::drop_low_digits(pq, trim.wrapping_sub(pq.exp) as usize);
                pq.exp = 0;
            }
        }
        trim = pp.exp.min(pq.exp);
        pp.exp = pp.exp.wrapping_sub(trim);
        pq.exp = pq.exp.wrapping_sub(trim);
    }
}

#[cfg(test)]
mod tests {
    use super::super::conv::i32tonum;
    use super::*;

    #[test]
    fn ratconst_tables_are_complete() {
        assert_eq!(ratconst::ALL.len(), ratconst::RATCONST_ARRAY_COUNT);
        assert_eq!(ratconst::RATCONST_ARRAY_COUNT, 73);
        let digits: usize = ratconst::ALL.iter().map(|(_, r)| r.mant.len()).sum();
        assert_eq!(digits, ratconst::RATCONST_DIGIT_COUNT);
        for (name, r) in ratconst::ALL.iter() {
            assert_eq!(r.cdigit as usize, r.mant.len(), "{name}");
            assert!(r.mant.iter().all(|&d| d < BASEX), "{name}");
        }
    }

    #[test]
    fn i32_constants_match_table() {
        // The IF_NULL initialisers in ChangeConstants would produce exactly
        // the table values, so treating them as never-null is lossless.
        let ctx = Ctx::initial();
        let same = |a: &Rat, v: i32| {
            let b = i32torat(v);
            a.pp == b.pp && a.pq == b.pq
        };
        assert!(same(&ctx.rat_six, 6));
        assert!(same(&ctx.rat_two, 2));
        assert!(same(&ctx.rat_zero, 0));
        assert!(same(&ctx.rat_one, 1));
        assert!(same(&ctx.rat_neg_one, -1));
        assert!(same(&ctx.rat_ten, 10));
        assert!(same(&ctx.rat_word, 0xffff));
        assert!(same(&ctx.rat_byte, 0xff));
        assert!(same(&ctx.rat_400, 400));
        assert!(same(&ctx.rat_360, 360));
        assert!(same(&ctx.rat_200, 200));
        assert!(same(&ctx.rat_180, 180));
        assert!(same(&ctx.rat_max_exp, 100000));
        assert!(same(&ctx.rat_max_fact, 3249));
        assert!(same(&ctx.rat_min_fact, -1000));
        assert_eq!(ctx.num_one, i32tonum(1, BASEX));
        assert_eq!(ctx.num_two, i32tonum(2, BASEX));
        assert_eq!(ctx.num_five, i32tonum(5, BASEX));
        assert_eq!(ctx.num_six, i32tonum(6, BASEX));
        assert_eq!(ctx.num_ten, i32tonum(10, BASEX));
        assert_eq!(ctx.rat_half.pp, ctx.num_one);
        assert_eq!(ctx.rat_half.pq, ctx.num_two);
        assert_eq!(ctx.pt_eight_five.pp, i32tonum(85, BASEX));
        assert_eq!(ctx.pt_eight_five.pq, i32tonum(100, BASEX));
    }
}
