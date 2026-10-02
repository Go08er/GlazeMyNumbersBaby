// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Rust port of Calculator.ViewModels/DataLoaders/UnitConverterDataLoader.cs
// (itself a port of the C++ UnitConverterDataLoader / UnitConverterDataConstants.h)
// and the converter part of Calculator.ViewModels/Common/NavCategory.cs.

//! Static (non-currency) unit data: every category, unit, conversion ratio,
//! default unit pair, ordering and whimsical unit of Windows Calculator.

use std::collections::HashMap;

use crate::converter::{Category, ConversionData, ConverterDataLoader, Unit, UnitRatios};
use crate::resources::resource_string;

/// Converter categories with their stable serialization ids
/// (`NavCategoryStates`; "these constants should never change").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConverterMode {
    Currency,
    Volume,
    Length,
    Weight,
    Temperature,
    Energy,
    Area,
    Speed,
    Time,
    Power,
    Data,
    Pressure,
    Angle,
}

impl ConverterMode {
    /// All converter modes in navigation order (Currency first).
    pub const ALL: [ConverterMode; 13] = [
        ConverterMode::Currency,
        ConverterMode::Volume,
        ConverterMode::Length,
        ConverterMode::Weight,
        ConverterMode::Temperature,
        ConverterMode::Energy,
        ConverterMode::Area,
        ConverterMode::Speed,
        ConverterMode::Time,
        ConverterMode::Power,
        ConverterMode::Data,
        ConverterMode::Pressure,
        ConverterMode::Angle,
    ];

    /// Serialization id, used as the [`Category::id`].
    pub const fn id(self) -> i32 {
        match self {
            ConverterMode::Volume => 4,
            ConverterMode::Length => 5,
            ConverterMode::Weight => 6,
            ConverterMode::Temperature => 7,
            ConverterMode::Energy => 8,
            ConverterMode::Area => 9,
            ConverterMode::Speed => 10,
            ConverterMode::Time => 11,
            ConverterMode::Power => 12,
            ConverterMode::Data => 13,
            ConverterMode::Pressure => 14,
            ConverterMode::Angle => 15,
            ConverterMode::Currency => 16,
        }
    }

    pub fn from_id(id: i32) -> Option<ConverterMode> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    /// Friendly (non-localized) name, e.g. `"Weight and Mass"`.
    pub const fn friendly_name(self) -> &'static str {
        match self {
            ConverterMode::Currency => "Currency",
            ConverterMode::Volume => "Volume",
            ConverterMode::Length => "Length",
            ConverterMode::Weight => "Weight and Mass",
            ConverterMode::Temperature => "Temperature",
            ConverterMode::Energy => "Energy",
            ConverterMode::Area => "Area",
            ConverterMode::Speed => "Speed",
            ConverterMode::Time => "Time",
            ConverterMode::Power => "Power",
            ConverterMode::Data => "Data",
            ConverterMode::Pressure => "Pressure",
            ConverterMode::Angle => "Angle",
        }
    }

    /// Resource key of the display name (`NavCategoryStates.GetNameResourceKey`).
    pub const fn name_resource_key(self) -> &'static str {
        match self {
            ConverterMode::Currency => "CategoryName_CurrencyText",
            ConverterMode::Volume => "CategoryName_VolumeText",
            ConverterMode::Length => "CategoryName_LengthText",
            ConverterMode::Weight => "CategoryName_WeightText",
            ConverterMode::Temperature => "CategoryName_TemperatureText",
            ConverterMode::Energy => "CategoryName_EnergyText",
            ConverterMode::Area => "CategoryName_AreaText",
            ConverterMode::Speed => "CategoryName_SpeedText",
            ConverterMode::Time => "CategoryName_TimeText",
            ConverterMode::Power => "CategoryName_PowerText",
            ConverterMode::Data => "CategoryName_DataText",
            ConverterMode::Pressure => "CategoryName_PressureText",
            ConverterMode::Angle => "CategoryName_AngleText",
        }
    }

    /// Localized (en-US) display name, e.g. `"Weight and mass"`.
    pub fn display_name(self) -> &'static str {
        resource_string(self.name_resource_key())
    }

    /// Whether values in this category may be negative.
    pub const fn supports_negative(self) -> bool {
        matches!(
            self,
            ConverterMode::Temperature | ConverterMode::Power | ConverterMode::Angle
        )
    }

    /// The engine category for this mode.
    pub fn category(self) -> Category {
        Category::new(self.id(), self.display_name(), self.supports_negative())
    }
}

