// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Ports of `Calculator.Tests/SnapshotJsonTests.cs` and
//! `SnapshotRoundTripTests.cs`.
//!
//! Upstream throws on invalid snapshots and resets the calculator when a
//! restore fails half-way; `restore_state` validates everything before it
//! touches the calculator and ignores invalid input, so the "failed restore"
//! cases check that the calculator is left as it was. The cases about
//! non-calculator modes (Date) do not apply.

use serde_json::{Value, json};

use super::new_vm;
use crate::snapshot::{
    ApplicationSnapshot, CalcManagerHistoryItem, CalcManagerToken, ExpressionCommandDeserializer,
    ExpressionCommandSerializer, ExpressionCommandWrapper, ExpressionDisplaySnapshot,
    SnapshotValidator, StandardCalculatorSnapshot,
};
use crate::{AngleUnit, Button, CalcMode, CalculatorViewModel, Radix, WordSize};

const COMMAND_ADD: i32 = 93;

fn create_application_snapshot() -> ApplicationSnapshot {
    ApplicationSnapshot {
        mode: 0,
        standard_calculator: Some(StandardCalculatorSnapshot::default()),
        extension: None,
    }
}

/// `SnapshotLaunchArguments.FromJson(JsonSerializer.Serialize(alias))`:
/// serialize, parse back and run `ValidateProtocol`.
fn parse_snapshot(snapshot: &ApplicationSnapshot) -> Result<ApplicationSnapshot, String> {
    let json = snapshot.to_json().to_string();
    let parsed = ApplicationSnapshot::from_json(&json)?;
    SnapshotValidator::validate_protocol(&parsed)?;
    Ok(parsed)
}

fn round_trip(command: ExpressionCommandWrapper) -> ExpressionCommandWrapper {
    let json = ExpressionCommandSerializer::serialize(&command).to_string();
    ExpressionCommandDeserializer::deserialize(&serde_json::from_str(&json).unwrap()).unwrap()
}

// ---- SnapshotJsonTests

#[test]
fn invalid_mode_sets_launch_error() {
    for mode in [-1i64, i32::MAX as i64] {
        let mut s = create_application_snapshot();
        s.mode = mode;
        assert!(parse_snapshot(&s).is_err());
    }
}

#[test]
fn calculator_mode_without_state_sets_launch_error() {
    for mode in [0, 1, 2] {
        let s = ApplicationSnapshot {
            mode,
            standard_calculator: None,
            extension: None,
        };
        assert!(parse_snapshot(&s).is_err());
    }
}

#[test]
fn invalid_history_token_index_sets_launch_error() {
    for command_index in [-2, 1] {
        let mut s = create_application_snapshot();
        let item = CalcManagerHistoryItem {
            commands: vec![ExpressionCommandWrapper::Binary(COMMAND_ADD)],
            tokens: vec![CalcManagerToken {
                op_code_name: "+".into(),
                command_index,
            }],
            ..Default::default()
        };
        s.standard_calculator
            .as_mut()
            .unwrap()
            .calc_manager
            .history_items = Some(vec![item]);
        assert!(parse_snapshot(&s).is_err());
    }
}

#[test]
fn invalid_expression_token_index_sets_launch_error() {
    for command_index in [-2, 1] {
        let mut s = create_application_snapshot();
        s.standard_calculator.as_mut().unwrap().expression_display =
            Some(ExpressionDisplaySnapshot {
                commands: vec![ExpressionCommandWrapper::Binary(COMMAND_ADD)],
                tokens: vec![CalcManagerToken {
                    op_code_name: "+".into(),
                    command_index,
                }],
            });
        assert!(parse_snapshot(&s).is_err());
    }
}

#[test]
fn null_primary_display_value_sets_launch_error() {
    let json = json!({ "m": 0, "s": { "m": { "h": null }, "p": { "d": null, "e": false }, "e": null, "c": [] } });
    assert!(ApplicationSnapshot::from_json(&json.to_string()).is_err());
}

#[test]
fn invalid_display_command_sets_launch_error() {
    for command in [209 /* ModeProgrammer */, i32::MAX] {
        let mut s = create_application_snapshot();
        s.standard_calculator
            .as_mut()
            .unwrap()
            .display_commands
            .push(ExpressionCommandWrapper::Binary(command));
        assert!(parse_snapshot(&s).is_err());
    }
}

#[test]
fn valid_display_commands_are_accepted() {
    let mut s = create_application_snapshot();
    let c = &mut s.standard_calculator.as_mut().unwrap().display_commands;
    c.push(ExpressionCommandWrapper::Unary(vec![
        Button::Degree.id() as i32,
        102, /* SIN */
    ]));
    c.push(ExpressionCommandWrapper::Binary(COMMAND_ADD));
    c.push(ExpressionCommandWrapper::Operand {
        commands: vec![131, 84, 132],
        is_negative: false,
        is_decimal_present: true,
        is_sci_fmt: false,
    });
    c.push(ExpressionCommandWrapper::Parentheses(128));
    let parsed = parse_snapshot(&s).expect("valid snapshot");
    assert_eq!(parsed, s);
}

