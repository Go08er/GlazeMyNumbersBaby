// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Date Calculation mode of Windows Calculator, ported to Rust.
//!
//! * [`engine`] — `DateCalculator.cs`: [`DateCalculationEngine`] adds/subtracts calendar
//!   durations and breaks the distance between two dates into years, months, weeks and days
//!   with the original's estimate-and-correct algorithm.
//! * [`state`] — the non-UI logic of `DateCalculatorViewModel.cs`: [`DateCalculatorState`]
//!   holds the inputs the GUI sets and produces every display string ("1 year, 2 months,
//!   1 week, 3 days", "Same dates", "305 days", "Friday, February 29, 2008",
//!   "Date out of Bound", ...).
//! * [`strings`] — the en-US resource strings involved.
//!
//! ```
//! use chrono::NaiveDate;
//! use datecalc::DateCalculatorState;
//!
//! let mut calc = DateCalculatorState::with_today(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
//! calc.set_from_date(NaiveDate::from_ymd_opt(2007, 5, 10).unwrap());
//! calc.set_to_date(NaiveDate::from_ymd_opt(2008, 3, 10).unwrap());
//! assert_eq!(calc.str_date_diff_result(), "10 months");
//! assert_eq!(calc.str_date_diff_result_in_days(), "305 days");
//!
//! calc.set_is_date_diff_mode(false);
//! calc.set_start_date(NaiveDate::from_ymd_opt(2008, 1, 31).unwrap());
//! calc.set_months_offset(1);
//! assert_eq!(calc.str_date_result(), "Friday, February 29, 2008");
//! ```
//!
//! # Deviations from the original
//!
//! * **Gregorian calendar only.** The original honours the user's calendar system through
//!   `Windows.Globalization.Calendar` (Japanese, Hijri, Um Al-Qura, Hebrew, ...); this port
//!   always uses the proleptic Gregorian calendar (via `chrono`), which is what the original
//!   does for `"GregorianCalendar"`. Japanese-era handling in the original already converts to
//!   Gregorian for arithmetic, so era boundaries make no difference here.
//! * **en-US only.** Resource strings, the list separator (", "), number rendering (ASCII
//!   digits, no grouping) and the `"longdate"` format (`dddd, MMMM d, yyyy`) are those of
//!   en-US.
//! * **Time zone of the resulting date.** The original's formatter renders the result in the
//!   user's time zone; this port renders it in the offset of the start date the GUI supplied
//!   (pass local-midnight values, or plain [`chrono::NaiveDate`]s, which are taken as UTC
//!   midnight).

pub mod engine;
pub mod state;
pub mod strings;

pub use engine::{DateCalculationEngine, DateDifference, DateTimeOffset, DateUnit};
pub use state::{
    DateCalculatorState, IntoDateTimeOffset, LONG_DATE_FORMAT, MAX_OFFSET_VALUE, PICKER_MAX_YEAR,
    PICKER_MIN_YEAR, format_long_date, picker_max_date, picker_min_date, utc_midnight,
};