/// Unit ids (`UnitConverterUnits` in UnitConverterDataConstants.h).
pub mod unit_ids {
    #![allow(missing_docs)]
    pub const AREA_ACRE: i32 = 1;
    pub const AREA_HECTARE: i32 = 2;
    pub const AREA_SQUARE_CENTIMETER: i32 = 3;
    pub const AREA_SQUARE_FOOT: i32 = 4;
    pub const AREA_SQUARE_INCH: i32 = 5;
    pub const AREA_SQUARE_KILOMETER: i32 = 6;
    pub const AREA_SQUARE_METER: i32 = 7;
    pub const AREA_SQUARE_MILE: i32 = 8;
    pub const AREA_SQUARE_MILLIMETER: i32 = 9;
    pub const AREA_SQUARE_YARD: i32 = 10;
    pub const DATA_BIT: i32 = 11;
    pub const DATA_BYTE: i32 = 12;
    pub const DATA_GIGABIT: i32 = 13;
    pub const DATA_GIGABYTE: i32 = 14;
    pub const DATA_KILOBIT: i32 = 15;
    pub const DATA_KILOBYTE: i32 = 16;
    pub const DATA_MEGABIT: i32 = 17;
    pub const DATA_MEGABYTE: i32 = 18;
    pub const DATA_PETABIT: i32 = 19;
    pub const DATA_PETABYTE: i32 = 20;
    pub const DATA_TERABIT: i32 = 21;
    pub const DATA_TERABYTE: i32 = 22;
    pub const ENERGY_BRITISH_THERMAL_UNIT: i32 = 23;
    pub const ENERGY_CALORIE: i32 = 24;
    pub const ENERGY_ELECTRON_VOLT: i32 = 25;
    pub const ENERGY_FOOT_POUND: i32 = 26;
    pub const ENERGY_JOULE: i32 = 27;
    pub const ENERGY_KILOCALORIE: i32 = 28;
    pub const ENERGY_KILOJOULE: i32 = 29;
    pub const LENGTH_CENTIMETER: i32 = 30;
    pub const LENGTH_FOOT: i32 = 31;
    pub const LENGTH_INCH: i32 = 32;
    pub const LENGTH_KILOMETER: i32 = 33;
    pub const LENGTH_METER: i32 = 34;
    pub const LENGTH_MICRON: i32 = 35;
    pub const LENGTH_MILE: i32 = 36;
    pub const LENGTH_MILLIMETER: i32 = 37;
    pub const LENGTH_NANOMETER: i32 = 38;
    pub const LENGTH_NAUTICAL_MILE: i32 = 39;
    pub const LENGTH_YARD: i32 = 40;
    pub const POWER_BRITISH_THERMAL_UNIT_PER_MINUTE: i32 = 41;
    pub const POWER_FOOT_POUND_PER_MINUTE: i32 = 42;
    pub const POWER_HORSEPOWER: i32 = 43;
    pub const POWER_KILOWATT: i32 = 44;
    pub const POWER_WATT: i32 = 45;
    pub const TEMPERATURE_DEGREES_CELSIUS: i32 = 46;
    pub const TEMPERATURE_DEGREES_FAHRENHEIT: i32 = 47;
    pub const TEMPERATURE_KELVIN: i32 = 48;
    pub const TIME_DAY: i32 = 49;
    pub const TIME_HOUR: i32 = 50;
    pub const TIME_MICROSECOND: i32 = 51;
    pub const TIME_MILLISECOND: i32 = 52;
    pub const TIME_MINUTE: i32 = 53;
    pub const TIME_SECOND: i32 = 54;
    pub const TIME_WEEK: i32 = 55;
    pub const TIME_YEAR: i32 = 56;
    pub const SPEED_CENTIMETERS_PER_SECOND: i32 = 57;
    pub const SPEED_FEET_PER_SECOND: i32 = 58;
    pub const SPEED_KILOMETERS_PER_HOUR: i32 = 59;
    pub const SPEED_KNOT: i32 = 60;
    pub const SPEED_MACH: i32 = 61;
    pub const SPEED_METERS_PER_SECOND: i32 = 62;
    pub const SPEED_MILES_PER_HOUR: i32 = 63;
    pub const VOLUME_CUBIC_CENTIMETER: i32 = 64;
    pub const VOLUME_CUBIC_FOOT: i32 = 65;
    pub const VOLUME_CUBIC_INCH: i32 = 66;
    pub const VOLUME_CUBIC_METER: i32 = 67;
    pub const VOLUME_CUBIC_YARD: i32 = 68;
    pub const VOLUME_CUP_US: i32 = 69;
    pub const VOLUME_FLUID_OUNCE_UK: i32 = 70;
    pub const VOLUME_FLUID_OUNCE_US: i32 = 71;
    pub const VOLUME_GALLON_UK: i32 = 72;
    pub const VOLUME_GALLON_US: i32 = 73;
    pub const VOLUME_LITER: i32 = 74;
    pub const VOLUME_MILLILITER: i32 = 75;
    pub const VOLUME_PINT_UK: i32 = 76;
    pub const VOLUME_PINT_US: i32 = 77;
    pub const VOLUME_TABLESPOON_US: i32 = 78;
    pub const VOLUME_TEASPOON_US: i32 = 79;
    pub const VOLUME_QUART_UK: i32 = 80;
    pub const VOLUME_QUART_US: i32 = 81;
    pub const WEIGHT_CARAT: i32 = 82;
    pub const WEIGHT_CENTIGRAM: i32 = 83;
    pub const WEIGHT_DECIGRAM: i32 = 84;
    pub const WEIGHT_DECAGRAM: i32 = 85;
    pub const WEIGHT_GRAM: i32 = 86;
    pub const WEIGHT_HECTOGRAM: i32 = 87;
    pub const WEIGHT_KILOGRAM: i32 = 88;
    pub const WEIGHT_LONG_TON: i32 = 89;
    pub const WEIGHT_MILLIGRAM: i32 = 90;
    pub const WEIGHT_OUNCE: i32 = 91;
    pub const WEIGHT_POUND: i32 = 92;
    pub const WEIGHT_SHORT_TON: i32 = 93;
    pub const WEIGHT_STONE: i32 = 94;
    pub const WEIGHT_TONNE: i32 = 95;
    pub const AREA_SOCCER_FIELD: i32 = 99;
    pub const DATA_FLOPPY_DISK: i32 = 100;
    pub const DATA_CD: i32 = 101;
    pub const DATA_DVD: i32 = 102;
    pub const ENERGY_BATTERY: i32 = 103;
    pub const LENGTH_PAPERCLIP: i32 = 105;
    pub const LENGTH_JUMBO_JET: i32 = 107;
    pub const POWER_LIGHT_BULB: i32 = 108;
    pub const POWER_HORSE: i32 = 109;
    pub const VOLUME_BATHTUB: i32 = 111;
    pub const WEIGHT_SNOWFLAKE: i32 = 113;
    pub const WEIGHT_ELEPHANT: i32 = 114;
    pub const VOLUME_TEASPOON_UK: i32 = 115;
    pub const VOLUME_TABLESPOON_UK: i32 = 116;
    pub const AREA_HAND: i32 = 118;
    pub const SPEED_TURTLE: i32 = 121;
    pub const SPEED_JET: i32 = 122;
    pub const WEIGHT_WHALE: i32 = 123;
    pub const VOLUME_COFFEE_CUP: i32 = 124;
    pub const VOLUME_SWIMMING_POOL: i32 = 125;
    pub const SPEED_HORSE: i32 = 126;
    pub const AREA_PAPER: i32 = 127;
    pub const AREA_CASTLE: i32 = 128;
    pub const ENERGY_BANANA: i32 = 129;
    pub const ENERGY_SLICE_OF_CAKE: i32 = 130;
    pub const LENGTH_HAND: i32 = 131;
    pub const POWER_TRAIN_ENGINE: i32 = 132;
    pub const WEIGHT_SOCCER_BALL: i32 = 133;
    pub const ANGLE_DEGREE: i32 = 134;
    pub const ANGLE_RADIAN: i32 = 135;
    pub const ANGLE_GRADIAN: i32 = 136;
    pub const PRESSURE_ATMOSPHERE: i32 = 137;
    pub const PRESSURE_BAR: i32 = 138;
    pub const PRESSURE_KILO_PASCAL: i32 = 139;
    pub const PRESSURE_MILLIMETER_OF_MERCURY: i32 = 140;
    pub const PRESSURE_PASCAL: i32 = 141;
    pub const PRESSURE_PSI: i32 = 142;
    pub const DATA_EXABITS: i32 = 143;
    pub const DATA_EXABYTES: i32 = 144;
    pub const DATA_EXBIBITS: i32 = 145;
    pub const DATA_EXBIBYTES: i32 = 146;
    pub const DATA_GIBIBITS: i32 = 147;
    pub const DATA_GIBIBYTES: i32 = 148;
    pub const DATA_KIBIBITS: i32 = 149;
    pub const DATA_KIBIBYTES: i32 = 150;
    pub const DATA_MEBIBITS: i32 = 151;
    pub const DATA_MEBIBYTES: i32 = 152;
    pub const DATA_PEBIBITS: i32 = 153;
    pub const DATA_PEBIBYTES: i32 = 154;
    pub const DATA_TEBIBITS: i32 = 155;
    pub const DATA_TEBIBYTES: i32 = 156;
    pub const DATA_YOBIBITS: i32 = 157;
    pub const DATA_YOBIBYTES: i32 = 158;
    pub const DATA_YOTTABIT: i32 = 159;
    pub const DATA_YOTTABYTE: i32 = 160;
    pub const DATA_ZEBIBITS: i32 = 161;
    pub const DATA_ZEBIBYTES: i32 = 162;
    pub const DATA_ZETABITS: i32 = 163;
    pub const DATA_ZETABYTES: i32 = 164;
    pub const AREA_PYEONG: i32 = 165;
    pub const ENERGY_KILOWATTHOUR: i32 = 166;
    pub const DATA_NIBBLE: i32 = 167;
    pub const LENGTH_ANGSTROM: i32 = 168;
}

