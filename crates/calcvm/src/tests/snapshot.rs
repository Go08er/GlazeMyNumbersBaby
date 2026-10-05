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

// ---- Extension: restore cost and bounds
//
// A restore replays the saved commands with the engine's display updates
// held back (one update at the end instead of one per command, each of which
// copied the whole expression), re-enters at most 100 memory slots that pass
// paste validation, and rejects command lists, displays and operators that
// the mode cannot have produced.

fn observed(vm: &CalculatorViewModel) -> String {
    format!(
        "{:?} display {:?} expression {:?} error {} parens {} {:?} fe {} (enabled {}) \
         {:?} {:?} {:?}\nmemory {:?}\nhistory {:?}",
        vm.mode(),
        vm.display_value(),
        vm.expression(),
        vm.is_error(),
        vm.open_parens(),
        vm.angle_unit(),
        vm.is_fe(),
        vm.is_enabled(Button::FToE),
        vm.radix(),
        vm.word_size(),
        vm.shift_mode(),
        vm.memory(),
        vm.history(),
    )
}

fn press_all(vm: &mut CalculatorViewModel, buttons: &[Button]) {
    for b in buttons {
        vm.press(*b);
    }
}

/// What a user does, for the saved-state tests: a key, or something in the
/// History or Memory panel, a paste, or coming back to the calculator from
/// another page (GMNB sets the mode again, unchanged).
#[derive(Clone, Copy, Debug)]
enum Act {
    Key(Button),
    /// Select History item `i` (0 = the newest).
    Recall(usize),
    ClearHistory,
    /// Click memory slot `i` (0 = the newest).
    MemoryItem(usize),
    Paste(&'static str),
    Reactivate,
}

fn act_all(vm: &mut CalculatorViewModel, acts: &[Act]) {
    for a in acts {
        match *a {
            Act::Key(b) => vm.press(b),
            Act::Recall(i) => vm.history_recall(i),
            Act::ClearHistory => vm.history_clear(),
            Act::MemoryItem(i) => vm.memory_recall(i),
            Act::Paste(text) => {
                vm.paste(text);
            }
            Act::Reactivate => vm.set_mode(vm.mode()),
        }
    }
}

fn keys(buttons: &[Button]) -> Vec<Act> {
    buttons.iter().map(|&b| Act::Key(b)).collect()
}

fn snapshot_json(mode: i64, display_commands: Value, x: Value) -> String {
    json!({
        "m": mode,
        "s": { "m": { "h": null }, "p": { "d": "0", "e": false }, "e": null, "c": display_commands },
        "x": x,
    })
    .to_string()
}

fn operand(digits: &[i32]) -> Value {
    json!({ "$t": 2, "n": false, "d": false, "s": false, "c": digits })
}

/// Saves `script`'s state, then checks that a calculator restored from it
/// shows the same, saves the same, and continues the same way as the
/// original for each of `more`. The original finishes before the restored
/// one is made (calculators on one thread share the engine's display
/// cache), as in the app, which restores at startup.
fn assert_restores_and_continues(mode: CalcMode, script: &[Button], more: &[&[Button]]) {
    let more: Vec<Vec<Act>> = more.iter().map(|m| keys(m)).collect();
    assert_acts_restore_and_continue(mode, &keys(script), &more);
}

/// [`assert_restores_and_continues`] for any [`Act`]s.
fn assert_acts_restore_and_continue(mode: CalcMode, script: &[Act], more: &[Vec<Act>]) {
    let run = |continuation: &[Act]| {
        let mut original = new_vm();
        original.set_mode(mode);
        act_all(&mut original, script);
        let state = original.save_state();
        let before = observed(&original);
        act_all(&mut original, continuation);
        (state, before, observed(&original))
    };
    let (state, before, _) = run(&[]);
    let mut restored = new_vm();
    restored.restore_state(&state);
    assert_eq!(observed(&restored), before, "{mode:?} {script:?}");
    assert_eq!(restored.save_state(), state, "{mode:?} {script:?}");
    for continuation in more {
        let (_, _, after) = run(continuation);
        let mut restored = new_vm();
        restored.restore_state(&state);
        act_all(&mut restored, continuation);
        assert_eq!(
            observed(&restored),
            after,
            "{mode:?} {script:?} then {continuation:?}"
        );
    }
}

/// [`CONTINUATIONS`], then a memory slot clicked, a paste, a return from
/// another page, F-E (disabled right after a History selection) and a
/// History selection, each followed by "=".
fn continuations_with_panels() -> Vec<Vec<Act>> {
    use Act::*;
    use Button::*;
    let mut more: Vec<Vec<Act>> = CONTINUATIONS.iter().map(|c| keys(c)).collect();
    for a in [MemoryItem(0), Paste("12"), Reactivate, Key(FToE), Recall(0)] {
        more.push(vec![a, Key(Equals)]);
    }
    more
}

/// The continuations every saved state below is checked with: the next
/// digit (typed into an operand, or replacing a shown value), "=" once and
/// twice (repeating the last operation), "(" (which multiplies a typed
/// number), an operator, backspace, memory recall, a sign change.
const CONTINUATIONS: &[&[Button]] = {
    use Button::*;
    &[
        &[Seven, Equals],
        &[Equals],
        &[Equals, Equals],
        &[OpenParenthesis, Two, Equals],
        &[Add, Two, Equals],
        &[Multiply, Equals],
        &[Backspace, Seven, Equals],
        &[MemoryRecall, Equals],
        &[Negate, Equals],
    ]
};

/// R12-M-07: app-produced states restore as they were saved and continue as
/// the original would. Values the display commands don't hold (a recalled
/// value, a constant, a result, a typed number ended by F-E or MS) and the
/// operation "=" repeats are restored too.
///
/// These compare a restore with the calculation it was saved from, not with
/// upstream: upstream's restore doesn't continue them (see the snapshot
/// module docs).
#[test]
fn app_states_restore_as_they_were_saved() {
    use Button::*;
    let scripts: Vec<(CalcMode, Vec<Button>)> = vec![
        // A pending operator and an operand being typed; grouped, negative
        // and long fractional memory slots.
        (
            CalcMode::Standard,
            vec![
                One, Two, Three, Four, Five, Six, Seven, Memory, Five, Negate, Memory, One, Divide,
                Three, Equals, Memory, One, Two, Add, Three, Four, Multiply, Five,
            ],
        ),
        // An error.
        (
            CalcMode::Standard,
            vec![Seven, Memory, One, Divide, Zero, Equals],
        ),
        // A recalled value that the display commands do not describe.
        (
            CalcMode::Standard,
            vec![Nine, Memory, Clear, Two, Add, MemoryRecall],
        ),
        // ... and one equal to the operand before it.
        (
            CalcMode::Standard,
            vec![Two, Memory, Clear, Two, Add, MemoryRecall],
        ),
        // Evaluated: "=" repeats "+ 1".
        (CalcMode::Standard, vec![Two, Add, One, Equals]),
        (CalcMode::Standard, vec![Two, Add, One, Equals, Equals]),
        // Evaluated with an inexact operand: "1 ÷ 3" is exact as evaluated
        // again, "0.3333333333333333 × 3" only as saved.
        (CalcMode::Standard, vec![One, Divide, Three, Equals]),
        (
            CalcMode::Standard,
            vec![One, Divide, Three, Equals, Multiply, Three, Equals],
        ),
        // Typing, and a recalled value, after "=".
        (CalcMode::Standard, vec![Two, Add, One, Equals, Seven]),
        (
            CalcMode::Standard,
            vec![Nine, Memory, Clear, Two, Add, One, Equals, MemoryRecall],
        ),
        // A typed number ended by MS (the last command is still a digit),
        // equal to the operand before it.
        (CalcMode::Standard, vec![Five, Add, Five, Memory]),
        // A unary operation after "=".
        (CalcMode::Standard, vec![Nine, Add, Seven, Equals, Sqrt]),
        // Parentheses, powers, an exponent in memory, gradians.
        (
            CalcMode::Scientific,
            vec![
                Grads,
                One,
                Exp,
                Three,
                Zero,
                Zero,
                Memory,
                OpenParenthesis,
                Two,
                Add,
                Three,
                CloseParenthesis,
                XPowerY,
                Two,
                Add,
                Five,
                Sin,
                Multiply,
                OpenParenthesis,
                Seven,
            ],
        ),
        (
            CalcMode::Scientific,
            vec![FToE, Two, Multiply, Pi, Equals, Memory, Add],
        ),
        // Precedence: "=" repeats "+ 14".
        (
            CalcMode::Scientific,
            vec![Two, Add, Three, Multiply, Four, Equals],
        ),
        // A typed number ended by F-E: "(" multiplies it.
        (CalcMode::Scientific, vec![One, Two, FToE]),
        // A result after MS in hex: "=" repeats "+ 1" (the expression is gone).
        (
            CalcMode::Programmer,
            vec![HexButton, F, F, Add, One, Equals, Memory],
        ),
        (
            CalcMode::Programmer,
            vec![HexButton, F, F, Add, One, Equals],
        ),
        // A typed number ended by a radix switch.
        (CalcMode::Programmer, vec![One, Two, HexButton]),
        // A padded binary display and a pending shift in a byte.
        (
            CalcMode::Programmer,
            vec![Byte, BinButton, One, Zero, One, Memory, Lsh, One],
        ),
        (
            CalcMode::Programmer,
            vec![Seven, RshL, Two, Equals, Memory, Not, Xor, Five],
        ),
    ];
    for (mode, script) in scripts {
        assert_restores_and_continues(mode, &script, CONTINUATIONS);
    }
    // Constants. A shown value comes back with the digits it showed (like
    // memory), and π and e have more: π × π, 2 + e + e or 2 − e can then
    // differ in the last digit, so the continuations avoid revealing them.
    assert_restores_and_continues(
        CalcMode::Scientific,
        &[Pi],
        &[
            &[Seven, Equals],
            &[Equals],
            &[OpenParenthesis, Two, Equals],
            &[Add, Two, Equals],
            &[Backspace, Seven, Equals],
            &[MemoryRecall, Equals],
            &[Negate, Equals],
        ],
    );
    assert_restores_and_continues(
        CalcMode::Scientific,
        &[Two, Add, Euler],
        &[
            &[Seven, Equals],
            &[Equals],
            &[OpenParenthesis, Two, Equals],
            &[Backspace, Seven, Equals],
            &[MemoryRecall, Equals],
        ],
    );
}

/// R12-M-07's four cases (the review's table: saved state, keys after the
/// restore, the original's result).
#[test]
fn restored_sessions_continue_as_the_original() {
    use Button::*;
    let cases: [(CalcMode, &[Button], &[Button], &str); 4] = [
        (
            CalcMode::Standard,
            &[Nine, Memory, Clear, Two, Add, MemoryRecall],
            &[Seven, Equals],
            "9",
        ),
        (CalcMode::Standard, &[Two, Add, One, Equals], &[Equals], "4"),
        (
            CalcMode::Programmer,
            &[HexButton, F, F, Add, One, Equals, Memory],
            &[Equals],
            "101",
        ),
        (
            CalcMode::Programmer,
            &[HexButton, F, F, Add, One, Equals, Memory],
            &[Seven, Equals],
            "8",
        ),
    ];
    for (mode, saved, after, result) in cases {
        let mut original = new_vm();
        original.set_mode(mode);
        press_all(&mut original, saved);
        let state = original.save_state();
        press_all(&mut original, after);
        assert_eq!(original.display_value(), result, "{saved:?} then {after:?}");
        let expected = observed(&original);
        drop(original);

        let mut restored = new_vm();
        restored.restore_state(&state);
        press_all(&mut restored, after);
        assert_eq!(restored.display_value(), result, "{saved:?} then {after:?}");
        assert_eq!(observed(&restored), expected, "{saved:?} then {after:?}");
    }
}

/// R13-M-05's case: a History selection shows the item's expression and
/// result while the engine holds the item replayed without "=", its last
/// operand typed (`SelectHistoryItem`, `Recalculate(fromHistory: true)`).
/// Restored, the expression reads as it did and "=" evaluates the item
/// again (5), instead of adding the shown result to its first operand (7).
#[test]
fn a_restored_history_selection_continues_as_the_original() {
    use Button::*;
    for mode in [CalcMode::Standard, CalcMode::Scientific] {
        let mut original = new_vm();
        original.set_mode(mode);
        press_all(&mut original, &[Two, Add, Three, Equals]);
        original.history_recall(0);
        let state = original.save_state();
        let before = observed(&original);
        original.press(Equals);
        assert_eq!(original.display_value(), "5", "{mode:?}");
        let expected = observed(&original);
        drop(original);

        let mut restored = new_vm();
        restored.restore_state(&state);
        assert_eq!(restored.display_value(), "5", "{mode:?}");
        assert_eq!(restored.expression(), "2 + 3=", "{mode:?}");
        assert!(!restored.is_enabled(FToE), "{mode:?}");
        assert_eq!(observed(&restored), before, "{mode:?}");
        restored.press(Equals);
        assert_eq!(restored.display_value(), "5", "{mode:?}");
        assert_eq!(restored.expression(), "2 + 3=", "{mode:?}");
        assert_eq!(observed(&restored), expected, "{mode:?}");
    }
}

/// R13-M-05: states saved right after selecting a History item, the newest
/// or an older one, or after a key or panel action that leaves the display
/// the item's (backspace, MS, M+, clearing the History, an angle unit,
/// coming back from another page) or the expression line the item's (a
/// digit, MR, a memory slot, a paste), restore as they were saved and
/// continue as the original would. Memory holds 9, so MR differs from both
/// the item's result and the operand the engine holds.
#[test]
fn history_selections_restore_as_they_were_saved() {
    use Act::*;
    use Button::*;
    let calculations = keys(&[
        Nine, Memory, Two, Add, Three, Equals, Four, Multiply, Five, Equals,
    ]);
    let more = continuations_with_panels();
    for mode in [CalcMode::Standard, CalcMode::Scientific] {
        let mut thens: Vec<Vec<Act>> = vec![
            vec![],
            vec![Key(Seven)],
            vec![Key(Backspace)],
            vec![Key(Memory)],
            vec![Key(MemoryRecall)],
            vec![Key(MemoryAdd)],
            vec![MemoryItem(0)],
            vec![Key(Sqrt)],
            vec![ClearHistory],
            vec![Reactivate],
            vec![Paste("12")],
        ];
        if mode == CalcMode::Scientific {
            thens.push(vec![Key(Radians)]);
        }
        for item in [0, 1] {
            for then in &thens {
                let mut script = calculations.clone();
                script.push(Recall(item));
                script.extend_from_slice(then);
                assert_acts_restore_and_continue(mode, &script, &more);
            }
        }
    }
    // Precedence (the engine holds "2 + 3 ×" and 4), a function in the
    // item, and F-E (the engine's operand shows as "4.e+0").
    for script in [
        &[Two, Add, Three, Multiply, Four, Equals][..],
        &[Two, Add, Nine, Sqrt, Equals],
        &[FToE, Two, Add, Three, Multiply, Four, Equals],
    ] {
        let mut script = keys(script);
        script.push(Recall(0));
        assert_acts_restore_and_continue(CalcMode::Scientific, &script, &more);
    }
}

/// A snapshot from before "k" (or from upstream) has no record of what the
/// display shows: a value the display commands don't produce is shown as a
/// result, so the next digit replaces it, and an evaluated expression is
/// evaluated again, so "=" repeats its operation.
#[test]
fn older_snapshots_show_values_as_results() {
    use Button::*;
    let old = |vm: &CalculatorViewModel| {
        let mut state: Value = serde_json::from_str(&vm.save_state()).unwrap();
        state["x"].as_object_mut().unwrap().remove("k");
        state.to_string()
    };
    for (saved, after, result) in [
        (
            &[Nine, Memory, Clear, Two, Add, MemoryRecall][..],
            &[Seven, Equals][..],
            "9",
        ),
        (&[Two, Add, One, Equals], &[Equals], "4"),
        (&[Two, Add, One, Equals], &[Seven, Equals], "8"),
        // Without the record, "=" can't repeat what MS hid; 7 replaces.
        (&[Two, Add, One, Equals, Memory], &[Seven, Equals], "7"),
    ] {
        let mut vm = new_vm();
        press_all(&mut vm, saved);
        let state = old(&vm);
        drop(vm);
        let mut restored = new_vm();
        restored.restore_state(&state);
        press_all(&mut restored, after);
        assert_eq!(restored.display_value(), result, "{saved:?} then {after:?}");
    }
}

/// The repeated operation is validated like the other commands: an operator
/// and an operand, valid in the snapshot's mode.
#[test]
fn a_malformed_repeated_operation_is_rejected() {
    let with_repeat = |mode: i64, eq: Value| {
        snapshot_json(
            mode,
            json!([]),
            json!({ "k": { "dv": "result", "eq": eq } }),
        )
    };
    let shift = json!([{ "$t": 1, "c": 89 }, operand(&[131])]);
    for (mode, eq) in [
        (0, json!([operand(&[131]), { "$t": 1, "c": 93 }])),
        (0, json!([{ "$t": 1, "c": 93 }])),
        (
            0,
            json!([{ "$t": 1, "c": 93 }, operand(&[131]), operand(&[131])]),
        ),
        (0, json!([{ "$t": 0, "c": [110] }, operand(&[131])])),
        (0, shift.clone()),
    ] {
        let mut vm = new_vm();
        press_all(&mut vm, &[Button::Four]);
        vm.restore_state(&with_repeat(mode, eq.clone()));
        assert_eq!(vm.display_value(), "4", "{eq}");
    }
    // A shift repeats in Programmer mode.
    let mut vm = new_vm();
    let mut state: Value = serde_json::from_str(&with_repeat(2, shift)).unwrap();
    state["s"]["p"]["d"] = json!("5");
    vm.restore_state(&state.to_string());
    assert_eq!(vm.mode(), CalcMode::Programmer);
    vm.press(Button::Equals);
    assert_eq!(vm.display_value(), "10");

    let mut vm = new_vm();
    let mut state: Value = serde_json::from_str(&snapshot_json(0, json!([]), json!({}))).unwrap();
    state["x"]["k"] = json!({ "dv": "typed?" });
    press_all(&mut vm, &[Button::Four]);
    vm.restore_state(&state.to_string());
    assert_eq!(vm.display_value(), "4");
}

/// A long calculation built from pastes (40 × a 100-term sum, about 12,000
/// keys and 800 KB of state) restores as saved, without the per-command
/// display updates that made it take seconds.
#[test]
fn a_long_pasted_calculation_restores_as_saved() {
    let sum = vec!["1"; 100].join("+");
    for mode in [CalcMode::Scientific, CalcMode::Programmer] {
        let mut original = new_vm();
        original.set_mode(mode);
        for _ in 0..40 {
            assert!(original.paste(&sum));
        }
        original.press(Button::Add);
        let state = original.save_state();
        assert!(state.len() > 700_000);

        let start = std::time::Instant::now();
        let mut restored = new_vm();
        restored.restore_state(&state);
        let elapsed = start.elapsed();
        assert_eq!(observed(&restored), observed(&original), "{mode:?}");
        assert_eq!(restored.display_value(), "3,961");
        assert!(
            elapsed < std::time::Duration::from_secs(20),
            "{mode:?}: {elapsed:?}"
        );

        restored.press(Button::One);
        restored.press(Button::Equals);
        assert_eq!(restored.display_value(), "3,962");
    }
}

#[test]
fn hostile_memory_restores_quickly() {
    for mode in [0, 1, 2] {
        // One huge expression (upstream: never finishes), a thousand
        // slots, and slots no paste would accept.
        let mut memory = vec!["1+".repeat(50_000), "abc".into(), "1e+99999".into()];
        memory.extend((0..1000).map(|i| (i + 1).to_string()));
        let state = snapshot_json(mode, json!([]), json!({ "mem": memory }));

        let start = std::time::Instant::now();
        let mut vm = new_vm();
        vm.restore_state(&state);
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "mode {mode}: {elapsed:?}"
        );
        let restored = vm.memory();
        assert_eq!(restored.len(), 97, "mode {mode}: {restored:?}");
        assert_eq!(restored[0], "1");
        assert_eq!(restored[96], "97");
        assert_eq!(vm.display_value(), "0");
    }
}

