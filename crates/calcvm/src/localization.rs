// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
// Rust port: GMNB contributors.

//! Port of the number-formatting slice of
//! `Calculator.ViewModels/Common/LocalizationSettings.cs`.
//!
//! Upstream reads the user's region settings through WinRT
//! (`DecimalFormatter`, `GetLocaleInfoEx`). This port is fixed to the en-US
//! fallback the C# class uses when no formatter is available: ASCII digits,
//! `.` decimal separator, `,` group separator, grouping `3;0`. With those
//! settings `LocalizeDisplayValue` and `GetEnglishValueFromLocalizedDigits`
//! are the identity, exactly as upstream short-circuits them for en-US.

/// `LocalizationSettings` (en-US only, see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalizationSettings {
    digit_symbols: [char; 10],
    decimal_separator: char,
    number_group_separator: char,
    number_grouping: &'static str,
    resolved_name: &'static str,
}

#[cfg_attr(not(test), allow(dead_code))]
const HEX_SYMBOLS: [char; 6] = ['A', 'B', 'C', 'D', 'E', 'F'];

impl Default for LocalizationSettings {
    fn default() -> Self {
        Self::en_us()
    }
}

// The whole number-formatting surface is ported; the view model itself only
// needs part of it.
#[cfg_attr(not(test), allow(dead_code))]
impl LocalizationSettings {
    /// The en-US defaults of `LocalizationSettings(null)`.
    pub const fn en_us() -> Self {
        LocalizationSettings {
            digit_symbols: ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'],
            decimal_separator: '.',
            number_group_separator: ',',
            number_grouping: "3;0",
            resolved_name: "en-US",
        }
    }

    /// `LocalizationSettings.GetInstance()` — the process-wide (here:
    /// constant) settings.
    pub fn get_instance() -> &'static LocalizationSettings {
        static INSTANCE: LocalizationSettings = LocalizationSettings::en_us();
        &INSTANCE
    }

    pub fn get_locale_name(&self) -> &str {
        self.resolved_name
    }

    pub fn is_digit_en_us_setting(&self) -> bool {
        self.get_digit_symbol_from_en_us_digit('0') == '0'
    }

    pub fn get_english_value_from_localized_digits(&self, localized_string: &str) -> String {
        if self.resolved_name == "en-US" {
            return localized_string.to_string();
        }
        localized_string
            .chars()
            .map(|ch| {
                let mut result = ch;
                if !self.is_en_us_digit(ch)
                    && let Some(index) = self.digit_symbols.iter().position(|&d| d == ch)
                {
                    result = char::from(b'0' + index as u8);
                }
                if result == self.decimal_separator {
                    result = '.';
                }
                result
            })
            .collect()
    }

    pub fn remove_group_separators(&self, source: &str) -> String {
        source
            .chars()
            .filter(|&c| c != ' ' && c != self.number_group_separator)
            .collect()
    }

    pub fn get_decimal_separator(&self) -> char {
        self.decimal_separator
    }

    pub fn get_number_group_separator(&self) -> char {
        self.number_group_separator
    }

    pub fn get_number_grouping_str(&self) -> &str {
        self.number_grouping
    }

    pub fn get_digit_symbol_from_en_us_digit(&self, digit_symbol: char) -> char {
        let digit = (digit_symbol as u32).wrapping_sub('0' as u32);
        assert!(digit <= 9, "digit_symbol out of range");
        self.digit_symbols[digit as usize]
    }

    pub fn is_en_us_digit(&self, digit: char) -> bool {
        digit.is_ascii_digit()
    }

    pub fn is_localized_digit(&self, digit: char) -> bool {
        self.digit_symbols.contains(&digit)
    }

    pub fn is_localized_hex_digit(&self, digit: char) -> bool {
        self.is_localized_digit(digit) || HEX_SYMBOLS.contains(&digit)
    }

    /// `LocalizeDisplayValue(ref string)` — maps ASCII digits to the locale's
    /// digit symbols (a no-op for en-US).
    pub fn localize_display_value(&self, string_to_localize: &str) -> String {
        if self.is_digit_en_us_setting() {
            return string_to_localize.to_string();
        }
        string_to_localize
            .chars()
            .map(|ch| {
                if self.is_en_us_digit(ch) {
                    self.get_digit_symbol_from_en_us_digit(ch)
                } else {
                    ch
                }
            })
            .collect()
    }

    /// The settings in the form the `copypaste` crate takes them.
    pub fn paste_locale(&self) -> copypaste::PasteLocale {
        copypaste::PasteLocale {
            decimal_separator: self.decimal_separator,
            group_separator: self.number_group_separator,
            digit_symbols: self.digit_symbols,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port of the en-US relevant parts of Calculator.Tests/LocalizationSettingsTests.cs.
    #[test]
    fn en_us_defaults() {
        let s = LocalizationSettings::get_instance();
        assert_eq!(s.get_locale_name(), "en-US");
        assert_eq!(s.get_decimal_separator(), '.');
        assert_eq!(s.get_number_group_separator(), ',');
        assert_eq!(s.get_number_grouping_str(), "3;0");
        assert!(s.is_digit_en_us_setting());
        assert_eq!(s.paste_locale(), copypaste::PasteLocale::EN_US);
    }

    #[test]
    fn remove_group_separators() {
        let s = LocalizationSettings::get_instance();
        assert_eq!(s.remove_group_separators("1,001"), "1001");
        assert_eq!(s.remove_group_separators("999"), "999");
        assert_eq!(s.remove_group_separators("1,001,001"), "1001001");
        assert_eq!(s.remove_group_separators("1,001, 001"), "1001001");
    }

    #[test]
    fn localize_is_identity_for_en_us() {
        let s = LocalizationSettings::get_instance();
        assert_eq!(s.localize_display_value("1,234.5"), "1,234.5");
        assert_eq!(
            s.get_english_value_from_localized_digits("1,234.5"),
            "1,234.5"
        );
        assert!(s.is_localized_digit('7'));
        assert!(s.is_localized_hex_digit('F'));
        assert!(!s.is_localized_hex_digit('G'));
    }

    // LocalizationSettingsTests: TestIsEnUsDigit / TestIsLocalizedDigit /
    // TestIsLocalizedHexDigit / TestRemoveGroupSeparators (en-US cases).
    #[test]
    fn upstream_en_us_cases() {
        let s = LocalizationSettings::get_instance();
        assert!(!s.is_en_us_digit('/'));
        for c in ['0', '1', '8', '9'] {
            assert!(s.is_en_us_digit(c));
        }
        assert!(!s.is_en_us_digit(':'));
        assert!(s.is_localized_digit('0'));
        assert!(!s.is_localized_digit('A'));
        assert!(s.is_localized_hex_digit('0'));
        assert!(s.is_localized_hex_digit('A'));
        assert!(!s.is_localized_hex_digit('G'));
        assert_eq!(s.remove_group_separators("1,000 000"), "1000000");
    }
}
