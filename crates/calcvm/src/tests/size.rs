// Rust port: GMNB contributors.

//! Extension: the size of a saved state ("Size" in the snapshot module
//! docs). A History item keeps every command and token of its
//! calculation, so nine Scientific calculations of 40 pastes of a 100-term
//! sum used to save 13.7 MB, and the settings file they went into was
//! refused whole at the next start. A saved state now fits
//! [`MAX_STATE_BYTES`]: the oldest History items go first, then a
//! calculation too long to fit is saved as a new one from its value;
//! memory, the modes and the rest are kept, and restoring it, then saving
//! again, gives the same text.

use std::sync::OnceLock;

use serde_json::{Value, json};

use super::new_vm;
use crate::snapshot::{
    ApplicationSnapshot, CalcManagerHistoryItem, MAX_RESTORED_KEYS, MAX_STATE_BYTES,
    StandardCalculatorSnapshot, json_string_len,
};
use crate::{AngleUnit, Button, CalcMode, CalculatorViewModel, HistoryEntry};

/// What the user sees of a calculator, but its History.
fn shown(vm: &CalculatorViewModel) -> String {
    format!(
        "{:?} {:?} expression {:?} error {} parens {} {:?} fe {} {:?} {:?} {:?}\nmemory {:?}",
        vm.mode(),
        vm.display_value(),
        vm.expression(),
        vm.is_error(),
        vm.open_parens(),
        vm.angle_unit(),
        vm.is_fe(),
        vm.radix(),
        vm.word_size(),
        vm.shift_mode(),
        vm.memory(),
    )
}

/// Each mode's History (newest first).
fn histories(vm: &mut CalculatorViewModel) -> (Vec<HistoryEntry>, Vec<HistoryEntry>) {
    let mode = vm.mode();
    vm.set_mode(CalcMode::Standard);
    let standard = vm.history();
    vm.set_mode(CalcMode::Scientific);
    let scientific = vm.history();
    vm.set_mode(mode);
    (standard, scientific)
}

fn sum_of_ones() -> String {
    vec!["1"; 100].join("+")
}

/// The saved state after a Scientific calculation of 40 pastes of a
/// 100-term sum and "=", as the app makes it (pasting takes seconds, so it
/// is made once).
fn long_calculation() -> &'static Value {
    static STATE: OnceLock<Value> = OnceLock::new();
    STATE.get_or_init(|| {
        let mut vm = new_vm();
        vm.set_mode(CalcMode::Scientific);
        for _ in 0..40 {
            assert!(vm.paste(&sum_of_ones()));
        }
        vm.press(Button::Equals);
        assert_eq!(vm.display_value(), "3,961");
        serde_json::from_str(&vm.save_state()).unwrap()
    })
}

/// That calculation's History item (0.72 MB as saved).
fn long_item() -> Value {
    long_calculation()["s"]["m"]["h"][0].clone()
}

/// A History item "`n` + 1 = `n + 1`" as the engine records it.
fn small_item(n: u32) -> Value {
    let digits: Vec<i32> = n
        .to_string()
        .bytes()
        .map(|d| 130 + i32::from(d - b'0'))
        .collect();
    json!({
        "t": [{ "t": n.to_string(), "c": 0 }, { "t": " ", "c": -1 }, { "t": "+", "c": 1 },
              { "t": " ", "c": -1 }, { "t": "1", "c": 2 }, { "t": "=", "c": -1 }],
        "c": [{ "$t": 2, "n": false, "d": false, "s": false, "c": digits }, { "$t": 1, "c": 93 },
              { "$t": 2, "n": false, "d": false, "s": false, "c": [131] }],
        "e": format!("{n}   +   1 ="), "r": (n + 1).to_string(),
    })
}

/// A calculator that finished the long calculation, with `scientific` and
/// `standard` as its Histories (oldest first) and `memory`: the state the
/// app is in after making those calculations (History items are restored
/// as they were saved).
fn session(scientific: Vec<Value>, standard: Vec<Value>, memory: &[&str]) -> CalculatorViewModel {
    session_from(long_calculation().clone(), scientific, standard, memory)
}

