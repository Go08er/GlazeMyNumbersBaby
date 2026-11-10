//! The engine under GMNB's theme scripts: one sandboxed, bounded Luau VM per
//! theme, the conversion of what a script returns into a host value tree,
//! and handles to script functions (hooks) that the host calls with a
//! deadline. The theme API itself (what is themable, the hooks, the draw
//! calls; docs/theming.md) is built on top of this, through
//! [`Engine::with_globals`].
//!
//! # The sandbox
//!
//! An [`Engine`] is set up in this order:
//!
//! 1. Luau's standard libraries are loaded, except `debug` (the theme
//!    contract has none) and `integer` (still behind a Luau flag):
//!    `coroutine`, `table`, `os`, `string`, `utf8`, `bit32`, `math`,
//!    `buffer` and `vector`. The heap cap is set.
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
//! `difftime` and `time`. Nothing in it reads or writes files, the network
//! or the environment. (A test pins this list.)
//!
//! # Limits
//!
//! Every entry into the VM, the load and each hook call, runs under a
//! deadline (an interrupt checks it at every loop and call). The limit is
//! of processor time, what the thread has run, so a machine busy with
//! other things doesn't fail a theme that did nothing slow. The entry stays
//! under its deadline until the host call returns: a script that catches the timeout error
//! with `pcall` is stopped at its next loop or call, coroutines included,
//! and a call that returns after its deadline (one long library call, which
//! Luau can't interrupt) is a [`ThemeError::Timeout`] too. Several calls
//! can share one limit (a [`Span`]).
//!
//! Each [`FunctionHandle`] counts its failures and is disabled after
//! [`Limits::strikes`] of them, or at once after a call that ran more than
//! four times its limit. The Luau heap has a hard cap ([`Limits::memory`]);
//! reaching it is fatal ([`ThemeError::is_fatal`]): the engine stops, and
//! the host unloads the theme. A memory error a script catches itself with
//! `pcall` never reaches the host: the cap held, and the theme goes on.
//! The tables the host builds for a call ([`HostTable`]) are counted apart,
//! in a Luau memory category of their own, and have their own allowance
//! ([`Limits::host_memory`]) on top of the cap: host work never pushes a
//! theme into a memory error, and the allowance is never the theme's to
//! use.
//!
//! Nothing a script does panics or aborts the process: errors come back as
//! a [`ThemeError`], with messages and tracebacks capped in length.
//!
//! Host functions registered with [`Engine::with_globals`] run outside the
//! interrupt's reach: they must bound their own work, and must not call
//! script functions except through [`Engine::call`].

pub mod check;
mod convert;
pub mod css;
pub mod draw;
mod engine;
mod error;
pub mod events;
pub mod field;
pub mod layer;
pub mod library;
pub mod path;
pub mod picture;
pub mod sched;
pub mod schema;
pub mod states;
pub mod style;
pub mod theme;
pub mod tokens;
pub mod types;
mod value;
pub mod writer;

use std::time::Duration;

pub use check::{Note, Notes};
pub use draw::{DisplayList, DrawLimits, Op, TextMeasure};
pub use engine::{Engine, HostTable, Log, Span};
pub use error::ThemeError;
pub use style::{Resolved, Style, resolve};
pub use tokens::{Ctx, OptionValue, Tokens};
pub use value::{FunctionHandle, ThemeValue};
pub use writer::{Update, Writer};

/// The pictures' own crate: decoders, the animation player, the field
/// pass. The app turns what it gives into textures.
pub use gmnb_media as media;
/// The Luau binding, for the API layer's globals and argument tables.
pub use mlua;

/// What a theme engine may use; the defaults are docs/theming.md's.
/// Every cap is a hard limit; going past one is an error, never a silent
/// cut (only log lines are cut short).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The Luau heap, in bytes: what the theme's own code allocates.
    pub memory: usize,
    /// What the host's own tables ([`HostTable`]: a call's `ctx`, a
    /// frame) may hold in the same heap, on top of [`Limits::memory`], in
    /// bytes.
    pub host_memory: usize,
    /// Running the theme file's main chunk.
    pub load: Duration,
    /// A `style` call.
    pub style: Duration,
    /// One `draw` call.
    pub frame: Duration,
    /// One `preview` call.
    pub preview: Duration,
    /// One `visible` call.
    pub visible: Duration,
    /// One option action's `run`.
    pub action: Duration,
    /// All `place` calls of one keypad frame together (a [`Span`]).
    pub place: Duration,
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
    /// One string (or map key) in a converted tree, in bytes: longer ones
    /// are refused.
    pub string_bytes: usize,
    /// All strings and map keys of a converted tree together, in bytes.
    pub total_string_bytes: usize,
    /// The draw API's caps.
    pub draw: DrawLimits,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            memory: 8 << 20,
            host_memory: 1 << 20,
            load: Duration::from_millis(250),
            style: Duration::from_millis(50),
            frame: Duration::from_millis(8),
            preview: Duration::from_millis(10),
            visible: Duration::from_millis(10),
            action: Duration::from_millis(10),
            place: Duration::from_millis(2),
            strikes: 3,
            source_bytes: 512 << 10,
            log_lines: 200,
            log_bytes: 16 << 10,
            message_bytes: 1 << 10,
            traceback_bytes: 2 << 10,
            depth: 32,
            nodes: 20_000,
            string_bytes: 4 << 10,
            total_string_bytes: 1 << 20,
            draw: DrawLimits::default(),
        }
    }
}

/// Which time limit a call (or a [`Span`] of calls) runs under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget {
    /// [`Limits::load`].
    Load,
    /// [`Limits::style`].
    Style,
    /// [`Limits::frame`].
    Frame,
    /// [`Limits::preview`].
    Preview,
    /// [`Limits::visible`].
    Visible,
    /// [`Limits::action`].
    Action,
    /// [`Limits::place`]: meant for a [`Span`].
    Place,
    /// Any other.
    Custom(Duration),
}

impl Budget {
    fn duration(self, limits: &Limits) -> Duration {
        match self {
            Budget::Load => limits.load,
            Budget::Style => limits.style,
            Budget::Frame => limits.frame,
            Budget::Preview => limits.preview,
            Budget::Visible => limits.visible,
            Budget::Action => limits.action,
            Budget::Place => limits.place,
            Budget::Custom(d) => d,
        }
    }
}
