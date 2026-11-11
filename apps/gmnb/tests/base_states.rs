//! States of the base look that the pairs against DGMNB (`tools/theming`,
//! `base.map`) don't reach, in the running app:
//!
//! - a long value in the History panel and in Programmer's radix list is
//!   shrunk to its room, as DGMNB's `label_fit` shrinks it (down to 10 px
//!   for a result, 9 for a radix value), and is not cut at its start: a
//!   16-digit product lost its leading digits in the panel, and a 64-digit
//!   binary value its first ones in the list (R4-C-1);
//! - in a narrow window Graphing's Equations and Graph switch is two round
//!   icon buttons of the header, the one that is on tinted, as DGMNB's, not
//!   a segmented control (R4-C-2);
//! - an equation's Analyze button is off while the equation is empty or
//!   has an error, as DGMNB's (R4-C-3).
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
            "gmnb-long-values-{}-{name}-{}",
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

/// GMNB in its base look, light, `size`, on `mode`, after `keys` are
/// typed and with `more` in its environment: the window's picture.
fn pictured(
    display: &Path,
    name: &str,
    mode: &str,
    keys: &str,
    size: &str,
    more: &[(&str, &str)],
) -> Pixbuf {
    let profile = Profile::new(name);
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
        .env("GMNB_SIZE", size)
        .env("GMNB_DARK", "0")
        .env("GMNB_THEME", "base")
        .env("GMNB_MODE", mode)
        .env("GMNB_KEYS", keys)
        .envs(more.iter().copied())
        .env("GMNB_SCREENSHOT", &shot)
        .env("GMNB_SCREENSHOT_DELAY_MS", "1800");
    let out = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("GMNB ran");
    let err = String::from_utf8_lossy(&out.stderr);
    Pixbuf::from_file(&shot).unwrap_or_else(|e| panic!("no picture ({e}): {err}"))
}

fn pixel(p: &Pixbuf, x: i32, y: i32) -> [u8; 3] {
    let bytes = p.read_pixel_bytes();
    let at = y as usize * p.rowstride() as usize + x as usize * p.n_channels() as usize;
    [bytes[at], bytes[at + 1], bytes[at + 2]]
}

/// The box (left, top, right, bottom) of what differs by more than 60
/// from `bg` in the band `x0..x1`, `y0..y1`: the ink of the text there.
fn ink(p: &Pixbuf, bg: [u8; 3], (x0, y0, x1, y1): (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    let mut found = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for y in y0..y1 {
        for x in x0..x1 {
            let c = pixel(p, x, y);
            if (0..3).any(|i| c[i].abs_diff(bg[i]) > 60) {
                found = (
                    found.0.min(x),
                    found.1.min(y),
                    found.2.max(x),
                    found.3.max(y),
                );
            }
        }
    }
    found
}

/// Scientific, `9^99=`: its result, `2.9512665430652752148753480226198e+94`,
/// is far wider than the History row. DGMNB sets it at 8 px high (ink);
/// at its full 22 px (17 high) it could only be shown cut at its start.
#[test]
fn a_long_history_result_is_shrunk_to_its_row() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    let p = pictured(&display, "history", "scientific", "9^99=", "760x640", &[]);
    let bg = pixel(&p, 750, 125);
    // The first row's result line, right of the panel's left edge.
    let (l, t, r, b) = ink(&p, bg, (480, 119, 752, 148));
    assert!(r > l, "no text in the row's result line");
    assert!(
        b - t < 10,
        "the result is {} px high ({l},{t} to {r},{b}): not shrunk to its row",
        b - t + 1
    );
    assert!(l <= 490, "the result starts at {l}: it is cut at its start");
}

/// Programmer, 64 binary digits: DGMNB sets the list's last row at 7 px
/// high (ink), to the row's end and past it; at 14 px (11 high) it was cut to
/// "… 1010 1010 …", ending at 442.
#[test]
fn a_long_radix_value_is_shrunk_to_its_row() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    let keys = format!("{{f8}}{}", "10".repeat(32));
    let p = pictured(&display, "radix", "programmer", &keys, "760x640", &[]);
    let bg = pixel(&p, 468, 213);
    let (l, t, r, b) = ink(&p, bg, (68, 212, 470, 238));
    assert!(r > l, "no text in the BIN row");
    assert!(
        b - t < 8,
        "the BIN value is {} px high ({l},{t} to {r},{b}): not shrunk to its row",
        b - t + 1
    );
    assert!(r >= 450, "the BIN value ends at {r}: it is cut short");
}

/// The darkest of `p` within the box, as its red, green and blue.
fn darkest(p: &Pixbuf, (x0, y0, x1, y1): (i32, i32, i32, i32)) -> [u8; 3] {
    let mut low = [255u8; 3];
    for y in y0..y1 {
        for x in x0..x1 {
            let c = pixel(p, x, y);
            if u32::from(c[0]) + u32::from(c[1]) + u32::from(c[2])
                < u32::from(low[0]) + u32::from(low[1]) + u32::from(low[2])
            {
                low = c;
            }
        }
    }
    low
}

/// Graphing at 760 x 640 with one equation: the row's Analyze button
/// (the "fx" before the cross) is dark when the equation is good and
/// faint when it has an error, DGMNB's two states (its glyph is 18 px, at
/// about 294 to 308 by 67 to 81).
#[test]
fn analyze_is_off_for_an_equation_with_an_error() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    let button = (290, 64, 312, 86);
    let good = pictured(
        &display,
        "analyze-good",
        "graphing",
        "",
        "760x640",
        &[("GMNB_EQUATIONS", "y=x")],
    );
    let bad = pictured(
        &display,
        "analyze-bad",
        "graphing",
        "",
        "760x640",
        &[("GMNB_EQUATIONS", "y=x+")],
    );
    let (good, bad) = (darkest(&good, button), darkest(&bad, button));
    assert!(good.iter().all(|c| *c < 90), "a good equation: {good:?}");
    assert!(
        bad.iter().all(|c| *c > 120),
        "an equation with an error: {bad:?}"
    );
}

/// Graphing at 420 x 640: the Equations button, on, is a circle of the
/// accent at a sixth of its strength (its top, clear of the glyph, is
/// pale), not the accent itself.
#[test]
fn the_narrow_graphing_switch_is_the_headers_round_buttons() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    let p = pictured(&display, "switch", "graphing", "", "420x640", &[]);
    let [r, g, b] = pixel(&p, 236, 8);
    assert!(
        r > 150 && g > 170 && b > 220,
        "the Equations button's top is ({r}, {g}, {b}), not a pale accent"
    );
}
