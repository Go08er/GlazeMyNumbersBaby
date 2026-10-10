// Rust port: GMNB contributors.

//! Extension: memory slots stored without working their number out
//! (`CalculatorManager::memorize_unread` / `memorize_written`, for a
//! restored session) hold what MS would have stored, are worked out by
//! what first reads them, and read the same whenever that is.

use std::cell::RefCell;
use std::rc::Rc;

use calcmanager::*;

/// The display: the primary display and the memory list as last shown, and
/// how often the list was.
#[derive(Default)]
struct Display {
    primary: String,
    is_error: bool,
    memory: Vec<String>,
    memory_shown: usize,
}

impl CalcDisplay for Display {
    fn set_primary_display(&mut self, text: &str, is_error: bool) {
        self.primary = text.to_string();
        self.is_error = is_error;
    }
    fn set_is_in_error(&mut self, is_error: bool) {
        self.is_error = is_error;
    }
    fn set_expression_display(&mut self, _: &[ExpressionToken], _: &[ExpressionCommand]) {}
    fn set_memorized_numbers(&mut self, numbers: &[String]) {
        self.memory = numbers.to_vec();
        self.memory_shown += 1;
    }
    fn set_parenthesis_number(&mut self, _: u32) {}
    fn on_no_right_paren_added(&mut self) {}
    fn on_history_item_added(&mut self, _: u32) {}
    fn max_digits_reached(&mut self) {}
    fn input_changed(&mut self) {}
    fn binary_operator_received(&mut self) {}
    fn memory_item_changed(&mut self, _: u32) {}
}

fn new_manager(mode: Command) -> (Rc<RefCell<Display>>, CalculatorManager) {
    let display = Rc::new(RefCell::new(Display::default()));
    let mut manager =
        CalculatorManager::new(display.clone(), Rc::new(EngineResourceProvider::default()));
    manager.send_command(mode).unwrap();
    (display, manager)
}

/// Presses the keys `text` names: digits (A–F too), `.`, `e` (Exp), `-`
/// (±), `/`, `+`, `=` and `c` (C).
fn press(manager: &mut CalculatorManager, text: &str) {
    for key in text.chars() {
        let command = match key {
            '.' => Command::CommandPNT,
            'e' => Command::CommandEXP,
            '-' => Command::CommandSIGN,
            '/' => Command::CommandDIV,
            '+' => Command::CommandADD,
            '=' => Command::CommandEQU,
            'c' => Command::CommandCLEAR,
            digit => Command(Command::Command0.0 + digit.to_digit(16).unwrap() as i32),
        };
        manager.send_command(command).unwrap();
    }
}

/// How a scenario stores the number it typed.
#[derive(Clone, Copy, Debug)]
enum Store {
    /// MS.
    Ms,
    /// As a restored session does.
    Unread,
}

fn store(manager: &mut CalculatorManager, how: Store) {
    match how {
        Store::Ms => manager.memorize_number().unwrap(),
        Store::Unread => assert!(manager.memorize_unread().unwrap()),
    }
}

/// A session that stores numbers with `store` (clearing each entry after,
/// as a restore does) and goes on.
type Scenario = fn(&mut CalculatorManager, Store);

/// What a scenario leaves: every number the engine holds, the list as it
/// is shown, and what each slot recalls as.
fn outcome(mode: Command, scenario: Scenario, how: Store) -> (String, Vec<String>, Vec<String>) {
    let (display, mut manager) = new_manager(mode);
    scenario(&mut manager, how);
    let state = format!("{:?}", manager.state());
    manager.set_memorized_numbers_string().unwrap();
    let shown = display.borrow().memory.clone();
    let recalled = (0..shown.len() as u32)
        .map(|i| {
            manager.memorized_number_load(i).unwrap();
            let recalled = display.borrow().primary.clone();
            press(&mut manager, "c");
            recalled
        })
        .collect();
    (state, shown, recalled)
}

