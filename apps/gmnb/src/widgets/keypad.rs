//! `Keypad` — a homogeneous grid of glass keys with three indulgences:
//!
//! * **Reveal**: a Fluent-style light that follows the pointer and lights up
//!   key *borders* near it (a nod to Windows' Reveal highlight), plus a soft
//!   glow on the hovered key.
//! * **Ripples** expanding from the exact press point, clipped to the key.
//! * **Cascade**: keys spring in diagonally whenever the keypad is shown.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene, gsk};

use super::animations_enabled;
use crate::theme::{Scheme, rgba};

pub const KEY_RADIUS: f32 = 14.0;
const RIPPLE_LIFE: f32 = 0.55;
const CASCADE_STEP: f32 = 0.022;
const CASCADE_DUR: f32 = 0.46;
const REVEAL_RADIUS: f32 = 120.0;

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

#[derive(Clone, Copy)]
pub struct Ripple {
    rect: graphene::Rect,
    x: f32,
    y: f32,
    born: Instant,
    hot: bool,
}

type PressedFn = Rc<dyn Fn(u32, f32, f32)>;

mod imp {
    use super::*;

    pub struct Keypad {
        pub keys: RefCell<HashMap<u32, gtk::Button>>,
        pub child_pos: RefCell<HashMap<gtk::Widget, (i32, i32)>>,
        pub handlers: RefCell<Vec<PressedFn>>,
        pub pointer: Cell<Option<(f32, f32)>>,
        pub last_press: Cell<Option<(f32, f32, Instant)>>,
        pub ripples: RefCell<Vec<Ripple>>,
        pub cascade_start: Cell<Option<Instant>>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
        pub scheme: Cell<Option<Scheme>>,
    }

