//! Print what the desktop integration sees: `cargo run -p appcore --example desktop_probe`.

use std::time::Duration;

use appcore::dbus;

fn main() {
    match dbus::Connection::open(dbus::Bus::Session, Duration::from_millis(500)) {
        Ok(mut conn) => {
            let accent =
                dbus::portal_setting(&mut conn, "org.freedesktop.appearance", "accent-color");
            let scheme =
                dbus::portal_setting(&mut conn, "org.freedesktop.appearance", "color-scheme");
            println!(
                "accent-color: {:?}",
                accent.as_ref().and_then(dbus::accent_color)
            );
            println!(
                "prefers dark: {:?}",
                scheme.as_ref().and_then(dbus::prefers_dark)
            );
        }
        Err(e) => println!("session bus: {e}"),
    }
    println!(
        "timezone: {:?}",
        dbus::system_timezone(Duration::from_millis(500))
    );
}
