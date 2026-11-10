//! The output (docs/theming-media.md, "The mixer and the stream"):
//! PulseAudio's native protocol over its unix socket, spoken by the
//! `pulseaudio` crate. PipeWire's `pipewire-pulse` and PulseAudio itself
//! serve it.
//!
//! One playback stream: signed 16-bit, stereo, 48 kHz, created corked. The
//! crate's reactor thread asks the [`Source`] for audio whenever the
//! server wants some; the source mixes while the stream is in use and
//! gives nothing while it is corked, so that a corked stream costs no
//! wake-up at all.
//!
//! Every wait on the server has a time limit: a server that stops
//! answering leaves the sound side "unavailable", never stuck.

use std::ffi::CString;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::Thread;
use std::time::{Duration, Instant};

use pulseaudio::protocol::stream::{BufferAttr, StreamFlags};
use pulseaudio::protocol::{self, Prop, Props};
use pulseaudio::{Client, PlaybackSource, PlaybackStream};

use crate::mixer::Mixer;

/// Bytes in a millisecond of the stream (48 kHz, stereo, 16-bit).
const BYTES_PER_MS: u32 = 192;
/// How much the server should hold: the latency asked for.
const TARGET_MS: u32 = 20;
/// The least the server asks for at once.
const REQUEST_MS: u32 = 5;
/// The most frames mixed for one request.
const MAX_FRAMES: usize = 4096;
/// Silence fed after the last voice before the stream is corked, in
/// frames (100 ms): what was written is heard first.
pub const QUIET_FRAMES: u64 = 4800;

static ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// How many times this process has tried to connect to a sound server.
pub fn connection_attempts() -> u64 {
    ATTEMPTS.load(Ordering::Relaxed)
}

/// A mutex's value, whatever a panic elsewhere did to it.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---- Where the server is ---------------------------------------------------

/// The server's socket, from the environment: `$PULSE_SERVER` if it is
/// `unix:<path>`, else `$PULSE_RUNTIME_PATH/native`, else
/// `$XDG_RUNTIME_DIR/pulse/native`; the first that is there. A
/// `PULSE_SERVER` of any other kind is not followed (no TCP).
pub fn socket_path(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    socket_candidates(env).into_iter().find(|p| p.exists())
}

fn socket_candidates(env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let set = |name: &str| env(name).filter(|v| !v.is_empty());
    let mut out = Vec::new();
    if let Some(server) = set("PULSE_SERVER")
        && let Some(path) = server.strip_prefix("unix:")
        && path.starts_with('/')
    {
        out.push(PathBuf::from(path));
    }
    if let Some(dir) = set("PULSE_RUNTIME_PATH") {
        out.push(Path::new(&dir).join("native"));
    }
    if let Some(dir) = set("XDG_RUNTIME_DIR") {
        out.push(Path::new(&dir).join("pulse/native"));
    }
    out
}

/// The cookie to greet the server with, always 256 bytes:
/// `$PULSE_COOKIE`'s file, else `~/.config/pulse/cookie`, else zeros.
/// (`pipewire-pulse` answers a shorter one with `Invalid`.)
pub fn cookie(env: &dyn Fn(&str) -> Option<String>) -> Vec<u8> {
    let set = |name: &str| env(name).filter(|v| !v.is_empty());
    let config = set("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| set("HOME").map(|h| Path::new(&h).join(".config")));
    let files = [
        set("PULSE_COOKIE").map(PathBuf::from),
        config.map(|c| c.join("pulse/cookie")),
    ];
    for file in files.into_iter().flatten() {
        // A cookie is 256 bytes; anything else there is not one.
        let small = std::fs::metadata(&file).is_ok_and(|m| m.is_file() && m.len() == 256);
        if small
            && let Ok(bytes) = std::fs::read(&file)
            && bytes.len() == 256
        {
            return bytes;
        }
    }
    vec![0; 256]
}

/// How the stream names itself to the server (what a volume control
/// shows).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    /// The application id.
    pub id: String,
    pub icon: String,
}

impl Default for Identity {
    fn default() -> Identity {
        Identity {
            name: "GMNB".into(),
            id: "io.github.Go08er.GlazeMyNumbersBaby".into(),
            icon: "io.github.Go08er.GlazeMyNumbersBaby".into(),
        }
    }
}

// ---- What the reactor's thread and the sound thread share ------------------------