#[test]
fn over_long_snapshots_are_rejected() {
    let mut vm = new_vm();
    vm.press(Button::Four);
    let before = vm.save_state();

    // 16,385 keys of display commands.
    let mut commands = vec![operand(&[131; 16_383])];
    commands.push(json!({ "$t": 1, "c": 93 }));
    vm.restore_state(&snapshot_json(1, Value::Array(commands), json!({})));
    assert_eq!(vm.save_state(), before);

    // An over-long display value.
    let mut s: Value = serde_json::from_str(&snapshot_json(0, json!([]), json!({}))).unwrap();
    s["s"]["p"]["d"] = json!("1".repeat(513));
    vm.restore_state(&s.to_string());
    assert_eq!(vm.save_state(), before);

    // At the limit, cheap keys restore in tens of milliseconds (the bounds in
    // these tests leave room for slow builders; the old replay took minutes).
    let mut commands = vec![];
    for _ in 0..(16_384 / 3) {
        commands.push(operand(&[131]));
        commands.push(json!({ "$t": 1, "c": 93 }));
    }
    let start = std::time::Instant::now();
    vm.restore_state(&snapshot_json(1, Value::Array(commands), json!({})));
    assert!(vm.expression().starts_with("1 + 1 + "));
    assert!(start.elapsed() < std::time::Duration::from_secs(20));
}