use unit_ids::*;

/// Loads unit converter data and provides conversion data for all
/// non-currency unit categories (`UnitConverterDataLoader`).
#[derive(Debug)]
pub struct UnitConverterDataLoader {
    region_code: String,
    categories: Vec<Category>,
    units_by_category: HashMap<i32, Vec<Unit>>,
    ratios_by_unit: HashMap<i32, UnitRatios>,
}

impl Default for UnitConverterDataLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl UnitConverterDataLoader {
    /// Loader for the US region (US customary default units, Fahrenheit).
    pub fn new() -> Self {
        Self::with_region("US")
    }

    /// Loader for a region given by its two-letter code (e.g. `"GB"`,
    /// `"JP"`). The region decides the default unit pairs, whether
    /// Fahrenheit is the default and whether pyeong is offered.
    pub fn with_region(region_code: &str) -> Self {
        UnitConverterDataLoader {
            region_code: if region_code.is_empty() {
                "US".into()
            } else {
                region_code.to_ascii_uppercase()
            },
            categories: Vec::new(),
            units_by_category: HashMap::new(),
            ratios_by_unit: HashMap::new(),
        }
    }

    /// The region code this loader was created with.
    pub fn region_code(&self) -> &str {
        &self.region_code
    }

    /// Returns the ordered units for a category without changing any state
    /// (`GetUnitsForCategory`; empty until [`load_data`](ConverterDataLoader::load_data)).
    pub fn get_units_for_category(&self, category: &Category) -> Vec<Unit> {
        self.get_ordered_units(category)
    }

    /// Conversion factor of each unit relative to its category's reference unit.
    // The literals are verbatim from the original data, extra digits included.
    #[allow(clippy::excessive_precision)]
    fn conversion_factor(mode: ConverterMode, unit_id: i32) -> Option<f64> {
        let factor = match mode {
            ConverterMode::Area => match unit_id {
                AREA_ACRE => 4046.8564224,
                AREA_SQUARE_METER => 1.0,
                AREA_SQUARE_FOOT => 0.09290304,
                AREA_SQUARE_YARD => 0.83612736,
                AREA_SQUARE_MILLIMETER => 0.000001,
                AREA_SQUARE_CENTIMETER => 0.0001,
                AREA_SQUARE_INCH => 0.00064516,
                AREA_SQUARE_MILE => 2589988.110336,
                AREA_SQUARE_KILOMETER => 1000000.0,
                AREA_HECTARE => 10000.0,
                AREA_HAND => 0.012516104,
                AREA_PAPER => 0.06032246,
                AREA_SOCCER_FIELD => 10869.66,
                AREA_CASTLE => 100000.0,
                AREA_PYEONG => 400.0 / 121.0,
                _ => return None,
            },
            ConverterMode::Data => match unit_id {
                DATA_BIT => 0.000000125,
                DATA_NIBBLE => 0.0000005,
                DATA_BYTE => 0.000001,
                DATA_KILOBYTE => 0.001,
                DATA_MEGABYTE => 1.0,
                DATA_GIGABYTE => 1000.0,
                DATA_TERABYTE => 1000000.0,
                DATA_PETABYTE => 1000000000.0,
                DATA_EXABYTES => 1000000000000.0,
                DATA_ZETABYTES => 1000000000000000.0,
                DATA_YOTTABYTE => 1000000000000000000.0,
                DATA_KILOBIT => 0.000125,
                DATA_MEGABIT => 0.125,
                DATA_GIGABIT => 125.0,
                DATA_TERABIT => 125000.0,
                DATA_PETABIT => 125000000.0,
                DATA_EXABITS => 125000000000.0,
                DATA_ZETABITS => 125000000000000.0,
                DATA_YOTTABIT => 125000000000000000.0,
                DATA_GIBIBITS => 134.217728,
                DATA_GIBIBYTES => 1073.741824,
                DATA_KIBIBITS => 0.000128,
                DATA_KIBIBYTES => 0.001024,
                DATA_MEBIBITS => 0.131072,
                DATA_MEBIBYTES => 1.048576,
                DATA_PEBIBITS => 140737488.355328,
                DATA_PEBIBYTES => 1125899906.842624,
                DATA_TEBIBITS => 137438.953472,
                DATA_TEBIBYTES => 1099511.627776,
                DATA_EXBIBITS => 144115188075.855872,
                DATA_EXBIBYTES => 1152921504606.846976,
                DATA_ZEBIBITS => 147573952589676.412928,
                DATA_ZEBIBYTES => 1180591620717411.303424,
                DATA_YOBIBITS => 151115727451828646.838272,
                DATA_YOBIBYTES => 1208925819614629174.706176,
                DATA_FLOPPY_DISK => 1.474560,
                DATA_CD => 700.0,
                DATA_DVD => 4700.0,
                _ => return None,
            },
            ConverterMode::Energy => match unit_id {
                ENERGY_CALORIE => 4.184,
                ENERGY_KILOCALORIE => 4184.0,
                ENERGY_BRITISH_THERMAL_UNIT => 1055.056,
                ENERGY_KILOJOULE => 1000.0,
                ENERGY_KILOWATTHOUR => 3600000.0,
                ENERGY_ELECTRON_VOLT => 0.0000000000000000001602176565,
                ENERGY_JOULE => 1.0,
                ENERGY_FOOT_POUND => 1.3558179483314,
                ENERGY_BATTERY => 9000.0,
                ENERGY_BANANA => 439614.0,
                ENERGY_SLICE_OF_CAKE => 1046700.0,
                _ => return None,
            },
            ConverterMode::Length => match unit_id {
                LENGTH_INCH => 0.0254,
                LENGTH_FOOT => 0.3048,
                LENGTH_YARD => 0.9144,
                LENGTH_MILE => 1609.344,
                LENGTH_MICRON => 0.000001,
                LENGTH_MILLIMETER => 0.001,
                LENGTH_NANOMETER => 0.000000001,
                LENGTH_ANGSTROM => 0.0000000001,
                LENGTH_CENTIMETER => 0.01,
                LENGTH_METER => 1.0,
                LENGTH_KILOMETER => 1000.0,
                LENGTH_NAUTICAL_MILE => 1852.0,
                LENGTH_PAPERCLIP => 0.035052,
                LENGTH_HAND => 0.18669,
                LENGTH_JUMBO_JET => 76.0,
                _ => return None,
            },
            ConverterMode::Power => match unit_id {
                POWER_BRITISH_THERMAL_UNIT_PER_MINUTE => 17.58426666666667,
                POWER_FOOT_POUND_PER_MINUTE => 0.0225969658055233,
                POWER_WATT => 1.0,
                POWER_KILOWATT => 1000.0,
                POWER_HORSEPOWER => 745.69987158227022,
                POWER_LIGHT_BULB => 60.0,
                POWER_HORSE => 745.7,
                POWER_TRAIN_ENGINE => 2982799.486329081,
                _ => return None,
            },
            ConverterMode::Time => match unit_id {
                TIME_DAY => 86400.0,
                TIME_SECOND => 1.0,
                TIME_WEEK => 604800.0,
                TIME_YEAR => 31557600.0,
                TIME_MILLISECOND => 0.001,
                TIME_MICROSECOND => 0.000001,
                TIME_MINUTE => 60.0,
                TIME_HOUR => 3600.0,
                _ => return None,
            },
            ConverterMode::Volume => match unit_id {
                VOLUME_CUP_US => 236.588237,
                VOLUME_PINT_US => 473.176473,
                VOLUME_PINT_UK => 568.26125,
                VOLUME_QUART_US => 946.352946,
                VOLUME_QUART_UK => 1136.5225,
                VOLUME_GALLON_US => 3785.411784,
                VOLUME_GALLON_UK => 4546.09,
                VOLUME_LITER => 1000.0,
                VOLUME_TEASPOON_US => 4.92892159375,
                VOLUME_TABLESPOON_US => 14.78676478125,
                VOLUME_CUBIC_CENTIMETER => 1.0,
                VOLUME_CUBIC_YARD => 764554.857984,
                VOLUME_CUBIC_METER => 1000000.0,
                VOLUME_MILLILITER => 1.0,
                VOLUME_CUBIC_INCH => 16.387064,
                VOLUME_CUBIC_FOOT => 28316.846592,
                VOLUME_FLUID_OUNCE_US => 29.5735295625,
                VOLUME_FLUID_OUNCE_UK => 28.4130625,
                VOLUME_TEASPOON_UK => 5.91938802083333333333,
                VOLUME_TABLESPOON_UK => 17.7581640625,
                VOLUME_COFFEE_CUP => 236.5882,
                VOLUME_BATHTUB => 378541.2,
                VOLUME_SWIMMING_POOL => 3750000000.0,
                _ => return None,
            },
            ConverterMode::Weight => match unit_id {
                WEIGHT_KILOGRAM => 1.0,
                WEIGHT_HECTOGRAM => 0.1,
                WEIGHT_DECAGRAM => 0.01,
                WEIGHT_GRAM => 0.001,
                WEIGHT_POUND => 0.45359237,
                WEIGHT_OUNCE => 0.028349523125,
                WEIGHT_MILLIGRAM => 0.000001,
                WEIGHT_CENTIGRAM => 0.00001,
                WEIGHT_DECIGRAM => 0.0001,
                WEIGHT_LONG_TON => 1016.0469088,
                WEIGHT_TONNE => 1000.0,
                WEIGHT_STONE => 6.35029318,
                WEIGHT_CARAT => 0.0002,
                WEIGHT_SHORT_TON => 907.18474,
                WEIGHT_SNOWFLAKE => 0.000002,
                WEIGHT_SOCCER_BALL => 0.4325,
                WEIGHT_ELEPHANT => 4000.0,
                WEIGHT_WHALE => 90000.0,
                _ => return None,
            },
            ConverterMode::Speed => match unit_id {
                SPEED_CENTIMETERS_PER_SECOND => 1.0,
                SPEED_FEET_PER_SECOND => 30.48,
                SPEED_KILOMETERS_PER_HOUR => 27.777777777777777777778,
                SPEED_KNOT => 51.44,
                SPEED_MACH => 34030.0,
                SPEED_METERS_PER_SECOND => 100.0,
                SPEED_MILES_PER_HOUR => 44.7,
                SPEED_TURTLE => 8.94,
                SPEED_HORSE => 2011.5,
                SPEED_JET => 24585.0,
                _ => return None,
            },
            ConverterMode::Angle => match unit_id {
                ANGLE_DEGREE => 1.0,
                ANGLE_RADIAN => 57.29577951308233,
                ANGLE_GRADIAN => 0.9,
                _ => return None,
            },
            ConverterMode::Pressure => match unit_id {
                PRESSURE_ATMOSPHERE => 1.0,
                PRESSURE_BAR => 0.9869232667160128,
                PRESSURE_KILO_PASCAL => 0.0098692326671601,
                PRESSURE_MILLIMETER_OF_MERCURY => 0.0013155687145324,
                PRESSURE_PASCAL => 9.869232667160128e-6,
                PRESSURE_PSI => 0.068045961016531,
                _ => return None,
            },
            ConverterMode::Temperature | ConverterMode::Currency => return None,
        };
        Some(factor)
    }

