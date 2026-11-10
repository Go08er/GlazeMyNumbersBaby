use std::cell::Cell;
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

/// A value a script handed the host, converted (by [`Engine::convert`],
/// [`Engine::load`] and [`Engine::call`]) under the caps of [`Limits`]:
///
/// - Luau numbers are doubles. One that is a whole number in `i64`'s range
///   is an [`Int`](ThemeValue::Int) (mlua's rule, so `-0.0` stays a
///   `Number`); any other finite one a [`Number`](ThemeValue::Number). NaN
///   and the infinities are an error ([`ThemeError::Unsupported`]), not
///   `Nil`, so a theme that divides by zero hears about it.
/// - A table is read raw: no metamethod runs and a metatable is ignored. One
///   whose keys are exactly `1..=n` is an [`Array`](ThemeValue::Array); one
///   with only string keys a [`Map`](ThemeValue::Map), in key order. The
///   empty table is an empty `Map`. Anything else is an error: mixed
///   tables (array entries and string keys), arrays with holes or not
///   starting at 1, other kinds of keys, and a table inside itself (a
///   cycle). The same table reached twice by different paths (not inside
///   itself) is converted twice.
/// - Strings and map keys must be UTF-8.
/// - A function is a [`FunctionHandle`]; the same function reached twice in
///   one conversion is one handle.
/// - Anything else (vectors, buffers, coroutines, userdata) is an error.
///
/// [`Engine::convert`]: crate::Engine::convert
/// [`Engine::load`]: crate::Engine::load
/// [`Engine::call`]: crate::Engine::call
/// [`Limits`]: crate::Limits
/// [`ThemeError::Unsupported`]: crate::ThemeError::Unsupported
#[derive(Clone, Debug)]
pub enum ThemeValue {
    Nil,
    Bool(bool),
    Int(i64),
    /// Finite.
    Number(f64),
    String(String),
    Array(Vec<ThemeValue>),
    Map(BTreeMap<String, ThemeValue>),
    Function(FunctionHandle),
}

impl ThemeValue {
    /// A number, whole or not.
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            ThemeValue::Int(i) => Some(i as f64),
            ThemeValue::Number(n) => Some(n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            ThemeValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_function(&self) -> Option<&FunctionHandle> {
        match self {
            ThemeValue::Function(f) => Some(f),
            _ => None,
        }
    }

    /// A map's entry.
    pub fn get(&self, key: &str) -> Option<&ThemeValue> {
        match self {
            ThemeValue::Map(m) => m.get(key),
            _ => None,
        }
    }

    /// Luau's name for the kind of value.
    pub fn type_name(&self) -> &'static str {
        match self {
            ThemeValue::Nil => "nil",
            ThemeValue::Bool(_) => "boolean",
            ThemeValue::Int(_) | ThemeValue::Number(_) => "number",
            ThemeValue::String(_) => "string",
            ThemeValue::Array(_) | ThemeValue::Map(_) => "table",
            ThemeValue::Function(_) => "function",
        }
    }
}

/// Functions are equal when they are the same handle; everything else by
/// value.
impl PartialEq for ThemeValue {
    fn eq(&self, other: &Self) -> bool {
        use ThemeValue::*;
        match (self, other) {
            (Nil, Nil) => true,
            (Bool(a), Bool(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Number(a), Number(b)) => a == b,
            (String(a), String(b)) => a == b,
            (Array(a), Array(b)) => a == b,
            (Map(a), Map(b)) => a == b,
            (Function(a), Function(b)) => Rc::ptr_eq(&a.0, &b.0),
            _ => false,
        }
    }
}

/// Written as a Luau literal, one entry a line (what `gmnb --check-theme`
/// prints).
impl fmt::Display for ThemeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write(f, 0)
    }
}

impl ThemeValue {
    fn write(&self, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        let pad = |f: &mut fmt::Formatter<'_>, n: usize| write!(f, "{:width$}", "", width = n * 2);
        match self {
            ThemeValue::Nil => f.write_str("nil"),
            ThemeValue::Bool(b) => write!(f, "{b}"),
            ThemeValue::Int(i) => write!(f, "{i}"),
            ThemeValue::Number(n) => write!(f, "{n:?}"),
            ThemeValue::String(s) => write!(f, "{s:?}"),
            ThemeValue::Function(h) => write!(f, "<function {}>", h.label()),
            ThemeValue::Array(a) if a.is_empty() => f.write_str("{}"),
            ThemeValue::Map(m) if m.is_empty() => f.write_str("{}"),
            ThemeValue::Array(a) => {
                f.write_str("{\n")?;
                for v in a {
                    pad(f, indent + 1)?;
                    v.write(f, indent + 1)?;
                    f.write_str(",\n")?;
                }
                pad(f, indent)?;
                f.write_str("}")
            }
            ThemeValue::Map(m) => {
                f.write_str("{\n")?;
                for (k, v) in m {
                    pad(f, indent + 1)?;
                    if is_name(k) {
                        write!(f, "{k} = ")?;
                    } else {
                        write!(f, "[{k:?}] = ")?;
                    }
                    v.write(f, indent + 1)?;
                    f.write_str(",\n")?;
                }
                pad(f, indent)?;
                f.write_str("}")
            }
        }
    }
}

/// Whether `s` can be written as a bare Luau field name.
fn is_name(s: &str) -> bool {
    const RESERVED: &[&str] = &[
        "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in",
        "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
    ];
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        && !RESERVED.contains(&s)
}

/// A script function the host can call ([`Engine::call`]) under a
/// deadline: a hook. It lives in its engine's registry and only that engine
/// calls it (another refuses it); it may outlive the engine, harmlessly.
///
/// Each handle counts its failed calls; once there are
/// [`Limits::strikes`] of them it is disabled, and calling it fails at once
/// with [`ThemeError::Disabled`]. Clones share the count.
///
/// [`Engine::call`]: crate::Engine::call
/// [`Limits::strikes`]: crate::Limits::strikes
/// [`ThemeError::Disabled`]: crate::ThemeError::Disabled
#[derive(Clone)]
pub struct FunctionHandle(pub(crate) Rc<Hook>);

pub(crate) struct Hook {
    pub(crate) engine: u64,
    pub(crate) key: mlua::RegistryKey,
    pub(crate) label: String,
    pub(crate) strikes: Cell<u32>,
    pub(crate) disabled: Cell<bool>,
}

impl FunctionHandle {
    /// Where the function was defined, for messages: its name if Luau knows
    /// it, and `source:line`.
    pub fn label(&self) -> &str {
        &self.0.label
    }

    /// Failed calls so far.
    pub fn strikes(&self) -> u32 {
        self.0.strikes.get()
    }

    /// Whether it failed too often to be called again.
    pub fn is_disabled(&self) -> bool {
        self.0.disabled.get()
    }
}

impl fmt::Debug for FunctionHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<function {}>", self.label())
    }
}
