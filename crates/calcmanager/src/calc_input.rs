// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `Header Files/CalcInput.h` and `CEngine/CalcInput.cpp`.
//!
//! Strings are kept as `Vec<char>` so that indices behave like the C++
//! `std::wstring` indices (the decimal symbol may be a non-ASCII character).

use ratpack::{CalcResult, Rational};

/// Space to hold enough digits for a quadword binary number (64) plus digit separator strings for that number (20)
pub const MAX_STRLEN: usize = 84;

const C_NUM_MAX_DIGITS: usize = MAX_STRLEN;
const C_EXP_MAX_DIGITS: usize = 4;

/// `CalcEngine::CalcNumSec`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CalcNumSec {
    pub value: Vec<char>,
    is_negative: bool,
}

impl CalcNumSec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.is_negative = false;
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    pub fn is_negative(&self) -> bool {
        self.is_negative
    }

    pub fn set_is_negative(&mut self, is_negative: bool) {
        self.is_negative = is_negative;
    }

    fn value_string(&self) -> String {
        self.value.iter().collect()
    }
}

/// `CalcEngine::CalcInput`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalcInput {
    has_exponent: bool,
    has_decimal: bool,
    dec_pt_index: usize,
    dec_symbol: char,
    base: CalcNumSec,
    exponent: CalcNumSec,
}

impl Default for CalcInput {
    fn default() -> Self {
        CalcInput::new('.')
    }
}

