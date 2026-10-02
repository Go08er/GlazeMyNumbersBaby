// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Port of `Calculator.Tests/HistoryTests.cs`. Like upstream, commands go
//! straight to the manager (`SendCommandToCalcManager`); `ReloadHistory` and
//! `ClearCommand` are the history view model's.

use super::new_vm;
use crate::{CalcMode, CalculatorViewModel};

// CalcManager command IDs used by the interop boundary.
const COMMAND_NULL: i32 = 0;
const COMMAND_SIGN: i32 = 80;
const COMMAND_CLEAR: i32 = 81;
const COMMAND_CENTR: i32 = 82;
const COMMAND_BACK: i32 = 83;
const COMMAND_DIV: i32 = 91;
const COMMAND_MUL: i32 = 92;
const COMMAND_ADD: i32 = 93;
const COMMAND_SUB: i32 = 94;
const COMMAND_SIN: i32 = 102;
const COMMAND_SQRT: i32 = 110;
const COMMAND_SQR: i32 = 111;
const COMMAND_REC: i32 = 114;
const COMMAND_PERCENT: i32 = 118;
const COMMAND_EQU: i32 = 121;
const COMMAND0: i32 = 130;
const COMMAND1: i32 = 131;
const COMMAND2: i32 = 132;
const COMMAND3: i32 = 133;
const COMMAND4: i32 = 134;
const COMMAND5: i32 = 135;
const COMMAND6: i32 = 136;
const COMMAND7: i32 = 137;
const COMMAND8: i32 = 138;
const COMMAND9: i32 = 139;
const MODE_BASIC: i32 = 200;
const MODE_SCIENTIFIC: i32 = 201;
const MODE_PROGRAMMER: i32 = 209;
const COMMAND_DEG: i32 = 321;
const COMMAND_RAD: i32 = 322;
const COMMAND_GRAD: i32 = 323;

fn initialize() -> CalculatorViewModel {
    new_vm()
}

fn send(vm: &mut CalculatorViewModel, command: i32) {
    vm.vm.send_command_to_calc_manager(command);
}

fn items_count(vm: &CalculatorViewModel) -> usize {
    vm.vm.history_vm.items_count()
}

fn item(vm: &CalculatorViewModel, index: usize) -> (String, String) {
    let i = &vm.vm.history_vm.items()[index];
    (i.expression().to_string(), i.result().to_string())
}

fn select(vm: &mut CalculatorViewModel, index: usize) {
    let item = vm.vm.history_vm.items()[index].clone();
    vm.vm.select_history_item(&item);
}

fn clear_command(vm: &mut CalculatorViewModel) {
    let v = &mut vm.vm;
    v.history_vm.on_clear(&mut v.standard_calculator_manager);
}

fn reload_history(vm: &mut CalculatorViewModel, mode: CalcMode) {
    let v = &mut vm.vm;
    v.history_vm
        .reload_history(&v.standard_calculator_manager, mode);
}

fn history_standard_order_of_operations_helper(
    expected_result: &str,
    expected_expression: &str,
    test_commands: &[i32],
) {
    let mut vm = initialize();
    let initial_size = items_count(&vm);
    for &command in test_commands {
        if command == COMMAND_NULL {
            break;
        }
        send(&mut vm, command);
    }
    let size_after_commands_add = items_count(&vm);
    if expected_result.is_empty() {
        assert_eq!(initial_size, size_after_commands_add);
    } else {
        assert_eq!(initial_size + 1, size_after_commands_add);
        assert_eq!(
            item(&vm, 0),
            (expected_expression.to_string(), expected_result.to_string())
        );
    }
}

#[test]
fn test_history_item_clicked() {
    let mut vm = initialize();
    send(&mut vm, MODE_SCIENTIFIC);
    for c in [
        COMMAND1,
        COMMAND_ADD,
        COMMAND5,
        COMMAND_ADD,
        COMMAND3,
        COMMAND_EQU,
    ] {
        send(&mut vm, c);
    }
    let last = items_count(&vm) - 1;
    select(&mut vm, last);
    assert_eq!(vm.display_value(), "9");
    let tokens = vm.vm.expression_tokens();
    assert_eq!(tokens[0], "1");
    assert_eq!(tokens[1], " ");
    assert_eq!(tokens[2], "+");
    assert_eq!(tokens[3], " ");
    assert_eq!(tokens[4], "5");
    assert_eq!(tokens[5], " ");
    assert_eq!(tokens[6], "+");
    assert_eq!(tokens[7], " ");
}

