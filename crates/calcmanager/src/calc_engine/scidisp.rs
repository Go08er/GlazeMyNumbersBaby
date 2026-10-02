// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CEngine/scidisp.cpp`.

use std::cell::RefCell;

use ratpack::{CalcResult, NumberFormat, Rational, rational_math};

use super::{CalcEngine, NumWidth};
use crate::engine_strings::*;

const MAX_EXPONENT: i32 = 4;
const MAX_GROUPING_SIZE: u32 = 16;

/// State of calc last time DisplayNum was called
struct LastDisp {
    value: Rational,
    precision: i32,
    radix: u32,
    n_fe: i32,
    numwidth: Option<NumWidth>,
    f_int_math: bool,
    b_record: bool,
    b_use_sep: bool,
}

thread_local! {
    /// `static LASTDISP gldPrevious` — shared by every engine (per thread here).
    static GLD_PREVIOUS: RefCell<LastDisp> = RefCell::new(LastDisp {
        value: Rational::from(0),
        precision: -1,
        radix: 0,
        n_fe: -1,
        numwidth: None,
        f_int_math: false,
        b_record: false,
        b_use_sep: false,
    });
}

fn number_format_as_int(f: NumberFormat) -> i32 {
    match f {
        NumberFormat::Float => 0,
        NumberFormat::Scientific => 1,
        NumberFormat::Engineering => 2,
    }
}

/// `wcstoul(str, &end, 10)` as used by `DigitGroupingStringToGroupingVector`:
/// returns the (64-bit, Linux `unsigned long`) value and the index just past
/// the parsed digits, or `start` if no conversion could be performed.
fn wcstoul10(s: &[char], start: usize) -> (u64, usize) {
    let mut i = start;
    while i < s.len() && (s[i] == ' ' || ('\t'..='\r').contains(&s[i])) {
        i += 1;
    }
    let mut negative = false;
    if i < s.len() && (s[i] == '+' || s[i] == '-') {
        negative = s[i] == '-';
        i += 1;
    }
    let digits_start = i;
    let mut value: u64 = 0;
    let mut overflow = false;
    while i < s.len() && s[i].is_ascii_digit() {
        let d = u64::from(s[i] as u32 - '0' as u32);
        match value.checked_mul(10).and_then(|v| v.checked_add(d)) {
            Some(v) => value = v,
            None => overflow = true,
        }
        i += 1;
    }
    if i == digits_start {
        return (0, start);
    }
    if overflow {
        return (u64::MAX, i);
    }
    (
        if negative {
            value.wrapping_neg()
        } else {
            value
        },
        i,
    )
}

impl CalcEngine {
    /// Truncates if too big, makes it a non negative - the number in rat. Doesn't do anything if not in INT mode
    pub(super) fn truncate_num_for_int_math(&self, rat: &Rational) -> CalcResult<Rational> {
        if !self.f_integer_mode {
            return Ok(rat.clone());
        }

        // Truncate to an integer. Do not round here.
        let mut result = rational_math::integer(rat)?;

        // Can be converting a dec negative number to Hex/Oct/Bin rep. Use 2's complement form
        // Check the range.
        if result < Rational::from(0) {
            // if negative make positive by doing a twos complement
            result = (-&result).sub(&Rational::from(1))?;
            result = result.bitxor(&self.get_chop_number())?;
        }

        result = result.bitand(&self.get_chop_number())?;

        Ok(result)
    }

    /****************************************************************************\
    * void DisplayNum(void)
    *
    * Convert m_currentVal to a string in the current radix.
    *
    * Updates the following variables:
    *   m_currentVal, m_numberString
    \****************************************************************************/
    pub(super) fn display_num(&mut self) -> CalcResult<()> {
        //
        // Only change the display if
        //  we are in record mode                               -OR-
        //  this is the first time DisplayNum has been called,  -OR-
        //  something important has changed since the last time DisplayNum was
        //  called.
        //
        let changed = GLD_PREVIOUS.with(|g| {
            let g = g.borrow();
            self.b_record
                || g.value != self.current_val
                || g.precision != self.precision
                || g.radix != self.radix
                || g.n_fe != number_format_as_int(self.n_fe)
                || !g.b_use_sep
                || g.numwidth != Some(self.numwidth)
                || g.f_int_math != self.f_integer_mode
                || g.b_record != self.b_record
        });

        if changed {
            GLD_PREVIOUS.with(|g| {
                let mut g = g.borrow_mut();
                g.precision = self.precision;
                g.radix = self.radix;
                g.n_fe = number_format_as_int(self.n_fe);
                g.numwidth = Some(self.numwidth);

                g.f_int_math = self.f_integer_mode;
                g.b_record = self.b_record;
                g.b_use_sep = true;
            });

            if self.b_record {
                // Display the string and return.
                self.number_string = self.input.to_string(self.radix);
            } else {
                // If we're in Programmer mode, perform integer truncation so e.g. 5 / 2 * 2 results in 4, not 5.
                if self.f_integer_mode {
                    self.current_val = self.truncate_num_for_int_math(&self.current_val)?;
                }
                self.number_string = self.get_string_for_display(&self.current_val, self.radix)?;
            }

            // Displayed number can go through transformation. So copy it after transformation
            let value = self.current_val.clone();
            GLD_PREVIOUS.with(|g| g.borrow_mut().value = value);

            if (self.radix == 10)
                && self.is_number_invalid(
                    &self.number_string,
                    MAX_EXPONENT,
                    self.precision,
                    self.radix,
                ) != 0
            {
                self.display_error(ratpack::CALC_E_OVERFLOW);
            } else {
                // Display the string and return.
                let grouped = self.group_digits_per_radix(&self.number_string, self.radix);
                self.set_primary_display(&grouped, false);
            }
        }
        Ok(())
    }

