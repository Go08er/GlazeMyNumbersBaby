// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `Calculator.Tests/DateCalculatorTests.cs`.
//!
//! Not ported, because they exercise non-Gregorian calendar systems this port does not
//! implement (see the crate docs):
//! * `JapaneseCalendarSubtractionBelowSupportedRangeReturnsNullAndRecovers` (relies on the
//!   Japanese calendar's lower bound, Meiji era, which Gregorian does not have);
//! * `UmAlQuraRangeAtCalendarLimitReturnsYearMonthDayDifference` (lunar months);
//! * the second half of `JapaneseDifferenceRecoveryPreservesTheCalendarSystem` (same lower
//!   bound); its first half is kept below.
//!
//! The Japanese era-transition tests only check that arithmetic ignores era boundaries, which
//! is pure Gregorian arithmetic, so they are kept (run on the Gregorian engine).

use chrono::{DateTime, FixedOffset, TimeZone};
use datecalc::{
    DateCalculationEngine, DateCalculatorState, DateDifference, DateTimeOffset, DateUnit,
    format_long_date,
};

#[derive(Clone, Copy)]
struct DateTimeTestCase {
    start_date: DateTimeOffset,
    end_date: DateTimeOffset,
    date_diff: DateDifference,
}

fn make_date(year: i32, month: u32, day: u32) -> DateTimeOffset {
    FixedOffset::east_opt(0)
        .unwrap()
        .with_ymd_and_hms(year, month, day, 0, 0, 0)
        .unwrap()
}

fn diff(year: i32, month: i32, week: i32, day: i32) -> DateDifference {
    DateDifference {
        year,
        month,
        week,
        day,
    }
}

struct Fixture {
    date: [DateTimeOffset; 15],
    datetime_difftest: [DateTimeTestCase; 9],
    datetime_bound_add: [DateTimeTestCase; 2],
    datetime_bound_subtract: [DateTimeTestCase; 2],
    datetime_add_case: [DateTimeTestCase; 3],
    datetime_subtract_case: [DateTimeTestCase; 3],
}

/// `TestClassSetup` (identical in both test classes).
fn fixture() -> Fixture {
    // Dates - DD.MM.YYYY
    let d = [
        make_date(9999, 12, 31),
        make_date(9999, 12, 30),
        make_date(9998, 12, 31),
        make_date(1601, 1, 1),
        make_date(1601, 1, 2),
        make_date(2008, 5, 10),
        make_date(2008, 3, 10),
        make_date(2008, 2, 29),
        make_date(2007, 2, 28),
        make_date(2007, 3, 10),
        make_date(2007, 5, 10),
        make_date(2008, 1, 29),
        make_date(2007, 1, 28),
        make_date(2008, 1, 31),
        make_date(2008, 3, 31),
    ];

    // Date Differences
    let dd = [
        diff(1, 1, 0, 0),
        diff(0, 1, 0, 10),
        diff(0, 0, 0, 2),
        diff(0, 0, 52, 1),
        diff(1, 0, 0, 0),
        diff(0, 0, 0, 365),
        diff(0, 1, 0, 0),
        diff(0, 1, 0, 2),
        diff(0, 0, 0, 31),
        diff(0, 11, 0, 1),
        diff(8398, 11, 0, 30),
        diff(2008, 0, 0, 0),
        diff(7991, 11, 0, 0),
        diff(0, 0, 416998, 1),
    ];

    let case = |s: usize, e: usize, x: usize| DateTimeTestCase {
        start_date: d[s],
        end_date: d[e],
        date_diff: dd[x],
    };

    Fixture {
        date: d,
        // Date Difference test cases
        datetime_difftest: [
            case(0, 3, 10),
            case(0, 2, 5),
            case(0, 2, 4),
            case(0, 2, 3),
            case(14, 7, 7),
            case(14, 7, 8),
            case(11, 8, 9),
            case(13, 0, 12),
            case(13, 0, 13),
        ],
        // Date Add Out of Bound test cases
        datetime_bound_add: [case(1, 0, 2), case(2, 0, 11)],
        // Date Subtract Out of Bound test cases
        datetime_bound_subtract: [case(3, 0, 2), case(14, 0, 11)],
        // Date Add test cases
        datetime_add_case: [case(13, 7, 6), case(14, 5, 1), case(13, 6, 1)],
        // Date Subtract test cases
        datetime_subtract_case: [case(14, 7, 6), case(6, 11, 1), case(9, 12, 1)],
    }
}

