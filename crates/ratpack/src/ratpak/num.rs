// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//-----------------------------------------------------------------------------
//  Package Title  ratpak
//  File           num.c
//  Copyright      (C) 1995-97 Microsoft
//  Date           01-16-95
//
//
//  Description
//
//     Contains number routines for add, mul, div, rem and other support
//  and longs.
//
//-----------------------------------------------------------------------------

use super::conv::{createnum, i32tonum};
use super::{CalcResult, Number};

//----------------------------------------------------------------------------
//
//    FUNCTION: addnum
//
//    DESCRIPTION: Does the number equivalent of *pa += b.
//    Assumes radix is the base of both numbers.
//
//    ALGORITHM: Adds each digit from least significant to most
//    significant.
//
//----------------------------------------------------------------------------

pub(crate) fn addnum(pa: &mut Number, b: &Number, radix: u32) -> CalcResult<()> {
    addnum_s(pa, b, b.sign, radix)
}

/// `addnum` where `b->sign` is taken to be `bsign`. Used where the C++
/// temporarily flips `b->sign` around a call (`b->sign *= -1; addnum(...);
/// b->sign *= -1;`).
pub(crate) fn addnum_s(pa: &mut Number, b: &Number, bsign: i32, radix: u32) -> CalcResult<()> {
    if b.cdigit() > 1 || b.d0() != 0 {
        // If b is zero we are done.
        if pa.cdigit() > 1 || pa.d0() != 0 {
            // pa and b are both nonzero.
            let c = addnum_core(pa, b, bsign, radix)?;
            *pa = c;
        } else {
            // if pa is zero and b isn't just copy b.
            pa.clone_from(b);
            pa.sign = bsign;
        }
    }
    Ok(())
}

/// `addnum(&a, a, radix)`: doubles `a` in place (the aliasing call).
pub(crate) fn addnum_self(pa: &mut Number, radix: u32) -> CalcResult<()> {
    if pa.cdigit() > 1 || pa.d0() != 0 {
        let c = addnum_core(pa, pa, pa.sign, radix)?;
        *pa = c;
    }
    Ok(())
}

/// `_addnum`: returns `a + b` (with `b`'s sign taken as `bsign`).
fn addnum_core(a: &Number, b: &Number, bsign: i32, radix: u32) -> CalcResult<Number> {
    let a_cdigit = a.cdigit();
    let b_cdigit = b.cdigit();
    super::charge(a.mant.len() + b.mant.len());

    // Calculate the overlap of the numbers after alignment, this includes
    // necessary padding 0's
    let mut cdigits = a_cdigit
        .wrapping_add(a.exp)
        .max(b_cdigit.wrapping_add(b.exp))
        .wrapping_sub(a.exp.min(b.exp));

    let mut c = createnum(cdigits.wrapping_add(1) as u32)?;
    let c_exp = a.exp.min(b.exp);
    let mut mexp = c_exp;
    let c_cdigit_init = cdigits;
    let mut c_cdigit = cdigits;

    let mut ia = 0usize;
    let mut ib = 0usize;
    let mut ic = 0usize;
    let mut cy: u32 = 0; // cy is the value of a carry after adding two 'digits'
    let mut fcompla = false; // fcompla is a flag to signal a is negative.
    let mut fcomplb = false; // fcomplb is a flag to signal b is negative.

    // Figure out the sign of the numbers
    if a.sign != bsign {
        cy = 1;
        fcompla = a.sign == -1;
        fcomplb = bsign == -1;
    }

    // Loop over all the digits, real and 0 padded. Here we know a and b are
    // aligned
    while cdigits > 0 {
        // Get digit from a, taking padding into account.
        let mut da = if mexp >= a.exp
            && cdigits.wrapping_add(a.exp).wrapping_sub(c_exp)
                > c_cdigit_init.wrapping_sub(a_cdigit)
        {
            let d = a.mant[ia];
            ia += 1;
            d
        } else {
            0
        };
        // Get digit from b, taking padding into account.
        let mut db = if mexp >= b.exp
            && cdigits.wrapping_add(b.exp).wrapping_sub(c_exp)
                > c_cdigit_init.wrapping_sub(b_cdigit)
        {
            let d = b.mant[ib];
            ib += 1;
            d
        } else {
            0
        };

        // Handle complementing for a and b digit. Might be a better way, but
        // haven't found it yet.
        if fcompla {
            da = radix.wrapping_sub(1).wrapping_sub(da);
        }
        if fcomplb {
            db = radix.wrapping_sub(1).wrapping_sub(db);
        }

        // Update carry as necessary
        cy = da.wrapping_add(db).wrapping_add(cy);
        c[ic] = cy % radix;
        ic += 1;
        cy /= radix;

        cdigits -= 1;
        mexp = mexp.wrapping_add(1);
    }

    // Handle carry from last sum as extra digit
    if cy != 0 && !(fcompla || fcomplb) {
        c[ic] = cy;
        c_cdigit += 1;
    }

    // Compute sign of result
    let sign;
    if !(fcompla || fcomplb) {
        sign = a.sign;
    } else if cy != 0 {
        sign = 1;
    } else {
        // In this particular case an overflow or underflow has occurred
        // and all the digits need to be complemented, at one time an
        // attempt to handle this above was made, it turned out to be much
        // slower on average.
        sign = -1;
        cy = 1;
        for d in c.iter_mut().take(c_cdigit.max(0) as usize) {
            cy = radix.wrapping_sub(1).wrapping_sub(*d).wrapping_add(cy);
            *d = cy % radix;
            cy /= radix;
        }
    }

    // Remove leading zeros, remember digits are in order of
    // increasing significance. i.e. 100 would be 0,0,1
    while c_cdigit > 1 && c[(c_cdigit - 1) as usize] == 0 {
        c_cdigit -= 1;
    }
    c.truncate(c_cdigit.max(0) as usize);
    Ok(Number {
        sign,
        exp: c_exp,
        mant: c,
    })
}

