//! The engine as the app sets it up (`Settings::from_env`: the session's
//! sound server, found from the environment), through a sound server's
//! life: there, gone while the stream rests, back, gone in the middle of a
//! sound. And with no server at all.
//!
//! Runs ONLY inside a sound sandbox (the design's
//! `theming-work/media/audiobox.sh`): no sound hardware, none of the
//! user's sound servers, its own process ids. The server it stops and
//! starts is the sandbox's private `pipewire-pulse`. Anywhere else it
//! refuses and exits.
//!
//!     audiobox.sh target/release/examples/lifecycle
//!     AUDIOBOX_NO_SERVER=1 audiobox.sh target/release/examples/lifecycle

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gmnb_sound::{
    Env, Glide, Layer, PlayParams, Played, Recipe, Settings, Sink, SoundEngine, SoundId,
    SoundSource, Status, ThemeSounds,
};

fn until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while !done() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    true
}

fn cpu_ms() -> f64 {
    let mut total = 0u64;
    if let Ok(dir) = std::fs::read_dir("/proc/self/task") {
        for e in dir.flatten() {
            if let Ok(s) = std::fs::read_to_string(e.path().join("schedstat")) {
                total += s
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
            }
        }
    }
    total as f64 / 1e6
}

fn threads() -> usize {
    std::fs::read_dir("/proc/self/task").map_or(0, |d| d.count())
}

/// The process ids of the sandbox's `pipewire-pulse`.
fn servers() -> Vec<u32> {
    let mut found = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for e in dir.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
                continue;
            };
            let name = std::fs::read_to_string(e.path().join("comm")).unwrap_or_default();
            if name.trim() == "pipewire-pulse" {
                found.push(pid);
            }
        }
    }
    found
}

