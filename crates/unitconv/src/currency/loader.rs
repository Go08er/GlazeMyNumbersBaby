// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Rust port of Calculator.ViewModels/DataLoaders/CurrencyDataLoader.cs.

//! The currency data loader: turns an exchange rate snapshot into converter
//! units and ratios, formats the ratio line ("1 USD = 0.8836 EUR") and the
//! `Updated <date> <time>` timestamp, and implements the original
//! cache / web / refresh state machine without doing any I/O on the network
//! itself.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, Datelike, FixedOffset, Local, TimeDelta, TimeZone, Timelike, Utc};

use super::info::{currency_info, is_excluded};
use super::snapshot::{CurrencySnapshot, load_cache, save_cache};
use super::{CurrencyError, NetworkAccessBehavior};
use crate::converter::{
    Category, ConversionData, ConverterDataLoader, CurrencyConverterDataLoader, CurrencyStaticData,
    SharedCurrencyCallback, Unit, UnitRatios,
};
use crate::number_formatting::group_digits;
use crate::resources::{format_resource, resource_string};

/// Category id of the currency converter (`NavCategoryStates` serialization id).
pub const CURRENCY_CATEGORY_ID: i32 = 16;

/// Currency unit ids start after the last static unit id.
const UNIT_END: i32 = 168;
const DEFAULT_CURRENCY_CODE: &str = "USD";
const DEFAULT_FROM_CURRENCY: &str = DEFAULT_CURRENCY_CODE;
const DEFAULT_TO_CURRENCY: &str = "EUR";
const DEFAULT_FROM_TO_CURRENCY_JSON: &str = include_str!("../../data/DefaultFromToCurrency.json");
const FROM_KEY: &str = "from";
const TO_KEY: &str = "to";

const FORMATTER_RATE_FRACTION_PADDING: usize = 2;
const FORMATTER_RATE_MIN_DECIMALS: i32 = 4;
const FORMATTER_RATE_MIN_SIGNIFICANT_DECIMALS: i32 = 4;

/// Age after which cached rates are refreshed from the web.
pub const DAY_DURATION: TimeDelta = TimeDelta::days(1);
/// Age after which rates are flagged as out of date (`CurrencyDataIsWeekOld`).
pub const WEEK_DURATION: TimeDelta = TimeDelta::days(7);

/// Load state (`CurrencyLoadStatus`, plus [`LoadedFromBundle`](Self::LoadedFromBundle)).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CurrencyLoadStatus {
    #[default]
    NotLoaded,
    FailedToLoad,
    LoadedFromCache,
    LoadedFromWeb,
    /// Loaded from the snapshot compiled into the crate (new in this port;
    /// the original had no offline fallback).
    LoadedFromBundle,
}

/// Where the currently loaded rates came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrencyDataSource {
    Cache,
    Web,
    Bundled,
}

/// A currency unit (`CurrencyUnit` + `CurrencyUnitMetadata`).
#[derive(Clone, Debug, PartialEq)]
pub struct CurrencyUnit {
    pub id: i32,
    /// Currency name, e.g. `"Dollar"`.
    pub name: String,
    /// Country / region name, e.g. `"United States"`.
    pub country_name: String,
    /// ISO code, e.g. `"USD"`.
    pub abbreviation: String,
    pub is_rtl_language: bool,
    pub is_conversion_source: bool,
    pub is_conversion_target: bool,
    /// Currency symbol, e.g. `"$"`.
    pub symbol: String,
    /// Number of fraction digits used to display amounts.
    pub fraction_digits: u32,
}

impl CurrencyUnit {
    /// The converter unit (`"<country> - <currency>"`).
    pub fn to_unit(&self) -> Unit {
        Unit::new_currency(
            self.id,
            &self.name,
            &self.country_name,
            self.abbreviation.clone(),
            self.is_rtl_language,
            self.is_conversion_source,
            self.is_conversion_target,
        )
    }

    /// Screen readers get the country and currency names rather than the
    /// abbreviation, composed the same way the unit list composes them.
    fn accessible_name(&self) -> String {
        let (first, second) = if self.is_rtl_language {
            (&self.name, &self.country_name)
        } else {
            (&self.country_name, &self.name)
        };
        format!("{first} {second}")
    }
}

/// Source of "now" (injectable for tests).
pub type Clock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

