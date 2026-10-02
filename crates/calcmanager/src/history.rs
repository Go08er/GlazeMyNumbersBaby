// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `Header Files/History.h` and `CEngine/History.cpp`
//! (`CHistoryCollector`).
//!
//! Helper class really a internal class to the engine, to accumulate each
//! history line of text by collecting the operands, operator, unary operator
//! etc. Since it is a separate entity, it can be unit tested on its own but
//! does rely on the engine calling it in appropriate order.

use ratpack::{AngleType, CalcErr, CalcResult, Rational};

use crate::calc_display::{CalcDisplayRef, ExpressionToken, HistoryDisplayRef};
use crate::calc_engine::CalcEngine;
use crate::ccommand::*;
use crate::command::{Command, CommandType};
use crate::expression_command::{
    BinaryCommand, ExpressionCommand, OpndCommand, Parentheses, UnaryCommand,
};

const ASCII_0: i32 = 48;

/// `E_BOUNDS` as thrown by `Truncate` (0x8000000B).
pub const E_BOUNDS: CalcErr = 0x8000_000B;

/// maximum depth you can get by precedence. It is just an array's size limit.
pub const MAXPRECDEPTH: usize = 25;

fn truncate<T>(v: &mut Vec<T>, index: u32) -> CalcResult<()> {
    if index as usize >= v.len() {
        return Err(E_BOUNDS);
    }

    v.truncate(index as usize);
    Ok(())
}

/// `CHistoryCollector`
pub struct HistoryCollector {
    history_display: Option<HistoryDisplayRef>,
    calc_display: Option<CalcDisplayRef>,

    i_cur_line_hist_start: i32, // index of the beginning of the current equation
    // a sort of state, set to the index before 2 after 2 in the expression 2 + 3 say. Useful for auto correct portion of history and for
    // attaching the unary op around the last operand
    last_op_start_index: i32, // index of the beginning of the last operand added to the history
    last_bin_op_start_index: i32, // index of the beginning of the last binary operator added to the history
    // Stack of index of opnd's beginning for each '('. A parallel array to m_hnoParNum, but abstracted independently of that
    operand_indices: [i32; MAXPRECDEPTH],
    cur_operand_index: i32,  // Stack index for the above stack
    b_last_opnd_brace: bool, // iff the last opnd in history is already braced so we can avoid putting another one for unary operator
    decimal_symbol: char,
    tokens: Option<Vec<ExpressionToken>>,
    commands: Option<Vec<ExpressionCommand>>,
}

impl HistoryCollector {
    pub fn new(
        calc_display: Option<CalcDisplayRef>,
        history_display: Option<HistoryDisplayRef>,
        decimal_symbol: char,
    ) -> Self {
        let mut hc = HistoryCollector {
            history_display,
            calc_display,
            i_cur_line_hist_start: -1,
            last_op_start_index: 0,
            last_bin_op_start_index: 0,
            operand_indices: [0; MAXPRECDEPTH],
            cur_operand_index: 0,
            b_last_opnd_brace: false,
            decimal_symbol,
            tokens: None,
            commands: None,
        };
        hc.reinit_history();
        hc
    }

    fn reinit_history(&mut self) {
        self.last_op_start_index = -1;
        self.last_bin_op_start_index = -1;
        self.cur_operand_index = 0;
        self.b_last_opnd_brace = false;
        if let Some(t) = self.tokens.as_mut() {
            t.clear();
        }
        if let Some(c) = self.commands.as_mut() {
            c.clear();
        }
    }

    pub fn add_opnd_to_history(&mut self, num_str: &str, rat: &Rational, f_repetition: bool) {
        let i_command_end = self.add_command(ExpressionCommand::Operand(
            self.get_operand_commands_from_string_rat(num_str, rat),
        ));
        self.last_op_start_index = self.ich_add_sz_to_equation_sz(num_str, i_command_end);

        if f_repetition {
            self.set_expression_display();
        }
        self.b_last_opnd_brace = false;
        self.last_bin_op_start_index = -1;
    }

