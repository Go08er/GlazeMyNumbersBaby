// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Port of `Calculator.ViewModels/StandardCalculatorViewModel.cs` (the view
//! model shared by Standard, Scientific and Programmer mode).
//!
//! Structure follows the C# class: property setters with their side effects
//! (`IsStandard`, `IsScientific`, `IsProgrammer`, `ValueBitLength`,
//! `IsFToEChecked`), the `ICalcDisplayTarget` callback handlers
//! (`SetPrimaryDisplay`, `SetExpressionDisplay`, …), `OnButtonPressed`,
//! the memory/history/paste commands and `Recalculate`.
//!
//! What is left out: everything that only feeds XAML/narrator (automation
//! names, announcements, `DisplayExpressionToken` metadata other than the
//! text, trace logging) and expression editing (`IsEditingEnabled` is
//! always false upstream too — `SaveEditedCommand`/`UpdateOperand` are
//! no-ops in the C# port).
//!
//! Engine callbacks are recorded by [`CalculatorDisplay`] and replayed
//! through the handlers after each manager call (see `display.rs`); use
//! [`StandardCalculatorViewModel::send_command`] and friends, which drain
//! the queue, rather than calling the manager directly.

use std::rc::Rc;

use calcmanager::{
    CalcDisplayRef, CalculatorManager, Command, CommandType, EngineResourceProvider,
    ExpressionCommand, ExpressionToken, NumWidth, RadixType,
};
use copypaste::{BitLength, NumberBase, PasteCommand, ViewMode};

use crate::display::{CalculatorDisplay, CalculatorDisplayRef, DisplayCallback};
use crate::history_vm::{HistoryItemViewModel, HistoryViewModel};
use crate::localization::LocalizationSettings;
use crate::memory_vm::MemoryItemViewModel;
use crate::{AngleUnit, Button, CalcMode, Event, Radix, ShiftMode, WordSize};

const STANDARD_MODE_PRECISION: i32 = 16;
const SCIENTIFIC_MODE_PRECISION: i32 = 32;
const PROGRAMMER_MODE_PRECISION: i32 = 64;

/// `CalculatorCommand` ids used by the view model (same numbers as
/// `NumbersAndOperatorsEnum` / [`Command`]).
pub(crate) mod cmd {
    pub const SIGN: i32 = 80;
    pub const CLEAR: i32 = 81;
    pub const CENTR: i32 = 82;
    pub const BACK: i32 = 83;
    pub const PNT: i32 = 84;
    pub const ADD: i32 = 93;
    pub const SUB: i32 = 94;
    pub const MUL: i32 = 92;
    pub const DIV: i32 = 91;
    pub const MOD: i32 = 95;
    pub const PWR: i32 = 97;
    pub const FE: i32 = 119;
    pub const EQU: i32 = 121;
    pub const EXP: i32 = 127;
    pub const OPENP: i32 = 128;
    pub const CLOSEP: i32 = 129;
    pub const ZERO: i32 = 130;
    pub const NINE: i32 = 139;
    pub const A: i32 = 140;
    pub const F: i32 = 145;
    pub const MEMORY: i32 = 146;
    pub const MODE_SCIENTIFIC: i32 = 201;
    pub const QWORD: i32 = 317;
    pub const DWORD: i32 = 318;
    pub const WORD: i32 = 319;
    pub const BYTE: i32 = 320;
    pub const DEG: i32 = 321;
    pub const RAD: i32 = 322;
    pub const GRAD: i32 = 323;
    pub const BINEDITSTART: i32 = 700;
    pub const BINEDITEND: i32 = 763;
}

/// What a contract-level operation is, for [`Event`] derivation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpKind {
    /// A button sent through `OnButtonPressed`: the command and whether the
    /// calculator was showing an error when it was pressed.
    Button { command: i32, was_in_error: bool },
    /// The value is replaced wholesale (clear, recall, paste, mode/radix/word
    /// size switch, F-E, restore).
    Replace,
    /// The current operand is edited in place (bit flip).
    Typing,
    /// No display change expected (memory store/add/subtract/clear, history
    /// list edits, angle unit).
    Quiet,
}

/// Which callbacks fired during the current operation.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OpTrace {
    primary: bool,
    expression: bool,
    error_entered: bool,
    binary_operator: bool,
    history_added: bool,
    memory: bool,
    forced_replace: bool,
    forced_history: bool,
}

/// `StandardCalculatorViewModel`
pub(crate) struct StandardCalculatorViewModel {
    calculator_display: CalculatorDisplayRef,
    pub(crate) standard_calculator_manager: CalculatorManager,
    pub(crate) history_vm: HistoryViewModel,

    // State fields
    pub(crate) current_angle_type: i32,
    is_standard: bool,
    is_scientific: bool,
    is_programmer: bool,
    is_bit_flip_checked: bool,
    is_last_operation_history_load: bool,
    pub(crate) value_bit_length: WordSize,
    pub(crate) shift_mode: ShiftMode,

    // Expression data
    pub(crate) tokens: Vec<ExpressionToken>,
    pub(crate) commands: Vec<ExpressionCommand>,

    pub(crate) display_value: String,
    pub(crate) is_in_error: bool,
    is_operator_command: bool,
    /// `ExpressionTokens` (only the token text is kept).
    expression_tokens: Vec<String>,
    decimal_display_value: String,
    hex_display_value: String,
    octal_display_value: String,
    binary_display_value: String,
    binary_digits: [bool; 64],
    pub(crate) memorized_numbers: Vec<MemoryItemViewModel>,
    is_memory_empty: bool,
    pub(crate) is_f_to_e_checked: bool,
    is_f_to_e_enabled: bool,
    are_hex_buttons_enabled: bool,
    pub(crate) current_radix_type: Radix,
    is_input_empty: bool,
    open_parenthesis_count: u32,

