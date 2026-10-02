//! Converter session helpers.

use std::path::PathBuf;

use unitconv::{NetworkAccessBehavior, UnitConverterViewModel, ViewModelConfig};

/// A converter view model with the app's currency cache and saved
/// preferences (`pages.converter`).
pub fn view_model(app: &str, prefs: Option<serde_json::Value>) -> UnitConverterViewModel {
    let config = ViewModelConfig {
        currency_cache_path: Some(currency_cache_path(app)),
        preferences: prefs
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
        ..ViewModelConfig::default()
    };
    UnitConverterViewModel::new(config)
}

pub fn currency_cache_path(app: &str) -> PathBuf {
    crate::dirs::cache_dir(app).join("currency.json")
}

/// A connectivity report (from GIO or the network portal) → loader policy.
/// Metered connections only fetch when asked.
pub fn network_behavior(available: bool, metered: bool) -> NetworkAccessBehavior {
    if !available {
        NetworkAccessBehavior::Offline
    } else if metered {
        NetworkAccessBehavior::OptIn
    } else {
        NetworkAccessBehavior::Normal
    }
}
