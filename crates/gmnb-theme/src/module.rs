//! A theme's WebAssembly modules (docs/theming.md, "Modules"):
//! `gmnb.module(file)`, `module:write`, `module:call`, `module:read`,
//! `module:picture`, and the pictures a module fills.
//!
//! # When what happens
//!
//! | When | What |
//! |---|---|
//! | The theme's file runs (applied, or listed by Settings) | `gmnb.module` names a `.wasm` file of the theme's folder. Nothing is read or compiled, and no engine is made. |
//! | The theme is applied: [`Theme::prepare_modules`] | The files are read and their sizes counted, before anything is compiled. Those compiled earlier this session (the same bytes) are taken from memory. wasmtime's engine is made, the first time any theme has a module to run. |
//! | On a thread of its own: [`Job::run`] | Each module is checked (no imports, no start function, no more memory than the cap) and compiled. |
//! | Back on the theme's thread: [`Theme::install_modules`] | Each module is started (instantiated). The theme's hooks may run from then on, and not before. |
//! | A hook calls a module | It runs on the hook's thread, inside the hook's own deadline. |
//!
//! # What stops a module
//!
//! Compiled code checks wasmtime's epoch at every loop and call. The
//! epoch is moved on by a timer on the calling thread's processor time
//! (`timer_create` on `CLOCK_THREAD_CPUTIME_ID`, its signal sent to that
//! thread), armed for what is left of the running hook's deadline when a
//! call starts and disarmed when it returns: nothing ticks at rest. When the
//! epoch moves, the store asks the running hook's [`Span`] whether its time
//! is up: a tick from another thread's timer, or one early, lets the call
//! go on (the timer armed again for what is left); one late is a tick late,
//! and the timeout is judged by the span, never by whether the timer fired.
//! A call stopped so is the hook's timeout, struck as a Luau timeout is, and
//! the module starts afresh at its next use (its memory as on load).
//!
//! # The same pixels everywhere
//!
//! Relaxed SIMD is off and NaNs are made canonical, and there are no
//! threads: a module computes the same bytes on every machine (the goldens
//! depend on it). What is on is what modules built by rustc for
//! `wasm32-unknown-unknown` (with `simd128`) need: WebAssembly 2.0
//! (bulk memory, reference types, multi-value, sign extension, saturating
//! conversions, SIMD), and nothing after it.
//!
//! # Nothing until a theme has a module
//!
//! The base look, and a theme that names no module, never make wasmtime's
//! engine (so wasmtime's signal handlers are never installed), the timer or
//! its signal handler, nor start a thread ([`stats`] counts them). Compiled
//! code is kept in memory for the session only, by its bytes' hash (the
//! last eight modules); nothing is ever written to disk, and nothing
//! precompiled is ever loaded.
//!
//! [`Theme::prepare_modules`]: crate::theme::Theme::prepare_modules
//! [`Theme::install_modules`]: crate::theme::Theme::install_modules

use std::cell::{OnceCell, RefCell};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use mlua::{Lua, MultiValue, Table, Value};
use wasmtime::{Val, ValType};

use crate::Limits;
use crate::draw::Shared;
use crate::engine::{Span, running};
use crate::error::{TIMEOUT_MESSAGE, cap};
use crate::picture::{ImageId, Refused};
use crate::theme::{Source, safe_relative};

/// The native stack a module may use.
const STACK: usize = 512 << 10;
/// Tables a module may have, and the elements of each.
const TABLES: usize = 4;
const TABLE_ELEMENTS: usize = 65_536;
/// What one `module:read` may copy out, in bytes: the buffer it makes is
/// in the theme's Luau heap, whose cap unloads the theme.
pub const READ_BYTES: usize = 1 << 20;
/// Compiled modules kept for the session.
const CACHE_SIZE: usize = 8;
/// Call sites whose wrong calls are noted (one note each).
const SITES: usize = 64;
/// The compile thread's stack: Cranelift's own, on a module of the cap.
const COMPILE_STACK: usize = 16 << 20;
/// Why a theme's hooks wait.
pub(crate) const HELD: &str = "the theme's modules are still being compiled";

// ---- What it has cost --------------------------------------------------------

static ENGINES: AtomicU64 = AtomicU64::new(0);
static COMPILES: AtomicU64 = AtomicU64::new(0);
static INSTANCES: AtomicU64 = AtomicU64::new(0);
static CALLS: AtomicU64 = AtomicU64::new(0);
static TIMEOUTS: AtomicU64 = AtomicU64::new(0);
static TIMERS: AtomicU64 = AtomicU64::new(0);
static THREADS: AtomicU64 = AtomicU64::new(0);

/// What this process has done with modules, for the gates
/// (`GMNB_THEME_STATS`): all 0 in the base look and in a theme without a
/// module.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// wasmtime engines made (0 or 1): with the first, wasmtime's signal
    /// handlers are installed.
    pub engines: u64,
    /// Modules compiled (one taken from memory isn't).
    pub compiles: u64,
    /// Modules started: once each as its theme is applied, and again after
    /// a timeout.
    pub instances: u64,
    /// Calls into a module (`module:call`).
    pub calls: u64,
    /// Calls stopped at their deadline.
    pub timeouts: u64,
    /// Timers on a thread's processor time made (one per thread that
    /// has called a module).
    pub timers: u64,
    /// Compile threads started.
    pub threads: u64,
}

/// What this process has done with modules.
pub fn stats() -> Stats {
    let get = |a: &AtomicU64| a.load(Ordering::Relaxed);
    Stats {
        engines: get(&ENGINES),
        compiles: get(&COMPILES),
        instances: get(&INSTANCES),
        calls: get(&CALLS),
        timeouts: get(&TIMEOUTS),
        timers: get(&TIMERS),
        threads: get(&THREADS),
    }
}

fn count(a: &AtomicU64) {
    a.fetch_add(1, Ordering::Relaxed);
}

// ---- The engine ------------------------------------------------------------------

