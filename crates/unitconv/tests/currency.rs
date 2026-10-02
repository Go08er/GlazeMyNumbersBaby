// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.
//
// Port of Calculator.Tests/CurrencyConverterTests.cs plus tests for the
// Frankfurter snapshot handling of the Rust port.

mod common;

use chrono::{FixedOffset, TimeDelta, Utc};
use common::*;
use unitconv::converter::{ConverterDataLoader, CurrencyConverterDataLoader};
use unitconv::currency::{
    BUNDLED_SNAPSHOT_DATE, CurrencyDataLoader, CurrencyDataLoaderConfig, CurrencyDataSource,
    CurrencyError, CurrencyLoadStatus, CurrencySnapshot, NetworkAccessBehavior, format_timestamp,
    info, load_cache, parse_frankfurter_v1, parse_frankfurter_v2, save_cache,
};

fn loader_with_cache(
    path: Option<std::path::PathBuf>,
    now_after_fixture: TimeDelta,
) -> CurrencyDataLoader {
    CurrencyDataLoader::new(CurrencyDataLoaderConfig {
        cache_path: path,
        fallback_snapshot: Some(fixture_snapshot()),
        clock: Some(clock_after_fixture(now_after_fixture)),
        ..Default::default()
    })
}

/// A loader with the fixture rates loaded from a fresh cache.
fn loaded_loader() -> CurrencyDataLoader {
    let path = prime_cache(&fixture_snapshot());
    let mut loader = loader_with_cache(Some(path), TimeDelta::minutes(5));
    assert!(
        loader.try_load_data_from_cache(),
        "Cache load failed while setting up the test."
    );
    loader
}

fn prime_cache(snapshot: &CurrencySnapshot) -> std::path::PathBuf {
    let path = temp_cache_path("rates");
    save_cache(&path, snapshot).unwrap();
    path
}

fn id_of(loader: &CurrencyDataLoader, code: &str) -> i32 {
    loader
        .currency_unit_by_code(code)
        .unwrap_or_else(|| panic!("{code} not loaded"))
        .id
}

#[test]
fn load_from_cache_fail_no_cache_key() {
    let mut loader = loader_with_cache(Some(temp_cache_path("missing")), TimeDelta::zero());
    assert!(
        !loader.try_load_data_from_cache(),
        "Loading from cache must fail when there is no cache"
    );
    assert!(!loader.load_finished());
    assert!(!loader.loaded_from_cache());

    let mut no_path = loader_with_cache(None, TimeDelta::zero());
    assert!(!no_path.try_load_data_from_cache());
}

#[test]
fn load_from_cache_success() {
    let path = prime_cache(&fixture_snapshot());
    let mut loader = loader_with_cache(Some(path), TimeDelta::hours(1));
    assert!(loader.try_load_data_from_cache());
    assert!(loader.load_finished());
    assert!(loader.loaded_from_cache());
    assert_eq!(loader.data_source(), Some(CurrencyDataSource::Cache));
}

#[test]
fn load_from_cache_fail_corrupt_files() {
    // (The original stored static data and ratios in two files; a missing
    // or unreadable part makes the cache unusable.)
    for contents in [
        "",
        "{",
        "[]",
        r#"{"base":"USD","rates_date":"x","fetched_at":"2026-01-01T00:00:00Z","currencies":[]}"#,
    ] {
        let path = temp_cache_path("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        let mut loader = loader_with_cache(Some(path), TimeDelta::zero());
        assert!(!loader.try_load_data_from_cache(), "{contents:?}");
        assert!(!loader.loaded_from_cache());
    }
}

#[test]
fn load_from_cache_sorts_currencies_by_country_name() {
    // Unknown codes use the provider's name as the country name.
    let json = r#"{"base":"AAA","rates_date":"2026-09-30","fetched_at":"2026-09-30T12:00:00Z","currencies":[
        {"code":"AAA","name":"Zebra","symbol":"A","rate":1.0},
        {"code":"ZZZ","name":"Alpha","symbol":"Z","rate":2.0},
        {"code":"MMM","name":"Éclair","symbol":"M","rate":3.0}
    ]}"#;
    let path = prime_cache(&CurrencySnapshot::from_json(json).unwrap());
    let mut loader = loader_with_cache(Some(path), TimeDelta::zero());
    assert!(loader.try_load_data_from_cache());
    let countries: Vec<String> = loader
        .get_ordered_currency_units()
        .into_iter()
        .map(|u| u.country_name)
        .collect();
    assert_eq!(countries, ["Alpha", "Éclair", "Zebra"]);
}

