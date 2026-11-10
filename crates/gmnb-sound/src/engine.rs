//! The sound engine the app holds (docs/theming-media.md, "The mixer and
//! the stream", "Environment hooks").
//!
//! Nothing exists until a theme that has sounds is loaded while sound is
//! on: no thread, no socket. Then there is one thread of ours (it renders
//! and decodes the theme's sounds, takes the plays, and starts and stops
//! the stream) and, with a sound server, the client's reactor thread,
//! which is where the mixing happens. Unloading the theme or switching
//! sound off fades out what is sounding and lets all of it go.
//!
//! [`SoundEngine::play`] never waits: it is one push onto a bounded queue
//! and one wake.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{JoinHandle, Thread};
use std::time::{Duration, Instant};

use crate::bank::{Budget, SoundSource, ThemeSounds, dump_name, prepare};
use crate::mixer::{MixStats, Mixer, PlayParams, STOP_FADE};
use crate::pulse::{self, Identity, Output, QUIET_FRAMES, Shared, lock};
use crate::recipe::limits;
use crate::table::SoundId;

/// A play still in the queue this long after it was asked for (and after
/// its own delay) is dropped: a sound is never played late.
const LATE: Duration = Duration::from_millis(50);
/// Frames the null output takes at a time (5.3 ms).
const NULL_QUANTUM: usize = 256;
/// How long dropping the engine waits for its thread before leaving it
/// behind (a sound server that has stopped answering mid-greeting).
const JOIN_LIMIT: Duration = Duration::from_secs(5);
/// How long the stream is given to play out a fade before it is closed.
const TAIL: Duration = Duration::from_millis(20);

/// Where the sound goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sink {
    /// Nowhere, and nothing is created: `GMNB_SOUND=0`.
    Off,
    /// The mixer runs into a sink that discards its output, with no
    /// server: `GMNB_SOUND=null`, and what tests use.
    Null,
    /// The sound server at this socket, greeted with this cookie.
    Socket { path: PathBuf, cookie: Vec<u8> },
    /// The session's sound server, looked for in the environment at each
    /// attempt ([`pulse::socket_path`], [`pulse::cookie`]).
    Session,
}

/// Where the engine's few lines for the terminal go.
pub type Log = Arc<dyn Fn(&str) + Send + Sync>;

/// How the engine is set up.
#[derive(Clone)]
pub struct Settings {
    /// Settings → Sound → Theme sounds (and the desktop's own switch).
    pub enabled: bool,
    /// The Settings volume, 0 to 1. The gain is its square: 0.5 is -12 dB.
    pub volume: f32,
    pub sink: Sink,
    /// `GMNB_SOUND_DUMP`: a folder every sound the theme makes is written
    /// to as a 16-bit WAV file.
    pub dump: Option<PathBuf>,
    /// How the stream is named to the server.
    pub identity: Identity,
    /// The least time between two attempts to connect that plays cause.
    pub retry: Duration,
    /// The longest any one wait on the sound server may take.
    pub server_limit: Duration,
    /// Where the lines for the terminal go; `None` is stderr.
    pub log: Option<Log>,
}

impl Default for Settings {
    /// Sound on at 50%, into the null sink: nothing here reaches a sound
    /// server. The app asks for the session's with [`Settings::from_env`].
    fn default() -> Settings {
        Settings {
            enabled: true,
            volume: 0.5,
            sink: Sink::Null,
            dump: None,
            identity: Identity::default(),
            retry: Duration::from_secs(5),
            server_limit: Duration::from_secs(2),
            log: None,
        }
    }
}

impl Settings {
    /// What the app uses: the session's sound server, unless the
    /// environment says otherwise (`GMNB_SOUND=0` or `null`,
    /// `GMNB_SOUND_DUMP=<folder>`).
    pub fn from_env(enabled: bool, volume: f32) -> Settings {
        Settings::from_lookup(enabled, volume, &|name| std::env::var(name).ok())
    }

