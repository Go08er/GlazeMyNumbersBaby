//! A tiny synchronous D-Bus client: just enough to read desktop settings
//! from the XDG portal, ask systemd-timedated for the time zone and watch for
//! setting changes.
//!
//! It deliberately starts no threads and needs no libraries, so it can run
//! first thing in `main` (before anything else may read the environment) and
//! costs nothing in a twin that otherwise wouldn't link a D-Bus stack.
//! Wire format: <https://dbus.freedesktop.org/doc/dbus-specification.html>.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

/// Largest message accepted (header fields + body).
const MAX_MESSAGE: usize = 16 << 20;
/// Deepest container nesting accepted in a message.
const MAX_DEPTH: usize = 32;

const METHOD_CALL: u8 = 1;
const METHOD_RETURN: u8 = 2;
const ERROR: u8 = 3;
pub const SIGNAL: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    Session,
    System,
}

/// A decoded D-Bus value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Byte(u8),
    Bool(bool),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F64(f64),
    /// Strings, object paths and signatures.
    Str(String),
    Variant(Box<Value>),
    Struct(Vec<Value>),
    /// Arrays; dictionaries are arrays of two-element structs.
    Array(Vec<Value>),
    /// A unix fd index (fds themselves aren't received).
    Fd(u32),
}

