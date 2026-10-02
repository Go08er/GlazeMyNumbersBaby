// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CEngine/scicomm.cpp`.
//!
//! Every `?` in this file corresponds to a point where the C++ code lets a
//! `uint32_t` error code propagate (there is no `catch` in scicomm.cpp).

use ratpack::{AngleType, CalcResult, NumberFormat, Rational};

use super::random::Mt19937;
use super::{CalcEngine, NumWidth, ratpack_pi, ratpack_two_pi};
use crate::calc_utils::*;
use crate::ccommand::*;
use crate::engine_strings::*;
use crate::history::MAXPRECDEPTH;
use crate::radix_type::RadixType;

/// NPrecedenceOfOp
///
/// returns a virtual number for precedence for the operator. We expect binary operator only, otherwise the lowest number
/// 0 is returned. Higher the number, higher the precedence of the operator.
fn n_precedence_of_op(nop_code: i32) -> i32 {
    match nop_code {
        IDC_OR | IDC_XOR => 0,
        IDC_AND | IDC_NAND | IDC_NOR => 1,
        IDC_ADD | IDC_SUB => 2,
        IDC_LSHF | IDC_RSHF | IDC_RSHFL | IDC_MOD | IDC_DIV | IDC_MUL => 3,
        IDC_PWR | IDC_ROOT | IDC_LOGBASEY => 4,
        _ => 0,
    }
}

/// Unary operator Function Name table Element
/// since unary operators button names aren't exactly friendly for history purpose,
/// we have this separate table to get its localized name and for its Inv function if it exists.
struct FunctionNameElement {
    /// Used by default if there are no rad or grad specific strings.
    degree_string: &'static str,
    /// Will fall back to degreeString if empty
    inverse_degree_string: &'static str,

    rad_string: &'static str,
    /// Will fall back to radString if empty
    inverse_rad_string: &'static str,

    grad_string: &'static str,
    /// Will fall back to gradString if empty
    inverse_grad_string: &'static str,

    programmer_mode_string: &'static str,
}

impl FunctionNameElement {
    const fn new(
        degree_string: &'static str,
        inverse_degree_string: &'static str,
        rad_string: &'static str,
        inverse_rad_string: &'static str,
        grad_string: &'static str,
        inverse_grad_string: &'static str,
        programmer_mode_string: &'static str,
    ) -> Self {
        FunctionNameElement {
            degree_string,
            inverse_degree_string,
            rad_string,
            inverse_rad_string,
            grad_string,
            inverse_grad_string,
            programmer_mode_string,
        }
    }

    const fn d(degree_string: &'static str) -> Self {
        Self::new(degree_string, "", "", "", "", "", "")
    }

    const fn di(degree_string: &'static str, inverse_degree_string: &'static str) -> Self {
        Self::new(degree_string, inverse_degree_string, "", "", "", "", "")
    }

    fn has_angle_strings(&self) -> bool {
        !self.rad_string.is_empty()
            || !self.inverse_rad_string.is_empty()
            || !self.grad_string.is_empty()
            || !self.inverse_grad_string.is_empty()
    }
}

/// Table for each unary operator
fn operator_string_table(op: i32) -> Option<FunctionNameElement> {
    type E = FunctionNameElement;
    Some(match op {
        IDC_CHOP => E::di("", SIDS_FRAC),

        IDC_SIN => E::new(
            SIDS_SIND, SIDS_ASIND, SIDS_SINR, SIDS_ASINR, SIDS_SING, SIDS_ASING, "",
        ),
        IDC_COS => E::new(
            SIDS_COSD, SIDS_ACOSD, SIDS_COSR, SIDS_ACOSR, SIDS_COSG, SIDS_ACOSG, "",
        ),
        IDC_TAN => E::new(
            SIDS_TAND, SIDS_ATAND, SIDS_TANR, SIDS_ATANR, SIDS_TANG, SIDS_ATANG, "",
        ),

        IDC_SINH => E::di("", SIDS_ASINH),
        IDC_COSH => E::di("", SIDS_ACOSH),
        IDC_TANH => E::di("", SIDS_ATANH),

        IDC_SEC => E::new(
            SIDS_SECD, SIDS_ASECD, SIDS_SECR, SIDS_ASECR, SIDS_SECG, SIDS_ASECG, "",
        ),
        IDC_CSC => E::new(
            SIDS_CSCD, SIDS_ACSCD, SIDS_CSCR, SIDS_ACSCR, SIDS_CSCG, SIDS_ACSCG, "",
        ),
        IDC_COT => E::new(
            SIDS_COTD, SIDS_ACOTD, SIDS_COTR, SIDS_ACOTR, SIDS_COTG, SIDS_ACOTG, "",
        ),

        IDC_SECH => E::di(SIDS_SECH, SIDS_ASECH),
        IDC_CSCH => E::di(SIDS_CSCH, SIDS_ACSCH),
        IDC_COTH => E::di(SIDS_COTH, SIDS_ACOTH),

        IDC_LN => E::di("", SIDS_POWE),
        IDC_SQR => E::d(SIDS_SQR),
        IDC_CUB => E::d(SIDS_CUBE),
        IDC_FAC => E::d(SIDS_FACT),
        IDC_REC => E::d(SIDS_RECIPROC),
        IDC_DMS => E::di("", SIDS_DEGREES),
        IDC_SIGN => E::d(SIDS_NEGATE),
        IDC_DEGREES => E::d(SIDS_DEGREES),
        IDC_POW2 => E::d(SIDS_TWOPOWX),
        IDC_LOGBASEY => E::d(SIDS_LOGBASEY),
        IDC_ABS => E::d(SIDS_ABS),
        IDC_CEIL => E::d(SIDS_CEIL),
        IDC_FLOOR => E::d(SIDS_FLOOR),
        IDC_NAND => E::d(SIDS_NAND),
        IDC_NOR => E::d(SIDS_NOR),
        IDC_RSHFL => E::d(SIDS_RSH),
        IDC_RORC => E::d(SIDS_ROR),
        IDC_ROLC => E::d(SIDS_ROL),
        IDC_CUBEROOT => E::d(SIDS_CUBEROOT),
        IDC_MOD => E::new(SIDS_MOD, "", "", "", "", "", SIDS_PROGRAMMER_MOD),
        _ => return None,
    })
}

