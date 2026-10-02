// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `Calculator.ViewModels/Common/CopyPasteManager.cs`.
//!
//! Everything here is pure: the clipboard access of the original (`CopyToClipboard`,
//! `GetStringToPaste`, `HasStringToPaste`) is the GUI's job. The GUI reads the clipboard text
//! and runs it through [`validate_paste_expression`]; the result is either the pasted text,
//! unchanged, or [`PASTE_ERROR_STRING`] (`"NoOp"`, test with [`is_error_message`]).
//!
//! String lengths are counted in UTF-16 code units, exactly like .NET's `string.Length`.

use std::sync::LazyLock;

use regex::Regex;

use crate::types::{BitLength, CategoryGroupType, NumberBase, PasteLocale, ViewMode};

/// The sentinel returned for text that cannot be pasted (`PasteErrorString`).
pub const PASTE_ERROR_STRING: &str = "NoOp";
/// Longest operand accepted in Standard mode, in significant digits.
pub const MAX_STANDARD_OPERAND_LENGTH: u32 = 16;
/// Longest operand accepted in Scientific mode, in significant digits.
pub const MAX_SCIENTIFIC_OPERAND_LENGTH: u32 = 32;
/// Longest input accepted by the converters.
pub const MAX_CONVERTER_INPUT_LENGTH: u32 = 16;
/// Most operands an expression may have.
pub const MAX_OPERAND_COUNT: u32 = 100;
/// Most digits an exponent may have.
pub const MAX_EXPONENT_LENGTH: u32 = 4;
/// Widest programmer word.
pub const MAX_PROGRAMMER_BIT_LENGTH: u32 = 64;
/// Longest text that is even considered for pasting.
pub const MAX_PASTEABLE_LENGTH: u32 = 512;

const C_VALID_BASIC_CHARACTER_SET: &str = "0123456789+-.e";
const C_VALID_STANDARD_CHARACTER_SET: &str = "0123456789+-.e*/";
const C_VALID_SCIENTIFIC_CHARACTER_SET: &str = "0123456789+-.e*/()^%";
const C_VALID_PROGRAMMER_CHARACTER_SET: &str = "0123456789+-.e*/()%abcdfABCDEF";

// The patterns are the original's, character for character. `\s` and `\d` are Unicode-aware
// in both .NET and the `regex` crate.
const C_WSPC: &str = r"[\s\x85]*";
const C_SIGNED_DEC_FLOAT: &str = r"(?:[-+]?(?:[0-9]+(\.[0-9]*)?|\.[0-9]+))";
const C_OPTIONAL_E_NOTATION: &str = r"(?:e[+-]?[0-9]+)?";

// Programmer Mode Integer patterns
const C_HEX_PROGRAMMER_CHARS: &str = r"([a-f]|[A-F]|\d)+((_|'|`)([a-f]|[A-F]|\d)+)*";
const C_DEC_PROGRAMMER_CHARS: &str = r"\d+((_|'|`)\d+)*";
const C_OCT_PROGRAMMER_CHARS: &str = r"[0-7]+((_|'|`)[0-7]+)*";
const C_BIN_PROGRAMMER_CHARS: &str = r"[0-1]+((_|'|`)[0-1]+)*";
const C_UINT_SUFFIXES: &str = r"[uU]?[lL]{0,2}";

fn wspc_lparens() -> String {
    format!("{C_WSPC}[(]*{C_WSPC}")
}

fn wspc_lparen_signed() -> String {
    format!("{C_WSPC}([-+]?[(])*{C_WSPC}")
}

fn wspc_rparens() -> String {
    format!("{C_WSPC}[)]*{C_WSPC}")
}

/// `FullMatch`: anchors the pattern at both ends (`\A(?:...)\z`).
fn full_match(pattern: &str) -> Regex {
    Regex::new(&format!(r"\A(?:{pattern})\z")).expect("valid paste pattern")
}

