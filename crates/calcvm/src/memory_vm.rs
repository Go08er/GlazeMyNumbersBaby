// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Port of `Calculator.ViewModels/MemoryItemViewModel.cs`.
//!
//! The C# item keeps a back-reference to the calculator view model so its
//! `Clear`/`MemoryAdd`/`MemorySubtract` methods can call
//! `OnMemoryClear(Position)` etc.; in Rust those calls go through
//! [`crate::CalculatorViewModel::memory_clear`] and friends with the item's
//! index, which is always equal to its `Position` (the view model renumbers
//! the slots after every insertion/removal, exactly like upstream).

/// `MemoryItemViewModel`
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemoryItemViewModel {
    pub(crate) position: i32,
    pub(crate) value: String,
}

impl Default for MemoryItemViewModel {
    fn default() -> Self {
        MemoryItemViewModel {
            position: -1,
            value: String::new(),
        }
    }
}