/// [`session`] from `state`, the long calculation's saved state.
fn session_from(
    mut state: Value,
    scientific: Vec<Value>,
    standard: Vec<Value>,
    memory: &[&str],
) -> CalculatorViewModel {
    state["s"]["m"]["h"] = Value::Array(scientific);
    state["x"]["hs"] = Value::Array(standard);
    state["x"]["mem"] = json!(memory);
    let mut vm = new_vm();
    assert!(vm.restore_state_checked(&state.to_string()));
    vm
}

/// The state saved whole, as before it was budgeted (the active History
/// written once).
fn unbudgeted(vm: &CalculatorViewModel) -> usize {
    json_string_len(&vm.vm.snapshot().to_json().to_string())
}

/// Both Histories as the calculator holds them (Standard, Scientific;
/// oldest first, every token and command).
fn held(vm: &CalculatorViewModel) -> [Vec<CalcManagerHistoryItem>; 2] {
    let x = vm.vm.snapshot().extension.unwrap();
    [
        x.standard_history.unwrap_or_default(),
        x.scientific_history.unwrap_or_default(),
    ]
}

/// What an item adds to a saved state.
fn item_len(item: &CalcManagerHistoryItem) -> usize {
    let mut state = ApplicationSnapshot {
        standard_calculator: Some(StandardCalculatorSnapshot::default()),
        ..Default::default()
    };
    let empty = json_string_len(&state.to_json().to_string());
    state
        .standard_calculator
        .as_mut()
        .unwrap()
        .calc_manager
        .history_items = Some(vec![item.clone()]);
    json_string_len(&state.to_json().to_string()) - empty
}

/// Saves `vm`, restores the state into a new calculator and checks that it
/// fits, shows what was shown, keeps memory and the modes, keeps the newest
/// items of each History and no fewer than fit, saves the same again, and
/// continues the same way. Returns how many items of each History were
/// kept.
fn check_trimmed(mut vm: CalculatorViewModel, more: &[Button]) -> (usize, usize) {
    let saved = vm.save_state();
    let len = json_string_len(&saved);
    assert!(len <= MAX_STATE_BYTES, "{len}");
    let before = shown(&vm);
    let all = held(&vm);
    for b in more {
        vm.press(*b);
    }
    let continued = shown(&vm);
    drop(vm);

    let mut restored = new_vm();
    assert!(restored.restore_state_checked(&saved), "restored exactly");
    assert_eq!(shown(&restored), before);
    let kept = held(&restored);
    for (all, kept) in all.iter().zip(&kept) {
        assert!(all.ends_with(kept), "the newest are kept");
    }
    // The item dropped last didn't fit (a few bytes for the brackets).
    let next = all
        .iter()
        .zip(&kept)
        .filter(|(all, kept)| all.len() > kept.len())
        .map(|(all, kept)| item_len(&all[all.len() - kept.len() - 1]))
        .max();
    if let Some(next) = next {
        assert!(len + next + 8 > MAX_STATE_BYTES, "{len} + {next}");
    }
    assert!(restored.save_state() == saved, "saves the same again");
    for b in more {
        restored.press(*b);
    }
    assert_eq!(shown(&restored), continued);
    (kept[0].len(), kept[1].len())
}

/// Codex's case: nine Scientific calculations of 40 pastes of a 100-term
/// sum, saved in Scientific mode right after the last "=", which saved
/// 13.7 MB (the History written twice; 7.2 MB once). Three items fit
/// beside the expression line, which holds the last calculation too: the
/// newest are kept, with memory, the Standard History and the angle unit.
#[test]
fn nine_long_calculations_keep_their_newest_items() {
    let standard: Vec<Value> = (1..=5).map(small_item).collect();
    let mut state = long_calculation().clone();
    state["x"]["a"] = json!("rad");
    let vm = session_from(state, vec![long_item(); 9], standard, &["7", "1.5", "-3"]);
    assert_eq!(vm.angle_unit(), AngleUnit::Radians);
    assert!(unbudgeted(&vm) > 7_000_000);
    let kept = check_trimmed(vm, &[Button::Add, Button::One, Button::Equals]);
    assert_eq!(kept, (5, 3));

    // Without the expression line (an angle switch after "=" clears it),
    // four.
    let mut vm = session(vec![long_item(); 9], vec![], &["7"]);
    vm.set_angle_unit(AngleUnit::Gradians);
    assert_eq!(vm.expression(), "");
    assert_eq!(
        check_trimmed(vm, &[Button::Add, Button::One, Button::Equals]),
        (0, 4)
    );
}

