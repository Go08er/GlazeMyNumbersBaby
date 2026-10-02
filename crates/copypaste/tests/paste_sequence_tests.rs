// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Paste behaviour of the view models, ported from the paste cases of
//! `Calculator.Tests/StandardCalculatorViewModelTests.cs` and
//! `Calculator.Tests/UnitConverterViewModelTests.cs`. The originals check the display after
//! the engine has processed the keys; without the engine these check the key sequence that
//! produces that display (expected display noted on each case).

use copypaste::*;

const EN_US: PasteLocale = PasteLocale::EN_US;

fn keys(text: &str, mode: ViewMode) -> Vec<PasteCommand> {
    calculator_paste_commands(text, mode, &EN_US).expect("paste accepted")
}

#[test]
fn standard_mode_paste_sequences() {
    use PasteCommand::*;

    // Display "-0.99": the negation waits for the first non-zero digit.
    assert_eq!(
        keys("-0.99", ViewMode::Standard),
        [ClearEntry, Digit(0), Decimal, Digit(9), Negate, Digit(9)]
    );
    // Display "2".
    assert_eq!(
        keys("1+1=", ViewMode::Standard),
        [ClearEntry, Digit(1), Add, Digit(1), Equals]
    );
    // Display "1" (the previous operation repeated).
    assert_eq!(
        keys("0=", ViewMode::Standard),
        [ClearEntry, Digit(0), Equals]
    );
    // Display "-1".
    assert_eq!(
        keys("-1", ViewMode::Standard),
        [ClearEntry, Digit(1), Negate]
    );
    // Display "-2", expression "negate(1 + 1)".
    assert_eq!(
        keys("-(1+1)", ViewMode::Standard),
        [
            ClearEntry,
            OpenParenthesis,
            Digit(1),
            Add,
            Digit(1),
            CloseParenthesis,
            Negate
        ]
    );
    // Display "-1", expression "negate(0 - (0 - 1))".
    assert_eq!(
        keys("-(-(-1))", ViewMode::Standard),
        [
            ClearEntry,
            OpenParenthesis,
            Subtract,
            OpenParenthesis,
            Subtract,
            Digit(1),
            CloseParenthesis,
            CloseParenthesis,
            Negate
        ]
    );
}

#[test]
fn scientific_mode_exponent_paste_sequences() {
    use PasteCommand::*;

    // Display "1.23e+10".
    let positive = [
        ClearEntry,
        Digit(1),
        Decimal,
        Digit(2),
        Digit(3),
        Exp,
        Digit(1),
        Digit(0),
    ];
    assert_eq!(keys("1.23e+10", ViewMode::Scientific), positive);
    assert_eq!(keys("1.23e10", ViewMode::Scientific), positive);
    // Display "135.e+10".
    assert_eq!(
        keys("135e10", ViewMode::Scientific),
        [
            ClearEntry,
            Digit(1),
            Digit(3),
            Digit(5),
            Exp,
            Digit(1),
            Digit(0)
        ]
    );
    // Display "1.23e-10".
    let negative = [
        ClearEntry,
        Digit(1),
        Decimal,
        Digit(2),
        Digit(3),
        Exp,
        Negate,
        Digit(1),
        Digit(0),
    ];
    assert_eq!(keys("1.23e-10", ViewMode::Scientific), negative);
    // Uppercase E (for exponent)
    assert_eq!(keys("1.23E-10", ViewMode::Scientific), negative);
    assert_eq!(
        keys("135E10", ViewMode::Scientific),
        [
            ClearEntry,
            Digit(1),
            Digit(3),
            Digit(5),
            Exp,
            Digit(1),
            Digit(0)
        ]
    );
}