    pub fn remove_last_opnd_from_history(&mut self) -> CalcResult<()> {
        self.truncate_equation_sz_from_ich(self.last_op_start_index)?;
        self.set_expression_display();
        self.last_op_start_index = -1;
        // This will not restore the m_lastBinOpStartIndex, as it isn't possible to remove that also later
        Ok(())
    }

    pub fn add_bin_op_to_history(
        &mut self,
        n_op_code: i32,
        is_integer_mode: bool,
        f_no_repetition: bool,
    ) {
        let i_command_end =
            self.add_command(ExpressionCommand::Binary(BinaryCommand::new(n_op_code)));
        self.last_bin_op_start_index = self.ich_add_sz_to_equation_sz(" ", -1);

        self.ich_add_sz_to_equation_sz(
            &CalcEngine::op_code_to_binary_string(n_op_code, is_integer_mode),
            i_command_end,
        );
        self.ich_add_sz_to_equation_sz(" ", -1);

        if f_no_repetition {
            self.set_expression_display();
        }
        self.last_op_start_index = -1;
    }

    /// This is expected to be called when a binary op in the last say 1+2+ is changing to another one say 1+2* (+ changed to *)
    /// It needs to know by this change a Precedence inversion happened. i.e. previous op was lower or equal to its previous op, but the new
    /// one isn't. (Eg. 1*2* to 1*2^). It can add explicit brackets to ensure the precedence is inverted. (Eg. (1*2) ^)
    pub fn change_last_bin_op(
        &mut self,
        n_op_code: i32,
        f_prec_inv_to_higher: bool,
        is_integer_mode: bool,
    ) -> CalcResult<()> {
        self.truncate_equation_sz_from_ich(self.last_bin_op_start_index)?;
        if f_prec_inv_to_higher {
            self.enclose_prec_inversion_brackets();
        }
        self.add_bin_op_to_history(n_op_code, is_integer_mode, true);
        Ok(())
    }

    pub fn push_last_opnd_start(&mut self, ich_opnd_start: i32) {
        let ich = if ich_opnd_start == -1 {
            self.last_op_start_index
        } else {
            ich_opnd_start
        };

        if self.cur_operand_index < self.operand_indices.len() as i32 {
            self.operand_indices[self.cur_operand_index as usize] = ich;
            self.cur_operand_index += 1;
        }
    }

    pub fn pop_last_opnd_start(&mut self) {
        if self.cur_operand_index > 0 {
            self.cur_operand_index -= 1;
            self.last_op_start_index = self.operand_indices[self.cur_operand_index as usize];
        }
    }

    pub fn add_open_brace_to_history(&mut self) {
        self.add_command(ExpressionCommand::Parentheses(Parentheses::new(IDC_OPENP)));
        let ich_opnd_start =
            self.ich_add_sz_to_equation_sz(&CalcEngine::op_code_to_string(IDC_OPENP), -1);
        self.push_last_opnd_start(ich_opnd_start);

        self.set_expression_display();
        self.last_bin_op_start_index = -1;
    }

    pub fn add_close_brace_to_history(&mut self) {
        self.add_command(ExpressionCommand::Parentheses(Parentheses::new(IDC_CLOSEP)));
        self.ich_add_sz_to_equation_sz(&CalcEngine::op_code_to_string(IDC_CLOSEP), -1);
        self.set_expression_display();
        self.pop_last_opnd_start();

        self.last_bin_op_start_index = -1;
        self.b_last_opnd_brace = true;
    }

    pub fn enclose_prec_inversion_brackets(&mut self) {
        // Top of the Opnd starts index or 0 is nothing is in top
        let ich_start = if self.cur_operand_index > 0 {
            self.operand_indices[(self.cur_operand_index - 1) as usize]
        } else {
            0
        };

        self.insert_sz_in_equation_sz(&CalcEngine::op_code_to_string(IDC_OPENP), -1, ich_start);
        self.ich_add_sz_to_equation_sz(&CalcEngine::op_code_to_string(IDC_CLOSEP), -1);
    }

    pub fn f_opnd_added_to_history(&self) -> bool {
        -1 != self.last_op_start_index
    }

