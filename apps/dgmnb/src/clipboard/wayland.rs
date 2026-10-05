//! Wayland clipboard: copy text or images (the graph) and paste text.
//!
//! Adapted from smithay-clipboard (MIT, © 2018 Lucas Timmins & Victor
//! Berger), reworked to offer any set of MIME types and to use the same
//! smithay-client-toolkit version as winit. It runs a worker thread with
//! its own event queue on winit's Wayland connection; pipes are serviced on
//! short-lived threads (with deadlines) so the worker never blocks.
//!
//! The worker borrows winit's `wl_display`, so it must be shut down — and
//! all its Wayland objects destroyed — before that display goes away:
//! [`Clipboard::shutdown`] does that and waits (briefly) for it.

use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use sctk::data_device_manager::data_device::{DataDevice, DataDeviceData, DataDeviceHandler};
use sctk::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use sctk::data_device_manager::data_source::{CopyPasteSource, DataSourceHandler};
use sctk::data_device_manager::{DataDeviceManagerState, WritePipe};
use sctk::reexports::calloop::EventLoop;
use sctk::reexports::calloop::channel::{self, Channel};
use sctk::reexports::calloop_wayland_source::WaylandSource;
use sctk::reexports::client::globals::{GlobalList, registry_queue_init};
use sctk::reexports::client::protocol::wl_data_device::WlDataDevice;
use sctk::reexports::client::protocol::wl_data_device_manager::DndAction;
use sctk::reexports::client::protocol::wl_data_source::WlDataSource;
use sctk::reexports::client::protocol::wl_keyboard::WlKeyboard;
use sctk::reexports::client::protocol::wl_pointer::WlPointer;
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use sctk::registry::{ProvidesRegistryState, RegistryState};
use sctk::seat::pointer::{PointerData, PointerEvent, PointerEventKind, PointerHandler};
use sctk::seat::{Capability, SeatHandler, SeatState};
use sctk::{
    delegate_data_device, delegate_pointer, delegate_registry, delegate_seat, registry_handlers,
};
use wayland_backend::client::{Backend, ObjectId};

use super::{MAX_PASTE, pipe};

const TEXT_MIMES: [&str; 4] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
];

/// How long closing the app waits for the worker to let go of the display.
const SHUTDOWN_WAIT: Duration = Duration::from_millis(500);

/// Pipe transfers running at once. Past this a request is refused (its
/// pipe just closes) rather than piling up threads.
const MAX_TRANSFERS: usize = 8;
static TRANSFERS: AtomicUsize = AtomicUsize::new(0);

