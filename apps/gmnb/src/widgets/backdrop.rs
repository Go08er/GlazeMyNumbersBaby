//! `Backdrop`: what is under every widget of the window.
//!
//! It paints `window.background` (its fill child, which the stylesheet
//! styles from the theme's custom properties) inside one group at the
//! Background opacity setting, then its child at full strength. The
//! window itself is a transparent surface, so below full opacity the
//! desktop shows through the background and nothing else.
//!
//! A theme's `background` layer will be drawn in the same group, over the
//! fill (docs/theming.md, "See-through windows").

use std::cell::{Cell, RefCell};

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;

    pub struct Backdrop {
        /// Painted from CSS: `.wc-backdrop`.
        pub fill: gtk::Box,
        pub child: RefCell<Option<gtk::Widget>>,
        /// How opaque the background is.
        pub alpha: Cell<f32>,
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
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Backdrop {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let alpha = self.alpha.get();
            if alpha < 1.0 {
                snapshot.push_opacity(f64::from(alpha));
            }
            obj.snapshot_child(&self.fill, snapshot);
            if alpha < 1.0 {
                snapshot.pop();
            }
            if let Some(child) = self.child.borrow().as_ref() {
                obj.snapshot_child(child, snapshot);
            }
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
}