/// What MS stores of a number being typed is what the slot stored unread
/// reads as, whenever and in whatever mode, radix or word size it is read:
/// a typed number in each mode (its digits past the mode's count refused,
/// its exponent, its sign), in each radix and word size, and the value
/// shown when no number is being typed.
#[test]
fn an_unread_slot_holds_what_ms_stores() {
    let scenarios: [(&str, Command, Scenario); 9] = [
        ("Standard", Command::ModeBasic, |m, how| {
            // 16 digits, the 17th refused.
            for keys in ["12.5", "0.00000000000000012345", "9.9999999999999999e9999"] {
                press(m, keys);
                store(m, how);
                press(m, "c");
            }
            press(m, "5-");
            store(m, how);
            press(m, "c");
        }),
        ("Scientific", Command::ModeScientific, |m, how| {
            // 32 digits, the 33rd refused; a negative exponent; no digit.
            for keys in [
                "9.99999999999999999999999999999999e9999-",
                "1.0000000000000000000000000000001-e9999",
                "123456789012345678901234567890123",
                "e5",
                "",
            ] {
                press(m, keys);
                store(m, how);
                press(m, "c");
            }
        }),
        ("the value shown", Command::ModeScientific, |m, how| {
            press(m, "1/3=");
            store(m, how);
            press(m, "c2");
            m.send_command(Command::CommandFE).unwrap();
            store(m, how);
            press(m, "c");
        }),
        ("read in Standard", Command::ModeScientific, |m, how| {
            press(m, "1.2345678901234567890123456789012e9999-");
            store(m, how);
            press(m, "c");
            m.send_command(Command::ModeBasic).unwrap();
        }),
        ("read in Programmer", Command::ModeScientific, |m, how| {
            for keys in ["300.75", "5-", "1.2345678901234567890123456789012e40"] {
                press(m, keys);
                store(m, how);
                press(m, "c");
            }
            m.send_command(Command::ModeProgrammer).unwrap();
            m.send_command(Command::CommandByte).unwrap();
            m.set_radix(RadixType::Binary).unwrap();
        }),
        ("Programmer", Command::ModeProgrammer, |m, how| {
            // The largest number, the one past it refused; a negative one.
            for keys in ["9223372036854775808", "5-", "0"] {
                press(m, keys);
                store(m, how);
                press(m, "c");
            }
        }),
        ("hexadecimal words", Command::ModeProgrammer, |m, how| {
            m.send_command(Command::CommandWord).unwrap();
            m.set_radix(RadixType::Hex).unwrap();
            for keys in ["FFFF", "7FFF1", "A-"] {
                press(m, keys);
                store(m, how);
                press(m, "c");
            }
        }),
        (
            "bytes read as quadwords",
            Command::ModeProgrammer,
            |m, how| {
                m.send_command(Command::CommandByte).unwrap();
                for keys in ["127", "128", "5-"] {
                    press(m, keys);
                    store(m, how);
                    press(m, "c");
                }
                m.set_radix(RadixType::Binary).unwrap();
                press(m, "10101010");
                store(m, how);
                press(m, "c");
                m.send_command(Command::CommandQword).unwrap();
                m.set_radix(RadixType::Octal).unwrap();
            },
        ),
        (
            "Programmer read in Scientific",
            Command::ModeProgrammer,
            |m, how| {
                m.set_radix(RadixType::Hex).unwrap();
                press(m, "FFFFFFFFFFFFFFFF");
                store(m, how);
                press(m, "c");
                m.send_command(Command::ModeScientific).unwrap();
            },
        ),
    ];
    for (name, mode, scenario) in scenarios {
        let ms = outcome(mode, scenario, Store::Ms);
        let unread = outcome(mode, scenario, Store::Unread);
        assert!(!ms.1.is_empty(), "{name}");
        assert_eq!(ms, unread, "{name}");
    }
}