#[test]
fn mode_specific_characters() {
    use PasteCommand::*;

    // '^' only in Scientific, '%' in Scientific and Programmer, 'e' is a hex digit in
    // Programmer.
    assert_eq!(
        keys("2^3", ViewMode::Scientific),
        [ClearEntry, Digit(2), XPowerY, Digit(3)]
    );
    assert_eq!(
        keys("2^3", ViewMode::Standard),
        [ClearEntry, Digit(2), Digit(3)]
    );
    assert_eq!(
        keys("7%2", ViewMode::Programmer),
        [ClearEntry, Digit(7), Mod, Digit(2)]
    );
    assert_eq!(
        keys("7%2", ViewMode::Standard),
        [ClearEntry, Digit(7), Digit(2)]
    );
    assert_eq!(
        keys("0xFE", ViewMode::Programmer),
        [ClearEntry, Digit(0), Digit(0xF), Digit(0xE)]
    );
    assert_eq!(
        keys("4*5/2", ViewMode::Standard),
        [ClearEntry, Digit(4), Multiply, Digit(5), Divide, Digit(2)]
    );
    // A leading '+' is dropped, an unmatched ')' is not sent.
    assert_eq!(keys("+5)", ViewMode::Scientific), [ClearEntry, Digit(5)]);
    // Hex letters cannot carry a pending negation.
    assert_eq!(
        keys("-A1", ViewMode::Programmer),
        [ClearEntry, Digit(0xA), Digit(1)]
    );
}

#[test]
fn error_sentinel_is_a_paste_error() {
    assert_eq!(
        calculator_paste_commands("NoOp", ViewMode::Standard, &EN_US),
        Err(PasteError)
    );
    assert_eq!(
        converter_paste_commands("NoOp", None, &EN_US),
        Err(PasteError)
    );
    assert_eq!(converter_paste_commands("", None, &EN_US), Err(PasteError));
    assert_eq!(
        converter_paste_commands("abc", None, &EN_US),
        Err(PasteError)
    );
}

#[test]
fn pasting_a_minus_after_digits_does_not_negate_the_value() {
    use ConverterPasteCommand::*;
    // Display "53".
    assert_eq!(
        converter_paste_commands("5-3", None, &EN_US).unwrap(),
        [Clear, Digit(5), Digit(3)]
    );
}

#[test]
fn pasting_a_leading_minus_negates_the_value() {
    use ConverterPasteCommand::*;
    // Display "-53".
    assert_eq!(
        converter_paste_commands("-53", None, &EN_US).unwrap(),
        [Clear, Digit(5), Negate, Digit(3)]
    );
}

#[test]
fn rejected_paste_says_why_instead_of_blanking_the_display() {
    use ConverterPasteCommand::*;
    assert_eq!(
        converter_paste_commands("53", None, &EN_US).unwrap(),
        [Clear, Digit(5), Digit(3)]
    );
    assert!(converter_paste_commands("NoOp", None, &EN_US).is_err());
}

#[test]
fn text_with_no_usable_number_is_rejected_before_it_reaches_the_converter() {
    for candidate in ["-", "-abc", ".", "abc"] {
        assert_eq!(
            "NoOp",
            validate_paste_expression_with_group(
                candidate,
                ViewMode::Length,
                CategoryGroupType::Converter,
                NumberBase::Unknown,
                BitLength::BitLengthUnknown
            ),
            "'{candidate}' should be rejected as a paste for a converter."
        );
    }
}

#[test]
fn currency_input_stops_at_the_currency_precision() {
    use ConverterPasteCommand::*;
    assert_eq!(
        converter_paste_commands("1.2345", Some(2), &EN_US).unwrap(),
        [Clear, Digit(1), Decimal, Digit(2), Digit(3)]
    );
    // Other converters are not limited.
    assert_eq!(
        converter_paste_commands("1.2345", None, &EN_US).unwrap(),
        [
            Clear,
            Digit(1),
            Decimal,
            Digit(2),
            Digit(3),
            Digit(4),
            Digit(5)
        ]
    );
}

