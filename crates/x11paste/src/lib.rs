//! Bounded text reads of X11 selections: the clipboard, the primary
//! selection and the data of a drag being dropped.
//!
//! [`read_text`] asks which formats the selection's owner offers, learns
//! each size before fetching, treats INCR size hints as untrusted, caps the
//! total and gives up after a deadline, whatever the owner or other clients
//! do. Text past the cap is refused or cut ([`Overflow`]); either way no more
//! than the cap (rounded up to whole 32-bit words) is ever transferred to
//! this client or allocated for it: every property read asks the X server
//! for at most what is still allowed.
//!
//! An incremental (INCR) transfer stopped early is abandoned by
//! [`read_text`], its window destroyed. Some owners serve one transfer at a
//! time and then wait for that window forever (xclip does), so
//! [`read_text_on`], which has a connection of its own, lets the rest of a
//! transfer it cut go by instead: in the background, until the deadline,
//! it deletes each remaining piece without reading it, as if read, so the
//! owner gets to the end.
//!
//! Reads block until they have the text or the deadline passes: make them
//! off the UI thread.

use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use x11rb::connection::Connection as _;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, Property, Window, WindowClass,
};
pub use x11rb::rust_connection::RustConnection;

/// Largest TARGETS list read.
const MAX_TARGETS: usize = 64 * 1024;
/// Most queued events discarded before a conversion.
const MAX_DRAIN: usize = 1024;

/// The selection to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    /// CLIPBOARD: what was last copied.
    Clipboard,
    /// PRIMARY: the text last selected (middle-click paste).
    Primary,
    /// XdndSelection: the data of the drag being dropped.
    Drag,
}

/// What a read does with more text than its limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    /// Nothing is read: the read fails as soon as the size is known to be
    /// over.
    Refuse,
    /// The first `max_bytes` are read, the rest never fetched; the text is
    /// cut to `max_bytes` bytes without a character cut in two.
    Cut,
}

/// How much a read takes, and for how long.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Most bytes read of the selection's data, and of the text returned
    /// when cutting.
    pub max_bytes: usize,
    /// How long the whole read may take.
    pub timeout: Duration,
    pub overflow: Overflow,
}

/// The atoms reads use, interned once per connection.
pub struct Atoms {
    clipboard: Atom,
    xdnd_selection: Atom,
    targets: Atom,
    incr: Atom,
    /// The property each conversion is written to.
    property: Atom,
    utf8: Atom,
    /// `text/plain;charset=utf-8`.
    plain_utf8: Atom,
    /// `text/plain`, no charset given.
    plain: Atom,
    text: Atom,
}

impl Atoms {
    pub fn new(c: &RustConnection) -> Option<Atoms> {
        const NAMES: [&str; 9] = [
            "CLIPBOARD",
            "XdndSelection",
            "TARGETS",
            "INCR",
            "X11PASTE_DATA",
            "UTF8_STRING",
            "text/plain;charset=utf-8",
            "text/plain",
            "TEXT",
        ];
        // Every request, then every reply: one round trip.
        let cookies = NAMES.map(|name| c.intern_atom(false, name.as_bytes()));
        let mut atoms = [0; NAMES.len()];
        for (atom, cookie) in atoms.iter_mut().zip(cookies) {
            *atom = cookie.ok()?.reply().ok()?.atom;
        }
        let [
            clipboard,
            xdnd_selection,
            targets,
            incr,
            property,
            utf8,
            plain_utf8,
            plain,
            text,
        ] = atoms;
        Some(Atoms {
            clipboard,
            xdnd_selection,
            targets,
            incr,
            property,
            utf8,
            plain_utf8,
            plain,
            text,
        })
    }

    fn selection(&self, selection: Selection) -> Atom {
        match selection {
            Selection::Clipboard => self.clipboard,
            Selection::Primary => AtomEnum::PRIMARY.into(),
            Selection::Drag => self.xdnd_selection,
        }
    }