/// Upstream accepts the Programmer-only operators in every mode; outside
/// Programmer mode the integer guard is off and a right shift by 10^7 takes
/// minutes (by 2^31 − 1, forever).
#[test]
fn programmer_operators_are_rejected_outside_programmer_mode() {
    let shift = |op: i32| {
        json!([
            operand(&[131]),
            { "$t": 1, "c": op },
            operand(&[131, 130, 130, 130, 130, 130, 130, 130]),
            { "$t": 1, "c": 93 },
        ])
    };
    let mut vm = new_vm();
    vm.press(Button::Four);
    let before = vm.save_state();
    let start = std::time::Instant::now();
    for mode in [0, 1] {
        for op in [86, 87, 88, 89, 90, 501, 502, 505] {
            vm.restore_state(&snapshot_json(mode, shift(op), json!({})));
            assert_eq!(vm.save_state(), before, "mode {mode} binary {op}");
        }
        for op in [99, 100, 101, 416, 417] {
            let unary = json!([operand(&[131]), { "$t": 0, "c": [op] }]);
            vm.restore_state(&snapshot_json(mode, unary, json!({})));
            assert_eq!(vm.save_state(), before, "mode {mode} unary {op}");
        }
        // In a history item of either mode.
        let item = json!({
            "t": [{ "t": "1", "c": 0 }, { "t": " Rsh ", "c": 1 }, { "t": "1", "c": 2 }, { "t": "=", "c": -1 }],
            "c": shift(505), "e": "1 Rsh 1 =", "r": "0",
        });
        for key in ["hs", "hc"] {
            vm.restore_state(&snapshot_json(
                mode,
                json!([]),
                json!({ key: [item.clone()] }),
            ));
            assert_eq!(vm.save_state(), before, "mode {mode} {key}");
        }
    }
    // Log base y is a Scientific operator.
    let logy = json!([operand(&[136, 134]), { "$t": 1, "c": 500 }, operand(&[132])]);
    vm.restore_state(&snapshot_json(1, logy, json!({})));
    assert_eq!(vm.expression(), "64 log base ");

    // In Programmer mode the same shifts are valid and the word-size guard
    // answers at once.
    for op in [89, 90, 505] {
        vm.restore_state(&snapshot_json(2, shift(op), json!({})));
        assert_eq!(vm.mode(), CalcMode::Programmer);
        assert!(vm.is_error(), "binary {op}");
    }
    assert!(start.elapsed() < std::time::Duration::from_secs(20));
}

