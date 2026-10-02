// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `CalculatorUnitTests/CalcEngineTests.cpp`.

use std::cell::RefCell;
use std::rc::Rc;

use calcmanager::engine_strings::{IDS_ERR_INPUT_OVERFLOW, IDS_ERR_UNK_CH};
use calcmanager::{
    CalcEngine, CalculatorHistory, EngineResourceProvider, HistoryDisplayRef, ResourceProvider,
};

const MAX_HISTORY_SIZE: usize = 20;

struct Fixture {
    m_calc_engine: CalcEngine,
    _m_history: Rc<RefCell<CalculatorHistory>>,
}

fn common_setup() -> Fixture {
    let m_resource_provider: Rc<dyn ResourceProvider> = Rc::new(EngineResourceProvider::default());
    let m_history = Rc::new(RefCell::new(CalculatorHistory::new(MAX_HISTORY_SIZE)));
    CalcEngine::initial_one_time_only_setup(&*m_resource_provider);
    let history: HistoryDisplayRef = m_history.clone();
    let m_calc_engine = CalcEngine::new(
        false, /* Respect Order of Operations */
        false, /* Set to Integer Mode */
        m_resource_provider,
        None,
        Some(history),
    )
    .expect("CCalcEngine");
    Fixture {
        m_calc_engine,
        _m_history: m_history,
    }
}

#[test]
fn test_group_digits_per_radix() {
    let f = common_setup();
    let e = &f.m_calc_engine;
    // Empty/Error cases
    assert!(
        e.group_digits_per_radix("", 10).is_empty(),
        "Verify grouping empty string returns empty string."
    );
    assert_eq!(
        "12345678",
        e.group_digits_per_radix("12345678", 9),
        "Verify grouping on invalid base returns original string"
    );

    // Octal
    assert_eq!(
        "1 234 567",
        e.group_digits_per_radix("1234567", 8),
        "Verify grouping in octal."
    );
    assert_eq!(
        "123",
        e.group_digits_per_radix("123", 8),
        "Verify minimum grouping in octal."
    );

    // Binary/Hexadecimal
    assert_eq!(
        "12 3456 7890",
        e.group_digits_per_radix("1234567890", 2),
        "Verify grouping in binary."
    );
    assert_eq!(
        "1234",
        e.group_digits_per_radix("1234", 2),
        "Verify minimum grouping in binary."
    );
    assert_eq!(
        "12 3456 7890",
        e.group_digits_per_radix("1234567890", 16),
        "Verify grouping in hexadecimal."
    );
    assert_eq!(
        "1234",
        e.group_digits_per_radix("1234", 16),
        "Verify minimum grouping in hexadecimal."
    );

    // Decimal
    assert_eq!(
        "1,234,567,890",
        e.group_digits_per_radix("1234567890", 10),
        "Verify grouping in base10."
    );
    assert_eq!(
        "1,234,567.89",
        e.group_digits_per_radix("1234567.89", 10),
        "Verify grouping in base10 with decimal."
    );
    assert_eq!(
        "1,234,567e89",
        e.group_digits_per_radix("1234567e89", 10),
        "Verify grouping in base10 with exponent."
    );
    assert_eq!(
        "1,234,567.89e5",
        e.group_digits_per_radix("1234567.89e5", 10),
        "Verify grouping in base10 with decimal and exponent."
    );
    assert_eq!(
        "-123,456,789",
        e.group_digits_per_radix("-123456789", 10),
        "Verify grouping in base10 with negative."
    );
}

