//! The engine against a stand-in sound server (`src/standin.rs`): a few
//! threads that speak enough of PulseAudio's protocol to take one playback
//! stream, ask it for audio while it runs, and note what the engine does. Nothing
//! is played and no real sound server is involved: the socket is in a
//! folder of the test's own.
//!
//! This is where the stream's rules are held in `cargo test`: created
//! corked, given nothing while corked, audio in front of the request to
//! start, corked and flushed 100 ms after the last voice; a play that
//! arrives while the server is slow to answer; a server that goes away and
//! comes back.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gmnb_sound::{
    Bank, Env, Glide, Layer, PlayParams, Played, Recipe, Settings, Sink, SoundEngine, SoundId,
    SoundSource, Status, ThemeSounds,
};

// The stand-in server (also the `stand-in` feature's `gmnb_sound::standin`,
// for the app's tests).
#[allow(dead_code)]
#[path = "../src/standin.rs"]
mod standin;
use standin::{Saw, Server};

// ---- The tests' own things -----------------------------------------------------

type Lines = Arc<Mutex<Vec<String>>>;

fn scratch(name: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "gmnb-sound-server-{}-{name}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn settings(socket: &Path, lines: &Lines) -> Settings {
    let lines = lines.clone();
    Settings {
        volume: 1.0,
        sink: Sink::Socket {
            path: socket.to_path_buf(),
            cookie: vec![0; 256],
        },
        retry: Duration::from_millis(200),
        log: Some(Arc::new(move |line: &str| {
            lines.lock().unwrap().push(line.to_string())
        })),
        ..Settings::default()
    }
}

