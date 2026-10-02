// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Tests for the gmnb contract on top of the port: programmer strings,
//! bit flips, shift operations, enablement, paste per mode, persistence and
//! event derivation.

use super::new_vm;
use crate::{
    AngleUnit, Button as B, CalcMode, CalculatorViewModel, Event, Radix, ShiftMode, WordSize,
};

fn press_all(vm: &mut CalculatorViewModel, buttons: &[B]) {
    for &b in buttons {
        vm.press(b);
    }
}

fn programmer() -> CalculatorViewModel {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Programmer);
    vm
}

fn radix_values(vm: &CalculatorViewModel) -> [String; 4] {
    [
        vm.radix_value(Radix::Hex),
        vm.radix_value(Radix::Dec),
        vm.radix_value(Radix::Oct),
        vm.radix_value(Radix::Bin),
    ]
}

// ---- Programmer radix strings

#[test]
fn minus_one_in_every_word_size_and_radix() {
    let mut vm = programmer();
    press_all(&mut vm, &[B::One, B::Negate]);
    assert_eq!(vm.display_value(), "-1");

    let cases = [
        (
            WordSize::Qword,
            [
                "FFFF FFFF FFFF FFFF",
                "-1",
                "1 777 777 777 777 777 777 777",
                "1111 1111 1111 1111 1111 1111 1111 1111 1111 1111 1111 1111 1111 1111 1111 1111",
            ],
        ),
        (
            WordSize::Dword,
            [
                "FFFF FFFF",
                "-1",
                "37 777 777 777",
                "1111 1111 1111 1111 1111 1111 1111 1111",
            ],
        ),
        (
            WordSize::Word,
            ["FFFF", "-1", "177 777", "1111 1111 1111 1111"],
        ),
        (WordSize::Byte, ["FF", "-1", "377", "1111 1111"]),
    ];
    for (word, expected) in cases {
        vm.set_word_size(word);
        assert_eq!(vm.word_size(), word);
        assert_eq!(radix_values(&vm), expected.map(String::from), "{word:?}");
        for i in 0..64 {
            assert_eq!(vm.bit(i), i < word.bits(), "{word:?} bit {i}");
        }
    }

    // The primary display follows the selected radix.
    vm.set_word_size(WordSize::Word);
    let displays = [
        (Radix::Hex, "FFFF"),
        (Radix::Oct, "177 777"),
        (Radix::Bin, "1111 1111 1111 1111"),
        (Radix::Dec, "-1"),
    ];
    for (radix, display) in displays {
        vm.set_radix(radix);
        assert_eq!(vm.radix(), radix);
        assert_eq!(vm.display_value(), display, "{radix:?}");
    }
}

#[test]
fn word_size_truncates_through_the_engine() {
    let mut vm = programmer();
    // 300 = 0x12C; as a BYTE that is 0x2C = 44.
    press_all(&mut vm, &[B::Three, B::Zero, B::Zero]);
    vm.set_word_size(WordSize::Byte);
    assert_eq!(
        radix_values(&vm),
        ["2C", "44", "54", "0010 1100"].map(String::from)
    );
    assert_eq!(vm.display_value(), "44");
    // Back to QWORD keeps the truncated value.
    vm.set_word_size(WordSize::Qword);
    assert_eq!(vm.display_value(), "44");
    // 255 in BYTE is -1.
    vm.press(B::Clear);
    vm.set_word_size(WordSize::Byte);
    vm.set_radix(Radix::Hex);
    press_all(&mut vm, &[B::F, B::F]);
    assert_eq!(vm.radix_value(Radix::Dec), "-1");
}

#[test]
fn binary_display_is_padded_to_nibbles() {
    let mut vm = programmer();
    vm.set_radix(Radix::Bin);
    press_all(&mut vm, &[B::One, B::Zero, B::One]);
    assert_eq!(vm.display_value(), "0101");
    press_all(&mut vm, &[B::One, B::One]);
    assert_eq!(vm.display_value(), "0001 0111");
    vm.press(B::Clear);
    assert_eq!(vm.display_value(), "0");
    // Errors are not padded.
    press_all(&mut vm, &[B::One, B::Divide, B::Zero, B::Equals]);
    assert!(vm.is_error());
    assert_eq!(vm.display_value(), "Cannot divide by zero");
    assert_eq!(radix_values(&vm), ["", "", "", ""].map(String::from));
}

// ---- Bit flips

