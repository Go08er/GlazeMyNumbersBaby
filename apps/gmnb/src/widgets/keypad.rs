//! `Keypad` — a homogeneous grid of keys. How a key looks is the
//! stylesheet's (`keys.*` of the style); the grid's gap, the keys' corner
//! radius, their icons' size and their labels' fonts come from the style
//! here.
//!
//! A key's label is set as DGMNB sets it (a [`FitLabel`]): in its class's
//! font, superscripts and subscripts at 0.62 of it, shrunk to the key's
//! width less 8, its capitals centred on the middle of DGMNB's cell.
//!
//! A theme's `keypad` layer is drawn over the keys (docs/theming.md, "The
//! keypad layer"): its `draw` hook is told every cell of the grid and its
//! state, and its `place` hook may move, scale and fade each cell's
//! picture. Only the picture: a key is clicked, focused and read out
//! where it really is. Without such a layer the keypad draws its
//! children and nothing else.
//!
//! One thing is built in: a key typed on the keyboard is shown `active`
//! for 110 ms ([`Keypad::flash`]).

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use gmnb_theme::events::{Action, KeyKind as CellKind, Via};
use gmnb_theme::layer::{KeyCell, Keys, LayerKind};
use gmnb_theme::types::{Font, Shadow};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene};

use super::fit::{Align, FitLabel};
use crate::layers::{self, Extra, Instance};
use crate::theme::Hub;

/// Visual role of a key; maps onto a CSS class.
pub use appcore::keys::{Key, KeyKind};

fn kind_css(kind: KeyKind) -> &'static str {
    match kind {
        KeyKind::Number => "wc-num",
        KeyKind::Operator => "wc-op",
        KeyKind::Function => "wc-fn",
        KeyKind::Equals => "wc-eq",
        KeyKind::Toggle => "wc-fn",
    }
}

/// The row of the style a key's label is set from.
fn kind_path(kind: KeyKind) -> &'static str {
    match kind {
        KeyKind::Number => "keys.number",
        KeyKind::Operator => "keys.operator",
        KeyKind::Function | KeyKind::Toggle => "keys.func",
        KeyKind::Equals => "keys.equals",
    }
}

const KEY_PATHS: [&str; 4] = ["keys.number", "keys.func", "keys.operator", "keys.equals"];

/// A key's label and what it is set from.
struct KeyText {
    label: FitLabel,
    /// The row of the style its font is from.
    path: &'static str,
    /// The key has no name of its own (no tooltip either): it is called
    /// what its label says.
    named_by_label: bool,
}

/// A key class's font, and the weight each of its states gives it
/// instead.
struct KeyFont {
    font: Font,
    hover: Option<f32>,
    active: Option<f32>,
    checked: Option<f32>,
    disabled: Option<f32>,
}

/// Whether something covers `widget` (crate::inert): what is insensitive
/// only for that keeps its look.
fn covered(widget: &gtk::Widget) -> bool {
    let mut up = widget.parent();
    while let Some(w) = up {
        if w.has_css_class("wc-inert") {
            return true;
        }
        up = w.parent();
    }
    false
}

/// What a key is to a theme.
fn cell_kind(kind: KeyKind) -> CellKind {
    match kind {
        KeyKind::Number => CellKind::Number,
        KeyKind::Operator => CellKind::Operator,
        KeyKind::Function => CellKind::Func,
        KeyKind::Equals => CellKind::Equals,
        KeyKind::Toggle => CellKind::Toggle,
    }
}

/// What a key does, when it is one of the things a theme is told of:
/// the backspace key by its icon, C and CE by their labels.
fn key_action(key: &Key) -> Option<Action> {
    if key.icon == Some(appcore::icons::BACKSPACE) {
        Some(Action::Backspace)
    } else if matches!(key.label.as_str(), "C" | "CE") {
        Some(Action::Clear)
    } else {
        None
    }
}

/// How long a key typed on the keyboard is shown `active`.
const FLASH: std::time::Duration = std::time::Duration::from_millis(110);

type PressedFn = Rc<dyn Fn(u32, f32, f32)>;