    /// The text formats asked for, best first: those that say they're
    /// UTF-8, then those that don't say (some owners offer only
    /// `text/plain`), then Latin-1.
    fn formats(&self) -> [Atom; 5] {
        [
            self.utf8,
            self.plain_utf8,
            self.plain,
            self.text,
            AtomEnum::STRING.into(),
        ]
    }

    fn decode(&self, kind: Atom, data: &[u8]) -> String {
        let latin1 = |d: &[u8]| d.iter().map(|&b| char::from(b)).collect();
        if kind == u32::from(AtomEnum::STRING) {
            latin1(data) // STRING is ISO 8859-1
        } else if kind == self.utf8 || kind == self.plain_utf8 {
            String::from_utf8_lossy(data).into_owned()
        } else {
            // TEXT lets the owner pick, and `text/plain` names no charset:
            // anything that isn't UTF-8 is most likely Latin-1 (or
            // ASCII-only COMPOUND_TEXT). For ASCII, as nearly all of it is,
            // that's GTK's reading of `text/plain` too (GTK 4.22 converts
            // it from ASCII, with escapes for other bytes).
            std::str::from_utf8(data).map_or_else(|_| latin1(data), str::to_string)
        }
    }
}

/// The text in `selection`, read on `c` (whose default screen is `screen`)
/// within `limits`; `None` if there is none, it couldn't be read in time, or
/// it was too long to [`Overflow::Refuse`]. CRLF line ends become LF.
///
/// `c` can be shared with other work: events queued on it may be discarded
/// (a bounded number of them).
pub fn read_text(
    c: &RustConnection,
    screen: usize,
    atoms: &Atoms,
    selection: Selection,
    limits: &Limits,
) -> Option<String> {
    read(c, screen, atoms, selection, limits, false).map(|(text, _)| text)
}

/// [`read_text`] on a connection of its own to `display` (an X display name
/// such as `:0`). Only the read is bounded by the deadline, not connecting:
/// a local X server answers that at once (or isn't running). It returns as
/// soon as it has the text; the rest of an INCR transfer it cut is let go
/// by on a thread of its own, within a deadline as long again, before the
/// connection closes.
pub fn read_text_on(
    display: &str,
    selection: Selection,
    limits: &Limits,
) -> Result<Option<String>, NoConnection> {
    let start = Instant::now();
    let (c, screen) = RustConnection::connect(Some(display)).map_err(|_| NoConnection)?;
    let atoms = Atoms::new(&c).ok_or(NoConnection)?;
    let limits = Limits {
        timeout: limits.timeout.saturating_sub(start.elapsed()),
        ..*limits
    };
    let Some((text, open)) = read(&c, screen, &atoms, selection, &limits, true) else {
        return Ok(None);
    };
    if let Some(win) = open {
        let (prop, deadline) = (atoms.property, Instant::now() + limits.timeout);
        // (Without a thread, closing the connection destroys the window.)
        let _ = std::thread::Builder::new()
            .name("x11paste-rest".into())
            .spawn(move || let_go(&c, win, prop, deadline));
    }
    Ok(Some(text))
}

/// [`read_text_on`] couldn't connect to the display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoConnection;

/// [`read_text`], and the window of an INCR transfer it cut if
/// `keep_open` (else it's destroyed).
fn read(
    c: &RustConnection,
    screen: usize,
    atoms: &Atoms,
    selection: Selection,
    limits: &Limits,
    keep_open: bool,
) -> Option<(String, Option<Window>)> {
    let read = Read {
        c,
        screen,
        atoms,
        selection: atoms.selection(selection),
        deadline: Instant::now() + limits.timeout,
        keep_open,
    };
    // Which formats does the owner offer? (Some owners don't answer
    // TARGETS; then just try each in turn.)
    let offered: Option<Vec<Atom>> = read
        .fetch(atoms.targets, MAX_TARGETS, Overflow::Refuse)
        .filter(|f| f.format == 32)
        .map(|f| {
            let (words, _) = f.data.as_chunks::<4>();
            words.iter().map(|w| u32::from_ne_bytes(*w)).collect()
        });
    for target in atoms.formats() {
        if offered.as_ref().is_some_and(|o| !o.contains(&target)) {
            continue;
        }
        match read.fetch(target, limits.max_bytes, limits.overflow) {
            Some(f) if f.format == 8 => {
                let open = f.open;
                return Some((text_of(atoms, f, limits), open));
            }
            Some(Fetched {
                open: Some(win), ..
            }) => abandon(c, win),
            _ => {}
        }
        if Instant::now() >= read.deadline {
            break;
        }
    }
    None
}

