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
/// shows ("the last style that worked, or the base look"). Chosen after
/// another, the style of the one before stood under the new theme's name;
/// at start, no style was ever written, and the window was bare (no keys,
/// no header: the stylesheet's properties unset). GMNB in Glazed, then
/// the theme chosen (`GMNB_STEPS`' `theme:`), is pixel for pixel GMNB
/// started in that theme (both pictured once the toast that says so is
/// gone), and that is the base look's picture.
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
    let err = shot(&started, &[("GMNB_THEME", "dev")], "7500");
    assert!(err.contains("has a problem"), "{err}");
    let err = shot(
        &switched,
        &[("GMNB_THEME", "glazed"), ("GMNB_STEPS", "theme:dev")],
        "7500",
    );
    assert!(err.contains("has a problem"), "{err}");
    let base = shots.join("base.png");
    shot(&base, &[("GMNB_THEME", "base")], "7500");
    let read = |path: &Path| std::fs::read(path).expect("a picture");
    let (a, b, c) = (read(&started), read(&switched), read(&base));
    let _ = std::fs::remove_dir_all(&shots);
    assert!(
        a == c,
        "started in the broken theme, GMNB isn't the base look"
    );
    assert!(
        b == c,
        "chosen after Glazed, the broken theme isn't the base look"
    );
}

/// A theme that doesn't load when GMNB starts: "the base look, and a
/// toast" (docs/theming.md). The hub learns it before the window has said
/// how it shows a toast, and the toast was never shown: the picture was
/// the base look's, unexplained.
#[test]
fn a_theme_that_fails_at_start_says_so() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    const THEME: &str = "return { api = 1, name = \"Unfinished\"";
    let shots = std::env::temp_dir().join(format!("gmnb-start-shots-{}", std::process::id()));
    std::fs::create_dir_all(&shots).unwrap();
    let (failed, base) = (shots.join("failed.png"), shots.join("base.png"));
    let shot = |path: &Path, theme: &str| {
        let env = [
            ("GMNB_STILL", "1"),
            ("GMNB_THEME", theme),
            ("GMNB_SCREENSHOT", path.to_str().unwrap()),
            ("GMNB_SCREENSHOT_DELAY_MS", "1500"),
        ];
        run(&display, THEME, &env, 30_000)
    };
    let err = shot(&failed, "dev");
    assert!(err.contains("couldn't be loaded"), "{err}");
    shot(&base, "base");
    let (a, b) = (std::fs::read(&failed), std::fs::read(&base));
    let _ = std::fs::remove_dir_all(&shots);
    let (a, b) = (a.expect("a picture"), b.expect("a picture"));
    assert!(a != b, "no toast: the picture is the base look's");
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
    assert!(
        err.contains("Settings built in"),
        "Settings never opened: {err}"
    );
    let tiles = err.lines().filter(|l| l.contains(": tile ")).count();
    // 384 tiles; twice that if `ctx` changed meanwhile.
    assert!(
        (384..=768).contains(&tiles),
        "{tiles} previews to open Settings"
    );
}

/// A theme with as many options and tiles as it may have (24 options of
/// 16 choices), each `visible` and each `preview` spinning 6 ms (on
/// `os.clock`, Luau's monotonic clock): 2.4 s of the theme's code in all. Settings used to make every call
/// before it was shown, holding the window for all of it (3.8 s with
/// hooks just under their 10 ms), and as long again on every change of
/// `ctx`. Now its calls share 10 ms a pass, and what doesn't fit waits
/// for a pass of its own: Settings is up after one pass, no pass is much
/// longer than its budget and one call, and every call is made in the end,
/// none of them failed or noted.
#[test]
fn settings_never_holds_the_window_for_a_slow_theme() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    let err = run(
        &display,
        SLOW,
        &[
            ("GMNB_PREFS", "1"),
            ("GMNB_THEME_STATS", "1"),
            ("GMNB_STEPS_AFTER_MS", "20000"),
            ("GMNB_STEPS", "quit"),
        ],
        60_000,
    );
    // The processor time Settings took to show itself.
    let built: f64 = err
        .lines()
        .find_map(|l| {
            l.split_once("Settings built in ")?
                .1
                .split_once("(")?
                .1
                .split_once(" ms")?
                .0
                .parse()
                .ok()
        })
        .unwrap_or_else(|| panic!("Settings never opened: {err}"));
    assert!(
        built < 600.0,
        "Settings took {built} ms of processor time to open"
    );
    // Everything asked and answered: 24 rows' `visible` and 384 tiles.
    let line = err
        .lines()
        .find(|l| l.contains("Settings has its answers"))
        .unwrap_or_else(|| panic!("Settings never had its answers: {err}"));
    let numbers: Vec<f64> = line
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter_map(|n| n.parse().ok())
        .collect();
    let [calls, passes, longest] = numbers[..] else {
        panic!("{line}");
    };
    assert!(calls >= 408.0, "{line}");
    // Many passes (about 250 on an idle machine; a spin that loses the
    // processor costs its pass less of its budget, and more fit).
    assert!(passes > 10.0, "{line}");
    // A pass's budget, the one call that may start just before it runs
    // out, and the app's own work between calls.
    assert!(longest <= 22.0, "{line}");
    let tiles = err
        .lines()
        .filter(|l| l.starts_with("gmnb theme dev: tile "))
        .count();
    assert!(tiles >= 384, "{tiles} tiles drawn");
    let notes: Vec<&str> = err
        .lines()
        .filter(|l| l.starts_with("note: ") && !l.starts_with("note: print: "))
        .collect();
    assert!(notes.is_empty(), "the slow hooks failed: {notes:?}");
}