impl CalcEngine {
    /// HandleErrorCommand
    ///
    /// When it is discovered by the state machine that at this point the input is not valid (eg. "1+)"), we want to proceed as though this input never
    /// occurred and may be some feedback to user like Beep. The rest of input can then continue by just ignoring this command.
    pub(super) fn handle_error_command(&mut self, idc: OpCode) {
        if !is_gui_setting_op_code(idc) {
            // We would have saved the prev command. Need to forget this state
            self.n_temp_com = self.n_last_com;
        }
    }

    pub(super) fn handle_max_digits_reached(&mut self) {
        if let Some(d) = &self.calc_display {
            d.borrow_mut().max_digits_reached();
        }
    }

    pub(super) fn clear_temporary_values(&mut self) -> CalcResult<()> {
        self.b_inv = false;
        self.input.clear();
        self.b_record = true;
        self.check_and_add_last_bin_op_to_history(true)?;
        self.display_num()?;
        self.b_error = false;
        Ok(())
    }

    pub(super) fn clear_display(&mut self) {
        if let Some(d) = &self.calc_display {
            d.borrow_mut().set_expression_display(&[], &[]);
        }
    }

    /// `CCalcEngine::ProcessCommand`
    pub fn process_command(&mut self, w_param: OpCode) -> CalcResult<()> {
        let mut w_param = w_param;
        if w_param == IDC_SET_RESULT {
            w_param = IDC_RECALL;
            self.b_set_calc_state = true;
        }

        self.process_command_worker(w_param)
    }

    /// Shared body of the "implicit multiplication after a closing
    /// parenthesis" blocks (digit and decimal point paths).
    fn implicit_multiplication_after_close_paren(&mut self) {
        // Treat this as an implicit multiplication
        self.n_op_code = IDC_MUL;
        self.last_val = self.current_val.clone();

        // We need to clear any previous state from last calculation
        self.hold_val = Rational::from(0);
        self.b_no_prev_equ = true;

        // Add the operand to history before adding the implicit multiplication
        if !self.history_collector.f_opnd_added_to_history() {
            self.history_collector.add_open_brace_to_history();
            self.history_collector.add_opnd_to_history(
                &self.number_string,
                &self.current_val,
                false,
            );
            self.history_collector.add_close_brace_to_history();
        }

        // Add the implicit multiplication to history
        self.history_collector
            .add_bin_op_to_history(self.n_op_code, self.f_integer_mode, true);

        self.b_change_op = true;
        self.n_prev_op_code = 0;

        // Clear any pending operations in the precedence stack
        while self.precedence_op_count > 0 {
            self.precedence_op_count -= 1;
            self.n_prec_op[self.precedence_op_count] = 0;
        }
    }