/// The request has to name the same units the expectation uses, or the engine reduces the
/// answer into ones the case never set.
fn units_present_in(difference: DateDifference) -> DateUnit {
    let mut units = DateUnit::empty();
    if difference.year != 0 {
        units |= DateUnit::YEAR;
    }
    if difference.month != 0 {
        units |= DateUnit::MONTH;
    }
    if difference.week != 0 {
        units |= DateUnit::WEEK;
    }
    if difference.day != 0 {
        units |= DateUnit::DAY;
    }
    units
}

/// A view model whose "today" is fixed, so the tests do not depend on the clock.
fn view_model() -> DateCalculatorState {
    DateCalculatorState::with_today(make_date(2024, 6, 15))
}

/// Built through the same long-date formatter the view model uses.
fn expected_date_string(date: DateTimeOffset) -> String {
    format_long_date(&date)
}

// ===================================================================== DateCalculatorUnitTests

#[test]
fn test_date_diff() {
    let f = fixture();
    let engine = DateCalculationEngine::new();
    for (test_index, test_case) in f.datetime_difftest.iter().enumerate() {
        let requested = units_present_in(test_case.date_diff);

        // The engine takes the dates in order; putting them the other way round is the view
        // model's job to normalize, which date_calc_view_model_date_diff_ignore_sign_test covers.
        let (earlier, later) = if test_case.start_date <= test_case.end_date {
            (test_case.start_date, test_case.end_date)
        } else {
            (test_case.end_date, test_case.start_date)
        };

        let difference = engine
            .try_get_date_difference(earlier, later, requested)
            .unwrap_or_else(|| panic!("TryGetDateDifference returned null for case {test_index}"));
        assert_eq!(
            test_case.date_diff.year, difference.year,
            "year, case {test_index}"
        );
        assert_eq!(
            test_case.date_diff.month, difference.month,
            "month, case {test_index}"
        );
        assert_eq!(
            test_case.date_diff.week, difference.week,
            "week, case {test_index}"
        );
        assert_eq!(
            test_case.date_diff.day, difference.day,
            "day, case {test_index}"
        );
    }
}

#[test]
fn one_year_difference_ending_at_the_top_of_range_is_calculated() {
    let engine = DateCalculationEngine::new();
    let difference = engine
        .try_get_date_difference(
            make_date(9998, 12, 31),
            make_date(9999, 12, 31),
            DateUnit::YEAR,
        )
        .expect("difference");
    assert_eq!(1, difference.year);
    assert_eq!(0, difference.day);
}

#[test]
fn full_supported_range_returns_year_month_day_difference() {
    let engine = DateCalculationEngine::new();
    let difference = engine
        .try_get_date_difference(
            make_date(1601, 1, 1),
            make_date(9999, 12, 31),
            DateUnit::YEAR | DateUnit::MONTH | DateUnit::DAY,
        )
        .expect("difference");
    assert_eq!(8398, difference.year);
    assert_eq!(11, difference.month);
    assert_eq!(30, difference.day);
}

#[test]
fn range_ending_in_year_9998_returns_year_month_day_difference() {
    let engine = DateCalculationEngine::new();
    let difference = engine
        .try_get_date_difference(
            make_date(1601, 1, 1),
            make_date(9998, 12, 31),
            DateUnit::YEAR | DateUnit::MONTH | DateUnit::DAY,
        )
        .expect("difference");
    assert_eq!(8397, difference.year);
    assert_eq!(11, difference.month);
    assert_eq!(30, difference.day);
}

