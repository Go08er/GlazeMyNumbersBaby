//! Playing animations (docs/theming-media.md, "The animation player").
//!
//! The thread that draws holds, for each animation, its **first** frame
//! (the still one: what reduced motion, a preview and a tile show), the
//! frame **shown**, and at most one frame **ahead**. One other thread,
//! made the first time any animation needs a frame after its first,
//! decodes forward through each file and holds its canvas.
//!
//! A draw wants frame `i`:
//!
//! | `i` is | Then |
//! |---|---|
//! | shown | It is drawn. |
//! | ahead | It becomes the frame shown; the frame after it is asked for. |
//! | neither | It is asked for; the frame shown is drawn meanwhile. When it arrives it becomes the frame shown, [`Player::poll`] names the animation (whoever drew it draws again), and the frame after it is asked for. |
//!
//! **The quarter core.** After a frame that took `d` of processor time to
//! decode and composite, the thread starts no other for `3d`. One thread
//! serves every animation, so the cap covers them together.
//!
//! **At rest** nothing is asked for, the thread blocks on its queue, and
//! it is never woken ([`Stats::wakeups`] counts).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::anim::Timeline;
use crate::stream::Stream;
use crate::{Bytes, Error, shielded, thread_time};

/// Media threads started since the process began, by every player.
static THREADS: AtomicU64 = AtomicU64::new(0);

/// How many media threads this process has ever started. 0 until an
/// animation has needed a frame after its first: never in the base look.
pub fn threads_started() -> u64 {
    THREADS.load(Ordering::Relaxed)
}

/// One frame of an animation, composited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Which frame of the file, from 0.
    pub index: u32,
    pub width: u32,
    pub height: u32,
    /// RGBA8, premultiplied, `width × 4` bytes a row.
    pub pixels: Bytes,
}

/// A file being decoded, as the thread sees it. (The files' own is
/// [`Stream`]; tests bring others.)
pub trait Source {
    /// Frame `index`, premultiplied, and how many frames were decoded to
    /// reach it.
    fn seek(&mut self, index: u32) -> Result<(Vec<u8>, u32), Error>;

    /// One frame on the way to frame `index`: `Some` once there, as
    /// [`Source::seek`] gives it; `None` after a frame decoded on the
    /// way. The thread rests after each. (A source that can't go a frame
    /// at a time goes all the way at once.)
    fn step(&mut self, index: u32) -> Result<Option<(Vec<u8>, u32)>, Error> {
        self.seek(index).map(Some)
    }
}

impl Source for Stream {
    fn seek(&mut self, index: u32) -> Result<(Vec<u8>, u32), Error> {
        Stream::seek(self, index)
    }

    fn step(&mut self, index: u32) -> Result<Option<(Vec<u8>, u32)>, Error> {
        Stream::step(self, index)
    }
}

/// Makes an animation's source, on the thread that will decode it.
type Make = Box<dyn FnOnce() -> Result<Box<dyn Source>, Error> + Send>;

enum Message {
    Open(u32, Make),
    Want(u32, u32),
    Close(u32),
}

struct Done {
    key: u32,
    index: u32,
    pixels: Result<Vec<u8>, Error>,
    /// The processor time it took, and how many frames were decoded.
    cost: Duration,
    decoded: u32,
}

/// What the thread and its player share.
#[derive(Default)]
struct Shared {
    done: Mutex<Vec<Done>>,
    started: AtomicU64,
    wakeups: AtomicU64,
    decoded: AtomicU64,
    busy_nanos: AtomicU64,
    rested_nanos: AtomicU64,
    ended: AtomicU64,
}

/// What a player has cost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Whether this player has started its thread.
    pub thread: bool,
    /// Whether that thread has ended.
    pub ended: bool,
    /// Times the thread was woken from its queue.
    pub wakeups: u64,
    /// Frames it decoded (the ones dropped on the way to another too).
    pub decoded: u64,
    /// The processor time it spent decoding.
    pub busy: Duration,
    /// The time it rested between frames to keep to its quarter core.
    pub rested: Duration,
}