#[test]
fn unary_command_survives_the_round_trip() {
    assert_eq!(
        round_trip(ExpressionCommandWrapper::Unary(vec![91, 92])),
        ExpressionCommandWrapper::Unary(vec![91, 92])
    );
}

#[test]
fn binary_command_survives_the_round_trip() {
    assert_eq!(
        round_trip(ExpressionCommandWrapper::Binary(93)),
        ExpressionCommandWrapper::Binary(93)
    );
}

#[test]
fn parentheses_command_survives_the_round_trip() {
    assert_eq!(
        round_trip(ExpressionCommandWrapper::Parentheses(106)),
        ExpressionCommandWrapper::Parentheses(106)
    );
}

#[test]
fn operand_command_carries_its_flags_through_the_round_trip() {
    let c = ExpressionCommandWrapper::Operand {
        commands: vec![131, 132],
        is_negative: true,
        is_decimal_present: true,
        is_sci_fmt: true,
    };
    assert_eq!(round_trip(c.clone()), c);
    // ... and through the engine type.
    assert_eq!(ExpressionCommandWrapper::from_command(&c.to_command()), c);
}

#[test]
fn malformed_unary_command_is_rejected_during_deserialization() {
    assert!(ExpressionCommandDeserializer::deserialize(&json!({ "$t": 0, "c": [] })).is_err());
    assert!(ExpressionCommandDeserializer::deserialize(&json!({ "$t": 0 })).is_err());
    assert!(ExpressionCommandDeserializer::deserialize(&json!({ "$t": 7, "c": 1 })).is_err());
}

#[test]
fn json_uses_the_upstream_property_names() {
    let mut vm = new_vm();
    for b in [Button::One, Button::Add, Button::Two] {
        vm.press(b);
    }
    let v: Value = serde_json::from_str(&vm.save_state()).unwrap();
    assert_eq!(v["m"], json!(0));
    assert_eq!(v["s"]["p"], json!({ "d": "2", "e": false }));
    assert_eq!(v["s"]["m"]["h"], Value::Null);
    assert_eq!(v["s"]["e"]["t"][0], json!({ "t": "1", "c": 0 }));
    assert_eq!(
        v["s"]["e"]["c"][0],
        json!({ "$t": 2, "n": false, "d": false, "s": false, "c": [131] })
    );
    assert_eq!(v["s"]["e"]["c"][1], json!({ "$t": 1, "c": 93 }));
    assert_eq!(v["s"]["c"][2]["c"], json!([132]));
}

// ---- SnapshotRoundTripTests

fn evaluate(vm: &mut CalculatorViewModel, commands: &[i32]) {
    for &c in commands {
        vm.vm.send_command_to_calc_manager(c);
    }
    vm.vm.send_command_to_calc_manager(121);
}

#[test]
fn snapshot_restores_history() {
    let mut source = new_vm();
    evaluate(&mut source, &[131, 93, 132]);
    evaluate(&mut source, &[133, 92, 133]);
    let captured = source.vm.snapshot();
    let history = captured
        .standard_calculator
        .as_ref()
        .unwrap()
        .calc_manager
        .history_items
        .clone()
        .expect("history captured");
    assert_eq!(history.len(), 2);

    // Upstream format only (no gmnb extension).
    let mut upstream_only = captured.clone();
    upstream_only.extension = None;

    let mut restored = new_vm();
    restored.restore_state(&upstream_only.to_json().to_string());
    let recaptured = restored
        .vm
        .snapshot()
        .standard_calculator
        .unwrap()
        .calc_manager
        .history_items
        .expect("restored history");
    assert_eq!(
        history.iter().map(|h| &h.expression).collect::<Vec<_>>(),
        recaptured.iter().map(|h| &h.expression).collect::<Vec<_>>()
    );
    assert_eq!(
        history.iter().map(|h| &h.result).collect::<Vec<_>>(),
        recaptured.iter().map(|h| &h.result).collect::<Vec<_>>()
    );

    // Display history is newest-first while the snapshot is oldest-first.
    let shown: Vec<_> = restored.history().into_iter().map(|h| h.result).collect();
    let expected: Vec<_> = history.iter().rev().map(|h| h.result.clone()).collect();
    assert_eq!(shown, expected);
}

#[test]
fn captured_history_carries_its_tokens_and_commands() {
    let mut source = new_vm();
    evaluate(&mut source, &[131, 93, 132]);
    let snapshot = source.vm.snapshot();
    let items = snapshot
        .standard_calculator
        .unwrap()
        .calc_manager
        .history_items
        .unwrap();
    assert_eq!(items.len(), 1);
    assert!(!items[0].tokens.is_empty());
    assert!(!items[0].commands.is_empty());
}

