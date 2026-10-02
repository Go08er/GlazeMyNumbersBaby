// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! The small enums `CopyPasteManager` is parameterised by, mirrored from
//! `Calculator.ViewModels/Common/NavCategory.cs`, `NumberBase.cs` and `BitLength.cs`.
//! The GUI maps its own mode/radix/word-size state onto these.

/// Calculator or converter mode the text is pasted into (`enum ViewMode`).
///
/// Discriminants match the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ViewMode {
    /// No mode (`ViewMode.None`); nothing validates.
    None = -1,
    /// Standard calculator.
    Standard = 0,
    /// Scientific calculator.
    Scientific = 1,
    /// Programmer calculator; validation additionally depends on [`NumberBase`] and
    /// [`BitLength`].
    Programmer = 2,
    /// Date calculation (no paste support).
    Date = 3,
    /// Volume converter.
    Volume = 4,
    /// Length converter.
    Length = 5,
    /// Weight and mass converter.
    Weight = 6,
    /// Temperature converter.
    Temperature = 7,
    /// Energy converter.
    Energy = 8,
    /// Area converter.
    Area = 9,
    /// Speed converter.
    Speed = 10,
    /// Time converter.
    Time = 11,
    /// Power converter.
    Power = 12,
    /// Data converter.
    Data = 13,
    /// Pressure converter.
    Pressure = 14,
    /// Angle converter.
    Angle = 15,
    /// Currency converter.
    Currency = 16,
    /// Graphing calculator (no paste support through this path).
    Graphing = 17,
}

impl ViewMode {
    /// `NavCategoryStates.GetGroupType(mode)`: which navigation group the mode belongs to.
    ///
    /// Standard, Scientific, Graphing, Programmer and Date are calculators; every unit
    /// converter (including Currency) is a converter; [`ViewMode::None`] belongs to no group.
    pub fn group_type(self) -> CategoryGroupType {
        match self {
            ViewMode::None => CategoryGroupType::None,
            ViewMode::Standard
            | ViewMode::Scientific
            | ViewMode::Graphing
            | ViewMode::Programmer
            | ViewMode::Date => CategoryGroupType::Calculator,
            ViewMode::Currency
            | ViewMode::Volume
            | ViewMode::Length
            | ViewMode::Weight
            | ViewMode::Temperature
            | ViewMode::Energy
            | ViewMode::Area
            | ViewMode::Speed
            | ViewMode::Time
            | ViewMode::Power
            | ViewMode::Data
            | ViewMode::Pressure
            | ViewMode::Angle => CategoryGroupType::Converter,
        }
    }
}

/// Navigation group of a mode (`enum CategoryGroupType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum CategoryGroupType {
    /// Not a known mode.
    None = -1,
    /// Standard, Scientific, Programmer, Date, Graphing.
    Calculator = 0,
    /// Unit and currency converters; any converter mode validates the same way.
    Converter = 1,
}

/// Radix of programmer mode (`enum NumberBase`). Discriminants match the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum NumberBase {
    /// No radix (every mode other than programmer).
    Unknown = -1,
    /// Hexadecimal.
    HexBase = 5,
    /// Decimal.
    DecBase = 6,
    /// Octal.
    OctBase = 7,
    /// Binary.
    BinBase = 8,
}

/// Word size of programmer mode (`enum BitLength`). Discriminants are the bit counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum BitLength {
    /// No word size (every mode other than programmer).
    BitLengthUnknown = -1,
    /// 8 bits.
    BitLengthByte = 8,
    /// 16 bits.
    BitLengthWord = 16,
    /// 32 bits.
    BitLengthDWord = 32,
    /// 64 bits.
    BitLengthQWord = 64,
}

/// Number formatting conventions the paste logic depends on (the relevant slice of the
/// original's `LocalizationSettings`). [`PasteLocale::EN_US`] is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasteLocale {
    /// Decimal separator (`.` in en-US).
    pub decimal_separator: char,
    /// Digit-group (thousands) separator (`,` in en-US).
    pub group_separator: char,
    /// The locale's symbols for the digits 0-9 (ASCII in en-US).
    pub digit_symbols: [char; 10],
}

impl PasteLocale {
    /// en-US: `.` decimal separator, `,` group separator, ASCII digits.
    pub const EN_US: PasteLocale = PasteLocale {
        decimal_separator: '.',
        group_separator: ',',
        digit_symbols: ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'],
    };

    /// `LocalizationSettings.GetEnglishValueFromLocalizedDigits`: maps the locale's digit
    /// symbols to ASCII digits and its decimal separator to `.`.
    pub fn english_value_from_localized_digits(&self, localized: &str) -> String {
        if *self == PasteLocale::EN_US {
            return localized.to_string();
        }
        localized
            .chars()
            .map(|ch| {
                let mut result = ch;
                if !ch.is_ascii_digit()
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

    /// `LocalizationSettings.RemoveGroupSeparators`: drops spaces and the group separator.
    pub fn remove_group_separators(&self, source: &str) -> String {
        source
            .chars()
            .filter(|&c| c != ' ' && c != self.group_separator)
            .collect()
    }

    /// `LocalizationSettings.IsLocalizedDigit`.
    pub fn is_localized_digit(&self, ch: char) -> bool {
        self.digit_symbols.contains(&ch)
    }

    /// The value (0-9) of one of the locale's digit symbols.
    pub fn localized_digit_value(&self, ch: char) -> Option<u8> {
        self.digit_symbols
            .iter()
            .position(|&d| d == ch)
            .map(|i| i as u8)
    }
}

impl Default for PasteLocale {
    fn default() -> Self {
        PasteLocale::EN_US
    }
}
