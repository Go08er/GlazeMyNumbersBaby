mod a11y;
mod bench;
mod inert;
mod keymap;
mod launch;
mod layers;
mod pages;
mod paste;
mod prefs;
mod settings;
mod sound;
mod steps;
mod theme;
mod undo;
mod widgets;
mod window;

use std::path::PathBuf;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, glib, graphene};

pub const APP_ID: &str = "io.github.Go08er.GlazeMyNumbersBaby";

/// Display name.
pub const APP_NAME: &str = "GMNB";

/// Directory name under the XDG config/cache dirs.
pub const DATA_DIR: &str = "gmnb";

/// The base look's typeface: Inter, the file DGMNB bundles (its family
/// name is "Inter Variable"). Themes bring their own fonts (Aurora:
/// Outfit).
static INTER: &[u8] = include_bytes!("../../dgmnb/assets/fonts/Inter.ttf");

/// The subsets DGMNB falls back to after Inter (`fonts.ui`): the arrows
/// and the mathematical signs Inter lacks (▾), and four scripts.
static FALLBACKS: [(&str, &[u8]); 5] = [
    (
        "NotoSansMath-subset.otf",
        include_bytes!("../../dgmnb/assets/fonts/NotoSansMath-subset.otf"),
    ),
    (
        "NotoSansArabic-subset.ttf",
        include_bytes!("../../dgmnb/assets/fonts/NotoSansArabic-subset.ttf"),
    ),
    (
        "NotoSansArmenian-subset.ttf",
        include_bytes!("../../dgmnb/assets/fonts/NotoSansArmenian-subset.ttf"),
    ),
    (
        "NotoSansBengali-subset.ttf",
        include_bytes!("../../dgmnb/assets/fonts/NotoSansBengali-subset.ttf"),
    ),
    (
        "NotoSansKhmer-subset.ttf",
        include_bytes!("../../dgmnb/assets/fonts/NotoSansKhmer-subset.ttf"),
    ),
];

/// The structure stylesheet (docs/theming.md, "The pieces").
pub const STYLESHEET: &str = include_str!("style.css");

/// `--theme <id>`: the theme for this run (`base`: the base look), without
/// changing the saved choice. `None` inside: not given.
pub static THEME_ARG: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();

/// Takes `--theme <id>` (or `--theme=<id>`) out of the arguments.
fn take_theme_arg(args: Vec<String>) -> (Vec<String>, Option<String>) {
    let mut rest = Vec::with_capacity(args.len());
    let mut theme = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--theme" {
            theme = args.next().or(theme);
        } else if let Some(id) = arg.strip_prefix("--theme=") {
            theme = Some(id.to_string());
        } else {
            rest.push(arg);
        }
    }
    (rest, theme)
}

