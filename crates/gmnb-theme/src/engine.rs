use std::cell::{Cell, RefCell};
use std::collections::{HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use mlua::chunk::ChunkMode;
use mlua::{
    Function, IntoLuaMulti, Lua, LuaOptions, LuaString, MultiValue, StdLib, Table, Value, VmState,
};

use crate::error::{BYTECODE, TIMEOUT_MESSAGE, cap, classify};
use crate::value::Hook;
use crate::{Budget, FunctionHandle, Limits, ThemeError, ThemeValue};

/// Luau's standard libraries but `debug` (the theme contract leaves it
/// out) and `integer` (behind a Luau flag in this version). The base
/// library always comes with them.
fn libs() -> StdLib {
    StdLib::COROUTINE
        | StdLib::TABLE
        | StdLib::OS
        | StdLib::STRING
        | StdLib::UTF8
        | StdLib::BIT
        | StdLib::MATH
        | StdLib::BUFFER
        | StdLib::VECTOR
}

/// Globals taken away before the sandbox closes (see the crate docs).
const REMOVED: [&str; 5] = ["require", "loadstring", "getfenv", "setfenv", "newproxy"];

/// How deep `print` may be re-entered (from a `__tostring` it calls).
const PRINT_NESTING: u32 = 4;

/// The Luau memory category of the host's own tables ([`HostTable`]): what
/// is allocated while it is active is counted apart from the theme's
/// ("main"), so the heap cap can be the theme's alone.
const HOST_CATEGORY: &str = "host";
/// Its number: mlua numbers categories as they are first named, after
/// "main" (0). (A test pins this: the host's bytes are seen to grow.)
const HOST_CATEGORY_ID: i32 = 1;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// One theme's Luau VM: sandboxed, its heap capped, every entry into it
/// under a deadline (see the crate docs). Not `Send`: a theme lives on the
/// thread that made it.
pub struct Engine {
    lua: Lua,
    id: u64,
    limits: Limits,
    shared: Rc<Shared>,
    /// What the host's tables held when last measured.
    host_bytes: Cell<usize>,
}

/// What the interrupt and `print` share with the engine.
struct Shared {
    /// Until when the running entry may run, if any is running.
    deadline: Cell<Option<Instant>>,
    /// Whether the interrupt stopped the running entry.
    timed_out: Cell<bool>,
    /// The fatal error that stopped the engine (the heap cap, or a panic
    /// inside mlua): everything after fails with it.
    dead: RefCell<Option<ThemeError>>,
    print_nesting: Cell<u32>,
    log: RefCell<LogBuf>,
}

/// What scripts printed ([`Engine::take_log`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    /// Oldest first.
    pub lines: Vec<String>,
    /// Older lines dropped to keep within [`Limits::log_lines`] and
    /// [`Limits::log_bytes`].
    pub dropped: usize,
}

/// A table the host makes once and refills in place before each call (a
/// frame's or a style call's context), with [`Engine::fill`]. Scripts see
/// it read-only, so it holds what the host last put in it, and no more.
pub struct HostTable {
    engine: u64,
    table: Table,
}

impl HostTable {
    /// The table, to pass to [`Engine::call`] or to put in another table.
    pub fn table(&self) -> &Table {
        &self.table
    }
}

/// One time limit shared by several calls, from when it was made
/// ([`Engine::span`]): all `place` calls of one keypad frame share
/// [`Limits::place`]. [`Engine::call`] makes one for each call.
#[derive(Clone, Copy, Debug)]
pub struct Span {
    start: Instant,
    limit: Duration,
}

impl Span {
    /// Whether the time is up.
    pub fn is_over(&self) -> bool {
        self.start.elapsed() >= self.limit
    }
}

impl Engine {
    /// A sandboxed engine with no globals beyond the libraries.
    pub fn new(limits: Limits) -> Result<Engine, ThemeError> {
        Engine::with_globals(limits, |_| Ok(()))
    }

