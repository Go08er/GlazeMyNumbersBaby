// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Per-category conversion tests for the Rust port of Windows Calculator's
// unit converter data (UnitConverterDataLoader).

use unitconv::converter::{Category, ConverterDataLoader, Unit, UnitConverter};
use unitconv::data_loader::unit_ids::*;
use unitconv::{
    Command, ConverterMode, UnitConverterDataLoader, UnitConverterViewModel, ViewModelConfig,
};

/// Types `input` into a fresh view model converting `from` → `to` and
/// returns the displayed (localized, en-US) result.
fn convert(mode: ConverterMode, from: i32, to: i32, input: &str) -> String {
    convert_in_region("US", mode, from, to, input)
}

fn convert_in_region(region: &str, mode: ConverterMode, from: i32, to: i32, input: &str) -> String {
    let mut vm = UnitConverterViewModel::new(ViewModelConfig {
        region: region.into(),
        ..Default::default()
    });
    vm.set_current_mode(mode);
    vm.set_unit1(from);
    vm.set_unit2(to);
    assert_eq!(
        vm.unit1().map(|u| u.id),
        Some(from),
        "unit {from} not offered in {mode:?}"
    );
    assert_eq!(
        vm.unit2().map(|u| u.id),
        Some(to),
        "unit {to} not offered in {mode:?}"
    );
    type_value(&mut vm, input);
    assert_eq!(vm.value1().replace(',', ""), input, "typed value");
    vm.value2().to_owned()
}

fn type_value(vm: &mut UnitConverterViewModel, input: &str) {
    vm.button_pressed(Command::Clear);
    let negative = input.starts_with('-');
    for c in input.trim_start_matches('-').chars() {
        let command = match c {
            '.' => Command::Decimal,
            d => Command::from_digit(d.to_digit(10).expect("digit")).unwrap(),
        };
        vm.button_pressed(command);
    }
    if negative {
        vm.button_pressed(Command::Negate);
    }
}

fn check(mode: ConverterMode, cases: &[(i32, i32, &str, &str)]) {
    for &(from, to, input, expected) in cases {
        assert_eq!(
            convert(mode, from, to, input),
            expected,
            "{mode:?}: {input} [{from}] -> [{to}]"
        );
    }
}

/// Every non-whimsical unit of the category must appear in `cases`.
fn assert_all_units_covered(mode: ConverterMode, cases: &[(i32, i32, &str, &str)]) {
    let loader = loaded("US");
    for unit in loader.get_ordered_units(&mode.category()) {
        if unit.is_whimsical {
            continue;
        }
        assert!(
            cases
                .iter()
                .any(|&(from, to, _, _)| from == unit.id || to == unit.id),
            "{mode:?}: no known conversion for {}",
            unit.name
        );
    }
}

fn loaded(region: &str) -> UnitConverterDataLoader {
    let mut loader = UnitConverterDataLoader::with_region(region);
    loader.load_data();
    loader
}

#[test]
fn length() {
    let cases = [
        (LENGTH_MILE, LENGTH_KILOMETER, "1", "1.609344"),
        (LENGTH_FOOT, LENGTH_INCH, "1", "12"),
        (LENGTH_YARD, LENGTH_FOOT, "1", "3"),
        (LENGTH_INCH, LENGTH_CENTIMETER, "1", "2.54"),
        (LENGTH_NAUTICAL_MILE, LENGTH_METER, "1", "1,852"),
        (LENGTH_KILOMETER, LENGTH_METER, "1", "1,000"),
        (LENGTH_METER, LENGTH_MILLIMETER, "1", "1,000"),
        (LENGTH_MILLIMETER, LENGTH_MICRON, "1", "1,000"),
        (LENGTH_MICRON, LENGTH_NANOMETER, "1", "1,000"),
        (LENGTH_NANOMETER, LENGTH_ANGSTROM, "1", "10"),
        (LENGTH_CENTIMETER, LENGTH_MILLIMETER, "1", "10"),
        (LENGTH_KILOMETER, LENGTH_MILE, "100", "62.13712"),
    ];
    check(ConverterMode::Length, &cases);
    assert_all_units_covered(ConverterMode::Length, &cases);
}

