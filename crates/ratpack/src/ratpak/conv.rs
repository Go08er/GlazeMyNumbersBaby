// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//---------------------------------------------------------------------------
//  Package Title  ratpak
//  File           conv.c
//  Copyright      (C) 1995-97 Microsoft
//  Date           01-16-95
//
//
//  Description
//
//     Contains conversion, input and output routines for numbers rationals
//  and i32s.
//
//---------------------------------------------------------------------------

use super::basex::{divnumx, mulnumx, numpowi32x};
use super::logic::{andrat, rshrat};
use super::num::{addnum, addnum_self, divnum, lessnum, mulnum, mulnum_self, remnum, zernum};
use super::rat::{divrat, mulrat, mulrat_self};
use super::support::{intrat, rat_gt, rat_lt, trimit};
use super::{BASEX, CalcResult, Ctx, MAX_LONG_SIZE, Number, Rat, trimnum};
use crate::{CALC_E_DOMAIN, CALC_E_INVALIDRANGE, CALC_E_OUTOFMEMORY, NumberFormat};

const MAX_ZEROS_AFTER_DECIMAL: i32 = 2;

/// digits 0..64 used by bases 2 .. 64
pub(crate) const DIGITS: &[u8; 64] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_@";

fn digits_find(c: char) -> Option<usize> {
    if c.is_ascii() {
        DIGITS.iter().position(|&d| d as char == c)
    } else {
        None
    }
}

/// `sizeof(NUMBER)`: sign, cdigit and exp.
const SIZEOF_NUMBER: u32 = 12;

//-----------------------------------------------------------------------------
//
//    FUNCTION: _createnum
//
//    ARGUMENTS: size of number in 'digits'
//
//    RETURN: a zeroed mantissa buffer of size + 1 digits (the C++ allocation).
//
//    DESCRIPTION: allocates and zeros out number type. Throws
//    CALC_E_INVALIDRANGE when the allocation size overflows 32 bits and
//    CALC_E_OUTOFMEMORY when the allocation fails.
//
//-----------------------------------------------------------------------------

pub(crate) fn createnum(size: u32) -> CalcResult<Vec<u32>> {
    // sizeof( MANTTYPE ) is the size of a 'digit'
    size.checked_add(1)
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(SIZEOF_NUMBER))
        .ok_or(CALC_E_INVALIDRANGE)?;
    let n = size as usize + 1;
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| CALC_E_OUTOFMEMORY)?;
    v.resize(n, 0);
    Ok(v)
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: numtorat
//
//    ARGUMENTS: pointer to a number, radix number is in.
//
//    RETURN: Rational representation of number.
//
//    DESCRIPTION: The rational representation of the number
//    is guaranteed to be in the form p (number with internal
//    base   representation) over q (number with internal base
//    representation)  Where p and q are integers.
//
//-----------------------------------------------------------------------------

pub(crate) fn numtorat(pin: &Number, radix: u32) -> CalcResult<Rat> {
    let mut pn_radixn = pin.clone();
    let mut qn_radixn = i32tonum(1, radix);

    // Ensure p and q start out as integers.
    if pn_radixn.exp < 0 {
        qn_radixn.exp = qn_radixn.exp.wrapping_sub(pn_radixn.exp);
        pn_radixn.exp = 0;
    }

    // There is probably a better way to do this.
    Ok(Rat {
        pp: numtonradixx(&pn_radixn, radix)?,
        pq: numtonradixx(&qn_radixn, radix)?,
    })
}

//----------------------------------------------------------------------------
//
//    FUNCTION: nRadixxtonum
//
//    ARGUMENTS: pointer to a number, base requested.
//
//    RETURN: number representation in radix requested.
//
//    DESCRIPTION: Does a base conversion on a number from
//    internal to requested base. Assumes number being passed
//    in is really in internal base form.
//
//----------------------------------------------------------------------------

