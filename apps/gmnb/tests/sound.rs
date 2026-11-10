//! Theme sounds in the running app: GMNB started on a private compositor
//! with a theme of the test's own, driven as a person would drive it
//! (`GMNB_STEPS`), each sound it plays written to stderr
//! (`GMNB_SOUND_TRACE`).
//!
//! It needs a display: `GMNB_TEST_WAYLAND_DISPLAY` (or
//! `DGMNB_TEST_WAYLAND_DISPLAY`, which the workspace's tests are run
//! with) names a compositor (a headless weston's socket); without one the
//! test is skipped.
//!
//! **Nothing here can reach a sound device.** Each run has a runtime
//! folder of its own (so no sound server's socket is in reach), no
//! `PULSE_*` variables and no session bus. A run either sends its sounds
//! to the null sink (`GMNB_SOUND=null`), or looks for the session's
//! server as the app always does, and finds there only the stand-in
//! server this test puts in its runtime folder: that it connects to that
//! one, and to no other, is part of what is tested.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use gmnb_sound::standin::Server;

/// The keypad's 7 and its = (calcmanager's command numbers).
const SEVEN: u32 = 137;
const EQUALS: u32 = 121;

/// Sounds for each kind of event, told apart by their order: #1 the keys,
/// #2 equals, #3 a result, #4 an error, #5 a change of mode, #6
/// operators and function keys.
const TONES: &str = r#"
local function tone(note, decay)
	return gmnb.synth({ gain = 0.4, layers = { { freq = note, env = { attack = 0.001, decay = decay } } } })
