// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `Calculator.ViewModels/Common/DateCalculator.cs`
//! (`CalculatorApp.ViewModel.Common.DateCalculation`).
//!
//! The original drives a `Windows.Globalization.Calendar` that has been switched to the UTC
//! time zone and to the user's calendar system. This port implements the Gregorian calendar
//! only (see the crate documentation), still in UTC, with the same range limits as the
//! original: the calendar cannot go past `9999-12-31T23:59:59.9999999Z` (the
//! `DateTimeOffset` maximum) or before `0001-01-01T00:00:00Z`, and
//! [`DateCalculationEngine::subtract_duration`] additionally refuses results before
//! `1601-01-01` (the `Windows.Foundation.DateTime` / FILETIME epoch).

use std::ops::{BitOr, BitOrAssign};

use chrono::{DateTime, Datelike, FixedOffset, Months, NaiveDate, TimeDelta, TimeZone, Utc};

/// A point in time together with the UTC offset it was expressed in; the Rust counterpart of
/// .NET's `System.DateTimeOffset`.
///
/// Like `DateTimeOffset`, two values compare equal when they denote the same instant, whatever
/// their offsets.
pub type DateTimeOffset = DateTime<FixedOffset>;

/// Set of date units a difference may be expressed in (`[Flags] enum DateUnit`).
///
/// The bit values are those of the original and are significant: the engine walks the units
/// as `1 << index` and tests `bits & 7` for "anything bigger than days".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DateUnit(u8);

impl DateUnit {
    /// `DateUnit.Year = 0x01`
    pub const YEAR: DateUnit = DateUnit(0x01);
    /// `DateUnit.Month = 0x02`
    pub const MONTH: DateUnit = DateUnit(0x02);
    /// `DateUnit.Week = 0x04`
    pub const WEEK: DateUnit = DateUnit(0x04);
    /// `DateUnit.Day = 0x08`
    pub const DAY: DateUnit = DateUnit(0x08);
    /// All four units (`Year | Month | Week | Day`), the view model's "all date units" format.
    pub const ALL: DateUnit = DateUnit(0x0F);

    /// The empty set (`(DateUnit)0`).
    pub const fn empty() -> DateUnit {
        DateUnit(0)
    }

    /// Builds a set from raw bits; bits above `0x0F` are kept but have no meaning.
    pub const fn from_bits(bits: u8) -> DateUnit {
        DateUnit(bits)
    }

    /// The raw bits of the set.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// `true` when every unit in `other` is also in `self`.
    pub const fn contains(self, other: DateUnit) -> bool {
        self.0 & other.0 == other.0
    }

    /// `true` when `self` and `other` share at least one unit.
    pub const fn intersects(self, other: DateUnit) -> bool {
        self.0 & other.0 != 0
    }

    /// `true` when no unit is set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for DateUnit {
    type Output = DateUnit;
    fn bitor(self, rhs: DateUnit) -> DateUnit {
        DateUnit(self.0 | rhs.0)
    }
}

impl BitOrAssign for DateUnit {
    fn bitor_assign(&mut self, rhs: DateUnit) {
        self.0 |= rhs.0;
    }
}

/// A duration expressed in calendar units (`struct DateDifference`).
///
/// Used both as the result of [`DateCalculationEngine::try_get_date_difference`] and as the
/// input of [`DateCalculationEngine::add_duration`] /
/// [`DateCalculationEngine::subtract_duration`] (where `week` is ignored, as in the original).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DateDifference {
    pub year: i32,
    pub month: i32,
    pub week: i32,
    pub day: i32,
}

impl DateDifference {
    /// `DateCalculationEngine.DateDifferenceUnknown`: every field set to `int.MinValue`.
    pub const UNKNOWN: DateDifference = DateDifference {
        year: i32::MIN,
        month: i32::MIN,
        week: i32::MIN,
        day: i32::MIN,
    };

    /// Convenience constructor.
    pub const fn new(year: i32, month: i32, week: i32, day: i32) -> DateDifference {
        DateDifference {
            year,
            month,
            week,
            day,
        }
    }
}

/// Raised where the WinRT calendar would throw `ArgumentException` (result out of range).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OutOfRange;

const C_MILLISECOND: i64 = 10_000;
const C_SECOND: i64 = 1000 * C_MILLISECOND;
const C_MINUTE: i64 = 60 * C_SECOND;
const C_HOUR: i64 = 60 * C_MINUTE;
const C_DAY: i64 = 24 * C_HOUR;

