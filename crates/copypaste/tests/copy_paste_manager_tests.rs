// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `Calculator.Tests/CopyPasteManagerTests.cs`.
//!
//! C# escapes: `\f` → `\u{0C}`, `\v` → `\u{0B}`, `\x85` → `\u{85}`.

use copypaste::*;

const NOOP: &str = "NoOp";

fn validate_standard_paste_expression(pasted_text: &str) -> String {
    validate_paste_expression(
        pasted_text,
        ViewMode::Standard,
        NumberBase::Unknown,
        BitLength::BitLengthUnknown,
    )
}

fn validate_scientific_paste_expression(pasted_text: &str) -> String {
    validate_paste_expression(
        pasted_text,
        ViewMode::Scientific,
        NumberBase::Unknown,
        BitLength::BitLengthUnknown,
    )
}

fn validate_converter_paste_expression(pasted_text: &str) -> String {
    validate_paste_expression_with_group(
        pasted_text,
        ViewMode::None,
        CategoryGroupType::Converter,
        NumberBase::Unknown,
        BitLength::BitLengthUnknown,
    )
}

fn programmer(base: NumberBase, bits: BitLength) -> impl Fn(&str) -> String {
    move |pasted_text| validate_paste_expression(pasted_text, ViewMode::Programmer, base, bits)
}

fn validate_programmer_hex_qword_paste_expression(pasted_text: &str) -> String {
    programmer(NumberBase::HexBase, BitLength::BitLengthQWord)(pasted_text)
}

/// Asserts every positive input comes back unchanged and every negative one is rejected.
fn check(validate: impl Fn(&str) -> String, positive: &[&str], negative: &[&str], what: &str) {
    for data in positive {
        assert_eq!(
            validate(data),
            *data,
            "{what}: expected {data:?} to be accepted"
        );
    }
    for data in negative {
        assert_eq!(
            validate(data),
            NOOP,
            "{what}: expected {data:?} to be rejected"
        );
    }
}

#[test]
fn functional_copy_paste_test() {
    // The original pastes each input into a StandardCalculatorViewModel and checks that the
    // resulting display re-validates in Standard, Scientific and Programmer (hex, QWORD) mode.
    // The calculator engine is not part of this crate, so the key presses OnPaste produces
    // are checked instead, and the display text those key presses lead to (written out by
    // hand) is re-validated.
    use PasteCommand::*;
    let cases: [(&str, &[PasteCommand], &str); 8] = [
        ("123", &[ClearEntry, Digit(1), Digit(2), Digit(3)], "123"),
        (
            "12345",
            &[ClearEntry, Digit(1), Digit(2), Digit(3), Digit(4), Digit(5)],
            "12,345",
        ),
        (
            "123+456",
            &[
                ClearEntry,
                Digit(1),
                Digit(2),
                Digit(3),
                Add,
                Digit(4),
                Digit(5),
                Digit(6),
            ],
            "456",
        ),
        (
            "1,234",
            &[ClearEntry, Digit(1), Digit(2), Digit(3), Digit(4)],
            "1,234",
        ),
        ("1 2 3", &[ClearEntry, Digit(1), Digit(2), Digit(3)], "123"),
        (
            "\n\r1,234\n",
            &[ClearEntry, Digit(1), Digit(2), Digit(3), Digit(4)],
            "1,234",
        ),
        ("\n 1+\n2 ", &[ClearEntry, Digit(1), Add, Digit(2)], "2"),
        ("1\"2", &[ClearEntry, Digit(1), Digit(2)], "12"),
    ];

    for (input, expected_commands, display_value) in cases {
        let pasted = validate_standard_paste_expression(input);
        assert_eq!(pasted, input);
        let commands =
            calculator_paste_commands(&pasted, ViewMode::Standard, &PasteLocale::EN_US).unwrap();
        assert_eq!(commands, expected_commands, "commands for {input:?}");

        assert_eq!(
            validate_standard_paste_expression(display_value),
            display_value
        );
        assert_eq!(
            validate_scientific_paste_expression(display_value),
            display_value
        );
        assert_eq!(
            validate_programmer_hex_qword_paste_expression(display_value),
            display_value
        );
    }
}

#[test]
fn validate_standard_paste_expression_test() {
    let positive_input = [
        "123",
        "+123",
        "-133",
        "12345.",
        "+12.34",
        "12.345",
        "012.034",
        "-23.032",
        "-.123",
        ".1234",
        "012.012",
        "123+456",
        "123+-234",
        "123*-345",
        "123*4*-3",
        "123*+4*-3",
        "1,234",
        "1 2 3",
        "\n\r1,234\n",
        "\u{0C}\n1+2\t\r\u{0B}\u{85}",
        "\n 1+\n2 ",
        "1\"2",
        "1234567891234567",
        "2+2=",
        "2+2=   ",
        "1.2e23",
        "12345e-23",
    ];
    let negative_input = [
        "(123)+(456)",
        "abcdef",
        "xyz",
        "ABab",
        "e+234",
        "12345678912345678",
        "SIN(2)",
        "2+2==",
        "2=+2",
        "2%2",
        "10^2",
    ];

    check(
        validate_standard_paste_expression,
        &positive_input,
        &negative_input,
        "standard",
    );
}

