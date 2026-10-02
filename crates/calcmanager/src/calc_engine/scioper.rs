// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CEngine/scioper.cpp`.

use ratpack::rational_math::*;
use ratpack::{CALC_E_NORESULT, CalcResult, Rational};

use super::CalcEngine;
use crate::ccommand::*;

impl CalcEngine {
    /// Routines to perform standard operations &|^~<<>>+-/*% and pwr.
    ///
    /// Mirrors the C++ `try { ... } catch (uint32_t dwErrCode) { DisplayError(dwErrCode); result = lhs; }`.
    pub(super) fn do_operation(
        &mut self,
        operation: i32,
        lhs: &Rational,
        rhs: &Rational,
    ) -> Rational {
        // Remove any variance in how 0 could be represented in rat e.g. -0, 0/n, etc.
        let result = if *lhs != Rational::from(0) {
            lhs.clone()
        } else {
            Rational::from(0)
        };

        match self.do_operation_try(operation, result, rhs) {
            Ok(r) => r,
            Err(dw_err_code) => {
                self.display_error(dw_err_code);

                // On error, return the original value
                lhs.clone()
            }
        }
    }

    fn do_operation_try(
        &mut self,
        operation: i32,
        mut result: Rational,
        rhs: &Rational,
    ) -> CalcResult<Rational> {
        match operation {
            IDC_AND => {
                result = result.bitand(rhs)?;
            }

            IDC_OR => {
                result = result.bitor(rhs)?;
            }

            IDC_XOR => {
                result = result.bitxor(rhs)?;
            }

            IDC_NAND => {
                result = result.bitand(rhs)?.bitxor(&self.get_chop_number())?;
            }

            IDC_NOR => {
                result = result.bitor(rhs)?.bitxor(&self.get_chop_number())?;
            }

            IDC_RSHF => {
                if self.f_integer_mode && result >= Rational::from(self.dw_word_bit_width) {
                    // Lsh/Rsh >= than current word size is always 0
                    return Err(CALC_E_NORESULT);
                }

                let w64_bits = rhs.to_u64()?;
                let f_msb = ((w64_bits >> (self.dw_word_bit_width - 1)) & 1) != 0;

                let hold_val = result.clone();
                result = rhs.shr(&hold_val)?;

                if f_msb {
                    result = integer(&result)?;

                    let mut temp_rat = self.get_chop_number().shr(&hold_val)?;
                    temp_rat = integer(&temp_rat)?;

                    result = result.bitor(&temp_rat.bitxor(&self.get_chop_number())?)?;
                }
            }
            IDC_RSHFL => {
                if self.f_integer_mode && result >= Rational::from(self.dw_word_bit_width) {
                    // Lsh/Rsh >= than current word size is always 0
                    return Err(CALC_E_NORESULT);
                }

                result = rhs.shr(&result)?;
            }
            IDC_LSHF => {
                if self.f_integer_mode && result >= Rational::from(self.dw_word_bit_width) {
                    // Lsh/Rsh >= than current word size is always 0
                    return Err(CALC_E_NORESULT);
                }

                result = rhs.shl(&result)?;
            }

            IDC_ADD => {
                result = result.add(rhs)?;
            }

            IDC_SUB => {
                result = rhs.sub(&result)?;
            }

            IDC_MUL => {
                result = result.mul(rhs)?;
            }

            IDC_DIV | IDC_MOD => {
                let mut i_numerator_sign = 1;
                let mut i_denominator_sign = 1;
                let mut temp = result;
                result = rhs.clone();

                if self.f_integer_mode {
                    let mut w64_bits = rhs.to_u64()?;
                    let mut f_msb = ((w64_bits >> (self.dw_word_bit_width - 1)) & 1) != 0;

                    if f_msb {
                        result = rhs
                            .bitxor(&self.get_chop_number())?
                            .add(&Rational::from(1))?;

                        i_numerator_sign = -1;
                    }

                    w64_bits = temp.to_u64()?;
                    f_msb = ((w64_bits >> (self.dw_word_bit_width - 1)) & 1) != 0;

                    if f_msb {
                        temp = temp
                            .bitxor(&self.get_chop_number())?
                            .add(&Rational::from(1))?;

                        i_denominator_sign = -1;
                    }
                }

                if operation == IDC_DIV {
                    result = result.div(&temp)?;
                    if self.f_integer_mode && (i_numerator_sign * i_denominator_sign) == -1 {
                        result = -(integer(&result)?);
                    }
                } else if self.f_integer_mode {
                    // Programmer mode, use remrat (remainder after division)
                    result = result.rem(&temp)?;

                    if i_numerator_sign == -1 {
                        result = -(integer(&result)?);
                    }
                } else {
                    // other modes, use modrat (modulus after division)
                    result = modulo(&result, &temp)?;
                }
            }

            IDC_PWR => {
                // Calculates rhs to the result(th) power.
                result = pow(rhs, &result)?;
            }

            IDC_ROOT => {
                // Calculates rhs to the result(th) root.
                result = root(rhs, &result)?;
            }

            IDC_LOGBASEY => {
                result = log(rhs)?.div(&log(&result)?)?;
            }

            _ => {}
        }

        Ok(result)
    }
}