/// The one engine, made the first time a theme has a module to run.
static ENGINE: OnceLock<Result<wasmtime::Engine, String>> = OnceLock::new();

/// What the engine compiles and runs (see the module's docs).
fn config() -> wasmtime::Config {
    let mut c = wasmtime::Config::new();
    c.epoch_interruption(true)
        .max_wasm_stack(STACK)
        // The same pixels on every machine.
        .wasm_simd(true)
        .wasm_relaxed_simd(false)
        .cranelift_nan_canonicalization(true)
        // (No threads, GC or component model: wasmtime is built without
        // them.)
        .wasm_shared_everything_threads(false)
        // What rustc's wasm32 output uses: WebAssembly 2.0 (its reference
        // types too, of functions only: no GC types without `gc`).
        .wasm_bulk_memory(true)
        .wasm_multi_value(true)
        // Nothing after it.
        .wasm_multi_memory(false)
        .wasm_memory64(false)
        .wasm_tail_call(false)
        .wasm_extended_const(false)
        .wasm_wide_arithmetic(false)
        .wasm_custom_page_sizes(false)
        .wasm_exceptions(false)
        .wasm_stack_switching(false)
        // A trap's own words are enough; nothing read from the
        // environment.
        .wasm_backtrace(false)
        .wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Disable)
        .debug_info(false);
    c
}

/// The engine, made now if it hasn't been (and with it wasmtime's signal
/// handlers).
fn engine() -> Result<&'static wasmtime::Engine, String> {
    ENGINE
        .get_or_init(|| {
            count(&ENGINES);
            wasmtime::Engine::new(&config()).map_err(|e| format!("{e:#}"))
        })
        .as_ref()
        .map_err(String::clone)
}

// ---- The timer --------------------------------------------------------------------

/// The signal the timer sends: one of the real-time signals, which
/// nothing else in GMNB uses.
fn tick_signal() -> libc::c_int {
    libc::SIGRTMIN() + 3
}

/// The timer's signal: moves the engine's epoch on (one atomic add,
/// which wasmtime documents as safe in a signal handler).
extern "C" fn on_tick(_: libc::c_int) {
    if let Some(Ok(engine)) = ENGINE.get() {
        engine.increment_epoch();
    }
}

/// Whether the timer's signal handler is installed (once, the first time
/// a timer is made).
static HANDLER: OnceLock<bool> = OnceLock::new();

/// A one-shot timer on this thread's processor time, its signal sent to
/// this thread alone. Armed when a call starts, disarmed when it returns:
/// it never fires at rest, and a thread that waits uses no processor time.
struct CpuTimer {
    id: libc::timer_t,
}

impl CpuTimer {
    fn new() -> Option<CpuTimer> {
        let installed = *HANDLER.get_or_init(|| {
            // SAFETY: installs a handler that only does an atomic add, for
            // a signal nothing else uses.
            unsafe {
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction = on_tick as *const () as usize;
                sa.sa_flags = libc::SA_RESTART;
                libc::sigemptyset(&mut sa.sa_mask);
                libc::sigaction(tick_signal(), &sa, std::ptr::null_mut()) == 0
            }
        });
        if !installed {
            return None;
        }
        // SAFETY: makes a timer whose signal goes to this thread, written
        // into the local `id`.
        let id = unsafe {
            let mut ev: libc::sigevent = std::mem::zeroed();
            ev.sigev_notify = libc::SIGEV_THREAD_ID;
            ev.sigev_signo = tick_signal();
            ev.sigev_notify_thread_id = libc::gettid();
            let mut id: libc::timer_t = std::ptr::null_mut();
            if libc::timer_create(libc::CLOCK_THREAD_CPUTIME_ID, &mut ev, &mut id) != 0 {
                return None;
            }
            id
        };
        count(&TIMERS);
        Some(CpuTimer { id })
    }

    /// Fires after `after` more of this thread's processor time. False:
    /// it can't be (nothing would stop a call; none is made).
    ///
    /// The signal is unblocked on this thread first: a mask is inherited
    /// across `exec` and by new threads, and with the signal blocked the
    /// timer would stop nothing.
    fn arm(&self, after: Duration) -> bool {
        // SAFETY: builds a signal set on the stack and changes this
        // thread's mask by it.
        let unblocked = unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, tick_signal());
            libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut()) == 0
        };
        unblocked && self.set(after.max(Duration::from_micros(1)))
    }

    fn disarm(&self) {
        self.set(Duration::ZERO);
    }

    fn set(&self, after: Duration) -> bool {
        let value = libc::itimerspec {
            it_interval: libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            it_value: libc::timespec {
                tv_sec: after.as_secs() as libc::time_t,
                tv_nsec: after.subsec_nanos() as libc::c_long,
            },
        };
        // SAFETY: the timer is this one's, alive until it is dropped.
        unsafe { libc::timer_settime(self.id, 0, &value, std::ptr::null_mut()) == 0 }
    }
}

impl Drop for CpuTimer {
    fn drop(&mut self) {
        // SAFETY: the timer is this one's.
        unsafe { libc::timer_delete(self.id) };
    }
}

thread_local! {
    /// This thread's timer, made the first time it calls a module (`None`:
    /// it couldn't be made).
    static TIMER: OnceCell<Option<CpuTimer>> = const { OnceCell::new() };
}

/// `f` with this thread's timer, if it can have one.
fn with_timer<R>(f: impl FnOnce(&CpuTimer) -> R) -> Option<R> {
    TIMER.with(|t| t.get_or_init(CpuTimer::new).as_ref().map(f))
}

// ---- Compiled modules, kept for the session ------------------------------------------

struct Cached {
    hash: u64,
    bytes: Arc<[u8]>,
    module: wasmtime::Module,
}

/// The modules compiled this session, the most recently used last: a
/// theme previewed and then applied, or reloaded, isn't compiled again.
/// Found by their bytes' hash, and then the bytes themselves.
static CACHE: Mutex<Vec<Cached>> = Mutex::new(Vec::new());