    // Port plumbing
    programmer_panel_dirty: bool,
    trace: OpTrace,
    events: Vec<Event>,
}

impl StandardCalculatorViewModel {
    /// `StandardCalculatorViewModel()` — no mode is set yet (the app sets
    /// `IsStandard`/`SetCalculatorType` right after construction).
    pub(crate) fn new() -> Self {
        let calculator_display = CalculatorDisplay::new_shared();
        let display_ref: CalcDisplayRef = calculator_display.clone();
        let manager =
            CalculatorManager::new(display_ref, Rc::new(EngineResourceProvider::default()));

        StandardCalculatorViewModel {
            calculator_display,
            standard_calculator_manager: manager,
            history_vm: HistoryViewModel::new(),
            current_angle_type: cmd::DEG,
            is_standard: false,
            is_scientific: false,
            is_programmer: false,
            is_bit_flip_checked: false,
            is_last_operation_history_load: false,
            value_bit_length: WordSize::Qword,
            shift_mode: ShiftMode::Arithmetic,
            tokens: Vec::new(),
            commands: Vec::new(),
            display_value: "0".to_string(),
            is_in_error: false,
            is_operator_command: false,
            expression_tokens: Vec::new(),
            decimal_display_value: "0".to_string(),
            hex_display_value: "0".to_string(),
            octal_display_value: "0".to_string(),
            binary_display_value: "0".to_string(),
            binary_digits: [false; 64],
            memorized_numbers: Vec::new(),
            is_memory_empty: true,
            is_f_to_e_checked: false,
            // The constructor's `IsOperandEnabled = true` sets this.
            is_f_to_e_enabled: true,
            are_hex_buttons_enabled: false,
            current_radix_type: Radix::Dec,
            // C# default(bool); the first `OnInputChanged` sets the real value.
            is_input_empty: false,
            open_parenthesis_count: 0,
            programmer_panel_dirty: false,
            trace: OpTrace::default(),
            events: Vec::new(),
        }
    }

    // ------------------------------------------------------------------
    // Port plumbing: manager calls + callback replay + events
    // ------------------------------------------------------------------

    /// `_standardCalculatorManager.SendCommand((CalculatorCommand)command)`
    /// followed by the replay of the callbacks it produced.
    pub(crate) fn send_command(&mut self, command: i32) {
        // The shipping app never catches engine exceptions; an `Err` leaves
        // the engine in the same state the C++ would be in.
        let _ = self
            .standard_calculator_manager
            .send_command(Command(command));
        self.drain();
    }

    /// Runs `f` on the manager and replays the callbacks it produced.
    pub(crate) fn with_manager<R>(&mut self, f: impl FnOnce(&mut CalculatorManager) -> R) -> R {
        let r = f(&mut self.standard_calculator_manager);
        self.drain();
        r
    }

    /// Replays the recorded engine callbacks through the
    /// `ICalcDisplayTarget`/`IHistoryDisplayTarget` handlers, then refreshes
    /// the programmer panel if a primary display update asked for it (the
    /// queries it makes never raise callbacks).
    pub(crate) fn drain(&mut self) {
        loop {
            let callbacks = self.calculator_display.borrow_mut().take();
            if callbacks.is_empty() {
                break;
            }
            for callback in callbacks {
                match callback {
                    DisplayCallback::PrimaryDisplay(text, is_error) => {
                        self.set_primary_display(&text, is_error)
                    }
                    DisplayCallback::IsInError(is_error) => self.set_is_in_error(is_error),
                    DisplayCallback::ExpressionDisplay(tokens, commands) => {
                        self.set_expression_display(tokens, commands)
                    }
                    DisplayCallback::ParenthesisNumber(count) => self.set_parenthesis_count(count),
                    DisplayCallback::NoRightParenAdded => self.on_no_right_paren_added(),
                    DisplayCallback::MaxDigitsReached => self.on_max_digits_reached(),
                    DisplayCallback::BinaryOperatorReceived => self.on_binary_operator_received(),
                    DisplayCallback::HistoryItemAdded(index) => {
                        self.history_vm
                            .on_history_item_added(&self.standard_calculator_manager, index);
                        self.trace.history_added = true;
                    }
                    DisplayCallback::MemorizedNumbers(numbers) => {
                        self.set_memorized_numbers(&numbers)
                    }
                    DisplayCallback::MemoryItemChanged(index) => self.on_memory_item_changed(index),
                    DisplayCallback::InputChanged => self.on_input_changed(),
                }
            }
        }
        if self.programmer_panel_dirty {
            self.programmer_panel_dirty = false;
            self.update_programmer_panel_display();
        }
    }

    /// Starts a contract-level operation (resets the callback trace).
    pub(crate) fn begin_op(&mut self) {
        self.trace = OpTrace::default();
    }

    pub(crate) fn force_replace(&mut self) {
        self.trace.forced_replace = true;
    }

    pub(crate) fn force_history_changed(&mut self) {
        self.trace.forced_history = true;
    }

    pub(crate) fn force_memory_changed(&mut self) {
        self.trace.memory = true;
    }

