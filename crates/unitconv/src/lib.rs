// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Rust port of the unit converter of Windows Calculator
// (https://github.com/microsoft/calculator): CalcManager/UnitConverter,
// Calculator.ViewModels/DataLoaders and Calculator.ViewModels/UnitConverterViewModel.

//! # unitconv
//!
//! The unit converter of Windows Calculator — including currency — ported to
//! Rust, UI-agnostic.
//!
//! * [`converter`]: the double-based [`UnitConverter`] engine (input editing,
//!   from/to switching, rounding and precision rules, suggested values).
//! * [`data_loader`]: every category and unit with exact ratios, default
//!   unit pairs, ordering and the whimsical units (bananas, jumbo jets, ...).
//! * [`currency`]: exchange rates from the Frankfurter API (feature
//!   `network`), cache files and a bundled offline snapshot.
//! * [`view_model`]: [`UnitConverterViewModel`], the state machine a GUI drives.
//!
//! ## Driving the view model
//!
//! ```
//! use unitconv::data_loader::unit_ids;
//! use unitconv::{Command, ConverterMode, UnitConverterViewModel, ViewModelConfig};
//!
//! let mut vm = UnitConverterViewModel::new(ViewModelConfig::default());
//!
//! // Pick a category and units (ids from `vm.units()` / `unit_ids`).
//! vm.set_current_mode(ConverterMode::Length);
//! vm.set_unit1(unit_ids::LENGTH_MILE);
//! vm.set_unit2(unit_ids::LENGTH_KILOMETER);
//!
//! // Type into the active field.
//! vm.button_pressed(Command::One);
//! vm.button_pressed(Command::Zero);
//! assert_eq!(vm.value1(), "10");
//! assert_eq!(vm.value2(), "16.09344");
//!
//! // "About equal to": regular units, then one whimsical unit.
//! let about: Vec<String> = vm
//!     .supplementary_results()
//!     .iter()
//!     .map(|r| format!("{} {}", r.value, r.unit.abbreviation))
//!     .collect();
//! assert_eq!(about.first().unwrap(), "8.69 nmi");
//! assert_eq!(about.last().unwrap(), "211.8 jumbo jets");
//!
//! // Edit the other field instead (the bottom value becomes the source).
//! vm.switch_active();
//! vm.button_pressed(Command::Five);
//! assert_eq!((vm.value1(), vm.value2()), ("3.106856", "5"));
//! ```
//!
//! ## Currency rates
//!
//! The view model loads cached rates (or the bundled snapshot) on creation.
//! Network I/O is the GUI's job, on a background thread:
//!
//! ```no_run
//! # #[cfg(feature = "network")] {
//! use std::sync::{Arc, Mutex};
//! use unitconv::{ConverterMode, UnitConverterViewModel, ViewModelConfig};
//!
//! let vm = Arc::new(Mutex::new(UnitConverterViewModel::new(ViewModelConfig {
//!     currency_cache_path: Some("/home/me/.cache/gmnb/currency.json".into()),
//!     ..Default::default()
//! })));
//!
//! // On startup (and whenever connectivity returns): refresh stale rates.
//! // On "Update rates" use `start_currency_refresh()` instead.
//! if vm.lock().unwrap().start_automatic_currency_fetch() {
//!     let vm = vm.clone();
//!     std::thread::spawn(move || {
//!         let result = unitconv::currency::fetch_latest(); // blocking HTTP
//!         // (A GTK app would send `result` to the main loop instead.)
//!         vm.lock().unwrap().finish_currency_fetch(result);
//!     });
//! }
//!
//! let mut vm = vm.lock().unwrap();
//! vm.set_current_mode(ConverterMode::Currency);
//! println!("{}", vm.currency_ratio_equality()); // "1 USD = 0.8836 EUR"
//! println!("{}", vm.currency_timestamp()); // "Updated 10/1/2026 9:02 PM"
//! println!("{}", vm.currency_status().text()); // "" / "Couldn’t get new rates. ..."
//! # }
//! ```

pub mod converter;
pub mod currency;
pub mod data_loader;
pub mod number_formatting;
pub mod resources;
pub mod view_model;

pub use converter::{Category, Command, ConversionData, Unit, UnitConverter};
pub use currency::{CurrencyError, CurrencySnapshot, NetworkAccessBehavior};
pub use data_loader::{ConverterMode, UnitConverterDataLoader};
pub use view_model::{
    CategoryInfo, ConverterPreferences, CurrencyStatus, NumberFormat, SupplementaryResult,
    UnitConverterViewModel, UnitInfo, ViewModelConfig,
};
