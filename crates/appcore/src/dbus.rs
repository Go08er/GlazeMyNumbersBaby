//! A tiny synchronous D-Bus client: just enough to read desktop settings
//! from the XDG portal, ask systemd-timedated for the time zone, open a link
//! through the portal and watch for setting changes.
//!
//! It deliberately starts no threads and needs no libraries, so it can run
//! first thing in `main` (before anything else may read the environment) and
//! costs nothing in a twin that otherwise wouldn't link a D-Bus stack.
//!
//! Everything read from the bus is treated as untrusted: signatures are
//! validated against the grammar before use, containers are decoded within
//! their declared byte ranges, and messages are bounded in size and in the
//! number of values they may expand to. Every operation has an overall
//! deadline. Wire format:
//! <https://dbus.freedesktop.org/doc/dbus-specification.html>.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

/// Largest message accepted (header fields + body). Desktop settings and
/// replies are tiny; anything near this is not for us.
const MAX_MESSAGE: usize = 1 << 20;
/// Most values one message may decode into (byte arrays count once).
const MAX_VALUES: usize = 16 * 1024;
/// While a call waits for its reply, other incoming messages are kept for
/// [`Connection::next_message`]. Signals a caller declared [`Wanted`] are
/// kept up to the first pair of bounds (they come from one trusted sender,
/// or, until it is known, on one object path), other signals up to the
/// second (method calls to us and stale replies are
/// dropped): a flood can neither push out an awaited signal nor queue more
/// than this many messages and wire bytes. The bounds count wire bytes; the
/// decoded messages take more heap than that, but each is bounded too (at
/// most [`MAX_VALUES`] values, see [`MAX_MESSAGE`]).
const MAX_WANTED_QUEUED: (usize, usize) = (1024, 16 << 20);
const MAX_OTHER_QUEUED: (usize, usize) = (64, 1 << 20);
/// Deepest container nesting accepted (the specification's limit per kind).
const MAX_DEPTH: usize = 32;

const METHOD_CALL: u8 = 1;
const METHOD_RETURN: u8 = 2;
const ERROR: u8 = 3;
pub const SIGNAL: u8 = 4;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

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
    /// `ay`, kept compact.
    Bytes(Vec<u8>),
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

/// Method-call arguments this client can send.
#[derive(Clone, Copy, Debug)]
pub enum Arg<'a> {
    Str(&'a str),
    U32(u32),
    /// An empty `a{sv}` (options dictionaries we have nothing to put in).
    EmptyDict,
    /// An `a{sv}` whose values are all strings.
    StrDict(&'a [(&'a str, &'a str)]),
}

#[derive(Clone, Debug, Default)]
pub struct Message {
    /// Bytes on the wire.
    pub size: usize,
    pub kind: u8,
    pub serial: u32,
    pub reply_serial: Option<u32>,
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error_name: Option<String>,
    /// Unique name of the sending connection (set by the bus).
    pub sender: Option<String>,
    pub body: Vec<Value>,
}

/// A signal a caller is waiting for (see [`Connection::set_wanted`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Wanted {
    /// The sending connection's unique name (the bus sets it, so no other
    /// peer can match); `None` while it isn't known yet: any peer can then
    /// send a matching signal, so `path` should narrow it down.
    pub sender: Option<String>,
    /// The object path, if only one is wanted.
    pub path: Option<String>,
    pub interface: String,
    pub member: String,
}

impl Wanted {
    fn matches(&self, m: &Message) -> bool {
        let field = |want: &Option<String>, got: &Option<String>| {
            want.is_none() || got.as_deref() == want.as_deref()
        };
        m.kind == SIGNAL
            && field(&self.sender, &m.sender)
            && field(&self.path, &m.path)
            && m.interface.as_deref() == Some(self.interface.as_str())
            && m.member.as_deref() == Some(self.member.as_str())
    }
}

pub struct Connection {
    stream: UnixStream,
    serial: u32,
    /// Bound on each whole call (write + reply); `None` waits forever.
    timeout: Option<Duration>,
    /// Signals read while waiting for a reply, and whether each was wanted.
    queued: VecDeque<(Message, bool)>,
    /// Count and bytes of the queued messages that were wanted / not.
    queued_wanted: (usize, usize),
    queued_other: (usize, usize),
    wanted: Vec<Wanted>,
    /// Our unique bus name (from `Hello`).
    unique: Option<String>,
}

fn err(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "D-Bus operation timed out")
}

/// Read exactly `buf.len()` bytes before `deadline` (if any), however slowly
/// the peer drips them.
fn read_exact_by(
    stream: &mut UnixStream,
    buf: &mut [u8],
    deadline: Option<Instant>,
) -> io::Result<()> {
    let mut got = 0;
    while got < buf.len() {
        if let Some(d) = deadline {
            let left = d.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(timed_out());
            }
            stream.set_read_timeout(Some(left))?;
        } else {
            stream.set_read_timeout(None)?;
        }
        match stream.read(&mut buf[got..]) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "bus closed")),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Err(timed_out());
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Write all of `data` before `deadline` (if any), re-checking the time
/// before every partial write.
fn write_all_by(stream: &mut UnixStream, data: &[u8], deadline: Option<Instant>) -> io::Result<()> {
    let mut done = 0;
    while done < data.len() {
        let left = deadline.map(|d| d.saturating_duration_since(Instant::now()));
        if left.is_some_and(|l| l.is_zero()) {
            return Err(timed_out());
        }
        stream.set_write_timeout(left)?;
        match stream.write(&data[done..]) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "bus closed")),
            Ok(n) => done += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Err(timed_out());
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

