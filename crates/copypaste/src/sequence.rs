// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Turning validated clipboard text into key presses, and the copy-side display helper.
//!
//! `CopyPasteManager` only validates; the conversion of the accepted text into calculator
//! input lives in the view models. These are UI-agnostic ports of:
//! * `StandardCalculatorViewModel.OnPaste` / `MapCharacterToButtonId` →
//!   [`calculator_paste_commands`], [`map_character_to_button_id`];
//! * `UnitConverterViewModel.OnPaste` / `MapCharacterToButtonId` →
//!   [`converter_paste_commands`];
//! * `StandardCalculatorViewModel.GetRawDisplayValue` (what Copy puts on the clipboard) →
//!   [`raw_display_value`].
//!
//! The GUI feeds the returned commands to the calculator/converter engine in order.

use crate::manager::is_error_message;
use crate::types::{PasteLocale, ViewMode};

/// A key press produced by pasting into a calculator: the subset of the original's
/// `NumbersAndOperatorsEnum` / `CalculatorCommand` that `OnPaste` can send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PasteCommand {
    /// `CalculatorCommand.CommandCENTR` (clear entry); always the first command.
    ClearEntry,
    /// A digit key, `0..=15`; 10-15 are the hexadecimal digits A-F.
    Digit(u8),
    /// Decimal separator.
    Decimal,
    /// `+`
    Add,
    /// `-`
    Subtract,
    /// `*`
    Multiply,
    /// `/`
    Divide,
    /// `^` (x to the power of y), Scientific mode only.
    XPowerY,
    /// `%` (modulo), Scientific and Programmer modes only.
    Mod,
    /// `=`
    Equals,
    /// `(`
    OpenParenthesis,
    /// `)`
    CloseParenthesis,
    /// `e`/`E` outside Programmer mode: start of the exponent.
    Exp,
    /// Change sign (+/−).
    Negate,
}

/// Result of [`map_character_to_button_id`] (`struct ButtonInfo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonInfo {
    /// The key the character stands for, `None` when it is ignored.
    pub button_id: Option<PasteCommand>,
    /// Whether a pending negation may be applied right after this key.
    pub can_send_negate: bool,
}

/// Why pasted text produced no input (the original shows the engine's "Invalid input" error,
/// `DisplayPasteError`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasteError;

impl std::fmt::Display for PasteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid input for paste")
    }
}

impl std::error::Error for PasteError {}

/// Maps one character to a calculator key (`StandardCalculatorViewModel.MapCharacterToButtonId`).
///
/// `^` maps only in Scientific mode, `%` only in Scientific and Programmer modes, and `e`/`E`
/// is the hex digit E in Programmer mode and the exponent key elsewhere. The locale's decimal
/// separator and digit symbols are recognised too.
pub fn map_character_to_button_id(ch: char, mode: ViewMode, locale: &PasteLocale) -> ButtonInfo {
    let is_scientific = mode == ViewMode::Scientific;
    let is_programmer = mode == ViewMode::Programmer;

    let mut result = ButtonInfo {
        button_id: None,
        can_send_negate: false,
    };

    match ch {
        '0'..='9' => {
            result.button_id = Some(PasteCommand::Digit(ch as u8 - b'0'));
            result.can_send_negate = true;
        }
        '*' => result.button_id = Some(PasteCommand::Multiply),
        '+' => result.button_id = Some(PasteCommand::Add),
        '-' => result.button_id = Some(PasteCommand::Subtract),
        '/' => result.button_id = Some(PasteCommand::Divide),
        '^' => {
            if is_scientific {
                result.button_id = Some(PasteCommand::XPowerY);
            }
        }
        '%' => {
            if is_scientific || is_programmer {
                result.button_id = Some(PasteCommand::Mod);
            }
        }
        '=' => result.button_id = Some(PasteCommand::Equals),
        '(' => result.button_id = Some(PasteCommand::OpenParenthesis),
        ')' => result.button_id = Some(PasteCommand::CloseParenthesis),
        'a' | 'A' => result.button_id = Some(PasteCommand::Digit(0xA)),
        'b' | 'B' => result.button_id = Some(PasteCommand::Digit(0xB)),
        'c' | 'C' => result.button_id = Some(PasteCommand::Digit(0xC)),
        'd' | 'D' => result.button_id = Some(PasteCommand::Digit(0xD)),
        'e' | 'E' => {
            result.button_id = Some(if is_programmer {
                PasteCommand::Digit(0xE)
            } else {
                PasteCommand::Exp
            });
        }
        'f' | 'F' => result.button_id = Some(PasteCommand::Digit(0xF)),
        _ => {
            if ch == locale.decimal_separator {
                result.button_id = Some(PasteCommand::Decimal);
            }
        }
    }

    if result.button_id.is_none()
        && let Some(digit) = locale.localized_digit_value(ch)
    {
        result.button_id = Some(PasteCommand::Digit(digit));
        result.can_send_negate = true;
    }

    if result.button_id == Some(PasteCommand::Digit(0)) {
        result.can_send_negate = false;
    }

    result
}

