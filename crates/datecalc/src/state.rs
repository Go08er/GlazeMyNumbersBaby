// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of the non-UI logic of `Calculator.ViewModels/DateCalculatorViewModel.cs`.
//!
//! [`DateCalculatorState`] is the view model without the XAML plumbing: the GUI sets inputs
//! through the `set_*` methods (which, like the original's observable properties, only
//! recompute when the value actually changes) and reads the display strings back through the
//! getters.

use chrono::{DateTime, FixedOffset, Local, NaiveDate, TimeZone, Utc};

use crate::engine::{DateCalculationEngine, DateDifference, DateTimeOffset, DateUnit};
use crate::strings;

/// Largest value offered by the years/months/days offset pickers (`MaxOffsetValue`).
pub const MAX_OFFSET_VALUE: i32 = 999;

/// Earliest year the original's date pickers allow (`DateCalculator.xaml.cs`, `c_minYear`).
pub const PICKER_MIN_YEAR: i32 = 1601;
/// Latest year the original's date pickers allow (`DateCalculator.xaml.cs`, `c_maxYear`).
/// The engine itself works up to 9999-12-31.
pub const PICKER_MAX_YEAR: i32 = 2550;

/// `strftime` pattern equivalent to the en-US Windows `"longdate"` format
/// (`dddd, MMMM d, yyyy`, e.g. "Friday, February 29, 2008").
pub const LONG_DATE_FORMAT: &str = "%A, %B %-d, %Y";

/// Formats a date the way the original's `"longdate"` `DateTimeFormatter` does for en-US,
/// using the date's own offset for the calendar day.
pub fn format_long_date(date: &DateTimeOffset) -> String {
    date.format(LONG_DATE_FORMAT).to_string()
}

/// First day the original's date pickers allow (1601-01-01).
pub fn picker_min_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(PICKER_MIN_YEAR, 1, 1).expect("valid date")
}

/// Last day the original's date pickers allow (2550-12-31).
pub fn picker_max_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(PICKER_MAX_YEAR, 12, 31).expect("valid date")
}

/// Midnight UTC on `date`, the form the view model works in for date differences.
pub fn utc_midnight(date: NaiveDate) -> DateTimeOffset {
    Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("valid time"))
        .fixed_offset()
}

/// Anything the GUI may hand the view model as a date: a [`DateTimeOffset`], a UTC or local
/// [`DateTime`], or a plain [`NaiveDate`] (taken as midnight UTC).
pub trait IntoDateTimeOffset {
    /// Converts to a [`DateTimeOffset`].
    fn into_date_time_offset(self) -> DateTimeOffset;
}

impl IntoDateTimeOffset for DateTime<FixedOffset> {
    fn into_date_time_offset(self) -> DateTimeOffset {
        self
    }
}

impl IntoDateTimeOffset for DateTime<Utc> {
    fn into_date_time_offset(self) -> DateTimeOffset {
        self.fixed_offset()
    }
}

impl IntoDateTimeOffset for DateTime<Local> {
    fn into_date_time_offset(self) -> DateTimeOffset {
        self.fixed_offset()
    }
}

impl IntoDateTimeOffset for NaiveDate {
    fn into_date_time_offset(self) -> DateTimeOffset {
        utc_midnight(self)
    }
}

/// `ClipTime`: keeps the calendar date as seen in the value's own offset and moves it to
/// midnight UTC.
fn clip_time(date_time: DateTimeOffset) -> DateTimeOffset {
    utc_midnight(date_time.date_naive())
}

/// `LocalizationSettings.LocalizeDisplayValue(value.ToString())` for en-US (ASCII digits, no
/// grouping).
fn localized_number_string(value: i32) -> String {
    value.to_string()
}

/// State of the Date Calculation mode (`DateCalculatorViewModel`).
///
/// Two sub-modes share the struct:
/// * **Difference between dates** (`is_date_diff_mode() == true`): set
///   [`from_date`](Self::set_from_date) and [`to_date`](Self::set_to_date), read
///   [`str_date_diff_result`](Self::str_date_diff_result) (e.g. "1 year, 2 months, 1 week,
///   3 days", "Same dates", "12 days") and
///   [`str_date_diff_result_in_days`](Self::str_date_diff_result_in_days) (the total in days,
///   only shown when [`is_diff_in_days`](Self::is_diff_in_days) is `false`).
/// * **Add or subtract days**: set [`start_date`](Self::set_start_date),
///   [`is_add_mode`](Self::set_is_add_mode) and the year/month/day offsets, read
///   [`str_date_result`](Self::str_date_result) (a long date, or "Date out of Bound").
#[derive(Debug, Clone)]
pub struct DateCalculatorState {
    // Inputs
    is_date_diff_mode: bool,
    is_add_mode: bool,
    days_offset: i32,
    months_offset: i32,
    years_offset: i32,
    from_date: DateTimeOffset,
    to_date: DateTimeOffset,
    start_date: DateTimeOffset,