#[test]
fn validate_scientific_paste_expression_test() {
    let positive_input = [
        "123",
        "+123",
        "-133",
        "123+456",
        "12345e+023",
        "1,234",
        "1.23",
        "-.123",
        ".1234",
        "012.012",
        "123+-234",
        "123*-345",
        "123*4*-3",
        "123*+4*-3",
        "1 2 3",
        "\n\r1,234\n",
        "\u{0C}\n1+2\t\r\u{0B}\u{85}",
        "\n 1+\n2 ",
        "1\"2",
        "1.2e+023",
        "12345e-23",
        "(123)+(456)",
        "12345678912345678123456789012345",
        "(123)+(456)=",
        "2+2=   ",
        "-(43)",
        "+(41213)",
        "-(432+3232)",
        "-(+(-3213)+(-2312))",
        "-(-(432+3232))",
        "1.2e23",
        "12^2",
        "-12.12^-2",
        "61%99-6.1%99",
        "1.1111111111111111111111111111111e+1142",
    ];
    let negative_input = [
        "abcdef",
        "xyz",
        "ABab",
        "e+234",
        "123456789123456781234567890123456",
        "11.1111111111111111111111111111111e+1142",
        "1.1e+10001",
        "0.11111111111111111111111111111111111e+111111SIN(2)",
        "2+2==",
        "2=+2",
    ];

    check(
        validate_scientific_paste_expression,
        &positive_input,
        &negative_input,
        "scientific",
    );
}

#[test]
fn scientific_paste_requires_a_numeric_operand() {
    for candidate in ["", "   ", "+", "-"] {
        assert_eq!(
            NOOP,
            validate_scientific_paste_expression(candidate),
            "{candidate:?}"
        );
    }
}

#[test]
fn scientific_paste_allows_terminal_operators() {
    for candidate in ["50%", "1+"] {
        assert_eq!(
            candidate,
            validate_scientific_paste_expression(candidate),
            "{candidate:?}"
        );
    }
}

#[test]
fn scientific_paste_rejects_digits_unsupported_by_calculator() {
    for candidate in ["\u{FF11}", "1+\u{FF11}", "1e+\u{FF11}"] {
        assert_eq!(
            NOOP,
            validate_scientific_paste_expression(candidate),
            "{candidate:?}"
        );
    }
}

#[test]
fn validate_programmer_dec_paste_expression_test() {
    // QWord
    let qword_positive_input = [
        "123",
        "+123",
        "-133",
        "123+456",
        "1,234",
        "1 2 3",
        "1'2'3'4",
        "1_2_3_4",
        "\n\r1,234\n",
        "\u{0C}\n1+2\t\r\u{0B}\u{85}",
        "\n 1+\n2 ",
        "1\"2",
        "(123)+(456)",
        "123+-234",
        "123*-345",
        "123*4*-3",
        "123*+4*-3",
        "9223372036854775807",
        "-9223372036854775808",
        "0n1234",
        "0N1234",
        "1234u",
        "1234ul",
        "1234ULL",
        "2+2=",
        "2+2=   ",
        "823%21",
    ];
    let qword_negative_input = [
        "1.23",
        "1''2",
        "'123",
        "123'",
        "1__2",
        "_123",
        "123_",
        "1.2e23",
        "1.2e+023",
        "12345e-23",
        "abcdef",
        "xyz",
        "ABab",
        "e+234",
        "9223372036854775808",
        "9223372036854775809",
        "SIN(2)",
        "-0n123",
        "0nn1234",
        "1234uu",
        "1234ulll",
        "2+2==",
        "2=+2",
    ];
    check(
        programmer(NumberBase::DecBase, BitLength::BitLengthQWord),
        &qword_positive_input,
        &qword_negative_input,
        "dec qword",
    );

    // DWord
    let dword_positive_input = [
        "123",
        "+123",
        "-133",
        "123+456",
        "1,234",
        "1 2 3",
        "1'2'3'4",
        "1_2_3_4",
        "\n\r1,234\n",
        "\u{0C}\n1+2\t\r\u{0B}\u{85}",
        "\n 1+\n2 ",
        "1\"2",
        "(123)+(456)",
        "123+-234",
        "123*-345",
        "123*4*-3",
        "123*+4*-3",
        "2147483647",
        "-2147483647",
        "0n1234",
        "0N1234",
        "1234u",
        "1234ul",
        "1234ULL",
    ];
    let dword_negative_input = [
        "1.23",
        "1''2",
        "'123",
        "123'",
        "1__2",
        "_123",
        "123_",
        "1.2e23",
        "1.2e+023",
        "12345e-23",
        "abcdef",
        "xyz",
        "ABab",
        "e+234",
        "2147483649",
        "SIN(2)",
        "-0n123",
        "0nn1234",
        "1234uu",
        "1234ulll",
    ];
    check(
        programmer(NumberBase::DecBase, BitLength::BitLengthDWord),
        &dword_positive_input,
        &dword_negative_input,
        "dec dword",
    );

    // Word
    let word_positive_input = [
        "123",
        "+123",
        "-133",
        "123+456",
        "1,234",
        "1 2 3",
        "1'2'3'4",
        "1_2_3_4",
        "\u{0C}\n1+2\t\r\u{0B}\u{85}",
        "1\"2",
        "(123)+(456)",
        "123+-234",
        "123*-345",
        "123*4*-3",
        "123*+4*-3",
        "32767",
        "-32767",
        "-32768",
        "0n1234",
        "0N1234",
        "1234u",
        "1234ul",
        "1234ULL",
    ];
    let word_negative_input = [
        "1.23",
        "1''2",
        "'123",
        "123'",
        "1__2",
        "_123",
        "123_",
        "1.2e23",
        "1.2e+023",
        "12345e-23",
        "abcdef",
        "xyz",
        "ABab",
        "e+234",
        "32769",
        "SIN(2)",
        "-0n123",
        "0nn1234",
        "1234uu",
        "1234ulll",
    ];
    check(
        programmer(NumberBase::DecBase, BitLength::BitLengthWord),
        &word_positive_input,
        &word_negative_input,
        "dec word",
    );

    // Byte
    let byte_positive_input = [
        "13", "+13", "-13", "13+46", "13+-34", "13*-3", "3*4*-3", "3*+4*-3", "1,3", "1 3", "1'2'3",
        "1_2_3", "1\"2", "127", "-127", "0n123", "0N123", "123u", "123ul", "123ULL",
    ];
    let byte_negative_input = [
        "1.23", "1''2", "'123", "123'", "1__2", "_123", "123_", "1.2e23", "1.2e+023", "15e-23",
        "abcdef", "xyz", "ABab", "e+24", "129", "SIN(2)", "-0n123", "0nn1234", "123uu", "123ulll",
    ];
    check(
        programmer(NumberBase::DecBase, BitLength::BitLengthByte),
        &byte_positive_input,
        &byte_negative_input,
        "dec byte",
    );
}

