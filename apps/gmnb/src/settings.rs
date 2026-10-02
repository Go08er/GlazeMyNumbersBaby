//! GMNB's persisted preferences + light session state (see
//! `appcore::settings` for how they are stored).

use appcore::settings::{HasPages, PageStates};
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
            mode: "standard".into(),
            width: 380,
            height: 640,
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

pub type Store = appcore::settings::Store<Settings>;

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