#[test]
fn flip_bit_goes_through_the_engine() {
    let mut vm = programmer();
    vm.take_events();
    vm.flip_bit(0);
    assert_eq!(vm.display_value(), "1");
    assert!(vm.bit(0));
    assert_eq!(vm.take_events(), [Event::Typing]);
    vm.flip_bit(4);
    assert_eq!(vm.display_value(), "17");
    vm.flip_bit(0);
    assert_eq!(vm.display_value(), "16");
    assert!(!vm.bit(0));
    assert!(vm.bit(4));

    vm.flip_bit(63);
    assert_eq!(vm.display_value(), "-9,223,372,036,854,775,792");
    assert_eq!(vm.radix_value(Radix::Hex), "8000 0000 0000 0010");

    vm.press(B::Clear);
    vm.set_word_size(WordSize::Byte);
    vm.flip_bit(8); // outside the word: ignored
    assert_eq!(vm.display_value(), "0");
    vm.flip_bit(7);
    assert_eq!(vm.display_value(), "-128");
    assert_eq!(vm.radix_value(Radix::Hex), "80");

    // Not in other modes.
    vm.set_mode(CalcMode::Standard);
    vm.flip_bit(0);
    assert_eq!(vm.display_value(), "0");
}

#[test]
fn flip_bit_clears_an_error_first() {
    let mut vm = programmer();
    press_all(&mut vm, &[B::One, B::Divide, B::Zero, B::Equals]);
    assert!(vm.is_error());
    vm.flip_bit(1);
    assert!(!vm.is_error());
    assert_eq!(vm.display_value(), "2");
}

// ---- Shift operations

#[test]
fn shift_modes() {
    let mut vm = programmer();
    assert_eq!(vm.shift_mode(), ShiftMode::Arithmetic);
    for mode in [
        ShiftMode::Logical,
        ShiftMode::Rotate,
        ShiftMode::RotateThroughCarry,
        ShiftMode::Arithmetic,
    ] {
        vm.set_shift_mode(mode);
        assert_eq!(vm.shift_mode(), mode);
    }

    // Arithmetic: 1 Lsh 3 = 8, -8 Rsh 1 = -4.
    press_all(&mut vm, &[B::One, B::Lsh, B::Three, B::Equals]);
    assert_eq!(vm.display_value(), "8");
    press_all(&mut vm, &[B::Eight, B::Negate, B::Rsh, B::One, B::Equals]);
    assert_eq!(vm.display_value(), "-4");

    // Logical right shift in a BYTE: 0xF8 >> 1 = 0x7C.
    vm.set_word_size(WordSize::Byte);
    vm.set_radix(Radix::Hex);
    press_all(&mut vm, &[B::F, B::Eight, B::RshL, B::One, B::Equals]);
    assert_eq!(vm.display_value(), "7C");
    press_all(&mut vm, &[B::F, B::Eight, B::Rsh, B::One, B::Equals]);
    assert_eq!(vm.display_value(), "FC");

    // Rotate (unary): 0x81 RoL = 0x03, 0x01 RoR = 0x80.
    press_all(&mut vm, &[B::Eight, B::One, B::Rol]);
    assert_eq!(vm.display_value(), "3");
    vm.press(B::Clear);
    press_all(&mut vm, &[B::One, B::Ror]);
    assert_eq!(vm.display_value(), "80");

    // Rotate through carry: 0x81 RoL-C = 0x02 (carry set), again = 0x05.
    vm.press(B::Clear);
    press_all(&mut vm, &[B::Eight, B::One, B::RolC]);
    assert_eq!(vm.display_value(), "2");
    vm.press(B::RolC);
    assert_eq!(vm.display_value(), "5");
}

// ---- Enablement

#[test]
fn digits_per_radix_and_mode() {
    let mut vm = new_vm();
    assert!(vm.is_enabled(B::Nine));
    assert!(!vm.is_enabled(B::A));
    assert!(vm.is_enabled(B::Decimal));

    vm.set_mode(CalcMode::Programmer);
    assert!(!vm.is_enabled(B::Decimal), "no decimal point in Programmer");
    assert!(vm.is_enabled(B::Nine));
    assert!(!vm.is_enabled(B::A));
    vm.set_radix(Radix::Hex);
    for d in B::DIGITS {
        assert!(vm.is_enabled(d), "{d:?} in HEX");
    }
    vm.set_radix(Radix::Oct);
    assert!(vm.is_enabled(B::Seven));
    assert!(!vm.is_enabled(B::Eight));
    assert!(!vm.is_enabled(B::Nine));
    assert!(!vm.is_enabled(B::F));
    vm.set_radix(Radix::Bin);
    assert!(vm.is_enabled(B::Zero));
    assert!(vm.is_enabled(B::One));
    assert!(!vm.is_enabled(B::Two));

    vm.set_mode(CalcMode::Scientific);
    assert_eq!(vm.radix(), Radix::Dec);
    assert!(vm.is_enabled(B::Nine));
    assert!(!vm.is_enabled(B::A));
    assert!(vm.is_enabled(B::Decimal));
}

