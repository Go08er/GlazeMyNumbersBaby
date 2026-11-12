//! Switching sound off, changing the theme or the volume while the sound
//! thread is busy rendering a theme's sounds: what is sounding must not
//! wait for the render to finish (R4-D-1). Against the stand-in server
//! (`src/standin.rs`), in a folder of the test's own: no real sound
//! device is involved.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use gmnb_sound::{
    Bank, Curve, Effect, Env, Filter, FilterKind, Fm, Glide, Layer, Lfo, PlayParams, Played,
    Recipe, Settings, Sink, SoundEngine, SoundId, SoundSource, SoundState, Status, ThemeSounds,
    Wave,
};

#[allow(dead_code)]
#[path = "../src/standin.rs"]
mod standin;
use standin::{Saw, Server};

fn scratch(name: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "gmnb-sound-off-{}-{name}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn settings(socket: &Path) -> Settings {
    Settings {
        volume: 1.0,
        sink: Sink::Socket {
            path: socket.to_path_buf(),
            cookie: vec![0; 256],
        },
        log: Some(std::sync::Arc::new(|_: &str| {})),
        ..Settings::default()
    }
}

fn until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while !done() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    true
}

/// A 3 s tone.
fn tone() -> SoundSource {
    SoundSource::Synth(Recipe {
        gain: 0.5,
        layers: vec![Layer {
            env: Env {
                attack: 0.0,
                decay: 3.0,
                ..Env::default()
            },
            ..Layer::default()
        }],
        ..Recipe::default()
    })
}

/// The most a recipe may ask for: it takes the renderer a long while.
fn heavy(seed: u32) -> SoundSource {
    let layer = |i: usize| Layer {
        wave: [Wave::Saw, Wave::Square, Wave::Noise, Wave::Sine][i % 4],
        freq: Some(Glide::exp(100.0, 8000.0, 4.0)),
        seed: seed + i as u32,
        fm: Some(Fm {
            ratio: 3.0,
            index: Glide::lin(0.0, 10.0, 4.0),
        }),
        vibrato: Some(Lfo {
            rate: 5.0,
            depth: 50.0,
        }),
        tremolo: Some(Lfo {
            rate: 7.0,
            depth: 0.5,
        }),
        filter: Some(Filter {
            kind: FilterKind::Bandpass,
            freq: Glide::exp(200.0, 9000.0, 4.0),
            q: 20.0,
        }),
        env: Env {
            attack: 0.0,
            decay: 4.0,
            curve: Curve::Exp,
            ..Env::default()
        },
        ..Layer::default()
    };
    SoundSource::Synth(Recipe {
        layers: (0..8).map(layer).collect(),
        effects: vec![
            Effect::Reverb {
                size: 1.0,
                damp: 0.5,
                mix: 0.5,
            },
            Effect::Delay {
                time: 0.5,
                feedback: 0.9,
                mix: 0.5,
                ping_pong: true,
            },
            Effect::Filter(Filter {
                kind: FilterKind::Lowpass,
                freq: Glide::exp(100.0, 9000.0, 4.0),
                q: 10.0,
            }),
            Effect::Crush {
                bits: 4.0,
                rate: 8000.0,
            },
        ],
        ..Recipe::default()
    })
}

/// Loads a tone, plays it, and then has the theme make three sounds that
/// take a long time to render (as a theme's `sound` function may, in the
/// middle of use); returns with the sound thread busy rendering them.
fn sounding(engine: &mut SoundEngine) -> Bank {
    let first = vec![tone()];
    let bank = engine.load(&ThemeSounds {
        sounds: first.clone(),
        ..ThemeSounds::default()
    });
    assert!(until(Duration::from_secs(20), || {
        bank.state(SoundId(1)) == SoundState::Ready && engine.status() == Status::Ready
    }));
    for _ in 0..50 {
        let before = engine.stats();
        assert_eq!(
            engine.play(&bank, SoundId(1), PlayParams::default()),
            Played::Queued
        );
        assert!(until(Duration::from_secs(30), || {
            let now = engine.stats();
            now.started + now.skipped > before.started + before.skipped
        }));
        if engine.stats().started > before.started {
            break;
        }
    }
    assert_eq!(engine.stats().voices, 1);
    let mut more = first;
    more.extend([heavy(1), heavy(100), heavy(200)]);
    engine.extend(&bank, &more);
    // The thread is now rendering, for far longer than the 3 s tone's
    // fade should be kept waiting.
    std::thread::sleep(Duration::from_millis(30));
    assert!(!bank.is_settled(), "the thread should be rendering");
    bank
}

/// When the stand-in server was last handed a sound (not silence).
fn last_sound(server: &Server) -> Option<Instant> {
    server
        .timed()
        .into_iter()
        .rev()
        .find(|(_, saw)| matches!(saw, Saw::Audio { sound: true, .. }))
        .map(|(at, _)| at)
}

#[test]
fn switching_sound_off_does_not_wait_for_a_render() {
    let dir = scratch("off");
    let server = Server::start(&dir.join("native"));
    let mut engine = SoundEngine::new(settings(&dir.join("native")));
    let _bank = sounding(&mut engine);
    assert!(until(Duration::from_secs(5), || server.audio() > 0));

    let off = Instant::now();
    engine.set_enabled(false);
    // Give the thread's render time to run out, were sound waiting for it.
    std::thread::sleep(Duration::from_millis(1500));
    let late = last_sound(&server).map(|at| at.saturating_duration_since(off));
    // The fade is 30 ms; the server holds a few ms more.
    assert!(
        late.is_some_and(|l| l < Duration::from_millis(120)),
        "sound went on {late:?} after it was switched off"
    );
    drop(engine);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn another_theme_does_not_wait_for_a_render_either() {
    let dir = scratch("theme");
    let server = Server::start(&dir.join("native"));
    let mut engine = SoundEngine::new(settings(&dir.join("native")));
    let _bank = sounding(&mut engine);
    assert!(until(Duration::from_secs(5), || server.audio() > 0));

    let at = Instant::now();
    let _next = engine.load(&ThemeSounds {
        sounds: vec![heavy(300)],
        ..ThemeSounds::default()
    });
    std::thread::sleep(Duration::from_millis(1500));
    let late = last_sound(&server).map(|t| t.saturating_duration_since(at));
    assert!(
        late.is_some_and(|l| l < Duration::from_millis(120)),
        "the last theme's sound went on {late:?} into the next"
    );
    drop(engine);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_volume_does_not_wait_for_a_render_either() {
    let dir = scratch("volume");
    let server = Server::start(&dir.join("native"));
    let mut engine = SoundEngine::new(settings(&dir.join("native")));
    let _bank = sounding(&mut engine);
    assert!(until(Duration::from_secs(5), || server.audio() > 0));

    let at = Instant::now();
    engine.set_volume(0.0);
    std::thread::sleep(Duration::from_millis(1500));
    let late = last_sound(&server).map(|t| t.saturating_duration_since(at));
    assert!(
        late.is_some_and(|l| l < Duration::from_millis(120)),
        "sound went on {late:?} after the volume was set to 0"
    );
    drop(engine);
    std::fs::remove_dir_all(&dir).unwrap();
}
