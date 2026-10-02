// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Port of CalculatorUnitTests/UnitConverterTest.cpp.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use unitconv::converter::{
    Category, Command, ConversionData, ConverterDataLoader, SuggestedValue, Unit, UnitConverter,
    UnitConverterVmCallback, UnitRatios,
};

fn length() -> Category {
    Category::new(1, "Length", true)
}
fn weight() -> Category {
    Category::new(2, "Weight", false)
}
fn inches() -> Unit {
    Unit::new(1, "Inches", "In", true, true, false)
}
fn feet() -> Unit {
    Unit::new(2, "Feet", "Ft", false, false, false)
}
fn pounds() -> Unit {
    Unit::new(3, "Pounds", "Lb", true, true, false)
}
fn kilograms() -> Unit {
    Unit::new(4, "Kilograms", "Kg", false, false, false)
}

struct TestUnitConverterConfigLoader {
    load_data_call_count: Arc<AtomicU32>,
}

impl ConverterDataLoader for TestUnitConverterConfigLoader {
    fn load_data(&mut self) {
        self.load_data_call_count.fetch_add(1, Ordering::SeqCst);
    }

    fn get_ordered_categories(&self) -> Vec<Category> {
        vec![length(), weight()]
    }

    fn get_ordered_units(&self, category: &Category) -> Vec<Unit> {
        match category.id {
            1 => vec![inches(), feet()],
            2 => vec![pounds(), kilograms()],
            _ => Vec::new(),
        }
    }

    #[allow(clippy::excessive_precision)] // verbatim from UnitConverterTest.cpp
    fn load_ordered_ratios(&self, unit: &Unit) -> UnitRatios {
        let conversion1 = ConversionData::new(1.0, 0.0, false);
        let conversion2 = ConversionData::new(0.08333333333333333333333333333333, 0.0, false);
        let conversion3 = ConversionData::new(12.0, 0.0, false);
        let conversion4 = ConversionData::new(0.453592, 0.0, false);
        let conversion5 = ConversionData::new(2.20462, 0.0, false);
        match unit.id {
            1 => vec![(inches(), conversion1), (feet(), conversion2)],
            2 => vec![(inches(), conversion3), (feet(), conversion1)],
            3 => vec![(pounds(), conversion1), (kilograms(), conversion4)],
            4 => vec![(pounds(), conversion5), (kilograms(), conversion1)],
            _ => Vec::new(),
        }
    }

    fn supports_category(&self, _target: &Category) -> bool {
        true
    }
}

#[derive(Default)]
struct TestUnitConverterVmCallback {
    last_from: String,
    last_to: String,
    last_suggested: Vec<SuggestedValue>,
    max_digits_reached_call_count: i32,
}

impl UnitConverterVmCallback for TestUnitConverterVmCallback {
    fn display_callback(&mut self, from: &str, to: &str) {
        self.last_from = from.into();
        self.last_to = to.into();
    }

    fn suggested_value_callback(&mut self, suggested_values: &[SuggestedValue]) {
        self.last_suggested = suggested_values.to_vec();
    }

    fn max_digits_reached(&mut self) {
        self.max_digits_reached_call_count += 1;
    }
}

struct Fixture {
    converter: UnitConverter,
    callback: Arc<Mutex<TestUnitConverterVmCallback>>,
    load_count: Arc<AtomicU32>,
}

impl Fixture {
    fn new() -> Self {
        let load_count = Arc::new(AtomicU32::new(0));
        let mut converter = UnitConverter::new(Box::new(TestUnitConverterConfigLoader {
            load_data_call_count: load_count.clone(),
        }));
        let callback = Arc::new(Mutex::new(TestUnitConverterVmCallback::default()));
        converter.set_view_model_callback(Some(callback.clone()));
        Fixture {
            converter,
            callback,
            load_count,
        }
    }

    fn send(&mut self, command: Command) {
        self.converter.send_command(command);
    }

    fn execute_commands(&mut self, commands: &[Command]) {
        for &command in commands {
            if command == Command::None {
                break;
            }
            self.send(command);
        }
    }

    fn check_display_values(&self, from: &str, to: &str) {
        let cb = self.callback.lock().unwrap();
        assert_eq!((cb.last_from.as_str(), cb.last_to.as_str()), (from, to));
    }