/// Stops the sandbox's sound server (by process id) and waits for it to
/// be gone. `ours` is the one this program started, if it did.
fn stop_server(ours: Option<&mut Child>) {
    for pid in servers() {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
    if let Some(child) = ours {
        let _ = child.wait();
    }
    assert!(until(Duration::from_secs(5), || servers().is_empty()));
}

/// Plays `sound` and waits until it has become a voice. A play the sound
/// thread got to more than 50 ms late is dropped by design (the sandbox
/// shares a capped processor); it is then played again, and counted.
fn start(engine: &SoundEngine, bank: &gmnb_sound::Bank, sound: SoundId) -> u64 {
    let mut late = 0;
    loop {
        let before = engine.stats();
        assert_eq!(
            engine.play(bank, sound, PlayParams::default()),
            Played::Queued
        );
        assert!(until(Duration::from_secs(10), || {
            let now = engine.stats();
            now.started + now.skipped > before.started + before.skipped
        }));
        if engine.stats().started > before.started {
            return late;
        }
        late += 1;
        assert!(late < 20, "no play was ever in time");
    }
}

fn start_server(socket: &Path) -> Child {
    let _ = std::fs::remove_file(socket);
    let child = Command::new("pipewire-pulse")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("pipewire-pulse");
    assert!(until(Duration::from_secs(10), || socket.exists()));
    std::thread::sleep(Duration::from_millis(500));
    child
}

fn sounds() -> ThemeSounds {
    let tone = |seconds: f64| {
        SoundSource::Synth(Recipe {
            gain: 0.5,
            layers: vec![Layer {
                freq: Some(Glide::fixed(440.0)),
                env: Env {
                    attack: 0.005,
                    decay: seconds,
                    ..Env::default()
                },
                ..Layer::default()
            }],
            ..Recipe::default()
        })
    };
    ThemeSounds {
        sounds: vec![tone(0.05), tone(3.0)],
        ..ThemeSounds::default()
    }
}

fn main() {
    let sandboxed = std::env::var_os("AUDIOBOX_PRIV").is_some()
        && std::fs::read_dir("/dev/snd").map_or(true, |mut d| d.next().is_none());
    if !sandboxed {
        eprintln!("lifecycle: not in a sound sandbox (audiobox.sh); refusing to run");
        std::process::exit(2);
    }
    let dir = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR");
    let socket = Path::new(&dir).join("pulse/native");
    let lines: Arc<Mutex<Vec<String>>> = Arc::default();
    let said = lines.clone();
    // What the app does: the session's server, from the environment.
    let settings = Settings {
        retry: Duration::from_millis(500),
        log: Some(Arc::new(move |line: &str| {
            println!("  stderr line: {line}");
            said.lock().unwrap().push(line.to_string());
        })),
        ..Settings::from_env(true, 1.0)
    };
    assert_eq!(settings.sink, Sink::Session);
    let count = || lines.lock().unwrap().len();
    let (click, long) = (SoundId(1), SoundId(2));

    if std::env::var_os("AUDIOBOX_NO_SERVER").is_some() {
        // The chapter's "no device" test: XDG_RUNTIME_DIR is an empty
        // folder.
        println!("no sound server, no sound hardware:");
        let (cpu, t) = (cpu_ms(), Instant::now());
        let mut engine = SoundEngine::new(settings);
        let bank = engine.load(&sounds());
        assert!(bank.wait_settled(Duration::from_secs(10)));
        assert!(until(Duration::from_secs(5), || engine.status()
            == Status::Unavailable));
        println!(
            "  unavailable after {:.1} ms: {:?}",
            t.elapsed().as_secs_f64() * 1e3,
            engine.unavailable_reason()
        );
        for _ in 0..100 {
            assert_eq!(
                engine.play(&bank, click, PlayParams::default()),
                Played::Unavailable
            );
        }
        println!(
            "  loading and 100 plays: {:.2} ms of processor time, {} line(s), {} connection attempt(s)",
            cpu_ms() - cpu,
            count(),
            engine.stats().connects
        );
        let (cpu, wakes) = (cpu_ms(), engine.stats().wakes);
        std::thread::sleep(Duration::from_secs(2));
        println!(
            "  the next 2 s: {:.3} ms of processor time, {} wake-up(s) of the sound thread, {} threads",
            cpu_ms() - cpu,
            engine.stats().wakes - wakes,
            threads()
        );
        assert_eq!(count(), 1);
        drop(engine);
        println!("  dropped: {} thread(s)", threads());
        return;
    }

    println!("the session's server, found from the environment:");
    let mut engine = SoundEngine::new(settings);
    let bank = engine.load(&sounds());
    assert!(bank.wait_settled(Duration::from_secs(10)));
    assert!(until(Duration::from_secs(5), || engine.status() == Status::Ready));
    start(&engine, &bank, click);
    assert!(until(Duration::from_secs(3), || engine.stats().corks == 1));
    let s = engine.stats();
    println!(
        "  ready; one click: {} frames given to the server in {} requests; corked after it; {} line(s)",
        s.fed_frames,
        s.requests,
        count()
    );
    assert!(s.fed_frames > 4800 && count() == 0);

    println!("the server stops while the stream rests:");
    let t = Instant::now();
    stop_server(None);
    let gone = until(Duration::from_secs(5), || {
        engine.status() == Status::Unavailable
    });
    println!(
        "  unavailable: {gone} ({:.0} ms after the kill): {:?}; {} line(s)",
        t.elapsed().as_secs_f64() * 1e3,
        engine.unavailable_reason(),
        count()
    );
    assert!(gone && count() == 1);
    // Plays go nowhere; inside the retry interval they don't even look.
    std::thread::sleep(Duration::from_millis(600));
    let connects = engine.stats().connects;
    for _ in 0..20 {
        assert_eq!(
            engine.play(&bank, click, PlayParams::default()),
            Played::Unavailable
        );
    }
    assert!(until(Duration::from_secs(3), || engine.stats().connects
        == connects + 1));
    std::thread::sleep(Duration::from_millis(100));
    let (cpu, wakes) = (cpu_ms(), engine.stats().wakes);
    std::thread::sleep(Duration::from_secs(2));
    println!(
        "  20 plays made one more attempt ({} in all); then 2 s: {:.3} ms of processor time, {} wake-up(s), {} threads, {} line(s)",
        engine.stats().connects,
        cpu_ms() - cpu,
        engine.stats().wakes - wakes,
        threads(),
        count()
    );
    assert_eq!(count(), 1);

    println!("the server comes back:");
    let mut server = start_server(&socket);
    // A play after the retry interval looks again (and is itself dropped).
    assert_eq!(
        engine.play(&bank, click, PlayParams::default()),
        Played::Unavailable
    );
    let back = until(Duration::from_secs(5), || engine.status() == Status::Ready);
    let fed = engine.stats().fed_frames;
    println!("  ready again: {back}; {} line(s)", count());
    assert!(back && count() == 2);
    start(&engine, &bank, click);
    assert!(until(Duration::from_secs(3), || engine.stats().fed_frames
        > fed + 4800));
    println!(
        "  the next click went out: {} frames to the new connection",
        engine.stats().fed_frames - fed
    );
    assert!(until(Duration::from_secs(3), || {
        let s = engine.stats();
        s.corks == s.uncorks
    }));

    println!("the server stops in the middle of a sound:");
    let late = start(&engine, &bank, long);
    std::thread::sleep(Duration::from_millis(300));
    println!(
        "  a 3 s sound is going ({late} play(s) came too late and were dropped): {} voice(s)",
        engine.stats().voices
    );
    assert_eq!(engine.stats().voices, 1);
    let t = Instant::now();
    stop_server(Some(&mut server));
    let gone = until(Duration::from_secs(5), || {
        engine.status() == Status::Unavailable
    });
    println!(
        "  unavailable: {gone} ({:.0} ms after the kill); voices left: {}; {} line(s)",
        t.elapsed().as_secs_f64() * 1e3,
        engine.stats().voices,
        count()
    );
    assert!(gone && count() == 3 && engine.stats().voices == 0);
    std::thread::sleep(Duration::from_millis(100));
    let (cpu, wakes) = (cpu_ms(), engine.stats().wakes);
    std::thread::sleep(Duration::from_secs(2));
    println!(
        "  then 2 s: {:.3} ms of processor time, {} wake-up(s), {} threads",
        cpu_ms() - cpu,
        engine.stats().wakes - wakes,
        threads()
    );

    println!("and back once more, then the theme is unloaded:");
    let mut server = start_server(&socket);
    std::thread::sleep(Duration::from_millis(500));
    engine.play(&bank, click, PlayParams::default());
    assert!(until(Duration::from_secs(5), || engine.status() == Status::Ready));
    start(&engine, &bank, long);
    std::thread::sleep(Duration::from_millis(200));
    engine.unload();
    let gone = until(Duration::from_secs(3), || threads() == 1);
    println!(
        "  unloaded while sounding: {} thread(s) left ({}); {} line(s) in all",
        threads(),
        if gone { "all gone" } else { "NOT all gone" },
        count()
    );
    drop(engine);
    stop_server(Some(&mut server));
}
