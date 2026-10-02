//! The navigation categories (upstream `NavCategory` / `ViewMode`).

use crate::icons as paths;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ViewMode {
    Standard,
    Scientific,
    Graphing,
    Programmer,
    Date,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Calculator,
    Converter,
}

impl ViewMode {
    /// Navigation order, as in the original app.
    pub const ALL: [ViewMode; 18] = [
        ViewMode::Standard,
        ViewMode::Scientific,
        ViewMode::Graphing,
        ViewMode::Programmer,
        ViewMode::Date,
        ViewMode::Currency,
        ViewMode::Volume,
        ViewMode::Length,
        ViewMode::Weight,
        ViewMode::Temperature,
        ViewMode::Energy,
        ViewMode::Area,
        ViewMode::Speed,
        ViewMode::Time,
        ViewMode::Power,
        ViewMode::Data,
        ViewMode::Pressure,
        ViewMode::Angle,
    ];

    pub fn title(self) -> &'static str {
        match self {
            ViewMode::Standard => "Standard",
            ViewMode::Scientific => "Scientific",
            ViewMode::Graphing => "Graphing",
            ViewMode::Programmer => "Programmer",
            ViewMode::Date => "Date calculation",
            ViewMode::Currency => "Currency",
            ViewMode::Volume => "Volume",
            ViewMode::Length => "Length",
            ViewMode::Weight => "Weight and mass",
            ViewMode::Temperature => "Temperature",
            ViewMode::Energy => "Energy",
            ViewMode::Area => "Area",
            ViewMode::Speed => "Speed",
            ViewMode::Time => "Time",
            ViewMode::Power => "Power",
            ViewMode::Data => "Data",
            ViewMode::Pressure => "Pressure",
            ViewMode::Angle => "Angle",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            ViewMode::Standard => "standard",
            ViewMode::Scientific => "scientific",
            ViewMode::Graphing => "graphing",
            ViewMode::Programmer => "programmer",
            ViewMode::Date => "date",
            ViewMode::Currency => "currency",
            ViewMode::Volume => "volume",
            ViewMode::Length => "length",
            ViewMode::Weight => "weight",
            ViewMode::Temperature => "temperature",
            ViewMode::Energy => "energy",
            ViewMode::Area => "area",
            ViewMode::Speed => "speed",
            ViewMode::Time => "time",
            ViewMode::Power => "power",
            ViewMode::Data => "data",
            ViewMode::Pressure => "pressure",
            ViewMode::Angle => "angle",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }

    pub fn icon(self) -> &'static str {
        match self {
            ViewMode::Standard => paths::STANDARD,
            ViewMode::Scientific => paths::SCIENTIFIC,
            ViewMode::Graphing => paths::GRAPHING,
            ViewMode::Programmer => paths::PROGRAMMER,
            ViewMode::Date => paths::DATE,
            ViewMode::Currency => paths::CURRENCY,
            ViewMode::Volume => paths::VOLUME,
            ViewMode::Length => paths::LENGTH,
            ViewMode::Weight => paths::WEIGHT,
            ViewMode::Temperature => paths::TEMPERATURE,
            ViewMode::Energy => paths::ENERGY,
            ViewMode::Area => paths::AREA,
            ViewMode::Speed => paths::SPEED,
            ViewMode::Time => paths::TIME,
            ViewMode::Power => paths::POWER,
            ViewMode::Data => paths::DATA,
            ViewMode::Pressure => paths::PRESSURE,
            ViewMode::Angle => paths::ANGLE,
        }
    }

    pub fn group(self) -> Group {
        match self {
            ViewMode::Standard
            | ViewMode::Scientific
            | ViewMode::Graphing
            | ViewMode::Programmer
            | ViewMode::Date => Group::Calculator,
            _ => Group::Converter,
        }
    }

    /// Upstream access keys: Alt+1…5 switch between the calculators.
    pub fn alt_number(self) -> Option<u32> {
        match self {
            ViewMode::Standard => Some(1),
            ViewMode::Scientific => Some(2),
            ViewMode::Graphing => Some(3),
            ViewMode::Programmer => Some(4),
            ViewMode::Date => Some(5),
            _ => None,
        }
    }

    /// The converter category behind a converter mode.
    pub fn converter_mode(self) -> Option<unitconv::ConverterMode> {
        use unitconv::ConverterMode as C;
        Some(match self {
            ViewMode::Currency => C::Currency,
            ViewMode::Volume => C::Volume,
            ViewMode::Length => C::Length,
            ViewMode::Weight => C::Weight,
            ViewMode::Temperature => C::Temperature,
            ViewMode::Energy => C::Energy,
            ViewMode::Area => C::Area,
            ViewMode::Speed => C::Speed,
            ViewMode::Time => C::Time,
            ViewMode::Power => C::Power,
            ViewMode::Data => C::Data,
            ViewMode::Pressure => C::Pressure,
            ViewMode::Angle => C::Angle,
            _ => return None,
        })
    }

    /// The calculator view-model mode behind a calculator mode.
    pub fn calc_mode(self) -> Option<calcvm::CalcMode> {
        match self {
            ViewMode::Standard => Some(calcvm::CalcMode::Standard),
            ViewMode::Scientific => Some(calcvm::CalcMode::Scientific),
            ViewMode::Programmer => Some(calcvm::CalcMode::Programmer),
            _ => None,
        }
    }

    /// Which page widget hosts this mode.
    pub fn page(self) -> PageKind {
        match self {
            ViewMode::Standard | ViewMode::Scientific | ViewMode::Programmer => {
                PageKind::Calculator
            }
            ViewMode::Graphing => PageKind::Graphing,
            ViewMode::Date => PageKind::Date,
            _ => PageKind::Converter,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageKind {
    Calculator,
    Graphing,
    Date,
    Converter,
}

impl PageKind {
    pub fn key(self) -> &'static str {
        match self {
            PageKind::Calculator => "calculator",
            PageKind::Graphing => "graphing",
            PageKind::Date => "date",
            PageKind::Converter => "converter",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_are_unique() {
        for m in ViewMode::ALL {
            assert_eq!(ViewMode::from_key(m.key()), Some(m));
        }
        let mut keys: Vec<_> = ViewMode::ALL.iter().map(|m| m.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), ViewMode::ALL.len());
    }

    #[test]
    fn every_mode_has_exactly_one_backend() {
        for m in ViewMode::ALL {
            let n = [
                m.converter_mode().is_some(),
                m.calc_mode().is_some(),
                m == ViewMode::Graphing,
                m == ViewMode::Date,
            ]
            .iter()
            .filter(|b| **b)
            .count();
            assert_eq!(n, 1, "{m:?}");
        }
        assert_eq!(
            ViewMode::ALL
                .iter()
                .filter(|m| m.converter_mode().is_some())
                .count(),
            unitconv::ConverterMode::ALL.len()
        );
    }
}