    /// Ends a contract-level operation and derives its [`Event`]s.
    ///
    /// The rule (the UI picks animations from it):
    ///
    /// | callbacks / operation                                         | events                 |
    /// |---------------------------------------------------------------|------------------------|
    /// | `SetIsInError(true)` / `SetPrimaryDisplay(_, true)` (error entered) | `Error` (only)    |
    /// | clear, recall, paste, restore, mode/radix/word-size switch, F-E | `Replace`            |
    /// | a key pressed while an error was shown that only clears it     | `Replace`              |
    /// | `OnHistoryItemAdded` (`=`, or a binary operator in Standard)   | `Result`               |
    /// | `BinaryOperatorReceived`                                       | `Result`               |
    /// | `InputChanged`/`SetPrimaryDisplay` for operand entry: 0–F, `.`, ⌫, `Exp`, ± while the engine is recording, bit flips | `Typing` |
    /// | any other display change (unary functions, %, `)`, π…)        | `Result`               |
    /// | `OnHistoryItemAdded`, history list edits, mode switch          | + `HistoryChanged`     |
    /// | `SetMemorizedNumbers` / `MemoryItemChanged`, memory clear      | + `MemoryChanged`      |
    pub(crate) fn end_op(&mut self, kind: OpKind) {
        let t = self.trace;
        let display_changed = t.primary || t.expression;
        let mut events = Vec::new();
        if t.error_entered {
            events.push(Event::Error);
        } else {
            match kind {
                OpKind::Replace => {
                    if display_changed || t.forced_replace {
                        events.push(Event::Replace);
                    }
                }
                OpKind::Typing => {
                    if display_changed {
                        events.push(Event::Typing);
                    }
                }
                OpKind::Quiet => {
                    if t.forced_replace || (display_changed && !t.history_added) {
                        events.push(Event::Replace);
                    }
                }
                OpKind::Button {
                    command,
                    was_in_error,
                } => {
                    if was_in_error && !is_recoverable_command(command) {
                        events.push(Event::Replace);
                    } else if t.history_added || t.binary_operator {
                        events.push(Event::Result);
                    } else if display_changed {
                        let recording = self.standard_calculator_manager.is_engine_recording();
                        if is_operand_entry_command(command) || (command == cmd::SIGN && recording)
                        {
                            events.push(Event::Typing);
                        } else {
                            events.push(Event::Result);
                        }
                    }
                }
            }
        }
        if t.history_added || t.forced_history {
            events.push(Event::HistoryChanged);
        }
        if t.memory {
            events.push(Event::MemoryChanged);
        }
        self.events.extend(events);
        self.trace = OpTrace::default();
    }

    pub(crate) fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    // ------------------------------------------------------------------
    // Read-only properties
    // ------------------------------------------------------------------

    pub(crate) fn display_value(&self) -> &str {
        &self.display_value
    }

    /// The expression line: the concatenated `ExpressionTokens`.
    pub(crate) fn expression(&self) -> String {
        self.expression_tokens.concat()
    }

    #[cfg(test)]
    pub(crate) fn expression_tokens(&self) -> &[String] {
        &self.expression_tokens
    }

    pub(crate) fn is_in_error(&self) -> bool {
        self.is_in_error
    }

    pub(crate) fn open_parenthesis_count(&self) -> u32 {
        self.open_parenthesis_count
    }

    pub(crate) fn is_input_empty(&self) -> bool {
        self.is_input_empty
    }

    pub(crate) fn is_memory_empty(&self) -> bool {
        self.is_memory_empty
    }

    pub(crate) fn is_f_to_e_enabled(&self) -> bool {
        self.is_f_to_e_enabled
    }

    pub(crate) fn are_hex_buttons_enabled(&self) -> bool {
        self.are_hex_buttons_enabled
    }

    #[cfg(test)]
    pub(crate) fn is_operator_command(&self) -> bool {
        self.is_operator_command
    }

    pub(crate) fn is_programmer(&self) -> bool {
        self.is_programmer
    }

    pub(crate) fn radix_display_value(&self, radix: Radix) -> &str {
        match radix {
            Radix::Hex => &self.hex_display_value,
            Radix::Dec => &self.decimal_display_value,
            Radix::Oct => &self.octal_display_value,
            Radix::Bin => &self.binary_display_value,
        }
    }

    pub(crate) fn binary_digits(&self) -> &[bool; 64] {
        &self.binary_digits
    }

    pub(crate) fn angle_unit(&self) -> AngleUnit {
        match self.current_angle_type {
            cmd::RAD => AngleUnit::Radians,
            cmd::GRAD => AngleUnit::Gradians,
            _ => AngleUnit::Degrees,
        }
    }

    /// `GetCalculatorMode()`
    pub(crate) fn get_calculator_mode(&self) -> CalcMode {
        if self.is_standard {
            CalcMode::Standard
        } else if self.is_scientific {
            CalcMode::Scientific
        } else {
            CalcMode::Programmer
        }
    }

    // ------------------------------------------------------------------
    // Mode properties
    // ------------------------------------------------------------------

    /// `IsBitFlipChecked` setter (the bit-flip keypad toggle; only consulted
    /// by the view upstream).
    pub(crate) fn set_is_bit_flip_checked(&mut self, value: bool) {
        self.is_bit_flip_checked = value;
    }

    /// `ValueBitLength` setter.
    pub(crate) fn set_value_bit_length(&mut self, value: WordSize) {
        if self.value_bit_length != value {
            self.value_bit_length = value;
            self.on_button_pressed(word_size_command(value));
            self.set_memorized_numbers_string();
        }
    }

    /// `IsStandard` setter (+ `HandlePropertySideEffects`).
    fn set_is_standard(&mut self, value: bool) {
        if self.is_standard != value {
            self.is_standard = value;
            if value {
                self.set_is_scientific(false);
                self.set_is_programmer(false);
            }
            if self.is_standard {
                self.on_button_pressed(Button::IsStandardMode.id() as i32);
            }
        }
    }

