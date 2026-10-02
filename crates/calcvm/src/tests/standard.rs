// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Port of `Calculator.Tests/StandardCalculatorViewModelTests.cs`.
//!
//! Left out: the automation-name / narrator announcement assertions (no
//! narrator resources here) and `CalculatorButtonPressedEventArgs` parsing
//! (a XAML command-parameter helper). `DisplayValue = "1001"` (a test-only
//! setter upstream) is replaced by typing 1001.

use super::{
    TestItem, change_mode, new_vm, validate_value_and_expression, validate_view_model_by_commands,
};
use crate::{Button as N, CalcMode, Radix};

const DEC: &str = ".";

// ---- Constructor Tests

#[test]
fn view_model_constructor_display_value_and_expression_initialized_test() {
    let vm = new_vm();
    assert_eq!(vm.display_value(), "0");
    assert_eq!(vm.expression(), "");
}

// ---- Basic Arithmetic Tests

// Expression: 135
#[test]
fn button_pressed_left_hand_operand_entered_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Three, "13", ""),
        TestItem(N::Five, "135", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
}

// Expression: 13.
#[test]
fn button_pressed_left_hand_operand_and_decimal_entered_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Three, "13", ""),
        TestItem(N::Decimal, "13.", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
    assert_eq!(DEC, ".");
}

// Expression: 13==
#[test]
fn button_pressed_left_hand_operand_and_equals_entered_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Three, "13", ""),
        TestItem(N::Equals, "13", ""),
        TestItem(N::Equals, "13", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
}

// Expression: 13+
#[test]
fn button_pressed_left_hand_operand_and_operation_entered_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Three, "13", ""),
        TestItem(N::Add, "13", "13 + "),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
    validate_value_and_expression(&vm, Some("13"), Some("13 + "));
}

// Expression: 13+801
#[test]
fn button_pressed_right_hand_operand_entered_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Three, "13", ""),
        TestItem(N::Add, "13", "13 + "),
        TestItem(N::Eight, "8", "13 + "),
        TestItem(N::Zero, "80", "13 + "),
        TestItem(N::One, "801", "13 + "),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
    validate_value_and_expression(&vm, Some("801"), Some("13 + "));
}

// Expression: 1+2=
#[test]
fn button_pressed_addition_with_equals_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Add, "1", "1 + "),
        TestItem(N::Two, "2", "1 + "),
        TestItem(N::Equals, "3", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
    // Upstream renders the "=" token right after the last operand.
    validate_value_and_expression(&vm, Some("3"), Some("1 + 2="));
}

// Expression: 1-2=
#[test]
fn button_pressed_subtraction_with_equals_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Subtract, "1", "1 - "),
        TestItem(N::Two, "2", "1 - "),
        TestItem(N::Equals, "-1", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
}

// Expression: 3*5=
#[test]
fn button_pressed_multiply_with_equals_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::Three, "3", ""),
        TestItem(N::Multiply, "3", "3 * "),
        TestItem(N::Five, "5", "3 * "),
        TestItem(N::Equals, "15", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
    validate_value_and_expression(&vm, Some("15"), Some("3 × 5="));
}

// Expression: 9/3=
#[test]
fn button_pressed_divide_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::Nine, "9", ""),
        TestItem(N::Divide, "9", "9 / "),
        TestItem(N::Three, "3", "9 / "),
        TestItem(N::Equals, "3", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
}

// Expression: 7.555*3=
#[test]
fn button_pressed_decimal_operation_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::Seven, "7", ""),
        TestItem(N::Decimal, "7.", ""),
        TestItem(N::Five, "7.5", ""),
        TestItem(N::Five, "7.55", ""),
        TestItem(N::Five, "7.555", ""),
        TestItem(N::Multiply, "7.555", "7.555 * "),
        TestItem(N::Three, "3", "7.555 * "),
        TestItem(N::Equals, "22.665", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
}