fn hash(bytes: &[u8]) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn cached(hash: u64, bytes: &[u8]) -> Option<wasmtime::Module> {
    let mut cache = CACHE.lock().ok()?;
    let at = cache
        .iter()
        .position(|c| c.hash == hash && *c.bytes == *bytes)?;
    let entry = cache.remove(at);
    let module = entry.module.clone();
    cache.push(entry);
    Some(module)
}

fn remember(hash: u64, bytes: Arc<[u8]>, module: &wasmtime::Module) {
    let Ok(mut cache) = CACHE.lock() else { return };
    cache.retain(|c| !(c.hash == hash && c.bytes == bytes));
    if cache.len() >= CACHE_SIZE {
        cache.remove(0);
    }
    cache.push(Cached {
        hash,
        bytes,
        module: module.clone(),
    });
}

// ---- Before compiling: what a module may be ----------------------------------------

/// Why `bytes` can't be a theme's module, if they can't: checked before
/// they are compiled. A module may import nothing, have no start function,
/// and declare no more memory than `memory` bytes (it can't grow past them
/// either).
pub(crate) fn precheck(bytes: &[u8], memory: usize) -> Result<(), String> {
    use wasmparser::{Encoding, Parser, Payload};
    if !bytes.starts_with(b"\0asm") {
        return Err("is not a WebAssembly module (by its first bytes)".into());
    }
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| format!("is not a well-formed WebAssembly module: {e}"))? {
            Payload::Version {
                encoding: Encoding::Component,
                ..
            } => return Err("is a component; a theme's module is a core module".into()),
            Payload::ImportSection(imports) => {
                if let Some(import) = imports.into_iter().next() {
                    let import = import.map_err(|e| format!("is not a well-formed module: {e}"))?;
                    return Err(format!(
                        "imports {}.{}: a theme's module may import nothing",
                        cap(import.module, 64),
                        cap(import.name, 64)
                    ));
                }
            }
            Payload::StartSection { .. } => {
                return Err("has a start function: a theme's module may not".into());
            }
            Payload::MemorySection(memories) => {
                for m in memories {
                    let m = m.map_err(|e| format!("is not a well-formed module: {e}"))?;
                    if m.memory64 || m.shared {
                        return Err(
                            "declares a 64-bit or shared memory; a module's memory is plain 32-bit"
                                .into(),
                        );
                    }
                    let page = 1u64 << m.page_size_log2.unwrap_or(16);
                    let bytes = m.initial.saturating_mul(page);
                    if bytes > memory as u64 {
                        return Err(format!(
                            "declares {} MiB of memory; a module's memory is at most {} MiB",
                            bytes >> 20,
                            memory >> 20
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

// ---- A theme's modules ------------------------------------------------------------------

/// What the running module is told, kept in its store: what its epoch
/// callback asks.
struct Data {
    limits: wasmtime::StoreLimits,
    /// The running hook's span, while a call runs.
    span: Option<Span>,
}

/// A module started: its store, instance and memory.
struct Live {
    store: wasmtime::Store<Data>,
    instance: wasmtime::Instance,
    memory: Option<wasmtime::Memory>,
}

/// Starts `module`, its memory capped at `memory` bytes. It has no start
/// function (checked before it was compiled), so no code of it runs.
fn instantiate(module: &wasmtime::Module, memory: usize) -> Result<Live, String> {
    let limits = wasmtime::StoreLimitsBuilder::new()
        .memory_size(memory)
        .instances(1)
        .memories(1)
        .tables(TABLES)
        .table_elements(TABLE_ELEMENTS)
        .build();
    let mut store = wasmtime::Store::new(module.engine(), Data { limits, span: None });
    store.limiter(|d| &mut d.limits);
    // The epoch moved: the call goes on unless the hook's time is up
    // (a tick from another thread's timer, or one early, arms this
    // thread's again for what is left; one that can't be armed again stops
    // the call, which nothing would stop then).
    store.epoch_deadline_callback(|ctx| match ctx.data().span {
        Some(span) if span.is_over() => Err(wasmtime::Trap::Interrupt.into()),
        Some(span) => match with_timer(|t| t.arm(span.left())) {
            Some(true) => Ok(wasmtime::UpdateDeadline::Continue(1)),
            _ => Err(wasmtime::Trap::Interrupt.into()),
        },
        None => Ok(wasmtime::UpdateDeadline::Continue(1)),
    });
    store.set_epoch_deadline(1);
    let instance = wasmtime::Instance::new(&mut store, module, &[])
        .map_err(|e| format!("couldn't be started: {}", short(&e)))?;
    count(&INSTANCES);
    let memory = instance.get_memory(&mut store, "memory").or_else(|| {
        let exports: Vec<_> = instance.exports(&mut store).collect();
        exports.into_iter().find_map(|e| e.into_memory())
    });
    Ok(Live {
        store,
        instance,
        memory,
    })
}

/// A wasmtime error, in one line.
fn short(e: &wasmtime::Error) -> String {
    let text = match e.downcast_ref::<wasmtime::Trap>() {
        Some(trap) => {
            let text = trap.to_string();
            text.strip_prefix("wasm trap: ")
                .unwrap_or(&text)
                .to_string()
        }
        None => format!("{e:#}"),
    };
    cap(&text.replace('\n', " | "), 300)
}

enum State {
    /// Named by the theme's file; not read yet.
    Named,
    /// Read, and given to a [`Job`] to compile.
    Compiling,
    /// Left out, and why.
    Refused(String),
    /// Compiled; and started, unless a timeout ended it (it is started
    /// again at its next use).
    Ready {
        module: wasmtime::Module,
        live: Option<Box<Live>>,
    },
}

struct Slot {
    file: String,
    state: State,
}

static NEXT_SET: AtomicU64 = AtomicU64::new(1);

/// One theme's modules.
pub(crate) struct Modules {
    /// Which theme's: what a [`Compiled`] is checked against.
    set: u64,
    source: Source,
    max: usize,
    bytes: u64,
    memory: usize,
    slots: Vec<Slot>,
    /// Files refused before they were given a slot (not `.wasm`, one too
    /// many, named in a hook), each once.
    refused: Vec<String>,
    /// The theme has been prepared: no more may be named.
    closed: bool,
    /// Every module is compiled (or refused) and started: hooks may run.
    ready: bool,
    /// What the theme's author should know of loading them: (file, why).
    load_notes: Vec<(String, String)>,
    /// What they should know of calls since the last look.
    notes: Vec<String>,
    /// The call sites already noted.
    sites: HashSet<String>,
}

impl Modules {
    pub(crate) fn new(source: Source, limits: &Limits) -> Modules {
        Modules {
            set: NEXT_SET.fetch_add(1, Ordering::Relaxed),
            source,
            max: limits.modules,
            bytes: limits.module_bytes,
            memory: limits.module_memory,
            slots: Vec::new(),
            refused: Vec::new(),
            closed: false,
            ready: false,
            load_notes: Vec::new(),
            notes: Vec::new(),
            sites: HashSet::new(),
        }
    }

    /// The files named, in order.
    pub(crate) fn files(&self) -> Vec<String> {
        self.slots.iter().map(|s| s.file.clone()).collect()
    }

    pub(crate) fn named(&self) -> bool {
        !self.slots.is_empty()
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.ready
    }

    pub(crate) fn take_load_notes(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.load_notes)
    }

    pub(crate) fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    fn refuse(&mut self, at: usize, why: String) {
        let file = self.slots[at].file.clone();
        self.load_notes.push((file, why.clone()));
        self.slots[at].state = State::Refused(why);
    }

    /// `gmnb.module(file)`: the slot of `file`, named now if it is new.
    /// `None`: refused (noted).
    fn name(&mut self, file: &str) -> Option<usize> {
        if let Some(at) = self.slots.iter().position(|s| s.file == file) {
            return Some(at);
        }
        let why = if !safe_relative(file) || !file.ends_with(".wasm") {
            format!("{file:?} is not a .wasm file's path inside the theme's folder")
        } else if self.closed {
            format!(
                "{file} was first named in a hook; a theme names its modules at the top of its file"
            )
        } else if self.slots.len() >= self.max {
            format!("{file}: a theme may have {} modules; left out", self.max)
        } else {
            self.slots.push(Slot {
                file: file.to_string(),
                state: State::Named,
            });
            return Some(self.slots.len() - 1);
        };
        if !self.refused.iter().any(|f| f == file) && self.refused.len() < 64 {
            self.refused.push(file.to_string());
            self.notes.push(format!("gmnb.module: {why}"));
        }
        None
    }

    /// The theme is applied: what is left to compile, if anything (see
    /// [`Theme::prepare_modules`](crate::theme::Theme::prepare_modules)).
    pub(crate) fn prepare(&mut self) -> Option<Job> {
        self.closed = true;
        // Ready already, or a job is out: nothing more (and the hooks wait
        // for that job's install).
        if self.ready
            || self
                .slots
                .iter()
                .any(|s| matches!(s.state, State::Compiling))
        {
            return None;
        }
        let mut items = Vec::new();
        let mut total = 0u64;
        for at in 0..self.slots.len() {
            if !matches!(self.slots[at].state, State::Named) {
                continue;
            }
            let file = self.slots[at].file.clone();
            let size = match self.source.size(&file) {
                Ok(size) => size,
                Err(e) => {
                    self.refuse(at, e);
                    continue;
                }
            };
            if total + size > self.bytes {
                let why = format!(
                    "the theme's modules are over {} KiB together; left out",
                    self.bytes >> 10
                );
                self.refuse(at, why);
                continue;
            }
            let bytes = match self.source.read(&file, self.bytes - total) {
                Ok(bytes) => bytes,
                Err(e) => {
                    self.refuse(at, e);
                    continue;
                }
            };
            total += size;
            let hash = hash(&bytes);
            match cached(hash, &bytes) {
                Some(module) => {
                    self.slots[at].state = State::Ready { module, live: None };
                }
                None => {
                    self.slots[at].state = State::Compiling;
                    items.push(Item {
                        slot: at,
                        bytes: bytes.into(),
                        hash,
                    });
                }
            }
        }
        let wanted = self
            .slots
            .iter()
            .any(|s| matches!(s.state, State::Ready { .. } | State::Compiling));
        if !wanted {
            self.finish();
            return None;
        }
        let engine = match engine() {
            Ok(engine) => engine,
            Err(why) => {
                let why = format!("this GMNB can't run modules: {why}");
                for at in 0..self.slots.len() {
                    if matches!(self.slots[at].state, State::Ready { .. } | State::Compiling) {
                        self.refuse(at, why.clone());
                    }
                }
                self.finish();
                return None;
            }
        };
        if items.is_empty() {
            self.finish();
            return None;
        }
        Some(Job {
            set: self.set,
            engine,
            memory: self.memory,
            items,
        })
    }

    /// What a [`Job`] compiled: each module refused or ready, and every
    /// one started.
    pub(crate) fn install(&mut self, compiled: Compiled) {
        if compiled.set != self.set {
            return;
        }
        for (at, result) in compiled.results {
            if !matches!(self.slots.get(at).map(|s| &s.state), Some(State::Compiling)) {
                continue;
            }
            match result {
                Ok(module) => self.slots[at].state = State::Ready { module, live: None },
                Err(why) => self.refuse(at, why),
            }
        }
        // A slot left waiting (a job that lost one) is refused, never left
        // half-done.
        for at in 0..self.slots.len() {
            if matches!(self.slots[at].state, State::Compiling | State::Named) {
                self.refuse(at, "wasn't compiled".into());
            }
        }
        self.finish();
    }

    /// Starts every module compiled: from now on the theme's hooks run.
    fn finish(&mut self) {
        for at in 0..self.slots.len() {
            if let Err(why) = self.start(at) {
                self.refuse(at, why);
            }
        }
        self.ready = true;
    }

    /// Starts the module of `at` if it is ready and isn't running.
    fn start(&mut self, at: usize) -> Result<(), String> {
        let memory = self.memory;
        if let State::Ready { module, live } = &mut self.slots[at].state
            && live.is_none()
        {
            *live = Some(Box::new(instantiate(module, memory)?));
        }
        Ok(())
    }

    /// Notes `message` about a call of `what` made where the theme's code
    /// is now, the first time a call there goes wrong: once per call site
    /// (whatever numbers it is given each time).
    fn tell(&mut self, lua: &Lua, what: &str, message: String) {
        let site = site(lua);
        let key = format!("{site} {what}");
        if self.sites.len() >= SITES || !self.sites.insert(key) {
            return;
        }
        self.notes.push(if site.is_empty() {
            message
        } else {
            format!("{site}: {message}")
        });
    }

    /// The module of `at`, made ready to use: started again if a timeout
    /// ended it. `Err`: why it can't be used (the call does nothing).
    fn ensure(&mut self, at: usize) -> Result<(), String> {
        if let State::Refused(why) = &self.slots[at].state {
            return Err(format!("{} was left out: {why}", self.slots[at].file));
        }
        if let Err(why) = self.start(at) {
            let file = self.slots[at].file.clone();
            self.refuse(at, why.clone());
            return Err(format!("{file} {why}"));
        }
        Ok(())
    }

    /// The running module of `at` ([`Modules::ensure`] first).
    fn live(&mut self, at: usize) -> Option<&mut Live> {
        match &mut self.slots[at].state {
            State::Ready {
                live: Some(live), ..
            } => Some(live),
            _ => None,
        }
    }

    /// A call of `at` was stopped at its deadline: the module starts
    /// afresh at its next use (a call stopped midway may have left its
    /// memory half-written).
    fn stopped(&mut self, at: usize) {
        count(&TIMEOUTS);
        if let State::Ready { live, .. } = &mut self.slots[at].state {
            *live = None;
        }
    }
}

/// One module to compile.
struct Item {
    slot: usize,
    bytes: Arc<[u8]>,
    hash: u64,
}

/// What is left to compile of a theme's modules
/// ([`Theme::prepare_modules`](crate::theme::Theme::prepare_modules)): run
/// it on a thread of its own ([`Job::run`], [`Job::spawn`]), and give what
/// it made to [`Theme::install_modules`](crate::theme::Theme::install_modules).
/// It reads no file and touches no theme: it can't be stopped midway, and
/// is bounded by the bytes it was given ([`Limits::module_bytes`]).
pub struct Job {
    set: u64,
    engine: &'static wasmtime::Engine,
    memory: usize,
    items: Vec<Item>,
}

/// What a [`Job`] compiled: each module, or why it was refused.
pub struct Compiled {
    set: u64,
    results: Vec<(usize, Result<wasmtime::Module, String>)>,
}

impl Job {
    /// The bytes it will compile.
    pub fn bytes(&self) -> u64 {
        self.items.iter().map(|i| i.bytes.len() as u64).sum()
    }

    /// Checks and compiles each module, here and now (call it on a thread
    /// of its own).
    pub fn run(self) -> Compiled {
        let results = self
            .items
            .into_iter()
            .map(|item| {
                // A panic in the checks or the compiler is the module
                // refused, never a compile that doesn't come back.
                let compiled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    precheck(&item.bytes, self.memory).and_then(|()| {
                        count(&COMPILES);
                        wasmtime::Module::new(self.engine, &item.bytes)
                            .map_err(|e| format!("doesn't compile: {}", short(&e)))
                    })
                }));
                let result = compiled
                    .unwrap_or_else(|_| Err("couldn't be compiled (the compiler failed)".into()));
                if let Ok(module) = &result {
                    remember(item.hash, item.bytes.clone(), module);
                }
                (item.slot, result)
            })
            .collect();
        Compiled {
            set: self.set,
            results,
        }
    }

    /// [`Job::run`] on a thread started for it, gone after; `done` is
    /// called there with what it made. `Err`: no thread could be started,
    /// and what comes back has every module of the job refused, saying so
    /// (install it as a job's).
    pub fn spawn(
        self,
        done: impl FnOnce(Compiled) + Send + 'static,
    ) -> Result<std::thread::JoinHandle<()>, Compiled> {
        let set = self.set;
        let slots: Vec<usize> = self.items.iter().map(|i| i.slot).collect();
        match std::thread::Builder::new()
            .name("gmnb-compile".into())
            .stack_size(COMPILE_STACK)
            .spawn(move || done(self.run()))
        {
            Ok(thread) => {
                count(&THREADS);
                Ok(thread)
            }
            Err(e) => Err(Compiled {
                set,
                results: slots
                    .into_iter()
                    .map(|slot| {
                        let why = format!("couldn't be compiled: no thread to do it on ({e})");
                        (slot, Err(why))
                    })
                    .collect(),
            }),
        }
    }
}

// ---- The Luau side ------------------------------------------------------------------------

/// `file:line` of the theme's code calling the host function now.
fn site(lua: &Lua) -> String {
    lua.inspect_stack(1, |d| {
        let src = d.source().short_src.map(|s| s.into_owned());
        match (src, d.current_line()) {
            (Some(src), Some(line)) => format!("{src}:{line}"),
            (Some(src), None) => src,
            _ => String::new(),
        }
    })
    .unwrap_or_default()
}

fn not_ready(file: &str) -> mlua::Error {
    mlua::Error::runtime(format!(
        "gmnb.module: {file} can't be used at the top of the file: only in a hook (draw, \
         preview, …), where it is always ready"
    ))
}

/// A whole number of 0 or more, to `u32::MAX`.
fn index(v: &Value) -> Option<usize> {
    let n = match v {
        Value::Integer(i) => *i as f64,
        Value::Number(n) => *n,
        _ => return None,
    };
    (n.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&n)).then_some(n as usize)
}

/// A Luau number as a wasm value of type `ty`, if it is one: a whole
/// number for an integer (an `i32` takes −2³¹ to 2³² − 1, the top half
/// as the same bits), any number for a float.
fn arg(v: &Value, ty: &ValType) -> Option<Val> {
    let n = match v {
        Value::Integer(i) => *i as f64,
        Value::Number(n) => *n,
        _ => return None,
    };
    let whole = n.fract() == 0.0;
    match ty {
        ValType::I32 if whole && (-2_147_483_648.0..4_294_967_296.0).contains(&n) => {
            Some(Val::I32(if n >= 2_147_483_648.0 {
                n as u32 as i32
            } else {
                n as i32
            }))
        }
        ValType::I64
            if whole
                && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&n) =>
        {
            Some(Val::I64(n as i64))
        }
        ValType::F32 => Some(Val::F32((n as f32).to_bits())),
        ValType::F64 => Some(Val::F64(n.to_bits())),
        _ => None,
    }
}

