// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CalculatorHistory.h` / `CalculatorHistory.cpp`.

use std::rc::Rc;

use crate::calc_display::{ExpressionToken, HistoryDisplay};
use crate::expression_command::ExpressionCommand;

/// `HISTORYITEMVECTOR`
#[derive(Clone, Debug, Default)]
pub struct HistoryItemVector {
    pub tokens: Vec<ExpressionToken>,
    pub commands: Vec<ExpressionCommand>,
    pub expression: String,
    pub result: String,
}

/// `HISTORYITEM`
#[derive(Clone, Debug, Default)]
pub struct HistoryItem {
    pub history_item_vector: HistoryItemVector,
}

fn get_generated_expression(tokens: &[ExpressionToken]) -> String {
    let mut expression = String::new();
    let mut is_first = true;

    for token in tokens {
        if is_first {
            is_first = false;
        } else {
            expression.push(' ');
        }
        expression.push_str(&token.0);
    }

    expression
}

/// `CalculationManager::CalculatorHistory`
#[derive(Debug)]
pub struct CalculatorHistory {
    history_items: Vec<Rc<HistoryItem>>,
    max_history_size: usize,
}

impl CalculatorHistory {
    pub fn new(max_size: usize) -> Self {
        CalculatorHistory {
            history_items: Vec::new(),
            max_history_size: max_size,
        }
    }

    pub fn add_item(&mut self, sp_history_item: Rc<HistoryItem>) -> u32 {
        if self.history_items.len() >= self.max_history_size {
            // C++: m_historyItems.erase(m_historyItems.begin())
            if !self.history_items.is_empty() {
                self.history_items.remove(0);
            }
        }

        self.history_items.push(sp_history_item);
        (self.history_items.len() - 1) as u32
    }

    pub fn remove_item(&mut self, u_idx: u32) -> bool {
        if (u_idx as usize) < self.history_items.len() {
            self.history_items.remove(u_idx as usize);
            return true;
        }

        false
    }

    pub fn get_history(&self) -> &[Rc<HistoryItem>] {
        &self.history_items
    }

    /// C++ asserts `uIdx < size()` and uses `at()`; here an out-of-range index yields `None`.
    pub fn get_history_item(&self, u_idx: u32) -> Option<Rc<HistoryItem>> {
        self.history_items.get(u_idx as usize).cloned()
    }

    pub fn clear_history(&mut self) {
        self.history_items.clear();
    }

    pub fn max_history_size(&self) -> usize {
        self.max_history_size
    }
}

impl HistoryDisplay for CalculatorHistory {
    fn add_to_history(
        &mut self,
        tokens: Vec<ExpressionToken>,
        commands: Vec<ExpressionCommand>,
        result: &str,
    ) -> u32 {
        let expression = get_generated_expression(&tokens);
        let sp_history_item = Rc::new(HistoryItem {
            history_item_vector: HistoryItemVector {
                tokens,
                commands,
                expression,
                result: result.to_string(),
            },
        });
        self.add_item(sp_history_item)
    }
}
