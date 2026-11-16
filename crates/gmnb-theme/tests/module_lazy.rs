//! Nothing of wasmtime exists until a theme with modules is applied
//! (docs/theming.md, "Modules"): the base look, the built-in themes that
//! have no module (Aurora, Sprinkles), Swirls listed but not applied,
//! Settings' listing of a theme with modules, and a theme that uses a
//! module at the top of its file make no engine, install no signal
//! handler, make no timer and start no thread; Swirls applied makes one
//! compile of its paint. One test in a process of its own (this file),
//! since the engine is the process's.

use std::time::Duration;

use gmnb_theme::layer::{Frame, LayerKind};
use gmnb_theme::module::{self, Stats};
use gmnb_theme::theme::{self, Source, Theme};
use gmnb_theme::{Ctx, Limits};

/// What a signal's handler is now: `SIG_DFL` if none was installed.
fn handler(signal: libc::c_int) -> usize {
    // SAFETY: reads the handler into a local struct, changing nothing.
    unsafe {
        let mut old: libc::sigaction = std::mem::zeroed();
        libc::sigaction(signal, std::ptr::null(), &mut old);
        old.sa_sigaction
    }
}

const PAINT: &str = r#"
(module
  (memory (export "memory") 1)
  (func (export "bump") (result i32)
    (i32.store (i32.const 0) (i32.add (i32.load (i32.const 0)) (i32.const 1)))
    (i32.load (i32.const 0))))
"#;

#[test]
fn nothing_is_made_until_a_theme_with_modules_is_applied() {
    let none = Stats::default();
    let signals = [libc::SIGILL, libc::SIGFPE, libc::SIGRTMIN() + 3];
    for s in signals {
        assert_eq!(handler(s), libc::SIG_DFL, "signal {s} before");
    }
    let limits = Limits {
        load: Duration::from_secs(20),
        style: Duration::from_secs(20),
        frame: Duration::from_secs(20),
        ..Limits::default()
    };

    // The built-in themes without modules, applied and drawn; and Swirls,
    // which has one, loaded and let go without being applied.
    for b in &theme::BUILT_IN {
        let mut t = Theme::load(Source::built_in(b.id).unwrap(), limits.clone()).unwrap();
        if b.id == "swirls" {
            assert_eq!(t.modules(), ["paint.wasm"]);
            continue;
        }
        assert!(t.modules().is_empty());
        assert!(t.prepare_modules().is_none());
        assert!(t.style(&Ctx::default()).is_ok());
        for kind in LayerKind::ALL {
            let _ = t.draw(kind, &Ctx::default(), &Frame::sized(760.0, 640.0));
        }
    }
    assert_eq!(module::stats(), none);

    // A theme with a module, listed as Settings lists it.
    let dir = std::env::temp_dir().join(format!("gmnb-module-lazy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("paint.wasm"), wat::parse_str(PAINT).unwrap()).unwrap();
    let file = |source: &str| std::fs::write(dir.join("theme.luau"), source).unwrap();
    file(
        r#"
        local paint = gmnb.module("paint.wasm")
        local picture = paint:picture()
        return { api = 1, name = "Modules", layers = { background = { draw = function()
            return paint:call("bump")
        end } } }
        "#,
    );
    let source = Source::folder("m", &dir);
    let listed = theme::list(std::slice::from_ref(&source), &limits);
    assert_eq!(listed[0].meta.as_ref().unwrap().name, "Modules");
    // Loaded, and dropped without being applied.
    let loaded = Theme::load(source.clone(), limits.clone()).unwrap();
    assert_eq!(loaded.modules(), ["paint.wasm"]);
    drop(loaded);
    assert_eq!(module::stats(), none);

    // Used at the top of its file: it doesn't load, and nothing is made.
    let applied = std::fs::read_to_string(dir.join("theme.luau")).unwrap();
    file(&format!(
        "gmnb.module('paint.wasm'):call('bump')\n{applied}"
    ));
    assert!(Theme::load(source.clone(), limits.clone()).is_err());
    assert_eq!(module::stats(), none);
    for s in signals {
        assert_eq!(handler(s), libc::SIG_DFL, "signal {s} with no module run");
    }

    // Applied: the engine (and wasmtime's handlers), one compile on one
    // thread of its own, one instance; then a call, with this thread's
    // timer.
    file(&applied);
    let mut t = Theme::load(source.clone(), limits.clone()).unwrap();
    let job = t.prepare_modules().unwrap();
    assert_eq!(module::stats(), Stats { engines: 1, ..none });
    assert_ne!(handler(libc::SIGILL), libc::SIG_DFL, "wasmtime's handler");
    assert_eq!(handler(libc::SIGRTMIN() + 3), libc::SIG_DFL);
    let (tx, rx) = std::sync::mpsc::channel();
    job.spawn(move |done| tx.send(done).unwrap())
        .unwrap_or_else(|_| panic!("no compile thread"))
        .join()
        .unwrap();
    t.install_modules(rx.recv().unwrap());
    let drawn = t
        .draw(
            LayerKind::Background,
            &Ctx::default(),
            &Frame::sized(10.0, 10.0),
        )
        .unwrap();
    assert_eq!(drawn.again, Some(1.0));
    assert_eq!(
        module::stats(),
        Stats {
            engines: 1,
            compiles: 1,
            instances: 1,
            calls: 1,
            timeouts: 0,
            timers: 1,
            threads: 1,
        }
    );
    assert_ne!(handler(libc::SIGRTMIN() + 3), libc::SIG_DFL, "the timer's");
    // Applied again (Reload): taken from memory, a fresh instance.
    let mut again = Theme::load(source, limits.clone()).unwrap();
    assert!(again.prepare_modules().is_none());
    assert_eq!(module::stats().compiles, 1);
    assert_eq!(module::stats().instances, 2);
    assert_eq!(module::stats().engines, 1);
    let _ = std::fs::remove_dir_all(&dir);

    // Swirls applied (`load_ready`, which the tools and tests use): its
    // paint compiled once, on a thread of its own, and started; drawn, it
    // calls it. Still the one engine.
    let swirls = Theme::load_ready(Source::built_in("swirls").unwrap(), limits).unwrap();
    assert!(swirls.modules_ready());
    let applied = module::stats();
    assert_eq!(
        (
            applied.engines,
            applied.compiles,
            applied.instances,
            applied.threads
        ),
        (1, 2, 3, 2)
    );
    swirls
        .draw(
            LayerKind::Background,
            &Ctx::default(),
            &Frame::sized(760.0, 640.0),
        )
        .unwrap();
    assert!(module::stats().calls > applied.calls);
    assert_eq!(module::stats().timeouts, 0);
}
