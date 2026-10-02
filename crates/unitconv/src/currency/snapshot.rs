// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Part of the Rust port of Windows Calculator's currency converter
// (Calculator.ViewModels/DataLoaders/CurrencyDataLoader.cs, CurrencyHttpClient.cs).
// The original parsed two JSON documents from a (now defunct) Microsoft
// service and cached them verbatim; this module parses responses of the
// Frankfurter API (https://frankfurter.dev) into a normalized snapshot that is
// also the cache format.

//! Exchange rate snapshots: parsing Frankfurter responses, JSON
//! (de)serialization, cache files and the bundled offline snapshot.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::CurrencyError;

/// Current snapshot format version.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// The base currency rates are requested in (the original service also
/// returned ratios relative to USD).
pub const DEFAULT_BASE_CURRENCY: &str = "USD";

/// `rates_date` of the snapshot compiled into the crate.
pub const BUNDLED_SNAPSHOT_DATE: &str = "2026-10-02";

const BUNDLED_SNAPSHOT_JSON: &str = include_str!("../../data/currency-snapshot-2026-10-02.json");

/// One currency's exchange rate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurrencyRate {
    /// ISO 4217 code, e.g. `"EUR"`.
    pub code: String,
    /// Name as published by the provider, e.g. `"Euro"`.
    pub name: String,
    /// Symbol as published by the provider, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Units of this currency per one unit of the snapshot's base currency.
    pub rate: f64,
    /// Date the rate was published for (`YYYY-MM-DD`), if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// A complete set of exchange rates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurrencySnapshot {
    /// Format version ([`SNAPSHOT_FORMAT_VERSION`]).
    #[serde(default = "default_version")]
    pub version: u32,
    /// Where the data came from (API root URL).
    #[serde(default)]
    pub source: String,
    /// Base currency every [`CurrencyRate::rate`] is relative to.
    pub base: String,
    /// Newest publication date among the rates (`YYYY-MM-DD`).
    pub rates_date: String,
    /// When the rates were retrieved. This is what the `Updated <date> <time>`
    /// status line shows, like the original cache timestamp.
    pub fetched_at: DateTime<Utc>,
    /// The rates, in provider order. Includes the base currency (rate 1).
    pub currencies: Vec<CurrencyRate>,
}

fn default_version() -> u32 {
    SNAPSHOT_FORMAT_VERSION
}

impl CurrencySnapshot {
    /// Parses a snapshot from its JSON representation (cache / bundled format).
    pub fn from_json(json: &str) -> Result<Self, CurrencyError> {
        let snapshot: CurrencySnapshot =
            serde_json::from_str(json).map_err(|e| CurrencyError::Parse(e.to_string()))?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    /// Serializes the snapshot to (pretty-printed) JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("snapshot serialization cannot fail")
    }

    /// Checks that the snapshot contains at least one usable rate.
    pub fn validate(&self) -> Result<(), CurrencyError> {
        if self.version > SNAPSHOT_FORMAT_VERSION {
            return Err(CurrencyError::Parse(format!(
                "unsupported snapshot version {}",
                self.version
            )));
        }
        if self
            .currencies
            .iter()
            .any(|c| c.rate.is_finite() && c.rate > 0.0)
        {
            Ok(())
        } else {
            Err(CurrencyError::NoData)
        }
    }

    /// Looks up a currency's rate entry by code.
    pub fn rate(&self, code: &str) -> Option<&CurrencyRate> {
        self.currencies.iter().find(|c| c.code == code)
    }

    /// The snapshot compiled into the crate, used when there is no usable
    /// cache and no network.
    pub fn bundled() -> Self {
        Self::bundled_ref().clone()
    }

    /// Borrowed [`bundled`](Self::bundled) snapshot (parsed once).
    pub fn bundled_ref() -> &'static Self {
        static BUNDLED: std::sync::OnceLock<CurrencySnapshot> = std::sync::OnceLock::new();
        BUNDLED.get_or_init(|| {
            Self::from_json(BUNDLED_SNAPSHOT_JSON).expect("bundled currency snapshot is valid")
        })
    }
}

/// Reads a cached snapshot from `path`.
pub fn load_cache(path: &Path) -> Result<CurrencySnapshot, CurrencyError> {
    let file = fs::File::open(path).map_err(CurrencyError::Io)?;
    // A real cache is ~30 KB; refuse to slurp anything absurd.
    let mut json = String::new();
    file.take(MAX_CACHE_BYTES + 1)
        .read_to_string(&mut json)
        .map_err(CurrencyError::Io)?;
    if json.len() as u64 > MAX_CACHE_BYTES {
        return Err(CurrencyError::Parse("cache file is too large".into()));
    }
    CurrencySnapshot::from_json(&json)
}

/// Largest cache file [`load_cache`] reads.
pub const MAX_CACHE_BYTES: u64 = 4 * 1024 * 1024;