/// Construction parameters for [`CurrencyDataLoader`].
#[derive(Clone)]
pub struct CurrencyDataLoaderConfig {
    /// Language tag used to pick the default currency pair from
    /// `DefaultFromToCurrency.json` (e.g. `"en-GB"` → GBP to USD).
    pub response_language: String,
    /// File the latest fetched snapshot is cached in (the GUI picks an XDG
    /// cache path). `None` disables caching.
    pub cache_path: Option<PathBuf>,
    /// Whether the UI language is right-to-left (unit names become
    /// `"<currency> - <country>"`).
    pub is_rtl_language: bool,
    /// Snapshot used when there is no usable cache; `None` uses
    /// [`CurrencySnapshot::bundled`].
    pub fallback_snapshot: Option<CurrencySnapshot>,
    /// Clock; `None` uses the system clock.
    pub clock: Option<Clock>,
}

impl Default for CurrencyDataLoaderConfig {
    fn default() -> Self {
        CurrencyDataLoaderConfig {
            response_language: "en-US".into(),
            cache_path: None,
            is_rtl_language: false,
            fallback_snapshot: None,
            clock: None,
        }
    }
}

impl std::fmt::Debug for CurrencyDataLoaderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CurrencyDataLoaderConfig")
            .field("response_language", &self.response_language)
            .field("cache_path", &self.cache_path)
            .field("is_rtl_language", &self.is_rtl_language)
            .field(
                "fallback_snapshot",
                &self.fallback_snapshot.as_ref().map(|s| &s.rates_date),
            )
            .finish_non_exhaustive()
    }
}

/// Loads currency data and provides currency units and ratios to the
/// converter engine (`CurrencyDataLoader`).
pub struct CurrencyDataLoader {
    response_language: String,
    is_rtl_language: bool,
    cache_path: Option<PathBuf>,
    fallback_snapshot: Option<CurrencySnapshot>,
    clock: Clock,

    currency_units: Vec<CurrencyUnit>,
    currency_ratio_map: HashMap<i32, HashMap<i32, ConversionData>>,
    snapshot: Option<CurrencySnapshot>,

    vm_callback: Option<SharedCurrencyCallback>,
    cache_timestamp: Option<DateTime<Utc>>,
    load_status: CurrencyLoadStatus,
    data_source: Option<CurrencyDataSource>,

    network_access_behavior: NetworkAccessBehavior,
    metered_override_set: bool,
    web_refresh_attempted: bool,

    /// `CURRENCY_UNIT_FROM_KEY` / `CURRENCY_UNIT_TO_KEY` local settings.
    last_used_currencies: Option<(String, String)>,
}

impl Default for CurrencyDataLoader {
    fn default() -> Self {
        Self::new(CurrencyDataLoaderConfig::default())
    }
}

impl std::fmt::Debug for CurrencyDataLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CurrencyDataLoader")
            .field("load_status", &self.load_status)
            .field("data_source", &self.data_source)
            .field("units", &self.currency_units.len())
            .field("cache_timestamp", &self.cache_timestamp)
            .finish_non_exhaustive()
    }
}

impl CurrencyDataLoader {
    pub fn new(config: CurrencyDataLoaderConfig) -> Self {
        CurrencyDataLoader {
            response_language: if config.response_language.is_empty() {
                "en-US".into()
            } else {
                config.response_language
            },
            is_rtl_language: config.is_rtl_language,
            cache_path: config.cache_path,
            fallback_snapshot: config.fallback_snapshot,
            clock: config.clock.unwrap_or_else(|| Arc::new(Utc::now)),
            currency_units: Vec::new(),
            currency_ratio_map: HashMap::new(),
            snapshot: None,
            vm_callback: None,
            cache_timestamp: None,
            load_status: CurrencyLoadStatus::NotLoaded,
            data_source: None,
            network_access_behavior: NetworkAccessBehavior::Normal,
            metered_override_set: false,
            web_refresh_attempted: false,
            last_used_currencies: None,
        }
    }

    fn now(&self) -> DateTime<Utc> {
        (self.clock)()
    }

    pub fn load_finished(&self) -> bool {
        self.load_status != CurrencyLoadStatus::NotLoaded
    }

    pub fn loaded_from_cache(&self) -> bool {
        self.load_status == CurrencyLoadStatus::LoadedFromCache
    }

    pub fn loaded_from_web(&self) -> bool {
        self.load_status == CurrencyLoadStatus::LoadedFromWeb
    }

    pub fn load_status(&self) -> CurrencyLoadStatus {
        self.load_status
    }

