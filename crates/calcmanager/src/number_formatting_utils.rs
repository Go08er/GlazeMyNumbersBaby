// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Port of `NumberFormattingUtils.h` / `NumberFormattingUtils.cpp`
//! (`UnitConversionManager::NumberFormattingUtils`). Not used by the engine
//! itself, but it lives in CalcManager and is covered by
//! `CalculatorManagerTest`.

/// Trims out any trailing zeros or decimals in the given input string
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

/// Get number of digits (whole number part + decimal part)
pub fn get_number_digits(value: &str) -> u32 {
    let mut value = value.to_string();
    trim_trailing_zeros(&mut value);
    let mut number_significant_digits = value.chars().count() as u32;
    if value.contains('.') {
        number_significant_digits -= 1;
    }
    if value.contains('-') {
        number_significant_digits -= 1;
    }
    number_significant_digits
}

/// Get number of digits (whole number part only)
pub fn get_number_digits_whole_number_part(value: f64) -> u32 {
    if value == 0.0 {
        1
    } else {
        (1.0 + f64::max(0.0, value.abs().log10())) as u32
    }
}

/// Rounds the given double to the given number of significant digits
/// (`std::fixed` with `precision(numSignificant)`).
pub fn round_significant_digits(num: f64, num_significant: u32) -> String {
    format!("{:.*}", num_significant as usize, num)
}

/// Convert a Number to Scientific Notation (`std::scientific`, default precision 6).
pub fn to_scientific_number(number: f64) -> String {
    let s = format!("{:.6e}", number);
    // Rust renders "3.423000e3"; iostreams render "3.423000e+03".
    match s.split_once('e') {
        Some((mantissa, exp)) => {
            let (sign, digits) = match exp.strip_prefix('-') {
                Some(d) => ('-', d),
                None => ('+', exp),
            };
            format!("{mantissa}e{sign}{digits:0>2}")
        }
        None => s,
    }
}