/// The rest of an INCR transfer to `win` that was cut: each piece is deleted
/// unread, as if read, so the owner gets to the end and can serve again.
/// Only each piece's size is asked for, to know the empty one that ends it.
/// Stops there or at `deadline`, then destroys `win`.
fn let_go(c: &RustConnection, win: Window, prop: Atom, deadline: Instant) {
    // The piece read last is still there: deleting it asks for the next.
    let _ = c.delete_property(win, prop);
    let _ = c.flush();
    while let Some(e) = next_event(c, deadline) {
        let Event::PropertyNotify(e) = e else {
            continue;
        };
        if e.window != win || e.atom != prop || e.state != Property::NEW_VALUE {
            continue;
        }
        let Some(head) = c
            .get_property(false, win, prop, AtomEnum::ANY, 0, 0)
            .ok()
            .and_then(|r| r.reply().ok())
        else {
            break;
        };
        if head.type_ == x11rb::NONE {
            continue;
        }
        let _ = c.delete_property(win, prop);
        let _ = c.flush();
        if head.bytes_after == 0 {
            break; // the empty piece: the end
        }
    }
    abandon(c, win);
}

/// Destroys a conversion's window: whatever its owner still writes there is
/// refused.
fn abandon(c: &RustConnection, win: Window) {
    let _ = c.destroy_window(win);
    let _ = c.flush();
}

