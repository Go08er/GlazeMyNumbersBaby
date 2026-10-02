// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Clipboard paste validation and sanitisation from Windows Calculator, ported to Rust.
//!
//! * [`manager`] — `CopyPasteManager.cs`: decides whether clipboard text may be pasted into
//!   the current mode ([`validate_paste_expression`]), with all the helpers the original
//!   exposes (operand extraction, per-mode regexes, length/value limits for every radix and
//!   word size, prefix/suffix handling, sanitisation).
//! * [`sequence`] — the view-model side of pasting: turning accepted text into key presses
//!   for the calculator or converter engine, and the copy-side display helper.
//! * [`types`] — the mode / radix / word-size enums and the number-format conventions.
//!
//! ```
//! use copypaste::{
//!     calculator_paste_commands, is_error_message, validate_paste_expression, BitLength,
//!     NumberBase, PasteCommand, PasteLocale, ViewMode,
//! };
//!
//! // Hex, QWORD: "0xFF" is fine, "xyz" is not.
//! let ok = validate_paste_expression("0xFF", ViewMode::Programmer, NumberBase::HexBase, BitLength::BitLengthQWord);
//! assert_eq!(ok, "0xFF");
//! let bad = validate_paste_expression("xyz", ViewMode::Programmer, NumberBase::HexBase, BitLength::BitLengthQWord);
//! assert!(is_error_message(&bad));
//!
//! // Standard mode: thousands separators are ignored, a leading minus negates.
//! let text = validate_paste_expression("-1,234", ViewMode::Standard, NumberBase::Unknown, BitLength::BitLengthUnknown);
//! let keys = calculator_paste_commands(&text, ViewMode::Standard, &PasteLocale::EN_US).unwrap();
//! use PasteCommand::*;
//! assert_eq!(keys, [ClearEntry, Digit(1), Negate, Digit(2), Digit(3), Digit(4)]);
//! ```
//!
//! # Deviations from the original
//!
//! * The validator keeps the original's string protocol: it returns the pasted text
//!   unchanged or the sentinel [`PASTE_ERROR_STRING`] (`"NoOp"`).
//! * Number conventions default to en-US ([`PasteLocale::EN_US`]); the `*_localized`
//!   functions take another [`PasteLocale`] instead of reading the user's region settings.
//! * `ViewMode::Programmer` with [`NumberBase::Unknown`] matches nothing (the original throws
//!   an `IndexOutOfRangeException`).
//! * Error tracing (`TraceLogger.LogError`) is dropped.

pub mod manager;
pub mod sequence;
pub mod types;

pub use manager::{
    CopyPasteMaxOperandLengthAndValue, MAX_CONVERTER_INPUT_LENGTH, MAX_EXPONENT_LENGTH,
    MAX_OPERAND_COUNT, MAX_PASTEABLE_LENGTH, MAX_PROGRAMMER_BIT_LENGTH,
    MAX_SCIENTIFIC_OPERAND_LENGTH, MAX_STANDARD_OPERAND_LENGTH, PASTE_ERROR_STRING,
    expression_regex_match, extract_operands, get_max_operand_length_and_value, is_error_message,
    operand_length, programmer_operand_length, remove_unwanted_chars_from_string,
    remove_unwanted_chars_from_string_localized, sanitize_operand,
    standard_scientific_operand_length, try_operand_to_ull, validate_paste_expression,
    validate_paste_expression_localized, validate_paste_expression_with_group,
};
pub use sequence::{
    ButtonInfo, ConverterPasteCommand, PasteCommand, PasteError, calculator_paste_commands,
    converter_paste_commands, map_character_to_button_id, raw_display_value,
};
pub use types::{BitLength, CategoryGroupType, NumberBase, PasteLocale, ViewMode};