#[test]
fn loaded_load_ordered_units() {
    let loader = loaded_loader();
    let units = loader.get_ordered_currency_units();
    assert!(!units.is_empty(), "No currency units were loaded.");
    for unit in &units {
        assert!(
            !unit.abbreviation.is_empty(),
            "A currency unit had no abbreviation."
        );
        assert!(
            !unit.country_name.is_empty(),
            "A currency unit had no country name."
        );
    }
    // Sorted by country name; precious metals are not currencies.
    let names: Vec<String> = loader
        .get_ordered_units(&unitconv::ConverterMode::Currency.category())
        .into_iter()
        .map(|u| u.name)
        .collect();
    assert_eq!(
        names,
        [
            "Canada - Dollar",
            "Europe - Euro",
            "Japan - Yen",
            "Kuwait - Dinar",
            "Switzerland - Franc",
            "United Kingdom - Pound",
            "United States - Dollar"
        ]
    );
    // Ids follow the static unit ids.
    assert_eq!(units[0].id, 169);
    let usd = loader.currency_unit_by_code("USD").unwrap();
    assert!(usd.is_conversion_source);
    assert!(
        loader
            .currency_unit_by_code("EUR")
            .unwrap()
            .is_conversion_target
    );
    assert_eq!(usd.to_unit().accessible_name, "United States Dollar");
}

#[test]
fn loaded_load_ordered_ratios() {
    let loader = loaded_loader();
    let units = loader.get_ordered_currency_units();
    let ratios = loader.load_ordered_currency_ratios(units[0].id);
    assert!(!ratios.is_empty(), "The first currency unit had no ratios.");
    assert!(
        ratios.contains_key(&units[0].id),
        "A currency should always convert to itself."
    );
    assert_eq!(ratios[&units[0].id].ratio, 1.0);
}

#[test]
fn ratio_math() {
    let loader = loaded_loader();
    let (usd, eur, jpy) = (
        id_of(&loader, "USD"),
        id_of(&loader, "EUR"),
        id_of(&loader, "JPY"),
    );
    let ratio = |a: i32, b: i32| loader.load_ordered_currency_ratios(a)[&b].ratio;
    assert_eq!(ratio(usd, eur), 0.88356);
    assert_eq!(ratio(usd, jpy), 157.94);
    assert!((ratio(eur, usd) - 1.0 / 0.88356).abs() < 1e-15);
    assert!((ratio(eur, jpy) - 157.94 / 0.88356).abs() < 1e-12);
    for a in loader.currency_units() {
        for b in loader.currency_units() {
            let product = ratio(a.id, b.id) * ratio(b.id, a.id);
            assert!(
                (product - 1.0).abs() < 1e-14,
                "{} <-> {}",
                a.abbreviation,
                b.abbreviation
            );
        }
    }
}

#[test]
fn loaded_get_currency_symbols_valid() {
    let loader = loaded_loader();
    let symbols = loader.get_currency_symbols_by_id(id_of(&loader, "USD"), id_of(&loader, "EUR"));
    assert_eq!(symbols, ("$".to_owned(), "€".to_owned()));
    let units = loader.get_ordered_currency_units();
    let symbols = loader.get_currency_symbols(&units[0].to_unit(), &units[1].to_unit());
    assert!(!symbols.0.is_empty() && !symbols.1.is_empty());
}

