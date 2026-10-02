// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CalculatorUnitTests/CalculatorManagerTest.cpp`.

use std::cell::RefCell;
use std::rc::Rc;

use calcmanager::number_formatting_utils::*;
use calcmanager::*;

#[derive(Default)]
struct CalculatorManagerDisplayTester {
    m_primary_display: String,
    m_expression: String,
    m_paren_display: u32,
    m_is_error: bool,
    m_memorized_number_strings: Vec<String>,
    m_max_digits_called_count: i32,
    m_binary_operator_received_call_count: i32,
}

impl CalculatorManagerDisplayTester {
    fn reset(&mut self) {
        self.m_is_error = false;
        self.m_max_digits_called_count = 0;
        self.m_binary_operator_received_call_count = 0;
    }

    fn get_primary_display(&self) -> &str {
        &self.m_primary_display
    }
    fn get_expression(&self) -> &str {
        &self.m_expression
    }
    fn get_memorized_numbers(&self) -> &[String] {
        &self.m_memorized_number_strings
    }
    fn get_is_error(&self) -> bool {
        self.m_is_error
    }
    fn get_max_digits_called_count(&self) -> i32 {
        self.m_max_digits_called_count
    }
    fn get_binary_operator_received_call_count(&self) -> i32 {
        self.m_binary_operator_received_call_count
    }
}

impl CalcDisplay for CalculatorManagerDisplayTester {
    fn set_primary_display(&mut self, text: &str, is_error: bool) {
        self.m_primary_display = text.to_string();
        self.m_is_error = is_error;
    }
    fn set_is_in_error(&mut self, is_error: bool) {
        self.m_is_error = is_error;
    }
    fn set_expression_display(
        &mut self,
        tokens: &[ExpressionToken],
        _commands: &[ExpressionCommand],
    ) {
        self.m_expression.clear();

        for current_pair in tokens {
            self.m_expression.push_str(&current_pair.0);
        }
    }
    fn set_memorized_numbers(&mut self, numbers: &[String]) {
        self.m_memorized_number_strings = numbers.to_vec();
    }
    fn set_parenthesis_number(&mut self, parenthesis_count: u32) {
        self.m_paren_display = parenthesis_count;
    }
    fn on_no_right_paren_added(&mut self) {
        // This method is used to create a narrator announcement when a close parenthesis cannot be added because there are no open parentheses
    }
    fn on_history_item_added(&mut self, _added_item_index: u32) {}
    fn max_digits_reached(&mut self) {
        self.m_max_digits_called_count += 1;
    }
    fn input_changed(&mut self) {}
    fn binary_operator_received(&mut self) {
        self.m_binary_operator_received_call_count += 1;
    }
    fn memory_item_changed(&mut self, _index_of_memory: u32) {}
}

type Tester = Rc<RefCell<CalculatorManagerDisplayTester>>;

/// Creates instance of CalculationManager before running tests
fn common_setup() -> (Tester, CalculatorManager) {
    let m_calculator_display_tester =
        Rc::new(RefCell::new(CalculatorManagerDisplayTester::default()));
    m_calculator_display_tester.borrow_mut().reset();
    let m_resource_provider: Rc<dyn ResourceProvider> = Rc::new(EngineResourceProvider::default());
    let m_calculator_manager =
        CalculatorManager::new(m_calculator_display_tester.clone(), m_resource_provider);
    (m_calculator_display_tester, m_calculator_manager)
}

/// Resets calculator state to start state after each test
fn cleanup(tester: &Tester, m_calculator_manager: &mut CalculatorManager) {
    m_calculator_manager.reset(true).unwrap();
    tester.borrow_mut().reset();
}

/// `TestDriver::Test`
fn test(
    m_display_tester: &Tester,
    m_calculator_manager: &mut CalculatorManager,
    expected_primary: &str,
    expected_expression: &str,
    test_commands: &[Command],
    cleanup: bool,
    is_scientific: bool,
) {
    if cleanup {
        m_calculator_manager.reset(true).unwrap();
    }

    if is_scientific {
        m_calculator_manager
            .send_command(Command::ModeScientific)
            .unwrap();
    }

    for &current_command in test_commands {
        m_calculator_manager.send_command(current_command).unwrap();
    }

    assert_eq!(
        expected_primary,
        m_display_tester.borrow().get_primary_display(),
        "commands: {test_commands:?}"
    );
    if expected_expression != "N/A" {
        assert_eq!(
            expected_expression,
            m_display_tester.borrow().get_expression(),
            "commands: {test_commands:?}"
        );
    }
}

fn execute_commands(m_calculator_manager: &mut CalculatorManager, commands: &[Command]) {
    for &command in commands {
        if command == Command::CommandNULL {
            break;
        }

        m_calculator_manager.send_command(command).unwrap();
    }
}

fn command_list_from_string_input(input: &str) -> Vec<Command> {
    let mut result = Vec::new();
    for ch in input.chars() {
        let mut as_command = Command::CommandNULL;
        if ch == '.' {
            as_command = Command::CommandPNT;
        } else if ch.is_ascii_digit() {
            let diff = ch as i32 - '0' as i32;
            as_command = Command(diff + Command::Command0.0);
        }

        if as_command != Command::CommandNULL {
            result.push(as_command);
        }
    }

    result
}

fn test_max_digits_reached_scenario(const_input: &str) {
    let (p_calculator_display, mut m_calculator_manager) = common_setup();

    // Make sure we're in a clean state.
    assert_eq!(
        0,
        p_calculator_display.borrow().get_max_digits_called_count()
    );

    let mut commands = command_list_from_string_input(const_input);
    assert!(!commands.is_empty());

    // The last element in the list should always cause MaxDigitsReached
    // Remember the command but remove from the actual input that is sent
    let final_input = commands.pop().unwrap();
    let input = &const_input[0..const_input.len() - 1];

    m_calculator_manager.set_standard_mode().unwrap();
    execute_commands(&mut m_calculator_manager, &commands);

    let expected_display = input;
    let display = p_calculator_display
        .borrow()
        .get_primary_display()
        .to_string();
    assert_eq!(expected_display, display);

    m_calculator_manager.send_command(final_input).unwrap();

    // Verify MaxDigitsReached
    let display = p_calculator_display
        .borrow()
        .get_primary_display()
        .to_string();
    assert_eq!(expected_display, display);

    // MaxDigitsReached should have been called once
    assert!(0 < p_calculator_display.borrow().get_max_digits_called_count());
    cleanup(&p_calculator_display, &mut m_calculator_manager);
}