struct Patterns {
    standard: Vec<Regex>,
    scientific: Vec<Regex>,
    programmer_hex: Vec<Regex>,
    programmer_dec: Vec<Regex>,
    programmer_oct: Vec<Regex>,
    programmer_bin: Vec<Regex>,
    unit_converter: Vec<Regex>,
}

static PATTERNS: LazyLock<Patterns> = LazyLock::new(|| {
    let lparens = wspc_lparens();
    let rparens = wspc_rparens();
    Patterns {
        standard: vec![full_match(&format!(
            "{C_WSPC}{C_SIGNED_DEC_FLOAT}{C_OPTIONAL_E_NOTATION}{C_WSPC}"
        ))],
        // Note the alternation: a bare optional sign (or nothing) is a complete alternative,
        // which is what lets trailing operators such as "50%" through.
        scientific: vec![full_match(&format!(
            "({C_WSPC}[-+]?)|({lparen_signed}){C_SIGNED_DEC_FLOAT}{C_OPTIONAL_E_NOTATION}{rparens}",
            lparen_signed = wspc_lparen_signed(),
        ))],
        programmer_hex: vec![
            full_match(&format!(
                "{lparens}(0[xX])?{C_HEX_PROGRAMMER_CHARS}{C_UINT_SUFFIXES}{rparens}"
            )),
            full_match(&format!("{lparens}{C_HEX_PROGRAMMER_CHARS}[hH]?{rparens}")),
        ],
        programmer_dec: vec![
            full_match(&format!(
                "{lparens}[-+]?{C_DEC_PROGRAMMER_CHARS}[lL]{{0,2}}{rparens}"
            )),
            full_match(&format!(
                "{lparens}(0[nN])?{C_DEC_PROGRAMMER_CHARS}{C_UINT_SUFFIXES}{rparens}"
            )),
        ],
        programmer_oct: vec![full_match(&format!(
            "{lparens}(0[otOT])?{C_OCT_PROGRAMMER_CHARS}{C_UINT_SUFFIXES}{rparens}"
        ))],
        programmer_bin: vec![
            full_match(&format!(
                "{lparens}(0[byBY])?{C_BIN_PROGRAMMER_CHARS}{C_UINT_SUFFIXES}{rparens}"
            )),
            full_match(&format!("{lparens}{C_BIN_PROGRAMMER_CHARS}[bB]?{rparens}")),
        ],
        unit_converter: vec![full_match(&format!("{C_WSPC}{C_SIGNED_DEC_FLOAT}{C_WSPC}"))],
    }
});

/// Maximum length and value an operand may have in a given mode
/// (`CopyPasteMaxOperandLengthAndValue`). A `max_value` of 0 means "no value limit".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyPasteMaxOperandLengthAndValue {
    pub max_length: u32,
    pub max_value: u64,
}

/// .NET `string.Length`.
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// .NET `string.IndexOf(char)` for an ASCII `ch`, in UTF-16 code units.
fn utf16_index_of(s: &str, ch: char) -> Option<usize> {
    s.find(ch).map(|byte_index| utf16_len(&s[..byte_index]))
}

/// `true` when `message` is the paste error sentinel (`IsErrorMessage`).
pub fn is_error_message(message: &str) -> bool {
    message == PASTE_ERROR_STRING
}

/// Validates clipboard text for `mode` (`ValidatePasteExpression(pastedText, mode,
/// programmerNumberBase, bitLengthType)`), deriving the category group from the mode.
///
/// Returns `pasted_text` unchanged when it can be pasted, [`PASTE_ERROR_STRING`] otherwise.
/// `programmer_number_base` and `bit_length_type` only matter in [`ViewMode::Programmer`];
/// pass [`NumberBase::Unknown`] / [`BitLength::BitLengthUnknown`] elsewhere.
pub fn validate_paste_expression(
    pasted_text: &str,
    mode: ViewMode,
    programmer_number_base: NumberBase,
    bit_length_type: BitLength,
) -> String {
    validate_paste_expression_with_group(
        pasted_text,
        mode,
        mode.group_type(),
        programmer_number_base,
        bit_length_type,
    )
}