/// Writes `snapshot` to `path` (creating parent directories; the file is
/// replaced atomically via a temporary file unique to this writer, so two
/// instances saving at once cannot interleave their bytes).
pub fn save_cache(path: &Path, snapshot: &CurrencySnapshot) -> io::Result<()> {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let tmp = std::path::PathBuf::from(tmp);
    fs::write(&tmp, snapshot.to_json())?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

#[derive(Deserialize)]
struct V2RateRow {
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    base: Option<String>,
    quote: String,
    rate: f64,
}

#[derive(Deserialize)]
struct V2Currency {
    iso_code: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    symbol: Option<String>,
}

/// Parses the responses of Frankfurter's v2 API:
/// `GET /v2/rates?base=<base>` (an array of `{date, base, quote, rate}`) and
/// `GET /v2/currencies` (an array of `{iso_code, name, symbol, ...}`).
///
/// `currencies_json` may be `None` (names then default to the codes).
pub fn parse_frankfurter_v2(
    rates_json: &str,
    currencies_json: Option<&str>,
    fetched_at: DateTime<Utc>,
    source: &str,
) -> Result<CurrencySnapshot, CurrencyError> {
    let rows: Vec<V2RateRow> = serde_json::from_str(rates_json)
        .map_err(|e| CurrencyError::Parse(format!("rates: {e}")))?;
    if rows.is_empty() {
        return Err(CurrencyError::NoData);
    }
    let names: BTreeMap<String, V2Currency> = match currencies_json {
        Some(json) => serde_json::from_str::<Vec<V2Currency>>(json)
            .map_err(|e| CurrencyError::Parse(format!("currencies: {e}")))?
            .into_iter()
            .map(|c| (c.iso_code.clone(), c))
            .collect(),
        None => BTreeMap::new(),
    };

    let base = rows
        .iter()
        .find_map(|r| r.base.clone())
        .unwrap_or_else(|| DEFAULT_BASE_CURRENCY.to_owned());
    let rates_date = rows
        .iter()
        .filter_map(|r| r.date.clone())
        .max()
        .unwrap_or_default();

    let mut currencies: Vec<CurrencyRate> = rows
        .into_iter()
        .filter(|r| r.base.as_deref().is_none_or(|b| b == base))
        .map(|r| {
            let meta = names.get(&r.quote);
            CurrencyRate {
                name: meta
                    .and_then(|m| m.name.clone())
                    .unwrap_or_else(|| r.quote.clone()),
                symbol: meta.and_then(|m| m.symbol.clone()),
                code: r.quote,
                rate: r.rate,
                date: r.date,
            }
        })
        .collect();

    if !currencies.iter().any(|c| c.code == base) {
        let meta = names.get(&base);
        currencies.push(CurrencyRate {
            code: base.clone(),
            name: meta
                .and_then(|m| m.name.clone())
                .unwrap_or_else(|| base.clone()),
            symbol: meta.and_then(|m| m.symbol.clone()),
            rate: 1.0,
            date: Some(rates_date.clone()).filter(|d| !d.is_empty()),
        });
    }

    let snapshot = CurrencySnapshot {
        version: SNAPSHOT_FORMAT_VERSION,
        source: source.to_owned(),
        base,
        rates_date,
        fetched_at,
        currencies,
    };
    snapshot.validate()?;
    Ok(snapshot)
}

#[derive(Deserialize)]
struct V1Latest {
    #[serde(default = "one")]
    amount: f64,
    base: String,
    date: String,
    rates: BTreeMap<String, f64>,
}

fn one() -> f64 {
    1.0
}

/// Parses the responses of Frankfurter's (deprecated, ECB only) v1 API:
/// `GET /v1/latest?base=<base>` and `GET /v1/currencies` (`{code: name}`).
pub fn parse_frankfurter_v1(
    latest_json: &str,
    currencies_json: Option<&str>,
    fetched_at: DateTime<Utc>,
    source: &str,
) -> Result<CurrencySnapshot, CurrencyError> {
    let latest: V1Latest = serde_json::from_str(latest_json)
        .map_err(|e| CurrencyError::Parse(format!("latest: {e}")))?;
    let names: BTreeMap<String, String> = match currencies_json {
        Some(json) => serde_json::from_str(json)
            .map_err(|e| CurrencyError::Parse(format!("currencies: {e}")))?,
        None => BTreeMap::new(),
    };
    let amount = if latest.amount.is_finite() && latest.amount > 0.0 {
        latest.amount
    } else {
        1.0
    };

    let entry = |code: &str, rate: f64| CurrencyRate {
        code: code.to_owned(),
        name: names.get(code).cloned().unwrap_or_else(|| code.to_owned()),
        symbol: None,
        rate,
        date: Some(latest.date.clone()),
    };
    let mut currencies: Vec<CurrencyRate> = latest
        .rates
        .iter()
        .map(|(code, rate)| entry(code, rate / amount))
        .collect();
    if !latest.rates.contains_key(&latest.base) {
        currencies.push(entry(&latest.base, 1.0));
        currencies.sort_by(|a, b| a.code.cmp(&b.code));
    }

    let snapshot = CurrencySnapshot {
        version: SNAPSHOT_FORMAT_VERSION,
        source: source.to_owned(),
        base: latest.base.clone(),
        rates_date: latest.date.clone(),
        fetched_at,
        currencies,
    };
    snapshot.validate()?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_snapshot_is_valid() {
        let snapshot = CurrencySnapshot::bundled();
        assert_eq!(snapshot.rates_date, BUNDLED_SNAPSHOT_DATE);
        assert_eq!(snapshot.base, "USD");
        assert_eq!(snapshot.rate("USD").unwrap().rate, 1.0);
        assert!(snapshot.currencies.len() > 100);
    }

    #[test]
    fn json_round_trip() {
        let snapshot = CurrencySnapshot::bundled();
        let again = CurrencySnapshot::from_json(&snapshot.to_json()).unwrap();
        assert_eq!(snapshot, again);
    }

    #[test]
    fn rejects_garbage() {
        assert!(CurrencySnapshot::from_json("not json").is_err());
        assert!(matches!(
            parse_frankfurter_v2("[]", None, Utc::now(), "test"),
            Err(CurrencyError::NoData)
        ));
        assert!(parse_frankfurter_v2("{\"oops\":1}", None, Utc::now(), "test").is_err());
    }
}