/// Converts text accepted by [`validate_paste_expression`](crate::validate_paste_expression)
/// into the commands Standard/Scientific/Programmer mode sends to the engine
/// (`StandardCalculatorViewModel.OnPaste`).
///
/// Unmapped characters (spaces, group separators, ...) are skipped. A leading `-` (or one
/// right after a binary operator) becomes a Negate after the following digit; a leading `+`
/// is dropped; `e-`/`e+` become Exp followed by Negate / nothing; signs inside parentheses
/// are tracked with a stack. Returns [`PasteError`] for the
/// [`PASTE_ERROR_STRING`](crate::PASTE_ERROR_STRING) sentinel.
pub fn calculator_paste_commands(
    pasted_string: &str,
    mode: ViewMode,
    locale: &PasteLocale,
) -> Result<Vec<PasteCommand>, PasteError> {
    if is_error_message(pasted_string) {
        return Err(PasteError);
    }

    let mut commands = vec![PasteCommand::ClearEntry];
    let mut is_first_legal_char = true;
    let mut send_negate = false;
    let mut is_previous_operator = false;
    let mut negate_stack: Vec<bool> = Vec::new();

    let chars: Vec<char> = pasted_string.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let mut send_command = true;
        let button_info = map_character_to_button_id(chars[i], mode, locale);

        let mut can_send_negate = button_info.can_send_negate;
        let Some(mapped_num_op) = button_info.button_id else {
            i += 1;
            continue;
        };

        if is_first_legal_char || is_previous_operator {
            is_first_legal_char = false;
            is_previous_operator = false;

            if mapped_num_op == PasteCommand::Subtract {
                send_negate = true;
                send_command = false;
            }
            if mapped_num_op == PasteCommand::Add {
                send_command = false;
            }
        }

        match mapped_num_op {
            PasteCommand::OpenParenthesis => {
                negate_stack.push(send_negate);
                send_negate = false;
            }
            PasteCommand::CloseParenthesis => {
                if let Some(pending) = negate_stack.pop() {
                    send_negate = pending;
                    can_send_negate = true;
                } else {
                    send_command = false;
                }
            }
            PasteCommand::Add
            | PasteCommand::Subtract
            | PasteCommand::Multiply
            | PasteCommand::Divide => {
                is_previous_operator = true;
            }
            _ => {}
        }

        if send_command {
            commands.push(mapped_num_op);

            if send_negate {
                if can_send_negate {
                    commands.push(PasteCommand::Negate);
                }
                if mapped_num_op != PasteCommand::Digit(0) && mapped_num_op != PasteCommand::Decimal
                {
                    send_negate = false;
                }
            }
        }

        if mapped_num_op == PasteCommand::Exp && i + 1 < chars.len() {
            let next_button = map_character_to_button_id(chars[i + 1], mode, locale);
            if next_button.button_id == Some(PasteCommand::Subtract) {
                commands.push(PasteCommand::Negate);
                i += 1;
            } else if next_button.button_id == Some(PasteCommand::Add) {
                i += 1;
            }
        }

        i += 1;
    }

    Ok(commands)
}

/// A key press produced by pasting into a unit/currency converter (the `UnitConverterCommand`s
/// `UnitConverterViewModel.OnPaste` can send).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConverterPasteCommand {
    /// `UnitConverterCommand.Clear`; sent before the first legal character.
    Clear,
    /// A decimal digit, `0..=9`.
    Digit(u8),
    /// Decimal separator.
    Decimal,
    /// Change sign.
    Negate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConverterButton {
    Digit(u8),
    Decimal,
    Negate,
}

