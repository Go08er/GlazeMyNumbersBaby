//! Settings → Theme in the running app: what a theme's options cost when
//! Settings shows them. GMNB started on a private compositor with a theme
//! of the test's own, Settings opened (`GMNB_PREFS=1`).
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
            "gmnb-settings-{}-{name}-{}",
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

/// Runs GMNB with `theme` (`GMNB_THEME_DIR`) and `env` for `ms`
/// milliseconds, then stops it: what it wrote to stderr.
fn run(display: &Path, theme: &str, env: &[(&str, &str)], ms: u64) -> String {
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
    while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    reader.join().unwrap()
}

/// A theme whose `style` fails the first time is chosen: the base look
/// shows ("the last style that worked, or the base look"), not the style
/// of the theme chosen before it, which stood under the new theme's
/// name. GMNB in Glazed, then the theme chosen (`GMNB_STEPS`' `theme:`),
/// pictured once the toast that says so is gone, is pixel for pixel GMNB
/// started in that theme.
#[test]
fn a_theme_whose_style_fails_shows_the_base_look() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    const THEME: &str = r#"
return { api = 1, name = "Broken", style = function(ctx) error("no style", 0) end }"#;
    let shots = std::env::temp_dir().join(format!("gmnb-settings-shots-{}", std::process::id()));
    std::fs::create_dir_all(&shots).unwrap();
    let (started, switched) = (shots.join("started.png"), shots.join("switched.png"));
    let common = [("GMNB_STILL", "1"), ("GMNB_MODE", "standard")];
    let shot = |path: &Path, more: &[(&str, &str)], delay: &str| {
        let mut env: Vec<(&str, &str)> = common.to_vec();
        env.extend_from_slice(more);
        env.push(("GMNB_SCREENSHOT", path.to_str().unwrap()));
        env.push(("GMNB_SCREENSHOT_DELAY_MS", delay));
        run(&display, THEME, &env, 30_000)
    };
    let err = shot(&started, &[("GMNB_THEME", "dev")], "2000");
    assert!(err.contains("has a problem"), "{err}");
    let err = shot(
        &switched,
        &[("GMNB_THEME", "glazed"), ("GMNB_STEPS", "theme:dev")],
        "7500",
    );
    assert!(err.contains("has a problem"), "{err}");
    let (a, b) = (std::fs::read(&started), std::fs::read(&switched));
    let _ = std::fs::remove_dir_all(&shots);
    let (a, b) = (a.expect("a picture"), b.expect("a picture"));
    assert!(a == b, "chosen after Glazed, the broken theme isn't the base look");
}

/// As many options as a theme may have (24), each a choice of as many
/// choices as it may have (16), each with a tile: Settings draws each tile
/// once ("a tile's picture until `ctx` changes"). It drew the tiles of
/// every option again as each next one was added, and again for each
/// grid when the style changed: 5,184 previews to open Settings, where
/// previews just under their 10 ms each held the window for 40 s.
#[test]
fn settings_draws_each_tile_once() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    const THEME: &str = r#"
local drawn = 0
local options = {}
for i = 1, 24 do
	local choices = {}
	for j = 1, 16 do
		choices[j] = { id = "c" .. j }
	end
	options[i] = {
		id = "o" .. i, type = "choice", label = "O" .. i, choices = choices,
		preview = function(draw, frame, choice)
			drawn += 1
			print("tile " .. drawn)
		end,
	}
end
return { api = 1, name = "Tiles", options = options }"#;
    let err = run(
        &display,
        THEME,
        &[("GMNB_PREFS", "1"), ("GMNB_THEME_STATS", "1")],
        6_000,
    );
    assert!(err.contains("Settings built in"), "Settings never opened: {err}");
    let tiles = err.lines().filter(|l| l.contains(": tile ")).count();
    // 384 tiles; twice that if `ctx` changed meanwhile.
    assert!(
        (384..=768).contains(&tiles),
        "{tiles} previews to open Settings"
    );
}
