// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CalculatorManager.h` / `CalculatorManager.cpp`.
//!
//! In C++ `CalculatorManager` *is* the `ICalcDisplay` handed to its three
//! engines (a raw `this` pointer) and forwards to the UI's callback. In Rust
//! that self-reference is split out into a small proxy ([`ManagerDisplay`])
//! that the engines share via `Rc<RefCell<..>>`; it holds the UI callback
//! and the `m_inHistoryItemLoadMode` flag, and gates exactly the two
//! callbacks the C++ manager gates (`SetPrimaryDisplay`,
//! `SetExpressionDisplay`).

use std::cell::RefCell;
use std::rc::Rc;

use ratpack::{CalcResult, Rational};

use crate::calc_display::{CalcDisplay, CalcDisplayRef, ExpressionToken, HistoryDisplayRef};
use crate::calc_engine::CalcEngine;
use crate::calculator_history::{CalculatorHistory, HistoryItem};
use crate::ccommand::*;
use crate::command::{CalculatorMode, CalculatorPrecision, Command};
use crate::expression_command::ExpressionCommand;
use crate::history::E_BOUNDS;
use crate::radix_type::RadixType;
use crate::resource::ResourceProvider;

const MAX_HISTORY_ITEMS: usize = 20;

/// The engine-facing half of `CalculatorManager`'s `ICalcDisplay`
/// implementation.
struct ManagerDisplay {
    display_callback: CalcDisplayRef,
    in_history_item_load_mode: bool,
}

impl CalcDisplay for ManagerDisplay {
    /// Used to set the primary display value on ViewModel
    fn set_primary_display(&mut self, display_string: &str, is_error: bool) {
        if !self.in_history_item_load_mode {
            self.display_callback
                .borrow_mut()
                .set_primary_display(display_string, is_error);
        }
    }

    fn set_is_in_error(&mut self, is_error: bool) {
        self.display_callback.borrow_mut().set_is_in_error(is_error);
    }

    /// Used to set the expression display value on ViewModel
    fn set_expression_display(
        &mut self,
        tokens: &[ExpressionToken],
        commands: &[ExpressionCommand],
    ) {
        if !self.in_history_item_load_mode {
            self.display_callback
                .borrow_mut()
                .set_expression_display(tokens, commands);
        }
    }

    fn set_parenthesis_number(&mut self, parenthesis_count: u32) {
        self.display_callback
            .borrow_mut()
            .set_parenthesis_number(parenthesis_count);
    }

    fn on_no_right_paren_added(&mut self) {
        self.display_callback.borrow_mut().on_no_right_paren_added();
    }

    fn max_digits_reached(&mut self) {
        self.display_callback.borrow_mut().max_digits_reached();
    }

    fn binary_operator_received(&mut self) {
        self.display_callback
            .borrow_mut()
            .binary_operator_received();
    }

    fn on_history_item_added(&mut self, added_item_index: u32) {
        self.display_callback
            .borrow_mut()
            .on_history_item_added(added_item_index);
    }

    fn set_memorized_numbers(&mut self, memorized_numbers: &[String]) {
        self.display_callback
            .borrow_mut()
            .set_memorized_numbers(memorized_numbers);
    }

    fn memory_item_changed(&mut self, index_of_memory: u32) {
        self.display_callback
            .borrow_mut()
            .memory_item_changed(index_of_memory);
    }