#[test]
fn test_add_oob() {
    let f = fixture();
    let engine = DateCalculationEngine::new();
    for (test_index, test_case) in f.datetime_bound_add.iter().enumerate() {
        let end_date = engine.add_duration(test_case.start_date, test_case.date_diff);
        assert!(
            end_date.is_none(),
            "AddDuration should return null for out-of-bound case {test_index}"
        );
    }
}

#[test]
fn test_subtract_oob() {
    let f = fixture();
    let engine = DateCalculationEngine::new();
    for (test_index, test_case) in f.datetime_bound_subtract.iter().enumerate() {
        // Subtract Duration
        let end_date = engine.subtract_duration(test_case.start_date, test_case.date_diff);
        // Assert for the result
        assert!(
            end_date.is_none(),
            "SubtractDuration should return null for out-of-bound case {test_index}"
        );
    }
}

#[test]
fn test_addition() {
    let f = fixture();
    let engine = DateCalculationEngine::new();
    for (test_index, test_case) in f.datetime_add_case.iter().enumerate() {
        let end_date = engine
            .add_duration(test_case.start_date, test_case.date_diff)
            .unwrap_or_else(|| panic!("AddDuration returned null for case {test_index}"));
        assert_eq!(
            test_case.end_date, end_date,
            "AddDuration produced the wrong date for case {test_index}"
        );
    }
}

#[test]
fn test_subtraction() {
    let f = fixture();
    let engine = DateCalculationEngine::new();
    for (test_index, test_case) in f.datetime_subtract_case.iter().enumerate() {
        let end_date = engine
            .subtract_duration(test_case.start_date, test_case.date_diff)
            .unwrap_or_else(|| panic!("SubtractDuration returned null for case {test_index}"));
        assert_eq!(
            test_case.end_date, end_date,
            "SubtractDuration produced the wrong date for case {test_index}"
        );
    }
}

// ================================================================ DateCalculatorViewModelTests

#[test]
fn date_calc_view_model_initialization_test() {
    let view_model = view_model();

    assert!(view_model.is_date_diff_mode());
    assert!(view_model.is_add_mode());

    assert_ne!(DateTime::<FixedOffset>::default(), view_model.from_date());
    assert_ne!(DateTime::<FixedOffset>::default(), view_model.to_date());
    assert_ne!(DateTime::<FixedOffset>::default(), view_model.start_date());

    assert_eq!(0, view_model.days_offset());
    assert_eq!(0, view_model.months_offset());
    assert_eq!(0, view_model.years_offset());

    assert!(view_model.is_diff_in_days());
    assert_eq!("Same dates", view_model.str_date_diff_result());
    assert_eq!("", view_model.str_date_diff_result_in_days());
    assert_eq!("", view_model.str_date_result());
}

#[test]
fn date_calc_view_model_initialization_with_clock_test() {
    // Same as above, but through the clock-based constructor the GUI uses.
    let view_model = DateCalculatorState::new();
    assert!(view_model.is_date_diff_mode());
    assert!(view_model.is_diff_in_days());
    assert_eq!("Same dates", view_model.str_date_diff_result());
    assert_eq!("", view_model.str_date_result());
}

#[test]
fn date_calc_view_model_add_subtract_init_test() {
    let mut view_model = view_model();
    view_model.set_is_date_diff_mode(false);

    assert!(!view_model.is_date_diff_mode());
    assert!(view_model.is_add_mode());

    assert_ne!(DateTime::<FixedOffset>::default(), view_model.from_date());
    assert_ne!(DateTime::<FixedOffset>::default(), view_model.to_date());
    assert_ne!(DateTime::<FixedOffset>::default(), view_model.start_date());

    assert_eq!(0, view_model.days_offset());
    assert_eq!(0, view_model.months_offset());
    assert_eq!(0, view_model.years_offset());

    assert!(view_model.is_diff_in_days());
    assert_eq!("Same dates", view_model.str_date_diff_result());
    assert_eq!("", view_model.str_date_diff_result_in_days());

    assert_ne!("", view_model.str_date_result());
    // Anchor: "today" is 2024-06-15 for these tests.
    assert_eq!("Saturday, June 15, 2024", view_model.str_date_result());
}