#[test]
fn area() {
    let cases = [
        (AREA_ACRE, AREA_SQUARE_METER, "1", "4,046.856"),
        (AREA_HECTARE, AREA_SQUARE_METER, "1", "10,000"),
        (AREA_SQUARE_KILOMETER, AREA_HECTARE, "1", "100"),
        (AREA_SQUARE_METER, AREA_SQUARE_CENTIMETER, "1", "10,000"),
        (AREA_SQUARE_CENTIMETER, AREA_SQUARE_MILLIMETER, "1", "100"),
        (AREA_SQUARE_FOOT, AREA_SQUARE_INCH, "1", "144"),
        (AREA_SQUARE_YARD, AREA_SQUARE_FOOT, "1", "9"),
        (AREA_SQUARE_MILE, AREA_ACRE, "1", "640"),
        (AREA_SQUARE_INCH, AREA_SQUARE_CENTIMETER, "1", "6.4516"),
    ];
    check(ConverterMode::Area, &cases);
    assert_all_units_covered(ConverterMode::Area, &cases);

    // Pyeong is only offered in Japan, Taiwan and Korea.
    assert_eq!(
        convert_in_region(
            "JP",
            ConverterMode::Area,
            AREA_PYEONG,
            AREA_SQUARE_METER,
            "121"
        ),
        "400"
    );
    assert_eq!(
        convert_in_region(
            "KR",
            ConverterMode::Area,
            AREA_PYEONG,
            AREA_SQUARE_METER,
            "1"
        ),
        "3.305785"
    );
    let us = loaded("US").get_ordered_units(&ConverterMode::Area.category());
    assert!(!us.iter().any(|u| u.id == AREA_PYEONG));
    let jp = loaded("JP").get_ordered_units(&ConverterMode::Area.category());
    assert_eq!(jp.last().unwrap().id, AREA_PYEONG);
}

#[test]
fn data() {
    let cases = [
        (DATA_GIBIBYTES, DATA_MEBIBYTES, "1", "1,024"),
        (DATA_GIGABYTE, DATA_MEGABYTE, "1", "1,000"),
        (DATA_BYTE, DATA_BIT, "1", "8"),
        (DATA_NIBBLE, DATA_BIT, "1", "4"),
        (DATA_KILOBYTE, DATA_BYTE, "1", "1,000"),
        (DATA_KIBIBYTES, DATA_BYTE, "1", "1,024"),
        (DATA_KILOBIT, DATA_BIT, "1", "1,000"),
        (DATA_KIBIBITS, DATA_BIT, "1", "1,024"),
        (DATA_MEGABIT, DATA_KILOBIT, "1", "1,000"),
        (DATA_MEBIBITS, DATA_KIBIBITS, "1", "1,024"),
        (DATA_GIGABIT, DATA_MEGABIT, "1", "1,000"),
        (DATA_GIBIBITS, DATA_MEBIBITS, "1", "1,024"),
        (DATA_TERABYTE, DATA_GIGABYTE, "1", "1,000"),
        (DATA_TEBIBYTES, DATA_GIBIBYTES, "1", "1,024"),
        (DATA_TERABIT, DATA_GIGABIT, "1", "1,000"),
        (DATA_TEBIBITS, DATA_GIBIBITS, "1", "1,024"),
        (DATA_PETABYTE, DATA_TERABYTE, "1", "1,000"),
        (DATA_PEBIBYTES, DATA_TEBIBYTES, "1", "1,024"),
        (DATA_PETABIT, DATA_TERABIT, "1", "1,000"),
        (DATA_PEBIBITS, DATA_TEBIBITS, "1", "1,024"),
        (DATA_EXABYTES, DATA_PETABYTE, "1", "1,000"),
        (DATA_EXBIBYTES, DATA_PEBIBYTES, "1", "1,024"),
        (DATA_EXABITS, DATA_PETABIT, "1", "1,000"),
        (DATA_EXBIBITS, DATA_PEBIBITS, "1", "1,024"),
        (DATA_ZETABYTES, DATA_EXABYTES, "1", "1,000"),
        (DATA_ZEBIBYTES, DATA_EXBIBYTES, "1", "1,024"),
        (DATA_ZETABITS, DATA_EXABITS, "1", "1,000"),
        (DATA_ZEBIBITS, DATA_EXBIBITS, "1", "1,024"),
        (DATA_YOTTABYTE, DATA_ZETABYTES, "1", "1,000"),
        (DATA_YOBIBYTES, DATA_ZEBIBYTES, "1", "1,024"),
        (DATA_YOTTABIT, DATA_ZETABITS, "1", "1,000"),
        (DATA_YOBIBITS, DATA_ZEBIBITS, "1", "1,024"),
        (DATA_GIBIBYTES, DATA_BYTE, "1", "1,073,741,824"),
        // Too large for 15 digits: scientific notation.
        (DATA_YOTTABYTE, DATA_BIT, "1", "8.000000e+24"),
    ];
    check(ConverterMode::Data, &cases);
    assert_all_units_covered(ConverterMode::Data, &cases);
}