    fn input_changed(&mut self) {
        self.display_callback.borrow_mut().input_changed();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EngineSlot {
    Standard,
    Scientific,
    Programmer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistorySlot {
    Standard,
    Scientific,
}

/// `CalculationManager::CalculatorManager`.
///
/// Owns the standard / scientific / programmer engines, the memory list and
/// the two history lists. All display updates are delivered synchronously to
/// the [`CalcDisplay`] passed to [`CalculatorManager::new`].
///
/// Methods that could let a C++ exception escape return `CalcResult`; an
/// `Err` carries the same `uint32_t` code the C++ would have thrown, and the
/// manager is left in the same (partially updated) state.
pub struct CalculatorManager {
    display_callback: CalcDisplayRef,
    proxy: Rc<RefCell<ManagerDisplay>>,
    current_calculator_engine: Option<EngineSlot>,
    scientific_calculator_engine: Option<CalcEngine>,
    standard_calculator_engine: Option<CalcEngine>,
    programmer_calculator_engine: Option<CalcEngine>,
    resource_provider: Rc<dyn ResourceProvider>,

    memorized_numbers: Vec<Rational>,
    persisted_primary_value: Rational,
    is_exponential_format: bool,
    current_degree_mode: Command,

    std_history: Rc<RefCell<CalculatorHistory>>,
    sci_history: Rc<RefCell<CalculatorHistory>>,
    history: Option<HistorySlot>,
}

impl CalculatorManager {
    const MAXIMUM_MEMORY_SIZE: usize = 100;

    /// `CalculatorManager(ICalcDisplay* displayCallback, IResourceProvider* resourceProvider)`
    pub fn new(
        display_callback: CalcDisplayRef,
        resource_provider: Rc<dyn ResourceProvider>,
    ) -> Self {
        CalcEngine::initial_one_time_only_setup(&*resource_provider);

        let proxy = Rc::new(RefCell::new(ManagerDisplay {
            display_callback: display_callback.clone(),
            in_history_item_load_mode: false,
        }));

        CalculatorManager {
            display_callback,
            proxy,
            current_calculator_engine: None,
            scientific_calculator_engine: None,
            standard_calculator_engine: None,
            programmer_calculator_engine: None,
            resource_provider,
            memorized_numbers: Vec::new(),
            persisted_primary_value: Rational::default(),
            is_exponential_format: false,
            current_degree_mode: Command::CommandNULL,
            std_history: Rc::new(RefCell::new(CalculatorHistory::new(MAX_HISTORY_ITEMS))),
            sci_history: Rc::new(RefCell::new(CalculatorHistory::new(MAX_HISTORY_ITEMS))),
            history: None,
        }
    }

    fn engine_display(&self) -> Option<CalcDisplayRef> {
        let d: CalcDisplayRef = self.proxy.clone();
        Some(d)
    }

    /// `m_currentCalculatorEngine` (dereferencing a null engine is a crash in
    /// C++; here it panics).
    fn current_engine(&mut self) -> &mut CalcEngine {
        let slot = self
            .current_calculator_engine
            .expect("CalculatorManager: no current calculator engine (set a mode first)");
        self.engine_mut(slot)
    }

    fn engine_mut(&mut self, slot: EngineSlot) -> &mut CalcEngine {
        let engine = match slot {
            EngineSlot::Standard => self.standard_calculator_engine.as_mut(),
            EngineSlot::Scientific => self.scientific_calculator_engine.as_mut(),
            EngineSlot::Programmer => self.programmer_calculator_engine.as_mut(),
        };
        engine.expect("engine slot without engine")
    }

    fn current_engine_ref(&self) -> Option<&CalcEngine> {
        match self.current_calculator_engine? {
            EngineSlot::Standard => self.standard_calculator_engine.as_ref(),
            EngineSlot::Scientific => self.scientific_calculator_engine.as_ref(),
            EngineSlot::Programmer => self.programmer_calculator_engine.as_ref(),
        }
    }

    fn history_ref(&self) -> Option<&Rc<RefCell<CalculatorHistory>>> {
        match self.history? {
            HistorySlot::Standard => Some(&self.std_history),
            HistorySlot::Scientific => Some(&self.sci_history),
        }
    }

    // ------------------------------------------------------------------
    // ICalcDisplay methods the C++ manager exposes publicly
    // ------------------------------------------------------------------

    pub fn display_paste_error(&mut self) {
        self.current_engine().display_error(
            ratpack::CALC_E_DOMAIN, /*code for "Invalid input" error*/
        );
    }

    fn input_changed(&mut self) {
        self.display_callback.borrow_mut().input_changed();
    }

    fn on_history_item_added(&mut self, added_item_index: u32) {
        self.display_callback
            .borrow_mut()
            .on_history_item_added(added_item_index);
    }

    /// Reset CalculatorManager.
    /// Set the mode to the standard calculator
    /// Set the degree mode as regular degree (as oppose to Rad or Grad)
    /// Clear all the entries and memories
    /// Clear Memory if clearMemory parameter is true.(Default value is true)
    pub fn reset(&mut self, clear_memory: bool) -> CalcResult<()> {
        self.set_standard_mode()?;

        if let Some(engine) = self.scientific_calculator_engine.as_mut() {
            engine.process_command(IDC_CLEAR)?;
            engine.process_command(IDC_DEG)?;

            if self.is_exponential_format {
                self.is_exponential_format = false;
                engine.process_command(IDC_FE)?;
            }
        }
        self.current_degree_mode = Command::CommandDEG;

        if let Some(engine) = self.programmer_calculator_engine.as_mut() {
            engine.process_command(IDC_CLEAR)?;
            engine.process_command(IDC_QWORD)?;
        }

        if clear_memory {
            self.memorized_number_clear_all()?;
        }
        Ok(())
    }

    /// Change the current calculator engine to standard calculator engine.
    pub fn set_standard_mode(&mut self) -> CalcResult<()> {
        if self.standard_calculator_engine.is_none() {
            let history: HistoryDisplayRef = self.std_history.clone();
            self.standard_calculator_engine = Some(CalcEngine::new(
                false, /* Respect Order of Operations */
                false, /* Set to Integer Mode */
                self.resource_provider.clone(),
                self.engine_display(),
                Some(history),
            )?);
        }

        self.current_calculator_engine = Some(EngineSlot::Standard);
        self.current_engine().process_command(IDC_DEC)?;
        self.current_engine().process_command(IDC_CLEAR)?;
        self.current_engine()
            .change_precision(CalculatorPrecision::StandardModePrecision as i32);
        self.update_max_int_digits();
        self.history = Some(HistorySlot::Standard);
        Ok(())
    }

    /// Change the current calculator engine to scientific calculator engine.
    pub fn set_scientific_mode(&mut self) -> CalcResult<()> {
        if self.scientific_calculator_engine.is_none() {
            let history: HistoryDisplayRef = self.sci_history.clone();
            self.scientific_calculator_engine = Some(CalcEngine::new(
                true,  /* Respect Order of Operations */
                false, /* Set to Integer Mode */
                self.resource_provider.clone(),
                self.engine_display(),
                Some(history),
            )?);
        }

        self.current_calculator_engine = Some(EngineSlot::Scientific);
        self.current_engine().process_command(IDC_DEC)?;
        self.current_engine().process_command(IDC_CLEAR)?;
        self.current_engine()
            .change_precision(CalculatorPrecision::ScientificModePrecision as i32);
        self.history = Some(HistorySlot::Scientific);
        Ok(())
    }

    /// Change the current calculator engine to programmer calculator engine.
    /// (Like the C++, this does not change which history list is current.)
    pub fn set_programmer_mode(&mut self) -> CalcResult<()> {
        if self.programmer_calculator_engine.is_none() {
            self.programmer_calculator_engine = Some(CalcEngine::new(
                true, /* Respect Order of Operations */
                true, /* Set to Integer Mode */
                self.resource_provider.clone(),
                self.engine_display(),
                None,
            )?);
        }

        self.current_calculator_engine = Some(EngineSlot::Programmer);
        self.current_engine().process_command(IDC_DEC)?;
        self.current_engine().process_command(IDC_CLEAR)?;
        self.current_engine()
            .change_precision(CalculatorPrecision::ProgrammerModePrecision as i32);
        Ok(())
    }

    /// Send command to the Calc Engine
    /// Cast Command Enum to OpCode.
    /// Handle special commands such as mode change and combination of two commands.
    pub fn send_command(&mut self, command: Command) -> CalcResult<()> {
        // When the expression line is cleared, we save the current state, which includes,
        // primary display, memory, and degree mode
        if command == Command::CommandCLEAR
            || command == Command::CommandEQU
            || command == Command::ModeBasic
            || command == Command::ModeScientific
            || command == Command::ModeProgrammer
        {
            match command {
                Command::ModeBasic => self.set_standard_mode()?,
                Command::ModeScientific => self.set_scientific_mode()?,
                Command::ModeProgrammer => self.set_programmer_mode()?,
                _ => self.current_engine().process_command(command.0)?,
            }

            self.input_changed();
            return Ok(());
        }

        if command == Command::CommandDEG
            || command == Command::CommandRAD
            || command == Command::CommandGRAD
        {
            self.current_degree_mode = command;
        }

        let inv_then = |op: Command| Some(op.0);
        let second = match command {
            Command::CommandASIN => inv_then(Command::CommandSIN),
            Command::CommandACOS => inv_then(Command::CommandCOS),
            Command::CommandATAN => inv_then(Command::CommandTAN),
            Command::CommandPOWE => inv_then(Command::CommandLN),
            Command::CommandASINH => inv_then(Command::CommandSINH),
            Command::CommandACOSH => inv_then(Command::CommandCOSH),
            Command::CommandATANH => inv_then(Command::CommandTANH),
            Command::CommandASEC => inv_then(Command::CommandSEC),
            Command::CommandACSC => inv_then(Command::CommandCSC),
            Command::CommandACOT => inv_then(Command::CommandCOT),
            Command::CommandASECH => inv_then(Command::CommandSECH),
            Command::CommandACSCH => inv_then(Command::CommandCSCH),
            Command::CommandACOTH => inv_then(Command::CommandCOTH),
            _ => None,
        };

        match second {
            Some(op) => {
                self.current_engine()
                    .process_command(Command::CommandINV.0)?;
                self.current_engine().process_command(op)?;
            }
            None => {
                if command == Command::CommandFE {
                    self.is_exponential_format = !self.is_exponential_format;
                }
                self.current_engine().process_command(command.0)?;
            }
        }

        self.input_changed();
        Ok(())
    }

    /// Load the persisted value that is saved in memory of CalcEngine
    #[allow(dead_code)]
    fn load_persisted_primary_value(&mut self) -> CalcResult<()> {
        let v = self.persisted_primary_value.clone();
        self.current_engine().set_persisted_mem_object(&v);
        self.current_engine().process_command(IDC_RECALL)?;
        self.input_changed();
        Ok(())
    }

    /// Memorize the current displayed value
    /// Notify the client with new the new memorize value vector
    pub fn memorize_number(&mut self) -> CalcResult<()> {
        if self.current_engine().f_in_error_state() {
            return Ok(());
        }

        self.current_engine().process_command(IDC_STORE)?;

        if let Some(memory_object) = self.current_engine().persisted_mem_object() {
            self.memorized_numbers.insert(0, memory_object);
        }

        if self.memorized_numbers.len() > Self::MAXIMUM_MEMORY_SIZE {
            self.memorized_numbers.truncate(Self::MAXIMUM_MEMORY_SIZE);
        }
        self.set_memorized_numbers_string()
    }

    /// Recall the memorized number.
    /// The memorized number gets loaded to the primary display
    pub fn memorized_number_load(&mut self, index_of_memory: u32) -> CalcResult<()> {
        if self.current_engine().f_in_error_state() {
            return Ok(());
        }

        self.memorized_number_select(index_of_memory)?;
        self.current_engine().process_command(IDC_RECALL)?;
        self.input_changed();
        Ok(())
    }

    /// Do the addition to the selected memory
    /// It adds primary display value to the selected memory
    /// Notify the client with new the new memorize value vector
    pub fn memorized_number_add(&mut self, index_of_memory: u32) -> CalcResult<()> {
        if self.current_engine().f_in_error_state() {
            return Ok(());
        }

        if self.memorized_numbers.is_empty() {
            self.memorize_number()?;
        } else {
            self.memorized_number_select(index_of_memory)?;
            self.current_engine().process_command(IDC_MPLUS)?;

            self.memorized_number_changed(index_of_memory)?;

            self.set_memorized_numbers_string()?;
        }

        self.display_callback
            .borrow_mut()
            .memory_item_changed(index_of_memory);
        Ok(())
    }

    pub fn memorized_number_clear(&mut self, index_of_memory: u32) {
        if (index_of_memory as usize) < self.memorized_numbers.len() {
            self.memorized_numbers.remove(index_of_memory as usize);
        }
    }

    /// Do the subtraction to the selected memory
    /// It adds primary display value to the selected memory
    /// Notify the client with new the new memorize value vector
    pub fn memorized_number_subtract(&mut self, index_of_memory: u32) -> CalcResult<()> {
        if self.current_engine().f_in_error_state() {
            return Ok(());
        }

        // To add negative of the number on display to the memory -x = x - 2x
        if self.memorized_numbers.is_empty() {
            self.memorize_number()?;
            self.memorized_number_subtract(0)?;
            self.memorized_number_subtract(0)?;
        } else {
            self.memorized_number_select(index_of_memory)?;
            self.current_engine().process_command(IDC_MMINUS)?;

            self.memorized_number_changed(index_of_memory)?;

            self.set_memorized_numbers_string()?;
        }

        self.display_callback
            .borrow_mut()
            .memory_item_changed(index_of_memory);
        Ok(())
    }

    /// Clear all the memorized values
    /// Notify the client with new the new memorize value vector
    pub fn memorized_number_clear_all(&mut self) -> CalcResult<()> {
        self.memorized_numbers.clear();

        self.current_engine().process_command(IDC_MCLEAR)?;
        self.set_memorized_numbers_string()
    }

    /// Helper function that selects a memory from the vector and set it to CCalcEngine
    /// Saved RAT number needs to be copied and passed in, as CCalcEngine destroyed the passed in RAT
    /// (`m_memorizedNumbers.at()` throws `std::out_of_range` in C++; here `Err(E_BOUNDS)`.)
    fn memorized_number_select(&mut self, index_of_memory: u32) -> CalcResult<()> {
        if self.current_engine().f_in_error_state() {
            return Ok(());
        }

        let memory_object = self
            .memorized_numbers
            .get(index_of_memory as usize)
            .cloned()
            .ok_or(E_BOUNDS)?;
        self.current_engine()
            .set_persisted_mem_object(&memory_object);
        Ok(())
    }

    /// Helper function that needs to be executed when memory is modified
    /// When memory is modified, destroy the old RAT and put the new RAT in vector
    fn memorized_number_changed(&mut self, index_of_memory: u32) -> CalcResult<()> {
        if self.current_engine().f_in_error_state() {
            return Ok(());
        }

        if let Some(memory_object) = self.current_engine().persisted_mem_object() {
            let slot = self
                .memorized_numbers
                .get_mut(index_of_memory as usize)
                .ok_or(E_BOUNDS)?;
            *slot = memory_object;
        }
        Ok(())
    }

    /// `GetHistoryItems()` — the history of the current standard/scientific
    /// mode (empty before any mode was set; C++ dereferences null there).
    pub fn get_history_items(&self) -> Vec<Rc<HistoryItem>> {
        self.history_ref()
            .map(|h| h.borrow().get_history().to_vec())
            .unwrap_or_default()
    }

    /// `GetHistoryItems(CalculatorMode mode)`
    pub fn get_history_items_for_mode(&self, mode: CalculatorMode) -> Vec<Rc<HistoryItem>> {
        if mode == CalculatorMode::Standard {
            self.std_history.borrow().get_history().to_vec()
        } else {
            self.sci_history.borrow().get_history().to_vec()
        }
    }

    pub fn set_history_items(&mut self, history_items: &[Rc<HistoryItem>]) {
        for history_item in history_items {
            let index = match self.history_ref() {
                Some(h) => h.borrow_mut().add_item(history_item.clone()),
                None => continue,
            };
            self.on_history_item_added(index);
        }
    }

    pub fn get_history_item(&self, u_idx: u32) -> Option<Rc<HistoryItem>> {
        self.history_ref()
            .and_then(|h| h.borrow().get_history_item(u_idx))
    }

    pub fn remove_history_item(&mut self, u_idx: u32) -> bool {
        self.history_ref()
            .is_some_and(|h| h.borrow_mut().remove_item(u_idx))
    }

    pub fn clear_history(&mut self) {
        if let Some(h) = self.history_ref() {
            h.borrow_mut().clear_history();
        }
    }

    pub fn max_history_size(&self) -> usize {
        self.history_ref()
            .map_or(MAX_HISTORY_ITEMS, |h| h.borrow().max_history_size())
    }

    pub fn set_radix(&mut self, i_radix_type: RadixType) -> CalcResult<()> {
        match i_radix_type {
            RadixType::Hex => self.current_engine().process_command(IDC_HEX)?,
            RadixType::Decimal => self.current_engine().process_command(IDC_DEC)?,
            RadixType::Octal => self.current_engine().process_command(IDC_OCT)?,
            RadixType::Binary => self.current_engine().process_command(IDC_BIN)?,
        }
        self.set_memorized_numbers_string()
    }

    pub fn set_memorized_numbers_string(&mut self) -> CalcResult<()> {
        let mut result_vector: Vec<String> = Vec::new();
        let engine = self
            .current_engine_ref()
            .expect("CalculatorManager: no current calculator engine (set a mode first)");
        for memory_item in &self.memorized_numbers {
            let radix = engine.get_current_radix();
            let string_value = engine.get_string_for_display(memory_item, radix)?;

            if !string_value.is_empty() {
                result_vector.push(engine.group_digits_per_radix(&string_value, radix));
            }
        }
        self.display_callback
            .borrow_mut()
            .set_memorized_numbers(&result_vector);
        Ok(())
    }

    pub fn get_current_degree_mode(&mut self) -> Command {
        if self.current_degree_mode == Command::CommandNULL {
            self.current_degree_mode = Command::CommandDEG;
        }
        self.current_degree_mode
    }

    pub fn get_result_for_radix(
        &mut self,
        radix: u32,
        precision: i32,
        group_digits_per_radix: bool,
    ) -> CalcResult<String> {
        if self.current_calculator_engine.is_some() {
            self.current_engine().get_current_result_for_radix(
                radix,
                precision,
                group_digits_per_radix,
            )
        } else {
            Ok(String::new())
        }
    }

    pub fn set_precision(&mut self, precision: i32) {
        self.current_engine().change_precision(precision);
    }

    pub fn update_max_int_digits(&mut self) {
        self.current_engine().update_max_int_digits();
    }

    pub fn decimal_separator(&self) -> char {
        match self.current_engine_ref() {
            Some(engine) => engine.decimal_separator(),
            None => self
                .resource_provider
                .get_cengine_string("sDecimal")
                .chars()
                .next()
                .unwrap_or('\0'),
        }
    }

    pub fn is_engine_recording(&mut self) -> bool {
        self.current_engine().f_in_recording_state()
    }

    pub fn is_input_empty(&mut self) -> bool {
        self.current_engine().is_input_empty()
    }

    pub fn set_in_history_item_load_mode(&mut self, is_history_item_load_mode: bool) {
        self.proxy.borrow_mut().in_history_item_load_mode = is_history_item_load_mode;
    }

    pub fn get_display_commands_snapshot(&self) -> Vec<ExpressionCommand> {
        self.current_engine_ref()
            .expect("CalculatorManager: no current calculator engine (set a mode first)")
            .get_history_collector_commands_snapshot()
    }

    /// The UI callback this manager reports to.
    pub fn display_callback(&self) -> &CalcDisplayRef {
        &self.display_callback
    }

    /// The current engine, if a mode has been set (read-only access for UIs
    /// and tests; not part of the C++ public surface).
    pub fn current_calculator_engine(&self) -> Option<&CalcEngine> {
        self.current_engine_ref()
    }
}