/// [`validate_paste_expression`] with an explicit category group (the five-argument
/// `ValidatePasteExpression` overload), en-US number conventions.
pub fn validate_paste_expression_with_group(
    pasted_text: &str,
    mode: ViewMode,
    mode_type: CategoryGroupType,
    programmer_number_base: NumberBase,
    bit_length_type: BitLength,
) -> String {
    validate_paste_expression_localized(
        pasted_text,
        mode,
        mode_type,
        programmer_number_base,
        bit_length_type,
        &PasteLocale::EN_US,
    )
}

/// [`validate_paste_expression_with_group`] under the number conventions of `locale`
/// (localized digits and decimal separator are mapped to ASCII, group separators dropped).
pub fn validate_paste_expression_localized(
    pasted_text: &str,
    mode: ViewMode,
    mode_type: CategoryGroupType,
    programmer_number_base: NumberBase,
    bit_length_type: BitLength,
    locale: &PasteLocale,
) -> String {
    if utf16_len(pasted_text) > MAX_PASTEABLE_LENGTH as usize {
        // PastedExpressionSizeGreaterThanMaxAllowed
        return PASTE_ERROR_STRING.to_string();
    }

    let english_string = locale.english_value_from_localized_digits(pasted_text);

    let mut paste_expression = remove_unwanted_chars_from_string_localized(&english_string, locale);

    if paste_expression.ends_with('=') {
        paste_expression.pop();
    }

    if mode == ViewMode::Scientific && !paste_expression.chars().any(|ch| ch.is_ascii_digit()) {
        return PASTE_ERROR_STRING.to_string();
    }

    let mut operands = extract_operands(&paste_expression, mode);
    if operands.is_empty() {
        return PASTE_ERROR_STRING.to_string();
    }

    if mode_type == CategoryGroupType::Converter {
        operands.clear();
        operands.push(paste_expression);
    }

    if !expression_regex_match(
        &operands,
        mode,
        mode_type,
        programmer_number_base,
        bit_length_type,
    ) {
        // InvalidExpressionForPresentMode
        return PASTE_ERROR_STRING.to_string();
    }

    pasted_text.to_string()
}

/// Splits a (sanitised) expression into its operands at the binary operators valid in
/// `mode` (`ExtractOperands`).
///
/// A `+`/`-` at the start, after `(`, after another operator, or (outside programmer mode)
/// right after an `e` is a sign, not an operator. Returns an empty list when there are more
/// than [`MAX_OPERAND_COUNT`] operands or an exponent longer than [`MAX_EXPONENT_LENGTH`].
pub fn extract_operands(paste_expression: &str, mode: ViewMode) -> Vec<String> {
    let mut operands: Vec<String> = Vec::new();
    let mut last_index = 0usize;
    let mut have_operator = false;
    let mut start_exp_counting = false;
    let mut start_of_expression = true;
    let mut is_previous_open_paren = false;
    let mut is_previous_operator = false;

    let valid_character_set = match mode {
        ViewMode::Standard => C_VALID_STANDARD_CHARACTER_SET,
        ViewMode::Scientific => C_VALID_SCIENTIFIC_CHARACTER_SET,
        ViewMode::Programmer => C_VALID_PROGRAMMER_CHARACTER_SET,
        _ => C_VALID_BASIC_CHARACTER_SET,
    };

    let mut exp_length = 0u32;
    let mut previous_char: Option<char> = None;

    for (i, current_char) in paste_expression.char_indices() {
        let prev = previous_char.replace(current_char);

        // If the current character is not a valid one, don't process it
        if !valid_character_set.contains(current_char) {
            continue;
        }

        if operands.len() >= MAX_OPERAND_COUNT as usize {
            // OperandCountGreaterThanMaxCount
            operands.clear();
            return operands;
        }

        if current_char.is_ascii_digit() {
            if start_exp_counting {
                exp_length += 1;
                if exp_length > MAX_EXPONENT_LENGTH {
                    // ExponentLengthGreaterThanMaxLength
                    operands.clear();
                    return operands;
                }
            }
            is_previous_operator = false;
        } else if current_char == 'e' {
            if mode != ViewMode::Programmer {
                start_exp_counting = true;
            }
            is_previous_operator = false;
        } else if matches!(current_char, '+' | '-' | '*' | '/' | '^' | '%') {
            if (current_char == '+' || current_char == '-')
                && (is_previous_open_paren
                    || start_of_expression
                    || is_previous_operator
                    || (mode != ViewMode::Programmer && !(i != 0 && prev != Some('e'))))
            {
                is_previous_operator = false;
                continue;
            }

            start_exp_counting = false;
            exp_length = 0;
            have_operator = true;
            is_previous_operator = true;
            operands.push(paste_expression[last_index..i].to_string());
            last_index = i + current_char.len_utf8();
        } else {
            is_previous_operator = false;
        }

        is_previous_open_paren = current_char == '(';
        start_of_expression = false;
    }

    if !have_operator {
        operands.clear();
        operands.push(paste_expression.to_string());
    } else {
        operands.push(paste_expression[last_index..].to_string());
    }

    operands
}