fn number_type(ty: &ValType) -> bool {
    matches!(
        ty,
        ValType::I32 | ValType::I64 | ValType::F32 | ValType::F64
    )
}

fn type_name(ty: &ValType) -> &'static str {
    match ty {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "f32",
        ValType::F64 => "f64",
        ValType::V128 => "v128",
        ValType::Ref(_) => "a reference",
    }
}

fn result(v: &Val) -> Value {
    Value::Number(match v {
        Val::I32(i) => f64::from(*i),
        Val::I64(i) => *i as f64,
        Val::F32(f) => f64::from(f32::from_bits(*f)),
        Val::F64(f) => f64::from_bits(*f),
        _ => 0.0,
    })
}

/// Which of the theme's modules `this` is (its table's `id`), for a call
/// of `what`: its slot and file. `Ok(None)`: none (noted; the call does
/// nothing). At the top of the file, before the modules are ready, any
/// use of one is an error.
fn which(
    lua: &Lua,
    m: &mut Modules,
    what: &str,
    this: &Value,
) -> mlua::Result<Option<(usize, String)>> {
    let id = match this {
        Value::Table(t) => index(&t.raw_get::<Value>("id")?),
        _ => None,
    };
    match id {
        // A module refused as it was named.
        Some(0) => {
            if !m.ready {
                return Err(not_ready("this module"));
            }
            let why = format!("{what}: this module was left out (gmnb.module's note says why)");
            m.tell(lua, what, why);
            Ok(None)
        }
        Some(at) if at <= m.slots.len() => {
            let file = m.slots[at - 1].file.clone();
            if !m.ready {
                return Err(not_ready(&file));
            }
            Ok(Some((at - 1, file)))
        }
        _ => {
            let why = match what.strip_prefix("module:") {
                Some(name) => format!(
                    "{what} was called on something that isn't a module (call it with a \
                     colon: paint:{name})"
                ),
                None => format!("{what} takes a module (gmnb.module) to take the picture from"),
            };
            m.tell(lua, what, why);
            Ok(None)
        }
    }
}