struct Anim {
    timeline: Arc<Timeline>,
    /// Makes the thread's source; taken when the thread is first told of
    /// this animation.
    make: Option<Make>,
    first: Arc<Frame>,
    shown: Arc<Frame>,
    ahead: Option<Arc<Frame>>,
    /// The frame a draw asked for that isn't shown yet.
    want: Option<u32>,
    /// The frame the thread was last asked for and hasn't given.
    asked: Option<u32>,
    /// The file failed after its first frame: it stands on the frame
    /// shown.
    failed: bool,
    /// Processor seconds a frame takes to decode (the mean of about a
    /// pass), and how many frames have been measured.
    cost: Option<f64>,
    measured: u32,
    /// When a frame was last asked for, in the caller's seconds.
    last_asked: f64,
}

/// A theme's animations, playing.
pub struct Player {
    anims: HashMap<u32, Anim>,
    /// The way to the thread: dropping it ends the thread.
    thread: Option<Sender<Message>>,
    shared: Arc<Shared>,
    notify: Arc<dyn Fn() + Send + Sync>,
    notes: Vec<String>,
    told_slow: bool,
}

/// How much of one processor core the thread may take.
pub const SHARE: f64 = 0.25;
/// An animation counts towards the load while it was asked for a frame
/// within this many seconds.
const PLAYING: f64 = 1.0;
/// Frames of an animation decoded before its mean cost is believed, and
/// the fewest the mean is taken over.
const MEASURED_FIRST: u32 = 24;

impl Player {
    /// A player with no thread yet. `notify` is called, from the media
    /// thread, whenever a frame has arrived: the owner then calls
    /// [`Player::poll`] on its own thread.
    pub fn new(notify: impl Fn() + Send + Sync + 'static) -> Player {
        Player {
            anims: HashMap::new(),
            thread: None,
            shared: Arc::default(),
            notify: Arc::new(notify),
            notes: Vec::new(),
            told_slow: false,
        }
    }

    /// Takes up an animation under `key`: decodes its first frame now, on
    /// the calling thread (it is the still frame, and it means reduced
    /// motion never starts the thread).
    pub fn open(&mut self, key: u32, bytes: Bytes, timeline: Arc<Timeline>) -> Result<(), Error> {
        let (format, w, h) = (timeline.format, timeline.width, timeline.height);
        let first = {
            let bytes = bytes.clone();
            shielded(move || Stream::open(bytes, format, w, h)?.seek(0))?.0
        };
        let make: Make =
            Box::new(move || Ok(Box::new(Stream::open(bytes, format, w, h)?) as Box<dyn Source>));
        self.open_with(key, timeline, first, make);
        Ok(())
    }

    /// [`Player::open`] with the first frame's pixels and the source
    /// given: for tests of what the thread does with a source that fails.
    #[doc(hidden)]
    pub fn open_with(
        &mut self,
        key: u32,
        timeline: Arc<Timeline>,
        first: Vec<u8>,
        make: Box<dyn FnOnce() -> Result<Box<dyn Source>, Error> + Send>,
    ) {
        self.close(key);
        let first = Arc::new(Frame {
            index: 0,
            width: timeline.width,
            height: timeline.height,
            pixels: Bytes::new(first),
        });
        self.anims.insert(
            key,
            Anim {
                timeline,
                make: Some(make),
                shown: first.clone(),
                first,
                ahead: None,
                want: None,
                asked: None,
                failed: false,
                cost: None,
                measured: 0,
                last_asked: f64::NEG_INFINITY,
            },
        );
    }

    /// Forgets an animation.
    pub fn close(&mut self, key: u32) {
        if let Some(anim) = self.anims.remove(&key)
            && anim.make.is_none()
            && let Some(thread) = &self.thread
        {
            let _ = thread.send(Message::Close(key));
        }
    }

    pub fn has(&self, key: u32) -> bool {
        self.anims.contains_key(&key)
    }

    /// Whether the thread has been asked for a frame it hasn't given yet.
    pub fn pending(&self) -> bool {
        self.anims.values().any(|a| a.asked.is_some())
    }