const C_UNITS_OF_DATE: usize = 4;
const C_UNITS_GREATER_THAN_DAYS: usize = 3;
const C_DAYS_IN_WEEK: u32 = 7;

/// Earliest instant the calendar can hold (`DateTimeOffset.MinValue`).
fn calendar_min() -> DateTime<Utc> {
    Utc.from_utc_datetime(
        &NaiveDate::from_ymd_opt(1, 1, 1)
            .expect("valid date")
            .and_hms_opt(0, 0, 0)
            .expect("valid time"),
    )
}

/// Latest instant the calendar can hold (`DateTimeOffset.MaxValue`, 100 ns resolution).
fn calendar_max() -> DateTime<Utc> {
    Utc.from_utc_datetime(
        &NaiveDate::from_ymd_opt(9999, 12, 31)
            .expect("valid date")
            .and_hms_nano_opt(23, 59, 59, 999_999_900)
            .expect("valid time"),
    )
}

/// `s_minSupportedDate`: dates are round-tripped through `Windows.Foundation.DateTime`, whose
/// epoch is 1601-01-01, so nothing earlier can be represented.
fn min_supported_date() -> DateTime<Utc> {
    Utc.from_utc_datetime(
        &NaiveDate::from_ymd_opt(1601, 1, 1)
            .expect("valid date")
            .and_hms_opt(0, 0, 0)
            .expect("valid time"),
    )
}

fn check_range(date: DateTime<Utc>) -> Result<DateTime<Utc>, OutOfRange> {
    if date < calendar_min() || date > calendar_max() {
        Err(OutOfRange)
    } else {
        Ok(date)
    }
}

/// `Calendar.AddMonths`: the day of month is clamped to the length of the target month.
fn add_months(date: DateTime<Utc>, months: i64) -> Result<DateTime<Utc>, OutOfRange> {
    let magnitude = u32::try_from(months.unsigned_abs()).map_err(|_| OutOfRange)?;
    let result = if months >= 0 {
        date.checked_add_months(Months::new(magnitude))
    } else {
        date.checked_sub_months(Months::new(magnitude))
    };
    check_range(result.ok_or(OutOfRange)?)
}

/// `Calendar.AddYears`: Feb 29 maps to Feb 28 in a common year.
fn add_years(date: DateTime<Utc>, years: i64) -> Result<DateTime<Utc>, OutOfRange> {
    add_months(date, years.checked_mul(12).ok_or(OutOfRange)?)
}

/// `Calendar.AddDays`.
fn add_days(date: DateTime<Utc>, days: i64) -> Result<DateTime<Utc>, OutOfRange> {
    let delta = TimeDelta::try_days(days).ok_or(OutOfRange)?;
    check_range(date.checked_add_signed(delta).ok_or(OutOfRange)?)
}

/// `Calendar.AddWeeks`.
fn add_weeks(date: DateTime<Utc>, weeks: i64) -> Result<DateTime<Utc>, OutOfRange> {
    add_days(date, weeks.checked_mul(7).ok_or(OutOfRange)?)
}

/// `DateTimeOffset.ToUniversalTime().Ticks`: 100 ns ticks since 0001-01-01 (any fixed origin
/// works, only differences are used).
fn utc_ticks(date: DateTime<Utc>) -> i64 {
    date.timestamp() * C_SECOND + i64::from(date.timestamp_subsec_nanos() / 100)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month");
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .expect("valid month");
    u32::try_from((next - first).num_days()).expect("month length is positive")
}

/// Date arithmetic over the Gregorian calendar (`sealed class DateCalculationEngine`).
///
/// The original takes a WinRT calendar identifier and honours the user's calendar system
/// (Japanese, Hijri, Um Al-Qura, ...). This port always uses the proleptic Gregorian
/// calendar in UTC, which is what the original does for `"GregorianCalendar"`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DateCalculationEngine {
    _private: (),
}

impl DateCalculationEngine {
    /// `DateDifferenceUnknown`, re-exported here for parity with the original's static field.
    pub const DATE_DIFFERENCE_UNKNOWN: DateDifference = DateDifference::UNKNOWN;

    /// Creates a Gregorian engine (`new DateCalculationEngine("GregorianCalendar")`).
    pub fn new() -> Self {
        DateCalculationEngine { _private: () }
    }

