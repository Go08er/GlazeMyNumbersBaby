//! The drawing layers at run time (docs/theming.md, "Drawing"): when a
//! theme's `background`, `keypad`, `display` and `overlay` hooks run, what
//! they are told, and what keeps them running.
//!
//! Each place a layer is drawn (the backdrop, a keypad, a readout) owns an
//! [`Instance`]. Its widget asks it for the layer's picture in `snapshot`
//! ([`Instance::picture`]); the hook runs only if something the layer
//! depends on changed since the picture was made, and the picture is
//! replayed otherwise.
//!
//! **Nothing runs at rest.** A source exists only in [`Instance::arm`]:
//! a one-shot timer or a frame callback, and only for a hook that asked
//! for another frame while the window is on screen, in use, allowed to
//! move and not settled (`gmnb_theme::sched`). The base look, a theme
//! with no layers and a theme whose hooks return `nil` never get there.
//!
//! Events are funnelled through [`Layers::emit`]: every layer that
//! listens is told, in its own coordinates, and so is whoever
//! [`Layers::listen`]s (sound, later), under reduced motion too.

pub mod replay;
pub mod tile;

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use gmnb_theme::draw::DisplayList;
pub use gmnb_theme::events::{Action, Source};
use gmnb_theme::events::{Event, EventKind, KeyKind, Page, Ring, Via};
use gmnb_theme::layer::{DisplayValue, Drawn, Frame, Keys, LayerKind, Placement};
use gmnb_theme::sched::{Change, Clock, Conditions, Fire, SETTLE, Wait};
use gmnb_theme::{OptionValue, TextMeasure};
use gtk::prelude::*;
use gtk::{glib, graphene};

use crate::theme::Hub;
use crate::widgets::keypad::Keypad;

/// Seconds on the clock every layer shares.
pub fn now() -> f64 {
    glib::monotonic_time() as f64 / 1.0e6
}

/// What a layer drew last.
#[derive(Clone)]
pub struct Picture {
    pub list: Rc<DisplayList>,
    /// Keypad: how `place` moves each cell (empty: none is moved).
    pub places: Rc<Vec<Option<Placement>>>,
    /// What `draw` returned when it drew this: `None` for nil.
    pub again: Option<f64>,
}

/// What the layers have cost, for the gates (`GMNB_THEME_STATS`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    /// Hook frames drawn, by layer kind (background, keypad, display,
    /// overlay).
    pub frames: [u64; 4],
    /// Frames that failed.
    pub failed: u64,
    /// Timers armed and frame callbacks installed, ever.
    pub timers: u64,
    pub ticks: u64,
    /// Sources alive now.
    pub armed: u32,
    /// Events emitted.
    pub events: u64,
    /// Milliseconds spent in hooks, and in the slowest one.
    pub hook_ms: f64,
    pub worst_ms: f64,
}

fn slot(kind: LayerKind) -> usize {
    match kind {
        LayerKind::Background => 0,
        LayerKind::Keypad => 1,
        LayerKind::Display => 2,
        LayerKind::Overlay => 3,
    }
}

/// Something that happened, where it happened.
pub struct Happened<'a> {
    pub kind: EventKind,
    pub page: Page,
    /// The control, readout or keypad it came from. `None`: nowhere in
    /// particular (each layer is told its own area).
    pub widget: Option<&'a gtk::Widget>,
    /// The point pressed, in `widget`'s coordinates; `None`: its middle.
    pub at: Option<(f32, f32)>,
    pub source: Option<Source>,
    pub via: Option<Via>,
    pub action: Option<Action>,
    pub key: Option<(Option<u32>, KeyKind)>,
    pub option: Option<(String, OptionValue)>,
}

impl<'a> Happened<'a> {
    pub fn new(kind: EventKind, page: Page, widget: Option<&'a gtk::Widget>) -> Self {
        Happened {
            kind,
            page,
            widget,
            at: None,
            source: None,
            via: None,
            action: None,
            key: None,
            option: None,
        }
    }