/// The mixer, and the few flags the two threads tell each other things
/// with.
pub struct Shared {
    mixer: Mutex<Mixer>,
    /// The connection whose source is answered; 0 for none.
    current: AtomicU64,
    /// Whether requests are answered with audio (the stream is in use) or
    /// with nothing (it is corked).
    feeding: AtomicBool,
    /// The last connection that went away by itself; 0 for none.
    lost: AtomicU64,
    /// 100 ms of silence have been fed since the last voice: time to cork.
    quiet: AtomicBool,
    /// Wakes the reactor, so that it asks the source again.
    waker: Mutex<Option<Waker>>,
    /// The sound thread, woken when `quiet` or `lost` is set.
    owner: Thread,
    /// Times the source was asked, and frames it gave.
    polls: AtomicU64,
    fed: AtomicU64,
    /// For measurements: each time the source was asked, when, for how
    /// many bytes, and how many it gave.
    trace: Mutex<Option<Vec<(Instant, usize, usize)>>>,
}

impl Shared {
    /// `owner` is the thread to wake when the stream should be corked or
    /// is gone.
    pub fn new(mixer: Mixer, owner: Thread) -> Arc<Shared> {
        Arc::new(Shared {
            mixer: Mutex::new(mixer),
            current: AtomicU64::new(0),
            feeding: AtomicBool::new(false),
            lost: AtomicU64::new(0),
            quiet: AtomicBool::new(false),
            waker: Mutex::new(None),
            owner,
            polls: AtomicU64::new(0),
            fed: AtomicU64::new(0),
            trace: Mutex::new(None),
        })
    }

    /// The mixer. Never held across a call to the server: the reactor
    /// asks the source while holding its own lock, and a call to the
    /// server needs that one.
    pub fn mixer(&self) -> MutexGuard<'_, Mixer> {
        lock(&self.mixer)
    }

    /// Whether 100 ms of silence have gone out since the last voice; the
    /// flag is taken.
    pub fn take_quiet(&self) -> bool {
        self.quiet.swap(false, Ordering::AcqRel)
    }

    /// Times the source was asked for audio.
    pub fn polls(&self) -> u64 {
        self.polls.load(Ordering::Relaxed)
    }

    /// Frames given to the server.
    pub fn fed(&self) -> u64 {
        self.fed.load(Ordering::Relaxed)
    }

    /// Starts (or stops) keeping a note of every request, for
    /// measurements, and returns the notes so far.
    pub fn trace(&self, on: bool) -> Vec<(Instant, usize, usize)> {
        let mut slot = lock(&self.trace);
        let old = slot.take().unwrap_or_default();
        *slot = on.then(Vec::new);
        old
    }

    fn note(&self, asked: usize, given: usize) {
        if let Some(notes) = lock(&self.trace).as_mut()
            && notes.len() < 100_000
        {
            notes.push((Instant::now(), asked, given));
        }
    }

    fn wake_reactor(&self) {
        if let Some(w) = lock(&self.waker).as_ref() {
            w.wake_by_ref();
        }
    }
}

/// What the reactor asks for audio.
struct Source {
    shared: Arc<Shared>,
    id: u64,
    scratch: Vec<i16>,
}

impl Source {
    fn fill(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<usize> {
        let shared = &self.shared;
        shared.polls.fetch_add(1, Ordering::Relaxed);
        {
            let mut slot = lock(&shared.waker);
            if !slot.as_ref().is_some_and(|w| w.will_wake(cx.waker())) {
                *slot = Some(cx.waker().clone());
            }
        }
        // Corked, or no longer the connection in use: nothing. The
        // request stays open, and the reactor asks again when woken.
        if shared.current.load(Ordering::Acquire) != self.id
            || !shared.feeding.load(Ordering::Acquire)
        {
            shared.note(buf.len(), 0);
            return Poll::Pending;
        }
        let frames = (buf.len() / 4).min(MAX_FRAMES);
        if frames == 0 {
            return Poll::Pending;
        }
        shared.note(buf.len(), frames * 4);
        self.scratch.clear();
        self.scratch.resize(frames * 2, 0);
        let quiet = {
            let mut mixer = shared.mixer();
            mixer.mix(&mut self.scratch);
            mixer.quiet_frames()
        };
        for (bytes, sample) in buf.as_chunks_mut::<2>().0.iter_mut().zip(&self.scratch) {
            *bytes = sample.to_le_bytes();
        }
        shared.fed.fetch_add(frames as u64, Ordering::Relaxed);
        if quiet >= QUIET_FRAMES && !shared.quiet.swap(true, Ordering::AcqRel) {
            shared.owner.unpark();
        }
        Poll::Ready(frames * 4)
    }
}

impl PlaybackSource for Source {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<usize> {
        let this = self.get_mut();
        // The reactor holds its lock here: a panic would poison it and
        // take the sound thread's next call down with it.
        let filled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| this.fill(cx, buf)));
        filled.unwrap_or_else(|_| {
            this.shared.feeding.store(false, Ordering::Release);
            this.shared.lost.store(this.id, Ordering::Release);
            this.shared.owner.unpark();
            Poll::Pending
        })
    }
}

impl Drop for Source {
    /// The reactor lets go of the source when the stream is deleted or the
    /// connection ends, whoever ended it.
    fn drop(&mut self) {
        self.shared.lost.store(self.id, Ordering::Release);
        self.shared.owner.unpark();
    }
}