fn prefix_equal(memorized_numbers: &[String], expected: &[&str]) -> bool {
    if memorized_numbers.len() < expected.len() {
        memorized_numbers
            .iter()
            .zip(expected.iter())
            .all(|(a, b)| a == b)
    } else {
        expected
            .iter()
            .zip(memorized_numbers.iter())
            .all(|(b, a)| a == b)
    }
}

#[test]
fn calculator_manager_test_standard() {
    let (m_display_tester, mut m_calculator_manager) = common_setup();
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "123.456",
        "",
        &[
            Command::Command1,
            Command::Command2,
            Command::Command3,
            Command::CommandPNT,
            Command::Command4,
            Command::Command5,
            Command::Command6,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "0 + ",
        &[Command::CommandADD],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "\u{221A}(0)",
        &[Command::CommandSQRT],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "7",
        "4 + 3=",
        &[
            Command::Command2,
            Command::CommandADD,
            Command::Command3,
            Command::CommandEQU,
            Command::Command4,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "4",
        "4=",
        &[Command::Command4, Command::CommandEQU],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "2",
        "\u{221A}(\u{221A}(\u{221A}(256)))",
        &[
            Command::Command2,
            Command::Command5,
            Command::Command6,
            Command::CommandSQRT,
            Command::CommandSQRT,
            Command::CommandSQRT,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-9",
        "-3 \u{00D7} 3=",
        &[
            Command::Command3,
            Command::CommandSUB,
            Command::Command6,
            Command::CommandEQU,
            Command::CommandMUL,
            Command::Command3,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "46",
        "54 - 8=",
        &[
            Command::Command9,
            Command::CommandMUL,
            Command::Command6,
            Command::CommandSUB,
            Command::CommandCENTR,
            Command::Command8,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0.36",
        "6 \u{00D7} 0.06=",
        &[
            Command::Command6,
            Command::CommandMUL,
            Command::Command6,
            Command::CommandPERCENT,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "60",
        "50 + 10=",
        &[
            Command::Command5,
            Command::Command0,
            Command::CommandADD,
            Command::Command2,
            Command::Command0,
            Command::CommandPERCENT,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "8",
        "4 + 4=",
        &[Command::Command4, Command::CommandADD, Command::CommandEQU],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "3",
        "5 \u{00D7} ",
        &[
            Command::Command5,
            Command::CommandADD,
            Command::CommandMUL,
            Command::Command3,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Overflow",
        "1.e-9999 \u{00F7} ",
        &[
            Command::Command1,
            Command::CommandEXP,
            Command::CommandSIGN,
            Command::Command9,
            Command::Command9,
            Command::Command9,
            Command::Command9,
            Command::CommandDIV,
            Command::Command1,
            Command::Command0,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "60",
        "50 + 10=",
        &[
            Command::Command5,
            Command::Command0,
            Command::CommandADD,
            Command::Command2,
            Command::Command0,
            Command::CommandPERCENT,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Result is undefined",
        "0 \u{00F7} ",
        &[
            Command::Command0,
            Command::CommandDIV,
            Command::Command0,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Cannot divide by zero",
        "1 \u{00F7} ",
        &[
            Command::Command1,
            Command::CommandDIV,
            Command::Command0,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "14",
        "14 + ",
        &[
            Command::Command1,
            Command::Command2,
            Command::CommandADD,
            Command::Command5,
            Command::CommandCENTR,
            Command::Command2,
            Command::CommandADD,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-0.01",
        "1/(-100)",
        &[
            Command::Command1,
            Command::Command0,
            Command::Command0,
            Command::CommandSIGN,
            Command::CommandREC,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "1",
        "",
        &[
            Command::Command1,
            Command::Command2,
            Command::Command3,
            Command::CommandBACK,
            Command::CommandBACK,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "",
        &[
            Command::Command1,
            Command::Command2,
            Command::Command3,
            Command::CommandBACK,
            Command::CommandBACK,
            Command::CommandBACK,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "0 + ",
        &[
            Command::Command4,
            Command::CommandSQRT,
            Command::CommandSUB,
            Command::Command2,
            Command::CommandADD,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "0 + ",
        &[
            Command::Command1,
            Command::Command0,
            Command::Command2,
            Command::Command4,
            Command::CommandSQRT,
            Command::CommandSUB,
            Command::Command3,
            Command::Command2,
            Command::CommandADD,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "\u{221A}(2.25) - 1.5=",
        &[
            Command::Command2,
            Command::CommandPNT,
            Command::Command2,
            Command::Command5,
            Command::CommandSQRT,
            Command::CommandSUB,
            Command::Command1,
            Command::CommandPNT,
            Command::Command5,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    cleanup(&m_display_tester, &mut m_calculator_manager);
}

#[test]
fn calculator_manager_test_scientific() {
    let (m_display_tester, mut m_calculator_manager) = common_setup();
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "123.456",
        "",
        &[
            Command::Command1,
            Command::Command2,
            Command::Command3,
            Command::CommandPNT,
            Command::Command4,
            Command::Command5,
            Command::Command6,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "0 + ",
        &[Command::CommandADD],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "\u{221A}(0)",
        &[Command::CommandSQRT],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "1",
        "1 + 0 \u{00D7} 2=",
        &[
            Command::Command1,
            Command::CommandADD,
            Command::Command0,
            Command::CommandMUL,
            Command::Command2,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "4",
        "4=",
        &[Command::Command4, Command::CommandEQU],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "2",
        "\u{221A}(\u{221A}(\u{221A}(256)))",
        &[
            Command::Command2,
            Command::Command5,
            Command::Command6,
            Command::CommandSQRT,
            Command::CommandSQRT,
            Command::CommandSQRT,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-9",
        "-3 \u{00D7} 3 + ",
        &[
            Command::Command3,
            Command::CommandSUB,
            Command::Command6,
            Command::CommandEQU,
            Command::CommandMUL,
            Command::Command3,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "38",
        "9 \u{00D7} 6 - 8 \u{00D7} 2 + ",
        &[
            Command::Command9,
            Command::CommandMUL,
            Command::Command6,
            Command::CommandSUB,
            Command::CommandCENTR,
            Command::Command8,
            Command::CommandMUL,
            Command::Command2,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Invalid input",
        "6 \u{00D7} \u{221A}(-6)",
        &[
            Command::Command6,
            Command::CommandMUL,
            Command::Command6,
            Command::CommandSIGN,
            Command::CommandSQRT,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "50.05",
        "50 + 1/(20) - ",
        &[
            Command::Command5,
            Command::Command0,
            Command::CommandADD,
            Command::Command2,
            Command::Command0,
            Command::CommandREC,
            Command::CommandSUB,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "8",
        "4 + 4=",
        &[Command::Command4, Command::CommandADD, Command::CommandEQU],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "3",
        "5 \u{00D7} ",
        &[
            Command::Command5,
            Command::CommandADD,
            Command::CommandMUL,
            Command::Command3,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Overflow",
        "1.e-9999 \u{00F7} ",
        &[
            Command::Command1,
            Command::CommandEXP,
            Command::CommandSIGN,
            Command::Command9,
            Command::Command9,
            Command::Command9,
            Command::Command9,
            Command::CommandDIV,
            Command::Command1,
            Command::Command0,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "60",
        "50 + 10=",
        &[
            Command::Command5,
            Command::Command0,
            Command::CommandADD,
            Command::Command2,
            Command::Command0,
            Command::CommandPERCENT,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Result is undefined",
        "0 \u{00F7} ",
        &[
            Command::Command0,
            Command::CommandDIV,
            Command::Command0,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Cannot divide by zero",
        "1 \u{00F7} ",
        &[
            Command::Command1,
            Command::CommandDIV,
            Command::Command0,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "14",
        "12 + 2 + ",
        &[
            Command::Command1,
            Command::Command2,
            Command::CommandADD,
            Command::Command5,
            Command::CommandCENTR,
            Command::Command2,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-0.01",
        "1/(-100)",
        &[
            Command::Command1,
            Command::Command0,
            Command::Command0,
            Command::CommandSIGN,
            Command::CommandREC,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "1",
        "",
        &[
            Command::Command1,
            Command::Command2,
            Command::Command3,
            Command::CommandBACK,
            Command::CommandBACK,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "",
        &[
            Command::Command1,
            Command::Command2,
            Command::Command3,
            Command::CommandBACK,
            Command::CommandBACK,
            Command::CommandBACK,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "\u{221A}(4) - 2 + ",
        &[
            Command::Command4,
            Command::CommandSQRT,
            Command::CommandSUB,
            Command::Command2,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "\u{221A}(0)",
        &[Command::Command0, Command::CommandSQRT],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "\u{221A}(1024) - 32 + ",
        &[
            Command::Command1,
            Command::Command0,
            Command::Command2,
            Command::Command4,
            Command::CommandSQRT,
            Command::CommandSUB,
            Command::Command3,
            Command::Command2,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "2.0009748976330773374220277351385",
        "\u{221A}(\u{221A}(\u{221A}(257)))",
        &[
            Command::Command2,
            Command::Command5,
            Command::Command7,
            Command::CommandSQRT,
            Command::CommandSQRT,
            Command::CommandSQRT,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "\u{221A}(2.25) - 1.5=",
        &[
            Command::Command2,
            Command::CommandPNT,
            Command::Command2,
            Command::Command5,
            Command::CommandSQRT,
            Command::CommandSUB,
            Command::Command1,
            Command::CommandPNT,
            Command::Command5,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "log(\u{221A}(2.25) \u{00F7} 1.5)",
        &[
            Command::CommandOPENP,
            Command::Command2,
            Command::CommandPNT,
            Command::Command2,
            Command::Command5,
            Command::CommandSQRT,
            Command::CommandDIV,
            Command::Command1,
            Command::CommandPNT,
            Command::Command5,
            Command::CommandCLOSEP,
            Command::CommandLOG,
        ],
        true,
        true,
    );
    cleanup(&m_display_tester, &mut m_calculator_manager);
}

#[test]
fn calculator_manager_test_scientific2() {
    let (m_display_tester, mut m_calculator_manager) = common_setup();
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "144",
        "sqr(12)",
        &[Command::Command1, Command::Command2, Command::CommandSQR],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "120",
        "fact(5)",
        &[Command::Command5, Command::CommandFAC],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "25",
        "5 ^ 2 + ",
        &[
            Command::Command5,
            Command::CommandPWR,
            Command::Command2,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "2",
        "8 yroot 3 \u{00D7} ",
        &[
            Command::Command8,
            Command::CommandROOT,
            Command::Command3,
            Command::CommandMUL,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "512",
        "cube(8)",
        &[Command::Command8, Command::CommandCUB],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "8",
        "cuberoot(cube(8))",
        &[
            Command::Command8,
            Command::CommandCUB,
            Command::CommandCUBEROOT,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "1",
        "log(10)",
        &[Command::Command1, Command::Command0, Command::CommandLOG],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "100,000",
        "10^(5)",
        &[Command::Command5, Command::CommandPOW10],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "2.3025850929940456840179914546844",
        "ln(10)",
        &[Command::Command1, Command::Command0, Command::CommandLN],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0.01745240643728351281941897851632",
        "sin\u{2080}(1)",
        &[Command::Command1, Command::CommandSIN],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0.99984769515639123915701155881391",
        "cos\u{2080}(1)",
        &[Command::Command1, Command::CommandCOS],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0.01745506492821758576512889521973",
        "tan\u{2080}(1)",
        &[Command::Command1, Command::CommandTAN],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "90",
        "sin\u{2080}\u{207B}\u{00B9}(1)",
        &[Command::Command1, Command::CommandASIN],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "cos\u{2080}\u{207B}\u{00B9}(1)",
        &[Command::Command1, Command::CommandACOS],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "45",
        "tan\u{2080}\u{207B}\u{00B9}(1)",
        &[Command::Command1, Command::CommandATAN],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "7.389056098930650227230427460575",
        "e^(2)",
        &[Command::Command2, Command::CommandPOWE],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "1",
        "5 ^ 0 + ",
        &[
            Command::Command5,
            Command::CommandPWR,
            Command::Command0,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "1",
        "0 ^ 0 + ",
        &[
            Command::Command0,
            Command::CommandPWR,
            Command::Command0,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-3",
        "-27 yroot 3 + ",
        &[
            Command::Command2,
            Command::Command7,
            Command::CommandSIGN,
            Command::CommandROOT,
            Command::Command3,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "8 ^ (2 \u{00F7} 3) - 4 + ",
        &[
            Command::Command8,
            Command::CommandPWR,
            Command::CommandOPENP,
            Command::Command2,
            Command::CommandDIV,
            Command::Command3,
            Command::CommandCLOSEP,
            Command::CommandSUB,
            Command::Command4,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "4 ^ (3 \u{00F7} 2) - 8 + ",
        &[
            Command::Command4,
            Command::CommandPWR,
            Command::CommandOPENP,
            Command::Command3,
            Command::CommandDIV,
            Command::Command2,
            Command::CommandCLOSEP,
            Command::CommandSUB,
            Command::Command8,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "17.161687912241792074207286679393",
        "10 ^ 1.23456 + ",
        &[
            Command::Command1,
            Command::Command0,
            Command::CommandPWR,
            Command::Command1,
            Command::CommandPNT,
            Command::Command2,
            Command::Command3,
            Command::Command4,
            Command::Command5,
            Command::Command6,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "1.0001523280439076654284264342126",
        "sec\u{2080}(1)",
        &[Command::Command1, Command::CommandSEC],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "57.298688498550183476612683735174",
        "csc\u{2080}(1)",
        &[Command::Command1, Command::CommandCSC],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "57.289961630759424687278147537113",
        "cot\u{2080}(1)",
        &[Command::Command1, Command::CommandCOT],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "sec\u{2080}\u{207B}\u{00B9}(1)",
        &[Command::Command1, Command::CommandASEC],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "90",
        "csc\u{2080}\u{207B}\u{00B9}(1)",
        &[Command::Command1, Command::CommandACSC],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "45",
        "cot\u{2080}\u{207B}\u{00B9}(1)",
        &[Command::Command1, Command::CommandACOT],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0.64805427366388539957497735322615",
        "sech(1)",
        &[Command::Command1, Command::CommandSECH],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0.85091812823932154513384276328718",
        "csch(1)",
        &[Command::Command1, Command::CommandCSCH],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "1.3130352854993313036361612469308",
        "coth(1)",
        &[Command::Command1, Command::CommandCOTH],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "sech\u{207B}\u{00B9}(1)",
        &[Command::Command1, Command::CommandASECH],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0.88137358701954302523260932497979",
        "csch\u{207B}\u{00B9}(1)",
        &[Command::Command1, Command::CommandACSCH],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0.54930614433405484569762261846126",
        "coth\u{207B}\u{00B9}(2)",
        &[Command::Command2, Command::CommandACOTH],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "256",
        "2^(8)",
        &[Command::Command8, Command::CommandPOW2],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "1",
        "N/A",
        &[Command::CommandRand, Command::CommandCeil],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "N/A",
        &[Command::CommandRand, Command::CommandFloor],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "N/A",
        &[
            Command::CommandRand,
            Command::CommandSIGN,
            Command::CommandCeil,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-1",
        "N/A",
        &[
            Command::CommandRand,
            Command::CommandSIGN,
            Command::CommandFloor,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "3",
        "floor(3.8)",
        &[
            Command::Command3,
            Command::CommandPNT,
            Command::Command8,
            Command::CommandFloor,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "4",
        "ceil(3.8)",
        &[
            Command::Command3,
            Command::CommandPNT,
            Command::Command8,
            Command::CommandCeil,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "1.4649735207179271671970404076786",
        "5 log base 3 + ",
        &[
            Command::Command5,
            Command::CommandLogBaseY,
            Command::Command3,
            Command::CommandADD,
        ],
        true,
        true,
    );
    cleanup(&m_display_tester, &mut m_calculator_manager);
}

#[test]
fn calculator_manager_test_scientific_parenthesis() {
    let (m_display_tester, mut m_calculator_manager) = common_setup();
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "3",
        "1 + (0 + 3)",
        &[
            Command::Command1,
            Command::CommandADD,
            Command::CommandOPENP,
            Command::CommandADD,
            Command::Command3,
            Command::CommandCLOSEP,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "12",
        "((12)",
        &[
            Command::CommandOPENP,
            Command::CommandOPENP,
            Command::Command1,
            Command::Command2,
            Command::CommandCLOSEP,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "12",
        "12 \u{00D7} (",
        &[
            Command::Command1,
            Command::Command2,
            Command::CommandCLOSEP,
            Command::CommandCLOSEP,
            Command::CommandOPENP,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "4",
        "2 \u{00D7} (2) + ",
        &[
            Command::Command2,
            Command::CommandOPENP,
            Command::Command2,
            Command::CommandCLOSEP,
            Command::CommandADD,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "8",
        "2 \u{00D7} (2) + 4=",
        &[
            Command::Command2,
            Command::CommandOPENP,
            Command::Command2,
            Command::CommandCLOSEP,
            Command::CommandADD,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "16",
        "(8) \u{00D7} 2=",
        &[
            Command::CommandOPENP,
            Command::Command8,
            Command::CommandCLOSEP,
            Command::Command2,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "28",
        "(7 \u{00D7} 2) \u{00D7} 2=",
        &[
            Command::CommandOPENP,
            Command::Command7,
            Command::CommandMUL,
            Command::Command2,
            Command::CommandCLOSEP,
            Command::Command2,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "28",
        "(14) \u{00D7} 2=",
        &[
            Command::CommandOPENP,
            Command::Command7,
            Command::CommandMUL,
            Command::Command2,
            Command::CommandCLOSEP,
            Command::Command2,
            Command::CommandEQU,
            Command::CommandOPENP,
            Command::Command1,
            Command::Command4,
            Command::CommandCLOSEP,
            Command::Command2,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "4",
        "(8) \u{00D7} 0.5=",
        &[
            Command::CommandOPENP,
            Command::Command8,
            Command::CommandCLOSEP,
            Command::Command0,
            Command::CommandPNT,
            Command::Command5,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "4",
        "(8) \u{00D7} 0.5=",
        &[
            Command::CommandOPENP,
            Command::Command8,
            Command::CommandCLOSEP,
            Command::CommandPNT,
            Command::Command5,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    cleanup(&m_display_tester, &mut m_calculator_manager);
}

#[test]
fn calculator_manager_test_scientific_error() {
    let (m_display_tester, mut m_calculator_manager) = common_setup();
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Cannot divide by zero",
        "1 \u{00F7} ",
        &[
            Command::Command1,
            Command::CommandDIV,
            Command::Command0,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    assert!(m_display_tester.borrow().get_is_error());
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Invalid input",
        "log(-2)",
        &[Command::Command2, Command::CommandSIGN, Command::CommandLOG],
        true,
        true,
    );
    assert!(m_display_tester.borrow().get_is_error());
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Result is undefined",
        "0 \u{00F7} ",
        &[
            Command::Command0,
            Command::CommandDIV,
            Command::Command0,
            Command::CommandEQU,
        ],
        true,
        true,
    );
    assert!(m_display_tester.borrow().get_is_error());
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Cannot divide by zero",
        "1 \u{00F7} ",
        &[
            Command::Command1,
            Command::CommandDIV,
            Command::Command0,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    assert!(m_display_tester.borrow().get_is_error());
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Invalid input",
        "log(-2)",
        &[Command::Command2, Command::CommandSIGN, Command::CommandLOG],
        true,
        false,
    );
    assert!(m_display_tester.borrow().get_is_error());
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "Result is undefined",
        "0 \u{00F7} ",
        &[
            Command::Command0,
            Command::CommandDIV,
            Command::Command0,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    assert!(m_display_tester.borrow().get_is_error());
    cleanup(&m_display_tester, &mut m_calculator_manager);
}

#[test]
fn calculator_manager_test_scientific_mode_change() {
    let (m_display_tester, mut m_calculator_manager) = common_setup();
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "N/A",
        &[Command::CommandRAD, Command::CommandPI, Command::CommandSIN],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-1",
        "N/A",
        &[Command::CommandRAD, Command::CommandPI, Command::CommandCOS],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "N/A",
        &[Command::CommandRAD, Command::CommandPI, Command::CommandTAN],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "N/A",
        &[
            Command::CommandGRAD,
            Command::Command4,
            Command::Command0,
            Command::Command0,
            Command::CommandSIN,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "1",
        "N/A",
        &[
            Command::CommandGRAD,
            Command::Command4,
            Command::Command0,
            Command::Command0,
            Command::CommandCOS,
        ],
        true,
        true,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "N/A",
        &[
            Command::CommandGRAD,
            Command::Command4,
            Command::Command0,
            Command::Command0,
            Command::CommandTAN,
        ],
        true,
        true,
    );
    cleanup(&m_display_tester, &mut m_calculator_manager);
}

#[test]
fn calculator_manager_test_mode_change() {
    let (m_display_tester, mut m_calculator_manager) = common_setup();
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "123",
        "",
        &[Command::Command1, Command::Command2, Command::Command3],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "",
        &[Command::ModeScientific],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "123",
        "",
        &[Command::Command1, Command::Command2, Command::Command3],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "",
        &[Command::ModeProgrammer],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "123",
        "",
        &[Command::Command1, Command::Command2, Command::Command3],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "",
        &[Command::ModeScientific],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "67",
        "67 + ",
        &[Command::Command6, Command::Command7, Command::CommandADD],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "",
        &[Command::ModeBasic],
        true,
        false,
    );
    cleanup(&m_display_tester, &mut m_calculator_manager);
}

#[test]
fn calculator_manager_test_programmer() {
    let (m_display_tester, mut m_calculator_manager) = common_setup();
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-18",
        "53 NAND 83 AND ",
        &[
            Command::ModeProgrammer,
            Command::Command5,
            Command::Command3,
            Command::CommandNand,
            Command::Command8,
            Command::Command3,
            Command::CommandAnd,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-120",
        "53 NOR 83 AND ",
        &[
            Command::ModeProgrammer,
            Command::Command5,
            Command::Command3,
            Command::CommandNor,
            Command::Command8,
            Command::Command3,
            Command::CommandAnd,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "10",
        "5 Lsh 1 AND ",
        &[
            Command::ModeProgrammer,
            Command::Command5,
            Command::CommandLSHF,
            Command::Command1,
            Command::CommandAnd,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "2",
        "5 Rsh 1 AND ",
        &[
            Command::ModeProgrammer,
            Command::Command5,
            Command::CommandRSHFL,
            Command::Command1,
            Command::CommandAnd,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-128",
        "-9223372036854775808 Rsh 56 AND ",
        &[
            Command::ModeProgrammer,
            Command::CommandBINPOS63,
            Command::CommandRSHF,
            Command::Command5,
            Command::Command6,
            Command::CommandAnd,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "2",
        "RoL(1)",
        &[
            Command::ModeProgrammer,
            Command::Command1,
            Command::CommandROL,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-9,223,372,036,854,775,808",
        "RoR(1)",
        &[
            Command::ModeProgrammer,
            Command::Command1,
            Command::CommandROR,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "RoR(1)",
        &[
            Command::ModeProgrammer,
            Command::Command1,
            Command::CommandRORC,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-9,223,372,036,854,775,808",
        "RoR(RoR(1))",
        &[
            Command::ModeProgrammer,
            Command::Command1,
            Command::CommandRORC,
            Command::CommandRORC,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "16,843,009",
        "4294967296 \u{00F7} 255=",
        &[
            Command::ModeProgrammer,
            Command::CommandDec,
            Command::Command4,
            Command::Command2,
            Command::Command9,
            Command::Command4,
            Command::Command9,
            Command::Command6,
            Command::Command7,
            Command::Command2,
            Command::Command9,
            Command::Command6,
            Command::CommandDIV,
            Command::Command2,
            Command::Command5,
            Command::Command5,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "16,843,009",
        "4294967303 \u{00F7} 255=",
        &[
            Command::ModeProgrammer,
            Command::CommandDec,
            Command::Command4,
            Command::Command2,
            Command::Command9,
            Command::Command4,
            Command::Command9,
            Command::Command6,
            Command::Command7,
            Command::Command3,
            Command::Command0,
            Command::Command3,
            Command::CommandDIV,
            Command::Command2,
            Command::Command5,
            Command::Command5,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "15,507",
        "1000000000 \u{00F7} 64487=",
        &[
            Command::ModeProgrammer,
            Command::CommandDec,
            Command::Command1,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::CommandDIV,
            Command::Command6,
            Command::Command4,
            Command::Command4,
            Command::Command8,
            Command::Command7,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "15,506",
        "1000000000 \u{00F7} 64488=",
        &[
            Command::ModeProgrammer,
            Command::CommandDec,
            Command::Command1,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::Command0,
            Command::CommandDIV,
            Command::Command6,
            Command::Command4,
            Command::Command4,
            Command::Command8,
            Command::Command8,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    cleanup(&m_display_tester, &mut m_calculator_manager);
}

#[test]
fn calculator_manager_test_standard_order_of_operations() {
    let (m_display_tester, mut m_calculator_manager) = common_setup();
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "1",
        "1/(1)",
        &[Command::Command1, Command::CommandREC],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "2",
        "\u{221A}(4)",
        &[Command::Command4, Command::CommandSQRT],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "2",
        "1 + \u{221A}(4)",
        &[
            Command::Command1,
            Command::CommandADD,
            Command::Command4,
            Command::CommandSQRT,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "3",
        "3 - ",
        &[
            Command::Command1,
            Command::CommandADD,
            Command::Command4,
            Command::CommandSQRT,
            Command::CommandSUB,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0.25",
        "2 \u{00D7} 1/(4)",
        &[
            Command::Command2,
            Command::CommandMUL,
            Command::Command4,
            Command::CommandREC,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0.06",
        "5 \u{00F7} 0.06",
        &[
            Command::Command5,
            Command::CommandDIV,
            Command::Command6,
            Command::CommandPERCENT,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "2",
        "\u{221A}(4) - ",
        &[Command::Command4, Command::CommandSQRT, Command::CommandSUB],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "49",
        "sqr(7) \u{00F7} ",
        &[Command::Command7, Command::CommandSQR, Command::CommandDIV],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "8",
        "\u{221A}(sqr(8))",
        &[Command::Command8, Command::CommandSQR, Command::CommandSQRT],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "12",
        "12 - ",
        &[
            Command::Command1,
            Command::Command0,
            Command::CommandADD,
            Command::Command2,
            Command::CommandSUB,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "12",
        "12 \u{00F7} ",
        &[
            Command::Command3,
            Command::CommandMUL,
            Command::Command4,
            Command::CommandDIV,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "2",
        "2 + ",
        &[
            Command::Command6,
            Command::CommandDIV,
            Command::Command3,
            Command::CommandSUB,
            Command::CommandADD,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "3",
        "3 \u{00D7} ",
        &[
            Command::Command7,
            Command::CommandSUB,
            Command::Command4,
            Command::CommandDIV,
            Command::CommandMUL,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "4",
        "16 + \u{221A}(16)",
        &[
            Command::Command8,
            Command::CommandMUL,
            Command::Command2,
            Command::CommandADD,
            Command::CommandSQRT,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-9",
        "9 \u{00D7} negate(9)",
        &[
            Command::Command9,
            Command::CommandADD,
            Command::Command0,
            Command::CommandMUL,
            Command::CommandSIGN,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "-90",
        "-90 \u{00D7} ",
        &[
            Command::Command9,
            Command::CommandSIGN,
            Command::Command0,
            Command::CommandADD,
            Command::CommandMUL,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "3",
        "1 + 2=",
        &[
            Command::Command1,
            Command::CommandADD,
            Command::Command2,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "40",
        "20 \u{00D7} 2=",
        &[
            Command::Command2,
            Command::Command0,
            Command::CommandMUL,
            Command::Command0,
            Command::Command2,
            Command::CommandEQU,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "3",
        "3 + ",
        &[
            Command::Command1,
            Command::CommandADD,
            Command::Command2,
            Command::CommandADD,
            Command::CommandBACK,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "",
        &[
            Command::Command1,
            Command::CommandADD,
            Command::Command2,
            Command::CommandADD,
            Command::CommandCLEAR,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "3 + ",
        &[
            Command::Command1,
            Command::CommandADD,
            Command::Command2,
            Command::CommandADD,
            Command::CommandCENTR,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "",
        &[
            Command::Command1,
            Command::CommandADD,
            Command::Command2,
            Command::CommandCLEAR,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "0",
        "1 + ",
        &[
            Command::Command1,
            Command::CommandADD,
            Command::Command2,
            Command::CommandCENTR,
        ],
        true,
        false,
    );
    test(
        &m_display_tester,
        &mut m_calculator_manager,
        "120",
        "120 \u{00D7} ",
        &[
            Command::Command1,
            Command::CommandMUL,
            Command::Command2,
            Command::CommandMUL,
            Command::Command3,
            Command::CommandMUL,
            Command::Command4,
            Command::CommandMUL,
            Command::Command5,
            Command::CommandMUL,
        ],
        true,
        false,
    );
    cleanup(&m_display_tester, &mut m_calculator_manager);
}

#[test]
fn calculator_manager_test_memory() {
    let (p_calculator_display, mut m_calculator_manager) = common_setup();
    let scientific_calculator_test52 = [Command::Command1, Command::CommandSTORE];
    let expected_primary_display_test_scientific52 = "1";
    let _expected_expression_display_test_scientific52 = "";

    let scientific_calculator_test53 = [Command::Command1];
    let _expected_primary_display_test_scientific53 = "1";
    let _expected_expression_display_test_scientific53 = "";

    cleanup(&p_calculator_display, &mut m_calculator_manager);
    execute_commands(&mut m_calculator_manager, &scientific_calculator_test52);
    let result_primary = p_calculator_display
        .borrow()
        .get_primary_display()
        .to_string();
    let _result_expression = p_calculator_display.borrow().get_expression().to_string();
    assert_eq!(expected_primary_display_test_scientific52, result_primary);

    cleanup(&p_calculator_display, &mut m_calculator_manager);
    execute_commands(&mut m_calculator_manager, &scientific_calculator_test53);
    m_calculator_manager.memorize_number().unwrap();
    m_calculator_manager
        .send_command(Command::CommandCLEAR)
        .unwrap();
    m_calculator_manager.memorized_number_load(0).unwrap();
    let result_primary = p_calculator_display
        .borrow()
        .get_primary_display()
        .to_string();
    let _result_expression = p_calculator_display.borrow().get_expression().to_string();
    assert_eq!(expected_primary_display_test_scientific52, result_primary);

    cleanup(&p_calculator_display, &mut m_calculator_manager);
    m_calculator_manager
        .send_command(Command::Command1)
        .unwrap();
    m_calculator_manager.memorize_number().unwrap();
    m_calculator_manager
        .send_command(Command::CommandCLEAR)
        .unwrap();
    m_calculator_manager
        .send_command(Command::Command2)
        .unwrap();
    m_calculator_manager.memorize_number().unwrap();
    m_calculator_manager
        .send_command(Command::CommandCLEAR)
        .unwrap();
    m_calculator_manager.memorized_number_load(1).unwrap();
    let result_primary = p_calculator_display
        .borrow()
        .get_primary_display()
        .to_string();
    assert_eq!("1", result_primary);

    m_calculator_manager.memorized_number_load(0).unwrap();
    let result_primary = p_calculator_display
        .borrow()
        .get_primary_display()
        .to_string();
    assert_eq!("2", result_primary);

    cleanup(&p_calculator_display, &mut m_calculator_manager);
    m_calculator_manager
        .send_command(Command::Command1)
        .unwrap();
    m_calculator_manager
        .send_command(Command::CommandSIGN)
        .unwrap();
    m_calculator_manager.memorize_number().unwrap();
    m_calculator_manager
        .send_command(Command::CommandADD)
        .unwrap();
    m_calculator_manager
        .send_command(Command::Command2)
        .unwrap();
    m_calculator_manager
        .send_command(Command::CommandEQU)
        .unwrap();
    m_calculator_manager.memorize_number().unwrap();
    m_calculator_manager
        .send_command(Command::CommandMUL)
        .unwrap();
    m_calculator_manager
        .send_command(Command::Command2)
        .unwrap();
    m_calculator_manager.memorize_number().unwrap();

    let memorized_numbers = p_calculator_display
        .borrow()
        .get_memorized_numbers()
        .to_vec();
    assert!(prefix_equal(&memorized_numbers, &["2", "1", "-1"]));

    m_calculator_manager
        .send_command(Command::CommandCLEAR)
        .unwrap();
    m_calculator_manager
        .send_command(Command::Command2)
        .unwrap();
    m_calculator_manager.memorized_number_add(0).unwrap();
    m_calculator_manager.memorized_number_add(1).unwrap();
    m_calculator_manager.memorized_number_add(2).unwrap();

    let memorized_numbers = p_calculator_display
        .borrow()
        .get_memorized_numbers()
        .to_vec();
    assert!(prefix_equal(&memorized_numbers, &["4", "3", "1"]));

    m_calculator_manager
        .send_command(Command::CommandCLEAR)
        .unwrap();
    m_calculator_manager
        .send_command(Command::Command1)
        .unwrap();
    m_calculator_manager
        .send_command(Command::CommandPNT)
        .unwrap();
    m_calculator_manager
        .send_command(Command::Command5)
        .unwrap();

    m_calculator_manager.memorized_number_subtract(0).unwrap();
    m_calculator_manager.memorized_number_subtract(1).unwrap();
    m_calculator_manager.memorized_number_subtract(2).unwrap();

    let memorized_numbers = p_calculator_display
        .borrow()
        .get_memorized_numbers()
        .to_vec();
    assert!(prefix_equal(&memorized_numbers, &["2.5", "1.5", "-0.5"]));

    // Memorizing 101 numbers, which exceeds the limit.
    cleanup(&p_calculator_display, &mut m_calculator_manager);
    for _ in 0..101 {
        m_calculator_manager
            .send_command(Command::Command1)
            .unwrap();
        m_calculator_manager.memorize_number().unwrap();
    }

    let memorized_numbers = p_calculator_display
        .borrow()
        .get_memorized_numbers()
        .to_vec();
    assert_eq!(100, memorized_numbers.len());

    // Memorizing new number, which should show up at the top of the memory
    m_calculator_manager
        .send_command(Command::Command2)
        .unwrap();
    m_calculator_manager.memorize_number().unwrap();
    let memorized_numbers = p_calculator_display
        .borrow()
        .get_memorized_numbers()
        .to_vec();
    assert_eq!("2", memorized_numbers[0]);

    // Test for trying to memorize invalid value
    m_calculator_manager
        .send_command(Command::Command2)
        .unwrap();
    m_calculator_manager
        .send_command(Command::CommandSIGN)
        .unwrap();
    m_calculator_manager
        .send_command(Command::CommandSQRT)
        .unwrap();
    m_calculator_manager.memorize_number().unwrap();
    cleanup(&p_calculator_display, &mut m_calculator_manager);
}

// Send 12345678910111213 and verify MaxDigitsReached
#[test]
fn calculator_manager_test_max_digits_reached() {
    test_max_digits_reached_scenario("1,234,567,891,011,1213");
}

#[test]
fn calculator_manager_test_max_digits_reached_leading_decimal() {
    test_max_digits_reached_scenario("0.12345678910111213");
}

#[test]
fn calculator_manager_test_max_digits_reached_trailing_decimal() {
    test_max_digits_reached_scenario("123,456,789,101,112.13");
}

#[test]
fn unit_conversion_manager_number_formatting_utils_trim_trailing_zeros() {
    let mut number = String::from("2.1032100000000");
    trim_trailing_zeros(&mut number);
    assert_eq!(number, "2.10321");
    number = String::from("-122.123200");
    trim_trailing_zeros(&mut number);
    assert_eq!(number, "-122.1232");
    number = String::from("0.0001200");
    trim_trailing_zeros(&mut number);
    assert_eq!(number, "0.00012");
    number = String::from("12.000");
    trim_trailing_zeros(&mut number);
    assert_eq!(number, "12");
    number = String::from("-12.00000");
    trim_trailing_zeros(&mut number);
    assert_eq!(number, "-12");
    number = String::from("0.000");
    trim_trailing_zeros(&mut number);
    assert_eq!(number, "0");
    number = String::from("322423");
    trim_trailing_zeros(&mut number);
    assert_eq!(number, "322423");
}

#[test]
fn unit_conversion_manager_number_formatting_utils_get_number_digits() {
    let mut number = "2.10321";
    let mut digits_count = get_number_digits(number);
    assert_eq!(digits_count, 6);
    number = "-122.1232";
    digits_count = get_number_digits(number);
    assert_eq!(digits_count, 7);
    number = "-3432";
    digits_count = get_number_digits(number);
    assert_eq!(digits_count, 4);
    number = "0";
    digits_count = get_number_digits(number);
    assert_eq!(digits_count, 1);
    number = "0.0001223";
    digits_count = get_number_digits(number);
    assert_eq!(digits_count, 8);
}

#[test]
fn unit_conversion_manager_number_formatting_utils_get_number_digits_whole_number_part() {
    let mut digits_count = get_number_digits_whole_number_part(2.10321);
    assert_eq!(digits_count, 1);
    digits_count = get_number_digits_whole_number_part(-122.1232);
    assert_eq!(digits_count, 3);
    digits_count = get_number_digits_whole_number_part(-3432.0);
    assert_eq!(digits_count, 4);
    digits_count = get_number_digits_whole_number_part(0.0);
    assert_eq!(digits_count, 1);
    digits_count = get_number_digits_whole_number_part(324328412837382.0);
    assert_eq!(digits_count, 15);
    #[allow(clippy::excessive_precision)]
    let v = 324328412837382.232213214324234;
    digits_count = get_number_digits_whole_number_part(v);
    assert_eq!(digits_count, 15);
    digits_count = get_number_digits_whole_number_part(0.032);
    assert_eq!(digits_count, 1);
    digits_count = get_number_digits_whole_number_part(0.00000000000000000001);
    assert_eq!(digits_count, 1);
}

#[test]
fn unit_conversion_manager_number_formatting_utils_round_significant_digits() {
    let mut result = round_significant_digits(12.342343242, 3);
    assert_eq!(result, "12.342");
    result = round_significant_digits(12.3429999, 3);
    assert_eq!(result, "12.343");
    result = round_significant_digits(12.342500001, 3);
    assert_eq!(result, "12.343");
    #[allow(clippy::excessive_precision)]
    let v = -2312.1244243346454345;
    result = round_significant_digits(v, 5);
    assert_eq!(result, "-2312.12442");
    result = round_significant_digits(0.3423432423, 5);
    assert_eq!(result, "0.34234");
    result = round_significant_digits(0.3423, 7);
    assert_eq!(result, "0.3423000");
}

#[test]
fn unit_conversion_manager_number_formatting_utils_to_scientific_number() {
    let mut result = to_scientific_number(3423.0);
    assert_eq!(result, "3.423000e+03");
    result = to_scientific_number(-21.0);
    assert_eq!(result, "-2.100000e+01");
    result = to_scientific_number(0.0232);
    assert_eq!(result, "2.320000e-02");
    result = to_scientific_number(-0.00921);
    assert_eq!(result, "-9.210000e-03");
    result = to_scientific_number(2343243345677.0);
    assert_eq!(result, "2.343243e+12");
    result = to_scientific_number(-3432474247332942.0);
    assert_eq!(result, "-3.432474e+15");
    result = to_scientific_number(0.000000003432432);
    assert_eq!(result, "3.432432e-09");
    result = to_scientific_number(-0.000000003432432);
    assert_eq!(result, "-3.432432e-09");
}

#[test]
fn calculator_manager_test_binary_operator_received() {
    let (p_calculator_display, mut m_calculator_manager) = common_setup();

    assert_eq!(
        0,
        p_calculator_display
            .borrow()
            .get_binary_operator_received_call_count()
    );

    m_calculator_manager.set_standard_mode().unwrap();
    execute_commands(
        &mut m_calculator_manager,
        &[Command::Command1, Command::CommandADD],
    );

    let display = p_calculator_display
        .borrow()
        .get_primary_display()
        .to_string();
    assert_eq!("1", display);

    // Verify BinaryOperatorReceived
    assert_eq!(
        1,
        p_calculator_display
            .borrow()
            .get_binary_operator_received_call_count()
    );
    cleanup(&p_calculator_display, &mut m_calculator_manager);
}

#[test]
fn calculator_manager_test_binary_operator_received_multiple() {
    let (p_calculator_display, mut m_calculator_manager) = common_setup();

    assert_eq!(
        0,
        p_calculator_display
            .borrow()
            .get_binary_operator_received_call_count()
    );

    m_calculator_manager.set_standard_mode().unwrap();
    execute_commands(
        &mut m_calculator_manager,
        &[
            Command::Command1,
            Command::CommandADD,
            Command::CommandSUB,
            Command::CommandMUL,
        ],
    );

    let display = p_calculator_display
        .borrow()
        .get_primary_display()
        .to_string();
    assert_eq!("1", display);

    // Verify BinaryOperatorReceived
    assert_eq!(
        3,
        p_calculator_display
            .borrow()
            .get_binary_operator_received_call_count()
    );
    cleanup(&p_calculator_display, &mut m_calculator_manager);
}

#[test]
fn calculator_manager_test_binary_operator_received_long_input() {
    let (p_calculator_display, mut m_calculator_manager) = common_setup();

    assert_eq!(
        0,
        p_calculator_display
            .borrow()
            .get_binary_operator_received_call_count()
    );

    m_calculator_manager.set_standard_mode().unwrap();
    execute_commands(
        &mut m_calculator_manager,
        &[
            Command::Command1,
            Command::CommandADD,
            Command::Command2,
            Command::CommandMUL,
            Command::Command1,
            Command::Command0,
            Command::CommandSUB,
            Command::Command5,
            Command::CommandDIV,
            Command::Command5,
            Command::CommandEQU,
        ],
    );

    let display = p_calculator_display
        .borrow()
        .get_primary_display()
        .to_string();
    assert_eq!("5", display);

    // Verify BinaryOperatorReceived
    assert_eq!(
        4,
        p_calculator_display
            .borrow()
            .get_binary_operator_received_call_count()
    );
    cleanup(&p_calculator_display, &mut m_calculator_manager);
}

/// Not in the C++ suite: the `m_parenDisplay` field is otherwise write-only.
#[test]
fn paren_count_is_reported() {
    let (p_calculator_display, mut m_calculator_manager) = common_setup();
    m_calculator_manager.set_scientific_mode().unwrap();
    execute_commands(
        &mut m_calculator_manager,
        &[Command::CommandOPENP, Command::CommandOPENP],
    );
    assert_eq!(2, p_calculator_display.borrow().m_paren_display);
}