    /// The event as a widget sees it: its rectangle and point in
    /// `target`'s coordinates (`size`: the target's own, for an event
    /// from nowhere in particular).
    fn seen_from(&self, target: &gtk::Widget, size: (f32, f32)) -> Option<Event> {
        let rect = match self.widget {
            None => [0.0, 0.0, size.0, size.1],
            Some(w) => {
                let b = w.compute_bounds(target)?;
                [b.x(), b.y(), b.width(), b.height()]
            }
        };
        let mut event = Event::at(self.kind, self.page, rect);
        if let (Some(w), Some((x, y))) = (self.widget, self.at)
            && let Some(p) = w.compute_point(target, &graphene::Point::new(x, y))
        {
            (event.x, event.y) = (p.x(), p.y());
        }
        event.source = self.source;
        event.via = self.via;
        event.action = self.action;
        event.key = self.key;
        event.option = self.option.clone();
        Some(event)
    }
}

type Listener = Box<dyn Fn(&Event)>;

/// What every layer of the window shares.
pub struct Layers {
    hub: RefCell<Weak<Hub>>,
    instances: RefCell<Vec<Weak<Instance>>>,
    /// When a key was last pressed, the pointer last moved over the
    /// window, an event last happened or the window last became active.
    last_activity: Cell<f64>,
    /// The window is the one in use.
    active: Cell<bool>,
    /// `GMNB_STILL`: layers see `motion = false`.
    still: bool,
    /// `GMNB_ACTIVE`: the window counts as in use whatever the
    /// compositor says (a headless one never says so).
    always_active: bool,
    /// Counts the themes loaded: an instance made for another starts
    /// afresh.
    generation: Cell<u64>,
    /// Which layers the loaded theme draws.
    drawn: Cell<[bool; 4]>,
    pub cache: replay::Cache,
    listeners: RefCell<Vec<Listener>>,
    stats: Cell<Stats>,
    next_id: Cell<u64>,
    /// When the pointer last pressed in the window, and when Space or
    /// Enter last did: how a control that says it was used was used.
    pointer_pressed: Cell<f64>,
    key_activated: Cell<f64>,
}

/// The page a widget is on: the name its page has in the window's stack
/// (a dialog's widgets are on none: Settings).
pub fn page_of(widget: &gtk::Widget) -> Page {
    let mut at = Some(widget.clone());
    while let Some(w) = at {
        let parent = w.parent();
        if let Some(stack) = parent.as_ref().and_then(|p| p.downcast_ref::<gtk::Stack>()) {
            match stack.page(&w).name().as_deref() {
                Some("calculator") => return Page::Calculator,
                Some("converter") => return Page::Converter,
                Some("date") => return Page::Date,
                Some("graphing") => return Page::Graphing,
                _ => {}
            }
        }
        at = parent;
    }
    Page::Settings
}

impl Layers {
    pub fn new() -> Rc<Layers> {
        Rc::new(Layers {
            hub: RefCell::new(Weak::new()),
            instances: RefCell::default(),
            last_activity: Cell::new(now()),
            active: Cell::new(false),
            still: std::env::var_os("GMNB_STILL").is_some(),
            always_active: std::env::var_os("GMNB_ACTIVE").is_some(),
            generation: Cell::new(0),
            drawn: Cell::new([false; 4]),
            cache: replay::Cache::default(),
            listeners: RefCell::default(),
            stats: Cell::default(),
            next_id: Cell::new(1),
            pointer_pressed: Cell::new(f64::NEG_INFINITY),
            key_activated: Cell::new(f64::NEG_INFINITY),
        })
    }

    pub fn set_hub(&self, hub: &Rc<Hub>) {
        self.hub.replace(Rc::downgrade(hub));
    }

    fn hub(&self) -> Option<Rc<Hub>> {
        self.hub.borrow().upgrade()
    }

    pub fn stats(&self) -> Stats {
        self.stats.get()
    }