impl Connection {
    /// Connect, authenticate and register with the bus, all within
    /// `timeout`; later calls each get the same budget.
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
        Self::open_address(&addr, timeout)
    }

    /// Like [`Connection::open`] for an explicit D-Bus address.
    pub fn open_address(addr: &str, timeout: Duration) -> io::Result<Connection> {
        let deadline = Some(Instant::now() + timeout);
        let stream = connect(addr)?;
        let mut conn = Connection {
            stream,
            serial: 0,
            timeout: Some(timeout),
            queued: VecDeque::new(),
            queued_wanted: (0, 0),
            queued_other: (0, 0),
            wanted: Vec::new(),
            unique: None,
        };
        conn.authenticate(deadline)?;
        let hello = conn.call_by(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "Hello",
            &[],
            deadline,
        )?;
        conn.unique = hello
            .body
            .first()
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(conn)
    }

    /// Budget for each later call; `None` waits forever (signal watching).
    pub fn set_timeout(&mut self, timeout: Option<Duration>) {
        self.timeout = timeout;
    }

    fn authenticate(&mut self, deadline: Option<Instant>) -> io::Result<()> {
        use std::os::unix::fs::MetadataExt;
        let uid = std::fs::metadata("/proc/self")?.uid();
        let hex: String = uid
            .to_string()
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect();
        write_all_by(
            &mut self.stream,
            format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes(),
            deadline,
        )?;
        let line = self.read_line(deadline)?;
        if !line.starts_with("OK ") {
            return Err(err(format!("D-Bus auth rejected: {line}")));
        }
        write_all_by(&mut self.stream, b"BEGIN\r\n", deadline)
    }

    fn read_line(&mut self, deadline: Option<Instant>) -> io::Result<String> {
        let mut line = Vec::new();
        let mut b = [0u8];
        while !line.ends_with(b"\r\n") {
            read_exact_by(&mut self.stream, &mut b, deadline)?;
            line.push(b[0]);
            if line.len() > 512 {
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
        let args: Vec<Arg> = args.iter().map(|a| Arg::Str(a)).collect();
        self.call_args(dest, path, interface, member, &args)
    }

    pub fn call_args(
        &mut self,
        dest: &str,
        path: &str,
        interface: &str,
        member: &str,
        args: &[Arg],
    ) -> io::Result<Vec<Value>> {
        self.call_reply(dest, path, interface, member, args)
            .map(|reply| reply.body)
    }

    /// [`call_args`](Self::call_args), returning the whole reply: its
    /// `sender` is the connection that took the call (the bus sets it, and
    /// lets only the connection it delivered a call to answer it).
    pub fn call_reply(
        &mut self,
        dest: &str,
        path: &str,
        interface: &str,
        member: &str,
        args: &[Arg],
    ) -> io::Result<Message> {
        let deadline = self.timeout.map(|t| Instant::now() + t);
        self.call_by(dest, path, interface, member, args, deadline)
    }

    fn call_by(
        &mut self,
        dest: &str,
        path: &str,
        interface: &str,
        member: &str,
        args: &[Arg],
        deadline: Option<Instant>,
    ) -> io::Result<Message> {
        self.serial = self.serial.wrapping_add(1).max(1);
        let serial = self.serial;
        let bytes = encode_call(serial, dest, path, interface, member, args);
        write_all_by(&mut self.stream, &bytes, deadline)?;
        loop {
            let msg = read_message(&mut self.stream, deadline)?;
            match msg.kind {
                METHOD_RETURN if msg.reply_serial == Some(serial) => return Ok(msg),
                ERROR if msg.reply_serial == Some(serial) => {
                    let detail = msg.body.first().and_then(Value::as_str).unwrap_or("");
                    return Err(io::Error::other(format!(
                        "{}: {detail}",
                        msg.error_name.unwrap_or_default()
                    )));
                }
                _ => self.queue(msg),
            }
        }
    }

    /// The signals to keep, whatever else arrives, while a call waits for
    /// its reply. Signals already kept are sorted again: one no longer
    /// wanted (an impostor's, kept while its sender wasn't known) is kept
    /// only as far as the other signals' bounds allow.
    pub fn set_wanted(&mut self, wanted: Vec<Wanted>) {
        self.wanted = wanted;
        let queued = std::mem::take(&mut self.queued);
        self.queued_wanted = (0, 0);
        self.queued_other = (0, 0);
        for (msg, _) in queued {
            self.queue(msg);
        }
    }

    /// Keeps a message that arrived while a call waited for its reply.
    fn queue(&mut self, msg: Message) {
        if msg.kind != SIGNAL {
            return;
        }
        let wanted = self.wanted.iter().any(|w| w.matches(&msg));
        let (used, max) = if wanted {
            (&mut self.queued_wanted, MAX_WANTED_QUEUED)
        } else {
            (&mut self.queued_other, MAX_OTHER_QUEUED)
        };
        if used.0 >= max.0 || used.1.saturating_add(msg.size) > max.1 {
            return;
        }
        used.0 += 1;
        used.1 += msg.size;
        self.queued.push_back((msg, wanted));
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

    /// The unique name currently owning `name`, if any.
    pub fn name_owner(&mut self, name: &str) -> Option<String> {
        self.call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetNameOwner",
            &[name],
        )
        .ok()?
        .first()?
        .as_str()
        .map(str::to_string)
    }

    /// The next incoming message (within this connection's timeout).
    pub fn next_message(&mut self) -> io::Result<Message> {
        match self.queued.pop_front() {
            Some((m, wanted)) => {
                let used = if wanted {
                    &mut self.queued_wanted
                } else {
                    &mut self.queued_other
                };
                used.0 -= 1;
                used.1 -= m.size;
                Ok(m)
            }
            None => {
                let deadline = self.timeout.map(|t| Instant::now() + t);
                read_message(&mut self.stream, deadline)
            }
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
// Signatures
// ---------------------------------------------------------------------------

fn is_basic(c: u8) -> bool {
    b"ybnqiuxtdsogh".contains(&c)
}

/// Parse one complete type at `sig[i..]`; returns the index after it.
fn complete_type(sig: &[u8], i: usize, arrays: usize, structs: usize) -> Option<usize> {
    match *sig.get(i)? {
        c if is_basic(c) || c == b'v' => Some(i + 1),
        b'a' => {
            if arrays >= MAX_DEPTH {
                return None;
            }
            if sig.get(i + 1) == Some(&b'{') {
                // Dict entry: a basic key and one value, only inside an array.
                if structs >= MAX_DEPTH || !is_basic(*sig.get(i + 2)?) {
                    return None;
                }
                let j = complete_type(sig, i + 3, arrays + 1, structs + 1)?;
                (sig.get(j) == Some(&b'}')).then_some(j + 1)
            } else {
                complete_type(sig, i + 1, arrays + 1, structs)
            }
        }
        b'(' => {
            if structs >= MAX_DEPTH {
                return None;
            }
            let mut j = i + 1;
            let mut fields = 0;
            while *sig.get(j)? != b')' {
                j = complete_type(sig, j, arrays, structs + 1)?;
                fields += 1;
            }
            // Empty structs are not allowed.
            (fields > 0).then_some(j + 1)
        }
        _ => None,
    }
}

/// A valid signature: ASCII, at most 255 bytes, a sequence of complete types.
pub fn valid_signature(sig: &str) -> bool {
    let b = sig.as_bytes();
    if b.len() > 255 || !sig.is_ascii() {
        return false;
    }
    let mut i = 0;
    while i < b.len() {
        match complete_type(b, i, 0, 0) {
            Some(j) => i = j,
            None => return false,
        }
    }
    true
}

/// Exactly one complete type (variant contents).
fn single_type(sig: &str) -> bool {
    valid_signature(sig)
        && !sig.is_empty()
        && complete_type(sig.as_bytes(), 0, 0, 0) == Some(sig.len())
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
    args: &[Arg],
) -> Vec<u8> {
    let mut body = Writer(Vec::new());
    let mut sig = String::new();
    for a in args {
        match a {
            Arg::Str(s) => {
                body.str(s);
                sig.push('s');
            }
            Arg::U32(v) => {
                body.u32(*v);
                sig.push('u');
            }
            Arg::EmptyDict => {
                // Length 0, then padding to the entries' 8-byte alignment
                // (required even for an empty array).
                body.u32(0);
                body.align(8);
                sig.push_str("a{sv}");
            }
            Arg::StrDict(entries) => {
                body.u32(0);
                let len_at = body.0.len() - 4;
                body.align(8);
                let start = body.0.len();
                for (k, v) in entries.iter() {
                    body.align(8);
                    body.str(k);
                    body.sig("s");
                    body.str(v);
                }
                let len = (body.0.len() - start) as u32;
                body.0[len_at..len_at + 4].copy_from_slice(&len.to_le_bytes());
                sig.push_str("a{sv}");
            }
        }
    }
    let mut fields = Writer(vec![0; 16]);
    fields.field(1, "o", |w| w.str(path));
    fields.field(2, "s", |w| w.str(interface));
    fields.field(3, "s", |w| w.str(member));
    fields.field(6, "s", |w| w.str(dest));
    if !args.is_empty() {
        fields.field(8, "g", |w| w.sig(&sig));
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
    /// End of the innermost container being read.
    end: usize,
    big: bool,
    /// Values decoded so far (budgeted).
    values: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8], big: bool) -> Self {
        Reader {
            data,
            pos: 0,
            end: data.len(),
            big,
            values: 0,
        }
    }

    fn align(&mut self, n: usize) -> io::Result<()> {
        let p = self.pos.div_ceil(n) * n;
        if p > self.end {
            return Err(err("truncated message"));
        }
        if self.data[self.pos..p].iter().any(|&b| b != 0) {
            return Err(err("non-zero padding"));
        }
        self.pos = p;
        Ok(())
    }
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or_else(|| err("overflow"))?;
        if end > self.end {
            return Err(err("truncated message"));
        }
        let s = &self.data[self.pos..end];
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
    fn text(bytes: &[u8]) -> io::Result<String> {
        let (body, nul) = bytes.split_at(bytes.len() - 1);
        if nul != [0] || body.contains(&0) {
            return Err(err("bad string terminator"));
        }
        String::from_utf8(body.to_vec()).map_err(|_| err("invalid UTF-8"))
    }
    fn string(&mut self) -> io::Result<String> {
        let n = self.u32()? as usize;
        let s = self.take(n.checked_add(1).ok_or_else(|| err("overflow"))?)?;
        Self::text(s)
    }
    fn signature(&mut self) -> io::Result<String> {
        let n = self.take(1)?[0] as usize;
        let s = Self::text(self.take(n + 1)?)?;
        if !valid_signature(&s) {
            return Err(err("invalid signature"));
        }
        Ok(s)
    }

    /// Read one value of the complete type at the start of `sig` (already
    /// validated); returns it and the rest of the signature.
    fn value<'s>(&mut self, sig: &'s str, depth: usize) -> io::Result<(Value, &'s str)> {
        if depth > 2 * MAX_DEPTH {
            return Err(err("message nested too deeply"));
        }
        self.values += 1;
        if self.values > MAX_VALUES {
            return Err(err("message too complex"));
        }
        let c = *sig
            .as_bytes()
            .first()
            .ok_or_else(|| err("empty signature"))?;
        if !c.is_ascii() {
            return Err(err("invalid signature"));
        }
        let rest = &sig[1..];
        let v = match c {
            b'y' => Value::Byte(self.take(1)?[0]),
            b'b' => match self.u32()? {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                _ => return Err(err("invalid boolean")),
            },
            b'n' => Value::I16(i16::from_le_bytes(self.fixed()?)),
            b'q' => Value::U16(u16::from_le_bytes(self.fixed()?)),
            b'i' => Value::I32(i32::from_le_bytes(self.fixed()?)),
            b'u' => Value::U32(self.u32()?),
            b'h' => Value::Fd(self.u32()?),
            b'x' => Value::I64(i64::from_le_bytes(self.fixed()?)),
            b't' => Value::U64(u64::from_le_bytes(self.fixed()?)),
            b'd' => Value::F64(f64::from_le_bytes(self.fixed()?)),
            b's' | b'o' => Value::Str(self.string()?),
            b'g' => Value::Str(self.signature()?),
            b'v' => {
                let inner = self.signature()?;
                if !single_type(&inner) {
                    return Err(err("variant must hold exactly one type"));
                }
                let (v, _) = self.value(&inner, depth + 1)?;
                Value::Variant(Box::new(v))
            }
            b'(' | b'{' => {
                self.align(8)?;
                let close = if c == b'(' { ')' } else { '}' };
                let mut items = Vec::new();
                let mut s = rest;
                while !s.starts_with(close) {
                    let (v, tail) = self.value(s, depth + 1)?;
                    items.push(v);
                    s = tail;
                }
                return Ok((Value::Struct(items), &s[1..]));
            }
            b'a' => {
                let len = self.u32()? as usize;
                // Measure the whole `a…` type (handles `a{…}` dict entries).
                let elem_len = complete_type(sig.as_bytes(), 0, 0, 0)
                    .ok_or_else(|| err("bad array type"))?
                    - 1;
                let elem = &rest[..elem_len];
                let align = alignment(elem);
                self.align(align)?;
                let start = self.pos;
                let end = start.checked_add(len).ok_or_else(|| err("overflow"))?;
                if end > self.end {
                    return Err(err("array overruns its container"));
                }
                let v = if elem == "y" {
                    Value::Bytes(self.take(len)?.to_vec())
                } else {
                    // Decode strictly within the array's byte range.
                    let outer = std::mem::replace(&mut self.end, end);
                    let mut items = Vec::new();
                    while self.pos < end {
                        let before = self.pos;
                        items.push(self.value(elem, depth + 1)?.0);
                        if self.pos == before {
                            return Err(err("array element made no progress"));
                        }
                    }
                    self.end = outer;
                    if self.pos != end {
                        return Err(err("array length mismatch"));
                    }
                    Value::Array(items)
                };
                return Ok((v, &rest[elem_len..]));
            }
            _ => return Err(err("unsupported type")),
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

fn read_message(stream: &mut UnixStream, deadline: Option<Instant>) -> io::Result<Message> {
    let mut fixed = [0u8; 16];
    read_exact_by(stream, &mut fixed, deadline)?;
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
    if body_len.saturating_add(fields_len) > MAX_MESSAGE {
        return Err(err("message too large"));
    }
    let header_len = (16 + fields_len).div_ceil(8) * 8;
    let mut data = vec![0u8; header_len + body_len];
    data[..16].copy_from_slice(&fixed);
    read_exact_by(stream, &mut data[16..], deadline)?;
    decode(&data)
}

fn decode(data: &[u8]) -> io::Result<Message> {
    if data.len() < 16 {
        return Err(err("truncated message"));
    }
    let big = data[0] == b'B';
    let mut r = Reader::new(data, big);
    r.pos = 12;
    let fields_len = r.u32()? as usize;
    let fields_end = 16usize
        .checked_add(fields_len)
        .ok_or_else(|| err("overflow"))?;
    if fields_end > data.len() {
        return Err(err("truncated header"));
    }
    let mut msg = Message {
        size: data.len(),
        kind: data[1],
        ..Default::default()
    };
    r.pos = 8;
    msg.serial = r.u32()?;
    r.pos = 16;
    r.end = fields_end;
    let mut body_sig = String::new();
    let mut seen = [false; 10];
    while r.pos < fields_end {
        r.align(8)?;
        let code = r.take(1)?[0];
        let sig = r.signature()?;
        if !single_type(&sig) {
            return Err(err("bad header field"));
        }
        // Known fields must have their specified wire type and appear once.
        let expected = match code {
            1 => Some("o"),
            2 | 3 | 4 | 6 | 7 => Some("s"),
            5 | 9 => Some("u"),
            8 => Some("g"),
            _ => None,
        };
        if let Some(want) = expected {
            if sig != want {
                return Err(err("header field has the wrong type"));
            }
            if std::mem::replace(&mut seen[code as usize], true) {
                return Err(err("duplicate header field"));
            }
        }
        let (v, _) = r.value(&sig, 1)?;
        match (code, v) {
            (1, Value::Str(s)) => msg.path = Some(s),
            (2, Value::Str(s)) => msg.interface = Some(s),
            (3, Value::Str(s)) => msg.member = Some(s),
            (4, Value::Str(s)) => msg.error_name = Some(s),
            (5, Value::U32(n)) => msg.reply_serial = Some(n),
            (7, Value::Str(s)) => msg.sender = Some(s),
            (8, Value::Str(s)) => body_sig = s,
            _ => {}
        }
    }
    r.end = data.len();
    r.align(8)?;
    let body = &data[r.pos..];
    let mut br = Reader::new(body, big);
    br.values = r.values;
    if !valid_signature(&body_sig) {
        return Err(err("invalid body signature"));
    }
    let mut sig = body_sig.as_str();
    while !sig.is_empty() {
        let (v, tail) = br.value(sig, 0)?;
        msg.body.push(v);
        sig = tail;
    }
    if br.pos != body.len() {
        return Err(err("trailing bytes in body"));
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
            PORTAL,
            PORTAL_PATH,
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

/// What became of an [`open_uri`] request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opened {
    Yes,
    /// The user dismissed the app chooser.
    Cancelled,
    /// The portal reported a failure.
    Failed,
    /// The portal took the request but hadn't answered when the wait ended
    /// (an app chooser may still be open).
    Unanswered,
}

/// Ask the desktop (through the OpenURI portal) to open a web link, and wait
/// up to `wait` for the portal's answer. `Err` means the request itself
/// failed (no portal, bus error).
pub fn open_uri(uri: &str, wait: Duration) -> io::Result<Opened> {
    open_uri_on(
        Connection::open(Bus::Session, Duration::from_secs(2))?,
        uri,
        wait,
    )
}

fn open_uri_on(mut conn: Connection, uri: &str, wait: Duration) -> io::Result<Opened> {
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    // Subscribe to the request's Response before making it, using a
    // handle_token so its object path is known in advance.
    let token = format!(
        "gmnb{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let sender = conn
        .unique
        .as_deref()
        .ok_or_else(|| err("no unique name"))?
        .trim_start_matches(':')
        .replace('.', "_");
    let expected = format!("{PORTAL_PATH}/request/{sender}/{token}");
    conn.add_match(&format!(
        "type='signal',sender='{PORTAL}',interface='org.freedesktop.portal.Request',\
         member='Response',path='{expected}'"
    ))?;
    // Only the portal's owner may answer. Its answer is kept whatever else
    // arrives while the calls below wait for their replies: from the owner
    // if it can be named first (starting the portal if it isn't running),
    // else, until the OpenURI call (which then starts it) is answered, any
    // sender's on the request's path. The answer to the call names the
    // owner; signals kept from anyone else are then let go.
    let response = |sender: Option<String>, path: Option<String>| Wanted {
        sender,
        path,
        interface: "org.freedesktop.portal.Request".into(),
        member: "Response".into(),
    };
    let mut owner = conn.name_owner(PORTAL);
    if owner.is_none() {
        let _ = conn.call_args(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "StartServiceByName",
            &[Arg::Str(PORTAL), Arg::U32(0)],
        );
        owner = conn.name_owner(PORTAL);
    }
    conn.set_wanted(vec![match &owner {
        Some(owner) => response(Some(owner.clone()), None),
        None => response(None, Some(expected.clone())),
    }]);
    let reply = conn.call_reply(
        PORTAL,
        PORTAL_PATH,
        "org.freedesktop.portal.OpenURI",
        "OpenURI",
        &[
            Arg::Str(""),
            Arg::Str(uri),
            Arg::StrDict(&[("handle_token", &token)]),
        ],
    )?;
    // The connection that answered is the one the bus gave the call to: the
    // owner (it may have restarted since it was named).
    let owner = reply
        .sender
        .clone()
        .or(owner)
        .ok_or_else(|| err("the portal has no owner"))?;
    conn.set_wanted(vec![response(Some(owner.clone()), None)]);
    // Portals older than 0.9 (2017) ignore handle_token and pick their own
    // path; listen there too. (One that answered before this subscription
    // would be missed, and the wait ends Unanswered rather than wrongly.)
    let handle = reply
        .body
        .first()
        .and_then(Value::as_str)
        .unwrap_or(&expected)
        .to_string();
    if handle != expected {
        conn.add_match(&format!(
            "type='signal',sender='{PORTAL}',interface='org.freedesktop.portal.Request',\
             member='Response',path='{handle}'"
        ))?;
    }
    let deadline = Instant::now() + wait;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(Opened::Unanswered);
        }
        conn.set_timeout(Some(left));
        let msg = match conn.next_message() {
            Err(e) if e.kind() == io::ErrorKind::TimedOut => return Ok(Opened::Unanswered),
            m => m?,
        };
        if msg.kind == SIGNAL
            && msg.interface.as_deref() == Some("org.freedesktop.portal.Request")
            && msg.member.as_deref() == Some("Response")
            && msg.path.as_deref() == Some(handle.as_str())
            && msg.sender.as_deref() == Some(owner.as_str())
        {
            return Ok(match msg.body.first().and_then(Value::as_u32) {
                Some(0) => Opened::Yes,
                Some(1) => Opened::Cancelled,
                _ => Opened::Failed,
            });
        }
    }
}

/// Block, calling `f(namespace, key, value)` for every `SettingChanged`
/// signal the settings portal emits. Only signals from the connection that
/// currently owns `org.freedesktop.portal.Desktop`, on its object path, are
/// accepted. Returns when the bus connection fails; run it on its own thread.
pub fn watch_portal_settings(f: impl FnMut(&str, &str, &Value)) -> io::Result<()> {
    watch_settings_on(Connection::open(Bus::Session, Duration::from_secs(2))?, f)
}

fn watch_settings_on(
    mut conn: Connection,
    mut f: impl FnMut(&str, &str, &Value),
) -> io::Result<()> {
    conn.add_match(&format!(
        "type='signal',sender='{PORTAL}',path='{PORTAL_PATH}',\
         interface='org.freedesktop.portal.Settings',member='SettingChanged'"
    ))?;
    // Follow ownership changes of the portal name, so a signal is checked
    // against whoever owns it at the time it was sent (the bus delivers the
    // NameOwnerChanged notice in order with the signals).
    conn.add_match(&format!(
        "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',\
         member='NameOwnerChanged',arg0='{PORTAL}'"
    ))?;
    let wanted = |owner: &Option<String>| {
        let mut wanted = vec![Wanted {
            sender: Some("org.freedesktop.DBus".into()),
            path: None,
            interface: "org.freedesktop.DBus".into(),
            member: "NameOwnerChanged".into(),
        }];
        wanted.extend(owner.iter().map(|owner| Wanted {
            sender: Some(owner.clone()),
            path: None,
            interface: "org.freedesktop.portal.Settings".into(),
            member: "SettingChanged".into(),
        }));
        wanted
    };
    let mut owner = conn.name_owner(PORTAL);
    loop {
        conn.set_wanted(wanted(&owner));
        conn.set_timeout(None);
        let msg = conn.next_message()?;
        if msg.kind == SIGNAL
            && msg.member.as_deref() == Some("NameOwnerChanged")
            && msg.sender.as_deref() == Some("org.freedesktop.DBus")
        {
            if let [name, _, new] = msg.body.as_slice()
                && name.as_str() == Some(PORTAL)
            {
                owner = new.as_str().filter(|s| !s.is_empty()).map(str::to_string);
            }
            continue;
        }
        if msg.kind != SIGNAL
            || msg.member.as_deref() != Some("SettingChanged")
            || msg.interface.as_deref() != Some("org.freedesktop.portal.Settings")
            || msg.path.as_deref() != Some(PORTAL_PATH)
        {
            continue;
        }
        // The portal may have restarted under a new unique name: re-check
        // ownership before rejecting a signal from an unexpected sender.
        if msg.sender.is_none() || msg.sender != owner {
            conn.set_timeout(Some(Duration::from_secs(2)));
            owner = conn.name_owner(PORTAL);
            if msg.sender.is_none() || msg.sender != owner {
                continue;
            }
        }
        if let [ns, key, value] = msg.body.as_slice()
            && let (Some(ns), Some(key)) = (ns.as_str(), key.as_str())
        {
            f(ns, key, value.unwrap_variant());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A complete little-endian message: header fields + `body_sig` + body.
    fn message(kind: u8, fields: impl FnOnce(&mut Writer), body_sig: &str, body: &[u8]) -> Vec<u8> {
        let mut f = Writer(vec![0; 16]);
        fields(&mut f);
        if !body_sig.is_empty() {
            f.field(8, "g", |w| w.sig(body_sig));
        }
        let n = (f.0.len() - 16) as u32;
        f.0[0] = b'l';
        f.0[1] = kind;
        f.0[3] = 1;
        f.0[4..8].copy_from_slice(&(body.len() as u32).to_le_bytes());
        f.0[8..12].copy_from_slice(&9u32.to_le_bytes());
        f.0[12..16].copy_from_slice(&n.to_le_bytes());
        f.align(8);
        f.0.extend_from_slice(body);
        f.0
    }

    fn reply(body_sig: &str, body: Vec<u8>) -> Vec<u8> {
        message(
            METHOD_RETURN,
            |f| f.field(5, "u", |w| w.u32(3)),
            body_sig,
            &body,
        )
    }

    /// Body holding one variant of `inner_sig`.
    fn variant(inner_sig: &str, value: impl FnOnce(&mut Writer)) -> Vec<u8> {
        let mut body = Writer(Vec::new());
        body.sig(inner_sig);
        value(&mut body);
        reply("v", body.0)
    }

    #[test]
    fn method_calls_round_trip_through_the_decoder() {
        let bytes = encode_call(
            7,
            "org.example",
            "/a/b",
            "org.example.I",
            "Do",
            &[Arg::Str("x"), Arg::Str("yz")],
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

    #[test]
    fn empty_dict_arguments_encode_per_spec() {
        let bytes = encode_call(1, "d", "/", "i", "m", &[Arg::Str("a"), Arg::EmptyDict]);
        let msg = decode(&bytes).unwrap();
        assert_eq!(msg.body, vec![Value::Str("a".into()), Value::Array(vec![])]);
    }

    #[test]
    fn decodes_portal_style_replies() {
        let bytes = variant("(ddd)", |w| {
            w.align(8);
            for c in [0.25f64, 0.5, 1.0] {
                w.0.extend_from_slice(&c.to_le_bytes());
            }
        });
        let msg = decode(&bytes).unwrap();
        assert_eq!(msg.reply_serial, Some(3));
        assert_eq!(accent_color(&msg.body[0]), Some([0.25, 0.5, 1.0]));
        let msg = decode(&variant("u", |w| w.u32(1))).unwrap();
        assert_eq!(prefers_dark(&msg.body[0]), Some(true));
        let msg = decode(&variant("s", |w| w.str("Europe/Paris"))).unwrap();
        assert_eq!(msg.body[0].as_str(), Some("Europe/Paris"));
        let bytes = variant("(ddd)", |w| {
            w.align(8);
            for c in [-1.0f64, -1.0, -1.0] {
                w.0.extend_from_slice(&c.to_le_bytes());
            }
        });
        assert_eq!(accent_color(&decode(&bytes).unwrap().body[0]), None);
    }

    /// A big-endian writer, for valid `B` messages (the shared one is
    /// little-endian only).
    struct Be(Vec<u8>);

    impl Be {
        fn align(&mut self, n: usize) {
            while !self.0.len().is_multiple_of(n) {
                self.0.push(0);
            }
        }
        fn u32(&mut self, v: u32) {
            self.align(4);
            self.0.extend_from_slice(&v.to_be_bytes());
        }
        fn f64(&mut self, v: f64) {
            self.align(8);
            self.0.extend_from_slice(&v.to_be_bytes());
        }
        fn str(&mut self, s: &str) {
            self.u32(s.len() as u32);
            self.0.extend_from_slice(s.as_bytes());
            self.0.push(0);
        }
        fn sig(&mut self, s: &str) {
            self.0.push(s.len() as u8);
            self.0.extend_from_slice(s.as_bytes());
            self.0.push(0);
        }
        /// An array whose elements align to `elem`; its length is patched in.
        fn array(&mut self, elem: usize, items: impl FnOnce(&mut Be)) {
            self.u32(0);
            let at = self.0.len() - 4;
            self.align(elem);
            let start = self.0.len();
            items(self);
            let len = (self.0.len() - start) as u32;
            self.0[at..at + 4].copy_from_slice(&len.to_be_bytes());
        }
        /// A method return to serial 3 with this body.
        fn reply(body_sig: &str, body: Be) -> Vec<u8> {
            let mut m = Be(vec![b'B', METHOD_RETURN, 0, 1]);
            m.u32(body.0.len() as u32);
            m.u32(9);
            m.array(8, |f| {
                f.align(8);
                f.0.push(5);
                f.sig("u");
                f.u32(3);
                f.align(8);
                f.0.push(8);
                f.sig("g");
                f.sig(body_sig);
            });
            m.align(8);
            m.0.extend_from_slice(&body.0);
            m.0
        }
    }

    /// Valid big-endian replies of the shapes we rely on: the settings
    /// portal's ReadAll (`a{sa{sv}}`) and timedated's Timezone (`v` of `s`).
    #[test]
    fn decodes_big_endian_replies() {
        let mut body = Be(Vec::new());
        body.array(8, |b| {
            b.align(8);
            b.str("org.freedesktop.appearance");
            b.array(8, |b| {
                b.align(8);
                b.str("color-scheme");
                b.sig("u");
                b.u32(1);
                b.align(8);
                b.str("accent-color");
                b.sig("(ddd)");
                b.align(8);
                for c in [0.25, 0.5, 1.0] {
                    b.f64(c);
                }
            });
        });
        let msg = decode(&Be::reply("a{sa{sv}}", body)).unwrap();
        assert_eq!(msg.reply_serial, Some(3));
        let Value::Array(namespaces) = &msg.body[0] else {
            panic!("{:?}", msg.body)
        };
        let Value::Struct(ns) = &namespaces[0] else {
            panic!()
        };
        assert_eq!(ns[0].as_str(), Some("org.freedesktop.appearance"));
        let Value::Array(settings) = &ns[1] else {
            panic!()
        };
        let value = |key: &str| {
            settings.iter().find_map(|e| match e {
                Value::Struct(kv) if kv[0].as_str() == Some(key) => Some(kv[1].clone()),
                _ => None,
            })
        };
        assert_eq!(prefers_dark(&value("color-scheme").unwrap()), Some(true));
        assert_eq!(
            accent_color(&value("accent-color").unwrap()),
            Some([0.25, 0.5, 1.0])
        );

        let mut body = Be(Vec::new());
        body.sig("s");
        body.str("Europe/Paris");
        let msg = decode(&Be::reply("v", body)).unwrap();
        assert_eq!(msg.body[0].as_str(), Some("Europe/Paris"));
    }

    #[test]
    fn signals_carry_their_sender() {
        let mut body = Writer(Vec::new());
        body.str("org.freedesktop.appearance");
        body.str("color-scheme");
        body.sig("u");
        body.u32(2);
        let bytes = message(
            SIGNAL,
            |f| {
                f.field(1, "o", |w| w.str(PORTAL_PATH));
                f.field(2, "s", |w| w.str("org.freedesktop.portal.Settings"));
                f.field(3, "s", |w| w.str("SettingChanged"));
                f.field(7, "s", |w| w.str(":1.42"));
            },
            "ssv",
            &body.0,
        );
        let msg = decode(&bytes).unwrap();
        assert_eq!(msg.sender.as_deref(), Some(":1.42"));
        assert_eq!(prefers_dark(&msg.body[2]), Some(false));
    }

    #[test]
    fn decodes_arrays_dicts_and_bytes() {
        let mut body = Writer(Vec::new());
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
        let msg = decode(&reply("a{su}", body.0)).unwrap();
        let Value::Array(items) = &msg.body[0] else {
            panic!()
        };
        assert_eq!(
            items[1],
            Value::Struct(vec![Value::Str("bb".into()), Value::U32(2)])
        );

        let mut body = Writer(Vec::new());
        body.u32(3);
        body.0.extend_from_slice(&[1, 2, 3]);
        assert_eq!(
            decode(&reply("ay", body.0)).unwrap().body[0],
            Value::Bytes(vec![1, 2, 3])
        );
    }

    // The round-3 review's hostile payloads.

    #[test]
    fn rejects_non_ascii_signatures() {
        let bytes = variant("é", |_| {});
        assert!(decode(&bytes).is_err());
        assert!(!valid_signature("é"));
    }

    #[test]
    fn rejects_empty_structs() {
        let mut body = Writer(Vec::new());
        body.u32(1);
        body.align(8);
        body.0.push(0);
        assert!(decode(&reply("a()", body.0)).is_err());
        assert!(!valid_signature("()"));
        assert!(!valid_signature("a()"));
    }

    #[test]
    fn arrays_stay_within_their_length() {
        // `au` claiming 1 byte, followed by a 4-byte integer.
        let mut body = Writer(Vec::new());
        body.u32(1);
        body.u32(42);
        assert!(decode(&reply("au", body.0)).is_err());
    }

    #[test]
    fn large_arrays_are_bounded() {
        // A valid `ay` decodes compactly…
        let mut body = Writer(Vec::new());
        body.u32(512 * 1024);
        body.0.extend(std::iter::repeat_n(7u8, 512 * 1024));
        let msg = decode(&reply("ay", body.0)).unwrap();
        assert!(matches!(&msg.body[0], Value::Bytes(b) if b.len() == 512 * 1024));
        // …but a huge `au` exceeds the value budget instead of fanning out.
        let n = MAX_VALUES + 10;
        let mut body = Writer(Vec::new());
        body.u32((n * 4) as u32);
        for i in 0..n {
            body.u32(i as u32);
        }
        assert!(decode(&reply("au", body.0)).is_err());
    }

    #[test]
    fn signature_grammar() {
        for good in ["", "s", "a{sv}", "(ddd)", "aa{s(ii)}", "v", "ay", "a(sv)"] {
            assert!(valid_signature(good), "{good}");
        }
        for bad in [
            "(", ")", "a", "{sv}", "a{vs}", "a{s}", "a{sss}", "z", "(()", "a{(i)s}",
        ] {
            assert!(!valid_signature(bad), "{bad}");
        }
        assert!(!valid_signature(&"(".repeat(40)));
        assert!(!valid_signature(&("a".repeat(40) + "y")));
        assert!(!single_type("ss"));
    }

    #[test]
    fn rejects_hostile_framing() {
        let bytes = encode_call(1, "a", "/", "b", "c", &[Arg::Str("x")]);
        assert!(decode(&bytes[..bytes.len() - 3]).is_err());
        assert!(decode(&bytes[..10]).is_err());
        // Bad string terminator.
        let mut body = Writer(Vec::new());
        body.u32(1);
        body.0.extend_from_slice(b"xy");
        assert!(decode(&reply("s", body.0)).is_err());
        // Invalid boolean.
        let mut body = Writer(Vec::new());
        body.u32(7);
        assert!(decode(&reply("b", body.0)).is_err());
    }

    #[test]
    fn slow_peers_cannot_stretch_a_deadline() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        let t = std::thread::spawn(move || {
            for _ in 0..40 {
                std::thread::sleep(Duration::from_millis(20));
                if b.write_all(b"x").is_err() {
                    break;
                }
            }
        });
        let start = Instant::now();
        let mut buf = [0u8; 64];
        let r = read_exact_by(
            &mut a,
            &mut buf,
            Some(Instant::now() + Duration::from_millis(100)),
        );
        assert!(r.is_err());
        assert!(start.elapsed() < Duration::from_millis(400));
        drop(a);
        let _ = t.join();
    }

    /// A `SettingChanged(ns, key, <u32>)` signal from `path`.
    fn settings_signal(path: &str, value: u32) -> Vec<u8> {
        let mut body = Writer(Vec::new());
        body.str("org.freedesktop.appearance");
        body.str("color-scheme");
        body.sig("u");
        body.u32(value);
        let mut bytes = message(
            SIGNAL,
            |f| {
                f.field(1, "o", |w| w.str(path));
                f.field(2, "s", |w| w.str("org.freedesktop.portal.Settings"));
                f.field(3, "s", |w| w.str("SettingChanged"));
            },
            "ssv",
            &body.0,
        );
        bytes[2] = 1; // NO_REPLY_EXPECTED
        bytes
    }

    /// A self-contained bus for a test: its own socket and a permissive
    /// policy, so it doesn't depend on the host's session configuration.
    struct PrivateBus {
        daemon: std::process::Child,
        dir: std::path::PathBuf,
        addr: String,
    }

    impl PrivateBus {
        /// `None` (and the test skips) without a working `dbus-daemon`.
        fn start(name: &str) -> Option<PrivateBus> {
            Self::start_with(name, "")
        }

        /// [`start`](Self::start) with `policy` (more `<allow>`/`<deny>`
        /// rules) after the permissive default ones.
        fn start_with(name: &str, policy: &str) -> Option<PrivateBus> {
            use std::io::BufRead;
            let dir = std::env::temp_dir().join(format!("appcore-{name}-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&dir);
            let conf = dir.join("bus.conf");
            std::fs::write(
                &conf,
                format!(
                    r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
    {policy}
  </policy>
</busconfig>"#,
                    dir.join("bus").display()
                ),
            )
            .ok()?;
            let Ok(mut daemon) = std::process::Command::new("dbus-daemon")
                .arg(format!("--config-file={}", conf.display()))
                .args(["--nofork", "--print-address=1"])
                .stdout(std::process::Stdio::piped())
                .spawn()
            else {
                eprintln!("no dbus-daemon; skipped");
                return None;
            };
            let mut addr = String::new();
            let _ = std::io::BufReader::new(daemon.stdout.take()?).read_line(&mut addr);
            let bus = PrivateBus {
                daemon,
                dir,
                addr: addr.trim().to_string(),
            };
            if bus.addr.is_empty() {
                eprintln!("dbus-daemon didn't start; skipped");
                return None;
            }
            Some(bus)
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.daemon.kill();
            let _ = self.daemon.wait();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// Real bus: signals from anyone but the portal's owner are ignored.
    /// Needs `dbus-daemon` on PATH; skipped otherwise.
    #[test]
    fn watcher_ignores_impostors() {
        let Some(bus) = PrivateBus::start("watch") else {
            return;
        };
        let addr = bus.addr.as_str();
        let t = Duration::from_secs(2);

        let (tx, rx) = std::sync::mpsc::channel();
        let watcher = Connection::open_address(addr, t).unwrap();
        std::thread::spawn(move || {
            let _ = watch_settings_on(watcher, |_, _, v| {
                let _ = tx.send(v.as_u32());
            });
        });
        std::thread::sleep(Duration::from_millis(200));

        // An impostor: right interface, wrong path, then right path, but it
        // doesn't own the portal name.
        let mut impostor = Connection::open_address(addr, t).unwrap();
        impostor
            .stream
            .write_all(&settings_signal("/fake", 1))
            .unwrap();
        impostor
            .stream
            .write_all(&settings_signal(PORTAL_PATH, 1))
            .unwrap();
        assert!(
            rx.recv_timeout(Duration::from_millis(400)).is_err(),
            "impostor accepted"
        );

        // The real owner of org.freedesktop.portal.Desktop.
        let mut portal = Connection::open_address(addr, t).unwrap();
        portal
            .call_args(
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "RequestName",
                &[Arg::Str(PORTAL), Arg::U32(4)],
            )
            .unwrap();
        portal
            .stream
            .write_all(&settings_signal(PORTAL_PATH, 2))
            .unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Some(2));
    }

    /// Claim the portal's name on `addr`.
    fn fake_portal(addr: &str) -> Connection {
        let mut portal = Connection::open_address(addr, Duration::from_secs(2)).unwrap();
        portal
            .call_args(
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "RequestName",
                &[Arg::Str(PORTAL), Arg::U32(4)],
            )
            .unwrap();
        portal
    }

    /// Wait for the OpenURI call; returns it after checking its arguments.
    fn take_open_uri(portal: &mut Connection, uri: &str) -> (Message, String) {
        let call = loop {
            let m = portal.next_message().unwrap();
            if m.kind == METHOD_CALL && m.member.as_deref() == Some("OpenURI") {
                break m;
            }
        };
        assert_eq!(call.path.as_deref(), Some(PORTAL_PATH));
        assert_eq!(
            call.interface.as_deref(),
            Some("org.freedesktop.portal.OpenURI")
        );
        let [Value::Str(parent), Value::Str(got), Value::Array(options)] = &call.body[..] else {
            panic!("OpenURI(s, s, a{{sv}}) expected, got {:?}", call.body);
        };
        assert_eq!((parent.as_str(), got.as_str()), ("", uri));
        let token = options
            .iter()
            .find_map(|e| match e {
                Value::Struct(kv) => match &kv[..] {
                    [Value::Str(k), Value::Variant(v)] if k == "handle_token" => {
                        v.as_str().map(str::to_string)
                    }
                    _ => None,
                },
                _ => None,
            })
            .expect("a handle_token option");
        let sender = call.sender.clone().unwrap();
        let path = format!(
            "{PORTAL_PATH}/request/{}/{token}",
            sender.trim_start_matches(':').replace('.', "_")
        );
        (call, path)
    }

    /// The portal's reply to `call`: the request's object path.
    fn request_handle(call: &Message, path: &str) -> Vec<u8> {
        let mut body = Writer(Vec::new());
        body.str(path);
        let sender = call.sender.clone().unwrap();
        message(
            METHOD_RETURN,
            |f| {
                f.field(5, "u", |w| w.u32(call.serial));
                f.field(6, "s", |w| w.str(&sender));
            },
            "o",
            &body.0,
        )
    }

    /// `Request.Response(code, {})` on `path`.
    fn response(path: &str, code: u32) -> Vec<u8> {
        let mut body = Writer(Vec::new());
        body.u32(code);
        body.u32(0); // an empty a{sv}
        body.align(8);
        let mut bytes = message(
            SIGNAL,
            |f| {
                f.field(1, "o", |w| w.str(path));
                f.field(2, "s", |w| w.str("org.freedesktop.portal.Request"));
                f.field(3, "s", |w| w.str("Response"));
            },
            "ua{sv}",
            &body.0,
        );
        bytes[2] = 1; // NO_REPLY_EXPECTED
        bytes
    }

    /// `Request.Response(code, {})` on `path`, sent straight to `to` (a
    /// signal with a destination reaches it whatever its match rules say).
    fn response_to(path: &str, code: u32, to: &str) -> Vec<u8> {
        let mut body = Writer(Vec::new());
        body.u32(code);
        body.u32(0); // an empty a{sv}
        body.align(8);
        let mut bytes = message(
            SIGNAL,
            |f| {
                f.field(1, "o", |w| w.str(path));
                f.field(2, "s", |w| w.str("org.freedesktop.portal.Request"));
                f.field(3, "s", |w| w.str("Response"));
                f.field(6, "s", |w| w.str(to));
            },
            "ua{sv}",
            &body.0,
        );
        bytes[2] = 1; // NO_REPLY_EXPECTED
        bytes
    }

    /// Real bus with a stand-in portal: the call is marshalled as the spec
    /// says, the answer is read from the request object (only from the
    /// portal), and silence or a missing portal are told apart.
    /// Needs `dbus-daemon` on PATH; skipped otherwise.
    #[test]
    fn open_uri_follows_the_request() {
        let Some(bus) = PrivateBus::start("openuri") else {
            return;
        };
        let t = Duration::from_secs(2);
        let uri = "https://example.org/a?b=c";
        let open = |wait| {
            let conn = Connection::open_address(&bus.addr, t).unwrap();
            std::thread::spawn(move || open_uri_on(conn, uri, wait))
        };

        // No portal on the bus: the request itself fails.
        assert!(open(t).join().unwrap().is_err());

        let mut portal = fake_portal(&bus.addr);
        let mut impostor = Connection::open_address(&bus.addr, t).unwrap();
        for (code, outcome) in [
            (0, Opened::Yes),
            (1, Opened::Cancelled),
            (2, Opened::Failed),
        ] {
            let opener = open(t);
            let (call, path) = take_open_uri(&mut portal, uri);
            portal
                .stream
                .write_all(&request_handle(&call, &path))
                .unwrap();
            // Someone else answering for the portal is ignored.
            impostor.stream.write_all(&response(&path, 0)).unwrap();
            std::thread::sleep(Duration::from_millis(50));
            portal.stream.write_all(&response(&path, code)).unwrap();
            assert_eq!(opener.join().unwrap().unwrap(), outcome);
        }

        // The portal took the request but stays quiet.
        let opener = open(Duration::from_millis(300));
        let (call, path) = take_open_uri(&mut portal, uri);
        portal
            .stream
            .write_all(&request_handle(&call, &path))
            .unwrap();
        assert_eq!(opener.join().unwrap().unwrap(), Opened::Unanswered);
    }

    /// A flood of signals sent straight to the opener while its OpenURI call
    /// waits for the reply cannot push out the portal's answer (with only
    /// the 64-message queue, the answer was dropped).
    #[test]
    fn a_flood_cannot_drop_the_portal_answer() {
        let Some(bus) = PrivateBus::start("flood") else {
            return;
        };
        let t = Duration::from_secs(5);
        let uri = "https://example.org/";
        let mut portal = fake_portal(&bus.addr);
        let mut flooder = Connection::open_address(&bus.addr, t).unwrap();
        let conn = Connection::open_address(&bus.addr, t).unwrap();
        let opener = std::thread::spawn(move || open_uri_on(conn, uri, t));
        let (call, path) = take_open_uri(&mut portal, uri);
        let to = call.sender.clone().unwrap();
        let junk = vec![b'x'; 16 << 10];
        for _ in 0..200 {
            let mut body = Writer(Vec::new());
            body.u32(junk.len() as u32);
            body.0.extend_from_slice(&junk);
            let signal = message(
                SIGNAL,
                |f| {
                    f.field(1, "o", |w| w.str("/x"));
                    f.field(2, "s", |w| w.str("x.Flood"));
                    f.field(3, "s", |w| w.str("Junk"));
                    f.field(6, "s", |w| w.str(&to));
                },
                "ay",
                &body.0,
            );
            flooder.stream.write_all(&signal).unwrap();
        }
        // The answer comes before the reply to the call.
        portal.stream.write_all(&response(&path, 0)).unwrap();
        portal
            .stream
            .write_all(&request_handle(&call, &path))
            .unwrap();
        assert_eq!(opener.join().unwrap().unwrap(), Opened::Yes);
    }

    /// The portal can't be named or started beforehand (the bus refuses
    /// GetNameOwner and StartServiceByName here; the OpenURI call itself
    /// reaches it): its answer, coming before the reply under a flood, is
    /// still kept, because the request's path marks it as wanted until the
    /// reply names the portal. An impostor's answer on that path, sent
    /// straight to the opener, is not taken for it.
    #[test]
    fn an_unnamed_portal_answer_survives_a_flood() {
        let deny = |member: &str| {
            format!(
                r#"<deny send_destination="org.freedesktop.DBus" send_interface="org.freedesktop.DBus" send_member="{member}"/>"#
            )
        };
        let Some(bus) = PrivateBus::start_with(
            "unnamed",
            &(deny("GetNameOwner") + &deny("StartServiceByName")),
        ) else {
            return;
        };
        let t = Duration::from_secs(5);
        let uri = "https://example.org/";
        let mut portal = fake_portal(&bus.addr);
        // The fallback path really is taken: nobody can name the portal.
        let mut probe = Connection::open_address(&bus.addr, t).unwrap();
        assert_eq!(probe.name_owner(PORTAL), None);
        assert!(
            probe
                .call_args(
                    "org.freedesktop.DBus",
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "StartServiceByName",
                    &[Arg::Str(PORTAL), Arg::U32(0)],
                )
                .is_err()
        );

        let mut flooder = Connection::open_address(&bus.addr, t).unwrap();
        let mut impostor = Connection::open_address(&bus.addr, t).unwrap();
        let conn = Connection::open_address(&bus.addr, t).unwrap();
        let opener = std::thread::spawn(move || open_uri_on(conn, uri, t));
        let (call, path) = take_open_uri(&mut portal, uri);
        let to = call.sender.clone().unwrap();
        // The impostor answers first, on the right path.
        impostor
            .stream
            .write_all(&response_to(&path, 2, &to))
            .unwrap();
        let junk = vec![b'x'; 16 << 10];
        for _ in 0..200 {
            let mut body = Writer(Vec::new());
            body.u32(junk.len() as u32);
            body.0.extend_from_slice(&junk);
            let signal = message(
                SIGNAL,
                |f| {
                    f.field(1, "o", |w| w.str("/x"));
                    f.field(2, "s", |w| w.str("x.Flood"));
                    f.field(3, "s", |w| w.str("Junk"));
                    f.field(6, "s", |w| w.str(&to));
                },
                "ay",
                &body.0,
            );
            flooder.stream.write_all(&signal).unwrap();
        }
        // The answer comes before the reply to the call.
        portal.stream.write_all(&response(&path, 0)).unwrap();
        portal
            .stream
            .write_all(&request_handle(&call, &path))
            .unwrap();
        assert_eq!(opener.join().unwrap().unwrap(), Opened::Yes);
    }

    /// The round-4 review's case: a body signature carried as a plain
    /// string (`s`, not `g`), holding a non-ASCII character.
    #[test]
    fn header_fields_must_have_their_wire_type() {
        let mut f = Writer(vec![0; 16]);
        f.field(5, "u", |w| w.u32(3));
        f.field(8, "s", |w| w.str("é"));
        let n = (f.0.len() - 16) as u32;
        f.0[0] = b'l';
        f.0[1] = METHOD_RETURN;
        f.0[3] = 1;
        f.0[8..12].copy_from_slice(&9u32.to_le_bytes());
        f.0[12..16].copy_from_slice(&n.to_le_bytes());
        f.align(8);
        assert!(decode(&f.0).is_err());

        // Duplicate known fields are rejected too.
        let dup = message(
            METHOD_RETURN,
            |f| {
                f.field(5, "u", |w| w.u32(3));
                f.field(5, "u", |w| w.u32(4));
            },
            "",
            &[],
        );
        assert!(decode(&dup).is_err());
        // A path must be an object path.
        let bad = message(SIGNAL, |f| f.field(1, "s", |w| w.str("/x")), "", &[]);
        assert!(decode(&bad).is_err());
    }

    #[test]
    fn string_dicts_encode_per_spec() {
        let bytes = encode_call(
            1,
            "d",
            "/",
            "i",
            "m",
            &[Arg::StrDict(&[("handle_token", "t1"), ("k", "v")])],
        );
        let msg = decode(&bytes).unwrap();
        let Value::Array(items) = &msg.body[0] else {
            panic!()
        };
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0],
            Value::Struct(vec![
                Value::Str("handle_token".into()),
                Value::Variant(Box::new(Value::Str("t1".into())))
            ])
        );
    }

    /// Seeded mutation fuzzing: no input may make the decoder panic.
    #[test]
    fn decoder_never_panics_on_mutated_messages() {
        let mut seeds = vec![
            encode_call(
                7,
                "org.example",
                "/a/b",
                "org.example.I",
                "Do",
                &[Arg::Str("x"), Arg::Str("yz")],
            ),
            encode_call(
                1,
                "d",
                "/",
                "i",
                "m",
                &[Arg::Str("a"), Arg::StrDict(&[("k", "v")])],
            ),
            variant("(ddd)", |w| {
                w.align(8);
                for c in [0.25f64, 0.5, 1.0] {
                    w.0.extend_from_slice(&c.to_le_bytes());
                }
            }),
            variant("s", |w| w.str("Europe/Paris")),
            settings_signal(PORTAL_PATH, 1),
        ];
        let mut ay = Writer(Vec::new());
        ay.u32(5);
        ay.0.extend_from_slice(&[1, 2, 3, 4, 5]);
        seeds.push(reply("ay", ay.0));
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut rnd = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for i in 0..200_000u32 {
            let mut m = seeds[i as usize % seeds.len()].clone();
            for _ in 0..1 + rnd() % 4 {
                if m.is_empty() {
                    break;
                }
                let at = (rnd() as usize) % m.len();
                match rnd() % 6 {
                    0 => m[at] ^= 1 << (rnd() % 8),
                    1 => m[at] = rnd() as u8,
                    2 => m.truncate(at),
                    3 => m.insert(at, rnd() as u8),
                    4 => {
                        m.remove(at);
                    }
                    _ => {
                        // Bend a length/count word.
                        let at = at & !3;
                        if at + 4 <= m.len() {
                            let v = (rnd() % 600) as u32;
                            m[at..at + 4].copy_from_slice(&v.to_le_bytes());
                        }
                    }
                }
            }
            let r = std::panic::catch_unwind(|| decode(&m));
            assert!(r.is_ok(), "decoder panicked on case {i}: {m:?}");
        }
    }

    /// Structure-aware fuzzing: well-framed messages whose header fields
    /// have random codes, (often wrong) wire types and hostile contents.
    #[test]
    fn decoder_never_panics_on_odd_headers() {
        const TEXTS: [&str; 14] = [
            "", "é", "aé", "a{sv}", "(", "ss", "/x", "s", "ay", "v", "(ddd)", "a{", "é(", "\u{7f}",
        ];
        const TYPES: [&str; 8] = ["o", "s", "g", "u", "y", "v", "b", "x"];
        let mut x: u64 = 0xD1B5_4A32_D192_ED03;
        let mut rnd = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for i in 0..100_000u32 {
            let mut f = Writer(vec![0; 16]);
            for _ in 0..rnd() % 6 {
                let code = (rnd() % 11) as u8;
                let ty = TYPES[(rnd() as usize) % TYPES.len()];
                let text = TEXTS[(rnd() as usize) % TEXTS.len()];
                f.field(code, ty, |w| match ty {
                    "o" | "s" => w.str(text),
                    "g" => w.sig(text),
                    "u" | "b" => w.u32(rnd() as u32 % 3),
                    "y" => w.u8(rnd() as u8),
                    "x" => {
                        w.align(8);
                        w.0.extend_from_slice(&rnd().to_le_bytes());
                    }
                    _ => {
                        w.sig("u");
                        w.u32(rnd() as u32);
                    }
                });
            }
            let n = (f.0.len() - 16) as u32;
            f.0[0] = if rnd() % 8 == 0 { b'B' } else { b'l' };
            f.0[1] = (rnd() % 5) as u8;
            f.0[3] = 1;
            let body_len = (rnd() % 48) as usize;
            f.0[4..8].copy_from_slice(&(body_len as u32).to_le_bytes());
            f.0[8..12].copy_from_slice(&i.to_le_bytes());
            f.0[12..16].copy_from_slice(&n.to_le_bytes());
            f.align(8);
            for _ in 0..body_len {
                f.0.push(rnd() as u8);
            }
            let m = f.0;
            let r = std::panic::catch_unwind(|| decode(&m));
            assert!(r.is_ok(), "decoder panicked on case {i}: {m:?}");
        }
    }

    #[test]
    fn addresses_unescape() {
        assert_eq!(unescape("/run/user/1000/bus"), "/run/user/1000/bus");
        assert_eq!(unescape("/tmp/a%20b"), "/tmp/a b");
    }
}
