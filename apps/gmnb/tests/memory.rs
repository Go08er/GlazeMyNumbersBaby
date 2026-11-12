//! GMNB's memory ceiling in the running app (docs/theming.md, "Limits":
//! "The memory of the whole of GMNB"): the kernel's `RLIMIT_DATA` at 4 GiB
//! in a started GMNB, and the backstop under it, which saves the session
//! and switches the theme off (here at a threshold lowered for the test,
//! `GMNB_TEST_MEMORY_BACKSTOP`). The off-screen limit at the source is
//! `gmnb-theme`'s (`tests/draw.rs`); that a program GMNB starts doesn't
//! inherit the ceiling, `themes_folder.rs`.
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

/// What GMNB's ceiling is: 4 GiB.
const CEILING: u64 = 4 << 30;

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

/// A folder of one run's own: its profile and its runtime folder.
struct Profile(PathBuf);

impl Profile {
    fn new(name: &str) -> Profile {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gmnb-memory-{}-{name}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in ["config", "data", "cache", "state", "run"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        let run = dir.join("run");
        std::fs::set_permissions(&run, std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .unwrap();
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
            .env("GMNB_ACTIVE", "1")
            .env("GMNB_SIZE", "360x600")
            .env("GMNB_MODE", "standard")
            // In case the steps never get to `quit`.
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

    /// The first line that contains `part`, once there is one.
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

/// The soft and hard "Max data size" (`RLIMIT_DATA`) of the process whose
/// `/proc/<pid>/limits` this is, in bytes (`u64::MAX`: unlimited).
fn data_limit(limits: &str) -> (u64, u64) {
    let line = limits
        .lines()
        .find(|l| l.starts_with("Max data size"))
        .expect("a Max data size line");
    let mut values = line["Max data size".len()..]
        .split_whitespace()
        .map(|v| v.parse().unwrap_or(u64::MAX));
    (values.next().unwrap(), values.next().unwrap())
}

/// A started GMNB has its ceiling in force: its private writable memory
/// may come to 4 GiB (or less, if it was started with less), its hard
/// limit left as it was. Read from the running process, once its window
/// is up.
#[test]
fn a_started_gmnb_may_not_grow_past_four_gib() {
    let display = display_or_skip!();
    let profile = Profile::new("ceiling");
    let ours = data_limit(&std::fs::read_to_string("/proc/self/limits").unwrap());
    let app = Running::start(
        &display,
        &profile,
        &[("GMNB_STEPS", "focused;wait:300;quit")],
    );
    app.line("GMNB_STEPS: focus");
    let limits =
        std::fs::read_to_string(format!("/proc/{}/limits", app.child.id())).expect("GMNB's limits");
    let err = app.finish();
    assert_eq!(
        data_limit(&limits),
        (ours.0.min(CEILING), ours.1),
        "{limits}\n{err}"
    );
}

/// The backstop: once a frame has painted a theme's layer, GMNB reads its
/// own memory, and past the backstop (lowered here to a byte, so that the
/// first reading is past it) it saves the session at once (the
/// calculator's History has what was worked out), switches the theme off
/// and says so: a toast, and a note Settings shows.
#[test]
fn past_the_backstop_the_session_is_saved_and_the_theme_switched_off() {
    let display = display_or_skip!();
    let profile = Profile::new("backstop");
    let theme = profile.path("data/gmnb/themes/hog");
    std::fs::create_dir_all(&theme).unwrap();
    std::fs::write(
        theme.join("theme.luau"),
        r##"
return {
	api = 1,
	name = "Hog",
	layers = {
		background = {
			draw = function(draw, frame)
				draw:rect(0, 0, frame.width, frame.height, "#336699")
				return 1 / 30
			end,
		},
	},
}
"##,
    )
    .unwrap();
    let app = Running::start(
        &display,
        &profile,
        &[
            ("GMNB_THEME", "base"),
            ("GMNB_TEST_MEMORY_BACKSTOP", "1"),
            (
                "GMNB_STEPS",
                "keys:12345+1=;wait:300;theme:hog;wait:3000;quit",
            ),
        ],
    );
    let toast = app.line("was switched off");
    assert!(
        toast.contains("Theme “Hog” was switched off: GMNB's memory reached"),
        "{toast}"
    );
    // Saved before anything else, while GMNB runs on (it saves again when
    // it closes): the History has 12345 + 1, its result 12,346.
    let saved = std::fs::read_to_string(profile.path("config/gmnb/settings.json"))
        .expect("the session saved");
    assert!(saved.contains(r#"\"r\":\"12,346\""#), "{saved}");
    let err = app.finish();
    assert!(
        err.contains("note: memory: GMNB's memory reached"),
        "no note for Settings: {err}"
    );
    // The theme drew no more once it was off: one toast, one reading past
    // the backstop.
    assert_eq!(
        err.matches("Theme “Hog” was switched off").count(),
        1,
        "{err}"
    );
}
