// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `Header Files/ICalcDisplay.h` and `Header Files/IHistoryDisplay.h`.
//!
//! The C++ callbacks receive `shared_ptr`s to the token / command vectors;
//! here they receive borrowed slices (a UI that wants to keep them clones
//! them). History receives the vectors by value because the collector
//! relinquishes them at that point (the C++ code nulls its own pointers right
//! after the call).

use std::cell::RefCell;
use std::rc::Rc;

use crate::expression_command::ExpressionCommand;

/// One token of the expression display: the text and the index of the
/// command (in the accompanying command list) that produced it, or `-1`.
pub type ExpressionToken = (String, i32);

/// `ICalcDisplay` — callback interface to be implemented by the clients of
/// the engine / `CalculatorManager`.
///
/// Callbacks are invoked synchronously from inside the engine call that
/// caused them. Implementations must not call back into the
/// `CalculatorManager` from a callback (the manager is mutably borrowed);
/// record what you need and query the manager after the call returns.
pub trait CalcDisplay {
    fn set_primary_display(&mut self, text: &str, is_error: bool);
    fn set_is_in_error(&mut self, is_in_error: bool);
    fn set_expression_display(
        &mut self,
        tokens: &[ExpressionToken],
        commands: &[ExpressionCommand],
    );
    fn set_parenthesis_number(&mut self, count: u32);
    fn on_no_right_paren_added(&mut self);
    /// not an error but still need to inform UI layer.
    fn max_digits_reached(&mut self);
    fn binary_operator_received(&mut self);
    fn on_history_item_added(&mut self, added_item_index: u32);
    fn set_memorized_numbers(&mut self, memorized_numbers: &[String]);
    fn memory_item_changed(&mut self, index_of_memory: u32);
    fn input_changed(&mut self);
}

/// `IHistoryDisplay` — callback interface to be implemented by the clients of
/// the engine if they require equation history.
pub trait HistoryDisplay {
    fn add_to_history(
        &mut self,
        tokens: Vec<ExpressionToken>,
        commands: Vec<ExpressionCommand>,
        result: &str,
    ) -> u32;
}

/// Shared handle to a display (the C++ engine stores a raw `ICalcDisplay*`).
pub type CalcDisplayRef = Rc<RefCell<dyn CalcDisplay>>;

/// Shared handle to a history sink (C++: `std::shared_ptr<IHistoryDisplay>`).
pub type HistoryDisplayRef = Rc<RefCell<dyn HistoryDisplay>>;
