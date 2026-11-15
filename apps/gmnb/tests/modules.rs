//! A theme's WebAssembly modules in the running app (docs/theming.md,
//! "Modules"): a theme with modules saved is shown at start with them
//! ready (the window waits); one chosen in Settings switches in once they
//! have compiled, and no hook of it runs before; Settings' list compiles
//! nothing, and the base look makes no engine; one of the chosen theme's
//! modules rebuilt while Settings is open reloads it. GMNB is started on a
//! private compositor with a theme of the test's own in its themes
//! folder, driven by `GMNB_STEPS`.
//!
//! It needs a display: `GMNB_TEST_WAYLAND_DISPLAY` (or
//! `DGMNB_TEST_WAYLAND_DISPLAY`, which the workspace's tests are run
//! with) names a compositor (a headless weston's socket); without one the
//! test is skipped. Each run has a profile and a runtime folder of its
//! own, no session bus and no sound (`GMNB_SOUND=null`).

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
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

macro_rules! display_or_skip {
    () => {
        match display() {
            Some(d) => d,
            None => {
                eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
                return;
            }
        }
    };
}

/// `bump()` adds `step` to the number at 0 of its memory and returns it.
fn module(step: i32) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module (memory (export "memory") 1)
             (func (export "bump") (result i32)
               (i32.store (i32.const 0) (i32.add (i32.load (i32.const 0)) (i32.const {step})))
               (i32.load (i32.const 0))))"#
    ))
    .unwrap()
}

/// A theme whose `style` and background `draw` call its module, and say
/// what it answered.
const THEME: &str = r#"
local paint = gmnb.module("modules/paint.wasm")
return {
    api = 1,
    name = "Modules",
    style = function()
        print("style bump " .. tostring(paint:call("bump")))
        return {}
    end,
    layers = { background = { draw = function()
        print("draw bump " .. tostring(paint:call("bump")))
        return nil
    end } },
}
"#;

/// A profile of one run's own, with the theme `modules` in its themes
/// folder (its module in a folder of its own).
struct Profile(PathBuf);

