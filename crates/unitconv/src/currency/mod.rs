// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Rust port of Windows Calculator's currency converter data layer
// (Calculator.ViewModels/DataLoaders/CurrencyDataLoader.cs,
// CurrencyHttpClient.cs, Common/NetworkManager.cs).

//! Currency data: exchange rate snapshots, the currency data loader and
//! (with the `network` feature) the Frankfurter HTTP client.
//!
//! The original app downloaded rates from Microsoft web services that no
//! longer exist. This port fetches real reference rates from the keyless
//! [Frankfurter API](https://frankfurter.dev), caches them to a
//! caller-provided path and falls back to a snapshot compiled into the crate
//! ([`CurrencySnapshot::bundled`]) when offline.

use std::fmt;

pub mod info;
mod loader;
mod snapshot;

#[cfg(feature = "network")]
mod http;

#[cfg(feature = "network")]
pub use http::{
    FRANKFURTER_API, FRANKFURTER_LEGACY_API, FetchConfig, fetch_latest, fetch_latest_with,
};
pub use loader::{
    CURRENCY_CATEGORY_ID, Clock, CurrencyDataLoader, CurrencyDataLoaderConfig, CurrencyDataSource,
    CurrencyLoadStatus, CurrencyUnit, DAY_DURATION, WEEK_DURATION, format_short_date,
    format_short_time, format_timestamp,
};
pub use snapshot::{
    BUNDLED_SNAPSHOT_DATE, CurrencyRate, CurrencySnapshot, DEFAULT_BASE_CURRENCY, MAX_CACHE_BYTES,
    SNAPSHOT_FORMAT_VERSION, load_cache, parse_frankfurter_v1, parse_frankfurter_v2, save_cache,
};

/// Whether the app may use the network for currency rates
/// (`NetworkAccessBehavior`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NetworkAccessBehavior {
    /// Unrestricted access.
    #[default]
    Normal,
    /// Metered connection: only fetch when the user explicitly asks
    /// ("Data charges may apply.").
    OptIn,
    /// No connectivity.
    Offline,
}

/// Errors from fetching, parsing or caching exchange rates.
#[derive(Debug)]
pub enum CurrencyError {
    /// Network or HTTP failure.
    Http(String),
    /// Malformed response or cache file.
    Parse(String),
    /// Cache file I/O failure.
    Io(std::io::Error),
    /// The data contained no usable rates.
    NoData,
    /// Network access is not allowed by the current [`NetworkAccessBehavior`].
    NetworkNotAllowed,
}

impl fmt::Display for CurrencyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CurrencyError::Http(e) => write!(f, "HTTP error: {e}"),
            CurrencyError::Parse(e) => write!(f, "invalid currency data: {e}"),
            CurrencyError::Io(e) => write!(f, "currency cache I/O error: {e}"),
            CurrencyError::NoData => f.write_str("no usable exchange rates"),
            CurrencyError::NetworkNotAllowed => f.write_str("network access is not allowed"),
        }
    }
}

impl std::error::Error for CurrencyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CurrencyError::Io(e) => Some(e),
            _ => None,
        }
    }
}
