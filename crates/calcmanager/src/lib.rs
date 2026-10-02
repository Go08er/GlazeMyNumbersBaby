// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Rust port of Windows Calculator's calculation engine (`CalcManager`):
//! `CCalcEngine` (the CEngine state machine), `CHistoryCollector`,
//! `CalcInput`, the expression-command types, `CalculatorHistory` and
//! `CalculatorManager`. Arbitrary-precision math comes from the `ratpack`
//! crate.
//!
//! # Driving the calculator from a UI
//!
//! A UI implements [`CalcDisplay`] (the C++ `ICalcDisplay`) and hands a shared
//! handle to [`CalculatorManager::new`]. Every display update is delivered
//! synchronously, from inside the manager call that caused it:
//!
//! ```no_run
//! use std::{cell::RefCell, rc::Rc};
//! use calcmanager::*;
//!
//! #[derive(Default)]
//! struct Ui { primary: String, expression: String }
//!
//! impl CalcDisplay for Ui {
//!     fn set_primary_display(&mut self, text: &str, _is_error: bool) { self.primary = text.to_string(); }
//!     fn set_is_in_error(&mut self, _is_in_error: bool) {}
//!     fn set_expression_display(&mut self, tokens: &[ExpressionToken], _commands: &[ExpressionCommand]) {
//!         self.expression = tokens.iter().map(|t| t.0.as_str()).collect();
//!     }
//!     fn set_parenthesis_number(&mut self, _count: u32) {}
//!     fn on_no_right_paren_added(&mut self) {}
//!     fn max_digits_reached(&mut self) {}
//!     fn binary_operator_received(&mut self) {}
//!     fn on_history_item_added(&mut self, _added_item_index: u32) {}
//!     fn set_memorized_numbers(&mut self, _numbers: &[String]) {}
//!     fn memory_item_changed(&mut self, _index: u32) {}
//!     fn input_changed(&mut self) {}
//! }
//!
//! let ui = Rc::new(RefCell::new(Ui::default()));
//! let mut manager = CalculatorManager::new(ui.clone(), Rc::new(EngineResourceProvider::default()));
//! manager.set_standard_mode().unwrap();
//! for c in [Command::Command1, Command::CommandADD, Command::Command2, Command::CommandEQU] {
//!     manager.send_command(c).unwrap();
//! }
//! assert_eq!(ui.borrow().primary, "3");
//! assert_eq!(ui.borrow().expression, "1 + 2=");
//! ```
//!
//! Rules of the road:
//!
//! * The UI keeps its own clone of the `Rc<RefCell<..>>` and reads its state
//!   after a manager call returns. Callbacks must **not** call back into the
//!   manager (it is mutably borrowed while it runs); e.g. the C# view model's
//!   habit of calling `GetResultForRadix` from inside `SetPrimaryDisplay`
//!   becomes "set a flag in the callback, query after `send_command`".
//!   Likewise, do not hold a `borrow()`/`borrow_mut()` of the display across a
//!   manager call — the manager borrows it mutably to deliver callbacks.
//! * Everything is single-threaded: ratpack's precision/radix globals and the
//!   engine's shared statics (engine string table, display cache) are
//!   thread-local, mirroring one C++ process per thread.
//! * Methods that could let a C++ exception escape return [`CalcResult`];
//!   `Err(code)` carries the exact `CALC_E_*` / `E_BOUNDS` value. The shipping
//!   app never catches these, so treating `Err` as "ignore" is fine.
//! * [`Command`] is a transparent `i32` newtype whose associated constants
//!   keep the C++ names and values (`Command::CommandADD == Command(93)`).

mod calc_display;
mod calc_engine;
mod calc_input;
mod calc_utils;
mod calculator_history;
mod calculator_manager;
pub mod ccommand;
mod command;
pub mod engine_strings;
mod expression_command;
mod history;
pub mod number_formatting_utils;
mod radix_type;
mod resource;

pub use calc_display::{
    CalcDisplay, CalcDisplayRef, ExpressionToken, HistoryDisplay, HistoryDisplayRef,
};
pub use calc_engine::{CalcEngine, NUM_WIDTH_LENGTH, NumWidth};
pub use calc_input::{CalcInput, CalcNumSec, MAX_STRLEN};
pub use calc_utils::{
    is_bin_op_code, is_digit_op_code, is_gui_setting_op_code, is_op_in_range, is_unary_op_code,
};
pub use calculator_history::{CalculatorHistory, HistoryItem, HistoryItemVector};
pub use calculator_manager::CalculatorManager;
pub use command::{CalculatorMode, CalculatorPrecision, Command, CommandType, MemoryCommand};
pub use expression_command::{
    BinaryCommand, ExpressionCommand, OpndCommand, Parentheses, SerializeCommandVisitor,
    UnaryCommand,
};
pub use history::{E_BOUNDS, HistoryCollector, MAXPRECDEPTH};
pub use radix_type::RadixType;
pub use resource::{
    EN_US_ENGINE_STRINGS, EngineResourceProvider, ResourceProvider, en_us_engine_string,
};

pub use ratpack::{AngleType, CalcErr, CalcResult, NumberFormat, Rational};
