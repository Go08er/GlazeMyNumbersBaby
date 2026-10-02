// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

/**************************************************************************/
/*** SCICALC Scientific Calculator for Windows 3.00.12                  ***/
/*** (c)1989 Microsoft Corporation.  All Rights Reserved.               ***/
/***                                                                    ***/
/*** scifunc.c                                                          ***/
/***                                                                    ***/
/*** Functions contained:                                               ***/
/***    SciCalcFunctions--do sin, cos, tan, com, log, ln, rec, fac, etc.***/
/***    DisplayError--Error display driver.                             ***/
/***                                                                    ***/
/*** Functions called:                                                  ***/
/***    SciCalcFunctions call DisplayError.                             ***/
/***                                                                    ***/
/***                                                                    ***/
/**************************************************************************/

use ratpack::rational_math::*;
use ratpack::{CalcErr, CalcResult, Rational};

use super::CalcEngine;
use crate::ccommand::*;
use crate::engine_strings::IDS_ERRORS_FIRST;

/// `SCODE_CODE(sc)`
fn scode_code(sc: CalcErr) -> i32 {
    (sc & 0xFFFF) as i32
}

fn r(i: i32) -> Rational {
    Rational::from(i)
}

impl CalcEngine {
    /// Routines for more complex mathematical functions/error checking.
    ///
    /// Mirrors the C++ `try { ... } catch (uint32_t nErrCode) { DisplayError(nErrCode); result = rat; }`.
    ///
    /// The only C++ call site is `m_currentVal = SciCalcFunctions(m_currentVal, op)`, and `rat` is a
    /// `const&` that aliases `m_currentVal`: when `IDC_DEGREES` re-enters `ProcessCommand(IDC_INV)`,
    /// pending input is committed to `m_currentVal` and `rat` observes the new value. So `rat` is
    /// read from `self.current_val` here rather than passed in.
    pub(super) fn sci_calc_functions(&mut self, op: u32) -> Rational {
        match self.sci_calc_functions_try(op) {
            Ok(result) => result,
            Err(n_err_code) => {
                self.display_error(n_err_code);
                self.current_val.clone()
            }
        }
    }

