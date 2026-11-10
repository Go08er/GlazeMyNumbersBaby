//! GMNB's persisted preferences + light session state (see
//! `appcore::settings` for how they are stored).

use std::collections::BTreeMap;

use appcore::graph::NumberPrecision;
use appcore::settings::{HasPages, PageStates};
use gmnb_theme::OptionValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "system" | "light" | "dark"
    pub theme: String,
    /// 0.2's palette, Freestyle colours (hex) and "Living background":
    /// Aurora's options now. They are kept and written back unchanged, so
    /// that going back to 0.2 finds the palette it had; 0.3 reads them
    /// once, to make `theme_options.aurora` ([`Settings::migrate`]).
    pub palette: String,
    pub custom_primary: String,
    pub custom_secondary: String,
    pub animated_background: bool,
    /// The theme in use: a theme id, or "" for the base look. An id that
    /// isn't installed falls back to the base look and is kept, so the
    /// theme returns when its folder does.
    pub active_theme: String,
    /// What the user changed of each theme's options, by theme id and
    /// option id. Read entry by entry: one that doesn't fit its option
    /// (or isn't a table of options at all) is ignored, and options a
    /// theme no longer has are kept.
    pub theme_options: serde_json::Map<String, Value>,
    /// "Use the desktop's accent colour": off, themes are told of none.
    pub system_accent: bool,
    /// Settings → Sound → Theme sounds (docs/theming-media.md,
    /// "Settings"). The app's, not a theme's.
    pub sound: bool,
    /// The sounds' volume, 0 to 1; the gain is its square (0.5: -12 dB).
    pub sound_volume: f64,
    /// How opaque the window's backdrop is, 0.1..=1. Below 1 the desktop
    /// shows through (and the compositor's blur, if it blurs translucent
    /// windows).
    pub background_opacity: f64,
    /// Render with GTK's default GPU renderer (Vulkan, or GL where Vulkan
    /// isn't available); off renders in software. Read once at startup.
    pub vulkan: bool,
    /// Graphing: significant digits each typed number keeps ("Number
    /// precision"); older files load the default, 14.
    pub literal_digits: NumberPrecision,
    pub mode: String,
    pub width: i32,
    pub height: i32,
    /// Opaque per-page state blobs (history, memory, graph equations, …).
    pub pages: PageStates,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            palette: "aurora".into(),
            custom_primary: "#7c4dff".into(),
            custom_secondary: "#ff6fb5".into(),
            animated_background: true,
            active_theme: String::new(),
            theme_options: Default::default(),
            system_accent: true,
            sound: true,
            sound_volume: DEFAULT_SOUND_VOLUME,
            background_opacity: 1.0,
            vulkan: true,
            literal_digits: NumberPrecision::DEFAULT,
            mode: "standard".into(),
            width: appcore::metrics::window::DEFAULT_WIDTH,
            height: appcore::metrics::window::DEFAULT_HEIGHT,
            pages: Default::default(),
        }
    }
}

impl HasPages for Settings {
    fn pages(&self) -> &PageStates {
        &self.pages
    }
    fn pages_mut(&mut self) -> &mut PageStates {
        &mut self.pages
    }
}

/// 0.2's palettes: the values its `palette` setting took, and Aurora's
/// `palette` option takes.
pub const PALETTES: [&str; 7] = [
    "aurora", "ember", "abyss", "orchard", "graphite", "system", "custom",
];

/// The built-in theme that carries 0.2's look.
pub const AURORA: &str = "aurora";

impl Settings {
    /// Coming from 0.2: people start on the base look (`active_theme` is
    /// left unset), and Aurora is as they left it. Without a
    /// `theme_options.aurora`, 0.2's palette settings become Aurora's
    /// options, each only if it is one 0.2 could have saved. The old keys
    /// stay as they are; once `theme_options.aurora` exists they are
    /// never read again.
    pub fn migrate(&mut self) {
        if self.theme_options.get(AURORA).is_some_and(Value::is_object) {
            return;
        }
        let mut aurora = serde_json::Map::new();
        if PALETTES.contains(&self.palette.as_str()) {
            aurora.insert("palette".into(), Value::from(self.palette.as_str()));
        }
        for (old, new) in [
            (&self.custom_primary, "primary"),
            (&self.custom_secondary, "secondary"),
        ] {
            let colour = old.trim();
            if colour.starts_with('#')
                && let Some(c) = appcore::color::parse_hex(colour)
            {
                aurora.insert(new.into(), Value::from(appcore::color::to_hex(c)));
            }
        }
        aurora.insert("living".into(), Value::from(self.animated_background));
        self.theme_options
            .insert(AURORA.into(), Value::Object(aurora));
    }