// Expression: 7/0
#[test]
fn button_pressed_divide_by_zero_negative_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::Seven, "7", ""),
        TestItem(N::Divide, "7", "7 / "),
        TestItem(N::Zero, "0", "7 / "),
        TestItem(N::Equals, "Cannot divide by zero", "7 / "),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
    assert!(vm.is_error());
    validate_value_and_expression(&vm, Some("Cannot divide by zero"), Some("7 ÷ "));
}

// Expression: 8/2*
#[test]
fn button_pressed_expression_with_multiple_operators_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::Eight, "8", ""),
        TestItem(N::Divide, "8", "8 / "),
        TestItem(N::Two, "2", "8 / "),
        TestItem(N::Multiply, "4", "8 / 2 * "),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
    // Standard mode evaluates (and records history) at the second operator.
    validate_value_and_expression(&vm, Some("4"), Some("4 × "));
}

// Expression: 8/+*2*
#[test]
fn button_pressed_expression_with_multiple_operators_in_succession_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::Eight, "8", ""),
        TestItem(N::Divide, "8", "8 / "),
        TestItem(N::Add, "8", "8 + "),
        TestItem(N::Multiply, "8", "8 * "),
        TestItem(N::Two, "2", "8 * "),
        TestItem(N::Multiply, "16", "8 * 2 * "),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
}

// Expression: 8*2==
#[test]
fn button_pressed_expression_with_multiple_equals_after_evaluate_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::Eight, "8", ""),
        TestItem(N::Multiply, "8", "8 * "),
        TestItem(N::Two, "2", "8 * "),
        TestItem(N::Equals, "16", ""),
        TestItem(N::Equals, "32", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
}

// Expression: 7-6 and Backspace
#[test]
fn button_pressed_expression_with_back_space_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::Seven, "7", ""),
        TestItem(N::Subtract, "7", "7 - "),
        TestItem(N::Six, "6", "7 - "),
        TestItem(N::Backspace, "0", "7 - "),
        TestItem(N::Backspace, "0", "7 - "),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
}

// Expression: 91-68 and Clear
#[test]
fn button_pressed_expression_with_clear_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::Nine, "9", ""),
        TestItem(N::One, "91", ""),
        TestItem(N::Subtract, "91", "91 - "),
        TestItem(N::Six, "6", "91 - "),
        TestItem(N::Eight, "68", "91 - "),
        TestItem(N::Clear, "0", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
    validate_value_and_expression(&vm, Some("0"), Some(""));
}

// ---- Paste Tests

// Low-level test of character mapping (MapCharacterToButtonId lives in the
// copypaste crate, which `paste` uses).
#[test]
fn verify_correct_character_mapping() {
    use copypaste::{PasteCommand, PasteLocale, ViewMode, map_character_to_button_id};
    let map = |c| map_character_to_button_id(c, ViewMode::Standard, &PasteLocale::EN_US).button_id;
    assert_eq!(map('0'), Some(PasteCommand::Digit(0)));
    assert_eq!(map('1'), Some(PasteCommand::Digit(1)));
    assert_eq!(map('+'), Some(PasteCommand::Add));
    assert_eq!(map('='), Some(PasteCommand::Equals));
    assert_eq!(map('a'), Some(PasteCommand::Digit(0xA)));
    assert_eq!(map('$'), None);
}

/// `OnPaste(pastedString)` called directly, as the upstream test does
/// (it skips `CopyPasteManager` validation, which would reject the
/// parentheses in Standard mode).
fn on_paste(vm: &mut crate::CalculatorViewModel, text: &str) -> bool {
    let mode = match vm.mode() {
        CalcMode::Standard => copypaste::ViewMode::Standard,
        CalcMode::Scientific => copypaste::ViewMode::Scientific,
        CalcMode::Programmer => copypaste::ViewMode::Programmer,
    };
    vm.vm.on_paste(text, mode)
}