#[test]
fn date_calc_view_model_date_diff_daylight_saving_time_test() {
    let f = fixture();
    let mut view_model = view_model();
    view_model.set_is_date_diff_mode(true);
    assert!(view_model.is_date_diff_mode());

    // 31.03.2008 -> 29.02.2008
    view_model.set_from_date(f.datetime_difftest[5].start_date);
    view_model.set_to_date(f.datetime_difftest[5].end_date);

    // Assert for the result
    assert!(!view_model.is_diff_in_days());
    assert_eq!("31 days", view_model.str_date_diff_result_in_days());
    assert_eq!("1 month, 2 days", view_model.str_date_diff_result());

    // Daylight Saving Time - Clock Forward
    // 10.03.2019 -> 11.03.2019
    view_model.set_from_date(make_date(2019, 3, 10));
    view_model.set_to_date(make_date(2019, 3, 11));
    assert!(view_model.is_diff_in_days());
    assert_eq!("1 day", view_model.str_date_diff_result());

    // 10.03.2019 -> 17.03.2019
    view_model.set_to_date(make_date(2019, 3, 17));
    assert!(!view_model.is_diff_in_days());
    assert_eq!("1 week", view_model.str_date_diff_result());

    // Daylight Saving Time - Clock Backward
    // 03.11.2019 -> 04.11.2019
    view_model.set_from_date(make_date(2019, 11, 3));
    view_model.set_to_date(make_date(2019, 11, 4));
    assert!(view_model.is_diff_in_days());
    assert_eq!("1 day", view_model.str_date_diff_result());
}

#[test]
fn date_calc_view_model_add_test() {
    let f = fixture();
    let mut view_model = view_model();

    view_model.set_is_date_diff_mode(false);
    view_model.set_is_add_mode(true);

    for (test_index, test_case) in f.datetime_add_case.iter().enumerate() {
        view_model.set_start_date(test_case.start_date);
        view_model.set_days_offset(test_case.date_diff.day);
        view_model.set_months_offset(test_case.date_diff.month);
        view_model.set_years_offset(test_case.date_diff.year);

        assert_eq!(
            expected_date_string(test_case.end_date),
            view_model.str_date_result(),
            "Add mode produced the wrong date for case {test_index}"
        );
    }
}

#[test]
fn date_calc_view_model_add_test_literal_strings() {
    // The C# test derives its expectations from the same formatter; pin the en-US format.
    let f = fixture();
    let mut view_model = view_model();
    view_model.set_is_date_diff_mode(false);
    let expected = [
        "Friday, February 29, 2008",
        "Saturday, May 10, 2008",
        "Monday, March 10, 2008",
    ];
    for (test_case, expected) in f.datetime_add_case.iter().zip(expected) {
        view_model.set_start_date(test_case.start_date);
        view_model.set_days_offset(test_case.date_diff.day);
        view_model.set_months_offset(test_case.date_diff.month);
        view_model.set_years_offset(test_case.date_diff.year);
        assert_eq!(expected, view_model.str_date_result());
    }
}

#[test]
fn date_calc_view_model_subtract_test() {
    let f = fixture();
    let mut view_model = view_model();

    view_model.set_is_date_diff_mode(false);
    view_model.set_is_add_mode(false);

    for (test_index, test_case) in f.datetime_subtract_case.iter().enumerate() {
        view_model.set_start_date(test_case.start_date);
        view_model.set_days_offset(test_case.date_diff.day);
        view_model.set_months_offset(test_case.date_diff.month);
        view_model.set_years_offset(test_case.date_diff.year);

        assert_eq!(
            expected_date_string(test_case.end_date),
            view_model.str_date_result(),
            "Subtract mode produced the wrong date for case {test_index}"
        );
    }
}