    fn count(&self, f: impl FnOnce(&mut Stats)) {
        let mut stats = self.stats.get();
        f(&mut stats);
        self.stats.set(stats);
    }

    /// A layer's hooks ran for this long.
    pub fn hook_took(&self, time: Duration) {
        let ms = time.as_secs_f64() * 1000.0;
        self.count(|s| {
            s.hook_ms += ms;
            s.worst_ms = s.worst_ms.max(ms);
        });
    }

    /// A number no other keypad or readout of this window has.
    pub fn next_id(&self) -> u64 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }

    /// Whether layers may move: the desktop allows it, and `GMNB_STILL`
    /// isn't set.
    pub fn motion(&self) -> bool {
        !self.still && crate::widgets::animations_enabled()
    }

    fn active(&self) -> bool {
        self.always_active || self.active.get()
    }

    fn settled(&self, now: f64) -> bool {
        now - self.last_activity.get() >= SETTLE
    }

    /// Whether the loaded theme draws `kind` (its hook exists and hasn't
    /// been switched off).
    pub fn draws(&self, kind: LayerKind) -> bool {
        self.drawn.get()[slot(kind)]
    }

    fn live(&self) -> Vec<Rc<Instance>> {
        let mut all = self.instances.borrow_mut();
        all.retain(|i| i.strong_count() > 0);
        all.iter().filter_map(Weak::upgrade).collect()
    }

    /// The layer of `kind` drawn in `widget`.
    pub fn instance(
        self: &Rc<Self>,
        kind: LayerKind,
        widget: &impl IsA<gtk::Widget>,
    ) -> Rc<Instance> {
        let widget = widget.upcast_ref::<gtk::Widget>();
        let instance = Rc::new(Instance {
            kind,
            layers: Rc::downgrade(self),
            widget: widget.downgrade(),
            state: RefCell::new(State::default()),
        });
        self.instances.borrow_mut().push(Rc::downgrade(&instance));
        // Coming on screen and leaving it are conditions of animating.
        let weak = Rc::downgrade(&instance);
        widget.connect_map(move |_| {
            if let Some(i) = weak.upgrade() {
                i.conditions_changed();
            }
        });
        let weak = Rc::downgrade(&instance);
        widget.connect_unmap(move |_| {
            if let Some(i) = weak.upgrade() {
                i.conditions_changed();
            }
        });
        instance
    }

    /// The theme was loaded, reloaded or unloaded: every layer starts
    /// afresh, and nothing of the old theme's keeps running.
    pub fn theme_changed(&self) {
        self.generation.set(self.generation.get() + 1);
        self.cache.clear();
        self.sync_drawn();
        for instance in self.live() {
            instance.reset();
        }
    }

    /// Reads from the theme which layers it draws.
    pub fn sync_drawn(&self) {
        let drawn = self.hub().map_or([false; 4], |hub| {
            hub.with_theme(|t| match t {
                None => [false; 4],
                Some(t) => LayerKind::ALL.map(|kind| t.draws(kind)),
            })
        });
        self.drawn.set(drawn);
    }

    /// `ctx` changed: every layer needs a new picture.
    pub fn ctx_changed(&self) {
        for instance in self.live() {
            instance.invalidate();
            instance.conditions_changed();
        }
    }

    /// The window became the one in use, or stopped being it.
    pub fn set_active(&self, active: bool) {
        if self.active.replace(active) == active {
            return;
        }
        if active {
            self.poke();
        }
        for instance in self.live() {
            instance.conditions_changed();
        }
    }

    /// Something happened (a key, the pointer over the window, an event,
    /// the window activated): the clock that stops after
    /// [`SETTLE`] seconds starts again, and what waited for it carries
    /// on.
    pub fn poke(&self) {
        let now = now();
        let was_settled = self.settled(now);
        self.last_activity.set(now);
        if was_settled {
            for instance in self.live() {
                instance.conditions_changed();
            }
        }
    }

    /// The pointer pressed somewhere in the window.
    pub fn pointer_pressed(&self) {
        self.pointer_pressed.set(now());
    }

    /// Space or Enter was pressed somewhere in the window: what activates
    /// the control with the keyboard's focus.
    pub fn key_activated(&self) {
        self.key_activated.set(now());
    }

    /// How a control that has just said it was used was used: by the
    /// pointer or the keyboard if either did something within the last
    /// second, else through the accessibility interface.
    fn via(&self) -> Via {
        let now = now();
        if now - self.pointer_pressed.get() < 1.0 {
            Via::Pointer
        } else if now - self.key_activated.get() < 1.0 {
            Via::Keyboard
        } else {
            Via::Assistive
        }
    }

    /// A keypad's key was used. `at`: the point of the click that did it,
    /// in the keypad's coordinates; `None` for a key typed on the
    /// keyboard, which presses the matching key on screen.
    pub fn key_pressed(&self, pad: &Keypad, id: u32, at: Option<(f32, f32)>) {
        let Some(button) = pad.button(id) else {
            return;
        };
        let (kind, action) = pad.key_meaning(id);
        let button = button.upcast::<gtk::Widget>();
        let mut happened = Happened::new(EventKind::Press, page_of(&button), Some(&button));
        happened.at = at
            .and_then(|(x, y)| pad.compute_point(&button, &graphene::Point::new(x, y)))
            .map(|p| (p.x(), p.y()));
        happened.source = Some(Source::Key);
        happened.via = Some(if at.is_some() {
            self.via()
        } else {
            Via::Keyboard
        });
        happened.action = action;
        happened.key = Some((Some(id), kind));
        self.emit(&happened);
    }

    /// A control that isn't a keypad's own key was used: `source` says
    /// what kind ([`Source::Key`]: a key the calculator doesn't number,
    /// the graphing keypad's).
    pub fn pressed(&self, widget: &impl IsA<gtk::Widget>, source: Source, action: Option<Action>) {
        let widget = widget.upcast_ref::<gtk::Widget>();
        let mut happened = Happened::new(EventKind::Press, page_of(widget), Some(widget));
        happened.source = Some(source);
        happened.via = Some(self.via());
        happened.action = action;
        if source == Source::Key {
            happened.key = Some((None, KeyKind::Other));
        }
        self.emit(&happened);
    }

    /// A calculation produced a value in `readout`, or an error.
    pub fn outcome(&self, readout: &impl IsA<gtk::Widget>, error: bool) {
        let readout = readout.upcast_ref::<gtk::Widget>();
        let kind = if error {
            EventKind::Error
        } else {
            EventKind::Result
        };
        self.emit(&Happened::new(kind, page_of(readout), Some(readout)));
    }

    /// A keypad came into view.
    pub fn shown(&self, pad: &Keypad) {
        if !self.draws_any() {
            return;
        }
        let pad = pad.upcast_ref::<gtk::Widget>();
        self.emit(&Happened::new(EventKind::Show, page_of(pad), Some(pad)));
    }

    /// The user changed page or calculator mode: `page` is the widget of
    /// the page now shown.
    pub fn mode_changed(&self, page: &gtk::Widget) {
        self.emit(&Happened::new(EventKind::Mode, page_of(page), Some(page)));
    }

    fn draws_any(&self) -> bool {
        self.drawn.get().contains(&true) || !self.listeners.borrow().is_empty()
    }

    /// Runs `f` once the frame being painted is done: for what a hook's
    /// failure asks for (telling the user, unloading the theme), which
    /// must not happen inside a `snapshot`.
    pub fn after_frame(&self, f: impl FnOnce() + 'static) {
        glib::idle_add_local_once(f);
    }

    /// Calls `f` for every event, in the window's coordinates, after the
    /// layers have been told (under reduced motion too, when layers are
    /// told nothing).
    pub fn listen(&self, f: impl Fn(&Event) + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    /// Something happened: every layer that listens for it is told, in
    /// its own coordinates.
    pub fn emit(&self, happened: &Happened) {
        self.poke();
        self.count(|s| s.events += 1);
        // A keypad's layer is told of its own keypad's keys and showing,
        // not of another's.
        let from_pad = (happened.kind == EventKind::Show || happened.source == Some(Source::Key))
            .then(|| {
                happened.widget.and_then(|w| {
                    if w.is::<Keypad>() {
                        Some(w.clone())
                    } else {
                        w.ancestor(Keypad::static_type())
                    }
                })
            })
            .flatten();
        let motion = self.motion();
        let at = now();
        if !self.draws_any() {
            return;
        }
        if motion && self.drawn.get().contains(&true) {
            for instance in self.live() {
                if !self.draws(instance.kind) {
                    continue;
                }
                let Some(widget) = instance.widget.upgrade().filter(|w| w.is_mapped()) else {
                    continue;
                };
                if instance.kind == LayerKind::Keypad
                    && from_pad.as_ref().is_some_and(|pad| *pad != widget)
                {
                    continue;
                }
                let size = (widget.width() as f32, widget.height() as f32);
                if let Some(event) = happened.seen_from(&widget, size) {
                    instance.happened(at, event);
                }
            }
        }
        let listeners = self.listeners.borrow();
        if listeners.is_empty() {
            return;
        }
        let Some(root) = happened
            .widget
            .and_then(|w| w.root())
            .map(|r| r.upcast::<gtk::Widget>())
        else {
            return;
        };
        let size = (root.width() as f32, root.height() as f32);
        if let Some(event) = happened.seen_from(&root, size) {
            for listener in listeners.iter() {
                listener(&event);
            }
        }
    }
}