fn patterns_for(
    mode: ViewMode,
    mode_type: CategoryGroupType,
    programmer_number_base: NumberBase,
) -> &'static [Regex] {
    let patterns = &*PATTERNS;
    match mode {
        ViewMode::Standard => &patterns.standard,
        ViewMode::Scientific => &patterns.scientific,
        ViewMode::Programmer => match programmer_number_base {
            NumberBase::HexBase => &patterns.programmer_hex,
            NumberBase::DecBase => &patterns.programmer_dec,
            NumberBase::OctBase => &patterns.programmer_oct,
            NumberBase::BinBase => &patterns.programmer_bin,
            // The original indexes its pattern table with (base - HexBase) and throws
            // IndexOutOfRangeException here; treat it as "nothing matches".
            NumberBase::Unknown => &[],
        },
        _ if mode_type == CategoryGroupType::Converter => &patterns.unit_converter,
        _ => &[],
    }
}

/// Checks every operand against the mode's patterns, length limit and (programmer mode)
/// value range (`ExpressionRegExMatch`).
///
/// In programmer mode a negative decimal operand may be one past the positive maximum
/// (e.g. `-9223372036854775808` in QWORD).
pub fn expression_regex_match(
    operands: &[String],
    mode: ViewMode,
    mode_type: CategoryGroupType,
    programmer_number_base: NumberBase,
    bit_length_type: BitLength,
) -> bool {
    if operands.is_empty() {
        return false;
    }

    let patterns = patterns_for(mode, mode_type, programmer_number_base);

    let max_operand_length_and_value =
        get_max_operand_length_and_value(mode, mode_type, programmer_number_base, bit_length_type);
    let mut exp_matched = true;

    for operand in operands {
        let operand_matched = patterns.iter().any(|p| p.is_match(operand));

        if operand_matched {
            let is_negative_value = operand.starts_with('-');
            let operand_value = sanitize_operand(operand);

            if operand_length(&operand_value, mode, mode_type, programmer_number_base)
                > max_operand_length_and_value.max_length
            {
                exp_matched = false;
                break;
            }

            if max_operand_length_and_value.max_value != 0 {
                let Some(operand_as_ull) =
                    try_operand_to_ull(&operand_value, programmer_number_base)
                else {
                    exp_matched = false;
                    break;
                };

                let is_overflow = operand_as_ull > max_operand_length_and_value.max_value;
                let is_max_negative_value =
                    operand_as_ull.wrapping_sub(1) == max_operand_length_and_value.max_value;
                if is_overflow && !(is_negative_value && is_max_negative_value) {
                    exp_matched = false;
                    break;
                }
            }
        }

        exp_matched = exp_matched && operand_matched;
    }

    exp_matched
}