    /// The animation's first frame: what stands for it when nothing
    /// moves. Asking for it disturbs nothing that plays.
    pub fn first(&self, key: u32) -> Option<Arc<Frame>> {
        self.anims.get(&key).map(|a| a.first.clone())
    }

    /// Asks the thread for a frame, starting it if this is the first
    /// frame anyone asks of it.
    fn ask(&mut self, key: u32, index: u32) {
        let Some(anim) = self.anims.get_mut(&key) else {
            return;
        };
        if self.thread.is_none() {
            let (tx, rx) = mpsc::channel();
            let (shared, notify) = (self.shared.clone(), self.notify.clone());
            let spawned = std::thread::Builder::new()
                .name("gmnb-media".into())
                .spawn(move || run(rx, &shared, &*notify));
            match spawned {
                Ok(_) => {
                    THREADS.fetch_add(1, Ordering::Relaxed);
                    self.shared.started.fetch_add(1, Ordering::Relaxed);
                    self.thread = Some(tx);
                }
                Err(e) => {
                    anim.failed = true;
                    self.notes
                        .push(format!("animations stand still: no thread for them ({e})"));
                    return;
                }
            }
        }
        let Some(thread) = &self.thread else {
            return;
        };
        if let Some(make) = anim.make.take() {
            let _ = thread.send(Message::Open(key, make));
        }
        anim.asked = Some(index);
        let _ = thread.send(Message::Want(key, index));
    }

    /// Asks for the frame that follows `index`, so that it is there
    /// before it is due. The first frame is always there already.
    fn ask_after(&mut self, key: u32, index: u32) {
        let Some(anim) = self.anims.get_mut(&key) else {
            return;
        };
        if !anim.timeline.moves() || anim.failed {
            return;
        }
        match anim.timeline.after(index) {
            0 => anim.ahead = Some(anim.first.clone()),
            next => self.ask(key, next),
        }
    }

    /// The frame to draw for a draw that wants frame `index` at `now`
    /// (seconds on the caller's clock): that frame if it is there, else
    /// the frame shown, with `index` asked for.
    pub fn frame(&mut self, key: u32, index: u32, now: f64) -> Option<Arc<Frame>> {
        let anim = self.anims.get_mut(&key)?;
        let index = index.min(anim.timeline.frames().saturating_sub(1));
        if anim.shown.index == index || anim.failed {
            // What was asked for meanwhile isn't waited for any more.
            anim.want = None;
            return Some(anim.shown.clone());
        }
        anim.last_asked = now;
        if anim.ahead.as_ref().is_some_and(|f| f.index == index) || index == 0 {
            anim.shown = match anim.ahead.take() {
                Some(ahead) if ahead.index == index => ahead,
                _ => anim.first.clone(),
            };
            anim.want = None;
            let shown = anim.shown.clone();
            self.ask_after(key, index);
            return Some(shown);
        }
        // Neither: the frame ahead, if any, is of no use now.
        anim.ahead = None;
        anim.want = Some(index);
        let shown = anim.shown.clone();
        if anim.asked != Some(index) {
            self.ask(key, index);
        }
        Some(shown)
    }