/// Run one pipe transfer on its own thread, within `MAX_TRANSFERS`. If it
/// can't run, `job` is dropped, which closes its pipe (and any reply
/// channel it holds).
fn transfer(job: impl FnOnce() + Send + 'static) {
    if TRANSFERS.fetch_add(1, Ordering::AcqRel) >= MAX_TRANSFERS {
        TRANSFERS.fetch_sub(1, Ordering::AcqRel);
        return;
    }
    struct Done;
    impl Drop for Done {
        fn drop(&mut self) {
            TRANSFERS.fetch_sub(1, Ordering::AcqRel);
        }
    }
    let spawned = std::thread::Builder::new()
        .name("clipboard-pipe".into())
        .spawn(move || {
            let _done = Done;
            job();
        });
    if spawned.is_err() {
        TRANSFERS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Data on offer: (MIME type, bytes).
type Offers = Vec<(String, Arc<[u8]>)>;

/// Sources kept on offer at most. A compositor may reject a source without
/// saying so (weston, for one set with a serial no newer than the current
/// selection's), and never cancels it.
const MAX_SOURCES: usize = 4;

/// Start of the private MIME type each source also offers, naming it, so a
/// selection the compositor announces can be told to be that source (an
/// offer carries no other trace of where it came from).
const TAG_PREFIX: &str = "application/x-dgmnb-copy-";

/// A source we've put on the clipboard.
struct Kept<S, K> {
    source: S,
    offers: Offers,
    /// The seat whose selection it was set as, with this serial.
    seat: K,
    serial: u32,
    /// Its private MIME type (see [`TAG_PREFIX`]).
    tag: String,
    /// The compositor announced it as the seat's selection, and nothing
    /// replaced it since.
    live: bool,
}

/// What we've put on the clipboard, oldest first: each source with its data,
/// so a request is answered from the source it names.
///
/// The newest isn't assumed to be the selection: the compositor may have
/// rejected it (a stale serial). A source is known to be the selection once
/// the compositor announces a selection offering its tag (it does to the
/// focused client, and to a client that gets the focus).
struct Sources<S, K> {
    kept: Vec<Kept<S, K>>,
}

impl<S, K: PartialEq> Sources<S, K> {
    fn new() -> Self {
        Sources { kept: Vec::new() }
    }

    /// Whether setting `offers` as `seat`'s selection with `serial` could
    /// change anything. Not if a source with them is the selection, nor if
    /// one was set with the same serial: that got the answer this would.
    /// (A held Ctrl+C repeats with the serial of the press.)
    fn wants(&self, offers: &Offers, seat: &K, serial: u32) -> bool {
        !self
            .kept
            .iter()
            .any(|k| k.offers == *offers && k.seat == *seat && (k.live || k.serial == serial))
    }

    /// Keeps a source just set as the selection. Returns those let go to
    /// stay within [`MAX_SOURCES`] (dropping a source destroys it): the
    /// oldest, but not one known to be a selection.
    fn push(&mut self, kept: Kept<S, K>) -> Vec<S> {
        self.kept.push(kept);
        let mut gone = Vec::new();
        while self.kept.len() > MAX_SOURCES {
            let i = self.kept.iter().position(|k| !k.live).unwrap_or(0);
            gone.push(self.kept.remove(i).source);
        }
        gone
    }

    /// The compositor announced `seat`'s selection, offering `mimes`. If it
    /// is one of ours, the ones set on that seat before it were replaced or
    /// rejected (the compositor handled their requests first), and are let
    /// go. Returns those.
    fn selection(&mut self, seat: &K, mimes: &[String]) -> Vec<S> {
        let current = self
            .kept
            .iter()
            .position(|k| k.seat == *seat && mimes.contains(&k.tag));
        let mut gone = Vec::new();
        for (i, k) in std::mem::take(&mut self.kept).into_iter().enumerate() {
            match current {
                _ if k.seat != *seat => self.kept.push(k),
                Some(c) if i < c => gone.push(k.source),
                _ => self.kept.push(Kept {
                    live: current == Some(i),
                    ..k
                }),
            }
        }
        gone
    }

    /// Lets go of the source `is` picks (the compositor cancelled it).
    fn cancelled(&mut self, is: impl Fn(&S) -> bool) -> Vec<S> {
        let (gone, kept) = std::mem::take(&mut self.kept)
            .into_iter()
            .partition(|k| is(&k.source));
        self.kept = kept;
        gone.into_iter().map(|k| k.source).collect()
    }

    /// The data `source` offers as `mime`.
    fn data(&self, is: impl Fn(&S) -> bool, mime: &str) -> Option<Arc<[u8]>> {
        self.kept
            .iter()
            .find(|k| is(&k.source))
            .and_then(|k| k.offers.iter().find(|o| o.0 == mime))
            .map(|o| o.1.clone())
    }
}

enum Command {
    Store(Offers),
    LoadText(Sender<Option<String>>),
    Exit,
}

pub struct Clipboard {
    tx: channel::Sender<Command>,
    worker: Option<(JoinHandle<()>, Receiver<()>)>,
}

impl Clipboard {
    /// # Safety
    /// `display` must be the live `wl_display` of winit's connection, and
    /// [`Clipboard::shutdown`] (or drop) must run before it is closed.
    pub unsafe fn new(display: *mut std::ffi::c_void) -> Option<Clipboard> {
        // SAFETY: forwarded from the caller.
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let conn = Connection::from_backend(backend);
        let (tx, rx) = channel::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("clipboard".into())
            .spawn(move || {
                worker(conn, rx);
                // Everything Wayland-side has been dropped by now.
                let _ = done_tx.send(());
            })
            .ok()?;
        Some(Clipboard {
            tx,
            worker: Some((handle, done_rx)),
        })
    }

    /// Stop the worker and wait for it to release the display.
    ///
    /// Returns false if it didn't stop in time: it then still holds the
    /// display, so the caller must not let the display close while this
    /// process keeps running (the app exits immediately instead).
    pub fn shutdown(&mut self) -> bool {
        let Some((handle, done)) = self.worker.take() else {
            return true;
        };
        let _ = self.tx.send(Command::Exit);
        match done.recv_timeout(SHUTDOWN_WAIT) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = handle.join();
                true
            }
            Err(mpsc::RecvTimeoutError::Timeout) => false,
        }
    }

    pub fn copy_text(&self, text: &str) {
        let data: Arc<[u8]> = Arc::from(text.as_bytes());
        let mut offers: Offers = TEXT_MIMES
            .iter()
            .filter(|m| **m != "STRING")
            .map(|m| (m.to_string(), data.clone()))
            .collect();
        // STRING is Latin-1: offered only when the text fits it.
        if let Some(latin1) = text
            .chars()
            .map(|c| u8::try_from(u32::from(c)).ok())
            .collect::<Option<Vec<u8>>>()
        {
            offers.push(("STRING".into(), Arc::from(latin1)));
        }
        let _ = self.tx.send(Command::Store(offers));
    }

    pub fn copy_png(&self, png: Vec<u8>) {
        let _ = self
            .tx
            .send(Command::Store(vec![("image/png".into(), Arc::from(png))]));
    }

    /// Read clipboard text (waits up to a second for the owner to send it).
    pub fn paste_text(&self) -> Option<String> {
        let (tx, rx) = mpsc::channel();
        self.tx.send(Command::LoadText(tx)).ok()?;
        rx.recv_timeout(Duration::from_secs(1)).ok().flatten()
    }
}