#[test]
fn loaded_get_currency_symbols_invalid() {
    let loader = loaded_loader();
    let symbols = loader.get_currency_symbols_by_id(-1, -2);
    assert_eq!(
        symbols,
        (String::new(), String::new()),
        "An unknown unit must not yield a symbol."
    );
}

#[test]
fn loaded_get_currency_ratio_equality_valid() {
    let loader = loaded_loader();
    let units = loader.get_ordered_currency_units();
    let (ratio, accessible) = loader.get_currency_ratio_equality_by_id(units[0].id, units[1].id);
    assert!(!ratio.is_empty(), "The ratio line was empty.");
    assert!(
        !accessible.is_empty(),
        "The accessible ratio line was empty."
    );
    // The accessible form spells the currencies out rather than abbreviating them.
    assert!(accessible.contains(&units[0].country_name));
    assert!(accessible.contains(&units[0].name));

    let (usd, eur, jpy, kwd) = (
        id_of(&loader, "USD"),
        id_of(&loader, "EUR"),
        id_of(&loader, "JPY"),
        id_of(&loader, "KWD"),
    );
    assert_eq!(
        loader.get_currency_ratio_equality_by_id(usd, eur),
        (
            "1 USD = 0.8836 EUR".to_owned(),
            "1 United States Dollar = 0.8836 Europe Euro".to_owned()
        )
    );
    assert_eq!(
        loader.get_currency_ratio_equality_by_id(eur, usd).0,
        "1 EUR = 1.1318 USD"
    );
    assert_eq!(
        loader.get_currency_ratio_equality_by_id(usd, jpy).0,
        "1 USD = 157.94 JPY"
    );
    assert_eq!(
        loader.get_currency_ratio_equality_by_id(jpy, usd).0,
        "1 JPY = 0.006332 USD"
    );
    assert_eq!(
        loader.get_currency_ratio_equality_by_id(usd, usd).0,
        "1 USD = 1.00 USD"
    );
    assert_eq!(
        loader.get_currency_ratio_equality_by_id(jpy, kwd).0,
        "1 JPY = 0.001953 KWD"
    );
    assert_eq!(
        loader.get_currency_ratio_equality_by_id(kwd, jpy).0,
        "1 KWD = 512.0607 JPY"
    );
}

#[test]
fn loaded_get_currency_ratio_equality_invalid() {
    let loader = loaded_loader();
    assert_eq!(
        loader.get_currency_ratio_equality_by_id(-1, -2),
        (String::new(), String::new())
    );
}

#[test]
fn load_from_web_success() {
    let mut loader = loader_with_cache(None, TimeDelta::zero());
    assert!(loader.try_load_data_from_web(Ok(fixture_snapshot())));
    assert!(loader.load_finished());
    assert!(loader.loaded_from_web());
}

#[test]
fn load_from_web_failure() {
    let mut loader = loader_with_cache(None, TimeDelta::zero());
    assert!(!loader.try_load_data_from_web(Err(CurrencyError::Http("boom".into()))));
    assert!(!loader.load_finished());
    assert!(!loader.try_load_data_from_web(Err(CurrencyError::NoData)));
}

#[test]
fn load_success_loaded_from_cache() {
    // A cache written moments ago is still fresh, so the loader uses it and never goes out.
    let path = prime_cache(&fixture_snapshot());
    let mut loader = loader_with_cache(Some(path), TimeDelta::minutes(1));
    loader.load_data();
    assert!(loader.loaded_from_cache());
    assert!(!loader.loaded_from_web());
    assert!(!loader.needs_web_refresh());
}

#[test]
fn load_success_loaded_from_web() {
    // A cache older than a day is refreshed from the web rather than used as-is.
    let path = prime_cache(&fixture_snapshot());
    let mut loader = loader_with_cache(Some(path.clone()), TimeDelta::days(2));
    loader.load_data();
    assert!(loader.loaded_from_cache());
    assert!(loader.needs_web_refresh());

    let fresh = snapshot_at(fixture_time() + TimeDelta::days(2));
    assert!(loader.finish_web_load(Ok(fresh.clone())));
    assert!(
        loader.loaded_from_web(),
        "A stale cache should have been refreshed from the web."
    );
    assert!(!loader.needs_web_refresh());
    // The fresh rates were written to the cache.
    assert_eq!(load_cache(&path).unwrap(), fresh);
}