    /// Takes in the frames that have arrived. The animations whose frame
    /// shown changed: whoever drew one of them draws again.
    pub fn poll(&mut self) -> Vec<u32> {
        let done = {
            let mut done = self.shared.done.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut *done)
        };
        let mut changed = Vec::new();
        for d in done {
            let Some(anim) = self.anims.get_mut(&d.key) else {
                continue;
            };
            if anim.asked == Some(d.index) {
                anim.asked = None;
            }
            let pixels = match d.pixels {
                Ok(pixels) => pixels,
                Err(e) => {
                    // It stands on the frame it reached, and is not tried
                    // again.
                    anim.failed = true;
                    anim.want = None;
                    anim.ahead = None;
                    self.notes.push(format!(
                        "an animation stopped at frame {}: {e}",
                        anim.shown.index + 1
                    ));
                    continue;
                }
            };
            if d.decoded > 0 {
                // The mean of the frames so far and, once a pass's worth
                // is in it, of about the last pass. Frames of one file
                // differ in what they cost (its first are the dearest,
                // with cold caches and fresh memory), and what counts is
                // what a pass takes.
                let each = d.cost.as_secs_f64() / f64::from(d.decoded);
                anim.measured = anim.measured.saturating_add(d.decoded);
                let pass = anim.timeline.frames().max(MEASURED_FIRST);
                let gain = f64::from(d.decoded) / f64::from(anim.measured.min(pass));
                let mean = anim.cost.unwrap_or(each);
                anim.cost = Some(mean + (each - mean) * gain.min(1.0));
            }
            let frame = Arc::new(Frame {
                index: d.index,
                width: anim.timeline.width,
                height: anim.timeline.height,
                pixels: Bytes::new(pixels),
            });
            if anim.want == Some(d.index) {
                anim.shown = frame;
                anim.want = None;
                if !changed.contains(&d.key) {
                    changed.push(d.key);
                }
                self.ask_after(d.key, d.index);
            } else if anim.want.is_none() && d.index == anim.timeline.after(anim.shown.index) {
                anim.ahead = Some(frame);
            }
            // Else no draw waits for it any more: it is dropped.
        }
        changed
    }

    /// How much slower than its file says an animation must play for the
    /// thread to keep to its quarter core: 1 unless the animations being
    /// played at `now` take more than that to decode at full speed.
    pub fn slow(&mut self, now: f64) -> f64 {
        let load: f64 = self
            .anims
            .values()
            .filter(|a| a.timeline.moves() && !a.failed && now - a.last_asked < PLAYING)
            .filter(|a| a.measured >= MEASURED_FIRST)
            .filter_map(|a| Some(a.cost? / a.timeline.mean_delay()))
            .sum();
        let slow = (load / SHARE).max(1.0);
        if slow > 1.05 && !self.told_slow {
            self.told_slow = true;
            self.notes.push(format!(
                "the theme's animations take {:.0}% of a processor core to decode, and a quarter is \
                 what they get: they play slower",
                load * 100.0
            ));
        }
        if slow.is_finite() { slow } else { 1.0 }
    }

    /// What the theme's author should know, each message once.
    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    pub fn stats(&self) -> Stats {
        self.shared.stats()
    }

    /// The player's counters, readable after the player has gone (whether
    /// its thread has ended, above all).
    pub fn watch(&self) -> Watch {
        Watch(self.shared.clone())
    }
}

/// A player's counters ([`Player::watch`]).
#[derive(Clone)]
pub struct Watch(Arc<Shared>);

impl Watch {
    pub fn stats(&self) -> Stats {
        self.0.stats()
    }
}

impl Shared {
    fn stats(&self) -> Stats {
        Stats {
            thread: self.started.load(Ordering::Relaxed) > 0,
            ended: self.ended.load(Ordering::Relaxed) > 0,
            wakeups: self.wakeups.load(Ordering::Relaxed),
            decoded: self.decoded.load(Ordering::Relaxed),
            busy: Duration::from_nanos(self.busy_nanos.load(Ordering::Relaxed)),
            rested: Duration::from_nanos(self.rested_nanos.load(Ordering::Relaxed)),
        }
    }
}

/// What the thread has been asked and not done: the latest frame asked
/// of each animation, in the order the animations were first asked.
#[derive(Default)]
struct Asked {
    sources: HashMap<u32, Result<Box<dyn Source>, Error>>,
    wants: Vec<(u32, u32)>,
}

impl Asked {
    fn take(&mut self, message: Message) {
        match message {
            Message::Open(key, make) => {
                // Opening reads only a header, but it is a decoder's code
                // all the same.
                self.sources.insert(key, shielded(make));
            }
            Message::Want(key, index) => match self.wants.iter_mut().find(|(k, _)| *k == key) {
                Some(want) => want.1 = index,
                None => self.wants.push((key, index)),
            },
            Message::Close(key) => {
                self.sources.remove(&key);
                self.wants.retain(|(k, _)| *k != key);
            }
        }
    }
}