#[test]
fn validate_programmer_oct_paste_expression_test() {
    // QWord
    let qword_positive_input = [
        "123",
        "123+456",
        "1,234",
        "1 2 3",
        "1'2'3'4",
        "1_2_3_4",
        "\n\r1,234\n",
        "\u{0C}\n1+2\t\r\u{0B}\u{85}",
        "\n 1+\n2 ",
        "1\"2",
        "(123)+(456)",
        "0t1234",
        "0T1234",
        "0o1234",
        "0O1234",
        "1234u",
        "1234ul",
        "1234ULL",
        "2+2=",
        "2+2=   ",
        "127%71",
        "1777777777777777777777",
    ];
    let qword_negative_input = [
        "+123",
        "1.23",
        "1''2",
        "'123",
        "123'",
        "1__2",
        "_123",
        "123_",
        "-133",
        "1.2e23",
        "1.2e+023",
        "12345e-23",
        "abcdef",
        "xyz",
        "ABab",
        "e+234",
        "12345678901234567890123",
        "2000000000000000000000",
        "SIN(2)",
        "123+-234",
        "0ot1234",
        "1234uu",
        "1234ulll",
        "2+2==",
        "2=+2",
        "89%12",
    ];
    check(
        programmer(NumberBase::OctBase, BitLength::BitLengthQWord),
        &qword_positive_input,
        &qword_negative_input,
        "oct qword",
    );

    // DWord
    let dword_positive_input = [
        "123",
        "123+456",
        "1,234",
        "1 2 3",
        "1'2'3'4",
        "1_2_3_4",
        "\n\r1,234\n",
        "\u{0C}\n1+2\t\r\u{0B}\u{85}",
        "\n 1+\n2 ",
        "1\"2",
        "(123)+(456)",
        "37777777777",
        "0t1234",
        "0T1234",
        "0o1234",
        "0O1234",
        "1234u",
        "1234ul",
        "1234ULL",
    ];
    let dword_negative_input = [
        "+123",
        "1.23",
        "1''2",
        "'123",
        "123'",
        "1__2",
        "_123",
        "123_",
        "-133",
        "1.2e23",
        "1.2e+023",
        "12345e-23",
        "abcdef",
        "xyz",
        "ABab",
        "e+234",
        "377777777771",
        "40000000000",
        "SIN(2)",
        "123+-234",
        "0ot1234",
        "1234uu",
        "1234ulll",
    ];
    check(
        programmer(NumberBase::OctBase, BitLength::BitLengthDWord),
        &dword_positive_input,
        &dword_negative_input,
        "oct dword",
    );

    // Word
    let word_positive_input = [
        "123",
        "123+456",
        "1,234",
        "1 2 3",
        "1'2'3'4",
        "1_2_3_4",
        "\u{0C}\n1+2\t\r\u{0B}\u{85}",
        "1\"2",
        "(123)+(456)",
        "177777",
        "0t1234",
        "0T1234",
        "0o1234",
        "0O1234",
        "1234u",
        "1234ul",
        "1234ULL",
    ];
    let word_negative_input = [
        "+123",
        "1.23",
        "1''2",
        "'123",
        "123'",
        "1__2",
        "_123",
        "123_",
        "-133",
        "1.2e23",
        "1.2e+023",
        "12345e-23",
        "abcdef",
        "xyz",
        "ABab",
        "e+234",
        "1777771",
        "200000",
        "SIN(2)",
        "123+-234",
        "0ot1234",
        "1234uu",
        "1234ulll",
    ];
    check(
        programmer(NumberBase::OctBase, BitLength::BitLengthWord),
        &word_positive_input,
        &word_negative_input,
        "oct word",
    );

    // Byte
    let byte_positive_input = [
        "13", "13+46", "1,3", "1 3", "1'2'3", "1_2_3", "1\"2", "377", "0t123", "0T123", "0o123",
        "0O123", "123u", "123ul", "123ULL",
    ];
    let byte_negative_input = [
        "+123", "1.23", "1''2", "'123", "123'", "1__2", "_123", "123_", "-13", "1.2e23",
        "1.2e+023", "15e-23", "abcdef", "xyz", "ABab", "e+24", "477", "400", "SIN(2)", "123+-34",
        "0ot123", "123uu", "123ulll",
    ];
    check(
        programmer(NumberBase::OctBase, BitLength::BitLengthByte),
        &byte_positive_input,
        &byte_negative_input,
        "oct byte",
    );
}