    /// [`Settings::from_env`] with the environment given.
    pub fn from_lookup(
        enabled: bool,
        volume: f32,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Settings {
        let sink = match env("GMNB_SOUND").as_deref() {
            Some("0") => Sink::Off,
            Some("null") => Sink::Null,
            _ => Sink::Session,
        };
        Settings {
            enabled,
            volume,
            sink,
            dump: env("GMNB_SOUND_DUMP")
                .filter(|d| !d.is_empty())
                .map(PathBuf::from),
            ..Settings::default()
        }
    }
}

/// What the sound side is doing, for Settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Switched off (in Settings, by the desktop, or `GMNB_SOUND=0`).
    Off,
    /// On, with no theme loaded that has sounds: nothing exists.
    Idle,
    /// A theme with sounds was just loaded; the output is being set up.
    Starting,
    /// Sounds can play.
    Ready,
    /// "No sound system found": no socket, a refused connection, or a
    /// stream that failed. Plays are dropped.
    Unavailable,
}

impl Status {
    fn from_u8(v: u8) -> Status {
        match v {
            2 => Status::Starting,
            3 => Status::Ready,
            4 => Status::Unavailable,
            _ => Status::Idle,
        }
    }
}

/// What became of a call to [`SoundEngine::play`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Played {
    /// Handed to the sound thread.
    Queued,
    /// Sound is off, or nothing is loaded.
    Off,
    /// There is no sound system.
    Unavailable,
    /// The sound is not rendered yet, was refused, or is not one of the
    /// bank's.
    NotReady,
    /// The bank is not the one loaded.
    Stale,
    /// 64 plays were already waiting.
    Dropped,
}

/// Where one of a bank's sounds is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundState {
    /// Not one of the bank's.
    Unknown,
    /// Waiting to be rendered or decoded.
    Pending,
    /// Ready to play.
    Ready,
    /// Refused or silent: playing it plays nothing. (Why is among the
    /// bank's notes.)
    Refused,
}

const PENDING: u8 = 1;
const READY: u8 = 2;
const REFUSED: u8 = 3;

struct BankInner {
    id: u64,
    /// One per sound a theme may have; 0 where there is none.
    states: [AtomicU8; limits::SOUNDS],
    /// For the theme's author: which sound, and what about it.
    notes: Mutex<Vec<(SoundId, String)>>,
    /// Signalled each time a sound leaves `PENDING`.
    settled: Condvar,
    settle: Mutex<()>,
}

/// A loaded theme's sounds: what [`SoundEngine::load`] gives and
/// [`SoundEngine::play`] takes. The sounds themselves are with the sound
/// thread; this says how far along they are.
#[derive(Clone)]
pub struct Bank(Arc<BankInner>);

impl Bank {
    fn new(id: u64) -> Bank {
        Bank(Arc::new(BankInner {
            id,
            states: [const { AtomicU8::new(0) }; limits::SOUNDS],
            notes: Mutex::new(Vec::new()),
            settled: Condvar::new(),
            settle: Mutex::new(()),
        }))
    }

    pub fn state(&self, sound: SoundId) -> SoundState {
        let state = sound
            .index()
            .and_then(|i| self.0.states.get(i))
            .map(|s| s.load(Ordering::Acquire));
        match state {
            Some(PENDING) => SoundState::Pending,
            Some(READY) => SoundState::Ready,
            Some(REFUSED) => SoundState::Refused,
            _ => SoundState::Unknown,
        }
    }

    /// Whether every sound has been rendered or refused.
    pub fn is_settled(&self) -> bool {
        self.0
            .states
            .iter()
            .all(|s| s.load(Ordering::Acquire) != PENDING)
    }

