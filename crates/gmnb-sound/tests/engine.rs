//! The engine, without a sound device: the null sink (what
//! `GMNB_SOUND=null` selects), a socket nobody listens on, and stand-in
//! servers that hang up or never answer. No test here can reach the
//! session's sound server: a test only ever names a socket in a folder it
//! made itself.

mod common;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gmnb_sound::{
    Bank, Env, FileData, Glide, Layer, PlayParams, Played, Recipe, Settings, Sink, SoundEngine,
    SoundId, SoundSource, SoundState, Status, ThemeSounds, dump_name, limits, render,
};

type Lines = Arc<Mutex<Vec<String>>>;

/// Settings for the null sink, with what the engine says kept in `lines`.
fn quiet(lines: &Lines) -> Settings {
    let lines = lines.clone();
    Settings {
        sink: Sink::Null,
        log: Some(Arc::new(move |line: &str| {
            lines.lock().unwrap().push(line.to_string())
        })),
        ..Settings::default()
    }
}

fn null() -> Settings {
    quiet(&Lines::default())
}

/// A tone of `seconds`.
fn tone(seconds: f64) -> Recipe {
    Recipe {
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
    }
}

fn theme(recipes: Vec<Recipe>) -> ThemeSounds {
    ThemeSounds {
        sounds: recipes.into_iter().map(SoundSource::Synth).collect(),
        ..ThemeSounds::default()
    }
}

fn loaded(engine: &mut SoundEngine, recipes: Vec<Recipe>) -> Bank {
    let bank = engine.load(&theme(recipes));
    assert!(bank.wait_settled(Duration::from_secs(20)));
    bank
}

/// Waits until `done()`, at most `limit`.
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