end
local key, equals, result, err, mode = tone("A5", 0.03), tone("C5", 0.05), tone("E6", 0.2), tone("A2", 0.1), tone("D4", 0.1)
local func = tone("B5", 0.03)
return {
	api = 1, name = "Tones",
	sounds = {
		key = { sound = key, pick = "key", rate = { 0.9, 1.1 }, pan = "position" },
		equals = equals,
		result = { sound = result, delay = 0.05 },
		error = err,
		mode = mode,
		operator = func,
		func = func,
	},
}"#;

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
    fn new(name: &str) -> Profile {
        static N: AtomicU32 = AtomicU32::new(0);
        // Short: a socket's path is at most 108 bytes.
        let base = std::env::temp_dir();
        let base = if base.as_os_str().len() > 40 {
            PathBuf::from("/tmp")
        } else {
            base
        };
        let dir = base.join(format!(
            "gmnb-snd-{}-{name}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        for sub in ["config", "data", "cache", "state", "run/pulse", "theme"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        std::fs::write(dir.join("theme/theme.luau"), TONES).unwrap();
        Profile(dir)
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.0.join(sub)
    }

    /// Where the app looks for the session's sound server.
    fn socket(&self) -> PathBuf {
        self.path("run/pulse/native")
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs GMNB once in `profile`: `theme` loaded (the test's, or none),
/// `env` added, for `ms` milliseconds; what it wrote to stderr.
fn run(display: &Path, profile: &Profile, theme: bool, env: &[(&str, &str)], ms: u64) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gmnb"));
    // Nothing of the session's: its display, bus, sound server, or any
    // GMNB_ setting it was started with.
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
        // A headless compositor never says which window is in use.
        .env("GMNB_ACTIVE", "1")
        .env("GMNB_SOUND_TRACE", "1")
        .env("GMNB_THEME_STATS", ms.saturating_sub(300).to_string())
        .env("GMNB_AUTOCLOSE_MS", ms.to_string());
    if theme {
        cmd.env("GMNB_THEME_DIR", profile.path("theme"));
    }
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
    let deadline = Instant::now() + Duration::from_millis(ms) + Duration::from_secs(60);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("GMNB did not close: {}", reader.join().unwrap());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    reader.join().unwrap()
}

/// The lines of the trace.
fn traced(stderr: &str) -> Vec<&str> {
    stderr
        .lines()
        .filter(|l| l.starts_with("sound: "))
        .collect()
}

/// The trace's line for the first event that starts with `what` after
/// line `from`, and its place.
fn line<'a>(lines: &[&'a str], from: usize, what: &str) -> (usize, &'a str) {
    let at = lines[from..]
        .iter()
        .position(|l| l.starts_with(what))
        .unwrap_or_else(|| panic!("no {what:?} after line {from} in {lines:#?}"));
    (from + at, lines[from + at])
}

/// What a play line says the sound and its numbers were ("#1 volume 1.00
/// rate 0.969 pan -0.44 delay 0.00"), if it was queued.
fn played(line: &str) -> Option<&str> {
    let (_, rest) = line.split_once(": #")?;
    rest.contains("Queued")
        .then(|| rest.split(" Queued").next().unwrap())
}

/// What the summary line says.
fn report(stderr: &str) -> &str {
    stderr
        .lines()
        .find(|l| l.starts_with("sound: status "))
        .unwrap_or_else(|| panic!("no summary in {stderr}"))
}

/// Pressing a digit, equals with a result, an error and a change of
/// mode each play the sound designed for it; a typed key the one a
/// clicked one plays; an assistive press nothing.
#[test]
fn events_play_their_sounds() {
    let Some(display) = display() else {
        eprintln!("GMNB_TEST_WAYLAND_DISPLAY not set; skipped");
        return;
    };
    let profile = Profile::new("events");
    let steps = format!(
        "keys:7;click:{SEVEN};activate:{SEVEN};wait:600;keys:=;wait:300;\
         keys:{{escape}};keys:1;keys:/;keys:0;keys:=;wait:300;\
         keys:{{escape}};keys:2;keys:+;keys:3;activate:{EQUALS};wait:600;keys:{{alt+2}}"
    );
    let stderr = run(
        &display,
        &profile,
        true,
        &[("GMNB_SOUND", "null"), ("GMNB_STEPS", &steps)],
        10_000,
    );
    let lines = traced(&stderr);
    // Typed, then clicked: the same key, the same sound, pitch and place.
    let (at, typed) = line(
        &lines,
        0,
        &format!("sound: press (keyboard) key {SEVEN}: digit"),
    );
    let (at, clicked) = line(
        &lines,
        at + 1,
        &format!("sound: press (pointer) key {SEVEN}: digit"),
    );
    assert!(
        played(typed).is_some_and(|p| p.starts_with("1 ")),
        "{typed}"
    );
    assert_eq!(played(typed), played(clicked), "{typed}\n{clicked}");
    // Through the accessibility interface: nothing.
    let (at, assistive) = line(
        &lines,
        at + 1,
        &format!("sound: press (assistive) key {SEVEN}"),
    );
    assert!(assistive.contains("not answered"), "{assistive}");
    // Equals, then its result.
    let (at, equals) = line(
        &lines,
        at + 1,
        &format!("sound: press (keyboard) key {EQUALS}: equals"),
    );
    assert!(
        played(equals).is_some_and(|p| p.starts_with("2 ")),
        "{equals}"
    );
    let (at, result) = line(
        &lines,
        at + 1,
        "sound: result (keyboard) after equals: result",
    );
    assert!(
        played(result).is_some_and(|p| p.starts_with("3 ") && p.ends_with("delay 0.05")),
        "{result}"
    );
    // 1 / 0 =: the running total ÷ shows makes no sound (the table plays
    // a result that ends a calculation); then an error.
    let (at, total) = line(&lines, at + 1, "sound: result (keyboard) after operator");
    assert!(total.contains(": -: nothing"), "{total}");
    let (at, error) = line(
        &lines,
        at + 1,
        "sound: error (keyboard) after equals: error",
    );
    assert!(
        played(error).is_some_and(|p| p.starts_with("4 ")),
        "{error}"
    );
    // 2 + 3 and = through the accessibility interface: neither the press
    // nor the result it brings sounds.
    let (at, press) = line(
        &lines,
        at + 1,
        &format!("sound: press (assistive) key {EQUALS}"),
    );
    assert!(press.contains("not answered"), "{press}");
    let (at, after) = line(&lines, at + 1, "sound: result (assistive) after equals");
    assert!(after.contains("not answered"), "{after}");
    // Alt+2: Scientific.
    let (_, mode) = line(&lines, at + 1, "sound: mode: mode");
    assert!(played(mode).is_some_and(|p| p.starts_with("5 ")), "{mode}");
    // Nothing sounds as the window comes on screen.
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("Queued") && l.contains("show")),
        "{lines:#?}"
    );
    // Every play became a voice in the null sink; no server was looked for.
    let summary = report(&stderr);
    assert!(
        summary.starts_with("sound: status Ready, thread yes, threads 1, connections 0"),
        "{summary}"
    );
    let queued = lines.iter().filter(|l| l.contains("Queued")).count();
    assert!(
        summary.contains(&format!("queued {queued}, started {queued}, skipped 0")),
        "{summary}\n{lines:#?}"
    );
}