#[test]
fn energy() {
    let cases = [
        (ENERGY_KILOCALORIE, ENERGY_CALORIE, "1", "1,000"),
        (ENERGY_KILOJOULE, ENERGY_JOULE, "1", "1,000"),
        (ENERGY_KILOWATTHOUR, ENERGY_KILOJOULE, "1", "3,600"),
        (ENERGY_CALORIE, ENERGY_JOULE, "1", "4.184"),
        (ENERGY_BRITISH_THERMAL_UNIT, ENERGY_JOULE, "1", "1,055.056"),
        (ENERGY_FOOT_POUND, ENERGY_JOULE, "1", "1.355818"),
        (ENERGY_ELECTRON_VOLT, ENERGY_JOULE, "1", "1.602177e-19"),
    ];
    check(ConverterMode::Energy, &cases);
    assert_all_units_covered(ConverterMode::Energy, &cases);
}

#[test]
fn power() {
    let cases = [
        (POWER_KILOWATT, POWER_WATT, "1", "1,000"),
        (POWER_HORSEPOWER, POWER_WATT, "1", "745.6999"),
        (
            POWER_BRITISH_THERMAL_UNIT_PER_MINUTE,
            POWER_WATT,
            "1",
            "17.58427",
        ),
        (POWER_FOOT_POUND_PER_MINUTE, POWER_WATT, "1", "0.022597"),
        (POWER_WATT, POWER_KILOWATT, "-5", "-0.005"),
    ];
    check(ConverterMode::Power, &cases);
    assert_all_units_covered(ConverterMode::Power, &cases);
}

#[test]
fn temperature() {
    use ConverterMode::Temperature as T;
    let (c, f, k) = (
        TEMPERATURE_DEGREES_CELSIUS,
        TEMPERATURE_DEGREES_FAHRENHEIT,
        TEMPERATURE_KELVIN,
    );
    let cases = [
        (c, f, "0", "32"),
        (c, k, "0", "273.15"),
        (f, k, "32", "273.15"),
        (c, f, "100", "212"),
        (f, c, "32", "0"),
        (f, c, "212", "100"),
        (k, c, "0", "-273.15"),
        (k, f, "0", "-459.67"),
        (c, f, "-40", "-40"),
        (f, c, "-40", "-40"),
        (c, f, "37", "98.6"),
        (k, c, "300", "26.85"),
    ];
    check(T, &cases);
    assert_all_units_covered(T, &cases);
}

#[test]
fn time() {
    let cases = [
        (TIME_MINUTE, TIME_SECOND, "1", "60"),
        (TIME_HOUR, TIME_MINUTE, "1", "60"),
        (TIME_DAY, TIME_HOUR, "1", "24"),
        (TIME_WEEK, TIME_DAY, "1", "7"),
        (TIME_YEAR, TIME_DAY, "1", "365.25"),
        (TIME_SECOND, TIME_MILLISECOND, "1", "1,000"),
        (TIME_MILLISECOND, TIME_MICROSECOND, "1", "1,000"),
        (TIME_WEEK, TIME_SECOND, "1", "604,800"),
    ];
    check(ConverterMode::Time, &cases);
    assert_all_units_covered(ConverterMode::Time, &cases);
}

#[test]
fn speed() {
    let cases = [
        (
            SPEED_METERS_PER_SECOND,
            SPEED_CENTIMETERS_PER_SECOND,
            "1",
            "100",
        ),
        (
            SPEED_KILOMETERS_PER_HOUR,
            SPEED_METERS_PER_SECOND,
            "36",
            "10",
        ),
        (
            SPEED_FEET_PER_SECOND,
            SPEED_CENTIMETERS_PER_SECOND,
            "1",
            "30.48",
        ),
        // The original uses 44.7 cm/s for a mile per hour.
        (
            SPEED_MILES_PER_HOUR,
            SPEED_KILOMETERS_PER_HOUR,
            "1",
            "1.6092",
        ),
        (SPEED_KNOT, SPEED_CENTIMETERS_PER_SECOND, "1", "51.44"),
        (SPEED_MACH, SPEED_METERS_PER_SECOND, "1", "340.3"),
    ];
    check(ConverterMode::Speed, &cases);
    assert_all_units_covered(ConverterMode::Speed, &cases);
}