/// A 50 ms tone and a 3 s one.
fn sounds() -> ThemeSounds {
    let tone = |seconds: f64| {
        SoundSource::Synth(Recipe {
            gain: 0.5,
            layers: vec![Layer {
                freq: Some(Glide::fixed(440.0)),
                env: Env {
                    attack: 0.0,
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

fn ready(engine: &mut SoundEngine) -> Bank {
    let bank = engine.load(&sounds());
    assert!(bank.wait_settled(Duration::from_secs(20)));
    assert!(
        until(Duration::from_secs(10), || engine.status() == Status::Ready),
        "{:?}",
        engine.unavailable_reason()
    );
    bank
}

/// Plays `sound` until one play is in time to become a voice.
fn start(engine: &SoundEngine, bank: &Bank, sound: u32) {
    for _ in 0..50 {
        let before = engine.stats();
        assert_eq!(
            engine.play(bank, SoundId(sound), PlayParams::default()),
            Played::Queued
        );
        assert!(until(Duration::from_secs(10), || {
            let now = engine.stats();
            now.started + now.skipped > before.started + before.skipped
        }));
        if engine.stats().started > before.started {
            return;
        }
    }
    panic!("no play was ever in time");
}

#[test]
fn the_stream_is_made_corked_fed_while_in_use_and_corked_again() {
    let dir = scratch("stream");
    let server = Server::start(&dir.join("native"));
    let lines = Lines::default();
    let mut engine = SoundEngine::new(settings(&dir.join("native"), &lines));
    let bank = ready(&mut engine);

    // Created corked, and given nothing while it rests.
    assert_eq!(server.saw(), [Saw::Stream { corked: true }]);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(server.saw().len(), 1);
    let stats = engine.stats();
    assert_eq!((stats.fed_frames, stats.uncorks, stats.connects), (0, 0, 1));

    // A play: the stream is started, and what comes is the sound.
    start(&engine, &bank, 1);
    assert!(until(Duration::from_secs(5), || server.audio() > 0));
    let saw = server.saw();
    assert_eq!(saw.iter().filter(|s| **s == Saw::Cork(false)).count(), 1);
    let first = saw.iter().find(|s| matches!(s, Saw::Audio { .. })).unwrap();
    assert!(
        matches!(first, Saw::Audio { sound: true, .. }),
        "the stream's first audio was silence: {first:?}"
    );

    // The sound, 100 ms of silence, then cork and flush, in that order.
    assert!(until(Duration::from_secs(5), || {
        server.count(|s| *s == Saw::Flush) == 1
    }));
    let saw = server.saw();
    let at = |what: &Saw| saw.iter().rposition(|s| s == what).unwrap();
    assert!(at(&Saw::Cork(true)) < at(&Saw::Flush));
    // 50 ms of sound and 100 ms of silence: 150 ms, give or take a request.
    let bytes = server.audio();
    assert!((27_000..=34_000).contains(&bytes), "{bytes} bytes");
    assert_eq!(engine.stats().fed_frames as usize * 4, bytes);

    // At rest again: nothing more is sent, and the sound thread sleeps.
    std::thread::sleep(Duration::from_millis(100));
    let (events, wakes) = (server.saw().len(), engine.stats().wakes);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(server.saw().len(), events, "{:?}", &server.saw()[events..]);
    assert_eq!(engine.stats().wakes, wakes);

    // Unloading deletes the stream and hangs up.
    engine.unload();
    assert!(until(Duration::from_secs(5), || {
        server.saw().last() == Some(&Saw::Closed)
    }));
    assert!(server.saw().contains(&Saw::Delete));
    assert!(
        lines.lock().unwrap().is_empty(),
        "{:?}",
        lines.lock().unwrap()
    );
    drop(engine);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_play_while_the_server_is_slow_to_answer_is_not_forgotten() {
    let dir = scratch("slow");
    let server = Server::start(&dir.join("native"));
    let lines = Lines::default();
    let mut engine = SoundEngine::new(settings(&dir.join("native"), &lines));
    let bank = ready(&mut engine);
    start(&engine, &bank, 1);
    // From here on the server takes 20 ms over a cork and over a flush.
    *server.slow.lock().unwrap() = Duration::from_millis(20);
    // The moment the sound thread starts to cork the stream, another play
    // arrives: its wake-up comes while the thread is waiting for the
    // server's answer.
    assert!(until(Duration::from_secs(5), || engine.stats().corks == 1));
    let before = engine.stats();
    assert_eq!(
        engine.play(&bank, SoundId(1), PlayParams::default()),
        Played::Queued
    );
    // It is taken up as soon as the answer is in (played, or skipped as
    // late if this machine dawdled), not left in the queue until some
    // later wake-up.
    assert!(
        until(Duration::from_secs(1), || {
            let now = engine.stats();
            now.started + now.skipped > before.started + before.skipped
        }),
        "the play was left waiting: {:?}",
        engine.stats()
    );
    *server.slow.lock().unwrap() = Duration::ZERO;

    // And the same for being told to stop: dropped while the server is
    // slow, the engine's thread still hears it.
    start(&engine, &bank, 1);
    *server.slow.lock().unwrap() = Duration::from_millis(20);
    let corks = engine.stats().corks;
    assert!(until(Duration::from_secs(5), || engine.stats().corks > corks));
    drop(engine);
    // (A thread that never heard it is given up on after 5 s, with a line
    // saying so.)
    let said = lines.lock().unwrap().clone();
    assert!(!said.iter().any(|l| l.contains("stuck")), "{said:?}");
    drop(server);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_server_that_goes_away_and_comes_back() {
    let dir = scratch("gone");
    let socket = dir.join("native");
    let server = Server::start(&socket);
    let lines = Lines::default();
    let said = |part: &str| {
        lines
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.contains(part))
            .count()
    };
    let mut engine = SoundEngine::new(settings(&socket, &lines));
    let bank = ready(&mut engine);

    // Gone while the stream rests.
    server.stop();
    assert!(until(Duration::from_secs(5), || {
        engine.status() == Status::Unavailable
    }));
    assert_eq!(said("no sound system found"), 1);
    assert_eq!(
        engine.play(&bank, SoundId(1), PlayParams::default()),
        Played::Unavailable
    );
    // It rests: no wake-ups, no attempts by itself.
    std::thread::sleep(Duration::from_millis(100));
    let (wakes, connects) = (engine.stats().wakes, engine.stats().connects);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(engine.stats().wakes, wakes);
    assert_eq!(engine.stats().connects, connects);

    // Back: the next play after the interval finds it, and the one after
    // that sounds.
    let server = Server::start(&socket);
    assert!(until(Duration::from_secs(10), || {
        engine.play(&bank, SoundId(1), PlayParams::default());
        std::thread::sleep(Duration::from_millis(20));
        engine.status() == Status::Ready
    }));
    assert_eq!(said("theme sounds: sound system found"), 1);
    assert_eq!(
        server.count(|s| matches!(s, Saw::Stream { corked: true })),
        1
    );
    start(&engine, &bank, 2);
    assert!(until(Duration::from_secs(5), || server.audio() > 9600));

    // Gone in the middle of that 3 s sound: silence, and one more line.
    server.stop();
    assert!(until(Duration::from_secs(5), || {
        engine.status() == Status::Unavailable
    }));
    assert_eq!(engine.stats().voices, 0);
    assert_eq!(said("no sound system found"), 2);
    assert_eq!(
        lines.lock().unwrap().len(),
        3,
        "{:?}",
        lines.lock().unwrap()
    );
    drop(engine);
    std::fs::remove_dir_all(&dir).unwrap();
}