#[derive(Default)]
struct State {
    /// The theme this state belongs to ([`Layers::generation`]).
    generation: u64,
    clock: Clock,
    ring: Ring,
    /// The layer asked for the pointer.
    wants_pointer: bool,
    pointer: Option<(f32, f32)>,
    picture: Option<Picture>,
    /// The size the picture was drawn for.
    size: (f32, f32),
    dirty: bool,
    timer: Option<glib::SourceId>,
    tick: Option<gtk::TickCallbackId>,
}

/// What a frame of a keypad or a readout is told beside the common
/// fields.
#[derive(Default)]
pub struct Extra<'a> {
    pub keys: Option<Keys<'a>>,
    pub value: Option<DisplayValue<'a>>,
}

/// One layer in one place.
pub struct Instance {
    pub kind: LayerKind,
    layers: Weak<Layers>,
    widget: glib::WeakRef<gtk::Widget>,
    state: RefCell<State>,
}

impl Instance {
    fn conditions(&self, layers: &Layers, now: f64) -> Conditions {
        Conditions {
            mapped: self.widget.upgrade().is_some_and(|w| w.is_mapped()),
            active: layers.active(),
            motion: layers.motion(),
            settled: layers.settled(now),
        }
    }

    fn disarm(&self, st: &mut State) {
        let had = u32::from(st.timer.is_some()) + u32::from(st.tick.is_some());
        if let Some(id) = st.timer.take() {
            id.remove();
        }
        if let Some(id) = st.tick.take() {
            id.remove();
        }
        if had > 0
            && let Some(layers) = self.layers.upgrade()
        {
            layers.count(|s| s.armed = s.armed.saturating_sub(had));
        }
    }