#[test]
fn volume() {
    let cases = [
        (VOLUME_LITER, VOLUME_MILLILITER, "1", "1,000"),
        (VOLUME_CUBIC_METER, VOLUME_LITER, "1", "1,000"),
        (VOLUME_CUBIC_CENTIMETER, VOLUME_MILLILITER, "1", "1"),
        (VOLUME_GALLON_US, VOLUME_QUART_US, "1", "4"),
        (VOLUME_QUART_US, VOLUME_PINT_US, "1", "2"),
        (VOLUME_PINT_US, VOLUME_CUP_US, "1", "2"),
        (VOLUME_CUP_US, VOLUME_FLUID_OUNCE_US, "1", "8"),
        (VOLUME_FLUID_OUNCE_US, VOLUME_TABLESPOON_US, "1", "2"),
        (VOLUME_TABLESPOON_US, VOLUME_TEASPOON_US, "1", "3"),
        (VOLUME_GALLON_UK, VOLUME_QUART_UK, "1", "4"),
        (VOLUME_QUART_UK, VOLUME_PINT_UK, "1", "2"),
        (VOLUME_PINT_UK, VOLUME_FLUID_OUNCE_UK, "1", "20"),
        (VOLUME_TABLESPOON_UK, VOLUME_TEASPOON_UK, "1", "3"),
        (VOLUME_FLUID_OUNCE_UK, VOLUME_TABLESPOON_UK, "1", "1.6"),
        (VOLUME_CUBIC_FOOT, VOLUME_CUBIC_INCH, "1", "1,728"),
        (VOLUME_CUBIC_YARD, VOLUME_CUBIC_FOOT, "1", "27"),
        (VOLUME_CUBIC_INCH, VOLUME_CUBIC_CENTIMETER, "1", "16.38706"),
        (VOLUME_GALLON_US, VOLUME_LITER, "1", "3.785412"),
    ];
    check(ConverterMode::Volume, &cases);
    assert_all_units_covered(ConverterMode::Volume, &cases);
}

#[test]
fn weight() {
    let cases = [
        (WEIGHT_KILOGRAM, WEIGHT_GRAM, "1", "1,000"),
        (WEIGHT_HECTOGRAM, WEIGHT_GRAM, "1", "100"),
        (WEIGHT_DECAGRAM, WEIGHT_GRAM, "1", "10"),
        (WEIGHT_GRAM, WEIGHT_DECIGRAM, "1", "10"),
        (WEIGHT_DECIGRAM, WEIGHT_CENTIGRAM, "1", "10"),
        (WEIGHT_CENTIGRAM, WEIGHT_MILLIGRAM, "1", "10"),
        (WEIGHT_POUND, WEIGHT_OUNCE, "1", "16"),
        (WEIGHT_POUND, WEIGHT_KILOGRAM, "1", "0.453592"),
        (WEIGHT_STONE, WEIGHT_POUND, "1", "14"),
        (WEIGHT_SHORT_TON, WEIGHT_POUND, "1", "2,000"),
        (WEIGHT_LONG_TON, WEIGHT_POUND, "1", "2,240"),
        (WEIGHT_TONNE, WEIGHT_KILOGRAM, "1", "1,000"),
        (WEIGHT_CARAT, WEIGHT_MILLIGRAM, "1", "200"),
    ];
    check(ConverterMode::Weight, &cases);
    assert_all_units_covered(ConverterMode::Weight, &cases);
}

#[test]
fn pressure() {
    let cases = [
        (PRESSURE_ATMOSPHERE, PRESSURE_KILO_PASCAL, "1", "101.325"),
        (PRESSURE_BAR, PRESSURE_KILO_PASCAL, "1", "100"),
        (PRESSURE_KILO_PASCAL, PRESSURE_PASCAL, "1", "1,000"),
        // The original's mmHg factor is slightly off (true value: 760); kept as is.
        (
            PRESSURE_ATMOSPHERE,
            PRESSURE_MILLIMETER_OF_MERCURY,
            "1",
            "760.1275",
        ),
        (
            PRESSURE_MILLIMETER_OF_MERCURY,
            PRESSURE_PASCAL,
            "1",
            "133.3",
        ),
        (PRESSURE_PSI, PRESSURE_KILO_PASCAL, "1", "6.894757"),
        (PRESSURE_ATMOSPHERE, PRESSURE_PSI, "1", "14.69595"),
    ];
    check(ConverterMode::Pressure, &cases);
    assert_all_units_covered(ConverterMode::Pressure, &cases);
}

#[test]
fn angle() {
    let cases = [
        (ANGLE_DEGREE, ANGLE_RADIAN, "180", "3.141593"),
        (ANGLE_RADIAN, ANGLE_DEGREE, "1", "57.29578"),
        (ANGLE_GRADIAN, ANGLE_DEGREE, "100", "90"),
        (ANGLE_DEGREE, ANGLE_GRADIAN, "-90", "-100"),
    ];
    check(ConverterMode::Angle, &cases);
    assert_all_units_covered(ConverterMode::Angle, &cases);
}

