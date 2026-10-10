// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! What the view model tells a screen reader, as upstream's does: each
//! `Announcement = …` of `StandardCalculatorViewModel`, in order.

use super::new_vm;
use crate::{Announcement, Button as B, CalcMode, CalculatorViewModel};

fn scientific() -> CalculatorViewModel {
    let mut vm = new_vm();
    vm.set_mode(CalcMode::Scientific);
    vm.take_announcements();
    vm
}

fn display(value: &str) -> Announcement {
    Announcement::Display {
        value: value.into(),
        feedback: None,
    }
}

fn with_feedback(value: &str, feedback: &'static str) -> Announcement {
    Announcement::Display {
        value: value.into(),
        feedback: Some(feedback),
    }
}

/// What pressing `buttons` announces.
fn said(vm: &mut CalculatorViewModel, buttons: &[B]) -> Vec<Announcement> {
    for &b in buttons {
        vm.press(b);
    }
    vm.take_announcements()
}

/// `HandlePropertySideEffects`: the display is announced whenever the value
/// shown changes, typed digits included, and not when it is shown again
/// unchanged. `OnBinaryOperatorReceived`: and after a binary operator,
/// changed or not, with the operator's auditory feedback
/// (`GetDisplayUpdatedNarratorAnnouncement`).
#[test]
fn the_display_is_announced_as_it_changes_and_after_a_binary_operator() {
    let mut vm = new_vm();
    assert_eq!(vm.take_announcements(), []);
    assert_eq!(said(&mut vm, &[B::Five]), [display("5")]);
    assert_eq!(said(&mut vm, &[B::Add]), [with_feedback("5", "plus")]);
    // Again: the display is the same, the operator is told again.
    assert_eq!(said(&mut vm, &[B::Add]), [with_feedback("5", "plus")]);
    assert_eq!(said(&mut vm, &[B::Three]), [display("3")]);
    assert_eq!(said(&mut vm, &[B::Equals]), [display("8")]);
    // "=" again repeats the last step: the engine shows its operand, then
    // the result, and each is a change of the value shown.
    assert_eq!(
        said(&mut vm, &[B::Equals, B::Clear]),
        [display("3"), display("11"), display("0")]
    );
    // The same value shown again says nothing.
    assert_eq!(said(&mut vm, &[B::Clear]), []);
    assert_eq!(said(&mut vm, &[B::Zero]), []);
    // A pending calculation evaluated by the next operator: its result,
    // with the operator, then the operator's own announcement.
    assert_eq!(
        said(&mut vm, &[B::Two, B::Multiply, B::Four, B::Subtract]),
        [
            display("2"),
            with_feedback("2", "times"),
            display("4"),
            with_feedback("8", "minus"),
            display("8"),
        ]
    );
    for (button, feedback) in [
        (B::Divide, "divided by"),
        (B::Multiply, "times"),
        (B::Subtract, "minus"),
        (B::Add, "plus"),
    ] {
        vm.press(B::Clear);
        vm.take_announcements();
        assert_eq!(said(&mut vm, &[button]), [with_feedback("0", feedback)]);
    }
}

/// `SetParenthesisCount`: each change of the open-parenthesis count is
/// announced in Scientific and Programmer, one by one where a step makes
/// several ("=" closing two, a paste); `OnNoRightParenAdded`: and a ")"
/// with none open (CEngine/scicomm.cpp:566-569).
#[test]
fn parentheses_are_announced_as_they_open_and_close() {
    use Announcement::{NoRightParenthesis, OpenParenthesisCount as Open};
    for mode in [CalcMode::Scientific, CalcMode::Programmer] {
        let mut vm = scientific();
        vm.set_mode(mode);
        vm.take_announcements();
        assert_eq!(said(&mut vm, &[B::CloseParenthesis]), [NoRightParenthesis]);
        assert_eq!(said(&mut vm, &[B::OpenParenthesis]), [Open(1)], "{mode:?}");
        assert_eq!(said(&mut vm, &[B::OpenParenthesis]), [Open(2)]);
        assert_eq!(
            said(&mut vm, &[B::Seven, B::CloseParenthesis]),
            [display("7"), Open(1)]
        );
        assert_eq!(said(&mut vm, &[B::Clear]), [Open(0), display("0")]);
        // "=" closes each in turn.
        vm.press(B::OpenParenthesis);
        vm.press(B::OpenParenthesis);
        vm.press(B::Two);
        vm.take_announcements();
        assert_eq!(said(&mut vm, &[B::Equals]), [Open(1), Open(0)], "{mode:?}");
        vm.press(B::Clear);
        vm.take_announcements();
        // A paste that opens and closes one: both changes.
        assert!(vm.paste("(1)"));
        assert_eq!(
            vm.take_announcements(),
            [Open(1), display("1"), Open(0)],
            "{mode:?}"
        );
        assert_eq!(said(&mut vm, &[B::CloseParenthesis]), [NoRightParenthesis]);
    }
}

/// Standard has no count to tell (`IsProgrammer || IsScientific`).
#[test]
fn standard_announces_no_parenthesis_count() {
    let mut vm = scientific();
    vm.press(B::OpenParenthesis);
    assert_eq!(
        vm.take_announcements(),
        [Announcement::OpenParenthesisCount(1)]
    );
    vm.set_mode(CalcMode::Standard);
    let said = vm.take_announcements();
    assert!(
        !said
            .iter()
            .any(|a| matches!(a, Announcement::OpenParenthesisCount(_))),
        "{said:?}"
    );
}

/// The feedback of a button is said with the next display announcement
/// and forgotten once said, or when a button without one is pressed.
#[test]
fn auditory_feedback_is_said_once() {
    let mut vm = scientific();
    assert_eq!(
        said(&mut vm, &[B::Two, B::XPowerY]),
        [display("2"), with_feedback("2", "to the power of")]
    );
    assert_eq!(said(&mut vm, &[B::Three]), [display("3")]);
    assert_eq!(B::Equals.auditory_feedback(), None);
    assert_eq!(B::Seven.auditory_feedback(), None);
    assert_eq!(B::Nand.auditory_feedback(), Some("Nand"));
}

/// Nobody listening, only the latest announcements are kept.
#[test]
fn untaken_announcements_are_bounded() {
    let mut vm = new_vm();
    for _ in 0..500 {
        vm.press(B::One);
        vm.press(B::Clear);
    }
    let kept = vm.take_announcements();
    assert_eq!(kept.len(), 64);
    assert_eq!(kept.last(), Some(&display("0")));
}