    fn sci_calc_functions_try(&mut self, op: u32) -> CalcResult<Rational> {
        let mut rat_value = self.current_val.clone();
        let mut result = Rational::default();
        let angletype = self.angletype;
        let rat = &rat_value;
        match op as i32 {
            IDC_CHOP => {
                result = if self.b_inv {
                    frac(rat)?
                } else {
                    integer(rat)?
                };
            }

            /* Return complement. */
            IDC_COM => {
                if self.radix == 10 && !self.f_integer_mode {
                    result = -(integer(rat)?.add(&r(1))?);
                } else {
                    result = rat.bitxor(&self.get_chop_number())?;
                }
            }

            IDC_ROL | IDC_ROLC => {
                if self.f_integer_mode {
                    result = integer(rat)?;

                    let mut w64_bits = result.to_u64()?;
                    let msb = (w64_bits >> (self.dw_word_bit_width - 1)) & 1;
                    w64_bits <<= 1; // LShift by 1

                    if op as i32 == IDC_ROL {
                        w64_bits |= msb; // Set the prev Msb as the current Lsb
                    } else {
                        w64_bits |= self.carry_bit; // Set the carry bit as the LSB
                        self.carry_bit = msb; // Store the msb as the next carry bit
                    }

                    result = Rational::from(w64_bits);
                }
            }

            IDC_ROR | IDC_RORC => {
                if self.f_integer_mode {
                    result = integer(rat)?;

                    let mut w64_bits = result.to_u64()?;
                    let lsb: u64 = if (w64_bits & 0x01) == 1 { 1 } else { 0 };
                    w64_bits >>= 1; // RShift by 1

                    if op as i32 == IDC_ROR {
                        w64_bits |= lsb << (self.dw_word_bit_width - 1);
                    } else {
                        w64_bits |= self.carry_bit << (self.dw_word_bit_width - 1);
                        self.carry_bit = lsb;
                    }

                    result = Rational::from(w64_bits);
                }
            }

            IDC_PERCENT => {
                // If the operator is multiply/divide, we evaluate this as "X [op] (Y%)"
                // Otherwise, we evaluate it as "X [op] (X * Y%)"
                if self.n_op_code == IDC_MUL || self.n_op_code == IDC_DIV {
                    result = rat.div(&r(100))?;
                } else {
                    result = rat.mul(&self.last_val.div(&r(100))?)?;
                }
            }

            IDC_SIN => {
                /* Sine; normal and arc */
                if !self.f_integer_mode {
                    result = if self.b_inv {
                        asin(rat, angletype)?
                    } else {
                        sin(rat, angletype)?
                    };
                }
            }

            IDC_SINH => {
                /* Sine- hyperbolic and archyperbolic */
                if !self.f_integer_mode {
                    result = if self.b_inv { asinh(rat)? } else { sinh(rat)? };
                }
            }

            IDC_COS => {
                /* Cosine, follows convention of sine function. */
                if !self.f_integer_mode {
                    result = if self.b_inv {
                        acos(rat, angletype)?
                    } else {
                        cos(rat, angletype)?
                    };
                }
            }

            IDC_COSH => {
                /* Cosine hyperbolic, follows convention of sine h function. */
                if !self.f_integer_mode {
                    result = if self.b_inv { acosh(rat)? } else { cosh(rat)? };
                }
            }

            IDC_TAN => {
                /* Same as sine and cosine. */
                if !self.f_integer_mode {
                    result = if self.b_inv {
                        atan(rat, angletype)?
                    } else {
                        tan(rat, angletype)?
                    };
                }
            }

            IDC_TANH => {
                /* Same as sine h and cosine h. */
                if !self.f_integer_mode {
                    result = if self.b_inv { atanh(rat)? } else { tanh(rat)? };
                }
            }

            IDC_SEC => {
                if !self.f_integer_mode {
                    result = if self.b_inv {
                        acos(&invert(rat)?, angletype)?
                    } else {
                        invert(&cos(rat, angletype)?)?
                    };
                }
            }

            IDC_CSC => {
                if !self.f_integer_mode {
                    result = if self.b_inv {
                        asin(&invert(rat)?, angletype)?
                    } else {
                        invert(&sin(rat, angletype)?)?
                    };
                }
            }

            IDC_COT => {
                if !self.f_integer_mode {
                    result = if self.b_inv {
                        atan(&invert(rat)?, angletype)?
                    } else {
                        invert(&tan(rat, angletype)?)?
                    };
                }
            }

            IDC_SECH => {
                if !self.f_integer_mode {
                    result = if self.b_inv {
                        acosh(&invert(rat)?)?
                    } else {
                        invert(&cosh(rat)?)?
                    };
                }
            }

            IDC_CSCH => {
                if !self.f_integer_mode {
                    result = if self.b_inv {
                        asinh(&invert(rat)?)?
                    } else {
                        invert(&sinh(rat)?)?
                    };
                }
            }

            IDC_COTH => {
                if !self.f_integer_mode {
                    result = if self.b_inv {
                        atanh(&invert(rat)?)?
                    } else {
                        invert(&tanh(rat)?)?
                    };
                }
            }

            IDC_REC => {
                /* Reciprocal. */
                result = invert(rat)?;
            }

            IDC_SQR => {
                /* Square */
                result = pow(rat, &r(2))?;
            }

            IDC_SQRT => {
                /* Square Root */
                result = root(rat, &r(2))?;
            }

            IDC_CUBEROOT | IDC_CUB => {
                /* Cubing and cube root functions. */
                result = if IDC_CUBEROOT == op as i32 {
                    root(rat, &r(3))?
                } else {
                    pow(rat, &r(3))?
                };
            }

            IDC_LOG => {
                /* Functions for common log. */
                result = log10(rat)?;
            }

            IDC_POW10 => {
                result = pow(&r(10), rat)?;
            }

            IDC_POW2 => {
                result = pow(&r(2), rat)?;
            }

            IDC_LN => {
                /* Functions for natural log. */
                result = if self.b_inv { exp(rat)? } else { log(rat)? };
            }

            IDC_FAC => {
                /* Calculate factorial.  Inverse is ineffective. */
                result = fact(rat)?;
            }

            IDC_DEGREES | IDC_DMS => {
                if op as i32 == IDC_DEGREES {
                    self.process_command(IDC_INV)?;
                    // `rat` aliases m_currentVal, which ProcessCommand(IDC_INV) may have updated.
                    rat_value = self.current_val.clone();
                    // This case falls through to IDC_DMS case because in the old Win32 Calc,
                    // the degrees functionality was achieved as 'Inv' of 'dms' operation,
                    // so setting the IDC_INV command first and then performing 'dms' operation as global variables m_bInv, m_bRecord
                    // are set properly through ProcessCommand(IDC_INV)
                }

                let rat = &rat_value;
                if !self.f_integer_mode {
                    let mut shft_rat: i32 = if self.b_inv { 100 } else { 60 };

                    let degree_rat = integer(rat)?;

                    let mut minute_rat = rat.sub(&degree_rat)?.mul(&r(shft_rat))?;

                    let mut second_rat = minute_rat.clone();

                    minute_rat = integer(&minute_rat)?;

                    second_rat = second_rat.sub(&minute_rat)?.mul(&r(shft_rat))?;

                    //
                    // degreeRat == degrees, minuteRat == minutes, secondRat == seconds
                    //

                    shft_rat = if self.b_inv { 60 } else { 100 };
                    second_rat = second_rat.div(&r(shft_rat))?;

                    minute_rat = minute_rat.add(&second_rat)?.div(&r(shft_rat))?;

                    result = degree_rat.add(&minute_rat)?;
                }
            }
            IDC_CEIL => {
                result = if frac(rat)? > r(0) {
                    integer(&rat.add(&r(1))?)?
                } else {
                    integer(rat)?
                };
            }

            IDC_FLOOR => {
                result = if frac(rat)? < r(0) {
                    integer(&rat.sub(&r(1))?)?
                } else {
                    integer(rat)?
                };
            }

            IDC_ABS => {
                result = abs(rat)?;
            }

            _ => {}
        } // end switch( op )

        Ok(result)
    }

    /// Routine to display error messages and set m_bError flag.  Errors are
    /// called with DisplayError (n), where n is a uint32_t   between 0 and 5.
    pub fn display_error(&mut self, n_error: CalcErr) {
        let error_string = Self::get_string_id(IDS_ERRORS_FIRST + scode_code(n_error));

        self.set_primary_display(&error_string, true /*isError*/);

        self.b_error = true; /* Set error flag.  Only cleared with CLEAR or CENTR. */

        self.history_collector.clear_history_line(&error_string);
    }
}
