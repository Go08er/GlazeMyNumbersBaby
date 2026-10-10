// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//-----------------------------------------------------------------------------
//  Package Title  ratpak
//  File           basex.c
//  Copyright      (C) 1995-97 Microsoft
//  Date           03-14-97
//
//
//  Description
//
//     Contains number routines for internal base computations, these assume
//  internal base is a power of 2.
//
//-----------------------------------------------------------------------------

use std::cell::RefCell;
use std::collections::HashMap;

use super::conv::{createnum, i32tonum};
use super::num::{addnum, addnum_self, finish_division, lessnum, zernum};
use super::{BASEX, BASEXPWR, CalcResult, Ctx, Number};

//----------------------------------------------------------------------------
//
//    FUNCTION: mulnumx
//
//    DESCRIPTION: Does the number equivalent of *pa *= b.
//    This is a stub which prevents multiplication by 1, this is a big speed
//    improvement.
//
//----------------------------------------------------------------------------

pub(crate) fn mulnumx(pa: &mut Number, b: &Number) -> CalcResult<()> {
    if b.cdigit() > 1 || b.d0() != 1 || b.exp != 0 {
        // If b is not one we multiply
        if pa.cdigit() > 1 || pa.d0() != 1 || pa.exp != 0 {
            // pa and b are both non-one.
            let c = mulnumx_core(pa, b)?;
            *pa = c;
        } else {
            // if pa is one and b isn't just copy b. and adjust the sign.
            let sign = pa.sign;
            pa.clone_from(b);
            pa.sign = pa.sign.wrapping_mul(sign);
        }
    } else {
        // B is +/- 1, But we do have to set the sign.
        pa.sign = pa.sign.wrapping_mul(b.sign);
    }
    Ok(())
}

/// `mulnumx(&a, a)`: squares `a` in place (the aliasing call).
pub(crate) fn mulnumx_self(pa: &mut Number) -> CalcResult<()> {
    if pa.cdigit() > 1 || pa.d0() != 1 || pa.exp != 0 {
        let c = mulnumx_core(pa, pa)?;
        *pa = c;
    } else {
        pa.sign = pa.sign.wrapping_mul(pa.sign);
    }
    Ok(())
}

//----------------------------------------------------------------------------
//
//    FUNCTION: _mulnumx
//
//    DESCRIPTION: Does the number equivalent of *pa *= b.
//    Assumes the base is BASEX of both numbers.  This algorithm is the
//    same one you learned in grade school, except the base isn't 10 it's
//    BASEX.
//
//----------------------------------------------------------------------------