/// Maximum operand length and value for a mode (`GetMaxOperandLengthAndValue`).
///
/// Programmer mode: the length is the number of digits of the radix needed for the word
/// (minus the sign bit in decimal), the value is the largest unsigned (hex/oct/bin) or
/// signed-positive (decimal) word.
pub fn get_max_operand_length_and_value(
    mode: ViewMode,
    mode_type: CategoryGroupType,
    programmer_number_base: NumberBase,
    bit_length_type: BitLength,
) -> CopyPasteMaxOperandLengthAndValue {
    match mode {
        ViewMode::Standard => CopyPasteMaxOperandLengthAndValue {
            max_length: MAX_STANDARD_OPERAND_LENGTH,
            max_value: 0,
        },
        ViewMode::Scientific => CopyPasteMaxOperandLengthAndValue {
            max_length: MAX_SCIENTIFIC_OPERAND_LENGTH,
            max_value: 0,
        },
        ViewMode::Programmer => {
            let bit_length: u32 = match bit_length_type {
                BitLength::BitLengthQWord => 64,
                BitLength::BitLengthDWord => 32,
                BitLength::BitLengthWord => 16,
                BitLength::BitLengthByte => 8,
                BitLength::BitLengthUnknown => 0,
            };

            let ln2 = 2f64.ln();
            let bits_per_digit: f64 = match programmer_number_base {
                NumberBase::BinBase => 2f64.ln() / ln2,
                NumberBase::OctBase => 8f64.ln() / ln2,
                NumberBase::DecBase => 10f64.ln() / ln2,
                NumberBase::HexBase => 16f64.ln() / ln2,
                NumberBase::Unknown => 0.0,
            };

            let sign_bit: u32 = if programmer_number_base == NumberBase::DecBase {
                1
            } else {
                0
            };

            // C# arithmetic: uint subtraction wraps, the double->uint cast saturates and the
            // shift count of a ulong is masked to 6 bits.
            let significant_bits = bit_length.wrapping_sub(sign_bit);
            let max_length = (f64::from(significant_bits) / bits_per_digit).ceil() as u32;
            let shift = (MAX_PROGRAMMER_BIT_LENGTH as i32).wrapping_sub(significant_bits as i32);
            let max_value = u64::MAX >> (shift & 63);

            CopyPasteMaxOperandLengthAndValue {
                max_length,
                max_value,
            }
        }
        _ if mode_type == CategoryGroupType::Converter => CopyPasteMaxOperandLengthAndValue {
            max_length: MAX_CONVERTER_INPUT_LENGTH,
            max_value: 0,
        },
        _ => CopyPasteMaxOperandLengthAndValue {
            max_length: 0,
            max_value: 0,
        },
    }
}

/// Strips digit separators (`'`, `_`, `` ` ``), parentheses and signs from an operand
/// (`SanitizeOperand`).
pub fn sanitize_operand(operand: &str) -> String {
    const UNWANTED_CHARS: [char; 7] = ['\'', '_', '`', '(', ')', '-', '+'];
    remove_chars_from_string(operand, &UNWANTED_CHARS)
}

/// Parses the leading digits of a sanitised operand in the given radix (`TryOperandToULL`),
/// after optional whitespace and an optional radix prefix (`0x`, `0n`, `0o`/`0t`, `0b`/`0y`).
/// Parsing stops at the first character that is not a digit of the radix (so suffixes are
/// ignored). Returns `None` for empty or negative input, no digits, or overflow.
pub fn try_operand_to_ull(operand: &str, number_base: NumberBase) -> Option<u64> {
    if operand.is_empty() || operand.starts_with('-') {
        return None;
    }

    let int_base: u32 = match number_base {
        NumberBase::HexBase => 16,
        NumberBase::OctBase => 8,
        NumberBase::BinBase => 2,
        _ => 10,
    };

    try_parse_leading_digits(operand, int_base)
}