    impl Default for Keypad {
        fn default() -> Self {
            Self {
                keys: RefCell::default(),
                child_pos: RefCell::default(),
                handlers: RefCell::default(),
                pointer: Cell::new(None),
                last_press: Cell::new(None),
                ripples: RefCell::default(),
                cascade_start: Cell::new(None),
                tick: RefCell::new(None),
                scheme: Cell::new(None),
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
            grid.set_row_spacing(5);
            grid.set_column_spacing(5);

            // Reveal light and press-point ripples.
            {
                let motion = gtk::EventControllerMotion::new();
                let weak = obj.downgrade();
                motion.connect_motion(move |_, x, y| {
                    if let Some(k) = weak.upgrade() {
                        k.imp().pointer.set(Some((x as f32, y as f32)));
                        k.queue_draw();
                    }
                });
                let weak = obj.downgrade();
                motion.connect_leave(move |_| {
                    if let Some(k) = weak.upgrade() {
                        k.imp().pointer.set(None);
                        k.queue_draw();
                    }
                });
                obj.add_controller(motion);

                // Observe presses (capture phase, never claimed) to learn the exact
                // point a key was hit, so ripples start under the finger.
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
            }

            obj.connect_map(|k| k.cascade());

            // Screenshot/dev hook: pretend the pointer hovers at "x,y".
            if let Some((x, y)) = std::env::var("GMNB_POINTER").ok().and_then(|v| {
                let (a, b) = v.split_once(',')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            }) {
                self.pointer.set(Some((x, y)));
            }
        }

        fn dispose(&self) {
            if let Some(id) = self.tick.take() {
                id.remove();
            }
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

fn ease_out_back(p: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (p - 1.0).powi(3) + c1 * (p - 1.0).powi(2)
}

fn ease_out_cubic(p: f32) -> f32 {
    1.0 - (1.0 - p).powi(3)
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

    pub fn set_spacing(&self, spacing: i32) {
        let g = self.grid_layout();
        g.set_row_spacing(spacing as u32);
        g.set_column_spacing(spacing as u32);
    }

    pub fn set_scheme(&self, scheme: Scheme) {
        self.imp().scheme.set(Some(scheme));
        self.queue_draw();
    }

    /// Add a key at (row, col) spanning (rows, cols). Returns the button.
    pub fn add(&self, key: Key, row: i32, col: i32, rows: i32, cols: i32) -> gtk::Button {
        let child: gtk::Widget = match key.icon {
            Some(path) => {
                let icon = super::icon::PathIcon::new(path, 22);
                icon.set_stroke_width(if key.kind == KeyKind::Equals {
                    2.3
                } else {
                    1.9
                });
                icon.upcast()
            }
            None => {
                let label = gtk::Label::new(None);
                label.set_markup(&key.label);
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
            .focus_on_click(false)
            .hexpand(true)
            .vexpand(true)
            .css_classes(["wc-key", kind_css(key.kind)])
            .build();
        if let Some(tip) = &key.tooltip {
            button.set_tooltip_text(Some(tip));
        }
        if let Some(name) = key.a11y.as_ref().or(key.tooltip.as_ref()) {
            button.update_property(&[gtk::accessible::Property::Label(name)]);
        }
        self.attach(&button, row, col, rows, cols);

        let id = key.id;
        let weak = self.downgrade();
        button.connect_clicked(move |b| {
            if let Some(k) = weak.upgrade() {
                k.on_key_clicked(id, b);
            }
        });
        self.imp().keys.borrow_mut().insert(id, button.clone());
        button
    }

    /// Attach an arbitrary widget (e.g. a menu button) into the grid.
    pub fn attach(&self, widget: &impl IsA<gtk::Widget>, row: i32, col: i32, rows: i32, cols: i32) {
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
        self.imp()
            .child_pos
            .borrow_mut()
            .insert(widget.clone().upcast(), (row, col));
    }

    pub fn ids(&self) -> Vec<u32> {
        self.imp().keys.borrow().keys().copied().collect()
    }

    pub fn button(&self, id: u32) -> Option<gtk::Button> {
        self.imp().keys.borrow().get(&id).cloned()
    }

    pub fn set_key_label(&self, id: u32, markup: &str) {
        if let Some(label) = self
            .button(id)
            .and_then(|b| b.child())
            .and_downcast::<gtk::Label>()
        {
            label.set_markup(markup);
        }
    }

    /// Relabel a key and keep its tooltip and accessible name in step.
    pub fn set_key(&self, id: u32, markup: &str, tip: &str) {
        self.set_key_label(id, markup);
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

    pub fn connect_pressed(&self, f: impl Fn(u32, f32, f32) + 'static) {
        self.imp().handlers.borrow_mut().push(Rc::new(f));
    }

    fn on_key_clicked(&self, id: u32, button: &gtk::Button) {
        let Some(rect) = button.compute_bounds(self) else {
            return;
        };
        let (x, y) = match self.imp().last_press.take() {
            Some((x, y, t))
                if t.elapsed().as_secs_f32() < 1.0
                    && rect.contains_point(&graphene::Point::new(x, y)) =>
            {
                (x, y)
            }
            _ => (
                rect.x() + rect.width() / 2.0,
                rect.y() + rect.height() / 2.0,
            ),
        };
        let hot = button.has_css_class("wc-eq");
        self.add_ripple(rect, x, y, hot);
        let handlers: Vec<PressedFn> = self.imp().handlers.borrow().clone();
        for h in handlers {
            h(id, x, y);
        }
    }

    /// Visual feedback for a key triggered from the keyboard. Returns the key
    /// centre (keypad coordinates) if it exists.
    pub fn flash(&self, id: u32) -> Option<(f32, f32)> {
        let button = self.button(id)?;
        if !button.is_drawable() {
            return None;
        }
        let rect = button.compute_bounds(self)?;
        let (x, y) = (
            rect.x() + rect.width() / 2.0,
            rect.y() + rect.height() / 2.0,
        );
        self.add_ripple(rect, x, y, button.has_css_class("wc-eq"));
        button.add_css_class("wc-flash");
        let b = button.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(110), move || {
            b.remove_css_class("wc-flash")
        });
        Some((x, y))
    }

    fn add_ripple(&self, rect: graphene::Rect, x: f32, y: f32, hot: bool) {
        if !animations_enabled() {
            return;
        }
        self.imp().ripples.borrow_mut().push(Ripple {
            rect,
            x,
            y,
            born: Instant::now(),
            hot,
        });
        self.ensure_ticking();
    }

    /// Play the entrance cascade.
    pub fn cascade(&self) {
        if !animations_enabled() {
            return;
        }
        self.imp().cascade_start.set(Some(Instant::now()));
        self.ensure_ticking();
    }

    fn cascade_len(&self) -> f32 {
        let pos = self.imp().child_pos.borrow();
        let max_diag = pos.values().map(|(r, c)| r + c).max().unwrap_or(0);
        max_diag as f32 * CASCADE_STEP + CASCADE_DUR
    }

    fn ensure_ticking(&self) {
        if self.imp().tick.borrow().is_some() {
            return;
        }
        let id = self.add_tick_callback(|k, _| {
            let imp = k.imp();
            imp.ripples
                .borrow_mut()
                .retain(|r| r.born.elapsed().as_secs_f32() < RIPPLE_LIFE);
            if let Some(start) = imp.cascade_start.get()
                && start.elapsed().as_secs_f32() > k.cascade_len()
            {
                imp.cascade_start.set(None);
            }
            k.queue_draw();
            if imp.ripples.borrow().is_empty() && imp.cascade_start.get().is_none() {
                imp.tick.replace(None);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        self.imp().tick.replace(Some(id));
    }

    fn draw(&self, s: &gtk::Snapshot) {
        let imp = self.imp();
        let cascade_t = imp.cascade_start.get().map(|t| t.elapsed().as_secs_f32());
        let pos = imp.child_pos.borrow();

        let mut child = self.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            match (cascade_t, pos.get(&c), c.compute_bounds(self)) {
                (Some(t), Some(&(row, col)), Some(rect)) => {
                    let delay = (row + col) as f32 * CASCADE_STEP;
                    let p = ((t - delay) / CASCADE_DUR).clamp(0.0, 1.0);
                    if p <= 0.0 {
                        child = next;
                        continue;
                    }
                    let scale = 0.86 + 0.14 * ease_out_back(p);
                    let cx = rect.x() + rect.width() / 2.0;
                    let cy = rect.y() + rect.height() / 2.0;
                    s.save();
                    s.translate(&graphene::Point::new(
                        cx,
                        cy + (1.0 - ease_out_cubic(p)) * 16.0,
                    ));
                    s.scale(scale, scale);
                    s.translate(&graphene::Point::new(-cx, -cy));
                    s.push_opacity(ease_out_cubic(p) as f64);
                    self.snapshot_child(&c, s);
                    s.pop();
                    s.restore();
                }
                _ => self.snapshot_child(&c, s),
            }
            child = next;
        }
        drop(pos);

        let Some(scheme) = imp.scheme.get() else {
            return;
        };
        let light = if scheme.dark {
            [1.0, 1.0, 1.0]
        } else {
            scheme.accent
        };

        // Ripples.
        for r in imp.ripples.borrow().iter() {
            let age = r.born.elapsed().as_secs_f32() / RIPPLE_LIFE;
            if age >= 1.0 {
                continue;
            }
            let reach = (r.rect.width().hypot(r.rect.height())) * 1.1;
            let radius = 6.0 + ease_out_cubic(age) * reach;
            let a = (1.0 - age).powf(1.4) * if scheme.dark { 0.26 } else { 0.30 };
            let color = if r.hot { scheme.on_hot } else { light };
            let rr = gsk::RoundedRect::from_rect(r.rect, KEY_RADIUS);
            s.push_rounded_clip(&rr);
            s.append_radial_gradient(
                &r.rect,
                &graphene::Point::new(r.x, r.y),
                radius,
                radius,
                0.0,
                1.0,
                &[
                    gsk::ColorStop::new(0.0, rgba(color, a * 0.5)),
                    gsk::ColorStop::new(0.8, rgba(color, a)),
                    gsk::ColorStop::new(1.0, rgba(color, 0.0)),
                ],
            );
            s.pop();
        }

        // Reveal: light the borders near the pointer, glow the hovered key.
        let Some((px, py)) = imp.pointer.get() else {
            return;
        };
        let pointer = graphene::Point::new(px, py);
        let border_a = if scheme.dark { 0.55 } else { 0.75 };
        let mut child = self.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            if !c.is_sensitive() || !c.is_drawable() {
                continue;
            }
            let Some(rect) = c.compute_bounds(self) else {
                continue;
            };
            let nearest_x = px.clamp(rect.x(), rect.x() + rect.width());
            let nearest_y = py.clamp(rect.y(), rect.y() + rect.height());
            let dist = (px - nearest_x).hypot(py - nearest_y);
            if dist > REVEAL_RADIUS {
                continue;
            }
            let rr = gsk::RoundedRect::from_rect(rect.inset_r(0.5, 0.5), KEY_RADIUS - 0.5);
            let builder = gsk::PathBuilder::new();
            builder.add_rounded_rect(&rr);
            let path = builder.to_path();
            s.push_stroke(&path, &gsk::Stroke::new(1.25));
            s.append_radial_gradient(
                &rect,
                &pointer,
                REVEAL_RADIUS,
                REVEAL_RADIUS,
                0.0,
                1.0,
                &[
                    gsk::ColorStop::new(0.0, rgba(light, border_a)),
                    gsk::ColorStop::new(1.0, rgba(light, 0.0)),
                ],
            );
            s.pop();

            if rect.contains_point(&pointer) {
                let glow_r = rect.width().max(rect.height()) * 0.95;
                s.push_rounded_clip(&gsk::RoundedRect::from_rect(rect, KEY_RADIUS));
                s.append_radial_gradient(
                    &rect,
                    &pointer,
                    glow_r,
                    glow_r,
                    0.0,
                    1.0,
                    &[
                        gsk::ColorStop::new(
                            0.0,
                            rgba(light, if scheme.dark { 0.12 } else { 0.16 }),
                        ),
                        gsk::ColorStop::new(1.0, rgba(light, 0.0)),
                    ],
                );
                s.pop();
            }
        }
    }
}
