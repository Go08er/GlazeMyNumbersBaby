// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Port of Calculator.Tests/UnitConverterViewModelTests.cs (the parts that are
// meaningful outside XAML) plus view model tests for the Rust port.

mod common;

use chrono::TimeDelta;
use common::*;
use unitconv::currency::{CurrencyDataSource, CurrencyError, NetworkAccessBehavior};
use unitconv::data_loader::unit_ids::*;
use unitconv::view_model::PASTE_ERROR_STRING;
use unitconv::{
    CategoryInfo, Command, ConverterMode, ConverterPreferences, CurrencyStatus,
    SupplementaryResult, UnitConverterViewModel, UnitInfo, ViewModelConfig,
};

fn vm() -> UnitConverterViewModel {
    UnitConverterViewModel::new(ViewModelConfig {
        fallback_snapshot: Some(fixture_snapshot()),
        clock: Some(clock_after_fixture(TimeDelta::hours(1))),
        ..Default::default()
    })
}

fn currency_vm() -> UnitConverterViewModel {
    let mut vm = vm();
    vm.set_current_mode(ConverterMode::Currency);
    vm
}

fn press(vm: &mut UnitConverterViewModel, keys: &str) {
    for c in keys.chars() {
        let command = match c {
            '.' => Command::Decimal,
            '-' => Command::Negate,
            '<' => Command::Backspace,
            'C' => Command::Clear,
            d => Command::from_digit(d.to_digit(10).unwrap()).unwrap(),
        };
        vm.button_pressed(command);
    }
}

fn unit_by_abbreviation(vm: &UnitConverterViewModel, abbreviation: &str) -> i32 {
    vm.units()
        .iter()
        .find(|u| u.abbreviation == abbreviation)
        .unwrap_or_else(|| panic!("{abbreviation}"))
        .id
}

fn select_negatable_category(vm: &mut UnitConverterViewModel) {
    let id = vm
        .categories()
        .iter()
        .find(|c| c.negate_visible())
        .unwrap()
        .id;
    vm.set_current_category(id);
}

// --- CategoryViewModelTests / UnitViewModelTests / SupplementaryResultsViewModelTests

#[test]
fn category_view_model() {
    let category = CategoryInfo {
        id: 3,
        name: "Length".into(),
        supports_negative: false,
    };
    assert_eq!(category.name, "Length");
    assert_eq!(category.id, 3);
    assert!(!category.negate_visible());
    let temperature = CategoryInfo {
        id: 7,
        name: "Temperature".into(),
        supports_negative: true,
    };
    assert!(temperature.negate_visible());
    assert_eq!(temperature.mode(), Some(ConverterMode::Temperature));
}

#[test]
fn unit_view_model() {
    let unit = UnitInfo {
        id: 11,
        name: "Centimeters".into(),
        accessible_name: "centimeters".into(),
        abbreviation: "cm".into(),
        is_whimsical: false,
    };
    assert_eq!(unit.name, "Centimeters");
    assert_eq!(unit.abbreviation, "cm");
    assert_eq!(unit.accessible_name, "centimeters");
    assert_eq!(unit.to_string(), "centimeters");
}

#[test]
fn supplementary_result_view_model() {
    let unit = UnitInfo {
        id: 11,
        name: "Centimeters".into(),
        accessible_name: "centimeters".into(),
        abbreviation: "cm".into(),
        is_whimsical: false,
    };
    let plain = SupplementaryResult {
        value: "3.5".into(),
        unit: unit.clone(),
    };
    assert_eq!(plain.value, "3.5");
    assert_eq!(plain.unit, unit);
    assert_eq!(plain.localized_automation_name(), "3.5 Centimeters");
    assert!(!plain.is_whimsical());
    let whimsical = SupplementaryResult {
        value: "2".into(),
        unit: UnitInfo {
            id: 90,
            name: "Jumbo Jets".into(),
            accessible_name: "jumbo jets".into(),
            abbreviation: "jj".into(),
            is_whimsical: true,
        },
    };
    assert!(whimsical.is_whimsical());
}