fn try_parse_leading_digits(operand: &str, int_base: u32) -> Option<u64> {
    let chars: Vec<char> = operand.chars().collect();
    let mut index = 0usize;
    while index < chars.len() && chars[index].is_whitespace() {
        index += 1;
    }

    if index + 1 < chars.len() && chars[index] == '0' {
        let prefix = chars[index + 1].to_ascii_uppercase();
        let has_radix_prefix = (int_base == 16 && prefix == 'X')
            || (int_base == 10 && prefix == 'N')
            || (int_base == 8 && (prefix == 'O' || prefix == 'T'))
            || (int_base == 2 && (prefix == 'B' || prefix == 'Y'));
        let first_digit = chars.get(index + 2).and_then(|&c| hex_value(c));
        if has_radix_prefix && first_digit.is_some_and(|d| d < int_base) {
            index += 2;
        }
    }

    let mut value: u64 = 0;
    let mut digit_count = 0usize;
    for &c in &chars[index..] {
        let Some(digit) = hex_value(c).filter(|&d| d < int_base) else {
            break;
        };
        value = value
            .checked_mul(u64::from(int_base))?
            .checked_add(u64::from(digit))?;
        digit_count += 1;
    }

    if digit_count == 0 { None } else { Some(value) }
}

fn hex_value(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some(c as u32 - '0' as u32),
        'a'..='f' => Some(c as u32 - 'a' as u32 + 10),
        'A'..='F' => Some(c as u32 - 'A' as u32 + 10),
        _ => None,
    }
}

/// Number of significant characters of a sanitised operand in a mode (`OperandLength`).
pub fn operand_length(
    operand: &str,
    mode: ViewMode,
    mode_type: CategoryGroupType,
    programmer_number_base: NumberBase,
) -> u32 {
    if mode_type == CategoryGroupType::Converter {
        return utf16_len(operand) as u32;
    }

    match mode {
        ViewMode::Standard | ViewMode::Scientific => standard_scientific_operand_length(operand),
        ViewMode::Programmer => programmer_operand_length(operand, programmer_number_base),
        _ => 0,
    }
}

/// Operand length in Standard/Scientific mode (`StandardScientificOperandLength`): the
/// decimal point (and a leading `0.`) and any exponent part do not count.
pub fn standard_scientific_operand_length(operand: &str) -> u32 {
    let has_decimal = operand.contains('.');
    let mut length = utf16_len(operand) as i64;

    if has_decimal && length >= 2 {
        if operand.starts_with("0.") {
            length -= 2;
        } else {
            length -= 1;
        }
    }

    if let Some(exponent_pos) = utf16_index_of(operand, 'e') {
        let exp_length = utf16_len(operand) as i64 - exponent_pos as i64;
        length -= exp_length;
    }

    length as u32
}

/// Operand length in Programmer mode (`ProgrammerOperandLength`): one radix suffix (`b`,
/// `h`, `ULL`, `UL`, `LL`, `U`, `L`) and one radix prefix (`0b`/`0y`, `-`/`0n`, `0t`/`0o`,
/// `0x`) do not count, case-insensitively.
pub fn programmer_operand_length(operand: &str, number_base: NumberBase) -> u32 {
    let mut prefixes: Vec<&str> = Vec::new();
    let mut suffixes: Vec<&str> = Vec::new();

    match number_base {
        NumberBase::BinBase => {
            prefixes.extend(["0B", "0Y"]);
            suffixes.push("B");
        }
        NumberBase::DecBase => {
            prefixes.extend(["-", "0N"]);
        }
        NumberBase::OctBase => {
            prefixes.extend(["0T", "0O"]);
        }
        NumberBase::HexBase => {
            prefixes.push("0X");
            suffixes.push("H");
        }
        NumberBase::Unknown => return 0,
    }

    // UInt suffixes are common across all modes
    suffixes.extend(["ULL", "UL", "LL", "U", "L"]);

    // ToUpperInvariant only matters for the ASCII affixes compared below.
    let operand_upper = operand.to_ascii_uppercase();
    let mut len = utf16_len(operand);

    // Detect suffix and subtract its length
    for suffix in &suffixes {
        if len < suffix.len() {
            continue;
        }

        if operand_upper.ends_with(suffix) {
            len -= suffix.len();
            break;
        }
    }

    // Detect prefix and subtract its length
    for prefix in &prefixes {
        if len < prefix.len() {
            continue;
        }

        if operand_upper.starts_with(prefix) {
            len -= prefix.len();
            break;
        }
    }

    len as u32
}

