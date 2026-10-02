// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Inputs for which the C++ engine has undefined behaviour (and which the
//! oracle therefore avoids). The Rust port deliberately deviates here: these
//! tests only require "no panic, no hang, sane state afterwards".

use std::cell::RefCell;
use std::rc::Rc;

use calcmanager::*;

#[derive(Default)]
struct Ui {
    primary: String,
    expression: String,
    is_error: bool,
    paren: u32,
}

impl CalcDisplay for Ui {
    fn set_primary_display(&mut self, text: &str, is_error: bool) {
        self.primary = text.to_string();
        self.is_error = is_error;
    }
    fn set_is_in_error(&mut self, is_in_error: bool) {
        self.is_error = is_in_error;
    }
    fn set_expression_display(
        &mut self,
        tokens: &[ExpressionToken],
        _commands: &[ExpressionCommand],
    ) {
        self.expression = tokens.iter().map(|t| t.0.as_str()).collect();
    }
    fn set_parenthesis_number(&mut self, count: u32) {
        self.paren = count;
    }
    fn on_no_right_paren_added(&mut self) {}
    fn max_digits_reached(&mut self) {}
    fn binary_operator_received(&mut self) {}
    fn on_history_item_added(&mut self, _added_item_index: u32) {}
    fn set_memorized_numbers(&mut self, _memorized_numbers: &[String]) {}
    fn memory_item_changed(&mut self, _index_of_memory: u32) {}
    fn input_changed(&mut self) {}
}

fn scientific() -> (Rc<RefCell<Ui>>, CalculatorManager) {
    let ui = Rc::new(RefCell::new(Ui::default()));
    let mut mgr = CalculatorManager::new(ui.clone(), Rc::new(EngineResourceProvider::default()));
    mgr.set_scientific_mode().unwrap();
    (ui, mgr)
}

fn send_all(mgr: &mut CalculatorManager, cmds: &[Command]) {
    for &c in cmds {
        mgr.send_command(c).unwrap();
    }
}

/// `( ( 8 ) 2 )`: the implicit multiplication after `)` wipes the precedence
/// stack including the outer parenthesis marker; the C++ then underflows a
/// `size_t` on the next `)`.
#[test]
fn close_paren_after_implicit_multiplication_does_not_underflow() {
    let (ui, mut mgr) = scientific();
    send_all(
        &mut mgr,
        &[
            Command::CommandOPENP,
            Command::CommandOPENP,
            Command::Command8,
            Command::CommandCLOSEP,
            Command::Command2,
            Command::CommandCLOSEP,
        ],
    );
    assert_eq!(ui.borrow().paren, 0);
    assert_eq!(ui.borrow().primary, "16");
    mgr.send_command(Command::CommandEQU).unwrap();
    assert_eq!(ui.borrow().primary, "16");
    assert!(!ui.borrow().is_error);

    // Same with a pending higher-precedence operator inside the wiped level.
    send_all(
        &mut mgr,
        &[
            Command::CommandCLEAR,
            Command::CommandOPENP,
            Command::CommandOPENP,
            Command::Command8,
            Command::CommandCLOSEP,
            Command::Command2,
            Command::CommandADD,
            Command::Command3,
            Command::CommandMUL,
            Command::Command4,
            Command::CommandCLOSEP,
            Command::CommandEQU,
        ],
    );
    assert!(!ui.borrow().is_error);
    assert_eq!(ui.borrow().paren, 0);
    // Calculator still works afterwards.
    send_all(
        &mut mgr,
        &[
            Command::CommandCLEAR,
            Command::Command1,
            Command::CommandADD,
            Command::Command2,
            Command::CommandEQU,
        ],
    );
    assert_eq!(ui.borrow().primary, "3");
}

/// 25 nested parentheses with a full precedence stack: the C++ `=` loops
/// forever because every automatic `)` is rejected.
#[test]
fn equals_with_full_precedence_stack_terminates() {
    let (ui, mut mgr) = scientific();
    // Each level pushes a 0 marker and `1 OR 2 AND 3 + 4 × 5 ^` pushes 4
    // operators of increasing precedence: 5 levels fill all 25 slots with an
    // operator on top, so every `)` is rejected.
    for _ in 0..5 {
        send_all(
            &mut mgr,
            &[
                Command::CommandOPENP,
                Command::Command1,
                Command::CommandOR,
                Command::Command2,
                Command::CommandAnd,
                Command::Command3,
                Command::CommandADD,
                Command::Command4,
                Command::CommandMUL,
                Command::Command5,
                Command::CommandPWR,
            ],
        );
    }
    assert_eq!(ui.borrow().paren, 5);
    let engine = mgr.current_calculator_engine().unwrap();
    assert_eq!(engine.open_paren_count(), 5);
    mgr.send_command(Command::Command2).unwrap();
    mgr.send_command(Command::CommandEQU).unwrap();
    // Recovers normally.
    send_all(
        &mut mgr,
        &[
            Command::CommandCLEAR,
            Command::Command6,
            Command::CommandDIV,
            Command::Command3,
            Command::CommandEQU,
        ],
    );
    assert_eq!(ui.borrow().primary, "2");
}

