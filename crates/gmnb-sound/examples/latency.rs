//! Measures the engine against a real sound server: how long a play takes
//! to be handed over and to reach the sink, what a stream at rest costs,
//! and that everything goes away again.
//!
//! Runs ONLY inside a sound sandbox: no sound hardware (`/dev/snd` empty),
//! none of the user's sound servers, a private PipeWire with one null sink
//! named `gmnb-null` (the design's `theming-work/media/audiobox.sh` is
//! one). Anywhere else it refuses and exits.
//!
//!     audiobox.sh target/release/examples/latency [quick]
//!
//! "On the sink" is the moment the sound shows on the null sink's monitor,
//! read back through a 5 ms record stream: an upper bound.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use gmnb_sound::{
    Bank, Env, Glide, Layer, PlayParams, Played, Recipe, Settings, Sink, SoundEngine, SoundId,
    SoundSource, Status, ThemeSounds,
};
use pulseaudio::protocol::{self, stream::BufferAttr};
use pulseaudio::{Client, RecordStream};

/// One burst of sound on the sink.
struct Burst {
    start: Instant,
    ms: f64,
    peak: i16,
}

/// Listens to the null sink's monitor. An onset's time is the arrival of
/// the chunk it is in, less the samples after it: late by the capture
/// path, never early.
struct Monitor {
    _client: Client,
    _stream: RecordStream,
    bursts: Receiver<Burst>,
}

/// Louder than this is sound.
const LOUD: i16 = 48;

impl Monitor {
    fn open(socket: &Path) -> Result<Monitor, String> {
        let conn = std::os::unix::net::UnixStream::connect(socket).map_err(|e| e.to_string())?;
        let client = Client::new_unix(c"GMNB latency monitor", conn, Some(vec![0u8; 256]))
            .map_err(|e| format!("{e:?}"))?;
        let (tx, bursts) = mpsc::channel();
        let (mut start, mut last, mut quiet, mut peak) =
            (None::<Instant>, Instant::now(), 0u32, 0i16);
        let sink = move |data: &[u8]| {
            let now = Instant::now();
            let frames = data.len() / 4;
            for (i, f) in data.as_chunks::<4>().0.iter().enumerate() {
                let l = i16::from_le_bytes([f[0], f[1]]);
                let r = i16::from_le_bytes([f[2], f[3]]);
                let v = l.saturating_abs().max(r.saturating_abs());
                let at = now - Duration::from_secs_f64((frames - i) as f64 / 48_000.0);
                if v > LOUD {
                    if start.is_none() {
                        start = Some(at);
                        peak = 0;
                    }
                    last = at;
                    quiet = 0;
                    peak = peak.max(v);
                } else if let Some(s) = start {
                    quiet += 1;
                    // 20 ms of quiet end a burst.
                    if quiet > 960 {
                        let _ = tx.send(Burst {
                            start: s,
                            ms: last.duration_since(s).as_secs_f64() * 1e3,
                            peak,
                        });
                        start = None;
                    }
                }
            }
        };
        let params = protocol::RecordStreamParams {
            sample_spec: protocol::SampleSpec {
                format: protocol::SampleFormat::S16Le,
                channels: 2,
                sample_rate: 48_000,
            },
            channel_map: protocol::ChannelMap::stereo(),
            source_name: Some(c"gmnb-null.monitor".to_owned()),
            buffer_attr: BufferAttr {
                max_length: u32::MAX,
                target_length: u32::MAX,
                pre_buffering: u32::MAX,
                minimum_request_length: u32::MAX,
                fragment_size: 5 * 192,
            },
            flags: protocol::stream::StreamFlags {
                adjust_latency: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let stream = block_on(client.create_record_stream(params, sink))
            .map_err(|e| format!("record stream: {e:?}"))?;
        Ok(Monitor {
            _client: client,
            _stream: stream,
            bursts,
        })
    }

    fn clear(&self) {
        while self.bursts.try_recv().is_ok() {}
    }

    /// The next burst that started after `since`, if one ends in time.
    fn next(&self, since: Instant, timeout: Duration) -> Option<Burst> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.checked_duration_since(Instant::now())?;
            let b = self.bursts.recv_timeout(left).ok()?;
            if b.start + Duration::from_millis(2) >= since {
                return Some(b);
            }
        }
    }
}

/// A future run to its end on this thread.
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    use std::task::{Context, Poll, Wake, Waker};
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(fut);
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::park();
    }
}

fn threads() -> usize {
    std::fs::read_dir("/proc/self/task").map_or(0, |d| d.count())
}

/// The process's processor time so far, in milliseconds, over all its
/// threads.
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

fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

