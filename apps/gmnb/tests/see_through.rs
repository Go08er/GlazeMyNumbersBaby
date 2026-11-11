//! A background layer on a see-through window, in the running app
//! (docs/theming.md, "See-through windows"): faded with
//! `window.background` to the Background opacity, or, if its table says
//! `fade = false`, left at full strength to thin itself.
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

/// A folder of one run's own: its profile, its runtime folder, its theme
/// and its picture.
struct Profile(PathBuf);

impl Profile {
    fn new(name: &str) -> Profile {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gmnb-see-through-{}-{name}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        for sub in ["config", "data", "cache", "state", "run", "theme"] {
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

/// GMNB at Background opacity `opacity` under a theme whose window is
/// black and whose background layer paints everything red, the layer's
/// table saying `fade`: the window's picture.
fn pictured(display: &Path, name: &str, fade: &str, opacity: &str) -> Pixbuf {
    let profile = Profile::new(name);
    let theme = format!(
        r##"
return {{
	api = 1,
	name = "See-through",
	style = function(ctx)
		return {{ window = {{ background = "#000000" }} }}
	end,
	layers = {{
		background = {{
			draw = function(draw, frame)
				draw:rect(0, 0, frame.width, frame.height, "#ff0000")
				return nil
			end,
			{fade}
		}},
	}},
}}
"##
    );
    std::fs::write(profile.path("theme").join("theme.luau"), theme).unwrap();
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
        .env("GMNB_THEME_DIR", profile.path("theme"))
        .env("GMNB_OPACITY", opacity)
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

/// The pixel at `x, y`: red, green, blue as they are stored (not
/// multiplied by the opacity), and the opacity.
fn pixel(p: &Pixbuf, x: i32, y: i32) -> [u8; 4] {
    assert!(p.has_alpha() && p.n_channels() == 4);
    let bytes = p.read_pixel_bytes();
    let at = y as usize * p.rowstride() as usize + x as usize * 4;
    [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
}

/// At Background opacity 0.25, in the window's left margin, where
/// nothing lies over the backdrop. A background layer is faded with
/// `window.background`, a quarter there; one that said `fade = false` is
/// all there, over the quarter of `window.background`; and at full
/// opacity there is no telling them apart.
#[test]
fn a_background_layer_is_faded_unless_it_thins_itself() {
    let Some(display) = display() else {
        eprintln!("skipped: no GMNB_TEST_WAYLAND_DISPLAY or DGMNB_TEST_WAYLAND_DISPLAY");
        return;
    };
    let margin = |p: &Pixbuf| pixel(p, 3, p.height() * 3 / 4);
    let near = |a: u8, b: u8| a.abs_diff(b) <= 3;

    let faded = margin(&pictured(&display, "faded", "", "0.25"));
    assert!(
        near(faded[3], 64) && faded[0] > 240 && faded[1] < 16 && faded[2] < 16,
        "faded: {faded:?}"
    );
    let said_so = margin(&pictured(&display, "fade", "fade = true,", "0.25"));
    assert_eq!(said_so, faded);

    let own = margin(&pictured(&display, "own", "fade = false,", "0.25"));
    assert_eq!(own, [255, 0, 0, 255], "the layer thins itself");

    let opaque = margin(&pictured(&display, "opaque", "", "1"));
    let opaque_own = margin(&pictured(&display, "opaque-own", "fade = false,", "1"));
    assert_eq!(opaque, [255, 0, 0, 255]);
    assert_eq!(opaque_own, opaque);
}