#[test]
fn stale_cache_survives_failed_background_refresh() {
    let path = prime_cache(&fixture_snapshot());
    let mut loader = loader_with_cache(Some(path), TimeDelta::days(3));
    loader.load_data();
    assert!(loader.needs_web_refresh());
    assert!(!loader.finish_web_load(Err(CurrencyError::Http("offline".into()))));
    // Like the original (web failure falls back to the cache), the data stays loaded.
    assert!(loader.loaded_from_cache());
    assert!(!loader.currency_units().is_empty());
    // One automatic attempt per session.
    assert!(!loader.needs_web_refresh());
}

#[test]
fn no_cache_uses_offline_snapshot() {
    let mut loader = loader_with_cache(Some(temp_cache_path("none")), TimeDelta::hours(1));
    loader.load_data();
    assert_eq!(loader.load_status(), CurrencyLoadStatus::LoadedFromBundle);
    assert_eq!(loader.data_source(), Some(CurrencyDataSource::Bundled));
    assert!(loader.load_finished());
    assert!(loader.needs_web_refresh());

    // The real bundled snapshot.
    let mut bundled = CurrencyDataLoader::default();
    bundled.load_data();
    assert_eq!(
        bundled.snapshot().unwrap().rates_date,
        BUNDLED_SNAPSHOT_DATE
    );
    assert!(bundled.currency_units().len() > 150);
    for excluded in info::EXCLUDED_CODES {
        assert!(
            bundled.currency_unit_by_code(excluded).is_none(),
            "{excluded}"
        );
    }
    // Every bundled currency has table metadata.
    for unit in bundled.currency_units() {
        assert!(
            info::currency_info(&unit.abbreviation).is_some(),
            "{} has no metadata",
            unit.abbreviation
        );
    }
    let gbp = bundled.currency_unit_by_code("GBP").unwrap();
    assert_eq!(
        (
            gbp.country_name.as_str(),
            gbp.name.as_str(),
            gbp.symbol.as_str()
        ),
        ("United Kingdom", "Pound", "£")
    );
}

#[test]
fn newer_bundled_snapshot_beats_old_cache() {
    let old = snapshot_at(fixture_time() - TimeDelta::days(400));
    let path = prime_cache(&old);
    let mut loader = loader_with_cache(Some(path), TimeDelta::hours(1));
    loader.load_data();
    assert_eq!(loader.data_source(), Some(CurrencyDataSource::Bundled));
    assert_eq!(loader.cache_timestamp(), Some(fixture_time()));
}

#[test]
fn network_behavior_gates_web_loads() {
    let mut loader = loader_with_cache(None, TimeDelta::zero());
    loader.on_network_behavior_changed(NetworkAccessBehavior::Offline);
    loader.load_data();
    assert!(!loader.needs_web_refresh());
    assert!(!loader.try_load_data_from_web(Ok(fixture_snapshot())));
    // REVIEW_2 R2-M-02: "offline" is the connectivity monitor's guess. Rates
    // the user explicitly asked for that did arrive are used.
    assert!(loader.try_load_data_from_web_override(Ok(fixture_snapshot())));
    assert!(loader.loaded_from_web());
    // A failed explicit refresh still fails.
    let mut offline = loader_with_cache(None, TimeDelta::zero());
    offline.on_network_behavior_changed(NetworkAccessBehavior::Offline);
    assert!(!offline.try_load_data_from_web_override(Err(CurrencyError::NetworkNotAllowed)));
    assert_eq!(offline.load_status(), CurrencyLoadStatus::FailedToLoad);

    // Metered: only an explicit refresh may use the network.
    let mut metered = loader_with_cache(None, TimeDelta::zero());
    metered.on_network_behavior_changed(NetworkAccessBehavior::OptIn);
    metered.load_data();
    assert!(!metered.needs_web_refresh());
    assert!(!metered.try_load_data_from_web(Ok(fixture_snapshot())));
    assert!(metered.try_load_data_from_web_override(Ok(fixture_snapshot())));
    assert!(metered.loaded_from_web());
}

