mod keymap;
mod pages;
mod prefs;
mod settings;
mod theme;
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

static OUTFIT: &[u8] = include_bytes!("../assets/fonts/Outfit-Variable.ttf");

fn main() -> glib::ExitCode {
    // SAFETY: first thing in main, before GTK or any other thread starts.
    unsafe { appcore::tz::fix_sandbox_timezone() };
    // NVIDIA's driver busy-waits on GPU fences by default, which turned the
    // gently drifting background into ~20% of a core. Ask it to sleep
    // instead (only affects this process; respects an explicit setting).
    if std::env::var_os("__GL_YIELD").is_none() {
        // SAFETY: first thing in main, before GTK or any other thread starts.
        unsafe { std::env::set_var("__GL_YIELD", "USLEEP") };
    }
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        register_fonts();
        load_static_css();
    });
    app.connect_activate(|app| {
        if let Some(win) = app.active_window() {
            win.present();
            return;
        }
        let win = window::Window::new(app);
        win.widget().present();
        if let Some(ms) = std::env::var("GMNB_AUTOCLOSE_MS")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            let w = win.widget();
            glib::timeout_add_local_once(Duration::from_millis(ms), move || w.close());
        }
        if std::env::var("GMNB_PREFS").as_deref() == Ok("1") {
            let w = win.clone();
            glib::timeout_add_local_once(Duration::from_millis(300), move || prefs::show(&w));
        }
        if std::env::var("GMNB_COMPACT").as_deref() == Ok("1") {
            let w = win.clone();
            glib::timeout_add_local_once(Duration::from_millis(300), move || {
                w.handle_key(&appcore::KeyPress::named(appcore::Named::Up).alt());
            });
        }
        if let Ok(keys) = std::env::var("GMNB_KEYS") {
            let w = win.clone();
            glib::timeout_add_local_once(Duration::from_millis(400), move || {
                w.simulate_keys(&keys.replace("\\n", "\n"))
            });
        }
        if let Ok(path) = std::env::var("GMNB_SCREENSHOT") {
            schedule_screenshot(win.widget(), PathBuf::from(path));
        }
    });
    app.run()
}

/// Make the bundled display font available to Pango without installing it.
fn register_fonts() {
    let dir = glib::user_cache_dir().join(DATA_DIR).join("fonts");
    let path = dir.join("Outfit-Variable.ttf");
    let stale = std::fs::metadata(&path)
        .map(|m| m.len() != OUTFIT.len() as u64)
        .unwrap_or(true);
    if stale {
        let _ = std::fs::create_dir_all(&dir);
        if let Err(e) = std::fs::write(&path, OUTFIT) {
            glib::g_warning!("gmnb", "could not cache font: {e}");
            return;
        }
    }
    let fontmap = pangocairo::FontMap::default();
    use pango::prelude::*;
    if let Err(e) = fontmap.add_font_file(&path) {
        glib::g_warning!("gmnb", "could not register font: {e}");
    }
}

fn load_static_css() {
    let provider = gtk::CssProvider::new();
    provider.connect_parsing_error(|_, section, err| {
        eprintln!("stylesheet error at {}: {err}", section.to_str());
    });
    provider.load_from_string(include_str!("style.css"));
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("display"),
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