/// What was fetched, decoded as `limits` say.
fn text_of(atoms: &Atoms, f: Fetched, limits: &Limits) -> String {
    let mut data = f.data;
    // A character cut in two at the cap is dropped, not replaced (STRING is
    // one byte a character).
    if f.cut
        && f.kind != u32::from(AtomEnum::STRING)
        && let Err(e) = std::str::from_utf8(&data)
        && e.error_len().is_none()
    {
        data.truncate(e.valid_up_to());
    }
    let mut text = atoms.decode(f.kind, &data).replace("\r\n", "\n");
    // Latin-1 can take up to twice its bytes as UTF-8.
    if limits.overflow == Overflow::Cut && text.len() > limits.max_bytes {
        let mut end = limits.max_bytes;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

struct Fetched {
    kind: Atom,
    format: u8,
    data: Vec<u8>,
    /// More was offered than was read.
    cut: bool,
    /// The window of an INCR transfer cut and kept open ([`Read::keep_open`]).
    open: Option<Window>,
}

/// A read of one selection, with its deadline.
struct Read<'a> {
    c: &'a RustConnection,
    screen: usize,
    atoms: &'a Atoms,
    selection: Atom,
    deadline: Instant,
    /// An INCR transfer cut short keeps its window, for [`let_go`].
    keep_open: bool,
}

impl Read<'_> {
    /// Convert the selection to `target` and read the result: one larger
    /// than `max` bytes is refused before anything is allocated for it, or
    /// its first `max` bytes are read.
    fn fetch(&self, target: Atom, max: usize, overflow: Overflow) -> Option<Fetched> {
        // A window of its own for this conversion, destroyed when it ends:
        // an owner still writing to an abandoned one, or refusing it late,
        // can't reach a later conversion.
        let requestor = Requestor::new(self.c, self.screen)?;
        let (c, win, prop) = (self.c, requestor.0, self.atoms.property);
        drain(c, self.deadline)?;
        c.convert_selection(win, self.selection, target, prop, x11rb::CURRENT_TIME)
            .ok()?;
        c.flush().ok()?;
        loop {
            if let Event::SelectionNotify(e) = next_event(c, self.deadline)?
                && e.requestor == win
                && e.selection == self.selection
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
        if head.type_ == self.atoms.incr {
            let fetched = self.fetch_incr(win, max, overflow);
            if fetched.as_ref().is_some_and(|f| f.open.is_some()) {
                requestor.keep();
            }
            return fetched;
        }
        let size = head.bytes_after as usize;
        let cut = size > max;
        if cut && overflow == Overflow::Refuse {
            let _ = c.delete_property(win, prop);
            let _ = c.flush();
            return None;
        }
        let take = size.min(max);
        let reply = c
            .get_property(!cut, win, prop, AtomEnum::ANY, 0, words(take))
            .ok()?
            .reply()
            .ok()?;
        if cut {
            let _ = c.delete_property(win, prop);
            let _ = c.flush();
        } else if reply.bytes_after != 0 {
            // It grew between the two reads: someone is still writing into
            // it.
            return None;
        }
        let mut data = reply.value;
        data.truncate(take);
        Some(Fetched {
            kind: reply.type_,
            format: reply.format,
            data,
            cut,
            open: None,
        })
    }

    /// The INCR protocol: the owner sends chunks as property updates and an
    /// empty chunk ends the transfer. Its advertised total is ignored, and
    /// every chunk must have the first one's type and format.
    fn fetch_incr(&self, win: Window, max: usize, overflow: Overflow) -> Option<Fetched> {
        let (c, prop) = (self.c, self.atoms.property);
        // Deleting the INCR property tells the owner to start.
        c.delete_property(win, prop).ok()?;
        c.flush().ok()?;
        let mut out: Option<Fetched> = None;
        loop {
            match next_event(c, self.deadline)? {
                Event::PropertyNotify(e)
                    if e.window == win && e.atom == prop && e.state == Property::NEW_VALUE => {}
                _ => continue,
            }
            let head = c
                .get_property(false, win, prop, AtomEnum::ANY, 0, 0)
                .ok()?
                .reply()
                .ok()?;
            if head.type_ == x11rb::NONE {
                // Gone already: a notice for a chunk we've read. (The end of
                // the transfer is an empty property, not a missing one.)
                continue;
            }
            let chunk = head.bytes_after as usize;
            let have = out.as_ref().map_or(0, |o| o.data.len());
            let fits = have.saturating_add(chunk) <= max;
            if !fits && overflow == Overflow::Refuse {
                // Leave the chunk there: deleting it would only ask for more.
                return None;
            }
            // (Cutting, `have` is below `max` here: reaching it ends the
            // transfer.)
            let take = chunk.min(max - have);
            let full = have + take == max;
            // Deleting the chunk asks for the next: not once cutting has
            // what it takes.
            let more = fits && !(full && overflow == Overflow::Cut);
            let reply = c
                .get_property(more, win, prop, AtomEnum::ANY, 0, words(take))
                .ok()?
                .reply()
                .ok()?;
            c.flush().ok()?;
            if fits && reply.bytes_after != 0 {
                return None; // it grew while we read it
            }
            if reply.value.is_empty() {
                return out.or(Some(Fetched {
                    kind: reply.type_,
                    format: 8,
                    data: Vec::new(),
                    cut: false,
                    open: None,
                }));
            }
            let mut value = reply.value;
            value.truncate(take);
            match &mut out {
                None => {
                    out = Some(Fetched {
                        kind: reply.type_,
                        format: reply.format,
                        data: value,
                        cut: false,
                        open: None,
                    })
                }
                Some(o) if o.kind == reply.type_ && o.format == reply.format => {
                    o.data.extend_from_slice(&value)
                }
                Some(_) => return None, // the owner changed format mid-transfer
            }
            if full && overflow == Overflow::Cut {
                // Stopped here, whatever else there is (the last chunk may
                // end inside a character). The chunk read last is still in
                // place: the owner waits for its deletion.
                let open = self.keep_open.then_some(win);
                return out.map(|o| Fetched {
                    cut: true,
                    open,
                    ..o
                });
            }
        }
    }
}

/// `bytes` in 32-bit words, as GetProperty counts them.
fn words(bytes: usize) -> u32 {
    u32::try_from(bytes.div_ceil(4)).unwrap_or(u32::MAX)
}

/// An input-only window that receives one conversion, destroyed on drop.
struct Requestor<'a>(Window, &'a RustConnection);