    // Outputs
    is_diff_in_days: bool,
    date_result: DateTimeOffset,
    str_date_diff_result: String,
    str_date_diff_result_automation_name: String,
    str_date_diff_result_in_days: String,
    str_date_result: String,
    str_date_result_automation_name: String,

    // Private members
    is_out_of_bound: bool,
    offset_values: Vec<String>,
    date_diff_result: DateDifference,
    date_diff_result_in_days: DateDifference,
    date_calc_engine: DateCalculationEngine,
    days_output_format: DateUnit,
    all_date_units_output_format: DateUnit,
    list_separator: String,
}

impl Default for DateCalculatorState {
    fn default() -> Self {
        Self::new()
    }
}

impl DateCalculatorState {
    /// Creates the state with every date set to now (in the local time zone), in
    /// "Difference between dates" mode, adding, with zero offsets.
    pub fn new() -> Self {
        Self::with_today(Local::now().fixed_offset())
    }

    /// Creates the state as if "now" were `today` (deterministic variant of [`new`](Self::new)).
    ///
    /// From/To dates become `today`'s calendar date at midnight UTC; the start date of
    /// add/subtract mode keeps `today` as is, as in the original.
    pub fn with_today(today: impl IntoDateTimeOffset) -> Self {
        let today = today.into_date_time_offset();
        let clipped = clip_time(today);

        let mut state = DateCalculatorState {
            is_date_diff_mode: true,
            is_add_mode: true,
            days_offset: 0,
            months_offset: 0,
            years_offset: 0,
            from_date: clipped,
            to_date: clipped,
            start_date: today,
            is_diff_in_days: false,
            date_result: today,
            str_date_diff_result: String::new(),
            str_date_diff_result_automation_name: String::new(),
            str_date_diff_result_in_days: String::new(),
            str_date_result: String::new(),
            str_date_result_automation_name: String::new(),
            is_out_of_bound: false,
            offset_values: (0..=MAX_OFFSET_VALUE)
                .map(localized_number_string)
                .collect(),
            date_diff_result: DateDifference::default(),
            date_diff_result_in_days: DateDifference::default(),
            date_calc_engine: DateCalculationEngine::new(),
            // InitializeDateOutputFormats
            days_output_format: DateUnit::DAY,
            all_date_units_output_format: DateUnit::YEAR
                | DateUnit::MONTH
                | DateUnit::WEEK
                | DateUnit::DAY,
            // GetListSeparator() + " "; the en-US list separator is ",".
            list_separator: ", ".to_string(),
        };

        state.update_display_result();
        state
    }

    // ---------------------------------------------------------------- input getters

    /// `true` for "Difference between dates", `false` for "Add or subtract days".
    pub fn is_date_diff_mode(&self) -> bool {
        self.is_date_diff_mode
    }

    /// `true` to add the offsets to the start date, `false` to subtract them.
    pub fn is_add_mode(&self) -> bool {
        self.is_add_mode
    }

    /// Days to add or subtract.
    pub fn days_offset(&self) -> i32 {
        self.days_offset
    }

    /// Months to add or subtract.
    pub fn months_offset(&self) -> i32 {
        self.months_offset
    }

    /// Years to add or subtract.
    pub fn years_offset(&self) -> i32 {
        self.years_offset
    }

    /// "From" date of the difference mode.
    pub fn from_date(&self) -> DateTimeOffset {
        self.from_date
    }

    /// "To" date of the difference mode.
    pub fn to_date(&self) -> DateTimeOffset {
        self.to_date
    }

    /// "From" date of the add/subtract mode.
    pub fn start_date(&self) -> DateTimeOffset {
        self.start_date
    }

    // ---------------------------------------------------------------- input setters

    /// Switches between "Difference between dates" (`true`) and "Add or subtract days".
    pub fn set_is_date_diff_mode(&mut self, value: bool) {
        if self.is_date_diff_mode != value {
            self.is_date_diff_mode = value;
            self.on_inputs_changed();
        }
    }

    /// Chooses between adding (`true`) and subtracting the offsets.
    pub fn set_is_add_mode(&mut self, value: bool) {
        if self.is_add_mode != value {
            self.is_add_mode = value;
            self.on_inputs_changed();
        }
    }