pub(crate) fn nradixxtonum(
    ctx: &Ctx,
    a: &Number,
    radix: u32,
    precision: i32,
) -> CalcResult<Number> {
    let mut sum = i32tonum(0, radix);
    // C++: i32tonum(BASEX, radix), BASEX converted to int32_t (INT_MIN).
    let mut powof_nradix = i32tonum(BASEX as i32, radix);

    // A large penalty is paid for conversion of digits no one will see anyway.
    // limit the digits to the minimum of the existing precision or the
    // requested precision.
    let mut cdigits: u32 = precision.wrapping_add(1) as u32;
    if cdigits > a.cdigit() as u32 {
        cdigits = a.cdigit() as u32;
    }

    // scale by the internal base to the internal exponent offset of the LSD
    numpowi32(
        ctx,
        &mut powof_nradix,
        a.exp.wrapping_add(a.cdigit().wrapping_sub(cdigits as i32)),
        radix,
        precision,
    )?;

    // Loop over all the relative digits from MSD to LSD
    let top = a.mant.len();
    for k in 0..cdigits as usize {
        let digit = a.mant[top - 1 - k];
        // Loop over all the bits from MSB to LSB
        let mut bitmask = BASEX / 2;
        while bitmask > 0 {
            addnum_self(&mut sum, radix)?;
            if digit & bitmask != 0 {
                sum.mant[0] |= 1;
            }
            bitmask /= 2;
        }
    }

    // Scale answer by power of internal exponent.
    mulnum(&mut sum, &powof_nradix, radix)?;

    sum.sign = a.sign;
    Ok(sum)
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: numtonRadixx
//
//    ARGUMENTS: pointer to a number, radix of that number.
//
//    RETURN: number representation in internal radix.
//
//    DESCRIPTION: Does a radix conversion on a number from
//    specified radix to requested radix.  Assumes the radix
//    specified is the radix of the number passed in.
//
//-----------------------------------------------------------------------------

pub(crate) fn numtonradixx(a: &Number, radix: u32) -> CalcResult<Number> {
    let mut pnumret = i32tonum(0, BASEX); // pnumret is the number in internal form.
    let mut num_radix = i32tonum(radix as i32, BASEX);

    // Digits are in reverse order, back over them LSD first.
    for &d in a.mant.iter().rev() {
        mulnumx(&mut pnumret, &num_radix)?;
        // WARNING:
        // This should just smack in each digit into a 'special' thisdigit.
        // and not do the overhead of recreating the number type each time.
        let thisdigit = i32tonum(d as i32, BASEX);
        addnum(&mut pnumret, &thisdigit, BASEX)?;
    }

    // Calculate the exponent of the external base for scaling.
    numpowi32x(&mut num_radix, a.exp)?;

    // ... and scale the result.
    mulnumx(&mut pnumret, &num_radix)?;

    // And propagate the sign.
    pnumret.sign = a.sign;

    Ok(pnumret)
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: StringToRat
//
//  ARGUMENTS:
//              mantissaIsNegative true if mantissa is less than zero
//              mantissa a string representation of a number
//              exponentIsNegative  true if exponent is less than zero
//              exponent a string representation of a number
//              radix is the number base used in the source string
//
//  RETURN: PRAT representation of string input.
//          Or nullptr if no number scanned.
//
//  EXPLANATION: This is for calc.
//
//-----------------------------------------------------------------------------

pub(crate) fn string_to_rat(
    ctx: &Ctx,
    mantissa_is_negative: bool,
    mantissa: &str,
    exponent_is_negative: bool,
    exponent: &str,
    radix: u32,
    precision: i32,
) -> CalcResult<Option<Rat>> {
    // holds exponent in rational form.
    let mut result_rat;

    // Deal with mantissa
    if mantissa.is_empty() {
        // Preset value if no mantissa
        if exponent.is_empty() {
            // Exponent not specified, preset value to zero
            result_rat = ctx.rat_zero.clone();
        } else {
            // Exponent specified, preset value to one
            result_rat = ctx.rat_one.clone();
        }
    } else {
        // Mantissa specified, convert to number form.
        let Some(pnummant) = string_to_number(ctx, mantissa, radix, precision)? else {
            return Ok(None);
        };

        // convert to rational form, and cleanup.
        result_rat = numtorat(&pnummant, radix)?;
    }

    // Deal with exponent
    let mut expt: i32 = 0;
    if !exponent.is_empty() {
        // Exponent specified, convert to number form.
        // Don't use native stuff, as it is restricted in the bases it can
        // handle.
        let Some(num_exp) = string_to_number(ctx, exponent, radix, precision)? else {
            return Ok(None);
        };

        // Convert exponent number form to native integral form,  and cleanup.
        expt = numtoi32(&num_exp, radix);
    }

    // Convert native integral exponent form to rational multiplier form.
    let mut pnumexp = i32tonum(radix as i32, BASEX);
    numpowi32x(&mut pnumexp, expt.wrapping_abs())?;

    let pratexp = Rat {
        pp: pnumexp,
        pq: i32tonum(1, BASEX),
    };

    if exponent_is_negative {
        // multiplier is less than 1, this means divide.
        divrat(ctx, &mut result_rat, &pratexp, precision)?;
    } else if expt > 0 {
        // multiplier is greater than 1, this means multiply.
        mulrat(ctx, &mut result_rat, &pratexp, precision)?;
    }
    // multiplier can be 1, in which case it'd be a waste of time to multiply.

    if mantissa_is_negative {
        // A negative number was used, adjust the sign.
        result_rat.pp.sign = result_rat.pp.sign.wrapping_mul(-1);
    }

    Ok(Some(result_rat))
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: StringToNumber
//
//  RETURN: pnumber representation of string input.
//          Or nullptr if no number scanned.
//
//  EXPLANATION: This is a state machine,
//
//    State      Description            Example, ^shows just read position.
//                                                which caused the transition
//
//    START      Start state            ^1.0
//    MANTS      Mantissa sign          -^1.0
//    LZ         Leading Zero           0^1.0
//    LZDP       Post LZ dec. pt.       000.^1
//    LD         Leading digit          1^.0
//    DZ         Post LZDP Zero         000.0^1
//    DD         Post Decimal digit     .01^2
//    DDP        Leading Digit dec. pt. 1.^2
//    EXPB       Exponent Begins        1.0e^2
//    EXPS       Exponent sign          1.0e+^5
//    EXPD       Exponent digit         1.0e1^2 or  even 1.0e0^1
//    EXPBZ      Exponent begin post 0  0.000e^+1
//    EXPSZ      Exponent sign post 0   0.000e+^1
//    EXPDZ      Exponent digit post 0  0.000e+1^2
//    ERR        Error case             0.0.^
//
//    Terminal   Description
//
//    DP         '.'
//    ZR         '0'
//    NZ         '1'..'9' 'A'..'Z' 'a'..'z' '@' '_'
//    SG         '+' '-'
//    EX         'e' '^' e is used for radix 10, ^ for all other radixes.
//
//-----------------------------------------------------------------------------

const DP: usize = 0;
const ZR: usize = 1;
const NZ: usize = 2;
const SG: usize = 3;
const EX: usize = 4;

const START: u8 = 0;
const MANTS: u8 = 1;
const LZ: u8 = 2;
const LZDP: u8 = 3;
const LD: u8 = 4;
const DZ: u8 = 5;
const DD: u8 = 6;
const DDP: u8 = 7;
const EXPB: u8 = 8;
const EXPS: u8 = 9;
const EXPD: u8 = 10;
const EXPBZ: u8 = 11;
const EXPSZ: u8 = 12;
const EXPDZ: u8 = 13;
const ERR: u8 = 14;

// New state is machine[state][terminal]
const MACHINE: [[u8; EX + 1]; ERR as usize + 1] = [
    //    DP,     ZR,      NZ,      SG,     EX
    // START
    [LZDP, LZ, LD, MANTS, ERR],
    // MANTS
    [LZDP, LZ, LD, ERR, ERR],
    // LZ
    [LZDP, LZ, LD, ERR, EXPBZ],
    // LZDP
    [ERR, DZ, DD, ERR, EXPB],
    // LD
    [DDP, LD, LD, ERR, EXPB],
    // DZ
    [ERR, DZ, DD, ERR, EXPBZ],
    // DD
    [ERR, DD, DD, ERR, EXPB],
    // DDP
    [ERR, DD, DD, ERR, EXPB],
    // EXPB
    [ERR, EXPD, EXPD, EXPS, ERR],
    // EXPS
    [ERR, EXPD, EXPD, ERR, ERR],
    // EXPD
    [ERR, EXPD, EXPD, ERR, ERR],
    // EXPBZ
    [ERR, EXPDZ, EXPDZ, EXPSZ, ERR],
    // EXPSZ
    [ERR, EXPDZ, EXPDZ, ERR, ERR],
    // EXPDZ
    [ERR, EXPDZ, EXPDZ, ERR, ERR],
    // ERR
    [ERR, ERR, ERR, ERR, ERR],
];

fn normalize_char_digit(c: char, radix: u32) -> char {
    // Allow upper and lower case letters as equivalent, base
    // is in the range where this is not ambiguous.
    if radix as usize >= 10 && radix as usize <= 35 {
        // DIGITS.find('A') == 10, DIGITS.find('Z') == 35
        return c.to_ascii_uppercase();
    }
    c
}

pub(crate) fn string_to_number(
    ctx: &Ctx,
    number_string: &str,
    radix: u32,
    precision: i32,
) -> CalcResult<Option<Number>> {
    let length = number_string.chars().count();
    let mut exp_sign: i32 = 1; // expSign is exponent sign ( +/- 1 )
    let mut exp_value: i32 = 0; // expValue is exponent mantissa, should be unsigned

    let mut buf = createnum(length as u32)?;
    let mut sign: i32 = 1;
    let mut cdigit: i32 = 0;
    let mut exp: i32 = 0;
    let mut pmant = length as isize - 1;

    let mut state = START; // state is the state of the input state machine.
    for c in number_string.chars() {
        // If the character is the decimal separator, use L'.' for the purposes of the state machine.
        let mut cur_char = if c == ctx.decimal_separator { '.' } else { c };

        // Switch states based on the character we encountered
        let s = state as usize;
        state = match cur_char {
            '-' | '+' => MACHINE[s][SG],
            '.' => MACHINE[s][DP],
            '0' => MACHINE[s][ZR],
            '^' | 'e' if cur_char == '^' || radix == 10 => MACHINE[s][EX],
            // Drop through in the 'e'-as-a-digit case
            _ => MACHINE[s][NZ],
        };

        // Now update our result value based on the state we are in
        match state {
            MANTS => {
                sign = if cur_char == '-' { -1 } else { 1 };
            }
            EXPSZ | EXPS => {
                exp_sign = if cur_char == '-' { -1 } else { 1 };
            }
            EXPDZ | EXPD => {
                cur_char = normalize_char_digit(cur_char, radix);
                match digits_find(cur_char) {
                    Some(pos) => {
                        exp_value = (exp_value as u32).wrapping_mul(radix) as i32;
                        exp_value = exp_value.wrapping_add(pos as i32);
                    }
                    None => state = ERR,
                }
            }
            LD | DD => {
                if state == LD {
                    exp = exp.wrapping_add(1);
                }
                cur_char = normalize_char_digit(cur_char, radix);
                match digits_find(cur_char) {
                    Some(pos) if pos < radix as usize => {
                        buf[pmant as usize] = pos as u32;
                        pmant -= 1;
                        exp = exp.wrapping_sub(1);
                        cdigit += 1;
                    }
                    _ => state = ERR,
                }
            }
            DZ => {
                exp = exp.wrapping_sub(1);
            }
            _ => {}
        }
    }

    if state == DZ || state == EXPDZ {
        cdigit = 1;
        exp = 0;
        sign = 1;
    } else {
        while cdigit < length as i32 {
            cdigit += 1;
            exp = exp.wrapping_sub(1);
        }

        exp = exp.wrapping_add(exp_sign.wrapping_mul(exp_value));
    }

    // If we don't have a number, clear our result.
    if cdigit == 0 {
        return Ok(None);
    }
    buf.truncate(cdigit as usize);
    let mut pnumret = Number {
        sign,
        exp,
        mant: buf,
    };
    stripzeroesnum(&mut pnumret, precision);
    Ok(Some(pnumret))
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: i32torat
//
//    DESCRIPTION: Converts int32_t input to rational (p over q)
//    form, where q is 1 and p is the int32_t.
//
//-----------------------------------------------------------------------------

pub(crate) fn i32torat(ini32: i32) -> Rat {
    Rat {
        pp: i32tonum(ini32, BASEX),
        pq: i32tonum(1, BASEX),
    }
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: Ui32torat
//
//    DESCRIPTION: Converts uint32_t input to rational (p over q)
//    form, where q is 1 and p is the uint32_t. Being unsigned cant take negative
//    numbers, but the full range of unsigned numbers
//
//-----------------------------------------------------------------------------

pub(crate) fn ui32torat(inui32: u32) -> Rat {
    Rat {
        pp: ui32tonum(inui32, BASEX),
        pq: i32tonum(1, BASEX),
    }
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: i32tonum
//
//    DESCRIPTION: Returns a number representation in the
//    base   requested of the int32_t value passed in.
//
//-----------------------------------------------------------------------------

pub(crate) fn i32tonum(ini32: i32, radix: u32) -> Number {
    let mut mant = Vec::with_capacity(MAX_LONG_SIZE as usize + 1);
    let sign;
    let mut v = ini32;
    if v < 0 {
        sign = -1;
        v = v.wrapping_mul(-1);
    } else {
        sign = 1;
    }

    // `ini32 % radix` and `ini32 /= radix` are evaluated in uint32_t.
    let mut u = v as u32;
    loop {
        mant.push(u % radix);
        u /= radix;
        if u == 0 {
            break;
        }
    }

    Number { sign, exp: 0, mant }
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: Ui32tonum
//
//    DESCRIPTION: Returns a number representation in the
//    base   requested of the uint32_t value passed in. Being unsigned number it has no
//    negative number and takes the full range of unsigned number
//
//-----------------------------------------------------------------------------

pub(crate) fn ui32tonum(mut ini32: u32, radix: u32) -> Number {
    let mut mant = Vec::with_capacity(MAX_LONG_SIZE as usize + 1);
    loop {
        mant.push(ini32 % radix);
        ini32 /= radix;
        if ini32 == 0 {
            break;
        }
    }
    Number {
        sign: 1,
        exp: 0,
        mant,
    }
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: rattoi32
//
//    DESCRIPTION: returns the int32_t representation of the
//    number input.  Assumes that the number is in the internal
//    base.
//
//-----------------------------------------------------------------------------

pub(crate) fn rattoi32(ctx: &Ctx, prat: &Rat, radix: u32, precision: i32) -> CalcResult<i32> {
    if rat_gt(ctx, prat, &ctx.rat_max_i32, precision)?
        || rat_lt(ctx, prat, &ctx.rat_min_i32, precision)?
    {
        // Don't attempt rattoi32 of anything too big or small
        return Err(CALC_E_DOMAIN);
    }

    let mut pint = prat.clone();

    intrat(ctx, &mut pint, radix, precision)?;
    divnumx(ctx, &mut pint.pp, &pint.pq, precision)?;
    pint.pq = ctx.num_one.clone();

    Ok(numtoi32(&pint.pp, BASEX))
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: rattoUi32
//
//    DESCRIPTION: returns the Ui32 representation of the
//    number input.  Assumes that the number is in the internal
//    base.
//
//-----------------------------------------------------------------------------

pub(crate) fn rattoui32(ctx: &Ctx, prat: &Rat, radix: u32, precision: i32) -> CalcResult<u32> {
    if rat_gt(ctx, prat, &ctx.rat_dword, precision)? || rat_lt(ctx, prat, &ctx.rat_zero, precision)?
    {
        // Don't attempt rattoui32 of anything too big or small
        return Err(CALC_E_DOMAIN);
    }

    let mut pint = prat.clone();

    intrat(ctx, &mut pint, radix, precision)?;
    divnumx(ctx, &mut pint.pp, &pint.pq, precision)?;
    pint.pq = ctx.num_one.clone();

    // This happens to work even if it is only signed
    Ok(numtoi32(&pint.pp, BASEX) as u32)
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: rattoUi64
//
//    DESCRIPTION: returns the 64 bit (irrespective of which processor this is running in) representation of the
//    number input.  Assumes that the number is in the internal
//    base. Can throw exception if the number exceeds 2^64
//    Implementation by getting the HI & LO 32 bit words and concatenating them, as the
//    internal base chosen happens to be 2^32, this is easier.
//-----------------------------------------------------------------------------

pub(crate) fn rattoui64(ctx: &Ctx, prat: &Rat, radix: u32, precision: i32) -> CalcResult<u64> {
    // first get the LO 32 bit word
    let mut pint = prat.clone();
    andrat(ctx, &mut pint, &ctx.rat_dword, radix, precision)?; // & 0xFFFFFFFF   (2 ^ 32 -1)
    let lo = rattoui32(ctx, &pint, radix, precision)?; // wont throw exception because already hi-dword chopped off

    let mut pint = prat.clone(); // previous pint will get freed by this as well
    let prat32 = i32torat(32);
    rshrat(ctx, &mut pint, &prat32, radix, precision)?;
    intrat(ctx, &mut pint, radix, precision)?;
    andrat(ctx, &mut pint, &ctx.rat_dword, radix, precision)?; // & 0xFFFFFFFF   (2 ^ 32 -1)
    let hi = rattoui32(ctx, &pint, radix, precision)?;

    Ok(((hi as u64) << 32) | lo as u64)
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: numtoi32
//
//    DESCRIPTION: returns the int32_t representation of the
//    number input.  Assumes that the number is really in the
//    base   claimed.
//
//-----------------------------------------------------------------------------

pub(crate) fn numtoi32(pnum: &Number, radix: u32) -> i32 {
    let mut lret: i32 = 0;

    let expt = pnum.exp;
    let mut idx = pnum.mant.len();
    let mut length = pnum.cdigit();
    while length > 0 && length.wrapping_add(expt) > 0 {
        idx -= 1;
        lret = (lret as u32).wrapping_mul(radix) as i32;
        lret = (lret as u32).wrapping_add(pnum.mant[idx]) as i32;
        length -= 1;
    }

    // while (expt-- > 0) lret *= radix;  -- evaluated in closed form.
    if expt > 0 {
        lret = (lret as u32).wrapping_mul(radix.wrapping_pow(expt as u32)) as i32;
    }
    lret.wrapping_mul(pnum.sign)
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: bool stripzeroesnum
//
//    ARGUMENTS:            a number representation
//
//    RETURN: true if stripping done, modifies number in place.
//
//    DESCRIPTION: Strips off trailing zeros.
//
//-----------------------------------------------------------------------------

pub(crate) fn stripzeroesnum(pnum: &mut Number, starting: i32) -> bool {
    let mut fstrip = false;
    // point pmant to the LeastCalculatedDigit
    let mut pmant: usize = 0;
    let mut cdigits = pnum.cdigit();
    // point pmant to the LSD
    if cdigits > starting {
        pmant = cdigits.wrapping_sub(starting) as usize;
        cdigits = starting;
    }

    // Check we haven't gone too far, and we are still looking at zeros.
    while cdigits > 0 && pnum.mant[pmant] == 0 {
        // move to next significant digit and keep track of digits we can
        // ignore later.
        pmant += 1;
        cdigits -= 1;
        fstrip = true;
    }

    // If there are zeros to remove.
    if fstrip {
        // Remove them.
        let old_cdigit = pnum.cdigit();
        pnum.mant.truncate(pmant + cdigits as usize);
        pnum.mant.drain(..pmant);
        // And adjust exponent and digit count accordingly.
        pnum.exp = pnum.exp.wrapping_add(old_cdigit - cdigits);
    }
    fstrip
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: NumberToString
//
//    ARGUMENTS: number representation
//          fmt, one of NumberFormat::Float, NumberFormat::Scientific or
//          NumberFormat::Engineering
//          integer radix and int32_t precision value
//
//    RETURN: String representation of number.
//
//    DESCRIPTION: Converts a number to its string
//    representation.
//
//-----------------------------------------------------------------------------

pub(crate) fn number_to_string(
    ctx: &Ctx,
    pnum: &mut Number,
    old_format: NumberFormat,
    radix: u32,
    precision: i32,
) -> CalcResult<String> {
    // The C++ recurses (`return NumberToString(pnum, oldFormat, ...)`) after a
    // rounding changed too much; that tail call is this loop.
    loop {
        stripzeroesnum(pnum, precision.wrapping_add(2));
        let mut length = pnum.cdigit();
        let mut exponent = pnum.exp.wrapping_add(length); // Actual number of digits to the left of decimal

        let mut format = old_format;
        if exponent > precision && format == NumberFormat::Float {
            // Force scientific mode to prevent user from assuming 33rd digit is exact.
            format = NumberFormat::Scientific;
        }

        // Make length small enough to fit in pret.
        if length > precision {
            length = precision;
        }

        // If there is a chance a round has to occur, round.
        // - if number is zero no rounding
        // - if number of digits is less than the maximum output no rounding
        let mut round: Option<Number> = None;
        if !zernum(pnum)
            && (pnum.cdigit() >= precision
                || (length.wrapping_sub(exponent) > precision
                    && exponent >= -MAX_ZEROS_AFTER_DECIMAL))
        {
            // Otherwise round.
            let mut r = i32tonum(radix as i32, radix);
            divnum(&mut r, &ctx.num_two, radix, precision)?;

            // Make round number exponent one below the LSD for the number.
            if exponent > 0 || format == NumberFormat::Float {
                r.exp = pnum
                    .exp
                    .wrapping_add(pnum.cdigit())
                    .wrapping_sub(r.cdigit())
                    .wrapping_sub(precision);
            } else {
                r.exp = pnum
                    .exp
                    .wrapping_add(pnum.cdigit())
                    .wrapping_sub(r.cdigit())
                    .wrapping_sub(precision)
                    .wrapping_sub(exponent);
                length = precision.wrapping_add(exponent);
            }

            r.sign = pnum.sign;
            round = Some(r);
        }

        if format == NumberFormat::Float {
            // Figure out if the exponent will fill more space than the non-exponent field.
            if length.wrapping_sub(exponent) > precision || exponent > precision.wrapping_add(3) {
                if exponent >= -MAX_ZEROS_AFTER_DECIMAL {
                    // C++ dereferences `round` unconditionally here; it is
                    // never null on this path for precision >= 2.
                    if let Some(r) = round.as_mut() {
                        r.exp = r.exp.wrapping_sub(exponent);
                    }
                    length = precision.wrapping_add(exponent);
                } else {
                    // Case where too many zeros are to the right or left of the
                    // decimal pt. And we are forced to switch to scientific form.
                    format = NumberFormat::Scientific;
                }
            } else if length.wrapping_add(exponent.wrapping_abs()) < precision
                && let Some(r) = round.as_mut()
            {
                // Minimum loss of precision occurs with listing leading zeros
                // if we need to make room for zeros sacrifice some digits.
                r.exp = r.exp.wrapping_sub(exponent);
            }
        }

        if let Some(r) = round {
            addnum(pnum, &r, radix)?;
            let offset = pnum
                .cdigit()
                .wrapping_add(pnum.exp)
                .wrapping_sub(r.cdigit().wrapping_add(r.exp));
            if stripzeroesnum(pnum, offset) {
                // WARNING: nesting/recursion, too much has been changed, need to
                // re-figure format.
                continue;
            }
        } else {
            stripzeroesnum(pnum, precision);
        }

        // Set up all the post rounding stuff.
        let mut use_sci_form = false;
        let mut eout = exponent.wrapping_sub(1); // Displayed exponent.
        let mut pmant = pnum.mant.len() as isize - 1;
        // Case where too many digits are to the left of the decimal or
        // NumberFormat::Scientific or NumberFormat::Engineering was specified.
        if format == NumberFormat::Scientific || format == NumberFormat::Engineering {
            use_sci_form = true;
            if eout != 0 {
                if format == NumberFormat::Engineering {
                    exponent = eout % 3;
                    eout -= exponent;
                    exponent += 1;

                    // Fix the case where 0.02e-3 should really be 2.e-6 etc.
                    if exponent < 0 {
                        exponent += 3;
                        eout -= 3;
                    }
                } else {
                    exponent = 1;
                }
            }
        } else {
            eout = 0;
        }

        // Begin building the result string
        let sep = ctx.decimal_separator;
        let mut result = String::new();

        // Make sure negative zeros aren't allowed.
        if pnum.sign == -1 && length > 0 {
            result.push('-');
        }

        if exponent <= 0 && !use_sci_form {
            result.push('0');
            result.push(sep);
            // Used up a digit unaccounted for.
        }

        while exponent < 0 {
            result.push('0');
            exponent += 1;
        }

        while length > 0 {
            exponent -= 1;
            result.push(DIGITS[pnum.mant[pmant as usize] as usize] as char);
            pmant -= 1;
            length -= 1;

            // Be more regular in using a decimal point.
            if exponent == 0 {
                result.push(sep);
            }
        }

        while exponent > 0 {
            result.push('0');
            exponent -= 1;
            // Be more regular in using a decimal point.
            if exponent == 0 {
                result.push(sep);
            }
        }

        if use_sci_form {
            result.push(if radix == 10 { 'e' } else { '^' });
            result.push(if eout < 0 { '-' } else { '+' });
            let mut e = eout.wrapping_abs() as u32;
            let mut exp_string: Vec<u8> = Vec::new();
            loop {
                exp_string.push(DIGITS[(e % radix) as usize]);
                e /= radix;
                if e == 0 {
                    break;
                }
            }
            result.extend(exp_string.iter().rev().map(|&b| b as char));
        }

        // Remove trailing decimal
        if result.ends_with(sep) {
            result.pop();
        }

        return Ok(result);
    }
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: RatToString
//
//  ARGUMENTS:
//              PRAT *representation of a number.
//              i32 representation of base  to  dump to screen.
//              fmt, one of NumberFormat::Float, NumberFormat::Scientific, or NumberFormat::Engineering
//              precision uint32_t
//
//  RETURN: string
//
//  DESCRIPTION: returns a string representation of rational number passed
//  in, at least to the precision digits.
//
//-----------------------------------------------------------------------------

pub(crate) fn rat_to_string(
    ctx: &Ctx,
    prat: &Rat,
    format: NumberFormat,
    radix: u32,
    precision: i32,
) -> CalcResult<String> {
    let mut p = rat_to_number(ctx, prat, radix, precision)?;
    number_to_string(ctx, &mut p, format, radix, precision)
}

pub(crate) fn rat_to_number(
    ctx: &Ctx,
    prat: &Rat,
    radix: u32,
    precision: i32,
) -> CalcResult<Number> {
    let mut temprat = prat.clone();
    // Convert p and q of rational form from internal base to requested base.
    // Scale by largest power of BASEX possible.
    let mut scaleby = temprat.pp.exp.min(temprat.pq.exp);
    scaleby = scaleby.max(0);

    temprat.pp.exp = temprat.pp.exp.wrapping_sub(scaleby);
    temprat.pq.exp = temprat.pq.exp.wrapping_sub(scaleby);

    let mut p = nradixxtonum(ctx, &temprat.pp, radix, precision)?;
    let q = nradixxtonum(ctx, &temprat.pq, radix, precision)?;

    // finally take the time hit to actually divide.
    divnum(&mut p, &q, radix, precision)?;

    Ok(p)
}

/// Converts a PRAT to a PNUMBER and back to a PRAT, flattening/simplifying the rational in the process
pub(crate) fn flatrat(ctx: &Ctx, prat: &mut Rat, radix: u32, precision: i32) -> CalcResult<()> {
    let pnum = rat_to_number(ctx, prat, radix, precision)?;
    *prat = numtorat(&pnum, radix)?;
    Ok(())
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: gcd
//
//  RETURN: Greatest common divisor in internal BASEX PNUMBER form.
//
//  DESCRIPTION: gcd uses remainders to find the greatest common divisor.
//
//  ASSUMPTIONS: gcd assumes inputs are integers.
//
//  NOTE: Before it was found that the TRIM macro actually kept the
//        size down cheaper than GCD, this routine was used extensively.
//        now it is not used but might be later.
//
//-----------------------------------------------------------------------------

#[allow(dead_code)]
pub(crate) fn gcd(a: &Number, b: &Number) -> CalcResult<Number> {
    if zernum(a) {
        return Ok(b.clone());
    } else if zernum(b) {
        return Ok(a.clone());
    }

    let (mut larger, mut smaller) = if lessnum(a, b) {
        (b.clone(), a.clone())
    } else {
        (a.clone(), b.clone())
    };

    while !zernum(&smaller) {
        remnum(&mut larger, &smaller, BASEX)?;
        // swap larger and smaller
        std::mem::swap(&mut larger, &mut smaller);
    }
    Ok(larger)
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: i32factnum
//
//  RETURN: Factorial of input in radix PNUMBER form.
//
//  NOTE:  Not currently used.
//
//-----------------------------------------------------------------------------

#[allow(dead_code)]
pub(crate) fn i32factnum(mut ini32: i32, radix: u32) -> CalcResult<Number> {
    let mut lret = i32tonum(1, radix);

    while ini32 > 0 {
        let tmp = i32tonum(ini32, radix);
        ini32 -= 1;
        mulnum(&mut lret, &tmp, radix)?;
    }
    Ok(lret)
}

//-----------------------------------------------------------------------------
//
//  FUNCTION: i32prodnum
//
//  RETURN: Factorial of input in base PNUMBER form.
//
//-----------------------------------------------------------------------------

#[allow(dead_code)]
pub(crate) fn i32prodnum(mut start: i32, stop: i32, radix: u32) -> CalcResult<Number> {
    let mut lret = i32tonum(1, radix);

    while start <= stop {
        if start != 0 {
            let tmp = i32tonum(start, radix);
            mulnum(&mut lret, &tmp, radix)?;
        }
        start = start.wrapping_add(1);
        if start == i32::MIN {
            break;
        }
    }
    Ok(lret)
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: numpowi32
//
//    ARGUMENTS: root as number power as int32_t and radix of
//               number along with the precision value in int32_t.
//
//    RETURN: None root is changed.
//
//    DESCRIPTION: changes numeric representation of root to
//    root ** power. Assumes radix is the radix of root.
//
//-----------------------------------------------------------------------------

pub(crate) fn numpowi32(
    ctx: &Ctx,
    proot: &mut Number,
    mut power: i32,
    radix: u32,
    precision: i32,
) -> CalcResult<()> {
    let mut lret = i32tonum(1, radix);

    while power > 0 {
        if power & 1 != 0 {
            mulnum(&mut lret, proot, radix)?;
        }
        mulnum_self(proot, radix)?;
        trimnum(ctx, proot, precision);
        power >>= 1;
    }
    *proot = lret;
    Ok(())
}

//-----------------------------------------------------------------------------
//
//    FUNCTION: ratpowi32
//
//    ARGUMENTS: root as rational, power as int32_t and precision as int32_t.
//
//    RETURN: None root is changed.
//
//    DESCRIPTION: changes rational representation of root to
//    root ** power.
//
//-----------------------------------------------------------------------------

pub(crate) fn ratpowi32(ctx: &Ctx, proot: &mut Rat, power: i32, precision: i32) -> CalcResult<()> {
    if power < 0 {
        // Take the positive power and invert answer.
        // (C++ negates `power`; for INT_MIN that recurses forever. The
        // magnitude is used here instead.)
        ratpowi32_abs(ctx, proot, power.unsigned_abs(), precision)?;
        std::mem::swap(&mut proot.pp, &mut proot.pq);
        Ok(())
    } else {
        ratpowi32_abs(ctx, proot, power as u32, precision)
    }
}

fn ratpowi32_abs(ctx: &Ctx, proot: &mut Rat, mut power: u32, precision: i32) -> CalcResult<()> {
    let mut lret = i32torat(1);

    while power > 0 {
        if power & 1 != 0 {
            mulnumx(&mut lret.pp, &proot.pp)?;
            mulnumx(&mut lret.pq, &proot.pq)?;
        }
        mulrat_self(ctx, proot, precision)?;
        trimit(ctx, &mut lret, precision);
        trimit(ctx, proot, precision);
        power >>= 1;
    }
    *proot = lret;
    Ok(())
}