    fn process_command_worker(&mut self, w_param: OpCode) -> CalcResult<()> {
        let mut w_param = w_param;

        // Save the last command.  Some commands are not saved in this manor, these
        // commands are:
        // Inv, Deg, Rad, Grad, Stat, FE, MClear, Back, and Exp.  The excluded
        // commands are not
        // really mathematical operations, rather they are GUI mode settings.

        if !is_gui_setting_op_code(w_param) {
            self.n_last_com = self.n_temp_com;
            self.n_temp_com = w_param;
        }

        // Clear expression shown after = sign, when user do any action.
        if !self.b_no_prev_equ {
            self.clear_display();
        }

        if self.b_error {
            if w_param == IDC_CLEAR {
                // handle "C" normally
            } else if w_param == IDC_CENTR {
                // treat "CE" as "C"
                w_param = IDC_CLEAR;
            } else {
                self.handle_error_command(w_param);
                return Ok(());
            }
        }

        // Toggle Record/Display mode if appropriate.
        if self.b_record {
            if is_bin_op_code(w_param)
                || is_unary_op_code(w_param)
                || is_op_in_range(w_param, IDC_FE, IDC_MMINUS)
                || is_op_in_range(w_param, IDC_OPENP, IDC_CLOSEP)
                || is_op_in_range(w_param, IDM_HEX, IDM_BIN)
                || is_op_in_range(w_param, IDM_QWORD, IDM_BYTE)
                || is_op_in_range(w_param, IDM_DEG, IDM_GRAD)
                || is_op_in_range(w_param, IDC_BINEDITSTART, IDC_BINEDITEND)
                || (IDC_INV == w_param)
                || (IDC_SIGN == w_param && 10 != self.radix)
                || (IDC_RAND == w_param)
                || (IDC_EULER == w_param)
            {
                self.b_record = false;
                self.current_val = self.input.to_rational(self.radix, self.precision)?;
                self.display_num()?; // Causes 3.000 to shrink to 3. on first op.
            }
        } else if is_digit_op_code(w_param) || w_param == IDC_PNT {
            self.b_record = true;
            self.input.clear();

            /*
             * Account for scenarios where an equation includes any input after closing parenthesis - i.e. "(8)2=16".
             * This prevents the calculator from ending an equation and adding to history prematurely.
             */
            if self.n_last_com != IDC_CLOSEP {
                self.check_and_add_last_bin_op_to_history(true)?;
            }
        }

        // Interpret digit keys.
        if is_digit_op_code(w_param) {
            let i_value = (w_param - IDC_0) as u32;

            // this is redundant, illegal keys are disabled
            if i_value >= self.radix {
                self.handle_error_command(w_param);
                return Ok(());
            }

            let max_str = self.get_max_decimal_value_string();
            if !self.input.try_add_digit(
                i_value,
                self.radix,
                self.f_integer_mode,
                &max_str,
                self.dw_word_bit_width,
                self.c_int_digits_sav,
            ) {
                self.handle_error_command(w_param);
                self.handle_max_digits_reached();
                return Ok(());
            }

            // Check if the last command was a closing parenthesis
            if self.n_last_com == IDC_CLOSEP {
                self.implicit_multiplication_after_close_paren();
            }
            self.display_num()?;

            return Ok(());
        }

        // BINARY OPERATORS:
        if is_bin_op_code(w_param) {
            // Change the operation if last input was operation.
            if is_bin_op_code(self.n_last_com) {
                let mut f_prec_inv_to_higher = false; // Is Precedence Inversion from lower to higher precedence happening ??

                self.n_op_code = w_param;

                // Check to see if by changing this binop, a Precedence inversion is happening.
                // Eg. 1 * 2  + and + is getting changed to ^. The previous precedence rules would have already computed
                // 1*2, so we will put additional brackets to cover for precedence inversion and it will become (1 * 2) ^
                // Here * is m_nPrevOpCode, m_currentVal is 2  (by 1*2), m_nLastCom is +, m_nOpCode is ^
                if self.f_precedence && 0 != self.n_prev_op_code {
                    let n_prev = n_precedence_of_op(self.n_prev_op_code);
                    let nx = n_precedence_of_op(self.n_last_com);
                    let ni = n_precedence_of_op(self.n_op_code);
                    if nx <= n_prev && ni > n_prev {
                        // condition for Precedence Inversion
                        f_prec_inv_to_higher = true;
                        self.n_prev_op_code = 0; // Once the precedence inversion has put additional brackets, its no longer required
                    }
                }
                self.history_collector.change_last_bin_op(
                    self.n_op_code,
                    f_prec_inv_to_higher,
                    self.f_integer_mode,
                )?;
                self.display_announce_binary_operator();
                return Ok(());
            }

            if !self.history_collector.f_opnd_added_to_history() {
                // if the prev command was ) or unop then it is already in history as a opnd form (...)
                self.history_collector.add_opnd_to_history(
                    &self.number_string,
                    &self.current_val,
                    false,
                );
            }

            /* m_bChangeOp is true if there was an operation done and the   */
            /* current m_currentVal is the result of that operation.  This is so */
            /* entering 3+4+5= gives 7 after the first + and 12 after the */
            /* the =.  The rest of this stuff attempts to do precedence in*/
            /* Scientific mode.                                           */
            if self.b_change_op {
                // DoPrecedenceCheckAgain:
                loop {
                    let mut nx = n_precedence_of_op(w_param);
                    let ni = n_precedence_of_op(self.n_op_code);

                    if (nx > ni) && self.f_precedence {
                        if self.precedence_op_count < MAXPRECDEPTH {
                            self.precedence_vals[self.precedence_op_count] = self.last_val.clone();

                            self.n_prec_op[self.precedence_op_count] = self.n_op_code;
                            self.history_collector.push_last_opnd_start(-1); // Eg. 1 + 2  *, Need to remember the start of 2 to do Precedence inversion if need to
                        } else {
                            self.precedence_op_count = MAXPRECDEPTH - 1;
                            self.handle_error_command(w_param);
                        }
                        self.precedence_op_count += 1;
                    } else {
                        /* do the last operation and then if the precedence array is not
                         * empty or the top is not the '(' demarcator then pop the top
                         * of the array and recheck precedence against the new operator
                         */
                        let (cur, last) = (self.current_val.clone(), self.last_val.clone());
                        self.current_val = self.do_operation(self.n_op_code, &cur, &last);
                        self.n_prev_op_code = self.n_op_code;

                        if !self.b_error {
                            self.display_num()?;
                            if !self.f_precedence {
                                let grouped_string =
                                    self.group_digits_per_radix(&self.number_string, self.radix);
                                self.history_collector.complete_equation(&grouped_string);
                                self.history_collector.add_opnd_to_history(
                                    &self.number_string,
                                    &self.current_val,
                                    false,
                                );
                            }
                        }

                        if (self.precedence_op_count != 0)
                            && (self.n_prec_op[self.precedence_op_count - 1] != 0)
                        {
                            self.precedence_op_count -= 1;
                            self.n_op_code = self.n_prec_op[self.precedence_op_count];

                            self.last_val = self.precedence_vals[self.precedence_op_count].clone();

                            nx = n_precedence_of_op(self.n_op_code);
                            // Precedence Inversion Higher to lower can happen which needs explicit enclosure of brackets
                            // Eg.  1 + 2 * Or 3 Or.  We would have pushed 1+ before, and now last + forces 2 Or 3 to be evaluated
                            // because last Or is less or equal to first + (after 1). But we see that 1+ is in stack and we evaluated to 2 Or 3
                            // This is precedence inversion happened because of operator changed in between. We put extra brackets like
                            // 1 + (2 Or 3)
                            if ni <= nx {
                                self.history_collector.enclose_prec_inversion_brackets();
                            }
                            self.history_collector.pop_last_opnd_start();
                            continue; // goto DoPrecedenceCheckAgain;
                        }
                    }
                    break;
                }
            }

            self.display_announce_binary_operator();
            self.last_val = self.current_val.clone();
            self.n_op_code = w_param;
            self.history_collector
                .add_bin_op_to_history(self.n_op_code, self.f_integer_mode, true);
            self.b_no_prev_equ = true;
            self.b_change_op = true;
            return Ok(());
        }

        // UNARY OPERATORS:
        if is_unary_op_code(w_param) || (w_param == IDC_DEGREES) {
            /* Functions are unary operations.                            */
            /* If the last thing done was an operator, m_currentVal was cleared. */
            /* In that case we better use the number before the operator  */
            /* was entered, otherwise, things like 5+ 1/x give Divide By  */
            /* zero.  This way 5+=gives 10 like most calculators do.      */
            if is_bin_op_code(self.n_last_com) {
                self.current_val = self.last_val.clone();
            }

            // we do not add percent sign to history or to two line display.
            // instead, we add the result of applying %.
            if w_param != IDC_PERCENT {
                if !self.history_collector.f_opnd_added_to_history() {
                    self.history_collector.add_opnd_to_history(
                        &self.number_string,
                        &self.current_val,
                        false,
                    );
                }

                self.history_collector
                    .add_unary_op_to_history(w_param, self.b_inv, self.angletype);
            }

            if matches!(
                w_param,
                IDC_SIN
                    | IDC_COS
                    | IDC_TAN
                    | IDC_SINH
                    | IDC_COSH
                    | IDC_TANH
                    | IDC_SEC
                    | IDC_CSC
                    | IDC_COT
                    | IDC_SECH
                    | IDC_CSCH
                    | IDC_COTH
            ) && self.is_current_too_big_for_trig()
            {
                self.current_val = Rational::from(0);
                self.display_error(ratpack::CALC_E_DOMAIN);
                return Ok(());
            }

            self.current_val = self.sci_calc_functions(w_param as u32);

            if self.b_error {
                return Ok(());
            }

            /* Display the result, reset flags, and reset indicators.     */
            self.display_num()?;

            if w_param == IDC_PERCENT {
                self.check_and_add_last_bin_op_to_history(true)?;
                self.history_collector.add_opnd_to_history(
                    &self.number_string,
                    &self.current_val,
                    true, /* Add to primary and secondary display */
                );
            }

            /* reset the m_bInv flag and indicators if it is set
            and have been used */

            if self.b_inv
                && matches!(
                    w_param,
                    IDC_CHOP
                        | IDC_SIN
                        | IDC_COS
                        | IDC_TAN
                        | IDC_LN
                        | IDC_DMS
                        | IDC_DEGREES
                        | IDC_SINH
                        | IDC_COSH
                        | IDC_TANH
                        | IDC_SEC
                        | IDC_CSC
                        | IDC_COT
                        | IDC_SECH
                        | IDC_CSCH
                        | IDC_COTH
                )
            {
                self.b_inv = false;
            }

            return Ok(());
        }

        // Tiny binary edit windows clicked. Toggle that bit and update display
        if is_op_in_range(w_param, IDC_BINEDITSTART, IDC_BINEDITEND) {
            // Same reasoning as for unary operators. We need to seed it previous number
            if is_bin_op_code(self.n_last_com) {
                self.current_val = self.last_val.clone();
            }

            self.check_and_add_last_bin_op_to_history(true)?;

            let mut cur = self.current_val.clone();
            let toggled = self.try_toggle_bit(&mut cur, (w_param - IDC_BINEDITSTART) as u32);
            self.current_val = cur;
            if toggled? {
                self.display_num()?;
            }

            return Ok(());
        }

        /* Now branch off to do other commands and functions.                 */
        match w_param {
            IDC_CLEAR => {
                /* Total clear.                                       */
                if !self.b_change_op {
                    // Preserve history, if everything done before was a series of unary operations.
                    self.check_and_add_last_bin_op_to_history(false)?;
                }

                self.last_val = Rational::from(0);

                self.b_change_op = false;
                self.open_paren_count = 0;
                self.precedence_op_count = 0;
                self.n_temp_com = 0;
                self.n_last_com = 0;
                self.n_op_code = 0;
                self.n_prev_op_code = 0;
                self.b_no_prev_equ = true;
                self.carry_bit = 0;

                /* clear the parenthesis status box indicator, this will not be
                cleared for CENTR */
                if let Some(d) = self.calc_display.clone() {
                    d.borrow_mut().set_parenthesis_number(0);
                    self.clear_display();
                }

                self.history_collector.clear_history_line("");
                self.clear_temporary_values()?;
            }

            IDC_CENTR => {
                /* Clear only temporary values.                       */
                // Clear the INV & leave (=xx indicator active
                self.clear_temporary_values()?;
            }

            IDC_BACK => {
                // Divide number by the current radix and truncate.
                // Only allow backspace if we're recording.
                if self.b_record {
                    self.input.backspace();
                    self.display_num()?;
                } else {
                    self.handle_error_command(w_param);
                }
            }

            /* EQU enables the user to press it multiple times after and      */
            /* operation to enable repeats of the last operation.             */
            IDC_EQU => {
                while self.open_paren_count > 0 {
                    // when m_bError is set and m_ParNum is non-zero it goes into infinite loop
                    if self.b_error {
                        break;
                    }
                    let open_before = self.open_paren_count;
                    // automatic closing of all the parenthesis to get a meaningful result as well as ensure data integrity
                    self.n_temp_com = self.n_last_com; // Put back this last saved command to the prev state so ) can be handled properly
                    self.process_command(IDC_CLOSEP)?;
                    self.n_last_com = self.n_temp_com; // Actually this is IDC_CLOSEP
                    self.n_temp_com = w_param; // put back in the state where last op seen was IDC_CLOSEP, and current op is IDC_EQU
                    if self.open_paren_count >= open_before {
                        // Deviation: the C++ loop spins forever when ")" is rejected
                        // (full precedence stack); stop instead of hanging.
                        break;
                    }
                }

                if !self.b_no_prev_equ {
                    // It is possible now unary op changed the num in screen, but still m_lastVal hasn't changed.
                    self.last_val = self.current_val.clone();
                }

                /* Last thing keyed in was an operator.  Lets do the op on*/
                /* a duplicate of the last entry.                     */
                if is_bin_op_code(self.n_last_com) {
                    self.current_val = self.last_val.clone();
                }

                if !self.history_collector.f_opnd_added_to_history() {
                    self.history_collector.add_opnd_to_history(
                        &self.number_string,
                        &self.current_val,
                        false,
                    );
                }

                // Evaluate the precedence stack.
                self.resolve_highest_precedence_operation()?;
                while self.f_precedence && self.precedence_op_count > 0 {
                    self.precedence_op_count -= 1;
                    self.n_op_code = self.n_prec_op[self.precedence_op_count];
                    self.last_val = self.precedence_vals[self.precedence_op_count].clone();

                    // Precedence Inversion check
                    let ni = n_precedence_of_op(self.n_prev_op_code);
                    let nx = n_precedence_of_op(self.n_op_code);
                    if ni <= nx {
                        self.history_collector.enclose_prec_inversion_brackets();
                    }
                    self.history_collector.pop_last_opnd_start();

                    self.b_no_prev_equ = true;

                    self.resolve_highest_precedence_operation()?;
                }

                if !self.b_error {
                    let grouped_string =
                        self.group_digits_per_radix(&self.number_string, self.radix);
                    self.history_collector.complete_equation(&grouped_string);

                    self.last_val = self.current_val.clone();
                    self.n_prev_op_code = 0;
                    self.precedence_op_count = 0;
                }

                self.b_change_op = false;
            }

            IDC_OPENP | IDC_CLOSEP => {
                // -IF- the Paren holding array is full and we try to add a paren
                // -OR- the paren holding array is empty and we try to remove a
                //      paren
                // -OR- the precedence holding array is full
                if (self.open_paren_count >= MAXPRECDEPTH && (w_param == IDC_OPENP))
                    || (self.open_paren_count == 0 && (w_param != IDC_OPENP))
                    || (self.precedence_op_count >= MAXPRECDEPTH
                        && self.n_prec_op[self.precedence_op_count - 1] != 0)
                {
                    if self.open_paren_count == 0
                        && (w_param != IDC_OPENP)
                        && let Some(d) = &self.calc_display
                    {
                        d.borrow_mut().on_no_right_paren_added();
                    }

                    self.handle_error_command(w_param);
                    return Ok(());
                }

                if w_param == IDC_OPENP {
                    // if there's an omitted multiplication sign
                    if is_digit_op_code(self.n_last_com)
                        || is_unary_op_code(self.n_last_com)
                        || self.n_last_com == IDC_PNT
                        || self.n_last_com == IDC_CLOSEP
                    {
                        self.process_command(IDC_MUL)?;
                    }

                    self.check_and_add_last_bin_op_to_history(true)?;
                    self.history_collector.add_open_brace_to_history();

                    // Open level of parentheses, save number and operation.
                    self.paren_vals[self.open_paren_count] = self.last_val.clone();

                    self.n_op[self.open_paren_count] =
                        if self.b_change_op { self.n_op_code } else { 0 };
                    self.open_paren_count += 1;

                    /* save a special marker on the precedence array */
                    if self.precedence_op_count < self.n_prec_op.len() {
                        self.n_prec_op[self.precedence_op_count] = 0;
                        self.precedence_op_count += 1;
                    }

                    self.last_val = Rational::from(0);
                    if is_bin_op_code(self.n_last_com) {
                        // We want 1 + ( to start as 1 + (0. Any number you type replaces 0. But if it is 1 + 3 (, it is
                        // treated as 1 + (3
                        self.current_val = Rational::from(0);
                    }
                    self.n_temp_com = 0;
                    self.n_op_code = 0;
                    self.b_change_op = false; // a ( is like starting a fresh sub equation
                } else {
                    // Last thing keyed in was an operator. Lets do the op on a duplicate of the last entry.
                    if is_bin_op_code(self.n_last_com) {
                        self.current_val = self.last_val.clone();
                    }

                    if !self.history_collector.f_opnd_added_to_history() {
                        self.history_collector.add_opnd_to_history(
                            &self.number_string,
                            &self.current_val,
                            false,
                        );
                    }

                    // Get the operation and number and return result.
                    let (cur, last) = (self.current_val.clone(), self.last_val.clone());
                    self.current_val = self.do_operation(self.n_op_code, &cur, &last);
                    self.n_prev_op_code = self.n_op_code;

                    // Now process the precedence stack till we get to an opcode which is zero.
                    loop {
                        // C++: m_nOpCode = m_nPrecOp[--m_precedenceOpCount]. When the
                        // count is already 0 (the marker was wiped by an implicit
                        // multiplication) C++ underflows a size_t (undefined
                        // behaviour); treat it as having reached the marker.
                        if self.precedence_op_count == 0 {
                            self.n_op_code = 0;
                            break;
                        }
                        self.precedence_op_count -= 1;
                        self.n_op_code = self.n_prec_op[self.precedence_op_count];
                        if self.n_op_code == 0 {
                            break;
                        }

                        // Precedence Inversion check
                        let ni = n_precedence_of_op(self.n_prev_op_code);
                        let nx = n_precedence_of_op(self.n_op_code);
                        if ni <= nx {
                            self.history_collector.enclose_prec_inversion_brackets();
                        }
                        self.history_collector.pop_last_opnd_start();

                        self.last_val = self.precedence_vals[self.precedence_op_count].clone();

                        let (cur, last) = (self.current_val.clone(), self.last_val.clone());
                        self.current_val = self.do_operation(self.n_op_code, &cur, &last);
                        self.n_prev_op_code = self.n_op_code;
                    }

                    self.history_collector.add_close_brace_to_history();

                    // Now get back the operation and opcode at the beginning of this parenthesis pair

                    self.open_paren_count -= 1;
                    self.last_val = self.paren_vals[self.open_paren_count].clone();
                    self.n_op_code = self.n_op[self.open_paren_count];

                    // m_bChangeOp should be true if m_nOpCode is valid
                    self.b_change_op = self.n_op_code != 0;
                }

                // Set the "(=xx" indicator.
                if let Some(d) = &self.calc_display {
                    d.borrow_mut()
                        .set_parenthesis_number(self.open_paren_count as u32);
                }

                if !self.b_error {
                    self.display_num()?;
                }
            }

            // BASE CHANGES:
            IDM_HEX | IDM_DEC | IDM_OCT | IDM_BIN => {
                self.set_radix_type_and_num_width(RadixType::from_index(w_param - IDM_HEX), None)?;
                self.history_collector
                    .update_history_expression(self.radix, self.precision)?;
            }

            IDM_QWORD | IDM_DWORD | IDM_WORD | IDM_BYTE => {
                if self.b_record {
                    self.current_val = self.input.to_rational(self.radix, self.precision)?;
                    self.b_record = false;
                }

                // Compat. mode BaseX: Qword, Dword, Word, Byte
                self.set_radix_type_and_num_width(None, NumWidth::from_index(w_param - IDM_QWORD))?;
            }

            IDM_DEG | IDM_RAD | IDM_GRAD => {
                self.angletype = match w_param - IDM_DEG {
                    0 => AngleType::Degrees,
                    1 => AngleType::Radians,
                    _ => AngleType::Gradians,
                };
            }

            IDC_SIGN => {
                if self.b_record {
                    let max_str = self.get_max_decimal_value_string();
                    if self.input.try_toggle_sign(self.f_integer_mode, &max_str) {
                        self.display_num()?;
                    } else {
                        self.handle_error_command(w_param);
                    }
                    return Ok(());
                }

                // Doing +/- while in Record mode is not a unary operation
                if is_bin_op_code(self.n_last_com) {
                    self.current_val = self.last_val.clone();
                }

                if !self.history_collector.f_opnd_added_to_history() {
                    self.history_collector.add_opnd_to_history(
                        &self.number_string,
                        &self.current_val,
                        false,
                    );
                }

                self.current_val = -&self.current_val;

                self.display_num()?;
                self.history_collector.add_unary_op_to_history(
                    IDC_SIGN,
                    self.b_inv,
                    self.angletype,
                );
            }

            IDC_RECALL => {
                if self.b_set_calc_state {
                    // Not a Memory recall. set the result
                    self.b_set_calc_state = false;
                } else {
                    // Recall immediate memory value.
                    // (C++ dereferences a possibly moved-out unique_ptr here; treat as 0.)
                    self.current_val = self.memory_value.clone().unwrap_or_default();
                }
                self.check_and_add_last_bin_op_to_history(true)?;
                self.display_num()?;
            }

            IDC_MPLUS => {
                /* MPLUS adds m_currentVal to immediate memory and kills the "mem"   */
                /* indicator if the result is zero.                           */
                let result = self
                    .memory_value
                    .clone()
                    .unwrap_or_default()
                    .add(&self.current_val)?;
                self.memory_value = Some(self.truncate_num_for_int_math(&result)?); // Memory should follow the current int mode
            }
            IDC_MMINUS => {
                /* MMINUS subtracts m_currentVal to immediate memory and kills the "mem"   */
                /* indicator if the result is zero.                           */
                let result = self
                    .memory_value
                    .clone()
                    .unwrap_or_default()
                    .sub(&self.current_val)?;
                self.memory_value = Some(self.truncate_num_for_int_math(&result)?);
            }
            IDC_STORE | IDC_MCLEAR => {
                let v = if w_param == IDC_STORE {
                    self.truncate_num_for_int_math(&self.current_val)?
                } else {
                    Rational::from(0)
                };
                self.memory_value = Some(v);
            }
            IDC_PI => {
                if !self.f_integer_mode {
                    self.check_and_add_last_bin_op_to_history(true)?; // pi is like entering the number
                    self.current_val = if self.b_inv {
                        ratpack_two_pi()?
                    } else {
                        ratpack_pi()?
                    };

                    self.display_num()?;
                    self.b_inv = false;
                    return Ok(());
                }
                self.handle_error_command(w_param);
            }
            IDC_RAND => {
                if !self.f_integer_mode {
                    self.check_and_add_last_bin_op_to_history(true)?; // rand is like entering the number

                    // wstringstream << fixed << setprecision(m_precision) << GenerateRandomNumber()
                    let prec = usize::try_from(self.precision).unwrap_or(6);
                    let s = format!("{:.*}", prec, self.generate_random_number());

                    let rat =
                        ratpack::string_to_rat(false, &s, false, "", self.radix, self.precision)?;
                    self.current_val = match rat {
                        Some(r) => r,
                        None => Rational::from(0),
                    };

                    self.display_num()?;
                    self.b_inv = false;
                    return Ok(());
                }
                self.handle_error_command(w_param);
            }
            IDC_EULER => {
                if !self.f_integer_mode {
                    self.check_and_add_last_bin_op_to_history(true)?; // e is like entering the number
                    self.current_val = ratpack::rat_exp();

                    self.display_num()?;
                    self.b_inv = false;
                    return Ok(());
                }
                self.handle_error_command(w_param);
            }
            IDC_FE => {
                // Toggle exponential notation display.
                self.n_fe = if self.n_fe == NumberFormat::Float {
                    NumberFormat::Scientific
                } else {
                    NumberFormat::Float
                };
                self.display_num()?;
            }

            IDC_EXP => {
                if self.b_record && !self.f_integer_mode && self.input.try_begin_exponent() {
                    self.display_num()?;
                    return Ok(());
                }
                self.handle_error_command(w_param);
            }

            IDC_PNT => {
                // Check if the last command was a closing parenthesis
                if self.n_last_com == IDC_CLOSEP {
                    self.implicit_multiplication_after_close_paren();
                }

                if self.b_record && !self.f_integer_mode && self.input.try_add_decimal_pt() {
                    self.display_num()?;
                    return Ok(());
                }
                self.handle_error_command(w_param);
            }

            IDC_INV => {
                self.b_inv = !self.b_inv;
            }

            _ => {}
        }

        Ok(())
    }