#[test]
fn date_calc_view_model_add_oob_test() {
    let f = fixture();
    let mut view_model = view_model();

    view_model.set_is_date_diff_mode(false);
    view_model.set_is_add_mode(true);
    assert!(!view_model.is_date_diff_mode());
    assert!(view_model.is_add_mode());

    for test_case in &f.datetime_bound_add {
        view_model.set_start_date(test_case.start_date);
        view_model.set_days_offset(test_case.date_diff.day);
        view_model.set_months_offset(test_case.date_diff.month);
        view_model.set_years_offset(test_case.date_diff.year);

        // Assert for the result
        assert_eq!("Date out of Bound", view_model.str_date_result());
        assert!(view_model.is_out_of_bound());
        assert!(view_model.date_result().is_none());
    }
}

#[test]
fn date_calc_view_model_subtract_oob_test() {
    let f = fixture();
    let mut view_model = view_model();

    view_model.set_is_date_diff_mode(false);
    view_model.set_is_add_mode(false);
    assert!(!view_model.is_date_diff_mode());
    assert!(!view_model.is_add_mode());

    for test_case in &f.datetime_bound_subtract {
        view_model.set_start_date(test_case.start_date);
        view_model.set_days_offset(test_case.date_diff.day);
        view_model.set_months_offset(test_case.date_diff.month);
        view_model.set_years_offset(test_case.date_diff.year);

        // Assert for the result
        assert_eq!("Date out of Bound", view_model.str_date_result());
    }
}

#[test]
fn date_calc_view_model_date_diff_ignore_sign_test() {
    let f = fixture();
    let mut view_model = view_model();

    view_model.set_is_date_diff_mode(true);
    assert!(view_model.is_date_diff_mode());

    view_model.set_from_date(f.date[10]); // 10.05.2007
    view_model.set_to_date(f.date[6]); // 10.03.2008

    assert!(!view_model.is_diff_in_days());
    assert_eq!("305 days", view_model.str_date_diff_result_in_days());
    assert_eq!("10 months", view_model.str_date_diff_result());

    view_model.set_from_date(f.date[6]); // 10.03.2008
    view_model.set_to_date(f.date[10]); // 10.05.2007

    assert!(!view_model.is_diff_in_days());
    assert_eq!("305 days", view_model.str_date_diff_result_in_days());
    assert_eq!("10 months", view_model.str_date_diff_result());
}

#[test]
fn date_calc_view_model_range_ending_at_maximum_shows_years_months_and_total_days() {
    let f = fixture();
    let mut view_model = view_model();
    view_model.set_is_date_diff_mode(true);

    view_model.set_from_date(f.date[13]); // 31.01.2008
    view_model.set_to_date(f.date[0]); // 31.12.9999

    let expected_breakdown = "7991 years, 11 months";
    let expected_days = "2918987 days";

    assert!(!view_model.is_diff_in_days());
    assert_eq!(expected_breakdown, view_model.str_date_diff_result());
    assert_eq!(expected_days, view_model.str_date_diff_result_in_days());
}

#[test]
fn date_calc_view_model_date_diff_result_in_positive_days_test() {
    let f = fixture();
    let mut view_model = view_model();

    view_model.set_is_date_diff_mode(true);
    assert!(view_model.is_date_diff_mode());

    view_model.set_from_date(f.date[1]); // 30.12.9999
    view_model.set_to_date(f.date[0]); // 31.12.9999

    assert!(view_model.is_diff_in_days());
    assert_eq!("1 day", view_model.str_date_diff_result());
    assert_eq!("", view_model.str_date_diff_result_in_days());
}

#[test]
fn date_calc_view_model_date_diff_automation_name_uses_localized_format() {
    let f = fixture();
    let mut view_model = view_model();
    view_model.set_is_date_diff_mode(true);
    view_model.set_from_date(f.date[1]);
    view_model.set_to_date(f.date[0]);

    assert_eq!(
        format!("Difference {}", view_model.str_date_diff_result()),
        view_model.str_date_diff_result_automation_name()
    );
}