/// Unit counts per category (US region), whimsical units included.
#[test]
fn unit_counts_and_order() {
    let loader = loaded("US");
    let count = |mode: ConverterMode| {
        let units = loader.get_ordered_units(&mode.category());
        (
            units.iter().filter(|u| !u.is_whimsical).count(),
            units.iter().filter(|u| u.is_whimsical).count(),
        )
    };
    assert_eq!(count(ConverterMode::Currency), (0, 0));
    assert_eq!(count(ConverterMode::Volume), (20, 3));
    assert_eq!(count(ConverterMode::Length), (12, 3));
    assert_eq!(count(ConverterMode::Weight), (14, 4));
    assert_eq!(count(ConverterMode::Temperature), (3, 0));
    assert_eq!(count(ConverterMode::Energy), (8, 3));
    assert_eq!(count(ConverterMode::Area), (10, 4));
    assert_eq!(count(ConverterMode::Speed), (7, 3));
    assert_eq!(count(ConverterMode::Time), (8, 0));
    assert_eq!(count(ConverterMode::Power), (5, 3));
    assert_eq!(count(ConverterMode::Data), (35, 3));
    assert_eq!(count(ConverterMode::Pressure), (6, 0));
    assert_eq!(count(ConverterMode::Angle), (3, 0));

    let names = |mode: ConverterMode| -> Vec<String> {
        loader
            .get_ordered_units(&mode.category())
            .iter()
            .map(|u| u.abbreviation.clone())
            .collect()
    };
    assert_eq!(
        names(ConverterMode::Length),
        [
            "A",
            "nm",
            "µm",
            "mm",
            "cm",
            "m",
            "km",
            "in",
            "ft",
            "yd",
            "mi",
            "nmi",
            "paperclips",
            "hands",
            "jumbo jets"
        ]
    );
    assert_eq!(names(ConverterMode::Temperature), ["°C", "°F", "K"]);
    assert_eq!(names(ConverterMode::Angle), ["deg", "rad", "grad"]);
    assert_eq!(
        names(ConverterMode::Pressure),
        ["atm", "ba", "kPa", "mmHg", "Pa", "psi"]
    );
    // Equal sort keys keep their insertion order (stable sort, like LINQ OrderBy).
    let data = names(ConverterMode::Data);
    assert_eq!(&data[..4], ["b", "nybl", "B", "Kb"]);
    let gi = data.iter().position(|n| n == "Gi").unwrap();
    assert_eq!(
        &data[gi..gi + 6],
        ["Gi", "floppy disks", "GB", "CDs", "GiB", "DVDs"]
    );
    // Kilowatt-hours sort last (order 166), even after the whimsical units.
    assert_eq!(names(ConverterMode::Energy).last().unwrap(), "kWh");
}