/// What a call of `export` with `args` is: the function, its name, the
/// values for it and how many it returns. `Err`: why it can't be made
/// (the call does nothing, with a note).
fn shape(
    m: &mut Modules,
    at: usize,
    file: &str,
    export: &Value,
    args: &[Value],
) -> Result<(wasmtime::Func, String, Vec<Val>, usize), String> {
    let Value::String(name) = export else {
        return Err(format!(
            "module:call: {file}: the function to call is named by a string"
        ));
    };
    let name = name.to_string_lossy();
    m.ensure(at)?;
    let live = m.live(at).ok_or("not started")?;
    let Some(func) = live.instance.get_func(&mut live.store, &name) else {
        return Err(format!(
            "module:call: {file} has no function {:?} to call",
            cap(&name, 64)
        ));
    };
    let ty = func.ty(&live.store);
    let params: Vec<ValType> = ty.params().collect();
    if !params.iter().all(number_type) || !ty.results().all(|t| number_type(&t)) {
        return Err(format!(
            "module:call: {file}: {name} takes or returns something other than numbers; a theme \
             can't call it"
        ));
    }
    let values: Option<Vec<Val>> = (args.len() == params.len())
        .then(|| args.iter().zip(&params).map(|(v, t)| arg(v, t)).collect())
        .flatten();
    let Some(values) = values else {
        let want: Vec<&str> = params.iter().map(type_name).collect();
        return Err(format!(
            "module:call: {file}: {name} takes ({}); it was given {} arguments that don't fit",
            want.join(", "),
            args.len()
        ));
    };
    Ok((func, name, values, ty.results().len()))
}