    /// A sandboxed engine, with the API layer's globals: `register` runs
    /// after `require` and the rest are removed and `print` replaced, and
    /// before the sandbox closes. Tables it puts in the globals are frozen.
    pub fn with_globals(
        limits: Limits,
        register: impl FnOnce(&Lua) -> mlua::Result<()>,
    ) -> Result<Engine, ThemeError> {
        let lua =
            Lua::new_with(libs(), LuaOptions::default()).map_err(|e| classify(&e, &limits))?;
        let shared = Rc::new(Shared {
            deadline: Cell::new(None),
            timed_out: Cell::new(false),
            dead: RefCell::new(None),
            print_nesting: Cell::new(0),
            log: RefCell::new(LogBuf {
                lines: VecDeque::new(),
                bytes: 0,
                dropped: 0,
                max_lines: limits.log_lines,
                max_bytes: limits.log_bytes,
            }),
        });
        let engine = Engine {
            lua,
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            limits,
            shared,
            host_bytes: Cell::new(0),
        };
        engine
            .set_up(register)
            .map_err(|e| classify(&e, &engine.limits))?;
        Ok(engine)
    }

    fn set_up(&self, register: impl FnOnce(&Lua) -> mlua::Result<()>) -> mlua::Result<()> {
        let lua = &self.lua;
        // Name the host's category first, so that it is number 1, and go
        // back to the theme's.
        lua.set_memory_category(HOST_CATEGORY)?;
        lua.set_memory_category("main")?;
        lua.set_memory_limit(self.limits.memory)?;
        let globals = lua.globals();
        for name in REMOVED {
            globals.raw_set(name, Value::Nil)?;
        }
        globals.raw_set("print", self.print_function()?)?;
        register(lua)?;
        freeze_reachable(&globals)?;
        lua.sandbox(true)?;
        let shared = Rc::clone(&self.shared);
        lua.set_interrupt(move |_| {
            if let Some(deadline) = shared.deadline.get()
                && Instant::now() >= deadline
            {
                shared.timed_out.set(true);
                return Err(mlua::Error::runtime(TIMEOUT_MESSAGE));
            }
            Ok(VmState::Continue)
        });
        Ok(())
    }

    /// `print`: its arguments, as `tostring` writes them, tab-separated,
    /// one line of the log, cut short at [`Limits::log_bytes`].
    fn print_function(&self) -> mlua::Result<Function> {
        let shared = Rc::clone(&self.shared);
        let tostring: Function = self.lua.globals().raw_get("tostring")?;
        let max = self.limits.log_bytes;
        self.lua.create_function(move |_, args: MultiValue| {
            let nesting = shared.print_nesting.get();
            if nesting >= PRINT_NESTING {
                return Err(mlua::Error::runtime("print called from print too deeply"));
            }
            shared.print_nesting.set(nesting + 1);
            let line = print_line(&tostring, args, max);
            shared.print_nesting.set(nesting);
            shared.log.borrow_mut().push(line?);
            Ok(())
        })
    }

    /// Runs the source's main chunk, under [`Limits::load`], and converts
    /// what it returns. `name` names the chunk in messages: `@path` for a
    /// file. Precompiled bytecode is refused.
    pub fn load(&self, name: &str, source: impl AsRef<[u8]>) -> Result<ThemeValue, ThemeError> {
        let source = source.as_ref();
        if source.len() > self.limits.source_bytes {
            return Err(ThemeError::Limit(format!(
                "a source of {} bytes (the cap is {})",
                source.len(),
                self.limits.source_bytes
            )));
        }
        let chunk = self.lua.load(source).set_name(name);
        if chunk.mode() == ChunkMode::Binary {
            return Err(ThemeError::Unsupported(BYTECODE.into()));
        }
        self.alive()?;
        let value = self
            .guarded(&self.span(Budget::Load), || {
                chunk
                    .set_mode(ChunkMode::Text)
                    .into_function()?
                    .call::<Value>(())
            })
            .and_then(|v| self.convert(v));
        // A load makes most of a theme's garbage, and Luau doesn't collect
        // when an allocation fails: start the hooks with a clean heap.
        let _ = self.lua.gc_collect();
        value
    }

    /// Calls a hook under `budget`'s deadline and converts its first return
    /// value. A failure counts against the hook (conversion errors too):
    /// after [`Limits::strikes`] of them it is disabled, and at once if the
    /// call ran more than four times its limit (one library call Luau
    /// couldn't interrupt). A fatal error ([`ThemeError::is_fatal`]) stops
    /// the whole engine.
    pub fn call(
        &self,
        hook: &FunctionHandle,
        budget: Budget,
        args: impl IntoLuaMulti,
    ) -> Result<ThemeValue, ThemeError> {
        self.call_in(hook, &self.span(budget), args)
    }

    /// A time limit for several calls ([`Engine::call_in`]), from now.
    pub fn span(&self, budget: Budget) -> Span {
        Span {
            start: Instant::now(),
            limit: budget.duration(&self.limits),
        }
    }