    /// Where the rates currently in use came from (`None` before any load).
    pub fn data_source(&self) -> Option<CurrencyDataSource> {
        self.data_source
    }

    /// The snapshot currently in use.
    pub fn snapshot(&self) -> Option<&CurrencySnapshot> {
        self.snapshot.as_ref()
    }

    /// When the rates in use were retrieved.
    pub fn cache_timestamp(&self) -> Option<DateTime<Utc>> {
        self.cache_timestamp
    }

    /// The configured cache file.
    pub fn cache_path(&self) -> Option<&std::path::Path> {
        self.cache_path.as_deref()
    }

    pub fn set_cache_path(&mut self, path: Option<PathBuf>) {
        self.cache_path = path;
    }

    /// Currency units in display order (sorted by country name).
    pub fn get_ordered_currency_units(&self) -> Vec<CurrencyUnit> {
        self.currency_units.clone()
    }

    /// Currency units in display order, borrowed.
    pub fn currency_units(&self) -> &[CurrencyUnit] {
        &self.currency_units
    }

    /// Looks up a loaded currency by id.
    pub fn currency_unit(&self, unit_id: i32) -> Option<&CurrencyUnit> {
        self.currency_units.iter().find(|u| u.id == unit_id)
    }

    /// Looks up a loaded currency by ISO code.
    pub fn currency_unit_by_code(&self, code: &str) -> Option<&CurrencyUnit> {
        self.currency_units.iter().find(|u| u.abbreviation == code)
    }

    /// Ratios from `unit_id` to every loaded currency, by target id.
    pub fn load_ordered_currency_ratios(&self, unit_id: i32) -> HashMap<i32, ConversionData> {
        self.currency_ratio_map
            .get(&unit_id)
            .cloned()
            .unwrap_or_default()
    }

    /// The symbols of two currencies, or empty strings if either is unknown.
    pub fn get_currency_symbols_by_id(&self, unit1_id: i32, unit2_id: i32) -> (String, String) {
        match (self.currency_unit(unit1_id), self.currency_unit(unit2_id)) {
            (Some(u1), Some(u2)) => (u1.symbol.clone(), u2.symbol.clone()),
            _ => (String::new(), String::new()),
        }
    }

    /// Fraction digits used to display amounts of a currency (`None` if unknown).
    pub fn fraction_digits(&self, unit_id: i32) -> Option<u32> {
        self.currency_unit(unit_id).map(|u| u.fraction_digits)
    }

    /// Rounds a ratio so that at least four meaningful decimals are shown:
    /// `0.00000000342334 -> 0.000000003423`, `0.000212 -> 0.000212`.
    ///
    /// Uses round-half-to-even like .NET's `Math.Round`.
    pub fn round_currency_ratio(ratio: f64) -> f64 {
        // Compute how many decimals we need to display two meaningful digits at minimum.
        let mut number_decimals = FORMATTER_RATE_MIN_DECIMALS;
        if ratio < 1.0 {
            let neg_log = -ratio.log10();
            // C#'s (int) cast of NaN/infinity yields int.MinValue.
            let truncated = if neg_log.is_finite() {
                neg_log as i32
            } else {
                i32::MIN
            };
            number_decimals = FORMATTER_RATE_MIN_DECIMALS
                .max(truncated.saturating_add(FORMATTER_RATE_MIN_SIGNIFICANT_DECIMALS));
        }

        let scale = 10f64.powi(number_decimals);
        (ratio * scale).round_ties_even() / scale
    }

    /// The ratio line and its accessible form, e.g.
    /// `("1 USD = 0.8836 EUR", "1 United States Dollar = 0.8836 Europe Euro")`.
    /// Empty strings if either currency is unknown.
    pub fn get_currency_ratio_equality_by_id(
        &self,
        unit1_id: i32,
        unit2_id: i32,
    ) -> (String, String) {
        let Some(ratio) = self
            .currency_ratio_map
            .get(&unit1_id)
            .and_then(|m| m.get(&unit2_id))
            .map(|d| d.ratio)
        else {
            return (String::new(), String::new());
        };
        let (Some(unit1), Some(unit2)) =
            (self.currency_unit(unit1_id), self.currency_unit(unit2_id))
        else {
            return (String::new(), String::new());
        };

        let rounded = Self::round_currency_ratio(ratio);
        let digit_symbol = "1";
        let rounded_format = format_ratio(rounded);
        let ratio_format = resource_string("CurrencyFromToRatioFormat");
        let ratio_string = format_resource(
            ratio_format,
            &[
                digit_symbol,
                &unit1.abbreviation,
                &rounded_format,
                &unit2.abbreviation,
            ],
        );
        let accessible_ratio_string = format_resource(
            ratio_format,
            &[
                digit_symbol,
                &unit1.accessible_name(),
                &rounded_format,
                &unit2.accessible_name(),
            ],
        );
        (ratio_string, accessible_ratio_string)
    }

