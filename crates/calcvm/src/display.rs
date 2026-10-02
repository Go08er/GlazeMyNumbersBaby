// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Port of `Calculator.ViewModels/Common/CalculatorDisplay.cs`.
//!
//! Upstream, `CalculatorDisplay` forwards every engine callback straight to
//! the view model (`ICalcDisplayTarget`) or the history view model
//! (`IHistoryDisplayTarget`), and the view model's handlers call back into the
//! `CalculatorManager` (`GetResultForRadix`, `IsInputEmpty`,
//! `GetHistoryItem`…) while the engine call is still on the stack.
//!
//! The Rust `CalculatorManager` is mutably borrowed while it runs, so the
//! forwarding is split in two: this type *records* the callbacks in order,
//! and the view model replays them through its handlers (the ports of the
//! C# `ICalcDisplayTarget` methods) right after the manager call returns.
//! Every handler therefore sees the engine state at the end of the call
//! rather than in the middle of it, which is indistinguishable for the
//! values the handlers query.

use std::cell::RefCell;
use std::rc::Rc;

use calcmanager::{CalcDisplay, ExpressionCommand, ExpressionToken};

/// One recorded `ICalcDisplay` callback.
#[derive(Clone, Debug)]
pub(crate) enum DisplayCallback {
    PrimaryDisplay(String, bool),
    IsInError(bool),
    ExpressionDisplay(Vec<ExpressionToken>, Vec<ExpressionCommand>),
    ParenthesisNumber(u32),
    NoRightParenAdded,
    MaxDigitsReached,
    BinaryOperatorReceived,
    /// Routed to the history view model upstream (`SetHistoryCallback`).
    HistoryItemAdded(u32),
    MemorizedNumbers(Vec<String>),
    MemoryItemChanged(u32),
    InputChanged,
}

/// `CalculatorDisplay`: the `ICalcDisplay` handed to the `CalculatorManager`.
#[derive(Default)]
pub(crate) struct CalculatorDisplay {
    queue: Vec<DisplayCallback>,
}

/// Shared handle (the manager keeps one clone, the view model the other).
pub(crate) type CalculatorDisplayRef = Rc<RefCell<CalculatorDisplay>>;

impl CalculatorDisplay {
    pub(crate) fn new_shared() -> CalculatorDisplayRef {
        Rc::new(RefCell::new(CalculatorDisplay::default()))
    }

    /// Takes the callbacks recorded since the last call, oldest first.
    pub(crate) fn take(&mut self) -> Vec<DisplayCallback> {
        std::mem::take(&mut self.queue)
    }
}

impl CalcDisplay for CalculatorDisplay {
    fn set_primary_display(&mut self, text: &str, is_error: bool) {
        self.queue
            .push(DisplayCallback::PrimaryDisplay(text.to_string(), is_error));
    }

    fn set_is_in_error(&mut self, is_in_error: bool) {
        self.queue.push(DisplayCallback::IsInError(is_in_error));
    }

    fn set_expression_display(
        &mut self,
        tokens: &[ExpressionToken],
        commands: &[ExpressionCommand],
    ) {
        self.queue.push(DisplayCallback::ExpressionDisplay(
            tokens.to_vec(),
            commands.to_vec(),
        ));
    }

    fn set_parenthesis_number(&mut self, count: u32) {
        self.queue.push(DisplayCallback::ParenthesisNumber(count));
    }

    fn on_no_right_paren_added(&mut self) {
        self.queue.push(DisplayCallback::NoRightParenAdded);
    }

    fn max_digits_reached(&mut self) {
        self.queue.push(DisplayCallback::MaxDigitsReached);
    }

    fn binary_operator_received(&mut self) {
        self.queue.push(DisplayCallback::BinaryOperatorReceived);
    }

    fn on_history_item_added(&mut self, added_item_index: u32) {
        self.queue
            .push(DisplayCallback::HistoryItemAdded(added_item_index));
    }

    fn set_memorized_numbers(&mut self, memorized_numbers: &[String]) {
        self.queue.push(DisplayCallback::MemorizedNumbers(
            memorized_numbers.to_vec(),
        ));
    }

    fn memory_item_changed(&mut self, index_of_memory: u32) {
        self.queue
            .push(DisplayCallback::MemoryItemChanged(index_of_memory));
    }

    fn input_changed(&mut self) {
        self.queue.push(DisplayCallback::InputChanged);
    }
}