impl<'a> Requestor<'a> {
    fn new(c: &'a RustConnection, screen: usize) -> Option<Requestor<'a>> {
        let root = c.setup().roots.get(screen)?.root;
        let win = c.generate_id().ok()?;
        let watch = CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE);
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
            &watch,
        )
        .ok()?;
        Some(Requestor(win, c))
    }

    /// Leaves the window to whoever has its id.
    fn keep(self) {
        std::mem::forget(self);
    }
}

impl Drop for Requestor<'_> {
    fn drop(&mut self) {
        abandon(self.1, self.0);
    }
}

/// Discard queued events: a bounded number, and never past `deadline`.
pub fn drain(c: &RustConnection, deadline: Instant) -> Option<()> {
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
pub fn next_event(c: &RustConnection, deadline: Instant) -> Option<Event> {
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
        ChangeWindowAttributesAux, PropMode, SELECTION_NOTIFY_EVENT, SelectionNotifyEvent,
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

    /// What a test owner serves.
    struct Offer {
        selection: &'static str,
        /// The property type of the text (and the one target offered besides
        /// TARGETS).
        kind: &'static str,
        data: Vec<u8>,
        /// Send through INCR in pieces of this many bytes.
        incr: Option<usize>,
    }

    /// Own the offer's selection on `display` and answer requests until they
    /// stop for half a second or the selection is taken. Yields how many
    /// INCR pieces were written.
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
            let (selection, kind) = (atom(offer.selection), atom(offer.kind));
            let (targets, incr) = (atom("TARGETS"), atom("INCR"));
            c.set_selection_owner(win, selection, x11rb::CURRENT_TIME)
                .unwrap();
            c.get_selection_owner(selection).unwrap().reply().unwrap();
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
                    c.change_property32(
                        PropMode::REPLACE,
                        r.requestor,
                        r.property,
                        AtomEnum::ATOM,
                        &[targets, kind],
                    )
                    .unwrap();
                    notify(r.property);
                    continue;
                }
                if r.target != kind {
                    notify(x11rb::NONE);
                    continue;
                }
                let Some(size) = offer.incr else {
                    c.change_property8(
                        PropMode::REPLACE,
                        r.requestor,
                        r.property,
                        kind,
                        &offer.data,
                    )
                    .unwrap();
                    notify(r.property);
                    continue;
                };
                let watch = ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE);
                c.change_window_attributes(r.requestor, &watch).unwrap();
                c.change_property32(PropMode::REPLACE, r.requestor, r.property, incr, &[1])
                    .unwrap();
                notify(r.property);
                for chunk in offer.data.chunks(size).chain([&[][..]]) {
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
                    let _ =
                        c.change_property8(PropMode::REPLACE, r.requestor, r.property, kind, chunk);
                    let _ = c.flush();
                }
            }
            taken
        });
        wait.recv().unwrap();
        owner
    }

    fn cut(max_bytes: usize) -> Limits {
        Limits {
            max_bytes,
            timeout: Duration::from_secs(2),
            overflow: Overflow::Cut,
        }
    }

    fn utf8(data: Vec<u8>, incr: Option<usize>) -> Offer {
        Offer {
            selection: "CLIPBOARD",
            kind: "UTF8_STRING",
            data,
            incr,
        }
    }

    /// Cutting reads the first `max_bytes` and asks for nothing more, in one
    /// piece or through INCR, however the owner splits it. Needs `Xvfb` on
    /// PATH; skipped otherwise.
    #[test]
    fn cutting_reads_no_further_than_the_cap() {
        let Some(x) = Xvfb::start() else {
            eprintln!("no Xvfb; skipped");
            return;
        };
        let (c, screen) = RustConnection::connect(Some(&x.display)).unwrap();
        let atoms = Atoms::new(&c).unwrap();
        let paste = |offer, limits: Limits| {
            let owner = own(&x.display, offer);
            let text = read_text(&c, screen, &atoms, Selection::Clipboard, &limits);
            (text, owner.join().unwrap())
        };
        const CAP: usize = 64 << 10;
        let sevens = |n| vec![b'7'; n];

        // In one piece: its first 64 KiB, or (refusing) nothing.
        let (text, _) = paste(utf8(sevens(2 << 20), None), cut(CAP));
        assert_eq!(text, Some("7".repeat(CAP)));
        let refuse = Limits {
            overflow: Overflow::Refuse,
            ..cut(CAP)
        };
        let (text, _) = paste(utf8(sevens(2 << 20), None), refuse);
        assert_eq!(text, None);
        // Exactly the cap, and under it: whole.
        let (text, _) = paste(utf8(sevens(CAP), None), cut(CAP));
        assert_eq!(text.map(|t| t.len()), Some(CAP));
        let (text, _) = paste(utf8(b"sin(x)\r\n".to_vec(), None), cut(CAP));
        assert_eq!(text.as_deref(), Some("sin(x)\n"));

        // Through INCR, a little over: four 16 KiB pieces fill it, and the
        // fourth isn't deleted, so the fifth is never asked for.
        let (text, taken) = paste(utf8(sevens(CAP + 1000), Some(16 << 10)), cut(CAP));
        assert_eq!(text, Some("7".repeat(CAP)));
        assert_eq!(taken, 4);
        // Pieces that don't divide it: the seventh 10,000-byte piece is read
        // only as far as the cap.
        let (text, taken) = paste(utf8(sevens(100_000), Some(10_000)), cut(CAP));
        assert_eq!(text, Some("7".repeat(CAP)));
        assert_eq!(taken, 7);
        // One huge piece: only its first 64 KiB cross.
        let (text, taken) = paste(utf8(sevens(4 << 20), Some(4 << 20)), cut(CAP));
        assert_eq!(text, Some("7".repeat(CAP)));
        assert_eq!(taken, 1);
        // Within the cap through INCR: whole, the transfer completed.
        let (text, taken) = paste(utf8(sevens(40_000), Some(16 << 10)), cut(CAP));
        assert_eq!(text.map(|t| t.len()), Some(40_000));
        assert_eq!(taken, 4);
        // Refusing, INCR past the cap stops at the piece that would pass it.
        let (text, taken) = paste(utf8(sevens(CAP + 1000), Some(16 << 10)), refuse);
        assert_eq!(text, None);
        assert_eq!(taken, 5);
    }

    /// The cut text is whole characters, at most the cap in UTF-8 bytes.
    #[test]
    fn cutting_keeps_whole_characters() {
        let Some(x) = Xvfb::start() else {
            eprintln!("no Xvfb; skipped");
            return;
        };
        let (c, screen) = RustConnection::connect(Some(&x.display)).unwrap();
        let atoms = Atoms::new(&c).unwrap();
        let paste = |offer, limits: Limits| {
            let owner = own(&x.display, offer);
            let text = read_text(&c, screen, &atoms, Selection::Clipboard, &limits);
            owner.join().unwrap();
            text.unwrap()
        };
        // An odd cap ends inside a two-byte character: it's dropped, not
        // replaced.
        let e_acute = "é".repeat(40_000).into_bytes();
        let text = paste(utf8(e_acute.clone(), None), cut(65_535));
        assert_eq!(text, "é".repeat(32_767));
        let text = paste(utf8(e_acute, Some(10_001)), cut(65_535));
        assert_eq!(text, "é".repeat(32_767));
        // Latin-1 doubles in UTF-8: 64 KiB of it is read, half of that kept.
        let latin1 = Offer {
            selection: "CLIPBOARD",
            kind: "STRING",
            data: vec![0xe9; 70_000],
            incr: None,
        };
        let text = paste(latin1, cut(65_536));
        assert_eq!(text, "é".repeat(32_768));
    }

    /// An owner that offers only `text/plain` (no charset) is read: as
    /// UTF-8 when it is, else as Latin-1 (R13-L-02).
    #[test]
    fn reads_plain_text_without_a_charset() {
        let Some(x) = Xvfb::start() else {
            eprintln!("no Xvfb; skipped");
            return;
        };
        let (c, screen) = RustConnection::connect(Some(&x.display)).unwrap();
        let atoms = Atoms::new(&c).unwrap();
        let paste = |data: &[u8]| {
            let owner = own(
                &x.display,
                Offer {
                    selection: "CLIPBOARD",
                    kind: "text/plain",
                    data: data.to_vec(),
                    incr: None,
                },
            );
            let text = read_text(&c, screen, &atoms, Selection::Clipboard, &cut(64 << 10));
            owner.join().unwrap();
            text
        };
        assert_eq!(paste(b"sin(x)").as_deref(), Some("sin(x)"));
        assert_eq!(paste("2×π é".as_bytes()).as_deref(), Some("2×π é"));
        assert_eq!(paste(b"caf\xe9\r\n").as_deref(), Some("café\n"));
    }

    /// The primary selection and a drag's data are read alike, here on a
    /// connection of the read's own.
    #[test]
    fn reads_primary_and_drag_selections() {
        let Some(x) = Xvfb::start() else {
            eprintln!("no Xvfb; skipped");
            return;
        };
        let limits = cut(64 << 10);
        // Nobody owns it: nothing, at once.
        let start = Instant::now();
        assert_eq!(
            read_text_on(&x.display, Selection::Primary, &limits),
            Ok(None)
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        // No such display: no connection, rather than no text.
        assert_eq!(
            read_text_on(":59000", Selection::Primary, &limits),
            Err(NoConnection)
        );
        for (selection, name) in [
            (Selection::Primary, "PRIMARY"),
            (Selection::Drag, "XdndSelection"),
            (Selection::Clipboard, "CLIPBOARD"),
        ] {
            let owner = own(
                &x.display,
                Offer {
                    selection: name,
                    kind: "UTF8_STRING",
                    data: format!("from {name}").into_bytes(),
                    incr: None,
                },
            );
            let text = read_text_on(&x.display, selection, &limits);
            owner.join().unwrap();
            assert_eq!(text, Ok(Some(format!("from {name}"))));
        }
    }

    /// On a connection of its own, the rest of an INCR transfer that was cut
    /// goes by unread to its end, so an owner that serves one transfer at a
    /// time (like xclip, and the test owner) serves the next paste too.
    #[test]
    fn own_connection_lets_the_rest_of_a_cut_transfer_go_by() {
        let Some(x) = Xvfb::start() else {
            eprintln!("no Xvfb; skipped");
            return;
        };
        const CAP: usize = 64 << 10;
        // 70,000 bytes: five 16 KiB pieces and the empty one that ends them.
        let owner = own(&x.display, utf8(vec![b'7'; 70_000], Some(16 << 10)));
        let paste = |timeout| {
            let limits = Limits {
                timeout,
                ..cut(CAP)
            };
            read_text_on(&x.display, Selection::Clipboard, &limits).unwrap()
        };
        assert_eq!(paste(Duration::from_secs(2)), Some("7".repeat(CAP)));
        // The rest goes by on a thread of its own; until it has, the owner
        // drops other requests (as xclip does), so a slow machine may need
        // to ask again (each time within the owner's half-second patience).
        let second = (0..3).find_map(|_| {
            thread::sleep(Duration::from_millis(50));
            paste(Duration::from_millis(400))
        });
        assert_eq!(second, Some("7".repeat(CAP)));
        // Every piece of both transfers written: each was asked for.
        assert_eq!(owner.join().unwrap(), 12);
    }
}