/// Removes spaces, group separators, `,`, `"`, currency symbols, bidi embedding marks and
/// no-break spaces (`RemoveUnwantedCharsFromString`), en-US conventions.
pub fn remove_unwanted_chars_from_string(input: &str) -> String {
    remove_unwanted_chars_from_string_localized(input, &PasteLocale::EN_US)
}

/// [`remove_unwanted_chars_from_string`] with `locale`'s group separator.
pub fn remove_unwanted_chars_from_string_localized(input: &str, locale: &PasteLocale) -> String {
    const UNWANTED_CHARS: [char; 19] = [
        ' ', ',', '"', '\u{00A5}', // ¥ yen
        '\u{00A4}', // ¤ currency sign
        '\u{20B5}', // ₵ cedi
        '$', '\u{20A1}', // ₡ colon
        '\u{20A9}', // ₩ won
        '\u{20AA}', // ₪ shekel
        '\u{20A6}', // ₦ naira
        '\u{20B9}', // ₹ rupee
        '\u{00A3}', // £ pound
        '\u{20AC}', // € euro
        '\u{202A}', // LEFT-TO-RIGHT EMBEDDING
        '\u{202B}', // RIGHT-TO-LEFT EMBEDDING
        '\u{202C}', // POP DIRECTIONAL FORMATTING
        '\u{202D}', // LEFT-TO-RIGHT OVERRIDE
        '\u{00A0}', // NO-BREAK SPACE
    ];
    let input = locale.remove_group_separators(input);
    remove_chars_from_string(&input, &UNWANTED_CHARS)
}

fn remove_chars_from_string(input: &str, chars_to_remove: &[char]) -> String {
    input
        .chars()
        .filter(|ch| !chars_to_remove.contains(ch))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_helpers() {
        assert_eq!(utf16_len("a\u{1F600}b"), 4);
        assert_eq!(utf16_index_of("\u{1F600}e", 'e'), Some(2));
        assert_eq!(utf16_index_of("abc", 'e'), None);
    }

    #[test]
    fn programmer_with_unknown_base_matches_nothing() {
        assert!(!expression_regex_match(
            &["123".to_string()],
            ViewMode::Programmer,
            CategoryGroupType::Calculator,
            NumberBase::Unknown,
            BitLength::BitLengthQWord,
        ));
    }

    #[test]
    fn unknown_bit_length_follows_csharp_wrapping() {
        let dec = get_max_operand_length_and_value(
            ViewMode::Programmer,
            CategoryGroupType::Calculator,
            NumberBase::DecBase,
            BitLength::BitLengthUnknown,
        );
        assert_eq!(dec.max_value, u64::MAX >> 1);
        let hex = get_max_operand_length_and_value(
            ViewMode::Programmer,
            CategoryGroupType::Calculator,
            NumberBase::HexBase,
            BitLength::BitLengthUnknown,
        );
        assert_eq!(
            hex,
            CopyPasteMaxOperandLengthAndValue {
                max_length: 0,
                max_value: u64::MAX
            }
        );
    }
}