#[test]
fn error_state_disables_operators_but_not_operands() {
    for mode in [
        CalcMode::Standard,
        CalcMode::Scientific,
        CalcMode::Programmer,
    ] {
        let mut vm = new_vm();
        vm.set_mode(mode);
        press_all(
            &mut vm,
            &[B::Five, B::Memory, B::One, B::Divide, B::Zero, B::Equals],
        );
        assert!(vm.is_error(), "{mode:?}");
        assert_eq!(vm.display_value(), "Cannot divide by zero");

        for b in [
            B::Zero,
            B::One,
            B::Equals,
            B::Clear,
            B::ClearEntry,
            B::Backspace,
            B::DecButton,
            B::IsStandardMode,
        ] {
            assert!(vm.is_enabled(b), "{b:?} should stay enabled in {mode:?}");
        }
        for b in [
            B::Decimal,
            B::Add,
            B::Subtract,
            B::Multiply,
            B::Divide,
            B::Negate,
            B::Percent,
            B::Sqrt,
            B::Invert,
            B::Sin,
            B::Factorial,
            B::OpenParenthesis,
            B::Pi,
            B::And,
            B::Lsh,
            B::Memory,
            B::MemoryAdd,
            B::MemorySubtract,
            B::MemoryRecall,
            B::MemoryClear,
            B::FToE,
            B::Degree,
            B::Qword,
        ] {
            assert!(!vm.is_enabled(b), "{b:?} should be disabled in {mode:?}");
        }

        // A digit clears the error and is entered.
        vm.press(B::Seven);
        assert!(!vm.is_error());
        assert_eq!(vm.display_value(), "7");
        assert!(vm.is_enabled(B::Add));
        assert!(vm.is_enabled(B::MemoryRecall));
    }
}

#[test]
fn operator_after_error_only_clears() {
    let mut vm = new_vm();
    press_all(&mut vm, &[B::One, B::Divide, B::Zero, B::Equals]);
    vm.press(B::Add);
    assert!(!vm.is_error());
    assert_eq!(vm.display_value(), "0");
    assert_eq!(vm.expression(), "");
}

#[test]
fn state_buttons_are_ignored_while_disabled() {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    press_all(&mut vm, &[B::One, B::Divide, B::Zero, B::Equals]);
    vm.set_angle_unit(AngleUnit::Radians);
    vm.press(B::FToE);
    assert_eq!(vm.angle_unit(), AngleUnit::Degrees);
    assert!(!vm.is_fe());
    assert!(vm.is_error());

    vm.set_mode(CalcMode::Programmer);
    press_all(&mut vm, &[B::One, B::Divide, B::Zero, B::Equals]);
    vm.set_word_size(WordSize::Byte);
    assert_eq!(vm.word_size(), WordSize::Qword);
    // The radix buttons stay enabled and clear the error.
    vm.set_radix(Radix::Hex);
    assert!(!vm.is_error());
    assert_eq!(vm.display_value(), "0");
}

#[test]
fn memory_buttons_need_memory() {
    let mut vm = new_vm();
    assert!(!vm.is_enabled(B::MemoryRecall));
    assert!(!vm.is_enabled(B::MemoryClear));
    assert!(vm.is_enabled(B::Memory));
    assert!(vm.is_enabled(B::MemoryAdd));
    vm.press(B::MemoryAdd);
    assert_eq!(vm.memory(), ["0"]);
    assert!(vm.is_enabled(B::MemoryRecall));
}

// ---- C / CE and parentheses

#[test]
fn clear_entry_key_follows_input() {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    assert!(!vm.shows_clear_entry());
    vm.press(B::Five);
    assert!(vm.shows_clear_entry());
    vm.press(B::Add);
    assert!(vm.shows_clear_entry(), "the operand is still on display");
    vm.press(B::ClearEntry);
    assert!(!vm.shows_clear_entry());
    assert_eq!(vm.expression(), "5 + ");
    vm.press(B::Clear);
    assert!(!vm.shows_clear_entry());
    assert_eq!(vm.expression(), "");
}

#[test]
fn open_parenthesis_count() {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    press_all(&mut vm, &[B::OpenParenthesis, B::OpenParenthesis]);
    assert_eq!(vm.open_parens(), 2);
    press_all(&mut vm, &[B::One, B::CloseParenthesis]);
    assert_eq!(vm.open_parens(), 1);
    vm.press(B::Equals);
    assert_eq!(vm.open_parens(), 0);
    assert_eq!(vm.display_value(), "1");
}

// ---- Scientific: angle unit and F-E through engine commands