/// Plays `sound` and waits until it has become a voice. (On a machine
/// so busy that the sound thread got to the play too late to play it, it
/// is played again.)
fn start(engine: &SoundEngine, bank: &Bank, sound: u32, params: PlayParams) {
    for _ in 0..50 {
        let before = engine.stats();
        assert_eq!(engine.play(bank, SoundId(sound), params), Played::Queued);
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

/// The processor time a thread of this process has had, in nanoseconds.
fn cpu_ns(tid: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/self/task/{tid}/schedstat"))
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse().ok())
        .unwrap_or(0)
}

/// The sound thread's id, once the thread has started.
fn thread_of(engine: &SoundEngine) -> u32 {
    assert!(until(Duration::from_secs(5), || engine
        .thread_id()
        .is_some()));
    engine.thread_id().unwrap()
}

/// Whether the thread is gone from /proc soon. (A thread that has been
/// joined can still be listed for a moment: the kernel takes it off the
/// list after it wakes whoever joins it.)
fn thread_gone(tid: u32) -> bool {
    until(Duration::from_secs(5), || !thread_exists(tid))
}

fn thread_exists(tid: u32) -> bool {
    std::path::Path::new(&format!("/proc/self/task/{tid}")).exists()
}

/// A folder of this test's own.
fn scratch(name: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "gmnb-sound-{}-{name}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn nothing_exists_until_a_theme_with_sounds_is_loaded() {
    let mut engine = SoundEngine::new(null());
    assert!(!engine.has_thread());
    assert_eq!(engine.status(), Status::Idle);
    assert_eq!(engine.stats().threads, 0);
    // A theme with no sounds and no `sound` function: still nothing.
    let bank = engine.load(&ThemeSounds::default());
    assert!(!engine.has_thread());
    assert_eq!(engine.stats().threads, 0);
    assert_eq!(
        engine.play(&bank, SoundId(1), PlayParams::default()),
        Played::Off
    );
    assert_eq!(engine.status(), Status::Idle);
    // One with a sound: the thread starts.
    let bank = loaded(&mut engine, vec![tone(0.02)]);
    assert!(engine.has_thread());
    assert_eq!(engine.stats().threads, 1);
    assert!(until(Duration::from_secs(5), || engine.status() == Status::Ready));
    assert_eq!(bank.state(SoundId(1)), SoundState::Ready);
    // One with only a `sound` function (it may make its sounds later).
    let mut engine = SoundEngine::new(null());
    engine.load(&ThemeSounds {
        hook: true,
        ..ThemeSounds::default()
    });
    assert!(engine.has_thread());
}

#[test]
fn switched_off_nothing_is_created() {
    let mut engine = SoundEngine::new(Settings {
        enabled: false,
        ..null()
    });
    let bank = engine.load(&theme(vec![tone(0.02)]));
    assert!(!engine.has_thread());
    assert_eq!(engine.status(), Status::Off);
    assert_eq!(
        engine.play(&bank, SoundId(1), PlayParams::default()),
        Played::Off
    );
    assert_eq!(bank.state(SoundId(1)), SoundState::Pending);
    assert_eq!(engine.stats().threads, 0);

    // The switch goes on: the theme's sounds are made now.
    engine.set_enabled(true);
    assert!(engine.has_thread());
    assert!(bank.wait_settled(Duration::from_secs(20)));
    assert_eq!(
        engine.play(&bank, SoundId(1), PlayParams::default()),
        Played::Queued
    );
    let tid = thread_of(&engine);

    // And off again: the thread ends, and plays go nowhere.
    engine.set_enabled(false);
    assert!(!engine.has_thread());
    assert_eq!(engine.status(), Status::Off);
    assert_eq!(
        engine.play(&bank, SoundId(1), PlayParams::default()),
        Played::Off
    );
    assert!(thread_gone(tid));

    // GMNB_SOUND=0: whatever Settings says.
    let mut engine = SoundEngine::new(Settings {
        sink: Sink::Off,
        ..null()
    });
    let bank = engine.load(&theme(vec![tone(0.02)]));
    engine.set_enabled(false);
    engine.set_enabled(true);
    assert!(!engine.has_thread());
    assert_eq!(engine.status(), Status::Off);
    assert_eq!(
        engine.play(&bank, SoundId(1), PlayParams::default()),
        Played::Off
    );
    assert_eq!(engine.stats().threads, 0);
}

#[test]
fn the_environment_chooses_the_sink() {
    let with = |vars: &'static [(&'static str, &'static str)]| {
        Settings::from_lookup(true, 0.5, &move |name| {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        })
    };
    assert_eq!(with(&[]).sink, Sink::Session);
    assert_eq!(with(&[("GMNB_SOUND", "0")]).sink, Sink::Off);
    assert_eq!(with(&[("GMNB_SOUND", "null")]).sink, Sink::Null);
    assert_eq!(with(&[("GMNB_SOUND", "1")]).sink, Sink::Session);
    assert_eq!(with(&[]).dump, None);
    assert_eq!(with(&[("GMNB_SOUND_DUMP", "")]).dump, None);
    assert_eq!(
        with(&[("GMNB_SOUND_DUMP", "/tmp/x")]).dump,
        Some(PathBuf::from("/tmp/x"))
    );
    // Without being asked for the session's server, an engine has none.
    assert_eq!(Settings::default().sink, Sink::Null);
    let s = Settings::default();
    assert!(s.enabled && s.volume == 0.5);
}

#[test]
fn only_the_loaded_banks_ready_sounds_play() {
    let mut engine = SoundEngine::new(null());
    let old = loaded(&mut engine, vec![tone(0.02)]);
    let bank = engine.load(&ThemeSounds {
        sounds: vec![
            SoundSource::Synth(tone(0.02)),
            SoundSource::Silent,
            SoundSource::Synth(Recipe::default()),
            SoundSource::File {
                name: "sounds/gone.ogg".into(),
                data: FileData::Missing("no such file".into()),
                gain: None,
            },
        ],
        ..ThemeSounds::default()
    });
    assert!(bank.wait_settled(Duration::from_secs(20)));
    let play = |bank: &Bank, id: u32| engine.play(bank, SoundId(id), PlayParams::default());
    assert_eq!(play(&bank, 1), Played::Queued);
    assert_eq!(play(&old, 1), Played::Stale);
    // Silent, refused and unknown sounds play nothing.
    for id in [0, 2, 3, 4, 5, 33, u32::MAX] {
        assert_eq!(play(&bank, id), Played::NotReady, "sound {id}");
    }
    assert_eq!(bank.state(SoundId(2)), SoundState::Refused);
    assert_eq!(bank.state(SoundId(5)), SoundState::Unknown);
    // The theme's author is told why, once.
    let notes = bank.take_notes();
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert_eq!(notes[0].0, SoundId(3));
    assert!(notes[0].1.contains("layers"));
    assert_eq!(
        notes[1],
        (SoundId(4), "sounds/gone.ogg: no such file".into())
    );
    assert!(bank.take_notes().is_empty());
}

