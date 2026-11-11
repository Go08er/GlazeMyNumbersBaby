//! The keyboard's focus ring in the running app (docs/theming.md, "The
//! focus ring", readability rule 5): GMNB started on a private compositor,
//! a key given the focus with its ring shown (`GMNB_FOCUS`), and the
//! window pictured.
//!
//! It needs a display: `GMNB_TEST_WAYLAND_DISPLAY` (or
//! `DGMNB_TEST_WAYLAND_DISPLAY`, which the workspace's tests are run
//! with) names a compositor (a headless weston's socket); without one the
//! test is skipped. Each run has a profile and a runtime folder of its
//! own, no session bus and no sound (`GMNB_SOUND=null`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use gtk::gdk_pixbuf::Pixbuf;

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

/// A folder of one run's own: its profile, its runtime folder, its picture.
struct Profile(PathBuf);

impl Profile {
    fn new(name: &str) -> Profile {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gmnb-focus-{}-{name}-{}",
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

/// GMNB with `env`, pictured once the key with class `class` has the
/// focus: the picture, and where the key is in it.
fn pictured(display: &Path, class: &str, env: &[(&str, &str)]) -> (Pixbuf, [i32; 4]) {
    let profile = Profile::new(class);
    let shot = profile.path("shot.png");
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
        .env("GMNB_SIZE", "360x600")
        .env("GMNB_MODE", "standard")
        .env("GMNB_FOCUS", class)
        .env("GMNB_SCREENSHOT", &shot)
        .env("GMNB_SCREENSHOT_DELAY_MS", "1800");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("GMNB ran");
    let err = String::from_utf8_lossy(&out.stderr);
    let at = format!("GMNB_FOCUS: {class} at ");
    let rect: Vec<i32> = err
        .lines()
        .find_map(|l| l.split_once(&at).map(|(_, r)| r.to_string()))
        .unwrap_or_else(|| panic!("nothing took the focus: {err}"))
        .split_whitespace()
        .map(|n| n.parse::<f32>().unwrap().round() as i32)
        .collect();
    let picture = Pixbuf::from_file(&shot).unwrap_or_else(|e| panic!("no picture ({e}): {err}"));
    (picture, [rect[0], rect[1], rect[2], rect[3]])
}

/// The colour of the pixel at `x, y`.
fn pixel(p: &Pixbuf, x: i32, y: i32) -> [u8; 3] {
    let bytes = p.read_pixel_bytes();
    let at = y as usize * p.rowstride() as usize + x as usize * p.n_channels() as usize;
    [bytes[at], bytes[at + 1], bytes[at + 2]]
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 6)
}

/// Swirls' ring is the paint's own gold (white in the Marble bands), 3 px
/// at 5 px out, and the paint around a key is gold too where the warm
/// band flows: the equals key, filled with it, lost its ring there. Its
/// edge, a line of the dark band along each side of the ring, is drawn
/// around the key: left of the key, 1.5 px out, the dark band; 3.5 px out,
/// the gold; 5.5 px out, the dark band again. Without the edge those were
/// whatever the paint was doing.
#[test]
fn swirls_ring_has_its_edge_round_a_key() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    for (bands, gold, dark) in [
        ("party", [0xff, 0xcf, 0x29], [0x01, 0x06, 0x09]),
        ("marble", [0xf5, 0xfa, 0xff], [0x02, 0x05, 0x07]),
    ] {
        let options = format!("bands={bands}");
        let (p, [x, y, _w, h]) = pictured(
            &display,
            "wc-eq",
            &[("GMNB_THEME", "swirls"), ("GMNB_THEME_OPTIONS", &options)],
        );
        // Along the middle of the key's left side, the pixels 2, 3 to 5
        // and 6 out.
        let mid = y + h / 2;
        let at = |out: i32| pixel(&p, x - out, mid);
        for (out, want, what) in [
            (2, dark, "the inner edge"),
            (3, gold, "the ring"),
            (4, gold, "the ring"),
            (5, gold, "the ring"),
            (6, dark, "the outer edge"),
        ] {
            assert!(
                near(at(out), want),
                "{bands}: {what}, {out} px out: {:?}, not {want:?}",
                at(out)
            );
        }
    }
}

/// The base look's ring is DGMNB's: 2 px of `accent_text` touching the
/// key, with no edge. Left of a number key: the ring 1 and 2 px out, the
/// page 3 px out.
#[test]
fn the_base_looks_ring_has_no_edge() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    let (p, [x, y, _w, h]) = pictured(
        &display,
        "wc-num",
        &[("GMNB_THEME", "base"), ("GMNB_DARK", "1")],
    );
    let mid = y + h / 2;
    let (ring, page) = (pixel(&p, x - 1, mid), pixel(&p, x - 3, mid));
    assert!(
        near(pixel(&p, x - 2, mid), ring),
        "{:?}",
        pixel(&p, x - 2, mid)
    );
    assert!(!near(ring, page), "no ring: {ring:?} on {page:?}");
    assert!(
        near(pixel(&p, x - 4, mid), page),
        "{:?}",
        pixel(&p, x - 4, mid)
    );
}