    /// Arms what the scheduler asked for: the only place a layer's timer
    /// or frame callback is made.
    fn arm(self: &Rc<Self>, st: &mut State, wait: Wait) {
        let Some(layers) = self.layers.upgrade() else {
            return;
        };
        match wait {
            Wait::None => self.disarm(st),
            Wait::Timer(seconds) => {
                self.disarm(st);
                let weak = Rc::downgrade(self);
                let id =
                    glib::timeout_add_local_once(Duration::from_secs_f64(seconds), move || {
                        if let Some(i) = weak.upgrade() {
                            i.timer_fired();
                        }
                    });
                st.timer = Some(id);
                layers.count(|s| {
                    s.timers += 1;
                    s.armed += 1;
                });
            }
            Wait::Frame => {
                if let Some(id) = st.timer.take() {
                    id.remove();
                    layers.count(|s| s.armed = s.armed.saturating_sub(1));
                }
                if st.tick.is_none()
                    && let Some(widget) = self.widget.upgrade()
                {
                    let weak = Rc::downgrade(self);
                    let id = widget.add_tick_callback(move |_, clock| match weak.upgrade() {
                        Some(i) => i.tick(clock.frame_time() as f64 / 1.0e6),
                        None => glib::ControlFlow::Break,
                    });
                    st.tick = Some(id);
                    layers.count(|s| {
                        s.ticks += 1;
                        s.armed += 1;
                    });
                }
            }
        }
    }