fn worker(conn: Connection, rx: Channel<Command>) {
    let Ok((globals, queue)) = registry_queue_init::<State>(&conn) else {
        return;
    };
    let Ok(mut event_loop) = EventLoop::<State>::try_new() else {
        return;
    };
    let Some(mut state) = State::new(&globals, &queue.handle()) else {
        return;
    };
    let handle = event_loop.handle();
    let inserted = handle.insert_source(rx, |event, _, state: &mut State| match event {
        channel::Event::Msg(Command::Store(offers)) => state.store(offers),
        channel::Event::Msg(Command::LoadText(reply)) => state.load_text(reply),
        // Exit asked for, or every sender gone: stop.
        channel::Event::Msg(Command::Exit) | channel::Event::Closed => state.exit = true,
    });
    if inserted.is_err() || WaylandSource::new(conn, queue).insert(handle).is_err() {
        return;
    }
    while !state.exit {
        if event_loop.dispatch(None, &mut state).is_err() {
            break;
        }
    }
    // `state` (seats, devices, sources) and the event loop holding the queue
    // drop here, destroying their Wayland objects while the display lives.
}

#[derive(Default)]
struct SeatData {
    keyboard: Option<WlKeyboard>,
    pointer: Option<WlPointer>,
    device: Option<DataDevice>,
    focused: bool,
    serial: u32,
}

// ObjectIds are hashed by identity only (as in smithay-clipboard).
#[allow(clippy::mutable_key_type)]
struct State {
    registry: RegistryState,
    seats_state: SeatState,
    manager: DataDeviceManagerState,
    seats: HashMap<ObjectId, SeatData>,
    latest: Option<ObjectId>,
    qh: QueueHandle<State>,
    sources: Sources<CopyPasteSource, ObjectId>,
    /// Sources made so far (numbers their tags).
    made: u64,
    exit: bool,
}

impl State {
    #[allow(clippy::mutable_key_type)]
    fn new(globals: &GlobalList, qh: &QueueHandle<State>) -> Option<State> {
        let manager = DataDeviceManagerState::bind(globals, qh).ok()?;
        let seats_state = SeatState::new(globals, qh);
        let seats = seats_state
            .seats()
            .map(|s| (s.id(), SeatData::default()))
            .collect();
        Some(State {
            registry: RegistryState::new(globals),
            seats_state,
            manager,
            seats,
            latest: None,
            qh: qh.clone(),
            sources: Sources::new(),
            made: 0,
            exit: false,
        })
    }