    /// What is saved of `theme`'s options. Whether each fits its option is
    /// the theme's to say (`gmnb_theme::theme::complete_options`).
    pub fn options_of(&self, theme: &str) -> BTreeMap<String, OptionValue> {
        let Some(Value::Object(saved)) = self.theme_options.get(theme) else {
            return BTreeMap::new();
        };
        saved
            .iter()
            .filter_map(|(id, value)| {
                let value = match value {
                    Value::Bool(b) => OptionValue::Bool(*b),
                    Value::String(s) => OptionValue::Text(s.clone()),
                    Value::Number(n) => OptionValue::Number(n.as_f64()?),
                    _ => return None,
                };
                Some((id.clone(), value))
            })
            .collect()
    }

    /// Saves one option of `theme`.
    pub fn set_option(&mut self, theme: &str, id: &str, value: &OptionValue) {
        let entry = self
            .theme_options
            .entry(theme.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
        if !entry.is_object() {
            *entry = Value::Object(Default::default());
        }
        let value = match value {
            OptionValue::Bool(b) => Value::from(*b),
            OptionValue::Text(s) => Value::from(s.as_str()),
            OptionValue::Number(n) => Value::from(*n),
        };
        if let Value::Object(saved) = entry {
            saved.insert(id.to_string(), value);
        }
        self.fit_theme_options(theme, id);
    }

    /// Keeps `theme_options` within the budget of a preference
    /// (`appcore::settings::FIELD_BUDGET`): over it, a save leaves the
    /// whole field out, every theme's options and Aurora's too, which then
    /// come from 0.2's keys again. What leaves is what a theme no longer
    /// has, most likely (a theme whose option ids change piles them up):
    /// from the theme with the most saved, its largest option, and never
    /// the option just set (`theme`, `id`), unless nothing else is left.
    /// A theme left with none stays as an empty table (Aurora's means
    /// 0.2's keys were already read).
    fn fit_theme_options(&mut self, theme: &str, id: &str) {
        fn size<T: Serialize>(v: &T) -> usize {
            serde_json::to_vec(v).map_or(usize::MAX, |v| v.len())
        }
        while size(&self.theme_options) > appcore::settings::FIELD_BUDGET {
            // (theme, option): the option to drop, in the fullest theme.
            let drop = self
                .theme_options
                .iter()
                .filter_map(|(name, saved)| {
                    let Value::Object(saved) = saved else {
                        return Some((size(saved), name.clone(), None));
                    };
                    let largest = saved
                        .iter()
                        .filter(|(option, _)| !(name == theme && *option == id))
                        .max_by_key(|(option, v)| {
                            (option.len() + size(v), std::cmp::Reverse((*option).clone()))
                        })?;
                    Some((size(saved), name.clone(), Some(largest.0.clone())))
                })
                .max_by_key(|(total, name, _)| (*total, std::cmp::Reverse(name.clone())));
            match drop {
                Some((_, name, None)) => {
                    self.theme_options.remove(&name);
                }
                Some((_, name, Some(option))) => {
                    if let Some(Value::Object(saved)) = self.theme_options.get_mut(&name) {
                        saved.remove(&option);
                    }
                }
                None => {
                    if let Some(Value::Object(saved)) = self.theme_options.get_mut(theme) {
                        saved.remove(id);
                    }
                    break;
                }
            }
        }
    }
}

pub type Store = appcore::settings::Store<Settings>;

/// The saved settings, 0.2's brought forward ([`Settings::migrate`]).
pub fn load_store() -> Store {
    let store = Store::load(crate::DATA_DIR);
    store.data.borrow_mut().migrate();
    store
}

/// The lowest backdrop opacity offered (text must stay readable).
pub const MIN_BACKGROUND_OPACITY: f64 = 0.1;

/// A saved backdrop opacity as one to draw with.
pub fn backdrop_alpha(v: f64) -> f32 {
    if v.is_finite() {
        v.clamp(MIN_BACKGROUND_OPACITY, 1.0) as f32
    } else {
        1.0
    }
}

/// The sounds' volume a new profile has.
pub const DEFAULT_SOUND_VOLUME: f64 = 0.5;

/// A saved sound volume as one to play at: 0 to 1.
pub fn sound_volume(v: f64) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0) as f32
    } else {
        DEFAULT_SOUND_VOLUME as f32
    }
}