    /// Helper function to resolve one item on the precedence stack.
    fn resolve_highest_precedence_operation(&mut self) -> CalcResult<()> {
        // Is there a valid operation around?
        if self.n_op_code != 0 {
            // If this is the first EQU in a string, set m_holdVal=m_currentVal
            // Otherwise let m_currentVal=m_holdVal.  This keeps m_currentVal constant
            // through all EQUs in a row.
            if self.b_no_prev_equ {
                self.hold_val = self.current_val.clone();
            } else {
                self.current_val = self.hold_val.clone();
                self.display_num()?; // to update the m_numberString
                self.history_collector.add_bin_op_to_history(
                    self.n_op_code,
                    self.f_integer_mode,
                    false,
                );
                self.history_collector.add_opnd_to_history(
                    &self.number_string,
                    &self.current_val,
                    false,
                ); // Adding the repeated last op to history
            }

            // Do the current or last operation.
            let (cur, last) = (self.current_val.clone(), self.last_val.clone());
            self.current_val = self.do_operation(self.n_op_code, &cur, &last);
            self.n_prev_op_code = self.n_op_code;
            self.last_val = self.current_val.clone();

            // Check for errors.  If this wasn't done, DisplayNum
            // would immediately overwrite any error message.
            if !self.b_error {
                self.display_num()?;
            }

            // No longer the first EQU.
            self.b_no_prev_equ = false;
        } else if !self.b_error {
            self.display_num()?;
        }
        Ok(())
    }

