// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CalculatorUnitTests/CalcInputTest.cpp`.

use calcmanager::{CalcInput, MAX_STRLEN};

fn setup() -> CalcInput {
    CalcInput::new('.')
}

#[test]
fn clear() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_toggle_sign(false, "999");
    m_calc_input.try_add_decimal_pt();
    m_calc_input.try_add_digit(2, 10, false, "999", 64, 32);
    m_calc_input.try_begin_exponent();
    m_calc_input.try_add_digit(3, 10, false, "999", 64, 32);

    assert_eq!(
        "-1.2e+3",
        m_calc_input.to_string(10),
        "Verify input is correct."
    );

    m_calc_input.clear();

    assert_eq!(
        "0",
        m_calc_input.to_string(10),
        "Verify input is 0 after clear."
    );
}

#[test]
fn try_toggle_sign_zero() {
    let mut m_calc_input = setup();
    assert!(
        m_calc_input.try_toggle_sign(false, "999"),
        "Verify toggling 0 succeeds."
    );
    assert_eq!(
        "0",
        m_calc_input.to_string(10),
        "Verify toggling 0 does not create -0."
    );
}

#[test]
fn try_toggle_sign_exponent() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_begin_exponent();
    m_calc_input.try_add_digit(2, 10, false, "999", 64, 32);
    assert!(
        m_calc_input.try_toggle_sign(false, "999"),
        "Verify toggling exponent sign succeeds."
    );
    assert_eq!(
        "1.e-2",
        m_calc_input.to_string(10),
        "Verify toggling exponent sign does not toggle base sign."
    );
    assert!(
        m_calc_input.try_toggle_sign(false, "999"),
        "Verify toggling exponent sign succeeds."
    );
    assert_eq!(
        "1.e+2",
        m_calc_input.to_string(10),
        "Verify toggling negative exponent sign does not toggle base sign."
    );
}

#[test]
fn try_toggle_sign_base() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    assert!(
        m_calc_input.try_toggle_sign(false, "999"),
        "Verify toggling base sign succeeds."
    );
    assert_eq!(
        "-1",
        m_calc_input.to_string(10),
        "Verify toggling base sign creates negative base."
    );
    assert!(
        m_calc_input.try_toggle_sign(false, "999"),
        "Verify toggling base sign succeeds."
    );
    assert_eq!(
        "1",
        m_calc_input.to_string(10),
        "Verify toggling negative base sign creates positive base."
    );
}

#[test]
fn try_toggle_sign_base_integer_mode() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    assert!(
        m_calc_input.try_toggle_sign(true, "999"),
        "Verify toggling base sign in integer mode succeeds."
    );
    assert_eq!(
        "-1",
        m_calc_input.to_string(10),
        "Verify toggling base sign creates negative base."
    );
}

#[test]
fn try_toggle_sign_rollover() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_add_digit(2, 10, false, "999", 64, 32);
    assert!(
        m_calc_input.try_toggle_sign(true, "127"),
        "Verify toggling base sign in integer mode succeeds."
    );
    m_calc_input.try_add_digit(8, 10, false, "999", 64, 32);
    assert!(
        !m_calc_input.try_toggle_sign(true, "127"),
        "Verify toggling base sign in integer mode fails on rollover."
    );
    assert_eq!(
        "-128",
        m_calc_input.to_string(10),
        "Verify toggling base sign on rollover does not change value."
    );
}

#[test]
fn try_add_digit_leading_zeroes() {
    let mut m_calc_input = setup();
    assert!(
        m_calc_input.try_add_digit(0, 10, false, "999", 64, 32),
        "Verify TryAddDigit succeeds."
    );
    assert!(
        m_calc_input.try_add_digit(0, 10, false, "999", 64, 32),
        "Verify TryAddDigit succeeds."
    );
    assert!(
        m_calc_input.try_add_digit(0, 10, false, "999", 64, 32),
        "Verify TryAddDigit succeeds."
    );
    assert_eq!(
        "0",
        m_calc_input.to_string(10),
        "Verify leading zeros are ignored."
    );
}

