// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Rust port of the non-UI logic of Calculator.ViewModels/UnitConverterViewModel.cs
// (plus the currency status logic of Calculator/Views/UnitConverter.xaml.cs).

//! A plain, UI-agnostic view model for the unit converter.
//!
//! [`UnitConverterViewModel`] owns the [`UnitConverter`] engine wired to the
//! static [`UnitConverterDataLoader`] and the [`CurrencyDataLoader`]. A GUI
//! calls methods for button presses / unit selection and reads display
//! strings back. Network I/O is never done here: when rates should be
//! refreshed the GUI runs [`fetch_latest`](crate::currency::fetch_latest) on a
//! background thread and passes the result to
//! [`finish_currency_fetch`](UnitConverterViewModel::finish_currency_fetch).

use std::any::Any;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::converter::{
    Category, Command, CurrencyConverterDataLoader, SuggestedValue, Unit, UnitConverter,
    UnitConverterVmCallback, ViewModelCurrencyCallback,
};
use crate::currency::{
    CURRENCY_CATEGORY_ID, Clock, CurrencyDataLoader, CurrencyDataLoaderConfig, CurrencyDataSource,
    CurrencyError, CurrencySnapshot, NetworkAccessBehavior, info,
};
use crate::data_loader::{ConverterMode, UnitConverterDataLoader};
use crate::number_formatting::group_digits;
use crate::resources::{INVALID_INPUT, format_resource, resource_string};

/// The error sentinel the clipboard validation (`CopyPasteManager`) returns
/// for unusable text.
pub const PASTE_ERROR_STRING: &str = "NoOp";

/// Fraction digits of the default currency formatter (the user's currency;
/// USD in the original's fallback).
const DEFAULT_CURRENCY_FRACTION_DIGITS: u32 = 2;

/// A converter category as shown in the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryInfo {
    pub id: i32,
    pub name: String,
    pub supports_negative: bool,
}

impl CategoryInfo {
    /// Whether the negate (±) button should be visible (`NegateVisibility`).
    pub fn negate_visible(&self) -> bool {
        self.supports_negative
    }

    /// The converter mode of this category.
    pub fn mode(&self) -> Option<ConverterMode> {
        ConverterMode::from_id(self.id)
    }

    fn to_category(&self) -> Category {
        Category::new(self.id, self.name.clone(), self.supports_negative)
    }
}

/// A unit as shown in the unit pickers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitInfo {
    pub id: i32,
    pub name: String,
    pub accessible_name: String,
    pub abbreviation: String,
    pub is_whimsical: bool,
}

impl UnitInfo {
    fn from_unit(unit: &Unit) -> Self {
        UnitInfo {
            id: unit.id,
            name: unit.name.clone(),
            accessible_name: unit.accessible_name.clone(),
            abbreviation: unit.abbreviation.clone(),
            is_whimsical: unit.is_whimsical,
        }
    }

    fn placeholder() -> Self {
        UnitInfo {
            id: crate::converter::EMPTY_UNIT_ID,
            name: String::new(),
            accessible_name: String::new(),
            abbreviation: String::new(),
            is_whimsical: false,
        }
    }

    fn to_model_unit(&self) -> Unit {
        Unit {
            id: self.id,
            name: self.name.clone(),
            accessible_name: self.accessible_name.clone(),
            abbreviation: self.abbreviation.clone(),
            is_conversion_source: false,
            is_conversion_target: false,
            is_whimsical: self.is_whimsical,
        }
    }
}

impl std::fmt::Display for UnitInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.accessible_name)
    }
}

/// One "about equal to" result, e.g. `0.1` [soccer fields].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupplementaryResult {
    /// Localized value.
    pub value: String,
    pub unit: UnitInfo,
}

impl SupplementaryResult {
    pub fn is_whimsical(&self) -> bool {
        self.unit.is_whimsical
    }

    /// `"<value> <unit name>"`.
    pub fn localized_automation_name(&self) -> String {
        format!("{} {}", self.value, self.unit.name)
    }
}

/// Settings the GUI should persist between runs (the original stored these
/// in `ApplicationData.LocalSettings`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConverterPreferences {
    /// Serialized engine state (`UnitConverterPreferences`): last non-currency
    /// category and its units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_converter: Option<String>,
    /// Last used "from" currency (`CURRENCY_UNIT_FROM_KEY`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency_from: Option<String>,
    /// Last used "to" currency (`CURRENCY_UNIT_TO_KEY`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency_to: Option<String>,
}

/// Decimal and digit grouping symbols used for display strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberFormat {
    pub decimal_separator: char,
    pub grouping_separator: String,
}

impl Default for NumberFormat {
    /// en-US: `1,234.5`.
    fn default() -> Self {
        NumberFormat {
            decimal_separator: '.',
            grouping_separator: ",".into(),
        }
    }
}

/// Construction parameters for [`UnitConverterViewModel`].
#[derive(Clone)]
pub struct ViewModelConfig {
    /// Two-letter region code deciding default units (`"US"`, `"GB"`, `"JP"`, ...).
    pub region: String,
    /// Language tag deciding the default currency pair (`"en-US"` → USD to EUR).
    pub language: String,
    /// File to cache fetched exchange rates in (e.g. under `$XDG_CACHE_HOME`).
    pub currency_cache_path: Option<PathBuf>,
    /// Previously saved [`preferences`](UnitConverterViewModel::preferences).
    pub preferences: ConverterPreferences,
    pub number_format: NumberFormat,
    pub is_rtl_language: bool,
    /// Initial network access behaviour.
    pub network_behavior: NetworkAccessBehavior,
    /// Offline snapshot; `None` uses the one bundled with the crate.
    pub fallback_snapshot: Option<CurrencySnapshot>,
    /// Clock for cache-age decisions; `None` uses the system clock.
    pub clock: Option<Clock>,
}

impl Default for ViewModelConfig {
    fn default() -> Self {
        ViewModelConfig {
            region: "US".into(),
            language: "en-US".into(),
            currency_cache_path: None,
            preferences: ConverterPreferences::default(),
            number_format: NumberFormat::default(),
            is_rtl_language: false,
            network_behavior: NetworkAccessBehavior::Normal,
            fallback_snapshot: None,
            clock: None,
        }
    }
}

/// Secondary currency status line (from the original view's code-behind).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrencyStatus {
    /// Nothing to report.
    Normal,
    /// Metered connection: "Data charges may apply."
    ChargesMayApply,
    /// The last refresh failed: "Couldn’t get new rates. Try again later."
    FailedToRefresh,
    /// No connectivity: "Offline. Please check your Network Settings"
    /// (the refresh button is hidden in this state).
    Offline,
}

