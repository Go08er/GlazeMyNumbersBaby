//! The engine under GMNB's theme scripts: one sandboxed, bounded Luau VM per
//! theme, the conversion of what a script returns into a host value tree,
//! and handles to script functions (hooks) that the host calls with a
//! deadline. The theme API itself (what is themable, the hooks, the draw
//! calls) is built on top of this, through [`Engine::with_globals`].
//!
//! # The sandbox
//!
//! An [`Engine`] is set up in this order:
//!
//! 1. Luau's standard libraries are loaded, except `integer` (still behind a
//!    Luau flag): `coroutine`, `table`, `os`, `string`, `utf8`, `bit32`,
//!    `math`, `buffer`, `vector` and `debug`.
//! 2. What reaches outside the VM, or past this crate's checks, is removed:
//!    `require` (mlua's filesystem loader), `loadstring` (it would compile
//!    code outside the load deadline's source cap), `getfenv` and `setfenv`
//!    (they swap a function's globals, and turn off Luau's fast paths), and
//!    `newproxy` (userdata, which no theme needs and the host can't convert).
//! 3. `print` writes to a capped log the host reads ([`Engine::take_log`]),
//!    not to stdout.
//! 4. The API layer registers its globals ([`Engine::with_globals`]).
//! 5. Every table then reachable from the globals is frozen (read-only),
//!    with its metatable, and `Lua::sandbox(true)` freezes the globals
//!    themselves and gives scripts a writable global table of their own that
//!    reads through to them.
//!
//! What is left, besides the API layer's globals: `assert`, `error`,
//! `gcinfo`, `getmetatable`, `ipairs`, `next`, `pairs`, `pcall`, `print`,
//! `rawequal`, `rawget`, `rawlen`, `rawset`, `select`, `setmetatable`,
//! `tonumber`, `tostring`, `type`, `typeof`, `unpack`, `xpcall`, `_G`,
//! `_VERSION`, `collectgarbage` (only `"count"` once sandboxed: mlua's own),
//! and the libraries above. Luau's `os` has only `clock`, `date`,
//! `difftime` and `time`; its `debug` only `info` and `traceback`. Nothing
//! in it reads or writes files, the network or the environment.
//!
//! # Limits
//!
//! Every entry into the VM, the load and each hook call, runs under a
//! deadline (an interrupt checks it at every loop and call), and stays under
//! it until the host call returns: a script that catches the timeout error
//! with `pcall` is stopped at its next loop or call, coroutines included,
//! and a call that returns after its deadline (one long library call, which
//! Luau can't interrupt) is a [`ThemeError::Timeout`] too. The Luau heap has
//! a hard cap ([`Limits::memory`]). Each [`FunctionHandle`] counts its
//! failures and is disabled after [`Limits::strikes`] of them. Nothing a
//! script does panics or aborts the process: errors come back as a
//! [`ThemeError`], with messages and tracebacks capped in length.
//!
//! Host functions registered with [`Engine::with_globals`] run outside the
//! interrupt's reach: they must bound their own work, and must not call
//! script functions except through [`Engine::call`].

mod convert;
mod engine;
mod error;
mod value;

use std::time::Duration;

pub use engine::{Engine, HostTable, Log};
pub use error::ThemeError;
pub use value::{FunctionHandle, ThemeValue};

/// The Luau binding, for the API layer's globals and argument tables.
pub use mlua;

/// What a theme engine may use. Every cap is a hard limit; going past one is
/// an error, never a silent cut (only log lines are cut short).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The Luau heap, in bytes.
    pub memory: usize,
    /// Running the theme file's main chunk.
    pub load: Duration,
    /// A style hook: called when the look has to be worked out again.
    pub style: Duration,
    /// A frame hook: called up to once a frame.
    pub frame: Duration,
    /// Failures after which a hook is disabled.
    pub strikes: u32,
    /// Size of a chunk's source, in bytes (Luau's compiler can't be
    /// interrupted, so it only ever sees this much).
    pub source_bytes: usize,
    /// Lines kept in the log (the oldest go first).
    pub log_lines: usize,
    /// Bytes kept in the log; a longer line is cut short.
    pub log_bytes: usize,
    /// An error message, in bytes.
    pub message_bytes: usize,
    /// An error's traceback, in bytes.
    pub traceback_bytes: usize,
    /// Tables nested in a converted value.
    pub depth: usize,
    /// Values in a converted tree, all levels together.
    pub nodes: usize,
    /// One string (or map key) in a converted tree, in bytes.
    pub string_bytes: usize,
    /// All strings and map keys of a converted tree together, in bytes.
    pub total_string_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            memory: 8 << 20,
            load: Duration::from_millis(200),
            style: Duration::from_millis(50),
            frame: Duration::from_millis(8),
            strikes: 3,
            source_bytes: 1 << 20,
            log_lines: 200,
            log_bytes: 16 << 10,
            message_bytes: 1 << 10,
            traceback_bytes: 2 << 10,
            depth: 32,
            nodes: 20_000,
            string_bytes: 64 << 10,
            total_string_bytes: 1 << 20,
        }
    }
}

/// Which deadline a call runs under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget {
    /// [`Limits::load`].
    Load,
    /// [`Limits::style`].
    Style,
    /// [`Limits::frame`].
    Frame,
    /// Any other.
    Custom(Duration),
}

impl Budget {
    fn duration(self, limits: &Limits) -> Duration {
        match self {
            Budget::Load => limits.load,
            Budget::Style => limits.style,
            Budget::Frame => limits.frame,
            Budget::Custom(d) => d,
        }
    }
}
