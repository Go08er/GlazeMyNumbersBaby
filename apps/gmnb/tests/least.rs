//! The least window: GMNB started at 300 × 400 on a private compositor,
//! on every page, light and dark, in the base look and in each built-in
//! theme, is still 300 × 400 when it has laid itself out (docs/theming.md,
//! open point 2: a new window may shrink to 300 × 400, whatever the
//! theme). What fits is only the window's size; how it reads at that
//! size is for the eye.
//!
//! It needs a display: `GMNB_TEST_WAYLAND_DISPLAY` (or
//! `DGMNB_TEST_WAYLAND_DISPLAY`, which the workspace's tests are run
//! with) names a compositor (a headless weston's socket); without one the
//! test is skipped. Each run has a profile and a runtime folder of its
//! own, no session bus and no sound (`GMNB_SOUND=null`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

/// The compositor to run on, as an absolute path (each run has its own
/// runtime folder, which a socket's bare name would be looked for in).
fn display() -> Option<PathBuf> {
    let named = std::env::var_os("GMNB_TEST_WAYLAND_DISPLAY")
        .or_else(|| std::env::var_os("DGMNB_TEST_WAYLAND_DISPLAY"))?;
    let named = PathBuf::from(named);
    if named.is_absolute() {
        return Some(named);
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    Some(Path::new(&runtime).join(named))
}

/// A folder of one run's own: its profile and its runtime folder.
struct Profile(PathBuf);

impl Profile {
    fn new(name: &str) -> Profile {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gmnb-least-{}-{name}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        for sub in ["config", "data", "cache", "state", "run"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        Profile(dir)
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.0.join(sub)
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The pages, as `GMNB_MODE` and what else shows them.
const PAGES: [(&str, &[(&str, &str)]); 8] = [
    ("standard", &[]),
    ("scientific", &[]),
    ("programmer", &[]),
    ("programmer", &[("GMNB_BITS", "1")]),
    ("date", &[]),
    ("graphing", &[]),
    ("currency", &[]),
    ("length", &[]),
];

/// The size GMNB's window has once laid out, started at 300 × 400 with
/// `theme` on `page` (its screenshot hook says), and what it wrote.
fn least(
    display: &Path,
    theme: &str,
    dark: bool,
    mode: &str,
    more: &[(&str, &str)],
) -> (String, String) {
    let profile = Profile::new(theme);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gmnb"));
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy().to_string();
        if name.starts_with("GMNB_") || name.starts_with("PULSE_") {
            cmd.env_remove(&name);
        }
    }
    for name in [
        "DISPLAY",
        "WAYLAND_SOCKET",
        "DBUS_SESSION_BUS_ADDRESS",
        "AT_SPI_BUS_ADDRESS",
    ] {
        cmd.env_remove(name);
    }
    cmd.env("WAYLAND_DISPLAY", display)
        .env("XDG_RUNTIME_DIR", profile.path("run"))
        .env("XDG_CONFIG_HOME", profile.path("config"))
        .env("XDG_DATA_HOME", profile.path("data"))
        .env("XDG_CACHE_HOME", profile.path("cache"))
        .env("XDG_STATE_HOME", profile.path("state"))
        .env("GDK_BACKEND", "wayland")
        .env("GSK_RENDERER", "cairo")
        .env("GTK_A11Y", "none")
        .env("NO_AT_BRIDGE", "1")
        .env("GSETTINGS_BACKEND", "memory")
        .env("GMNB_SOUND", "null")
        .env("GMNB_STILL", "1")
        .env("GMNB_THEME", theme)
        .env("GMNB_DARK", if dark { "1" } else { "0" })
        .env("GMNB_MODE", mode)
        .env("GMNB_SIZE", "300x400")
        .env("GMNB_SCREENSHOT", profile.path("shot.png"))
        .env("GMNB_SCREENSHOT_DELAY_MS", "1000");
    for (k, v) in more {
        cmd.env(k, v);
    }
    let out = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("GMNB ran");
    let err = String::from_utf8_lossy(&out.stderr).to_string();
    // "screenshot: <path> (300x400)"
    let size = err
        .lines()
        .find_map(|l| l.strip_prefix("screenshot: "))
        .and_then(|l| l.rsplit_once(" ("))
        .map(|(_, s)| s.trim_end_matches(')').to_string())
        .unwrap_or_default();
    (size, err)
}

fn every_page_fits(theme: &str) {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    let mut bad = Vec::new();
    for dark in [false, true] {
        for (mode, more) in PAGES {
            let (size, err) = least(&display, theme, dark, mode, more);
            if size != "300x400" {
                let what = more
                    .iter()
                    .map(|(k, v)| format!(" {k}={v}"))
                    .collect::<String>();
                let said = if size.is_empty() {
                    err.trim().to_string()
                } else {
                    size
                };
                bad.push(format!("{theme} {mode}{what} dark={dark}: {said}"));
            }
        }
    }
    assert!(bad.is_empty(), "the least window grew:\n{}", bad.join("\n"));
}

/// The base look keeps DGMNB's least window on every page.
#[test]
fn the_base_look_keeps_the_least_window() {
    every_page_fits("base");
}

/// Glazed lets its keys shrink (short keys below 24 pixels: `keys.short`)
/// and keeps the least window on every page.
#[test]
fn glazed_keeps_the_least_window() {
    every_page_fits("glazed");
}

/// Aurora's keys and bits keep no least size, as the base's, and it keeps
/// the least window on every page. (The graph's function analysis, not a
/// page here, is 359 wide there: its header; the base's is 332.)
#[test]
fn aurora_keeps_the_least_window() {
    every_page_fits("aurora");
}

/// Swirls' keys and bits keep no least size, as the base's, and it keeps
/// the least window on every page.
#[test]
fn swirls_keeps_the_least_window() {
    every_page_fits("swirls");
}
