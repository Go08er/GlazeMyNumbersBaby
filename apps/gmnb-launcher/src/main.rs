//! `gmnb` as installed: checks the CPU, then runs the real GMNB.
//!
//! On x86-64 GMNB is built for x86-64-v3, the level with AVX2 (Intel Core
//! from Haswell, 2013, and AMD from Excavator, 2015, but not every Pentium,
//! Celeron or Atom since), so any other CPU would kill it with an illegal
//! instruction before it could say why. This launcher is built for any
//! x86-64 and says why instead: on stderr, and to someone who started GMNB
//! from their desktop with a notification or a dialog.
//!
//! The real binary lives beside it, as `../libexec/gmnb/gmnb` (or
//! `../lib/gmnb/gmnb`, Arch's place for such programs), and is exec'd with
//! the same arguments and environment.

use std::ffi::OsString;
use std::io::IsTerminal;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

const MESSAGE: &str = "GMNB needs a CPU with AVX2 (x86-64-v3): Intel Core from Haswell (2013) or AMD from Excavator (2015) on, though not every Pentium, Celeron or Atom. DGMNB runs on any 64-bit PC.";
const TITLE: &str = "GMNB needs a CPU with AVX2";

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

/// What x86-64-v3 code needs: AVX, AVX2, BMI1, BMI2, F16C, FMA, LZCNT,
/// MOVBE and XSAVE, on top of x86-64-v2's CMPXCHG16B, POPCNT, SSE3, SSSE3,
/// SSE4.1 and SSE4.2.
#[cfg(any(target_arch = "x86_64", test))]
const V3: [&str; 15] = [
    "avx",
    "avx2",
    "bmi1",
    "bmi2",
    "f16c",
    "fma",
    "lzcnt",
    "movbe",
    "xsave",
    "cmpxchg16b",
    "popcnt",
    "sse3",
    "ssse3",
    "sse4.1",
    "sse4.2",
];

/// Whether a CPU with the features `has` reports runs x86-64-v3 code.
#[cfg(any(target_arch = "x86_64", test))]
fn supports_v3(has: impl Fn(&str) -> Option<bool>) -> bool {
    V3.iter().all(|f| has(f) == Some(true))
}

#[cfg(target_arch = "x86_64")]
fn cpu_ok() -> bool {
    supports_v3(detected)
}

/// Whether this CPU has `feature` (one of [`V3`]; `None` for any other
/// name). The AVX check includes the operating system saving the AVX
/// registers.
#[cfg(target_arch = "x86_64")]
fn detected(feature: &str) -> Option<bool> {
    use std::arch::is_x86_feature_detected as has;
    Some(match feature {
        "avx" => has!("avx"),
        "avx2" => has!("avx2"),
        "bmi1" => has!("bmi1"),
        "bmi2" => has!("bmi2"),
        "f16c" => has!("f16c"),
        "fma" => has!("fma"),
        "lzcnt" => has!("lzcnt"),
        "movbe" => has!("movbe"),
        "xsave" => has!("xsave"),
        "cmpxchg16b" => has!("cmpxchg16b"),
        "popcnt" => has!("popcnt"),
        "sse3" => has!("sse3"),
        "ssse3" => has!("ssse3"),
        "sse4.1" => has!("sse4.1"),
        "sse4.2" => has!("sse4.2"),
        _ => return None,
    })
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

    /// A CPU reporting `flags` (as /proc/cpuinfo names them, which match
    /// Rust's names for these features apart from SSE3 and SSE4.x).
    fn cpu(flags: &str) -> impl Fn(&str) -> Option<bool> {
        let flags: Vec<String> = flags
            .split_whitespace()
            .map(|f| match f {
                "pni" => "sse3".into(),
                "sse4_1" => "sse4.1".into(),
                "sse4_2" => "sse4.2".into(),
                "abm" => "lzcnt".into(),
                "cx16" => "cmpxchg16b".into(),
                f => f.into(),
            })
            .collect();
        move |f| Some(flags.iter().any(|x| x == f))
    }

    // The x86-64-relevant flags of each CPU, from /proc/cpuinfo.
    const HASWELL: &str = "fpu sse sse2 pni ssse3 fma cx16 sse4_1 sse4_2 movbe popcnt xsave avx \
                           f16c abm bmi1 avx2 bmi2";
    const NEHALEM: &str = "fpu sse sse2 pni ssse3 cx16 sse4_1 sse4_2 popcnt";
    const SANDY_BRIDGE: &str = "fpu sse sse2 pni ssse3 cx16 sse4_1 sse4_2 popcnt xsave avx";
    // A Gemini Lake Celeron (Goldmont Plus, 2017-19): MOVBE, but no AVX.
    const GOLDMONT_PLUS: &str = "fpu sse sse2 pni ssse3 cx16 sse4_1 sse4_2 movbe popcnt xsave";

    #[test]
    fn x86_64_v3_cpus_pass_and_older_ones_fail() {
        assert!(supports_v3(cpu(HASWELL)));
        for old in [NEHALEM, SANDY_BRIDGE, GOLDMONT_PLUS] {
            assert!(!supports_v3(cpu(old)), "{old}");
        }
        // Each feature on its own stops it.
        for missing in V3 {
            let has = cpu(HASWELL);
            assert!(!supports_v3(|f| if f == missing {
                Some(false)
            } else {
                has(f)
            }));
        }
        // So does a name the check doesn't know.
        assert!(!supports_v3(|_| None));
    }

    /// Every feature the list names is one the detector checks (a typo
    /// would make every CPU fail).
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn the_detector_knows_every_feature() {
        for f in V3 {
            assert!(detected(f).is_some(), "{f}");
        }
        assert_eq!(detected("avx512f"), None);
    }

    #[test]
    fn the_message_fits_a_gvariant_string() {
        // The portal's text is single-quoted GVariant text.
        assert!(!MESSAGE.contains('\'') && !TITLE.contains('\''));
    }
}