#[test]
fn validate_programmer_hex_paste_expression_test() {
    // QWord
    let qword_positive_input = [
        "123",
        "123+456",
        "1,234",
        "1 2 3",
        "1'2'3'4",
        "1_2_3_4",
        "12345e-23",
        "\n\r1,234\n",
        "\u{0C}\n1+2\t\r\u{0B}\u{85}",
        "\u{0C}\n1+2\t\r\u{0B}\u{85}",
        "\n 1+\n2 ",
        "e+234",
        "1\"2",
        "(123)+(456)",
        "abcdef",
        "ABab",
        "ABCDF21abc41a",
        "0x1234",
        "0xab12",
        "0X1234",
        "AB12h",
        "BC34H",
        "1234u",
        "1234ul",
        "1234ULL",
        "2+2=",
        "2+2=   ",
        "A4C3%12",
        "1233%AB",
        "FFC1%F2",
    ];
    let qword_negative_input = [
        "+123",
        "1.23",
        "1''2",
        "'123",
        "123'",
        "1__2",
        "_123",
        "123_",
        "-133",
        "1.2e+023",
        "1.2e23",
        "xyz",
        "ABCDEF21abc41abc7",
        "SIN(2)",
        "123+-234",
        "1234x",
        "A0x1234",
        "0xx1234",
        "1234uu",
        "1234ulll",
        "2+2==",
        "2=+2",
    ];
    check(
        programmer(NumberBase::HexBase, BitLength::BitLengthQWord),
        &qword_positive_input,
        &qword_negative_input,
        "hex qword",
    );

    // DWord
    let dword_positive_input = [
        "123",
        "123+456",
        "1,234",
        "1 2 3",
        "1'2'3'4",
        "1_2_3_4",
        "12345e-23",
        "\n\r1,234\n",
        "\u{0C}\n1+2\t\r\u{0B}\u{85}",
        "\n 1+\n2 ",
        "e+234",
        "1\"2",
        "(123)+(456)",
        "abcdef",
        "ABab",
        "ABCD123a",
        "0x1234",
        "0xab12",
        "0X1234",
        "AB12h",
        "BC34H",
        "1234u",
        "1234ul",
        "1234ULL",
    ];
    let dword_negative_input = [
        "+123",
        "1.23",
        "1''2",
        "'123",
        "123'",
        "1__2",
        "_123",
        "123_",
        "-133",
        "1.2e+023",
        "1.2e23",
        "xyz",
        "ABCD123ab",
        "SIN(2)",
        "123+-234",
        "1234x",
        "A0x1234",
        "0xx1234",
        "1234uu",
        "1234ulll",
    ];
    check(
        programmer(NumberBase::HexBase, BitLength::BitLengthDWord),
        &dword_positive_input,
        &dword_negative_input,
        "hex dword",
    );

    // Word
    let word_positive_input = [
        "123",
        "13+456",
        "1,34",
        "12 3",
        "1'2'3'4",
        "1_2_3_4",
        "15e-23",
        "\r1",
        "\n\r1,4",
        "\n1,4\n",
        "\u{0C}\n1+2\t\r\u{0B}",
        "\n 1+\n2 ",
        "e+24",
        "1\"2",
        "(23)+(4)",
        "aef",
        "ABab",
        "A1a3",
        "FFFF",
        "0x1234",
        "0xab12",
        "0X1234",
        "AB12h",
        "BC34H",
        "1234u",
        "1234ul",
        "1234ULL",
    ];
    let word_negative_input = [
        "+123", "1.23", "1''2", "'123", "123'", "1__2", "_123", "123_", "-133", "1.2e+023",
        "1.2e23", "xyz", "A1a3b", "SIN(2)", "123+-234", "1234x", "A0x1234", "0xx1234", "1234uu",
        "1234ulll",
    ];
    check(
        programmer(NumberBase::HexBase, BitLength::BitLengthWord),
        &word_positive_input,
        &word_negative_input,
        "hex word",
    );

    // Byte
    let byte_positive_input = [
        "13", "13+6", "1,4", "2 3", "1'2", "1_2", "5e-3", "\r1", "a", "ab", "A1", "0x12", "0xab",
        "0X12", "A9h", "B8H", "12u", "12ul", "12ULL",
    ];
    let byte_negative_input = [
        "+3", "1.2", "1''2", "'12", "12'", "1__2", "_12", "12_", "-3", "1.1e+02", "1.2e3", "xz",
        "A3a", "SIN(2)", "13+-23", "12x", "A0x1", "0xx12", "12uu", "12ulll",
    ];
    check(
        programmer(NumberBase::HexBase, BitLength::BitLengthByte),
        &byte_positive_input,
        &byte_negative_input,
        "hex byte",
    );
}