#[test]
fn angle_units_drive_the_engine() {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    assert_eq!(vm.angle_unit(), AngleUnit::Degrees);
    press_all(&mut vm, &[B::Nine, B::Zero, B::Sin]);
    assert_eq!(vm.display_value(), "1");
    assert_eq!(vm.expression(), "sin₀(90)");

    vm.press(B::Clear);
    vm.set_angle_unit(AngleUnit::Gradians);
    assert_eq!(vm.angle_unit(), AngleUnit::Gradians);
    press_all(&mut vm, &[B::One, B::Zero, B::Zero, B::Sin]);
    assert_eq!(vm.display_value(), "1");

    vm.press(B::Clear);
    vm.press(B::Radians);
    assert_eq!(vm.angle_unit(), AngleUnit::Radians);
    press_all(&mut vm, &[B::Zero, B::Cos]);
    assert_eq!(vm.display_value(), "1");
    assert_eq!(vm.expression(), "cosᵣ(0)");

    // The Scientific engine keeps the unit across mode switches.
    vm.set_mode(CalcMode::Standard);
    vm.set_mode(CalcMode::Scientific);
    assert_eq!(vm.angle_unit(), AngleUnit::Radians);
    press_all(&mut vm, &[B::Zero, B::Cos]);
    assert_eq!(vm.expression(), "cosᵣ(0)");
}

#[test]
fn fe_toggle_and_reset_by_clear() {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    press_all(&mut vm, &[B::One, B::Two, B::Three, B::Four, B::Five]);
    vm.press(B::FToE);
    assert!(vm.is_fe());
    assert_eq!(vm.display_value(), "1.2345e+4");
    vm.press(B::FToE);
    assert!(!vm.is_fe());
    assert_eq!(vm.display_value(), "12,345");

    // Clear unchecks F-E and puts the engine back to floating point.
    vm.press(B::FToE);
    vm.press(B::Clear);
    assert!(!vm.is_fe());
    press_all(&mut vm, &[B::Five, B::Zero, B::Zero, B::Equals]);
    assert_eq!(vm.display_value(), "500");

    // Leaving Scientific does the same.
    vm.press(B::FToE);
    assert_eq!(vm.display_value(), "5.e+2");
    vm.set_mode(CalcMode::Standard);
    vm.set_mode(CalcMode::Scientific);
    assert!(!vm.is_fe());
    press_all(&mut vm, &[B::Five, B::Zero, B::Zero, B::Equals]);
    assert_eq!(vm.display_value(), "500");
}

#[test]
fn recalled_history_does_not_desync_other_engines() {
    // Upstream's Recalculate resets every engine (Reset(false)); the view
    // model's angle unit and word size must still match the engines.
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    vm.set_angle_unit(AngleUnit::Radians);
    vm.set_mode(CalcMode::Programmer);
    vm.set_word_size(WordSize::Byte);
    vm.set_mode(CalcMode::Standard);
    press_all(&mut vm, &[B::One, B::Add, B::Two, B::Equals]);
    vm.history_recall(0);
    assert_eq!(vm.display_value(), "3");

    vm.set_mode(CalcMode::Scientific);
    assert_eq!(vm.angle_unit(), AngleUnit::Radians);
    press_all(&mut vm, &[B::Zero, B::Cos]);
    assert_eq!(vm.expression(), "cosᵣ(0)");

    vm.set_mode(CalcMode::Programmer);
    assert_eq!(vm.word_size(), WordSize::Byte);
    press_all(&mut vm, &[B::One, B::Negate]);
    assert_eq!(vm.radix_value(Radix::Hex), "FF");
}

// ---- Paste

#[test]
fn paste_valid_and_invalid_per_mode() {
    let mut vm = new_vm();
    assert!(vm.paste("1,234.5"));
    assert_eq!(vm.display_value(), "1,234.5");
    assert!(vm.paste("12+3="));
    assert_eq!(vm.display_value(), "15");
    assert!(vm.paste("-7"));
    assert_eq!(vm.display_value(), "-7");
    for bad in ["abc", "(1+2)", "1^2", "", "12#"] {
        assert!(!vm.paste(bad), "{bad:?} must be rejected in Standard");
        assert!(vm.is_error());
        assert_eq!(vm.display_value(), "Invalid input");
        assert_eq!(vm.copy_text(), "Invalid input");
    }
    // The next key clears the paste error.
    vm.press(B::Four);
    assert!(!vm.is_error());
    assert_eq!(vm.display_value(), "4");

    vm.set_mode(CalcMode::Scientific);
    assert!(vm.paste("(1+2)*3="));
    assert_eq!(vm.display_value(), "9");
    assert!(vm.paste("2^10="));
    assert_eq!(vm.display_value(), "1,024");
    assert!(vm.paste("1.5e3"));
    assert_eq!(vm.display_value(), "1.5e+3");
    assert!(!vm.paste("hello"));
    assert!(vm.is_error());

    vm.set_mode(CalcMode::Programmer);
    assert!(vm.paste("255"));
    assert_eq!(vm.display_value(), "255");
    assert!(!vm.paste("1.5"), "no decimals in Programmer");
    assert!(!vm.paste("FF"), "hex digits in DEC");
    vm.set_radix(Radix::Hex);
    assert!(vm.paste("0xFF"));
    assert_eq!(vm.display_value(), "FF");
    assert!(vm.paste("ff+1="));
    assert_eq!(vm.display_value(), "100");
    assert!(!vm.paste("FG"));
    vm.set_radix(Radix::Bin);
    assert!(vm.paste("1010"));
    assert_eq!(vm.display_value(), "1010");
    assert!(!vm.paste("102"));
    vm.set_radix(Radix::Dec);
    vm.set_word_size(WordSize::Byte);
    assert!(vm.paste("127"));
    assert!(!vm.paste("256"), "too large for a BYTE");
}