// Various strings get pasted
#[test]
fn paste_expressions() {
    let mut vm = new_vm();

    assert!(on_paste(&mut vm, "-0.99"));
    validate_value_and_expression(&vm, Some("-0.99"), None);

    assert!(on_paste(&mut vm, "1+1="));
    validate_value_and_expression(&vm, Some("2"), None);

    // This result is not obvious: it's the result of the previous operation
    assert!(on_paste(&mut vm, "0="));
    validate_value_and_expression(&vm, Some("1"), None);

    // Negative value
    assert!(on_paste(&mut vm, "-1"));
    validate_value_and_expression(&vm, Some("-1"), None);

    // Negated expression
    assert!(on_paste(&mut vm, "-(1+1)"));
    validate_value_and_expression(&vm, Some("-2"), Some("negate(1 + 1)"));

    // More complicated Negated expression
    assert!(on_paste(&mut vm, "-(-(-1))"));
    validate_value_and_expression(&vm, Some("-1"), Some("negate(0 - (0 - 1))"));

    // Switch to scientific mode
    vm.set_mode(CalcMode::Scientific);

    assert!(!vm.is_fe());

    // Positive exponent
    assert!(on_paste(&mut vm, "1.23e+10"));
    validate_value_and_expression(&vm, Some("1.23e+10"), None);

    assert!(on_paste(&mut vm, "1.23e10"));
    validate_value_and_expression(&vm, Some("1.23e+10"), None);

    assert!(on_paste(&mut vm, "135e10"));
    validate_value_and_expression(&vm, Some("135.e+10"), None);

    // Negative exponent
    assert!(on_paste(&mut vm, "1.23e-10"));
    validate_value_and_expression(&vm, Some("1.23e-10"), None);

    // Uppercase E (for exponent)
    assert!(on_paste(&mut vm, "1.23E-10"));
    validate_value_and_expression(&vm, Some("1.23e-10"), None);

    assert!(on_paste(&mut vm, "135E10"));
    validate_value_and_expression(&vm, Some("135.e+10"), None);
}

// ---- Automation Name Tests (display parts only)

#[test]
fn calculation_result_automation_name_verification() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::IsStandardMode, "0", ""),
        TestItem(N::One, "1", ""),
        TestItem(N::Two, "12", ""),
        TestItem(N::Three, "123", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);

    let items2 = [
        TestItem(N::IsScientificMode, "0", ""),
        TestItem(N::One, "1", ""),
        TestItem(N::Add, "1", "1 + "),
        TestItem(N::Two, "2", "1 + "),
        TestItem(N::Multiply, "2", "1 + 2 * "),
        TestItem(N::Three, "3", "1 + 2 * "),
        TestItem(N::Equals, "7", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items2, true);

    let items3 = [
        TestItem(N::Clear, "0", ""),
        TestItem(N::IsScientificMode, "0", ""),
        TestItem(N::Five, "5", ""),
        TestItem(N::InvSin, "Invalid input", "asind(5)"),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items3, false);
    assert!(vm.is_error());
}

// ---- Mode Switch Tests