#[test]
fn categories_and_names() {
    let loader = loaded("US");
    let categories = loader.get_ordered_categories();
    let names: Vec<&str> = categories.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Currency",
            "Volume",
            "Length",
            "Weight and mass",
            "Temperature",
            "Energy",
            "Area",
            "Speed",
            "Time",
            "Power",
            "Data",
            "Pressure",
            "Angle"
        ]
    );
    let ids: Vec<i32> = categories.iter().map(|c| c.id).collect();
    assert_eq!(ids, [16, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    let negative: Vec<&str> = categories
        .iter()
        .filter(|c| c.supports_negative)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(negative, ["Temperature", "Power", "Angle"]);

    let mmhg = loader
        .get_ordered_units(&ConverterMode::Pressure.category())
        .into_iter()
        .find(|u| u.id == PRESSURE_MILLIMETER_OF_MERCURY)
        .unwrap();
    assert_eq!(mmhg.name, "Millimeters of mercury");
    let carat = loader
        .get_ordered_units(&ConverterMode::Weight.category())
        .into_iter()
        .find(|u| u.id == WEIGHT_CARAT)
        .unwrap();
    assert_eq!(
        (carat.name.as_str(), carat.abbreviation.as_str()),
        ("Carats", "CD")
    );
}

fn default_pair(region: &str, mode: ConverterMode) -> (String, String) {
    let mut vm = UnitConverterViewModel::new(ViewModelConfig {
        region: region.into(),
        ..Default::default()
    });
    vm.set_current_mode(mode);
    (
        vm.unit1().unwrap().abbreviation.clone(),
        vm.unit2().unwrap().abbreviation.clone(),
    )
}

#[test]
fn default_unit_pairs_us() {
    let expected = [
        (ConverterMode::Volume, "mL", "tsp. (US)"),
        (ConverterMode::Length, "cm", "in"),
        (ConverterMode::Weight, "kg", "lb"),
        (ConverterMode::Temperature, "°C", "°F"),
        (ConverterMode::Energy, "J", "kcal"),
        (ConverterMode::Area, "m²", "ft²"),
        (ConverterMode::Speed, "km/h", "mph"),
        (ConverterMode::Time, "hr", "min"),
        (ConverterMode::Power, "kW", "hp (US)"),
        (ConverterMode::Data, "GB", "MB"),
        (ConverterMode::Pressure, "atm", "ba"),
        (ConverterMode::Angle, "deg", "rad"),
    ];
    for (mode, from, to) in expected {
        assert_eq!(
            default_pair("US", mode),
            (from.to_owned(), to.to_owned()),
            "{mode:?}"
        );
    }
}

#[test]
fn default_unit_pairs_other_regions() {
    // SI regions convert from customary units.
    assert_eq!(
        default_pair("DE", ConverterMode::Length),
        ("in".into(), "cm".into())
    );
    assert_eq!(
        default_pair("DE", ConverterMode::Temperature),
        ("°F".into(), "°C".into())
    );
    assert_eq!(
        default_pair("DE", ConverterMode::Weight),
        ("lb".into(), "kg".into())
    );
    assert_eq!(
        default_pair("DE", ConverterMode::Volume),
        ("tsp. (US)".into(), "mL".into())
    );
    // The UK uses watts and UK teaspoons.
    assert_eq!(
        default_pair("GB", ConverterMode::Power),
        ("W".into(), "hp (US)".into())
    );
    // Liberia: US customary units but Fahrenheit too; the Bahamas: Fahrenheit only.
    assert_eq!(
        default_pair("LR", ConverterMode::Length),
        ("cm".into(), "in".into())
    );
    assert_eq!(
        default_pair("BS", ConverterMode::Temperature),
        ("°C".into(), "°F".into())
    );
    assert_eq!(
        default_pair("BS", ConverterMode::Length),
        ("in".into(), "cm".into())
    );
}

/// Converting A → B → A returns the original value for every unit pair.
#[test]
fn round_trip_stability() {
    let ratios = loaded("US");
    let mut loader = UnitConverterDataLoader::new();
    loader.load_data();
    let mut converter = UnitConverter::new(Box::new(loader));
    converter.initialize();
    for category in converter.get_categories() {
        if category.id == ConverterMode::Currency.id() {
            continue;
        }
        let (units, _, _) = converter.set_current_category(&category);
        for a in &units {
            for b in &units {
                for input in [Command::Seven, Command::Three] {
                    let slope = ratios
                        .load_ordered_ratios(b)
                        .into_iter()
                        .find(|(u, _)| u == a)
                        .unwrap()
                        .1
                        .ratio;
                    check_round_trip(&mut converter, &category, a, b, input, slope);
                }
            }
        }
    }
}

fn check_round_trip(
    converter: &mut UnitConverter,
    category: &Category,
    a: &Unit,
    b: &Unit,
    digit: Command,
    back_slope: f64,
) {
    converter.set_current_category(category);
    converter.set_current_unit_types(a, b);
    converter.send_command(Command::Clear);
    converter.send_command(digit);
    let original: f64 = converter.current_display().parse().unwrap();
    let there = converter.return_display().to_owned();
    // Make the result the value being edited and convert it back.
    converter.switch_active(&there);
    converter.calculate();
    let back: f64 = converter.return_display().parse().unwrap();
    let error = ((back - original) / original).abs();
    // The intermediate display is rounded to a number of decimal places, so
    // the round trip is only as good as that rounding (plus 7 significant
    // digits on the way back).
    let decimals = match there.split_once('e') {
        Some((mantissa, exponent)) => {
            mantissa.split_once('.').map_or(0, |(_, f)| f.len()) as i32
                - exponent.parse::<i32>().unwrap()
        }
        None => there.split_once('.').map_or(0, |(_, f)| f.len()) as i32,
    };
    let half_ulp = 0.5 * 10f64.powi(-decimals);
    let allowed = (half_ulp * back_slope.abs() * 1.001) / original.abs() + 1e-6;
    assert!(
        error <= allowed,
        "{}: {} {} -> {} {} -> {} {} (error {error:e})",
        category.name,
        original,
        a.abbreviation,
        there,
        b.abbreviation,
        back,
        a.abbreviation
    );
}

/// Ratios are consistent: ratio(a→b) · ratio(b→a) = 1.
#[test]
fn ratio_consistency() {
    let loader = loaded("JP");
    for category in loader.get_ordered_categories() {
        if category.id == ConverterMode::Temperature.id()
            || category.id == ConverterMode::Currency.id()
        {
            continue;
        }
        let units = loader.get_ordered_units(&category);
        for a in &units {
            let ratios = loader.load_ordered_ratios(a);
            assert_eq!(
                ratios.len(),
                units.len(),
                "{} has ratios to every unit",
                a.name
            );
            for (b, data) in &ratios {
                let back = loader
                    .load_ordered_ratios(b)
                    .into_iter()
                    .find(|(u, _)| u == a)
                    .unwrap()
                    .1;
                let product = data.ratio * back.ratio;
                assert!(
                    (product - 1.0).abs() < 1e-12,
                    "{} <-> {}: {product}",
                    a.name,
                    b.name
                );
                assert_eq!(data.offset, 0.0);
            }
        }
    }
}

/// The whimsical unit closest to "1" is appended to the supplementary results.
#[test]
fn whimsical_results() {
    let cases: [(ConverterMode, i32, &str, i32, &str, &str); 8] = [
        (
            ConverterMode::Area,
            AREA_SQUARE_METER,
            "1000",
            AREA_SOCCER_FIELD,
            "0.09",
            "soccer fields",
        ),
        (
            ConverterMode::Data,
            DATA_GIGABYTE,
            "1",
            DATA_CD,
            "1.43",
            "CDs",
        ),
        (
            ConverterMode::Energy,
            ENERGY_JOULE,
            "100000",
            ENERGY_BANANA,
            "0.23",
            "bananas",
        ),
        (
            ConverterMode::Length,
            LENGTH_METER,
            "76",
            LENGTH_JUMBO_JET,
            "1",
            "jumbo jets",
        ),
        (
            ConverterMode::Power,
            POWER_KILOWATT,
            "1",
            POWER_HORSE,
            "1.34",
            "horses",
        ),
        (
            ConverterMode::Speed,
            SPEED_KILOMETERS_PER_HOUR,
            "1",
            SPEED_TURTLE,
            "3.11",
            "turtles",
        ),
        (
            ConverterMode::Volume,
            VOLUME_MILLILITER,
            "1000",
            VOLUME_COFFEE_CUP,
            "4.23",
            "coffee cups",
        ),
        (
            ConverterMode::Weight,
            WEIGHT_KILOGRAM,
            "1",
            WEIGHT_SOCCER_BALL,
            "2.31",
            "soccer balls",
        ),
    ];
    for (mode, from, input, whimsical_id, value, abbreviation) in cases {
        let mut vm = UnitConverterViewModel::default();
        vm.set_current_mode(mode);
        vm.set_unit1(from);
        type_value(&mut vm, input);
        let results = vm.supplementary_results();
        assert!(!results.is_empty(), "{mode:?}");
        let last = results.last().unwrap();
        assert!(last.is_whimsical(), "{mode:?}: {results:?}");
        assert_eq!(
            (
                last.unit.id,
                last.value.as_str(),
                last.unit.abbreviation.as_str()
            ),
            (whimsical_id, value, abbreviation),
            "{mode:?}"
        );
        assert_eq!(results.iter().filter(|r| r.is_whimsical()).count(), 1);
        // Whimsical units are never offered in the pickers.
        assert!(!vm.units().iter().any(|u| u.is_whimsical));
    }

    // Every whimsical unit shows up for some value.
    let all_whimsical = [
        (
            ConverterMode::Area,
            AREA_SQUARE_METER,
            AREA_HAND,
            "0.012516104",
        ),
        (
            ConverterMode::Area,
            AREA_SQUARE_METER,
            AREA_PAPER,
            "0.06032246",
        ),
        (
            ConverterMode::Area,
            AREA_SQUARE_KILOMETER,
            AREA_CASTLE,
            "0.1",
        ),
        (
            ConverterMode::Data,
            DATA_MEGABYTE,
            DATA_FLOPPY_DISK,
            "1.47456",
        ),
        (ConverterMode::Data, DATA_GIGABYTE, DATA_DVD, "4.7"),
        (ConverterMode::Energy, ENERGY_KILOJOULE, ENERGY_BATTERY, "9"),
        (
            ConverterMode::Energy,
            ENERGY_KILOWATTHOUR,
            ENERGY_SLICE_OF_CAKE,
            "0.29075",
        ),
        (
            ConverterMode::Length,
            LENGTH_CENTIMETER,
            LENGTH_PAPERCLIP,
            "3.5052",
        ),
        (
            ConverterMode::Length,
            LENGTH_CENTIMETER,
            LENGTH_HAND,
            "18.669",
        ),
        (ConverterMode::Power, POWER_WATT, POWER_LIGHT_BULB, "60"),
        (
            ConverterMode::Power,
            POWER_KILOWATT,
            POWER_TRAIN_ENGINE,
            "2982.799486",
        ),
        (
            ConverterMode::Speed,
            SPEED_KILOMETERS_PER_HOUR,
            SPEED_HORSE,
            "72.414",
        ),
        (
            ConverterMode::Speed,
            SPEED_KILOMETERS_PER_HOUR,
            SPEED_JET,
            "885.06",
        ),
        (
            ConverterMode::Volume,
            VOLUME_LITER,
            VOLUME_BATHTUB,
            "378.5412",
        ),
        (
            ConverterMode::Volume,
            VOLUME_CUBIC_METER,
            VOLUME_SWIMMING_POOL,
            "3750",
        ),
        (
            ConverterMode::Weight,
            WEIGHT_MILLIGRAM,
            WEIGHT_SNOWFLAKE,
            "2",
        ),
        (ConverterMode::Weight, WEIGHT_TONNE, WEIGHT_ELEPHANT, "4"),
        (ConverterMode::Weight, WEIGHT_TONNE, WEIGHT_WHALE, "90"),
    ];
    for (mode, from, whimsical_id, input) in all_whimsical {
        let mut vm = UnitConverterViewModel::default();
        vm.set_current_mode(mode);
        vm.set_unit1(from);
        type_value(&mut vm, input);
        let last = vm.supplementary_results().last().unwrap();
        assert_eq!(
            last.unit.id,
            whimsical_id,
            "{mode:?} {input}: {:?}",
            vm.supplementary_results()
        );
        assert_eq!(last.value, "1", "{mode:?} {input}");
    }
}

/// Supplementary results: regular units sorted by |log10(value)|, rounded
/// to 2/1/0 decimals, zeros dropped, then the whimsical one.
#[test]
fn supplementary_result_ordering_and_rounding() {
    let mut vm = UnitConverterViewModel::default();
    vm.set_current_mode(ConverterMode::Length);
    vm.set_unit1(LENGTH_METER);
    vm.set_unit2(LENGTH_FOOT);
    type_value(&mut vm, "1");
    let results: Vec<(String, String)> = vm
        .supplementary_results()
        .iter()
        .map(|r| (r.value.clone(), r.unit.abbreviation.clone()))
        .collect();
    assert_eq!(results[0], ("1.09".to_owned(), "yd".to_owned()));
    assert_eq!(results[1], ("39.37".to_owned(), "in".to_owned()));
    assert_eq!(results[2], ("100".to_owned(), "cm".to_owned()));
    assert!(results.contains(&("1,000".to_owned(), "mm".to_owned())));
    assert!(results.contains(&("1,000,000".to_owned(), "µm".to_owned())));
    // 1 m in km/mi/nmi rounds to 0.00 and is dropped; nm and Å are huge.
    assert!(
        !results
            .iter()
            .any(|(_, u)| u == "km" || u == "mi" || u == "nmi")
    );
    assert_eq!(results.last().unwrap().1, "hands");
    // From/to units are not repeated.
    assert!(!results.iter().any(|(_, u)| u == "m" || u == "ft"));
}

#[test]
fn every_static_unit_converts_to_a_finite_number() {
    // Port of UnitConverterDataLoaderTests.AllStaticUnitsProduceFiniteConversionsInBothDirections.
    let mut vm = UnitConverterViewModel::default();
    let mut units_checked = 0;
    let categories: Vec<i32> = vm
        .categories()
        .iter()
        .filter(|c| c.id != ConverterMode::Currency.id())
        .map(|c| c.id)
        .collect();
    for category in categories {
        vm.set_current_category(category);
        let units: Vec<i32> = vm.units().iter().map(|u| u.id).collect();
        assert!(!units.is_empty());
        let reference = units[0];
        for unit in units {
            vm.set_unit1(unit);
            vm.set_unit2(reference);
            vm.button_pressed(Command::Clear);
            vm.button_pressed(Command::One);
            assert_is_real_number(vm.value2());

            vm.set_unit1(reference);
            vm.set_unit2(unit);
            assert_is_real_number(vm.value2());
            units_checked += 1;
        }
    }
    assert!(
        units_checked > 100,
        "only {units_checked} units were checked"
    );
}

fn assert_is_real_number(displayed: &str) {
    assert!(!displayed.trim().is_empty());
    let bare = displayed.replace(',', "");
    let value: f64 = bare
        .parse()
        .unwrap_or_else(|_| panic!("'{displayed}' is not a number"));
    assert!(value.is_finite(), "'{displayed}'");
}