#[test]
fn copy_strips_group_separators() {
    let mut vm = new_vm();
    assert!(vm.paste("1234567.25"));
    assert_eq!(vm.display_value(), "1,234,567.25");
    assert_eq!(vm.copy_text(), "1234567.25");

    vm.set_mode(CalcMode::Programmer);
    vm.set_radix(Radix::Bin);
    press_all(&mut vm, &[B::One, B::One, B::One, B::One, B::One]);
    assert_eq!(vm.display_value(), "0001 1111");
    assert_eq!(vm.copy_text(), "00011111");
}

// ---- Persistence

fn assert_same_state(a: &CalculatorViewModel, b: &CalculatorViewModel) {
    assert_eq!(a.mode(), b.mode());
    assert_eq!(a.display_value(), b.display_value());
    assert_eq!(a.expression(), b.expression());
    assert_eq!(a.is_error(), b.is_error());
    assert_eq!(a.history(), b.history());
    assert_eq!(a.memory(), b.memory());
    assert_eq!(a.angle_unit(), b.angle_unit());
    assert_eq!(a.is_fe(), b.is_fe());
    assert_eq!(a.radix(), b.radix());
    assert_eq!(a.word_size(), b.word_size());
    assert_eq!(a.shift_mode(), b.shift_mode());
    for r in Radix::ALL {
        assert_eq!(a.radix_value(r), b.radix_value(r));
    }
}

fn round_trip(vm: &CalculatorViewModel) -> CalculatorViewModel {
    let mut restored = new_vm();
    restored.restore_state(&vm.save_state());
    assert_same_state(vm, &restored);
    restored
}

#[test]
fn round_trip_pending_expression_history_and_memory() {
    let mut vm = new_vm();
    press_all(&mut vm, &[B::One, B::Add, B::Two, B::Equals, B::Memory]);
    press_all(&mut vm, &[B::Three, B::Add, B::Four, B::Equals]);
    press_all(
        &mut vm,
        &[B::One, B::Decimal, B::Five, B::Negate, B::Memory],
    );
    press_all(&mut vm, &[B::Five, B::Multiply, B::Six]);
    assert_eq!(vm.memory(), ["-1.5", "3"]);
    assert_eq!(vm.expression(), "5 × ");

    let mut restored = round_trip(&vm);
    assert_eq!(restored.display_value(), "6");
    // The calculation continues where it was left.
    restored.press(B::Equals);
    assert_eq!(restored.display_value(), "30");
    restored.memory_recall(1);
    assert_eq!(restored.display_value(), "3");
    restored.memory_recall(0);
    assert_eq!(restored.display_value(), "-1.5");
    assert_eq!(restored.history().len(), 3);
}

#[test]
fn round_trip_after_equals_and_continue() {
    let mut vm = new_vm();
    press_all(&mut vm, &[B::Seven, B::Multiply, B::Six, B::Equals]);
    assert_eq!(vm.expression(), "7 × 6=");
    let mut restored = round_trip(&vm);
    assert_eq!(restored.display_value(), "42");
    // Like a recalled history item, the expression is loaded into the engine
    // ("7 × 6" pending), so "=" evaluates it.
    restored.press(B::Equals);
    assert_eq!(restored.display_value(), "42");
    restored.press(B::Add);
    restored.press(B::One);
    restored.press(B::Equals);
    assert_eq!(restored.display_value(), "43");
}

#[test]
fn round_trip_error() {
    let mut vm = new_vm();
    press_all(&mut vm, &[B::One, B::Divide, B::Zero, B::Equals]);
    let mut restored = round_trip(&vm);
    assert!(restored.is_error());
    assert_eq!(restored.display_value(), "Cannot divide by zero");
    restored.press(B::Five);
    assert!(!restored.is_error());
    assert_eq!(restored.display_value(), "5");
}

