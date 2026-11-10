use std::fmt;
use std::time::Duration;

use crate::Limits;

/// Why a theme's load or hook call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeError {
    /// The source doesn't compile.
    Syntax(String),
    /// The script raised an error, or a library call failed. The traceback
    /// may be empty.
    Runtime { message: String, traceback: String },
    /// The call ran past its deadline.
    Timeout(Duration),
    /// The Luau heap reached its cap (in bytes). Fatal: the engine is
    /// stopped, and the host unloads the theme.
    Memory(usize),
    /// A host cap was reached: the source's size, or the size of the value
    /// tree a script returned.
    Limit(String),
    /// Something the host doesn't take: precompiled bytecode, or a value
    /// with no host form (a mixed or cyclic table, NaN, a coroutine...).
    Unsupported(String),
    /// The hook failed this many times (or once, running more than four
    /// times its limit) and is no longer called.
    Disabled(u32),
    /// A bug in GMNB, not the theme: a panic inside mlua, caught. Fatal.
    Internal(String),
}

impl ThemeError {
    /// Whether the engine is stopped by it ([`Engine::is_dead`]): the host
    /// drops the engine and unloads the theme.
    ///
    /// [`Engine::is_dead`]: crate::Engine::is_dead
    pub fn is_fatal(&self) -> bool {
        matches!(self, ThemeError::Memory(_) | ThemeError::Internal(_))
    }
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeError::Syntax(m) => write!(f, "syntax error: {m}"),
            ThemeError::Runtime { message, traceback } if traceback.is_empty() => {
                write!(f, "error: {message}")
            }
            ThemeError::Runtime { message, traceback } => {
                write!(f, "error: {message}\n{traceback}")
            }
            ThemeError::Timeout(d) => write!(f, "ran past its {} ms deadline", d.as_millis()),
            ThemeError::Memory(cap) => write!(f, "ran out of memory (the cap is {cap} bytes)"),
            ThemeError::Limit(m) => write!(f, "too big: {m}"),
            ThemeError::Unsupported(m) => write!(f, "not supported: {m}"),
            ThemeError::Disabled(n) => write!(f, "disabled after failing {n} times"),
            ThemeError::Internal(m) => write!(f, "internal error (not the theme's): {m}"),
        }
    }
}

impl std::error::Error for ThemeError {}

/// The error the interrupt raises; the engine reports a timeout by its own
/// flag, never by this text (a script can raise the same string).
pub(crate) const TIMEOUT_MESSAGE: &str = "the theme script ran past its deadline";

/// A theme error for an mlua error raised while running script code.
pub(crate) fn classify(err: &mlua::Error, limits: &Limits) -> ThemeError {
    let mut traceback = None;
    let mut e = err;
    loop {
        match e {
            mlua::Error::CallbackError {
                traceback: tb,
                cause,
            } => {
                traceback.get_or_insert(tb.as_str());
                e = cause;
            }
            mlua::Error::WithContext { cause, .. } => e = cause,
            _ => break,
        }
    }
    match e {
        mlua::Error::MemoryError(_) => ThemeError::Memory(limits.memory),
        mlua::Error::SyntaxError { message, .. } => {
            if message.contains("attempt to load a binary chunk") {
                ThemeError::Unsupported(BYTECODE.into())
            } else {
                ThemeError::Syntax(cap(message, limits.message_bytes))
            }
        }
        mlua::Error::RuntimeError(text) => {
            // mlua's message handler appends Luau's traceback.
            let (message, tb) = match text.find("\nstack traceback:") {
                Some(at) => (&text[..at], text[at + 1..].trim_end()),
                None => (text.as_str(), traceback.unwrap_or("")),
            };
            runtime(message, tb, limits)
        }
        other => runtime(&other.to_string(), traceback.unwrap_or(""), limits),
    }
}

pub(crate) const BYTECODE: &str = "precompiled Luau bytecode; themes are loaded from source";

pub(crate) fn runtime(message: &str, traceback: &str, limits: &Limits) -> ThemeError {
    ThemeError::Runtime {
        message: cap(message, limits.message_bytes),
        traceback: cap(traceback, limits.traceback_bytes),
    }
}

/// `s` cut to at most `max` bytes, at a character boundary, saying how much
/// was cut.
pub(crate) fn cap(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} more bytes)", &s[..end], s.len() - end)
}

#[cfg(test)]
mod tests {
    use super::cap;

    #[test]
    fn cap_cuts_at_a_character_boundary() {
        assert_eq!(cap("abc", 3), "abc");
        assert_eq!(cap("abcd", 3), "abc… (1 more bytes)");
        // "é" is two bytes: a cut inside it backs off to before it.
        assert_eq!(cap("aé", 2), "a… (2 more bytes)");
        assert_eq!(cap("é", 0), "… (2 more bytes)");
    }
}