/// Whether GSK_RENDERER came from the environment GMNB was started in (so
/// the "Vulkan acceleration" setting doesn't decide the renderer).
pub static RENDERER_FROM_ENV: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Whether GMNB set GSK_RENDERER for itself (until the window has its
/// renderer).
static RENDERER_SET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn main() -> glib::ExitCode {
    launch::mark("main");
    // SAFETY: first thing in main, before GTK or any other thread starts.
    unsafe { appcore::tz::fix_sandbox_timezone() };
    // NVIDIA's driver busy-waits on GPU fences by default, which turned the
    // gently drifting background into ~20% of a core. Ask it to sleep
    // instead (only affects this process; respects an explicit setting).
    if std::env::var_os("__GL_YIELD").is_none() {
        // SAFETY: first thing in main, before GTK or any other thread starts.
        unsafe { std::env::set_var("__GL_YIELD", "USLEEP") };
    }
    // "Vulkan acceleration" off: GSK's software renderer, from this launch
    // on (the renderer is picked when the first window is realised). An
    // explicit GSK_RENDERER wins.
    let from_env = std::env::var_os("GSK_RENDERER").is_some();
    let _ = RENDERER_FROM_ENV.set(from_env);
    if !from_env && !settings::wants_vulkan() {
        // SAFETY: first thing in main, before GTK or any other thread starts.
        unsafe { std::env::set_var("GSK_RENDERER", "cairo") };
        RENDERER_SET.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    // Wayland only: GDK must not fall back to its X11 backend, so that
    // without a Wayland session there is no display, rather than an X one.
    gdk::set_allowed_backends("wayland");
    // Initialise libadwaita before the application starts up. AdwApplication
    // would do it after GtkApplication's startup, which has already loaded
    // the icon theme; libadwaita then adds its own icon path, and GTK loads
    // the whole theme a second time on the first icon lookup.
    let display = adw::init().is_ok();
    // Without a display, only a second instance can do anything: hand
    // activation to the running GMNB, which needs no display. Otherwise
    // fail as GTK would, before any startup code runs without one.
    if !display && !already_running() {
        eprintln!("gmnb: needs a Wayland session, but cannot connect to a Wayland display");
        return glib::ExitCode::FAILURE;
    }
    if display && icon_installed() {
        // What GtkApplication's startup would conclude, without loading the
        // whole icon theme on the main thread to find out: GTK then loads it
        // on its own thread while the window is being set up.
        gtk::Window::set_default_icon_name(APP_ID);
    }
    launch::mark("adw-init");
    let app = adw::Application::builder().application_id(APP_ID).build();
    // GMNB ships no GResources. Without this, GtkApplication's startup adds
    // "<base path>/icons/" to the icon theme, which makes GTK throw away and
    // reload the whole theme a second time.
    app.set_resource_base_path(None);
    let no_display = std::rc::Rc::new(std::cell::Cell::new(false));
    app.connect_startup({
        let no_display = no_display.clone();
        move |app| {
            launch::mark("startup");
            // Only reachable if the running instance quit in the meantime.
            if !display || gdk::Display::default().is_none() {
                eprintln!("gmnb: needs a Wayland session, but cannot connect to a Wayland display");
                no_display.set(true);
                app.quit();
                return;
            }
            started_up();
        }
    });
    let failed = no_display.clone();
    app.connect_activate(move |app| {
        if failed.get() {
            return;
        }
        activate(app);
    });
    let (args, theme) = take_theme_arg(std::env::args().collect());
    let _ = THEME_ARG.set(theme.filter(|t| !t.is_empty()));
    let code = app.run_with_args(&args);
    if no_display.get() {
        glib::ExitCode::FAILURE
    } else {
        code
    }
}

fn started_up() {
    // Before any window is realized: every text field pastes within
    // bounds, an insensitive range takes no value from assistive
    // technology (what a dialog covers is insensitive: crate::inert), and a
    // text field made under a covered layer is read-only like those there.
    paste::guard_all();
    inert::refuse_insensitive_values();
    inert::guard_new_fields();
    // Every activatable row can be activated by assistive technology,
    // and a label offers only the actions it can carry out.
    a11y::install();
    register_fonts();
    launch::mark("fonts");
    load_static_css();
    launch::mark("css");
}

fn activate(app: &adw::Application) {
    if let Some(win) = app.active_window() {
        win.present();
        return;
    }
    launch::mark("activate");
    let win = window::Window::new(app);
    launch::mark("window-built");
    launch::exit_after_first_frame(&win.widget());
    win.present();
    launch::mark("presented");
    // GTK reads GSK_RENDERER once, when it makes its first renderer: the
    // window's, realised by present(). What GMNB set for itself must not
    // reach the browser or anything else it starts.
    if win.widget().renderer().is_some()
        && RENDERER_SET.swap(false, std::sync::atomic::Ordering::Relaxed)
    {
        // SAFETY: GTK has read the variable, and nothing else reads it.
        // GLib's threads may read other variables meanwhile; glibc's
        // unsetenv takes the environment lock and frees nothing they could
        // be reading.
        unsafe { std::env::remove_var("GSK_RENDERER") };
    }
    if let Some(ms) = std::env::var("GMNB_AUTOCLOSE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        let w = win.widget();
        glib::timeout_add_local_once(Duration::from_millis(ms), move || w.close());
    }
    if std::env::var("GMNB_PREFS").as_deref() == Ok("1") {
        let w = win.clone();
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            let start = std::time::Instant::now();
            prefs::show(&w);
            if std::env::var_os("GMNB_THEME_STATS").is_some() {
                eprintln!(
                    "theme: Settings built in {:.1} ms",
                    start.elapsed().as_secs_f64() * 1000.0
                );
            }
        });
    }
    // Dev/screenshot hook: About, as Settings' row opens it.
    if std::env::var("GMNB_PREFS").as_deref() == Ok("about") {
        let w = win.clone();
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            prefs::about(&w.widget(), &w.ctx().hub.credits());
        });
    }
    if std::env::var("GMNB_COMPACT").as_deref() == Ok("1") {
        let w = win.clone();
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            w.handle_key(&appcore::KeyPress::named(appcore::Named::Up).alt());
        });
    }
    if let Ok(steps) = std::env::var("GMNB_STEPS") {
        steps::run(&win, &steps);
    }
    if let Ok(keys) = std::env::var("GMNB_KEYS") {
        let w = win.clone();
        glib::timeout_add_local_once(Duration::from_millis(400), move || {
            w.simulate_keys(&keys.replace("\\n", "\n"))
        });
    }
    if let Ok(spec) = std::env::var("GMNB_BENCH") {
        bench::run(&win, &spec);
    }
    if let Ok(when) = std::env::var("GMNB_THEME_STATS") {
        // What theming holds once the window has settled: after 1.2 s, or
        // after as many milliseconds as the variable says.
        let w = win.clone();
        let after = when.parse().unwrap_or(1200);
        glib::timeout_add_local_once(Duration::from_millis(after), move || {
            let hub = &w.ctx().hub;
            eprintln!("theme: {:?}", hub.stats());
            // Each layer hook: its failures, and whether it was switched off.
            eprintln!("theme hooks: {:?}", hub.hooks());
            eprintln!("sound: {}", hub.sound_report());
        });
    }
    if let Ok(class) = std::env::var("GMNB_FOCUS") {
        focus_by_class(win.widget(), class);
    }
    if let Ok(path) = std::env::var("GMNB_SCREENSHOT") {
        schedule_screenshot(win.widget(), PathBuf::from(path));
    }
}