impl Value {
    /// Look through any number of variant wrappers.
    pub fn unwrap_variant(&self) -> &Value {
        match self {
            Value::Variant(v) => v.unwrap_variant(),
            v => v,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self.unwrap_variant() {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self.unwrap_variant() {
            Value::U32(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self.unwrap_variant() {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Message {
    pub kind: u8,
    pub serial: u32,
    pub reply_serial: Option<u32>,
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error_name: Option<String>,
    pub body: Vec<Value>,
}

pub struct Connection {
    stream: UnixStream,
    serial: u32,
    /// Messages read while waiting for a reply (signals, mostly).
    queued: VecDeque<Message>,
}

fn err(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

impl Connection {
    /// Connect, authenticate and register with the bus. `timeout` bounds every
    /// read and write (use [`Connection::set_timeout`] to change it later).
    pub fn open(bus: Bus, timeout: Duration) -> io::Result<Connection> {
        let addr = match bus {
            Bus::Session => std::env::var("DBUS_SESSION_BUS_ADDRESS").ok(),
            Bus::System => std::env::var("DBUS_SYSTEM_BUS_ADDRESS").ok(),
        };
        let addr = match (addr, bus) {
            (Some(a), _) => a,
            (None, Bus::System) => "unix:path=/var/run/dbus/system_bus_socket".into(),
            (None, Bus::Session) => {
                let dir = std::env::var("XDG_RUNTIME_DIR").map_err(|_| err("no session bus"))?;
                format!("unix:path={dir}/bus")
            }
        };
        let stream = connect(&addr)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let mut conn = Connection {
            stream,
            serial: 0,
            queued: VecDeque::new(),
        };
        conn.authenticate()?;
        conn.call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "Hello",
            &[],
        )?;
        Ok(conn)
    }

    /// `None` blocks forever (for a signal-watching thread).
    pub fn set_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.stream.set_read_timeout(timeout)
    }

    fn authenticate(&mut self) -> io::Result<()> {
        use std::os::unix::fs::MetadataExt;
        let uid = std::fs::metadata("/proc/self")?.uid();
        let hex: String = uid
            .to_string()
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect();
        self.stream
            .write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes())?;
        let line = self.read_line()?;
        if !line.starts_with("OK ") {
            return Err(err(format!("D-Bus auth rejected: {line}")));
        }
        self.stream.write_all(b"BEGIN\r\n")
    }

    fn read_line(&mut self) -> io::Result<String> {
        let mut line = Vec::new();
        let mut b = [0u8];
        while !line.ends_with(b"\r\n") {
            self.stream.read_exact(&mut b)?;
            line.push(b[0]);
            if line.len() > 4096 {
                return Err(err("auth line too long"));
            }
        }
        line.truncate(line.len() - 2);
        Ok(String::from_utf8_lossy(&line).into_owned())
    }

    /// Call a method whose arguments are all strings and wait for its reply.
    pub fn call(
        &mut self,
        dest: &str,
        path: &str,
        interface: &str,
        member: &str,
        args: &[&str],
    ) -> io::Result<Vec<Value>> {
        self.serial = self.serial.wrapping_add(1).max(1);
        let serial = self.serial;
        let bytes = encode_call(serial, dest, path, interface, member, args);
        self.stream.write_all(&bytes)?;
        loop {
            let msg = read_message(&mut self.stream)?;
            match msg.kind {
                METHOD_RETURN if msg.reply_serial == Some(serial) => return Ok(msg.body),
                ERROR if msg.reply_serial == Some(serial) => {
                    let detail = msg.body.first().and_then(Value::as_str).unwrap_or("");
                    return Err(io::Error::other(format!(
                        "{}: {detail}",
                        msg.error_name.unwrap_or_default()
                    )));
                }
                _ => {
                    if self.queued.len() < 64 {
                        self.queued.push_back(msg);
                    }
                }
            }
        }
    }

    /// Subscribe to signals matching `rule` (D-Bus match rule syntax).
    pub fn add_match(&mut self, rule: &str) -> io::Result<()> {
        self.call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "AddMatch",
            &[rule],
        )
        .map(|_| ())
    }

    /// The next incoming message (blocks up to the read timeout).
    pub fn next_message(&mut self) -> io::Result<Message> {
        match self.queued.pop_front() {
            Some(m) => Ok(m),
            None => read_message(&mut self.stream),
        }
    }
}

/// Connect to the first usable `unix:` address in a D-Bus address list.
fn connect(addresses: &str) -> io::Result<UnixStream> {
    let mut last = err(format!("no usable D-Bus address in {addresses:?}"));
    for addr in addresses.split(';') {
        let Some(params) = addr.strip_prefix("unix:") else {
            continue;
        };
        for kv in params.split(',') {
            let Some((k, v)) = kv.split_once('=') else {
                continue;
            };
            let v = unescape(v);
            let attempt = match k {
                "path" => UnixStream::connect(&v),
                "abstract" => {
                    use std::os::linux::net::SocketAddrExt;
                    std::os::unix::net::SocketAddr::from_abstract_name(v.as_bytes())
                        .and_then(|a| UnixStream::connect_addr(&a))
                }
                _ => continue,
            };
            match attempt {
                Ok(s) => return Ok(s),
                Err(e) => last = e,
            }
        }
    }
    Err(last)
}

fn unescape(v: &str) -> String {
    let bytes = v.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(h) = v.get(i + 1..i + 3)
            && let Ok(b) = u8::from_str_radix(h, 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------------------------------------------------------------------------
// Marshalling
// ---------------------------------------------------------------------------

struct Writer(Vec<u8>);

impl Writer {
    fn align(&mut self, n: usize) {
        while !self.0.len().is_multiple_of(n) {
            self.0.push(0);
        }
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.align(4);
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
        self.0.push(0);
    }
    fn sig(&mut self, s: &str) {
        self.u8(s.len() as u8);
        self.0.extend_from_slice(s.as_bytes());
        self.0.push(0);
    }
    /// A header field: struct (code, variant).
    fn field(&mut self, code: u8, sig: &str, value: impl FnOnce(&mut Writer)) {
        self.align(8);
        self.u8(code);
        self.sig(sig);
        value(self);
    }
}

fn encode_call(
    serial: u32,
    dest: &str,
    path: &str,
    interface: &str,
    member: &str,
    args: &[&str],
) -> Vec<u8> {
    let mut body = Writer(Vec::new());
    for a in args {
        body.str(a);
    }
    let mut fields = Writer(vec![0; 16]);
    fields.field(1, "o", |w| w.str(path));
    fields.field(2, "s", |w| w.str(interface));
    fields.field(3, "s", |w| w.str(member));
    fields.field(6, "s", |w| w.str(dest));
    if !args.is_empty() {
        fields.field(8, "g", |w| w.sig(&"s".repeat(args.len())));
    }
    let fields_len = (fields.0.len() - 16) as u32;
    let mut msg = fields.0;
    msg[0] = b'l';
    msg[1] = METHOD_CALL;
    msg[2] = 0;
    msg[3] = 1;
    msg[4..8].copy_from_slice(&(body.0.len() as u32).to_le_bytes());
    msg[8..12].copy_from_slice(&serial.to_le_bytes());
    msg[12..16].copy_from_slice(&fields_len.to_le_bytes());
    let mut w = Writer(msg);
    w.align(8);
    w.0.extend_from_slice(&body.0);
    w.0
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    big: bool,
}

impl Reader<'_> {
    fn align(&mut self, n: usize) -> io::Result<()> {
        let p = self.pos.div_ceil(n) * n;
        if p > self.data.len() {
            return Err(err("truncated message"));
        }
        self.pos = p;
        Ok(())
    }
    fn take(&mut self, n: usize) -> io::Result<&[u8]> {
        let end = self.pos.checked_add(n).ok_or_else(|| err("overflow"))?;
        let s = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| err("truncated message"))?;
        self.pos = end;
        Ok(s)
    }
    fn fixed<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        self.align(N)?;
        let mut b: [u8; N] = self.take(N)?.try_into().map_err(|_| err("short"))?;
        if self.big {
            b.reverse();
        }
        Ok(b)
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.fixed()?))
    }
    fn string(&mut self) -> io::Result<String> {
        let n = self.u32()? as usize;
        let s = self.take(n + 1)?;
        Ok(String::from_utf8_lossy(&s[..n]).into_owned())
    }
    fn signature(&mut self) -> io::Result<String> {
        let n = self.take(1)?[0] as usize;
        let s = self.take(n + 1)?;
        Ok(String::from_utf8_lossy(&s[..n]).into_owned())
    }

    /// Read one value of the complete type at the start of `sig`; returns the
    /// value and the rest of the signature.
    fn value<'s>(&mut self, sig: &'s str, depth: usize) -> io::Result<(Value, &'s str)> {
        if depth > MAX_DEPTH {
            return Err(err("message nested too deeply"));
        }
        let c = sig.chars().next().ok_or_else(|| err("empty signature"))?;
        let rest = &sig[1..];
        let v = match c {
            'y' => Value::Byte(self.take(1)?[0]),
            'b' => Value::Bool(self.u32()? != 0),
            'n' => Value::I16(i16::from_le_bytes(self.fixed()?)),
            'q' => Value::U16(u16::from_le_bytes(self.fixed()?)),
            'i' => Value::I32(i32::from_le_bytes(self.fixed()?)),
            'u' => Value::U32(self.u32()?),
            'h' => Value::Fd(self.u32()?),
            'x' => Value::I64(i64::from_le_bytes(self.fixed()?)),
            't' => Value::U64(u64::from_le_bytes(self.fixed()?)),
            'd' => Value::F64(f64::from_le_bytes(self.fixed()?)),
            's' | 'o' => Value::Str(self.string()?),
            'g' => Value::Str(self.signature()?),
            'v' => {
                let inner = self.signature()?;
                let (v, tail) = self.value(&inner, depth + 1)?;
                if !tail.is_empty() {
                    return Err(err("variant with more than one type"));
                }
                Value::Variant(Box::new(v))
            }
            '(' | '{' => {
                self.align(8)?;
                let close = if c == '(' { ')' } else { '}' };
                let mut items = Vec::new();
                let mut s = rest;
                while !s.starts_with(close) {
                    let (v, tail) = self.value(s, depth + 1)?;
                    items.push(v);
                    s = tail;
                }
                return Ok((Value::Struct(items), &s[1..]));
            }
            'a' => {
                let len = self.u32()? as usize;
                let elem_end = type_end(rest)?;
                let elem = &rest[..elem_end];
                self.align(alignment(elem))?;
                let end = self.pos.checked_add(len).ok_or_else(|| err("overflow"))?;
                if end > self.data.len() {
                    return Err(err("truncated array"));
                }
                let mut items = Vec::new();
                while self.pos < end {
                    items.push(self.value(elem, depth + 1)?.0);
                }
                return Ok((Value::Array(items), &rest[elem_end..]));
            }
            other => return Err(err(format!("unsupported type {other:?}"))),
        };
        Ok((v, rest))
    }
}