#[test]
fn button_pressed_calculator_mode_switch() {
    let mut vm = new_vm();
    // Standard mode: 1+2*3 = 9 (left-to-right evaluation)
    let items = [
        TestItem(N::IsStandardMode, "0", ""),
        TestItem(N::One, "1", ""),
        TestItem(N::Add, "1", "1 + "),
        TestItem(N::Two, "2", "1 + "),
        TestItem(N::Multiply, "3", "1 + 2 * "),
        TestItem(N::Three, "3", "1 + 2 * "),
        TestItem(N::Equals, "9", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);

    // Scientific mode: 1+2*3 = 7 (operator precedence)
    let items2 = [
        TestItem(N::IsScientificMode, "0", ""),
        TestItem(N::One, "1", ""),
        TestItem(N::Add, "1", "1 + "),
        TestItem(N::Two, "2", "1 + "),
        TestItem(N::Multiply, "2", "1 + 2 * "),
        TestItem(N::Three, "3", "1 + 2 * "),
        TestItem(N::Equals, "7", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items2, true);
}

// ---- Programmer Mode Tests

fn binary_digits(vm: &crate::CalculatorViewModel) -> Vec<bool> {
    (0..64).map(|i| vm.bit(i)).collect()
}

#[test]
fn programmer_mode_auto_converted_value() {
    let mut vm = new_vm();
    validate_view_model_by_commands(&mut vm, &[TestItem(N::None, "", "")], true);
    vm.set_mode(CalcMode::Programmer);

    let items = [
        TestItem(N::HexButton, "0", ""),
        TestItem(N::F, "F", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, false);
    assert_eq!(vm.radix_value(Radix::Hex), "F");
    assert_eq!(vm.radix_value(Radix::Dec), "15");
    assert_eq!(vm.radix_value(Radix::Oct), "17");
    assert_eq!(vm.radix_value(Radix::Bin), "1111");

    let mut val = vec![false; 64];
    val[..4].fill(true);
    assert_eq!(binary_digits(&vm), val);
}

#[test]
fn programmer_mode_buttons_disable() {
    let mut vm = new_vm();
    change_mode(&mut vm, CalcMode::Programmer);

    // Hex accepts A-F; the other radices do not, and the keypad reflects that.
    vm.set_radix(Radix::Hex);
    assert!(
        vm.is_enabled(N::A),
        "Hex digits should be available in hex."
    );

    vm.set_radix(Radix::Dec);
    assert!(
        !vm.is_enabled(N::A),
        "Hex digits should be unavailable in decimal."
    );

    vm.set_radix(Radix::Oct);
    assert!(
        !vm.is_enabled(N::A),
        "Hex digits should be unavailable in octal."
    );

    vm.set_radix(Radix::Bin);
    assert!(
        !vm.is_enabled(N::A),
        "Hex digits should be unavailable in binary."
    );
}

#[test]
fn programmer_mode_radix_grouping() {
    let mut vm = new_vm();
    validate_view_model_by_commands(&mut vm, &[TestItem(N::None, "", "")], true);
    vm.set_mode(CalcMode::Programmer);

    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Two, "12", ""),
        TestItem(N::Three, "123", ""),
        TestItem(N::Four, "1,234", ""),
        TestItem(N::Five, "12,345", ""),
        TestItem(N::Six, "123,456", ""),
        TestItem(N::Seven, "1,234,567", ""),
        TestItem(N::Eight, "12,345,678", ""),
        TestItem(N::Nine, "123,456,789", ""),
        TestItem(N::None, "1", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
    assert_eq!(vm.radix_value(Radix::Hex), "75B CD15");
    assert_eq!(vm.radix_value(Radix::Dec), "123,456,789");
    assert_eq!(vm.radix_value(Radix::Oct), "726 746 425");
    assert_eq!(
        vm.radix_value(Radix::Bin),
        "0111 0101 1011 1100 1101 0001 0101"
    );

    let mut val = vec![false; 64];
    for i in [0, 2, 4, 8, 10, 11, 14, 15, 16, 17, 19, 20, 22, 24, 25, 26] {
        val[i] = true;
    }
    assert_eq!(binary_digits(&vm), val);
}

#[test]
fn programmer_mode_not() {
    let mut vm = new_vm();
    validate_view_model_by_commands(&mut vm, &[TestItem(N::None, "", "")], true);
    vm.set_mode(CalcMode::Programmer);

    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Not, "-2", "~(1)"),
        TestItem(N::None, "N/A", "N/A"),
    ];
    validate_view_model_by_commands(&mut vm, &items, false);
    assert_eq!(vm.radix_value(Radix::Hex), "FFFF FFFF FFFF FFFE");
    assert_eq!(vm.radix_value(Radix::Dec), "-2");
    assert_eq!(vm.radix_value(Radix::Oct), "1 777 777 777 777 777 777 776");
    assert_eq!(
        vm.radix_value(Radix::Bin),
        "1111 1111 1111 1111 1111 1111 1111 1111 1111 1111 1111 1111 1111 1111 1111 1110"
    );
    assert_eq!(vm.display_value(), "-2");

    let mut val = vec![true; 64];
    val[0] = false;
    assert_eq!(binary_digits(&vm), val);
}

#[test]
fn programmer_mode_and_or() {
    let mut vm = new_vm();
    validate_view_model_by_commands(&mut vm, &[TestItem(N::None, "", "")], true);
    vm.set_mode(CalcMode::Programmer);

    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Or, "1", "1 ||"),
        TestItem(N::Two, "2", "1 ||"),
        TestItem(N::Equals, "3", ""),
        TestItem(N::None, "3", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, false);

    let items2 = [
        TestItem(N::One, "1", ""),
        TestItem(N::And, "1", "1 &"),
        TestItem(N::Two, "2", "1 &"),
        TestItem(N::Equals, "0", ""),
        TestItem(N::None, "0", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items2, false);
}

#[test]
fn programmer_mode_clear() {
    let mut vm = new_vm();
    validate_view_model_by_commands(&mut vm, &[TestItem(N::None, "", "")], true);
    vm.set_mode(CalcMode::Programmer);

    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Or, "1", "1 ||"),
        TestItem(N::Two, "2", "1 ||"),
        TestItem(N::ClearEntry, "0", "1 ||"),
        TestItem(N::None, "", "1 ||"),
    ];
    validate_view_model_by_commands(&mut vm, &items, false);
    assert_eq!(vm.expression(), "1 OR ");

    let items2 = [
        TestItem(N::One, "1", ""),
        TestItem(N::And, "1", "1 &"),
        TestItem(N::Two, "2", "1 &"),
        TestItem(N::Clear, "0", ""),
        TestItem(N::None, "0", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items2, false);
    assert_eq!(vm.expression(), "");
}

