// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Port of `Calculator.ViewModels/HistoryViewModel.cs` and
//! `Calculator.ViewModels/HistoryItemViewModel.cs`.
//!
//! The C# view model keeps a reference to the `CalculatorManagerWrapper`;
//! here the manager is passed to the methods that need it, because it is
//! owned by the calculator view model.
//!
//! Ordering: the manager stores history oldest-first (`AddItem` appends),
//! `Items` is newest-first (`Items.Insert(0, …)`, `ReloadHistory` iterates
//! the manager list in reverse).
//!
//! Not ported: the narrator strings (`AccExpression`, `AccResult`,
//! `HistoryAnnouncement`) and the hide/clicked events, which only drive
//! XAML.

use calcmanager::{
    CalculatorManager, CalculatorMode, ExpressionCommand, ExpressionToken, HistoryItem,
};

use crate::CalcMode;
use crate::localization::LocalizationSettings;

/// `HistoryItemViewModel`
#[derive(Clone, Debug)]
pub(crate) struct HistoryItemViewModel {
    expression: String,
    result: String,
    tokens: Vec<ExpressionToken>,
    commands: Vec<ExpressionCommand>,
}

impl HistoryItemViewModel {
    pub(crate) fn new(
        expression: String,
        result: String,
        tokens: Vec<ExpressionToken>,
        commands: Vec<ExpressionCommand>,
    ) -> Self {
        HistoryItemViewModel {
            expression,
            result,
            tokens,
            commands,
        }
    }

    fn from_manager_item(item: &HistoryItem) -> Self {
        let localizer = LocalizationSettings::get_instance();
        let v = &item.history_item_vector;
        let expression = localizer.localize_display_value(&v.expression);
        let result = localizer.localize_display_value(&v.result);
        HistoryItemViewModel::new(expression, result, v.tokens.clone(), v.commands.clone())
    }

    pub(crate) fn expression(&self) -> &str {
        &self.expression
    }

    pub(crate) fn result(&self) -> &str {
        &self.result
    }

    pub(crate) fn get_tokens(&self) -> &[ExpressionToken] {
        &self.tokens
    }

    pub(crate) fn get_commands(&self) -> &[ExpressionCommand] {
        &self.commands
    }
}

/// `HistoryViewModel`
#[derive(Debug)]
pub(crate) struct HistoryViewModel {
    current_mode: CalculatorMode,
    items: Vec<HistoryItemViewModel>,
    are_history_shortcuts_enabled: bool,
}

impl HistoryViewModel {
    pub(crate) fn new() -> Self {
        HistoryViewModel {
            current_mode: CalculatorMode::Standard,
            items: Vec::new(),
            are_history_shortcuts_enabled: true,
        }
    }

    /// `Items` (newest first).
    pub(crate) fn items(&self) -> &[HistoryItemViewModel] {
        &self.items
    }

    pub(crate) fn items_count(&self) -> usize {
        self.items.len()
    }

    pub(crate) fn set_are_history_shortcuts_enabled(&mut self, enabled: bool) {
        self.are_history_shortcuts_enabled = enabled;
    }

    /// `IHistoryDisplayTarget.OnHistoryItemAdded`
    pub(crate) fn on_history_item_added(
        &mut self,
        manager: &CalculatorManager,
        added_item_index: u32,
    ) {
        let Some(new_item) = manager.get_history_item(added_item_index) else {
            return;
        };

        let item = HistoryItemViewModel::from_manager_item(&new_item);

        // Check if we have hit the max items
        if self.items.len() >= manager.max_history_size() {
            self.items.pop();
        }

        self.items.insert(0, item);
    }

    /// `OnClear` (the `ClearCommand`).
    pub(crate) fn on_clear(&mut self, manager: &mut CalculatorManager) {
        if self.are_history_shortcuts_enabled {
            manager.clear_history();
            self.clear_items();
        }
    }

    /// `ClearItems`
    pub(crate) fn clear_items(&mut self) {
        self.items.clear();
    }

    /// `DeleteItem(HistoryItemViewModel e)`, with `e` given by its position in
    /// [`items`](Self::items).
    ///
    /// Deviation: upstream passes the position in `Items` (newest-first)
    /// straight to `CalculatorManager.RemoveHistoryItem`, whose list is
    /// oldest-first, so the engine drops a different entry than the one
    /// removed from the list (visible after the next `ReloadHistory`). The
    /// index is translated here so both lists lose the same item.
    pub(crate) fn delete_item(
        &mut self,
        manager: &mut CalculatorManager,
        item_index: usize,
    ) -> bool {
        if item_index >= self.items.len() {
            return false;
        }
        let manager_len = manager.get_history_items().len();
        if item_index >= manager_len {
            return false;
        }
        let manager_index = (manager_len - 1 - item_index) as u32;
        if manager.remove_history_item(manager_index) {
            self.items.remove(item_index);
            return true;
        }
        false
    }

    /// `ReloadHistory(ViewMode currentMode)`
    pub(crate) fn reload_history(&mut self, manager: &CalculatorManager, current_mode: CalcMode) {
        self.current_mode = match current_mode {
            CalcMode::Standard => CalculatorMode::Standard,
            CalcMode::Scientific => CalculatorMode::Scientific,
            CalcMode::Programmer => return,
        };

        let history_list_model = manager.get_history_items_for_mode(self.current_mode);
        // Iterate in reverse order
        self.items = history_list_model
            .iter()
            .rev()
            .map(|h| HistoryItemViewModel::from_manager_item(h))
            .collect();
    }

    /// `GetMaxItemSize`
    #[cfg(test)]
    pub(crate) fn get_max_item_size(&self, manager: &CalculatorManager) -> usize {
        manager.max_history_size()
    }
}
