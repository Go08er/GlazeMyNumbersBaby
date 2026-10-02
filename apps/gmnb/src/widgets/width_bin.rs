//! `WidthBin` — a bin that reports its allocated width, so pages can switch
//! between narrow and wide layouts. Layout changes are deferred to an idle
//! callback so the widget tree is never mutated mid-allocation.

use std::cell::{Cell, RefCell};

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

type WidthFn = Box<dyn Fn(i32)>;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct WidthBin {
        pub last: Cell<i32>,
        pub pending: Cell<bool>,
        pub callback: RefCell<Option<WidthFn>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WidthBin {
        const NAME: &'static str = "GmnbWidthBin";
        type Type = super::WidthBin;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for WidthBin {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    // No layout manager on purpose: with one, GTK never calls size_allocate.
    impl WidgetImpl for WidthBin {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.obj().first_child() {
                Some(child) => child.measure(orientation, for_size),
                None => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.obj().first_child() {
                child.allocate(width, height, baseline, None);
            }
            if width != self.last.get() {
                self.last.set(width);
                if !self.pending.replace(true) {
                    let weak = self.obj().downgrade();
                    glib::idle_add_local_once(move || {
                        if let Some(bin) = weak.upgrade() {
                            bin.imp().pending.set(false);
                            if let Some(f) = bin.imp().callback.borrow().as_ref() {
                                f(bin.imp().last.get());
                            }
                        }
                    });
                }
            }
        }
    }
}

glib::wrapper! {
    pub struct WidthBin(ObjectSubclass<imp::WidthBin>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl WidthBin {
    pub fn new(child: &impl IsA<gtk::Widget>) -> Self {
        let bin: Self = glib::Object::new();
        child.set_parent(&bin);
        bin
    }

    pub fn connect_width(&self, f: impl Fn(i32) + 'static) {
        self.imp().callback.replace(Some(Box::new(f)));
    }
}