    /// `Updated <short date> <short time>` in local time, or `""` if no
    /// rates are loaded.
    pub fn get_currency_timestamp(&self) -> String {
        match self.cache_timestamp {
            Some(ts) => {
                let local = ts.with_timezone(&Local);
                format_resource(
                    resource_string("CurrencyTimestampFormat"),
                    &[&format_short_date(&local), &format_short_time(&local)],
                )
            }
            None => String::new(),
        }
    }

    /// Whether the rates in use are more than a week old.
    pub fn is_week_old(&self) -> bool {
        self.is_older_than(WEEK_DURATION)
    }

    fn is_older_than(&self, duration: TimeDelta) -> bool {
        self.cache_timestamp
            .is_none_or(|ts| ts + duration < self.now())
    }

    /// The current network access behaviour.
    pub fn network_access_behavior(&self) -> NetworkAccessBehavior {
        self.network_access_behavior
    }

    /// Updates the network access behaviour (`OnNetworkBehaviorChanged`);
    /// the GUI calls this from its network monitor.
    pub fn on_network_behavior_changed(&mut self, new_behavior: NetworkAccessBehavior) {
        if new_behavior != self.network_access_behavior
            && new_behavior == NetworkAccessBehavior::Normal
        {
            // Connectivity came back: allow another automatic refresh.
            self.web_refresh_attempted = false;
        }
        self.network_access_behavior = new_behavior;
        if let Some(callback) = &self.vm_callback {
            lock(callback).network_behavior_changed(new_behavior);
        }
    }

    /// Whether the GUI should fetch fresh rates in the background: the data
    /// in use is missing, bundled or more than a day old, the network is
    /// unrestricted and no automatic attempt has been made yet.
    ///
    /// (The original performed this web request inside `LoadData`.)
    pub fn needs_web_refresh(&self) -> bool {
        let stale = match self.data_source {
            None | Some(CurrencyDataSource::Bundled) => true,
            Some(CurrencyDataSource::Cache) | Some(CurrencyDataSource::Web) => {
                self.is_older_than(DAY_DURATION)
            }
        };
        self.network_access_behavior == NetworkAccessBehavior::Normal
            && !self.web_refresh_attempted
            && stale
    }

    /// The last used currency pair (`CURRENCY_UNIT_FROM_KEY` / `_TO_KEY`).
    pub fn last_used_currencies(&self) -> Option<(&str, &str)> {
        self.last_used_currencies
            .as_ref()
            .map(|(f, t)| (f.as_str(), t.as_str()))
    }

    /// Remembers the last used currency pair; used as the default selection
    /// the next time units are built.
    pub fn set_last_used_currencies(&mut self, from: &str, to: &str) {
        self.last_used_currencies = Some((from.to_owned(), to.to_owned()));
    }

    /// Loads `snapshot` directly (e.g. one obtained elsewhere), as if it came
    /// from `source`. Returns false if the snapshot has no usable rates.
    pub fn load_snapshot(
        &mut self,
        snapshot: CurrencySnapshot,
        source: CurrencyDataSource,
    ) -> bool {
        if snapshot.validate().is_err() {
            return false;
        }
        self.cache_timestamp = Some(snapshot.fetched_at);
        self.load_status = match source {
            CurrencyDataSource::Cache => CurrencyLoadStatus::LoadedFromCache,
            CurrencyDataSource::Web => CurrencyLoadStatus::LoadedFromWeb,
            CurrencyDataSource::Bundled => CurrencyLoadStatus::LoadedFromBundle,
        };
        self.data_source = Some(source);
        self.finalize_units(snapshot);
        true
    }

    fn load_fallback_snapshot(&mut self) -> bool {
        let snapshot = self
            .fallback_snapshot
            .clone()
            .unwrap_or_else(|| CurrencySnapshot::bundled_ref().clone());
        self.load_snapshot(snapshot, CurrencyDataSource::Bundled)
    }