#[test]
fn try_add_digit_max_count() {
    let mut m_calc_input = setup();
    assert!(
        m_calc_input.try_add_digit(1, 10, false, "999", 64, 32),
        "Verify TryAddDigit for base with length < maxDigits succeeds."
    );
    assert_eq!(
        "1",
        m_calc_input.to_string(10),
        "Verify adding digit for base with length < maxDigits succeeded."
    );
    assert!(
        !m_calc_input.try_add_digit(2, 10, false, "999", 64, 1),
        "Verify TryAddDigit for base with length > maxDigits fails."
    );
    assert_eq!(
        "1",
        m_calc_input.to_string(10),
        "Verify digit for base was not added."
    );
    m_calc_input.try_begin_exponent();
    assert!(
        m_calc_input.try_add_digit(1, 10, false, "999", 64, 32),
        "Verify TryAddDigit for exponent with length < maxDigits succeeds."
    );
    assert!(
        m_calc_input.try_add_digit(2, 10, false, "999", 64, 32),
        "Verify TryAddDigit for exponent with length < maxDigits succeeds."
    );
    assert!(
        m_calc_input.try_add_digit(3, 10, false, "999", 64, 32),
        "Verify TryAddDigit for exponent with length < maxDigits succeeds."
    );
    assert!(
        m_calc_input.try_add_digit(4, 10, false, "999", 64, 32),
        "Verify TryAddDigit for exponent with length < maxDigits succeeds."
    );
    assert!(
        !m_calc_input.try_add_digit(5, 10, false, "999", 64, 32),
        "Verify TryAddDigit for exponent with length > maxDigits fails."
    );
    assert_eq!(
        "1.e+1234",
        m_calc_input.to_string(10),
        "Verify adding digits for exponent with length < maxDigits succeeded."
    );

    m_calc_input.clear();
    m_calc_input.try_add_decimal_pt();
    assert!(
        m_calc_input.try_add_digit(1, 10, false, "999", 64, 1),
        "Verify decimal point and leading zero does not count toward maxDigits."
    );
    assert_eq!(
        "0.1",
        m_calc_input.to_string(10),
        "Verify input value checking dec pt and leading zero impact on maxDigits."
    );
}

#[test]
fn try_add_digit_values() {
    let mut m_calc_input = setup();
    // Use an arbitrary value > 16 to test that input accepts digits > hexadecimal 0xF.
    // TryAddDigit does not validate whether the digit fits within the current radix.
    for i in 0..25u32 {
        assert!(
            m_calc_input.try_add_digit(i, 10, false, "999", 64, 32),
            "Verify TryAddDigit succeeds for {i}"
        );
        m_calc_input.clear();
    }
}

#[test]
fn try_add_digit_rollover_base_check() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    assert!(
        !m_calc_input.try_add_digit(2, 16, true, "999", 64, 1),
        "Verify TryAddDigit rollover fails for bases other than 8,10."
    );
    assert!(
        !m_calc_input.try_add_digit(1, 2, true, "999", 64, 1),
        "Verify TryAddDigit rollover fails for bases other than 8,10."
    );
}

#[test]
fn try_add_digit_rollover_octal_byte() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 8, true, "777", 64, 32);
    assert!(
        m_calc_input.try_add_digit(2, 8, true, "377", 8, 1),
        "Verify we can add an extra digit in OctalByte if first digit <= 3."
    );

    m_calc_input.clear();
    m_calc_input.try_add_digit(4, 8, true, "777", 64, 32);
    assert!(
        !m_calc_input.try_add_digit(2, 8, true, "377", 8, 1),
        "Verify we cannot add an extra digit in OctalByte if first digit > 3."
    );
}