#[test]
fn test_history_item_add_single_item() {
    let mut vm = initialize();
    let initial_size = items_count(&vm);
    for c in [COMMAND1, COMMAND_ADD, COMMAND8, COMMAND_EQU] {
        send(&mut vm, c);
    }
    assert_eq!(items_count(&vm), initial_size + 1);
    assert_eq!(item(&vm, 0), ("1   +   8 =".to_string(), "9".to_string()));
}

#[test]
fn test_history_item_add_max_items() {
    let mut vm = initialize();
    for c in [COMMAND1, COMMAND_ADD, COMMAND1, COMMAND_EQU] {
        send(&mut vm, c);
    }
    let max = vm
        .vm
        .history_vm
        .get_max_item_size(&vm.vm.standard_calculator_manager);
    for _ in 1..max {
        for c in [COMMAND1, COMMAND_ADD, COMMAND2, COMMAND_EQU] {
            send(&mut vm, c);
        }
    }
    assert_eq!(items_count(&vm), max);
    assert_eq!(
        item(&vm, items_count(&vm) - 1),
        ("1   +   1 =".to_string(), "2".to_string())
    );
    for c in [COMMAND1, COMMAND_ADD, COMMAND5, COMMAND_EQU] {
        send(&mut vm, c);
    }
    assert_eq!(
        item(&vm, items_count(&vm) - 1),
        ("1   +   2 =".to_string(), "3".to_string())
    );
}

#[test]
fn test_history_clear_command() {
    let mut vm = initialize();
    for c in [
        MODE_SCIENTIFIC,
        COMMAND1,
        COMMAND_ADD,
        COMMAND2,
        COMMAND_EQU,
        MODE_BASIC,
        COMMAND1,
        COMMAND_ADD,
        COMMAND2,
        COMMAND_EQU,
    ] {
        send(&mut vm, c);
    }
    clear_command(&mut vm);
    assert_eq!(items_count(&vm), 0);
}

#[test]
fn test_history_clear_command_with_empty_history() {
    let mut vm = initialize();
    assert_eq!(items_count(&vm), 0);
    send(&mut vm, MODE_SCIENTIFIC);
    clear_command(&mut vm);
    assert_eq!(items_count(&vm), 0);
}

#[test]
fn test_reload_history() {
    let mut vm = initialize();
    vm.vm.reset_calc_manager(false);
    let scientific_items = 5;
    send(&mut vm, MODE_SCIENTIFIC);
    for i in 0..scientific_items {
        for c in [COMMAND1, COMMAND_ADD, 130 + i, COMMAND_EQU] {
            send(&mut vm, c);
        }
    }

    send(&mut vm, MODE_BASIC);
    let standard_items = 2;
    for i in 0..standard_items {
        for c in [COMMAND1, COMMAND_ADD, 130 + i, COMMAND_EQU] {
            send(&mut vm, c);
        }
    }

    send(&mut vm, MODE_SCIENTIFIC);
    reload_history(&mut vm, CalcMode::Scientific);
    assert_eq!(items_count(&vm), scientific_items as usize);
    for i in 0..scientific_items {
        let expr = format!("1   +   {i} =");
        let result = (1 + i).to_string();
        assert_eq!(item(&vm, items_count(&vm) - 1 - i as usize), (expr, result));
    }

    reload_history(&mut vm, CalcMode::Standard);
    send(&mut vm, MODE_BASIC);
    assert_eq!(items_count(&vm), standard_items as usize);
    for i in 0..standard_items {
        let expr = format!("1   +   {i} =");
        let result = (1 + i).to_string();
        assert_eq!(item(&vm, items_count(&vm) - 1 - i as usize), (expr, result));
    }
}

#[test]
fn test_history_item_with_pretty_expressions() {
    let mut vm = initialize();
    for c in [MODE_SCIENTIFIC, COMMAND2, COMMAND_SQRT, COMMAND_EQU] {
        send(&mut vm, c);
    }
    let (expression, result) = item(&vm, items_count(&vm) - 1);
    // The stored expression is the readable form, not the raw command name.
    assert!(!expression.is_empty());
    assert!(!expression.contains("CommandSQRT"));
    assert_eq!(result, "1.4142135623730950488016887242097");
}