// --- UnitConverterViewModelTests

#[test]
fn entering_value_after_switching_active_updates_second_value() {
    let mut vm = vm();
    vm.switch_active();
    press(&mut vm, "7");
    assert_eq!(vm.value2(), "7");
}

#[test]
fn max_digits_reached_is_reported() {
    let mut vm = vm();
    press(&mut vm, "123456789012345");
    assert_eq!(vm.max_digits_reached_count(), 0);
    press(&mut vm, "6");
    assert_eq!(vm.max_digits_reached_count(), 1);
    assert_eq!(vm.value1(), "123,456,789,012,345");
}

#[test]
fn conversion_result_narration_substitutes_all_placeholders() {
    let mut vm = vm();
    vm.set_current_mode(ConverterMode::Length);
    press(&mut vm, "5");
    let result = vm.conversion_result_text();
    for placeholder in ["%1", "%2", "%3", "%4"] {
        assert!(!result.contains(placeholder));
    }
    assert_eq!(result, "5 Centimeters is 1.968504 Inches");
}

#[test]
fn switching_active_value_swaps_the_from_and_to_automation_formats() {
    let mut vm = vm();
    vm.set_current_mode(ConverterMode::Length);
    press(&mut vm, "5");
    assert_eq!(vm.value1_automation_name(), "Convert from 5 Centimeters");
    assert_eq!(vm.value2_automation_name(), "Converts into 1.968504 Inches");

    vm.switch_active();
    assert_eq!(vm.value1_automation_name(), "Converts into 5 Centimeters");
    assert_eq!(vm.value2_automation_name(), "Convert from 1.968504 Inches");
}

#[test]
fn pasting_a_minus_after_digits_does_not_negate_the_value() {
    let mut vm = vm();
    select_negatable_category(&mut vm);
    vm.paste("5-3");
    assert_eq!(vm.value1(), "53");
}

#[test]
fn pasting_a_leading_minus_negates_the_value() {
    let mut vm = vm();
    select_negatable_category(&mut vm);
    vm.paste("-53");
    assert_eq!(vm.value1(), "-53");
}

#[test]
fn rejected_paste_says_why_instead_of_blanking_the_display() {
    let mut vm = vm();
    select_negatable_category(&mut vm);
    vm.paste("53");
    assert_eq!(vm.value1(), "53");

    vm.paste(PASTE_ERROR_STRING);

    assert!(!vm.value1().is_empty());
    assert_eq!(vm.value1(), vm.value2());
    assert_eq!(vm.value1(), "Invalid input");

    // Text without a single usable character is rejected too.
    vm.paste("53");
    vm.paste("abc");
    assert_eq!(vm.value1(), "Invalid input");
}

#[test]
fn large_values_are_displayed_with_group_separators() {
    let mut vm = vm();
    press(&mut vm, "1234567");
    assert!(vm.value1().contains(','));
    assert_eq!(vm.value1(), "1,234,567");
}

#[test]
fn length_suggestions_preserve_whimsical_unit_metadata() {
    let mut vm = vm();
    vm.set_current_mode(ConverterMode::Length);
    let centimeters = unit_by_abbreviation(&vm, "cm");
    let inches = unit_by_abbreviation(&vm, "in");
    vm.set_unit1(centimeters);
    vm.set_unit2(inches);

    press(&mut vm, "C47");

    let result = vm.supplementary_results().last().unwrap();
    assert_eq!(result.unit.name, "hands");
    assert_eq!(result.unit.abbreviation, "hands");
    assert!(result.is_whimsical());
}