    /// Sets the number of days to add or subtract (the UI offers 0..=999).
    pub fn set_days_offset(&mut self, value: i32) {
        if self.days_offset != value {
            self.days_offset = value;
            self.on_inputs_changed();
        }
    }

    /// Sets the number of months to add or subtract (the UI offers 0..=999).
    pub fn set_months_offset(&mut self, value: i32) {
        if self.months_offset != value {
            self.months_offset = value;
            self.on_inputs_changed();
        }
    }

    /// Sets the number of years to add or subtract (the UI offers 0..=999).
    pub fn set_years_offset(&mut self, value: i32) {
        if self.years_offset != value {
            self.years_offset = value;
            self.on_inputs_changed();
        }
    }

    /// Sets the "From" date of the difference mode. Only its calendar date (in its own
    /// offset) is used.
    pub fn set_from_date(&mut self, value: impl IntoDateTimeOffset) {
        let value = value.into_date_time_offset();
        // DateTimeOffset equality: same instant, whatever the offset.
        if self.from_date != value {
            self.from_date = value;
            self.on_inputs_changed();
        }
    }

    /// Sets the "To" date of the difference mode. Only its calendar date (in its own offset)
    /// is used.
    pub fn set_to_date(&mut self, value: impl IntoDateTimeOffset) {
        let value = value.into_date_time_offset();
        if self.to_date != value {
            self.to_date = value;
            self.on_inputs_changed();
        }
    }

    /// Sets the date the add/subtract mode starts from. The result is displayed in this
    /// value's offset.
    pub fn set_start_date(&mut self, value: impl IntoDateTimeOffset) {
        let value = value.into_date_time_offset();
        if self.start_date != value {
            self.start_date = value;
            self.on_inputs_changed();
        }
    }

    // ---------------------------------------------------------------- outputs

    /// `true` when the difference is shown in days only (so the secondary "in days" line is
    /// hidden).
    pub fn is_diff_in_days(&self) -> bool {
        self.is_diff_in_days
    }

    /// The difference, e.g. "1 year, 2 months, 1 week, 3 days", "Same dates", "1 day" or
    /// "Calculation failed".
    pub fn str_date_diff_result(&self) -> &str {
        &self.str_date_diff_result
    }

    /// Accessible name of the difference ("Difference …").
    pub fn str_date_diff_result_automation_name(&self) -> &str {
        &self.str_date_diff_result_automation_name
    }

    /// The difference in days (e.g. "305 days"), or empty when
    /// [`is_diff_in_days`](Self::is_diff_in_days) is `true`.
    pub fn str_date_diff_result_in_days(&self) -> &str {
        &self.str_date_diff_result_in_days
    }

    /// The resulting date as a long date (e.g. "Friday, February 29, 2008"), or
    /// "Date out of Bound". Empty until add/subtract mode has been entered once.
    pub fn str_date_result(&self) -> &str {
        &self.str_date_result
    }

    /// Accessible name of the resulting date ("Resulting date …").
    pub fn str_date_result_automation_name(&self) -> &str {
        &self.str_date_result_automation_name
    }

    /// `true` when the last add/subtract left the supported range.
    pub fn is_out_of_bound(&self) -> bool {
        self.is_out_of_bound
    }

    /// The last successfully computed add/subtract result, in the start date's offset, or
    /// `None` when the last computation was out of bound.
    pub fn date_result(&self) -> Option<DateTimeOffset> {
        if self.is_out_of_bound {
            None
        } else {
            Some(self.date_result.with_timezone(self.start_date.offset()))
        }
    }

    /// The broken-down difference (years, months, weeks, days), or
    /// [`DateDifference::UNKNOWN`] when the calculation failed.
    pub fn date_diff_result(&self) -> DateDifference {
        self.date_diff_result
    }

    /// The difference in days only (in `day`), or [`DateDifference::UNKNOWN`].
    pub fn date_diff_result_in_days(&self) -> DateDifference {
        self.date_diff_result_in_days
    }

    /// The strings offered by the year/month/day offset pickers: "0" to "999".
    pub fn offset_values(&self) -> &[String] {
        &self.offset_values
    }

    /// The text the Copy command puts on the clipboard (`OnCopy`): the difference in
    /// difference mode, the resulting date otherwise.
    pub fn copy_text(&self) -> &str {
        if self.is_date_diff_mode {
            &self.str_date_diff_result
        } else {
            &self.str_date_result
        }
    }

    // ---------------------------------------------------------------- logic