/// Rests until `until`, taking in what is asked meanwhile. `false`: the
/// player has gone (the thread ends at once, not after its rest).
fn rest(rx: &Receiver<Message>, asked: &mut Asked, shared: &Shared, until: Instant) -> bool {
    let from = Instant::now();
    loop {
        let now = Instant::now();
        if now >= until {
            break;
        }
        match rx.recv_timeout(until - now) {
            Ok(message) => asked.take(message),
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
    let rested = from.elapsed().as_nanos() as u64;
    shared.rested_nanos.fetch_add(rested, Ordering::Relaxed);
    true
}

/// The processor time since `cpu` (the wall's since `wall` where the
/// system can't tell).
fn spent(cpu: Option<Duration>, wall: Instant) -> Duration {
    match (cpu, thread_time()) {
        (Some(then), Some(now)) => now.saturating_sub(then),
        _ => wall.elapsed(),
    }
}

/// The media thread.
fn run(rx: Receiver<Message>, shared: &Shared, notify: &(dyn Fn() + Send + Sync)) {
    struct Ended<'a>(&'a Shared);
    impl Drop for Ended<'_> {
        fn drop(&mut self) {
            self.0.ended.fetch_add(1, Ordering::Relaxed);
        }
    }
    let _ended = Ended(shared);
    let mut asked = Asked::default();
    // Not before this: three times what the last frame took.
    let mut rest_until: Option<Instant> = None;
    loop {
        if asked.wants.is_empty() {
            // At rest: nothing wakes the thread but a message.
            let Ok(message) = rx.recv() else {
                return;
            };
            shared.wakeups.fetch_add(1, Ordering::Relaxed);
            asked.take(message);
        }
        if let Some(until) = rest_until.take()
            && !rest(&rx, &mut asked, shared, until)
        {
            return;
        }
        // What was asked meanwhile: only the latest of each animation
        // counts.
        loop {
            match rx.try_recv() {
                Ok(message) => asked.take(message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if asked.wants.is_empty() {
            continue;
        }
        let (key, mut index) = asked.wants.remove(0);
        // A frame at a time, with the rest after each: the frames on the
        // way to one far ahead (or, from the start again, to one behind)
        // are held to the quarter core like any other.
        let mut cost = Duration::ZERO;
        let mut decoded = 0;
        let result = loop {
            let Some(source) = asked.sources.get_mut(&key) else {
                // Closed meanwhile.
                break None;
            };
            let (cpu, wall) = (thread_time(), Instant::now());
            let step = match source {
                Ok(source) => shielded(|| source.step(index)),
                Err(e) => Err(e.clone()),
            };
            let took = spent(cpu, wall);
            cost += took;
            shared
                .busy_nanos
                .fetch_add(took.as_nanos() as u64, Ordering::Relaxed);
            rest_until = Some(Instant::now() + took * 3);
            match step {
                Ok(Some((pixels, n))) => {
                    decoded += n;
                    shared.decoded.fetch_add(u64::from(n), Ordering::Relaxed);
                    break Some(Ok(pixels));
                }
                Ok(None) => {
                    decoded += 1;
                    shared.decoded.fetch_add(1, Ordering::Relaxed);
                    if let Some(until) = rest_until.take()
                        && !rest(&rx, &mut asked, shared, until)
                    {
                        return;
                    }
                    // A later frame of this animation, asked for
                    // meanwhile, is the one gone on to.
                    if let Some(at) = asked.wants.iter().position(|(k, _)| *k == key) {
                        index = asked.wants.remove(at).1;
                    }
                }
                Err(e) => {
                    // A source that failed is not gone on with.
                    asked.sources.insert(key, Err(e.clone()));
                    break Some(Err(e));
                }
            }
        };
        let Some(pixels) = result else {
            continue;
        };
        shared
            .done
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Done {
                key,
                index,
                pixels,
                cost,
                decoded,
            });
        notify();
    }
}