#[test]
fn round_trip_scientific_fe_and_angle() {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    vm.set_angle_unit(AngleUnit::Gradians);
    press_all(&mut vm, &[B::Two, B::Multiply, B::Three, B::Equals]);
    press_all(
        &mut vm,
        &[B::One, B::Two, B::Three, B::Four, B::Five, B::FToE],
    );
    assert_eq!(vm.display_value(), "1.2345e+4");
    let mut restored = round_trip(&vm);
    assert!(restored.is_fe());
    restored.press(B::Clear);
    press_all(&mut restored, &[B::One, B::Zero, B::Zero, B::Sin]);
    assert_eq!(restored.display_value(), "1");
}

#[test]
fn round_trip_programmer_hex_byte() {
    let mut vm = programmer();
    vm.set_word_size(WordSize::Byte);
    vm.set_radix(Radix::Hex);
    vm.set_shift_mode(ShiftMode::Rotate);
    press_all(
        &mut vm,
        &[
            B::F,
            B::E,
            B::Memory,
            B::Clear,
            B::Seven,
            B::Memory,
            B::A,
            B::Or,
            B::Five,
        ],
    );
    assert_eq!(vm.memory(), ["7", "FE"]);
    let mut restored = round_trip(&vm);
    assert_eq!(restored.display_value(), "5");
    assert_eq!(restored.expression(), "A OR ");
    restored.press(B::Equals);
    assert_eq!(restored.display_value(), "F");
    restored.set_radix(Radix::Dec);
    assert_eq!(restored.memory(), ["7", "-2"]);

    // After "=" (the expression is reloaded like a history item).
    let mut vm = programmer();
    vm.set_radix(Radix::Oct);
    press_all(&mut vm, &[B::Seven, B::Add, B::One, B::Equals]);
    assert_eq!(vm.display_value(), "10");
    let mut restored = round_trip(&vm);
    assert_eq!(restored.radix(), Radix::Oct);
    assert_eq!(restored.expression(), "7 + 1=");
    restored.press(B::Equals); // repeats "+ 1" like the original would
    assert_eq!(restored.display_value(), "11");
    restored.press(B::Two);
    assert_eq!(restored.display_value(), "2");
}

#[test]
fn round_trip_keeps_both_histories() {
    let mut vm = new_vm();
    press_all(&mut vm, &[B::One, B::Add, B::One, B::Equals]);
    vm.set_mode(CalcMode::Scientific);
    press_all(
        &mut vm,
        &[B::Two, B::Add, B::Two, B::Multiply, B::Two, B::Equals],
    );
    let mut restored = round_trip(&vm);
    assert_eq!(restored.history()[0].result, "6");
    restored.set_mode(CalcMode::Standard);
    assert_eq!(restored.history().len(), 1);
    assert_eq!(restored.history()[0].expression, "1   +   1 =");
    // Restoring twice gives the same calculator again.
    let again = round_trip(&restored);
    assert_eq!(again.history()[0].result, "2");
}

// ---- Events

#[test]
fn events_for_a_simple_sum() {
    let mut vm = new_vm();
    assert!(vm.take_events().is_empty());
    let mut seen = Vec::new();
    for b in [B::One, B::Two, B::Add, B::Seven, B::Equals] {
        vm.press(b);
        seen.push(vm.take_events());
    }
    assert_eq!(
        seen,
        [
            vec![Event::Typing],
            vec![Event::Typing],
            vec![Event::Result],
            vec![Event::Typing],
            vec![Event::Result, Event::HistoryChanged],
        ]
    );
    assert_eq!(vm.display_value(), "19");
}

