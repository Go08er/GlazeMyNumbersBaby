//! Wayland clipboard: copy text or images (the graph) and paste text.
//!
//! Adapted from smithay-clipboard (MIT, © 2018 Lucas Timmins & Victor
//! Berger), reworked to offer any set of MIME types and to use the same
//! smithay-client-toolkit version as winit. It runs a worker thread with
//! its own event queue on winit's Wayland connection; pipes are serviced on
//! short-lived threads so the worker never blocks.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use sctk::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
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

const TEXT_MIMES: [&str; 4] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
];

/// Largest clipboard text we'll read.
const MAX_PASTE: u64 = 1 << 20;

enum Command {
    Store(Vec<(String, Arc<[u8]>)>),
    LoadText(Sender<Option<String>>),
}

pub struct Clipboard {
    tx: channel::Sender<Command>,
}

impl Clipboard {
    /// # Safety
    /// `display` must be the live `wl_display` of winit's connection.
    pub unsafe fn new(display: *mut std::ffi::c_void) -> Option<Clipboard> {
        // SAFETY: forwarded from the caller.
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let conn = Connection::from_backend(backend);
        let (tx, rx) = channel::channel();
        std::thread::Builder::new()
            .name("clipboard".into())
            .spawn(move || worker(conn, rx))
            .ok()?;
        Some(Clipboard { tx })
    }

    pub fn copy_text(&self, text: &str) {
        let data: Arc<[u8]> = Arc::from(text.as_bytes());
        let offers = TEXT_MIMES
            .iter()
            .map(|m| (m.to_string(), data.clone()))
            .collect();
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
    let inserted = handle.insert_source(rx, |event, _, state: &mut State| {
        if let channel::Event::Msg(cmd) = event {
            match cmd {
                Command::Store(offers) => state.store(offers),
                Command::LoadText(reply) => state.load_text(reply),
            }
        }
    });
    if inserted.is_err() || WaylandSource::new(conn, queue).insert(handle).is_err() {
        return;
    }
    while event_loop.dispatch(None, &mut state).is_ok() {}
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
    sources: Vec<CopyPasteSource>,
    offers: Vec<(String, Arc<[u8]>)>,
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
            sources: Vec::new(),
            offers: Vec::new(),
        })
    }

    fn seat(&self) -> Option<&SeatData> {
        self.seats.get(self.latest.as_ref()?)
    }

    fn store(&mut self, offers: Vec<(String, Arc<[u8]>)>) {
        let Some(seat) = self.seat() else { return };
        let (Some(device), serial) = (seat.device.as_ref(), seat.serial) else {
            return;
        };
        let source = self
            .manager
            .create_copy_paste_source(&self.qh, offers.iter().map(|o| o.0.clone()));
        source.set_selection(device, serial);
        self.offers = offers;
        self.sources.push(source);
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
        let Some(pipe) = mime.and_then(|m| offer.receive(m).ok()) else {
            let _ = reply.send(None);
            return;
        };
        std::thread::spawn(move || {
            let mut file = std::fs::File::from(OwnedFd::from(pipe));
            let mut buf = Vec::new();
            let ok = (&mut file).take(MAX_PASTE).read_to_end(&mut buf).is_ok();
            let text = ok.then(|| String::from_utf8_lossy(&buf).replace("\r\n", "\n"));
            let _ = reply.send(text);
        });
    }

    fn send(&mut self, mime: String, pipe: WritePipe) {
        let Some((_, data)) = self.offers.iter().find(|o| o.0 == mime).cloned() else {
            return;
        };
        std::thread::spawn(move || {
            let mut file = std::fs::File::from(OwnedFd::from(pipe));
            let _ = file.write_all(&data);
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
    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
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
        _: &WlDataSource,
        mime: String,
        pipe: WritePipe,
    ) {
        self.send(mime, pipe);
    }
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        self.sources.retain(|s| s.inner() != source);
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
