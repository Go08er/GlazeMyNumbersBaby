// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! View-model layer of the Windows Calculator port: the UI-agnostic logic that
//! lives in `Calculator.ViewModels` upstream (StandardCalculatorViewModel,
//! HistoryViewModel, memory, programmer-mode state, …).
//!
//! # UI CONTRACT
//! The GTK front-end codes against the public API of
//! [`CalculatorViewModel`] below. The implementation behind it may change
//! freely; the signatures are what the UI relies on.
//!
//! # Implementation
//! [`CalculatorViewModel`] is a thin facade over the port of
//! `StandardCalculatorViewModel.cs` (`standard_vm`), which drives the
//! `calcmanager` engine exactly like upstream: every button goes through
//! `OnButtonPressed` and becomes an engine command (no value arithmetic in
//! the view model), the display/expression/memory/history state is whatever
//! the engine callbacks deliver. Module map:
//!
//! | upstream (`Calculator.ViewModels/…`)                     | here            |
//! |-----------------------------------------------------------|-----------------|
//! | `StandardCalculatorViewModel.cs`                          | `standard_vm`   |
//! | `Common/CalculatorDisplay.cs`                             | `display`       |
//! | `Common/LocalizationSettings.cs` (en-US)                  | `localization`  |
//! | `HistoryViewModel.cs`, `HistoryItemViewModel.cs`          | `history_vm`    |
//! | `MemoryItemViewModel.cs`                                  | `memory_vm`     |
//! | `Snapshots.cs`, `Utils/JsonUtils.cs`, `Common/ExpressionCommand{Serializer,Deserializer}.cs` | `snapshot` |
//! | `Common/NumbersAndOperatorsEnum.cs`                       | `buttons`       |
//!
//! The rule that turns engine callbacks into [`Event`]s is documented on
//! `StandardCalculatorViewModel::end_op` and summarised on [`Event`].

mod buttons;
mod display;
mod history_vm;
mod localization;
mod memory_vm;
mod snapshot;
mod standard_vm;

#[cfg(test)]
mod tests;

pub use buttons::{BIN_END, BIN_START, Button};

