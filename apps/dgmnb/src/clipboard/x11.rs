//! X11 clipboard (the CLIPBOARD selection). Copying goes through
//! x11-clipboard, which keeps its own X connections and serving thread
//! (dropping it stops both). Pasting uses our own bounded reader on its
//! "getter" connection: it asks which formats the owner offers, learns each
//! size before fetching, treats INCR size hints as untrusted, caps the total
//! and gives up after a deadline. It runs on a short-lived worker, so the UI
//! waits at most `PASTE_TIMEOUT` whatever the owner or the X server does.

use std::os::fd::AsRawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use x11_clipboard::{Atom, Context, RustConnection};
use x11rb::connection::Connection as _;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, Property};
use x11rb::wrapper::ConnectionExt as _;

use super::MAX_PASTE;

/// How long one paste may take in total.
const PASTE_TIMEOUT: Duration = Duration::from_secs(1);
/// Largest TARGETS list we'll read.
const MAX_TARGETS: usize = 64 * 1024;
/// Most queued events discarded before a conversion.
const MAX_DRAIN: usize = 1024;

pub struct Clipboard {
    inner: Arc<x11_clipboard::Clipboard>,
    png: Atom,
    reader: Arc<Reader>,
    /// A paste is still running: only one uses the reader at a time.
    busy: Arc<AtomicBool>,
}