/// `UnitConverterViewModel.MapCharacterToButtonId`.
fn map_converter_character(ch: char, locale: &PasteLocale) -> (Option<ConverterButton>, bool) {
    if ch.is_ascii_digit() {
        return (Some(ConverterButton::Digit(ch as u8 - b'0')), true);
    }
    if ch == locale.decimal_separator {
        return (Some(ConverterButton::Decimal), true);
    }
    if ch == '-' {
        return (Some(ConverterButton::Negate), false);
    }
    if let Some(digit) = locale.localized_digit_value(ch) {
        return (Some(ConverterButton::Digit(digit)), true);
    }
    (None, false)
}

/// Converts text accepted by [`validate_paste_expression`](crate::validate_paste_expression)
/// (in a converter mode) into converter commands (`UnitConverterViewModel.OnPaste`).
///
/// `currency_fraction_digits` is `Some(n)` when the current category is Currency (with `n`
/// the source currency's fraction digits): input stops once `n` digits follow the `.`, as the
/// original's `UpdateInputBlocked` does. Pass `None` for the other converters.
///
/// Returns [`PasteError`] for empty text, the error sentinel, or text without a single legal
/// character.
pub fn converter_paste_commands(
    string_to_paste: &str,
    currency_fraction_digits: Option<usize>,
    locale: &PasteLocale,
) -> Result<Vec<ConverterPasteCommand>, PasteError> {
    if string_to_paste.is_empty() || is_error_message(string_to_paste) {
        return Err(PasteError);
    }

    let mut commands = Vec::new();
    let mut is_first_legal_char = true;
    let mut send_negate = false;
    let mut accumulation = String::new();

    for ch in string_to_paste.chars() {
        let (button_id, can_send_negate) = map_converter_character(ch, locale);

        let Some(button_id) = button_id else {
            send_negate = false;
            continue;
        };

        if is_first_legal_char {
            // Send Clear before sending anything that will actually apply to the field.
            commands.push(ConverterPasteCommand::Clear);
            is_first_legal_char = false;

            // A leading minus is a sign, but it has to follow the digit it applies to or
            // the engine ignores it, so remember it rather than sending it now.
            if button_id == ConverterButton::Negate {
                send_negate = true;
            }
        }

        if button_id != ConverterButton::Negate {
            commands.push(match button_id {
                ConverterButton::Digit(d) => ConverterPasteCommand::Digit(d),
                ConverterButton::Decimal => ConverterPasteCommand::Decimal,
                ConverterButton::Negate => unreachable!("handled above"),
            });

            if send_negate {
                if can_send_negate {
                    commands.push(ConverterPasteCommand::Negate);
                }
                send_negate = false;
            }
        }

        accumulation.push(ch);
        if is_input_blocked(&accumulation, currency_fraction_digits) {
            break;
        }
    }

    if is_first_legal_char {
        // No legal characters found — show paste error
        return Err(PasteError);
    }

    Ok(commands)
}

/// `UnitConverterViewModel.UpdateInputBlocked`: currency input is full once it has as many
/// fraction digits as the currency allows. `currency_input` is in en-US form.
fn is_input_blocked(currency_input: &str, currency_fraction_digits: Option<usize>) -> bool {
    match (currency_input.find('.'), currency_fraction_digits) {
        (Some(byte_pos), Some(fraction_digits)) => {
            let pos_of_decimal = currency_input[..byte_pos].encode_utf16().count();
            pos_of_decimal + fraction_digits + 1 == currency_input.encode_utf16().count()
        }
        _ => false,
    }
}

/// What Copy puts on the clipboard for a calculator display
/// (`StandardCalculatorViewModel.GetRawDisplayValue`): the display text without group
/// separators, or verbatim while the calculator shows an error.
pub fn raw_display_value(display_value: &str, is_in_error: bool, locale: &PasteLocale) -> String {
    if is_in_error {
        display_value.to_string()
    } else {
        locale.remove_group_separators(display_value)
    }
}