    /// AddUnaryOpToHistory
    ///
    /// This is does the postfix to prefix translation of the input and adds the text to the history. Eg. doing 2 + 4 (sqrt),
    /// this routine will ensure the last sqrt call unary operator, actually goes back in history and wraps 4 in sqrt(4)
    pub fn add_unary_op_to_history(&mut self, n_op_code: i32, f_inv: bool, angletype: AngleType) {
        // When successfully applying a unary op, there should be an opnd already
        // A very special case of % which is a funny post op unary op.
        if IDC_PERCENT == n_op_code {
            let i_command_end =
                self.add_command(ExpressionCommand::Unary(UnaryCommand::new(n_op_code)));
            self.ich_add_sz_to_equation_sz(
                &CalcEngine::op_code_to_string(n_op_code),
                i_command_end,
            );
        } else {
            // all the other unary ops
            let sp_expression_command = if IDC_SIGN == n_op_code {
                UnaryCommand::new(n_op_code)
            } else {
                let angle_op_code = match angletype {
                    AngleType::Degrees => Command::CommandDEG,
                    AngleType::Radians => Command::CommandRAD,
                    AngleType::Gradians => Command::CommandGRAD,
                };
                let angle_op_code = angle_op_code.0;

                let pick = |inverse: Command, plain: i32| if f_inv { inverse.0 } else { plain };
                match n_op_code {
                    IDC_SIN => {
                        UnaryCommand::new2(angle_op_code, pick(Command::CommandASIN, IDC_SIN))
                    }
                    IDC_COS => {
                        UnaryCommand::new2(angle_op_code, pick(Command::CommandACOS, IDC_COS))
                    }
                    IDC_TAN => {
                        UnaryCommand::new2(angle_op_code, pick(Command::CommandATAN, IDC_TAN))
                    }
                    IDC_SINH => UnaryCommand::new(pick(Command::CommandASINH, IDC_SINH)),
                    IDC_COSH => UnaryCommand::new(pick(Command::CommandACOSH, IDC_COSH)),
                    IDC_TANH => UnaryCommand::new(pick(Command::CommandATANH, IDC_TANH)),
                    IDC_SEC => {
                        UnaryCommand::new2(angle_op_code, pick(Command::CommandASEC, IDC_SEC))
                    }
                    IDC_CSC => {
                        UnaryCommand::new2(angle_op_code, pick(Command::CommandACSC, IDC_CSC))
                    }
                    IDC_COT => {
                        UnaryCommand::new2(angle_op_code, pick(Command::CommandACOT, IDC_COT))
                    }
                    IDC_SECH => UnaryCommand::new(pick(Command::CommandASECH, IDC_SECH)),
                    IDC_CSCH => UnaryCommand::new(pick(Command::CommandACSCH, IDC_CSCH)),
                    IDC_COTH => UnaryCommand::new(pick(Command::CommandACOTH, IDC_COTH)),
                    IDC_LN => UnaryCommand::new(pick(Command::CommandPOWE, IDC_LN)),
                    _ => UnaryCommand::new(n_op_code),
                }
            };

            let i_command_end = self.add_command(ExpressionCommand::Unary(sp_expression_command));

            let mut operand_str = CalcEngine::op_code_to_unary_string(n_op_code, f_inv, angletype);
            if !self.b_last_opnd_brace {
                // The opnd is already covered in braces. No need for additional braces around it
                operand_str.push_str(&CalcEngine::op_code_to_string(IDC_OPENP));
            }
            self.insert_sz_in_equation_sz(&operand_str, i_command_end, self.last_op_start_index);

            if !self.b_last_opnd_brace {
                self.ich_add_sz_to_equation_sz(&CalcEngine::op_code_to_string(IDC_CLOSEP), -1);
            }
        }

        self.set_expression_display();
        self.b_last_opnd_brace = false;
        // m_lastOpStartIndex remains the same as last opnd is just replaced by unaryop(lastopnd)
        self.last_bin_op_start_index = -1;
    }