    /// CheckAndAddLastBinOpToHistory
    ///
    ///  This is a very confusing helper routine to add the last entered binary operator to the history. This is expected to
    /// leave the history with <exp> <binop> state. It can really add the last entered binary op, or it can actually remove
    /// the last operand from history. This happens because you can 'type' or 'compute' over last operand in some cases, thereby
    /// effectively removing only it from the equation but still keeping the previous portion of the equation. Eg. 1 + 4 sqrt 5. The last
    /// 5 will remove sqrt(4) as it is not used anymore to participate in 1 + 5
    /// If you are messing with this, test cases like this CE, statistical functions, ( & MR buttons
    pub(super) fn check_and_add_last_bin_op_to_history(
        &mut self,
        add_to_history: bool,
    ) -> CalcResult<()> {
        if self.b_change_op {
            if self.history_collector.f_opnd_added_to_history() {
                // if last time opnd was added but the last command was not a binary operator, then it must have come
                // from commands which add the operand, like unary operator. So history at this is showing 1 + sqrt(4)
                // but in reality the sqrt(4) is getting replaced by new number (may be unary op, or MR or SUM etc.)
                // So erase the last operand
                self.history_collector.remove_last_opnd_from_history()?;
            }
        } else if self.history_collector.f_opnd_added_to_history() && !self.b_error {
            // Corner case, where opnd is already in history but still a new opnd starting (1 + 4 sqrt 5). This is yet another
            // special casing of previous case under if (m_bChangeOp), but this time we can do better than just removing it
            // Let us make a current value =. So in case of 4 SQRT (or a equation under braces) and then a new equation is started, we can just form
            // a useful equation of sqrt(4) = 2 and continue a new equation from now on. But no point in doing this for things like
            // MR, SUM etc. All you will get is 5 = 5 kind of no useful equation.
            if (is_unary_op_code(self.n_last_com)
                || IDC_SIGN == self.n_last_com
                || IDC_CLOSEP == self.n_last_com)
                && 0 == self.open_paren_count
            {
                if add_to_history {
                    let grouped = self.group_digits_per_radix(&self.number_string, self.radix);
                    self.history_collector.complete_history_line(&grouped);
                }
            } else {
                self.history_collector.remove_last_opnd_from_history()?;
            }
        }
        Ok(())
    }