#[test]
fn a_hundred_plays_at_once_leave_twelve_voices() {
    let mut engine = SoundEngine::new(null());
    let bank = loaded(&mut engine, vec![tone(1.5)]);
    // Each to start in half a second: however busy the machine, the sound
    // thread gets to them in time, and a voice that waits is a voice.
    let soon = PlayParams {
        delay: 0.5,
        ..PlayParams::default()
    };
    let mut queued = 0;
    for _ in 0..100 {
        match engine.play(&bank, SoundId(1), soon) {
            Played::Queued => queued += 1,
            Played::Dropped => {}
            other => panic!("{other:?}"),
        }
    }
    assert!(queued >= limits::QUEUE as u64, "{queued}");
    assert!(until(Duration::from_secs(5), || {
        let s = engine.stats();
        s.started + s.skipped == queued
    }));
    let stats = engine.stats();
    assert_eq!(stats.queued, queued);
    assert_eq!(stats.queued + stats.dropped, 100);
    // (None was late, unless the machine stood still for half a second.)
    if stats.skipped == 0 {
        assert_eq!(stats.voices, 12);
        assert_eq!(stats.mixer.unwrap().most_voices, 12);
    }
    assert!(stats.voices <= 12 && stats.mixer.unwrap().most_voices <= 12);
    assert_eq!(stats.mixer.unwrap().started, stats.started);
}

#[test]
fn a_full_queue_drops_and_nothing_plays_late() {
    // A click first; then the largest recipes there can be, which keep
    // the sound thread busy for a good while: a hundred plays arrive
    // while it renders the first of them.
    for attempt in 0.. {
        let mut engine = SoundEngine::new(null());
        let bank = engine.load(&theme(vec![tone(0.5), common::worst(), common::worst()]));
        assert!(until(Duration::from_secs(20), || {
            bank.state(SoundId(1)) == SoundState::Ready
        }));
        let results: Vec<Played> = (0..100)
            .map(|_| engine.play(&bank, SoundId(1), PlayParams::default()))
            .collect();
        // All hundred calls returned while the thread was still inside
        // that one render: none of them waited for it. (If it is not,
        // this machine was too busy for the test to mean anything: again.)
        if bank.state(SoundId(2)) != SoundState::Pending {
            assert!(attempt < 5, "the plays never arrived during the render");
            continue;
        }
        let queued = results.iter().filter(|r| **r == Played::Queued).count();
        let dropped = results.iter().filter(|r| **r == Played::Dropped).count();
        assert_eq!(queued + dropped, 100);
        // The queue filled, and the rest were dropped at once.
        assert!(dropped >= 30, "{dropped} dropped");
        assert!(bank.wait_settled(Duration::from_secs(60)));
        // By the time the thread got to the queued ones they were late:
        // skipped, not played.
        assert!(until(Duration::from_secs(5), || {
            let s = engine.stats();
            s.started + s.skipped == queued as u64
        }));
        let stats = engine.stats();
        assert!(stats.skipped >= 60, "{stats:?}");
        // A play made now sounds.
        start(&engine, &bank, 1, PlayParams::default());
        break;
    }
}