//----------------------------------------------------------------------------
//
//    FUNCTION: mulnum
//
//    DESCRIPTION: Does the number equivalent of *pa *= b.
//    Assumes radix is the radix of both numbers.  This algorithm is the
//    same one you learned in grade school.
//
//----------------------------------------------------------------------------

pub(crate) fn mulnum(pa: &mut Number, b: &Number, radix: u32) -> CalcResult<()> {
    if b.cdigit() > 1 || b.d0() != 1 || b.exp != 0 {
        // If b is one we don't multiply exactly.
        if pa.cdigit() > 1 || pa.d0() != 1 || pa.exp != 0 {
            // pa and b are both non-one.
            let c = mulnum_core(pa, b, radix)?;
            *pa = c;
        } else {
            // if pa is one and b isn't just copy b, and adjust the sign.
            let sign = pa.sign;
            pa.clone_from(b);
            pa.sign = pa.sign.wrapping_mul(sign);
        }
    } else {
        // But we do have to set the sign.
        pa.sign = pa.sign.wrapping_mul(b.sign);
    }
    Ok(())
}

/// `mulnum(&a, a, radix)`: squares `a` in place (the aliasing call).
pub(crate) fn mulnum_self(pa: &mut Number, radix: u32) -> CalcResult<()> {
    if pa.cdigit() > 1 || pa.d0() != 1 || pa.exp != 0 {
        let c = mulnum_core(pa, pa, radix)?;
        *pa = c;
    } else {
        pa.sign = pa.sign.wrapping_mul(pa.sign);
    }
    Ok(())
}

/// `_mulnum`: returns `a * b`.
fn mulnum_core(a: &Number, b: &Number, radix: u32) -> CalcResult<Number> {
    let a_cdigit = a.cdigit();
    let b_cdigit = b.cdigit();
    let ibdigit0 = a_cdigit.wrapping_add(b_cdigit).wrapping_sub(1);
    let mut c = createnum(ibdigit0.wrapping_add(1) as u32)?;
    let mut c_cdigit = ibdigit0;
    let sign = a.sign.wrapping_mul(b.sign);
    let exp = a.exp.wrapping_add(b.exp);
    let radix64 = radix as u64;
    super::charge(a.mant.len() * b.mant.len());

    let mut icdigit = 0usize; // Index of digit being calculated in final result.
    for (ia, &da) in a.mant.iter().enumerate() {
        let iadigit = a_cdigit - ia as i32;
        for (ib, &dbv) in b.mant.iter().enumerate() {
            let ibdigit = b_cdigit - ib as i32;
            let pchc = ia + ib;
            let mut cy: u64 = 0;
            let mut mcy: u64 = da as u64 * dbv as u64;
            if mcy != 0 {
                icdigit = 0;
                if ibdigit == 1 && iadigit == 1 {
                    c_cdigit += 1;
                }
            }
            // If result is nonzero, or while result of carry is nonzero...
            while mcy != 0 || cy != 0 {
                // update carry from addition(s) and multiply.
                cy += c[pchc + icdigit] as u64 + (mcy % radix64);

                // update result digit from
                c[pchc + icdigit] = (cy % radix64) as u32;
                icdigit += 1;

                // update carries from
                mcy /= radix64;
                cy /= radix64;
            }
        }
    }

    // prevent different kinds of zeros, by stripping leading duplicate zeros.
    // digits are in order of increasing significance.
    while c_cdigit > 1 && c[(c_cdigit - 1) as usize] == 0 {
        c_cdigit -= 1;
    }
    c.truncate(c_cdigit.max(0) as usize);
    Ok(Number { sign, exp, mant: c })
}