    /// `IsScientific` setter (+ `HandlePropertySideEffects`).
    fn set_is_scientific(&mut self, value: bool) {
        if self.is_scientific != value {
            self.is_scientific = value;
            if value {
                self.set_is_standard(false);
                self.set_is_programmer(false);
            }
            if self.is_scientific {
                self.on_button_pressed(Button::IsScientificMode.id() as i32);
            }
        }
    }

    /// `IsProgrammer` setter (+ `HandlePropertySideEffects`).
    fn set_is_programmer(&mut self, value: bool) {
        if self.is_programmer != value {
            self.is_programmer = value;
            if !self.is_programmer {
                self.set_is_bit_flip_checked(false);
            }
            if value {
                self.set_is_standard(false);
                self.set_is_scientific(false);
            }
            if self.is_programmer {
                self.on_button_pressed(Button::IsProgrammerMode.id() as i32);
            }
        }
    }

    /// `IsFToEChecked` setter as seen through the F-E `ToggleButton`.
    ///
    /// Upstream the toggle's `IsChecked` is two-way bound to this property and
    /// its `Checked`/`Unchecked` handlers call `FtoEButtonToggled()`, so every
    /// change of the property — by a click or by the view model itself (Clear,
    /// CE, switching to Standard/Programmer) — sends `FToE` to the engine.
    /// Without the XAML the view model has to do that itself, otherwise the
    /// engine keeps showing E-notation while the toggle reads "off".
    pub(crate) fn set_is_f_to_e_checked(&mut self, value: bool) {
        if self.is_f_to_e_checked != value {
            self.is_f_to_e_checked = value;
            self.ftoe_button_toggled();
        }
    }

    // ------------------------------------------------------------------
    // Public view-model methods
    // ------------------------------------------------------------------

    /// `SetCalculatorType(ViewMode targetState)`.
    pub(crate) fn set_calculator_type(&mut self, target_state: CalcMode) {
        // Deviation: upstream clears `IsInError` unconditionally. When the
        // mode does not change (upstream: navigating back from a converter;
        // here: every page activation) no mode command is sent, so the error
        // the calculator shows — an engine error, a paste error or a restored
        // one — is still there; keep reporting it as an error instead of
        // presenting the error text as a normal value.
        let same_mode = target_state == self.get_calculator_mode()
            && (self.is_standard || self.is_scientific || self.is_programmer);
        if !same_mode {
            self.set_is_in_error(false);
        }

        match target_state {
            CalcMode::Standard => {
                self.set_is_standard(true);
                self.reset_radix_and_update_memory(true);
                self.set_precision(STANDARD_MODE_PRECISION);
                self.update_max_int_digits();
            }
            CalcMode::Scientific => {
                self.set_is_scientific(true);
                self.resync_scientific_engine();
                self.reset_radix_and_update_memory(true);
                self.set_precision(SCIENTIFIC_MODE_PRECISION);
            }
            CalcMode::Programmer => {
                self.set_is_programmer(true);
                self.resync_programmer_engine();
                self.reset_radix_and_update_memory(false);
                self.set_precision(PROGRAMMER_MODE_PRECISION);
            }
        }
    }

    /// Deviation: `Recalculate` (history recall) calls
    /// `CalculatorManager.Reset(false)`, which puts the Scientific engine back
    /// to degrees while `_currentAngleType` keeps the user's choice (the reset
    /// angle is only re-sent to the engine that is current at that point).
    /// Re-send the view model's angle when entering Scientific mode if the
    /// engine disagrees.
    fn resync_scientific_engine(&mut self) {
        let engine_angle = self
            .standard_calculator_manager
            .current_calculator_engine()
            .map(|e| e.angle_type());
        let wanted = match self.current_angle_type {
            cmd::RAD => calcmanager::AngleType::Radians,
            cmd::GRAD => calcmanager::AngleType::Gradians,
            _ => calcmanager::AngleType::Degrees,
        };
        if engine_angle.is_some_and(|a| a != wanted) {
            self.send_command(self.current_angle_type);
        }
    }

    /// Deviation: `CalculatorManager.Reset` (history recall in Standard or
    /// Scientific mode) sends QWORD to the Programmer engine, while
    /// `ValueBitLength` keeps the user's word size. Re-send it when entering
    /// Programmer mode if the engine disagrees.
    fn resync_programmer_engine(&mut self) {
        let engine_width = self
            .standard_calculator_manager
            .current_calculator_engine()
            .map(|e| e.num_width());
        if engine_width.is_some_and(|w| w != num_width(self.value_bit_length)) {
            self.send_command(word_size_command(self.value_bit_length));
        }
    }

    /// `GetRawDisplayValue()` — what Copy puts on the clipboard: the display
    /// without group separators, or the error text verbatim
    /// (`copypaste::raw_display_value` is the shared port of it).
    pub(crate) fn get_raw_display_value(&self) -> String {
        let locale = LocalizationSettings::get_instance().paste_locale();
        copypaste::raw_display_value(&self.display_value, self.is_in_error, &locale)
    }