#[test]
fn failed_refresh_keeps_previous_rates() {
    let mut loader = loaded_loader();
    let before = loader.get_ordered_currency_units();
    assert!(!loader.try_load_data_from_web_override(Err(CurrencyError::Http("timeout".into()))));
    assert_eq!(loader.load_status(), CurrencyLoadStatus::FailedToLoad);
    assert_eq!(loader.get_ordered_currency_units(), before);
}

#[test]
fn timestamp_and_week_old_flag() {
    let loader = loaded_loader();
    assert_eq!(
        format_timestamp(fixture_time(), FixedOffset::west_opt(5 * 3600).unwrap()),
        "Updated 9/30/2026 7:00 AM"
    );
    assert!(
        loader
            .get_currency_timestamp()
            .starts_with("Updated 9/30/2026 ")
            || loader
                .get_currency_timestamp()
                .starts_with("Updated 10/1/2026 ")
    );
    assert!(!loader.is_week_old());

    let path = prime_cache(&fixture_snapshot());
    let mut old = loader_with_cache(Some(path), TimeDelta::days(8));
    old.load_data();
    assert!(old.is_week_old());

    let empty = CurrencyDataLoader::default();
    assert_eq!(empty.get_currency_timestamp(), "");
}

#[test]
fn default_currency_pair_follows_language_and_last_used() {
    let load = |language: &str, last_used: Option<(&str, &str)>| {
        let mut loader = CurrencyDataLoader::new(CurrencyDataLoaderConfig {
            response_language: language.into(),
            ..Default::default()
        });
        if let Some((from, to)) = last_used {
            loader.set_last_used_currencies(from, to);
        }
        loader.load_data();
        let from = loader
            .currency_units()
            .iter()
            .find(|u| u.is_conversion_source)
            .unwrap()
            .abbreviation
            .clone();
        let to = loader
            .currency_units()
            .iter()
            .find(|u| u.is_conversion_target)
            .unwrap()
            .abbreviation
            .clone();
        (from, to)
    };
    // LocaleDefaultCurrencyMapIsPackaged
    assert_eq!(load("en-US", None), ("USD".into(), "EUR".into()));
    assert_eq!(load("en-GB", None), ("GBP".into(), "USD".into()));
    assert_eq!(load("en-CA", None), ("CAD".into(), "USD".into()));
    assert_eq!(load("ja-JP", None), ("USD".into(), "JPY".into()));
    assert_eq!(load("xx-YY", None), ("USD".into(), "EUR".into()));
    // es-VE maps to the retired VEF: falls back to USD -> EUR.
    assert_eq!(load("es-VE", None), ("USD".into(), "EUR".into()));
    // Last used currencies win.
    assert_eq!(
        load("en-US", Some(("CHF", "JPY"))),
        ("CHF".into(), "JPY".into())
    );
}