// ---- Unary Operator Tests

#[test]
fn button_pressed_unary_operator_test() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::IsStandardMode, "0", ""),
        TestItem(N::Five, "5", ""),
        TestItem(N::Invert, "0.2", "reciproc(5)"),
        TestItem(N::Equals, "0.2", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);

    let items2 = [
        TestItem(N::One, "1", ""),
        TestItem(N::Six, "16", ""),
        TestItem(N::Sqrt, "4", "sqrt(16)"),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items2, false);
    assert_eq!(vm.expression(), "√(16)");

    let items3 = [
        TestItem(N::Six, "6", ""),
        TestItem(N::Negate, "-6", ""),
        TestItem(N::Nine, "-69", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items3, false);

    let items4 = [
        TestItem(N::Clear, "0", ""),
        TestItem(N::IsScientificMode, "0", ""),
        TestItem(N::Five, "5", ""),
        TestItem(N::InvSin, "Invalid input", "asind(5)"),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items4, false);

    let items5 = [
        TestItem(N::Clear, "0", ""),
        TestItem(N::Four, "4", ""),
        TestItem(N::Factorial, "24", "fact(4)"),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items5, false);
    assert_eq!(vm.expression(), "fact(4)");
}

// ---- Memory Tests

fn type_1001(vm: &mut crate::CalculatorViewModel) {
    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Zero, "10", ""),
        TestItem(N::Zero, "100", ""),
        TestItem(N::One, "1,001", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(vm, &items, true);
}

#[test]
fn is_memory_empty_test() {
    let mut vm = new_vm();
    assert_eq!(vm.memory().len(), 0);
    assert!(!vm.is_enabled(N::MemoryRecall));
    vm.press(N::Memory); // OnMemoryButtonPressed
    assert_eq!(vm.memory().len(), 1);
    assert!(vm.is_enabled(N::MemoryRecall));
    vm.press(N::MemoryClear); // ClearMemoryCommand
    assert_eq!(vm.memory().len(), 0);
    assert!(!vm.is_enabled(N::MemoryClear));
}