    /// Adds `duration` to `start_date`: years first, then months, then days (weeks are
    /// ignored). Returns `None` when the result falls outside the calendar's range.
    ///
    /// The result is expressed in UTC (offset `+00:00`), like the UTC calendar of the
    /// original; compare it as an instant.
    pub fn add_duration(
        &self,
        start_date: DateTimeOffset,
        duration: DateDifference,
    ) -> Option<DateTimeOffset> {
        let attempt = || -> Result<DateTime<Utc>, OutOfRange> {
            let mut date = check_range(start_date.with_timezone(&Utc))?;
            if duration.year != 0 {
                date = add_years(date, i64::from(duration.year))?;
            }
            if duration.month != 0 {
                date = add_months(date, i64::from(duration.month))?;
            }
            if duration.day != 0 {
                date = add_days(date, i64::from(duration.day))?;
            }
            Ok(date)
        };
        attempt().ok().map(|d| d.fixed_offset())
    }

    /// Subtracts `duration` from `start_date`. Unlike addition the smaller units go first:
    /// days, then months, then years (weeks are ignored). Returns `None` when the result
    /// falls outside the calendar's range or before 1601-01-01.
    pub fn subtract_duration(
        &self,
        start_date: DateTimeOffset,
        duration: DateDifference,
    ) -> Option<DateTimeOffset> {
        let attempt = || -> Result<DateTime<Utc>, OutOfRange> {
            let mut date = check_range(start_date.with_timezone(&Utc))?;
            // For Subtract the algorithm is different than Add: smaller units first, then larger.
            if duration.day != 0 {
                date = add_days(date, -i64::from(duration.day))?;
            }
            if duration.month != 0 {
                date = add_months(date, -i64::from(duration.month))?;
            }
            if duration.year != 0 {
                date = add_years(date, -i64::from(duration.year))?;
            }
            Ok(date)
        };
        match attempt() {
            Ok(date) if date >= min_supported_date() => Some(date.fixed_offset()),
            _ => None,
        }
    }

    /// Computes the difference between two dates in the requested units
    /// (`TryGetDateDifference`). The dates may be given in either order.
    ///
    /// Units are filled greedily from the largest requested one down, each estimated from
    /// the day count (days-in-year of the *later* date, days-in-month of the *earlier* date,
    /// 7 days per week) and then corrected by walking the calendar. Whatever remains is always
    /// returned in `day`, even if [`DateUnit::DAY`] was not requested. Returns `None` when the
    /// calculation fails (or when either date is outside the calendar's range).
    pub fn try_get_date_difference(
        &self,
        date1: DateTimeOffset,
        date2: DateTimeOffset,
        output_format: DateUnit,
    ) -> Option<DateDifference> {
        let date1 = check_range(date1.with_timezone(&Utc)).ok()?;
        let date2 = check_range(date2.with_timezone(&Utc)).ok()?;

        let (start_date, end_date) = if date1 < date2 {
            (date1, date2)
        } else {
            (date2, date1)
        };
        let mut pivot_date = start_date;
        let mut days_diff = Self::get_difference_in_days(start_date, end_date) as u32;
        let mut difference_in_dates = [0u32; C_UNITS_OF_DATE];

        // If output has units other than days (bits 0-2: Year, Month, Week)
        if output_format.bits() & 7 != 0 {
            let days_in_month = Self::calendar_days_in_month(start_date);
            let approximate_days_in_year = Self::calendar_days_in_year(end_date);
            let days_in = [approximate_days_in_year, days_in_month, C_DAYS_IN_WEEK, 1];

            for unit_index in 0..C_UNITS_GREATER_THAN_DAYS {
                let temp_pivot_date = pivot_date;
                let date_unit = DateUnit::from_bits(1 << unit_index);

                if !output_format.intersects(date_unit) {
                    continue;
                }

                let mut is_end_date_hit = false;
                difference_in_dates[unit_index] = days_diff / days_in[unit_index];

                while difference_in_dates[unit_index] != 0 {
                    match Self::adjust_calendar_date(
                        temp_pivot_date,
                        date_unit,
                        i64::from(difference_in_dates[unit_index]),
                    ) {
                        Ok(date) => {
                            pivot_date = date;
                            break;
                        }
                        // The day-based estimate can overshoot the calendar's upper bound.
                        Err(OutOfRange) => difference_in_dates[unit_index] -= 1,
                    }
                }

                loop {
                    let temp_days_diff = Self::get_difference_in_days(pivot_date, end_date);

                    if temp_days_diff < 0 {
                        if difference_in_dates[unit_index] == 0 {
                            return None;
                        }
                        difference_in_dates[unit_index] -= 1;
                        // Stepping back towards the start date cannot leave the range; the
                        // original lets an exception escape here, this port reports failure.
                        pivot_date = Self::adjust_calendar_date(
                            temp_pivot_date,
                            date_unit,
                            i64::from(difference_in_dates[unit_index]),
                        )
                        .ok()?;
                        is_end_date_hit = true;
                    } else if temp_days_diff > 0 {
                        if is_end_date_hit {
                            break;
                        }
                        match Self::adjust_calendar_date(
                            temp_pivot_date,
                            date_unit,
                            i64::from(difference_in_dates[unit_index]) + 1,
                        ) {
                            Ok(date) => {
                                pivot_date = date;
                                difference_in_dates[unit_index] += 1;
                            }
                            // The current pivot is valid; finish with smaller units.
                            Err(OutOfRange) => break,
                        }
                    }

                    if temp_days_diff == 0 {
                        break;
                    }
                }

                let signed_days_diff = Self::get_difference_in_days(pivot_date, end_date);
                if signed_days_diff < 0 {
                    return None;
                }
                days_diff = signed_days_diff as u32;
            }
        }

        difference_in_dates[3] = days_diff;

        Some(DateDifference {
            year: difference_in_dates[0] as i32,
            month: difference_in_dates[1] as i32,
            week: difference_in_dates[2] as i32,
            day: difference_in_dates[3] as i32,
        })
    }