#[test]
fn events_for_typical_inputs() {
    let mut vm = new_vm();
    let step = |vm: &mut CalculatorViewModel, b: B| {
        vm.press(b);
        vm.take_events()
    };
    // Standard evaluates at the second operator and records history.
    assert_eq!(step(&mut vm, B::Two), [Event::Typing]);
    assert_eq!(step(&mut vm, B::Multiply), [Event::Result]);
    assert_eq!(step(&mut vm, B::Three), [Event::Typing]);
    assert_eq!(
        step(&mut vm, B::Add),
        [Event::Result, Event::HistoryChanged]
    );
    // Unary function and percent are results; negating an operand is typing.
    assert_eq!(step(&mut vm, B::Nine), [Event::Typing]);
    assert_eq!(step(&mut vm, B::Sqrt), [Event::Result]);
    assert_eq!(step(&mut vm, B::Percent), [Event::Result]);
    assert_eq!(step(&mut vm, B::Clear), [Event::Replace]);
    assert_eq!(step(&mut vm, B::Five), [Event::Typing]);
    assert_eq!(step(&mut vm, B::Negate), [Event::Typing]);
    assert_eq!(step(&mut vm, B::Decimal), [Event::Typing]);
    assert_eq!(step(&mut vm, B::Backspace), [Event::Typing]);
    // "-5 =" completes an equation of its own (history "-5 =").
    assert_eq!(
        step(&mut vm, B::Equals),
        [Event::Result, Event::HistoryChanged]
    );
    assert_eq!(
        step(&mut vm, B::Negate),
        [Event::Result],
        "negating a result is a unary operation"
    );
    // Memory.
    assert_eq!(step(&mut vm, B::Memory), [Event::MemoryChanged]);
    assert_eq!(step(&mut vm, B::MemoryAdd), [Event::MemoryChanged]);
    // Recalling a number ends the unary-only line "negate(-5)", which
    // Standard mode records in history.
    assert_eq!(
        step(&mut vm, B::MemoryRecall),
        [Event::Replace, Event::HistoryChanged]
    );
    assert_eq!(step(&mut vm, B::MemoryClear), [Event::MemoryChanged]);
    // Errors.
    for b in [B::One, B::Divide, B::Zero] {
        step(&mut vm, b);
    }
    assert_eq!(step(&mut vm, B::Equals), [Event::Error]);
    assert_eq!(
        step(&mut vm, B::Add),
        [Event::Replace],
        "an operator only clears the error"
    );
    for b in [B::One, B::Divide, B::Zero] {
        step(&mut vm, b);
    }
    assert_eq!(step(&mut vm, B::Equals), [Event::Error]);
    assert_eq!(
        step(&mut vm, B::Four),
        [Event::Typing],
        "a digit clears the error and is typed"
    );
}

#[test]
fn events_for_replacements() {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    assert_eq!(vm.take_events(), [Event::Replace, Event::HistoryChanged]);
    vm.set_mode(CalcMode::Scientific);
    assert!(
        !vm.take_events().contains(&Event::HistoryChanged),
        "same mode: nothing changes"
    );

    assert!(vm.paste("12"));
    assert_eq!(vm.take_events(), [Event::Replace]);
    assert!(vm.paste("1+2="));
    assert_eq!(vm.take_events(), [Event::Replace, Event::HistoryChanged]);
    assert!(!vm.paste("nope"));
    assert_eq!(vm.take_events(), [Event::Error]);

    vm.press(B::Clear);
    vm.take_events();
    vm.press(B::FToE);
    assert_eq!(vm.take_events(), [Event::Replace]);
    vm.set_angle_unit(AngleUnit::Radians);
    assert!(vm.take_events().is_empty());

    vm.history_recall(0);
    assert_eq!(vm.take_events(), [Event::Replace]);
    vm.history_remove(0);
    assert_eq!(vm.take_events(), [Event::HistoryChanged]);

    vm.set_mode(CalcMode::Programmer);
    vm.take_events();
    vm.press(B::Five);
    vm.press(B::Memory);
    vm.take_events();
    vm.set_radix(Radix::Bin);
    assert_eq!(
        vm.take_events(),
        [Event::Replace, Event::MemoryChanged],
        "memory is shown in the new radix"
    );
    vm.set_word_size(WordSize::Word);
    assert_eq!(vm.take_events(), [Event::Replace]);
    vm.memory_clear(0);
    assert_eq!(vm.take_events(), [Event::MemoryChanged]);

    let state = vm.save_state();
    let mut other = new_vm();
    other.restore_state(&state);
    assert_eq!(
        other.take_events(),
        [Event::Replace, Event::HistoryChanged, Event::MemoryChanged]
    );
}

#[test]
fn fe_is_disabled_after_a_history_recall_until_the_next_key() {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    press_all(&mut vm, &[B::Two, B::Add, B::Three, B::Equals]);
    assert!(vm.is_enabled(B::FToE));
    vm.history_recall(0);
    assert_eq!(vm.display_value(), "5");
    assert_eq!(vm.expression(), "2 + 3=");
    assert!(!vm.is_enabled(B::FToE));
    vm.press(B::FToE); // ignored while disabled
    assert!(!vm.is_fe());
    vm.press(B::Multiply);
    assert!(vm.is_enabled(B::FToE));
}

#[test]
fn scientific_expression_line() {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    press_all(
        &mut vm,
        &[
            B::OpenParenthesis,
            B::One,
            B::Add,
            B::Two,
            B::CloseParenthesis,
            B::Multiply,
        ],
    );
    assert_eq!(vm.expression(), "(1 + 2) × ");
    press_all(&mut vm, &[B::Three, B::Equals]);
    assert_eq!(vm.expression(), "(1 + 2) × 3=");
    assert_eq!(vm.display_value(), "9");
    assert_eq!(vm.history()[0].expression, "( 1   +   2 )   ×   3 =");
    press_all(&mut vm, &[B::Two, B::XPowerY, B::One, B::Zero, B::Equals]);
    assert_eq!(vm.expression(), "2 ^ 10=");
    assert_eq!(vm.display_value(), "1,024");
}

