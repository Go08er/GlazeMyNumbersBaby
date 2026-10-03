//! Dev hook: `GMNB_LAUNCH_TIMING=1` prints startup milestones to stderr as
//! `launch <what> <µs>` (CLOCK_MONOTONIC, so a launcher can subtract its
//! own exec time) and exits as soon as the first frame has been painted.

use std::sync::OnceLock;

use adw::prelude::*;
use gtk::glib;

pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("GMNB_LAUNCH_TIMING").is_some())
}

pub fn mark(what: &str) {
    if enabled() {
        eprintln!("launch {what} {}", glib::monotonic_time());
    }
}

/// Report the first painted frame of `win`, then exit.
pub fn exit_after_first_frame(win: &adw::ApplicationWindow) {
    if !enabled() {
        return;
    }
    win.connect_map(|w| {
        mark("map");
        if let Some(clock) = w.frame_clock() {
            clock.connect_after_paint(|_| {
                mark("first-frame");
                std::process::exit(0);
            });
        }
    });
}