#[test]
fn try_add_digit_rollover_octal_word() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 8, true, "777", 64, 32);
    assert!(
        m_calc_input.try_add_digit(2, 8, true, "377", 16, 1),
        "Verify we can add an extra digit in OctalByte if first digit == 1."
    );

    m_calc_input.clear();
    m_calc_input.try_add_digit(2, 8, true, "777", 64, 32);
    assert!(
        !m_calc_input.try_add_digit(2, 8, true, "377", 16, 1),
        "Verify we cannot add an extra digit in OctalByte if first digit > 1."
    );
}

#[test]
fn try_add_digit_rollover_octal_dword() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 8, true, "777", 64, 32);
    assert!(
        m_calc_input.try_add_digit(2, 8, true, "377", 32, 1),
        "Verify we can add an extra digit in OctalByte if first digit <= 3."
    );

    m_calc_input.clear();
    m_calc_input.try_add_digit(4, 8, true, "777", 64, 32);
    assert!(
        !m_calc_input.try_add_digit(2, 8, true, "377", 32, 1),
        "Verify we cannot add an extra digit in OctalByte if first digit > 3."
    );
}

#[test]
fn try_add_digit_rollover_octal_qword() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 8, true, "777", 64, 32);
    assert!(
        m_calc_input.try_add_digit(2, 8, true, "377", 64, 1),
        "Verify we can add an extra digit in OctalByte if first digit == 1."
    );

    m_calc_input.clear();
    m_calc_input.try_add_digit(2, 8, true, "777", 64, 32);
    assert!(
        !m_calc_input.try_add_digit(2, 8, true, "377", 64, 1),
        "Verify we cannot add an extra digit in OctalByte if first digit > 1."
    );
}

#[test]
fn try_add_digit_rollover_decimal() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, true, "127", 64, 32);
    assert!(
        !m_calc_input.try_add_digit(0, 10, true, "1", 8, 1),
        "Verify we cannot add a digit if input size matches maxStr size."
    );
    m_calc_input.try_add_digit(2, 10, true, "127", 64, 32);
    assert!(
        !m_calc_input.try_add_digit(2, 10, true, "110", 8, 2),
        "Verify we cannot add a digit if n char comparison > 0."
    );
    assert!(
        m_calc_input.try_add_digit(7, 10, true, "130", 8, 2),
        "Verify we can add a digit if n char comparison < 0."
    );

    m_calc_input.clear();
    m_calc_input.try_add_digit(1, 10, true, "127", 64, 32);
    m_calc_input.try_add_digit(2, 10, true, "127", 64, 32);
    assert!(
        !m_calc_input.try_add_digit(8, 10, true, "127", 8, 2),
        "Verify we cannot add a digit if digit exceeds max value."
    );
    assert!(
        m_calc_input.try_add_digit(7, 10, true, "127", 8, 2),
        "Verify we can add a digit if digit does not exceed max value."
    );

    m_calc_input.backspace();
    m_calc_input.try_toggle_sign(true, "127");
    assert!(
        !m_calc_input.try_add_digit(9, 10, true, "127", 8, 2),
        "Negative value: verify we cannot add a digit if digit exceeds max value."
    );
    assert!(
        m_calc_input.try_add_digit(8, 10, true, "127", 8, 2),
        "Negative value: verify we can add a digit if digit does not exceed max value."
    );
}

#[test]
fn try_add_decimal_pt_empty() {
    let mut m_calc_input = setup();
    assert!(
        !m_calc_input.has_decimal_pt(),
        "Verify input has no decimal point."
    );
    assert!(
        m_calc_input.try_add_decimal_pt(),
        "Verify adding decimal to empty input."
    );
    assert!(
        m_calc_input.has_decimal_pt(),
        "Verify input has decimal point."
    );
    assert_eq!(
        "0.",
        m_calc_input.to_string(10),
        "Verify decimal on empty input."
    );
}

#[test]
fn try_add_decimal_point_twice() {
    let mut m_calc_input = setup();
    assert!(
        !m_calc_input.has_decimal_pt(),
        "Verify input has no decimal point."
    );
    assert!(
        m_calc_input.try_add_decimal_pt(),
        "Verify adding decimal to empty input."
    );
    assert!(
        m_calc_input.has_decimal_pt(),
        "Verify input has decimal point."
    );
    assert!(
        !m_calc_input.try_add_decimal_pt(),
        "Verify adding decimal point fails if input has decimal point."
    );
}

