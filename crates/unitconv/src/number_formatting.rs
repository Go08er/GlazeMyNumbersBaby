// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Rust port of CalcManager/NumberFormattingUtils.{h,cpp} (namespace
// UnitConversionManager::NumberFormattingUtils).

//! Number formatting helpers used by the unit converter engine.
//!
//! These mirror the C++ helpers exactly, including their quirks: e.g.
//! [`round_significant_digits`] is really "round to N *decimal places*"
//! (`std::fixed` + `precision(N)`), and [`to_scientific_number`] is
//! `std::scientific` with the default precision of 6 (printf `%e`).

/// Trims out any trailing zeros or decimals in the given input string.
///
/// Strings without a `.` are left untouched (so `"100"` stays `"100"`).
pub fn trim_trailing_zeros(number: &mut String) {
    if !number.contains('.') {
        return;
    }

    if let Some(i) = number.rfind(|c| c != '0') {
        number.truncate(i + 1);
    }

    if number.ends_with('.') {
        number.pop();
    }
}

/// Returns a trimmed copy of `number` (see [`trim_trailing_zeros`]).
pub fn trimmed(number: &str) -> String {
    let mut s = number.to_owned();
    trim_trailing_zeros(&mut s);
    s
}

/// Get number of digits (whole number part + decimal part) of a display string.
///
/// Like the original this simply counts characters after trimming trailing
/// zeros, minus one for a decimal point and one for a minus sign (so a leading
/// `"0"` in `"0.5"` counts as a digit).
pub fn get_number_digits(value: &str) -> u32 {
    let value = trimmed(value);
    let mut number_significant_digits = value.chars().count() as u32;
    if value.contains('.') {
        number_significant_digits = number_significant_digits.saturating_sub(1);
    }
    if value.contains('-') {
        number_significant_digits = number_significant_digits.saturating_sub(1);
    }
    number_significant_digits
}

/// Get number of digits (whole number part only).
pub fn get_number_digits_whole_number_part(value: f64) -> u32 {
    if value == 0.0 {
        1
    } else {
        // static_cast<unsigned int>(1 + max(0.0, log10(abs(value))))
        let digits = 1.0 + f64::max(0.0, value.abs().log10());
        if digits.is_nan() {
            1
        } else {
            // `as` saturates for out-of-range values (C++ would be UB).
            digits as u32
        }
    }
}

/// Rounds the given double to the given number of digits after the decimal
/// point, formatted in fixed notation (`std::fixed` with `precision(n)`).
///
/// Ties are resolved on the exact binary value (round-half-even), like the
/// C/C++ runtime.
pub fn round_significant_digits(num: f64, num_significant: u32) -> String {
    if !num.is_finite() {
        return non_finite_to_string(num);
    }
    format!("{:.*}", num_significant as usize, num)
}

/// Convert a number to scientific notation, like `std::scientific` with the
/// default precision: six fractional digits and an exponent with a sign and
/// at least two digits (e.g. `4.535920e-15`, `2.204620e+15`).
pub fn to_scientific_number(number: f64) -> String {
    if !number.is_finite() {
        return non_finite_to_string(number);
    }
    let formatted = format!("{number:.6e}");
    let (mantissa, exponent) = formatted
        .split_once('e')
        .expect("Rust's {:e} formatting always contains an exponent");
    let exponent: i32 = exponent.parse().expect("exponent is an integer");
    let sign = if exponent < 0 { '-' } else { '+' };
    format!("{mantissa}e{sign}{:02}", exponent.unsigned_abs())
}

fn non_finite_to_string(num: f64) -> String {
    // What the MSVC runtime prints for these values.
    if num.is_nan() {
        if num.is_sign_negative() {
            "-nan(ind)".into()
        } else {
            "nan".into()
        }
    } else if num < 0.0 {
        "-inf".into()
    } else {
        "inf".into()
    }
}

/// Inserts `separator` between groups of three digits of an integer digit string.
pub(crate) fn group_digits(int_part: &str, separator: &str) -> String {
    let digits: Vec<char> = int_part.chars().collect();
    let mut out = String::with_capacity(int_part.len() + int_part.len() / 3 * separator.len());
    for (i, c) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push_str(separator);
        }
        out.push(*c);
    }
    out
}

/// `std::stod` equivalent used by the engine on its own display strings.
///
/// Returns 0 for strings that do not start with a number (the C++ code would
/// throw; the engine only ever feeds it well-formed strings).
pub(crate) fn stod(s: &str) -> f64 {
    let s = s.trim();
    if let Ok(v) = s.parse::<f64>() {
        return v;
    }
    // stod parses the longest valid prefix ("12." -> 12, "1e" -> 1).
    let mut end = s.len();
    while end > 0 {
        if let Some(prefix) = s.get(..end)
            && let Ok(v) = prefix.parse::<f64>()
        {
            return v;
        }
        end -= 1;
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim() {
        let cases = [
            ("100", "100"),
            ("100.", "100"),
            ("100.000", "100"),
            ("1.2300", "1.23"),
            ("0.0", "0"),
            ("-0.50", "-0.5"),
            ("30.", "30"),
        ];
        for (input, expected) in cases {
            assert_eq!(trimmed(input), expected, "{input}");
        }
    }

    #[test]
    fn number_digits() {
        assert_eq!(get_number_digits("123"), 3);
        assert_eq!(get_number_digits("-1.50"), 2);
        assert_eq!(get_number_digits("0.5"), 2);
        assert_eq!(get_number_digits("12345678"), 8);
        assert_eq!(get_number_digits_whole_number_part(0.0), 1);
        assert_eq!(get_number_digits_whole_number_part(0.5), 1);
        assert_eq!(get_number_digits_whole_number_part(9.99), 1);
        assert_eq!(get_number_digits_whole_number_part(10.0), 2);
        assert_eq!(get_number_digits_whole_number_part(-559989.7), 6);
        assert_eq!(get_number_digits_whole_number_part(2.2e15), 16);
    }

    #[test]
    fn fixed_and_scientific() {
        assert_eq!(round_significant_digits(277.75, 1), "277.8");
        assert_eq!(round_significant_digits(2777.75, 0), "2778");
        assert_eq!(round_significant_digits(1.360776, 2), "1.36");
        assert_eq!(to_scientific_number(4.53592e-15), "4.535920e-15");
        assert_eq!(to_scientific_number(2.20462e15), "2.204620e+15");
        assert_eq!(to_scientific_number(1.5e100), "1.500000e+100");
        assert_eq!(to_scientific_number(-3.0), "-3.000000e+00");
    }

    #[test]
    fn stod_prefix() {
        assert_eq!(stod("12."), 12.0);
        assert_eq!(stod("-0"), 0.0);
        assert_eq!(stod("4.535920e-15"), 4.53592e-15);
        assert_eq!(stod("1e"), 1.0);
        assert_eq!(stod(""), 0.0);
    }
}