/// Dev/screenshot hook: `GMNB_FOCUS=<css class>` gives the keyboard's
/// focus, ring shown, to the first widget with that class, as Tab to it
/// would (DGMNB: `DGMNB_KEYS` with `{tab}`s).
fn focus_by_class(win: adw::ApplicationWindow, class: String) {
    glib::timeout_add_local_once(Duration::from_millis(600), move || {
        let mut todo = vec![win.clone().upcast::<gtk::Widget>()];
        while let Some(w) = todo.pop() {
            // (A text field takes it in its text, which grab_focus says.)
            if w.has_css_class(&class) && w.grab_focus() {
                win.set_focus_visible(true);
                return;
            }
            let mut child = w.last_child();
            while let Some(c) = child {
                child = c.prev_sibling();
                todo.push(c);
            }
        }
        eprintln!("GMNB_FOCUS: nothing focusable with class {class}");
    });
}

/// Whether another GMNB owns the application's bus name.
fn already_running() -> bool {
    appcore::dbus::Connection::open(appcore::dbus::Bus::Session, Duration::from_secs(1))
        .is_ok_and(|mut c| c.name_owner(APP_ID).is_some())
}

/// Whether the app icon is installed where every package puts it (the
/// hicolor theme, which every icon theme falls back to). If it's only
/// somewhere else, GtkApplication's own check still finds it.
fn icon_installed() -> bool {
    let file = format!("icons/hicolor/scalable/apps/{APP_ID}.svg");
    std::iter::once(glib::user_data_dir())
        .chain(glib::system_data_dirs())
        .any(|dir| dir.join(&file).is_file())
}