impl Clipboard {
    pub fn new() -> Option<Clipboard> {
        let inner = x11_clipboard::Clipboard::new().ok()?;
        let png = inner.setter.get_atom("image/png").ok()?;
        let reader = Reader::new(&inner.getter)?;
        Some(Clipboard {
            inner: Arc::new(inner),
            png,
            reader: Arc::new(reader),
            busy: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn copy_text(&self, text: &str) {
        let a = &self.inner.setter.atoms;
        let _ = self
            .inner
            .store(a.clipboard, a.utf8_string, text.as_bytes());
    }

    pub fn copy_png(&self, png: Vec<u8>) {
        let a = &self.inner.setter.atoms;
        let _ = self.inner.store(a.clipboard, self.png, png);
    }

    pub fn paste_text(&self) -> Option<String> {
        if self.busy.swap(true, Ordering::AcqRel) {
            return None; // the last paste is still stuck
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let (inner, reader, busy) = (self.inner.clone(), self.reader.clone(), self.busy.clone());
        let spawned = std::thread::Builder::new()
            .name("x11-paste".into())
            .spawn(move || {
                let text = read_text(&inner.getter, &reader, PASTE_TIMEOUT);
                busy.store(false, Ordering::Release);
                let _ = tx.send(text);
            });
        if spawned.is_err() {
            self.busy.store(false, Ordering::Release);
            return None;
        }
        rx.recv_timeout(PASTE_TIMEOUT + Duration::from_millis(100))
            .ok()
            .flatten()
    }
}

/// What the reader needs from the X server: the text formats we ask for,
/// best first, and a ring of properties to receive into. Each conversion
/// uses the next one, so an owner still writing into an abandoned
/// transfer's property can't land in a later paste.
struct Reader {
    utf8: Atom,
    plain: Atom,
    text: Atom,
    string: Atom,
    props: [Atom; 4],
    next: AtomicUsize,
}

impl Reader {
    fn new(cx: &Context) -> Option<Reader> {
        let mut props = [0; 4];
        for (i, p) in props.iter_mut().enumerate() {
            *p = cx.get_atom(&format!("DGMNB_PASTE_{i}")).ok()?;
        }
        Some(Reader {
            utf8: cx.atoms.utf8_string,
            plain: cx.get_atom("text/plain;charset=utf-8").ok()?,
            text: cx.get_atom("TEXT").ok()?,
            string: cx.atoms.string,
            props,
            next: AtomicUsize::new(0),
        })
    }

    /// The property for the next conversion.
    fn prop(&self) -> Atom {
        self.props[self.next.fetch_add(1, Ordering::Relaxed) % self.props.len()]
    }

    fn decode(&self, kind: Atom, data: &[u8]) -> String {
        let latin1 = |d: &[u8]| d.iter().map(|&b| char::from(b)).collect();
        if kind == self.string {
            latin1(data) // STRING is ISO 8859-1
        } else if kind == self.utf8 || kind == self.plain {
            String::from_utf8_lossy(data).into_owned()
        } else {
            // TEXT lets the owner pick; anything that isn't UTF-8 is most
            // likely Latin-1 (or ASCII-only COMPOUND_TEXT).
            std::str::from_utf8(data).map_or_else(|_| latin1(data), str::to_string)
        }
    }
}

fn read_text(cx: &Context, r: &Reader, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    let clipboard = cx.atoms.clipboard;
    // Which formats does the owner offer? (Some owners don't answer
    // TARGETS; then just try each in turn.)
    let offered: Option<Vec<Atom>> =
        fetch(cx, r, clipboard, cx.atoms.targets, deadline, MAX_TARGETS)
            .filter(|f| f.format == 32)
            .map(|f| {
                let (words, _) = f.data.as_chunks::<4>();
                words.iter().map(|w| u32::from_ne_bytes(*w)).collect()
            });
    for target in [r.utf8, r.plain, r.text, r.string] {
        if offered.as_ref().is_some_and(|o| !o.contains(&target)) {
            continue;
        }
        if let Some(f) = fetch(cx, r, clipboard, target, deadline, MAX_PASTE)
            && f.format == 8
        {
            return Some(r.decode(f.kind, &f.data).replace("\r\n", "\n"));
        }
        if Instant::now() >= deadline {
            break;
        }
    }
    None
}

struct Fetched {
    kind: Atom,
    format: u8,
    data: Vec<u8>,
}

/// Convert `selection` to `target` and read the result, refusing anything
/// larger than `max` bytes before allocating for it.
fn fetch(
    cx: &Context,
    r: &Reader,
    selection: Atom,
    target: Atom,
    deadline: Instant,
    max: usize,
) -> Option<Fetched> {
    let (c, win, prop) = (&cx.connection, cx.window, r.prop());
    // Start from an empty property and an empty event queue.
    c.delete_property(win, prop).ok()?;
    c.sync().ok()?;
    drain(c, deadline)?;
    c.convert_selection(win, selection, target, prop, x11rb::CURRENT_TIME)
        .ok()?;
    c.flush().ok()?;
    loop {
        if let Event::SelectionNotify(e) = next_event(c, deadline)?
            && e.requestor == win
            && e.selection == selection
            && e.target == target
        {
            if e.property == prop {
                break;
            }
            if e.property == x11rb::NONE {
                return None; // refused
            }
            // Otherwise it answers an earlier request: not ours.
        }
    }
    // Learn the type and size without transferring anything.
    let head = c
        .get_property(false, win, prop, AtomEnum::ANY, 0, 0)
        .ok()?
        .reply()
        .ok()?;
    if head.type_ == cx.atoms.incr {
        return fetch_incr(cx, prop, deadline, max);
    }
    let size = head.bytes_after as usize;
    if size > max {
        let _ = c.delete_property(win, prop);
        let _ = c.flush();
        return None;
    }
    let reply = c
        .get_property(true, win, prop, AtomEnum::ANY, 0, size.div_ceil(4) as u32)
        .ok()?
        .reply()
        .ok()?;
    // It grew between the two reads: someone is still writing into it.
    if reply.bytes_after != 0 {
        return None;
    }
    Some(Fetched {
        kind: reply.type_,
        format: reply.format,
        data: reply.value,
    })
}

/// The INCR protocol: the owner sends chunks as property updates and an
/// empty chunk ends the transfer. Its advertised total is ignored, and every
/// chunk must have the first one's type and format.
fn fetch_incr(cx: &Context, prop: Atom, deadline: Instant, max: usize) -> Option<Fetched> {
    let (c, win) = (&cx.connection, cx.window);
    // Deleting the INCR property tells the owner to start.
    c.delete_property(win, prop).ok()?;
    c.flush().ok()?;
    let mut out: Option<Fetched> = None;
    loop {
        match next_event(c, deadline)? {
            Event::PropertyNotify(e)
                if e.window == win && e.atom == prop && e.state == Property::NEW_VALUE => {}
            _ => continue,
        }
        let head = c
            .get_property(false, win, prop, AtomEnum::ANY, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        let chunk = head.bytes_after as usize;
        let have = out.as_ref().map_or(0, |o| o.data.len());
        if have.saturating_add(chunk) > max {
            // Leave the chunk there: deleting it would only ask for more.
            return None;
        }
        let reply = c
            .get_property(true, win, prop, AtomEnum::ANY, 0, chunk.div_ceil(4) as u32)
            .ok()?
            .reply()
            .ok()?;
        c.flush().ok()?;
        if reply.bytes_after != 0 {
            return None;
        }
        if reply.value.is_empty() {
            return out.or(Some(Fetched {
                kind: reply.type_,
                format: 8,
                data: Vec::new(),
            }));
        }
        match &mut out {
            None => {
                out = Some(Fetched {
                    kind: reply.type_,
                    format: reply.format,
                    data: reply.value,
                })
            }
            Some(o) if o.kind == reply.type_ && o.format == reply.format => {
                o.data.extend_from_slice(&reply.value)
            }
            Some(_) => return None, // the owner changed format mid-transfer
        }
    }
}

/// Discard queued events: a bounded number, and never past `deadline`.
fn drain(c: &RustConnection, deadline: Instant) -> Option<()> {
    for _ in 0..MAX_DRAIN {
        if Instant::now() >= deadline {
            return None;
        }
        if c.poll_for_event().ok()?.is_none() {
            break;
        }
    }
    Some(())
}

/// The next X event, or `None` once `deadline` passes, even while events
/// keep arriving.
fn next_event(c: &RustConnection, deadline: Instant) -> Option<Event> {
    loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|l| !l.is_zero())?;
        if let Some(e) = c.poll_for_event().ok()? {
            return Some(e);
        }
        let mut fd = libc::pollfd {
            fd: c.stream().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = left.as_millis().clamp(1, i32::MAX as u128) as i32;
        // SAFETY: one valid pollfd for the duration of the call.
        unsafe { libc::poll(&mut fd, 1, ms) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc;
    use std::thread::{self, JoinHandle};
    use x11rb::connection::RequestConnection as _;
    use x11rb::protocol::xproto::{
        ChangeWindowAttributesAux, CreateWindowAux, EventMask, PropMode, SELECTION_NOTIFY_EVENT,
        SelectionNotifyEvent, WindowClass,
    };

    /// A private X server for the test.
    struct Xvfb {
        child: Child,
        display: String,
    }

    impl Xvfb {
        fn start() -> Option<Xvfb> {
            let child = Command::new("Xvfb")
                .args([
                    "-displayfd",
                    "1",
                    "-nolisten",
                    "tcp",
                    "-screen",
                    "0",
                    "16x16x24",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .ok()?;
            let mut x = Xvfb {
                child,
                display: String::new(),
            };
            let out = x.child.stdout.take()?;
            BufReader::new(out).read_line(&mut x.display).ok()?;
            x.display = format!(":{}", x.display.trim());
            (x.display.len() > 1).then_some(x)
        }
    }

    impl Drop for Xvfb {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// What a test owner offers.
    struct Offer {
        /// The TARGETS answer, or `None` for an owner that refuses it.
        targets: Option<&'static [&'static str]>,
        /// target → (property type, bytes)
        serves: Vec<(&'static str, &'static str, Vec<u8>)>,
        /// Send through INCR: (advertised size, chunk size).
        incr: Option<(u32, usize)>,
    }

    /// Own CLIPBOARD on `display` and answer requests until the transfer
    /// stalls or the selection is taken. Yields the INCR chunks the reader
    /// took.
    fn own(display: &str, offer: Offer) -> JoinHandle<usize> {
        let (ready, wait) = mpsc::channel();
        let display = display.to_string();
        let owner = thread::spawn(move || {
            let (c, screen) = RustConnection::connect(Some(&display)).unwrap();
            c.maximum_request_bytes(); // allow big properties
            let root = c.setup().roots[screen].root;
            let win = c.generate_id().unwrap();
            c.create_window(
                0,
                win,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_ONLY,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap();
            let atom = |n: &str| {
                c.intern_atom(false, n.as_bytes())
                    .unwrap()
                    .reply()
                    .unwrap()
                    .atom
            };
            let clipboard = atom("CLIPBOARD");
            let targets = atom("TARGETS");
            let incr = atom("INCR");
            let offered: Option<Vec<u32>> =
                offer.targets.map(|ts| ts.iter().map(|t| atom(t)).collect());
            let serves: Vec<(u32, u32, Vec<u8>)> = offer
                .serves
                .into_iter()
                .map(|(t, k, d)| (atom(t), atom(k), d))
                .collect();
            c.set_selection_owner(win, clipboard, x11rb::CURRENT_TIME)
                .unwrap();
            c.get_selection_owner(clipboard).unwrap().reply().unwrap();
            ready.send(()).unwrap();
            let idle = || Instant::now() + Duration::from_millis(500);
            let mut taken = 0;
            while let Some(ev) = next_event(&c, idle()) {
                let r = match ev {
                    Event::SelectionRequest(r) => r,
                    Event::SelectionClear(_) => return taken,
                    _ => continue,
                };
                let notify = |property: u32| {
                    let e = SelectionNotifyEvent {
                        response_type: SELECTION_NOTIFY_EVENT,
                        sequence: 0,
                        time: r.time,
                        requestor: r.requestor,
                        selection: r.selection,
                        target: r.target,
                        property,
                    };
                    c.send_event(false, r.requestor, EventMask::NO_EVENT, e)
                        .unwrap();
                    c.flush().unwrap();
                };
                if r.target == targets {
                    match &offered {
                        Some(list) => {
                            c.change_property32(
                                PropMode::REPLACE,
                                r.requestor,
                                r.property,
                                AtomEnum::ATOM,
                                list,
                            )
                            .unwrap();
                            notify(r.property);
                        }
                        None => notify(x11rb::NONE),
                    }
                    continue;
                }
                let Some((_, kind, data)) = serves.iter().find(|s| s.0 == r.target) else {
                    notify(x11rb::NONE);
                    continue;
                };
                let Some((advertised, size)) = offer.incr else {
                    c.change_property8(PropMode::REPLACE, r.requestor, r.property, *kind, data)
                        .unwrap();
                    notify(r.property);
                    continue;
                };
                let watch = ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE);
                c.change_window_attributes(r.requestor, &watch).unwrap();
                c.change_property32(
                    PropMode::REPLACE,
                    r.requestor,
                    r.property,
                    incr,
                    &[advertised],
                )
                .unwrap();
                notify(r.property);
                for chunk in data.chunks(size).chain([&[][..]]) {
                    // Wait for the reader to take the previous piece.
                    loop {
                        match next_event(&c, idle()) {
                            Some(Event::PropertyNotify(e))
                                if e.window == r.requestor
                                    && e.atom == r.property
                                    && e.state == Property::DELETE =>
                            {
                                break;
                            }
                            Some(_) => {}
                            None => return taken,
                        }
                    }
                    taken += 1;
                    c.change_property8(PropMode::REPLACE, r.requestor, r.property, *kind, chunk)
                        .unwrap();
                    c.flush().unwrap();
                }
            }
            taken
        });
        wait.recv().unwrap();
        owner
    }

    /// R5-M-01: an owner that never answers while property changes keep
    /// hitting our window (spam, or an abandoned transfer) can't stretch the
    /// deadline. Needs `Xvfb`; skipped otherwise.
    #[test]
    fn a_flood_of_events_cannot_stretch_the_deadline() {
        let Some(x) = Xvfb::start() else {
            eprintln!("no Xvfb; skipped");
            return;
        };
        let cx = Context::new(Some(&x.display)).unwrap();
        let r = Reader::new(&cx).unwrap();
        let (owning, owned) = mpsc::channel();
        let spammer = {
            let (display, ours) = (x.display.clone(), cx.window);
            thread::spawn(move || {
                let (c, screen) = RustConnection::connect(Some(&display)).unwrap();
                let root = c.setup().roots[screen].root;
                let win = c.generate_id().unwrap();
                c.create_window(
                    0,
                    win,
                    root,
                    0,
                    0,
                    1,
                    1,
                    0,
                    WindowClass::INPUT_ONLY,
                    0,
                    &CreateWindowAux::new(),
                )
                .unwrap();
                let atom = |n: &str| {
                    c.intern_atom(false, n.as_bytes())
                        .unwrap()
                        .reply()
                        .unwrap()
                        .atom
                };
                c.set_selection_owner(win, atom("CLIPBOARD"), x11rb::CURRENT_TIME)
                    .unwrap();
                c.get_selection_owner(atom("CLIPBOARD"))
                    .unwrap()
                    .reply()
                    .unwrap();
                owning.send(()).unwrap();
                // Never answer; keep touching the reader's window for a while.
                let (junk, until) = (atom("JUNK"), Instant::now() + Duration::from_millis(1200));
                while Instant::now() < until {
                    let _ =
                        c.change_property8(PropMode::REPLACE, ours, junk, AtomEnum::STRING, b"x");
                    let _ = c.flush();
                }
            })
        };
        owned.recv().unwrap();
        let start = Instant::now();
        assert_eq!(read_text(&cx, &r, Duration::from_millis(300)), None);
        let took = start.elapsed();
        spammer.join().unwrap();
        assert!(took < Duration::from_millis(900), "paste took {took:?}");

        // The reason it holds even when the queue never empties: with events
        // waiting and the deadline gone, nothing more is consumed.
        let (c, _) = RustConnection::connect(Some(&x.display)).unwrap();
        let junk = c.intern_atom(false, b"JUNK").unwrap().reply().unwrap().atom;
        for _ in 0..50 {
            c.change_property8(PropMode::REPLACE, cx.window, junk, AtomEnum::STRING, b"y")
                .unwrap();
        }
        c.sync().unwrap();
        cx.connection.sync().unwrap();
        let gone = Instant::now() - Duration::from_millis(1);
        assert!(next_event(&cx.connection, gone).is_none());
        assert!(drain(&cx.connection, gone).is_none());
        assert!(cx.connection.poll_for_event().unwrap().is_some(), "events were waiting");
    }

    /// Needs `Xvfb` on PATH; skipped otherwise.
    #[test]
    fn paste_negotiates_formats_and_stays_bounded() {
        let Some(x) = Xvfb::start() else {
            eprintln!("no Xvfb; skipped");
            return;
        };
        let cx = Context::new(Some(&x.display)).unwrap();
        let t = Reader::new(&cx).unwrap();
        let paste_counting = |offer: Offer| {
            let owner = own(&x.display, offer);
            let start = Instant::now();
            let text = read_text(&cx, &t, Duration::from_secs(2));
            assert!(start.elapsed() < Duration::from_millis(2500));
            (text, owner.join().unwrap())
        };
        let paste = |offer| paste_counting(offer).0;
        let plain = |targets, serves| Offer {
            targets,
            serves,
            incr: None,
        };

        // UTF8_STRING wins over STRING.
        let both = plain(
            Some(&["TARGETS", "STRING", "UTF8_STRING"]),
            vec![
                ("STRING", "STRING", b"latin".to_vec()),
                ("UTF8_STRING", "UTF8_STRING", "√2×3".into()),
            ],
        );
        assert_eq!(paste(both).as_deref(), Some("√2×3"));
        // STRING alone is Latin-1.
        let latin = plain(
            Some(&["TARGETS", "STRING"]),
            vec![("STRING", "STRING", b"caf\xe9 1\r\n2".to_vec())],
        );
        assert_eq!(paste(latin).as_deref(), Some("café 1\n2"));
        // TEXT, answered as UTF-8.
        let text = plain(
            Some(&["TARGETS", "TEXT"]),
            vec![("TEXT", "UTF8_STRING", "½".into())],
        );
        assert_eq!(paste(text).as_deref(), Some("½"));
        // An owner that won't say what it has still gets asked for STRING.
        let quiet = plain(None, vec![("STRING", "STRING", b"42".to_vec())]);
        assert_eq!(paste(quiet).as_deref(), Some("42"));
        // Oversized in one piece: refused without fetching it.
        let big = plain(
            Some(&["TARGETS", "UTF8_STRING"]),
            vec![("UTF8_STRING", "UTF8_STRING", vec![b'7'; 2 << 20])],
        );
        assert_eq!(paste(big), None);

        let incr = |advertised, data: Vec<u8>| Offer {
            targets: Some(&["TARGETS", "UTF8_STRING"]),
            serves: vec![("UTF8_STRING", "UTF8_STRING", data)],
            incr: Some((advertised, 64 << 10)),
        };
        // INCR within the cap, whatever size it claims.
        let fine = incr(u32::MAX, b"1234".repeat(40_000));
        assert_eq!(paste(fine).map(|s| s.len()), Some(160_000));
        // INCR past the cap: refused as soon as the total would pass it
        // (16 chunks fill 1 MiB; the 17th is looked at, not read).
        let flood = incr(1, vec![b'9'; MAX_PASTE * 3]);
        let (text, taken) = paste_counting(flood);
        assert_eq!(text, None);
        assert_eq!(taken, 17);
    }
}