/// `count` copies of `unit`, as display commands.
fn chain(unit: &[Value], count: usize) -> Value {
    Value::Array(
        unit.iter()
            .cloned()
            .cycle()
            .take(unit.len() * count)
            .collect(),
    )
}

/// Arithmetic `f` does on this thread, in `ratpack::work_done` units.
fn work(f: impl FnOnce()) -> u64 {
    let start = calcmanager::work_done();
    f();
    calcmanager::work_done() - start
}

/// A crafted "√7 + √7 + …" at the key limit replayed for half a minute
/// (every √ costs milliseconds). The replay now stops at its budget: the
/// pending expression is dropped, the displayed value, memory and history
/// are kept, and the work done is bounded.
#[test]
fn a_costly_crafted_expression_is_dropped_within_the_budget() {
    use crate::snapshot::{MAX_RESTORED_KEYS, REPLAY_WORK};
    let sqrt7 = [
        operand(&[137]),
        json!({ "$t": 0, "c": [110] }),
        json!({ "$t": 1, "c": 93 }),
    ];
    let units = MAX_RESTORED_KEYS / 4;
    let item = json!({
        "t": [{ "t": "1", "c": 0 }, { "t": " + ", "c": 1 }, { "t": "1", "c": 2 }, { "t": "=", "c": -1 }],
        "c": [operand(&[131]), { "$t": 1, "c": 93 }, operand(&[131])], "e": "1 + 1 =", "r": "2",
    });
    let mut s: Value = serde_json::from_str(&snapshot_json(
        1,
        chain(&sqrt7, units),
        json!({ "mem": ["5"], "hc": [item] }),
    ))
    .unwrap();
    s["s"]["p"]["d"] = json!("42");

    let mut vm = new_vm();
    let done = work(|| vm.restore_state(&s.to_string()));
    // Each √ and + is at most a few million units.
    assert!(done < REPLAY_WORK + 20_000_000, "{done}");
    assert_eq!(vm.mode(), CalcMode::Scientific);
    assert_eq!(vm.display_value(), "42");
    assert_eq!(vm.expression(), "");
    assert_eq!(vm.memory(), ["5"]);
    assert_eq!(vm.history().len(), 1);
    vm.press(Button::Add);
    vm.press(Button::One);
    vm.press(Button::Equals);
    assert_eq!(vm.display_value(), "43");

    // A short one is restored whole, well within the budget.
    let mut s: Value =
        serde_json::from_str(&snapshot_json(1, chain(&sqrt7, 3), json!({}))).unwrap();
    s["s"]["p"]["d"] = json!("2.645751311064591");
    let mut vm = new_vm();
    let done = work(|| vm.restore_state(&s.to_string()));
    assert!(done < REPLAY_WORK / 10, "{done}");
    assert_eq!(vm.expression(), "√(7) + √(7) + √(7) + ");
}

