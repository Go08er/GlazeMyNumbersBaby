//! Luau values into [`ThemeValue`]s: raw reads only (no script code runs, so
//! no deadline is needed), every cap checked before the work it bounds.

use std::collections::{BTreeMap, HashMap};
use std::ffi::c_void;

use mlua::{Function, LuaString, Table, Value};

use crate::engine::Engine;
use crate::error::{cap, classify};
use crate::{FunctionHandle, Limits, ThemeError, ThemeValue};

pub(crate) fn convert(engine: &Engine, value: Value) -> Result<ThemeValue, ThemeError> {
    let mut c = Converter {
        engine,
        limits: engine.limits(),
        nodes: 1,
        string_bytes: 0,
        ancestors: Vec::new(),
        functions: HashMap::new(),
    };
    c.value(value, 0).map_err(|f| f.into_error(c.limits))
}

struct Converter<'a> {
    engine: &'a Engine,
    limits: &'a Limits,
    /// Values so far, counted as their tables are read (before descending).
    nodes: usize,
    string_bytes: usize,
    /// The tables being converted, outermost first: one met again is a cycle.
    ancestors: Vec<*const c_void>,
    /// Functions already given a handle in this conversion.
    functions: HashMap<*const c_void, FunctionHandle>,
}

impl Converter<'_> {
    fn value(&mut self, v: Value, depth: usize) -> Result<ThemeValue, Fail> {
        Ok(match v {
            Value::Nil => ThemeValue::Nil,
            Value::Boolean(b) => ThemeValue::Bool(b),
            Value::Integer(i) => ThemeValue::Int(i),
            Value::Number(n) if n.is_finite() => ThemeValue::Number(n),
            Value::Number(n) => {
                return Err(Fail::unsupported(format!(
                    "the number {n} (only finite numbers are taken)"
                )));
            }
            Value::String(s) => ThemeValue::String(self.string(&s)?),
            Value::Table(t) => self.table(&t, depth)?,
            Value::Function(f) => ThemeValue::Function(self.function(f)?),
            other => return Err(Fail::unsupported(format!("a {} value", other.type_name()))),
        })
    }

    fn string(&mut self, s: &LuaString) -> Result<String, Fail> {
        let bytes = s.as_bytes();
        if bytes.len() > self.limits.string_bytes {
            return Err(Fail::limit(format!(
                "a string of {} bytes (the cap is {})",
                bytes.len(),
                self.limits.string_bytes
            )));
        }
        self.string_bytes += bytes.len();
        if self.string_bytes > self.limits.total_string_bytes {
            return Err(Fail::limit(format!(
                "strings of more than {} bytes in all",
                self.limits.total_string_bytes
            )));
        }
        match std::str::from_utf8(&bytes) {
            Ok(s) => Ok(s.to_owned()),
            Err(_) => Err(Fail::unsupported("a string that isn't UTF-8".into())),
        }
    }

    fn function(&mut self, f: Function) -> Result<FunctionHandle, Fail> {
        let p = f.to_pointer();
        if let Some(h) = self.functions.get(&p) {
            return Ok(h.clone());
        }
        let h = self.engine.handle(f).map_err(Fail::Lua)?;
        self.functions.insert(p, h.clone());
        Ok(h)
    }

    fn table(&mut self, t: &Table, depth: usize) -> Result<ThemeValue, Fail> {
        if depth >= self.limits.depth {
            return Err(Fail::limit(format!(
                "tables nested more than {} deep",
                self.limits.depth
            )));
        }
        let p = t.to_pointer();
        if self.ancestors.contains(&p) {
            return Err(Fail::unsupported("a table inside itself (a cycle)".into()));
        }
        // Read the entries raw, stopping at the node cap.
        let room = self.limits.nodes.saturating_sub(self.nodes);
        let mut entries = Vec::new();
        let mut full = false;
        let read = t.for_each::<Value, Value>(|k, v| {
            if entries.len() >= room {
                full = true;
                return Err(mlua::Error::runtime("node cap"));
            }
            entries.push((k, v));
            Ok(())
        });
        if full {
            return Err(Fail::limit(format!(
                "more than {} values in all",
                self.limits.nodes
            )));
        }
        read.map_err(Fail::Lua)?;
        self.nodes += entries.len();
        if entries.is_empty() {
            return Ok(ThemeValue::Map(BTreeMap::new()));
        }

        let n = entries.len();
        let (mut strings, mut indices, mut stray) = (0, 0, 0);
        for (k, _) in &entries {
            match k {
                Value::String(_) => strings += 1,
                Value::Integer(i) if *i >= 1 && *i as u64 <= n as u64 => indices += 1,
                Value::Integer(_) => stray += 1,
                Value::Number(_) => {
                    return Err(Fail::unsupported(
                        "a table key that is a number but not a whole one".into(),
                    ));
                }
                other => {
                    return Err(Fail::unsupported(format!(
                        "a table key of type {}",
                        other.type_name()
                    )));
                }
            }
        }
        if strings > 0 && strings < n {
            return Err(Fail::unsupported(
                "a table with both array entries and string keys".into(),
            ));
        }
        if strings == 0 && (stray > 0 || indices != n) {
            return Err(Fail::unsupported(
                "an array whose keys aren't 1 to n (a hole, or a start other than 1)".into(),
            ));
        }

        self.ancestors.push(p);
        let out = if strings == n {
            self.map(entries, depth)
        } else {
            self.array(entries, depth)
        };
        self.ancestors.pop();
        out
    }

    fn map(&mut self, entries: Vec<(Value, Value)>, depth: usize) -> Result<ThemeValue, Fail> {
        let mut keyed = Vec::with_capacity(entries.len());
        for (k, v) in entries {
            let Value::String(k) = k else {
                return Err(Fail::unsupported("a table with mixed keys".into()));
            };
            keyed.push((self.string(&k)?, v));
        }
        // In key order, so that the first error reported doesn't depend on
        // the table's layout.
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        let mut map = BTreeMap::new();
        for (k, v) in keyed {
            match self.value(v, depth + 1) {
                Ok(v) => {
                    map.insert(k, v);
                }
                Err(f) => return Err(f.at(Seg::Key(k))),
            }
        }
        Ok(ThemeValue::Map(map))
    }

    fn array(
        &mut self,
        mut entries: Vec<(Value, Value)>,
        depth: usize,
    ) -> Result<ThemeValue, Fail> {
        entries.sort_by_key(|(k, _)| match k {
            Value::Integer(i) => *i,
            _ => 0,
        });
        let mut out = Vec::with_capacity(entries.len());
        for (i, (_, v)) in entries.into_iter().enumerate() {
            out.push(
                self.value(v, depth + 1)
                    .map_err(|f| f.at(Seg::Index(i + 1)))?,
            );
        }
        Ok(ThemeValue::Array(out))
    }
}