#[test]
fn is_operator_command_test() {
    let mut vm = new_vm();
    let check = |vm: &mut crate::CalculatorViewModel, b, expected| {
        vm.press(b);
        assert_eq!(vm.vm.is_operator_command(), expected, "{b:?}");
    };
    for b in [
        N::One,
        N::Two,
        N::Three,
        N::Four,
        N::Five,
        N::Six,
        N::Seven,
        N::Eight,
        N::Nine,
        N::Decimal,
        N::Zero,
    ] {
        check(&mut vm, b, false);
    }
    check(&mut vm, N::Multiply, true);
    check(&mut vm, N::Add, true);
    check(&mut vm, N::Zero, false);
}

#[test]
fn on_memory_button_pressed() {
    let mut vm = new_vm();
    type_1001(&mut vm);
    vm.press(N::Memory);
    vm.press(N::Memory);
    assert_eq!(vm.memory().len(), 2);
}

#[test]
fn on_memory_add_when_memory_empty() {
    let mut vm = new_vm();
    type_1001(&mut vm);
    vm.memory_add(0);
    vm.memory_recall(0);
    assert_eq!(vm.display_value(), "1,001");
}

#[test]
fn on_memory_subtract_when_memory_empty() {
    let mut vm = new_vm();
    type_1001(&mut vm);
    vm.memory_subtract(0);
    vm.memory_recall(0);
    assert_eq!(vm.display_value(), "-1,001");
}

#[test]
fn on_negative_entry_in_memory() {
    let mut vm = new_vm();
    change_mode(&mut vm, CalcMode::Standard);
    type_1001(&mut vm);
    vm.press(N::Negate);
    vm.press(N::Memory);
    vm.memory_recall(0);
    assert_eq!(vm.display_value(), "-1,001");
    assert_eq!(vm.memory()[0], "-1,001");

    change_mode(&mut vm, CalcMode::Scientific);
    assert_eq!(vm.memory()[0], "-1,001");

    change_mode(&mut vm, CalcMode::Programmer);
    assert_eq!(vm.memory()[0], "-1,001");
}

#[test]
fn on_decimal_entry_in_memory() {
    let mut vm = new_vm();
    change_mode(&mut vm, CalcMode::Standard);
    type_1001(&mut vm);
    vm.press(N::Decimal);
    assert_eq!(vm.display_value(), "1,001.");
    vm.press(N::One);
    assert_eq!(vm.display_value(), "1,001.1");
    vm.press(N::Memory);
    vm.memory_recall(0);
    assert_eq!(vm.display_value(), "1,001.1");
    assert_eq!(vm.memory()[0], "1,001.1");

    change_mode(&mut vm, CalcMode::Scientific);
    assert_eq!(vm.memory()[0], "1,001.1");

    change_mode(&mut vm, CalcMode::Programmer);
    assert_eq!(vm.memory()[0], "1,001");
}

#[test]
fn on_negative_decimal_in_memory() {
    let mut vm = new_vm();
    type_1001(&mut vm);
    vm.press(N::Decimal);
    vm.press(N::One);
    vm.press(N::Negate);
    vm.press(N::Memory);
    vm.memory_recall(0);
    assert_eq!(vm.display_value(), "-1,001.1");
}

