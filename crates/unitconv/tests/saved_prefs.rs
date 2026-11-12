//! Hostile saved converter preferences (`pages.converter`).

use unitconv::{ConverterPreferences, UnitConverterViewModel, ViewModelConfig};

fn with(unit_converter: &str) -> UnitConverterViewModel {
    UnitConverterViewModel::new(ViewModelConfig {
        preferences: ConverterPreferences {
            unit_converter: Some(unit_converter.into()),
            ..Default::default()
        },
        ..ViewModelConfig::default()
    })
}

/// R5-E-1: a saved category the converter doesn't have (a damaged or
/// hand-made `settings.json`) left it on a category that isn't in the
/// list, with one unit, and saved that again at every exit.
#[test]
fn a_saved_category_that_does_not_exist_is_ignored() {
    let ids: Vec<i32> = UnitConverterViewModel::new(ViewModelConfig::default())
        .categories()
        .iter()
        .map(|c| c.id)
        .collect();
    for bad in ["99999", "-5", "2147483648", "0", ""] {
        let vm = with(&format!("1;a;b;1;1;0;|1;a;b;1;1;0;|{bad};1;x;|"));
        let id = vm.current_category().map(|c| c.id).unwrap();
        assert!(
            ids.contains(&id),
            "category {id} from {bad:?} is not in the list"
        );
        assert!(vm.units().len() > 1, "{bad:?}");
    }
}