    /// Waits until every sound has been rendered or refused; `false` if
    /// that takes longer than `limit`. (For tools and tests: the app
    /// never waits.)
    pub fn wait_settled(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        let mut guard = lock(&self.0.settle);
        while !self.is_settled() {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            guard = self
                .0
                .settled
                .wait_timeout(guard, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        true
    }

    /// What the theme's author should know about its sounds, since the
    /// last call: which sound (by its order, from 1), and what.
    pub fn take_notes(&self) -> Vec<(SoundId, String)> {
        std::mem::take(&mut lock(&self.0.notes))
    }
}

/// Counted since the engine was made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Plays handed to the sound thread.
    pub queued: u64,
    /// Plays dropped because 64 were waiting.
    pub dropped: u64,
    /// Plays that became voices.
    pub started: u64,
    /// Plays the sound thread skipped: the sound was not ready, or the
    /// play had waited too long.
    pub skipped: u64,
    /// Times a connection to a sound server was attempted.
    pub connects: u64,
    /// Times the output was started and stopped.
    pub uncorks: u64,
    pub corks: u64,
    /// Times the sound thread woke.
    pub wakes: u64,
    /// Frames the null sink took.
    pub null_frames: u64,
    /// Sound threads started.
    pub threads: u64,
    /// Times the sound server asked the running thread's stream for
    /// audio, and frames it was given.
    pub requests: u64,
    pub fed_frames: u64,
    /// The mixer of the thread now running, if there is one.
    pub mixer: Option<MixStats>,
    /// Voices sounding or waiting now.
    pub voices: usize,
}

/// Engine-wide: what every sound thread of this engine adds to.
#[derive(Default)]
struct Counters {
    queued: AtomicU64,
    dropped: AtomicU64,
    started: AtomicU64,
    skipped: AtomicU64,
    connects: AtomicU64,
    uncorks: AtomicU64,
    corks: AtomicU64,
    wakes: AtomicU64,
    null_frames: AtomicU64,
    threads: AtomicU64,
    /// The Settings volume, as bits.
    volume: AtomicU32,
    /// Whether "no sound system" has been said (and not taken back).
    said_unavailable: AtomicBool,
}

/// What the app's thread and one sound thread share.
struct Link {
    status: AtomicU8,
    /// When a connection was last attempted, in ms since `epoch`, plus 1.
    attempt: AtomicU64,
    epoch: Instant,
    /// The sound thread's id in /proc, once it runs.
    tid: AtomicU32,
    /// The mixer (shared with the reactor too).
    shared: Mutex<Option<Arc<Shared>>>,
    /// Why there is no sound system, when there is none.
    reason: Mutex<String>,
}

struct PlayMsg {
    bank: u64,
    sound: SoundId,
    params: PlayParams,
    at: Instant,
}

enum Ctl {
    /// A theme's sounds: the bank, and the sounds from this place in it.
    Sounds(Bank, usize, Vec<SoundSource>),
    Volume,
    Retry,
    Quit,
}

struct Worker {
    plays: SyncSender<PlayMsg>,
    ctl: Sender<Ctl>,
    thread: Thread,
    join: JoinHandle<()>,
    done: Receiver<()>,
    link: Arc<Link>,
}

impl Worker {
    fn tell(&self, c: Ctl) {
        let _ = self.ctl.send(c);
        self.thread.unpark();
    }

    /// Waits for the thread to end, [`JOIN_LIMIT`] at most.
    fn join(self, log: &dyn Fn(&str)) {
        match self.done.recv_timeout(JOIN_LIMIT) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                let _ = self.join.join();
            }
            Err(RecvTimeoutError::Timeout) => {
                log("theme sounds: the sound thread is stuck on the sound server; leaving it");
            }
        }
    }
}

/// The app's handle on theme sounds. It lives on the app's main thread.
pub struct SoundEngine {
    settings: Settings,
    counters: Arc<Counters>,
    worker: Option<Worker>,
    /// Threads told to stop, to be joined.
    retired: Vec<Worker>,
    /// What is loaded: the bank, and its sounds (kept so that switching
    /// sound on later can make them).
    current: Option<(Bank, Vec<SoundSource>, bool)>,
    next_bank: u64,
}

impl SoundEngine {
    /// Creates nothing: no thread, no socket.
    pub fn new(settings: Settings) -> SoundEngine {
        let counters = Arc::new(Counters::default());
        counters
            .volume
            .store(settings.volume.to_bits(), Ordering::Relaxed);
        SoundEngine {
            settings,
            counters,
            worker: None,
            retired: Vec::new(),
            current: None,
            next_bank: 1,
        }
    }

    fn log(&self) -> impl Fn(&str) + use<> {
        let log = self.settings.log.clone();
        move |line: &str| say(&log, line)
    }

    fn live(&self) -> bool {
        self.settings.enabled && self.settings.sink != Sink::Off
    }

    /// Takes a theme's sounds; whatever was loaded before is let go (what
    /// is sounding fades out). Returns at once: the sounds are rendered
    /// and decoded on the sound thread, in order, and each can be played
    /// once it is ready ([`Bank::state`]).
    ///
    /// If the theme has sounds (or a `sound` function) and sound is on,
    /// this is where the sound thread starts and the sound server is
    /// connected to. Otherwise nothing is created.
    pub fn load(&mut self, sounds: &ThemeSounds) -> Bank {
        let bank = Bank::new(self.next_bank);
        self.next_bank += 1;
        let list: Vec<SoundSource> = sounds.sounds.clone();
        for state in bank.0.states.iter().take(list.len()) {
            state.store(PENDING, Ordering::Release);
        }
        self.current = Some((bank.clone(), list, sounds.hook));
        if self.live() && sounds.any() {
            self.start();
        } else {
            self.stop();
        }
        bank
    }