    fn check_suggested_values(&self, expected: &[(&str, Unit)]) {
        let cb = self.callback.lock().unwrap();
        let actual: Vec<(&str, i32)> = cb
            .last_suggested
            .iter()
            .map(|(v, u)| (v.as_str(), u.id))
            .collect();
        let expected: Vec<(&str, i32)> = expected.iter().map(|(v, u)| (*v, u.id)).collect();
        assert_eq!(actual, expected);
    }

    fn max_digits_reached_call_count(&self) -> i32 {
        self.callback.lock().unwrap().max_digits_reached_call_count
    }
}

// Test ctor/initialization states
#[test]
fn unit_converter_test_init() {
    let mut f = Fixture::new();
    assert_eq!(0, f.load_count.load(Ordering::SeqCst)); // shouldn't have initialized the loader yet
    f.converter.initialize();
    assert_eq!(1, f.load_count.load(Ordering::SeqCst)); // now we should have loaded
}

// Verify a basic input command stream.'3', '2', '.', '0'
#[test]
fn unit_converter_test_basic() {
    let mut f = Fixture::new();
    f.send(Command::Three);
    f.check_display_values("3", "3");
    f.check_suggested_values(&[("0.25", feet())]);
    f.send(Command::Zero);
    f.check_display_values("30", "30");
    f.check_suggested_values(&[("2.5", feet())]);
    f.send(Command::Decimal);
    f.check_display_values("30.", "30");
    f.check_suggested_values(&[("2.5", feet())]);
    f.send(Command::Zero);
    f.check_display_values("30.0", "30");
    f.check_suggested_values(&[("2.5", feet())]);
}

// Verify a basic copy paste steam. '20.43' with backspace button pressed
#[test]
fn unit_converter_test_backspace_basic() {
    let mut f = Fixture::new();
    f.send(Command::Two);
    f.send(Command::Zero);
    f.send(Command::Decimal);
    f.send(Command::Four);
    f.send(Command::Three);
    f.send(Command::Backspace);

    f.check_display_values("20.4", "20.4");
    f.send(Command::Backspace);
    f.check_display_values("20.", "20");
    f.send(Command::Backspace);
    f.check_display_values("20", "20");
    f.send(Command::Backspace);
    f.check_display_values("2", "2");
    f.send(Command::Backspace);
    f.check_display_values("0", "0");
}

// Verify a basic copy paste steam. '20.43' with clear button pressed
#[test]
fn unit_converter_test_clear() {
    let mut f = Fixture::new();
    f.send(Command::Two);
    f.send(Command::Zero);
    f.send(Command::Decimal);
    f.send(Command::Four);
    f.send(Command::Three);
    f.send(Command::Clear);

    f.check_display_values("0", "0");
}

// Check the getter functions
#[test]
fn unit_converter_test_getters() {
    let mut f = Fixture::new();
    assert_eq!(f.converter.get_categories(), vec![length(), weight()]);
    assert_eq!(
        f.converter.set_current_category(&length()).0,
        vec![inches(), feet()]
    );
}

// Test getting category after it has been set.
#[test]
fn unit_converter_test_get_category() {
    let mut f = Fixture::new();
    f.converter.set_current_category(&weight());
    assert_eq!(f.converter.get_current_category(), weight());
}

// Test switching of unit types
#[test]
fn unit_converter_test_unit_type_switching() {
    let mut f = Fixture::new();
    // Enter 57 into the from field, then switch focus to the to field (making it the new from field)
    f.send(Command::Five);
    f.send(Command::Seven);
    f.converter.switch_active("57");
    // Now set unit conversion to go from kilograms to pounds
    f.converter.set_current_category(&weight());
    f.converter.set_current_unit_types(&kilograms(), &pounds());
    f.send(Command::Five);
    f.check_display_values("5", "11.0231");
    f.check_suggested_values(&[]);
}

// Test input escaping
#[test]
fn unit_converter_test_quote() {
    let input1 = "Weight";
    let output1 = "Weight";
    let input2 = "{p}Weig;[ht|";
    let output2 = "{lb}p{rb}Weig{sc}{lc}ht{p}";
    let input3 = "{{{t;s}}},:]";
    let output3 = "{lb}{lb}{lb}t{sc}s{rb}{rb}{rb}{cm}{co}{rc}";
    assert_eq!(UnitConverter::quote(input1), output1);
    assert_eq!(UnitConverter::quote(input2), output2);
    assert_eq!(UnitConverter::quote(input3), output3);
}

