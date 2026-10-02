// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Ports of `Calculator.Tests/StandardCalculatorViewModelTests.cs`,
//! `HistoryTests.cs`, `SnapshotJsonTests.cs` and `SnapshotRoundTripTests.cs`
//! (every case that does not depend on XAML or narrator resources), plus
//! tests for the gmnb contract (programmer strings, bit flips,
//! enablement, paste, persistence, events).

mod gmnb;
mod history;
mod snapshot;
mod standard;

use crate::{Button, CalcMode, CalculatorViewModel};

/// `TestItem`: a command with its expected display and expression. Like
/// upstream's `ValidateViewModelByCommands`, only the display is checked;
/// the expression column is kept from the upstream tables for reference
/// (expressions are asserted separately where they matter).
pub(super) struct TestItem(
    pub Button,
    pub &'static str,
    #[allow(dead_code)] pub &'static str,
);

/// `InitializeViewModel()`: a new view model in Standard mode.
pub(super) fn new_vm() -> CalculatorViewModel {
    CalculatorViewModel::new()
}

/// `ChangeMode(viewModel, mode)`.
pub(super) fn change_mode(vm: &mut CalculatorViewModel, mode: CalcMode) {
    vm.set_mode(mode);
}

/// `ValidateViewModelByCommands(viewModel, items, doReset)`.
pub(super) fn validate_view_model_by_commands(
    vm: &mut CalculatorViewModel,
    items: &[TestItem],
    do_reset: bool,
) {
    if do_reset {
        vm.press(Button::Clear);
        vm.press(Button::ClearEntry);
        vm.press(Button::MemoryClear); // ClearMemoryCommand
    }

    for item in items {
        if item.0 == Button::None {
            break;
        }
        vm.press(item.0);
        if item.1 != "N/A" {
            assert_eq!(vm.display_value(), item.1, "after {:?}", item.0);
        }
    }
}

/// `ValidateViewModelValueAndSecondaryExpression(value, expression)`.
pub(super) fn validate_value_and_expression(
    vm: &CalculatorViewModel,
    value: Option<&str>,
    expression: Option<&str>,
) {
    if let Some(value) = value {
        assert_eq!(vm.display_value(), value);
    }
    if let Some(expression) = expression {
        assert_eq!(vm.expression(), expression);
    }
}