#[test]
fn construction_and_category_switching_keep_the_converter_consistent() {
    let mut vm = vm();
    assert!(!vm.categories().is_empty());
    assert!(!vm.units().is_empty());
    assert!(vm.current_category().is_some());
    assert!(vm.unit1().is_some());
    assert!(vm.unit2().is_some());
    assert!(vm.value1_active() ^ vm.value2_active());

    // The first category with units (Volume) is current after construction.
    let original = vm.current_category().unwrap().clone();
    assert_eq!(original.mode(), Some(ConverterMode::Volume));
    let original_units: Vec<i32> = vm.units().iter().map(|u| u.id).collect();
    let other = vm
        .categories()
        .iter()
        .find(|c| c.id != original.id)
        .unwrap()
        .id;

    vm.set_current_category(other);
    let units: Vec<i32> = vm.units().iter().map(|u| u.id).collect();
    assert_ne!(original_units, units);
    assert!(units.contains(&vm.unit1().unwrap().id));
    assert!(units.contains(&vm.unit2().unwrap().id));

    vm.set_current_category(original.id);
    let units: Vec<i32> = vm.units().iter().map(|u| u.id).collect();
    assert_eq!(original_units, units);
}

#[test]
fn category_switch_publishes_units_containing_the_selection() {
    let mut vm = vm();
    for category in [
        ConverterMode::Weight,
        ConverterMode::Temperature,
        ConverterMode::Data,
        ConverterMode::Currency,
    ] {
        vm.set_current_mode(category);
        let units: Vec<i32> = vm.units().iter().map(|u| u.id).collect();
        assert!(units.contains(&vm.unit1().unwrap().id), "{category:?}");
        assert!(units.contains(&vm.unit2().unwrap().id), "{category:?}");
        assert!(vm.is_drop_down_enabled());
    }
}

#[test]
fn input_follows_the_active_value_and_is_formatted_for_display() {
    let mut vm = vm();
    let first_was_active = vm.value1_active();

    press(&mut vm, "1.");
    assert!(vm.value1().ends_with('.'));
    press(&mut vm, "5");
    assert_eq!(vm.value1(), "1.5");

    vm.switch_active();
    assert_ne!(first_was_active, vm.value1_active());
    assert!(vm.value1_active() ^ vm.value2_active());

    press(&mut vm, "8");
    assert_eq!(vm.value2(), "8");

    vm.switch_active();
    assert_eq!(first_was_active, vm.value1_active());
    assert!(vm.value1_active() ^ vm.value2_active());

    let name = vm.value1_automation_name();
    assert!(!name.is_empty());
    assert!(name.contains(&vm.unit1().unwrap().accessible_name));
}

#[test]
fn activating_a_field_switches_only_when_needed() {
    let mut vm = vm();
    vm.activate_value1();
    assert!(vm.value1_active());
    vm.activate_value2();
    assert!(vm.value2_active());
    vm.activate_value2();
    assert!(vm.value2_active());
    vm.activate_value1();
    assert!(vm.value1_active());
}

#[test]
fn partial_display_values() {
    let mut vm = vm();
    select_negatable_category(&mut vm);
    press(&mut vm, "-");
    // Negating zero shows a signed zero, typing then fills in digits.
    assert_eq!(vm.value1(), "-0");
    press(&mut vm, "5");
    assert_eq!(vm.value1(), "-5");
    press(&mut vm, ".");
    assert_eq!(vm.value1(), "-5.");
    press(&mut vm, "0");
    assert_eq!(vm.value1(), "-5.0");
}

#[test]
fn values_are_kept_when_switching_categories() {
    let mut vm = vm();
    vm.set_current_mode(ConverterMode::Temperature);
    press(&mut vm, "40-");
    assert_eq!((vm.value1(), vm.value2()), ("-40", "-40"));
    // Length has no negative values: the sign is dropped.
    vm.set_current_mode(ConverterMode::Length);
    assert_eq!(vm.value1(), "40");
    assert_eq!(vm.value2(), "15.74803");
}