    /// `OnPasteCommand` + `PasteAsync` + `OnPaste`. `CopyPasteManager.GetStringToPaste`
    /// is `copypaste::validate_paste_expression`, `OnPaste`'s key mapping is
    /// `copypaste::calculator_paste_commands`. Returns false when the
    /// "Invalid input" error was shown.
    pub(crate) fn paste(&mut self, pasted_text: &str) -> bool {
        let (mode, number_base, bit_length_type) = if self.is_scientific {
            (
                ViewMode::Scientific,
                NumberBase::Unknown,
                BitLength::BitLengthUnknown,
            )
        } else if self.is_programmer {
            (
                ViewMode::Programmer,
                number_base(self.current_radix_type),
                bit_length(self.value_bit_length),
            )
        } else {
            (
                ViewMode::Standard,
                NumberBase::Unknown,
                BitLength::BitLengthUnknown,
            )
        };

        let localizer = LocalizationSettings::get_instance();
        let pasted_string = copypaste::validate_paste_expression_localized(
            pasted_text,
            mode,
            mode.group_type(),
            number_base,
            bit_length_type,
            &localizer.paste_locale(),
        );
        self.on_paste(&pasted_string, mode)
    }

    /// `OnPaste(string pastedString)`.
    pub(crate) fn on_paste(&mut self, pasted_string: &str, mode: ViewMode) -> bool {
        let localizer = LocalizationSettings::get_instance();
        match copypaste::calculator_paste_commands(pasted_string, mode, &localizer.paste_locale()) {
            Err(_) => {
                self.display_paste_error();
                false
            }
            Ok(commands) => {
                for command in commands {
                    self.send_command(paste_command_id(command));
                }
                true
            }
        }
    }

    /// `DisplayPasteError()`: the engine's "Invalid input" string, in error.
    fn display_paste_error(&mut self) {
        const IDS_ERRORS_FIRST: i32 = 99;
        const IDS_DOMAIN: i32 = IDS_ERRORS_FIRST + 1;
        let error_string =
            calcmanager::en_us_engine_string(&IDS_DOMAIN.to_string()).unwrap_or("Invalid input");
        self.set_primary_display(error_string, true);
        self.drain();
    }

    /// `OnMemoryButtonPressed()` (MS).
    pub(crate) fn on_memory_button_pressed(&mut self) {
        let _ = self.with_manager(|m| m.memorize_number());
    }

    /// `OnMemoryItemPressed(object memoryItemPosition)` (MR / recall a slot).
    pub(crate) fn on_memory_item_pressed(&mut self, position: usize) {
        if !self.memorized_numbers.is_empty() {
            let _ = self.with_manager(|m| m.memorized_number_load(position as u32));
        }
    }

    /// `OnMemoryAdd(object memoryItemPosition)` (M+).
    pub(crate) fn on_memory_add(&mut self, position: usize) {
        let _ = self.with_manager(|m| m.memorized_number_add(position as u32));
    }

    /// `OnMemorySubtract(object memoryItemPosition)` (M−).
    pub(crate) fn on_memory_subtract(&mut self, position: usize) {
        let _ = self.with_manager(|m| m.memorized_number_subtract(position as u32));
    }

    /// `OnMemoryClear(object memoryItemPosition)` (clear one slot).
    pub(crate) fn on_memory_clear(&mut self, position: usize) {
        if !self.memorized_numbers.is_empty() && position < self.memorized_numbers.len() {
            self.with_manager(|m| m.memorized_number_clear(position as u32));

            self.memorized_numbers.remove(position);
            for (i, slot) in self.memorized_numbers.iter_mut().enumerate() {
                slot.position = i as i32;
            }

            if self.memorized_numbers.is_empty() {
                self.is_memory_empty = true;
            }
            self.trace.memory = true;
        }
    }

    /// `OnClearMemoryCommand` (MC).
    pub(crate) fn on_clear_memory_command(&mut self) {
        let _ = self.with_manager(|m| m.memorized_number_clear_all());
    }

    /// `SelectHistoryItem(HistoryItemViewModel item)`.
    pub(crate) fn select_history_item(&mut self, item: &HistoryItemViewModel) {
        let tokens = item.get_tokens().to_vec();
        let commands = item.get_commands().to_vec();
        self.set_history_expression_display(tokens.clone(), commands.clone());
        self.set_expression_display(tokens, commands);
        self.set_primary_display(item.result(), false);
        self.is_f_to_e_enabled = false;
        self.drain();
    }

    /// `SwitchProgrammerModeBase(NumberBase numberBase)`.
    pub(crate) fn switch_programmer_mode_base(&mut self, number_base: Radix) {
        if self.is_in_error {
            self.send_command(cmd::CLEAR);
        }

        self.are_hex_buttons_enabled = number_base == Radix::Hex;
        self.current_radix_type = number_base;
        let radix_type = radix_type(number_base);
        let _ = self.with_manager(|m| m.set_radix(radix_type));
    }

    /// `SwitchAngleType(NumbersAndOperatorsEnum num)`.
    pub(crate) fn switch_angle_type(&mut self, num: i32) {
        self.on_button_pressed(num);
    }

    /// `FtoEButtonToggled()`.
    fn ftoe_button_toggled(&mut self) {
        self.on_button_pressed(Button::FToE.id() as i32);
    }

    /// `ResetCalcManager(bool clearMemory)`.
    #[cfg(test)]
    pub(crate) fn reset_calc_manager(&mut self, clear_memory: bool) {
        let _ = self.with_manager(|m| m.reset(clear_memory));
    }

    /// `ResetManagedCalculatorSubmodes()`.
    pub(crate) fn reset_managed_calculator_submodes(&mut self) {
        self.current_angle_type = cmd::DEG;
        // Not through the F-E toggle: `Reset()` already switched the engine
        // back to floating point.
        self.is_f_to_e_checked = false;
        self.set_is_bit_flip_checked(false);
        self.value_bit_length = WordSize::Qword;
        self.current_radix_type = Radix::Dec;
        self.are_hex_buttons_enabled = false;
    }

