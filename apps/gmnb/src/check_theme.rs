//! `gmnb --check-theme FILE`: loads a theme script in the sandbox it will
//! run in, prints what it returns (or why it failed) and what it printed,
//! and exits. Needs no display.

use std::io::Read;
use std::path::Path;

use gmnb_theme::{Engine, Limits};
use gtk::glib;

/// The exit code, if GMNB was started with `--check-theme`.
pub fn from_args() -> Option<glib::ExitCode> {
    let mut args = std::env::args_os().skip(1);
    if args.next()? != "--check-theme" {
        return None;
    }
    let (Some(path), None) = (args.next(), args.next()) else {
        eprintln!("usage: gmnb --check-theme FILE");
        return Some(glib::ExitCode::from(2));
    };
    Some(check(Path::new(&path)))
}

fn check(path: &Path) -> glib::ExitCode {
    let limits = Limits::default();
    // One byte past the cap is enough to be refused for size.
    let mut source = Vec::new();
    let read = std::fs::File::open(path).and_then(|f| {
        f.take(limits.source_bytes as u64 + 1)
            .read_to_end(&mut source)
    });
    if let Err(e) = read {
        eprintln!("gmnb: {}: {e}", path.display());
        return glib::ExitCode::from(2);
    }
    let result = Engine::new(limits).and_then(|engine| {
        let value = engine.load(&format!("@{}", path.display()), &source);
        let log = engine.take_log();
        if log.dropped > 0 {
            eprintln!("print: ({} earlier lines dropped)", log.dropped);
        }
        for line in log.lines {
            eprintln!("print: {line}");
        }
        value
    });
    match result {
        Ok(value) => {
            println!("{value}");
            glib::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("gmnb: {}: {e}", path.display());
            glib::ExitCode::FAILURE
        }
    }
}