    /// Completes the automatic background refresh started because
    /// [`needs_web_refresh`](Self::needs_web_refresh) was true (the web half
    /// of the original `LoadData`). On failure the rates already loaded
    /// (cache or bundled) stay in use and still count as loaded.
    pub fn finish_web_load(&mut self, fetched: Result<CurrencySnapshot, CurrencyError>) -> bool {
        let previous_status = self.load_status;
        let had_data = !self.currency_units.is_empty();
        self.web_refresh_attempted = true;

        let did_load = self.try_load_data_from_web(fetched);
        if !did_load {
            self.load_status = if had_data && previous_status != CurrencyLoadStatus::NotLoaded {
                previous_status
            } else {
                CurrencyLoadStatus::FailedToLoad
            };
        }

        self.update_displayed_timestamp();
        self.notify_data_load_finished(did_load || had_data);
        did_load
    }

    fn reset_load_status(&mut self) {
        self.load_status = CurrencyLoadStatus::NotLoaded;
    }

    fn notify_data_load_finished(&mut self, did_load: bool) {
        if !did_load {
            self.load_status = CurrencyLoadStatus::FailedToLoad;
        }
        if let Some(callback) = &self.vm_callback {
            lock(callback).currency_data_load_finished(did_load);
        }
    }

    fn update_displayed_timestamp(&self) {
        if let Some(callback) = &self.vm_callback {
            let timestamp = self.get_currency_timestamp();
            let is_week_old = self.is_week_old();
            lock(callback).currency_timestamp_callback(&timestamp, is_week_old);
        }
    }

    fn finalize_units(&mut self, snapshot: CurrencySnapshot) {
        let (mut from_currency, mut to_currency) = self.get_default_from_to_currency();

        let mut seen = HashSet::new();
        let mut static_data: Vec<(CurrencyStaticData, f64)> = snapshot
            .currencies
            .iter()
            .filter(|c| !is_excluded(&c.code) && seen.insert(c.code.clone()))
            .map(|c| (static_data_for(c), c.rate))
            .collect();
        static_data.sort_by(|(a, _), (b, _)| {
            collation_key(&a.country_name)
                .cmp(&collation_key(&b.country_name))
                .then_with(|| a.country_name.cmp(&b.country_name))
                .then_with(|| collation_key(&a.currency_name).cmp(&collation_key(&b.currency_name)))
                .then_with(|| a.currency_code.cmp(&b.currency_code))
        });

        self.currency_units.clear();
        let mut base_ratios: Vec<f64> = Vec::new();
        let mut is_conversion_source_set = false;
        let mut is_conversion_target_set = false;
        let mut i = 1;
        for (currency_unit, ratio) in static_data {
            if ratio.is_finite() && ratio > 0.0 {
                let id = UNIT_END + i;

                let is_conversion_source = from_currency == currency_unit.currency_code;
                is_conversion_source_set = is_conversion_source_set || is_conversion_source;

                let is_conversion_target = to_currency == currency_unit.currency_code;
                is_conversion_target_set = is_conversion_target_set || is_conversion_target;

                self.currency_units.push(CurrencyUnit {
                    id,
                    fraction_digits: super::info::fraction_digits(&currency_unit.currency_code),
                    name: currency_unit.currency_name,
                    country_name: currency_unit.country_name,
                    abbreviation: currency_unit.currency_code,
                    is_rtl_language: self.is_rtl_language,
                    is_conversion_source,
                    is_conversion_target,
                    symbol: currency_unit.currency_symbol,
                });
                base_ratios.push(ratio);
                i += 1;
            }
        }

        if !is_conversion_source_set || !is_conversion_target_set {
            self.guarantee_selected_units();
            from_currency = DEFAULT_FROM_CURRENCY.into();
            to_currency = DEFAULT_TO_CURRENCY.into();
        }

        self.currency_ratio_map.clear();
        for (unit, &unit_factor) in self.currency_units.iter().zip(&base_ratios) {
            let conversions = self
                .currency_units
                .iter()
                .zip(&base_ratios)
                .map(|(target, &conversion_ratio)| {
                    (
                        target.id,
                        ConversionData::ratio(conversion_ratio / unit_factor),
                    )
                })
                .collect();
            self.currency_ratio_map.insert(unit.id, conversions);
        }

        self.snapshot = Some(snapshot);
        // SaveSelectedUnitsToLocalSettings
        self.last_used_currencies = Some((from_currency, to_currency));
    }

