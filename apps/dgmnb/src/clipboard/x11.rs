//! X11 clipboard (the CLIPBOARD selection). Copying goes through
//! x11-clipboard, which keeps its own X connections and serving thread
//! (dropping it stops both). Pasting uses the shared bounded reader
//! (`x11paste`) on its "getter" connection: it asks which formats the owner
//! offers, learns each size before fetching, treats INCR size hints as
//! untrusted, refuses anything past `MAX_PASTE` and gives up after a
//! deadline. It runs on a short-lived worker, so the UI waits at most
//! `PASTE_TIMEOUT` whatever the owner or the X server does.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use x11_clipboard::{Atom, Context};
use x11paste::{Atoms as Reader, Limits, Overflow, Selection};

use super::MAX_PASTE;

/// How long one paste may take in total.
const PASTE_TIMEOUT: Duration = Duration::from_secs(1);

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
        let reader = Reader::new(&inner.getter.connection)?;
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

/// The CLIPBOARD's text, refused (not cut) past `MAX_PASTE`.
fn read_text(cx: &Context, r: &Reader, timeout: Duration) -> Option<String> {
    let limits = Limits {
        max_bytes: MAX_PASTE,
        timeout,
        overflow: Overflow::Refuse,
    };
    x11paste::read_text(&cx.connection, cx.screen, r, Selection::Clipboard, &limits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc;
    use std::thread::{self, JoinHandle};
    use std::time::Instant;
    use x11_clipboard::RustConnection;
    use x11paste::{drain, next_event};
    use x11rb::connection::{Connection as _, RequestConnection as _};
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{
        AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, CreateWindowAux, EventMask,
        PropMode, Property, SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, WindowClass,
    };
    use x11rb::wrapper::ConnectionExt as _;

    /// A private X server for the test.
    struct Xvfb {
        child: Child,
        display: String,
    }

    impl Xvfb {
        fn start() -> Option<Xvfb> {
            let child = Command::new("Xvfb")
                // -noreset: a server that resets when its last client
                // goes would drop the next test connection mid-setup.
                .args([
                    "-displayfd",
                    "1",
                    "-noreset",
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

    /// A misbehaving owner: answers UTF8_STRING with INCR, then keeps writing
    /// "stale" into the requestor's property for a second, whatever the
    /// requestor does. Yields the window it was writing to.
    fn keep_writing(display: &str) -> JoinHandle<Option<u32>> {
        let (ready, wait) = mpsc::channel();
        let display = display.to_string();
        let owner = thread::spawn(move || {
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
            let (clipboard, targets, utf8, incr) = (
                atom("CLIPBOARD"),
                atom("TARGETS"),
                atom("UTF8_STRING"),
                atom("INCR"),
            );
            c.set_selection_owner(win, clipboard, x11rb::CURRENT_TIME)
                .unwrap();
            c.get_selection_owner(clipboard).unwrap().reply().unwrap();
            ready.send(()).unwrap();
            let until = Instant::now() + Duration::from_secs(1);
            let mut victim = None;
            while Instant::now() < until {
                while let Ok(Some(ev)) = c.poll_for_event() {
                    let Event::SelectionRequest(r) = ev else {
                        continue;
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
                        let _ = c.send_event(false, r.requestor, EventMask::NO_EVENT, e);
                    };
                    if r.target == targets {
                        let _ = c.change_property32(
                            PropMode::REPLACE,
                            r.requestor,
                            r.property,
                            AtomEnum::ATOM,
                            &[targets, utf8],
                        );
                        notify(r.property);
                    } else if r.target == utf8 && victim.is_none() {
                        let _ = c.change_property32(
                            PropMode::REPLACE,
                            r.requestor,
                            r.property,
                            incr,
                            &[1],
                        );
                        notify(r.property);
                        victim = Some((r.requestor, r.property));
                    }
                }
                if let Some((w, p)) = victim {
                    let _ = c.change_property8(PropMode::REPLACE, w, p, utf8, b"stale");
                }
                let _ = c.flush();
                thread::sleep(Duration::from_micros(100));
            }
            victim.map(|(w, _)| w)
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
        let r = Reader::new(&cx.connection).unwrap();
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
        assert!(
            cx.connection.poll_for_event().unwrap().is_some(),
            "events were waiting"
        );
    }

    /// Needs `Xvfb` on PATH; skipped otherwise.
    #[test]
    fn paste_negotiates_formats_and_stays_bounded() {
        let Some(x) = Xvfb::start() else {
            eprintln!("no Xvfb; skipped");
            return;
        };
        let cx = Context::new(Some(&x.display)).unwrap();
        let t = Reader::new(&cx.connection).unwrap();
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
        // Plain `text/plain` alone (R13-L-02): UTF-8 when it is, else
        // Latin-1; and it's taken over STRING.
        let unlabelled = plain(
            Some(&["TARGETS", "STRING", "text/plain"]),
            vec![
                ("STRING", "STRING", b"latin".to_vec()),
                ("text/plain", "text/plain", "sin(x)·½".into()),
            ],
        );
        assert_eq!(paste(unlabelled).as_deref(), Some("sin(x)·½"));
        let unlabelled = plain(
            Some(&["TARGETS", "text/plain"]),
            vec![("text/plain", "text/plain", b"caf\xe9".to_vec())],
        );
        assert_eq!(paste(unlabelled).as_deref(), Some("café"));
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

        // R6-M-04: an owner that keeps writing into the property it was
        // given for an abandoned transfer can't land in the pastes that
        // follow, however many there are (the old four-property ring
        // wrapped after four).
        let stubborn = keep_writing(&x.display);
        // (Whatever it answers this paste with is its own business.)
        let _ = read_text(&cx, &t, Duration::from_millis(200));
        for i in 0..6 {
            let fresh = format!("fresh {i}");
            let owner = own(
                &x.display,
                plain(
                    Some(&["TARGETS", "UTF8_STRING"]),
                    vec![("UTF8_STRING", "UTF8_STRING", fresh.clone().into_bytes())],
                ),
            );
            let got = read_text(&cx, &t, Duration::from_secs(2));
            owner.join().unwrap();
            assert_eq!(got.as_deref(), Some(fresh.as_str()));
        }
        // The window it was told to write to is gone: nothing it writes can
        // reach a later conversion.
        let target = stubborn.join().unwrap().expect("it was asked for text");
        let window = cx.connection.get_window_attributes(target).unwrap().reply();
        assert!(
            window.is_err(),
            "the abandoned conversion's window still exists"
        );
    }
}