impl Profile {
    fn new(name: &str) -> Profile {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gmnb-modules-{}-{name}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in ["config", "data", "cache", "state", "run"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        std::fs::set_permissions(
            dir.join("run"),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        let profile = Profile(dir);
        std::fs::create_dir_all(profile.theme().join("modules")).unwrap();
        std::fs::write(profile.theme().join("theme.luau"), THEME).unwrap();
        std::fs::write(profile.module(), module(1)).unwrap();
        profile
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.0.join(sub)
    }

    fn theme(&self) -> PathBuf {
        self.path("data/gmnb/themes/modules")
    }

    fn module(&self) -> PathBuf {
        self.theme().join("modules/paint.wasm")
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A GMNB running, its stderr gathered as it comes.
struct Running {
    child: Child,
    err: Arc<Mutex<String>>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl Running {
    fn start(display: &Path, profile: &Profile, env: &[(&str, &str)]) -> Running {
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
            "GDK_DEBUG",
            "GIO_USE_PORTALS",
            "GTK_USE_PORTAL",
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
            .env("GMNB_ACCENT", "none")
            .env("GMNB_AUTOCLOSE_MS", "40000");
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("GMNB started");
        let stderr = child.stderr.take().unwrap();
        let err = Arc::new(Mutex::new(String::new()));
        let gathered = err.clone();
        let reader = std::thread::spawn(move || {
            for line in std::io::BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                let mut err = gathered.lock().unwrap();
                err.push_str(&line);
                err.push('\n');
            }
        });
        Running {
            child,
            err,
            reader: Some(reader),
        }
    }

    fn stderr(&self) -> String {
        self.err.lock().unwrap().clone()
    }

    /// The first line with `part` in it, once there is one.
    fn line(&self, part: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(40);
        loop {
            if let Some(line) = self.stderr().lines().find(|l| l.contains(part)) {
                return line.to_string();
            }
            assert!(
                Instant::now() < deadline,
                "no line with {part:?}: {}",
                self.stderr()
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Waits for GMNB to end (or ends it): all it wrote to stderr.
    fn finish(mut self) -> String {
        let deadline = Instant::now() + Duration::from_secs(60);
        while self.child.try_wait().unwrap().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(r) = self.reader.take() {
            let _ = r.join();
        }
        self.stderr()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The `modules: Stats { ... }` of a `GMNB_THEME_STATS` line.
fn module_stats(line: &str) -> &str {
    let at = line
        .find("modules: Stats")
        .unwrap_or_else(|| panic!("{line}"));
    let rest = &line[at..];
    &rest[..rest.find('}').map_or(rest.len(), |i| i + 1)]
}

const NONE: &str = "modules: Stats { engines: 0, compiles: 0, instances: 0, calls: 0, timeouts: 0, \
                    timers: 0, threads: 0 }";

/// Saved as the theme, it is shown at start with its modules ready: its
/// first `style` (before the window is first shown) already has them.
#[test]
fn a_theme_with_modules_is_ready_at_start() {
    let display = display_or_skip!();
    let profile = Profile::new("start");
    let app = Running::start(
        &display,
        &profile,
        &[
            ("GMNB_THEME", "modules"),
            ("GMNB_THEME_STATS", "1500"),
            ("GMNB_STEPS", "wait:1500;quit"),
        ],
    );
    let stats = app.line("theme: Stats");
    let err = app.finish();
    let first = err
        .lines()
        .find(|l| l.contains("gmnb theme modules: "))
        .unwrap_or_else(|| panic!("{err}"));
    assert_eq!(first, "gmnb theme modules: style bump 1", "{err}");
    assert!(!err.contains("still being compiled"), "{err}");
    assert!(
        err.contains("gmnb theme modules: draw bump"),
        "the background drew: {err}"
    );
    let modules = module_stats(&stats);
    for part in [
        "engines: 1",
        "compiles: 1",
        "instances: 1",
        "threads: 1",
        "timeouts: 0",
    ] {
        assert!(modules.contains(part), "{part}: {stats}");
    }
    assert!(stats.contains("machines: 1"), "{stats}");
}

/// Chosen in Settings, it switches in once its modules have compiled, and
/// no hook of it runs before. Settings' list (which runs its file) and
/// the base look before it made no engine, timer or thread.
#[test]
fn a_theme_with_modules_chosen_in_settings_switches_in_when_they_are_ready() {
    let display = display_or_skip!();
    let profile = Profile::new("choose");
    let app = Running::start(
        &display,
        &profile,
        &[
            ("GMNB_PREFS", "1"),
            ("GMNB_THEME_STATS", "900"),
            ("GMNB_STEPS", "themes;choose:modules;wait:2000;themes;quit"),
        ],
    );
    let stats = app.line("theme: Stats");
    assert_eq!(module_stats(&stats), NONE, "listed, not applied: {stats}");
    let listed = app.line("themes: ");
    assert!(listed.contains(" modules"), "{listed}");
    let err = app.finish();
    let first = err
        .lines()
        .find(|l| l.contains("gmnb theme modules: "))
        .unwrap_or_else(|| panic!("{err}"));
    assert_eq!(first, "gmnb theme modules: style bump 1", "{err}");
    assert!(!err.contains("still being compiled"), "{err}");
    // Nothing to tell its author but what it printed.
    assert!(
        !err.lines()
            .any(|l| l.starts_with("note:") && !l.starts_with("note: print: ")),
        "{err}"
    );
}

/// While Settings is open, the chosen theme's module rebuilt (written
/// again, as a build writes it) reloads the theme: it then runs the new
/// module.
#[test]
fn a_module_rebuilt_while_settings_is_open_reloads_its_theme() {
    let display = display_or_skip!();
    let profile = Profile::new("watch");
    let app = Running::start(
        &display,
        &profile,
        &[
            ("GMNB_THEME", "modules"),
            ("GMNB_PREFS", "1"),
            ("GMNB_STEPS", "wait:6000;quit"),
        ],
    );
    app.line("gmnb theme modules: style bump 1");
    // Settings is open (and watching) a moment after the window is.
    std::thread::sleep(Duration::from_millis(1500));
    // Written elsewhere and moved into place, as a build writes it.
    let next = profile.theme().join("modules/paint.wasm.new");
    std::fs::write(&next, module(100)).unwrap();
    std::fs::rename(&next, profile.module()).unwrap();
    app.line("gmnb theme modules: style bump 100");
    let err = app.finish();
    assert!(!err.contains("still being compiled"), "{err}");
}