    /// change the display area from a static text to an editbox, which has the focus can make
    /// Magnifier (Accessibility tool) work
    pub(super) fn set_primary_display(&mut self, sz_text: &str, is_error: bool) {
        if let Some(d) = &self.calc_display {
            d.borrow_mut().set_primary_display(sz_text, is_error);
            d.borrow_mut().set_is_in_error(is_error);
        }
    }

    fn display_announce_binary_operator(&mut self) {
        // If m_pCalcDisplay is null, this is not a high priority function
        // and should not be the reason we crash.
        if let Some(d) = &self.calc_display {
            d.borrow_mut().binary_operator_received();
        }
    }

    /// `CCalcEngine::OpCodeToUnaryString`
    pub fn op_code_to_unary_string(n_op_code: i32, f_inv: bool, angletype: AngleType) -> String {
        // Try to lookup the ID in the UFNE table
        let mut ids: &str = "";

        if let Some(element) = operator_string_table(n_op_code) {
            if !element.has_angle_strings() || AngleType::Degrees == angletype {
                if f_inv {
                    ids = element.inverse_degree_string;
                }

                if ids.is_empty() {
                    ids = element.degree_string;
                }
            } else if AngleType::Radians == angletype {
                if f_inv {
                    ids = element.inverse_rad_string;
                }
                if ids.is_empty() {
                    ids = element.rad_string;
                }
            } else if AngleType::Gradians == angletype {
                if f_inv {
                    ids = element.inverse_grad_string;
                }
                if ids.is_empty() {
                    ids = element.grad_string;
                }
            }

            if !ids.is_empty() {
                return Self::get_string(ids);
            }
        }

        // If we didn't find an ID in the table, use the op code.
        Self::op_code_to_string(n_op_code)
    }