    /// [`Engine::call`] within a span shared with other calls. Once the
    /// span is over, a call fails with [`ThemeError::Timeout`] without
    /// running and without a strike (the host should stop calling).
    pub fn call_in(
        &self,
        hook: &FunctionHandle,
        span: &Span,
        args: impl IntoLuaMulti,
    ) -> Result<ThemeValue, ThemeError> {
        self.alive()?;
        let h = &hook.0;
        if h.engine != self.id {
            return Err(ThemeError::Unsupported(
                "a function from another theme engine".into(),
            ));
        }
        if h.disabled.get() {
            return Err(ThemeError::Disabled(h.strikes.get()));
        }
        if span.is_over() {
            return Err(ThemeError::Timeout(span.limit));
        }
        let result = self
            .guarded(span, || {
                let f: Function = self.lua.registry_value(&h.key)?;
                f.call::<Value>(args)
            })
            .and_then(|v| self.convert(v));
        if result.is_err() {
            let strikes = h.strikes.get().saturating_add(1);
            h.strikes.set(strikes);
            let overran = span.start.elapsed() > span.limit.saturating_mul(4);
            if strikes >= self.limits.strikes || overran {
                h.disabled.set(true);
            }
        }
        result
    }

    /// Converts a Luau value (raw reads: no script code runs).
    pub fn convert(&self, value: Value) -> Result<ThemeValue, ThemeError> {
        self.alive()?;
        let r = self.shielded(|| Ok(crate::convert::convert(self, value)));
        self.fatal(r.and_then(|r| r))
    }

    /// Whether a fatal error ([`ThemeError::is_fatal`]) stopped the engine:
    /// the host drops it (and the theme) then. Everything fails after, with
    /// that error.
    pub fn is_dead(&self) -> bool {
        self.shared.dead.borrow().is_some()
    }