    /// Whole days from `date1` to `date2`, truncated towards zero (`GetDifferenceInDays`).
    fn get_difference_in_days(date1: DateTime<Utc>, date2: DateTime<Utc>) -> i32 {
        let ticks_difference = utc_ticks(date2) - utc_ticks(date1);
        (ticks_difference / C_DAY) as i32
    }

    /// `TryGetCalendarDaysInMonth` (never fails for the Gregorian calendar).
    fn calendar_days_in_month(date: DateTime<Utc>) -> u32 {
        days_in_month(date.year(), date.month())
    }

    /// `TryGetCalendarDaysInYear`: the sum of the lengths of the months of `date`'s year.
    fn calendar_days_in_year(date: DateTime<Utc>) -> u32 {
        (1..=12)
            .map(|month| days_in_month(date.year(), month))
            .sum()
    }

    /// `AdjustCalendarDate`: moves `date` by `difference` years, months or weeks.
    fn adjust_calendar_date(
        date: DateTime<Utc>,
        date_unit: DateUnit,
        difference: i64,
    ) -> Result<DateTime<Utc>, OutOfRange> {
        match date_unit {
            DateUnit::YEAR => add_years(date, difference),
            DateUnit::MONTH => add_months(date, difference),
            DateUnit::WEEK => add_weeks(date, difference),
            _ => Ok(date),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
    }

    #[test]
    fn day_difference_truncates_towards_zero() {
        let a = utc(2020, 1, 1);
        let b = a + TimeDelta::hours(47);
        assert_eq!(DateCalculationEngine::get_difference_in_days(a, b), 1);
        assert_eq!(DateCalculationEngine::get_difference_in_days(b, a), -1);
    }

    #[test]
    fn days_in_year_and_month() {
        assert_eq!(
            DateCalculationEngine::calendar_days_in_year(utc(2008, 6, 1)),
            366
        );
        assert_eq!(
            DateCalculationEngine::calendar_days_in_year(utc(9999, 6, 1)),
            365
        );
        assert_eq!(
            DateCalculationEngine::calendar_days_in_month(utc(2007, 2, 3)),
            28
        );
        assert_eq!(
            DateCalculationEngine::calendar_days_in_month(utc(2008, 2, 3)),
            29
        );
        assert_eq!(
            DateCalculationEngine::calendar_days_in_month(utc(9999, 12, 31)),
            31
        );
    }

    #[test]
    fn calendar_rejects_dates_past_year_9999() {
        assert!(add_days(utc(9999, 12, 31), 1).is_err());
        assert!(add_years(utc(9999, 1, 1), 1).is_err());
        assert!(add_days(utc(1, 1, 1), -1).is_err());
        assert_eq!(add_months(utc(2008, 1, 31), 1), Ok(utc(2008, 2, 29)));
        assert_eq!(add_years(utc(2008, 2, 29), 1), Ok(utc(2009, 2, 28)));
    }

    #[test]
    fn date_unit_flags() {
        let all = DateUnit::YEAR | DateUnit::MONTH | DateUnit::WEEK | DateUnit::DAY;
        assert_eq!(all, DateUnit::ALL);
        assert_eq!(all.bits() & 7, 7);
        assert!(all.contains(DateUnit::WEEK));
        assert!(!DateUnit::DAY.intersects(DateUnit::YEAR));
        assert!(DateUnit::empty().is_empty());
    }
}