#[test]
fn test_is_number_invalid() {
    let f = common_setup();
    let e = &f.m_calc_engine;

    // Binary Number Checks
    let valid_bin_strs = ["0", "1", "0011", "1100"];
    let invalid_bin_strs = ["2", "A", "0.1"];
    for s in valid_bin_strs {
        assert_eq!(0, e.is_number_invalid(s, 0, 0, 2 /* Binary */));
    }
    for s in invalid_bin_strs {
        assert_eq!(IDS_ERR_UNK_CH, e.is_number_invalid(s, 0, 0, 2 /* Binary */));
    }

    // Octal Number Checks
    let valid_oct_strs = ["0", "7", "01234567", "76543210"];
    let invalid_oct_strs = ["8", "A", "0.7"];
    for s in valid_oct_strs {
        assert_eq!(0, e.is_number_invalid(s, 0, 0, 8 /* Octal */));
    }
    for s in invalid_oct_strs {
        assert_eq!(IDS_ERR_UNK_CH, e.is_number_invalid(s, 0, 0, 8 /* Octal */));
    }

    // Hexadecimal Number Checks
    let valid_hex_strs = ["0", "F", "0123456789ABCDEF", "FEDCBA9876543210"];
    let invalid_hex_strs = ["G", "abcdef", "x", "0.1"];
    for s in valid_hex_strs {
        assert_eq!(0, e.is_number_invalid(s, 0, 0, 16 /* HEx */));
    }
    for s in invalid_hex_strs {
        assert_eq!(IDS_ERR_UNK_CH, e.is_number_invalid(s, 0, 0, 16 /* Hex */));
    }

    // Decimal Number Checks

    // Special case errors: long exponent, long mantissa
    let long_exp = "1e12345";
    assert_eq!(
        0,
        e.is_number_invalid(
            long_exp, 5, /* Max exp length */
            100, 10 /* Decimal */
        )
    );
    assert_eq!(
        IDS_ERR_INPUT_OVERFLOW,
        e.is_number_invalid(
            long_exp, 4, /* Max exp length */
            100, 10 /* Decimal */
        )
    );
    // Mantissa length is sum of:
    //  - digits before decimal separator, minus leading zeroes
    //  - digits after decimal separator, including trailing zeroes
    // Each of these mantissa values should calculate as a length of 5
    let long_mant_strs = [
        "10000",
        "10.000",
        "0000012345",
        "123.45",
        "0.00123",
        "0.12345",
        "-123.45e678",
    ];
    for s in long_mant_strs {
        assert_eq!(
            0,
            e.is_number_invalid(
                s, 100, 5,  /* Max mantissa length */
                10  /* Decimal */
            )
        );
    }
    for s in long_mant_strs {
        assert_eq!(
            IDS_ERR_INPUT_OVERFLOW,
            e.is_number_invalid(
                s, 100, 4,  /* Max mantissa length */
                10  /* Decimal */
            )
        );
    }

    // Regex matching (descriptions taken from CalcUtils.cpp)
    // Use 100 for exp/mantissa length as they are tested above
    let valid_dec_strs = [
        // Start with an optional + or -
        "+1",
        "-1",
        "1",
        // Followed by zero or more digits
        "-",
        "",
        "1234567890",
        // Followed by an optional decimal point
        "1.0",
        "-.",
        "1.",
        // Followed by zero or more digits
        "0.0",
        "0.123456",
        // Followed by an optional exponent ('e')
        "1e",
        "1.e",
        "-e",
        // If there's an exponent, its optionally followed by + or -
        // and followed by zero or more digits
        "1e+12345",
        "1e-12345",
        "1e123",
        // All together
        "-123.456e+789",
    ];
    let invalid_dec_strs = ["x123", "123-", "1e1.2", "1-e2"];
    for s in valid_dec_strs {
        assert_eq!(0, e.is_number_invalid(s, 100, 100, 10 /* Dec */), "{s}");
    }
    for s in invalid_dec_strs {
        assert_eq!(
            IDS_ERR_UNK_CH,
            e.is_number_invalid(s, 100, 100, 10 /* Dec */),
            "{s}"
        );
    }
}