#[test]
fn swap_units() {
    let mut vm = vm();
    vm.set_current_mode(ConverterMode::Length);
    vm.set_unit1(LENGTH_METER);
    vm.set_unit2(LENGTH_FOOT);
    press(&mut vm, "2");
    assert_eq!(vm.value2(), "6.56168");
    vm.swap_units();
    assert_eq!(vm.unit1().unwrap().id, LENGTH_FOOT);
    assert_eq!(vm.unit2().unwrap().id, LENGTH_METER);
    assert_eq!((vm.value1(), vm.value2()), ("2", "0.6096"));
    // Like any unit change, the next digit starts a new value.
    press(&mut vm, "3");
    assert_eq!((vm.value1(), vm.value2()), ("3", "0.9144"));
}

#[test]
fn backspace_and_clear() {
    let mut vm = vm();
    press(&mut vm, "12.5");
    press(&mut vm, "<");
    assert_eq!(vm.value1(), "12.");
    press(&mut vm, "<<");
    assert_eq!(vm.value1(), "1");
    press(&mut vm, "<");
    assert_eq!(vm.value1(), "0");
    press(&mut vm, "99C");
    assert_eq!((vm.value1(), vm.value2()), ("0", "0"));

    // Escape closes an open unit picker instead of clearing.
    press(&mut vm, "7");
    vm.set_drop_down_open(true);
    press(&mut vm, "C");
    assert_eq!(vm.value1(), "7");
    vm.set_drop_down_open(false);
    press(&mut vm, "C");
    assert_eq!(vm.value1(), "0");
}

#[test]
fn scientific_results_are_localized() {
    let mut vm = vm();
    vm.set_current_mode(ConverterMode::Energy);
    vm.set_unit1(ENERGY_ELECTRON_VOLT);
    vm.set_unit2(ENERGY_KILOWATTHOUR);
    press(&mut vm, "1");
    assert_eq!(vm.value2(), "4.450490e-26");
}

#[test]
fn preferences_round_trip() {
    let mut vm = vm();
    vm.set_current_mode(ConverterMode::Weight);
    vm.set_unit1(WEIGHT_STONE);
    vm.set_unit2(WEIGHT_GRAM);
    vm.set_current_mode(ConverterMode::Currency);
    let eur = unit_by_abbreviation(&vm, "EUR");
    let jpy = unit_by_abbreviation(&vm, "JPY");
    vm.set_unit1(eur);
    vm.set_unit2(jpy);
    let preferences = vm.preferences().clone();
    assert_eq!(preferences.currency_from.as_deref(), Some("EUR"));
    assert_eq!(preferences.currency_to.as_deref(), Some("JPY"));
    assert!(
        preferences
            .unit_converter
            .as_deref()
            .unwrap()
            .contains("Weight and mass")
    );

    // Serializable for the GUI's settings file.
    let json = serde_json::to_string(&preferences).unwrap();
    let restored: ConverterPreferences = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, preferences);

    let mut again = UnitConverterViewModel::new(ViewModelConfig {
        preferences: restored,
        fallback_snapshot: Some(fixture_snapshot()),
        ..Default::default()
    });
    assert_eq!(again.current_mode(), Some(ConverterMode::Weight));
    assert_eq!(again.unit1().unwrap().id, WEIGHT_STONE);
    assert_eq!(again.unit2().unwrap().id, WEIGHT_GRAM);
    again.set_current_mode(ConverterMode::Currency);
    assert_eq!(again.unit1().unwrap().abbreviation, "EUR");
    assert_eq!(again.unit2().unwrap().abbreviation, "JPY");
}

#[test]
fn reset_view_restores_defaults() {
    let mut vm = vm();
    vm.set_current_mode(ConverterMode::Length);
    vm.set_unit1(LENGTH_MILE);
    press(&mut vm, "42");
    vm.reset_view();
    assert_eq!(vm.value1(), "0");
    assert_eq!(vm.current_mode(), Some(ConverterMode::Length));
    assert_eq!(vm.unit1().unwrap().abbreviation, "cm");
    assert_eq!(vm.unit2().unwrap().abbreviation, "in");
}