// Test output unescaping
#[test]
fn unit_converter_test_unquote() {
    let input1 = "Weight";
    let input2 = "{p}Weig;[ht|";
    let input3 = "{{{t;s}}},:]";
    assert_eq!(UnitConverter::unquote(input1), input1);
    assert_eq!(
        UnitConverter::unquote(&UnitConverter::quote(input1)),
        input1
    );
    assert_eq!(
        UnitConverter::unquote(&UnitConverter::quote(input2)),
        input2
    );
    assert_eq!(
        UnitConverter::unquote(&UnitConverter::quote(input3)),
        input3
    );
}

// Test backspace commands
#[test]
fn unit_converter_test_backspace() {
    let mut f = Fixture::new();
    f.converter.set_current_category(&weight());
    f.converter.set_current_unit_types(&pounds(), &pounds());
    f.send(Command::Three);
    f.send(Command::Zero);
    f.send(Command::Decimal);
    f.send(Command::One);
    f.send(Command::Two);
    f.check_display_values("30.12", "30.12");
    f.check_suggested_values(&[("13.66", kilograms())]);
    f.send(Command::Backspace);
    f.check_display_values("30.1", "30.1");
    f.check_suggested_values(&[("13.65", kilograms())]);
    f.send(Command::Backspace);
    f.check_display_values("30.", "30");
    f.check_suggested_values(&[("13.61", kilograms())]);
    f.send(Command::Backspace);
    f.check_display_values("30", "30");
    f.check_suggested_values(&[("13.61", kilograms())]);
    f.send(Command::Backspace);
    f.check_display_values("3", "3");
    f.check_suggested_values(&[("1.36", kilograms())]);
    f.send(Command::Backspace);
    f.check_display_values("0", "0");
    f.check_suggested_values(&[]);
}

// Test large values
#[test]
fn unit_converter_test_scientific_inputs() {
    let mut f = Fixture::new();
    f.converter.set_current_category(&weight());
    f.converter.set_current_unit_types(&pounds(), &kilograms());
    f.send(Command::Decimal);
    for _ in 0..13 {
        f.send(Command::Zero);
    }
    f.send(Command::One);
    f.check_display_values("0.00000000000001", "4.535920e-15");
    f.converter.switch_active("4.535920e-15");
    for _ in 0..16 {
        f.send(Command::Nine);
    }
    f.check_display_values("999999999999999", "2.204620e+15");
    f.converter.switch_active("2.20463e+15");
    for c in [
        Command::One,
        Command::Two,
        Command::Three,
        Command::Four,
        Command::Five,
        Command::Six,
        Command::Seven,
    ] {
        f.send(c);
    }
    f.check_display_values("1234567", "559989.7");
    f.converter.switch_active("559989.7");
    for c in [
        Command::One,
        Command::Two,
        Command::Three,
        Command::Four,
        Command::Five,
        Command::Six,
        Command::Seven,
        Command::Eight,
    ] {
        f.send(c);
    }
    f.check_display_values("12345678", "27217529");
}

// Test large values
#[test]
fn unit_converter_test_supplementary_result_rounding() {
    let mut f = Fixture::new();
    f.send(Command::Three);
    f.send(Command::Three);
    f.send(Command::Three);
    f.check_suggested_values(&[("27.75", feet())]);
    f.send(Command::Three);
    f.check_suggested_values(&[("277.8", feet())]);
    f.send(Command::Three);
    f.check_suggested_values(&[("2778", feet())]);
}

const FIFTEEN_DIGITS: [Command; 15] = [
    Command::One,
    Command::Two,
    Command::Three,
    Command::Four,
    Command::Five,
    Command::Six,
    Command::Seven,
    Command::Eight,
    Command::Nine,
    Command::One,
    Command::Zero,
    Command::One,
    Command::One,
    Command::One,
    Command::Two,
];

#[test]
fn unit_converter_test_max_digits_reached() {
    let mut f = Fixture::new();
    f.execute_commands(&FIFTEEN_DIGITS);

    assert_eq!(0, f.max_digits_reached_call_count());

    f.execute_commands(&[Command::One]);

    assert_eq!(1, f.max_digits_reached_call_count());
}