    /// `CCalcEngine::IsNumberInvalid`
    ///
    /// For radix 10 this hand-implements the full match of
    /// `[+-]?(\d*)[<dec>]?(\d*)(?:e[+-]?(\d*))?$`.
    pub fn is_number_invalid(
        &self,
        number_string: &str,
        i_max_exp: i32,
        i_max_mantissa: i32,
        radix: u32,
    ) -> i32 {
        let mut i_error = 0;

        if radix == 10 {
            // start with an optional + or -
            // followed by zero or more digits
            // followed by an optional decimal point
            // followed by zero or more digits
            // followed by an optional exponent
            // in case there's an exponent:
            //      its optionally followed by a + or -
            //      which is followed by zero or more digits
            let s: Vec<char> = number_string.chars().collect();
            let mut i = 0;
            if i < s.len() && (s[i] == '+' || s[i] == '-') {
                i += 1;
            }
            let g1_start = i;
            while i < s.len() && s[i].is_ascii_digit() {
                i += 1;
            }
            let g1 = &s[g1_start..i];
            if i < s.len() && s[i] == self.decimal_separator {
                i += 1;
            }
            let g2_start = i;
            while i < s.len() && s[i].is_ascii_digit() {
                i += 1;
            }
            let g2_len = i - g2_start;
            let mut g3_len = 0;
            if i < s.len() && s[i] == 'e' {
                i += 1;
                if i < s.len() && (s[i] == '+' || s[i] == '-') {
                    i += 1;
                }
                let g3_start = i;
                while i < s.len() && s[i].is_ascii_digit() {
                    i += 1;
                }
                g3_len = i - g3_start;
            }

            if i == s.len() {
                // Check that exponent isn't too long
                if g3_len as i64 > i64::from(i_max_exp) {
                    i_error = IDS_ERR_INPUT_OVERFLOW;
                } else {
                    let leading_zeros = g1.iter().take_while(|&&c| c == '0').count();
                    let i_mantissa = (g1.len() - leading_zeros) + g2_len;
                    if i_mantissa as i64 > i64::from(i_max_mantissa) {
                        i_error = IDS_ERR_INPUT_OVERFLOW;
                    }
                }
            } else {
                i_error = IDS_ERR_UNK_CH;
            }
        } else {
            for c in number_string.chars() {
                if radix == 16 {
                    if !(c.is_ascii_digit() || ('A'..='F').contains(&c)) {
                        i_error = IDS_ERR_UNK_CH;
                    }
                } else if (c as u32) < ('0' as u32) || (c as u32) >= ('0' as u32) + radix {
                    i_error = IDS_ERR_UNK_CH;
                }
            }
        }

        i_error
    }

    /****************************************************************************\
    *
    * DigitGroupingStringToGroupingVector
    *
    * Description:
    *   This will take the digit grouping string found in the regional applet and
    *   represent this string as a vector.
    *
    *   groupingString
    *   0;0      - no grouping
    *   3;0      - group every 3 digits
    *   3        - group 1st 3, then no grouping after
    *   3;0;0    - group 1st 3, then no grouping after
    *   3;2;0    - group 1st 3 and then every 2 digits
    *   4;0      - group every 4 digits
    *   5;3;2;0  - group 5, then 3, then every 2
    *   5;3;2    - group 5, then 3, then 2, then no grouping after
    *
    * Returns: the groupings as a vector
    *
    \****************************************************************************/
    pub fn digit_grouping_string_to_grouping_vector(grouping_string: &str) -> Vec<u32> {
        let s: Vec<char> = grouping_string.chars().collect();
        let length = s.len();
        let mut grouping = Vec::new();
        let mut itr = 0usize;
        while itr != length {
            // Try to parse a grouping number from the string
            let (value, next) = wcstoul10(&s, itr);
            let current_group = value as u32;

            // If we successfully parsed a group, add it to the grouping.
            if current_group < MAX_GROUPING_SIZE {
                grouping.push(current_group);
            }

            // If we found a grouping and aren't at the end of the string yet,
            // jump to the next position in the string (the ';').
            // The loop will then increment us to the next character, which should be a number.
            if next < length {
                itr = next;
            }
            itr += 1;
        }

        grouping
    }

