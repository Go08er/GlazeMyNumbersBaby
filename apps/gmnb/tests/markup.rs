//! A theme's own words (its name, its options' group names, what its
//! errors say) are shown as text: never read as Pango markup, where a
//! theme named `<span size="99999999">W</span>` once made a toast whose
//! glyph was too big to draw (a crash under the software renderer) and
//! one named "R&B" an empty toast.
//!
//! It needs a display: `GMNB_TEST_WAYLAND_DISPLAY` (or
//! `DGMNB_TEST_WAYLAND_DISPLAY`, which the workspace's tests are run
//! with) names a compositor (a headless weston's socket); without one the
//! test is skipped. Each run has a profile and a runtime folder of its
//! own, no session bus and no sound (`GMNB_SOUND=null`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

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

/// A folder of one run's own: its profile, its runtime folder, its theme.
struct Profile(PathBuf);

impl Profile {
    fn new(name: &str, theme: &str) -> Profile {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gmnb-markup-{}-{name}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        for sub in ["config", "data", "cache", "state", "run", "theme"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        std::fs::write(dir.join("theme/theme.luau"), theme).unwrap();
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

/// Runs GMNB with `theme` (`GMNB_THEME_DIR`) and `env` until it closes,
/// or for `ms` milliseconds (then it is stopped): whether it closed by
/// itself and well, and what it wrote to stderr.
fn run(display: &Path, theme: &str, env: &[(&str, &str)], ms: u64) -> (Option<bool>, String) {
    let profile = Profile::new("run", theme);
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
        // A headless compositor never says which window is in use.
        .env("GMNB_ACTIVE", "1")
        .env("GMNB_THEME_DIR", profile.path("theme"));
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("GMNB started");
    let mut err = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut err, &mut s);
        s
    });
    let deadline = Instant::now() + Duration::from_millis(ms);
    let closed = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status.success());
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    (closed, reader.join().unwrap())
}

/// What GTK, Pango and GDK say when text was taken for markup, or when
/// markup asked for more than they can draw.
fn markup_trouble(err: &str) -> Vec<&str> {
    err.lines()
        .filter(|l| {
            l.contains("from markup")
                || l.contains("Pango-WARNING")
                || l.contains("Gdk-CRITICAL")
                || l.contains("exceeds AdwBreakpointBin")
        })
        .collect()
}

/// A theme whose name is markup, and whose background fails to draw on
/// every press: after the third, the toast that says it was switched off
/// names it, as text.
#[test]
fn a_themes_name_in_a_toast_is_text() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    const THEME: &str = r#"
return {
	api = 1,
	name = '<span size="99999999">W</span>',
	layers = { background = { draw = function() error("no", 0) end, events = { press = 1 } } },
}"#;
    let steps = "keys:1;keys:2;keys:3;keys:4;wait:1500;quit";
    let (closed, err) = run(&display, THEME, &[("GMNB_STEPS", steps)], 30_000);
    assert!(
        err.contains("its background drawing failed and was switched off"),
        "the toast was never asked for: {err}"
    );
    assert_eq!(closed, Some(true), "GMNB did not close well: {err}");
    let trouble = markup_trouble(&err);
    assert!(trouble.is_empty(), "{}", trouble.join("\n"));
}

/// A theme whose name, error and option groups have `&` and `<` in them:
/// Settings shows its options' groups as they are named (one was lost to
/// a parse error, and one asked the dialog for 122,731 pixels).
#[test]
fn a_themes_words_in_settings_are_text() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    const THEME: &str = r#"
return {
	api = 1,
	name = "R&B <3",
	options = {
		{ id = "a", type = "toggle", label = "A", group = "Salt & pepper", default = true },
		{ id = "b", type = "toggle", label = "B", group = '<span size="99999999">W</span>' },
	},
	style = function(ctx) error("x < y", 0) end,
}"#;
    let (_, err) = run(&display, THEME, &[("GMNB_PREFS", "1")], 4_000);
    assert!(
        err.contains("has a problem"),
        "the theme's style never ran: {err}"
    );
    let trouble = markup_trouble(&err);
    assert!(trouble.is_empty(), "{}", trouble.join("\n"));
}
