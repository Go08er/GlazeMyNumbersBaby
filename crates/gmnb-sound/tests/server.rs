//! The engine against a stand-in sound server: a few threads in this test
//! that speak enough of PulseAudio's protocol to take one playback stream,
//! ask it for audio while it runs, and note what the engine does. Nothing
//! is played and no real sound server is involved: the socket is in a
//! folder of the test's own.
//!
//! This is where the stream's rules are held in `cargo test`: created
//! corked, given nothing while corked, audio in front of the request to
//! start, corked and flushed 100 ms after the last voice; a play that
//! arrives while the server is slow to answer; a server that goes away and
//! comes back.

use std::io::{Cursor, Read};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gmnb_sound::{
    Bank, Env, Glide, Layer, PlayParams, Played, Recipe, Settings, Sink, SoundEngine, SoundId,
    SoundSource, Status, ThemeSounds,
};
use pulseaudio::protocol::{self, Command};

/// What the stand-in server saw, in order.
#[derive(Clone, Debug, PartialEq)]
enum Saw {
    /// A stream was created; whether corked.
    Stream {
        corked: bool,
    },
    /// Audio: how many bytes, and whether any of it was not silence.
    Audio {
        bytes: usize,
        sound: bool,
    },
    Cork(bool),
    Flush,
    Delete,
    /// The client hung up.
    Closed,
}

/// One connection's stream, as the server keeps it.
#[derive(Default)]
struct StreamState {
    exists: bool,
    corked: bool,
    /// Bytes held, and bytes asked of the client and not yet given.
    held: usize,
    asked: usize,
}

const TARGET: usize = 3072;
const VERSION: u16 = 35;

struct Server {
    path: PathBuf,
    saw: Arc<Mutex<Vec<Saw>>>,
    /// How long the server takes to answer a cork or a flush.
    slow: Arc<Mutex<Duration>>,
    stop: Arc<AtomicBool>,
    /// The connections taken, to hang up on.
    conns: Arc<Mutex<Vec<UnixStream>>>,
}

impl Server {
    fn start(path: &Path) -> Server {
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path).unwrap();
        let server = Server {
            path: path.to_path_buf(),
            saw: Arc::default(),
            slow: Arc::default(),
            stop: Arc::default(),
            conns: Arc::default(),
        };
        let (saw, slow, stop, conns) = (
            server.saw.clone(),
            server.slow.clone(),
            server.stop.clone(),
            server.conns.clone(),
        );
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else { break };
                if stop.load(Ordering::Acquire) {
                    break;
                }
                conns.lock().unwrap().push(conn.try_clone().unwrap());
                let (saw, slow, stop) = (saw.clone(), slow.clone(), stop.clone());
                std::thread::spawn(move || serve(conn, saw, slow, stop));
            }
        });
        server
    }

    fn saw(&self) -> Vec<Saw> {
        self.saw.lock().unwrap().clone()
    }

    fn count(&self, what: impl Fn(&Saw) -> bool) -> usize {
        self.saw().iter().filter(|s| what(s)).count()
    }

    /// Bytes of audio taken so far.
    fn audio(&self) -> usize {
        self.saw()
            .iter()
            .map(|s| match s {
                Saw::Audio { bytes, .. } => *bytes,
                _ => 0,
            })
            .sum()
    }

    /// Goes away: hangs up on everyone and stops listening.
    fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        for conn in self.conns.lock().unwrap().drain(..) {
            let _ = conn.shutdown(std::net::Shutdown::Both);
        }
        // Wake the accepting thread, so that it sees it is to stop.
        let _ = UnixStream::connect(&self.path);
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