    fn alive(&self) -> Result<(), ThemeError> {
        match &*self.shared.dead.borrow() {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }

    /// Stops the engine if `r` is a fatal error.
    fn fatal<T>(&self, r: Result<T, ThemeError>) -> Result<T, ThemeError> {
        if let Err(e) = &r
            && e.is_fatal()
        {
            self.shared
                .dead
                .borrow_mut()
                .get_or_insert_with(|| e.clone());
        }
        r
    }

    /// A [`HostTable`] with room for `array` entries and `fields` named
    /// ones. It is the host's: outside the theme's heap cap, within
    /// [`Limits::host_memory`].
    pub fn host_table(&self, array: usize, fields: usize) -> Result<HostTable, ThemeError> {
        self.alive()?;
        let table = self.as_host(|| self.lua.create_table_with_capacity(array, fields))?;
        table.set_readonly(true);
        Ok(HostTable {
            engine: self.id,
            table,
        })
    }

    /// Refills a [`HostTable`]: `fill` gets it writable (use raw access,
    /// `raw_set` and `raw_seti`, which run no metamethods), and it is
    /// read-only again after.
    ///
    /// What the host stores is the host's to bound, not the theme's: the
    /// heap cap is lifted meanwhile (with no cap mlua also skips the
    /// protected call it otherwise makes around every store, which made a
    /// 1200-store refill cost 0.3 ms), and what is allocated is counted in
    /// the host's own memory category, which the theme's cap leaves out
    /// ([`Engine::host_memory`]). So `fill` must not run script code. A
    /// refill that leaves the host's tables over [`Limits::host_memory`] is
    /// a [`ThemeError::Limit`]: not fatal, and not the theme's doing.
    pub fn fill<R>(
        &self,
        host: &HostTable,
        fill: impl FnOnce(&Table) -> mlua::Result<R>,
    ) -> Result<R, ThemeError> {
        self.alive()?;
        if host.engine != self.id {
            return Err(ThemeError::Unsupported(
                "a host table from another theme engine".into(),
            ));
        }
        struct Lock<'a>(&'a Table);
        impl Drop for Lock<'_> {
            fn drop(&mut self) {
                self.0.set_readonly(true);
            }
        }
        host.table.set_readonly(false);
        let _lock = Lock(&host.table);
        self.as_host(|| fill(&host.table))
    }

    /// Runs `f`, which allocates for the host and runs no script code:
    /// with the heap cap lifted, in the host's memory category. The cap is
    /// back after, moved by what the host now holds.
    fn as_host<R>(&self, f: impl FnOnce() -> mlua::Result<R>) -> Result<R, ThemeError> {
        struct Restore<'a>(&'a Engine);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                let _ = self.0.lua.set_memory_category("main");
                self.0.measure_and_cap();
            }
        }
        let classify = |e: mlua::Error| classify(&e, &self.limits);
        self.lua
            .set_memory_category(HOST_CATEGORY)
            .map_err(classify)?;
        let restore = Restore(self);
        self.lua.set_memory_limit(0).map_err(classify)?;
        let r = self.fatal(self.shielded(f));
        drop(restore);
        if self.host_bytes.get() > self.limits.host_memory {
            // Host tables that were dropped count until they are collected.
            let _ = self.lua.gc_collect();
            self.host_memory();
        }
        let held = self.host_bytes.get();
        if held > self.limits.host_memory {
            return Err(ThemeError::Limit(format!(
                "host tables of {held} bytes (the allowance is {})",
                self.limits.host_memory
            )));
        }
        r
    }

    /// Bytes of the Luau heap the host's own tables hold now
    /// ([`Engine::host_table`], [`Engine::fill`]): live ones, as the
    /// collector counts them.
    pub fn host_memory(&self) -> usize {
        if self.lua.set_memory_limit(0).is_ok() {
            self.measure_and_cap();
        }
        self.host_bytes.get()
    }

    /// With the heap cap lifted (so that reading can't fail for memory):
    /// reads what the host's tables hold, and sets the cap for script code.
    /// That is the theme's [`Limits::memory`], and on top of it what the
    /// host holds (at most [`Limits::host_memory`]): host work never takes
    /// from the theme's room, and the theme never gets the host's. (Host
    /// tables collected since then leave the cap that much too high until
    /// the next refill: never by more than the allowance.)
    fn measure_and_cap(&self) {
        let mut bytes = self.host_bytes.get();
        // SAFETY: `lua_totalbytes` only reads a counter of the state it is
        // given, which `exec_raw` hands over locked and valid; nothing is
        // pushed or popped.
        let _: mlua::Result<()> = unsafe {
            self.lua.exec_raw((), |state| {
                bytes = mlua::ffi::lua_totalbytes(state, HOST_CATEGORY_ID);
            })
        };
        self.host_bytes.set(bytes);
        let host = bytes.min(self.limits.host_memory);
        let _ = self
            .lua
            .set_memory_limit(self.limits.memory.saturating_add(host));
    }

    /// What scripts printed since the last call; the log is then empty.
    pub fn take_log(&self) -> Log {
        let mut log = self.shared.log.borrow_mut();
        log.bytes = 0;
        Log {
            lines: log.lines.drain(..).collect(),
            dropped: std::mem::take(&mut log.dropped),
        }
    }

    /// The Luau state, for building arguments and the API layer's tables.
    /// Script functions must only be called through [`Engine::call`], which
    /// gives them a deadline.
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Bytes in the Luau heap, the theme's and the host's
    /// ([`Engine::host_memory`]) together.
    pub fn used_memory(&self) -> usize {
        self.lua.used_memory()
    }

    /// A handle for `f`, in this engine's registry.
    pub(crate) fn handle(&self, f: Function) -> mlua::Result<FunctionHandle> {
        let label = label(&f);
        let key = self.lua.create_registry_value(f)?;
        Ok(FunctionHandle(Rc::new(Hook {
            engine: self.id,
            key,
            label,
            strikes: Cell::new(0),
            disabled: Cell::new(false),
        })))
    }

    /// Runs `f`, which enters the VM, under the span's deadline (or an
    /// earlier one already running), and maps what goes wrong.
    fn guarded<T>(
        &self,
        span: &Span,
        f: impl FnOnce() -> mlua::Result<T>,
    ) -> Result<T, ThemeError> {
        let entry = Entry::new(&self.shared, span.start.checked_add(span.limit));
        let result = self.shielded(f);
        let timed_out = entry.timed_out();
        drop(entry);
        self.fatal(match result {
            // A fatal error stands, whatever the clock says.
            Err(e) if e.is_fatal() => Err(e),
            // However the script ended: it caught the interrupt's error,
            // or one library call ran past the deadline uninterrupted.
            _ if timed_out => Err(ThemeError::Timeout(span.limit)),
            other => other,
        })
    }

    /// Runs `f` with a backstop for panics inside mlua, which are fatal.
    fn shielded<T>(&self, f: impl FnOnce() -> mlua::Result<T>) -> Result<T, ThemeError> {
        match catch_unwind(AssertUnwindSafe(f)) {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(classify(&e, &self.limits)),
            Err(panic) => {
                let what = panic
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_default();
                Err(ThemeError::Internal(cap(&what, self.limits.message_bytes)))
            }
        }
    }
}