/// Six in each mode (a Standard item this long only comes from a saved
/// state: Standard mode evaluates at each operator), 14 MB as saved
/// before. The larger History loses its oldest first, so the two end
/// within an item of each other, and on a tie the one not shown loses.
#[test]
fn long_histories_in_both_modes_are_trimmed_alike() {
    let vm = session(vec![long_item(); 6], vec![long_item(); 6], &["42"]);
    assert!(unbudgeted(&vm) > 9_000_000);
    let kept = check_trimmed(vm, &[Button::Multiply, Button::Two, Button::Equals]);
    assert_eq!(kept, (1, 2));

    // In Standard mode (which starts a new calculation: room for four).
    let mut vm = session(vec![long_item(); 6], vec![long_item(); 6], &["42"]);
    vm.set_mode(CalcMode::Standard);
    assert_eq!(check_trimmed(vm, &[Button::Two, Button::Add]), (2, 2));
    let mut vm = session(vec![long_item(); 6], vec![long_item(); 5], &["42"]);
    vm.set_mode(CalcMode::Standard);
    assert_eq!(check_trimmed(vm, &[Button::Two, Button::Add]), (2, 2));
}

/// A long History beside a short one: only the long one is trimmed, its
/// oldest items first, whichever mode is shown (in Programmer mode, the
/// History of the mode before it is "s"."m"."h").
#[test]
fn the_larger_history_is_trimmed_first() {
    let mut scientific = vec![];
    for i in 0..10 {
        scientific.push(long_item());
        scientific.push(small_item(100 + i));
    }
    let standard: Vec<Value> = (1..=20).map(small_item).collect();
    for mode in [
        CalcMode::Standard,
        CalcMode::Scientific,
        CalcMode::Programmer,
    ] {
        let mut vm = session(scientific.clone(), standard.clone(), &["5"]);
        if mode != vm.mode() {
            vm.set_mode(mode);
        }
        let (kept_standard, kept_scientific) = check_trimmed(vm, &[Button::One]);
        assert_eq!(kept_standard, 20, "{mode:?}");
        // Three long items with the expression line (in Scientific mode,
        // where the last calculation is still shown), four without, and
        // the small items between them.
        let long = if mode == CalcMode::Scientific { 3 } else { 4 };
        assert_eq!(kept_scientific, 2 * long + 1, "{mode:?}");
    }
}

/// A calculation longer than the restore replays (56 pastes) used to be
/// saved as it was, and the whole state, memory and History with it, was
/// refused at the next start. It is saved as a new calculation from the
/// value shown; once evaluated, its History item is left out too.
#[test]
fn a_calculation_too_long_to_replay_keeps_the_rest() {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    for b in [
        Button::Two,
        Button::Add,
        Button::Two,
        Button::Equals,
        Button::Memory,
    ] {
        vm.press(b);
    }
    for _ in 0..56 {
        assert!(vm.paste(&sum_of_ones()));
    }
    let saved = vm.save_state();
    assert!(json_string_len(&saved) < 1000, "{saved}");
    let shown_before = vm.display_value();
    let mut restored = new_vm();
    assert!(!restored.restore_state_checked(&saved), "a new calculation");
    assert_eq!(restored.display_value(), shown_before);
    assert_eq!(restored.expression(), "");
    assert_eq!(restored.memory(), ["4"]);
    assert_eq!(restored.history().len(), 1);
    drop(restored);

    vm.press(Button::Equals);
    assert_eq!(vm.history().len(), 2);
    let value = vm.display_value();
    let saved = vm.save_state();
    let mut restored = new_vm();
    assert!(!restored.restore_state_checked(&saved));
    assert_eq!(restored.display_value(), value);
    assert_eq!(restored.memory(), ["4"]);
    assert_eq!(restored.history()[0].expression, "2   +   2 =");
    assert_eq!(restored.history().len(), 1);
    restored.press(Button::Add);
    restored.press(Button::One);
    restored.press(Button::Equals);
    assert_eq!(restored.display_value(), "5,546");
}