#[test]
fn on_decimal_added_to_memory() {
    let mut vm = new_vm();
    type_1001(&mut vm);
    vm.press(N::Decimal);
    vm.press(N::One);
    vm.press(N::Memory);

    let items2 = [
        TestItem(N::One, "1", ""),
        TestItem(N::Zero, "10", ""),
        TestItem(N::Zero, "100", ""),
        TestItem(N::One, "1,001", ""),
        TestItem(N::Decimal, "1,001.", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items2, false);
    vm.press(N::Memory);
    vm.memory_add(1);
    vm.memory_recall(1);
    assert_eq!(vm.display_value(), "2,002.1");
}

#[test]
fn on_memory_saved_in_hex_radix_and_switched_to_standard_mode() {
    let mut vm = new_vm();
    change_mode(&mut vm, CalcMode::Programmer);
    let items = [
        TestItem(N::HexButton, "0", ""),
        TestItem(N::F, "F", ""),
        TestItem(N::F, "FF", ""),
        TestItem(N::None, "FF", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
    vm.press(N::Memory);
    change_mode(&mut vm, CalcMode::Scientific);
    vm.memory_recall(0);
    assert_eq!(vm.display_value(), "255");
    assert_eq!(vm.memory()[0], "255");
}

#[test]
fn on_memory_saved_in_hex_radix_and_radix_changes() {
    let mut vm = new_vm();
    change_mode(&mut vm, CalcMode::Programmer);
    let items = [
        TestItem(N::HexButton, "0", ""),
        TestItem(N::F, "F", ""),
        TestItem(N::F, "FF", ""),
        TestItem(N::None, "FF", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
    vm.press(N::Memory);

    vm.set_radix(Radix::Oct);
    assert_eq!(vm.memory()[0], "377");

    vm.set_radix(Radix::Dec);
    assert_eq!(vm.memory()[0], "255");

    vm.set_radix(Radix::Bin);
    assert_eq!(vm.memory()[0], "1111 1111");
}

#[test]
fn on_memory_button_pressed_max_times() {
    let mut vm = new_vm();
    type_1001(&mut vm);
    for _ in 0..110 {
        vm.press(N::Memory);
    }
    assert_eq!(vm.memory().len(), 100);
}

#[test]
fn on_memory_item_pressed() {
    let mut vm = new_vm();
    type_1001(&mut vm);
    vm.press(N::Memory);

    let items2 = [
        TestItem(N::Two, "2", ""),
        TestItem(N::Zero, "20", ""),
        TestItem(N::Zero, "200", ""),
        TestItem(N::Two, "2,002", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items2, false);
    vm.press(N::Memory);
    vm.memory_recall(1);

    assert_eq!(vm.display_value(), "1,001");
}

#[test]
fn on_memory_item_pressed_no_memory() {
    let mut vm = new_vm();
    let items = [
        TestItem(N::One, "1", ""),
        TestItem(N::Two, "12", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items, true);
    vm.memory_recall(0);
    assert_eq!(vm.display_value(), "12");
    assert_eq!(vm.memory().len(), 0);
}

#[test]
fn on_memory_item_add_and_subtract() {
    let mut vm = new_vm();
    type_1001(&mut vm);
    vm.press(N::Memory);

    let items2 = [
        TestItem(N::Two, "2", ""),
        TestItem(N::Zero, "20", ""),
        TestItem(N::Zero, "200", ""),
        TestItem(N::Two, "2,002", ""),
        TestItem(N::None, "", ""),
    ];
    validate_view_model_by_commands(&mut vm, &items2, false);
    vm.press(N::Memory);
    vm.memory_recall(1);
    vm.memory_add(0);

    assert_eq!(vm.memory()[0], "3,003");
}

// ---- Raw Formatting Tests (GetRawDisplayValue)

#[test]
fn verify_raw_formatting() {
    let mut vm = new_vm();
    type_1001(&mut vm);
    assert_eq!(vm.copy_text(), "1001");

    vm.press(N::Clear);
    for b in [N::Nine, N::Nine, N::Nine] {
        vm.press(b);
    }
    assert_eq!(vm.copy_text(), "999");

    assert!(vm.paste("1001001"));
    assert_eq!(vm.display_value(), "1,001,001");
    assert_eq!(vm.copy_text(), "1001001");

    // An error is copied verbatim.
    for b in [N::Clear, N::One, N::Divide, N::Zero, N::Equals] {
        vm.press(b);
    }
    assert_eq!(vm.copy_text(), "Cannot divide by zero");
}