/// The deadline of one entry into the VM. A nested entry (a host function
/// calling back in) keeps the earlier of its own and the running one; the
/// running one is restored when it ends, panics included.
struct Entry<'a> {
    shared: &'a Shared,
    deadline: Option<Instant>,
    outer: Option<Instant>,
    outer_timed_out: bool,
}

impl<'a> Entry<'a> {
    /// `end`: `None` for no deadline of its own.
    fn new(shared: &'a Shared, end: Option<Instant>) -> Self {
        let outer = shared.deadline.get();
        let deadline = match (outer, end) {
            (Some(o), Some(d)) => Some(o.min(d)),
            (Some(o), None) => Some(o),
            (None, d) => d,
        };
        shared.deadline.set(deadline);
        Entry {
            shared,
            deadline,
            outer,
            outer_timed_out: shared.timed_out.replace(false),
        }
    }

    fn timed_out(&self) -> bool {
        self.shared.timed_out.get() || self.deadline.is_some_and(|d| Instant::now() > d)
    }
}

impl Drop for Entry<'_> {
    fn drop(&mut self) {
        self.shared.deadline.set(self.outer);
        let inner = self.shared.timed_out.get();
        self.shared.timed_out.set(self.outer_timed_out || inner);
    }
}

struct LogBuf {
    lines: VecDeque<String>,
    bytes: usize,
    dropped: usize,
    max_lines: usize,
    max_bytes: usize,
}

impl LogBuf {
    fn push(&mut self, mut line: String) {
        if line.len() > self.max_bytes {
            line = cap(&line, self.max_bytes.saturating_sub(32));
        }
        while self.lines.len() >= self.max_lines.max(1)
            || (!self.lines.is_empty() && self.bytes + line.len() > self.max_bytes)
        {
            let Some(old) = self.lines.pop_front() else {
                break;
            };
            self.bytes -= old.len();
            self.dropped += 1;
        }
        if self.max_lines == 0 {
            self.dropped += 1;
            return;
        }
        self.bytes += line.len();
        self.lines.push_back(line);
    }
}

fn print_line(tostring: &Function, args: MultiValue, max: usize) -> mlua::Result<String> {
    let mut line = String::new();
    for (i, v) in args.into_iter().enumerate() {
        if line.len() >= max {
            break;
        }
        if i > 0 {
            line.push('\t');
        }
        let s = match v {
            Value::String(s) => s,
            // Runs `__tostring`, still under the call's deadline.
            v => tostring.call::<LuaString>(v)?,
        };
        let bytes = s.as_bytes();
        let room = max - line.len().min(max);
        line.push_str(&String::from_utf8_lossy(&bytes[..bytes.len().min(room)]));
    }
    if line.len() > max {
        line = cap(&line, max);
    }
    Ok(line)
}

/// Freezes every table reachable from `globals` (keys, values and
/// metatables), but `globals` itself, which the sandbox freezes.
fn freeze_reachable(globals: &Table) -> mlua::Result<()> {
    let mut seen = HashSet::from([globals.to_pointer()]);
    let mut stack = vec![globals.clone()];
    while let Some(t) = stack.pop() {
        let mut found = Vec::new();
        t.for_each::<Value, Value>(|k, v| {
            found.extend([k, v].into_iter().filter_map(|x| match x {
                Value::Table(t) => Some(t),
                _ => None,
            }));
            Ok(())
        })?;
        found.extend(t.metatable());
        for f in found {
            if seen.insert(f.to_pointer()) {
                stack.push(f);
            }
        }
        if t.to_pointer() != globals.to_pointer() {
            t.set_readonly(true);
        }
    }
    Ok(())
}

/// `name (source:line)`, for messages.
fn label(f: &Function) -> String {
    let info = f.info();
    let place = match (info.short_src, info.line_defined) {
        (Some(src), Some(line)) => format!("{src}:{line}"),
        (Some(src), None) => src,
        _ => "?".into(),
    };
    let label = match info.name {
        Some(name) if !name.is_empty() => format!("{name} ({place})"),
        _ => place,
    };
    cap(&label, 200)
}