#[test]
fn test_history_item_with_pretty_expressions_across_angle_modes() {
    let mut vm = initialize();
    for c in [
        MODE_SCIENTIFIC,
        COMMAND_DEG,
        COMMAND1,
        COMMAND_SIN,
        COMMAND_ADD,
        COMMAND_RAD,
        COMMAND1,
        COMMAND_SIN,
        COMMAND_ADD,
        COMMAND_GRAD,
        COMMAND1,
        COMMAND_SIN,
        COMMAND_EQU,
    ] {
        send(&mut vm, c);
    }
    let s = |id| calcmanager::en_us_engine_string(id).unwrap();
    let expected = format!(
        "{}( 1 )   +   {}( 1 )   +   {}( 1 ) =",
        s("67"),
        s("73"),
        s("79")
    );
    assert_eq!(item(&vm, items_count(&vm) - 1).0, expected);
}

#[test]
fn test_history_item_load_and_continue_calculation() {
    let mut vm = initialize();
    for c in [
        MODE_BASIC,
        COMMAND1,
        COMMAND_ADD,
        COMMAND5,
        COMMAND_ADD,
        COMMAND3,
        COMMAND_EQU,
    ] {
        send(&mut vm, c);
    }

    let last = items_count(&vm) - 1;
    select(&mut vm, last);

    for c in [COMMAND_ADD, COMMAND5, COMMAND_EQU] {
        send(&mut vm, c);
    }
    assert_eq!(vm.display_value(), "11");
    let last = items_count(&vm) - 1;
    select(&mut vm, last);
    assert_eq!(vm.display_value(), "6");

    let second_last = items_count(&vm) - 2;
    select(&mut vm, second_last);
    assert_eq!(vm.display_value(), "9");
}

#[test]
fn test_display_value_automation_names() {
    // Display parts of the upstream test (the automation names are narrator text).
    let mut vm = initialize();
    for c in [COMMAND1, COMMAND_ADD, COMMAND8, COMMAND_EQU] {
        send(&mut vm, c);
    }
    assert_eq!(vm.display_value(), "9");

    for c in [
        MODE_SCIENTIFIC,
        COMMAND1,
        COMMAND_ADD,
        COMMAND5,
        COMMAND_EQU,
    ] {
        send(&mut vm, c);
    }
    assert_eq!(vm.display_value(), "6");

    for c in [
        MODE_PROGRAMMER,
        COMMAND1,
        COMMAND_ADD,
        COMMAND2,
        COMMAND_EQU,
    ] {
        send(&mut vm, c);
    }
    assert_eq!(vm.display_value(), "3");
}

#[test]
fn test_radix_automation_name() {
    let mut vm = initialize();
    send(&mut vm, MODE_PROGRAMMER);
    vm.set_mode(CalcMode::Programmer); // IsProgrammer = true
    for c in [COMMAND1, COMMAND_ADD, COMMAND7, COMMAND_EQU] {
        send(&mut vm, c);
    }
    assert_eq!(
        format!("HexaDecimal {}", vm.radix_value(crate::Radix::Hex)),
        "HexaDecimal 8"
    );
    assert_eq!(
        format!("Octal {}", vm.radix_value(crate::Radix::Oct)),
        "Octal 10"
    );
    assert_eq!(
        format!("Binary {}", vm.radix_value(crate::Radix::Bin)),
        "Binary 1000"
    );
}

#[test]
fn test_history_empty() {
    let mut vm = initialize();
    assert_eq!(items_count(&vm), 0);
    send(&mut vm, MODE_SCIENTIFIC);
    assert_eq!(items_count(&vm), 0);
}