// ---- Waiting, with a limit --------------------------------------------------------

struct Unpark(Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Runs `fut` on this thread; `None` if it has not finished in `limit`.
///
/// The thread sleeps by parking, and so does the sound thread's own loop.
/// A wake-up meant for that loop (a play, or being told to stop) that
/// comes while the thread is parked here is used up here. So one is left
/// behind on the way out: the caller's loop goes round once more and looks
/// at its queues before it sleeps. (Without this a play that arrived
/// during a cork stayed in the queue until the next play.)
fn wait<F: Future>(fut: F, limit: Duration) -> Option<F::Output> {
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut fut = pin!(fut);
    let deadline = Instant::now() + limit;
    let result = loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            break Some(v);
        }
        match deadline.checked_duration_since(Instant::now()) {
            Some(left) => std::thread::park_timeout(left),
            None => break None,
        }
    };
    std::thread::current().unpark();
    result
}

fn answered<T>(
    what: &str,
    result: Option<Result<T, pulseaudio::ClientError>>,
) -> Result<T, String> {
    match result {
        Some(Ok(v)) => Ok(v),
        Some(Err(e)) => Err(format!("{what}: {e}")),
        None => Err(format!("{what}: the sound server did not answer")),
    }
}

// ---- The stream ---------------------------------------------------------------------

/// A connection with its one playback stream.
pub struct Output {
    stream: PlaybackStream,
    // Dropped after the stream.
    _client: Client,
    shared: Arc<Shared>,
    id: u64,
    corked: bool,
    limit: Duration,
}

impl Output {
    /// Connects to the server at `socket` and creates the stream, corked.
    /// `limit` bounds every wait on the server, this one included.
    pub fn open(
        socket: &Path,
        cookie: &[u8],
        identity: &Identity,
        shared: &Arc<Shared>,
        limit: Duration,
    ) -> Result<Output, String> {
        ATTEMPTS.fetch_add(1, Ordering::Relaxed);
        let text = |s: &str| CString::new(s.replace('\0', "")).unwrap_or_default();
        let conn = std::os::unix::net::UnixStream::connect(socket)
            .map_err(|e| format!("{}: {e}", socket.display()))?;
        // The greeting is read with blocking reads.
        conn.set_read_timeout(Some(limit))
            .and_then(|()| conn.set_write_timeout(Some(limit)))
            .map_err(|e| format!("{}: {e}", socket.display()))?;
        let client = Client::new_unix(text(&identity.name), conn, Some(cookie))
            .map_err(|e| format!("{}: {e}", socket.display()))?;

        let mut props = Props::new();
        props.set(Prop::MediaRole, c"event");
        props.set(Prop::MediaName, c"Theme sounds");
        props.set(Prop::ApplicationName, text(&identity.name));
        props.set(Prop::ApplicationId, text(&identity.id));
        props.set(Prop::ApplicationIconName, text(&identity.icon));
        let params = protocol::PlaybackStreamParams {
            sample_spec: protocol::SampleSpec {
                format: protocol::SampleFormat::S16Le,
                channels: 2,
                sample_rate: crate::recipe::RATE,
            },
            channel_map: protocol::ChannelMap::stereo(),
            cvolume: Some(protocol::ChannelVolume::norm(2)),
            sink_name: Some(protocol::DEFAULT_SINK.to_owned()),
            buffer_attr: BufferAttr {
                max_length: u32::MAX,
                target_length: TARGET_MS * BYTES_PER_MS,
                pre_buffering: 0,
                minimum_request_length: REQUEST_MS * BYTES_PER_MS,
                fragment_size: u32::MAX,
            },
            flags: StreamFlags {
                adjust_latency: true,
                start_corked: true,
                ..Default::default()
            },
            props,
            ..Default::default()
        };
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let source = Source {
            shared: shared.clone(),
            id,
            scratch: Vec::new(),
        };
        shared.feeding.store(false, Ordering::Release);
        shared.quiet.store(false, Ordering::Release);
        let stream = answered(
            "creating the stream",
            wait(client.create_playback_stream(params, source), limit),
        )?;
        let spec = stream.sample_spec();
        if spec.format != protocol::SampleFormat::S16Le
            || spec.channels != 2
            || spec.sample_rate != crate::recipe::RATE
        {
            return Err("the sound server changed the stream's format".into());
        }
        shared.current.store(id, Ordering::Release);
        Ok(Output {
            stream,
            _client: client,
            shared: shared.clone(),
            id,
            corked: true,
            limit,
        })
    }

    pub fn is_corked(&self) -> bool {
        self.corked
    }

    /// Whether the connection went away by itself.
    pub fn is_lost(&self) -> bool {
        self.shared.lost.load(Ordering::Acquire) == self.id
    }