/// Whether the saved settings ask for GPU rendering, read before GTK starts
/// (the renderer is chosen when the first window is realised). Screenshot
/// runs use defaults, as they do for everything else.
pub fn wants_vulkan() -> bool {
    #[derive(Default, Serialize, Deserialize)]
    #[serde(default)]
    struct Early {
        vulkan: Option<bool>,
    }
    if std::env::var_os("GMNB_SCREENSHOT").is_some()
        && std::env::var_os("GMNB_REAL_STORE").is_none()
    {
        return true;
    }
    appcore::settings::Store::<Early>::load(crate::DATA_DIR)
        .data
        .into_inner()
        .vulkan
        .unwrap_or(true)
}

/// Save, logging (not failing) on error.
pub trait Persist {
    fn persist(&self);
}

impl Persist for Store {
    fn persist(&self) {
        if let Err(e) = self.save() {
            gtk::glib::g_warning!("gmnb", "{e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings file's text, loaded as GMNB loads it.
    fn loaded(name: &str, json: &str) -> Settings {
        let dir = std::env::temp_dir().join(format!("gmnb-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, json).unwrap();
        let store = Store::load_from(path);
        store.data.borrow_mut().migrate();
        let s = store.data.borrow().clone();
        let _ = std::fs::remove_dir_all(&dir);
        s
    }

    fn text(s: &str) -> OptionValue {
        OptionValue::Text(s.into())
    }

    /// Coming from 0.2 (docs/theming.md, "How options are saved"): the
    /// base look, Aurora as it was left, an opaque GPU window.
    #[test]
    fn older_settings_start_on_the_base_look_with_aurora_as_it_was() {
        let s = loaded("older", r#"{"theme":"dark","palette":"ember"}"#);
        assert!(s.vulkan);
        assert_eq!(s.background_opacity, 1.0);
        assert_eq!(s.theme, "dark");
        assert_eq!(s.literal_digits.digits(), Some(14));
        assert_eq!(s.active_theme, "");
        assert!(s.system_accent);
        assert_eq!(s.theme_options["aurora"]["palette"], "ember");
        assert_eq!(
            s.options_of("aurora"),
            BTreeMap::from([
                ("palette".to_string(), text("ember")),
                ("primary".to_string(), text("#7c4dff")),
                ("secondary".to_string(), text("#ff6fb5")),
                ("living".to_string(), OptionValue::Bool(true)),
            ])
        );
        // A new profile is the same but for the palette.
        let mut new = Settings::default();
        new.migrate();
        assert_eq!(new.active_theme, "");
        assert_eq!(new.options_of("aurora")["palette"], text("aurora"));
        assert_eq!((new.width, new.height), (360, 600));
    }

    /// 0.2's keys are kept and written back unchanged, so going back to
    /// 0.2 finds the palette it had; and they are read only once.
    #[test]
    fn the_old_keys_are_written_back_and_read_once() {
        let mut s = loaded(
            "kept",
            r##"{"palette":"custom","custom_primary":"#ABCDEF","custom_secondary":"#123456",
                "animated_background":false,"width":500,"height":700}"##,
        );
        assert_eq!(
            s.options_of("aurora"),
            BTreeMap::from([
                ("palette".to_string(), text("custom")),
                ("primary".to_string(), text("#abcdef")),
                ("secondary".to_string(), text("#123456")),
                ("living".to_string(), OptionValue::Bool(false)),
            ])
        );
        // Saved sizes are kept.
        assert_eq!((s.width, s.height), (500, 700));
        // 0.3 changes Aurora's options, not 0.2's keys.
        s.set_option("aurora", "palette", &text("abyss"));
        s.active_theme = "aurora".into();
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["palette"], "custom");
        assert_eq!(json["custom_primary"], "#ABCDEF");
        assert_eq!(json["custom_secondary"], "#123456");
        assert_eq!(json["animated_background"], false);
        assert_eq!(json["theme_options"]["aurora"]["palette"], "abyss");
        // Loaded again (by 0.3): the old keys are not read a second time.
        let again = loaded("again", &json.to_string());
        assert_eq!(again.options_of("aurora")["palette"], text("abyss"));
        assert_eq!(again.active_theme, "aurora");
        // Even an Aurora with nothing changed stands for "already moved".
        let s = loaded(
            "moved",
            r#"{"palette":"ember","theme_options":{"aurora":{}}}"#,
        );
        assert!(s.options_of("aurora").is_empty());
        assert_eq!(s.palette, "ember");
    }

    /// What 0.2 could not have saved is not brought forward.
    #[test]
    fn only_what_0_2_could_have_saved_is_migrated() {
        let s = loaded(
            "odd",
            r#"{"palette":"neon","custom_primary":"blue","custom_secondary":"7c4dff"}"#,
        );
        assert_eq!(
            s.options_of("aurora"),
            BTreeMap::from([("living".to_string(), OptionValue::Bool(true))])
        );
        assert_eq!(s.palette, "neon");
    }

    /// Each entry loads by itself: one mistyped entry costs only that
    /// entry.
    #[test]
    fn each_theme_entry_loads_by_itself() {
        let s = loaded(
            "entries",
            r##"{"theme":"light","active_theme":7,"system_accent":"no","palette":"abyss",
                "theme_options":{"aurora":5,
                                 "mono":{"accent":"#00ff00","square":"yes","extra":[1],"n":0.25},
                                 "gone":{"kept":true}}}"##,
        );
        assert_eq!(s.theme, "light");
        // Mistyped: their defaults.
        assert_eq!(s.active_theme, "");
        assert!(s.system_accent);
        // A mistyped Aurora entry is made again from 0.2's keys.
        assert_eq!(s.options_of("aurora")["palette"], text("abyss"));
        // Values of a type no option takes are left out; the rest stand
        // (whether they fit their option is the theme's to say).
        assert_eq!(
            s.options_of("mono"),
            BTreeMap::from([
                ("accent".to_string(), text("#00ff00")),
                ("square".to_string(), text("yes")),
                ("n".to_string(), OptionValue::Number(0.25)),
            ])
        );
        // Options of a theme that is gone are kept.
        assert_eq!(s.theme_options["gone"]["kept"], true);
        assert!(s.options_of("missing").is_empty());

        // `theme_options` itself mistyped: only it is lost.
        let s = loaded(
            "whole",
            r#"{"theme":"dark","theme_options":"x","active_theme":"mono","system_accent":false}"#,
        );
        assert_eq!(s.theme, "dark");
        assert_eq!(s.active_theme, "mono");
        assert!(!s.system_accent);
        assert_eq!(s.options_of("aurora")["palette"], text("aurora"));
    }

    /// `"vulkan": "false"` (a string) used to fail the whole file, and the
    /// defaults saved over it lost the history, memory and equations too.
    #[test]
    fn a_mistyped_vulkan_setting_keeps_everything_else() {
        let dir = std::env::temp_dir().join(format!("gmnb-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{"theme": "dark", "vulkan": "false", "background_opacity": 0.4,
                "mode": "graphing", "pages": {"calculator": {"m": 1, "x": {"mem": ["7"]}},
                "graphing": [{"text": "sin(x)"}]}}"#,
        )
        .unwrap();

        let store = Store::load_from(path.clone());
        let s = store.data.borrow();
        assert!(s.vulkan, "the mistyped setting takes its default");
        assert_eq!(s.theme, "dark");
        assert_eq!(s.background_opacity, 0.4);
        assert_eq!(s.mode, "graphing");
        assert_eq!(s.pages["calculator"]["x"]["mem"][0], "7");
        assert_eq!(
            appcore::graph::restore(s.pages.get("graphing").cloned())[0].text,
            "sin(x)"
        );
        assert!(dir.join("settings.json.bad").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Sound: on at 50% in a new profile and in a file from before it;
    /// each key loads by itself, and a volume out of range plays within
    /// it.
    #[test]
    fn the_sound_settings_load_by_themselves() {
        let s = loaded("sound-old", r#"{"theme":"dark","background_opacity":0.4}"#);
        assert!(s.sound);
        assert_eq!(s.sound_volume, 0.5);
        assert_eq!(s.background_opacity, 0.4);
        let new = Settings::default();
        assert!(new.sound && new.sound_volume == 0.5);

        let s = loaded("sound-set", r#"{"sound":false,"sound_volume":0.25}"#);
        assert!(!s.sound);
        assert_eq!(s.sound_volume, 0.25);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(
            (json["sound"].clone(), json["sound_volume"].clone()),
            (false.into(), 0.25.into())
        );

        // Mistyped: that key's default, the rest kept.
        let s = loaded(
            "sound-odd",
            r#"{"sound":"no","sound_volume":0.75,"theme":"light"}"#,
        );
        assert!(s.sound);
        assert_eq!((s.sound_volume, s.theme.as_str()), (0.75, "light"));
        let s = loaded("sound-odd2", r#"{"sound":false,"sound_volume":"loud"}"#);
        assert!(!s.sound);
        assert_eq!(s.sound_volume, 0.5);

        assert_eq!(sound_volume(0.3), 0.3);
        assert_eq!(sound_volume(-1.0), 0.0);
        assert_eq!(sound_volume(7.0), 1.0);
        assert_eq!(sound_volume(f64::NAN), 0.5);
    }

    #[test]
    fn saved_opacity_is_kept_drawable() {
        assert_eq!(backdrop_alpha(0.4), 0.4);
        assert_eq!(backdrop_alpha(0.0), MIN_BACKGROUND_OPACITY as f32);
        assert_eq!(backdrop_alpha(7.0), 1.0);
        assert_eq!(backdrop_alpha(f64::NAN), 1.0);
    }

    /// `theme_options` is one preference under the settings store's
    /// budget: over it, a save left the whole field out, every theme's
    /// options with it, and Aurora came from 0.2's keys again (its
    /// palette back as 0.2 had it). Options of themes that change their
    /// option ids (here: made from the time) pile up in it.
    #[test]
    fn theme_options_stay_within_the_budget_of_a_preference() {
        let mut s = Settings::default();
        s.migrate();
        s.palette = "ember".into(); // 0.2's key, to be ignored once moved
        s.set_option("aurora", "palette", &text("abyss"));
        for n in 0..400 {
            let id = format!("option-{n}-{}", "x".repeat(60));
            s.set_option("churn", &id, &text("v"));
        }
        s.active_theme = "aurora".into();
        let dir = std::env::temp_dir().join(format!("gmnb-budget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let store = Store::load_from(path.clone());
        *store.data.borrow_mut() = s;
        let saved = store.save();
        let again = Store::load_from(path);
        again.data.borrow_mut().migrate();
        let again = again.data.borrow().clone();
        let _ = std::fs::remove_dir_all(&dir);
        saved.expect("a save within every budget");
        assert_eq!(again.options_of("aurora")["palette"], text("abyss"));
        assert_eq!(again.active_theme, "aurora");
    }

    /// The option just set is kept while it can be, even when it is large.
    #[test]
    fn the_option_just_set_is_kept_while_it_fits() {
        let mut s = Settings::default();
        s.set_option("a", "old", &text(&"o".repeat(9_000)));
        s.set_option("b", "old", &text(&"o".repeat(9_000)));
        assert!(s.options_of("a").is_empty(), "the fuller theme's went");
        assert_eq!(s.options_of("b").len(), 1);
        s.set_option("b", "new", &text(&"n".repeat(9_000)));
        assert!(s.options_of("b").contains_key("new"));
        assert!(!s.options_of("b").contains_key("old"));
        s.set_option("b", "huge", &text(&"h".repeat(40_000)));
        assert!(s.options_of("b").is_empty(), "too big for the field");
    }
}