/// `module:call(export, ...)`: on this thread, inside the running hook's
/// deadline.
fn call(lua: &Lua, modules: &RefCell<Modules>, args: MultiValue) -> mlua::Result<MultiValue> {
    let mut args = args.into_iter();
    let this = args.next().unwrap_or(Value::Nil);
    let export = args.next().unwrap_or(Value::Nil);
    let args: Vec<Value> = args.collect();
    let mut m = modules.borrow_mut();
    let Some((at, file)) = which(lua, &mut m, "module:call", &this)? else {
        return Ok(MultiValue::new());
    };
    let (func, name, values, results) = match shape(&mut m, at, &file, &export, &args) {
        Ok(shaped) => shaped,
        Err(why) => {
            m.tell(lua, "module:call", why);
            return Ok(MultiValue::new());
        }
    };
    let running = running(lua);
    let Some(span) = running.as_ref().and_then(|r| r.span()) else {
        return Err(mlua::Error::runtime(
            "module:call can only be called while one of the theme's hooks runs",
        ));
    };
    let timeout = |m: &mut Modules| {
        m.stopped(at);
        if let Some(r) = &running {
            r.stop();
        }
        Err(mlua::Error::runtime(TIMEOUT_MESSAGE))
    };
    // No time left (a `pcall` around a call that ran out): nothing runs.
    if span.is_over() {
        return timeout(&mut m);
    }
    let Some(live) = m.live(at) else {
        return Ok(MultiValue::new());
    };
    count(&CALLS);
    let mut out = vec![Val::I32(0); results];
    live.store.data_mut().span = Some(span);
    live.store.set_epoch_deadline(1);
    let done = match with_timer(|t| t.arm(span.left())) {
        Some(true) => {
            let done = func.call(&mut live.store, &values, &mut out);
            with_timer(CpuTimer::disarm);
            Some(done)
        }
        _ => None,
    };
    live.store.data_mut().span = None;
    match done {
        None => Err(mlua::Error::runtime(
            "module:call: this system can't time a module's call, so none is made",
        )),
        Some(Ok(())) => Ok(out.iter().map(result).collect()),
        // Judged by the span: the timer may be a tick late, or another
        // thread's.
        Some(Err(_)) if span.is_over() => timeout(&mut m),
        Some(Err(e)) => Err(mlua::Error::runtime(format!(
            "module:call: {file}: {name}: {}",
            short(&e)
        ))),
    }
}

