// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CEngine/sciset.cpp`.

use ratpack::rational_math::*;
use ratpack::{CalcResult, Rational};

use super::{CalcEngine, NumWidth};
use crate::radix_type::RadixType;

impl CalcEngine {
    /// To be called when either the radix or num width changes. You can use `None` in either of these values to mean
    /// dont change that.
    pub(super) fn set_radix_type_and_num_width(
        &mut self,
        radixtype: Option<RadixType>,
        numwidth: Option<NumWidth>,
    ) -> CalcResult<()> {
        // When in integer mode, the number is represented in 2's complement form. When a bit width is changing, we can
        // change the number representation back to sign, abs num form in ratpak. Soon when display sees this, it will
        // convert to 2's complement form, but this time all high bits will be propagated. Eg. -127, in byte mode is
        // represented as 1000,0001. This puts it back as sign=-1, 01111111 . But DisplayNum will see this and convert it
        // back to 1111,1111,1000,0001 when in Word mode.
        if self.f_integer_mode {
            let w64_bits = self.current_val.to_u64()?;
            let f_msb = ((w64_bits >> (self.dw_word_bit_width - 1)) & 1) != 0; // make sure you use the old width

            if f_msb {
                // If high bit is set, then get the decimal number in -ve 2'scompl form.
                let temp_result = self.current_val.bitxor(&self.get_chop_number())?;

                self.current_val = -(temp_result.add(&Rational::from(1))?);
            }
        }

        if let Some(radixtype) = radixtype {
            self.radix = self.n_radix_from_radix_type(radixtype);
            // radixtype is not even saved
        }

        if let Some(numwidth) = numwidth {
            self.numwidth = numwidth;
            self.dw_word_bit_width = self.dw_word_bit_width_from_num_width(numwidth);
        }

        // inform ratpak that a change in base or precision has occurred
        self.base_or_precision_changed();

        // display the correct number for the new state (ie convert displayed
        //  number to correct base)
        self.display_num()
    }

    pub(super) fn dw_word_bit_width_from_num_width(&self, numwidth: NumWidth) -> i32 {
        match numwidth {
            NumWidth::DwordWidth => 32,
            NumWidth::WordWidth => 16,
            NumWidth::ByteWidth => 8,
            NumWidth::QwordWidth => 64,
        }
    }

    pub(super) fn n_radix_from_radix_type(&self, radixtype: RadixType) -> u32 {
        match radixtype {
            RadixType::Hex => 16,
            RadixType::Octal => 8,
            RadixType::Binary => 2,
            RadixType::Decimal => 10,
        }
    }

    ///  Toggles a given bit into the number representation. returns true if it changed it actually.
    pub(super) fn try_toggle_bit(&mut self, rat: &mut Rational, wbitno: u32) -> CalcResult<bool> {
        let wmax = self.dw_word_bit_width_from_num_width(self.numwidth) as u32;
        if wbitno >= wmax {
            return Ok(false); // ignore error cant happen
        }

        let mut result = integer(rat)?;

        // Remove any variance in how 0 could be represented in rat e.g. -0, 0/n, etc.
        result = if result != Rational::from(0) {
            result
        } else {
            Rational::from(0)
        };

        // XOR the result with 2^wbitno power
        *rat = result.bitxor(&pow(&Rational::from(2), &Rational::from(wbitno as i32))?)?;

        Ok(true)
    }

    /// Returns the nearest power of two
    fn quick_log2(mut i_num: i32) -> i32 {
        let mut i_res = 0;

        // while first digit is a zero
        while (i_num & 1) == 0 {
            i_res += 1;
            i_num >>= 1;
        }

        // if our number isn't a perfect square
        i_num >>= 1;
        if i_num != 0 {
            // find the largest digit
            i_num >>= 1;
            while i_num != 0 {
                i_res += 1;
                i_num >>= 1;
            }

            // and then add two
            i_res += 2;
        }

        i_res
    }

    ////////////////////////////////////////////////////////////////////////
    //
    //  UpdateMaxIntDigits
    //
    // determine the maximum number of digits needed for the current precision,
    // word size, and base.  This number is conservative towards the small side
    // such that there may be some extra bits left over. For example, base 8 requires 3 bits per digit.
    // A word size of 32 bits allows for 10 digits with a remainder of two bits.  Bases
    // that require variable number of bits (non-power-of-two bases) are approximated
    // by the next highest power-of-two base (again, to be conservative and guarantee
    // there will be no over flow verse the current word size for numbers entered).
    // Base 10 is a special case and always uses the base 10 precision (m_nPrecisionSav).
    pub fn update_max_int_digits(&mut self) {
        if self.radix == 10 {
            // if in integer mode you still have to honor the max digits you can enter based on bit width
            if self.f_integer_mode {
                self.c_int_digits_sav =
                    self.get_max_decimal_value_string().chars().count() as i32 - 1;
                // This is the max digits you can enter a decimal in fixed width mode aka integer mode -1. The last digit
                // has to be checked separately
            } else {
                self.c_int_digits_sav = self.precision;
            }
        } else {
            self.c_int_digits_sav = self.dw_word_bit_width / Self::quick_log2(self.radix as i32);
        }
    }

    pub(super) fn change_base_constants(radix: u32, max_int_digits: i32, precision: i32) {
        if 10 == radix {
            ratpack::change_constants(radix, precision); // Base 10 precision for internal computing still needs to be 32, to
        // take care of decimals precisely. For eg. to get the HI word of a qword, we do a rsh, which depends on getting
        // 18446744073709551615 / 4294967296 = 4294967295.9999917... This is important it works this and doesn't reduce
        // the precision to number of digits allowed to enter. In other words, precision and # of allowed digits to be
        // entered are different.
        } else {
            ratpack::change_constants(radix, max_int_digits + 1);
        }
    }

    fn base_or_precision_changed(&mut self) {
        self.update_max_int_digits();
        CalcEngine::change_base_constants(self.radix, self.c_int_digits_sav, self.precision);
    }
}