    fn timer_fired(self: &Rc<Self>) {
        let Some(layers) = self.layers.upgrade() else {
            return;
        };
        let now = now();
        let fire = {
            let mut st = self.state.borrow_mut();
            // A one-shot timer is gone once it has fired.
            if st.timer.take().is_some() {
                layers.count(|s| s.armed = s.armed.saturating_sub(1));
            }
            let c = self.conditions(&layers, now);
            st.clock.fire(now, c)
        };
        if matches!(fire, Fire::Draw | Fire::NotYet) {
            self.redraw();
        }
    }

    fn tick(self: &Rc<Self>, now: f64) -> glib::ControlFlow {
        let Some(layers) = self.layers.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let fire = {
            let mut st = self.state.borrow_mut();
            let c = self.conditions(&layers, now);
            st.clock.fire(now, c)
        };
        match fire {
            Fire::Draw => {
                self.redraw();
                glib::ControlFlow::Continue
            }
            Fire::NotYet => glib::ControlFlow::Continue,
            Fire::Hold | Fire::Idle => {
                // Returning Break removes the callback: only forget it.
                if self.state.borrow_mut().tick.take().is_some() {
                    layers.count(|s| s.armed = s.armed.saturating_sub(1));
                }
                glib::ControlFlow::Break
            }
        }
    }

    /// The layer needs a new picture at its widget's next frame.
    fn redraw(&self) {
        self.state.borrow_mut().dirty = true;
        if let Some(widget) = self.widget.upgrade() {
            widget.queue_draw();
        }
    }

    /// Something the layer depends on changed (a key's state, the value,
    /// `ctx`): it is drawn again, if the theme draws it at all.
    pub fn invalidate(&self) {
        if self.layers.upgrade().is_some_and(|l| l.draws(self.kind)) {
            self.redraw();
        }
    }

    /// The pointer moved over the layer's widget, or left it. Only a
    /// layer that asked for the pointer is drawn again.
    pub fn set_pointer(&self, pointer: Option<(f32, f32)>) {
        let changed = {
            let mut st = self.state.borrow_mut();
            let changed = st.pointer != pointer;
            st.pointer = pointer;
            changed && st.wants_pointer
        };
        if changed {
            self.invalidate();
        }
    }

    /// What the window is doing changed: a waiting request may go on, a
    /// pending one may have to wait.
    pub fn conditions_changed(self: &Rc<Self>) {
        let Some(layers) = self.layers.upgrade() else {
            return;
        };
        let change = {
            let mut st = self.state.borrow_mut();
            let c = self.conditions(&layers, now());
            let change = st.clock.conditions(c);
            if change == Change::Disarm {
                self.disarm(&mut st);
            }
            change
        };
        if change == Change::Draw {
            self.redraw();
        }
    }

    fn happened(&self, at: f64, event: Event) {
        let told = self.state.borrow_mut().ring.push(at, event);
        if told {
            self.redraw();
        }
    }

    /// Forgets everything of the theme that was (its timers first), and
    /// has the widget drawn again.
    fn reset(&self) {
        {
            let mut st = self.state.borrow_mut();
            self.disarm(&mut st);
            let pointer = st.pointer;
            *st = State {
                pointer,
                ..State::default()
            };
        }
        if let Some(widget) = self.widget.upgrade() {
            widget.queue_draw();
        }
    }