/// Writes a bundled font file to the cache directory (once: a file of
/// the same size is taken to be it), for Pango to load.
pub fn cache_font(name: &str, bytes: &[u8]) -> Option<PathBuf> {
    let dir = glib::user_cache_dir().join(DATA_DIR).join("fonts");
    let path = dir.join(name);
    let stale = std::fs::metadata(&path)
        .map(|m| m.len() != bytes.len() as u64)
        .unwrap_or(true);
    if stale {
        let _ = std::fs::create_dir_all(&dir);
        if let Err(e) = std::fs::write(&path, bytes) {
            glib::g_warning!("gmnb", "could not cache font: {e}");
            return None;
        }
    }
    Some(path)
}

/// Makes a font file available to this app only, under the family name
/// inside it.
pub fn add_font_file(path: &std::path::Path) {
    let fontmap = pangocairo::FontMap::default();
    use pango::prelude::*;
    if let Err(e) = fontmap.add_font_file(path) {
        glib::g_warning!("gmnb", "could not register font: {e}");
    }
}

/// Make the base look's font available to Pango without installing it.
fn register_fonts() {
    if let Some(path) = cache_font("Inter.ttf", INTER) {
        add_font_file(&path);
    }
    for (name, bytes) in FALLBACKS {
        if let Some(path) = cache_font(name, bytes) {
            add_font_file(&path);
        }
    }
}

fn load_static_css() {
    let provider = gtk::CssProvider::new();
    provider.connect_parsing_error(|_, section, err| {
        eprintln!("stylesheet error at {}: {err}", section.to_str());
    });
    provider.load_from_string(STYLESHEET);
    let Some(display) = gdk::Display::default() else {
        return;
    };
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        // Above USER so a desktop's gtk.css can't repaint the app's design.
        gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
    );
}

/// Dev/packaging helper: `GMNB_SCREENSHOT=out.png` renders the window
/// offscreen to a PNG once it has settled, then quits.
fn schedule_screenshot(win: adw::ApplicationWindow, path: PathBuf) {
    let delay = std::env::var("GMNB_SCREENSHOT_DELAY_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1600);
    glib::timeout_add_local_once(Duration::from_millis(delay), move || {
        let (w, h) = (win.width(), win.height());
        let paintable = gtk::WidgetPaintable::new(Some(&win));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, w as f64, h as f64);
        let result = snapshot
            .to_node()
            .zip(win.renderer())
            .map(|(node, renderer)| {
                renderer.render_texture(
                    &node,
                    Some(&graphene::Rect::new(0.0, 0.0, w as f32, h as f32)),
                )
            })
            .ok_or("nothing rendered")
            .and_then(|tex| tex.save_to_png(&path).map_err(|_| "save failed"));
        match result {
            Ok(()) => eprintln!("screenshot: {} ({w}x{h})", path.display()),
            Err(e) => eprintln!("screenshot failed: {e}"),
        }
        if let Some(app) = win.application() {
            app.quit();
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_theme_argument_is_taken_out() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            super::take_theme_arg(args(&["gmnb", "--theme", "base", "x"])),
            (args(&["gmnb", "x"]), Some("base".to_string()))
        );
        assert_eq!(
            super::take_theme_arg(args(&["gmnb", "--theme=mono"])),
            (args(&["gmnb"]), Some("mono".to_string()))
        );
        assert_eq!(
            super::take_theme_arg(args(&["gmnb", "--theme"])),
            (args(&["gmnb"]), None)
        );
        assert_eq!(
            super::take_theme_arg(args(&["gmnb"])),
            (args(&["gmnb"]), None)
        );
    }

    /// GMNB depends on the theme engine before the theme API uses it, so
    /// that every package build already compiles its C++ (Luau): here it
    /// links, a theme file's table comes back and its hook runs.
    #[test]
    fn theme_engine_links() {
        use gmnb_theme::{Budget, Engine, Limits, ThemeValue};
        let engine = Engine::new(Limits::default()).unwrap();
        let theme = engine
            .load(
                "=theme",
                "return { api = 1, style = function(ctx) return ctx end }",
            )
            .unwrap();
        assert_eq!(theme.get("api"), Some(&ThemeValue::Int(1)));
        let style = theme
            .get("style")
            .and_then(ThemeValue::as_function)
            .unwrap();
        assert_eq!(
            engine.call(style, Budget::Style, "dark"),
            Ok(ThemeValue::String("dark".into()))
        );
    }
}