#[test]
fn date_calc_view_model_date_diff_from_date_higher_than_to_date() {
    let f = fixture();
    let mut view_model = view_model();

    view_model.set_is_date_diff_mode(true);
    assert!(view_model.is_date_diff_mode());

    view_model.set_from_date(f.date[0]); // 31.12.9999
    view_model.set_to_date(f.date[1]); // 30.12.9999

    assert!(view_model.is_diff_in_days());
    assert_eq!("1 day", view_model.str_date_diff_result());
    assert_eq!("", view_model.str_date_diff_result_in_days());
}

#[test]
fn date_calc_view_model_preserves_calendar_date_for_offset_picker_values() {
    let plus_two = FixedOffset::east_opt(2 * 3600).unwrap();
    let mut view_model = view_model();
    view_model.set_is_date_diff_mode(true);
    view_model.set_from_date(plus_two.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap());
    view_model.set_to_date(plus_two.with_ymd_and_hms(2024, 4, 1, 0, 0, 0).unwrap());

    assert!(!view_model.is_diff_in_days());
    assert_eq!("1 month", view_model.str_date_diff_result());
}

#[test]
fn date_calc_view_model_add_subtract_result_automation_name_test() {
    let f = fixture();
    let mut view_model = view_model();
    view_model.set_is_date_diff_mode(false);
    view_model.set_is_add_mode(true);
    view_model.set_start_date(f.date[13]);
    view_model.set_days_offset(1);
    view_model.set_months_offset(0);
    view_model.set_years_offset(0);

    let automation_name = view_model.str_date_result_automation_name();

    assert_eq!(
        format!("Resulting date {}", view_model.str_date_result()),
        automation_name
    );
    assert_eq!("Resulting date Friday, February 1, 2008", automation_name);
}

/// Originally run on a `"JapaneseCalendar"` engine: Showa ends 1989-01-07 and Heisei begins on
/// the 8th. Date arithmetic must not notice.
#[test]
fn ja_era_transition_addition() {
    let engine = DateCalculationEngine::new();

    let result = engine.add_duration(make_date(1989, 1, 7), diff(0, 0, 0, 1));

    assert_eq!(
        Some(make_date(1989, 1, 8)),
        result,
        "AddDuration returned null across the Showa/Heisei boundary"
    );
}

/// Originally run on a `"JapaneseCalendar"` engine.
#[test]
fn ja_era_transition_subtraction() {
    let engine = DateCalculationEngine::new();

    let result = engine.subtract_duration(make_date(1989, 1, 8), diff(0, 0, 0, 1));

    assert_eq!(
        Some(make_date(1989, 1, 7)),
        result,
        "SubtractDuration returned null across the Showa/Heisei boundary"
    );
}

/// Originally run on a `"JapaneseCalendar"` engine.
#[test]
fn ja_era_transition_difference() {
    let engine = DateCalculationEngine::new();

    let showa_to_heisei = engine
        .try_get_date_difference(make_date(1989, 1, 7), make_date(1989, 1, 8), DateUnit::DAY)
        .expect("TryGetDateDifference returned null across Showa/Heisei");
    assert_eq!(1, showa_to_heisei.day);

    // Heisei ends 2019-04-30 and Reiwa begins on the 1st; the same must hold there.
    let heisei_to_reiwa = engine
        .try_get_date_difference(make_date(2019, 4, 30), make_date(2019, 5, 1), DateUnit::DAY)
        .expect("TryGetDateDifference returned null across Heisei/Reiwa");
    assert_eq!(1, heisei_to_reiwa.day);
}

/// First half of `JapaneseDifferenceRecoveryPreservesTheCalendarSystem` (the second half
/// depends on the Japanese calendar's lower bound).
#[test]
fn long_range_difference_from_1900_succeeds() {
    let engine = DateCalculationEngine::new();
    assert!(
        engine
            .try_get_date_difference(
                make_date(1900, 1, 1),
                make_date(9998, 12, 31),
                DateUnit::YEAR | DateUnit::MONTH | DateUnit::DAY,
            )
            .is_some()
    );
}