    /// Called after = with the result of the equation
    /// Responsible for clearing the top line of current running history display, as well as adding yet another element to
    /// history of equations
    pub fn complete_history_line(&mut self, num_str: &str) {
        if let Some(history_display) = self.history_display.clone() {
            let tokens = self.tokens.take().unwrap_or_default();
            let commands = self.commands.take().unwrap_or_default();
            let added_item_index = history_display
                .borrow_mut()
                .add_to_history(tokens, commands, num_str);
            // C++ dereferences m_pCalcDisplay unconditionally here.
            if let Some(d) = &self.calc_display {
                d.borrow_mut().on_history_item_added(added_item_index);
            }
        }

        self.tokens = None;
        self.commands = None;
        self.i_cur_line_hist_start = -1; // It will get recomputed at the first Opnd
        self.reinit_history();
    }

    pub fn complete_equation(&mut self, num_str: &str) {
        // Add only '=' token and not add EQU command, because
        // EQU command breaks loading from history (it duplicate history entries).
        self.ich_add_sz_to_equation_sz(&CalcEngine::op_code_to_string(IDC_EQU), -1);

        self.set_expression_display();
        self.complete_history_line(num_str);
    }

    pub fn clear_history_line(&mut self, err_str: &str) {
        if err_str.is_empty() {
            // in case of error let the display stay as it is
            if let Some(d) = &self.calc_display {
                d.borrow_mut().set_expression_display(&[], &[]);
            }
            self.i_cur_line_hist_start = -1; // It will get recomputed at the first Opnd
            self.reinit_history();
        }
    }

    /// Adds the given string psz to the globally maintained current equation string at the end.
    /// Also returns the 0 based index in the string just added. Can throw out of memory error
    fn ich_add_sz_to_equation_sz(&mut self, s: &str, icommand_index: i32) -> i32 {
        let tokens = self.tokens.get_or_insert_with(Vec::new);
        tokens.push((s.to_string(), icommand_index));
        (tokens.len() - 1) as i32
    }

    /// Inserts a given string into the global m_pszEquation at the given index ich taking care of reallocations etc.
    fn insert_sz_in_equation_sz(&mut self, s: &str, icommand_index: i32, ich: i32) {
        let tokens = self.tokens.get_or_insert_with(Vec::new);
        // C++: m_spTokens->emplace(begin() + ich, ...) — an out-of-range ich is
        // undefined behaviour there; clamp instead of panicking.
        let pos = (ich.max(0) as usize).min(tokens.len());
        tokens.insert(pos, (s.to_string(), icommand_index));
    }

    /// Chops off the current equation string from the given index
    fn truncate_equation_sz_from_ich(&mut self, ich: i32) -> CalcResult<()> {
        // Truncate commands
        let min_idx: i32 = -1;
        let tokens = self.tokens.get_or_insert_with(Vec::new);
        let n_tokens = tokens.len() as u32;

        // NOTE: faithfully ported, including the original condition
        // `(minIdx != -1) || (curTokenId < minIdx)` which can never be true
        // (minIdx starts at -1 and token ids are >= -1), so the command list is
        // never truncated here.
        let mut i = ich as u32;
        while i < n_tokens {
            let cur_token_id = tokens[i as usize].1;
            if cur_token_id != -1 && ((min_idx != -1) || (cur_token_id < min_idx)) {
                // Unreachable (see above); kept for parity with the C++ source.
                if let Some(commands) = self.commands.as_mut() {
                    truncate(commands, cur_token_id as u32)?;
                }
            }
            i += 1;
        }

        truncate(tokens, ich as u32)
    }

    /// Adds the m_pszEquation into the running history text
    fn set_expression_display(&mut self) {
        if let Some(d) = &self.calc_display {
            let tokens: &[ExpressionToken] = self.tokens.as_deref().unwrap_or(&[]);
            let commands: &[ExpressionCommand] = self.commands.as_deref().unwrap_or(&[]);
            d.borrow_mut().set_expression_display(tokens, commands);
        }
    }

    pub fn add_command(&mut self, sp_command: ExpressionCommand) -> i32 {
        let commands = self.commands.get_or_insert_with(Vec::new);
        commands.push(sp_command);
        (commands.len() - 1) as i32
    }