// A recalled session must not inherit memory from the current one (when the
// snapshot carries no memory, as upstream's never do).
#[test]
fn restoring_a_snapshot_clears_memory() {
    let mut source = new_vm();
    evaluate(&mut source, &[131, 93, 132]);
    let mut captured = source.vm.snapshot();
    captured.extension = None;

    let mut restored = new_vm();
    restored.press(Button::Three);
    restored.press(Button::Memory);
    assert!(
        !restored.memory().is_empty(),
        "Memory was not set up, so the test proves nothing."
    );
    restored.restore_state(&captured.to_json().to_string());
    assert!(
        restored.memory().is_empty(),
        "Restoring a snapshot left the previous session's memory behind."
    );
}

#[test]
fn restoring_a_malformed_history_command_is_rejected() {
    let mut snapshot = new_vm().vm.snapshot();
    let item = CalcManagerHistoryItem {
        expression: "1 + 2 =".into(),
        result: "3".into(),
        commands: vec![ExpressionCommandWrapper::Unary(vec![])],
        ..Default::default()
    };
    snapshot
        .standard_calculator
        .as_mut()
        .unwrap()
        .calc_manager
        .history_items = Some(vec![item]);

    let mut target = new_vm();
    target.press(Button::Seven);
    target.press(Button::Memory);
    let before = target.save_state();
    target.restore_state(&snapshot.to_json().to_string());
    assert_eq!(
        target.save_state(),
        before,
        "an invalid snapshot must leave the calculator untouched"
    );
    assert_eq!(target.display_value(), "7");
}

#[test]
fn garbage_is_ignored() {
    let mut vm = new_vm();
    vm.press(Button::Four);
    for state in [
        "",
        "{",
        "null",
        "[]",
        "{\"m\": 9}",
        "{\"m\": 0}",
        "{\"m\": \"x\", \"s\": {}}",
    ] {
        vm.restore_state(state);
        assert_eq!(vm.display_value(), "4", "{state:?}");
    }
}

#[test]
fn successful_scientific_restore_keeps_the_scientific_engine() {
    let mut calculator = new_vm();
    calculator.set_mode(CalcMode::Scientific);
    calculator.set_angle_unit(AngleUnit::Radians);
    assert_eq!(calculator.angle_unit(), AngleUnit::Radians);

    let mut fresh = new_vm();
    fresh.set_mode(CalcMode::Scientific);
    calculator.restore_state(&fresh.save_state());
    assert_eq!(calculator.angle_unit(), AngleUnit::Degrees);
    assert_eq!(calculator.mode(), CalcMode::Scientific);

    // AssertScientificOrderOfOperations
    for b in [
        Button::One,
        Button::Add,
        Button::Two,
        Button::Multiply,
        Button::Three,
        Button::Equals,
    ] {
        calculator.press(b);
    }
    assert_eq!(calculator.display_value(), "7");
}

#[test]
fn successful_programmer_restore_keeps_the_programmer_engine() {
    let mut calculator = new_vm();
    calculator.set_mode(CalcMode::Programmer);
    calculator.set_radix(Radix::Hex);
    assert_eq!(calculator.radix(), Radix::Hex);

    let mut fresh = new_vm();
    fresh.set_mode(CalcMode::Programmer);
    calculator.restore_state(&fresh.save_state());
    assert_eq!(calculator.radix(), Radix::Dec);

    // AssertProgrammerIgnoresDecimalPoint
    for b in [Button::One, Button::Decimal, Button::Five] {
        calculator.press(b);
    }
    assert_eq!(calculator.display_value(), "15");
}

#[test]
fn errored_programmer_calculator_restores_bit_length() {
    // FailedProgrammerRestoreResetsBitLength, for a successful restore of a
    // fresh Programmer snapshot.
    let mut calculator = new_vm();
    calculator.set_mode(CalcMode::Programmer);
    calculator.set_word_size(WordSize::Byte);
    for b in [Button::One, Button::Divide, Button::Zero, Button::Equals] {
        calculator.press(b);
    }
    assert!(calculator.is_error());

    let mut fresh = new_vm();
    fresh.set_mode(CalcMode::Programmer);
    calculator.restore_state(&fresh.save_state());
    assert_eq!(calculator.word_size(), WordSize::Qword);
    for b in [
        Button::Two,
        Button::Five,
        Button::Five,
        Button::Add,
        Button::One,
        Button::Equals,
    ] {
        calculator.press(b);
    }
    assert_eq!(calculator.display_value(), "256");
}

#[test]
fn errored_scientific_engine_angle_mode_is_reset() {
    // FailedScientificRestoreResetsErroredEngineAngleMode, for a successful
    // restore of a fresh Scientific snapshot.
    let mut calculator = new_vm();
    calculator.set_mode(CalcMode::Scientific);
    calculator.set_angle_unit(AngleUnit::Radians);
    for b in [Button::One, Button::Divide, Button::Zero, Button::Equals] {
        calculator.press(b);
    }
    assert!(calculator.is_error());

    let mut fresh = new_vm();
    fresh.set_mode(CalcMode::Scientific);
    calculator.restore_state(&fresh.save_state());
    for b in [Button::Nine, Button::Zero, Button::Sin] {
        calculator.press(b);
    }
    assert_eq!(calculator.display_value(), "1");
}