/// One change of an option costs one pass: whether each row is shown is
/// asked in the pass that draws the tiles again for the style the change
/// made (it was a pass of its own after it, a second budget in the same
/// turn of the main loop).
#[test]
fn an_option_changed_costs_one_pass() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    const THEME: &str = r#"
local options = {
	{ id = "level", type = "slider", label = "Level", min = 0, max = 1, step = 0.1, default = 0.5 },
	{ id = "pick", type = "choice", label = "Pick", choices = { { id = "a" }, { id = "b" } },
	  preview = function(draw, frame, choice) end },
}
for i = 1, 3 do
	options[#options + 1] = { id = "v" .. i, type = "toggle", label = "V" .. i, default = true,
		visible = function(o, ctx) return true end }
end
return { api = 1, name = "Changes", options = options, style = function(ctx)
	return { shape = { radius = math.floor(ctx.options.level * 10) } }
end }"#;
    let err = run(
        &display,
        THEME,
        &[
            ("GMNB_THEME_STATS", "1"),
            ("GMNB_STEPS_AFTER_MS", "500"),
            (
                "GMNB_STEPS",
                "settings;wait:800;slide:wc-option-slider:0.8;wait:800;quit",
            ),
        ],
        30_000,
    );
    let (_, after) = err
        .split_once("GMNB_STEPS: sliding")
        .unwrap_or_else(|| panic!("the slider never moved: {err}"));
    let answered: Vec<&str> = after
        .lines()
        .filter(|l| l.contains("Settings has its answers"))
        .collect();
    assert_eq!(answered.len(), 1, "{answered:?}");
    assert!(after.contains("sound:"), "the steps did not finish: {err}");
}

/// Settings closed while it still has calls to make: none of them is
/// made once it has closed (nothing of Settings runs while it is closed).
#[test]
fn settings_closed_asks_nothing_more() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    let err = run(
        &display,
        SLOW,
        &[
            ("GMNB_STEPS_AFTER_MS", "500"),
            ("GMNB_STEPS", "settings;wait:400;close;wait:1500;quit"),
        ],
        30_000,
    );
    let (before, after) = err
        .split_once("GMNB_STEPS: closed")
        .unwrap_or_else(|| panic!("Settings was never closed: {err}"));
    let asked = before
        .lines()
        .filter(|l| l.starts_with("gmnb theme dev: tile "))
        .count();
    // (Some of the 384 were still to draw when it closed.)
    assert!(asked < 384, "{asked} tiles drawn before Settings closed");
    let late: Vec<&str> = after
        .lines()
        .filter(|l| l.starts_with("gmnb theme dev: tile "))
        .collect();
    assert!(
        late.is_empty(),
        "{} tiles drawn after Settings closed",
        late.len()
    );
    assert!(after.contains("sound:"), "the steps did not finish: {err}");
}

/// The Background opacity dragged faster than the screen shows it (nine
/// steps at once, as the toolkit reports a quick drag): the window, and
/// the theme's `style`, follow once a frame, at the last of them. It
/// called `style` for every step ("at most once per frame" was left to
/// the toolkit's own coalescing of the pointer's motion).
#[test]
fn the_opacity_follows_a_drag_once_a_frame() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    const THEME: &str = r#"
return { api = 1, name = "Counted", style = function(ctx)
	print(string.format("style at %.2f", ctx.opacity))
	return {}
end }"#;
    let err = run(
        &display,
        THEME,
        &[
            ("GMNB_ACTIVE", "1"),
            ("GMNB_STEPS_AFTER_MS", "500"),
            (
                "GMNB_STEPS",
                "settings;wait:800;slide:wc-opacity-slider:0.9,0.8,0.7,0.6,0.5,0.4,0.3,0.2,0.15;wait:800;quit",
            ),
        ],
        30_000,
    );
    let (_, after) = err
        .split_once("GMNB_STEPS: sliding")
        .unwrap_or_else(|| panic!("the slider never moved: {err}"));
    // Nine steps, one frame: one `style` call (there were nine).
    let styles: Vec<&str> = after
        .lines()
        .filter(|l| l.starts_with("gmnb theme dev: style at"))
        .collect();
    assert_eq!(styles.len(), 1, "{styles:?}");
    assert!(styles[0].ends_with("style at 0.15"), "{styles:?}");
    assert!(after.contains("sound:"), "the steps did not finish: {err}");
}

