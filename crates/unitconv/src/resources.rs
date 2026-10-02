// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// en-US strings extracted from Calculator/Resources/en-US/Resources.resw
// (and CEngineStrings.resw) of Windows Calculator.

//! en-US resource strings used by the unit converter.
//!
//! Values are verbatim from `Resources.resw`, except that surrounding
//! whitespace is trimmed (the original `UnitName_MillimeterOfMercury` value
//! has a trailing space and is looked up with a trailing-space key).

/// Looks up an en-US resource string; returns `""` for unknown keys (like
/// `ResourceLoader.GetString`).
pub fn resource_string(key: &str) -> &'static str {
    let key = key.trim();
    STRINGS
        .binary_search_by(|(k, _)| (*k).cmp(key))
        .map_or("", |i| STRINGS[i].1)
}

/// `CEngineStrings.resw` id 100 (`IDS_DOMAIN`): shown when a paste is rejected.
pub const INVALID_INPUT: &str = "Invalid input";

/// Substitutes `%1`, `%2`, ... in a resource format string
/// (`LocalizationStringUtil.GetLocalizedString`).
pub fn format_resource(format: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(format.len() + args.iter().map(|a| a.len()).sum::<usize>());
    let mut chars = format.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c == '%' {
            let mut number = String::new();
            while let Some(&(_, d)) = chars.peek() {
                if d.is_ascii_digit() {
                    number.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            match number.parse::<usize>() {
                Ok(n) if n >= 1 && n <= args.len() => out.push_str(args[n - 1]),
                _ => {
                    out.push('%');
                    out.push_str(&number);
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

static STRINGS: &[(&str, &str)] = &[
    ("CategoryName_AngleText", "Angle"),
    ("CategoryName_AreaText", "Area"),
    ("CategoryName_CurrencyText", "Currency"),
    ("CategoryName_DataText", "Data"),
    ("CategoryName_EnergyText", "Energy"),
    ("CategoryName_LengthText", "Length"),
    ("CategoryName_PowerText", "Power"),
    ("CategoryName_PressureText", "Pressure"),
    ("CategoryName_SpeedText", "Speed"),
    ("CategoryName_TemperatureText", "Temperature"),
    ("CategoryName_TimeText", "Time"),
    ("CategoryName_VolumeText", "Volume"),
    ("CategoryName_WeightText", "Weight and mass"),
    ("CurrencyFromToRatioFormat", "%1 %2 = %3 %4"),
    ("CurrencyRatesUpdateFailed", "Could not update rates"),
    ("CurrencyRatesUpdated", "Currency rates updated"),
    ("CurrencyTimestampFormat", "Updated %1 %2"),
    ("DataChargesMayApply", "Data charges may apply."),
    (
        "FailedToRefresh",
        "Couldn’t get new rates. Try again later.",
    ),
    ("Format_ConversionResult", "%1 %2 is %3 %4"),
    ("Format_MaxDigitsReached", "Max digits reached. %1"),
    ("Format_ValueFrom", "Convert from %1 %2"),
    ("Format_ValueTo", "Converts into %1 %2"),
    (
        "OfflineStatusHyperlinkText",
        "Offline. Please check your%HL%Network Settings%HL%",
    ),
    ("RefreshButtonText.Content", "Update rates"),
    ("SupplementaryResultsHeader.Text", "About equal to"),
    ("UnitAbbreviation_Acre", "ac"),
    ("UnitAbbreviation_Angstrom", "A"),
    ("UnitAbbreviation_Atmosphere", "atm"),
    ("UnitAbbreviation_BTUPerMinute", "BTU/min"),
    ("UnitAbbreviation_Banana", "bananas"),
    ("UnitAbbreviation_Bar", "ba"),
    ("UnitAbbreviation_Bathtub", "bathtubs"),
    ("UnitAbbreviation_Battery", "batteries"),
    ("UnitAbbreviation_Bit", "b"),
    ("UnitAbbreviation_BritishThermalUnit", "BTU"),
    ("UnitAbbreviation_Byte", "B"),
    ("UnitAbbreviation_CD", "CDs"),
    ("UnitAbbreviation_Calorie", "cal"),
    ("UnitAbbreviation_Carat", "CD"),
    ("UnitAbbreviation_Castle", "castles"),
    ("UnitAbbreviation_Centigram", "cg"),
    ("UnitAbbreviation_Centimeter", "cm"),
    ("UnitAbbreviation_CentimetersPerSecond", "cm/s"),
    ("UnitAbbreviation_CoffeeCup", "coffee cups"),
    ("UnitAbbreviation_CubicCentimeter", "cm³"),
    ("UnitAbbreviation_CubicFoot", "ft³"),
    ("UnitAbbreviation_CubicInch", "in³"),
    ("UnitAbbreviation_CubicMeter", "m³"),
    ("UnitAbbreviation_CubicYard", "yd³"),
    ("UnitAbbreviation_CupUS", "cup (US)"),
    ("UnitAbbreviation_DVD", "DVDs"),
    ("UnitAbbreviation_Day", "d"),
    ("UnitAbbreviation_Decagram", "dag"),
    ("UnitAbbreviation_Decigram", "dg"),
    ("UnitAbbreviation_Degree", "deg"),
    ("UnitAbbreviation_DegreesCelsius", "°C"),
    ("UnitAbbreviation_DegreesFahrenheit", "°F"),
    ("UnitAbbreviation_Electron-Volt", "eV"),
    ("UnitAbbreviation_Elephant", "elephants"),
    ("UnitAbbreviation_Exabits", "E"),
    ("UnitAbbreviation_Exabytes", "EB"),
    ("UnitAbbreviation_Exbibits", "Ei"),
    ("UnitAbbreviation_Exbibytes", "EiB"),
    ("UnitAbbreviation_FeetPerSecond", "ft/s"),
    ("UnitAbbreviation_FloppyDisk", "floppy disks"),
    ("UnitAbbreviation_FluidOunceUK", "fl oz (UK)"),
    ("UnitAbbreviation_FluidOunceUS", "fl oz (US)"),
    ("UnitAbbreviation_Foot", "ft"),
    ("UnitAbbreviation_Foot-Pound", "ft•lb"),
    ("UnitAbbreviation_Foot-PoundPerMinute", "ft•lb/min"),
    ("UnitAbbreviation_GallonUK", "gal (UK)"),
    ("UnitAbbreviation_GallonUS", "gal (US)"),
    ("UnitAbbreviation_Gibibits", "Gi"),
    ("UnitAbbreviation_Gibibytes", "GiB"),
    ("UnitAbbreviation_Gigabit", "Gb"),
    ("UnitAbbreviation_Gigabyte", "GB"),
    ("UnitAbbreviation_Gradian", "grad"),
    ("UnitAbbreviation_Gram", "g"),
    ("UnitAbbreviation_Hand", "hands"),
    ("UnitAbbreviation_Hectare", "ha"),
    ("UnitAbbreviation_Hectogram", "hg"),
    ("UnitAbbreviation_Horse", "horses"),
    ("UnitAbbreviation_Horsepower", "hp (US)"),
    ("UnitAbbreviation_Hour", "hr"),
    ("UnitAbbreviation_Inch", "in"),
    ("UnitAbbreviation_Jet", "jets"),
    ("UnitAbbreviation_Joule", "J"),
    ("UnitAbbreviation_JumboJet", "jumbo jets"),
    ("UnitAbbreviation_Kelvin", "K"),
    ("UnitAbbreviation_Kibibits", "Ki"),
    ("UnitAbbreviation_Kibibytes", "KiB"),
    ("UnitAbbreviation_KiloPascal", "kPa"),
    ("UnitAbbreviation_Kilobit", "Kb"),
    ("UnitAbbreviation_Kilobyte", "KB"),
    ("UnitAbbreviation_Kilocalorie", "kcal"),
    ("UnitAbbreviation_Kilogram", "kg"),
    ("UnitAbbreviation_Kilojoule", "kJ"),
    ("UnitAbbreviation_Kilometer", "km"),
    ("UnitAbbreviation_KilometersPerHour", "km/h"),
    ("UnitAbbreviation_Kilowatt", "kW"),
    ("UnitAbbreviation_Kilowatthour", "kWh"),
    ("UnitAbbreviation_Knot", "kn"),
    ("UnitAbbreviation_LightBulb", "light bulbs"),
    ("UnitAbbreviation_Liter", "L"),
    ("UnitAbbreviation_LongTon", "ton (UK)"),
    ("UnitAbbreviation_Mach", "M"),
    ("UnitAbbreviation_Mebibits", "Mi"),
    ("UnitAbbreviation_Mebibytes", "MiB"),
    ("UnitAbbreviation_Megabit", "Mb"),
    ("UnitAbbreviation_Megabyte", "MB"),
    ("UnitAbbreviation_Meter", "m"),
    ("UnitAbbreviation_MetersPerSecond", "m/s"),
    ("UnitAbbreviation_Micron", "µm"),
    ("UnitAbbreviation_Microsecond", "µs"),
    ("UnitAbbreviation_Mile", "mi"),
    ("UnitAbbreviation_MilesPerHour", "mph"),
    ("UnitAbbreviation_Milligram", "mg"),
    ("UnitAbbreviation_Milliliter", "mL"),
    ("UnitAbbreviation_Millimeter", "mm"),
    ("UnitAbbreviation_MillimeterOfMercury", "mmHg"),
    ("UnitAbbreviation_Millisecond", "ms"),
    ("UnitAbbreviation_Minute", "min"),
    ("UnitAbbreviation_Nanometer", "nm"),
    ("UnitAbbreviation_NauticalMile", "nmi"),
    ("UnitAbbreviation_Nibble", "nybl"),
    ("UnitAbbreviation_Ounce", "oz"),
    ("UnitAbbreviation_PSI", "psi"),
    ("UnitAbbreviation_Paper", "sheets of paper"),
    ("UnitAbbreviation_Paperclip", "paperclips"),
    ("UnitAbbreviation_Pascal", "Pa"),
    ("UnitAbbreviation_Pebibits", "Pi"),
    ("UnitAbbreviation_Pebibytes", "PiB"),
    ("UnitAbbreviation_Petabit", "Pb"),
    ("UnitAbbreviation_Petabyte", "PB"),
    ("UnitAbbreviation_PintUK", "pt (UK)"),
    ("UnitAbbreviation_PintUS", "pt (US)"),
    ("UnitAbbreviation_Pound", "lb"),
    ("UnitAbbreviation_Pyeong", "Pyeong"),
    ("UnitAbbreviation_QuartUK", "qt (UK)"),
    ("UnitAbbreviation_QuartUS", "qt (US)"),
    ("UnitAbbreviation_Radian", "rad"),
    ("UnitAbbreviation_Second", "s"),
    ("UnitAbbreviation_ShortTon", "ton (US)"),
    ("UnitAbbreviation_SliceOfCake", "slices of cake"),
    ("UnitAbbreviation_Snowflake", "snowflakes"),
    ("UnitAbbreviation_SoccerBall", "soccer balls"),
    ("UnitAbbreviation_SoccerField", "soccer fields"),
    ("UnitAbbreviation_SquareCentimeter", "cm²"),
    ("UnitAbbreviation_SquareFoot", "ft²"),
    ("UnitAbbreviation_SquareInch", "in²"),
    ("UnitAbbreviation_SquareKilometer", "km²"),
    ("UnitAbbreviation_SquareMeter", "m²"),
    ("UnitAbbreviation_SquareMile", "mi²"),
    ("UnitAbbreviation_SquareMillimeter", "mm²"),
    ("UnitAbbreviation_SquareYard", "yd²"),
    ("UnitAbbreviation_Stone", "st"),
    ("UnitAbbreviation_SwimmingPool", "swimming pools"),
    ("UnitAbbreviation_TablespoonUK", "tbsp. (UK)"),
    ("UnitAbbreviation_TablespoonUS", "tbsp. (US)"),
    ("UnitAbbreviation_TeaspoonUK", "tsp. (UK)"),
    ("UnitAbbreviation_TeaspoonUS", "tsp. (US)"),
    ("UnitAbbreviation_Tebibits", "Ti"),
    ("UnitAbbreviation_Tebibytes", "TiB"),
    ("UnitAbbreviation_Terabit", "Tb"),
    ("UnitAbbreviation_Terabyte", "TB"),
    ("UnitAbbreviation_Tonne", "t"),
    ("UnitAbbreviation_TrainEngine", "train engines"),
    ("UnitAbbreviation_Turtle", "turtles"),
    ("UnitAbbreviation_Watt", "W"),
    ("UnitAbbreviation_Week", "wk"),
    ("UnitAbbreviation_Whale", "whales"),
    ("UnitAbbreviation_Yard", "yd"),
    ("UnitAbbreviation_Year", "yr"),
    ("UnitAbbreviation_Yobibits", "Yi"),
    ("UnitAbbreviation_Yobibytes", "YiB"),
    ("UnitAbbreviation_Yottabit", "Y"),
    ("UnitAbbreviation_Yottabyte", "YB"),
    ("UnitAbbreviation_Zebibits", "Zi"),
    ("UnitAbbreviation_Zebibytes", "ZiB"),
    ("UnitAbbreviation_Zetabits", "Z"),
    ("UnitAbbreviation_Zetabytes", "ZB"),
    ("UnitName_Acre", "Acres"),
    ("UnitName_Angstrom", "Angstroms"),
    ("UnitName_Atmosphere", "Atmospheres"),
    ("UnitName_BTUPerMinute", "BTUs/minute"),
    ("UnitName_Banana", "bananas"),
    ("UnitName_Bar", "Bars"),
    ("UnitName_Bathtub", "bathtubs"),
    ("UnitName_Battery", "batteries"),
    ("UnitName_Bit", "Bits"),
    ("UnitName_BritishThermalUnit", "British thermal units"),
    ("UnitName_Byte", "Bytes"),
    ("UnitName_CD", "CDs"),
    ("UnitName_Calorie", "Thermal calories"),
    ("UnitName_Carat", "Carats"),
    ("UnitName_Castle", "castles"),
    ("UnitName_Centigram", "Centigrams"),
    ("UnitName_Centimeter", "Centimeters"),
    ("UnitName_CentimetersPerSecond", "Centimeters per second"),
    ("UnitName_CoffeeCup", "coffee cups"),
    ("UnitName_CubicCentimeter", "Cubic centimeters"),
    ("UnitName_CubicFoot", "Cubic feet"),
    ("UnitName_CubicInch", "Cubic inches"),
    ("UnitName_CubicMeter", "Cubic meters"),
    ("UnitName_CubicYard", "Cubic yards"),
    ("UnitName_CupUS", "Cups (US)"),
    ("UnitName_DVD", "DVDs"),
    ("UnitName_Day", "Days"),
    ("UnitName_Decagram", "Dekagrams"),
    ("UnitName_Decigram", "Decigrams"),
    ("UnitName_Degree", "Degrees"),
    ("UnitName_DegreesCelsius", "Celsius"),
    ("UnitName_DegreesFahrenheit", "Fahrenheit"),
    ("UnitName_Electron-Volt", "Electron volts"),
    ("UnitName_Elephant", "elephants"),
    ("UnitName_Exabits", "Exabits"),
    ("UnitName_Exabytes", "Exabytes"),
    ("UnitName_Exbibits", "Exbibits"),
    ("UnitName_Exbibytes", "Exbibytes"),
    ("UnitName_FeetPerSecond", "Feet per second"),
    ("UnitName_FloppyDisk", "floppy disks"),
    ("UnitName_FluidOunceUK", "Fluid ounces (UK)"),
    ("UnitName_FluidOunceUS", "Fluid ounces (US)"),
    ("UnitName_Foot", "Feet"),
    ("UnitName_Foot-Pound", "Foot-pounds"),
    ("UnitName_Foot-PoundPerMinute", "Foot-pounds/minute"),
    ("UnitName_GallonUK", "Gallons (UK)"),
    ("UnitName_GallonUS", "Gallons (US)"),
    ("UnitName_Gibibits", "Gibibits"),
    ("UnitName_Gibibytes", "Gibibytes"),
    ("UnitName_Gigabit", "Gigabits"),
    ("UnitName_Gigabyte", "Gigabytes"),
    ("UnitName_Gradian", "Gradians"),
    ("UnitName_Gram", "Grams"),
    ("UnitName_Hand", "hands"),
    ("UnitName_Hectare", "Hectares"),
    ("UnitName_Hectogram", "Hectograms"),
    ("UnitName_Horse", "horses"),
    ("UnitName_Horsepower", "Horsepower (US)"),
    ("UnitName_Hour", "Hours"),
    ("UnitName_Inch", "Inches"),
    ("UnitName_Jet", "jets"),
    ("UnitName_Joule", "Joules"),
    ("UnitName_JumboJet", "jumbo jets"),
    ("UnitName_Kelvin", "Kelvin"),
    ("UnitName_Kibibits", "Kibibits"),
    ("UnitName_Kibibytes", "Kibibytes"),
    ("UnitName_KiloPascal", "Kilopascals"),
    ("UnitName_Kilobit", "Kilobits"),
    ("UnitName_Kilobyte", "Kilobytes"),
    ("UnitName_Kilocalorie", "Food calories"),
    ("UnitName_Kilogram", "Kilograms"),
    ("UnitName_Kilojoule", "Kilojoules"),
    ("UnitName_Kilometer", "Kilometers"),
    ("UnitName_KilometersPerHour", "Kilometers per hour"),
    ("UnitName_Kilowatt", "Kilowatts"),
    ("UnitName_Kilowatthour", "Kilowatt-hours"),
    ("UnitName_Knot", "Knots"),
    ("UnitName_LightBulb", "light bulbs"),
    ("UnitName_Liter", "Liters"),
    ("UnitName_LongTon", "Long tons (UK)"),
    ("UnitName_Mach", "Mach"),
    ("UnitName_Mebibits", "Mebibits"),
    ("UnitName_Mebibytes", "Mebibytes"),
    ("UnitName_Megabit", "Megabits"),
    ("UnitName_Megabyte", "Megabytes"),
    ("UnitName_Meter", "Meters"),
    ("UnitName_MetersPerSecond", "Meters per second"),
    ("UnitName_Micron", "Microns"),
    ("UnitName_Microsecond", "Microseconds"),
    ("UnitName_Mile", "Miles"),
    ("UnitName_MilesPerHour", "Miles per hour"),
    ("UnitName_Milligram", "Milligrams"),
    ("UnitName_Milliliter", "Milliliters"),
    ("UnitName_Millimeter", "Millimeters"),
    ("UnitName_MillimeterOfMercury", "Millimeters of mercury"),
    ("UnitName_Millisecond", "Milliseconds"),
    ("UnitName_Minute", "Minutes"),
    ("UnitName_Nanometer", "Nanometers"),
    ("UnitName_NauticalMile", "Nautical miles"),
    ("UnitName_Nibble", "Nibble"),
    ("UnitName_Ounce", "Ounces"),
    ("UnitName_PSI", "Pounds per square inch"),
    ("UnitName_Paper", "sheets of paper"),
    ("UnitName_Paperclip", "paperclips"),
    ("UnitName_Pascal", "Pascals"),
    ("UnitName_Pebibits", "Pebibits"),
    ("UnitName_Pebibytes", "Pebibytes"),
    ("UnitName_Petabit", "Petabits"),
    ("UnitName_Petabyte", "Petabytes"),
    ("UnitName_PintUK", "Pints (UK)"),
    ("UnitName_PintUS", "Pints (US)"),
    ("UnitName_Pound", "Pounds"),
    ("UnitName_Pyeong", "Pyeong"),
    ("UnitName_QuartUK", "Quarts (UK)"),
    ("UnitName_QuartUS", "Quarts (US)"),
    ("UnitName_Radian", "Radians"),
    ("UnitName_Second", "Seconds"),
    ("UnitName_ShortTon", "Short tons (US)"),
    ("UnitName_SliceOfCake", "slices of cake"),
    ("UnitName_Snowflake", "snowflakes"),
    ("UnitName_SoccerBall", "soccer balls"),
    ("UnitName_SoccerField", "soccer fields"),
    ("UnitName_SquareCentimeter", "Square centimeters"),
    ("UnitName_SquareFoot", "Square feet"),
    ("UnitName_SquareInch", "Square inches"),
    ("UnitName_SquareKilometer", "Square kilometers"),
    ("UnitName_SquareMeter", "Square meters"),
    ("UnitName_SquareMile", "Square miles"),
    ("UnitName_SquareMillimeter", "Square millimeters"),
    ("UnitName_SquareYard", "Square yards"),
    ("UnitName_Stone", "Stone"),
    ("UnitName_SwimmingPool", "swimming pools"),
    ("UnitName_TablespoonUK", "Tablespoons (UK)"),
    ("UnitName_TablespoonUS", "Tablespoons (US)"),
    ("UnitName_TeaspoonUK", "Teaspoons (UK)"),
    ("UnitName_TeaspoonUS", "Teaspoons (US)"),
    ("UnitName_Tebibits", "Tebibits"),
    ("UnitName_Tebibytes", "Tebibytes"),
    ("UnitName_Terabit", "Terabits"),
    ("UnitName_Terabyte", "Terabytes"),
    ("UnitName_Tonne", "Metric tonnes"),
    ("UnitName_TrainEngine", "train engines"),
    ("UnitName_Turtle", "turtles"),
    ("UnitName_Watt", "Watts"),
    ("UnitName_Week", "Weeks"),
    ("UnitName_Whale", "whales"),
    ("UnitName_Yard", "Yards"),
    ("UnitName_Year", "Years"),
    ("UnitName_Yobibits", "Yobibits"),
    ("UnitName_Yobibytes", "Yobibytes"),
    ("UnitName_Yottabit", "Yottabits"),
    ("UnitName_Yottabyte", "Yottabytes"),
    ("UnitName_Zebibits", "Zebibits"),
    ("UnitName_Zebibytes", "Zebibytes"),
    ("UnitName_Zetabits", "Zetabits"),
    ("UnitName_Zetabytes", "Zetabytes"),
    ("UpdatingCurrencyRates", "Updating currency rates"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted() {
        for pair in STRINGS.windows(2) {
            assert!(pair[0].0 < pair[1].0, "{} !< {}", pair[0].0, pair[1].0);
        }
    }

    #[test]
    fn lookups() {
        assert_eq!(resource_string("UnitName_Mile"), "Miles");
        assert_eq!(
            resource_string("UnitName_MillimeterOfMercury "),
            "Millimeters of mercury"
        );
        assert_eq!(resource_string("UnitAbbreviation_DegreesCelsius"), "°C");
        assert_eq!(
            resource_string("CategoryName_WeightText"),
            "Weight and mass"
        );
        assert_eq!(resource_string("NoSuchKey"), "");
    }

    #[test]
    fn formatting() {
        assert_eq!(
            format_resource("%1 %2 = %3 %4", &["1", "USD", "0.88", "EUR"]),
            "1 USD = 0.88 EUR"
        );
        assert_eq!(
            format_resource("Updated %1 %2", &["10/1/2026", "9:47 PM"]),
            "Updated 10/1/2026 9:47 PM"
        );
        assert_eq!(format_resource("100%", &[]), "100%");
    }
}
