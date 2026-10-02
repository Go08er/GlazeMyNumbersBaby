// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use unitconv::currency::{Clock, CurrencySnapshot};

/// When the fixture rates were "fetched".
pub fn fixture_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 30, 12, 0, 0).unwrap()
}

/// A small, fixed set of real-looking rates (USD base).
pub fn fixture_snapshot() -> CurrencySnapshot {
    snapshot_at(fixture_time())
}

pub fn snapshot_at(fetched_at: DateTime<Utc>) -> CurrencySnapshot {
    let json = format!(
        r#"{{
        "version": 1,
        "source": "test",
        "base": "USD",
        "rates_date": "2026-09-30",
        "fetched_at": "{}",
        "currencies": [
            {{"code": "CAD", "name": "Canadian Dollar", "symbol": "$", "rate": 1.4226}},
            {{"code": "CHF", "name": "Swiss Franc", "symbol": "CHF", "rate": 0.83451}},
            {{"code": "EUR", "name": "Euro", "symbol": "€", "rate": 0.88356}},
            {{"code": "GBP", "name": "British Pound", "symbol": "£", "rate": 0.75463}},
            {{"code": "JPY", "name": "Japanese Yen", "symbol": "¥", "rate": 157.94}},
            {{"code": "KWD", "name": "Kuwaiti Dinar", "symbol": "د.ك", "rate": 0.30844}},
            {{"code": "USD", "name": "United States Dollar", "symbol": "$", "rate": 1.0}},
            {{"code": "XAU", "name": "Gold (Troy Ounce)", "symbol": "oz t", "rate": 0.00024}}
        ]
    }}"#,
        fetched_at.to_rfc3339()
    );
    CurrencySnapshot::from_json(&json).unwrap()
}

/// The fictional planet currencies the open-source Windows Calculator ships
/// (CurrencyHttpClient.cs), as a snapshot.
#[allow(clippy::excessive_precision)] // verbatim from CurrencyHttpClient.cs
pub fn planet_snapshot(fetched_at: DateTime<Utc>) -> CurrencySnapshot {
    let currencies = [
        ("MAR", "Mars", 1.00),
        ("MON", "Moon", 0.50),
        ("NEP", "Neptune", 0.00125),
        ("SAT", "Saturn", 0.25),
        ("URA", "Uranus", 2.75),
        ("VEN", "Venus", 900.00),
        ("JUP", "Jupiter", 1.23456789123456789),
        ("MER", "Mercury", 2.00),
        ("JPY", "Test No Fractional Digits", 0.00125),
        ("JOD", "Test Fractional Digits", 0.25),
    ];
    let entries: Vec<String> = currencies
        .iter()
        .map(|(code, name, rate)| {
            format!(r#"{{"code":"{code}","name":"{name}","symbol":"¤","rate":{rate}}}"#)
        })
        .collect();
    let json = format!(
        r#"{{"base":"MAR","rates_date":"2026-09-30","fetched_at":"{}","currencies":[{}]}}"#,
        fetched_at.to_rfc3339(),
        entries.join(",")
    );
    CurrencySnapshot::from_json(&json).unwrap()
}

/// A clock frozen at `now`.
pub fn clock_at(now: DateTime<Utc>) -> Clock {
    Arc::new(move || now)
}

/// A clock `delta` after the fixture time.
pub fn clock_after_fixture(delta: TimeDelta) -> Clock {
    clock_at(fixture_time() + delta)
}

/// A unique temporary file path (not created).
pub fn temp_cache_path(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir()
        .join(format!("unitconv-test-{}-{n}", std::process::id()))
        .join(format!("{name}.json"))
}