fn alignment(sig: &str) -> usize {
    match sig.as_bytes().first() {
        Some(b'n' | b'q') => 2,
        Some(b'b' | b'i' | b'u' | b'h' | b's' | b'o' | b'a') => 4,
        Some(b'x' | b't' | b'd' | b'(' | b'{') => 8,
        _ => 1,
    }
}

/// Length of the first complete type in `sig`.
fn type_end(sig: &str) -> io::Result<usize> {
    let b = sig.as_bytes();
    let mut i = 0;
    while b.get(i) == Some(&b'a') {
        i += 1;
    }
    match b.get(i) {
        Some(b'(' | b'{') => {
            let mut depth = 0;
            for (j, &c) in b.iter().enumerate().skip(i) {
                match c {
                    b'(' | b'{' => depth += 1,
                    b')' | b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(j + 1);
                        }
                    }
                    _ => {}
                }
            }
            Err(err("unbalanced signature"))
        }
        Some(_) => Ok(i + 1),
        None => Err(err("incomplete signature")),
    }
}

fn read_message(stream: &mut impl Read) -> io::Result<Message> {
    let mut fixed = [0u8; 16];
    stream.read_exact(&mut fixed)?;
    let big = match fixed[0] {
        b'l' => false,
        b'B' => true,
        _ => return Err(err("bad endianness byte")),
    };
    let word = |i: usize| {
        let b: [u8; 4] = fixed[i..i + 4].try_into().unwrap_or_default();
        if big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        }
    };
    let (body_len, fields_len) = (word(4) as usize, word(12) as usize);
    if body_len + fields_len > MAX_MESSAGE {
        return Err(err("message too large"));
    }
    let header_len = (16 + fields_len).div_ceil(8) * 8;
    let mut data = vec![0u8; header_len + body_len];
    data[..16].copy_from_slice(&fixed);
    stream.read_exact(&mut data[16..])?;
    decode(&data)
}