use standard_vm::{OpKind, StandardCalculatorViewModel, cmd};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CalcMode {
    Standard,
    Scientific,
    Programmer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Radix {
    Hex,
    Dec,
    Oct,
    Bin,
}

impl Radix {
    pub const ALL: [Radix; 4] = [Radix::Hex, Radix::Dec, Radix::Oct, Radix::Bin];
    pub fn label(self) -> &'static str {
        match self {
            Radix::Hex => "HEX",
            Radix::Dec => "DEC",
            Radix::Oct => "OCT",
            Radix::Bin => "BIN",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WordSize {
    Qword,
    Dword,
    Word,
    Byte,
}

impl WordSize {
    pub fn bits(self) -> u32 {
        match self {
            WordSize::Qword => 64,
            WordSize::Dword => 32,
            WordSize::Word => 16,
            WordSize::Byte => 8,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            WordSize::Qword => "QWORD",
            WordSize::Dword => "DWORD",
            WordSize::Word => "WORD",
            WordSize::Byte => "BYTE",
        }
    }
    /// The word size the upstream button cycles to next.
    pub fn next(self) -> WordSize {
        match self {
            WordSize::Qword => WordSize::Dword,
            WordSize::Dword => WordSize::Word,
            WordSize::Word => WordSize::Byte,
            WordSize::Byte => WordSize::Qword,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShiftMode {
    Arithmetic,
    Logical,
    Rotate,
    RotateThroughCarry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AngleUnit {
    Degrees,
    Radians,
    Gradians,
}

impl AngleUnit {
    pub fn label(self) -> &'static str {
        match self {
            AngleUnit::Degrees => "DEG",
            AngleUnit::Radians => "RAD",
            AngleUnit::Gradians => "GRAD",
        }
    }
    pub fn next(self) -> AngleUnit {
        match self {
            AngleUnit::Degrees => AngleUnit::Radians,
            AngleUnit::Radians => AngleUnit::Gradians,
            AngleUnit::Gradians => AngleUnit::Degrees,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    /// Upstream's `HistoryItemViewModel.Expression`: the engine tokens joined
    /// with single spaces, e.g. "12   ×   3 =" (the operator tokens carry
    /// their own spaces, hence three).
    pub expression: String,
    /// e.g. "36"
    pub result: String,
}

/// What just happened, so the UI can pick an animation / announce it.
///
/// Derived from the engine display callbacks of each call:
///
/// * `SetIsInError(true)` / an error primary display → [`Event::Error`]
///   (instead of the value events below);
/// * `InputChanged`/`SetPrimaryDisplay` while an operand is being entered
///   (0–F, `.`, ⌫, `Exp`, ± while the engine is recording, bit flips) →
///   [`Event::Typing`];
/// * `BinaryOperatorReceived`, unary function / % / `)` results, `=` →
///   [`Event::Result`];
/// * `OnHistoryItemAdded` → [`Event::Result`] + [`Event::HistoryChanged`];
/// * mode switch, clear/CE, memory or history recall, paste, radix or word
///   size change, F-E, restore → [`Event::Replace`];
/// * `SetMemorizedNumbers` / `MemoryItemChanged` (and clearing a slot) →
///   [`Event::MemoryChanged`]; history list edits and mode switches →
///   [`Event::HistoryChanged`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Digit entry / backspace edited the number being typed.
    Typing,
    /// A computation produced a new value (=, a binary operator that
    /// evaluated a pending op, a unary function, %…).
    Result,
    /// The value was replaced wholesale (clear, recall, paste, mode switch…).
    Replace,
    /// The engine entered an error state.
    Error,
    MemoryChanged,
    HistoryChanged,
}

/// The calculator view-model shared by Standard, Scientific and Programmer.
pub struct CalculatorViewModel {
    vm: StandardCalculatorViewModel,
}

impl Default for CalculatorViewModel {
    fn default() -> Self {
        Self::new()
    }
}

fn angle_command(unit: AngleUnit) -> i32 {
    match unit {
        AngleUnit::Degrees => cmd::DEG,
        AngleUnit::Radians => cmd::RAD,
        AngleUnit::Gradians => cmd::GRAD,
    }
}

impl CalculatorViewModel {
    /// A calculator in Standard mode (`ApplicationViewModel.Initialize`).
    pub fn new() -> Self {
        let mut this = CalculatorViewModel {
            vm: StandardCalculatorViewModel::new(),
        };
        this.vm.begin_op();
        this.apply_mode(CalcMode::Standard);
        this.vm.end_op(OpKind::Quiet);
        this.vm.take_events();
        this
    }

    // ---- mode
    pub fn mode(&self) -> CalcMode {
        self.vm.get_calculator_mode()
    }

    /// `ApplicationViewModel.Mode` setter: `SetCalculatorType(mode)` plus
    /// what `MainPage` does on a mode change (history shortcuts,
    /// `HistoryVM.ReloadHistory`).
    pub fn set_mode(&mut self, mode: CalcMode) {
        let changed = mode != self.mode();
        self.vm.begin_op();
        self.apply_mode(mode);
        if changed {
            self.vm.force_replace();
            self.vm.force_history_changed();
        }
        self.vm.end_op(OpKind::Replace);
    }

    fn apply_mode(&mut self, mode: CalcMode) {
        self.vm.set_calculator_type(mode);
        let vm = &mut self.vm;
        vm.history_vm
            .set_are_history_shortcuts_enabled(mode != CalcMode::Programmer);
        vm.history_vm
            .reload_history(&vm.standard_calculator_manager, mode);
    }

    // ---- input
    /// Press any calculator button (digits, operators, functions, memory
    /// buttons MS/MR/M+/M−/MC, parentheses, `FToE`, `BINPOS*` via
    /// [`CalculatorViewModel::flip_bit`], …).
    ///
    /// Buttons are `OnButtonPressed` upstream; the ones that are not engine
    /// commands there are routed like the XAML does: mode buttons →
    /// [`set_mode`](Self::set_mode), HEX/DEC/OCT/BIN →
    /// [`set_radix`](Self::set_radix), QWORD…BYTE →
    /// [`set_word_size`](Self::set_word_size), DEG/RAD/GRAD →
    /// [`set_angle_unit`](Self::set_angle_unit), MR/M+/M− → slot 0, MC →
    /// clear all memory. `FToE` is ignored while disabled (the toggle is
    /// disabled upstream, and pressing it would desynchronise the flag from
    /// the engine).
    pub fn press(&mut self, button: Button) {
        match button {
            Button::IsStandardMode => self.set_mode(CalcMode::Standard),
            Button::IsScientificMode => self.set_mode(CalcMode::Scientific),
            Button::IsProgrammerMode => self.set_mode(CalcMode::Programmer),
            Button::HexButton => self.set_radix(Radix::Hex),
            Button::DecButton => self.set_radix(Radix::Dec),
            Button::OctButton => self.set_radix(Radix::Oct),
            Button::BinButton => self.set_radix(Radix::Bin),
            Button::Qword => self.set_word_size(WordSize::Qword),
            Button::Dword => self.set_word_size(WordSize::Dword),
            Button::Word => self.set_word_size(WordSize::Word),
            Button::Byte => self.set_word_size(WordSize::Byte),
            Button::Degree => self.set_angle_unit(AngleUnit::Degrees),
            Button::Radians => self.set_angle_unit(AngleUnit::Radians),
            Button::Grads => self.set_angle_unit(AngleUnit::Gradians),
            Button::MemoryRecall => self.memory_recall(0),
            Button::MemoryAdd => self.memory_add(0),
            Button::MemorySubtract => self.memory_subtract(0),
            Button::MemoryClear => {
                self.vm.begin_op();
                self.vm.on_clear_memory_command();
                self.vm.end_op(OpKind::Quiet);
            }
            Button::BitflipButton => self.vm.set_is_bit_flip_checked(true),
            Button::FullKeypadButton => self.vm.set_is_bit_flip_checked(false),
            // Not calculator buttons (graphing) / no button.
            Button::None
            | Button::LessThan
            | Button::LessThanOrEqualTo
            | Button::GreaterThan
            | Button::GreaterThanOrEqualTo
            | Button::X
            | Button::Y
            | Button::Submit => {}
            Button::FToE => {
                if self.is_enabled(Button::FToE) {
                    self.vm.begin_op();
                    let checked = self.vm.is_f_to_e_checked;
                    self.vm.set_is_f_to_e_checked(!checked);
                    self.vm.end_op(OpKind::Replace);
                }
            }
            Button::Memory => {
                self.vm.begin_op();
                self.vm.on_button_pressed(cmd::MEMORY);
                self.vm.end_op(OpKind::Quiet);
            }
            Button::Clear | Button::ClearEntry => {
                self.vm.begin_op();
                self.vm.on_button_pressed(button.id() as i32);
                self.vm.end_op(OpKind::Replace);
            }
            _ => self.press_command(button.id() as i32),
        }
    }

    fn press_command(&mut self, command: i32) {
        self.vm.begin_op();
        let was_in_error = self.vm.is_in_error();
        self.vm.on_button_pressed(command);
        self.vm.end_op(OpKind::Button {
            command,
            was_in_error,
        });
    }

    // ---- display
    /// Primary display string, formatted exactly as shown (digit grouping…).
    pub fn display_value(&self) -> String {
        self.vm.display_value().to_string()
    }
    /// The expression line above the value: the engine's tokens
    /// concatenated, e.g. "12 × 3=" (upstream renders the `=` token without
    /// a leading space).
    pub fn expression(&self) -> String {
        self.vm.expression()
    }
    pub fn is_error(&self) -> bool {
        self.vm.is_in_error()
    }
    /// Number of currently open parentheses (shown on the "(" key).
    pub fn open_parens(&self) -> u32 {
        self.vm.open_parenthesis_count()
    }
    /// Upstream shows "CE" while a number is being entered and "C" otherwise
    /// (scientific/programmer share one key): `!IsInputEmpty`.
    pub fn shows_clear_entry(&self) -> bool {
        !self.vm.is_input_empty()
    }

    // ---- scientific
    pub fn angle_unit(&self) -> AngleUnit {
        self.vm.angle_unit()
    }
    /// `SwitchAngleType`: sends DEG/RAD/GRAD to the engine. Ignored while an
    /// error is shown (the angle buttons are disabled then).
    pub fn set_angle_unit(&mut self, unit: AngleUnit) {
        if !self.is_enabled(Button::Degree) {
            return;
        }
        self.vm.begin_op();
        self.vm.switch_angle_type(angle_command(unit));
        self.vm.end_op(OpKind::Quiet);
    }
    pub fn is_fe(&self) -> bool {
        self.vm.is_f_to_e_checked
    }

    // ---- programmer
    pub fn radix(&self) -> Radix {
        self.vm.current_radix_type
    }
    /// `SwitchProgrammerModeBase` (Programmer mode only; clears an error
    /// first, like upstream).
    pub fn set_radix(&mut self, radix: Radix) {
        if !self.vm.is_programmer() {
            return;
        }
        self.vm.begin_op();
        self.vm.switch_programmer_mode_base(radix);
        self.vm.force_replace();
        self.vm.end_op(OpKind::Replace);
    }
    /// The current value rendered in `radix` (programmer side panel).
    pub fn radix_value(&self, radix: Radix) -> String {
        self.vm.radix_display_value(radix).to_string()
    }
    pub fn word_size(&self) -> WordSize {
        self.vm.value_bit_length
    }
    /// `ValueBitLength` setter: sends QWORD/DWORD/WORD/BYTE to the engine
    /// (Programmer mode only; ignored while an error is shown, when the
    /// word-size button is disabled).
    pub fn set_word_size(&mut self, w: WordSize) {
        if !self.vm.is_programmer() || !self.is_enabled(Button::Qword) {
            return;
        }
        self.vm.begin_op();
        self.vm.set_value_bit_length(w);
        self.vm.end_op(OpKind::Replace);
    }
    pub fn shift_mode(&self) -> ShiftMode {
        self.vm.shift_mode
    }
    /// The bit-shift radio buttons only change which shift keys the view
    /// shows (no engine state); the UI maps Lsh/Rsh accordingly.
    pub fn set_shift_mode(&mut self, s: ShiftMode) {
        self.vm.shift_mode = s;
    }
    /// Bit `index` (0 = LSB) of the current value (`BinaryDigits`).
    pub fn bit(&self, index: u32) -> bool {
        self.vm
            .binary_digits()
            .get(index as usize)
            .copied()
            .unwrap_or(false)
    }
    /// Toggles bit `index` through the engine (`BINPOS0 + index`, the bit-flip
    /// keypad). Only bits inside the word size can be flipped.
    pub fn flip_bit(&mut self, index: u32) {
        if !self.vm.is_programmer() || index >= self.vm.value_bit_length.bits() {
            return;
        }
        self.vm.begin_op();
        self.vm.on_button_pressed(cmd::BINEDITSTART + index as i32);
        self.vm.end_op(OpKind::Typing);
    }
    /// Whether a button is currently usable (radix digits, "." in
    /// programmer, memory recall with empty memory, …).
    ///
    /// Mirrors upstream's bindings: A–F need `AreHEXButtonsEnabled`, 2–9 / 8–9
    /// are disabled in BIN / OCT (`NumberPad.OnCurrentRadixTypePropertyChanged`),
    /// "." is disabled in Programmer (`IsDecimalEnabled`), MR/MC need memory
    /// (`IsMemoryEmpty`), F-E needs `IsFToEEnabled`; while an error is shown
    /// the `ErrorLayout` visual states disable every operator, function,
    /// memory, angle, F-E and word-size button and ".", leaving digits,
    /// `=`, C, CE, ⌫ and the radix/mode buttons usable.
    pub fn is_enabled(&self, button: Button) -> bool {
        let vm = &self.vm;
        if let Some(d) = button.digit_value() {
            if d >= 10 {
                return vm.are_hex_buttons_enabled();
            }
            if vm.is_programmer() {
                return match vm.current_radix_type {
                    Radix::Bin => d < 2,
                    Radix::Oct => d < 8,
                    Radix::Dec | Radix::Hex => true,
                };
            }
            return true;
        }
        let in_error = vm.is_in_error();
        match button {
            Button::Decimal => !vm.is_programmer() && !in_error,
            Button::MemoryRecall | Button::MemoryClear => !vm.is_memory_empty() && !in_error,
            Button::FToE => vm.is_f_to_e_enabled() && !in_error,
            Button::Equals
            | Button::Clear
            | Button::ClearEntry
            | Button::Backspace
            | Button::HexButton
            | Button::DecButton
            | Button::OctButton
            | Button::BinButton
            | Button::IsStandardMode
            | Button::IsScientificMode
            | Button::IsProgrammerMode
            | Button::BitflipButton
            | Button::FullKeypadButton
            | Button::None
            | Button::LessThan
            | Button::LessThanOrEqualTo
            | Button::GreaterThan
            | Button::GreaterThanOrEqualTo
            | Button::X
            | Button::Y
            | Button::Submit => true,
            _ => !in_error,
        }
    }

    // ---- memory (index 0 = most recent, as displayed)
    pub fn memory(&self) -> Vec<String> {
        self.vm
            .memorized_numbers
            .iter()
            .map(|m| m.value.clone())
            .collect()
    }
    /// `OnMemoryItemPressed(index)`.
    pub fn memory_recall(&mut self, index: usize) {
        self.vm.begin_op();
        self.vm.on_memory_item_pressed(index);
        self.vm.end_op(OpKind::Replace);
    }
    /// `OnMemoryAdd(index)` (stores the value when memory is empty).
    pub fn memory_add(&mut self, index: usize) {
        self.vm.begin_op();
        self.vm.on_memory_add(index);
        self.vm.end_op(OpKind::Quiet);
    }
    /// `OnMemorySubtract(index)` (stores the negated value when memory is
    /// empty).
    pub fn memory_subtract(&mut self, index: usize) {
        self.vm.begin_op();
        self.vm.on_memory_subtract(index);
        self.vm.end_op(OpKind::Quiet);
    }
    /// `OnMemoryClear(index)`.
    pub fn memory_clear(&mut self, index: usize) {
        self.vm.begin_op();
        self.vm.on_memory_clear(index);
        self.vm.end_op(OpKind::Quiet);
    }

    // ---- history (index 0 = most recent; per mode like upstream)
    /// `HistoryVM.Items` of the current mode. Programmer mode has no history
    /// (upstream hides the panel), so this is empty there.
    pub fn history(&self) -> Vec<HistoryEntry> {
        if self.vm.is_programmer() {
            return Vec::new();
        }
        self.vm
            .history_vm
            .items()
            .iter()
            .map(|h| HistoryEntry {
                expression: h.expression().to_string(),
                result: h.result().to_string(),
            })
            .collect()
    }
    /// `SelectHistoryItem`: shows the item's expression and result and loads
    /// it into the engine so the calculation can be continued.
    pub fn history_recall(&mut self, index: usize) {
        if self.vm.is_programmer() {
            return;
        }
        let Some(item) = self.vm.history_vm.items().get(index).cloned() else {
            return;
        };
        self.vm.begin_op();
        self.vm.select_history_item(&item);
        self.vm.force_replace();
        self.vm.end_op(OpKind::Replace);
    }
    /// `HistoryViewModel.DeleteItem`.
    pub fn history_remove(&mut self, index: usize) {
        if self.vm.is_programmer() {
            return;
        }
        self.vm.begin_op();
        let vm = &mut self.vm;
        if vm
            .history_vm
            .delete_item(&mut vm.standard_calculator_manager, index)
        {
            vm.force_history_changed();
        }
        self.vm.end_op(OpKind::Quiet);
    }
    /// `HistoryViewModel.OnClear` (disabled in Programmer mode, where the
    /// history shortcuts are off).
    pub fn history_clear(&mut self) {
        self.vm.begin_op();
        let vm = &mut self.vm;
        let had_items = vm.history_vm.items_count() > 0;
        vm.history_vm.on_clear(&mut vm.standard_calculator_manager);
        if had_items && vm.history_vm.items_count() == 0 {
            vm.force_history_changed();
        }
        self.vm.end_op(OpKind::Quiet);
    }

    // ---- clipboard
    /// `GetRawDisplayValue()`: the display without group separators (or the
    /// error text verbatim).
    pub fn copy_text(&self) -> String {
        self.vm.get_raw_display_value()
    }
    /// Paste text; returns false if it isn't a valid number/expression for
    /// the current mode (UI then shows "Invalid input").
    ///
    /// `OnPasteCommand` → `CopyPasteManager.ValidatePasteExpression` →
    /// `OnPaste`: the accepted text is fed to the engine as key presses; on
    /// rejection the view model shows the engine's "Invalid input" error
    /// (`DisplayPasteError`), so [`is_error`](Self::is_error) is true until
    /// the next key.
    pub fn paste(&mut self, text: &str) -> bool {
        self.vm.begin_op();
        let ok = self.vm.paste(text);
        self.vm.force_replace();
        self.vm.end_op(OpKind::Replace);
        ok
    }

    // ---- events / persistence
    pub fn take_events(&mut self) -> Vec<Event> {
        self.vm.take_events()
    }
    /// The upstream calculator snapshot (`ApplicationSnapshot` JSON) plus a
    /// gmnb extension carrying both histories, memory, radix, word size,
    /// angle unit, F-E and shift mode (see the `snapshot` module).
    pub fn save_state(&self) -> String {
        self.vm.snapshot().to_json().to_string()
    }
    /// `ApplicationViewModel.RestoreFromSnapshot`. Invalid or untrusted
    /// input that fails `SnapshotValidator.ValidateProtocol` is ignored and
    /// leaves the calculator unchanged.
    pub fn restore_state(&mut self, state: &str) {
        let Ok(snapshot) = snapshot::ApplicationSnapshot::from_json(state) else {
            return;
        };
        if snapshot::SnapshotValidator::validate_protocol(&snapshot).is_err() {
            return;
        }
        let (Ok(mode), Some(standard)) = (
            snapshot::SnapshotValidator::mode(&snapshot),
            snapshot.standard_calculator.as_ref(),
        ) else {
            return;
        };

        self.vm.begin_op();
        self.apply_mode(mode);
        self.vm
            .restore_snapshot(standard, snapshot.extension.as_ref());
        let vm = &mut self.vm;
        vm.history_vm
            .reload_history(&vm.standard_calculator_manager, mode);
        vm.force_replace();
        vm.force_history_changed();
        vm.force_memory_changed();
        self.vm.end_op(OpKind::Replace);
    }
}