    /// Tells the engine about sounds the theme has made since it was
    /// loaded (inside its `sound` function): `sounds` is the theme's whole
    /// list now, and what is new at its end is added to the bank.
    pub fn extend(&mut self, bank: &Bank, sounds: &[SoundSource]) {
        let Some((current, list, _)) = &mut self.current else {
            return;
        };
        if current.0.id != bank.0.id || sounds.len() <= list.len() {
            return;
        }
        let from = list.len();
        let new: Vec<SoundSource> = sounds[from..].to_vec();
        for state in bank.0.states.iter().skip(from).take(new.len()) {
            state.store(PENDING, Ordering::Release);
        }
        list.extend(new.iter().cloned());
        if let Some(w) = &self.worker {
            w.tell(Ctl::Sounds(bank.clone(), from, new));
        }
    }

    /// Lets the loaded sounds go: what is sounding fades out within 30 ms,
    /// the stream and the connection are closed, the sound thread ends.
    pub fn unload(&mut self) {
        self.current = None;
        self.stop();
    }

    /// The switch. Off: as [`SoundEngine::unload`], but the theme's sounds
    /// are remembered. On: they are made again, and the sound server is
    /// tried again.
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.settings.enabled == enabled {
            return;
        }
        self.settings.enabled = enabled;
        let wanted = self
            .current
            .as_ref()
            .is_some_and(|(_, list, hook)| !list.is_empty() || *hook);
        if self.live() && wanted {
            // The sounds were let go with the thread: all are to be made.
            if let Some((bank, list, _)) = &self.current {
                for state in bank.0.states.iter().take(list.len()) {
                    state.store(PENDING, Ordering::Release);
                }
            }
            self.start();
        } else {
            self.stop();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.settings.enabled
    }

    /// The Settings volume, 0 to 1 (the gain is its square).
    pub fn set_volume(&mut self, volume: f32) {
        let volume = if volume.is_nan() {
            0.0
        } else {
            volume.clamp(0.0, 1.0)
        };
        self.settings.volume = volume;
        self.counters
            .volume
            .store(volume.to_bits(), Ordering::Relaxed);
        if let Some(w) = &self.worker {
            w.tell(Ctl::Volume);
        }
    }

    pub fn volume(&self) -> f32 {
        self.settings.volume
    }

    /// Starts the sound thread for what is loaded (or hands a running one
    /// the new sounds).
    fn start(&mut self) {
        self.reap();
        if self.current.is_none() {
            return;
        }
        if self.worker.is_none() {
            self.worker = spawn(&self.settings, &self.counters);
        }
        if let (Some(w), Some((bank, list, _))) = (&self.worker, &self.current) {
            w.tell(Ctl::Sounds(bank.clone(), 0, list.clone()));
        }
    }

    /// Tells the sound thread to fade out, close and end. It is joined
    /// later ([`SoundEngine::reap`], or when the engine is dropped): this
    /// does not wait.
    fn stop(&mut self) {
        if let Some(w) = self.worker.take() {
            w.tell(Ctl::Quit);
            self.retired.push(w);
        }
        self.reap();
    }

    /// Joins the threads that have ended.
    fn reap(&mut self) {
        let (ended, running): (Vec<Worker>, Vec<Worker>) = std::mem::take(&mut self.retired)
            .into_iter()
            .partition(|w| w.join.is_finished());
        self.retired = running;
        for w in ended {
            let _ = w.join.join();
        }
    }

    /// Plays one of the bank's sounds. Never waits: one push onto a
    /// bounded queue and one wake. What is not ready, or one too many, is
    /// dropped, never played late.
    pub fn play(&self, bank: &Bank, sound: SoundId, params: PlayParams) -> Played {
        let Some(w) = &self.worker else {
            return Played::Off;
        };
        if !self
            .current
            .as_ref()
            .is_some_and(|(b, ..)| b.0.id == bank.0.id)
        {
            return Played::Stale;
        }
        if !self.available() {
            return Played::Unavailable;
        }
        if bank.state(sound) != SoundState::Ready {
            return Played::NotReady;
        }
        let msg = PlayMsg {
            bank: bank.0.id,
            sound,
            params: params.clamped(),
            at: Instant::now(),
        };
        match w.plays.try_send(msg) {
            Ok(()) => {
                self.counters.queued.fetch_add(1, Ordering::Relaxed);
                w.thread.unpark();
                Played::Queued
            }
            Err(_) => {
                self.counters.dropped.fetch_add(1, Ordering::Relaxed);
                Played::Dropped
            }
        }
    }