/// One client: answers its commands, and asks for audio every 5 ms while
/// its stream runs.
fn serve(
    mut conn: UnixStream,
    saw: Arc<Mutex<Vec<Saw>>>,
    slow: Arc<Mutex<Duration>>,
    stop: Arc<AtomicBool>,
) {
    let note = |s: Saw| saw.lock().unwrap().push(s);
    let out = Arc::new(Mutex::new(conn.try_clone().unwrap()));
    let state = Arc::new(Mutex::new(StreamState::default()));
    let done = Arc::new(AtomicBool::new(false));

    // The server's clock: plays what it holds, and asks for more.
    let (out2, state2, done2, stop2) = (out.clone(), state.clone(), done.clone(), stop.clone());
    std::thread::spawn(move || {
        while !done2.load(Ordering::Acquire) && !stop2.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
            let mut st = state2.lock().unwrap();
            if !st.exists || st.corked {
                continue;
            }
            st.held = st.held.saturating_sub(960);
            let want = TARGET.saturating_sub(st.held + st.asked);
            if want >= 960 {
                st.asked += want;
                let request = Command::Request(protocol::Request {
                    channel: 0,
                    length: want as u32,
                });
                let mut out = out2.lock().unwrap();
                if protocol::write_command_message(&mut *out, u32::MAX, &request, VERSION).is_err()
                {
                    break;
                }
            }
        }
    });

    let mut payload = Vec::new();
    while let Ok(desc) = protocol::read_descriptor(&mut conn) {
        payload.resize(desc.length as usize, 0);
        if conn.read_exact(&mut payload).is_err() {
            break;
        }
        if desc.channel != u32::MAX {
            let mut st = state.lock().unwrap();
            st.held += payload.len();
            st.asked = st.asked.saturating_sub(payload.len());
            note(Saw::Audio {
                bytes: payload.len(),
                sound: payload.iter().any(|b| *b != 0),
            });
            continue;
        }
        let Ok((seq, cmd)) = Command::read_tag_prefixed(&mut Cursor::new(&payload), VERSION) else {
            break;
        };
        let wait = *slow.lock().unwrap();
        let ack = |out: &Mutex<UnixStream>| {
            let mut out = out.lock().unwrap();
            protocol::write_ack_message(&mut *out, seq).is_ok()
        };
        let ok = match cmd {
            Command::Auth(_) => {
                let reply = protocol::AuthReply {
                    version: VERSION,
                    ..Default::default()
                };
                let mut out = out.lock().unwrap();
                protocol::write_reply_message(&mut *out, seq, &reply, VERSION).is_ok()
            }
            Command::SetClientName(_) => {
                let reply = protocol::SetClientNameReply { client_id: 1 };
                let mut out = out.lock().unwrap();
                protocol::write_reply_message(&mut *out, seq, &reply, VERSION).is_ok()
            }
            Command::CreatePlaybackStream(params) => {
                let corked = params.flags.start_corked;
                {
                    let mut st = state.lock().unwrap();
                    st.exists = true;
                    st.corked = corked;
                }
                note(Saw::Stream { corked });
                let mut attr = params.buffer_attr;
                attr.max_length = 4 << 20;
                attr.target_length = TARGET as u32;
                let reply = protocol::CreatePlaybackStreamReply {
                    channel: 0,
                    stream_index: 0,
                    sample_spec: params.sample_spec,
                    channel_map: params.channel_map,
                    buffer_attr: attr,
                    requested_bytes: 0,
                    ..Default::default()
                };
                let mut out = out.lock().unwrap();
                protocol::write_reply_message(&mut *out, seq, &reply, VERSION).is_ok()
            }
            Command::CorkPlaybackStream(p) => {
                std::thread::sleep(wait);
                state.lock().unwrap().corked = p.cork;
                note(Saw::Cork(p.cork));
                ack(&out)
            }
            Command::FlushPlaybackStream(_) => {
                std::thread::sleep(wait);
                state.lock().unwrap().held = 0;
                note(Saw::Flush);
                ack(&out)
            }
            Command::DeletePlaybackStream(_) => {
                state.lock().unwrap().exists = false;
                note(Saw::Delete);
                ack(&out)
            }
            _ => {
                let mut out = out.lock().unwrap();
                protocol::write_error(&mut *out, seq, &protocol::PulseError::NotImplemented).is_ok()
            }
        };
        if !ok {
            break;
        }
    }
    done.store(true, Ordering::Release);
    note(Saw::Closed);
}

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