#[test]
fn play_never_waits() {
    let mut engine = SoundEngine::new(null());
    let bank = loaded(&mut engine, vec![tone(0.3)]);
    // Twelve voices going, the mixer at work, and plays at the rate of a
    // very fast typist.
    let mut times = Vec::new();
    for i in 0..3000 {
        let t = Instant::now();
        let played = engine.play(&bank, SoundId(1), PlayParams::default());
        times.push(t.elapsed());
        assert!(matches!(played, Played::Queued | Played::Dropped));
        if i % 50 == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    times.sort();
    let (median, p99) = (times[times.len() / 2], times[times.len() * 99 / 100]);
    println!(
        "play: median {median:?}, p99 {p99:?}, worst {:?}",
        times[times.len() - 1]
    );
    // The worst is whatever the scheduler did to this thread, once or
    // twice. A play that waited on the mixer (at work every 5 ms here) or
    // on the queue would show in more than one call in a hundred.
    assert!(median < Duration::from_micros(100), "median {median:?}");
    assert!(p99 < Duration::from_millis(1), "p99 {p99:?}");
}

#[test]
fn at_rest_the_sound_thread_does_nothing() {
    let mut engine = SoundEngine::new(null());
    let bank = loaded(&mut engine, vec![tone(0.05)]);
    let tid = thread_of(&engine);
    start(&engine, &bank, 1, PlayParams::default());
    // The sound, 100 ms of silence, then the output is stopped.
    assert!(until(Duration::from_secs(5), || engine.stats().corks == 1));
    let stats = engine.stats();
    assert_eq!((stats.uncorks, stats.corks), (1, 1));
    // 50 ms of sound and 100 ms of silence went into the sink.
    assert!(
        (7000..=9000).contains(&stats.null_frames),
        "{} frames",
        stats.null_frames
    );
    std::thread::sleep(Duration::from_millis(50));
    let (cpu, wakes) = (cpu_ns(tid), engine.stats().wakes);
    std::thread::sleep(Duration::from_secs(2));
    let after = engine.stats();
    assert_eq!(after.wakes, wakes, "the sound thread woke at rest");
    assert_eq!(after.null_frames, stats.null_frames);
    let used = cpu_ns(tid) - cpu;
    assert!(used < 5_000_000, "{used} ns of processor time at rest");
    // The next play starts it again.
    start(&engine, &bank, 1, PlayParams::default());
    assert!(until(Duration::from_secs(5), || engine.stats().corks == 2));
    assert_eq!(engine.stats().uncorks, 2);
}

#[test]
fn a_delayed_play_keeps_the_output_going() {
    let mut engine = SoundEngine::new(null());
    let bank = loaded(&mut engine, vec![tone(0.02)]);
    let late = PlayParams {
        delay: 0.4,
        ..PlayParams::default()
    };
    start(&engine, &bank, 1, late);
    // 400 ms of waiting, the sound, 100 ms of silence: about 25 000
    // frames before the output stops (fewer by however long the play
    // waited for the sound thread), not the 5800 of the sound and the
    // silence alone.
    assert!(until(Duration::from_secs(5), || engine.stats().corks == 1));
    let frames = engine.stats().null_frames;
    assert!((15_000..=27_500).contains(&frames), "{frames} frames");
}

#[test]
fn dropping_the_engine_joins_its_thread() {
    let mut engine = SoundEngine::new(null());
    let bank = loaded(&mut engine, vec![tone(2.0)]);
    let tid = thread_of(&engine);
    assert!(thread_exists(tid));
    for _ in 0..5 {
        start(&engine, &bank, 1, PlayParams::default());
    }
    assert!(engine.stats().voices >= 1);
    drop(engine);
    assert!(thread_gone(tid));
}

#[test]
fn unloading_lets_everything_go() {
    let mut engine = SoundEngine::new(null());
    let bank = loaded(&mut engine, vec![tone(2.0)]);
    let tid = thread_of(&engine);
    start(&engine, &bank, 1, PlayParams::default());
    engine.unload();
    assert!(!engine.has_thread());
    assert_eq!(engine.status(), Status::Idle);
    assert_eq!(
        engine.play(&bank, SoundId(1), PlayParams::default()),
        Played::Off
    );
    assert!(thread_gone(tid));
    // Loading again starts afresh.
    let bank = loaded(&mut engine, vec![tone(0.02)]);
    assert_eq!(engine.stats().threads, 2);
    assert_eq!(
        engine.play(&bank, SoundId(1), PlayParams::default()),
        Played::Queued
    );
}

#[test]
fn another_theme_takes_over_the_same_thread() {
    let mut engine = SoundEngine::new(null());
    let first = loaded(&mut engine, vec![tone(3.5)]);
    let tid = thread_of(&engine);
    start(&engine, &first, 1, PlayParams::default());
    let second = loaded(&mut engine, vec![tone(0.02), tone(0.03)]);
    assert_eq!(engine.thread_id(), Some(tid));
    assert_eq!(engine.stats().threads, 1);
    // The first theme's voice fades out (30 ms of the sink's time, not
    // the seconds it had left).
    assert!(until(Duration::from_secs(2), || engine.stats().voices == 0));
    assert_eq!(
        engine.play(&first, SoundId(1), PlayParams::default()),
        Played::Stale
    );
    assert_eq!(
        engine.play(&second, SoundId(2), PlayParams::default()),
        Played::Queued
    );
}

#[test]
fn sounds_made_later_join_the_bank() {
    let mut engine = SoundEngine::new(null());
    let mut sounds = theme(vec![tone(0.02)]);
    sounds.hook = true;
    let bank = engine.load(&sounds);
    assert!(bank.wait_settled(Duration::from_secs(20)));
    assert_eq!(bank.state(SoundId(2)), SoundState::Unknown);
    assert_eq!(
        engine.play(&bank, SoundId(2), PlayParams::default()),
        Played::NotReady
    );
    // The theme's `sound` function made another.
    sounds.sounds.push(SoundSource::Synth(tone(0.03)));
    engine.extend(&bank, &sounds.sounds);
    assert_ne!(bank.state(SoundId(2)), SoundState::Unknown);
    assert!(bank.wait_settled(Duration::from_secs(20)));
    assert_eq!(
        engine.play(&bank, SoundId(2), PlayParams::default()),
        Played::Queued
    );
    // Told again with nothing new: nothing happens.
    engine.extend(&bank, &sounds.sounds);
    assert!(bank.is_settled());
    // The theme-wide limits go on counting: 32 sounds in all.
    sounds
        .sounds
        .extend((0..40).map(|_| SoundSource::Synth(tone(0.01))));
    engine.extend(&bank, &sounds.sounds);
    assert!(bank.wait_settled(Duration::from_secs(20)));
    assert_eq!(bank.state(SoundId(32)), SoundState::Ready);
    assert_eq!(bank.state(SoundId(33)), SoundState::Unknown);
    assert_eq!(
        engine.play(&bank, SoundId(33), PlayParams::default()),
        Played::NotReady
    );
}

#[test]
fn the_volume_is_kept_within_bounds() {
    let mut engine = SoundEngine::new(null());
    assert_eq!(engine.volume(), 0.5);
    engine.set_volume(2.0);
    assert_eq!(engine.volume(), 1.0);
    engine.set_volume(f32::NAN);
    assert_eq!(engine.volume(), 0.0);
    // With a thread running it is told, and nothing else changes.
    let bank = loaded(&mut engine, vec![tone(0.05)]);
    engine.set_volume(0.8);
    start(&engine, &bank, 1, PlayParams::default());
    assert_eq!(engine.stats().started, 1);
}

// ---- GMNB_SOUND_DUMP ---------------------------------------------------------------

#[test]
fn every_sound_is_dumped_as_a_wav_file() {
    let dir = scratch("dump");
    let out = dir.join("out/sounds");
    let file = dir.join("tap.wav");
    std::fs::write(&file, gmnb_sound::wav::encode(&render(&tone(0.01)))).unwrap();
    let sounds = ThemeSounds {
        sounds: vec![
            SoundSource::Synth(common::click_soft()),
            SoundSource::File {
                name: "sounds/tap.wav".into(),
                data: FileData::Path(file),
                gain: None,
            },
            SoundSource::Synth(Recipe::default()),
            SoundSource::Synth(common::error_buzz()),
        ],
        ..ThemeSounds::default()
    };
    let mut engine = SoundEngine::new(Settings {
        dump: Some(out.clone()),
        ..null()
    });
    let bank = engine.load(&sounds);
    assert!(bank.wait_settled(Duration::from_secs(20)));
    drop(engine);

    let mut names: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    // Named by order and a hash; the refused sound (3) has no file.
    let expected: Vec<String> = [0, 1, 3]
        .iter()
        .map(|i| dump_name(i + 1, &sounds.sounds[*i]))
        .collect();
    assert_eq!(names, expected);
    assert!(names[0].starts_with("01-") && names[2].starts_with("04-"));
    // 16-bit stereo at 48 kHz, the samples the mixer plays.
    let read = |name: &str| {
        let mut r = hound::WavReader::open(out.join(name)).unwrap();
        let spec = r.spec();
        assert_eq!(
            (spec.channels, spec.sample_rate, spec.bits_per_sample),
            (2, 48_000, 16)
        );
        r.samples::<i16>().map(Result::unwrap).collect::<Vec<i16>>()
    };
    assert_eq!(read(&names[0]), render(&common::click_soft()));
    assert_eq!(
        common::hash(&read(&names[0])),
        "21c869ae7b1c53cb7049330a055fc4e7be80f991af3e04719b7ceaa69b5ce816"
    );
    assert_eq!(read(&names[1]), render(&tone(0.01)));
    assert_eq!(read(&names[2]), render(&common::error_buzz()));
    std::fs::remove_dir_all(&dir).unwrap();
}

// ---- No sound system ---------------------------------------------------------------

/// Settings for a sound server at `socket`, which is in a folder of the
/// test's own.
fn server(socket: PathBuf, lines: &Lines) -> Settings {
    Settings {
        sink: Sink::Socket {
            path: socket,
            cookie: vec![0; 256],
        },
        ..quiet(lines)
    }
}

#[test]
fn with_no_sound_server_there_is_silence_and_one_line() {
    // What XDG_RUNTIME_DIR pointing at an empty folder comes to.
    let dir = scratch("empty");
    let lines = Lines::default();
    let mut engine = SoundEngine::new(server(dir.join("pulse/native"), &lines));
    let bank = loaded(&mut engine, vec![common::click_soft(), tone(0.1)]);
    assert!(until(Duration::from_secs(5), || engine.status()
        == Status::Unavailable));
    assert!(
        engine
            .unavailable_reason()
            .unwrap()
            .contains("pulse/native")
    );
    assert!(!engine.available());
    for i in 0..100 {
        let played = engine.play(&bank, SoundId(1 + i % 2), PlayParams::default());
        assert_eq!(played, Played::Unavailable);
    }
    let stats = engine.stats();
    assert_eq!((stats.queued, stats.started, stats.connects), (0, 0, 1));
    assert_eq!(
        lines.lock().unwrap().len(),
        1,
        "{:?}",
        lines.lock().unwrap()
    );
    assert!(lines.lock().unwrap()[0].contains("no sound system found"));

    // No thread left spinning: over two seconds the sound thread does
    // not wake and uses no processor time to speak of.
    let tid = thread_of(&engine);
    std::thread::sleep(Duration::from_millis(50));
    let (cpu, wakes) = (cpu_ns(tid), engine.stats().wakes);
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(engine.stats().wakes, wakes);
    let used = cpu_ns(tid) - cpu;
    assert!(
        used < 5_000_000,
        "{used} ns of processor time with no server"
    );
    assert_eq!(engine.stats().connects, 1);
    assert_eq!(lines.lock().unwrap().len(), 1);
    drop(engine);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Settings hears of each change of state, from the sound thread: "No
/// sound system found" as soon as it is so, and nothing at rest.
#[test]
fn each_change_of_state_is_told() {
    type Told = Arc<Mutex<Vec<Status>>>;
    let hook = |told: &Told| -> Option<gmnb_sound::StatusHook> {
        let told = told.clone();
        Some(Arc::new(move |status| told.lock().unwrap().push(status)))
    };
    // No server: unavailable, told once.
    let dir = scratch("told");
    let told = Told::default();
    let mut engine = SoundEngine::new(Settings {
        on_status: hook(&told),
        ..server(dir.join("pulse/native"), &Lines::default())
    });
    loaded(&mut engine, vec![tone(0.02)]);
    assert!(until(Duration::from_secs(5), || !told
        .lock()
        .unwrap()
        .is_empty()));
    // By the time it is told, the state shows.
    assert_eq!(engine.status(), Status::Unavailable);
    assert_eq!(*told.lock().unwrap(), [Status::Unavailable]);
    drop(engine);
    // The null sink: ready, once.
    let told = Told::default();
    let mut engine = SoundEngine::new(Settings {
        on_status: hook(&told),
        ..null()
    });
    loaded(&mut engine, vec![tone(0.02)]);
    assert!(until(Duration::from_secs(5), || !told
        .lock()
        .unwrap()
        .is_empty()));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(*told.lock().unwrap(), [Status::Ready]);
    drop(engine);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn it_looks_again_politely() {
    let dir = scratch("retry");
    let lines = Lines::default();
    let mut engine = SoundEngine::new(Settings {
        retry: Duration::from_secs(1),
        ..server(dir.join("pulse/native"), &lines)
    });
    let loading = Instant::now();
    let bank = loaded(&mut engine, vec![tone(0.02)]);
    assert!(until(Duration::from_secs(5), || engine.status()
        == Status::Unavailable));
    let play = |engine: &SoundEngine| engine.play(&bank, SoundId(1), PlayParams::default());
    // Plays inside the interval change nothing. (If this machine took the
    // whole interval to get here, there is nothing to see.)
    for _ in 0..50 {
        assert_eq!(play(&engine), Played::Unavailable);
    }
    let in_time = loading.elapsed() < Duration::from_millis(900);
    std::thread::sleep(Duration::from_millis(50));
    if in_time {
        assert_eq!(engine.stats().connects, 1);
    }
    // After it, one play (of many) makes one more attempt.
    std::thread::sleep(Duration::from_millis(1100));
    let before = engine.stats().connects;
    let burst = Instant::now();
    for _ in 0..50 {
        assert_eq!(play(&engine), Played::Unavailable);
    }
    let in_time = burst.elapsed() < Duration::from_millis(900);
    assert!(until(Duration::from_secs(5), || engine.stats().connects
        == before + 1));
    std::thread::sleep(Duration::from_millis(50));
    if in_time {
        assert_eq!(
            engine.stats().connects,
            before + 1,
            "fifty plays, one attempt"
        );
    }
    // Time alone makes none: only a play does.
    let before = engine.stats().connects;
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(engine.stats().connects, before);
    // Loading a theme looks again; so does switching sound off and on.
    let bank2 = loaded(&mut engine, vec![tone(0.02)]);
    assert!(until(Duration::from_secs(5), || engine.stats().connects
        == before + 1));
    engine.set_enabled(false);
    engine.set_enabled(true);
    assert!(until(Duration::from_secs(5), || engine.stats().connects
        == before + 2));
    assert!(until(Duration::from_secs(5), || engine.status()
        == Status::Unavailable));
    assert_eq!(
        engine.play(&bank2, SoundId(1), PlayParams::default()),
        Played::Unavailable
    );
    // Through all of it, the state changed once: one line.
    assert_eq!(
        lines.lock().unwrap().len(),
        1,
        "{:?}",
        lines.lock().unwrap()
    );
    drop(engine);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_server_that_hangs_up_is_no_server() {
    let dir = scratch("hangup");
    let socket = dir.join("native");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let accepting = std::thread::spawn(move || {
        // Take the connection and close it at once, twice over.
        for conn in listener.incoming().take(2) {
            drop(conn);
        }
    });
    let lines = Lines::default();
    let mut engine = SoundEngine::new(Settings {
        retry: Duration::from_millis(100),
        ..server(socket, &lines)
    });
    let bank = loaded(&mut engine, vec![tone(0.02)]);
    assert!(until(Duration::from_secs(5), || engine.status()
        == Status::Unavailable));
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(
        engine.play(&bank, SoundId(1), PlayParams::default()),
        Played::Unavailable
    );
    assert!(until(Duration::from_secs(5), || engine.stats().connects == 2));
    accepting.join().unwrap();
    assert_eq!(lines.lock().unwrap().len(), 1);
    drop(engine);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_server_that_never_answers_does_not_hold_the_engine() {
    let dir = scratch("mute");
    let socket = dir.join("native");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    let holding = std::thread::spawn(move || {
        // Take the connection, read what comes, say nothing.
        let (conn, _) = listener.accept().unwrap();
        let _ = stopped.recv();
        drop(conn);
    });
    let lines = Lines::default();
    // The engine gives the server a second to answer each time.
    let patient = |lines: &Lines| Settings {
        server_limit: Duration::from_secs(1),
        ..server(socket.clone(), lines)
    };
    let mut engine = SoundEngine::new(patient(&lines));
    let started = Instant::now();
    let bank = engine.load(&theme(vec![tone(0.02)]));
    // Loading did not wait for the server: it is back while the sound
    // thread is still waiting for the greeting to be answered.
    if started.elapsed() < Duration::from_millis(500) {
        assert_eq!(engine.status(), Status::Starting);
    }
    let tid = thread_of(&engine);
    assert!(until(Duration::from_secs(10), || engine.status()
        == Status::Unavailable));
    // It waited its second, not for ever.
    assert!(started.elapsed() >= Duration::from_millis(900));
    // The sounds were made all the same; there is just nowhere to play.
    assert!(bank.wait_settled(Duration::from_secs(20)));
    assert_eq!(
        engine.play(&bank, SoundId(1), PlayParams::default()),
        Played::Unavailable
    );

    // Unloaded while the sound thread is waiting on the mute server:
    // unloading does not wait for it...
    let mut engine2 = SoundEngine::new(patient(&lines));
    engine2.load(&theme(vec![tone(0.02)]));
    let tid2 = thread_of(&engine2);
    let unloading = Instant::now();
    engine2.unload();
    assert!(
        unloading.elapsed() < Duration::from_millis(500),
        "unload waited for the sound thread"
    );
    // ...and dropping joins the thread, which was hung up on rather than
    // left to give the server its second.
    let dropping = Instant::now();
    drop(engine2);
    assert!(thread_gone(tid2));
    assert!(
        dropping.elapsed() < Duration::from_millis(900),
        "the engine waited {:?} for the server",
        dropping.elapsed()
    );
    drop(engine);
    assert!(thread_gone(tid));
    stop.send(()).unwrap();
    holding.join().unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A server that greets the engine a byte at a time, each byte well
/// inside the limit on one wait (the greeting's blocking reads are timed
/// one by one): it would hold the sound thread for seconds.
fn trickle(listener: std::os::unix::net::UnixListener, gap: Duration, conns: usize) {
    use pulseaudio::protocol::{self, Command};
    use std::io::{Read, Write};
    const VERSION: u16 = 35;
    let mut serving = Vec::new();
    for conn in listener.incoming().take(conns) {
        let Ok(mut conn) = conn else { break };
        serving.push(std::thread::spawn(move || {
            let mut payload = Vec::new();
            while let Ok(desc) = protocol::read_descriptor(&mut conn) {
                payload.resize((desc.length as usize).min(1 << 16), 0);
                if conn.read_exact(&mut payload).is_err() {
                    return;
                }
                let read = Command::read_tag_prefixed(&mut std::io::Cursor::new(&payload), VERSION);
                let mut answer = Vec::new();
                match read {
                    Ok((seq, Command::Auth(_))) => {
                        let r = protocol::AuthReply {
                            version: VERSION,
                            ..Default::default()
                        };
                        protocol::write_reply_message(&mut answer, seq, &r, VERSION).unwrap();
                    }
                    Ok((seq, Command::SetClientName(_))) => {
                        let r = protocol::SetClientNameReply { client_id: 1 };
                        protocol::write_reply_message(&mut answer, seq, &r, VERSION).unwrap();
                    }
                    _ => return,
                }
                for byte in answer {
                    std::thread::sleep(gap);
                    if conn.write_all(&[byte]).is_err() {
                        return;
                    }
                }
            }
        }));
    }
    for s in serving {
        let _ = s.join();
    }
}

#[test]
fn a_server_that_greets_a_byte_at_a_time_does_not_hold_the_engine() {
    let dir = scratch("trickle");
    let socket = dir.join("native");
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    // A byte every 100 ms: the greeting would take some 7 s.
    let serving = std::thread::spawn(move || trickle(listener, Duration::from_millis(100), 2));
    let lines = Lines::default();
    let patient = |lines: &Lines| Settings {
        server_limit: Duration::from_secs(1),
        ..server(socket.clone(), lines)
    };

    // Unloaded while it is being greeted: the thread is back at once.
    let mut engine = SoundEngine::new(patient(&lines));
    engine.load(&theme(vec![tone(0.02)]));
    let tid = thread_of(&engine);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(engine.status(), Status::Starting);
    let unloaded = Instant::now();
    engine.unload();
    assert!(
        thread_gone(tid),
        "the greeting still holds the sound thread"
    );
    assert!(
        unloaded.elapsed() < Duration::from_secs(1),
        "the greeting held the sound thread {:?} after the theme went",
        unloaded.elapsed()
    );

    // Dropped while it is being greeted: no wait, no thread left behind.
    let mut engine2 = SoundEngine::new(patient(&lines));
    engine2.load(&theme(vec![tone(0.02)]));
    let tid2 = thread_of(&engine2);
    std::thread::sleep(Duration::from_millis(500));
    let dropped = Instant::now();
    drop(engine2);
    assert!(
        dropped.elapsed() < Duration::from_secs(1),
        "dropping the engine took {:?}",
        dropped.elapsed()
    );
    assert!(thread_gone(tid2));
    // Hung up on to stop, it said nothing: that is not "no sound system".
    assert!(
        lines.lock().unwrap().is_empty(),
        "{:?}",
        lines.lock().unwrap()
    );
    drop(engine);
    serving.join().unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}