/// A slot is read by what reads it, and only then: MR reads its slot, a
/// slot cleared isn't read, and what shows the list again (MS, M+, M−, a
/// radix switch) reads them all. Storing one shows nothing.
#[test]
fn an_unread_slot_is_read_by_what_first_reads_it() {
    let (display, mut manager) = new_manager(Command::ModeScientific);
    let shown = display.borrow().memory_shown;
    for keys in ["1", "2", "3", "4", "5"] {
        press(&mut manager, keys);
        assert!(manager.memorize_unread().unwrap());
        press(&mut manager, "c");
    }
    // 10^10000 and 10^-19999, which no number typed is.
    assert!(
        manager
            .memorize_written(false, "1.", false, 10_000)
            .unwrap()
    );
    assert!(manager.memorize_written(true, "9.5", true, 19_999).unwrap());
    assert_eq!(manager.unread_memory(), 7);
    assert_eq!(display.borrow().memory_shown, shown);
    assert_eq!(display.borrow().primary, "0");

    // MR: the slot it recalls.
    manager.memorized_number_load(3).unwrap();
    assert_eq!(display.borrow().primary, "4");
    assert_eq!(manager.unread_memory(), 6);
    manager.memorized_number_load(3).unwrap();
    assert_eq!(manager.unread_memory(), 6);
    assert_eq!(display.borrow().memory_shown, shown);

    // A slot cleared: none.
    manager.memorized_number_clear(2);
    assert_eq!(manager.unread_memory(), 5);
    manager.memorized_number_load(2).unwrap();
    assert_eq!(display.borrow().primary, "4");
    assert_eq!(manager.unread_memory(), 5);

    // M+ on one: the list is shown again, every slot written out.
    press(&mut manager, "c7");
    manager.memorized_number_add(4).unwrap();
    assert_eq!(manager.unread_memory(), 0);
    assert_eq!(
        display.borrow().memory,
        ["-9.5e-19999", "1.e+10000", "4", "3", "9", "1"]
    );

    // And MS, M− and a radix switch.
    type Shows = fn(&mut CalculatorManager);
    let all: [Shows; 3] = [
        |m| m.memorize_number().unwrap(),
        |m| m.memorized_number_subtract(0).unwrap(),
        |m| m.set_radix(RadixType::Decimal).unwrap(),
    ];
    for show in all {
        let (display, mut manager) = new_manager(Command::ModeScientific);
        for keys in ["1.5", "2"] {
            press(&mut manager, keys);
            assert!(manager.memorize_unread().unwrap());
            press(&mut manager, "c");
        }
        press(&mut manager, "4");
        show(&mut manager);
        assert_eq!(manager.unread_memory(), 0);
        let memory = display.borrow().memory.clone();
        assert_eq!(memory[memory.len() - 1], "1.5");
    }

    // MC forgets them unread.
    let (_, mut manager) = manager_with_unread();
    manager.memorized_number_clear_all().unwrap();
    assert_eq!(manager.unread_memory(), 0);
    assert!(manager.memorized_number_load(0).is_err());
}

fn manager_with_unread() -> (Rc<RefCell<Display>>, CalculatorManager) {
    let (display, mut manager) = new_manager(Command::ModeScientific);
    press(&mut manager, "8");
    assert!(manager.memorize_unread().unwrap());
    press(&mut manager, "c");
    (display, manager)
}

/// As MS: nothing is stored in an error, and the list keeps to a hundred
/// slots, the oldest making room.
#[test]
fn unread_slots_are_stored_as_ms_stores() {
    let (display, mut manager) = new_manager(Command::ModeScientific);
    press(&mut manager, "1/0=");
    assert!(display.borrow().is_error);
    assert!(!manager.memorize_unread().unwrap());
    assert!(
        !manager
            .memorize_written(false, "1.", false, 10_000)
            .unwrap()
    );
    press(&mut manager, "c");

    for i in 0..=100 {
        press(&mut manager, &i.to_string());
        assert!(manager.memorize_unread().unwrap());
        press(&mut manager, "c");
    }
    assert_eq!(manager.unread_memory(), 100);
    manager.memorized_number_load(99).unwrap();
    assert_eq!(display.borrow().primary, "1");
    manager.memorized_number_load(0).unwrap();
    assert_eq!(display.borrow().primary, "100");
    assert!(manager.memorized_number_load(100).is_err());

    // Out of a written number's bounds, or its mode.
    assert!(
        !manager
            .memorize_written(false, "1.", false, 20_000)
            .unwrap()
    );
    assert!(
        !manager
            .memorize_written(false, &"1".repeat(33), false, 10_000)
            .unwrap()
    );
    manager.send_command(Command::ModeProgrammer).unwrap();
    assert!(
        !manager
            .memorize_written(false, "1.", false, 10_000)
            .unwrap()
    );
}