/// Settings in a new window (360 × 600), in the base look and in each
/// built-in theme, is no wider than the window. libadwaita's preferences
/// dialog asked for 360 px whatever its rows needed, and its bottom sheet
/// 12 more: "AdwBottomSheet exceeds AdwBreakpointBin width: requested
/// 372 px, 360 px available", the sheet cut off at the window's edge.
#[test]
fn settings_fits_a_new_window() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    fits(&display, "360x600");
}

/// Settings in the least window (300 × 400) too. It doesn't yet: the
/// Theme group's two buttons, side by side, need 269 px in the base look
/// and 302 in Aurora, where 264 are left. They are the themes-folder
/// work's (a wrapping box fits them).
#[test]
#[ignore = "the Theme group's buttons don't wrap yet"]
fn settings_fits_the_least_window() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    fits(&display, "300x400");
}

/// Settings opened at `size` in the base look and each built-in theme,
/// and nothing in it wider than the window.
fn fits(display: &Path, size: &str) {
    const THEME: &str = r#"return { api = 1, name = "Plain" }"#;
    let mut bad = Vec::new();
    for theme in ["base", "aurora", "mono", "glazed", "swirls"] {
        let env = [
            ("GMNB_THEME", theme),
            ("GMNB_PREFS", "1"),
            ("GMNB_SIZE", size),
        ];
        let err = run(display, THEME, &env, 3_500);
        if let Some(line) = err.lines().find(|l| l.contains("exceeds")) {
            bad.push(format!("{theme}: {line}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// The theme of the tests above that time Settings: 24 options of 16
/// choices, each `visible` and `preview` spinning 6 ms on `os.clock`.
/// Each tile it draws says so.
const SLOW: &str = r#"
local function spin(ms)
	local t = os.clock()
	while os.clock() - t < ms / 1000 do
	end
end
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
			spin(6)
			drawn += 1
			print("tile " .. drawn)
		end,
		visible = function(options, ctx)
			spin(6)
			return true
		end,
	}
end
return { api = 1, name = "Slow", options = options }"#;

/// Settings → Sound has its switch (the theme's sounds on or off: the
/// mute) and its volume slider, in the base look and with a theme that has
/// sounds. Each takes the keyboard's focus there (`GMNB_FOCUS`), which
/// says when nothing does, as it does for a class no widget has.
#[test]
fn settings_has_a_sound_switch_and_a_volume_slider() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    const THEME: &str = r#"return { api = 1, name = "Plain" }"#;
    let missing = |theme: &str, class: &str| {
        let env = [
            ("GMNB_THEME", theme),
            ("GMNB_PREFS", "1"),
            ("GMNB_FOCUS", class),
        ];
        run(&display, THEME, &env, 4_000)
            .contains(&format!("GMNB_FOCUS: nothing focusable with class {class}"))
    };
    assert!(
        missing("base", "wc-no-such-row"),
        "GMNB_FOCUS didn't say it found nothing for a class no widget has"
    );
    for theme in ["base", "glazed"] {
        for class in ["wc-sound-switch", "wc-sound-volume-slider"] {
            assert!(
                !missing(theme, class),
                "{theme}: Settings -> Sound has no {class}"
            );
        }
    }
}

/// Choosing another theme while Settings shows preview tiles: the tiles
/// of the theme that was are not asked of the one that is (its engine
/// refuses another's hooks), and that refusal is not a note of the new
/// theme's ("preview: a function from another theme engine").
#[test]
fn choosing_a_theme_leaves_no_stray_note_from_the_old_tiles() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    let env = [
        ("GMNB_THEME", "aurora"),
        ("GMNB_PREFS", "1"),
        ("GMNB_STEPS_AFTER_MS", "2500"),
        ("GMNB_STEPS", "wait:500;theme:glazed;wait:500;quit"),
    ];
    let err = run(&display, "return { api = 1, name = \"Dev\" }", &env, 15_000);
    assert!(err.contains("sound:"), "the steps did not run: {err}");
    let notes: Vec<&str> = err.lines().filter(|l| l.starts_with("note: ")).collect();
    assert!(notes.is_empty(), "stray notes after the switch: {notes:?}");
}
