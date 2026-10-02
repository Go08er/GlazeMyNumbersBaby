// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `ExpressionCommandInterface.h`, `ExpressionCommand.h` and
//! `ExpressionCommand.cpp`.
//!
//! The C++ class hierarchy (`IExpressionCommand` → `IOperatorCommand` →
//! `IUnaryCommand` / `IBinaryCommand`, `IOpndCommand`, `IParenthesisCommand`)
//! becomes the [`ExpressionCommand`] enum whose variants wrap the concrete
//! classes (`CParentheses`, `CUnaryCommand`, `CBinaryCommand`,
//! `COpndCommand`). Commands are plain values; the shipping app only ever
//! consumed value snapshots of them (via `ExpressionCommandWrapper`).

use ratpack::{CalcResult, NumberFormat, Rational};

use crate::ccommand::*;
use crate::command::{Command, CommandType};

const CH_NEGATE: char = '-';
const CH_EXP: char = 'e';
const CH_PLUS: char = '+';

/// `ISerializeCommandVisitor`
pub trait SerializeCommandVisitor {
    fn visit_opnd(&mut self, opnd_cmd: &mut OpndCommand);
    fn visit_unary(&mut self, unary_cmd: &mut UnaryCommand);
    fn visit_binary(&mut self, binary_cmd: &mut BinaryCommand);
    fn visit_parentheses(&mut self, para_cmd: &mut Parentheses);
}

/// `IExpressionCommand` (and its concrete implementations).
#[derive(Clone, Debug)]
pub enum ExpressionCommand {
    /// `CUnaryCommand`
    Unary(UnaryCommand),
    /// `CBinaryCommand`
    Binary(BinaryCommand),
    /// `COpndCommand`
    Operand(OpndCommand),
    /// `CParentheses`
    Parentheses(Parentheses),
}

impl ExpressionCommand {
    /// `IExpressionCommand::GetCommandType`
    pub fn get_command_type(&self) -> CommandType {
        match self {
            ExpressionCommand::Unary(c) => c.get_command_type(),
            ExpressionCommand::Binary(c) => c.get_command_type(),
            ExpressionCommand::Operand(c) => c.get_command_type(),
            ExpressionCommand::Parentheses(c) => c.get_command_type(),
        }
    }

    /// `IExpressionCommand::Accept`
    pub fn accept(&mut self, command_visitor: &mut dyn SerializeCommandVisitor) {
        match self {
            ExpressionCommand::Unary(c) => c.accept(command_visitor),
            ExpressionCommand::Binary(c) => c.accept(command_visitor),
            ExpressionCommand::Operand(c) => c.accept(command_visitor),
            ExpressionCommand::Parentheses(c) => c.accept(command_visitor),
        }
    }
}

/// `CParentheses`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parentheses {
    command: i32,
}

impl Parentheses {
    pub fn new(command: i32) -> Self {
        Parentheses { command }
    }

    pub fn get_command(&self) -> i32 {
        self.command
    }

    pub fn get_command_type(&self) -> CommandType {
        CommandType::Parentheses
    }

    pub fn accept(&mut self, command_visitor: &mut dyn SerializeCommandVisitor) {
        command_visitor.visit_parentheses(self);
    }
}

/// `CUnaryCommand`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnaryCommand {
    command: Vec<i32>,
}

impl UnaryCommand {
    pub fn new(command: i32) -> Self {
        UnaryCommand {
            command: vec![command],
        }
    }

    pub fn new2(command1: i32, command2: i32) -> Self {
        UnaryCommand {
            command: vec![command1, command2],
        }
    }

    pub fn get_commands(&self) -> &[i32] {
        &self.command
    }

    pub fn get_command_type(&self) -> CommandType {
        CommandType::UnaryCommand
    }

    pub fn set_command(&mut self, command: i32) {
        self.command.clear();
        self.command.push(command);
    }

    pub fn set_commands(&mut self, command1: i32, command2: i32) {
        self.command.clear();
        self.command.push(command1);
        self.command.push(command2);
    }

    pub fn accept(&mut self, command_visitor: &mut dyn SerializeCommandVisitor) {
        command_visitor.visit_unary(self);
    }
}

/// `CBinaryCommand`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryCommand {
    command: i32,
}

impl BinaryCommand {
    pub fn new(command: i32) -> Self {
        BinaryCommand { command }
    }

    pub fn set_command(&mut self, command: i32) {
        self.command = command;
    }

    pub fn get_command(&self) -> i32 {
        self.command
    }

    pub fn get_command_type(&self) -> CommandType {
        CommandType::BinaryCommand
    }

    pub fn accept(&mut self, command_visitor: &mut dyn SerializeCommandVisitor) {
        command_visitor.visit_binary(self);
    }
}