#[test]
fn unit_converter_test_max_digits_reached_leading_decimal() {
    let mut f = Fixture::new();
    f.execute_commands(&[
        Command::Zero,
        Command::Decimal,
        Command::One,
        Command::Two,
        Command::Three,
        Command::Four,
        Command::Five,
        Command::Six,
        Command::Seven,
        Command::Eight,
        Command::Nine,
        Command::One,
        Command::Zero,
        Command::One,
        Command::One,
        Command::One,
    ]);

    assert_eq!(0, f.max_digits_reached_call_count());

    f.execute_commands(&[Command::Two]);

    assert_eq!(1, f.max_digits_reached_call_count());
}

#[test]
fn unit_converter_test_max_digits_reached_trailing_decimal() {
    let mut f = Fixture::new();
    f.execute_commands(&FIFTEEN_DIGITS);
    f.execute_commands(&[Command::Decimal]);

    assert_eq!(0, f.max_digits_reached_call_count());

    f.execute_commands(&[Command::One]);

    assert_eq!(1, f.max_digits_reached_call_count());
}

#[test]
fn unit_converter_test_max_digits_reached_multiple_times() {
    let mut f = Fixture::new();
    f.execute_commands(&FIFTEEN_DIGITS);

    assert_eq!(0, f.max_digits_reached_call_count());

    for count in 1..=10 {
        f.execute_commands(&[Command::Three]);

        assert_eq!(count, f.max_digits_reached_call_count(), "{count}");
    }
}

// ---------------------------------------------------------------------------
// Additional engine tests (not in the original suite)

#[test]
fn reset_restores_default_units_and_value() {
    let mut f = Fixture::new();
    f.converter.set_current_category(&weight());
    f.converter.set_current_unit_types(&kilograms(), &pounds());
    f.send(Command::Five);
    f.send(Command::Reset);
    f.check_display_values("0", "0");
    assert_eq!(f.converter.get_current_category(), length());
    assert_eq!(f.converter.from_type(), &inches());
    assert_eq!(f.converter.to_type(), &inches());
    assert!(!f.converter.is_switched_active());
}

#[test]
fn negate_only_in_categories_supporting_negative_values() {
    let mut f = Fixture::new();
    f.send(Command::Five);
    f.send(Command::Negate);
    f.check_display_values("-5", "-5");
    f.check_suggested_values(&[("-0.42", feet())]);
    f.send(Command::Backspace);
    f.check_display_values("0", "0");

    // Switching to a category without negative values strips the sign.
    f.send(Command::Five);
    f.send(Command::Negate);
    f.converter.set_current_category(&weight());
    assert_eq!(f.converter.current_display(), "5");
    f.send(Command::Negate);
    assert_eq!(f.converter.current_display(), "5");
}

#[test]
fn switch_active_swaps_units_and_replaces_value_on_next_digit() {
    let mut f = Fixture::new();
    f.converter.set_current_category(&weight());
    f.converter.set_current_unit_types(&pounds(), &kilograms());
    f.send(Command::One);
    f.send(Command::Zero);
    f.check_display_values("10", "4.53592");
    f.converter.switch_active("4.53592");
    assert!(f.converter.is_switched_active());
    assert_eq!(f.converter.from_type(), &kilograms());
    assert_eq!(f.converter.to_type(), &pounds());
    // Negate does not end the "switched" state; any other command clears.
    f.send(Command::Two);
    f.check_display_values("2", "4.40924");
}

#[test]
fn user_preferences_round_trip() {
    let mut f = Fixture::new();
    f.converter.set_current_category(&weight());
    f.converter.set_current_unit_types(&kilograms(), &pounds());
    let saved = f.converter.save_user_preferences();
    assert_eq!(
        saved,
        "4;Kilograms;Kg;0;0;0;|3;Pounds;Lb;1;1;0;|2;0;Weight;|"
    );

    let mut g = Fixture::new();
    g.converter.restore_user_preferences(&saved);
    assert_eq!(g.converter.get_current_category(), weight());
    assert_eq!(g.converter.from_type(), &kilograms());
    assert_eq!(g.converter.to_type(), &pounds());

    // Garbage is ignored.
    let mut h = Fixture::new();
    h.converter.restore_user_preferences("nonsense|x|");
    h.converter.restore_user_preferences("");
    assert_eq!(h.converter.get_current_category(), length());
}

#[test]
fn none_command_is_ignored() {
    let mut f = Fixture::new();
    f.send(Command::None);
    assert_eq!(f.converter.current_display(), "0");
    f.send(Command::Seven);
    f.send(Command::None);
    f.check_display_values("7", "7");
}