#[test]
fn test_round_currency_ratio() {
    let cases: [(f64, f64); 27] = [
        (1234567.0, 1234567.0),
        (0.0, 0.0),
        (9999.999, 9999.999),
        (8765.4321, 8765.4321),
        (4815.162342, 4815.1623),
        (4815.162358, 4815.1624),
        (4815.162388934723, 4815.1624),
        (0.12, 0.12),
        (0.123, 0.123),
        (0.1234, 0.1234),
        (0.12343, 0.1234),
        (0.0321, 0.0321),
        (0.03211, 0.03211),
        (0.032119, 0.03212),
        (0.00322119, 0.003221),
        (0.00123269, 0.001233),
        (0.00076269, 0.0007627),
        (0.000069, 0.000069),
        (0.000061, 0.000061),
        (0.000054612, 0.00005461),
        (0.000054616, 0.00005462),
        (0.000005416, 0.000005416),
        (0.0000016134324, 0.000001613),
        (0.0000096134324, 0.000009613),
        (0.0000032169348392, 0.000003217),
        (0.000000002134987218, 0.000000002135),
        (0.000000000000087231445, 0.00000000000008723),
    ];
    for (ratio, expected) in cases {
        assert_eq!(
            CurrencyDataLoader::round_currency_ratio(ratio),
            expected,
            "RoundCurrencyRatio({ratio})"
        );
    }
}

// ---------------------------------------------------------------------------
// Snapshot parsing / serialization

const V2_RATES: &str = include_str!("fixtures/frankfurter_v2_rates.json");
const V2_CURRENCIES: &str = include_str!("fixtures/frankfurter_v2_currencies.json");
const V1_LATEST: &str = include_str!("fixtures/frankfurter_v1_latest.json");
const V1_CURRENCIES: &str = include_str!("fixtures/frankfurter_v1_currencies.json");

#[test]
fn parses_frankfurter_v2() {
    let snapshot =
        parse_frankfurter_v2(V2_RATES, Some(V2_CURRENCIES), fixture_time(), "test").unwrap();
    assert_eq!(snapshot.base, "USD");
    assert_eq!(snapshot.rates_date, "2026-10-02");
    assert_eq!(snapshot.currencies.len(), 165);
    let usd = snapshot.rate("USD").unwrap();
    assert_eq!(
        (usd.rate, usd.name.as_str(), usd.symbol.as_deref()),
        (1.0, "United States Dollar", Some("$"))
    );
    let eur = snapshot.rate("EUR").unwrap();
    assert_eq!((eur.rate, eur.symbol.as_deref()), (0.88356, Some("€")));
    // A currency without a symbol.
    assert_eq!(snapshot.rate("CMD").unwrap().symbol, None);
    // Every published code has metadata or is deliberately excluded.
    for rate in &snapshot.currencies {
        assert!(
            info::currency_info(&rate.code).is_some() || info::is_excluded(&rate.code),
            "{} ({}) is missing from the currency table",
            rate.code,
            rate.name
        );
    }

    // Without the currencies document names default to the codes.
    let bare = parse_frankfurter_v2(V2_RATES, None, fixture_time(), "test").unwrap();
    assert_eq!(bare.rate("EUR").unwrap().name, "EUR");
}

#[test]
fn bundled_snapshot_matches_the_raw_responses() {
    let parsed =
        parse_frankfurter_v2(V2_RATES, Some(V2_CURRENCIES), fixture_time(), "test").unwrap();
    let bundled = CurrencySnapshot::bundled();
    assert_eq!(bundled.currencies, parsed.currencies);
    assert_eq!(bundled.rates_date, parsed.rates_date);
    assert_eq!(bundled.source, "https://api.frankfurter.dev/v2");
}

#[test]
fn parses_frankfurter_v1() {
    let snapshot =
        parse_frankfurter_v1(V1_LATEST, Some(V1_CURRENCIES), fixture_time(), "test").unwrap();
    assert_eq!(snapshot.base, "USD");
    assert_eq!(snapshot.rates_date, "2026-10-01");
    // 29 ECB rates plus the base currency.
    assert_eq!(snapshot.currencies.len(), 30);
    assert_eq!(snapshot.rate("USD").unwrap().rate, 1.0);
    assert_eq!(snapshot.rate("EUR").unwrap().rate, 0.88511);
    assert_eq!(snapshot.rate("EUR").unwrap().name, "Euro");

    // A loader built from it gets names and symbols from the table.
    let mut loader = CurrencyDataLoader::default();
    assert!(loader.load_snapshot(snapshot, CurrencyDataSource::Web));
    assert_eq!(loader.currency_unit_by_code("EUR").unwrap().symbol, "€");

    // Amounts other than 1 are normalized.
    let doubled = r#"{"amount":2.0,"base":"EUR","date":"2026-10-01","rates":{"USD":2.4}}"#;
    let snapshot = parse_frankfurter_v1(doubled, None, fixture_time(), "test").unwrap();
    assert_eq!(snapshot.rate("USD").unwrap().rate, 1.2);
    assert_eq!(snapshot.rate("EUR").unwrap().rate, 1.0);
}

