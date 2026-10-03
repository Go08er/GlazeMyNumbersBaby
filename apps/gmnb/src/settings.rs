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
    /// How opaque the window's backdrop is, 0.1..=1. Below 1 the desktop
    /// shows through (and the compositor's blur, if it blurs translucent
    /// windows).
    pub background_opacity: f64,
    /// Render with GTK's default GPU renderer (Vulkan, or GL where Vulkan
    /// isn't available); off renders in software. Read once at startup.
    pub vulkan: bool,
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
            background_opacity: 1.0,
            vulkan: true,
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

    #[test]
    fn older_settings_keep_an_opaque_gpu_window() {
        let s: Settings = serde_json::from_str(r#"{"theme":"dark","palette":"ember"}"#).unwrap();
        assert!(s.vulkan);
        assert_eq!(s.background_opacity, 1.0);
        assert_eq!(s.theme, "dark");
    }

    #[test]
    fn saved_opacity_is_kept_drawable() {
        assert_eq!(backdrop_alpha(0.4), 0.4);
        assert_eq!(backdrop_alpha(0.0), MIN_BACKGROUND_OPACITY as f32);
        assert_eq!(backdrop_alpha(7.0), 1.0);
        assert_eq!(backdrop_alpha(f64::NAN), 1.0);
    }
}