    /// To Update the operands in the Expression according to the current Radix
    pub fn update_history_expression(&mut self, radix: u32, precision: i32) -> CalcResult<()> {
        if self.tokens.is_none() {
            return Ok(());
        }

        let n_tokens = self.tokens.as_ref().map_or(0, |t| t.len());
        for idx in 0..n_tokens {
            let command_position = self.tokens.as_ref().unwrap()[idx].1;
            if command_position != -1 {
                // C++: m_spCommands->at(commandPosition)
                let new_token = match self
                    .commands
                    .as_ref()
                    .and_then(|c| c.get(command_position as usize))
                {
                    Some(ExpressionCommand::Operand(opnd_command))
                        if opnd_command.get_command_type() == CommandType::OperandCommand =>
                    {
                        Some(opnd_command.get_string(radix, precision)?)
                    }
                    _ => None,
                };
                if let Some(new_token) = new_token {
                    let new_commands = self.get_operand_commands_from_string(&new_token);
                    self.tokens.as_mut().unwrap()[idx].0 = new_token;
                    if let Some(ExpressionCommand::Operand(opnd_command)) = self
                        .commands
                        .as_mut()
                        .and_then(|c| c.get_mut(command_position as usize))
                    {
                        opnd_command.set_commands(new_commands);
                    }
                }
            }
        }

        self.set_expression_display();
        Ok(())
    }

    pub fn set_decimal_symbol(&mut self, decimal_symbol: char) {
        self.decimal_symbol = decimal_symbol;
    }

    /// Update the commands corresponding to the passed string Number
    fn get_operand_commands_from_string(&self, num_str: &str) -> Vec<i32> {
        let chars: Vec<char> = num_str.chars().collect();
        let mut commands = Vec::new();
        // Check for negate
        let f_negative = chars.first() == Some(&'-');

        for &ch in chars.iter().skip(if f_negative { 1 } else { 0 }) {
            if ch == self.decimal_symbol {
                commands.push(IDC_PNT);
            } else if ch == 'e' {
                commands.push(IDC_EXP);
            } else if ch == '-' {
                commands.push(IDC_SIGN);
            } else if ch == '+' {
                // Ignore.
            } else {
                // Number
                let mut num = ch as i32 - ASCII_0;
                num += IDC_0;
                commands.push(num);
            }
        }

        // If the number is negative, append a sign command at the end.
        if f_negative {
            commands.push(IDC_SIGN);
        }
        commands
    }

    /// `GetOperandCommandsFromString(numStr, rat)`
    pub fn get_operand_commands_from_string_rat(
        &self,
        num_str: &str,
        rat: &Rational,
    ) -> OpndCommand {
        let chars: Vec<char> = num_str.chars().collect();
        let mut commands = Vec::new();
        // Check for negate
        let f_negative = chars.first() == Some(&'-');
        let mut f_sci_fmt = false;
        let mut f_decimal = false;

        for &ch in chars.iter().skip(if f_negative { 1 } else { 0 }) {
            if ch == self.decimal_symbol {
                commands.push(IDC_PNT);
                if !f_sci_fmt {
                    f_decimal = true;
                }
            } else if ch == 'e' {
                commands.push(IDC_EXP);
                f_sci_fmt = true;
            } else if ch == '-' {
                commands.push(IDC_SIGN);
            } else if ch == '+' {
                // Ignore.
            } else {
                // Number
                let mut num = ch as i32 - ASCII_0;
                num += IDC_0;
                commands.push(num);
            }
        }

        let mut operand_command = OpndCommand::new(commands, f_negative, f_decimal, f_sci_fmt);
        operand_command.initialize(rat);
        operand_command
    }

    pub fn get_commands(&self) -> Vec<ExpressionCommand> {
        self.commands.clone().unwrap_or_default()
    }

    /// `m_iCurLineHistStart` (kept for parity; never read by the engine).
    pub fn cur_line_hist_start(&self) -> i32 {
        self.i_cur_line_hist_start
    }
}