#[test]
fn try_add_decimal_point_exponent() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_begin_exponent();
    m_calc_input.try_add_digit(2, 10, false, "999", 64, 32);
    assert!(
        !m_calc_input.try_add_decimal_pt(),
        "Verify adding decimal point fails if input has exponent."
    );
}

#[test]
fn try_begin_exponent_no_exponent() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    assert!(
        m_calc_input.try_begin_exponent(),
        "Verify adding exponent succeeds on input without exponent."
    );
    assert_eq!(
        "1.e+0",
        m_calc_input.to_string(10),
        "Verify exponent present."
    );
}

#[test]
fn try_begin_exponent_with_exponent() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    assert!(
        m_calc_input.try_begin_exponent(),
        "Verify adding exponent succeeds on input without exponent."
    );
    assert!(
        !m_calc_input.try_begin_exponent(),
        "Verify cannot add exponent if input already has exponent."
    );
}

#[test]
fn backspace_zero() {
    let mut m_calc_input = setup();
    m_calc_input.backspace();
    assert_eq!(
        "0",
        m_calc_input.to_string(10),
        "Verify backspace on 0 is still 0."
    );
}

#[test]
fn backspace_single_char() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    assert_eq!(
        "1",
        m_calc_input.to_string(10),
        "Verify input before backspace."
    );
    m_calc_input.backspace();
    assert_eq!(
        "0",
        m_calc_input.to_string(10),
        "Verify input after backspace."
    );
}

#[test]
fn backspace_multi_char() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_add_digit(2, 10, false, "999", 64, 32);
    assert_eq!(
        "12",
        m_calc_input.to_string(10),
        "Verify input before backspace."
    );
    m_calc_input.backspace();
    assert_eq!(
        "1",
        m_calc_input.to_string(10),
        "Verify input after backspace."
    );
}

#[test]
fn backspace_decimal() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_add_decimal_pt();
    assert_eq!(
        "1.",
        m_calc_input.to_string(10),
        "Verify input before backspace."
    );
    assert!(
        m_calc_input.has_decimal_pt(),
        "Verify input has decimal point."
    );
    m_calc_input.backspace();
    assert_eq!(
        "1",
        m_calc_input.to_string(10),
        "Verify input after backspace."
    );
    assert!(
        !m_calc_input.has_decimal_pt(),
        "Verify decimal point was removed."
    );
}

#[test]
fn backspace_multi_char_decimal() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_add_decimal_pt();
    m_calc_input.try_add_digit(2, 10, false, "999", 64, 32);
    m_calc_input.try_add_digit(3, 10, false, "999", 64, 32);
    assert_eq!(
        "1.23",
        m_calc_input.to_string(10),
        "Verify input before backspace."
    );
    m_calc_input.backspace();
    assert_eq!(
        "1.2",
        m_calc_input.to_string(10),
        "Verify input after backspace."
    );
}

// Issue #817: Prefixed multiple zeros
#[test]
fn backspace_zero_decimal_without_prefix_zeros() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(0, 10, false, "999", 64, 32);
    m_calc_input.try_add_decimal_pt();
    assert_eq!(
        "0.",
        m_calc_input.to_string(10),
        "Verify input before backspace."
    );
    m_calc_input.backspace();
    m_calc_input.try_add_digit(0, 10, false, "999", 64, 32);
    assert_eq!(
        "0",
        m_calc_input.to_string(10),
        "Verify input after backspace."
    );
}

#[test]
fn set_decimal_symbol() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_decimal_pt();
    assert_eq!(
        "0.",
        m_calc_input.to_string(10),
        "Verify default decimal point."
    );
    m_calc_input.set_decimal_symbol(',');
    assert_eq!(
        "0,",
        m_calc_input.to_string(10),
        "Verify new decimal point."
    );
}