    fn seat(&self) -> Option<&SeatData> {
        self.seats.get(self.latest.as_ref()?)
    }

    /// Puts `offers` on the clipboard, unless that can't change anything
    /// (see [`Sources::wants`]).
    fn store(&mut self, offers: Offers) {
        let Some(seat_id) = self.latest.clone() else {
            return;
        };
        let Some(seat) = self.seats.get(&seat_id) else {
            return;
        };
        let (Some(device), serial) = (seat.device.as_ref(), seat.serial) else {
            return;
        };
        if !self.sources.wants(&offers, &seat_id, serial) {
            return;
        }
        self.made += 1;
        let tag = format!("{TAG_PREFIX}{}-{}", std::process::id(), self.made);
        let source = self.manager.create_copy_paste_source(
            &self.qh,
            offers.iter().map(|o| o.0.clone()).chain([tag.clone()]),
        );
        source.set_selection(device, serial);
        drop(self.sources.push(Kept {
            source,
            offers,
            seat: seat_id,
            serial,
            tag,
            live: false,
        }));
    }

    fn load_text(&mut self, reply: Sender<Option<String>>) {
        let offer = self
            .seat()
            .and_then(|s| s.device.as_ref())
            .and_then(|d| d.data().selection_offer());
        let Some(offer) = offer else {
            let _ = reply.send(None);
            return;
        };
        let mime = offer.with_mime_types(|mimes| {
            TEXT_MIMES
                .iter()
                .find(|m| mimes.iter().any(|o| o == *m))
                .map(|m| m.to_string())
        });
        let latin1 = mime.as_deref() == Some("STRING");
        let Some(pipe) = mime.and_then(|m| offer.receive(m).ok()) else {
            let _ = reply.send(None);
            return;
        };
        transfer(move || {
            let text = pipe::read_all(OwnedFd::from(pipe), MAX_PASTE).map(|buf| {
                let text = if latin1 {
                    buf.iter().map(|&b| char::from(b)).collect()
                } else {
                    String::from_utf8_lossy(&buf).into_owned()
                };
                text.replace("\r\n", "\n")
            });
            let _ = reply.send(text);
        });
    }

    fn send(&mut self, source: &WlDataSource, mime: String, pipe: WritePipe) {
        let data = self.sources.data(|s| s.inner() == source, &mime);
        let Some(data) = data else {
            return; // dropping the pipe tells the reader there's nothing
        };
        transfer(move || {
            pipe::write_all(OwnedFd::from(pipe), &data);
        });
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seats.insert(seat.id(), SeatData::default());
    }

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        let pointer = match capability {
            Capability::Pointer => self.seats_state.get_pointer(qh, &seat).ok(),
            _ => None,
        };
        let Some(data) = self.seats.get_mut(&seat.id()) else {
            return;
        };
        match capability {
            Capability::Keyboard => {
                data.keyboard = Some(seat.get_keyboard(qh, seat.id()));
                if data.device.is_none() {
                    data.device = Some(self.manager.get_data_device(qh, &seat));
                }
            }
            Capability::Pointer => data.pointer = pointer,
            _ => {}
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        let Some(data) = self.seats.get_mut(&seat.id()) else {
            return;
        };
        match capability {
            Capability::Keyboard => {
                data.device = None;
                if let Some(k) = data.keyboard.take()
                    && k.version() >= 3
                {
                    k.release();
                }
            }
            Capability::Pointer => {
                if let Some(p) = data.pointer.take()
                    && p.version() >= 3
                {
                    p.release();
                }
            }
            _ => {}
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.seats.remove(&seat.id());
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        pointer: &WlPointer,
        events: &[PointerEvent],
    ) {
        let Some(seat) = pointer.data::<PointerData>().map(|d| d.seat().id()) else {
            return;
        };
        for e in events {
            if let PointerEventKind::Press { serial, .. } | PointerEventKind::Release { serial, .. } =
                e.kind
                && let Some(data) = self.seats.get_mut(&seat)
            {
                data.serial = serial;
                self.latest = Some(seat.clone());
            }
        }
    }
}

impl Dispatch<WlKeyboard, ObjectId, State> for State {
    fn event(
        state: &mut State,
        _: &WlKeyboard,
        event: <WlKeyboard as Proxy>::Event,
        seat: &ObjectId,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        use sctk::reexports::client::protocol::wl_keyboard::Event;
        let Some(data) = state.seats.get_mut(seat) else {
            return;
        };
        match event {
            Event::Key { serial, .. } | Event::Modifiers { serial, .. } => {
                data.serial = serial;
                state.latest = Some(seat.clone());
            }
            Event::Enter { serial, .. } => {
                data.serial = serial;
                data.focused = true;
                state.latest = Some(seat.clone());
            }
            Event::Leave { .. } => data.focused = false,
            _ => {}
        }
    }
}

impl DataDeviceHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataDevice,
        _: f64,
        _: f64,
        _: &WlSurface,
    ) {
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}
    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        let Some(data) = device.data::<DataDeviceData>() else {
            return;
        };
        let mimes = data
            .selection_offer()
            .map(|offer| offer.with_mime_types(<[String]>::to_vec))
            .unwrap_or_default();
        drop(self.sources.selection(&data.seat().id(), &mimes));
    }
    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
}