fn mulnumx_core(a: &Number, b: &Number) -> CalcResult<Number> {
    const MASK: u32 = !BASEX;
    let a_cdigit = a.cdigit();
    let b_cdigit = b.cdigit();
    let ibdigit0 = a_cdigit.wrapping_add(b_cdigit).wrapping_sub(1);
    let mut c = createnum(ibdigit0.wrapping_add(1) as u32)?;
    let mut c_cdigit = ibdigit0;
    let sign = a.sign.wrapping_mul(b.sign);
    let exp = a.exp.wrapping_add(b.exp);

    super::charge(a.mant.len() * b.mant.len());
    let mut icdigit = 0usize; // Index of digit being calculated in final result.
    for (ia, &da) in a.mant.iter().enumerate() {
        let iadigit = a_cdigit - ia as i32;
        for (ib, &dbv) in b.mant.iter().enumerate() {
            let ibdigit = b_cdigit - ib as i32;
            let ptrc = ia + ib;
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
                cy += c[ptrc + icdigit] as u64 + ((mcy as u32) & MASK) as u64;

                // update result digit from
                c[ptrc + icdigit] = (cy as u32) & MASK;
                icdigit += 1;

                // update carries from
                mcy >>= BASEXPWR;
                cy >>= BASEXPWR;
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

//-----------------------------------------------------------------------------
//
//    FUNCTION: numpowi32x
//
//    DESCRIPTION: changes numeric representation of root to
//    root ** power. Assumes base BASEX
//    decomposes the exponent into it's sums of powers of 2, so on average
//    it will take n+n/2 multiplies where n is the highest on bit.
//
//-----------------------------------------------------------------------------

pub(crate) fn numpowi32x(proot: &mut Number, mut power: i32) -> CalcResult<()> {
    let mut lret = i32tonum(1, BASEX);

    // Once the power remaining is zero we are done.
    while power > 0 {
        // If this bit in the power decomposition is on, multiply the result
        // by the root number.
        if power & 1 != 0 {
            mulnumx(&mut lret, proot)?;
        }

        // multiply the root number by itself to scale for the next bit (i.e.
        // square it.
        mulnumx_self(proot)?;

        // move the next bit of the power into place.
        power >>= 1;
    }
    *proot = lret;
    Ok(())
}

thread_local! {
    /// Extension: see [`with_powers_kept`].
    static POWERS: RefCell<Option<Powers>> = const { RefCell::new(None) };
}

/// Extension: the powers of a radix [`radix_power`] has worked out since
/// [`with_powers_kept`] began.
#[derive(Default)]
struct Powers {
    /// By radix: the radix to the power 2^k at index k, each the square of
    /// the one before, as `numpowi32x` squares its root.
    squares: HashMap<u32, Vec<Number>>,
    /// By radix and power: the radix to that power.
    powers: HashMap<(u32, i32), Number>,
}

/// Extension: runs `f` keeping the powers of a radix that the numbers read
/// in it (`string_to_rat`) are scaled by, so that numbers read together
/// share them, and forgets them afterwards. A hundred memory slots near
/// 10^±9999 each need 10^9999 or a power near it, a number of 33,000 bits
/// that takes fourteen squarings to reach, 1.2 million units of work each
/// time. With a power worked out once, and its squares once, the hundred
/// take 4 million units between them where they have one exponent and 21
/// million where no two have the same.
///
/// It changes no number read: a power is the same `Number`, digit for
/// digit, whether `numpowi32x` works it out or it was kept (multiplying
/// reads nothing but its two numbers). What it changes is how much is
/// worked out, and so [`work_done`](super::work_done): a caller that bounds
/// a computation by that mustn't share powers across what it bounds, which
/// is why they are kept only inside `f`.
pub(crate) fn with_powers_kept<R>(f: impl FnOnce() -> R) -> R {
    struct Forget(bool);
    impl Drop for Forget {
        fn drop(&mut self) {
            if self.0 {
                POWERS.with(|powers| *powers.borrow_mut() = None);
            }
        }
    }
    let outermost = POWERS.with(|powers| {
        let mut powers = powers.borrow_mut();
        let outermost = powers.is_none();
        if outermost {
            *powers = Some(Powers::default());
        }
        outermost
    });
    let _forget = Forget(outermost);
    f()
}

/// Extension: `radix` to the power `power` in the internal base, as
/// `numpowi32x` makes it of `i32tonum(radix, BASEX)` (1 for a power below
/// 1), reusing what [`with_powers_kept`] keeps when inside it.
pub(crate) fn radix_power(radix: u32, power: i32) -> CalcResult<Number> {
    POWERS.with(|kept| {
        let mut kept = kept.borrow_mut();
        let Some(kept) = kept.as_mut() else {
            let mut root = i32tonum(radix as i32, BASEX);
            numpowi32x(&mut root, power)?;
            return Ok(root);
        };
        if let Some(known) = kept.powers.get(&(radix, power)) {
            return Ok(known.clone());
        }
        // `numpowi32x`, step for step, with the root's squares kept: bit k
        // of the power multiplies the result by the root squared k times,
        // and the next square is made whether or not a bit is left to use
        // it (as there, where it can also be what fails).
        let squares = kept
            .squares
            .entry(radix)
            .or_insert_with(|| vec![i32tonum(radix as i32, BASEX)]);
        let mut lret = i32tonum(1, BASEX);
        let mut left = power;
        let mut k = 0;
        while left > 0 {
            if left & 1 != 0 {
                mulnumx(&mut lret, &squares[k])?;
            }
            if squares.len() == k + 1 {
                let mut next = squares[k].clone();
                mulnumx_self(&mut next)?;
                squares.push(next);
            }
            k += 1;
            left >>= 1;
        }
        kept.powers.insert((radix, power), lret.clone());
        Ok(lret)
    })
}

//----------------------------------------------------------------------------
//
//    FUNCTION: divnumx
//
//    DESCRIPTION: Does the number equivalent of *pa /= b.
//    Assumes radix is the internal radix representation.
//    This is a stub which prevents division by 1, this is a big speed
//    improvement.
//
//----------------------------------------------------------------------------

pub(crate) fn divnumx(ctx: &Ctx, pa: &mut Number, b: &Number, precision: i32) -> CalcResult<()> {
    if b.cdigit() > 1 || b.d0() != 1 || b.exp != 0 {
        // b is not one.
        if pa.cdigit() > 1 || pa.d0() != 1 || pa.exp != 0 {
            // pa and b are both not one.
            let c = divnumx_core(ctx, pa, b, precision)?;
            *pa = c;
        } else {
            // if pa is one and b is not one, just copy b, and adjust the sign.
            let sign = pa.sign;
            pa.clone_from(b);
            pa.sign = pa.sign.wrapping_mul(sign);
        }
    } else {
        // b is one so don't divide, but set the sign.
        pa.sign = pa.sign.wrapping_mul(b.sign);
    }
    Ok(())
}

//----------------------------------------------------------------------------
//
//    FUNCTION: _divnumx
//
//    DESCRIPTION: Does the number equivalent of *pa /= b.
//    Assumes radix is the internal radix representation.
//
//----------------------------------------------------------------------------

fn divnumx_core(ctx: &Ctx, a: &Number, b: &Number, precision: i32) -> CalcResult<Number> {
    // set a maximum number of internal digits to shoot for in the divide.
    let mut thismax = precision.wrapping_add(ctx.g_ratio);

    if thismax < a.cdigit() {
        // a has more digits than precision specified, bump up digits to shoot
        // for.
        thismax = a.cdigit();
    }

    if thismax < b.cdigit() {
        // b has more digits than precision specified, bump up digits to shoot
        // for.
        thismax = b.cdigit();
    }

    // Create c (the divide answer) and set up exponent and sign.
    let mut c = createnum(thismax.wrapping_add(1) as u32)?;
    let c_exp = a
        .cdigit()
        .wrapping_add(a.exp)
        .wrapping_sub(b.cdigit().wrapping_add(b.exp))
        .wrapping_add(1);
    let c_sign = a.sign.wrapping_mul(b.sign);

    let mut ptrc = thismax as usize;
    let mut cdigits: i32 = 0;

    let mut rem = a.clone(); // remainder after applying guess.
    rem.sign = b.sign;
    rem.exp = b.cdigit().wrapping_add(b.exp).wrapping_sub(rem.cdigit());

    while cdigits < thismax && !zernum(&rem) {
        cdigits += 1;
        c[ptrc] = 0;
        super::charge(rem.mant.len() + b.mant.len());
        while !lessnum(&rem, b) {
            let mut digit: i32 = 1;
            let mut tmp = b.clone(); // current guess being worked on for divide.
            // lasttmp allows a backup when the algorithm guesses one bit too far.
            let mut lasttmp = i32tonum(0, BASEX);
            while lessnum(&tmp, &rem) {
                lasttmp.clone_from(&tmp);
                addnum_self(&mut tmp, BASEX)?;
                digit = digit.wrapping_mul(2);
            }
            if lessnum(&rem, &tmp) {
                // too far, back up...
                digit /= 2;
                tmp = lasttmp;
            }

            tmp.sign = tmp.sign.wrapping_mul(-1);
            addnum(&mut rem, &tmp, BASEX)?;
            c[ptrc] |= digit as u32;
        }
        rem.exp = rem.exp.wrapping_add(1);
        ptrc = ptrc.wrapping_sub(1);
    }

    Ok(finish_division(c, thismax, cdigits, c_exp, c_sign))
}

#[cfg(test)]
mod tests {
    use super::super::work_done;
    use super::*;

    fn alone(radix: u32, power: i32) -> CalcResult<Number> {
        let mut root = i32tonum(radix as i32, BASEX);
        numpowi32x(&mut root, power).map(|()| root)
    }

    fn work(f: impl FnOnce()) -> u64 {
        let before = work_done();
        f();
        work_done() - before
    }

    /// A power kept is the power `numpowi32x` works out, digit for digit:
    /// in every radix numbers are read in, powers in any order, asked for
    /// again, and below 1.
    #[test]
    fn a_power_kept_is_the_power_worked_out() {
        let powers = [
            0,
            1,
            2,
            3,
            7,
            8,
            31,
            32,
            33,
            100,
            255,
            256,
            1000,
            4095,
            4096,
            9998,
            9999,
            19_999,
            9999,
            1,
            5000,
            -1,
            i32::MIN,
        ];
        with_powers_kept(|| {
            for radix in [2, 8, 10, 16] {
                for power in powers {
                    assert_eq!(
                        radix_power(radix, power),
                        alone(radix, power),
                        "{radix}^{power}"
                    );
                }
            }
            // Kept across one begun inside another, until the outer ends.
            with_powers_kept(|| assert_eq!(radix_power(10, 9999), alone(10, 9999)));
            assert_eq!(work(|| drop(radix_power(10, 9999))), 0);
            // A power not asked for yet shares the squares: less work.
            let shared = work(|| drop(radix_power(10, 9997)));
            assert!(shared > 0 && shared < work(|| drop(alone(10, 9997))) / 3);
        });
    }

    /// Outside `with_powers_kept` nothing is kept: a power costs what
    /// `numpowi32x` costs, each time; and inside, the first does too.
    #[test]
    fn a_power_costs_the_same_where_none_is_kept() {
        let once = work(|| drop(alone(10, 9999)));
        assert!(once > 100_000, "{once}");
        for _ in 0..2 {
            assert_eq!(work(|| drop(radix_power(10, 9999))), once);
        }
        for _ in 0..2 {
            with_powers_kept(|| {
                assert_eq!(work(|| drop(radix_power(10, 9999))), once);
                assert_eq!(work(|| drop(radix_power(10, 9999))), 0);
            });
        }
        assert_eq!(work(|| drop(radix_power(10, 9999))), once);
    }
}