/// One cell of the grid: a key, or another widget attached to it.
struct Cell_ {
    widget: gtk::Widget,
    row: i32,
    col: i32,
    /// A calculator key's number.
    id: Option<u32>,
    kind: CellKind,
    action: Option<Action>,
}

mod imp {
    use super::*;

    pub struct Keypad {
        pub keys: RefCell<HashMap<u32, gtk::Button>>,
        /// Every cell, by row and column.
        pub(super) cells: RefCell<Vec<Cell_>>,
        pub handlers: RefCell<Vec<PressedFn>>,
        pub last_press: Cell<Option<(f32, f32, Instant)>>,
        /// When Space or Enter was last pressed on one of the keys.
        pub last_activation: Cell<Option<Instant>>,
        /// How the key clicked last was clicked.
        pub via: Cell<Via>,
        /// From the style: the keys' corner radius (`keys.radius`), the
        /// grid's gap (`keys.gap`) and the compact one's
        /// (`keys.gap_compact`), and which of the two is in use.
        pub radius: Cell<f32>,
        pub gaps: Cell<(f32, f32)>,
        pub compact: Cell<bool>,
        /// The keys' icons, and the size and line width the style gives
        /// them (`keys.icon`).
        pub icons: RefCell<Vec<super::super::icon::PathIcon>>,
        pub icon: Cell<(f32, f32)>,
        /// The keys' labels, and from the style what they are set in: each
        /// class's font (`keys.number`, …) and the halo of text on what
        /// is raised (`window.raised_text_halo`).
        pub(super) texts: RefCell<HashMap<u32, KeyText>>,
        pub(super) fonts: RefCell<HashMap<&'static str, KeyFont>>,
        pub halo: RefCell<Vec<Shadow>>,
        /// The grid's rows and columns, as the cells put in it so far
        /// make them: the labels read it as they are drawn.
        pub grid: Rc<Cell<(usize, usize)>>,
        /// The theme's `keypad` layer here, once it has been asked for.
        pub layer: OnceCell<Rc<Instance>>,
        /// This keypad's number among the window's, for the layer.
        pub pad: Cell<u64>,
        /// The cells a frame was last told, and how often their layout
        /// (not their state) has changed.
        pub told: RefCell<(u64, Vec<KeyCell>)>,
        /// `GMNB_POINTER`: where the pointer is taken to be.
        pub fixed_pointer: Cell<Option<(f32, f32)>>,
        /// The keypad came into view and the theme hasn't been told yet:
        /// it is told when the keypad is next drawn, which is when it has
        /// its place in the window.
        pub show_pending: Cell<bool>,
    }