    /// `CCalcEngine::OpCodeToBinaryString`
    pub fn op_code_to_binary_string(n_op_code: i32, is_integer_mode: bool) -> String {
        // Try to lookup the ID in the UFNE table
        let mut ids: &str = "";

        if let Some(element) = operator_string_table(n_op_code) {
            if is_integer_mode && !element.programmer_mode_string.is_empty() {
                ids = element.programmer_mode_string;
            } else {
                ids = element.degree_string;
            }
        }

        if !ids.is_empty() {
            return Self::get_string(ids);
        }

        // If we didn't find an ID in the table, use the op code.
        Self::op_code_to_string(n_op_code)
    }

    pub fn is_current_too_big_for_trig(&self) -> bool {
        self.current_val >= self.max_trigonometric_num
    }

    pub fn get_current_radix(&self) -> u32 {
        self.radix
    }

    pub fn get_current_result_for_radix(
        &mut self,
        radix: u32,
        precision: i32,
        group_digits_per_radix: bool,
    ) -> CalcResult<String> {
        let rat = if self.b_record {
            self.input.to_rational(self.radix, self.precision)?
        } else {
            self.current_val.clone()
        };

        ratpack::change_constants(self.radix, precision);

        let number_string = self.get_string_for_display(&rat, radix)?;
        if !number_string.is_empty() {
            // Revert the precision to previously stored precision
            ratpack::change_constants(self.radix, self.precision);
        }

        if group_digits_per_radix {
            Ok(self.group_digits_per_radix(&number_string, radix))
        } else {
            Ok(number_string)
        }
    }

    pub fn get_string_for_display(&self, rat: &Rational, radix: u32) -> CalcResult<String> {
        // Check for standard\scientific mode
        if !self.f_integer_mode {
            return rat.to_string_radix(radix, self.n_fe, self.precision);
        }

        // Programmer mode
        // Find most significant bit to determine if number is negative
        let temp_rat = self.truncate_num_for_int_math(rat)?;

        let attempt = || -> CalcResult<String> {
            let mut temp_rat = temp_rat.clone();
            let w64_bits = temp_rat.to_u64()?;
            let f_msb = ((w64_bits >> (self.dw_word_bit_width - 1)) & 1) != 0;
            if (radix == 10) && f_msb {
                // If high bit is set, then get the decimal number in negative 2's complement form.
                temp_rat = -(temp_rat
                    .bitxor(&self.get_chop_number())?
                    .add(&Rational::from(1))?);
            }

            temp_rat.to_string_radix(radix, self.n_fe, self.precision)
        };

        // catch (uint32_t) {} — result stays empty
        Ok(attempt().unwrap_or_default())
    }

    fn generate_random_number(&mut self) -> f64 {
        self.random_generator
            .get_or_insert_with(Mt19937::from_entropy)
            .uniform_01()
    }
}