    fn guarantee_selected_units(&mut self) {
        let mut is_conversion_source_set = false;
        let mut is_conversion_target_set = false;

        for unit in &mut self.currency_units {
            unit.is_conversion_source = false;
            unit.is_conversion_target = false;

            if !is_conversion_source_set && unit.abbreviation == DEFAULT_FROM_CURRENCY {
                unit.is_conversion_source = true;
                is_conversion_source_set = true;
            }
            if !is_conversion_target_set && unit.abbreviation == DEFAULT_TO_CURRENCY {
                unit.is_conversion_target = true;
                is_conversion_target_set = true;
            }
        }

        if let Some(first) = self.currency_units.first_mut() {
            if !is_conversion_source_set {
                first.is_conversion_source = true;
            }
            if !is_conversion_target_set {
                first.is_conversion_target = true;
            }
        }
    }

    fn get_default_from_to_currency(&self) -> (String, String) {
        // First, check if we previously stored the last used currencies.
        if let Some((from, to)) = &self.last_used_currencies {
            return (from.clone(), to.clone());
        }

        // Second, see if the current locale has preset defaults in DefaultFromToCurrency.json.
        if let Ok(serde_json::Value::Object(map)) =
            serde_json::from_str(DEFAULT_FROM_TO_CURRENCY_JSON)
            && let Some(regional) = map.get(&self.response_language)
            && let (Some(from), Some(to)) = (
                regional.get(FROM_KEY).and_then(|v| v.as_str()),
                regional.get(TO_KEY).and_then(|v| v.as_str()),
            )
        {
            return (from.to_owned(), to.to_owned());
        }

        (DEFAULT_FROM_CURRENCY.into(), DEFAULT_TO_CURRENCY.into())
    }
}

impl ConverterDataLoader for CurrencyDataLoader {
    /// Loads cached rates (or, failing that, the bundled snapshot) and
    /// reports completion. Web requests are left to the caller; see
    /// [`CurrencyDataLoader::needs_web_refresh`].
    fn load_data(&mut self) {
        if self.load_finished() {
            return;
        }

        // RegisterForNetworkBehaviorChanges: report the current behaviour.
        self.on_network_behavior_changed(self.network_access_behavior);

        let mut did_load = self.try_load_data_from_cache();

        // A snapshot bundled with a newer build beats an older cache.
        let fallback_is_newer = || {
            let fallback_time = self.fallback_snapshot.as_ref().map_or_else(
                || CurrencySnapshot::bundled_ref().fetched_at,
                |s| s.fetched_at,
            );
            self.cache_timestamp
                .is_some_and(|cached| fallback_time > cached)
        };
        if !did_load || fallback_is_newer() {
            // The original tries the web here; that is the caller's job
            // (needs_web_refresh), so use the offline snapshot meanwhile.
            did_load = self.load_fallback_snapshot() || did_load;
        }

        self.update_displayed_timestamp();
        self.notify_data_load_finished(did_load);
    }

    fn get_ordered_categories(&self) -> Vec<Category> {
        // The model uses the categories from UnitConverterDataLoader.
        Vec::new()
    }

    fn get_ordered_units(&self, _category: &Category) -> Vec<Unit> {
        self.currency_units
            .iter()
            .map(CurrencyUnit::to_unit)
            .collect()
    }

    fn load_ordered_ratios(&self, unit: &Unit) -> UnitRatios {
        let Some(ratios) = self.currency_ratio_map.get(&unit.id) else {
            return UnitRatios::new();
        };
        self.currency_units
            .iter()
            .filter_map(|target| ratios.get(&target.id).map(|data| (target.to_unit(), *data)))
            .collect()
    }

    fn supports_category(&self, target: &Category) -> bool {
        target.id == CURRENCY_CATEGORY_ID
    }

    fn as_currency_loader(&self) -> Option<&dyn CurrencyConverterDataLoader> {
        Some(self)
    }

    fn as_currency_loader_mut(&mut self) -> Option<&mut dyn CurrencyConverterDataLoader> {
        Some(self)
    }
}

impl CurrencyConverterDataLoader for CurrencyDataLoader {
    fn set_view_model_callback(&mut self, callback: Option<SharedCurrencyCallback>) {
        self.vm_callback = callback;
        self.on_network_behavior_changed(self.network_access_behavior);
    }

    fn get_currency_symbols(&self, unit1: &Unit, unit2: &Unit) -> (String, String) {
        self.get_currency_symbols_by_id(unit1.id, unit2.id)
    }

    fn get_currency_ratio_equality(&self, unit1: &Unit, unit2: &Unit) -> (String, String) {
        self.get_currency_ratio_equality_by_id(unit1.id, unit2.id)
    }