#[test]
fn custom_number_format() {
    let mut vm = UnitConverterViewModel::new(ViewModelConfig {
        number_format: unitconv::NumberFormat {
            decimal_separator: ',',
            grouping_separator: ".".into(),
        },
        fallback_snapshot: Some(fixture_snapshot()),
        ..Default::default()
    });
    vm.set_current_mode(ConverterMode::Length);
    vm.set_unit1(LENGTH_KILOMETER);
    vm.set_unit2(LENGTH_METER);
    press(&mut vm, "1234.5");
    assert_eq!(vm.value1(), "1.234,5");
    assert_eq!(vm.value2(), "1.234.500");
    vm.paste("2,5");
    assert_eq!(vm.value1(), "2,5");
    assert_eq!(vm.value2(), "2.500");
}

// --- Currency

#[test]
fn currency_defaults_symbols_ratio_and_timestamp() {
    let vm = currency_vm();
    assert!(vm.is_currency_current_category());
    assert!(vm.is_currency_data_loaded());
    assert!(!vm.is_currency_loading_visible());
    assert_eq!(vm.unit1().unwrap().name, "United States - Dollar");
    assert_eq!(vm.unit2().unwrap().name, "Europe - Euro");
    assert_eq!((vm.currency_symbol1(), vm.currency_symbol2()), ("$", "€"));
    assert!(vm.currency_symbol_visible());
    assert_eq!(vm.currency_ratio_equality(), "1 USD = 0.8836 EUR");
    assert_eq!(
        vm.currency_ratio_equality_automation_name(),
        "1 United States Dollar = 0.8836 Europe Euro"
    );
    assert!(vm.currency_timestamp().starts_with("Updated "));
    assert!(!vm.currency_data_is_week_old());
    assert_eq!(vm.currency_rates_date(), Some("2026-09-30"));
    assert_eq!(vm.currency_data_source(), Some(CurrencyDataSource::Bundled));
    // No supplementary results for currencies.
    assert!(vm.supplementary_results().is_empty());
    assert_eq!(vm.currency_status(), CurrencyStatus::Normal);
}

#[test]
fn currency_conversion_and_formatting() {
    let mut vm = currency_vm();
    press(&mut vm, "100");
    assert_eq!((vm.value1(), vm.value2()), ("100", "88.36"));

    // Amounts are shown with the currency's fraction digits once there is a decimal point.
    press(&mut vm, "C1.");
    assert_eq!(vm.value1(), "1.00");
    press(&mut vm, "5");
    assert_eq!((vm.value1(), vm.value2()), ("1.50", "1.33"));
    press(&mut vm, "5");
    assert_eq!(vm.value1(), "1.55");
    // Input is blocked after the currency's fraction digits...
    press(&mut vm, "9");
    assert_eq!(vm.value1(), "1.55");
    // ...except for backspace and clear.
    press(&mut vm, "<");
    assert_eq!(vm.value1(), "1.50");

    // Large values are grouped; yen have no fraction digits.
    let jpy = unit_by_abbreviation(&vm, "JPY");
    vm.set_unit2(jpy);
    press(&mut vm, "C1000");
    assert_eq!((vm.value1(), vm.value2()), ("1,000", "157,940"));
    assert_eq!(vm.currency_ratio_equality(), "1 USD = 157.94 JPY");
    assert_eq!(vm.currency_symbol2(), "¥");

    // Three fraction digits for the Kuwaiti dinar.
    let kwd = unit_by_abbreviation(&vm, "KWD");
    vm.set_unit2(kwd);
    press(&mut vm, "C1");
    assert_eq!(vm.value2(), "0.308");
}