#[test]
fn validate_programmer_bin_paste_expression_test() {
    // QWord
    let qword_positive_input = [
        "100",
        "100+101",
        "1,001",
        "1 0 1",
        "1'0'0'1",
        "1_0_0_1",
        "\n\r1,010\n",
        "\u{0C}\n1+11\t\r\u{0B}\u{85}",
        "\n 1+\n1 ",
        "1\"1",
        "(101)+(10)",
        "0b1001",
        "0B1111",
        "0y1001",
        "0Y1001",
        "1100b",
        "1101B",
        "1111u",
        "1111ul",
        "1111ULL",
        "1010101010101010101010101011110110100100101010101001010101001010",
        "1+10=",
        "1+10=   ",
        "1001%10",
    ];
    let qword_negative_input = [
        "+10101",
        "1.01",
        "1''0",
        "'101",
        "101'",
        "1__0",
        "_101",
        "101_",
        "-10101001",
        "123",
        "1.2e23",
        "1.2e+023",
        "101010e-1010",
        "abcdef",
        "xyz",
        "ABab",
        "e+10101",
        "b1001",
        "10b01",
        "0x10",
        "1001x",
        "1001h",
        "0bb1111",
        "1111uu",
        "1111ulll",
        "10101010101010101010101010111101101001001010101010010101010010100",
        "SIN(01010)",
        "10+-10101010101",
        "1+10==",
        "1=+10",
    ];
    check(
        programmer(NumberBase::BinBase, BitLength::BitLengthQWord),
        &qword_positive_input,
        &qword_negative_input,
        "bin qword",
    );

    // DWord
    let dword_positive_input = [
        "100",
        "100+101",
        "1,001",
        "1 0 1",
        "1'0'0'1",
        "1_0_0_1",
        "\n\r1,010\n",
        "\u{0C}\n1+11\t\r\u{0B}\u{85}",
        "\n 1+\n1 ",
        "1\"1",
        "(101)+(10)",
        "0b1001",
        "0B1111",
        "0y1001",
        "0Y1001",
        "1100b",
        "1101B",
        "1111u",
        "1111ul",
        "1111ULL",
        "10101001001010101101010111111100",
    ];
    let dword_negative_input = [
        "+10101",
        "1.01",
        "1''0",
        "'101",
        "101'",
        "1__0",
        "_101",
        "101_",
        "-10101001",
        "123",
        "1.2e23",
        "1.2e+023",
        "101010e-1010",
        "abcdef",
        "xyz",
        "ABab",
        "e+10101",
        "b1001",
        "10b01",
        "0x10",
        "1001x",
        "1001h",
        "0bb1111",
        "1111uu",
        "1111ulll",
        "101010010010101011010101111111001",
        "SIN(01010)",
        "10+-10101010101",
    ];
    check(
        programmer(NumberBase::BinBase, BitLength::BitLengthDWord),
        &dword_positive_input,
        &dword_negative_input,
        "bin dword",
    );

    // Word
    let word_positive_input = [
        "100",
        "100+101",
        "1,001",
        "1 0 1",
        "1'0'0'1",
        "1_0_0_1",
        "\n\r1,010\n",
        "\u{0C}\n1+11\t\r\u{0B}\u{85}",
        "\n 1+\n1 ",
        "1\"1",
        "(101)+(10)",
        "0b1001",
        "0B1111",
        "0y1001",
        "0Y1001",
        "1100b",
        "1101B",
        "1111u",
        "1111ul",
        "1111ULL",
        "1010101010010010",
    ];
    let word_negative_input = [
        "+10101",
        "1.01",
        "1''0",
        "'101",
        "101'",
        "1__0",
        "_101",
        "101_",
        "-10101001",
        "123",
        "1.2e23",
        "1.2e+023",
        "101010e-1010",
        "abcdef",
        "xyz",
        "ABab",
        "e+10101",
        "b1001",
        "10b01",
        "0x10",
        "1001x",
        "1001h",
        "0bb1111",
        "1111uu",
        "1111ulll",
        "10101010100100101",
        "SIN(01010)",
        "10+-10101010101",
    ];
    check(
        programmer(NumberBase::BinBase, BitLength::BitLengthWord),
        &word_positive_input,
        &word_negative_input,
        "bin word",
    );

    // Byte
    let byte_positive_input = [
        "100",
        "100+101",
        "1,001",
        "1 0 1",
        "1'0'0'1",
        "1_0_0_1",
        "\n\r1,010\n",
        "\n 1+\n1 ",
        "1\"1",
        "(101)+(10)",
        "0b1001",
        "0B1111",
        "0y1001",
        "0Y1001",
        "1100b",
        "1101B",
        "1111u",
        "1111ul",
        "1111ULL",
        "10100010",
        "11111111",
    ];
    let byte_negative_input = [
        "+10101",
        "1.01",
        "1''0",
        "'101",
        "101'",
        "1__0",
        "_101",
        "101_",
        "-10101001",
        "123",
        "1.2e23",
        "1.2e+023",
        "101010e-1010",
        "abcdef",
        "xyz",
        "ABab",
        "e+10101",
        "b1001",
        "10b01",
        "0x10",
        "1001x",
        "1001h",
        "0bb1111",
        "1111uu",
        "1111ulll",
        "101000101",
        "100000000",
        "SIN(01010)",
        "10+-1010101",
    ];
    check(
        programmer(NumberBase::BinBase, BitLength::BitLengthByte),
        &byte_positive_input,
        &byte_negative_input,
        "bin byte",
    );
}

#[test]
fn programmer_prefixed_values_respect_byte_and_word_ranges() {
    use BitLength::*;
    use NumberBase::*;
    let cases = [
        ("0n127", DecBase, BitLengthByte, "0n127"),
        ("0n128", DecBase, BitLengthByte, "NoOp"),
        ("0n32767", DecBase, BitLengthWord, "0n32767"),
        ("0n32768", DecBase, BitLengthWord, "NoOp"),
        ("0o377", OctBase, BitLengthByte, "0o377"),
        ("0o400", OctBase, BitLengthByte, "NoOp"),
        ("0t177777", OctBase, BitLengthWord, "0t177777"),
        ("0t200000", OctBase, BitLengthWord, "NoOp"),
        ("0b11111111", BinBase, BitLengthByte, "0b11111111"),
        ("0b100000000", BinBase, BitLengthByte, "NoOp"),
        (
            "0y1111111111111111",
            BinBase,
            BitLengthWord,
            "0y1111111111111111",
        ),
        ("0y10000000000000000", BinBase, BitLengthWord, "NoOp"),
        ("0xFF", HexBase, BitLengthByte, "0xFF"),
        ("0x100", HexBase, BitLengthByte, "NoOp"),
        ("0xFFFF", HexBase, BitLengthWord, "0xFFFF"),
        ("0x10000", HexBase, BitLengthWord, "NoOp"),
    ];

    for (value, base, length, expected) in cases {
        assert_eq!(
            expected,
            validate_paste_expression(value, ViewMode::Programmer, base, length),
            "{value}"
        );
    }
}

#[test]
fn binary_zero_with_suffix_remains_valid() {
    assert_eq!(
        "0b",
        programmer(NumberBase::BinBase, BitLength::BitLengthByte)("0b")
    );
}

#[test]
fn validate_converter_paste_expression_test() {
    let positive_input = [
        "123",
        "+123",
        "-133",
        "12345.",
        "012.012",
        "1,234",
        "1 2 3",
        "\n\r1,234\n",
        "\u{0C}\n12\t\r\u{0B}\u{85}",
        "1\"2",
        "100=",
        "100=   ",
    ];
    let negative_input = [
        "(123)+(456)",
        "1.2e23",
        "12345e-23",
        "\n 1+\n2 ",
        "123+456",
        "abcdef",
        "\n 1+\n2 ",
        "xyz",
        "ABab",
        "e+234",
        "12345678912345678",
        "SIN(2)",
        "123+-234",
        "100==",
        "=100",
    ];

    check(
        validate_converter_paste_expression,
        &positive_input,
        &negative_input,
        "converter",
    );
}