fn stats(label: &str, v: &mut [f64]) -> String {
    if v.is_empty() {
        return format!("{label}: no samples");
    }
    v.sort_by(f64::total_cmp);
    let p = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    format!(
        "{label}: n {}  p50 {:.2}  p95 {:.2}  max {:.2} ms",
        v.len(),
        p(0.5),
        p(0.95),
        v[v.len() - 1]
    )
}

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    /// A time between `lo` and `hi` milliseconds.
    fn ms(&mut self, lo: u32, hi: u32) -> Duration {
        Duration::from_micros(u64::from(lo * 1000 + self.next() % ((hi - lo) * 1000)))
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

/// A 45 ms click, and a 4 s hum too quiet for the monitor to count.
fn sounds() -> ThemeSounds {
    let click = Recipe {
        gain: 0.5,
        layers: vec![Layer {
            freq: Some(Glide::exp(950.0, 420.0, 0.03)),
            env: Env {
                attack: 0.0005,
                decay: 0.045,
                ..Env::default()
            },
            ..Layer::default()
        }],
        ..Recipe::default()
    };
    let hum = Recipe {
        gain: 0.0005,
        layers: vec![Layer {
            freq: Some(Glide::fixed(200.0)),
            env: Env {
                attack: 0.01,
                decay: 0.01,
                sustain: 1.0,
                hold: 3.9,
                release: 0.05,
                ..Env::default()
            },
            ..Layer::default()
        }],
        ..Recipe::default()
    };
    ThemeSounds {
        sounds: vec![SoundSource::Synth(click), SoundSource::Synth(hum)],
        ..ThemeSounds::default()
    }
}

fn open(socket: &Path) -> (SoundEngine, Bank, f64) {
    let t = Instant::now();
    let mut engine = SoundEngine::new(Settings {
        // Full volume: the click peaks at half of full scale on the sink.
        volume: 1.0,
        sink: Sink::Socket {
            path: socket.to_path_buf(),
            cookie: vec![0; 256],
        },
        ..Settings::default()
    });
    let bank = engine.load(&sounds());
    assert!(until(Duration::from_secs(5), || engine.status()
        != Status::Starting));
    let ready = t.elapsed().as_secs_f64() * 1e3;
    assert_eq!(
        engine.status(),
        Status::Ready,
        "{:?}",
        engine.unavailable_reason()
    );
    assert!(bank.wait_settled(Duration::from_secs(10)));
    (engine, bank, ready)
}

fn main() {
    let sandboxed = std::env::var_os("AUDIOBOX_PRIV").is_some()
        && std::fs::read_dir("/dev/snd").map_or(true, |mut d| d.next().is_none());
    if !sandboxed {
        eprintln!("latency: not in a sound sandbox (audiobox.sh); refusing to run");
        std::process::exit(2);
    }
    let quick = std::env::args().nth(1).is_some();
    let dir = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR");
    let socket = Path::new(&dir).join("pulse/native");
    let click = SoundId(1);
    let hum = SoundId(2);
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    let mut rng = Rng(0x1234_5678);

    let (rss0, threads0) = (rss_kib(), threads());
    println!("before any sound: RSS {rss0} KiB, {threads0} thread(s)");
    let (engine, _bank, ready) = open(&socket);
    std::thread::sleep(Duration::from_millis(500));
    println!(
        "theme loaded: output ready after {ready:.1} ms; RSS {} KiB (+{}), {} threads (+{})",
        rss_kib(),
        rss_kib().saturating_sub(rss0),
        threads(),
        threads() - threads0
    );

    // At rest, before anything has played: the stream was created corked.
    let (cpu, s0) = (cpu_ms(), engine.stats());
    let rest = if quick { 3.0 } else { 6.0 };
    std::thread::sleep(Duration::from_secs_f64(rest));
    let s1 = engine.stats();
    println!(
        "at rest (corked, never played): {:.4}% of one core; {:.1} requests and {:.1} sound-thread wake-ups a second",
        (cpu_ms() - cpu) / (rest * 10.0),
        (s1.requests - s0.requests) as f64 / rest,
        (s1.wakes - s0.wakes) as f64 / rest
    );
    drop(engine);

    let monitor = Monitor::open(&socket).expect("the null sink's monitor");
    std::thread::sleep(Duration::from_millis(300));

    // The start-up case: a stream created corked, left alone for 400 ms,
    // then its first play.
    let (mut made, mut mixed, mut heard) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..if quick { 10 } else { 40 } {
        let (engine, bank, ready) = open(&socket);
        made.push(ready);
        std::thread::sleep(Duration::from_millis(400));
        monitor.clear();
        let at = Instant::now();
        assert_eq!(
            engine.play(&bank, click, PlayParams::default()),
            Played::Queued
        );
        if let Some(b) = monitor.next(at, Duration::from_millis(900)) {
            heard.push(ms(b.start.duration_since(at)));
        }
        mixed.extend(engine.take_latencies().into_iter().map(ms));
        std::thread::sleep(Duration::from_millis(150));
    }
    println!(
        "{}",
        stats(
            "load -> output ready (connect, stream created corked)",
            &mut made
        )
    );
    println!(
        "{}",
        stats(
            "first play after 400 ms corked: play -> handed to the server",
            &mut mixed
        )
    );
    println!(
        "{}",
        stats(
            "first play after 400 ms corked: play -> on the sink (upper bound)",
            &mut heard
        )
    );

    let (engine, bank, _) = open(&socket);

    // From corked: each play finds the stream corked and flushed after
    // the last one.
    let (mut calls, mut mixed, mut heard, mut lens, mut peaks) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for _ in 0..if quick { 15 } else { 80 } {
        let corks = engine.stats().corks;
        std::thread::sleep(rng.ms(250, 450));
        assert!(engine.stats().corks > corks || engine.stats().uncorks == 0);
        monitor.clear();
        let at = Instant::now();
        assert_eq!(
            engine.play(&bank, click, PlayParams::default()),
            Played::Queued
        );
        calls.push(ms(at.elapsed()));
        if let Some(b) = monitor.next(at, Duration::from_millis(600)) {
            heard.push(ms(b.start.duration_since(at)));
            lens.push(b.ms);
            peaks.push(f64::from(b.peak));
        }
        mixed.extend(engine.take_latencies().into_iter().map(ms));
    }
    println!("{}", stats("from corked: the play call itself", &mut calls));
    println!(
        "{}",
        stats("from corked: play -> handed to the server", &mut mixed)
    );
    println!(
        "{}",
        stats("from corked: play -> on the sink (upper bound)", &mut heard)
    );
    println!("{}", stats("  bursts heard, length", &mut lens));
    println!(
        "{}",
        stats("  bursts heard, peak (of 32767; not ms)", &mut peaks)
    );
    let s = engine.stats();
    println!(
        "  {} plays started, {} uncorks, {} corks, {} skipped",
        s.started, s.uncorks, s.corks, s.skipped
    );

    // Stream running: a hum too quiet to count keeps it going.
    let (mut calls, mut mixed, mut heard, mut lens) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut hummed = Instant::now() - Duration::from_secs(10);
    let before = engine.stats();
    for _ in 0..if quick { 40 } else { 200 } {
        if hummed.elapsed() > Duration::from_secs(3) {
            engine.play(&bank, hum, PlayParams::default());
            hummed = Instant::now();
            std::thread::sleep(Duration::from_millis(100));
            engine.take_latencies();
        }
        std::thread::sleep(rng.ms(120, 220));
        monitor.clear();
        let at = Instant::now();
        assert_eq!(
            engine.play(&bank, click, PlayParams::default()),
            Played::Queued
        );
        calls.push(ms(at.elapsed()));
        if let Some(b) = monitor.next(at, Duration::from_millis(600)) {
            heard.push(ms(b.start.duration_since(at)));
            lens.push(b.ms);
        }
        mixed.extend(engine.take_latencies().into_iter().map(ms));
    }
    let after = engine.stats();
    println!(
        "{}",
        stats("stream running: the play call itself", &mut calls)
    );
    println!(
        "{}",
        stats("stream running: play -> handed to the server", &mut mixed)
    );
    println!(
        "{}",
        stats(
            "stream running: play -> on the sink (upper bound)",
            &mut heard
        )
    );
    println!("{}", stats("  bursts heard, length", &mut lens));
    println!(
        "  the stream was stopped {} time(s) in between",
        after.corks - before.corks
    );

    // Overlap: eight plays 5 ms apart are one longer, louder burst.
    std::thread::sleep(Duration::from_millis(4200));
    monitor.clear();
    let at = Instant::now();
    for _ in 0..8 {
        engine.play(&bank, click, PlayParams::default());
        std::thread::sleep(Duration::from_millis(5));
    }
    match monitor.next(at, Duration::from_millis(800)) {
        Some(b) => println!(
            "overlap: 8 plays 5 ms apart; heard one burst of {:.0} ms, peak {}",
            b.ms, b.peak
        ),
        None => println!("overlap: 8 plays 5 ms apart; nothing heard"),
    }

    // At rest again, after all that: corked and flushed.
    drop(monitor);
    assert!(until(Duration::from_secs(3), || {
        let s = engine.stats();
        s.corks == s.uncorks
    }));
    std::thread::sleep(Duration::from_millis(300));
    let (cpu, s0) = (cpu_ms(), engine.stats());
    std::thread::sleep(Duration::from_secs_f64(rest));
    let s1 = engine.stats();
    println!(
        "at rest (corked after playing): {:.4}% of one core; {:.1} requests and {:.1} sound-thread wake-ups a second; {} threads",
        (cpu_ms() - cpu) / (rest * 10.0),
        (s1.requests - s0.requests) as f64 / rest,
        (s1.wakes - s0.wakes) as f64 / rest,
        threads()
    );

    // Unloading: fade, cork, flush, close; the threads go.
    engine.play(&bank, hum, PlayParams::default());
    std::thread::sleep(Duration::from_millis(200));
    let mut engine = engine;
    let t = Instant::now();
    engine.unload();
    let call = ms(t.elapsed());
    let gone = until(Duration::from_secs(3), || threads() == threads0);
    println!(
        "unload while sounding: the call took {call:.3} ms; threads back to {} after {:.0} ms ({})",
        threads(),
        ms(t.elapsed()),
        if gone { "all gone" } else { "NOT all gone" }
    );
    drop(engine);
    std::thread::sleep(Duration::from_millis(200));
    println!(
        "all closed: RSS {} KiB, {} thread(s); connection attempts in this run: {}",
        rss_kib(),
        threads(),
        gmnb_sound::connection_attempts()
    );
}