/// A History item longer than the restore replays, in a state saved
/// before saves were budgeted, is left out alone; it used to make the
/// restore refuse the whole state.
#[test]
fn an_unreplayable_saved_item_is_left_out_alone() {
    let mut long = small_item(1);
    long["c"] = Value::Array(
        std::iter::repeat_n(
            [
                json!({ "$t": 2, "n": false, "d": false, "s": false, "c": [131] }),
                json!({ "$t": 1, "c": 93 }),
            ],
            MAX_RESTORED_KEYS / 3 + 1,
        )
        .flatten()
        .collect(),
    );
    long["t"] = json!([{ "t": "1", "c": 0 }, { "t": "=", "c": -1 }]);
    for key in ["hs", "hc"] {
        let state = json!({
            "m": 1,
            "s": { "m": { "h": null }, "p": { "d": "8", "e": false }, "e": null, "c": [] },
            "x": { key: [small_item(1), long.clone(), small_item(2)], "mem": ["9"] },
        });
        let mut vm = new_vm();
        vm.restore_state(&state.to_string());
        assert_eq!(vm.display_value(), "8");
        assert_eq!(vm.memory(), ["9"], "{key}");
        let (standard, scientific) = histories(&mut vm);
        let kept = if key == "hs" { standard } else { scientific };
        assert_eq!(kept.len(), 2, "{key}");
        assert_eq!(kept[0].result, "3");
    }
}

/// The History `"s"."m"."h"` holds is written there only, and `"hm"` says
/// whose it is: in Programmer mode, the mode's before it, which keeps it.
#[test]
fn the_current_history_is_written_once() {
    for (before, name, other) in [
        (CalcMode::Standard, "s", "hc"),
        (CalcMode::Scientific, "c", "hs"),
    ] {
        for mode in [before, CalcMode::Programmer] {
            let mut vm = new_vm();
            vm.set_mode(if before == CalcMode::Standard {
                CalcMode::Scientific
            } else {
                CalcMode::Standard
            });
            for b in [
                Button::Three,
                Button::Multiply,
                Button::Three,
                Button::Equals,
            ] {
                vm.press(b);
            }
            vm.set_mode(before);
            for b in [Button::Two, Button::Add, Button::Two, Button::Equals] {
                vm.press(b);
            }
            vm.set_mode(mode);
            let saved = vm.save_state();
            let v: Value = serde_json::from_str(&saved).unwrap();
            assert_eq!(v["x"]["hm"], name, "{mode:?}");
            assert_eq!(v["s"]["m"]["h"][0]["e"], "2   +   2 =", "{mode:?}");
            let own = if name == "s" { "hs" } else { "hc" };
            assert!(v["x"].get(own).is_none(), "{mode:?}");
            assert_eq!(v["x"][other][0]["e"], "3   ×   3 =", "{mode:?}");

            let (standard, scientific) = histories(&mut vm);
            drop(vm);
            let mut restored = new_vm();
            restored.restore_state(&saved);
            assert_eq!(restored.save_state(), saved, "{mode:?}");
            assert_eq!(
                histories(&mut restored),
                (standard.clone(), scientific.clone())
            );

            // The same state written as before "hm" (both lists in "x")
            // restores the same.
            let mut older = v.clone();
            older["x"][own] = older["s"]["m"]["h"].clone();
            older["x"].as_object_mut().unwrap().remove("hm");
            let mut restored = new_vm();
            restored.restore_state(&older.to_string());
            assert_eq!(restored.save_state(), saved, "{mode:?} older");

            // Written twice is invalid.
            let mut twice = v.clone();
            twice["x"][own] = json!([]);
            let mut restored = new_vm();
            restored.press(Button::Eight);
            let unchanged = restored.save_state();
            restored.restore_state(&twice.to_string());
            assert_eq!(restored.save_state(), unchanged);
        }
    }
}

/// [`json_string_len`] counts what `serde_json` writes.
#[test]
fn json_string_lengths_are_serde_jsons() {
    for text in [
        "",
        "plain",
        r#"{"t":"1","c":-1}"#,
        "back\\slash \"quoted\"",
        "\u{0}\u{1}\u{8}\t\n\u{b}\u{c}\r\u{1f} \u{7f}",
        "× ÷ − √ π ⁻¹ ₀ 1,234.5",
    ] {
        assert_eq!(
            json_string_len(text),
            serde_json::to_string(text).unwrap().len(),
            "{text:?}"
        );
    }
}