/// The module's memory, made ready to use, for a call of `what`. `Err`:
/// why it can't be used.
fn memory(m: &mut Modules, at: usize, file: &str, what: &str) -> Result<wasmtime::Memory, String> {
    m.ensure(at).map_err(|why| format!("{what}: {why}"))?;
    m.live(at)
        .and_then(|live| live.memory)
        .ok_or_else(|| format!("{what}: {file} has no memory"))
}

/// `module:write(offset, buffer)`.
fn write(
    lua: &Lua,
    modules: &RefCell<Modules>,
    (this, offset, data): (Value, Value, Value),
) -> mlua::Result<()> {
    let what = "module:write";
    let mut m = modules.borrow_mut();
    let Some((at, file)) = which(lua, &mut m, what, &this)? else {
        return Ok(());
    };
    let written = (|| {
        let (Some(offset), Value::Buffer(data)) = (index(&offset), &data) else {
            return Err(format!(
                "{what}: {file}: it takes an offset (a whole number) and a buffer"
            ));
        };
        let memory = memory(&mut m, at, &file, what)?;
        let live = m.live(at).ok_or("not started")?;
        let bytes = memory.data_mut(&mut live.store);
        let size = bytes.len();
        let into = offset
            .checked_add(data.len())
            .filter(|end| *end <= size)
            .map(|end| &mut bytes[offset..end])
            .ok_or_else(|| {
                format!(
                    "{what}: {file}: {} bytes at {offset} don't fit its memory of {size}",
                    data.len()
                )
            })?;
        // Straight from the buffer into the module's memory.
        data.clone()
            .cursor()
            .read_exact(into)
            .map_err(|e| format!("{what}: {e}"))
    })();
    if let Err(why) = written {
        m.tell(lua, what, why);
    }
    Ok(())
}

/// `module:read(offset, length)`: a new buffer.
fn read(
    lua: &Lua,
    modules: &RefCell<Modules>,
    (this, offset, length): (Value, Value, Value),
) -> mlua::Result<Value> {
    let what = "module:read";
    let mut m = modules.borrow_mut();
    let Some((at, file)) = which(lua, &mut m, what, &this)? else {
        return Ok(Value::Nil);
    };
    let copied = (|| {
        let (Some(offset), Some(length)) = (index(&offset), index(&length)) else {
            return Err(format!(
                "{what}: {file}: it takes an offset and a length (whole numbers)"
            ));
        };
        if length > READ_BYTES {
            return Err(format!(
                "{what}: {file}: {length} bytes; one read is at most {READ_BYTES}"
            ));
        }
        let memory = memory(&mut m, at, &file, what)?;
        let live = m.live(at).ok_or("not started")?;
        let bytes = memory.data(&live.store);
        offset
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .map(|end| lua.create_buffer(&bytes[offset..end]))
            .ok_or_else(|| {
                format!(
                    "{what}: {file}: {length} bytes at {offset} aren't in its memory of {}",
                    bytes.len()
                )
            })
    })();
    match copied {
        Ok(buffer) => buffer.map(Value::Buffer),
        Err(why) => {
            m.tell(lua, what, why);
            Ok(Value::Nil)
        }
    }
}

/// The picture a picture table names (its `id`), if it has one.
fn picture_of(this: &Value) -> mlua::Result<Option<ImageId>> {
    let id = match this {
        Value::Table(t) => index(&t.raw_get::<Value>("id")?),
        _ => None,
    };
    Ok(id.filter(|id| *id >= 1).map(|id| ImageId(id as u32 - 1)))
}

/// `picture:take(module, offset, width, height)`: `width × height × 4`
/// bytes of the module's memory become the picture.
fn take(
    lua: &Lua,
    modules: &RefCell<Modules>,
    shared: &Shared,
    (this, module, offset, width, height): (Value, Value, Value, Value, Value),
) -> mlua::Result<bool> {
    let what = "picture:take";
    let mut m = modules.borrow_mut();
    let Some((at, file)) = which(lua, &mut m, what, &module)? else {
        return Ok(false);
    };
    // A picture over the count takes nothing.
    let Some(picture) = picture_of(&this)? else {
        return Ok(false);
    };
    let side = shared.recorder.borrow().limits().canvas_side as usize;
    let taken = (|| {
        let (Some(offset), Some(width), Some(height)) =
            (index(&offset), index(&width), index(&height))
        else {
            return Err(format!(
                "{what}: {file}: it takes an offset, a width and a height (whole numbers)"
            ));
        };
        if width == 0 || height == 0 || width > side || height > side {
            return Err(format!(
                "{what}: {width}×{height}; a module's picture is 1×1 to {side}×{side}"
            ));
        }
        let memory = memory(&mut m, at, &file, what)?;
        let live = m.live(at).ok_or("not started")?;
        let bytes = memory.data(&live.store);
        let length = width * height * 4;
        let from = offset
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .map(|end| &bytes[offset..end])
            .ok_or_else(|| {
                format!(
                    "{what}: {file}: {width}×{height} at {offset} isn't in its memory of {}",
                    bytes.len()
                )
            })?;
        let now = shared.now();
        match shared.pictures.borrow_mut().take_module(
            picture,
            width as u32,
            height as u32,
            from,
            now,
        ) {
            Ok(()) => Ok(true),
            // Too soon: the old picture stays, and a layer drawing is
            // drawn once more when it can be changed.
            Err(Refused::TooSoon(wait)) => {
                if wait.is_finite() {
                    shared.recorder.borrow_mut().retry(wait.max(1.0 / 60.0));
                }
                Ok(false)
            }
            Err(Refused::Over(why)) => Err(why),
        }
    })();
    match taken {
        Ok(taken) => Ok(taken),
        Err(why) => {
            m.tell(lua, what, why);
            Ok(false)
        }
    }
}