#[test]
fn rejects_bad_responses() {
    assert!(parse_frankfurter_v2("not json", None, Utc::now(), "test").is_err());
    assert!(parse_frankfurter_v2("[]", None, Utc::now(), "test").is_err());
    assert!(
        parse_frankfurter_v2(
            r#"{"status":422,"message":"invalid currency: ABC"}"#,
            None,
            Utc::now(),
            "t"
        )
        .is_err()
    );
    assert!(parse_frankfurter_v1("[]", None, Utc::now(), "test").is_err());
    assert!(
        parse_frankfurter_v1(
            r#"{"base":"USD","date":"2026-10-01","rates":{"EUR":0}}"#,
            None,
            Utc::now(),
            "t"
        )
        .is_ok_and(|s| s.rate("USD").is_some())
    );
}

#[test]
fn cache_round_trip() {
    let path = temp_cache_path("roundtrip");
    let snapshot = fixture_snapshot();
    save_cache(&path, &snapshot).unwrap();
    assert_eq!(load_cache(&path).unwrap(), snapshot);
    // Overwrite.
    let newer = snapshot_at(fixture_time() + TimeDelta::days(1));
    save_cache(&path, &newer).unwrap();
    assert_eq!(load_cache(&path).unwrap(), newer);
    assert!(matches!(
        load_cache(&temp_cache_path("nope")),
        Err(CurrencyError::Io(_))
    ));
    // No temporary files are left behind.
    let dir = path.parent().unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn oversized_cache_is_refused() {
    let path = temp_cache_path("huge");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let padding = " ".repeat(unitconv::currency::MAX_CACHE_BYTES as usize);
    std::fs::write(&path, format!("{}{padding}", fixture_snapshot().to_json())).unwrap();
    assert!(matches!(load_cache(&path), Err(CurrencyError::Parse(_))));
}

#[test]
fn concurrent_cache_writers_never_corrupt_the_file() {
    let path = temp_cache_path("concurrent");
    let snapshots: Vec<_> = (0..4)
        .map(|d| snapshot_at(fixture_time() + TimeDelta::days(d)))
        .collect();
    std::thread::scope(|s| {
        for snap in &snapshots {
            let path = &path;
            s.spawn(move || {
                for _ in 0..25 {
                    save_cache(path, snap).unwrap();
                }
            });
        }
    });
    let loaded = load_cache(&path).unwrap();
    assert!(snapshots.contains(&loaded));
}

#[cfg(feature = "network")]
#[test]
#[ignore = "requires network access"]
fn live_fetch_latest() {
    let snapshot = unitconv::currency::fetch_latest().expect("fetch from api.frankfurter.dev");
    assert_eq!(snapshot.base, "USD");
    assert!(!snapshot.rates_date.is_empty());
    assert!(snapshot.rate("EUR").is_some_and(|r| r.rate > 0.0));
    assert!(snapshot.rate("JPY").is_some_and(|r| r.rate > 0.0));
    assert!((Utc::now() - snapshot.fetched_at).num_minutes().abs() < 5);
    let mut loader = CurrencyDataLoader::default();
    assert!(loader.try_load_data_from_web(Ok(snapshot)));
    println!("{} currencies", loader.currency_units().len());

    let ecb = unitconv::currency::fetch_latest_with(&unitconv::currency::FetchConfig {
        providers: Some("ecb".into()),
        ..Default::default()
    })
    .expect("ECB-only fetch");
    assert!(ecb.rate("EUR").is_some());
}