#[test]
fn test_history_standard_order_of_operations() {
    let cases: &[(&str, &str, &[i32])] = &[
        ("", "", &[COMMAND1, COMMAND_REC, COMMAND_NULL]),
        ("", "", &[COMMAND4, COMMAND_SQRT, COMMAND_NULL]),
        (
            "",
            "",
            &[COMMAND1, COMMAND_ADD, COMMAND4, COMMAND_SQRT, COMMAND_NULL],
        ),
        (
            "3",
            "1   +   \u{221A}( 4 ) =",
            &[
                COMMAND1,
                COMMAND_ADD,
                COMMAND4,
                COMMAND_SQRT,
                COMMAND_SUB,
                COMMAND_NULL,
            ],
        ),
        (
            "",
            "",
            &[COMMAND2, COMMAND_MUL, COMMAND4, COMMAND_REC, COMMAND_NULL],
        ),
        (
            "",
            "",
            &[
                COMMAND5,
                COMMAND_DIV,
                COMMAND6,
                COMMAND_PERCENT,
                COMMAND_NULL,
            ],
        ),
        ("", "", &[COMMAND4, COMMAND_SQRT, COMMAND_SUB, COMMAND_NULL]),
        ("", "", &[COMMAND7, COMMAND_SQR, COMMAND_DIV, COMMAND_NULL]),
        ("", "", &[COMMAND8, COMMAND_SQR, COMMAND_SQRT, COMMAND_NULL]),
        (
            "12",
            "10   +   2 =",
            &[
                COMMAND1,
                COMMAND0,
                COMMAND_ADD,
                COMMAND2,
                COMMAND_SUB,
                COMMAND_NULL,
            ],
        ),
        (
            "12",
            "3   \u{00D7}   4 =",
            &[COMMAND3, COMMAND_MUL, COMMAND4, COMMAND_DIV, COMMAND_NULL],
        ),
        (
            "2",
            "6   \u{00F7}   3 =",
            &[
                COMMAND6,
                COMMAND_DIV,
                COMMAND3,
                COMMAND_SUB,
                COMMAND_ADD,
                COMMAND_NULL,
            ],
        ),
        (
            "3",
            "7   -   4 =",
            &[
                COMMAND7,
                COMMAND_SUB,
                COMMAND4,
                COMMAND_DIV,
                COMMAND_MUL,
                COMMAND_NULL,
            ],
        ),
        (
            "16",
            "8   \u{00D7}   2 =",
            &[
                COMMAND8,
                COMMAND_MUL,
                COMMAND2,
                COMMAND_ADD,
                COMMAND_SQRT,
                COMMAND_NULL,
            ],
        ),
        (
            "9",
            "9   +   0 =",
            &[
                COMMAND9,
                COMMAND_ADD,
                COMMAND0,
                COMMAND_MUL,
                COMMAND_SIGN,
                COMMAND_NULL,
            ],
        ),
        (
            "",
            "",
            &[
                COMMAND9,
                COMMAND_SIGN,
                COMMAND0,
                COMMAND_ADD,
                COMMAND_MUL,
                COMMAND_NULL,
            ],
        ),
        (
            "3",
            "1   +   2 =",
            &[COMMAND1, COMMAND_ADD, COMMAND2, COMMAND_EQU, COMMAND_NULL],
        ),
        (
            "40",
            "20   \u{00D7}   2 =",
            &[
                COMMAND2,
                COMMAND0,
                COMMAND_MUL,
                COMMAND0,
                COMMAND2,
                COMMAND_EQU,
                COMMAND_NULL,
            ],
        ),
        (
            "3",
            "1   +   2 =",
            &[
                COMMAND1,
                COMMAND_ADD,
                COMMAND2,
                COMMAND_ADD,
                COMMAND_BACK,
                COMMAND_NULL,
            ],
        ),
        (
            "3",
            "1   +   2 =",
            &[
                COMMAND1,
                COMMAND_ADD,
                COMMAND2,
                COMMAND_ADD,
                COMMAND_CLEAR,
                COMMAND_NULL,
            ],
        ),
        (
            "3",
            "1   +   2 =",
            &[
                COMMAND1,
                COMMAND_ADD,
                COMMAND2,
                COMMAND_ADD,
                COMMAND_CENTR,
                COMMAND_NULL,
            ],
        ),
        (
            "",
            "",
            &[COMMAND1, COMMAND_ADD, COMMAND2, COMMAND_CLEAR, COMMAND_NULL],
        ),
        (
            "",
            "",
            &[COMMAND1, COMMAND_ADD, COMMAND2, COMMAND_CENTR, COMMAND_NULL],
        ),
    ];
    for (i, (result, expression, commands)) in cases.iter().enumerate() {
        eprintln!("TestHistoryStandardOrderOfOperations_{}", i + 1);
        history_standard_order_of_operations_helper(result, expression, commands);
    }
}