#[test]
fn decimal_is_disabled_for_currencies_without_minor_units() {
    let mut vm = currency_vm();
    assert!(vm.is_decimal_enabled());
    let jpy = unit_by_abbreviation(&vm, "JPY");
    vm.set_unit1(jpy);
    assert!(!vm.is_decimal_enabled());
    // Converting into dollars still shows cents.
    let usd = unit_by_abbreviation(&vm, "USD");
    vm.set_unit2(usd);
    press(&mut vm, "C500");
    assert_eq!(vm.value2(), "3.17");
    // Back to a non-currency category the decimal point works again.
    vm.set_current_mode(ConverterMode::Length);
    assert!(vm.is_decimal_enabled());
}

#[test]
fn entering_digit_after_currency_unit_change_replaces_value() {
    let mut vm = currency_vm();
    vm.paste("1.23");
    assert_eq!(vm.value1(), "1.23");
    let replacement = vm
        .units()
        .iter()
        .find(|u| u.id != vm.unit1().unwrap().id)
        .unwrap()
        .id;
    vm.set_unit1(replacement);

    press(&mut vm, "7");

    assert_eq!(vm.value1(), "7");
}

#[test]
fn changing_currency_truncates_to_its_fraction_digits() {
    let mut vm = currency_vm();
    vm.paste("12.34");
    let jpy = unit_by_abbreviation(&vm, "JPY");
    vm.set_unit1(jpy);
    assert_eq!(vm.value1(), "12");
    assert_eq!(vm.value_from_unlocalized(), "12");
}

#[test]
fn switching_active_in_currency_updates_the_ratio_direction() {
    let mut vm = currency_vm();
    press(&mut vm, "10");
    vm.switch_active();
    assert_eq!((vm.currency_symbol1(), vm.currency_symbol2()), ("$", "€"));
    assert_eq!(vm.currency_ratio_equality(), "1 EUR = 1.1318 USD");
    press(&mut vm, "10");
    assert_eq!((vm.value1(), vm.value2()), ("11.32", "10"));
}

#[test]
fn entering_currency_after_background_load_uses_loaded_ratios() {
    let mut vm = vm();
    assert!(!vm.is_currency_current_category());

    // The background fetch returns the original's planet currencies.
    assert!(vm.start_automatic_currency_fetch());
    vm.finish_currency_fetch(Ok(planet_snapshot(fixture_time() + TimeDelta::hours(1))));
    assert!(vm.is_currency_data_loaded());

    vm.set_current_mode(ConverterMode::Currency);
    let mars = unit_by_abbreviation(&vm, "MAR");
    let moon = unit_by_abbreviation(&vm, "MON");
    vm.set_unit1(mars);
    vm.set_unit2(moon);
    assert_eq!(
        vm.units().iter().find(|u| u.id == mars).unwrap().name,
        "Mars - MAR"
    );

    press(&mut vm, "100");

    assert_eq!(vm.value1(), "100");
    assert_eq!(vm.value2(), "50");
    assert_eq!(vm.currency_data_source(), Some(CurrencyDataSource::Web));
}

#[test]
fn currency_load_finishing_inside_currency_uses_loaded_ratios() {
    let mut vm = currency_vm();
    assert!(vm.start_automatic_currency_fetch());
    vm.finish_currency_fetch(Ok(planet_snapshot(fixture_time() + TimeDelta::hours(1))));

    let mars = unit_by_abbreviation(&vm, "MAR");
    let moon = unit_by_abbreviation(&vm, "MON");
    vm.set_unit1(mars);
    vm.set_unit2(moon);

    press(&mut vm, "100");

    assert_eq!(vm.value1(), "100");
    assert_eq!(vm.value2(), "50");
}