/// A conversion failure, with the path to it built on the way out.
enum Fail {
    Limit(String, Vec<Seg>),
    Unsupported(String, Vec<Seg>),
    /// Raised by mlua (an allocation failing at the heap cap).
    Lua(mlua::Error),
}

enum Seg {
    Key(String),
    Index(usize),
}

impl Fail {
    fn limit(m: String) -> Self {
        Fail::Limit(m, Vec::new())
    }

    fn unsupported(m: String) -> Self {
        Fail::Unsupported(m, Vec::new())
    }

    fn at(mut self, seg: Seg) -> Self {
        if let Fail::Limit(_, path) | Fail::Unsupported(_, path) = &mut self {
            path.push(seg);
        }
        self
    }

    fn into_error(self, limits: &Limits) -> ThemeError {
        let located = |m: String, path: Vec<Seg>| {
            let mut s = m;
            s.push_str(" at value");
            for seg in path.iter().rev() {
                match seg {
                    Seg::Key(k) if is_plain(k) => {
                        s.push('.');
                        s.push_str(k);
                    }
                    Seg::Key(k) => s.push_str(&format!("[{k:?}]")),
                    Seg::Index(i) => s.push_str(&format!("[{i}]")),
                }
            }
            cap(&s, limits.message_bytes)
        };
        match self {
            Fail::Limit(m, path) => ThemeError::Limit(located(m, path)),
            Fail::Unsupported(m, path) => ThemeError::Unsupported(located(m, path)),
            Fail::Lua(e) => classify(&e, limits),
        }
    }
}

fn is_plain(k: &str) -> bool {
    let mut c = k.chars();
    c.next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && c.all(|c| c == '_' || c.is_ascii_alphanumeric())
}
