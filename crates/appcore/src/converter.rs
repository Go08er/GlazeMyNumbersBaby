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

/// Paste clipboard text into the converter as Windows Calculator does:
/// validated for converter input first (`GetStringToPaste`), so anything but
/// a plain signed decimal within the length limits shows "Invalid input"
/// rather than being filtered into a different number ("1e3" is not 13).
pub fn paste(vm: &mut UnitConverterViewModel, text: &str) {
    // Converter validation depends only on the category group, so any
    // converter mode will do.
    let checked = copypaste::validate_paste_expression_with_group(
        text,
        copypaste::ViewMode::Length,
        copypaste::CategoryGroupType::Converter,
        copypaste::NumberBase::Unknown,
        copypaste::BitLength::BitLengthUnknown,
    );
    vm.paste(&checked);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pasted(text: &str) -> String {
        let mut vm = UnitConverterViewModel::new(ViewModelConfig::default());
        paste(&mut vm, text);
        vm.value1().to_string()
    }

    /// R6-M-02: as in Windows Calculator, converter paste takes a plain
    /// signed decimal or nothing; it never filters text into another number.
    #[test]
    fn converter_paste_is_validated() {
        assert_eq!(pasted("1e3"), "Invalid input");
        assert_eq!(pasted("1+2"), "Invalid input");
        assert_eq!(pasted("12abc"), "Invalid input");
        assert_eq!(pasted(&"9".repeat(200)), "Invalid input");
        assert_eq!(pasted("12.5"), "12.5");
        // Accepted; the default category can't go negative, so the sign is
        // dropped there, as in the original.
        assert_eq!(pasted("-3"), "3");
    }
}