fn decode(data: &[u8]) -> io::Result<Message> {
    let big = data.first() == Some(&b'B');
    let mut r = Reader { data, pos: 12, big };
    let fields_len = r.u32()? as usize;
    let fields_end = 16 + fields_len;
    let mut msg = Message {
        kind: data[1],
        ..Default::default()
    };
    r.pos = 8;
    msg.serial = r.u32()?;
    r.pos = 16;
    let mut body_sig = String::new();
    while r.pos < fields_end {
        r.align(8)?;
        let code = r.take(1)?[0];
        let sig = r.signature()?;
        let (v, _) = r.value(&sig, 1)?;
        match (code, v) {
            (1, Value::Str(s)) => msg.path = Some(s),
            (2, Value::Str(s)) => msg.interface = Some(s),
            (3, Value::Str(s)) => msg.member = Some(s),
            (4, Value::Str(s)) => msg.error_name = Some(s),
            (5, Value::U32(n)) => msg.reply_serial = Some(n),
            (8, Value::Str(s)) => body_sig = s,
            _ => {}
        }
    }
    r.pos = fields_end;
    r.align(8)?;
    let body = &data[r.pos..];
    let mut br = Reader {
        data: body,
        pos: 0,
        big,
    };
    let mut sig = body_sig.as_str();
    while !sig.is_empty() {
        let (v, tail) = br.value(sig, 0)?;
        msg.body.push(v);
        sig = tail;
    }
    Ok(msg)
}