    fn get_currency_timestamp(&self) -> String {
        CurrencyDataLoader::get_currency_timestamp(self)
    }

    /// Loads the cache file. If the cached rates are more than a day old the
    /// original first tried the web; here they are loaded and
    /// [`needs_web_refresh`](CurrencyDataLoader::needs_web_refresh) reports
    /// that a fetch is due.
    fn try_load_data_from_cache(&mut self) -> bool {
        self.reset_load_status();

        let Some(path) = &self.cache_path else {
            return false;
        };
        let Ok(snapshot) = load_cache(path) else {
            return false;
        };

        // (The original also rejected caches written for another response
        // language; names now come from a built-in table, so that is moot.)
        self.cache_timestamp = Some(snapshot.fetched_at);
        self.load_status = CurrencyLoadStatus::LoadedFromCache;
        self.data_source = Some(CurrencyDataSource::Cache);
        self.finalize_units(snapshot);
        true
    }

    fn try_load_data_from_web(&mut self, fetched: Result<CurrencySnapshot, CurrencyError>) -> bool {
        self.load_from_web(fetched, false)
    }

    fn try_load_data_from_web_override(
        &mut self,
        fetched: Result<CurrencySnapshot, CurrencyError>,
    ) -> bool {
        self.metered_override_set = true;
        self.web_refresh_attempted = true;
        let did_load = self.load_from_web(fetched, true);
        if !did_load {
            self.load_status = CurrencyLoadStatus::FailedToLoad;
        }

        self.update_displayed_timestamp();
        did_load
    }
}

impl CurrencyDataLoader {
    /// Adopts fetched rates. The network behaviour gates only automatic
    /// refreshes: it comes from a connectivity monitor that can be wrong
    /// (VPNs, sandboxes, systems without NetworkManager), and rates the
    /// user explicitly asked for that did arrive must not be thrown away.
    fn load_from_web(
        &mut self,
        fetched: Result<CurrencySnapshot, CurrencyError>,
        explicit: bool,
    ) -> bool {
        self.reset_load_status();

        if !explicit
            && (self.network_access_behavior == NetworkAccessBehavior::Offline
                || (self.network_access_behavior == NetworkAccessBehavior::OptIn
                    && !self.metered_override_set))
        {
            return false;
        }

        let snapshot = match fetched {
            Ok(snapshot) if snapshot.validate().is_ok() => snapshot,
            _ => return false,
        };

        self.cache_timestamp = Some(snapshot.fetched_at);

        // If we fail to save to cache it's okay, we should still continue.
        if let Some(path) = &self.cache_path {
            let _ = save_cache(path, &snapshot);
        }

        self.load_status = CurrencyLoadStatus::LoadedFromWeb;
        self.data_source = Some(CurrencyDataSource::Web);
        self.finalize_units(snapshot);
        true
    }
}

fn lock<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Static data for a currency from the built-in table, falling back to what
/// the provider published.
fn static_data_for(rate: &super::CurrencyRate) -> CurrencyStaticData {
    let country_code: String = rate.code.chars().take(2).collect();
    match currency_info(&rate.code) {
        Some(info) => CurrencyStaticData {
            country_code,
            country_name: info.country_name.into(),
            currency_code: rate.code.clone(),
            currency_name: info.currency_name.into(),
            currency_symbol: info.symbol.into(),
        },
        None => CurrencyStaticData {
            country_code,
            country_name: rate.name.clone(),
            currency_code: rate.code.clone(),
            currency_name: rate.code.clone(),
            currency_symbol: rate
                .symbol
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "¤".into()),
        },
    }
}

/// Approximates a culture-aware (en-US) string comparison: case and common
/// Latin diacritics are ignored at the primary level ("Éclair" sorts between
/// "Alpha" and "Zebra").
fn collation_key(s: &str) -> String {
    let mut key = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => key.push('a'),
            'ç' | 'ć' | 'č' => key.push('c'),
            'ď' | 'ð' => key.push('d'),
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => key.push('e'),
            'ì' | 'í' | 'î' | 'ï' | 'ī' => key.push('i'),
            'ł' => key.push('l'),
            'ñ' | 'ń' | 'ň' => key.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => key.push('o'),
            'ř' => key.push('r'),
            'ś' | 'š' | 'ş' | 'ș' => key.push('s'),
            'ť' | 'ţ' | 'ț' => key.push('t'),
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => key.push('u'),
            'ý' | 'ÿ' => key.push('y'),
            'ź' | 'ż' | 'ž' => key.push('z'),
            'ß' => key.push_str("ss"),
            'æ' => key.push_str("ae"),
            'œ' => key.push_str("oe"),
            'þ' => key.push_str("th"),
            // Apostrophes and hyphens are ignored by word sort.
            '\'' | '’' | 'ʻ' | '-' => {}
            c => key.push(c),
        }
    }
    key
}

