//! A stand-in sound server, for tests (the `stand-in` feature): a few
//! threads that speak enough of PulseAudio's protocol to take one playback
//! stream per connection, ask it for audio while it runs, and note what
//! the client does and when. Nothing is played, and no real sound server
//! is involved: it listens on a socket the test gives it, in a folder of
//! the test's own.
//!
//! `tests/server.rs` holds the engine's stream to its rules with it; the
//! app's tests count the connections it is made (none in the base look)
//! and time a press to the moment its sound reaches the server.
//!
//! Self-contained (std and `pulseaudio::protocol` only): `tests/server.rs`
//! includes this file directly.

use std::io::{Cursor, Read};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pulseaudio::protocol::{self, Command};

/// What the stand-in server saw, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Saw {
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

/// What the server holds of a running stream (16 ms at 48 kHz stereo).
const TARGET: usize = 3072;
const VERSION: u16 = 35;

type Log = Arc<Mutex<Vec<(Instant, Saw)>>>;

pub struct Server {
    path: PathBuf,
    saw: Log,
    /// How long the server takes to answer a cork or a flush.
    pub slow: Arc<Mutex<Duration>>,
    stop: Arc<AtomicBool>,
    /// The connections taken, to hang up on.
    conns: Arc<Mutex<Vec<UnixStream>>>,
    accepted: Arc<AtomicUsize>,
}

impl Server {
    /// Listens at `path` (whatever was there is removed first).
    pub fn start(path: &Path) -> Server {
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path).expect("the stand-in server's socket");
        let server = Server {
            path: path.to_path_buf(),
            saw: Arc::default(),
            slow: Arc::default(),
            stop: Arc::default(),
            conns: Arc::default(),
            accepted: Arc::default(),
        };
        let (saw, slow, stop, conns, accepted) = (
            server.saw.clone(),
            server.slow.clone(),
            server.stop.clone(),
            server.conns.clone(),
            server.accepted.clone(),
        );
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(conn) = conn else { break };
                if stop.load(Ordering::Acquire) {
                    break;
                }
                accepted.fetch_add(1, Ordering::AcqRel);
                if let Ok(c) = conn.try_clone() {
                    conns.lock().unwrap_or_else(|e| e.into_inner()).push(c);
                }
                let (saw, slow, stop) = (saw.clone(), slow.clone(), stop.clone());
                std::thread::spawn(move || serve(conn, saw, slow, stop));
            }
        });
        server
    }

    /// The connections it has taken.
    pub fn connections(&self) -> usize {
        self.accepted.load(Ordering::Acquire)
    }

    /// What it saw, in order.
    pub fn saw(&self) -> Vec<Saw> {
        self.timed().into_iter().map(|(_, s)| s).collect()
    }

    /// What it saw, each with when.
    pub fn timed(&self) -> Vec<(Instant, Saw)> {
        self.saw.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn count(&self, what: impl Fn(&Saw) -> bool) -> usize {
        self.saw().iter().filter(|s| what(s)).count()
    }

    /// Bytes of audio taken so far.
    pub fn audio(&self) -> usize {
        self.saw()
            .iter()
            .map(|s| match s {
                Saw::Audio { bytes, .. } => *bytes,
                _ => 0,
            })
            .sum()
    }

    /// When the first audio that was not silence arrived after `since`.
    pub fn sound_after(&self, since: Instant) -> Option<Instant> {
        self.timed()
            .into_iter()
            .find(|(t, s)| *t >= since && matches!(s, Saw::Audio { sound: true, .. }))
            .map(|(t, _)| t)
    }

    /// Goes away: hangs up on everyone and stops listening.
    pub fn stop(&self) {
        if self.stop.swap(true, Ordering::AcqRel) {
            return;
        }
        for conn in self
            .conns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
        {
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
fn serve(mut conn: UnixStream, saw: Log, slow: Arc<Mutex<Duration>>, stop: Arc<AtomicBool>) {
    let note = |s: Saw| {
        saw.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((Instant::now(), s))
    };
    let Ok(out) = conn.try_clone() else { return };
    let out = Arc::new(Mutex::new(out));
    let state = Arc::new(Mutex::new(StreamState::default()));
    let done = Arc::new(AtomicBool::new(false));

    // The server's clock: plays what it holds, and asks for more.
    let (out2, state2, done2, stop2) = (out.clone(), state.clone(), done.clone(), stop.clone());
    std::thread::spawn(move || {
        while !done2.load(Ordering::Acquire) && !stop2.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(5));
            let mut st = state2.lock().unwrap_or_else(|e| e.into_inner());
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
                let mut out = out2.lock().unwrap_or_else(|e| e.into_inner());
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
            let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
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
        let wait = *slow.lock().unwrap_or_else(|e| e.into_inner());
        let reply = |write: &dyn Fn(&mut UnixStream) -> bool| {
            let mut out = out.lock().unwrap_or_else(|e| e.into_inner());
            write(&mut out)
        };
        let ok = match cmd {
            Command::Auth(_) => reply(&|out| {
                let r = protocol::AuthReply {
                    version: VERSION,
                    ..Default::default()
                };
                protocol::write_reply_message(out, seq, &r, VERSION).is_ok()
            }),
            Command::SetClientName(_) => reply(&|out| {
                let r = protocol::SetClientNameReply { client_id: 1 };
                protocol::write_reply_message(out, seq, &r, VERSION).is_ok()
            }),
            Command::CreatePlaybackStream(params) => {
                let corked = params.flags.start_corked;
                {
                    let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                    st.exists = true;
                    st.corked = corked;
                }
                note(Saw::Stream { corked });
                let mut attr = params.buffer_attr;
                attr.max_length = 4 << 20;
                attr.target_length = TARGET as u32;
                let r = protocol::CreatePlaybackStreamReply {
                    channel: 0,
                    stream_index: 0,
                    sample_spec: params.sample_spec,
                    channel_map: params.channel_map,
                    buffer_attr: attr,
                    requested_bytes: 0,
                    ..Default::default()
                };
                reply(&|out| protocol::write_reply_message(out, seq, &r, VERSION).is_ok())
            }
            Command::CorkPlaybackStream(p) => {
                std::thread::sleep(wait);
                state.lock().unwrap_or_else(|e| e.into_inner()).corked = p.cork;
                note(Saw::Cork(p.cork));
                reply(&|out| protocol::write_ack_message(out, seq).is_ok())
            }
            Command::FlushPlaybackStream(_) => {
                std::thread::sleep(wait);
                state.lock().unwrap_or_else(|e| e.into_inner()).held = 0;
                note(Saw::Flush);
                reply(&|out| protocol::write_ack_message(out, seq).is_ok())
            }
            Command::DeletePlaybackStream(_) => {
                state.lock().unwrap_or_else(|e| e.into_inner()).exists = false;
                note(Saw::Delete);
                reply(&|out| protocol::write_ack_message(out, seq).is_ok())
            }
            _ => reply(&|out| {
                protocol::write_error(out, seq, &protocol::PulseError::NotImplemented).is_ok()
            }),
        };
        if !ok {
            break;
        }
    }
    done.store(true, Ordering::Release);
    note(Saw::Closed);
}