    /// Whether a play could sound now: sound is on, a theme with sounds is
    /// loaded, and there is a sound system. (What "From event to sound"
    /// checks first, before the theme's `sound` function is run.)
    ///
    /// While there is no sound system, asking is what makes the engine
    /// look for one again, at most every [`Settings::retry`].
    pub fn available(&self) -> bool {
        let Some(w) = &self.worker else {
            return false;
        };
        match Status::from_u8(w.link.status.load(Ordering::Acquire)) {
            Status::Unavailable => {
                let now = w.link.epoch.elapsed().as_millis() as u64 + 1;
                let last = w.link.attempt.load(Ordering::Acquire);
                let due = now.saturating_sub(last) >= self.settings.retry.as_millis() as u64;
                if due
                    && w.link
                        .attempt
                        .compare_exchange(last, now, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                {
                    w.tell(Ctl::Retry);
                }
                false
            }
            _ => true,
        }
    }

    /// For Settings: "No sound system found" is [`Status::Unavailable`].
    pub fn status(&self) -> Status {
        if !self.live() {
            return Status::Off;
        }
        match &self.worker {
            Some(w) => Status::from_u8(w.link.status.load(Ordering::Acquire)),
            None => Status::Idle,
        }
    }

    /// Why there is no sound system, while there is none.
    pub fn unavailable_reason(&self) -> Option<String> {
        let w = self.worker.as_ref()?;
        (self.status() == Status::Unavailable).then(|| lock(&w.link.reason).clone())
    }

    pub fn stats(&self) -> Stats {
        let c = &self.counters;
        let get = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let shared = self
            .worker
            .as_ref()
            .and_then(|w| lock(&w.link.shared).clone());
        let (mixer, voices) = match &shared {
            Some(s) => {
                let m = s.mixer();
                (Some(m.stats()), m.voices())
            }
            None => (None, 0),
        };
        let (requests, fed_frames) = shared.as_ref().map_or((0, 0), |s| (s.polls(), s.fed()));
        Stats {
            queued: get(&c.queued),
            dropped: get(&c.dropped),
            started: get(&c.started),
            skipped: get(&c.skipped),
            connects: get(&c.connects),
            uncorks: get(&c.uncorks),
            corks: get(&c.corks),
            wakes: get(&c.wakes),
            null_frames: get(&c.null_frames),
            threads: get(&c.threads),
            requests,
            fed_frames,
            mixer,
            voices,
        }
    }

    /// How long each play waited from the call to its first frames being
    /// mixed, since the last call (for measurements).
    pub fn take_latencies(&self) -> Vec<Duration> {
        let shared = self
            .worker
            .as_ref()
            .and_then(|w| lock(&w.link.shared).clone());
        shared.map_or_else(Vec::new, |s| s.mixer().take_latencies())
    }

    /// The sound thread's id under `/proc/self/task`, while it runs (for
    /// measuring what it costs).
    pub fn thread_id(&self) -> Option<u32> {
        let tid = self.worker.as_ref()?.link.tid.load(Ordering::Acquire);
        (tid != 0).then_some(tid)
    }

    /// Whether a sound thread exists.
    pub fn has_thread(&self) -> bool {
        self.worker.is_some()
    }
}

impl Drop for SoundEngine {
    /// Stops the sound thread and waits for it to end.
    fn drop(&mut self) {
        self.stop();
        let log = self.log();
        for w in std::mem::take(&mut self.retired) {
            w.join(&log);
        }
    }
}

fn say(log: &Option<Log>, line: &str) {
    match log {
        Some(log) => log(line),
        None => eprintln!("gmnb: {line}"),
    }
}

/// This thread's id under /proc.
fn thread_id() -> u32 {
    std::fs::read_link("/proc/thread-self")
        .ok()
        .and_then(|p| p.file_name()?.to_str()?.parse().ok())
        .unwrap_or(0)
}

fn spawn(settings: &Settings, counters: &Arc<Counters>) -> Option<Worker> {
    let (plays_tx, plays_rx) = mpsc::sync_channel(limits::QUEUE);
    let (ctl_tx, ctl_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let link = Arc::new(Link {
        status: AtomicU8::new(2),
        attempt: AtomicU64::new(0),
        epoch: Instant::now(),
        tid: AtomicU32::new(0),
        shared: Mutex::new(None),
        reason: Mutex::new(String::new()),
    });
    let (settings2, counters2, link2) = (settings.clone(), counters.clone(), link.clone());
    let join = std::thread::Builder::new()
        .name("gmnb-sound".into())
        .spawn(move || {
            let run = Run::new(settings2, counters2, link2, plays_rx, ctl_rx);
            run.run();
            let _ = done_tx.send(());
        });
    match join {
        Ok(join) => {
            counters.threads.fetch_add(1, Ordering::Relaxed);
            Some(Worker {
                plays: plays_tx,
                ctl: ctl_tx,
                thread: join.thread().clone(),
                join,
                done: done_rx,
                link,
            })
        }
        Err(e) => {
            say(
                &settings.log,
                &format!("theme sounds: no thread for sound ({e})"),
            );
            None
        }
    }
}

// ---- The sound thread ----------------------------------------------------------

enum Out {
    /// Not set up yet, or no sound system.
    None,
    /// The null sink: `ticking` while it takes frames, with the time the
    /// next ones are due.
    Null {
        ticking: bool,
        next: Instant,
    },
    Server(Output),
}

struct Run {
    settings: Settings,
    counters: Arc<Counters>,
    link: Arc<Link>,
    plays: Receiver<PlayMsg>,
    ctl: Receiver<Ctl>,
    shared: Arc<Shared>,
    out: Out,
    bank: Option<Bank>,
    /// The bank's sounds, ready to mix; `None` where one is not (yet).
    pcm: Vec<Option<Arc<[i16]>>>,
    /// Sounds still to make: their place in the bank, and what they are.
    jobs: VecDeque<(usize, SoundSource)>,
    budget: Budget,
    want_connect: bool,
    scratch: Vec<i16>,
}

impl Run {
    fn new(
        settings: Settings,
        counters: Arc<Counters>,
        link: Arc<Link>,
        plays: Receiver<PlayMsg>,
        ctl: Receiver<Ctl>,
    ) -> Run {
        link.tid.store(thread_id(), Ordering::Release);
        let volume = f32::from_bits(counters.volume.load(Ordering::Relaxed));
        let shared = Shared::new(Mixer::new(volume * volume), std::thread::current());
        *lock(&link.shared) = Some(shared.clone());
        Run {
            settings,
            counters,
            link,
            plays,
            ctl,
            shared,
            out: Out::None,
            bank: None,
            pcm: Vec::new(),
            jobs: VecDeque::new(),
            budget: Budget::default(),
            want_connect: false,
            scratch: Vec::new(),
        }
    }

    fn count(&self, counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    fn run(mut self) {
        loop {
            self.count(&self.counters.wakes);
            loop {
                match self.ctl.try_recv() {
                    Ok(Ctl::Quit) | Err(TryRecvError::Disconnected) => return self.shut_down(),
                    Ok(c) => self.control(c),
                    Err(TryRecvError::Empty) => break,
                }
            }
            // The output first: Settings can say "No sound system found"
            // before a second of sounds has been rendered.
            if self.want_connect {
                self.connect();
            }
            while let Ok(p) = self.plays.try_recv() {
                self.play(p);
            }
            // One sound, then round again: plays are taken between sounds.
            if let Some((index, source)) = self.jobs.pop_front() {
                self.make(index, &source);
                continue;
            }
            match self.upkeep() {
                Some(wait) => std::thread::park_timeout(wait),
                None => std::thread::park(),
            }
        }
    }

    fn control(&mut self, c: Ctl) {
        match c {
            Ctl::Sounds(bank, from, sources) => {
                if from == 0 {
                    // Another theme's sounds (or the same again): what is
                    // sounding fades out, and everything is made anew.
                    self.shared.mixer().stop();
                    self.pcm.clear();
                    self.jobs.clear();
                    self.budget = Budget::default();
                    self.bank = Some(bank);
                    // A theme was loaded: look for the sound system again.
                    self.want_connect = true;
                } else if self.bank.as_ref().is_none_or(|b| b.0.id != bank.0.id) {
                    return;
                }
                self.jobs
                    .extend(sources.into_iter().enumerate().map(|(i, s)| (from + i, s)));
            }
            Ctl::Volume => {
                let v = f32::from_bits(self.counters.volume.load(Ordering::Relaxed));
                self.shared.mixer().set_gain(v * v);
            }
            Ctl::Retry => self.want_connect = true,
            Ctl::Quit => {}
        }
    }

    fn set_status(&self, status: Status, reason: &str) {
        let value = match status {
            Status::Starting => 2,
            Status::Ready => 3,
            Status::Unavailable => 4,
            Status::Off | Status::Idle => 1,
        };
        *lock(&self.link.reason) = reason.to_string();
        // One line per change of state, however many themes are loaded
        // while it lasts. (Said before the state shows: whoever sees the
        // state finds the line there.)
        let said = &self.counters.said_unavailable;
        match status {
            Status::Unavailable if !said.swap(true, Ordering::AcqRel) => say(
                &self.settings.log,
                &format!("theme sounds: no sound system found ({reason}); staying silent"),
            ),
            Status::Ready if said.swap(false, Ordering::AcqRel) => {
                say(&self.settings.log, "theme sounds: sound system found")
            }
            _ => {}
        }
        self.link.status.store(value, Ordering::Release);
    }

    fn connect(&mut self) {
        self.want_connect = false;
        let server = match &self.settings.sink {
            Sink::Off => return,
            Sink::Null => {
                if !matches!(self.out, Out::Null { .. }) {
                    self.out = Out::Null {
                        ticking: false,
                        next: Instant::now(),
                    };
                }
                return self.set_status(Status::Ready, "");
            }
            Sink::Socket { path, cookie } => Ok((path.clone(), cookie.clone())),
            Sink::Session => {
                let env = |name: &str| std::env::var(name).ok();
                pulse::socket_path(&env)
                    .map(|path| (path, pulse::cookie(&env)))
                    .ok_or_else(|| "no sound server's socket".to_string())
            }
        };
        if matches!(self.out, Out::Server(_)) {
            return;
        }
        let now = self.link.epoch.elapsed().as_millis() as u64 + 1;
        self.link.attempt.store(now, Ordering::Release);
        self.count(&self.counters.connects);
        let opened = server.and_then(|(path, cookie)| {
            Output::open(
                &path,
                &cookie,
                &self.settings.identity,
                &self.shared,
                self.settings.server_limit,
            )
        });
        match opened {
            Ok(out) => {
                self.out = Out::Server(out);
                self.set_status(Status::Ready, "");
            }
            Err(reason) => self.set_status(Status::Unavailable, &reason),
        }
    }

    /// The connection failed or went away: silence, said once.
    fn lose(&mut self, reason: &str) {
        if let Out::Server(out) = std::mem::replace(&mut self.out, Out::None) {
            out.close(Duration::from_millis(50));
        }
        self.shared.mixer().clear();
        let now = self.link.epoch.elapsed().as_millis() as u64 + 1;
        self.link.attempt.store(now, Ordering::Release);
        self.set_status(Status::Unavailable, reason);
    }

    fn settle(&self, bank: &Bank, index: usize, state: u8) {
        if let Some(s) = bank.0.states.get(index) {
            s.store(state, Ordering::Release);
        }
        let _guard = lock(&bank.0.settle);
        bank.0.settled.notify_all();
    }

    /// Renders or decodes one sound.
    fn make(&mut self, index: usize, source: &SoundSource) {
        let Some(bank) = self.bank.clone() else {
            return;
        };
        let id = SoundId(index as u32 + 1);
        // A decoder that panics is caught inside; so is anything else
        // here, rather than end the thread.
        let budget = &mut self.budget;
        let made =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| prepare(source, budget)))
                .unwrap_or_else(|_| Err("the sound could not be made".into()));
        if self.pcm.len() <= index {
            self.pcm.resize(index + 1, None);
        }
        match made {
            Ok(samples) => {
                if let Some(dir) = &self.settings.dump {
                    self.dump(dir, index + 1, source, &samples);
                }
                let playable = samples.len() >= 2;
                self.pcm[index] = playable.then(|| samples.into());
                self.settle(&bank, index, if playable { READY } else { REFUSED });
            }
            Err(note) => {
                let what = match source {
                    SoundSource::File { name, .. } => format!("{name}: {note}"),
                    _ => note,
                };
                lock(&bank.0.notes).push((id, what));
                self.settle(&bank, index, REFUSED);
            }
        }
    }

    fn dump(&self, dir: &std::path::Path, order: usize, source: &SoundSource, samples: &[i16]) {
        let path = dir.join(dump_name(order, source));
        let written = std::fs::create_dir_all(dir).and_then(|()| crate::wav::write(&path, samples));
        if let Err(e) = written {
            say(
                &self.settings.log,
                &format!("theme sounds: {}: {e}", path.display()),
            );
        }
    }

    fn play(&mut self, p: PlayMsg) {
        let pcm = self
            .bank
            .as_ref()
            .filter(|b| b.0.id == p.bank)
            .and_then(|_| self.pcm.get(p.sound.index()?)?.clone());
        let waited = p.at.elapsed();
        let delay = Duration::from_secs_f32(p.params.delay.clamp(0.0, limits::PLAY_DELAY));
        let (Some(pcm), false, false) = (pcm, waited > delay + LATE, matches!(self.out, Out::None))
        else {
            return self.count(&self.counters.skipped);
        };
        let params = PlayParams {
            delay: delay.saturating_sub(waited).as_secs_f32(),
            ..p.params
        };
        self.shared.mixer().play_asked(pcm, params, Some(p.at));
        self.count(&self.counters.started);
        match &mut self.out {
            Out::Server(out) if out.is_corked() => {
                self.counters.uncorks.fetch_add(1, Ordering::Relaxed);
                if let Err(reason) = out.uncork() {
                    self.lose(&reason);
                }
            }
            Out::Null { ticking, next } if !*ticking => {
                self.counters.uncorks.fetch_add(1, Ordering::Relaxed);
                *ticking = true;
                *next = Instant::now();
            }
            _ => {}
        }
    }

    /// Looks after the output; returns how long to sleep (`None`: until
    /// woken).
    fn upkeep(&mut self) -> Option<Duration> {
        match &mut self.out {
            Out::None => None,
            Out::Server(out) => {
                if out.is_lost() {
                    self.lose("the sound server went away");
                    return None;
                }
                // The source said 100 ms of silence have gone out. (A play
                // may have come since: the mixer knows.)
                if self.shared.take_quiet()
                    && !out.is_corked()
                    && self.shared.mixer().quiet_frames() >= QUIET_FRAMES
                {
                    self.counters.corks.fetch_add(1, Ordering::Relaxed);
                    if let Err(reason) = out.cork() {
                        self.lose(&reason);
                    }
                }
                None
            }
            Out::Null { ticking, next } => {
                if !*ticking {
                    return None;
                }
                let quantum = Duration::from_secs_f64(NULL_QUANTUM as f64 / 48_000.0);
                let now = Instant::now();
                // Behind by more than a few quanta (the thread was busy
                // rendering): don't race to catch up.
                if now.saturating_duration_since(*next) > quantum * 8 {
                    *next = now;
                }
                while *next <= now {
                    self.scratch.clear();
                    self.scratch.resize(NULL_QUANTUM * 2, 0);
                    let quiet = {
                        let mut mixer = self.shared.mixer();
                        mixer.mix(&mut self.scratch);
                        mixer.quiet_frames()
                    };
                    self.counters
                        .null_frames
                        .fetch_add(NULL_QUANTUM as u64, Ordering::Relaxed);
                    *next += quantum;
                    if quiet >= QUIET_FRAMES {
                        // What a corked stream is: no more frames wanted.
                        self.counters.corks.fetch_add(1, Ordering::Relaxed);
                        *ticking = false;
                        return None;
                    }
                }
                Some(next.saturating_duration_since(Instant::now()))
            }
        }
    }

    /// Fades out what is sounding, closes the stream and the connection.
    fn shut_down(mut self) {
        let fade = Duration::from_secs_f64(f64::from(STOP_FADE) / 48_000.0);
        match std::mem::replace(&mut self.out, Out::None) {
            Out::Server(out) => {
                if !out.is_corked() && !out.is_lost() && self.shared.mixer().is_active() {
                    self.shared.mixer().stop();
                    // The source goes on mixing: wait for the fade to have
                    // gone out and been heard.
                    let deadline = Instant::now() + fade * 3;
                    while self.shared.mixer().is_active() && Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    std::thread::sleep(TAIL);
                }
                out.close(Duration::from_millis(250));
            }
            Out::Null { .. } | Out::None => {}
        }
        self.shared.mixer().clear();
        *lock(&self.link.shared) = None;
    }
}