/// A crafted history item: clicking it replays within the budget too, and
/// the calculation continues from its saved result.
#[test]
fn a_costly_crafted_history_item_is_replayed_within_the_budget() {
    use crate::snapshot::REPLAY_WORK;
    let fact = [
        operand(&[130, 84, 135]),
        json!({ "$t": 0, "c": [113] }),
        json!({ "$t": 1, "c": 93 }),
    ];
    let item = json!({
        "t": [{ "t": "x", "c": 0 }, { "t": "=", "c": -1 }],
        "c": chain(&fact, 1000), "e": "x =", "r": "77",
    });
    let mut vm = new_vm();
    vm.restore_state(&snapshot_json(1, json!([]), json!({ "hc": [item] })));
    assert_eq!(vm.history().len(), 1);
    // One 0.5! is about 47 million units.
    let done = work(|| vm.history_recall(0));
    assert!(done < REPLAY_WORK + 60_000_000, "{done}");
    assert_eq!(vm.display_value(), "77");
    vm.press(Button::Add);
    vm.press(Button::One);
    vm.press(Button::Equals);
    assert_eq!(vm.display_value(), "78");
}

/// Memory slots near 10^-9999 cost millions of units each: the oldest that
/// fit the memory budget are kept, the rest dropped, and the work bounded.
#[test]
fn costly_crafted_memory_is_restored_within_the_budget() {
    use crate::snapshot::{MEMORY_WORK, VALUE_WORK};
    let tiny = "9.9999999999999999999999999999999e-9999";
    let mut memory = vec![tiny; 100];
    memory[99] = "3"; // the oldest
    let mut vm = new_vm();
    let done = work(|| vm.restore_state(&snapshot_json(1, json!([]), json!({ "mem": memory }))));
    // The budget, one slot past it, and showing the kept slots (formatting
    // each costs about what entering it did).
    assert!(done < 2 * MEMORY_WORK + VALUE_WORK, "{done}");
    let kept = vm.memory();
    assert!((2..100).contains(&kept.len()), "{kept:?}");
    assert_eq!(kept.last().map(String::as_str), Some("3"));
    assert_eq!(vm.display_value(), "0");
}