    fn on_inputs_changed(&mut self) {
        if self.is_date_diff_mode {
            let clipped_from = clip_time(self.from_date);
            let clipped_to = clip_time(self.to_date);

            match self.date_calc_engine.try_get_date_difference(
                clipped_from,
                clipped_to,
                self.days_output_format,
            ) {
                Some(in_days) => {
                    self.date_diff_result_in_days = in_days;
                    self.date_diff_result = self
                        .date_calc_engine
                        .try_get_date_difference(
                            clipped_from,
                            clipped_to,
                            self.all_date_units_output_format,
                        )
                        .unwrap_or(in_days);
                }
                None => {
                    self.date_diff_result = DateDifference::UNKNOWN;
                    self.date_diff_result_in_days = DateDifference::UNKNOWN;
                }
            }
        } else {
            self.is_out_of_bound = false;
            let duration = DateDifference {
                year: self.years_offset,
                month: self.months_offset,
                week: 0,
                day: self.days_offset,
            };

            let result = if self.is_add_mode {
                self.date_calc_engine
                    .add_duration(self.start_date, duration)
            } else {
                self.date_calc_engine
                    .subtract_duration(self.start_date, duration)
            };

            match result {
                Some(date) => self.date_result = date,
                None => self.is_out_of_bound = true,
            }
        }

        self.update_display_result();
    }

    fn update_display_result(&mut self) {
        if self.is_date_diff_mode {
            if self.date_diff_result_in_days == DateDifference::UNKNOWN {
                self.is_diff_in_days = false;
                self.str_date_diff_result_in_days = String::new();
                self.str_date_diff_result = strings::CALCULATION_FAILED.to_string();
            } else if self.date_diff_result_in_days.day == 0 {
                self.is_diff_in_days = true;
                self.str_date_diff_result_in_days = String::new();
                self.str_date_diff_result = strings::DATE_SAME_DATES.to_string();
            } else if self.date_diff_result == DateDifference::UNKNOWN
                || (self.date_diff_result.year == 0
                    && self.date_diff_result.month == 0
                    && self.date_diff_result.week == 0)
            {
                self.is_diff_in_days = true;
                self.str_date_diff_result_in_days = String::new();
                self.str_date_diff_result = self.get_date_diff_string_in_days();
            } else {
                self.is_diff_in_days = false;
                self.str_date_diff_result = self.get_date_diff_string();
                self.str_date_diff_result_in_days = self.get_date_diff_string_in_days();
            }

            self.update_str_date_diff_result_automation_name();
        } else {
            if self.is_out_of_bound {
                self.str_date_result = strings::DATE_OUT_OF_BOUND_MESSAGE.to_string();
            } else {
                // The original's DateTimeFormatter renders in the user's time zone; the start
                // date's offset stands in for it.
                let local = self.date_result.with_timezone(self.start_date.offset());
                self.str_date_result = format_long_date(&local);
            }
            self.update_str_date_result_automation_name();
        }
    }

    fn update_str_date_diff_result_automation_name(&mut self) {
        self.str_date_diff_result_automation_name = strings::format_with_param(
            strings::DATE_DIFFERENCE_RESULT_AUTOMATION_NAME,
            &self.str_date_diff_result,
        );
    }

    fn update_str_date_result_automation_name(&mut self) {
        self.str_date_result_automation_name = strings::format_with_param(
            strings::DATE_RESULTING_DATE_AUTOMATION_NAME,
            &self.str_date_result,
        );
    }

    fn get_date_diff_string(&self) -> String {
        let diff = &self.date_diff_result;
        let mut parts: Vec<String> = Vec::new();
        let unit = |value: i32, singular: &str, plural: &str| {
            format!(
                "{} {}",
                localized_number_string(value),
                if value == 1 { singular } else { plural }
            )
        };

        if diff.year > 0 {
            parts.push(unit(diff.year, strings::DATE_YEAR, strings::DATE_YEARS));
        }
        if diff.month > 0 {
            parts.push(unit(diff.month, strings::DATE_MONTH, strings::DATE_MONTHS));
        }
        if diff.week > 0 {
            parts.push(unit(diff.week, strings::DATE_WEEK, strings::DATE_WEEKS));
        }
        if diff.day > 0 || parts.is_empty() {
            parts.push(unit(diff.day, strings::DATE_DAY, strings::DATE_DAYS));
        }

        parts.join(&self.list_separator)
    }

    fn get_date_diff_string_in_days(&self) -> String {
        let day = self.date_diff_result_in_days.day;
        format!(
            "{} {}",
            localized_number_string(day),
            if day == 1 {
                strings::DATE_DAY
            } else {
                strings::DATE_DAYS
            }
        )
    }
}
