//! Everything GMNB and DGMNB share that isn't drawing: navigation modes,
//! key layouts and the keyboard map, settings storage, colour maths, graph
//! session persistence and a little desktop integration (D-Bus, time zone).
//!
//! The calculator engines themselves live in their own crates (`calcvm`,
//! `unitconv`, `datecalc`, `graphing`); this crate is the layer between
//! them and a UI toolkit, so each twin only has to render and route input.

pub mod color;
pub mod converter;
pub mod dbus;
pub mod dirs;
pub mod graph;
pub mod icons;
pub mod input;
pub mod keys;
pub mod modes;
pub mod settings;
pub mod tz;

pub use input::{Key, KeyPress, Named};
pub use modes::{Group, PageKind, ViewMode};
