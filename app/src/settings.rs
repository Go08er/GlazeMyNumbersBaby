//! Persisted preferences + light session state, stored as JSON in the XDG
//! config dir (inside the sandbox under Flatpak).

use std::cell::RefCell;
use std::path::PathBuf;

use gtk::glib;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "system" | "light" | "dark"
    pub theme: String,
    pub palette: String,
    /// Freestyle palette colours (hex).
    pub custom_primary: String,
    pub custom_secondary: String,
    pub animated_background: bool,
    pub mode: String,
    pub width: i32,
    pub height: i32,
    /// Opaque per-page state blobs (history, memory, graph equations, …).
    pub pages: serde_json::Map<String, serde_json::Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            palette: "aurora".into(),
            custom_primary: "#7c4dff".into(),
            custom_secondary: "#ff6fb5".into(),
            animated_background: true,
            mode: "standard".into(),
            width: 380,
            height: 640,
            pages: Default::default(),
        }
    }
}

pub struct Store {
    path: PathBuf,
    pub data: RefCell<Settings>,
}

impl Store {
    pub fn load() -> Store {
        let path = glib::user_config_dir()
            .join(crate::DATA_DIR)
            .join("settings.json");
        let data = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Store {
            path,
            data: RefCell::new(data),
        }
    }

    /// An in-memory store that never touches disk (screenshots, tests).
    pub fn ephemeral() -> Store {
        Store {
            path: PathBuf::new(),
            data: RefCell::new(Settings::default()),
        }
    }

    pub fn save(&self) {
        if self.path.as_os_str().is_empty() {
            return;
        }
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_vec_pretty(&*self.data.borrow()) {
            Ok(bytes) => {
                let tmp = self.path.with_extension("json.tmp");
                if std::fs::write(&tmp, bytes)
                    .and_then(|_| std::fs::rename(&tmp, &self.path))
                    .is_err()
                {
                    glib::g_warning!("gmnb", "could not save settings to {}", self.path.display());
                }
            }
            Err(e) => glib::g_warning!("gmnb", "could not serialise settings: {e}"),
        }
    }

    pub fn page_state(&self, key: &str) -> Option<serde_json::Value> {
        self.data.borrow().pages.get(key).cloned()
    }

    pub fn set_page_state(&self, key: &str, value: serde_json::Value) {
        self.data.borrow_mut().pages.insert(key.to_string(), value);
    }
}
