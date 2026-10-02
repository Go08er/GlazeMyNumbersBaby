//! Flatpak can only map a host `/etc/localtime` that points into
//! `/usr/share/zoneinfo`; on NixOS (→ `/etc/zoneinfo/...`) the sandbox falls
//! back to UTC, so "Updated 4:36 AM" shows UTC and Date's "today" rolls over
//! early. Ask systemd-timedated for the real zone and export it as `TZ`.

use std::path::Path;
use std::time::Duration;

/// Set `TZ` from systemd-timedated when running in a Flatpak sandbox whose
/// local time is wrong. Does nothing outside Flatpak or if `TZ` is set.
///
/// # Safety
///
/// Mutates the process environment: call it first thing in `main`, before
/// any other thread exists. (The D-Bus query itself starts no threads.)
pub unsafe fn fix_sandbox_timezone() {
    if std::env::var_os("TZ").is_some() || !Path::new("/.flatpak-info").exists() {
        return;
    }
    let Some(zone) = crate::dbus::system_timezone(Duration::from_millis(500)) else {
        return;
    };
    let already = std::fs::read_link("/etc/localtime").is_ok_and(|t| t.ends_with(&zone));
    if valid_zone(&zone, Path::new("/usr/share/zoneinfo")) && !already {
        // SAFETY: the caller guarantees no other threads exist yet.
        unsafe { std::env::set_var("TZ", zone) };
    }
}

/// A zone name that names a real tzdata file under `root`.
pub fn valid_zone(zone: &str, root: &Path) -> bool {
    !zone.is_empty()
        && !zone.starts_with('/')
        && !zone.split('/').any(|part| part.is_empty() || part == "..")
        && root.join(zone).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_names_are_validated() {
        let root = std::env::temp_dir().join(format!("appcore-tz-{}", std::process::id()));
        std::fs::create_dir_all(root.join("America")).unwrap();
        std::fs::write(root.join("America/Chicago"), b"TZif").unwrap();
        assert!(valid_zone("America/Chicago", &root));
        assert!(!valid_zone("America/Nowhere", &root));
        assert!(!valid_zone("../America/Chicago", &root));
        assert!(!valid_zone("/etc/passwd", &root));
        assert!(!valid_zone("", &root));
        assert!(!valid_zone("America", &root));
    }
}