    /// The server's buffer: what it holds at most before playing, and the
    /// least it asks for at once, in milliseconds.
    pub fn buffer_ms(&self) -> (f64, f64) {
        let a = self.stream.buffer_attr();
        let ms = |bytes: u32| f64::from(bytes) / f64::from(BYTES_PER_MS);
        (ms(a.target_length), ms(a.minimum_request_length))
    }

    /// Starts the stream. The source answers from now on, so the first
    /// frames go out with the request to start, not after its answer.
    pub fn uncork(&mut self) -> Result<(), String> {
        self.shared.quiet.store(false, Ordering::Release);
        self.shared.feeding.store(true, Ordering::Release);
        let result = answered(
            "starting the stream",
            wait(self.stream.uncork(), self.limit),
        );
        // Whatever the reactor made of the answer, it asks the source
        // again now.
        self.shared.wake_reactor();
        result?;
        self.corked = false;
        Ok(())
    }

    /// Stops the server asking for audio and drops what it still holds
    /// (silence left queued would play first on the next start, in front
    /// of the sound).
    pub fn cork(&mut self) -> Result<(), String> {
        self.shared.feeding.store(false, Ordering::Release);
        self.corked = true;
        answered("stopping the stream", wait(self.stream.cork(), self.limit))?;
        answered("emptying the stream", wait(self.stream.flush(), self.limit))?;
        Ok(())
    }

    /// Corks, flushes and deletes the stream, then lets the connection
    /// go. Waits `limit` at most for each answer.
    pub fn close(mut self, limit: Duration) {
        self.limit = limit;
        if !self.is_lost() {
            let _ = self.cork();
            let _ = wait(self.stream.clone().delete(), limit);
        }
        self.shared.current.store(0, Ordering::Release);
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.shared.feeding.store(false, Ordering::Release);
        let _ =
            self.shared
                .current
                .compare_exchange(self.id, 0, Ordering::AcqRel, Ordering::Acquire);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn the_socket_is_looked_for_in_order() {
        let paths = |vars: &[(&str, &str)]| -> Vec<String> {
            socket_candidates(&env(vars))
                .iter()
                .map(|p| p.display().to_string())
                .collect()
        };
        assert_eq!(
            paths(&[
                ("PULSE_SERVER", "unix:/run/flatpak/pulse/native"),
                ("PULSE_RUNTIME_PATH", "/run/p"),
                ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ]),
            [
                "/run/flatpak/pulse/native",
                "/run/p/native",
                "/run/user/1000/pulse/native"
            ]
        );
        // No TCP, no host names, nothing relative: such a PULSE_SERVER is
        // not followed.
        for server in ["tcp:example.org:4713", "example.org", "unix:relative", ""] {
            assert_eq!(
                paths(&[("PULSE_SERVER", server), ("XDG_RUNTIME_DIR", "/run/u")]),
                ["/run/u/pulse/native"],
                "{server:?}"
            );
        }
        assert!(paths(&[]).is_empty());
        assert_eq!(socket_path(&env(&[])), None);
        assert_eq!(
            socket_path(&env(&[("XDG_RUNTIME_DIR", "/nonexistent/gmnb-sound-test")])),
            None
        );
    }

    #[test]
    fn the_cookie_is_always_256_bytes() {
        let dir = std::env::temp_dir().join(format!("gmnb-sound-cookie-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pulse")).unwrap();
        let none = dir.join("none").display().to_string();
        assert_eq!(cookie(&env(&[])), vec![0; 256]);
        assert_eq!(cookie(&env(&[("PULSE_COOKIE", &none)])), vec![0; 256]);

        let good = dir.join("good");
        std::fs::write(&good, [7u8; 256]).unwrap();
        let good = good.display().to_string();
        assert_eq!(cookie(&env(&[("PULSE_COOKIE", &good)])), vec![7; 256]);

        // One of another size is not a cookie: the next place is tried.
        let short = dir.join("short");
        std::fs::write(&short, [9u8; 12]).unwrap();
        let short = short.display().to_string();
        std::fs::write(dir.join("pulse/cookie"), [3u8; 256]).unwrap();
        let config = dir.display().to_string();
        assert_eq!(
            cookie(&env(&[
                ("PULSE_COOKIE", &short),
                ("XDG_CONFIG_HOME", &config)
            ])),
            vec![3; 256]
        );
        assert_eq!(cookie(&env(&[("PULSE_COOKIE", &short)])), vec![0; 256]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn waiting_has_a_limit() {
        let started = Instant::now();
        let never = std::future::pending::<()>();
        assert_eq!(wait(never, Duration::from_millis(30)), None);
        assert!(started.elapsed() >= Duration::from_millis(30));
        assert_eq!(wait(std::future::ready(5), Duration::ZERO), Some(5));
    }
}