#[test]
fn converter_modes_validate_through_their_group() {
    for mode in [ViewMode::Currency, ViewMode::Temperature, ViewMode::Data] {
        assert_eq!(mode.group_type(), CategoryGroupType::Converter);
        assert_eq!(
            validate_paste_expression(
                "12.5",
                mode,
                NumberBase::Unknown,
                BitLength::BitLengthUnknown
            ),
            "12.5"
        );
        assert_eq!(
            validate_paste_expression(
                "1+2",
                mode,
                NumberBase::Unknown,
                BitLength::BitLengthUnknown
            ),
            "NoOp"
        );
    }
    assert_eq!(ViewMode::Date.group_type(), CategoryGroupType::Calculator);
    assert_eq!(
        ViewMode::Graphing.group_type(),
        CategoryGroupType::Calculator
    );
    assert_eq!(ViewMode::None.group_type(), CategoryGroupType::None);
}

#[test]
fn copy_uses_the_raw_display_value() {
    assert_eq!(raw_display_value("1,234,567.5", false, &EN_US), "1234567.5");
    assert_eq!(
        raw_display_value("Cannot divide by zero", true, &EN_US),
        "Cannot divide by zero"
    );
}

#[test]
fn localized_digits_and_separators() {
    use PasteCommand::*;

    // de-DE style: ',' decimal separator, '.' group separator.
    let de = PasteLocale {
        decimal_separator: ',',
        group_separator: '.',
        ..PasteLocale::EN_US
    };
    let text = validate_paste_expression_localized(
        "1.234,5",
        ViewMode::Standard,
        CategoryGroupType::Calculator,
        NumberBase::Unknown,
        BitLength::BitLengthUnknown,
        &de,
    );
    assert_eq!(text, "1.234,5");
    assert_eq!(
        calculator_paste_commands(&text, ViewMode::Standard, &de).unwrap(),
        [
            ClearEntry,
            Digit(1),
            Digit(2),
            Digit(3),
            Digit(4),
            Decimal,
            Digit(5)
        ]
    );

    // Arabic-Indic digits.
    let ar = PasteLocale {
        decimal_separator: '\u{066B}',
        group_separator: '\u{066C}',
        digit_symbols: ['٠', '١', '٢', '٣', '٤', '٥', '٦', '٧', '٨', '٩'],
    };
    assert_eq!(
        ar.english_value_from_localized_digits("A\u{0661}\u{0662}\u{0663}"),
        "A123"
    );
    let text = validate_paste_expression_localized(
        "١٢٣٬٤",
        ViewMode::Standard,
        CategoryGroupType::Calculator,
        NumberBase::Unknown,
        BitLength::BitLengthUnknown,
        &ar,
    );
    assert_eq!(text, "١٢٣٬٤");
    assert_eq!(
        calculator_paste_commands(&text, ViewMode::Standard, &ar).unwrap(),
        [ClearEntry, Digit(1), Digit(2), Digit(3), Digit(4)]
    );
    assert_eq!(
        converter_paste_commands("-١٫٥", None, &ar).unwrap(),
        [
            ConverterPasteCommand::Clear,
            ConverterPasteCommand::Digit(1),
            ConverterPasteCommand::Negate,
            ConverterPasteCommand::Decimal,
            ConverterPasteCommand::Digit(5)
        ]
    );

    // LocalizationSettingsTests.TestRemoveGroupSeparators
    assert_eq!(EN_US.remove_group_separators("1,000 000"), "1000000");
}

#[test]
fn map_character_to_button_id_flags() {
    let zero = map_character_to_button_id('0', ViewMode::Standard, &EN_US);
    assert_eq!(zero.button_id, Some(PasteCommand::Digit(0)));
    assert!(!zero.can_send_negate);
    let seven = map_character_to_button_id('7', ViewMode::Standard, &EN_US);
    assert!(seven.can_send_negate);
    assert_eq!(
        map_character_to_button_id(',', ViewMode::Standard, &EN_US).button_id,
        None
    );
    assert_eq!(
        map_character_to_button_id('.', ViewMode::Standard, &EN_US).button_id,
        Some(PasteCommand::Decimal)
    );
}