impl CalcInput {
    pub fn new(dec_symbol: char) -> Self {
        CalcInput {
            has_exponent: false,
            has_decimal: false,
            dec_pt_index: 0,
            dec_symbol,
            base: CalcNumSec::new(),
            exponent: CalcNumSec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.base.clear();
        self.exponent.clear();
        self.has_exponent = false;
        self.has_decimal = false;
        self.dec_pt_index = 0;
    }

    pub fn try_toggle_sign(&mut self, is_integer_mode: bool, max_num_str: &str) -> bool {
        // Zero is always positive
        if self.base.is_empty() {
            self.base.set_is_negative(false);
            self.exponent.set_is_negative(false);
        } else if self.has_exponent {
            self.exponent.set_is_negative(!self.exponent.is_negative());
        } else {
            // When in integer only mode, it isn't always allowed to toggle, as toggling can cause the num to be out of
            // bounds. For eg. in byte -128 is valid, but when it toggled it becomes 128, which is more than 127.
            if is_integer_mode && self.base.is_negative() {
                let max: Vec<char> = max_num_str.chars().collect();
                // Decide if this additional digit will fit for the given bit width
                // (C++: maxNumStr.back() on an empty string is undefined; treat as '\0'.)
                let max_back = max.last().copied().unwrap_or('\0');
                if self.base.value.len() >= max.len()
                    && self.base.value.last().copied().unwrap_or('\0') > max_back
                {
                    // Last digit is more than the allowed positive number. Fail
                    return false;
                }
            }
            self.base.set_is_negative(!self.base.is_negative());
        }

        true
    }

    pub fn try_add_digit(
        &mut self,
        value: u32,
        radix: u32,
        is_integer_mode: bool,
        max_num_str: &str,
        word_bit_width: i32,
        max_digits: i32,
    ) -> bool {
        // Convert from an integer into a character
        // This includes both normal digits and alpha 'digits' for radixes > 10
        let ch_digit = if value < 10 {
            char::from_u32('0' as u32 + value).unwrap_or('\0')
        } else {
            char::from_u32('A' as u32 + value - 10).unwrap_or('\0')
        };

        let has_decimal = self.has_decimal;
        let has_exponent = self.has_exponent;
        let (p_num_sec, max_count): (&mut CalcNumSec, usize) = if has_exponent {
            (&mut self.exponent, C_EXP_MAX_DIGITS)
        } else {
            let p_num_sec = &mut self.base;
            // C++: size_t maxCount = maxDigits (a negative int wraps to a huge size_t)
            let mut max_count = max_digits as isize as usize;
            // Don't include the decimal point in the count. In that way you can enter the maximum allowed precision.
            // Precision doesn't include decimal point.
            if has_decimal {
                max_count = max_count.wrapping_add(1);
            }
            // First leading 0 is not counted in input restriction as the output can be of that form
            // See NumberToString algorithm. REVIEW: We don't have such input restriction mimicking based on output of NumberToString for exponent
            // NumberToString can give 10 digit exponent, but we still restrict the exponent here to be only 4 digits.
            if !p_num_sec.is_empty() && p_num_sec.value[0] == '0' {
                max_count = max_count.wrapping_add(1);
            }
            (p_num_sec, max_count)
        };

        // Ignore leading zeros
        if p_num_sec.is_empty() && (value == 0) {
            return true;
        }

        if p_num_sec.value.len() < max_count {
            p_num_sec.value.push(ch_digit);
            return true;
        }

        // if we are in integer mode, within the base, and we're on the last digit then
        // there are special cases where we can actually add one more digit.
        if is_integer_mode && p_num_sec.value.len() == max_count && !has_exponent {
            let mut allow_extra_digit = false;

            if radix == 8 {
                match word_bit_width % 3 {
                    1 => {
                        // in 16 or 64bit word size, if the first digit is a 1 we can enter 6 (16bit) or 22 (64bit) digits
                        allow_extra_digit = p_num_sec.value.first() == Some(&'1');
                    }
                    2 => {
                        // in 8 or 32bit word size, if the first digit is a 3 or less we can enter 3 (8bit) or 11 (32bit) digits
                        allow_extra_digit = p_num_sec.value.first().copied().unwrap_or('\0') <= '3';
                    }
                    _ => {}
                }
            } else if radix == 10 {
                let max: Vec<char> = max_num_str.chars().collect();
                // If value length is at least the max, we know we can't add another digit.
                if p_num_sec.value.len() < max.len() {
                    // Compare value to substring of maxNumStr of value.size() length.
                    // If cmpResult > 0:
                    // eg. max is "127", and the current number is "20". first digit itself says we are out.
                    // Additional digit is not possible

                    // If cmpResult < 0:
                    // Success case. eg. max is "127", and current number is say "11". The second digit '1' being <
                    // corresponding digit '2', means all digits are possible to append, like 119 will still be < 127

                    // If cmpResult == 0:
                    // Undecided still. The case when max is "127", and current number is "12". Look for the new number being 7 or less to allow
                    let len = p_num_sec.value.len();
                    let cmp_result = p_num_sec.value.as_slice().cmp(&max[0..len]);
                    if cmp_result == std::cmp::Ordering::Less {
                        allow_extra_digit = true;
                    } else if cmp_result == std::cmp::Ordering::Equal {
                        let last_char = max[len];
                        if ch_digit <= last_char {
                            allow_extra_digit = true;
                        } else if p_num_sec.is_negative()
                            && (ch_digit as u32) <= (last_char as u32) + 1
                        {
                            // Negative value case, eg. max is "127", and current number is "-12". Then 8 is also valid, as the range
                            // is always from -(max+1)...max in signed mode
                            allow_extra_digit = true;
                        }
                    }
                }
            }

            if allow_extra_digit {
                p_num_sec.value.push(ch_digit);
                return true;
            }
        }

        false
    }

    pub fn try_add_decimal_pt(&mut self) -> bool {
        // Already have a decimal pt or we're in the exponent
        if self.has_decimal || self.has_exponent {
            return false;
        }

        if self.base.is_empty() {
            self.base.value.push('0'); // Add a leading zero
        }

        self.dec_pt_index = self.base.value.len();
        self.base.value.push(self.dec_symbol);
        self.has_decimal = true;

        true
    }

    pub fn has_decimal_pt(&self) -> bool {
        self.has_decimal
    }

    pub fn try_begin_exponent(&mut self) -> bool {
        // For compatibility, add a trailing dec point to base num if it doesn't have one
        self.try_add_decimal_pt();

        if self.has_exponent {
            // Already entering exponent
            return false;
        }

        self.has_exponent = true; // Entering exponent
        true
    }

    pub fn backspace(&mut self) {
        if self.has_exponent {
            if !self.exponent.is_empty() {
                self.exponent.value.pop();

                if self.exponent.is_empty() {
                    self.exponent.clear();
                }
            } else {
                self.has_exponent = false;
            }
        } else {
            if !self.base.is_empty() {
                self.base.value.pop();
                if self.base.value.len() == 1 && self.base.value[0] == '0' {
                    self.base.value.pop();
                }
            }

            if self.base.value.len() <= self.dec_pt_index {
                // Backed up over decimal point
                self.has_decimal = false;
                self.dec_pt_index = 0;
            }

            if self.base.is_empty() {
                self.base.clear();
            }
        }
    }

    pub fn set_decimal_symbol(&mut self, dec_symbol: char) {
        if self.dec_symbol != dec_symbol {
            self.dec_symbol = dec_symbol;

            if self.has_decimal {
                // Change to new decimal pt
                if let Some(c) = self.base.value.get_mut(self.dec_pt_index) {
                    *c = self.dec_symbol;
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.base.is_empty() && !self.has_exponent && self.exponent.is_empty() && !self.has_decimal
    }

    /// `CalcInput::ToString`
    pub fn to_string(&self, radix: u32) -> String {
        // In theory both the base and exponent could be C_NUM_MAX_DIGITS long.
        if (self.base.value.len() > MAX_STRLEN)
            || (self.has_exponent && self.exponent.value.len() > MAX_STRLEN)
        {
            return String::new();
        }

        let mut result: Vec<char> = Vec::new();

        if self.base.is_negative() {
            result.push('-');
        }

        if self.base.is_empty() {
            result.push('0');
        } else {
            result.extend_from_slice(&self.base.value);
        }

        if self.has_exponent {
            // Add a decimal point if it is not already there
            if !self.has_decimal {
                result.push(self.dec_symbol);
            }

            result.push(if radix == 10 { 'e' } else { '^' });
            result.push(if self.exponent.is_negative() {
                '-'
            } else {
                '+'
            });

            if self.exponent.is_empty() {
                result.push('0');
            } else {
                result.extend_from_slice(&self.exponent.value);
            }
        }

        // Base and Exp can each be up to C_NUM_MAX_DIGITS in length, plus 4 characters for sign, dec, exp, and expSign.
        if result.len() > C_NUM_MAX_DIGITS * 2 + 4 {
            return String::new();
        }

        result.into_iter().collect()
    }

    /// `CalcInput::ToRational`
    pub fn to_rational(&self, radix: u32, precision: i32) -> CalcResult<Rational> {
        let rat = ratpack::string_to_rat(
            self.base.is_negative(),
            &self.base.value_string(),
            self.exponent.is_negative(),
            &self.exponent.value_string(),
            radix,
            precision,
        )?;
        match rat {
            None => Ok(Rational::from(0)),
            Some(r) => Ok(r),
        }
    }
}