    /// Explicit conversion data (temperature): `(ratio, offset, offset_first)`.
    #[allow(clippy::excessive_precision)]
    fn explicit_conversions(source_id: i32) -> Option<&'static [(i32, f64, f64, bool)]> {
        const CELSIUS: &[(i32, f64, f64, bool)] = &[
            (TEMPERATURE_DEGREES_CELSIUS, 1.0, 0.0, false),
            (TEMPERATURE_DEGREES_FAHRENHEIT, 1.8, 32.0, false),
            (TEMPERATURE_KELVIN, 1.0, 273.15, false),
        ];
        const FAHRENHEIT: &[(i32, f64, f64, bool)] = &[
            (
                TEMPERATURE_DEGREES_CELSIUS,
                0.55555555555555555555555555555556,
                -32.0,
                true,
            ),
            (TEMPERATURE_DEGREES_FAHRENHEIT, 1.0, 0.0, false),
            (
                TEMPERATURE_KELVIN,
                0.55555555555555555555555555555556,
                459.67,
                true,
            ),
        ];
        const KELVIN: &[(i32, f64, f64, bool)] = &[
            (TEMPERATURE_DEGREES_CELSIUS, 1.0, -273.15, true),
            (TEMPERATURE_DEGREES_FAHRENHEIT, 1.8, -459.67, false),
            (TEMPERATURE_KELVIN, 1.0, 0.0, false),
        ];
        match source_id {
            TEMPERATURE_DEGREES_CELSIUS => Some(CELSIUS),
            TEMPERATURE_DEGREES_FAHRENHEIT => Some(FAHRENHEIT),
            TEMPERATURE_KELVIN => Some(KELVIN),
            _ => None,
        }
    }
}