/// After `MemorizeNumber` the engine's memory value has been moved out (C++
/// leaves a null `unique_ptr`); a direct `MR` / `M+` / `M-` dereferences it.
#[test]
fn direct_memory_commands_after_manager_memory_ops() {
    let (ui, mut mgr) = scientific();
    send_all(&mut mgr, &[Command::Command7]);
    mgr.memorize_number().unwrap();
    mgr.send_command(Command::CommandRECALL).unwrap();
    assert_eq!(ui.borrow().primary, "0");
    mgr.memorize_number().unwrap();
    mgr.send_command(Command::CommandMPLUS).unwrap();
    mgr.memorize_number().unwrap();
    mgr.send_command(Command::CommandMMINUS).unwrap();
    mgr.send_command(Command::CommandRECALL).unwrap();
    assert!(!ui.borrow().is_error);
}

/// `m_memorizedNumbers.at(i)` throws `std::out_of_range` in C++.
#[test]
fn memory_index_out_of_range_is_an_error() {
    let (_ui, mut mgr) = scientific();
    mgr.send_command(Command::Command5).unwrap();
    mgr.memorize_number().unwrap();
    assert_eq!(mgr.memorized_number_load(3), Err(E_BOUNDS));
    assert_eq!(mgr.memorized_number_add(3), Err(E_BOUNDS));
    assert_eq!(mgr.memorized_number_subtract(3), Err(E_BOUNDS));
    mgr.memorized_number_load(0).unwrap();
}

/// History queries before any standard/scientific mode (C++ dereferences a
/// null `m_pHistory`).
#[test]
fn history_queries_in_programmer_mode_without_history() {
    let ui = Rc::new(RefCell::new(Ui::default()));
    let mut mgr = CalculatorManager::new(ui.clone(), Rc::new(EngineResourceProvider::default()));
    mgr.set_programmer_mode().unwrap();
    assert!(mgr.get_history_items().is_empty());
    assert!(!mgr.remove_history_item(0));
    mgr.clear_history();
    assert_eq!(mgr.max_history_size(), 20);
    send_all(
        &mut mgr,
        &[
            Command::CommandA,
            Command::CommandXor,
            Command::Command3,
            Command::CommandEQU,
        ],
    );
    assert_eq!(ui.borrow().primary, "3");
}

/// Random keyboard mashing over the parenthesis / precedence state machine
/// (including the inputs above) never panics or hangs. Expensive operations
/// (shifts / powers with huge operands are slow in ratpack, C++ included)
/// are left out on purpose.
#[test]
fn fuzz_no_panics() {
    let cmds: Vec<Command> = [
        Command::CommandSIGN,
        Command::CommandCLEAR,
        Command::CommandCENTR,
        Command::CommandBACK,
        Command::CommandPNT,
        Command::CommandADD,
        Command::CommandSUB,
        Command::CommandMUL,
        Command::CommandDIV,
        Command::CommandMOD,
        Command::CommandAnd,
        Command::CommandOR,
        Command::CommandXor,
        Command::CommandSQRT,
        Command::CommandSQR,
        Command::CommandREC,
        Command::CommandPERCENT,
        Command::CommandINV,
        Command::CommandFE,
        Command::CommandPI,
        Command::CommandSTORE,
        Command::CommandRECALL,
        Command::CommandMPLUS,
        Command::CommandMMINUS,
        Command::CommandMCLEAR,
        Command::CommandSET_RESULT,
        Command::CommandDegrees,
        Command::CommandAbs,
        Command::CommandFloor,
        Command::CommandCeil,
        Command::CommandNand,
        Command::CommandNor,
        Command::ModeBasic,
        Command::ModeScientific,
    ]
    .to_vec();
    let mut seed: u64 = 0x1234_5678_9abc_def0;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..30 {
        let (_ui, mut mgr) = scientific();
        for _ in 0..150 {
            let r = next();
            // Bias toward parentheses / digits / equals to reach deep states.
            let c = match r % 10 {
                0..=2 => Command(128 + (r >> 8) as i32 % 2),
                3..=5 => Command(130 + (r >> 8) as i32 % 10),
                6 => Command::CommandEQU,
                _ => cmds[(r >> 8) as usize % cmds.len()],
            };
            let _ = mgr.send_command(c);
            match (r >> 40) % 23 {
                0 => {
                    let _ = mgr.memorize_number();
                }
                1 => {
                    let _ = mgr.memorized_number_add(((r >> 20) % 3) as u32);
                }
                2 => {
                    let _ = mgr.get_result_for_radix(16, 64, true);
                }
                _ => {}
            }
        }
    }
}