    /// `SetNativeCalculatorMode(ViewMode mode)`.
    pub(crate) fn set_native_calculator_mode(&mut self, mode: CalcMode) {
        let _ = self.with_manager(|m| match mode {
            CalcMode::Standard => m.set_standard_mode(),
            CalcMode::Scientific => m.set_scientific_mode(),
            CalcMode::Programmer => m.set_programmer_mode(),
        });
    }

    /// `SendCommandToCalcManager(int command)`.
    #[cfg(test)]
    pub(crate) fn send_command_to_calc_manager(&mut self, command: i32) {
        self.send_command(command);
    }

    // ------------------------------------------------------------------
    // ICalcDisplayTarget handlers
    // ------------------------------------------------------------------

    /// `SetPrimaryDisplay(string displayStringValue, bool isError)`.
    pub(crate) fn set_primary_display(&mut self, display_string_value: &str, is_error: bool) {
        let localized_display_string_value =
            self.localize_display_value(display_string_value, is_error);

        if self.display_value != localized_display_string_value {
            self.display_value = localized_display_string_value;
        }

        self.set_is_in_error(is_error);

        if self.is_programmer {
            // `UpdateProgrammerPanelDisplay()` queries the manager; it runs
            // once the current manager call has returned (see `drain`).
            self.programmer_panel_dirty = true;
        }
        self.trace.primary = true;
    }

    /// `SetExpressionDisplay(tokens, commands)`.
    pub(crate) fn set_expression_display(
        &mut self,
        tokens: Vec<ExpressionToken>,
        commands: Vec<ExpressionCommand>,
    ) {
        self.tokens = tokens;
        self.commands = commands;
        // `if (!IsEditingEnabled)` — editing is never enabled.
        self.set_tokens();
        self.trace.expression = true;
    }

    /// `SetParenthesisCount(uint parenthesisCount)`.
    fn set_parenthesis_count(&mut self, parenthesis_count: u32) {
        if self.open_parenthesis_count == parenthesis_count {
            return;
        }
        self.open_parenthesis_count = parenthesis_count;
        // (narrator announcement)
    }

    /// `OnNoRightParenAdded()` — narrator only.
    fn on_no_right_paren_added(&mut self) {}

    /// `SetIsInError(bool isError)` / the `IsInError` setter.
    pub(crate) fn set_is_in_error(&mut self, is_error: bool) {
        if is_error && !self.is_in_error {
            self.trace.error_entered = true;
        }
        self.is_in_error = is_error;
    }

    /// `SetMemorizedNumbers(string[] newMemorizedNumbers)`.
    pub(crate) fn set_memorized_numbers(&mut self, new_memorized_numbers: &[String]) {
        let localizer = LocalizationSettings::get_instance();
        let before = self.memorized_numbers.clone();

        if new_memorized_numbers.is_empty() {
            self.memorized_numbers.clear();
            self.is_memory_empty = true;
        } else if new_memorized_numbers.len() > self.memorized_numbers.len() {
            while new_memorized_numbers.len() > self.memorized_numbers.len() {
                let new_value_position =
                    new_memorized_numbers.len() - self.memorized_numbers.len() - 1;
                let string_value = &new_memorized_numbers[new_value_position];

                let memory_slot = MemoryItemViewModel {
                    position: 0,
                    value: localizer.localize_display_value(string_value),
                };

                self.memorized_numbers.insert(0, memory_slot);
                self.is_memory_empty = false; // `IsAlwaysOnTop` (never set here)

                for slot in self.memorized_numbers.iter_mut().skip(1) {
                    slot.position += 1;
                }
            }
        } else if new_memorized_numbers.len() == self.memorized_numbers.len() {
            for (slot, new_string_value) in
                self.memorized_numbers.iter_mut().zip(new_memorized_numbers)
            {
                let new_string_value = localizer.localize_display_value(new_string_value);
                if slot.value != new_string_value {
                    slot.value = new_string_value;
                }
            }
        }
        if self.memorized_numbers != before {
            self.trace.memory = true;
        }
    }

    /// `OnMaxDigitsReached()` — narrator only.
    fn on_max_digits_reached(&mut self) {}

    /// `OnBinaryOperatorReceived()` — narrator only upstream.
    fn on_binary_operator_received(&mut self) {
        self.trace.binary_operator = true;
    }

    /// `OnMemoryItemChanged(uint indexOfMemory)` — narrator only upstream.
    fn on_memory_item_changed(&mut self, _index_of_memory: u32) {
        self.trace.memory = true;
    }