impl ConverterDataLoader for UnitConverterDataLoader {
    #[rustfmt::skip] // keep the unit table one unit per line
    fn load_data(&mut self) {
        let region = self.region_code.as_str();

        // Determine region-dependent flags
        let use_us_customary_and_fahrenheit = matches!(region, "US" | "FM" | "MH" | "PW");
        let use_us_customary = use_us_customary_and_fahrenheit || region == "LR";
        let use_si = !use_us_customary;
        let use_fahrenheit = use_us_customary_and_fahrenheit || matches!(region, "BS" | "KY" | "LR");
        let use_watt_instead_of_kilowatt = region == "GB";
        let use_pyeong = matches!(region, "JP" | "TW" | "KP" | "KR");

        // Build categories — include Currency placeholder at position 0 to match the
        // navigation order.
        self.categories = ConverterMode::ALL.iter().map(|m| m.category()).collect();

        // Build units per category: (unit, order)
        let mut units_by_category: HashMap<i32, Vec<(Unit, i32)>> = HashMap::new();
        let mut add_unit = |mode: ConverterMode,
                            id: i32,
                            name_key: &str,
                            abbr_key: &str,
                            order: i32,
                            is_source: bool,
                            is_target: bool,
                            is_whimsical: bool| {
            units_by_category.entry(mode.id()).or_default().push((
                Unit::new(
                    id,
                    resource_string(name_key),
                    resource_string(abbr_key),
                    is_source,
                    is_target,
                    is_whimsical,
                ),
                order,
            ));
        };
        use ConverterMode as M;
        let (f, t) = (false, true);

        // ---- Area ----
        add_unit(M::Area, AREA_ACRE, "UnitName_Acre", "UnitAbbreviation_Acre", 9, f, f, f);
        add_unit(M::Area, AREA_HECTARE, "UnitName_Hectare", "UnitAbbreviation_Hectare", 4, f, f, f);
        add_unit(M::Area, AREA_SQUARE_CENTIMETER, "UnitName_SquareCentimeter", "UnitAbbreviation_SquareCentimeter", 2, f, f, f);
        add_unit(M::Area, AREA_SQUARE_FOOT, "UnitName_SquareFoot", "UnitAbbreviation_SquareFoot", 7, use_si, use_us_customary, f);
        add_unit(M::Area, AREA_SQUARE_INCH, "UnitName_SquareInch", "UnitAbbreviation_SquareInch", 6, f, f, f);
        add_unit(M::Area, AREA_SQUARE_KILOMETER, "UnitName_SquareKilometer", "UnitAbbreviation_SquareKilometer", 5, f, f, f);
        add_unit(M::Area, AREA_SQUARE_METER, "UnitName_SquareMeter", "UnitAbbreviation_SquareMeter", 3, use_us_customary, use_si, f);
        add_unit(M::Area, AREA_SQUARE_MILE, "UnitName_SquareMile", "UnitAbbreviation_SquareMile", 10, f, f, f);
        add_unit(M::Area, AREA_SQUARE_MILLIMETER, "UnitName_SquareMillimeter", "UnitAbbreviation_SquareMillimeter", 1, f, f, f);
        add_unit(M::Area, AREA_SQUARE_YARD, "UnitName_SquareYard", "UnitAbbreviation_SquareYard", 8, f, f, f);
        add_unit(M::Area, AREA_HAND, "UnitName_Hand", "UnitAbbreviation_Hand", 11, f, f, t);
        add_unit(M::Area, AREA_PAPER, "UnitName_Paper", "UnitAbbreviation_Paper", 12, f, f, t);
        add_unit(M::Area, AREA_SOCCER_FIELD, "UnitName_SoccerField", "UnitAbbreviation_SoccerField", 13, f, f, t);
        add_unit(M::Area, AREA_CASTLE, "UnitName_Castle", "UnitAbbreviation_Castle", 14, f, f, t);
        if use_pyeong {
            add_unit(M::Area, AREA_PYEONG, "UnitName_Pyeong", "UnitAbbreviation_Pyeong", 15, f, f, f);
        }

        // ---- Data ----
        add_unit(M::Data, DATA_BIT, "UnitName_Bit", "UnitAbbreviation_Bit", 1, f, f, f);
        add_unit(M::Data, DATA_BYTE, "UnitName_Byte", "UnitAbbreviation_Byte", 3, f, f, f);
        add_unit(M::Data, DATA_EXABITS, "UnitName_Exabits", "UnitAbbreviation_Exabits", 24, f, f, f);
        add_unit(M::Data, DATA_EXABYTES, "UnitName_Exabytes", "UnitAbbreviation_Exabytes", 26, f, f, f);
        add_unit(M::Data, DATA_EXBIBITS, "UnitName_Exbibits", "UnitAbbreviation_Exbibits", 25, f, f, f);
        add_unit(M::Data, DATA_EXBIBYTES, "UnitName_Exbibytes", "UnitAbbreviation_Exbibytes", 27, f, f, f);
        add_unit(M::Data, DATA_GIBIBITS, "UnitName_Gibibits", "UnitAbbreviation_Gibibits", 13, f, f, f);
        add_unit(M::Data, DATA_GIBIBYTES, "UnitName_Gibibytes", "UnitAbbreviation_Gibibytes", 15, f, f, f);
        add_unit(M::Data, DATA_GIGABIT, "UnitName_Gigabit", "UnitAbbreviation_Gigabit", 12, f, f, f);
        add_unit(M::Data, DATA_GIGABYTE, "UnitName_Gigabyte", "UnitAbbreviation_Gigabyte", 14, t, f, f);
        add_unit(M::Data, DATA_KIBIBITS, "UnitName_Kibibits", "UnitAbbreviation_Kibibits", 5, f, f, f);
        add_unit(M::Data, DATA_KIBIBYTES, "UnitName_Kibibytes", "UnitAbbreviation_Kibibytes", 7, f, f, f);
        add_unit(M::Data, DATA_KILOBIT, "UnitName_Kilobit", "UnitAbbreviation_Kilobit", 4, f, f, f);
        add_unit(M::Data, DATA_KILOBYTE, "UnitName_Kilobyte", "UnitAbbreviation_Kilobyte", 6, f, f, f);
        add_unit(M::Data, DATA_MEBIBITS, "UnitName_Mebibits", "UnitAbbreviation_Mebibits", 9, f, f, f);
        add_unit(M::Data, DATA_MEBIBYTES, "UnitName_Mebibytes", "UnitAbbreviation_Mebibytes", 11, f, f, f);
        add_unit(M::Data, DATA_MEGABIT, "UnitName_Megabit", "UnitAbbreviation_Megabit", 8, f, f, f);
        add_unit(M::Data, DATA_MEGABYTE, "UnitName_Megabyte", "UnitAbbreviation_Megabyte", 10, f, t, f);
        add_unit(M::Data, DATA_NIBBLE, "UnitName_Nibble", "UnitAbbreviation_Nibble", 2, f, f, f);
        add_unit(M::Data, DATA_PEBIBITS, "UnitName_Pebibits", "UnitAbbreviation_Pebibits", 21, f, f, f);
        add_unit(M::Data, DATA_PEBIBYTES, "UnitName_Pebibytes", "UnitAbbreviation_Pebibytes", 23, f, f, f);
        add_unit(M::Data, DATA_PETABIT, "UnitName_Petabit", "UnitAbbreviation_Petabit", 20, f, f, f);
        add_unit(M::Data, DATA_PETABYTE, "UnitName_Petabyte", "UnitAbbreviation_Petabyte", 22, f, f, f);
        add_unit(M::Data, DATA_TEBIBITS, "UnitName_Tebibits", "UnitAbbreviation_Tebibits", 17, f, f, f);
        add_unit(M::Data, DATA_TEBIBYTES, "UnitName_Tebibytes", "UnitAbbreviation_Tebibytes", 19, f, f, f);
        add_unit(M::Data, DATA_TERABIT, "UnitName_Terabit", "UnitAbbreviation_Terabit", 16, f, f, f);
        add_unit(M::Data, DATA_TERABYTE, "UnitName_Terabyte", "UnitAbbreviation_Terabyte", 18, f, f, f);
        add_unit(M::Data, DATA_YOBIBITS, "UnitName_Yobibits", "UnitAbbreviation_Yobibits", 33, f, f, f);
        add_unit(M::Data, DATA_YOBIBYTES, "UnitName_Yobibytes", "UnitAbbreviation_Yobibytes", 35, f, f, f);
        add_unit(M::Data, DATA_YOTTABIT, "UnitName_Yottabit", "UnitAbbreviation_Yottabit", 32, f, f, f);
        add_unit(M::Data, DATA_YOTTABYTE, "UnitName_Yottabyte", "UnitAbbreviation_Yottabyte", 34, f, f, f);
        add_unit(M::Data, DATA_ZEBIBITS, "UnitName_Zebibits", "UnitAbbreviation_Zebibits", 29, f, f, f);
        add_unit(M::Data, DATA_ZEBIBYTES, "UnitName_Zebibytes", "UnitAbbreviation_Zebibytes", 31, f, f, f);
        add_unit(M::Data, DATA_ZETABITS, "UnitName_Zetabits", "UnitAbbreviation_Zetabits", 28, f, f, f);
        add_unit(M::Data, DATA_ZETABYTES, "UnitName_Zetabytes", "UnitAbbreviation_Zetabytes", 30, f, f, f);
        add_unit(M::Data, DATA_FLOPPY_DISK, "UnitName_FloppyDisk", "UnitAbbreviation_FloppyDisk", 13, f, f, t);
        add_unit(M::Data, DATA_CD, "UnitName_CD", "UnitAbbreviation_CD", 14, f, f, t);
        add_unit(M::Data, DATA_DVD, "UnitName_DVD", "UnitAbbreviation_DVD", 15, f, f, t);

        // ---- Energy ----
        add_unit(M::Energy, ENERGY_BRITISH_THERMAL_UNIT, "UnitName_BritishThermalUnit", "UnitAbbreviation_BritishThermalUnit", 7, f, f, f);
        add_unit(M::Energy, ENERGY_CALORIE, "UnitName_Calorie", "UnitAbbreviation_Calorie", 4, f, f, f);
        add_unit(M::Energy, ENERGY_ELECTRON_VOLT, "UnitName_Electron-Volt", "UnitAbbreviation_Electron-Volt", 1, f, f, f);
        add_unit(M::Energy, ENERGY_FOOT_POUND, "UnitName_Foot-Pound", "UnitAbbreviation_Foot-Pound", 6, f, f, f);
        add_unit(M::Energy, ENERGY_JOULE, "UnitName_Joule", "UnitAbbreviation_Joule", 2, t, f, f);
        add_unit(M::Energy, ENERGY_KILOWATTHOUR, "UnitName_Kilowatthour", "UnitAbbreviation_Kilowatthour", 166, t, f, f);
        add_unit(M::Energy, ENERGY_KILOCALORIE, "UnitName_Kilocalorie", "UnitAbbreviation_Kilocalorie", 5, f, t, f);
        add_unit(M::Energy, ENERGY_KILOJOULE, "UnitName_Kilojoule", "UnitAbbreviation_Kilojoule", 3, f, f, f);
        add_unit(M::Energy, ENERGY_BATTERY, "UnitName_Battery", "UnitAbbreviation_Battery", 8, f, f, t);
        add_unit(M::Energy, ENERGY_BANANA, "UnitName_Banana", "UnitAbbreviation_Banana", 9, f, f, t);
        add_unit(M::Energy, ENERGY_SLICE_OF_CAKE, "UnitName_SliceOfCake", "UnitAbbreviation_SliceOfCake", 10, f, f, t);

        // ---- Length ----
        add_unit(M::Length, LENGTH_ANGSTROM, "UnitName_Angstrom", "UnitAbbreviation_Angstrom", 1, f, f, f);
        add_unit(M::Length, LENGTH_CENTIMETER, "UnitName_Centimeter", "UnitAbbreviation_Centimeter", 5, use_us_customary, use_si, f);
        add_unit(M::Length, LENGTH_FOOT, "UnitName_Foot", "UnitAbbreviation_Foot", 9, f, f, f);
        add_unit(M::Length, LENGTH_INCH, "UnitName_Inch", "UnitAbbreviation_Inch", 8, use_si, use_us_customary, f);
        add_unit(M::Length, LENGTH_KILOMETER, "UnitName_Kilometer", "UnitAbbreviation_Kilometer", 7, f, f, f);
        add_unit(M::Length, LENGTH_METER, "UnitName_Meter", "UnitAbbreviation_Meter", 6, f, f, f);
        add_unit(M::Length, LENGTH_MICRON, "UnitName_Micron", "UnitAbbreviation_Micron", 3, f, f, f);
        add_unit(M::Length, LENGTH_MILE, "UnitName_Mile", "UnitAbbreviation_Mile", 11, f, f, f);
        add_unit(M::Length, LENGTH_MILLIMETER, "UnitName_Millimeter", "UnitAbbreviation_Millimeter", 4, f, f, f);
        add_unit(M::Length, LENGTH_NANOMETER, "UnitName_Nanometer", "UnitAbbreviation_Nanometer", 2, f, f, f);
        add_unit(M::Length, LENGTH_NAUTICAL_MILE, "UnitName_NauticalMile", "UnitAbbreviation_NauticalMile", 12, f, f, f);
        add_unit(M::Length, LENGTH_YARD, "UnitName_Yard", "UnitAbbreviation_Yard", 10, f, f, f);
        add_unit(M::Length, LENGTH_PAPERCLIP, "UnitName_Paperclip", "UnitAbbreviation_Paperclip", 13, f, f, t);
        add_unit(M::Length, LENGTH_HAND, "UnitName_Hand", "UnitAbbreviation_Hand", 14, f, f, t);
        add_unit(M::Length, LENGTH_JUMBO_JET, "UnitName_JumboJet", "UnitAbbreviation_JumboJet", 15, f, f, t);

        // ---- Power ----
        add_unit(M::Power, POWER_BRITISH_THERMAL_UNIT_PER_MINUTE, "UnitName_BTUPerMinute", "UnitAbbreviation_BTUPerMinute", 5, f, f, f);
        add_unit(M::Power, POWER_FOOT_POUND_PER_MINUTE, "UnitName_Foot-PoundPerMinute", "UnitAbbreviation_Foot-PoundPerMinute", 4, f, f, f);
        add_unit(M::Power, POWER_HORSEPOWER, "UnitName_Horsepower", "UnitAbbreviation_Horsepower", 3, f, t, f);
        add_unit(M::Power, POWER_KILOWATT, "UnitName_Kilowatt", "UnitAbbreviation_Kilowatt", 2, !use_watt_instead_of_kilowatt, f, f);
        add_unit(M::Power, POWER_WATT, "UnitName_Watt", "UnitAbbreviation_Watt", 1, use_watt_instead_of_kilowatt, f, f);
        add_unit(M::Power, POWER_LIGHT_BULB, "UnitName_LightBulb", "UnitAbbreviation_LightBulb", 6, f, f, t);
        add_unit(M::Power, POWER_HORSE, "UnitName_Horse", "UnitAbbreviation_Horse", 7, f, f, t);
        add_unit(M::Power, POWER_TRAIN_ENGINE, "UnitName_TrainEngine", "UnitAbbreviation_TrainEngine", 8, f, f, t);

        // ---- Temperature ----
        add_unit(M::Temperature, TEMPERATURE_DEGREES_CELSIUS, "UnitName_DegreesCelsius", "UnitAbbreviation_DegreesCelsius", 1, use_fahrenheit, !use_fahrenheit, f);
        add_unit(M::Temperature, TEMPERATURE_DEGREES_FAHRENHEIT, "UnitName_DegreesFahrenheit", "UnitAbbreviation_DegreesFahrenheit", 2, !use_fahrenheit, use_fahrenheit, f);
        add_unit(M::Temperature, TEMPERATURE_KELVIN, "UnitName_Kelvin", "UnitAbbreviation_Kelvin", 3, f, f, f);

        // ---- Time ----
        add_unit(M::Time, TIME_DAY, "UnitName_Day", "UnitAbbreviation_Day", 6, f, f, f);
        add_unit(M::Time, TIME_HOUR, "UnitName_Hour", "UnitAbbreviation_Hour", 5, t, f, f);
        add_unit(M::Time, TIME_MICROSECOND, "UnitName_Microsecond", "UnitAbbreviation_Microsecond", 1, f, f, f);
        add_unit(M::Time, TIME_MILLISECOND, "UnitName_Millisecond", "UnitAbbreviation_Millisecond", 2, f, f, f);
        add_unit(M::Time, TIME_MINUTE, "UnitName_Minute", "UnitAbbreviation_Minute", 4, f, t, f);
        add_unit(M::Time, TIME_SECOND, "UnitName_Second", "UnitAbbreviation_Second", 3, f, f, f);
        add_unit(M::Time, TIME_WEEK, "UnitName_Week", "UnitAbbreviation_Week", 7, f, f, f);
        add_unit(M::Time, TIME_YEAR, "UnitName_Year", "UnitAbbreviation_Year", 8, f, f, f);

        // ---- Speed ----
        add_unit(M::Speed, SPEED_CENTIMETERS_PER_SECOND, "UnitName_CentimetersPerSecond", "UnitAbbreviation_CentimetersPerSecond", 1, f, f, f);
        add_unit(M::Speed, SPEED_FEET_PER_SECOND, "UnitName_FeetPerSecond", "UnitAbbreviation_FeetPerSecond", 4, f, f, f);
        add_unit(M::Speed, SPEED_KILOMETERS_PER_HOUR, "UnitName_KilometersPerHour", "UnitAbbreviation_KilometersPerHour", 3, use_us_customary, use_si, f);
        add_unit(M::Speed, SPEED_KNOT, "UnitName_Knot", "UnitAbbreviation_Knot", 6, f, f, f);
        add_unit(M::Speed, SPEED_MACH, "UnitName_Mach", "UnitAbbreviation_Mach", 7, f, f, f);
        add_unit(M::Speed, SPEED_METERS_PER_SECOND, "UnitName_MetersPerSecond", "UnitAbbreviation_MetersPerSecond", 2, f, f, f);
        add_unit(M::Speed, SPEED_MILES_PER_HOUR, "UnitName_MilesPerHour", "UnitAbbreviation_MilesPerHour", 5, use_si, use_us_customary, f);
        add_unit(M::Speed, SPEED_TURTLE, "UnitName_Turtle", "UnitAbbreviation_Turtle", 8, f, f, t);
        add_unit(M::Speed, SPEED_HORSE, "UnitName_Horse", "UnitAbbreviation_Horse", 9, f, f, t);
        add_unit(M::Speed, SPEED_JET, "UnitName_Jet", "UnitAbbreviation_Jet", 10, f, f, t);

        // ---- Volume ----
        add_unit(M::Volume, VOLUME_CUBIC_CENTIMETER, "UnitName_CubicCentimeter", "UnitAbbreviation_CubicCentimeter", 2, f, f, f);
        add_unit(M::Volume, VOLUME_CUBIC_FOOT, "UnitName_CubicFoot", "UnitAbbreviation_CubicFoot", 13, f, f, f);
        add_unit(M::Volume, VOLUME_CUBIC_INCH, "UnitName_CubicInch", "UnitAbbreviation_CubicInch", 12, f, f, f);
        add_unit(M::Volume, VOLUME_CUBIC_METER, "UnitName_CubicMeter", "UnitAbbreviation_CubicMeter", 4, f, f, f);
        add_unit(M::Volume, VOLUME_CUBIC_YARD, "UnitName_CubicYard", "UnitAbbreviation_CubicYard", 14, f, f, f);
        add_unit(M::Volume, VOLUME_CUP_US, "UnitName_CupUS", "UnitAbbreviation_CupUS", 8, f, f, f);
        add_unit(M::Volume, VOLUME_FLUID_OUNCE_UK, "UnitName_FluidOunceUK", "UnitAbbreviation_FluidOunceUK", 17, f, f, f);
        add_unit(M::Volume, VOLUME_FLUID_OUNCE_US, "UnitName_FluidOunceUS", "UnitAbbreviation_FluidOunceUS", 7, f, f, f);
        add_unit(M::Volume, VOLUME_GALLON_UK, "UnitName_GallonUK", "UnitAbbreviation_GallonUK", 20, f, f, f);
        add_unit(M::Volume, VOLUME_GALLON_US, "UnitName_GallonUS", "UnitAbbreviation_GallonUS", 11, f, f, f);
        add_unit(M::Volume, VOLUME_LITER, "UnitName_Liter", "UnitAbbreviation_Liter", 3, f, f, f);
        add_unit(M::Volume, VOLUME_MILLILITER, "UnitName_Milliliter", "UnitAbbreviation_Milliliter", 1, use_us_customary, use_si, f);
        add_unit(M::Volume, VOLUME_PINT_UK, "UnitName_PintUK", "UnitAbbreviation_PintUK", 18, f, f, f);
        add_unit(M::Volume, VOLUME_PINT_US, "UnitName_PintUS", "UnitAbbreviation_PintUS", 9, f, f, f);
        add_unit(M::Volume, VOLUME_TABLESPOON_US, "UnitName_TablespoonUS", "UnitAbbreviation_TablespoonUS", 6, f, f, f);
        add_unit(M::Volume, VOLUME_TEASPOON_US, "UnitName_TeaspoonUS", "UnitAbbreviation_TeaspoonUS", 5, use_si, use_us_customary && region != "GB", f);
        add_unit(M::Volume, VOLUME_QUART_UK, "UnitName_QuartUK", "UnitAbbreviation_QuartUK", 19, f, f, f);
        add_unit(M::Volume, VOLUME_QUART_US, "UnitName_QuartUS", "UnitAbbreviation_QuartUS", 10, f, f, f);
        add_unit(M::Volume, VOLUME_TEASPOON_UK, "UnitName_TeaspoonUK", "UnitAbbreviation_TeaspoonUK", 15, f, use_us_customary && region == "GB", f);
        add_unit(M::Volume, VOLUME_TABLESPOON_UK, "UnitName_TablespoonUK", "UnitAbbreviation_TablespoonUK", 16, f, f, f);
        add_unit(M::Volume, VOLUME_COFFEE_CUP, "UnitName_CoffeeCup", "UnitAbbreviation_CoffeeCup", 22, f, f, t);
        add_unit(M::Volume, VOLUME_BATHTUB, "UnitName_Bathtub", "UnitAbbreviation_Bathtub", 23, f, f, t);
        add_unit(M::Volume, VOLUME_SWIMMING_POOL, "UnitName_SwimmingPool", "UnitAbbreviation_SwimmingPool", 24, f, f, t);

        // ---- Weight ----
        add_unit(M::Weight, WEIGHT_CARAT, "UnitName_Carat", "UnitAbbreviation_Carat", 1, f, f, f);
        add_unit(M::Weight, WEIGHT_CENTIGRAM, "UnitName_Centigram", "UnitAbbreviation_Centigram", 3, f, f, f);
        add_unit(M::Weight, WEIGHT_DECIGRAM, "UnitName_Decigram", "UnitAbbreviation_Decigram", 4, f, f, f);
        add_unit(M::Weight, WEIGHT_DECAGRAM, "UnitName_Decagram", "UnitAbbreviation_Decagram", 6, f, f, f);
        add_unit(M::Weight, WEIGHT_GRAM, "UnitName_Gram", "UnitAbbreviation_Gram", 5, f, f, f);
        add_unit(M::Weight, WEIGHT_HECTOGRAM, "UnitName_Hectogram", "UnitAbbreviation_Hectogram", 7, f, f, f);
        add_unit(M::Weight, WEIGHT_KILOGRAM, "UnitName_Kilogram", "UnitAbbreviation_Kilogram", 8, use_us_customary, use_si, f);
        add_unit(M::Weight, WEIGHT_LONG_TON, "UnitName_LongTon", "UnitAbbreviation_LongTon", 14, f, f, f);
        add_unit(M::Weight, WEIGHT_MILLIGRAM, "UnitName_Milligram", "UnitAbbreviation_Milligram", 2, f, f, f);
        add_unit(M::Weight, WEIGHT_OUNCE, "UnitName_Ounce", "UnitAbbreviation_Ounce", 10, f, f, f);
        add_unit(M::Weight, WEIGHT_POUND, "UnitName_Pound", "UnitAbbreviation_Pound", 11, use_si, use_us_customary, f);
        add_unit(M::Weight, WEIGHT_SHORT_TON, "UnitName_ShortTon", "UnitAbbreviation_ShortTon", 13, f, f, f);
        add_unit(M::Weight, WEIGHT_STONE, "UnitName_Stone", "UnitAbbreviation_Stone", 12, f, f, f);
        add_unit(M::Weight, WEIGHT_TONNE, "UnitName_Tonne", "UnitAbbreviation_Tonne", 9, f, f, f);
        add_unit(M::Weight, WEIGHT_SNOWFLAKE, "UnitName_Snowflake", "UnitAbbreviation_Snowflake", 15, f, f, t);
        add_unit(M::Weight, WEIGHT_SOCCER_BALL, "UnitName_SoccerBall", "UnitAbbreviation_SoccerBall", 16, f, f, t);
        add_unit(M::Weight, WEIGHT_ELEPHANT, "UnitName_Elephant", "UnitAbbreviation_Elephant", 17, f, f, t);
        add_unit(M::Weight, WEIGHT_WHALE, "UnitName_Whale", "UnitAbbreviation_Whale", 18, f, f, t);

        // ---- Pressure ----
        add_unit(M::Pressure, PRESSURE_ATMOSPHERE, "UnitName_Atmosphere", "UnitAbbreviation_Atmosphere", 1, t, f, f);
        add_unit(M::Pressure, PRESSURE_BAR, "UnitName_Bar", "UnitAbbreviation_Bar", 2, f, t, f);
        add_unit(M::Pressure, PRESSURE_KILO_PASCAL, "UnitName_KiloPascal", "UnitAbbreviation_KiloPascal", 3, f, f, f);
        // (The original looks these keys up with a trailing space.)
        add_unit(M::Pressure, PRESSURE_MILLIMETER_OF_MERCURY, "UnitName_MillimeterOfMercury ", "UnitAbbreviation_MillimeterOfMercury ", 4, f, f, f);
        add_unit(M::Pressure, PRESSURE_PASCAL, "UnitName_Pascal", "UnitAbbreviation_Pascal", 5, f, f, f);
        add_unit(M::Pressure, PRESSURE_PSI, "UnitName_PSI", "UnitAbbreviation_PSI", 6, f, f, f);

        // ---- Angle ----
        add_unit(M::Angle, ANGLE_DEGREE, "UnitName_Degree", "UnitAbbreviation_Degree", 1, t, f, f);
        add_unit(M::Angle, ANGLE_RADIAN, "UnitName_Radian", "UnitAbbreviation_Radian", 2, f, t, f);
        add_unit(M::Angle, ANGLE_GRADIAN, "UnitName_Gradian", "UnitAbbreviation_Gradian", 3, f, f, f);

        // Sort units by order (stable, like LINQ OrderBy) and store
        self.units_by_category = units_by_category
            .into_iter()
            .map(|(category_id, mut units)| {
                units.sort_by_key(|(_, order)| *order);
                (category_id, units.into_iter().map(|(unit, _)| unit).collect())
            })
            .collect();

        // Ensure all categories have an entry (Currency gets empty)
        for category in &self.categories {
            self.units_by_category.entry(category.id).or_default();
        }

        // Build the ratio map: for each unit, compute conversions to all other units in the same category
        self.ratios_by_unit.clear();
        for category in &self.categories {
            let Some(mode) = ConverterMode::from_id(category.id) else { continue };
            if mode == ConverterMode::Currency {
                continue;
            }
            let units = &self.units_by_category[&category.id];
            for source_unit in units {
                let mut entries = UnitRatios::new();
                if let Some(explicit_map) = Self::explicit_conversions(source_unit.id) {
                    // Temperature: use explicit conversion data
                    for &(target_id, ratio, offset, offset_first) in explicit_map {
                        if let Some(target_unit) = units.iter().find(|u| u.id == target_id) {
                            entries.push((target_unit.clone(), ConversionData::new(ratio, offset, offset_first)));
                        }
                    }
                } else {
                    // Standard factor-based conversion: ratio = sourceFactor / targetFactor
                    let Some(source_factor) = Self::conversion_factor(mode, source_unit.id) else {
                        continue;
                    };
                    for target_unit in units {
                        if let Some(target_factor) = Self::conversion_factor(mode, target_unit.id)
                            && target_factor > 0.0
                        {
                            entries.push((target_unit.clone(), ConversionData::ratio(source_factor / target_factor)));
                        }
                    }
                }
                self.ratios_by_unit.insert(source_unit.id, entries);
            }
        }
    }

    fn get_ordered_categories(&self) -> Vec<Category> {
        self.categories.clone()
    }

    fn get_ordered_units(&self, category: &Category) -> Vec<Unit> {
        self.units_by_category
            .get(&category.id)
            .cloned()
            .unwrap_or_default()
    }

    fn load_ordered_ratios(&self, unit: &Unit) -> UnitRatios {
        self.ratios_by_unit
            .get(&unit.id)
            .cloned()
            .unwrap_or_default()
    }

    fn supports_category(&self, target: &Category) -> bool {
        target.id != ConverterMode::Currency.id()
    }
}