#[test]
fn leaving_currency_clears_the_currency_symbols_and_ratio() {
    let mut vm = currency_vm();
    assert!(!vm.currency_symbol1().is_empty());

    vm.set_current_mode(ConverterMode::Length);

    assert_eq!(vm.currency_symbol1(), "");
    assert_eq!(vm.currency_symbol2(), "");
    assert!(!vm.currency_symbol_visible());
    assert_eq!(vm.currency_ratio_equality(), "");
}

#[test]
fn currency_refresh_completes_after_initial_load() {
    let mut vm = currency_vm();
    let stale_timestamp = vm.currency_timestamp().to_owned();

    assert!(vm.start_currency_refresh());
    assert!(vm.is_currency_loading_visible());
    assert!(!vm.start_currency_refresh(), "a refresh is already running");
    vm.finish_currency_fetch(Ok(snapshot_at(fixture_time() + TimeDelta::days(3))));

    assert!(vm.is_currency_data_loaded());
    assert!(!vm.is_currency_loading_visible());
    assert!(!vm.currency_data_load_failed());
    assert_ne!(stale_timestamp, vm.currency_timestamp());
    assert_eq!(vm.currency_data_source(), Some(CurrencyDataSource::Web));
    // The selection survives the refresh.
    assert_eq!(vm.unit1().unwrap().abbreviation, "USD");
    assert_eq!(vm.unit2().unwrap().abbreviation, "EUR");
}

/// REVIEW_2 R2-M-02: the connectivity monitor can wrongly report "offline"
/// (VPNs, sandboxes, no NetworkManager). Automatic refreshes respect it; a
/// refresh the user asks for is attempted and its rates are used.
#[test]
fn explicit_refresh_overrides_a_wrong_offline_signal() {
    let mut vm = currency_vm();
    vm.set_network_behavior(NetworkAccessBehavior::Offline);
    assert!(!vm.start_automatic_currency_fetch());
    assert!(vm.start_currency_refresh());
    vm.finish_currency_fetch(Ok(snapshot_at(fixture_time() + TimeDelta::days(3))));
    assert!(!vm.currency_data_load_failed());
    assert_eq!(vm.currency_data_source(), Some(CurrencyDataSource::Web));
    assert!(vm.is_currency_data_loaded());
}

#[test]
fn metered_connections_only_fetch_when_asked() {
    let mut vm = currency_vm();
    vm.set_network_behavior(NetworkAccessBehavior::OptIn);
    assert!(!vm.start_automatic_currency_fetch());
    assert_eq!(vm.currency_status(), CurrencyStatus::ChargesMayApply);
    assert!(vm.start_currency_refresh());
    vm.finish_currency_fetch(Ok(snapshot_at(fixture_time() + TimeDelta::days(3))));
    assert_eq!(vm.currency_data_source(), Some(CurrencyDataSource::Web));
    // Back on an unrestricted network, the automatic refresh is allowed
    // again only if the data is stale; it was just refreshed.
    vm.set_network_behavior(NetworkAccessBehavior::Normal);
    assert!(!vm.start_automatic_currency_fetch());
}

#[test]
fn failed_refresh_reports_and_keeps_rates() {
    let mut vm = currency_vm();
    press(&mut vm, "100");
    assert!(vm.start_currency_refresh());
    vm.finish_currency_fetch(Err(CurrencyError::Http("no route to host".into())));
    assert!(vm.currency_data_load_failed());
    assert!(!vm.is_currency_loading_visible());
    assert_eq!(vm.currency_status(), CurrencyStatus::FailedToRefresh);
    assert_eq!(
        vm.currency_status().text(),
        "Couldn’t get new rates. Try again later."
    );
    // Still converting with the previous rates.
    press(&mut vm, "C100");
    assert_eq!(vm.value2(), "88.36");
    // Leaving and re-entering Currency keeps working.
    vm.set_current_mode(ConverterMode::Length);
    vm.set_current_mode(ConverterMode::Currency);
    press(&mut vm, "C2");
    assert_eq!(vm.value2(), "1.77");
}