impl CurrencyStatus {
    /// The (en-US) status text; empty for [`Normal`](Self::Normal).
    pub fn text(self) -> String {
        match self {
            CurrencyStatus::Normal => String::new(),
            CurrencyStatus::ChargesMayApply => resource_string("DataChargesMayApply").into(),
            CurrencyStatus::FailedToRefresh => resource_string("FailedToRefresh").into(),
            CurrencyStatus::Offline => {
                let text = resource_string("OfflineStatusHyperlinkText");
                text.split("%HL%")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        }
    }

    /// Whether the "Update rates" button should be shown.
    pub fn refresh_visible(self) -> bool {
        self != CurrencyStatus::Offline
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConversionParameter {
    Source,
    Target,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FetchKind {
    Automatic,
    Manual,
}

/// Callback events, dispatched synchronously after each engine call (the
/// original marshalled them to the UI thread).
#[derive(Debug)]
enum Event {
    Display(String, String),
    Suggested(Vec<SuggestedValue>),
    MaxDigitsReached,
    CurrencyDataLoadFinished(bool),
    CurrencyTimestamp(String, bool),
    NetworkBehaviorChanged(NetworkAccessBehavior),
}

#[derive(Default)]
struct EventQueue(VecDeque<Event>);

impl UnitConverterVmCallback for EventQueue {
    fn display_callback(&mut self, from: &str, to: &str) {
        self.0.push_back(Event::Display(from.into(), to.into()));
    }

    fn suggested_value_callback(&mut self, suggested_values: &[SuggestedValue]) {
        self.0
            .push_back(Event::Suggested(suggested_values.to_vec()));
    }

    fn max_digits_reached(&mut self) {
        self.0.push_back(Event::MaxDigitsReached);
    }
}

impl ViewModelCurrencyCallback for EventQueue {
    fn currency_data_load_finished(&mut self, did_load: bool) {
        self.0.push_back(Event::CurrencyDataLoadFinished(did_load));
    }

    fn currency_symbols_callback(&mut self, _from_symbol: &str, _to_symbol: &str) {}

    fn currency_ratios_callback(&mut self, _ratio_equality: &str, _acc_ratio_equality: &str) {}

    fn currency_timestamp_callback(&mut self, timestamp: &str, is_week_old_data: bool) {
        self.0
            .push_back(Event::CurrencyTimestamp(timestamp.into(), is_week_old_data));
    }

    fn network_behavior_changed(&mut self, new_behavior: NetworkAccessBehavior) {
        self.0
            .push_back(Event::NetworkBehaviorChanged(new_behavior));
    }
}

/// The unit converter view model (`UnitConverterViewModel`).
///
/// "Value 1" / "Unit 1" are the top field and picker, "Value 2" / "Unit 2"
/// the bottom ones. Exactly one value is active (being typed into); it is
/// the conversion source.
pub struct UnitConverterViewModel {
    model: UnitConverter,
    events: Arc<Mutex<EventQueue>>,
    number_format: NumberFormat,

    categories: Vec<CategoryInfo>,
    current_category: Option<CategoryInfo>,
    is_currency_current_category: bool,
    units: Vec<UnitInfo>,
    unit1: Option<UnitInfo>,
    unit2: Option<UnitInfo>,
    value1: String,
    value2: String,
    value1_active: bool,
    value2_active: bool,
    supplementary_results: Vec<SupplementaryResult>,

    currency_symbol1: String,
    currency_symbol2: String,
    currency_ratio_equality: String,
    currency_ratio_equality_automation_name: String,
    currency_timestamp: String,
    network_behavior: NetworkAccessBehavior,
    currency_data_load_failed: bool,
    currency_data_is_week_old: bool,
    is_currency_loading_visible: bool,
    is_currency_data_loaded: bool,
    metered_connection_override: bool,
    fetch_in_flight: Option<FetchKind>,

    is_decimal_enabled: bool,
    is_drop_down_open: bool,
    is_drop_down_enabled: bool,
    is_input_blocked: bool,

    cached_suggested_values: Vec<SuggestedValue>,
    value_from_unlocalized: String,
    value_to_unlocalized: String,
    value1_cp: ConversionParameter,
    /// Fraction digits of the currency formatters for unit 1 / unit 2
    /// (`None` until a currency pair has been selected).
    currency_fraction_digits1: Option<u32>,
    currency_fraction_digits2: Option<u32>,

    max_digits_reached_count: u32,
    preferences: ConverterPreferences,
}

impl Default for UnitConverterViewModel {
    fn default() -> Self {
        Self::new(ViewModelConfig::default())
    }
}

impl std::fmt::Debug for UnitConverterViewModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnitConverterViewModel")
            .field("current_category", &self.current_category)
            .field("unit1", &self.unit1)
            .field("unit2", &self.unit2)
            .field("value1", &self.value1)
            .field("value2", &self.value2)
            .finish_non_exhaustive()
    }
}

impl UnitConverterViewModel {
    /// Creates the view model: loads the static units, restores
    /// preferences, selects the saved (or first) category and loads currency
    /// rates from the cache or the bundled snapshot.
    pub fn new(config: ViewModelConfig) -> Self {
        let events = Arc::new(Mutex::new(EventQueue::default()));

        let data_loader = UnitConverterDataLoader::with_region(&config.region);
        let mut currency_loader = CurrencyDataLoader::new(CurrencyDataLoaderConfig {
            response_language: config.language.clone(),
            cache_path: config.currency_cache_path.clone(),
            is_rtl_language: config.is_rtl_language,
            fallback_snapshot: config.fallback_snapshot.clone(),
            clock: config.clock.clone(),
        });
        if let (Some(from), Some(to)) = (
            &config.preferences.currency_from,
            &config.preferences.currency_to,
        ) {
            currency_loader.set_last_used_currencies(from, to);
        }
        currency_loader.on_network_behavior_changed(config.network_behavior);
        currency_loader.set_view_model_callback(Some(events.clone()));

        let model = UnitConverter::with_currency_loader(
            Box::new(data_loader),
            Some(Box::new(currency_loader)),
        );

        let mut vm = UnitConverterViewModel {
            model,
            events,
            number_format: config.number_format,
            categories: Vec::new(),
            current_category: None,
            is_currency_current_category: false,
            units: Vec::new(),
            unit1: None,
            unit2: None,
            value1: "0".into(),
            value2: "0".into(),
            value1_active: true,
            value2_active: false,
            supplementary_results: Vec::new(),
            currency_symbol1: String::new(),
            currency_symbol2: String::new(),
            currency_ratio_equality: String::new(),
            currency_ratio_equality_automation_name: String::new(),
            currency_timestamp: String::new(),
            network_behavior: config.network_behavior,
            currency_data_load_failed: false,
            currency_data_is_week_old: false,
            is_currency_loading_visible: false,
            is_currency_data_loaded: false,
            metered_connection_override: false,
            fetch_in_flight: None,
            is_decimal_enabled: true,
            is_drop_down_open: false,
            is_drop_down_enabled: true,
            is_input_blocked: false,
            cached_suggested_values: Vec::new(),
            value_from_unlocalized: String::new(),
            value_to_unlocalized: String::new(),
            value1_cp: ConversionParameter::Source,
            currency_fraction_digits1: None,
            currency_fraction_digits2: None,
            max_digits_reached_count: 0,
            preferences: config.preferences,
        };

        let callback = vm.events.clone();
        vm.model.set_view_model_callback(Some(callback));
        vm.process_events();

        // Initialize the engine and populate data
        vm.model.initialize();
        vm.populate_data();

        // Start loading currency data (cache or bundled snapshot; fetching
        // fresh rates is up to the caller, see `start_automatic_currency_fetch`).
        if let Some(loader) = vm.model.currency_data_loader_mut() {
            loader.load_data();
        }
        vm.process_events();
        vm
    }

    // ---------------------------------------------------------------------
    // Getters

    /// All converter categories, Currency first.
    pub fn categories(&self) -> &[CategoryInfo] {
        &self.categories
    }

    pub fn current_category(&self) -> Option<&CategoryInfo> {
        self.current_category.as_ref()
    }

    pub fn current_mode(&self) -> Option<ConverterMode> {
        self.current_category.as_ref().and_then(CategoryInfo::mode)
    }

    pub fn is_currency_current_category(&self) -> bool {
        self.is_currency_current_category
    }

    /// Units offered by both pickers (whimsical units are never offered).
    pub fn units(&self) -> &[UnitInfo] {
        &self.units
    }

    pub fn unit1(&self) -> Option<&UnitInfo> {
        self.unit1.as_ref()
    }

    pub fn unit2(&self) -> Option<&UnitInfo> {
        self.unit2.as_ref()
    }

    /// Display string of the top field.
    pub fn value1(&self) -> &str {
        &self.value1
    }

    /// Display string of the bottom field.
    pub fn value2(&self) -> &str {
        &self.value2
    }

    pub fn value1_active(&self) -> bool {
        self.value1_active
    }

    pub fn value2_active(&self) -> bool {
        self.value2_active
    }

    /// The "About equal to" results: regular units sorted by how readable
    /// they are, then at most one whimsical unit.
    pub fn supplementary_results(&self) -> &[SupplementaryResult] {
        &self.supplementary_results
    }

    pub fn supplementary_visible(&self) -> bool {
        !self.supplementary_results.is_empty()
    }

    /// Currency symbol shown next to value 1 (empty outside Currency).
    pub fn currency_symbol1(&self) -> &str {
        &self.currency_symbol1
    }

    pub fn currency_symbol2(&self) -> &str {
        &self.currency_symbol2
    }

    pub fn currency_symbol_visible(&self) -> bool {
        !self.currency_symbol1.is_empty() && !self.currency_symbol2.is_empty()
    }

    /// e.g. `"1 USD = 0.8836 EUR"` (empty outside Currency).
    pub fn currency_ratio_equality(&self) -> &str {
        &self.currency_ratio_equality
    }

    pub fn currency_ratio_equality_automation_name(&self) -> &str {
        &self.currency_ratio_equality_automation_name
    }

    /// e.g. `"Updated 10/1/2026 9:02 PM"`.
    pub fn currency_timestamp(&self) -> &str {
        &self.currency_timestamp
    }

    /// The rates are more than a week old (the original shows the timestamp
    /// in bold red).
    pub fn currency_data_is_week_old(&self) -> bool {
        self.currency_data_is_week_old
    }

    pub fn currency_data_load_failed(&self) -> bool {
        self.currency_data_load_failed
    }

    /// Show a progress indicator instead of the currency fields.
    pub fn is_currency_loading_visible(&self) -> bool {
        self.is_currency_loading_visible
    }

    pub fn is_currency_data_loaded(&self) -> bool {
        self.is_currency_data_loaded
    }

    pub fn network_behavior(&self) -> NetworkAccessBehavior {
        self.network_behavior
    }

    /// The secondary status line under the currency timestamp.
    pub fn currency_status(&self) -> CurrencyStatus {
        match self.network_behavior {
            NetworkAccessBehavior::Normal => {
                if self.currency_data_load_failed {
                    CurrencyStatus::FailedToRefresh
                } else {
                    CurrencyStatus::Normal
                }
            }
            NetworkAccessBehavior::OptIn => {
                if self.metered_connection_override && self.currency_data_load_failed {
                    CurrencyStatus::FailedToRefresh
                } else {
                    CurrencyStatus::ChargesMayApply
                }
            }
            NetworkAccessBehavior::Offline => CurrencyStatus::Offline,
        }
    }

    /// Publication date of the rates in use (`YYYY-MM-DD`).
    pub fn currency_rates_date(&self) -> Option<&str> {
        self.currency_loader()
            .snapshot()
            .map(|s| s.rates_date.as_str())
    }

    /// Where the rates in use came from.
    pub fn currency_data_source(&self) -> Option<CurrencyDataSource> {
        self.currency_loader().data_source()
    }

    /// The currency data loader (rates, snapshot, load status).
    pub fn currency_data_loader(&self) -> &CurrencyDataLoader {
        self.currency_loader()
    }

    /// Whether the decimal point button is enabled (disabled for
    /// currencies without minor units, e.g. JPY).
    pub fn is_decimal_enabled(&self) -> bool {
        self.is_decimal_enabled
    }

    pub fn is_drop_down_enabled(&self) -> bool {
        self.is_drop_down_enabled
    }

    /// How many times input was rejected because the maximum number of
    /// digits was reached.
    pub fn max_digits_reached_count(&self) -> u32 {
        self.max_digits_reached_count
    }

    /// The unlocalized value being edited (what Copy puts on the clipboard).
    pub fn value_from_unlocalized(&self) -> &str {
        &self.value_from_unlocalized
    }

    /// The unlocalized converted value.
    pub fn value_to_unlocalized(&self) -> &str {
        &self.value_to_unlocalized
    }

    /// Text to put on the clipboard for Copy (`OnCopyCommand`).
    pub fn copy_text(&self) -> &str {
        &self.value_from_unlocalized
    }

    /// Preferences to persist (updated on every unit change).
    pub fn preferences(&self) -> &ConverterPreferences {
        &self.preferences
    }

    /// The engine (advanced use).
    pub fn model(&self) -> &UnitConverter {
        &self.model
    }

    /// Accessible name of value 1, e.g. `"Convert from 5 Centimeters"`.
    pub fn value1_automation_name(&self) -> String {
        let format = if self.value1_cp == ConversionParameter::Source {
            "Format_ValueFrom"
        } else {
            "Format_ValueTo"
        };
        format_resource(
            resource_string(format),
            &[
                &self.value1,
                self.unit1
                    .as_ref()
                    .map_or("", |u| u.accessible_name.as_str()),
            ],
        )
    }

    /// Accessible name of value 2, e.g. `"Converts into 1.97 Inches"`.
    pub fn value2_automation_name(&self) -> String {
        let format = if self.value1_cp == ConversionParameter::Source {
            "Format_ValueTo"
        } else {
            "Format_ValueFrom"
        };
        format_resource(
            resource_string(format),
            &[
                &self.value2,
                self.unit2
                    .as_ref()
                    .map_or("", |u| u.accessible_name.as_str()),
            ],
        )
    }

    /// e.g. `"5 Centimeters is 1.97 Inches"` (what Narrator announces).
    pub fn conversion_result_text(&self) -> String {
        let (from_value, from_unit, to_value, to_unit) = if self.value1_active {
            (&self.value1, &self.unit1, &self.value2, &self.unit2)
        } else {
            (&self.value2, &self.unit2, &self.value1, &self.unit1)
        };
        format_resource(
            resource_string("Format_ConversionResult"),
            &[
                from_value,
                from_unit.as_ref().map_or("", |u| u.name.as_str()),
                to_value,
                to_unit.as_ref().map_or("", |u| u.name.as_str()),
            ],
        )
    }

    // ---------------------------------------------------------------------
    // Commands

    /// Selects a category by id (no-op if it is already current).
    pub fn set_current_category(&mut self, category_id: i32) {
        if self
            .current_category
            .as_ref()
            .is_some_and(|c| c.id == category_id)
        {
            return;
        }
        let Some(category) = self
            .categories
            .iter()
            .find(|c| c.id == category_id)
            .cloned()
        else {
            return;
        };
        self.set_current_category_internal(category);
    }

    /// Selects a category by mode.
    pub fn set_current_mode(&mut self, mode: ConverterMode) {
        self.set_current_category(mode.id());
    }

    /// Selects the unit of the top picker.
    pub fn set_unit1(&mut self, unit_id: i32) {
        let Some(unit) = self.units.iter().find(|u| u.id == unit_id).cloned() else {
            return;
        };
        if self.unit1.as_ref().is_some_and(|u| u.id == unit_id) {
            return;
        }
        self.unit1 = Some(unit);
        self.on_unit_changed();
    }

    /// Selects the unit of the bottom picker.
    pub fn set_unit2(&mut self, unit_id: i32) {
        let Some(unit) = self.units.iter().find(|u| u.id == unit_id).cloned() else {
            return;
        };
        if self.unit2.as_ref().is_some_and(|u| u.id == unit_id) {
            return;
        }
        self.unit2 = Some(unit);
        self.on_unit_changed();
    }

    /// Swaps the two units, keeping the value being edited (an addition;
    /// the original has no swap button).
    pub fn swap_units(&mut self) {
        if self.unit1.is_none() || self.unit2.is_none() || self.unit1 == self.unit2 {
            return;
        }
        std::mem::swap(&mut self.unit1, &mut self.unit2);
        self.on_unit_changed();
    }

    /// Makes the other value the one being edited (`SwitchActiveCommand`).
    pub fn switch_active(&mut self) {
        // Switch conversion parameter mapping
        self.value1_cp = match self.value1_cp {
            ConversionParameter::Source => ConversionParameter::Target,
            ConversionParameter::Target => ConversionParameter::Source,
        };

        // The active side follows the source.
        if self.value1_cp == ConversionParameter::Source {
            self.value2_active = false;
            self.value1_active = true;
        } else {
            self.value1_active = false;
            self.value2_active = true;
        }

        // Swap the unlocalized values
        std::mem::swap(
            &mut self.value_from_unlocalized,
            &mut self.value_to_unlocalized,
        );

        self.is_input_blocked = false;
        let new_value = if self.value_from_unlocalized.is_empty() {
            "0".to_owned()
        } else {
            self.value_from_unlocalized.clone()
        };
        self.model.switch_active(&new_value);
        self.process_events();

        // The engine reports the ratio for the new direction (C++
        // `CurrencyRatiosCallback`); the field symbols stay with their units.
        if self.is_currency_current_category
            && let (Some(from), Some(to)) = (self.unit_from(), self.unit_to())
        {
            let (ratio, accessible_ratio) = self
                .currency_loader()
                .get_currency_ratio_equality_by_id(from.id, to.id);
            self.currency_ratio_equality = ratio;
            self.currency_ratio_equality_automation_name = accessible_ratio;
        }

        self.update_is_decimal_enabled();
    }

    /// The user focused value 1 (switches if it was not active).
    pub fn activate_value1(&mut self) {
        if !self.value1_active {
            self.switch_active();
        }
    }

    /// The user focused value 2 (switches if it was not active).
    pub fn activate_value2(&mut self) {
        if !self.value2_active {
            self.switch_active();
        }
    }

    /// A number pad button or key press (`ButtonPressedCommand`): digits,
    /// decimal, negate, backspace, clear. ([`Command::Reset`] is not a
    /// button; use [`reset_view`](Self::reset_view).)
    pub fn button_pressed(&mut self, command: Command) {
        let command = match command {
            Command::Reset => Command::None,
            c => c,
        };

        if command == Command::Clear && self.is_drop_down_open {
            return;
        }

        // Block input if max decimal digits reached (except for clear/backspace)
        if self.is_input_blocked
            && !self.model.is_switched_active()
            && command != Command::Clear
            && command != Command::Backspace
        {
            return;
        }

        self.send_command(command);
    }

    /// Tells the view model whether a unit picker is open (Escape then
    /// closes the picker instead of clearing).
    pub fn set_drop_down_open(&mut self, open: bool) {
        self.is_drop_down_open = open;
    }

    /// Pastes text into the active value (`OnPaste`). The text should
    /// already be validated (e.g. by the copy/paste crate); `"NoOp"` or empty
    /// text shows "Invalid input".
    pub fn paste(&mut self, text: &str) {
        if text.is_empty() || text == PASTE_ERROR_STRING {
            self.display_paste_error();
            return;
        }

        let mut is_first_legal_char = true;
        let mut send_negate = false;
        let mut accumulation = String::new();

        for ch in text.chars() {
            let Some((command, can_send_negate)) = self.map_character_to_command(ch) else {
                send_negate = false;
                continue;
            };

            if is_first_legal_char {
                // Send Clear before sending anything that will actually apply to the field.
                self.send_command(Command::Clear);
                is_first_legal_char = false;

                // A leading minus is a sign, but it has to follow the digit it applies to or
                // the engine ignores it, so remember it rather than sending it now.
                if command == Command::Negate {
                    send_negate = true;
                }
            }

            if command != Command::Negate {
                self.send_command(command);

                if send_negate {
                    if can_send_negate {
                        self.send_command(Command::Negate);
                    }
                    send_negate = false;
                }
            }

            accumulation.push(if ch == self.number_format.decimal_separator {
                '.'
            } else {
                ch
            });
            self.update_input_blocked(&accumulation);
            if self.is_input_blocked {
                break;
            }
        }

        if is_first_legal_char {
            // No legal characters found — show paste error
            self.display_paste_error();
        }
    }

    /// Resets the converter (`ResetView`).
    pub fn reset_view(&mut self) {
        self.send_command(Command::Reset);
        self.reset_category();
    }

    /// Updates the network access behaviour (from the GUI's network monitor).
    pub fn set_network_behavior(&mut self, behavior: NetworkAccessBehavior) {
        self.currency_loader_mut()
            .on_network_behavior_changed(behavior);
        self.process_events();
    }

    /// If fresh rates should be fetched automatically (no cache, bundled
    /// data, or cache older than a day; network unrestricted), marks a fetch
    /// as in flight and returns true. The caller then runs
    /// [`fetch_latest`](crate::currency::fetch_latest) on a background thread
    /// and passes the result to [`finish_currency_fetch`](Self::finish_currency_fetch).
    pub fn start_automatic_currency_fetch(&mut self) -> bool {
        if self.fetch_in_flight.is_some() || !self.currency_loader().needs_web_refresh() {
            return false;
        }
        self.fetch_in_flight = Some(FetchKind::Automatic);
        true
    }

    /// The user pressed "Update rates" (`RefreshCurrencyRatiosAsync`). Returns
    /// true if the caller should now fetch (in the background) and call
    /// [`finish_currency_fetch`](Self::finish_currency_fetch); false if a
    /// refresh is already running.
    ///
    /// This works whatever the network behaviour: a metered connection is
    /// the user's to spend, and an "offline" report from the connectivity
    /// monitor may be wrong, so rates that do arrive are used.
    pub fn start_currency_refresh(&mut self) -> bool {
        if self.is_currency_loading_visible || self.fetch_in_flight == Some(FetchKind::Manual) {
            return false;
        }
        if self.network_behavior == NetworkAccessBehavior::OptIn {
            self.metered_connection_override = true;
        }

        self.is_currency_data_loaded = false;
        self.currency_data_load_failed = false;
        self.is_currency_loading_visible = true;
        self.fetch_in_flight = Some(FetchKind::Manual);
        true
    }

    /// Whether a fetch started by [`start_automatic_currency_fetch`](Self::start_automatic_currency_fetch)
    /// or [`start_currency_refresh`](Self::start_currency_refresh) has not finished yet.
    pub fn is_currency_fetch_in_flight(&self) -> bool {
        self.fetch_in_flight.is_some()
    }

    /// Completes a currency fetch with its result. On success the rates are
    /// cached (if a cache path is configured) and the currency view is
    /// refreshed; on failure the rates in use are kept.
    pub fn finish_currency_fetch(&mut self, result: Result<CurrencySnapshot, CurrencyError>) {
        match self.fetch_in_flight.take() {
            Some(FetchKind::Manual) => {
                let (did_load, _timestamp) = self.model.refresh_currency_ratios(result);
                self.process_events();
                self.on_currency_data_load_finished(did_load);
            }
            Some(FetchKind::Automatic) | None => {
                self.currency_loader_mut().finish_web_load(result);
                self.process_events();
            }
        }
    }

    // ---------------------------------------------------------------------
    // Internals

    fn currency_loader(&self) -> &CurrencyDataLoader {
        let loader: &dyn Any = self
            .model
            .currency_data_loader()
            .expect("view model always has a currency loader");
        loader
            .downcast_ref::<CurrencyDataLoader>()
            .expect("currency loader type")
    }

    fn currency_loader_mut(&mut self) -> &mut CurrencyDataLoader {
        let loader: &mut dyn Any = self
            .model
            .currency_data_loader_mut()
            .expect("view model always has a currency loader");
        loader
            .downcast_mut::<CurrencyDataLoader>()
            .expect("currency loader type")
    }

    fn send_command(&mut self, command: Command) {
        self.model.send_command(command);
        self.process_events();
    }

    fn process_events(&mut self) {
        loop {
            let event = lock(&self.events).0.pop_front();
            let Some(event) = event else { break };
            match event {
                Event::Display(from, to) => self.update_display(&from, &to),
                Event::Suggested(values) => self.update_supplementary_results(values),
                Event::MaxDigitsReached => self.max_digits_reached_count += 1,
                Event::CurrencyDataLoadFinished(did_load) => {
                    self.on_currency_data_load_finished(did_load)
                }
                Event::CurrencyTimestamp(timestamp, is_week_old) => {
                    self.currency_data_is_week_old = is_week_old;
                    self.currency_timestamp = timestamp;
                }
                Event::NetworkBehaviorChanged(behavior) => {
                    self.currency_data_load_failed = false;
                    self.network_behavior = behavior;
                }
            }
        }
    }

    fn populate_data(&mut self) {
        self.categories = self
            .model
            .get_categories()
            .iter()
            .map(|c| CategoryInfo {
                id: c.id,
                name: c.name.clone(),
                supports_negative: c.supports_negative,
            })
            .collect();

        self.restore_user_preferences();

        let current = self.model.get_current_category();
        let category = self
            .categories
            .iter()
            .find(|c| c.id == current.id)
            .cloned()
            .unwrap_or(CategoryInfo {
                id: current.id,
                name: current.name,
                supports_negative: current.supports_negative,
            });
        self.set_current_category_internal(category);
    }

    fn set_current_category_internal(&mut self, category: CategoryInfo) {
        self.is_currency_current_category = category.id == CURRENCY_CATEGORY_ID;
        self.current_category = Some(category);
        self.reset_category();
    }

    fn reset_category(&mut self) {
        self.is_input_blocked = false;
        self.set_selected_units();

        self.is_currency_loading_visible =
            self.is_currency_current_category && !self.is_currency_data_loaded;
        self.is_drop_down_enabled = self
            .units
            .first()
            .is_some_and(|u| u.id != crate::converter::EMPTY_UNIT_ID);
        if !self.is_currency_current_category {
            // (The original only ever updates this flag inside Currency, so a
            // currency without minor units could leave '.' disabled elsewhere.)
            self.is_decimal_enabled = true;
        }

        self.on_unit_changed();
    }

    fn set_selected_units(&mut self) {
        if self.is_currency_current_category {
            // (The original skips this after a failed refresh; here a failed
            // refresh keeps the previous rates, which are still valid.)
            if self.is_currency_data_loaded && !self.currency_loader().currency_units().is_empty() {
                self.model.reset_categories_and_ratios();
            }
            self.set_selected_currency_units();
            return;
        }

        let Some(category) = self
            .current_category
            .as_ref()
            .map(CategoryInfo::to_category)
        else {
            return;
        };
        let (units, from_unit, to_unit) = self.model.set_current_category(&category);
        self.process_events();

        self.build_unit_list(&units);
        let from = self.find_unit_in_list(&from_unit);
        let to = self.find_unit_in_list(&to_unit);
        self.set_unit_from(Some(from));
        self.set_unit_to(Some(to));
    }

    fn set_selected_currency_units(&mut self) {
        // Make sure the engine treats Currency as the current category (the
        // original relied on Currency being its first category).
        if let Some(category) = self
            .current_category
            .as_ref()
            .map(CategoryInfo::to_category)
        {
            self.model.set_current_category(&category);
            self.process_events();
        }

        let mut units = Vec::new();
        let mut from_unit = None;
        let mut to_unit = None;
        for currency in self.currency_loader().currency_units() {
            let unit = UnitInfo::from_unit(&currency.to_unit());
            if currency.is_conversion_source {
                from_unit = Some(unit.clone());
            }
            if currency.is_conversion_target {
                to_unit = Some(unit.clone());
            }
            units.push(unit);
        }

        if units.is_empty() {
            units.push(UnitInfo::placeholder());
        }

        // Publish a complete source before selected units.
        self.units = units;
        let from = from_unit.or_else(|| self.units.first().cloned());
        let to = to_unit.or_else(|| self.units.get(1).or(self.units.first()).cloned());
        self.set_unit_from(from);
        self.set_unit_to(to);
    }

    fn build_unit_list(&mut self, model_units: &[Unit]) {
        let mut units: Vec<UnitInfo> = model_units
            .iter()
            .filter(|u| !u.is_whimsical)
            .map(UnitInfo::from_unit)
            .collect();
        if units.is_empty() {
            units.push(UnitInfo::placeholder());
        }
        self.units = units;
    }

    fn find_unit_in_list(&self, target: &Unit) -> UnitInfo {
        self.units
            .iter()
            .find(|u| u.id == target.id)
            .or(self.units.first())
            .cloned()
            .unwrap_or_else(UnitInfo::placeholder)
    }

    fn unit_from(&self) -> Option<&UnitInfo> {
        match self.value1_cp {
            ConversionParameter::Source => self.unit1.as_ref(),
            ConversionParameter::Target => self.unit2.as_ref(),
        }
    }

    fn unit_to(&self) -> Option<&UnitInfo> {
        match self.value1_cp {
            ConversionParameter::Target => self.unit1.as_ref(),
            ConversionParameter::Source => self.unit2.as_ref(),
        }
    }

    fn set_unit_from(&mut self, unit: Option<UnitInfo>) {
        match self.value1_cp {
            ConversionParameter::Source => self.unit1 = unit,
            ConversionParameter::Target => self.unit2 = unit,
        }
    }

    fn set_unit_to(&mut self, unit: Option<UnitInfo>) {
        match self.value1_cp {
            ConversionParameter::Target => self.unit1 = unit,
            ConversionParameter::Source => self.unit2 = unit,
        }
    }

    fn set_value_from(&mut self, value: String) {
        match self.value1_cp {
            ConversionParameter::Source => self.value1 = value,
            ConversionParameter::Target => self.value2 = value,
        }
    }

    fn set_value_to(&mut self, value: String) {
        match self.value1_cp {
            ConversionParameter::Target => self.value1 = value,
            ConversionParameter::Source => self.value2 = value,
        }
    }

    /// Fraction digits of the "from" currency formatter.
    fn currency_fraction_digits_from(&self) -> Option<u32> {
        match self.value1_cp {
            ConversionParameter::Source => self.currency_fraction_digits1,
            ConversionParameter::Target => self.currency_fraction_digits2,
        }
    }

    fn currency_fraction_digits_to(&self) -> Option<u32> {
        match self.value1_cp {
            ConversionParameter::Target => self.currency_fraction_digits1,
            ConversionParameter::Source => self.currency_fraction_digits2,
        }
    }

    fn on_unit_changed(&mut self) {
        let (Some(unit_from), Some(unit_to)) = (self.unit_from().cloned(), self.unit_to().cloned())
        else {
            return;
        };

        self.update_currency_formatter();

        if self.is_currency_current_category {
            let loader = self.currency_loader();
            let (symbol1, symbol2) = loader.get_currency_symbols_by_id(unit_from.id, unit_to.id);
            let (ratio, accessible_ratio) =
                loader.get_currency_ratio_equality_by_id(unit_from.id, unit_to.id);
            if self.value1_cp == ConversionParameter::Source {
                self.currency_symbol1 = symbol1;
                self.currency_symbol2 = symbol2;
            } else {
                self.currency_symbol1 = symbol2;
                self.currency_symbol2 = symbol1;
            }
            self.currency_ratio_equality = ratio;
            self.currency_ratio_equality_automation_name = accessible_ratio;
        } else {
            self.currency_symbol1.clear();
            self.currency_symbol2.clear();
            self.currency_ratio_equality.clear();
            self.currency_ratio_equality_automation_name.clear();
        }

        // Always tell the engine the current unit types
        self.model
            .set_current_unit_types(&unit_from.to_model_unit(), &unit_to.to_model_unit());
        self.process_events();

        self.save_user_preferences();
    }

    fn update_currency_formatter(&mut self) {
        if !self.is_currency_current_category {
            return;
        }
        let (Some(unit1), Some(unit2)) = (&self.unit1, &self.unit2) else {
            return;
        };
        if unit1.abbreviation.is_empty() || unit2.abbreviation.is_empty() {
            return;
        }

        self.currency_fraction_digits1 = Some(info::fraction_digits(&unit1.abbreviation));
        self.currency_fraction_digits2 = Some(info::fraction_digits(&unit2.abbreviation));

        self.update_is_decimal_enabled();

        let digits_from = self
            .currency_fraction_digits_from()
            .unwrap_or(DEFAULT_CURRENCY_FRACTION_DIGITS);
        if let Some(prepared) = try_prepare_currency_input_for_paste(
            &self.value_from_unlocalized,
            digits_from,
            self.number_format.decimal_separator,
        ) {
            self.paste(&prepared);
        }
    }

    fn update_is_decimal_enabled(&mut self) {
        if !self.is_currency_current_category {
            return;
        }
        if let Some(digits) = self.currency_fraction_digits_from() {
            self.is_decimal_enabled = digits > 0;
        }
    }

    fn update_input_blocked(&mut self, currency_input: &str) {
        // currency_input is unlocalized and uses '.' as the decimal separator
        self.is_input_blocked = false;
        if let Some(pos_of_decimal) = currency_input.find('.')
            && self.is_currency_current_category
            && let Some(digits) = self.currency_fraction_digits_from()
        {
            self.is_input_blocked = pos_of_decimal + digits as usize + 1 == currency_input.len();
        }
    }

    fn update_display(&mut self, from: &str, to: &str) {
        self.value_from_unlocalized = from.to_owned();
        self.value_to_unlocalized = to.to_owned();

        let value_from =
            self.convert_to_localized_string(from, true, self.currency_fraction_digits_from());
        self.set_value_from(value_from);
        self.update_input_blocked(from);
        let value_to =
            self.convert_to_localized_string(to, true, self.currency_fraction_digits_to());
        self.set_value_to(value_to);
    }

    fn update_supplementary_results(&mut self, suggested_values: Vec<SuggestedValue>) {
        self.cached_suggested_values = suggested_values;

        let mut results = Vec::new();
        let mut whimsicals = Vec::new();
        for (value, model_unit) in &self.cached_suggested_values {
            let unit = UnitInfo::from_unit(model_unit);
            let result = SupplementaryResult {
                value: self.convert_to_localized_string(
                    value,
                    false,
                    Some(DEFAULT_CURRENCY_FRACTION_DIGITS),
                ),
                unit,
            };
            if result.is_whimsical() {
                whimsicals.push(result);
            } else {
                results.push(result);
            }
        }
        if let Some(first) = whimsicals.into_iter().next() {
            results.push(first);
        }
        self.supplementary_results = results;
    }

    fn display_paste_error(&mut self) {
        self.value1 = INVALID_INPUT.into();
        self.value2 = INVALID_INPUT.into();
    }

    fn map_character_to_command(&self, ch: char) -> Option<(Command, bool)> {
        if let Some(digit) = ch.to_digit(10) {
            return Command::from_digit(digit).map(|c| (c, true));
        }
        if ch == self.number_format.decimal_separator {
            return Some((Command::Decimal, true));
        }
        if ch == '-' {
            return Some((Command::Negate, false));
        }
        None
    }

    fn save_user_preferences(&mut self) {
        if self.unit1.is_none() || self.unit2.is_none() {
            return;
        }

        if !self.is_currency_current_category {
            self.preferences.unit_converter = Some(self.model.save_user_preferences());
        } else {
            let from = self
                .unit_from()
                .map(|u| u.abbreviation.clone())
                .unwrap_or_default();
            let to = self
                .unit_to()
                .map(|u| u.abbreviation.clone())
                .unwrap_or_default();
            if !from.is_empty() && !to.is_empty() {
                self.currency_loader_mut()
                    .set_last_used_currencies(&from, &to);
                self.preferences.currency_from = Some(from);
                self.preferences.currency_to = Some(to);
            }
        }
    }

    fn restore_user_preferences(&mut self) {
        if !self.is_currency_current_category
            && let Some(preferences) = self.preferences.unit_converter.clone()
        {
            self.model.restore_user_preferences(&preferences);
        }
    }

    fn on_currency_data_load_finished(&mut self, did_load: bool) {
        self.is_currency_data_loaded = true;
        if did_load && self.is_currency_current_category {
            self.model.calculate();
            self.process_events();
            self.reset_category();
        }
        self.is_currency_loading_visible = false;
        self.currency_data_load_failed = !did_load;
    }

    /// `ConvertToLocalizedString`: formats an unlocalized engine string for
    /// display (digit grouping, decimal separator, currency rounding).
    fn convert_to_localized_string(
        &self,
        string_to_localize: &str,
        allow_partial_strings: bool,
        currency_fraction_digits: Option<u32>,
    ) -> String {
        if string_to_localize.is_empty() {
            return "0".into();
        }
        // If units haven't been set, the currency formatters don't exist yet:
        // fall back to the default formatter. (Outside Currency the default
        // is always used; the original kept using a stale currency formatter.)
        let last_currency_fraction_digits = if self.is_currency_current_category {
            currency_fraction_digits.unwrap_or(DEFAULT_CURRENCY_FRACTION_DIGITS)
        } else {
            DEFAULT_CURRENCY_FRACTION_DIGITS
        };
        self.localize(
            string_to_localize,
            allow_partial_strings,
            last_currency_fraction_digits,
        )
    }

    fn localize(
        &self,
        s: &str,
        allow_partial_strings: bool,
        last_currency_fraction_digits: u32,
    ) -> String {
        if s.is_empty() {
            return "0".into();
        }
        if !is_parsable_double(s) {
            return self.localize_display_value(s);
        }

        // Handle scientific notation
        if let Some(pos_of_e) = s.find(['e', 'E']) {
            let rest = &s[pos_of_e + 1..];
            let sign_of_e = rest.chars().next().filter(|c| *c == '+' || *c == '-');
            let exponent = match sign_of_e {
                Some(sign) => &rest[sign.len_utf8()..],
                None => rest,
            };
            // The formatter's fraction digits are temporarily 0 while recursing.
            return format!(
                "{}e{}{}",
                self.localize(&s[..pos_of_e], allow_partial_strings, 0),
                sign_of_e.unwrap_or('+'),
                self.localize(exponent, allow_partial_strings, 0)
            );
        }

        let (negative, unsigned) = match s.as_bytes()[0] {
            b'-' => (true, &s[1..]),
            b'+' => (false, &s[1..]),
            _ => (false, s),
        };
        let (int_part, frac_part, has_decimal) = match unsigned.split_once('.') {
            Some((i, f)) => (i, f, true),
            None => (unsigned, "", false),
        };
        // A formatter prints a parsed number: no redundant leading zeros.
        let int_part = int_part.trim_start_matches('0');
        let int_part = if int_part.is_empty() { "0" } else { int_part };

        let decimal_point_always_displayed =
            has_decimal && allow_partial_strings && last_currency_fraction_digits > 0;
        let separator = &self.number_format.grouping_separator;
        let mut result = if negative {
            String::from("-")
        } else {
            String::new()
        };

        if self.is_currency_current_category {
            if has_decimal {
                // The currency formatter rounds (half down) to the currency's
                // increment and shows exactly that many fraction digits.
                let (int_rounded, frac_rounded) = round_decimal_half_down(
                    int_part,
                    frac_part,
                    last_currency_fraction_digits as usize,
                    negative,
                );
                result.push_str(&group_digits(&int_rounded, separator));
                if !frac_rounded.is_empty() || decimal_point_always_displayed {
                    result.push(self.number_format.decimal_separator);
                    result.push_str(&frac_rounded);
                }
            } else {
                result.push_str(&group_digits(int_part, separator));
            }
        } else {
            // Force post-decimal digits so trailing zeroes aren't cut off.
            result.push_str(&group_digits(int_part, separator));
            if !frac_part.is_empty() || decimal_point_always_displayed {
                result.push(self.number_format.decimal_separator);
                result.push_str(frac_part);
            }
        }
        result
    }

    /// `LocalizeDisplayValue`: only the decimal separator is localized.
    fn localize_display_value(&self, s: &str) -> String {
        s.chars()
            .map(|c| {
                if c == '.' {
                    self.number_format.decimal_separator
                } else {
                    c
                }
            })
            .collect()
    }
}

/// Prepares the unlocalized value for re-entry after the currency changed:
/// truncates it to `fraction_digits` decimals and localizes the decimal
/// separator. `None` for empty or scientific values.
pub fn try_prepare_currency_input_for_paste(
    value: &str,
    fraction_digits: u32,
    decimal_separator: char,
) -> Option<String> {
    if value.is_empty() || value.contains(['e', 'E']) {
        return None;
    }
    let mut prepared = truncate_fraction_digits(value, fraction_digits as usize);
    if decimal_separator != '.' {
        prepared = prepared.replace('.', &decimal_separator.to_string());
    }
    Some(prepared)
}

fn truncate_fraction_digits(n: &str, digit_count: usize) -> String {
    let Some(i) = n.find('.') else {
        return n.to_owned();
    };
    if digit_count == 0 {
        return n[..i].to_owned();
    }
    let actual_digit_count = n.len() - i - 1;
    if actual_digit_count <= digit_count {
        return n.to_owned();
    }
    n[..n.len() - (actual_digit_count - digit_count)].to_owned()
}

/// Whether `s` is a number `double.TryParse(NumberStyles.Float)` accepts
/// (sign, digits, decimal point, exponent; at least one digit).
fn is_parsable_double(s: &str) -> bool {
    let t = s.trim();
    t.bytes().any(|b| b.is_ascii_digit())
        && t.bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'))
        && t.parse::<f64>().is_ok_and(f64::is_finite)
}

/// Rounds the decimal `int.frac` to `digits` fraction digits, ties toward
/// negative infinity (`RoundingAlgorithm.RoundHalfDown`). Returns the integer
/// digits and exactly `digits` fraction digits.
fn round_decimal_half_down(
    int_part: &str,
    frac_part: &str,
    digits: usize,
    negative: bool,
) -> (String, String) {
    if frac_part.len() <= digits {
        let mut frac = frac_part.to_owned();
        while frac.len() < digits {
            frac.push('0');
        }
        return (int_part.to_owned(), frac);
    }

    let kept = &frac_part[..digits];
    let rest = &frac_part.as_bytes()[digits..];
    let round_magnitude_up = match rest[0] {
        b'6'..=b'9' => true,
        b'5' => rest[1..].iter().any(|&b| b != b'0') || negative,
        _ => false,
    };

    let mut all: Vec<u8> = int_part.bytes().chain(kept.bytes()).collect();
    if round_magnitude_up {
        let mut i = all.len();
        loop {
            if i == 0 {
                all.insert(0, b'1');
                break;
            }
            i -= 1;
            if all[i] == b'9' {
                all[i] = b'0';
            } else {
                all[i] += 1;
                break;
            }
        }
    }
    let split = all.len() - digits;
    let int_rounded = String::from_utf8(all[..split].to_vec()).expect("ascii digits");
    let frac_rounded = String::from_utf8(all[split..].to_vec()).expect("ascii digits");
    (
        if int_rounded.is_empty() {
            "0".into()
        } else {
            int_rounded
        },
        frac_rounded,
    )
}

fn lock<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_down_rounding() {
        assert_eq!(
            round_decimal_half_down("1", "5", 2, false),
            ("1".into(), "50".into())
        );
        assert_eq!(
            round_decimal_half_down("0", "125", 2, false),
            ("0".into(), "12".into())
        );
        assert_eq!(
            round_decimal_half_down("0", "125", 2, true),
            ("0".into(), "13".into())
        );
        assert_eq!(
            round_decimal_half_down("0", "1251", 2, false),
            ("0".into(), "13".into())
        );
        assert_eq!(
            round_decimal_half_down("9", "996", 2, false),
            ("10".into(), "00".into())
        );
        assert_eq!(
            round_decimal_half_down("15794", "56", 0, false),
            ("15795".into(), "".into())
        );
        assert_eq!(
            round_decimal_half_down("15794", "5", 0, false),
            ("15794".into(), "".into())
        );
        assert_eq!(
            round_decimal_half_down("88", "356000000000009", 2, false),
            ("88".into(), "36".into())
        );
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate_fraction_digits("1.2345", 2), "1.23");
        assert_eq!(truncate_fraction_digits("1.2", 2), "1.2");
        assert_eq!(truncate_fraction_digits("1.2", 0), "1");
        assert_eq!(truncate_fraction_digits("12", 2), "12");
        assert_eq!(
            try_prepare_currency_input_for_paste("1.5e+20", 2, '.'),
            None
        );
        assert_eq!(try_prepare_currency_input_for_paste("", 2, '.'), None);
        assert_eq!(
            try_prepare_currency_input_for_paste("3.14159", 2, ','),
            Some("3,14".into())
        );
    }

    #[test]
    fn parsable() {
        assert!(is_parsable_double("1."));
        assert!(is_parsable_double(".5"));
        assert!(is_parsable_double("-0"));
        assert!(is_parsable_double("4.535920e-15"));
        assert!(!is_parsable_double("-"));
        assert!(!is_parsable_double("."));
        assert!(!is_parsable_double("inf"));
        assert!(!is_parsable_double("Invalid input"));
    }
}