    /// `OnInputChanged()`.
    fn on_input_changed(&mut self) {
        self.is_input_empty = self.standard_calculator_manager.is_input_empty();
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// `LocalizeDisplayValue(string displayValue)`.
    ///
    /// Deviation: upstream also pads error messages ("00Cannot divide by
    /// zero") when the radix is binary; padding is only applied to numbers.
    fn localize_display_value(&self, display_value: &str, is_error: bool) -> String {
        let mut result = display_value.to_string();

        if self.is_programmer && self.current_radix_type == Radix::Bin && !is_error {
            result = add_padding(&result);
        }

        LocalizationSettings::get_instance().localize_display_value(&result)
    }

    /// `SetHistoryExpressionDisplay(tokens, commands)`.
    pub(crate) fn set_history_expression_display(
        &mut self,
        tokens: Vec<ExpressionToken>,
        commands: Vec<ExpressionCommand>,
    ) {
        self.tokens = tokens;
        self.commands = commands;
        // IsEditingEnabled = false;

        self.with_manager(|m| m.set_in_history_item_load_mode(true));
        self.recalculate(true);
        self.with_manager(|m| m.set_in_history_item_load_mode(false));
        self.is_last_operation_history_load = true;
    }

    /// `SetTokens(tokens)`: `ExpressionTokens` from the engine tokens.
    fn set_tokens(&mut self) {
        let localizer = LocalizationSettings::get_instance();
        self.expression_tokens = self
            .tokens
            .iter()
            .map(|(token, _)| localizer.localize_display_value(token))
            .collect();
    }

    /// `OnButtonPressed(NumbersAndOperatorsEnum numOpEnum)`.
    pub(crate) fn on_button_pressed(&mut self, cmdenum: i32) {
        if self.is_in_error {
            self.send_command(cmd::CLEAR);

            if !is_recoverable_command(cmdenum) {
                return;
            }
        }

        // `IsEditingEnabled` is always false: the else branch.
        if cmdenum == cmd::MEMORY {
            self.on_memory_button_pressed();
        } else {
            if (cmdenum == cmd::CLEAR
                || cmdenum == cmd::CENTR
                || cmdenum == Button::IsStandardMode.id() as i32
                || cmdenum == Button::IsProgrammerMode.id() as i32)
                && self.is_f_to_e_checked
            {
                self.set_is_f_to_e_checked(false);
            }

            let is_angle = cmdenum == cmd::DEG || cmdenum == cmd::RAD || cmdenum == cmd::GRAD;
            if is_angle {
                self.current_angle_type = cmdenum;
            }

            self.is_operator_command = !is_digit_or_backspace(cmdenum);

            if self.is_last_operation_history_load && !is_angle {
                self.is_f_to_e_enabled = true;
                self.is_last_operation_history_load = false;
            }

            self.send_command(cmdenum);
        }
    }

    /// `ResetRadixAndUpdateMemory(bool resetRadix)`.
    fn reset_radix_and_update_memory(&mut self, reset_radix: bool) {
        if reset_radix {
            self.are_hex_buttons_enabled = false;
            self.current_radix_type = Radix::Dec;
            let _ = self.with_manager(|m| m.set_radix(RadixType::Decimal));
        } else {
            let _ = self.with_manager(|m| m.set_memorized_numbers_string());
        }
    }

    fn set_precision(&mut self, precision: i32) {
        self.with_manager(|m| m.set_precision(precision));
    }

    fn update_max_int_digits(&mut self) {
        self.with_manager(|m| m.update_max_int_digits());
    }

    /// `SetMemorizedNumbersString()`.
    fn set_memorized_numbers_string(&mut self) {
        let _ = self.with_manager(|m| m.set_memorized_numbers_string());
    }

    /// `Recalculate(bool fromHistory = false)`.
    fn recalculate(&mut self, from_history: bool) {
        let current_degree_mode = self.standard_calculator_manager.get_current_degree_mode().0;
        let current_commands = get_commands_from_expression_commands(&self.commands);

        let saved_tokens = self.tokens.clone();
        let saved_commands = self.commands.clone();

        let _ = self.with_manager(|m| m.reset(false));
        if self.is_scientific {
            self.send_command(cmd::MODE_SCIENTIFIC);
        }

        if self.is_f_to_e_checked {
            self.send_command(cmd::FE);
        }

        self.send_command(current_degree_mode);

        for command in current_commands {
            self.send_command(command);
        }

        if from_history {
            // This is for the cases where the expression is loaded from history
            // Use the FE command to make the engine end recording.
            self.send_command(cmd::FE);
            self.send_command(cmd::FE);
        }

        if self.is_in_error {
            self.set_expression_display(saved_tokens, saved_commands);
        }
    }

    /// `UpdateProgrammerPanelDisplay()`.
    fn update_programmer_panel_display(&mut self) {
        const PRECISION: i32 = 64;
        let mut hex_display_string = String::new();
        let mut decimal_display_string = String::new();
        let mut octal_display_string = String::new();
        let mut binary_display_string = String::new();

        let m = &mut self.standard_calculator_manager;
        if !self.is_in_error {
            hex_display_string = m
                .get_result_for_radix(16, PRECISION, true)
                .unwrap_or_default();
            if hex_display_string.is_empty() {
                hex_display_string = self.display_value.clone();
                decimal_display_string = self.display_value.clone();
                octal_display_string = self.display_value.clone();
                binary_display_string = self.display_value.clone();
            } else {
                decimal_display_string = m
                    .get_result_for_radix(10, PRECISION, true)
                    .unwrap_or_default();
                octal_display_string = m
                    .get_result_for_radix(8, PRECISION, true)
                    .unwrap_or_default();
                binary_display_string = m
                    .get_result_for_radix(2, PRECISION, true)
                    .unwrap_or_default();
            }
        }

        let localizer = LocalizationSettings::get_instance();
        binary_display_string = add_padding(&binary_display_string);

        self.hex_display_value = localizer.localize_display_value(&hex_display_string);
        self.decimal_display_value = localizer.localize_display_value(&decimal_display_string);
        self.octal_display_value = localizer.localize_display_value(&octal_display_string);
        self.binary_display_value = localizer.localize_display_value(&binary_display_string);

        let mut binary_value_array = [false; 64];
        let binary_value = self
            .standard_calculator_manager
            .get_result_for_radix(2, PRECISION, false)
            .unwrap_or_default();
        for (idx, c) in binary_value.chars().rev().take(64).enumerate() {
            binary_value_array[idx] = c == '1';
        }
        self.binary_digits = binary_value_array;
    }
}

/// `AddPadding(string binaryString)`: left-pads a binary string with zeros to
/// a multiple of four digits.
pub(crate) fn add_padding(binary_string: &str) -> String {
    if binary_string.is_empty() {
        return binary_string.to_string();
    }

    let english_value =
        LocalizationSettings::get_instance().get_english_value_from_localized_digits(binary_string);
    if english_value == "0" {
        return binary_string.to_string();
    }

    let length_without_padding = binary_string.chars().filter(|&c| c != ' ').count();
    let mut pad = 4 - (length_without_padding % 4);
    if pad == 4 {
        pad = 0;
    }
    format!("{}{}", "0".repeat(pad), binary_string)
}

/// `IsDigitOrBackspace(int cmd)`.
fn is_digit_or_backspace(c: i32) -> bool {
    (cmd::ZERO..=cmd::NINE).contains(&c) || c == cmd::PNT || c == cmd::BACK || c == cmd::EXP
}

/// Operand-entry keys for [`Event::Typing`]: `IsDigitOrBackspace` plus the
/// hex digits A–F.
fn is_operand_entry_command(c: i32) -> bool {
    is_digit_or_backspace(c) || (cmd::A..=cmd::F).contains(&c)
}

/// `IsRecoverableCommand(int command)`: keys that still take effect after
/// the error was cleared.
pub(crate) fn is_recoverable_command(command: i32) -> bool {
    (cmd::ZERO..=cmd::NINE).contains(&command)
        || command == cmd::PNT
        || (cmd::BINEDITSTART..=cmd::BINEDITEND).contains(&command)
        || (cmd::A..=cmd::F).contains(&command)
}

/// `GetCommandsFromExpressionCommands(IList<ExpressionCommandWrapper>)`.
pub(crate) fn get_commands_from_expression_commands(
    expression_commands: &[ExpressionCommand],
) -> Vec<i32> {
    let mut commands = Vec::new();
    for command in expression_commands {
        match command.get_command_type() {
            CommandType::UnaryCommand => {
                if let ExpressionCommand::Unary(u) = command {
                    commands.extend_from_slice(u.get_commands());
                }
            }
            CommandType::BinaryCommand => {
                if let ExpressionCommand::Binary(b) = command {
                    commands.push(b.get_command());
                }
            }
            CommandType::Parentheses => {
                if let ExpressionCommand::Parentheses(p) = command {
                    commands.push(p.get_command());
                }
            }
            CommandType::OperandCommand => {
                if let ExpressionCommand::Operand(o) = command {
                    let mut need_sign = o.is_negative();
                    for &code in o.get_commands() {
                        commands.push(code);
                        if need_sign && code != cmd::ZERO {
                            commands.push(cmd::SIGN);
                            need_sign = false;
                        }
                    }
                }
            }
        }
    }
    commands
}

pub(crate) fn word_size_command(w: WordSize) -> i32 {
    match w {
        WordSize::Qword => cmd::QWORD,
        WordSize::Dword => cmd::DWORD,
        WordSize::Word => cmd::WORD,
        WordSize::Byte => cmd::BYTE,
    }
}

fn num_width(w: WordSize) -> NumWidth {
    match w {
        WordSize::Qword => NumWidth::QwordWidth,
        WordSize::Dword => NumWidth::DwordWidth,
        WordSize::Word => NumWidth::WordWidth,
        WordSize::Byte => NumWidth::ByteWidth,
    }
}

/// `GetRadixTypeFromNumberBase(NumberBase numberBase)`.
pub(crate) fn radix_type(r: Radix) -> RadixType {
    match r {
        Radix::Bin => RadixType::Binary,
        Radix::Hex => RadixType::Hex,
        Radix::Oct => RadixType::Octal,
        Radix::Dec => RadixType::Decimal,
    }
}

pub(crate) fn number_base(r: Radix) -> NumberBase {
    match r {
        Radix::Hex => NumberBase::HexBase,
        Radix::Dec => NumberBase::DecBase,
        Radix::Oct => NumberBase::OctBase,
        Radix::Bin => NumberBase::BinBase,
    }
}

pub(crate) fn bit_length(w: WordSize) -> BitLength {
    match w {
        WordSize::Qword => BitLength::BitLengthQWord,
        WordSize::Dword => BitLength::BitLengthDWord,
        WordSize::Word => BitLength::BitLengthWord,
        WordSize::Byte => BitLength::BitLengthByte,
    }
}

/// The `CalculatorCommand` `OnPaste` sends for a mapped key
/// (`(CalculatorCommand)(int)mappedNumOp`).
pub(crate) fn paste_command_id(c: PasteCommand) -> i32 {
    match c {
        PasteCommand::ClearEntry => cmd::CENTR,
        PasteCommand::Digit(d) => cmd::ZERO + d as i32,
        PasteCommand::Decimal => cmd::PNT,
        PasteCommand::Add => cmd::ADD,
        PasteCommand::Subtract => cmd::SUB,
        PasteCommand::Multiply => cmd::MUL,
        PasteCommand::Divide => cmd::DIV,
        PasteCommand::XPowerY => cmd::PWR,
        PasteCommand::Mod => cmd::MOD,
        PasteCommand::Equals => cmd::EQU,
        PasteCommand::OpenParenthesis => cmd::OPENP,
        PasteCommand::CloseParenthesis => cmd::CLOSEP,
        PasteCommand::Exp => cmd::EXP,
        PasteCommand::Negate => cmd::SIGN,
    }
}
