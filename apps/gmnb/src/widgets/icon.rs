//! `PathIcon` — line icons stroked from SVG path data at runtime.
//!
//! Using `gsk::Path` instead of icon files means icons are crisp at any
//! scale, take their colour from CSS `color`, and can *draw themselves in*
//! (animated dash) when a navigation row is selected — purely for joy.

use std::cell::{Cell, RefCell};
use std::time::Instant;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene, gsk};

use super::animations_enabled;

/// 24×24-viewbox stroke paths.
pub use appcore::icons as paths;

mod imp {
    use super::*;

    pub struct PathIcon {
        pub path: RefCell<Option<gsk::Path>>,
        pub length: Cell<f32>,
        pub size: Cell<i32>,
        pub width: Cell<f32>,
        /// The line's width in pixels, when a style gives it (else it is
        /// worked out from `width`, as 0.2 did).
        pub stroke_px: Cell<Option<f32>>,
        pub draw_start: Cell<Option<Instant>>,
        /// It asks for no least size ([`super::PathIcon::set_loose`]).
        pub loose: Cell<bool>,
    }

    impl Default for PathIcon {
        fn default() -> Self {
            Self {
                path: RefCell::new(None),
                length: Cell::new(0.0),
                size: Cell::new(18),
                width: Cell::new(1.75),
                stroke_px: Cell::new(None),
                draw_start: Cell::new(None),
                loose: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PathIcon {
        const NAME: &'static str = "GmnbPathIcon";
        type Type = super::PathIcon;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("pathicon");
            klass.set_accessible_role(gtk::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for PathIcon {}

    impl WidgetImpl for PathIcon {
        fn measure(&self, _o: gtk::Orientation, _f: i32) -> (i32, i32, i32, i32) {
            let s = self.size.get();
            (if self.loose.get() { 0 } else { s }, s, -1, -1)
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            let Some(path) = self.path.borrow().clone() else {
                return;
            };
            let obj = self.obj();
            let size = self.size.get() as f32;
            let k = size / 24.0;
            let ox = ((obj.width() as f32 - size) / 2.0).round();
            let oy = ((obj.height() as f32 - size) / 2.0).round();
            // On-screen width shrinks only gently with size, so small icons
            // don't go spidery; divide by k because the path is scaled.
            let px = self
                .stroke_px
                .get()
                .unwrap_or_else(|| self.width.get() * (0.6 + 0.4 * k.min(1.5)));
            let stroke = gsk::Stroke::new(px / k);
            stroke.set_line_cap(gsk::LineCap::Round);
            stroke.set_line_join(gsk::LineJoin::Round);
            if let Some(t) = self.draw_start.get() {
                let p = (t.elapsed().as_secs_f32() / 0.7).min(1.0);
                let e = 1.0 - (1.0 - p).powi(3);
                let len = self.length.get().max(1.0);
                stroke.set_dash(&[len * e, len]);
            }
            s.save();
            s.translate(&graphene::Point::new(ox, oy));
            s.scale(k, k);
            s.append_stroke(&path, &stroke, &obj.color());
            s.restore();
        }
    }
}

glib::wrapper! {
    pub struct PathIcon(ObjectSubclass<imp::PathIcon>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl PathIcon {
    pub fn new(data: &str, size: i32) -> Self {
        let icon: Self = glib::Object::new();
        icon.set_path(data);
        icon.imp().size.set(size);
        icon.set_valign(gtk::Align::Center);
        icon.set_halign(gtk::Align::Center);
        icon
    }

    pub fn set_path(&self, data: &str) {
        match gsk::Path::parse(data) {
            Ok(path) => {
                self.imp().length.set(gsk::PathMeasure::new(&path).length());
                self.imp().path.replace(Some(path));
            }
            Err(e) => glib::g_warning!("gmnb", "bad icon path {data:?}: {e}"),
        }
        self.queue_draw();
    }

    /// An icon that asks for no least size: it is drawn at its size
    /// about the middle of whatever it is given, past its edges if need
    /// be (a key's icon: DGMNB's keys shrink below what they show).
    pub fn set_loose(&self, loose: bool) {
        if self.imp().loose.replace(loose) != loose {
            self.queue_resize();
        }
    }

    pub fn set_stroke_width(&self, w: f32) {
        self.imp().width.set(w);
        self.queue_draw();
    }

    /// The icon as a style says: its drawn size, and its line's width,
    /// both in pixels.
    pub fn set_style(&self, size: f32, stroke: f32) {
        let imp = self.imp();
        let size = size.round().max(1.0) as i32;
        imp.stroke_px.set(Some(stroke));
        if imp.size.replace(size) != size {
            self.queue_resize();
        }
        self.queue_draw();
    }

    /// Re-trace the icon's strokes from nothing.
    pub fn animate_draw(&self) {
        if !animations_enabled() {
            return;
        }
        self.imp().draw_start.set(Some(Instant::now()));
        self.add_tick_callback(|icon, _| {
            icon.queue_draw();
            let done = icon
                .imp()
                .draw_start
                .get()
                .is_none_or(|t| t.elapsed().as_secs_f32() > 0.7);
            if done {
                icon.imp().draw_start.set(None);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
}

/// A flat toggle button showing a path icon.
pub fn icon_toggle(data: &str, tooltip: &str) -> gtk::ToggleButton {
    let b = gtk::ToggleButton::builder()
        .child(&PathIcon::new(data, 18))
        .tooltip_text(tooltip)
        .css_classes(["flat", "wc-icon-button"])
        .build();
    b.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    b
}