// ======================================================================= additional coverage

#[test]
fn full_breakdown_uses_every_unit_and_pluralises() {
    let mut view_model = view_model();
    view_model.set_from_date(make_date(2020, 1, 1));
    view_model.set_to_date(make_date(2021, 3, 11));
    // 1 year -> 2021-01-01, 2 months -> 2021-03-01, 1 week -> 03-08, 3 days.
    assert_eq!(
        "1 year, 2 months, 1 week, 3 days",
        view_model.str_date_diff_result()
    );
    assert_eq!("435 days", view_model.str_date_diff_result_in_days());
    assert!(!view_model.is_diff_in_days());

    view_model.set_to_date(make_date(2022, 2, 16));
    assert_eq!(
        "2 years, 1 month, 2 weeks, 1 day",
        view_model.str_date_diff_result()
    );
}

#[test]
fn same_dates_after_change_and_copy_text() {
    let mut view_model = view_model();
    view_model.set_from_date(make_date(2020, 1, 1));
    view_model.set_to_date(make_date(2020, 1, 13));
    assert_eq!("1 week, 5 days", view_model.str_date_diff_result());
    assert_eq!("12 days", view_model.str_date_diff_result_in_days());
    assert_eq!("1 week, 5 days", view_model.copy_text());

    view_model.set_to_date(make_date(2020, 1, 1));
    assert_eq!("Same dates", view_model.str_date_diff_result());
    assert!(view_model.is_diff_in_days());

    view_model.set_is_date_diff_mode(false);
    assert_eq!(view_model.str_date_result(), view_model.copy_text());
}

#[test]
fn time_of_day_is_ignored_for_differences() {
    let offset = FixedOffset::west_opt(5 * 3600).unwrap();
    let mut view_model = view_model();
    view_model.set_from_date(offset.with_ymd_and_hms(2020, 1, 1, 23, 30, 0).unwrap());
    view_model.set_to_date(offset.with_ymd_and_hms(2020, 1, 2, 0, 15, 0).unwrap());
    assert_eq!("1 day", view_model.str_date_diff_result());
}

#[test]
fn result_is_shown_in_the_start_date_offset() {
    // Local midnight of 2024-03-01 in UTC+2 is 2024-02-29T22:00Z; one day later is still
    // 2024-03-02 locally.
    let plus_two = FixedOffset::east_opt(2 * 3600).unwrap();
    let mut view_model = view_model();
    view_model.set_is_date_diff_mode(false);
    view_model.set_start_date(plus_two.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap());
    view_model.set_days_offset(1);
    assert_eq!("Saturday, March 2, 2024", view_model.str_date_result());
    assert_eq!(
        Some(plus_two.with_ymd_and_hms(2024, 3, 2, 0, 0, 0).unwrap()),
        view_model.date_result()
    );
}

#[test]
fn offset_values_cover_zero_to_999() {
    let view_model = view_model();
    let values = view_model.offset_values();
    assert_eq!(1000, values.len());
    assert_eq!("0", values[0]);
    assert_eq!("999", values[999]);
}

#[test]
fn naive_dates_are_accepted() {
    use chrono::NaiveDate;
    let mut view_model =
        DateCalculatorState::with_today(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
    view_model.set_to_date(NaiveDate::from_ymd_opt(2024, 1, 2).unwrap());
    assert_eq!("1 day", view_model.str_date_diff_result());
}

#[test]
fn unknown_difference_shows_calculation_failed() {
    // Only reachable with dates outside the calendar (which the C# type cannot even hold).
    let mut view_model = view_model();
    view_model.set_to_date(
        FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(10000, 1, 1, 0, 0, 0)
            .unwrap(),
    );
    assert_eq!("Calculation failed", view_model.str_date_diff_result());
    assert_eq!("", view_model.str_date_diff_result_in_days());
    assert!(!view_model.is_diff_in_days());
    assert_eq!(DateDifference::UNKNOWN, view_model.date_diff_result());
}
