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
mod restore_fuzz;
mod size;
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

/// The largest single allocation `f` asks for on this thread, in bytes: for
/// the resource-bound tests, which check that nothing a saved state holds
/// makes the restore allocate in proportion to a number in it (other
/// threads, other tests among them, aren't counted).
pub(super) fn largest_allocation(f: impl FnOnce()) -> usize {
    alloc_probe::largest(f)
}

mod alloc_probe {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        /// The largest request so far while measuring, `None` otherwise.
        static LARGEST: Cell<Option<usize>> = const { Cell::new(None) };
    }

    fn note(size: usize) {
        // `try_with`: allocations while the thread is being torn down.
        let _ = LARGEST.try_with(|largest| {
            if let Some(max) = largest.get()
                && size > max
            {
                largest.set(Some(size));
            }
        });
    }

    /// The system allocator, noting each request's size first.
    struct Probe;

    // SAFETY: every call is passed on to `System` unchanged.
    unsafe impl GlobalAlloc for Probe {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            note(layout.size());
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            note(layout.size());
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            note(new_size);
            unsafe { System.realloc(ptr, layout, new_size) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[global_allocator]
    static PROBE: Probe = Probe;

    pub(super) fn largest(f: impl FnOnce()) -> usize {
        LARGEST.with(|largest| largest.set(Some(0)));
        f();
        LARGEST.with(|largest| largest.replace(None)).unwrap_or(0)
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