/// The base look, the switch off and the desktop's event sounds off make
/// no connection to a sound server, and no thread; a theme with sounds
/// connects, to the server in the run's own runtime folder and to no
/// other, and its sound reaches it.
#[test]
fn only_a_theme_with_sounds_connects() {
    let Some(display) = display() else {
        eprintln!("GMNB_TEST_WAYLAND_DISPLAY not set; skipped");
        return;
    };
    let steps = "keys:7;wait:400;keys:8";
    let nothing = "sound: status Off, thread no, threads 0, connections 0, queued 0";

    // The base look.
    let profile = Profile::new("base");
    let server = Server::start(&profile.socket());
    let stderr = run(&display, &profile, false, &[("GMNB_STEPS", steps)], 2500);
    assert!(
        report(&stderr)
            .starts_with("sound: status Idle, thread no, threads 0, connections 0, queued 0"),
        "{stderr}"
    );
    // (Not even told of events.)
    assert_eq!(traced(&stderr).len(), 1, "{stderr}");
    assert_eq!(server.connections(), 0);
    drop(server);

    // A theme with sounds, Theme sounds switched off.
    let profile = Profile::new("switch");
    std::fs::create_dir_all(profile.path("config/gmnb")).unwrap();
    std::fs::write(
        profile.path("config/gmnb/settings.json"),
        r#"{"sound": false}"#,
    )
    .unwrap();
    let server = Server::start(&profile.socket());
    let stderr = run(&display, &profile, true, &[("GMNB_STEPS", steps)], 2500);
    assert!(report(&stderr).starts_with(nothing), "{stderr}");
    assert_eq!(server.connections(), 0);
    drop(server);

    // The desktop's event sounds off (gtk-enable-event-sounds).
    let profile = Profile::new("desktop");
    std::fs::create_dir_all(profile.path("config/gtk-4.0")).unwrap();
    std::fs::write(
        profile.path("config/gtk-4.0/settings.ini"),
        "[Settings]\ngtk-enable-event-sounds=false\n",
    )
    .unwrap();
    let server = Server::start(&profile.socket());
    let stderr = run(&display, &profile, true, &[("GMNB_STEPS", steps)], 2500);
    let summary = report(&stderr);
    assert!(summary.starts_with(nothing), "{stderr}");
    assert!(
        summary.ends_with("Turned off by the desktop's sound settings"),
        "{summary}"
    );
    assert_eq!(server.connections(), 0);
    drop(server);

    // On: the session's server is looked for where the environment says,
    // and found only in the run's own folder.
    let profile = Profile::new("on");
    let server = Server::start(&profile.socket());
    let stderr = run(&display, &profile, true, &[("GMNB_STEPS", steps)], 3000);
    let summary = report(&stderr);
    assert!(
        summary.starts_with("sound: status Ready, thread yes, threads 1, connections 1,"),
        "{stderr}"
    );
    assert_eq!(server.connections(), 1);
    assert!(
        server.count(|s| matches!(s, gmnb_sound::standin::Saw::Audio { sound: true, .. })) > 0,
        "no sound reached the server: {:?}",
        server.saw()
    );
    // Created corked, started for the keys, corked again at rest, and
    // closed when the app was.
    let closed = Instant::now() + Duration::from_secs(5);
    while server.saw().last() != Some(&gmnb_sound::standin::Saw::Closed) && Instant::now() < closed
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    let saw = server.saw();
    assert_eq!(
        saw.first(),
        Some(&gmnb_sound::standin::Saw::Stream { corked: true })
    );
    assert!(
        saw.contains(&gmnb_sound::standin::Saw::Cork(true)),
        "{saw:?}"
    );
    assert_eq!(
        saw.last(),
        Some(&gmnb_sound::standin::Saw::Closed),
        "{saw:?}"
    );
}
