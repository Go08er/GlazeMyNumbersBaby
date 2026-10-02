//! DGMNB — Don't Glaze My Numbers, Baby. The resource-lean twin of GMNB:
//! the same calculator engines and features, drawn in software with winit,
//! softbuffer and tiny-skia, and nothing running while you're not using it.

mod a11y;
mod app;
mod calc;
mod clipboard;
mod conv;
mod date;
mod edit;
mod gfx;
mod graph;
mod svgpath;
mod text;
mod theme;
mod ui;

use std::time::Duration;

use appcore::dbus;
use winit::event_loop::EventLoop;

use crate::app::{App, Desktop, UserEvent};

/// Light/dark preference and accent colour from the XDG Settings portal.
fn read_desktop() -> Desktop {
    let Ok(mut conn) = dbus::Connection::open(dbus::Bus::Session, Duration::from_millis(400))
    else {
        return Desktop::default();
    };
    let scheme = dbus::portal_setting(&mut conn, "org.freedesktop.appearance", "color-scheme");
    let accent = dbus::portal_setting(&mut conn, "org.freedesktop.appearance", "accent-color");
    Desktop {
        dark: scheme.as_ref().and_then(dbus::prefers_dark),
        accent: accent.as_ref().and_then(dbus::accent_color),
        portal: scheme.is_some() || accent.is_some(),
    }
}

fn main() {
    // SAFETY: first thing in main; no other thread exists yet.
    unsafe { appcore::tz::fix_sandbox_timezone() };
    let desktop = read_desktop();
    let event_loop = match EventLoop::<UserEvent>::with_user_event().build() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("dgmnb: no display: {e}");
            std::process::exit(1);
        }
    };
    let proxy = event_loop.create_proxy();
    if desktop.portal {
        // Follow light/dark and accent changes live (one sleeping thread).
        let proxy = proxy.clone();
        let _ = std::thread::Builder::new()
            .name("portal".into())
            .stack_size(64 * 1024)
            .spawn(move || {
                let mut state = desktop;
                let _ = dbus::watch_portal_settings(|ns, key, value| {
                    if ns != "org.freedesktop.appearance" {
                        return;
                    }
                    match key {
                        "color-scheme" => state.dark = dbus::prefers_dark(value),
                        "accent-color" => state.accent = dbus::accent_color(value),
                        _ => return,
                    }
                    let _ = proxy.send_event(UserEvent::Desktop(state));
                });
            });
    }
    let mut app = App::new(proxy, desktop);
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("dgmnb: {e}");
    }
}