/// `picture:ready()`: whether a `take` now would be taken. When it
/// wouldn't, a layer drawing is owed a frame for when it would, as after a
/// refused `take`: a theme that skips its work on `false` is never left
/// showing a picture it has since moved past.
fn ready(shared: &Shared, this: &Value) -> mlua::Result<bool> {
    let Some(picture) = picture_of(this)? else {
        return Ok(false);
    };
    let now = shared.now();
    let wait = shared.pictures.borrow().module_wait(picture, now);
    if wait <= 0.0 {
        return Ok(true);
    }
    if wait.is_finite() {
        shared.recorder.borrow_mut().retry(wait.max(1.0 / 60.0));
    }
    Ok(false)
}

/// Adds `gmnb.module`. Each module's table is made once and kept, by its
/// slot, in a table of the registry's; a module picture's table is made
/// by each `module:picture()`.
pub(crate) fn register(
    lua: &Lua,
    gmnb: &Table,
    shared: &Rc<Shared>,
    modules: &Rc<RefCell<Modules>>,
) -> mlua::Result<()> {
    let m = modules.clone();
    let call = lua.create_function(move |lua, args: MultiValue| call(lua, &m, args))?;
    let m = modules.clone();
    let write = lua.create_function(move |lua, args| write(lua, &m, args))?;
    let m = modules.clone();
    let read = lua.create_function(move |lua, args| read(lua, &m, args))?;
    let (m, s) = (modules.clone(), shared.clone());
    let take = lua.create_function(move |lua, args| take(lua, &m, &s, args))?;
    let s = shared.clone();
    let is_ready = lua.create_function(move |_, this: Value| ready(&s, &this))?;
    // A module picture's size is what its last `take` gave it (0 × 0
    // before its first).
    let s = shared.clone();
    let size = lua.create_function(move |_, (this, key): (Value, Value)| {
        let Value::String(key) = key else {
            return Ok(Value::Nil);
        };
        let canvas = picture_of(&this)?.and_then(|id| s.pictures.borrow().canvas(id).cloned());
        let (w, h) = canvas.map_or((0, 0), |c| (c.width, c.height));
        Ok(match &*key.as_bytes() {
            b"width" => Value::Number(f64::from(w)),
            b"height" => Value::Number(f64::from(h)),
            _ => Value::Nil,
        })
    })?;
    let meta = lua.create_table_with_capacity(0, 2)?;
    meta.raw_set("__index", size)?;
    meta.raw_set("__metatable", false)?;
    meta.set_readonly(true);
    let meta = Rc::new(lua.create_registry_value(meta)?);
    let s = shared.clone();
    let picture = lua.create_function(move |lua, _this: Value| {
        // (Asked in a statement of its own: the pictures are free again
        // before the table is made.)
        let id = s.pictures.borrow_mut().module_picture();
        let t = lua.create_table_with_capacity(0, 3)?;
        if let Some(id) = id {
            t.raw_set("id", id.0 + 1)?;
        }
        t.raw_set("take", take.clone())?;
        t.raw_set("ready", is_ready.clone())?;
        t.set_metatable(Some(lua.registry_value::<Table>(&meta)?))?;
        t.set_readonly(true);
        Ok(t)
    })?;
    let table = move |lua: &Lua, id: usize, file: Option<&str>| -> mlua::Result<Table> {
        let t = lua.create_table_with_capacity(0, 6)?;
        t.raw_set("id", id)?;
        if let Some(file) = file {
            t.raw_set("file", file)?;
        }
        t.raw_set("call", call.clone())?;
        t.raw_set("write", write.clone())?;
        t.raw_set("read", read.clone())?;
        t.raw_set("picture", picture.clone())?;
        t.set_readonly(true);
        Ok(t)
    };
    // What a module refused as it is named is: its calls do nothing
    // (noted).
    let left_out = Rc::new(lua.create_registry_value(table(lua, 0, None)?)?);
    // The modules' tables, by slot + 1.
    let tables = Rc::new(lua.create_registry_value(lua.create_table()?)?);
    let m = modules.clone();
    gmnb.raw_set(
        "module",
        lua.create_function(move |lua, file: Value| {
            let Value::String(file) = file else {
                return Err(mlua::Error::runtime(
                    "gmnb.module: expected a .wasm file's path in the theme's folder",
                ));
            };
            let file = file.to_str()?.to_string();
            let at = m.borrow_mut().name(&file);
            let Some(at) = at else {
                return lua.registry_value::<Table>(&left_out);
            };
            let tables: Table = lua.registry_value(&tables)?;
            if let Value::Table(t) = tables.raw_get::<Value>(at + 1)? {
                return Ok(t);
            }
            let t = table(lua, at + 1, Some(&file))?;
            tables.raw_set(at + 1, t.clone())?;
            Ok(t)
        })?,
    )?;
    Ok(())
}