    impl Default for Keypad {
        fn default() -> Self {
            Self {
                keys: RefCell::default(),
                cells: RefCell::default(),
                handlers: RefCell::default(),
                last_press: Cell::new(None),
                last_activation: Cell::new(None),
                via: Cell::new(Via::Pointer),
                radius: Cell::new(appcore::metrics::shape::RADIUS),
                gaps: Cell::new((
                    appcore::metrics::calc::KEY_GAP,
                    appcore::metrics::calc::KEY_GAP_COMPACT,
                )),
                compact: Cell::new(false),
                icons: RefCell::default(),
                icon: Cell::new((appcore::metrics::controls::KEY_ICON, 1.5)),
                texts: RefCell::default(),
                fonts: RefCell::default(),
                halo: RefCell::default(),
                grid: Rc::default(),
                layer: OnceCell::new(),
                pad: Cell::new(0),
                told: RefCell::default(),
                fixed_pointer: Cell::new(None),
                show_pending: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Keypad {
        const NAME: &'static str = "GmnbKeypad";
        type Type = super::Keypad;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::GridLayout>();
            klass.set_css_name("keypad");
        }
    }

    impl ObjectImpl for Keypad {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            let grid = obj.grid_layout();
            grid.set_row_homogeneous(true);
            grid.set_column_homogeneous(true);
            obj.sync_gap();

            // Where the pointer is, for a layer that asked for it.
            {
                let motion = gtk::EventControllerMotion::new();
                let weak = obj.downgrade();
                motion.connect_motion(move |_, x, y| {
                    if let Some(layer) = weak.upgrade().and_then(|k| k.layer()) {
                        layer.set_pointer(Some((x as f32, y as f32)));
                    }
                });
                let weak = obj.downgrade();
                motion.connect_leave(move |_| {
                    if let Some(k) = weak.upgrade()
                        && let Some(layer) = k.layer()
                    {
                        layer.set_pointer(k.imp().fixed_pointer.get());
                    }
                });
                obj.add_controller(motion);

                // Observe presses (capture phase, never claimed) to learn the exact
                // point a key was hit: a press event says where.
                let click = gtk::GestureClick::new();
                click.set_propagation_phase(gtk::PropagationPhase::Capture);
                let weak = obj.downgrade();
                click.connect_pressed(move |_, _, x, y| {
                    if let Some(k) = weak.upgrade() {
                        k.imp()
                            .last_press
                            .set(Some((x as f32, y as f32, Instant::now())));
                    }
                });
                obj.add_controller(click);

                // And Space or Enter on a key with the focus: a click with
                // neither before it came through the accessibility
                // interface.
                let keys = gtk::EventControllerKey::new();
                keys.set_propagation_phase(gtk::PropagationPhase::Capture);
                let weak = obj.downgrade();
                keys.connect_key_pressed(move |_, key, _, _| {
                    use gtk::gdk::Key;
                    if matches!(
                        key,
                        Key::space | Key::Return | Key::KP_Enter | Key::ISO_Enter
                    ) && let Some(k) = weak.upgrade()
                    {
                        k.imp().last_activation.set(Some(Instant::now()));
                    }
                    glib::Propagation::Proceed
                });
                obj.add_controller(keys);
            }

            obj.connect_map(|k| k.cascade());

            // Screenshot/dev hook: pretend the pointer hovers at "x,y".
            if let Some((x, y)) = std::env::var("GMNB_POINTER").ok().and_then(|v| {
                let (a, b) = v.split_once(',')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            }) {
                self.fixed_pointer.set(Some((x, y)));
            }
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Keypad {
        fn snapshot(&self, s: &gtk::Snapshot) {
            self.obj().draw(s);
        }
    }
}

glib::wrapper! {
    pub struct Keypad(ObjectSubclass<imp::Keypad>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Keypad {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Keypad {
    pub fn new() -> Self {
        Self::default()
    }

    fn grid_layout(&self) -> gtk::GridLayout {
        self.layout_manager()
            .and_downcast::<gtk::GridLayout>()
            .expect("grid layout")
    }

    fn sync_gap(&self) {
        let (gap, compact) = self.imp().gaps.get();
        let gap = if self.imp().compact.get() {
            compact
        } else {
            gap
        };
        let gap = gap.round().max(0.0) as u32;
        let g = self.grid_layout();
        g.set_row_spacing(gap);
        g.set_column_spacing(gap);
    }

    /// The compact window's keypad: `keys.gap_compact` between its keys.
    pub fn set_compact(&self, compact: bool) {
        self.imp().compact.set(compact);
        self.sync_gap();
    }

    /// What the keypad takes from the style in Rust: the grid's gaps, the
    /// keys' radius (what a layer is told), their icons and their labels'
    /// fonts.
    pub fn set_style(&self, style: &gmnb_theme::Style) {
        let imp = self.imp();
        {
            let mut fonts = imp.fonts.borrow_mut();
            fonts.clear();
            for path in KEY_PATHS {
                if let Some(c) = style.control(path) {
                    fonts.insert(
                        path,
                        KeyFont {
                            font: c.font.clone(),
                            hover: c.hover.font_weight,
                            active: c.active.font_weight,
                            checked: c.checked.font_weight,
                            disabled: c.disabled.font_weight,
                        },
                    );
                }
            }
        }
        imp.halo
            .replace(style.shadows("window.raised_text_halo").to_vec());
        let ids: Vec<u32> = imp.texts.borrow().keys().copied().collect();
        for id in ids {
            self.sync_text(id);
        }
        imp.radius.set(style.radius("keys.radius").uniform());
        imp.gaps
            .set((style.number("keys.gap"), style.number("keys.gap_compact")));
        self.sync_gap();
        if let Some(icon) = style.icon("keys.icon") {
            let stroke = icon.stroke.unwrap_or_else(|| style.icon_stroke(icon.size));
            imp.icon.set((icon.size, stroke));
            for i in imp.icons.borrow().iter() {
                i.set_style(icon.size, stroke);
            }
        }
        self.queue_draw();
    }

    /// Key `id`'s label in the font and halo its key's state gives it:
    /// its class's font, in the weight of the state the key is in (the
    /// stylesheet's order: `checked`, then `hover` or `active`, then
    /// `disabled`), and no halo on a key that can't be pressed (it
    /// smears the faint glyphs), as the stylesheet has it for a button's
    /// label.
    fn sync_text(&self, id: u32) {
        let imp = self.imp();
        let texts = imp.texts.borrow();
        let (Some(text), Some(button)) = (texts.get(&id), self.button(id)) else {
            return;
        };
        let enabled = button.is_sensitive();
        if enabled {
            text.label.set_halo(&imp.halo.borrow());
        } else {
            text.label.set_halo(&[]);
        }
        let fonts = imp.fonts.borrow();
        let Some(f) = fonts.get(text.path) else {
            return;
        };
        let flags = button.state_flags();
        let has = |class: &str| button.has_css_class(class);
        let mut weight = f.font.weight;
        if flags.contains(gtk::StateFlags::CHECKED) || has("wc-on") {
            weight = f.checked.unwrap_or(weight);
        }
        if !enabled {
            // Covered, not switched off: as it was.
            if has("wc-off") || !covered(button.upcast_ref()) {
                weight = f.disabled.unwrap_or(weight);
            }
        } else if flags.contains(gtk::StateFlags::ACTIVE)
            || has("wc-flash")
            || has("keyboard-activating")
        {
            weight = f.active.unwrap_or(weight);
        } else if flags.contains(gtk::StateFlags::PRELIGHT) {
            weight = f.hover.unwrap_or(weight);
        }
        // DGMNB shrinks a label to its key with no least size.
        if weight == f.font.weight {
            text.label.set_font(&f.font, 1.0);
        } else {
            let font = Font {
                weight,
                ..f.font.clone()
            };
            text.label.set_font(&font, 1.0);
        }
    }

    /// The theme's `keypad` layer drawn here.
    fn layer(&self) -> Option<Rc<Instance>> {
        let imp = self.imp();
        if let Some(layer) = imp.layer.get() {
            return Some(layer.clone());
        }
        let hub = Hub::current()?;
        let all = hub.layers();
        imp.pad.set(all.next_id());
        let layer = all.instance(LayerKind::Keypad, self);
        layer.set_pointer(imp.fixed_pointer.get());
        Some(imp.layer.get_or_init(|| layer).clone())
    }

    /// A cell's state changed: a layer that draws the keypad draws it
    /// again.
    fn cell_changed(&self) {
        if let Some(layer) = self.layer() {
            layer.invalidate();
        }
    }

    /// Add a key at (row, col) spanning (rows, cols). Returns the button.
    pub fn add(&self, key: Key, row: i32, col: i32, rows: i32, cols: i32) -> gtk::Button {
        let mut text = None;
        let child: gtk::Widget = match key.icon {
            Some(path) => {
                let (size, stroke) = self.imp().icon.get();
                let icon = super::icon::PathIcon::new(path, size.round() as i32);
                icon.set_style(size, stroke);
                // A key is as small as its place: DGMNB's keys shrink
                // below what they show (its least window is 300 × 400).
                icon.set_loose(true);
                self.imp().icons.borrow_mut().push(icon.clone());
                icon.upcast()
            }
            None => {
                // Drawn only: the key is named below.
                let label = FitLabel::drawn();
                label.set_markup(&key.label);
                label.set_align(Align::Center);
                label.set_loose(true);
                let inset = (appcore::metrics::controls::KEY_LABEL_MARGIN / 2.0) as i32;
                label.set_margin_start(inset);
                label.set_margin_end(inset);
                text = Some(label.clone());
                label.upcast()
            }
        };
        let button = gtk::Button::builder()
            .child(&child)
            // 2nd / hyp / inverse are toggles; screen readers hear their state.
            .accessible_role(if key.kind == KeyKind::Toggle {
                gtk::AccessibleRole::ToggleButton
            } else {
                gtk::AccessibleRole::Button
            })
            // A click focuses the key, as upstream's (all but the
            // Scientific 2nd: CalculatorPage::build_scientific), so the
            // keys after it are as after Tab to it: Enter after clicking 2
            // is the page's ("="), which a calculator key ignores
            // (window::Step::PageOnly). The ring stays hidden
            // (window::hide_ring_on_press).
            .focus_on_click(true)
            .hexpand(true)
            .vexpand(true)
            .css_classes(["wc-key", kind_css(key.kind)])
            .build();
        if let Some(tip) = &key.tooltip {
            button.set_tooltip_text(Some(tip));
        }
        // Its own name or its tooltip, else what its label says (which
        // is drawn, not a label the toolkit could read it from).
        button.update_property(&[gtk::accessible::Property::Label(&key.accessible_name())]);
        self.put(
            &button,
            (row, col, rows, cols),
            Some(key.id),
            cell_kind(key.kind),
            key_action(&key),
        );

        let id = key.id;
        self.imp().keys.borrow_mut().insert(id, button.clone());
        if let Some(label) = text {
            // About the middle of DGMNB's cell, not of the toolkit's
            // whole-pixel allocation.
            label.set_cell_of(
                self,
                &button,
                self.imp().grid.clone(),
                (row.max(0) as usize, col.max(0) as usize),
                (rows.max(1) as usize, cols.max(1) as usize),
            );
            self.imp().texts.borrow_mut().insert(
                id,
                KeyText {
                    label,
                    path: kind_path(key.kind),
                    named_by_label: key.a11y.is_none() && key.tooltip.is_none(),
                },
            );
            self.sync_text(id);
            // Its font's weight and its halo follow the key's state.
            let weak = self.downgrade();
            button.connect_state_flags_changed(move |_, _| {
                if let Some(k) = weak.upgrade() {
                    k.sync_text(id);
                }
            });
            let weak = self.downgrade();
            button.connect_css_classes_notify(move |_| {
                if let Some(k) = weak.upgrade() {
                    k.sync_text(id);
                }
            });
        }
        let weak = self.downgrade();
        button.connect_clicked(move |b| {
            if let Some(k) = weak.upgrade() {
                k.on_key_clicked(id, b);
            }
        });
        button
    }

    /// Attach an arbitrary widget (e.g. a menu button) into the grid.
    pub fn attach(&self, widget: &impl IsA<gtk::Widget>, row: i32, col: i32, rows: i32, cols: i32) {
        self.put(widget, (row, col, rows, cols), None, CellKind::Other, None);
    }

    fn put(
        &self,
        widget: &impl IsA<gtk::Widget>,
        (row, col, rows, cols): (i32, i32, i32, i32),
        id: Option<u32>,
        kind: CellKind,
        action: Option<Action>,
    ) {
        widget.set_parent(self);
        let lc = self
            .grid_layout()
            .layout_child(widget)
            .downcast::<gtk::GridLayoutChild>()
            .expect("grid layout child");
        lc.set_row(row);
        lc.set_column(col);
        lc.set_row_span(rows);
        lc.set_column_span(cols);
        let widget = widget.clone().upcast::<gtk::Widget>();
        let grid = &self.imp().grid;
        let (r, c) = grid.get();
        grid.set((
            r.max((row + rows).max(0) as usize),
            c.max((col + cols).max(0) as usize),
        ));
        // What a layer is told of a cell changes with these.
        let weak = self.downgrade();
        widget.connect_state_flags_changed(move |_, _| {
            if let Some(k) = weak.upgrade() {
                k.cell_changed();
            }
        });
        let weak = self.downgrade();
        widget.connect_css_classes_notify(move |_| {
            if let Some(k) = weak.upgrade() {
                k.cell_changed();
            }
        });
        let mut cells = self.imp().cells.borrow_mut();
        let at = cells.partition_point(|c| (c.row, c.col) <= (row, col));
        cells.insert(
            at,
            Cell_ {
                widget,
                row,
                col,
                id,
                kind,
                action,
            },
        );
    }

    pub fn ids(&self) -> Vec<u32> {
        self.imp().keys.borrow().keys().copied().collect()
    }

    pub fn button(&self, id: u32) -> Option<gtk::Button> {
        self.imp().keys.borrow().get(&id).cloned()
    }

    /// What key `id` is to a theme, and what it does (if it is a clear
    /// or the backspace).
    pub fn key_meaning(&self, id: u32) -> (CellKind, Option<Action>) {
        self.imp()
            .cells
            .borrow()
            .iter()
            .find(|c| c.id == Some(id))
            .map_or((CellKind::Other, None), |c| (c.kind, c.action))
    }

    /// Relabel a key. One named by its label is called what the new one
    /// says.
    pub fn set_key_label(&self, id: u32, markup: &str) {
        let texts = self.imp().texts.borrow();
        let Some(text) = texts.get(&id) else {
            return;
        };
        text.label.set_markup(markup);
        if text.named_by_label
            && let Some(b) = self.button(id)
        {
            let name = appcore::keys::plain_label(markup);
            b.update_property(&[gtk::accessible::Property::Label(&name)]);
        }
    }

    /// Relabel a key and keep its tooltip and accessible name in step.
    pub fn set_key(&self, id: u32, markup: &str, tip: &str) {
        if let Some(text) = self.imp().texts.borrow_mut().get_mut(&id) {
            text.label.set_markup(markup);
            text.named_by_label = false;
        }
        if let Some(b) = self.button(id) {
            b.set_tooltip_text(Some(tip));
            b.update_property(&[gtk::accessible::Property::Label(tip)]);
        }
    }

    pub fn set_key_sensitive(&self, id: u32, sensitive: bool) {
        if let Some(b) = self.button(id) {
            b.set_sensitive(sensitive);
        }
    }

    /// `f(id, x, y)` when a key is clicked: the point pressed, in the
    /// keypad's coordinates, or the key's middle if it wasn't the
    /// pointer's doing.
    pub fn connect_pressed(&self, f: impl Fn(u32, f32, f32) + 'static) {
        self.imp().handlers.borrow_mut().push(Rc::new(f));
    }

    fn on_key_clicked(&self, id: u32, button: &gtk::Button) {
        let Some(rect) = button.compute_bounds(self) else {
            return;
        };
        let imp = self.imp();
        let recent = |t: Instant| t.elapsed().as_secs_f32() < 1.0;
        let (x, y) = match imp.last_press.take() {
            Some((x, y, t)) if recent(t) && rect.contains_point(&graphene::Point::new(x, y)) => {
                imp.via.set(Via::Pointer);
                (x, y)
            }
            _ => {
                imp.via
                    .set(if imp.last_activation.take().is_some_and(recent) {
                        Via::Keyboard
                    } else {
                        Via::Assistive
                    });
                (
                    rect.x() + rect.width() / 2.0,
                    rect.y() + rect.height() / 2.0,
                )
            }
        };
        let handlers: Vec<PressedFn> = self.imp().handlers.borrow().clone();
        for h in handlers {
            h(id, x, y);
        }
    }

    /// How the key clicked last was clicked: by the pointer, by Space or
    /// Enter with the focus on it, or through the accessibility
    /// interface (neither came before the click).
    pub fn clicked_via(&self) -> Via {
        self.imp().via.get()
    }

    /// A key typed on the keyboard: it is shown `active` for 110 ms.
    /// Returns the key's middle (keypad coordinates) if it is on screen.
    pub fn flash(&self, id: u32) -> Option<(f32, f32)> {
        let button = self.button(id)?;
        if !button.is_drawable() {
            return None;
        }
        let rect = button.compute_bounds(self)?;
        button.add_css_class("wc-flash");
        let b = button.clone();
        glib::timeout_add_local_once(FLASH, move || b.remove_css_class("wc-flash"));
        Some((
            rect.x() + rect.width() / 2.0,
            rect.y() + rect.height() / 2.0,
        ))
    }

    /// The keypad came into view (it was mapped, or its page's mode
    /// changed): the theme's layers are told (`show`) when it is next
    /// drawn.
    pub fn cascade(&self) {
        self.imp().show_pending.set(true);
        self.queue_draw();
    }

    /// The cells as a layer is told them: those on screen, by row and
    /// column, with the widget each is.
    fn cells(&self) -> Vec<(gtk::Widget, KeyCell)> {
        let imp = self.imp();
        let radius = imp.radius.get();
        let mut out = Vec::new();
        for c in imp.cells.borrow().iter() {
            let w = &c.widget;
            if !w.is_drawable() {
                continue;
            }
            let Some(rect) = w.compute_bounds(self) else {
                continue;
            };
            let flags = w.state_flags();
            out.push((
                w.clone(),
                KeyCell {
                    id: c.id,
                    kind: c.kind,
                    rect: [rect.x(), rect.y(), rect.width(), rect.height()],
                    row: c.row,
                    col: c.col,
                    radius: radius.min(rect.width().min(rect.height()) / 2.0),
                    enabled: w.is_sensitive(),
                    hovered: flags.contains(gtk::StateFlags::PRELIGHT),
                    pressed: flags.contains(gtk::StateFlags::ACTIVE) || w.has_css_class("wc-flash"),
                    checked: flags.contains(gtk::StateFlags::CHECKED) || w.has_css_class("wc-on"),
                },
            ));
        }
        out
    }

    fn draw(&self, s: &gtk::Snapshot) {
        let imp = self.imp();
        // The layer first: it has to be there to be told.
        let layer = self.layer();
        if imp.show_pending.replace(false)
            && let Some(hub) = Hub::current()
        {
            hub.layers().shown(self);
        }
        let layer =
            layer.filter(|_| Hub::current().is_some_and(|h| h.layers().draws(LayerKind::Keypad)));
        let Some(layer) = layer else {
            // No layer: the keys, as the toolkit draws them.
            let mut child = self.first_child();
            while let Some(c) = child {
                self.snapshot_child(&c, s);
                child = c.next_sibling();
            }
            return;
        };
        let (widgets, cells): (Vec<gtk::Widget>, Vec<KeyCell>) = self.cells().into_iter().unzip();
        let layout = {
            // The cells' tables are kept while only their states change.
            let mut told = imp.told.borrow_mut();
            let same = told.1.len() == cells.len()
                && told.1.iter().zip(&cells).all(|(a, b)| {
                    (a.id, a.kind, a.rect, a.row, a.col, a.radius)
                        == (b.id, b.kind, b.rect, b.row, b.col, b.radius)
                });
            if !same {
                told.0 += 1;
            }
            told.1.clone_from(&cells);
            told.0
        };
        let size = (self.width() as f32, self.height() as f32);
        let picture = layer.picture(
            size,
            Extra {
                keys: Some(Keys {
                    pad: imp.pad.get(),
                    layout,
                    cells: &cells,
                }),
                value: None,
            },
        );
        let places = picture.as_ref().map(|p| p.places.clone());
        for (i, (widget, cell)) in widgets.iter().zip(&cells).enumerate() {
            let place = places.as_ref().and_then(|p| p.get(i).copied().flatten());
            let Some(place) = place else {
                self.snapshot_child(widget, s);
                continue;
            };
            if place.opacity <= 0.0 || place.scale <= 0.0 {
                continue;
            }
            // The picture only: scaled about the cell's centre, moved,
            // faded.
            let [x, y, w, h] = cell.rect;
            let (cx, cy) = (x + w / 2.0, y + h / 2.0);
            s.save();
            s.translate(&graphene::Point::new(cx + place.dx, cy + place.dy));
            s.scale(place.scale, place.scale);
            s.translate(&graphene::Point::new(-cx, -cy));
            if place.opacity < 1.0 {
                s.push_opacity(f64::from(place.opacity));
            }
            self.snapshot_child(widget, s);
            if place.opacity < 1.0 {
                s.pop();
            }
            s.restore();
        }
        if let Some(picture) = picture {
            layers::paint(s, self, &picture.list, [&[], &[]]);
        }
    }
}