#[test]
fn to_string_empty() {
    let m_calc_input = setup();
    assert_eq!(
        "0",
        m_calc_input.to_string(10),
        "Verify ToString of empty value."
    );
}

#[test]
fn to_string_negative() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_toggle_sign(false, "999");
    assert_eq!(
        "-1",
        m_calc_input.to_string(10),
        "Verify ToString of negative value."
    );
}

#[test]
fn to_string_exponent_base10() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_begin_exponent();
    assert_eq!(
        "1.e+0",
        m_calc_input.to_string(10),
        "Verify ToString of empty base10 exponent."
    );
}

#[test]
fn to_string_exponent_base8() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_begin_exponent();
    assert_eq!(
        "1.^+0",
        m_calc_input.to_string(8),
        "Verify ToString of empty base8 exponent."
    );
}

#[test]
fn to_string_exponent_negative() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 8, false, "999", 64, 32);
    m_calc_input.try_begin_exponent();
    m_calc_input.try_toggle_sign(false, "999");
    assert_eq!(
        "1.e-0",
        m_calc_input.to_string(10),
        "Verify ToString of empty negative exponent."
    );
}

#[test]
fn to_string_exponent_positive() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_begin_exponent();
    m_calc_input.try_add_digit(2, 10, false, "999", 64, 32);
    m_calc_input.try_add_digit(3, 10, false, "999", 64, 32);
    m_calc_input.try_add_digit(4, 10, false, "999", 64, 32);
    assert_eq!(
        "1.e+234",
        m_calc_input.to_string(10),
        "Verify ToString of exponent with value."
    );
}

#[test]
fn to_string_integer() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    assert_eq!(
        "1",
        m_calc_input.to_string(10),
        "Verify ToString of integer value hides decimal."
    );
}

#[test]
fn to_string_base_too_long() {
    let mut m_calc_input = setup();
    let mut max_str = String::new();
    for _ in 0..MAX_STRLEN + 1 {
        max_str.push('1');
        m_calc_input.try_add_digit(1, 10, false, &max_str, 64, 100);
    }
    let result = m_calc_input.to_string(10);
    assert!(
        result.is_empty(),
        "Verify ToString of base value that is too large yields empty string."
    );
}

#[test]
fn to_string_exponent_too_long() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_begin_exponent();
    let mut max_str = String::from("11");
    let mut exponent_capped = false;
    for _ in 0..MAX_STRLEN + 1 {
        max_str.push('1');
        if !m_calc_input.try_add_digit(1, 10, false, &max_str, 64, (MAX_STRLEN + 25) as i32) {
            exponent_capped = true;
        }
    }
    let result = m_calc_input.to_string(10);

    // TryAddDigit caps the exponent length to C_EXP_MAX_DIGITS = 4, so ToString() succeeds.
    // If that cap is removed, ToString() should return an empty string.
    if exponent_capped {
        assert_eq!(
            "1.e+1111", result,
            "Verify ToString succeeds; exponent length is capped at C_EXP_MAX_DIGITS."
        );
    } else {
        assert!(
            result.is_empty(),
            "Verify ToString of exponent value that is too large yields empty string."
        );
    }
}

#[test]
fn to_rational() {
    let mut m_calc_input = setup();
    m_calc_input.try_add_digit(1, 10, false, "999", 64, 32);
    m_calc_input.try_add_digit(2, 10, false, "999", 64, 32);
    m_calc_input.try_add_digit(3, 10, false, "999", 64, 32);
    assert_eq!(
        "123",
        m_calc_input.to_string(10),
        "Verify input before conversion to rational."
    );

    // C++ passes `false` as the precision argument (i.e. 0).
    let rat = m_calc_input.to_rational(10, 0).expect("ToRational");
    assert_eq!(
        1,
        rat.p().mantissa().len(),
        "Verify digit count of rational."
    );
    assert_eq!(
        123,
        rat.p().mantissa()[0],
        "Verify first digit of mantissa."
    );
}