    /// The layer's picture for its widget's `snapshot`, of `size`. The
    /// hook runs if the layer needs a new picture; `extra` says what a
    /// keypad's or a readout's frame holds. `None`: the theme doesn't
    /// draw this layer, or its hook failed (a readout then paints the
    /// base way).
    pub fn picture(self: &Rc<Self>, size: (f32, f32), extra: Extra) -> Option<Picture> {
        let layers = self.layers.upgrade()?;
        if !layers.draws(self.kind) {
            return None;
        }
        let hub = layers.hub()?;
        let mut st = self.state.borrow_mut();
        let generation = layers.generation.get();
        if st.generation != generation {
            // The first frame of this theme here.
            let (events, pointer) = hub.with_theme(|t| {
                t.and_then(|t| t.layer(self.kind))
                    .map(|l| (l.events.clone(), l.pointer))
                    .unwrap_or_default()
            });
            self.disarm(&mut st);
            st.clock.reset();
            st.ring = Ring::new(&events);
            st.wants_pointer = pointer;
            st.picture = None;
            st.generation = generation;
            st.dirty = true;
        }
        if !st.dirty && st.size == size {
            return st.picture.clone();
        }
        // The time of the frame being painted, when there is one.
        let now = self
            .widget
            .upgrade()
            .and_then(|w| w.frame_clock())
            .map_or_else(now, |c| c.frame_time() as f64 / 1.0e6);
        let c = self.conditions(&layers, now);
        let timing = st.clock.begin(now, c);
        let drawn = {
            let State {
                ring,
                pointer,
                wants_pointer,
                ..
            } = &mut *st;
            let frame = Frame {
                width: size.0,
                height: size.1,
                time: timing.time,
                dt: timing.dt,
                motion: c.motion,
                settled: c.settled,
                active: c.active,
                pointer: pointer.filter(|_| *wants_pointer),
                // Under reduced motion there are no events.
                events: if c.motion {
                    ring.at(now).collect()
                } else {
                    Vec::new()
                },
                keys: extra.keys,
                value: extra.value,
            };
            hub.draw_layer(self.kind, &frame)
        };
        st.dirty = false;
        st.size = size;
        match drawn {
            Ok(Drawn {
                list,
                again,
                places,
            }) => {
                layers.count(|s| s.frames[slot(self.kind)] += 1);
                st.picture = Some(Picture {
                    list: Rc::new(list),
                    places: Rc::new(places),
                    again,
                });
                let wait = st.clock.end(now, again, c);
                self.arm(&mut st, wait);
            }
            Err(failure) => {
                // Nothing is drawn for this layer this frame.
                layers.count(|s| s.failed += 1);
                st.picture = None;
                let wait = st.clock.end(now, None, c);
                self.arm(&mut st, wait);
                drop(st);
                hub.layer_failed(self.kind, failure);
                return None;
            }
        }
        st.picture.clone()
    }
}

/// Replays a picture a theme drew into `s`, in `widget`'s coordinates.
/// `glyphs`: a readout's laid-out characters, and its old value's.
pub fn paint(
    s: &gtk::Snapshot,
    widget: &impl IsA<gtk::Widget>,
    list: &DisplayList,
    glyphs: [&[replay::Glyph]; 2],
) {
    let Some(hub) = Hub::current() else {
        return;
    };
    let pango = widget.pango_context();
    hub.with_theme(|theme| {
        let scene = replay::Scene {
            cache: &hub.layers().cache,
            theme,
            pango: &pango,
            glyphs,
        };
        replay::replay(s, list, &scene);
    });
}

/// Measures `draw:text_size` with the widget's Pango context.
pub fn measure(widget: &impl IsA<gtk::Widget>) -> Rc<dyn TextMeasure> {
    Rc::new(replay::Measure(widget.pango_context()))
}