//----------------------------------------------------------------------------
//
//    FUNCTION: remnum
//
//    DESCRIPTION: Does the number equivalent of *pa %= b.
//            Repeatedly subtracts off powers of 2 of b until *pa < b.
//
//----------------------------------------------------------------------------

pub(crate) fn remnum(pa: &mut Number, b: &Number, radix: u32) -> CalcResult<()> {
    // Once *pa is less than b, *pa is the remainder.
    while !lessnum(pa, b) {
        let mut tmp = b.clone(); // tmp is the working remainder.
        if lessnum(&tmp, pa) {
            // Start off close to the right answer for subtraction.
            tmp.exp = pa.cdigit().wrapping_add(pa.exp).wrapping_sub(tmp.cdigit());
            if pa.msd() <= tmp.msd() {
                // Don't take the chance that the numbers are equal.
                tmp.exp = tmp.exp.wrapping_sub(1);
            }
        }

        // lasttmp is the last remainder which worked.
        let mut lasttmp = i32tonum(0, radix);

        while lessnum(&tmp, pa) {
            lasttmp.clone_from(&tmp);
            addnum_self(&mut tmp, radix)?;
        }

        if lessnum(pa, &tmp) {
            // too far, back up...
            tmp = lasttmp;
        }

        // Subtract the working remainder from the remainder holder.
        tmp.sign = -pa.sign;
        addnum(pa, &tmp, radix)?;
    }
    Ok(())
}

//---------------------------------------------------------------------------
//
//    FUNCTION: divnum
//
//    DESCRIPTION: Does the number equivalent of *pa /= b.
//    Assumes radix is the radix of both numbers.
//
//---------------------------------------------------------------------------

pub(crate) fn divnum(pa: &mut Number, b: &Number, radix: u32, precision: i32) -> CalcResult<()> {
    if b.cdigit() > 1 || b.d0() != 1 || b.exp != 0 {
        // b is not one
        let c = divnum_core(pa, b, radix, precision)?;
        *pa = c;
    } else {
        // But we do have to set the sign.
        pa.sign = pa.sign.wrapping_mul(b.sign);
    }
    Ok(())
}

/// `_divnum`
fn divnum_core(a: &Number, b: &Number, radix: u32, precision: i32) -> CalcResult<Number> {
    let mut thismax = precision.wrapping_add(2);
    if thismax < a.cdigit() {
        thismax = a.cdigit();
    }
    if thismax < b.cdigit() {
        thismax = b.cdigit();
    }

    let mut c = createnum(thismax.wrapping_add(1) as u32)?;
    let c_exp = a
        .cdigit()
        .wrapping_add(a.exp)
        .wrapping_sub(b.cdigit().wrapping_add(b.exp))
        .wrapping_add(1);
    let c_sign = a.sign.wrapping_mul(b.sign);

    let mut ptrc = thismax as usize;
    let mut rem = a.clone();
    let mut tmp = b.clone();
    tmp.sign = a.sign;
    rem.exp = b.cdigit().wrapping_add(b.exp).wrapping_sub(rem.cdigit());

    // Build a table of multiplications of the divisor, this is quicker for
    // more than radix 'digits'. The C++ uses a list with emplace_front;
    // here numbers[k] == k * tmp and it is walked from the back.
    let mut numbers: Vec<Number> = Vec::with_capacity(radix as usize);
    numbers.push(i32tonum(0, radix));
    for _ in 1..radix {
        let mut new_value = numbers[numbers.len() - 1].clone();
        addnum(&mut new_value, &tmp, radix)?;
        numbers.push(new_value);
    }
    drop(tmp);

    let mut cdigits: i32 = 0;
    while cdigits < thismax && !zernum(&rem) {
        cdigits += 1;
        super::charge(rem.mant.len() * numbers.len());
        let mut digit = radix as i32 - 1;
        let mut multiple = 0usize;
        for k in (0..numbers.len()).rev() {
            if !lessnum(&rem, &numbers[k]) {
                multiple = k;
                break;
            }
            digit -= 1;
            if digit == 0 {
                multiple = k;
                break;
            }
        }

        if digit != 0 {
            let m = &numbers[multiple];
            addnum_s(&mut rem, m, -m.sign, radix)?;
        }
        rem.exp = rem.exp.wrapping_add(1);
        c[ptrc] = digit as u32;
        ptrc = ptrc.wrapping_sub(1);
    }

    Ok(finish_division(c, thismax, cdigits, c_exp, c_sign))
}