impl DataSourceHandler for State {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }
    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        mime: String,
        pipe: WritePipe,
    ) {
        self.send(source, mime, pipe);
    }
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        drop(self.sources.cancelled(|s| s.inner() == source));
    }
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

impl DataOfferHandler for State {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}

impl ProvidesRegistryState for State {
    registry_handlers![SeatState];

    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
}

delegate_seat!(State);
delegate_pointer!(State);
delegate_data_device!(State);
delegate_registry!(State);

impl Drop for Clipboard {
    fn drop(&mut self) {
        if !self.shutdown() {
            // Never let the worker outlive the display it borrows.
            eprintln!("dgmnb: clipboard worker did not stop; exiting now");
            std::process::exit(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(source: u32, text: &str, seat: u8, serial: u32) -> Kept<u32, u8> {
        Kept {
            source,
            offers: vec![("text/plain".into(), Arc::from(text.as_bytes()))],
            seat,
            serial,
            tag: format!("{TAG_PREFIX}{source}"),
            live: false,
        }
    }

    fn offers(text: &str) -> Offers {
        vec![("text/plain".into(), Arc::from(text.as_bytes()))]
    }

    fn sources(s: &Sources<u32, u8>) -> Vec<(u32, bool)> {
        s.kept.iter().map(|k| (k.source, k.live)).collect()
    }

    /// Review 12, question 2: the newest source may have been rejected (a
    /// stale serial), so copying the same text again with a new serial must
    /// set it again; with the same serial it would be rejected again.
    #[test]
    fn copying_again_after_a_rejection_sets_the_selection() {
        let mut s = Sources::new();
        assert!(s.wants(&offers("5"), &0, 7));
        drop(s.push(kept(1, "5", 0, 7)));
        // Nothing confirmed it.
        assert!(!s.wants(&offers("5"), &0, 7));
        assert!(s.wants(&offers("5"), &0, 8));
        assert!(s.wants(&offers("5"), &1, 7));
        assert!(s.wants(&offers("6"), &0, 7));
        // Once it is the selection, the same text is already there.
        drop(s.selection(&0, &["text/plain".into(), format!("{TAG_PREFIX}1")]));
        assert!(!s.wants(&offers("5"), &0, 8));
        assert!(s.wants(&offers("6"), &0, 8));
        // Another client's selection: ours isn't the selection any more.
        drop(s.selection(&0, &["text/plain".into()]));
        assert!(s.wants(&offers("5"), &0, 9));
    }

    /// Past four sources the oldest are let go, but never the one the
    /// compositor confirmed: newer ones may all have been rejected.
    #[test]
    fn the_selection_outlives_rejected_sources() {
        let mut s = Sources::new();
        drop(s.push(kept(1, "a", 0, 5)));
        assert!(s.selection(&0, &[format!("{TAG_PREFIX}1")]).is_empty());
        // Rejected (weston: the same serial), never cancelled.
        let mut gone = Vec::new();
        for (source, text) in [(2, "b"), (3, "c"), (4, "d"), (5, "e"), (6, "f")] {
            gone.extend(s.push(kept(source, text, 0, 5)));
        }
        assert_eq!(gone, [2, 3]);
        assert_eq!(sources(&s), [(1, true), (4, false), (5, false), (6, false)]);
        // A newer source confirmed: the ones set before it were replaced or
        // rejected.
        assert_eq!(s.selection(&0, &[format!("{TAG_PREFIX}5")]), [1, 4]);
        assert_eq!(sources(&s), [(5, true), (6, false)]);
        // Cancelled sources go.
        assert_eq!(s.cancelled(|&x| x == 5), [5]);
        assert_eq!(sources(&s), [(6, false)]);
        assert_eq!(
            s.data(|&x| x == 6, "text/plain").as_deref(),
            Some(&b"f"[..])
        );
        assert_eq!(s.data(|&x| x == 6, &format!("{TAG_PREFIX}6")), None);
    }

    /// Each seat has its own selection.
    #[test]
    fn seats_keep_their_own_selections() {
        let mut s = Sources::new();
        drop(s.push(kept(1, "a", 0, 5)));
        drop(s.push(kept(2, "b", 1, 6)));
        drop(s.push(kept(3, "c", 0, 7)));
        assert_eq!(s.selection(&0, &[format!("{TAG_PREFIX}3")]), [1]);
        assert_eq!(sources(&s), [(2, false), (3, true)]);
        assert!(s.selection(&1, &[format!("{TAG_PREFIX}2")]).is_empty());
        assert_eq!(sources(&s), [(2, true), (3, true)]);
    }

    fn clipboard_threads() -> usize {
        std::fs::read_dir("/proc/self/task")
            .map(|d| {
                d.filter_map(|t| std::fs::read_to_string(t.ok()?.path().join("comm")).ok())
                    .filter(|n| n.trim() == "clipboard")
                    .count()
            })
            .unwrap_or(0)
    }

    /// The worker must be gone (and its Wayland objects destroyed) before
    /// the display connection it borrows is dropped. It copies text, so it
    /// never uses your session's display: it runs only against the socket
    /// in DGMNB_TEST_WAYLAND_DISPLAY (say, a headless weston's), and is
    /// skipped otherwise.
    #[test]
    fn worker_stops_before_the_display_goes() {
        let Some(name) = std::env::var_os("DGMNB_TEST_WAYLAND_DISPLAY") else {
            eprintln!("DGMNB_TEST_WAYLAND_DISPLAY not set; skipped");
            return;
        };
        let path = match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(dir) => std::path::Path::new(&dir).join(&name),
            None => std::path::PathBuf::from(&name),
        };
        let conn = std::os::unix::net::UnixStream::connect(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| Connection::from_socket(s).map_err(|e| e.to_string()))
            .unwrap_or_else(|e| panic!("no compositor at {}: {e}", path.display()));
        let before = clipboard_threads();
        let display = conn.backend().display_ptr();
        // SAFETY: `conn` outlives the clipboard; we shut it down first.
        let mut clip = unsafe { Clipboard::new(display.cast()) }.expect("clipboard");
        clip.copy_text("probe");
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(clipboard_threads(), before + 1);
        assert!(clip.shutdown(), "worker didn't confirm it stopped");
        // Joined, it has exited; the kernel drops its /proc entry a moment
        // later (it clears the id join waits on before releasing the task).
        let mut left = clipboard_threads();
        for _ in 0..100 {
            if left == before {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            left = clipboard_threads();
        }
        assert_eq!(left, before, "worker still running after shutdown");
        drop(clip);
        conn.roundtrip()
            .expect("display still healthy after shutdown");
        drop(conn);
    }
}