// ---------------------------------------------------------------------------
// Desktop helpers
// ---------------------------------------------------------------------------

/// One setting from the XDG Settings portal (`org.freedesktop.appearance`
/// keys like `color-scheme` and `accent-color`), variant wrapper removed.
pub fn portal_setting(conn: &mut Connection, namespace: &str, key: &str) -> Option<Value> {
    let call = |conn: &mut Connection, method| {
        conn.call(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
            method,
            &[namespace, key],
        )
    };
    // ReadOne (portal v2) returns the value; old portals only have Read,
    // which wraps it in an extra variant. unwrap_variant handles both.
    let body = call(conn, "ReadOne").or_else(|_| call(conn, "Read")).ok()?;
    body.into_iter().next().map(|v| v.unwrap_variant().clone())
}

/// `accent-color`: `(ddd)` in 0..1 (out of range means "none set").
pub fn accent_color(v: &Value) -> Option<[f32; 3]> {
    let Value::Struct(items) = v.unwrap_variant() else {
        return None;
    };
    let rgb: Vec<f64> = items
        .iter()
        .filter_map(|i| match i {
            Value::F64(f) => Some(*f),
            _ => None,
        })
        .collect();
    match rgb.as_slice() {
        [r, g, b] if [r, g, b].iter().all(|c| (0.0..=1.0).contains(*c)) => {
            Some([*r as f32, *g as f32, *b as f32])
        }
        _ => None,
    }
}

