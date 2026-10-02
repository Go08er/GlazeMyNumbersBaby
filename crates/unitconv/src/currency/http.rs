// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Replacement for Calculator.ViewModels/DataLoaders/CurrencyHttpClient.cs.
// The original fetched static data and ratios from Microsoft endpoints that
// have since been retired (the open-source app ships fictional planet
// currencies instead). This client fetches real reference rates from the
// free, keyless Frankfurter API.

//! Blocking HTTP client for the Frankfurter exchange rate API.
//!
//! Call [`fetch_latest`] from a background thread; it never touches converter
//! state. Hand the result to
//! [`UnitConverterViewModel::finish_currency_fetch`](crate::UnitConverterViewModel::finish_currency_fetch).

use std::time::Duration;

use chrono::Utc;

use super::snapshot::{DEFAULT_BASE_CURRENCY, parse_frankfurter_v1, parse_frankfurter_v2};
use super::{CurrencyError, CurrencySnapshot};

/// Root of the public Frankfurter API.
pub const FRANKFURTER_API: &str = "https://api.frankfurter.dev";
/// Legacy host of the v1 API.
pub const FRANKFURTER_LEGACY_API: &str = "https://api.frankfurter.app";

/// Where and how to fetch rates.
#[derive(Clone, Debug)]
pub struct FetchConfig {
    /// API root, e.g. [`FRANKFURTER_API`] or a self-hosted instance.
    pub api_root: String,
    /// Base currency for the ratios.
    pub base: String,
    /// Restrict v2 to specific providers, e.g. `Some("ecb")` for European
    /// Central Bank reference rates only. `None` uses Frankfurter's blend of
    /// central bank sources (many more currencies).
    pub providers: Option<String>,
    /// Fall back to the v1 API (ECB only) if v2 fails.
    pub fallback_to_v1: bool,
    /// Overall timeout per request.
    pub timeout: Duration,
}

impl Default for FetchConfig {
    fn default() -> Self {
        FetchConfig {
            api_root: FRANKFURTER_API.to_owned(),
            base: DEFAULT_BASE_CURRENCY.to_owned(),
            providers: None,
            fallback_to_v1: true,
            timeout: Duration::from_secs(20),
        }
    }
}

/// Fetches the latest rates with the default configuration: Frankfurter v2
/// (`/v2/rates?base=USD` + `/v2/currencies`), falling back to v1
/// (`/v1/latest?base=USD` + `/v1/currencies`) on `api.frankfurter.dev` and
/// then on the legacy `api.frankfurter.app` host.
///
/// Blocking; call it from a background thread.
pub fn fetch_latest() -> Result<CurrencySnapshot, CurrencyError> {
    fetch_latest_with(&FetchConfig::default())
}

/// Like [`fetch_latest`] with explicit configuration.
pub fn fetch_latest_with(config: &FetchConfig) -> Result<CurrencySnapshot, CurrencyError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(config.timeout))
        .user_agent(concat!("gmnb-unitconv/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();

    let root = config.api_root.trim_end_matches('/');
    let v2 = fetch_v2(&agent, root, config);
    if v2.is_ok() || !config.fallback_to_v1 {
        return v2;
    }

    let mut last_error = v2.unwrap_err();
    let mut roots = vec![root.to_owned()];
    if root == FRANKFURTER_API {
        roots.push(FRANKFURTER_LEGACY_API.to_owned());
    }
    for root in roots {
        match fetch_v1(&agent, &root, config) {
            Ok(snapshot) => return Ok(snapshot),
            Err(e) => last_error = e,
        }
    }
    Err(last_error)
}

fn fetch_v2(
    agent: &ureq::Agent,
    root: &str,
    config: &FetchConfig,
) -> Result<CurrencySnapshot, CurrencyError> {
    let mut rates_url = format!("{root}/v2/rates?base={}", config.base);
    if let Some(providers) = &config.providers {
        rates_url.push_str("&providers=");
        rates_url.push_str(providers);
    }
    let rates = get(agent, &rates_url)?;
    // Names and symbols are nice to have; rates alone are still usable.
    let currencies = get(agent, &format!("{root}/v2/currencies")).ok();
    parse_frankfurter_v2(
        &rates,
        currencies.as_deref(),
        Utc::now(),
        &format!("{root}/v2"),
    )
}

fn fetch_v1(
    agent: &ureq::Agent,
    root: &str,
    config: &FetchConfig,
) -> Result<CurrencySnapshot, CurrencyError> {
    let latest = get(agent, &format!("{root}/v1/latest?base={}", config.base))?;
    let currencies = get(agent, &format!("{root}/v1/currencies")).ok();
    parse_frankfurter_v1(
        &latest,
        currencies.as_deref(),
        Utc::now(),
        &format!("{root}/v1"),
    )
}

fn get(agent: &ureq::Agent, url: &str) -> Result<String, CurrencyError> {
    agent
        .get(url)
        .header("Accept", "application/json")
        .call()
        .map_err(|e| CurrencyError::Http(format!("{url}: {e}")))?
        .body_mut()
        .with_config()
        .limit(8 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| CurrencyError::Http(format!("{url}: {e}")))
}