/// The common tail of `_divnum`/`_divnumx`: moves the `cdigits` digits that
/// were written downwards from `c[thismax]` to the bottom of the mantissa and
/// fixes up exponent and digit count.
pub(crate) fn finish_division(
    mut c: Vec<u32>,
    thismax: i32,
    cdigits: i32,
    c_exp: i32,
    sign: i32,
) -> Number {
    if cdigits == 0 {
        // A zero, make sure no weird exponents creep in
        c.truncate(1);
        return Number {
            sign,
            exp: 0,
            mant: c,
        };
    }
    let start = (thismax - cdigits + 1) as usize;
    let end = start + cdigits as usize;
    c.truncate(end);
    c.drain(..start);
    let mut c_cdigit = cdigits;
    // prevent different kinds of zeros, by stripping leading duplicate
    // zeros. digits are in order of increasing significance.
    while c_cdigit > 1 && c[(c_cdigit - 1) as usize] == 0 {
        c_cdigit -= 1;
    }
    c.truncate(c_cdigit as usize);
    Number {
        sign,
        exp: c_exp.wrapping_sub(cdigits),
        mant: c,
    }
}

//---------------------------------------------------------------------------
//
//    FUNCTION: equnum
//
//    DESCRIPTION: Does the number equivalent of ( a == b )
//    Only assumes that a and b are the same radix.
//
//---------------------------------------------------------------------------

pub(crate) fn equnum(a: &Number, b: &Number) -> bool {
    let diff = a
        .cdigit()
        .wrapping_add(a.exp)
        .wrapping_sub(b.cdigit().wrapping_add(b.exp));
    if diff != 0 {
        // If the exponents are different, these are different numbers.
        return false;
    }
    // OK the exponents match. Loop over all digits until we run out of
    // digits or there is a difference in the digits.
    let ccdigits = a.mant.len().max(b.mant.len());
    for k in 0..ccdigits {
        let da = if k < a.mant.len() {
            a.mant[a.mant.len() - 1 - k]
        } else {
            0
        };
        let db = if k < b.mant.len() {
            b.mant[b.mant.len() - 1 - k]
        } else {
            0
        };
        if da != db {
            return false;
        }
    }
    // In this case, they are equal.
    true
}

//---------------------------------------------------------------------------
//
//    FUNCTION: lessnum
//
//    DESCRIPTION: Does the number equivalent of ( abs(a) < abs(b) )
//    Only assumes that a and b are the same radix, WARNING THIS IS AN.
//    UNSIGNED COMPARE!
//
//---------------------------------------------------------------------------

pub(crate) fn lessnum(a: &Number, b: &Number) -> bool {
    let diff = a
        .cdigit()
        .wrapping_add(a.exp)
        .wrapping_sub(b.cdigit().wrapping_add(b.exp));
    if diff < 0 {
        // The exponent of a is less than b
        return true;
    }
    if diff > 0 {
        return false;
    }
    let ccdigits = a.mant.len().max(b.mant.len());
    for k in 0..ccdigits {
        let da = if k < a.mant.len() {
            a.mant[a.mant.len() - 1 - k]
        } else {
            0
        };
        let db = if k < b.mant.len() {
            b.mant[b.mant.len() - 1 - k]
        } else {
            0
        };
        let diff = da.wrapping_sub(db) as i32;
        if diff != 0 {
            return diff < 0;
        }
    }
    // In this case, they are equal.
    false
}

//----------------------------------------------------------------------------
//
//    FUNCTION: zernum
//
//    DESCRIPTION: Does the number equivalent of ( !a )
//
//----------------------------------------------------------------------------

pub(crate) fn zernum(a: &Number) -> bool {
    // loop over all the digits until you find a nonzero or until you run
    // out of digits
    a.mant.iter().all(|&d| d == 0)
}
