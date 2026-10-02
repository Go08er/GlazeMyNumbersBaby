//! The navigation categories (upstream `NavCategory` / `ViewMode`).

use crate::widgets::icon::paths;

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