/// `color-scheme`: `Some(true)` prefers dark, `Some(false)` prefers light,
/// `None` has no preference.
pub fn prefers_dark(v: &Value) -> Option<bool> {
    match v.as_u32()? {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// The system time zone from systemd-timedated, e.g. `America/Chicago`.
pub fn system_timezone(timeout: Duration) -> Option<String> {
    let mut conn = Connection::open(Bus::System, timeout).ok()?;
    let body = conn
        .call(
            "org.freedesktop.timedate1",
            "/org/freedesktop/timedate1",
            "org.freedesktop.DBus.Properties",
            "Get",
            &["org.freedesktop.timedate1", "Timezone"],
        )
        .ok()?;
    body.first()?.as_str().map(str::to_string)
}

/// Block forever, calling `f(namespace, key, value)` for every portal
/// `SettingChanged` signal. Run it on its own thread; returns on bus errors.
pub fn watch_portal_settings(mut f: impl FnMut(&str, &str, &Value)) -> io::Result<()> {
    let mut conn = Connection::open(Bus::Session, Duration::from_secs(2))?;
    conn.add_match(
        "type='signal',interface='org.freedesktop.portal.Settings',member='SettingChanged'",
    )?;
    conn.set_timeout(None)?;
    loop {
        let msg = conn.next_message()?;
        if msg.kind == SIGNAL
            && msg.member.as_deref() == Some("SettingChanged")
            && let [ns, key, value] = msg.body.as_slice()
            && let (Some(ns), Some(key)) = (ns.as_str(), key.as_str())
        {
            f(ns, key, value.unwrap_variant());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_calls_round_trip_through_the_decoder() {
        let bytes = encode_call(
            7,
            "org.example",
            "/a/b",
            "org.example.I",
            "Do",
            &["x", "yz"],
        );
        let msg = decode(&bytes).unwrap();
        assert_eq!(msg.kind, METHOD_CALL);
        assert_eq!(msg.serial, 7);
        assert_eq!(msg.path.as_deref(), Some("/a/b"));
        assert_eq!(msg.member.as_deref(), Some("Do"));
        assert_eq!(
            msg.body,
            vec![Value::Str("x".into()), Value::Str("yz".into())]
        );
    }

    /// Build a reply whose body is a single variant of `inner_sig`.
    fn reply(inner_sig: &str, value: impl FnOnce(&mut Writer)) -> Vec<u8> {
        let mut body = Writer(Vec::new());
        body.sig(inner_sig);
        value(&mut body);
        let mut f = Writer(vec![0; 16]);
        f.field(5, "u", |w| w.u32(3));
        f.field(8, "g", |w| w.sig("v"));
        let n = (f.0.len() - 16) as u32;
        f.0[0] = b'l';
        f.0[1] = METHOD_RETURN;
        f.0[3] = 1;
        f.0[4..8].copy_from_slice(&(body.0.len() as u32).to_le_bytes());
        f.0[8..12].copy_from_slice(&9u32.to_le_bytes());
        f.0[12..16].copy_from_slice(&n.to_le_bytes());
        f.align(8);
        f.0.extend_from_slice(&body.0);
        f.0
    }

    #[test]
    fn decodes_portal_style_replies() {
        let bytes = reply("(ddd)", |w| {
            w.align(8);
            for c in [0.25f64, 0.5, 1.0] {
                w.0.extend_from_slice(&c.to_le_bytes());
            }
        });
        let msg = decode(&bytes).unwrap();
        assert_eq!(msg.reply_serial, Some(3));
        assert_eq!(accent_color(&msg.body[0]), Some([0.25, 0.5, 1.0]));

        let msg = decode(&reply("u", |w| w.u32(1))).unwrap();
        assert_eq!(prefers_dark(&msg.body[0]), Some(true));

        let msg = decode(&reply("s", |w| w.str("Europe/Paris"))).unwrap();
        assert_eq!(msg.body[0].as_str(), Some("Europe/Paris"));

        // Out-of-range accent means "no accent set".
        let bytes = reply("(ddd)", |w| {
            w.align(8);
            for c in [-1.0f64, -1.0, -1.0] {
                w.0.extend_from_slice(&c.to_le_bytes());
            }
        });
        assert_eq!(accent_color(&decode(&bytes).unwrap().body[0]), None);
    }

    #[test]
    fn decodes_arrays_and_dicts() {
        let mut body = Writer(Vec::new());
        // a{su}: two entries
        let len_at = body.0.len();
        body.u32(0);
        body.align(8);
        let start = body.0.len();
        for (k, v) in [("a", 1u32), ("bb", 2)] {
            body.align(8);
            body.str(k);
            body.u32(v);
        }
        let len = (body.0.len() - start) as u32;
        body.0[len_at..len_at + 4].copy_from_slice(&len.to_le_bytes());
        let mut r = Reader {
            data: &body.0,
            pos: 0,
            big: false,
        };
        let (v, rest) = r.value("a{su}", 0).unwrap();
        assert!(rest.is_empty());
        let Value::Array(items) = v else { panic!() };
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[1],
            Value::Struct(vec![Value::Str("bb".into()), Value::U32(2)])
        );
    }

    #[test]
    fn rejects_hostile_input() {
        // Truncated.
        let bytes = encode_call(1, "a", "/", "b", "c", &["x"]);
        assert!(decode(&bytes[..bytes.len() - 3]).is_err());
        // Absurd nesting.
        let sig = "(".repeat(40) + "y" + &")".repeat(40);
        let data = vec![0u8; 400];
        let mut r = Reader {
            data: &data,
            pos: 0,
            big: false,
        };
        assert!(r.value(&sig, 0).is_err());
        // Oversized length prefix.
        let mut hdr = [0u8; 16];
        hdr[0] = b'l';
        hdr[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_message(&mut &hdr[..]).is_err());
    }

    #[test]
    fn addresses_unescape() {
        assert_eq!(unescape("/run/user/1000/bus"), "/run/user/1000/bus");
        assert_eq!(unescape("/tmp/a%20b"), "/tmp/a b");
    }
}