/// A slot's number is worked out with the constants as they were when it
/// was written, whatever radix they are in when it is read. They decide
/// how many digits a quotient keeps: read with them in radix 2, a decimal
/// number of 32 digits and a negative exponent would lose its last ten.
/// (The apps show the list, which reads every slot, when the mode changes,
/// before the radix can; a slot must not depend on that.)
#[test]
fn an_unread_slot_reads_the_same_whatever_the_constants() {
    let digits = "1.2345678901234567890123456789012";
    let value = |how: Store, radix: Option<u32>| {
        let (_, mut manager) = new_manager(Command::ModeScientific);
        press(&mut manager, digits);
        press(&mut manager, "e40-");
        store(&mut manager, how);
        press(&mut manager, "c");
        if let Some(radix) = radix {
            ratpack::change_constants(radix, 64);
        }
        let state = manager.state();
        ratpack::change_constants(10, 32);
        state.values.into_iter().last().unwrap().1
    };
    let stored = value(Store::Ms, None);
    for radix in [2, 8, 10, 16] {
        assert!(value(Store::Unread, Some(radix)) == stored, "radix {radix}");
    }

    // What that guards against: the number read with the constants in
    // `radix`, as they are or with the ratio of radix 10.
    let read = |radix, as_in_decimal: bool| {
        ratpack::change_constants(radix, 64);
        let read = || ratpack::string_to_rat(false, digits, true, "40", 10, 32);
        let read = if as_in_decimal {
            ratpack::with_ratio_of(10, read)
        } else {
            read()
        };
        ratpack::change_constants(10, 32);
        read.unwrap().unwrap()
    };
    assert!(read(10, false) == stored);
    assert!(read(2, false) != stored);
    assert!(read(2, true) == stored);
}

/// What a slot costs to work out is bounded by what one may hold, not by
/// a budget: the costliest, 32 digits with an exponent of 19,999, takes
/// under eight million units of work, and a number as typed far less.
#[test]
fn the_costliest_unread_slot_is_bounded() {
    let work = |f: &mut dyn FnMut()| {
        let before = work_done();
        f();
        work_done() - before
    };
    let nines = format!("9.{}", "9".repeat(31));
    for negative_exponent in [false, true] {
        let (_, mut manager) = new_manager(Command::ModeScientific);
        let stored = work(&mut || {
            assert!(
                manager
                    .memorize_written(true, &nines, negative_exponent, MAX_WRITTEN_EXPONENT)
                    .unwrap()
            );
        });
        assert_eq!(stored, 0);
        let read = work(&mut || manager.read_memory());
        assert!((1_000_000..8_000_000).contains(&read), "{read}");
        assert_eq!(manager.unread_memory(), 0);

        let (_, mut manager) = manager_typed(&nines, negative_exponent);
        let read = work(&mut || manager.read_memory());
        assert!((200_000..4_000_000).contains(&read), "{read}");
    }
}

/// A Scientific calculator with `mantissa` and an exponent of 9999 typed
/// and stored unread.
fn manager_typed(
    mantissa: &str,
    negative_exponent: bool,
) -> (Rc<RefCell<Display>>, CalculatorManager) {
    let (display, mut manager) = new_manager(Command::ModeScientific);
    press(&mut manager, mantissa);
    press(
        &mut manager,
        if negative_exponent { "e9999-" } else { "e9999" },
    );
    let before = work_done();
    assert!(manager.memorize_unread().unwrap());
    assert_eq!(work_done(), before);
    press(&mut manager, "c");
    (display, manager)
}