/// Formats a currency ratio like the original ratio `DecimalFormatter`
/// (grouped, decimal point always displayed, at least two fraction digits).
fn format_ratio(value: f64) -> String {
    // `{}` prints the shortest representation that round-trips, never in
    // scientific notation.
    let s = format!("{value}");
    let (sign, s) = match s.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", s.as_str()),
    };
    let (int_part, frac_part) = s.split_once('.').unwrap_or((s, ""));
    let mut frac = frac_part.to_owned();
    while frac.len() < FORMATTER_RATE_FRACTION_PADDING {
        frac.push('0');
    }
    format!("{sign}{}.{frac}", group_digits(int_part, ","))
}

/// en-US short date (`M/d/yyyy`), e.g. `10/1/2026`.
pub fn format_short_date<Tz: TimeZone>(dt: &DateTime<Tz>) -> String {
    format!("{}/{}/{}", dt.month(), dt.day(), dt.year())
}

/// en-US short time (`h:mm tt`), e.g. `9:47 PM`.
pub fn format_short_time<Tz: TimeZone>(dt: &DateTime<Tz>) -> String {
    let (is_pm, hour) = dt.hour12();
    format!(
        "{}:{:02} {}",
        hour,
        dt.minute(),
        if is_pm { "PM" } else { "AM" }
    )
}

/// `Updated <short date> <short time>` for `timestamp` shown at UTC offset `offset`.
pub fn format_timestamp(timestamp: DateTime<Utc>, offset: FixedOffset) -> String {
    let local = timestamp.with_timezone(&offset);
    format_resource(
        resource_string("CurrencyTimestampFormat"),
        &[&format_short_date(&local), &format_short_time(&local)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_formatting() {
        assert_eq!(format_ratio(0.8836), "0.8836");
        assert_eq!(format_ratio(1.0), "1.00");
        assert_eq!(format_ratio(1234567.0), "1,234,567.00");
        assert_eq!(format_ratio(4815.1623), "4,815.1623");
        assert_eq!(format_ratio(0.00000000000008723), "0.00000000000008723");
    }

    #[test]
    fn grouping() {
        assert_eq!(group_digits("1", ","), "1");
        assert_eq!(group_digits("123", ","), "123");
        assert_eq!(group_digits("1234", ","), "1,234");
        assert_eq!(group_digits("1234567", ","), "1,234,567");
        assert_eq!(group_digits("123456", "\u{a0}"), "123\u{a0}456");
    }

    #[test]
    fn collation() {
        let mut names = vec![
            "Zebra",
            "Éclair",
            "Alpha",
            "Türkiye",
            "Turkmenistan",
            "Tunisia",
        ];
        names.sort_by_key(|n| collation_key(n));
        assert_eq!(
            names,
            [
                "Alpha",
                "Éclair",
                "Tunisia",
                "Türkiye",
                "Turkmenistan",
                "Zebra"
            ]
        );
    }

    #[test]
    fn timestamp_format() {
        let ts = Utc.with_ymd_and_hms(2026, 10, 2, 2, 2, 40).unwrap();
        let cdt = FixedOffset::west_opt(5 * 3600).unwrap();
        assert_eq!(format_timestamp(ts, cdt), "Updated 10/1/2026 9:02 PM");
        assert_eq!(
            format_timestamp(ts, FixedOffset::east_opt(0).unwrap()),
            "Updated 10/2/2026 2:02 AM"
        );
        let noon = Utc.with_ymd_and_hms(2026, 1, 5, 12, 0, 0).unwrap();
        assert_eq!(
            format_timestamp(noon, FixedOffset::east_opt(0).unwrap()),
            "Updated 1/5/2026 12:00 PM"
        );
        let midnight = Utc.with_ymd_and_hms(2026, 1, 5, 0, 7, 0).unwrap();
        assert_eq!(
            format_timestamp(midnight, FixedOffset::east_opt(0).unwrap()),
            "Updated 1/5/2026 12:07 AM"
        );
    }
}