#[test]
fn test_history_standard_order_of_operations_multiple() {
    let mut vm = initialize();
    let commands = [
        COMMAND1,
        COMMAND_MUL,
        COMMAND2,
        COMMAND_MUL,
        COMMAND3,
        COMMAND_MUL,
        COMMAND4,
        COMMAND_MUL,
        COMMAND5,
        COMMAND_MUL,
    ];
    let initial_size = items_count(&vm);
    for c in commands {
        send(&mut vm, c);
    }
    assert_eq!(items_count(&vm), initial_size + 4);
    assert_eq!(
        item(&vm, 0),
        ("24   \u{00D7}   5 =".to_string(), "120".to_string())
    );
    assert_eq!(
        item(&vm, 1),
        ("6   \u{00D7}   4 =".to_string(), "24".to_string())
    );
    assert_eq!(
        item(&vm, 2),
        ("2   \u{00D7}   3 =".to_string(), "6".to_string())
    );
    assert_eq!(
        item(&vm, 3),
        ("1   \u{00D7}   2 =".to_string(), "2".to_string())
    );
}

// ---- Contract-level history behaviour

#[test]
fn history_is_per_engine_and_newest_first() {
    let mut vm = new_vm();
    for b in [
        crate::Button::One,
        crate::Button::Add,
        crate::Button::Two,
        crate::Button::Equals,
    ] {
        vm.press(b);
    }
    for b in [
        crate::Button::Three,
        crate::Button::Add,
        crate::Button::Four,
        crate::Button::Equals,
    ] {
        vm.press(b);
    }
    let h = vm.history();
    assert_eq!(h.len(), 2);
    assert_eq!(h[0].expression, "3   +   4 =");
    assert_eq!(h[0].result, "7");
    assert_eq!(h[1].expression, "1   +   2 =");

    vm.set_mode(CalcMode::Scientific);
    assert!(vm.history().is_empty(), "Scientific has its own history");
    for b in [
        crate::Button::Five,
        crate::Button::Multiply,
        crate::Button::Five,
        crate::Button::Equals,
    ] {
        vm.press(b);
    }
    assert_eq!(vm.history().len(), 1);

    vm.set_mode(CalcMode::Programmer);
    assert!(vm.history().is_empty(), "Programmer has no history");
    vm.history_clear(); // disabled in Programmer
    vm.history_remove(0);

    vm.set_mode(CalcMode::Standard);
    assert_eq!(vm.history().len(), 2);
    vm.set_mode(CalcMode::Scientific);
    assert_eq!(vm.history()[0].result, "25");
}

#[test]
fn history_remove_removes_the_displayed_item_from_the_engine_too() {
    let mut vm = new_vm();
    for d in [crate::Button::One, crate::Button::Two, crate::Button::Three] {
        vm.press(d);
        vm.press(crate::Button::Add);
        vm.press(d);
        vm.press(crate::Button::Equals);
    }
    let results: Vec<_> = vm.history().into_iter().map(|h| h.result).collect();
    assert_eq!(results, ["6", "4", "2"]);

    vm.history_remove(0); // newest: 3 + 3
    let results: Vec<_> = vm.history().into_iter().map(|h| h.result).collect();
    assert_eq!(results, ["4", "2"]);

    // The engine's list lost the same item (visible after a reload).
    vm.set_mode(CalcMode::Scientific);
    vm.set_mode(CalcMode::Standard);
    let results: Vec<_> = vm.history().into_iter().map(|h| h.result).collect();
    assert_eq!(results, ["4", "2"]);

    vm.history_remove(5); // out of range: no-op
    assert_eq!(vm.history().len(), 2);
    vm.history_clear();
    assert!(vm.history().is_empty());
}

#[test]
fn history_recall_shows_item_and_continues() {
    let mut vm = new_vm();
    for b in [
        crate::Button::One,
        crate::Button::Add,
        crate::Button::Two,
        crate::Button::Equals,
    ] {
        vm.press(b);
    }
    vm.press(crate::Button::Clear);
    assert_eq!(vm.display_value(), "0");
    vm.take_events();

    vm.history_recall(0);
    assert_eq!(vm.display_value(), "3");
    assert_eq!(vm.expression(), "1 + 2=");
    assert_eq!(vm.take_events(), [crate::Event::Replace]);

    for b in [
        crate::Button::Multiply,
        crate::Button::Three,
        crate::Button::Equals,
    ] {
        vm.press(b);
    }
    assert_eq!(vm.display_value(), "9");
}