/// `COpndCommand`
#[derive(Clone, Debug)]
pub struct OpndCommand {
    commands: Vec<i32>,
    f_negative: bool,
    f_sci_fmt: bool,
    f_decimal: bool,
    f_initialized: bool,
    token: String,
    value: Rational,
}

impl OpndCommand {
    pub fn new(commands: Vec<i32>, f_negative: bool, f_decimal: bool, f_sci_fmt: bool) -> Self {
        OpndCommand {
            commands,
            f_negative,
            f_sci_fmt,
            f_decimal,
            f_initialized: false,
            token: String::new(),
            value: Rational::default(),
        }
    }

    pub fn initialize(&mut self, rat: &Rational) {
        self.value = rat.clone();
        self.f_initialized = true;
    }

    pub fn get_commands(&self) -> &[i32] {
        &self.commands
    }

    pub fn set_commands(&mut self, commands: Vec<i32>) {
        self.commands = commands;
    }

    pub fn append_command(&mut self, command: i32) {
        if self.f_sci_fmt {
            self.clear_all_and_append_command(Command(command));
        } else {
            self.commands.push(command);
        }

        if command == IDC_PNT {
            self.f_decimal = true;
        }
    }

    pub fn toggle_sign(&mut self) {
        for &n_op_code in &self.commands {
            if n_op_code != IDC_0 {
                self.f_negative = !self.f_negative;
                break;
            }
        }
    }

    pub fn remove_from_end(&mut self) {
        if self.f_sci_fmt {
            self.clear_all_and_append_command(Command::Command0);
        } else {
            let n_commands = self.commands.len();

            if n_commands == 1 {
                self.clear_all_and_append_command(Command::Command0);
            } else {
                // C++: m_commands->at(nCommands - 1) (throws on an empty vector)
                if let Some(&n_op_code) = self.commands.last()
                    && n_op_code == IDC_PNT
                {
                    self.f_decimal = false;
                }

                self.commands.pop();
            }
        }
    }

    pub fn is_negative(&self) -> bool {
        self.f_negative
    }

    pub fn is_sci_fmt(&self) -> bool {
        self.f_sci_fmt
    }

    pub fn is_decimal_present(&self) -> bool {
        self.f_decimal
    }

    pub fn get_command_type(&self) -> CommandType {
        CommandType::OperandCommand
    }

    fn clear_all_and_append_command(&mut self, command: Command) {
        self.commands.clear();
        self.commands.push(command.0);
        self.f_sci_fmt = false;
        self.f_negative = false;
        self.f_decimal = false;
    }

    /// `COpndCommand::GetToken`
    pub fn get_token(&mut self, decimal_symbol: char) -> String {
        const CH_ZERO: char = '0';

        let n_commands = self.commands.len();
        let mut token: Vec<char> = Vec::new();

        for i in 0..n_commands {
            let n_op_code = self.commands[i];

            if n_op_code == IDC_PNT {
                token.push(decimal_symbol);
            } else if n_op_code == IDC_EXP {
                token.push(CH_EXP);
                // C++: m_commands->at(i + 1) throws std::out_of_range when EXP is last.
                let next_op_code = self.commands.get(i + 1).copied();
                if next_op_code != Some(IDC_SIGN) {
                    token.push(CH_PLUS);
                }
            } else if n_op_code == IDC_SIGN {
                token.push(CH_NEGATE);
            } else {
                let num = (n_op_code - IDC_0).to_string();
                token.extend(num.chars());
            }
        }

        // Remove zeros
        for i in 0..token.len() {
            if token[i] != CH_ZERO {
                if token[i] == decimal_symbol {
                    // C++: m_token.erase(0, i - 1) — with i == 0 the count wraps
                    // to npos and the whole token is erased.
                    let count = if i == 0 { token.len() } else { i - 1 };
                    token.drain(0..count);
                } else {
                    token.drain(0..i);
                }

                if self.f_negative {
                    token.insert(0, CH_NEGATE);
                }

                self.token = token.into_iter().collect();
                return self.token.clone();
            }
        }

        self.token = CH_ZERO.to_string();

        self.token.clone()
    }

    /// `COpndCommand::GetString`
    pub fn get_string(&self, radix: u32, precision: i32) -> CalcResult<String> {
        if self.f_initialized {
            return self
                .value
                .to_string_radix(radix, NumberFormat::Float, precision);
        }

        Ok(String::new())
    }

    /// The operand's value (set by `Initialize`); `None` if not initialized.
    pub fn value(&self) -> Option<&Rational> {
        if self.f_initialized {
            Some(&self.value)
        } else {
            None
        }
    }

    pub fn accept(&mut self, command_visitor: &mut dyn SerializeCommandVisitor) {
        command_visitor.visit_opnd(self);
    }
}