#[test]
fn test_digit_grouping_string_to_grouping_vector() {
    let _f = common_setup();
    let mut grouping_vector: Vec<u32> = vec![];
    assert_eq!(
        grouping_vector,
        CalcEngine::digit_grouping_string_to_grouping_vector(""),
        "Verify empty grouping"
    );

    grouping_vector = vec![1];
    assert_eq!(
        grouping_vector,
        CalcEngine::digit_grouping_string_to_grouping_vector("1"),
        "Verify simple grouping"
    );

    grouping_vector = vec![3, 0];
    assert_eq!(
        grouping_vector,
        CalcEngine::digit_grouping_string_to_grouping_vector("3;0"),
        "Verify standard grouping"
    );

    grouping_vector = vec![3, 0, 0];
    assert_eq!(
        grouping_vector,
        CalcEngine::digit_grouping_string_to_grouping_vector("3;0;0"),
        "Verify expanded non-repeating grouping"
    );

    grouping_vector = vec![5, 3, 2, 4, 6];
    assert_eq!(
        grouping_vector,
        CalcEngine::digit_grouping_string_to_grouping_vector("5;3;2;4;6"),
        "Verify long grouping"
    );

    grouping_vector = vec![15, 15, 15, 0];
    assert_eq!(
        grouping_vector,
        CalcEngine::digit_grouping_string_to_grouping_vector("15;15;15;0"),
        "Verify large grouping"
    );

    grouping_vector = vec![4, 7, 0];
    assert_eq!(
        grouping_vector,
        CalcEngine::digit_grouping_string_to_grouping_vector("4;16;7;25;0"),
        "Verify we ignore oversize grouping"
    );

    grouping_vector = vec![3, 0];
    let non_repeating_grouping = "3;0;0";
    let repeating_grouping = &non_repeating_grouping[0..3];
    assert_eq!(
        grouping_vector,
        CalcEngine::digit_grouping_string_to_grouping_vector(repeating_grouping),
        "Verify we don't go past the end of wstring_view range"
    );
}

#[test]
fn test_group_digits() {
    let f = common_setup();
    let e = &f.m_calc_engine;
    let mut result = "1234567";
    assert_eq!(
        result,
        e.group_digits("", &[3, 0], "1234567", false),
        "Verify handling of empty delimiter."
    );
    assert_eq!(
        result,
        e.group_digits(",", &[], "1234567", false),
        "Verify handling of empty grouping."
    );

    result = "1,234,567";
    assert_eq!(
        result,
        e.group_digits(",", &[3, 0], "1234567", false),
        "Verify standard digit grouping."
    );

    result = "1 234 567";
    assert_eq!(
        result,
        e.group_digits(" ", &[3, 0], "1234567", false),
        "Verify delimiter change."
    );

    result = "1|||234|||567";
    assert_eq!(
        result,
        e.group_digits("|||", &[3, 0], "1234567", false),
        "Verify long delimiter."
    );

    result = "12,345e67";
    assert_eq!(
        result,
        e.group_digits(",", &[3, 0], "12345e67", false),
        "Verify respect of exponent."
    );

    result = "12,345.67";
    assert_eq!(
        result,
        e.group_digits(",", &[3, 0], "12345.67", false),
        "Verify respect of decimal."
    );

    result = "1,234.56e7";
    assert_eq!(
        result,
        e.group_digits(",", &[3, 0], "1234.56e7", false),
        "Verify respect of exponent and decimal."
    );

    result = "-1,234,567";
    assert_eq!(
        result,
        e.group_digits(",", &[3, 0], "-1234567", true),
        "Verify negative number grouping."
    );

    // Test various groupings
    result = "1234567890123456";
    assert_eq!(
        result,
        e.group_digits(",", &[0, 0], "1234567890123456", false),
        "Verify no grouping."
    );

    result = "1234567890123,456";
    assert_eq!(
        result,
        e.group_digits(",", &[3], "1234567890123456", false),
        "Verify non-repeating grouping."
    );
    assert_eq!(
        result,
        e.group_digits(",", &[3, 0, 0], "1234567890123456", false),
        "Verify expanded form non-repeating grouping."
    );

    result = "12,34,56,78,901,23456";
    assert_eq!(
        result,
        e.group_digits(",", &[5, 3, 2, 0], "1234567890123456", false),
        "Verify multigroup with repeating grouping."
    );

    result = "1234,5678,9012,3456";
    assert_eq!(
        result,
        e.group_digits(",", &[4, 0], "1234567890123456", false),
        "Verify repeating non-standard grouping."
    );

    result = "123456,78,901,23456";
    assert_eq!(
        result,
        e.group_digits(",", &[5, 3, 2], "1234567890123456", false),
        "Verify multigroup non-repeating grouping."
    );
    assert_eq!(
        result,
        e.group_digits(",", &[5, 3, 2, 0, 0], "1234567890123456", false),
        "Verify expanded form multigroup non-repeating grouping."
    );
}
