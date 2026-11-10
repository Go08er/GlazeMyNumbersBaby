//! `WidthBin` — a bin that reports its allocated width, so pages can switch
//! between narrow and wide layouts. Layout changes are deferred to an idle
//! callback so the widget tree is never mutated mid-allocation.
//!
//! `SizeBin` — a bin exactly as high, or as wide, as its page says, whatever
//! its child would ask: the readout is `display.heights` high, the side
//! panel `layout.calculator.panel_width` wide.

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
        pub last_height: Cell<i32>,
        pub pending: Cell<bool>,
        pub callback: RefCell<Option<WidthFn>>,
        pub height_callback: RefCell<Option<WidthFn>>,
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
            let wider = width != self.last.replace(width);
            let taller = height != self.last_height.replace(height);
            if (wider || taller) && !self.pending.replace(true) {
                let weak = self.obj().downgrade();
                glib::idle_add_local_once(move || {
                    if let Some(bin) = weak.upgrade() {
                        let imp = bin.imp();
                        imp.pending.set(false);
                        if let Some(f) = imp.callback.borrow().as_ref() {
                            f(imp.last.get());
                        }
                        if let Some(f) = imp.height_callback.borrow().as_ref() {
                            f(imp.last_height.get());
                        }
                    }
                });
            }
        }
    }

    #[derive(Default)]
    pub struct SizeBin {
        /// `None`: the child's own.
        pub width: Cell<Option<i32>>,
        pub height: Cell<Option<i32>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SizeBin {
        const NAME: &'static str = "GmnbSizeBin";
        type Type = super::SizeBin;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for SizeBin {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for SizeBin {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let fixed = match orientation {
                gtk::Orientation::Horizontal => self.width.get(),
                _ => self.height.get(),
            };
            match (fixed, self.obj().first_child()) {
                (Some(n), _) => (n, n, -1, -1),
                (None, Some(child)) => {
                    let (min, nat, _, _) = child.measure(orientation, for_size);
                    (min, nat, -1, -1)
                }
                (None, None) => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            if let Some(child) = self.obj().first_child() {
                child.allocate(width, height, -1, None);
            }
        }
    }

    #[derive(Default)]
    pub struct Cells {
        pub gap: Cell<f32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Cells {
        const NAME: &'static str = "GmnbCells";
        type Type = super::Cells;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Cells {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Cells {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let shown = super::shown_children(self.obj().upcast_ref());
            let (mut min, mut nat) = (0, 0);
            // Every cell as wide (and the row as high) as the largest.
            for child in &shown {
                let (m, n, _, _) = child.measure(orientation, -1);
                min = min.max(m);
                nat = nat.max(n);
            }
            if orientation == gtk::Orientation::Horizontal && !shown.is_empty() {
                let n = shown.len() as i32;
                let gaps = (self.gap.get() * (n - 1) as f32).ceil() as i32;
                min = min * n + gaps;
                nat = nat * n + gaps;
            }
            (min, nat, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let shown = super::shown_children(self.obj().upcast_ref());
            if shown.is_empty() {
                return;
            }
            let n = shown.len() as f32;
            let gap = self.gap.get();
            let cell = ((width as f32 - gap * (n - 1.0)) / n).max(0.0);
            for (i, child) in shown.iter().enumerate() {
                let x = i as f32 * (cell + gap);
                let (min, _, _, _) = child.measure(gtk::Orientation::Horizontal, height);
                let w = (cell.round() as i32).max(min);
                // The cell's middle, to the fraction of a pixel.
                let at = x + (cell - w as f32) / 2.0;
                let t = gtk::gsk::Transform::new().translate(&gtk::graphene::Point::new(at, 0.0));
                child.allocate(w, height, -1, Some(t));
            }
        }
    }

    /// Where a strip's child goes: `width` px of the strip, from its
    /// start or from its end, the child `inset` px in from both sides of
    /// that room.
    #[derive(Clone, Copy)]
    pub struct Slot {
        pub end: bool,
        pub width: f32,
        pub inset: f32,
    }

    #[derive(Default)]
    pub struct Strip {
        pub slots: RefCell<Vec<(gtk::Widget, Slot)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Strip {
        const NAME: &'static str = "GmnbStrip";
        type Type = super::Strip;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Strip {
        fn dispose(&self) {
            self.slots.borrow_mut().clear();
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Strip {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let slots = self.slots.borrow();
            if orientation == gtk::Orientation::Horizontal {
                // DGMNB's rows overlap in a narrow window rather than
                // widen it: the strip asks for no width of its own.
                let nat: f32 = slots.iter().map(|(_, s)| s.width).sum();
                return (0, nat.ceil() as i32, -1, -1);
            }
            let most = slots
                .iter()
                .filter(|(w, _)| w.is_visible())
                .map(|(w, _)| w.measure(orientation, -1).0)
                .max()
                .unwrap_or(0);
            (most, most, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let (mut start, mut end) = (0.0, width as f32);
            for (w, s) in self.slots.borrow().iter() {
                if !w.is_visible() {
                    continue;
                }
                let x = if s.end {
                    end -= s.width;
                    end
                } else {
                    start += s.width;
                    start - s.width
                };
                let room = s.width - 2.0 * s.inset;
                let min = w.measure(gtk::Orientation::Horizontal, height).0;
                let cw = (room.round() as i32).max(min);
                let at = x + s.inset + (room - cw as f32) / 2.0;
                let t = gtk::gsk::Transform::new().translate(&gtk::graphene::Point::new(at, 0.0));
                w.allocate(cw, height, -1, Some(t));
            }
        }
    }
}

/// The children of `w` that are shown.
fn shown_children(w: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    let mut c = w.first_child();
    while let Some(child) = c {
        if child.is_visible() {
            out.push(child.clone());
        }
        c = child.next_sibling();
    }
    out
}

glib::wrapper! {
    /// A row of equal cells, as DGMNB lays one out (`Rect::columns`):
    /// `gap` apart, each child centred in its cell to the fraction of a
    /// pixel (GTK gives whole pixels and the spare ones to the first
    /// cells, which puts a label up to a pixel off DGMNB's).
    pub struct Cells(ObjectSubclass<imp::Cells>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Cells {
    pub fn new(gap: f32) -> Self {
        let cells: Self = glib::Object::new();
        cells.imp().gap.set(gap);
        cells
    }

    pub fn set_gap(&self, gap: f32) {
        if self.imp().gap.replace(gap) != gap {
            self.queue_resize();
        }
    }

    pub fn append(&self, child: &impl IsA<gtk::Widget>) {
        child.set_parent(self);
    }

    /// Takes `child` out (it was appended).
    pub fn remove(&self, child: &impl IsA<gtk::Widget>) {
        if child.parent().as_ref() == Some(self.upcast_ref()) {
            child.unparent();
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

    /// `f` is called with the width after it changed (and, once, the
    /// height did: the callbacks run together).
    pub fn connect_width(&self, f: impl Fn(i32) + 'static) {
        self.imp().callback.replace(Some(Box::new(f)));
    }

    /// `f` is called with the height after it changed.
    pub fn connect_height(&self, f: impl Fn(i32) + 'static) {
        self.imp().height_callback.replace(Some(Box::new(f)));
    }
}

glib::wrapper! {
    pub struct SizeBin(ObjectSubclass<imp::SizeBin>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl SizeBin {
    pub fn new(child: &impl IsA<gtk::Widget>) -> Self {
        let bin: Self = glib::Object::new();
        child.set_parent(&bin);
        bin
    }

    /// The bin's height, which its child is given whole.
    pub fn set_height(&self, height: i32) {
        let height = Some(height.max(0));
        if self.imp().height.replace(height) != height {
            self.queue_resize();
        }
    }

    /// The bin's width, which its child is given whole.
    pub fn set_width(&self, width: i32) {
        let width = Some(width.max(0));
        if self.imp().width.replace(width) != width {
            self.queue_resize();
        }
    }
}

glib::wrapper! {
    /// A row of slots, as DGMNB lays out the programmer bar: each child in
    /// so many pixels from the start, or from the end, whatever the row's
    /// width (in a narrow window they overlap, as DGMNB's do, rather than
    /// widen it).
    pub struct Strip(ObjectSubclass<imp::Strip>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Strip {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// `child` in the next `width` px from the start (`end`: from the
    /// end), `inset` px in from that room's sides.
    pub fn append(&self, child: &impl IsA<gtk::Widget>, end: bool, width: f32, inset: f32) {
        let child = child.as_ref();
        child.set_parent(self);
        self.imp()
            .slots
            .borrow_mut()
            .push((child.clone(), imp::Slot { end, width, inset }));
    }
}

impl Default for Strip {
    fn default() -> Self {
        Self::new()
    }
}