#[test]
fn validate_paste_expression_error_states() {
    let mut exp_too_long = String::new();
    for _ in 0..MAX_PASTEABLE_LENGTH / 8 {
        exp_too_long += "-1234567";
    }
    assert_eq!(
        validate_paste_expression_with_group(
            &exp_too_long,
            ViewMode::Standard,
            CategoryGroupType::Calculator,
            NumberBase::Unknown,
            BitLength::BitLengthUnknown
        ),
        exp_too_long
    );
    exp_too_long += "1";
    assert_eq!(
        validate_paste_expression_with_group(
            &exp_too_long,
            ViewMode::Standard,
            CategoryGroupType::Calculator,
            NumberBase::Unknown,
            BitLength::BitLengthUnknown
        ),
        "NoOp"
    );

    assert_eq!(
        validate_paste_expression_with_group(
            "",
            ViewMode::Standard,
            CategoryGroupType::Calculator,
            NumberBase::Unknown,
            BitLength::BitLengthUnknown
        ),
        "NoOp"
    );

    assert_eq!(
        validate_paste_expression_with_group(
            "1a23f456",
            ViewMode::Standard,
            CategoryGroupType::Calculator,
            NumberBase::Unknown,
            BitLength::BitLengthUnknown
        ),
        "NoOp"
    );

    assert_eq!(
        validate_paste_expression_with_group(
            "123",
            ViewMode::None,
            CategoryGroupType::None,
            NumberBase::Unknown,
            BitLength::BitLengthUnknown
        ),
        "NoOp"
    );
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn validate_extract_operands() {
    assert_eq!(
        extract_operands("123456", ViewMode::Standard),
        strings(&["123456"])
    );
    assert_eq!(
        extract_operands("123^456", ViewMode::Standard),
        strings(&["123^456"])
    );

    let two_operands = strings(&["123", "456"]);
    assert_eq!(
        extract_operands("123+456", ViewMode::Standard),
        two_operands
    );
    assert_eq!(
        extract_operands("123-456", ViewMode::Standard),
        two_operands
    );
    assert_eq!(
        extract_operands("123*456", ViewMode::Standard),
        two_operands
    );
    assert_eq!(
        extract_operands("123/456", ViewMode::Standard),
        two_operands
    );

    assert_eq!(
        extract_operands("123e456", ViewMode::Standard),
        strings(&["123e456"])
    );
    assert_eq!(
        extract_operands("123e4567", ViewMode::Standard),
        strings(&["123e4567"])
    );

    assert_eq!(
        extract_operands("((45)+(-30))", ViewMode::Scientific),
        strings(&["((45)", "(-30))"])
    );
}

#[test]
fn validate_extract_operands_errors() {
    let mut exp_operand_limit = String::new();
    for _ in 0..MAX_OPERAND_COUNT {
        exp_operand_limit += "+1";
    }
    assert_eq!(
        extract_operands(&exp_operand_limit, ViewMode::Standard).len(),
        100
    );

    exp_operand_limit += "+1";
    assert_eq!(
        extract_operands(&exp_operand_limit, ViewMode::Standard).len(),
        0
    );

    assert_eq!(extract_operands("12e9999", ViewMode::Standard).len(), 1);
    assert_eq!(extract_operands("12e10000", ViewMode::Standard).len(), 0);
}

#[test]
fn validate_expression_reg_ex_match() {
    use BitLength::*;
    use CategoryGroupType::{Calculator, Converter};
    use NumberBase::*;

    assert!(!expression_regex_match(
        &[],
        ViewMode::Standard,
        Calculator,
        Unknown,
        BitLengthUnknown
    ));
    assert!(!expression_regex_match(
        &strings(&["123"]),
        ViewMode::None,
        Calculator,
        Unknown,
        BitLengthUnknown
    ));
    assert!(!expression_regex_match(
        &strings(&["123"]),
        ViewMode::Currency,
        CategoryGroupType::None,
        Unknown,
        BitLengthUnknown
    ));

    // Verify operand lengths > max return false
    assert!(!expression_regex_match(
        &strings(&["12345678901234567"]),
        ViewMode::Standard,
        Calculator,
        Unknown,
        BitLengthUnknown
    ));
    assert!(!expression_regex_match(
        &strings(&["123456789012345678901234567890123"]),
        ViewMode::Scientific,
        Calculator,
        Unknown,
        BitLengthUnknown
    ));
    assert!(!expression_regex_match(
        &strings(&["12345678901234567"]),
        ViewMode::None,
        Converter,
        Unknown,
        BitLengthUnknown
    ));
    assert!(!expression_regex_match(
        &strings(&["11111111111111111"]),
        ViewMode::Programmer,
        Calculator,
        HexBase,
        BitLengthQWord
    ));
    assert!(!expression_regex_match(
        &strings(&["12345678901234567890"]),
        ViewMode::Programmer,
        Calculator,
        DecBase,
        BitLengthQWord
    ));
    assert!(!expression_regex_match(
        &strings(&["11111111111111111111111"]),
        ViewMode::Programmer,
        Calculator,
        OctBase,
        BitLengthQWord
    ));
    assert!(!expression_regex_match(
        &strings(&["10000000000000000000000000000000000000000000000000000000000000000"]),
        ViewMode::Programmer,
        Calculator,
        BinBase,
        BitLengthQWord
    ));

    assert!(!expression_regex_match(
        &strings(&["9223372036854775808"]),
        ViewMode::Programmer,
        Calculator,
        DecBase,
        BitLengthQWord
    ));

    assert!(expression_regex_match(
        &strings(&["((((((((((((((((((((123))))))))))))))))))))"]),
        ViewMode::Scientific,
        Calculator,
        Unknown,
        BitLengthUnknown
    ));
    assert!(expression_regex_match(
        &strings(&["9223372036854775807"]),
        ViewMode::Programmer,
        Calculator,
        DecBase,
        BitLengthQWord
    ));
    assert!(expression_regex_match(
        &strings(&["-9223372036854775808"]),
        ViewMode::Programmer,
        Calculator,
        DecBase,
        BitLengthQWord
    ));

    // Verify all operands must match patterns
    assert!(expression_regex_match(
        &strings(&["123", "456"]),
        ViewMode::Standard,
        Calculator,
        Unknown,
        BitLengthUnknown
    ));
    assert!(expression_regex_match(
        &strings(&["123", "1e23"]),
        ViewMode::Standard,
        Calculator,
        Unknown,
        BitLengthUnknown
    ));
    assert!(!expression_regex_match(
        &strings(&["123", "fab10"]),
        ViewMode::Standard,
        Calculator,
        Unknown,
        BitLengthUnknown
    ));

    assert!(expression_regex_match(
        &strings(&[
            "1.23e+456",
            "1.23e456",
            ".23e+456",
            "123e-456",
            "12e2",
            "12e+2",
            "12e-2",
            "-12e2",
            "-12e+2",
            "-12e-2"
        ]),
        ViewMode::Scientific,
        Calculator,
        Unknown,
        BitLengthUnknown
    ));

    assert!(!expression_regex_match(
        &strings(&["123", "12345678901234567"]),
        ViewMode::Standard,
        Calculator,
        Unknown,
        BitLengthUnknown
    ));
    assert!(!expression_regex_match(
        &strings(&["123", "9223372036854775808"]),
        ViewMode::Programmer,
        Calculator,
        DecBase,
        BitLengthQWord
    ));
}

#[test]
fn validate_get_max_operand_length_and_value() {
    use BitLength::*;
    use NumberBase::*;
    let none = CategoryGroupType::None;

    let result =
        get_max_operand_length_and_value(ViewMode::Standard, none, Unknown, BitLengthUnknown);
    assert_eq!(result.max_length, MAX_STANDARD_OPERAND_LENGTH);
    assert_eq!(result.max_value, 0);

    let result =
        get_max_operand_length_and_value(ViewMode::Scientific, none, Unknown, BitLengthUnknown);
    assert_eq!(result.max_length, MAX_SCIENTIFIC_OPERAND_LENGTH);
    assert_eq!(result.max_value, 0);

    let result = get_max_operand_length_and_value(
        ViewMode::None,
        CategoryGroupType::Converter,
        Unknown,
        BitLengthUnknown,
    );
    assert_eq!(result.max_length, MAX_CONVERTER_INPUT_LENGTH);
    assert_eq!(result.max_value, 0);

    let ull_qword_max = u64::MAX;
    let ull_dword_max = u64::from(u32::MAX);
    let ull_word_max = u64::from(u16::MAX);
    let ull_byte_max = u64::from(u8::MAX);

    let expect = |base, bits, max_length: u32, max_value: u64| {
        let result = get_max_operand_length_and_value(ViewMode::Programmer, none, base, bits);
        assert_eq!(result.max_length, max_length, "{base:?} {bits:?} length");
        assert_eq!(result.max_value, max_value, "{base:?} {bits:?} value");
    };

    // Hex
    expect(HexBase, BitLengthQWord, 16, ull_qword_max);
    expect(HexBase, BitLengthDWord, 8, ull_dword_max);
    expect(HexBase, BitLengthWord, 4, ull_word_max);
    expect(HexBase, BitLengthByte, 2, ull_byte_max);

    // Dec
    expect(DecBase, BitLengthQWord, 19, ull_qword_max >> 1);
    expect(DecBase, BitLengthDWord, 10, ull_dword_max >> 1);
    expect(DecBase, BitLengthWord, 5, ull_word_max >> 1);
    expect(DecBase, BitLengthByte, 3, ull_byte_max >> 1);

    // Oct
    expect(OctBase, BitLengthQWord, 22, ull_qword_max);
    expect(OctBase, BitLengthDWord, 11, ull_dword_max);
    expect(OctBase, BitLengthWord, 6, ull_word_max);
    expect(OctBase, BitLengthByte, 3, ull_byte_max);

    // Bin
    expect(BinBase, BitLengthQWord, 64, ull_qword_max);
    expect(BinBase, BitLengthDWord, 32, ull_dword_max);
    expect(BinBase, BitLengthWord, 16, ull_word_max);
    expect(BinBase, BitLengthByte, 8, ull_byte_max);

    // Invalid
    let result = get_max_operand_length_and_value(ViewMode::None, none, Unknown, BitLengthUnknown);
    assert_eq!(result.max_length, 0);
    assert_eq!(result.max_value, 0);
}

#[test]
fn validate_sanitize_operand() {
    assert_eq!(sanitize_operand("((1234"), "1234");
    assert_eq!(sanitize_operand("1234))"), "1234");
    assert_eq!(sanitize_operand("1234))"), "1234");
    assert_eq!(sanitize_operand("-1234"), "1234");
    assert_eq!(sanitize_operand("+1234"), "1234");
    assert_eq!(sanitize_operand("-(1234)"), "1234");
    assert_eq!(sanitize_operand("+(1234)"), "1234");
    assert_eq!(sanitize_operand("12-34"), "1234");
    assert_eq!(sanitize_operand("((((1234))))"), "1234");
    assert_eq!(sanitize_operand("1'2'3'4"), "1234");
    assert_eq!(sanitize_operand("'''''1234''''"), "1234");
    assert_eq!(sanitize_operand("1_2_3_4"), "1234");
    assert_eq!(sanitize_operand("______1234___"), "1234");
}

#[test]
fn validate_prefix_currency_symbols() {
    for symbol in [
        '\u{00A5}', '\u{00A4}', '\u{20B5}', '\u{0024}', '\u{20A1}', '\u{20A9}', '\u{20AA}',
        '\u{20A6}', '\u{20B9}', '\u{00A3}', '\u{20AC}',
    ] {
        assert_eq!(
            remove_unwanted_chars_from_string(&format!("{symbol}5")),
            "5",
            "{symbol}"
        );
    }
}

#[test]
fn validate_try_operand_to_ull() {
    use NumberBase::*;

    // Hex
    assert_eq!(try_operand_to_ull("1234", HexBase), Some(0x1234));
    assert_eq!(try_operand_to_ull("FF", HexBase), Some(0xFF));
    assert_eq!(
        try_operand_to_ull("FFFFFFFFFFFFFFFF", HexBase),
        Some(0xFFFF_FFFF_FFFF_FFFF)
    );
    assert_eq!(
        try_operand_to_ull("0xFFFFFFFFFFFFFFFF", HexBase),
        Some(0xFFFF_FFFF_FFFF_FFFF)
    );
    assert_eq!(
        try_operand_to_ull("0XFFFFFFFFFFFFFFFF", HexBase),
        Some(0xFFFF_FFFF_FFFF_FFFF)
    );
    assert_eq!(
        try_operand_to_ull("0X0FFFFFFFFFFFFFFFF", HexBase),
        Some(0xFFFF_FFFF_FFFF_FFFF)
    );

    // Dec
    assert_eq!(try_operand_to_ull("1234", DecBase), Some(1234));
    assert_eq!(
        try_operand_to_ull("18446744073709551615", DecBase),
        Some(0xFFFF_FFFF_FFFF_FFFF)
    );
    assert_eq!(
        try_operand_to_ull("018446744073709551615", DecBase),
        Some(0xFFFF_FFFF_FFFF_FFFF)
    );

    // Oct
    assert_eq!(try_operand_to_ull("777", OctBase), Some(511)); // 0777 octal
    assert_eq!(try_operand_to_ull("0777", OctBase), Some(511));
    assert_eq!(
        try_operand_to_ull("1777777777777777777777", OctBase),
        Some(0xFFFF_FFFF_FFFF_FFFF)
    );
    assert_eq!(
        try_operand_to_ull("01777777777777777777777", OctBase),
        Some(0xFFFF_FFFF_FFFF_FFFF)
    );

    // Bin
    assert_eq!(try_operand_to_ull("1111", BinBase), Some(0b1111));
    assert_eq!(try_operand_to_ull("0010", BinBase), Some(0b10));
    assert_eq!(
        try_operand_to_ull(
            "1111111111111111111111111111111111111111111111111111111111111111",
            BinBase
        ),
        Some(0xFFFF_FFFF_FFFF_FFFF)
    );
    assert_eq!(
        try_operand_to_ull(
            "01111111111111111111111111111111111111111111111111111111111111111",
            BinBase
        ),
        Some(0xFFFF_FFFF_FFFF_FFFF)
    );

    // Invalid / overflow
    assert_eq!(try_operand_to_ull("0xFFFFFFFFFFFFFFFFF1", HexBase), None);
    assert_eq!(try_operand_to_ull("18446744073709551616", DecBase), None);
    assert_eq!(try_operand_to_ull("2000000000000000000000", OctBase), None);
    assert_eq!(
        try_operand_to_ull(
            "11111111111111111111111111111111111111111111111111111111111111111",
            BinBase
        ),
        None
    );
    assert_eq!(try_operand_to_ull("-1", DecBase), None);
    assert_eq!(try_operand_to_ull("5555", BinBase), None);
    assert_eq!(try_operand_to_ull("xyz", BinBase), None);
}

#[test]
fn validate_standard_scientific_operand_length() {
    assert_eq!(standard_scientific_operand_length(""), 0);
    assert_eq!(standard_scientific_operand_length("0.2"), 1);
    assert_eq!(standard_scientific_operand_length("1.2"), 2);
    assert_eq!(standard_scientific_operand_length("0."), 0);
    assert_eq!(standard_scientific_operand_length("12345"), 5);
    assert_eq!(standard_scientific_operand_length("-12345"), 6);
}

#[test]
fn validate_programmer_operand_length() {
    use NumberBase::*;
    assert_eq!(programmer_operand_length("1001", BinBase), 4);
    assert_eq!(programmer_operand_length("1001b", BinBase), 4);
    assert_eq!(programmer_operand_length("1001B", BinBase), 4);
    assert_eq!(programmer_operand_length("0b1001", BinBase), 4);
    assert_eq!(programmer_operand_length("0B1001", BinBase), 4);
    assert_eq!(programmer_operand_length("0y1001", BinBase), 4);
    assert_eq!(programmer_operand_length("0Y1001", BinBase), 4);
    assert_eq!(programmer_operand_length("0b", BinBase), 1);

    assert_eq!(programmer_operand_length("123456", OctBase), 6);
    assert_eq!(programmer_operand_length("0t123456", OctBase), 6);
    assert_eq!(programmer_operand_length("0T123456", OctBase), 6);
    assert_eq!(programmer_operand_length("0o123456", OctBase), 6);
    assert_eq!(programmer_operand_length("0O123456", OctBase), 6);

    assert_eq!(programmer_operand_length("", DecBase), 0);
    assert_eq!(programmer_operand_length("-", DecBase), 0);
    assert_eq!(programmer_operand_length("12345", DecBase), 5);
    assert_eq!(programmer_operand_length("-12345", DecBase), 5);
    assert_eq!(programmer_operand_length("0n12345", DecBase), 5);
    assert_eq!(programmer_operand_length("0N12345", DecBase), 5);

    assert_eq!(programmer_operand_length("123ABC", HexBase), 6);
    assert_eq!(programmer_operand_length("0x123ABC", HexBase), 6);
    assert_eq!(programmer_operand_length("0X123ABC", HexBase), 6);
    assert_eq!(programmer_operand_length("123ABCh", HexBase), 6);
    assert_eq!(programmer_operand_length("123ABCH", HexBase), 6);
}
