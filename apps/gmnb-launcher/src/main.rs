//! `gmnb` as installed: checks the CPU, then runs the real GMNB.
//!
//! On x86-64 GMNB is built for x86-64-v3 (Intel Haswell from 2013, AMD
//! Excavator from 2015, Ryzen and later), so an older CPU would kill it
//! with an illegal instruction before it could say why. This launcher is
//! built for any x86-64 and says why instead: on stderr, and to someone who
//! started GMNB from their desktop with a notification or a dialog.
//!
//! The real binary lives beside it, as `../libexec/gmnb/gmnb` (or
//! `../lib/gmnb/gmnb`, Arch's place for such programs), and is exec'd with
//! the same arguments and environment.

use std::ffi::OsString;
use std::io::IsTerminal;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

const MESSAGE: &str = "GMNB needs a CPU from 2013 or newer (Intel Haswell / AMD Excavator / Ryzen or later). DGMNB runs on any 64-bit PC.";
const TITLE: &str = "GMNB needs a newer CPU";

fn main() -> ExitCode {
    if !cpu_ok() {
        eprintln!("gmnb: {MESSAGE}");
        if !std::io::stderr().is_terminal() {
            tell_desktop();
        }
        return ExitCode::FAILURE;
    }
    let Some(real) = std::env::current_exe().ok().and_then(|me| real_binary(&me)) else {
        eprintln!(
            "gmnb: the GMNB program is missing (expected ../libexec/gmnb/gmnb beside this launcher)"
        );
        return ExitCode::FAILURE;
    };
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let err = Command::new(&real).arg0("gmnb").args(args).exec();
    eprintln!("gmnb: could not run {}: {err}", real.display());
    ExitCode::FAILURE
}

/// Whether this CPU runs x86-64-v3 code: AVX, AVX2, BMI1, BMI2, F16C, FMA,
/// LZCNT, MOVBE and XSAVE, on top of x86-64-v2's CMPXCHG16B, POPCNT, SSE3,
/// SSSE3, SSE4.1 and SSE4.2. (The AVX check includes the operating system
/// saving the AVX registers.)
#[cfg(target_arch = "x86_64")]
fn cpu_ok() -> bool {
    use std::arch::is_x86_feature_detected as has;
    has!("avx")
        && has!("avx2")
        && has!("bmi1")
        && has!("bmi2")
        && has!("f16c")
        && has!("fma")
        && has!("lzcnt")
        && has!("movbe")
        && has!("xsave")
        && has!("cmpxchg16b")
        && has!("popcnt")
        && has!("sse3")
        && has!("ssse3")
        && has!("sse4.1")
        && has!("sse4.2")
}

/// Other architectures get a GMNB built for their baseline.
#[cfg(not(target_arch = "x86_64"))]
fn cpu_ok() -> bool {
    true
}

/// The real GMNB for the launcher at `me` (…/bin/gmnb).
fn real_binary(me: &Path) -> Option<PathBuf> {
    let prefix = me.parent()?.parent()?;
    ["libexec", "lib"]
        .iter()
        .map(|dir| prefix.join(dir).join("gmnb").join("gmnb"))
        .find(|p| p.is_file())
}

/// Shows the message to a desktop user: a notification if anything will
/// take one, else a dialog. Stops at the first that works; each is skipped
/// quietly if it isn't installed or has nothing to show it on.
fn tell_desktop() {
    let portal = format!("{{'title': <'{TITLE}'>, 'body': <'{MESSAGE}'>}}");
    let attempts: [&[&str]; 4] = [
        &[
            "notify-send",
            "--app-name=GMNB",
            "--icon=io.github.Go08er.GlazeMyNumbersBaby",
            TITLE,
            MESSAGE,
        ],
        // Inside the Flatpak sandbox: the notification portal.
        &[
            "gdbus",
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Notification.AddNotification",
            "gmnb-cpu",
            &portal,
        ],
        &["zenity", "--error", "--title=GMNB", "--text", MESSAGE],
        &["kdialog", "--title", "GMNB", "--error", MESSAGE],
    ];
    for cmd in attempts {
        let ok = Command::new(cmd[0])
            .args(&cmd[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_real_binary_beside_it() {
        let dir = std::env::temp_dir().join(format!("gmnb-launcher-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let me = dir.join("bin/gmnb");
        assert_eq!(real_binary(&me), None);
        std::fs::create_dir_all(dir.join("lib/gmnb")).unwrap();
        std::fs::write(dir.join("lib/gmnb/gmnb"), b"").unwrap();
        assert_eq!(real_binary(&me), Some(dir.join("lib/gmnb/gmnb")));
        // libexec first, where both exist.
        std::fs::create_dir_all(dir.join("libexec/gmnb")).unwrap();
        std::fs::write(dir.join("libexec/gmnb/gmnb"), b"").unwrap();
        assert_eq!(real_binary(&me), Some(dir.join("libexec/gmnb/gmnb")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_message_fits_a_gvariant_string() {
        // The portal's text is single-quoted GVariant text.
        assert!(!MESSAGE.contains('\'') && !TITLE.contains('\''));
    }
}