    pub fn group_digits_per_radix(&self, number_string: &str, radix: u32) -> String {
        if number_string.is_empty() {
            return String::new();
        }

        match radix {
            10 => self.group_digits(
                &self.group_separator.to_string(),
                &self.dec_grouping,
                number_string,
                number_string.starts_with('-'),
            ),
            8 => self.group_digits(" ", &[3, 0], number_string, false),
            2 | 16 => self.group_digits(" ", &[4, 0], number_string, false),
            _ => number_string.to_string(),
        }
    }

    /****************************************************************************\
    *
    * GroupDigits
    *
    * Description:
    *   This routine will take a grouping vector and the display string and
    *   add the separator according to the pattern indicated by the separator.
    *
    *   Grouping
    *   0,0      - no grouping
    *   3,0      - group every 3 digits
    *   3        - group 1st 3, then no grouping after
    *   3,0,0    - group 1st 3, then no grouping after
    *   3,2,0    - group 1st 3 and then every 2 digits
    *   4,0      - group every 4 digits
    *   5,3,2,0  - group 5, then 3, then every 2
    *   5,3,2    - group 5, then 3, then 2, then no grouping after
    *
    \***************************************************************************/
    pub fn group_digits(
        &self,
        delimiter: &str,
        grouping: &[u32],
        display_string: &str,
        is_num_negative: bool,
    ) -> String {
        // if there's nothing to do, bail
        if delimiter.is_empty() || grouping.is_empty() {
            return display_string.to_string();
        }

        let display: Vec<char> = display_string.chars().collect();

        // Find the position of exponential 'e' in the string
        let exp = display.iter().position(|&c| c == 'e');

        // Find the position of decimal point in the string
        let dec = display.iter().position(|&c| c == self.decimal_separator);

        // Create an iterator that points to the end of the portion of the number subject to grouping (i.e. left of the decimal)
        // (`idx` is the number of characters left of the reverse iterator.)
        let mut idx = if let Some(d) = dec {
            d
        } else if let Some(e) = exp {
            e
        } else {
            display.len()
        };

        let mut result: Vec<char> = Vec::new();
        let mut grouping_size: u32 = 0;

        let mut group_itr = 0usize;
        let mut curr_grouping = grouping[0];
        // Mark the 'end' of the string as either rend() or rend()-1 if there is a negative sign
        // We exclude the sign here because we don't want to end up with e.g. "-,123,456"
        // Then, iterate from back to front, adding group delimiters as needed.
        let reverse_end = if is_num_negative { 1 } else { 0 };
        while idx > reverse_end {
            idx -= 1;
            result.push(display[idx]);
            grouping_size += 1;

            // If a group is complete, add a separator
            // Do not add a separator if:
            // - grouping size is 0
            // - we are at the end of the digit string
            if curr_grouping != 0
                && grouping_size.is_multiple_of(curr_grouping)
                && idx != reverse_end
            {
                // (C++ appends the delimiter as-is and later reverses the whole
                // result, so a multi-character delimiter ends up reversed.)
                result.extend(delimiter.chars());
                grouping_size = 0; // reset for a new group

                // Shift the grouping to next values if they exist
                if group_itr != grouping.len() {
                    group_itr += 1;

                    // Loop through grouping vector until we find a non-zero value.
                    // "0" values may appear in a form of either e.g. "3;0" or "3;0;0".
                    // A 0 in the last position means repeat the previous grouping.
                    // A 0 in another position is a group. So, "3;0;0" means "group 3, then group 0 repeatedly"
                    // This could be expressed as just "3" but GetLocaleInfo is returning 3;0;0 in some cases instead.
                    curr_grouping = 0;
                    while group_itr != grouping.len() {
                        // If it's a non-zero value, that's our new group
                        if grouping[group_itr] != 0 {
                            curr_grouping = grouping[group_itr];
                            break;
                        }

                        // Otherwise, save the previous grouping in case we need to repeat it
                        curr_grouping = grouping[group_itr - 1];
                        group_itr += 1;
                    }
                }
            }
        }

        // now copy the negative sign if it is there
        if is_num_negative {
            result.push(display[0]);
        }

        result.reverse();
        // Add the right (fractional or exponential) part of the number to the final string.
        if let Some(d) = dec {
            result.extend_from_slice(&display[d..]);
        } else if let Some(e) = exp {
            result.extend_from_slice(&display[e..]);
        }

        result.into_iter().collect()
    }
}