#[test]
fn caches_fetched_rates_and_reloads_them() {
    let path = temp_cache_path("vm-cache");
    let config = || ViewModelConfig {
        currency_cache_path: Some(path.clone()),
        fallback_snapshot: Some(fixture_snapshot()),
        clock: Some(clock_after_fixture(TimeDelta::hours(30))),
        ..Default::default()
    };
    let mut first = UnitConverterViewModel::new(config());
    assert_eq!(
        first.currency_data_source(),
        Some(CurrencyDataSource::Bundled)
    );
    assert!(first.start_automatic_currency_fetch());
    assert!(
        !first.start_automatic_currency_fetch(),
        "only one fetch at a time"
    );
    first.finish_currency_fetch(Ok(snapshot_at(fixture_time() + TimeDelta::hours(29))));
    assert_eq!(first.currency_data_source(), Some(CurrencyDataSource::Web));
    assert!(!first.start_automatic_currency_fetch());

    // Next start: the cache is fresh (1 hour old) and used without fetching.
    let second = UnitConverterViewModel::new(config());
    assert_eq!(
        second.currency_data_source(),
        Some(CurrencyDataSource::Cache)
    );
    let mut second = second;
    assert!(!second.start_automatic_currency_fetch());
}

#[test]
fn network_behavior_and_status() {
    let mut vm = currency_vm();
    vm.set_network_behavior(NetworkAccessBehavior::Offline);
    assert_eq!(vm.network_behavior(), NetworkAccessBehavior::Offline);
    assert_eq!(vm.currency_status(), CurrencyStatus::Offline);
    assert_eq!(
        vm.currency_status().text(),
        "Offline. Please check your Network Settings"
    );
    assert!(!vm.currency_status().refresh_visible());
    assert!(!vm.start_automatic_currency_fetch());

    vm.set_network_behavior(NetworkAccessBehavior::OptIn);
    assert_eq!(vm.currency_status(), CurrencyStatus::ChargesMayApply);
    assert_eq!(vm.currency_status().text(), "Data charges may apply.");
    assert!(!vm.start_automatic_currency_fetch());
    // An explicit refresh on a metered connection is allowed.
    assert!(vm.start_currency_refresh());
    vm.finish_currency_fetch(Ok(snapshot_at(fixture_time() + TimeDelta::hours(2))));
    assert!(!vm.currency_data_load_failed());
    assert_eq!(vm.currency_data_source(), Some(CurrencyDataSource::Web));

    // Connectivity is back: an automatic refresh may run again if data is stale.
    vm.set_network_behavior(NetworkAccessBehavior::Normal);
    assert_eq!(vm.currency_status(), CurrencyStatus::Normal);
}

#[test]
fn week_old_rates_are_flagged() {
    let vm = UnitConverterViewModel::new(ViewModelConfig {
        fallback_snapshot: Some(fixture_snapshot()),
        clock: Some(clock_after_fixture(TimeDelta::days(10))),
        ..Default::default()
    });
    assert!(vm.currency_data_is_week_old());
}

#[test]
fn real_bundled_snapshot_drives_the_converter() {
    let mut vm = UnitConverterViewModel::default();
    vm.set_current_mode(ConverterMode::Currency);
    assert!(vm.units().len() > 150);
    assert_eq!(vm.unit1().unwrap().abbreviation, "USD");
    assert_eq!(vm.unit2().unwrap().abbreviation, "EUR");
    assert!(vm.currency_ratio_equality().starts_with("1 USD = 0.88"));
    press(&mut vm, "1");
    let value: f64 = vm.value2().parse().unwrap();
    assert!(value > 0.5 && value < 1.5);
}

#[test]
fn view_model_can_move_between_threads() {
    fn assert_send<T: Send>() {}
    assert_send::<UnitConverterViewModel>();
    assert_send::<unitconv::UnitConverter>();
    assert_send::<unitconv::currency::CurrencyDataLoader>();
}
