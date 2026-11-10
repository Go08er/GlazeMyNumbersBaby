//! `Backdrop`: what is under every widget of the window, and what a
//! theme draws over all of them.
//!
//! It paints `window.background` (its fill child, which the stylesheet
//! styles from the theme's custom properties) and then the theme's
//! `background` layer, both inside one group at the Background opacity
//! setting; then its child at full strength; then the theme's `overlay`
//! layer. The window itself is a transparent surface, so below full
//! opacity the desktop shows through the background and nothing else
//! (docs/theming.md, "See-through windows").
//!
//! The overlay is paint, not a widget: it never takes a click.
//!
//! Being the window's content, the backdrop is also where the window's
//! doings are watched for the layers: the pointer moving over it and keys
//! pressed in it wake the clock that settles after 45 seconds, and the
//! window being the one in use lets layers animate. The controllers that
//! do so run nothing of their own: no timer, no frame callback.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gmnb_theme::layer::LayerKind;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::layers::{self, Extra, Instance};
use crate::theme::Hub;

mod imp {
    use super::*;

    pub struct Backdrop {
        /// Painted from CSS: `.wc-backdrop`.
        pub fill: gtk::Box,
        pub child: RefCell<Option<gtk::Widget>>,
        /// How opaque the background is.
        pub alpha: Cell<f32>,
        /// The theme's `background` and `overlay` layers.
        pub background: RefCell<Option<Rc<Instance>>>,
        pub overlay: RefCell<Option<Rc<Instance>>>,
        /// The window whose being in use is followed.
        pub watched: RefCell<Option<(gtk::Window, glib::SignalHandlerId)>>,
    }

    impl Default for Backdrop {
        fn default() -> Self {
            Self {
                fill: gtk::Box::builder()
                    .css_classes(["wc-backdrop"])
                    .can_target(false)
                    .can_focus(false)
                    .accessible_role(gtk::AccessibleRole::Presentation)
                    .build(),
                child: RefCell::new(None),
                alpha: Cell::new(1.0),
                background: RefCell::new(None),
                overlay: RefCell::new(None),
                watched: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Backdrop {
        const NAME: &'static str = "GmnbBackdrop";
        type Type = super::Backdrop;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BinLayout>();
            klass.set_css_name("backdrop");
        }
    }

    impl ObjectImpl for Backdrop {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_overflow(gtk::Overflow::Hidden);
            self.fill.set_parent(&*obj);
        }

        fn dispose(&self) {
            if let Some((window, id)) = self.watched.take() {
                window.disconnect(id);
            }
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Backdrop {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let size = (obj.width() as f32, obj.height() as f32);
            let layer = |slot: &RefCell<Option<Rc<Instance>>>| {
                let picture = slot
                    .borrow()
                    .as_ref()
                    .and_then(|i| i.picture(size, Extra::default()));
                if let Some(picture) = picture {
                    layers::paint(snapshot, &*obj, &picture.list, [&[], &[]]);
                }
            };
            let alpha = self.alpha.get();
            if alpha < 1.0 {
                snapshot.push_opacity(f64::from(alpha));
            }
            obj.snapshot_child(&self.fill, snapshot);
            layer(&self.background);
            if alpha < 1.0 {
                snapshot.pop();
            }
            if let Some(child) = self.child.borrow().as_ref() {
                obj.snapshot_child(child, snapshot);
            }
            layer(&self.overlay);
        }
    }
}

glib::wrapper! {
    pub struct Backdrop(ObjectSubclass<imp::Backdrop>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Backdrop {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Backdrop {
    pub fn set_child(&self, child: Option<&impl IsA<gtk::Widget>>) {
        if let Some(old) = self.imp().child.take() {
            old.unparent();
        }
        if let Some(child) = child {
            child.set_parent(self);
            self.imp().child.replace(Some(child.clone().upcast()));
        }
    }

    /// How opaque the background is drawn, 0.1 to 1: below 1 the desktop
    /// shows through it.
    pub fn set_alpha(&self, alpha: f32) {
        let alpha = alpha.clamp(0.0, 1.0);
        if self.imp().alpha.replace(alpha) != alpha {
            self.queue_draw();
        }
    }

    /// Draws `hub`'s theme's `background` and `overlay` layers here, and
    /// watches the window for them.
    pub fn set_hub(&self, hub: &Rc<Hub>) {
        let imp = self.imp();
        let all = hub.layers().clone();
        let background = all.instance(LayerKind::Background, self);
        let overlay = all.instance(LayerKind::Overlay, self);
        imp.background.replace(Some(background.clone()));
        imp.overlay.replace(Some(overlay.clone()));

        // The pointer over the window: it wakes the settled clock, and a
        // layer that asked for it is told where it is.
        let motion = gtk::EventControllerMotion::new();
        motion.set_propagation_phase(gtk::PropagationPhase::Capture);
        {
            let (all, bg, over) = (all.clone(), background.clone(), overlay.clone());
            motion.connect_motion(move |_, x, y| {
                all.poke();
                let at = Some((x as f32, y as f32));
                bg.set_pointer(at);
                over.set_pointer(at);
            });
        }
        motion.connect_leave(move |_| {
            background.set_pointer(None);
            overlay.set_pointer(None);
        });
        self.add_controller(motion);

        // A key pressed anywhere in the window (never claimed here).
        // Space and Enter are what use the control with the focus: a
        // control that then says it was used was used by the keyboard.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        {
            let all = all.clone();
            keys.connect_key_pressed(move |_, key, _, _| {
                all.poke();
                use gtk::gdk::Key;
                if matches!(
                    key,
                    Key::space | Key::Return | Key::KP_Enter | Key::ISO_Enter
                ) {
                    all.key_activated();
                }
                glib::Propagation::Proceed
            });
        }
        self.add_controller(keys);

        // And a press of the pointer: by the pointer.
        let click = gtk::GestureClick::new();
        click.set_button(0);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        {
            let all = all.clone();
            click.connect_pressed(move |_, _, _, _| all.pointer_pressed());
        }
        self.add_controller(click);

        // The window being the one in use.
        let weak = Rc::downgrade(&all);
        self.connect_root_notify(move |backdrop| {
            let imp = backdrop.imp();
            if let Some((window, id)) = imp.watched.take() {
                window.disconnect(id);
            }
            let (Some(all), Some(window)) = (
                weak.upgrade(),
                backdrop.root().and_downcast::<gtk::Window>(),
            ) else {
                return;
            };
            all.set_active(window.is_active());
            let weak = weak.clone();
            let id = window.connect_is_active_notify(move |window| {
                if let Some(all) = weak.upgrade() {
                    all.set_active(window.is_active());
                }
            });
            imp.watched.replace(Some((window, id)));
        });
    }
}