#[test]
fn hex_digits_are_typing() {
    let mut vm = programmer();
    vm.set_radix(Radix::Hex);
    vm.take_events();
    vm.press(B::A);
    assert_eq!(vm.take_events(), [Event::Typing]);
    vm.press(B::F);
    assert_eq!(vm.take_events(), [Event::Typing]);
    assert_eq!(vm.display_value(), "AF");
    vm.press(B::Not);
    assert_eq!(vm.take_events(), [Event::Result]);
    assert_eq!(vm.display_value(), "FFFF FFFF FFFF FF50");
}

#[test]
fn round_trip_scientific_after_equals_keeps_angle_and_fe() {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    vm.set_angle_unit(AngleUnit::Radians);
    vm.press(B::FToE);
    press_all(&mut vm, &[B::Two, B::Multiply, B::Three, B::Equals]);
    assert_eq!(vm.display_value(), "6.e+0");
    vm.press(B::Memory);
    let mut restored = round_trip(&vm);
    assert!(restored.is_fe());
    assert_eq!(restored.memory(), ["6.e+0"]);
    restored.press(B::Clear); // also turns F-E off, like upstream
    assert!(!restored.is_fe());
    press_all(&mut restored, &[B::Zero, B::Cos]);
    assert_eq!(restored.expression(), "cosᵣ(0)");
    assert_eq!(restored.display_value(), "1");
    restored.memory_recall(0);
    assert_eq!(restored.display_value(), "6");
}

#[test]
fn round_trip_recalled_value_is_not_lost() {
    // After MR the engine is not recording, so the display commands are empty;
    // the value is re-entered on restore.
    let mut vm = new_vm();
    press_all(
        &mut vm,
        &[B::Four, B::Two, B::Memory, B::Clear, B::MemoryRecall],
    );
    assert_eq!(vm.display_value(), "42");
    let mut restored = round_trip(&vm);
    press_all(&mut restored, &[B::Add, B::One, B::Equals]);
    assert_eq!(restored.display_value(), "43");
}

#[test]
fn restored_error_survives_page_activation() {
    // The GTK page restores, then activates the same mode.
    let mut vm = new_vm();
    press_all(&mut vm, &[B::One, B::Divide, B::Zero, B::Equals]);
    let mut restored = round_trip(&vm);
    restored.set_mode(restored.mode());
    assert!(restored.is_error());
    assert!(!restored.is_enabled(B::Add));
    assert_eq!(restored.copy_text(), "Cannot divide by zero");

    // Same for a paste error and an engine error.
    let mut vm = new_vm();
    assert!(!vm.paste("x"));
    vm.set_mode(CalcMode::Standard);
    assert!(vm.is_error());
    press_all(&mut vm, &[B::One, B::Divide, B::Zero, B::Equals]);
    vm.set_mode(CalcMode::Standard);
    assert!(vm.is_error());
    // A different mode starts clean.
    vm.set_mode(CalcMode::Scientific);
    assert!(!vm.is_error());
    assert_eq!(vm.display_value(), "0");
}

#[test]
fn memory_round_trip_at_the_entry_limits() {
    let mut vm = new_vm();
    press_all(
        &mut vm,
        &[B::One, B::Divide, B::Three, B::Equals, B::Memory],
    );
    press_all(
        &mut vm,
        &[B::Two, B::Divide, B::Three, B::Equals, B::Negate, B::Memory],
    );
    for _ in 0..2 {
        for _ in 0..8 {
            vm.press(B::Nine);
        }
        vm.press(B::Multiply);
    }
    vm.press(B::Equals);
    vm.press(B::Memory);
    press_all(
        &mut vm,
        &[
            B::One,
            B::Divide,
            B::Seven,
            B::Zero,
            B::Zero,
            B::Zero,
            B::Zero,
            B::Zero,
            B::Zero,
            B::Zero,
            B::Equals,
            B::Memory,
        ],
    );
    let memory = vm.memory();
    assert_eq!(memory.len(), 4, "{memory:?}");
    let restored = round_trip(&vm);
    assert_eq!(restored.memory(), memory);

    // Scientific precision.
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    press_all(&mut vm, &[B::Two, B::Sqrt, B::Memory, B::Pi, B::Memory]);
    let memory = vm.memory();
    let restored = round_trip(&vm);
    assert_eq!(restored.memory(), memory);
}
