// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

//! Fetches the latest rates from the Frankfurter API and writes them in the
//! snapshot format used by the cache and the bundled offline data:
//!
//! ```sh
//! cargo run -p unitconv --example refresh_snapshot -- crates/unitconv/data/currency-snapshot-YYYY-MM-DD.json
//! ```
//!
//! To ship it as the bundled snapshot, point `BUNDLED_SNAPSHOT_JSON` and
//! `BUNDLED_SNAPSHOT_DATE` in `src/currency/snapshot.rs` at the new file. The
//! tests compare the bundled file with the raw API responses in
//! `tests/fixtures/frankfurter_v2_{rates,currencies}.json` (re-download those
//! with curl at the same time) and pin a few values derived from it in
//! `tests/currency.rs` and `tests/view_model.rs` (counts, the USD→EUR rate).

fn main() {
    let snapshot = match unitconv::currency::fetch_latest() {
        Ok(snapshot) => snapshot,
        Err(e) => {
            eprintln!("fetch failed: {e}");
            std::process::exit(1);
        }
    };
    let json = snapshot.to_json() + "\n";
    match std::env::args().nth(1) {
        Some(path) => {
            std::fs::write(&path, json).expect("write snapshot");
            eprintln!(
                "wrote {} rates dated {} from {} to {path}",
                snapshot.currencies.len(),
                snapshot.rates_date,
                snapshot.source
            );
        }
        None => print!("{json}"),
    }
}
