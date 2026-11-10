//! Two small pieces the pages lay out with as DGMNB does.
//!
//! * [`FitLabel`]: one line of text in a style's font, shrunk to fit its
//!   width (down to a least size) and set with its capitals centred
//!   between its top and bottom, as DGMNB's `label_fit`. The date page's
//!   results and the converter's values. Its colour and halo are the CSS
//!   `color` and `text-shadow` it gets.
//! * [`MaxWidth`]: a bin that gives its child the width it has, but no more
//!   than a most, at its start (DGMNB's columns and buttons are
//!   `w.min(MAX)` from the left; `AdwClamp` centres).

use std::cell::{Cell, RefCell};

use gmnb_theme::types::Font;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, pango};

/// A Pango font description and the attributes that go with it (features,
/// letter spacing) for `font` at `size` pixels. The bundled Inter is given
/// its optical size explicitly (`Font::effective_variations`), as the
/// stylesheet's fonts are.
pub fn pango_font(font: &Font, size: f32) -> (pango::FontDescription, pango::AttrList) {
    let mut fd = pango::FontDescription::new();
    fd.set_family(&font.family.join(","));
    fd.set_absolute_size(f64::from(size) * f64::from(pango::SCALE));
    fd.set_weight(pango::Weight::__Unknown(font.weight.round() as i32));
    fd.set_style(if font.italic {
        pango::Style::Italic
    } else {
        pango::Style::Normal
    });
    let sized = Font {
        size,
        ..font.clone()
    };
    let variations: Vec<String> = sized
        .effective_variations()
        .iter()
        .map(|(tag, v)| format!("{tag}={v}"))
        .collect();
    if !variations.is_empty() {
        fd.set_variations(Some(&variations.join(",")));
    }
    let attrs = pango::AttrList::new();
    if !font.features.is_empty() {
        let features: Vec<String> = font
            .features
            .iter()
            .map(|(tag, on)| format!("{tag} {on}"))
            .collect();
        attrs.insert(pango::AttrFontFeatures::new(&features.join(", ")));
    }
    if font.spacing != 0.0 {
        let spacing = font.spacing * size * pango::SCALE as f32;
        attrs.insert(pango::AttrInt::new_letter_spacing(spacing.round() as i32));
    }
    (fd, attrs)
}

mod imp {
    use super::*;

    pub struct FitLabel {
        pub text: RefCell<String>,
        pub font: RefCell<Option<Font>>,
        pub min_size: Cell<f32>,
    }

    impl Default for FitLabel {
        fn default() -> Self {
            Self {
                text: RefCell::default(),
                font: RefCell::default(),
                min_size: Cell::new(1.0),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FitLabel {
        const NAME: &'static str = "GmnbFitLabel";
        type Type = super::FitLabel;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("fitlabel");
            klass.set_accessible_role(gtk::AccessibleRole::Label);
        }
    }

    impl ObjectImpl for FitLabel {}

    impl WidgetImpl for FitLabel {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk::Orientation::Horizontal {
                // It shrinks to whatever width it is given.
                let natural = self.obj().layout_at(None).pixel_size().0;
                (0, natural, -1, -1)
            } else {
                let natural = self.obj().layout_at(None).pixel_size().1;
                (natural, natural, -1, -1)
            }
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            let obj = self.obj();
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            let layout = obj.layout_at(Some(w));
            // The capitals centred, on a whole pixel (DGMNB rounds its
            // baselines): `H`'s ink is the cap height.
            let probe = obj.layout_at_size(obj.fitted_size(w), "H");
            let (ink, _) = probe.pixel_extents();
            let baseline = (h / 2.0 + ink.height() as f32 / 2.0).round();
            let y = baseline - layout.baseline() as f32 / pango::SCALE as f32;
            #[allow(deprecated)]
            s.render_layout(&obj.style_context(), 0.0, f64::from(y), &layout);
        }
    }
}

glib::wrapper! {
    pub struct FitLabel(ObjectSubclass<imp::FitLabel>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for FitLabel {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl FitLabel {
    pub fn new() -> Self {
        Self::default()
    }

    /// One that is also a control (a converter value, picked to type
    /// into): a button to assistive technology and a stop for Tab.
    pub fn button() -> Self {
        glib::Object::builder()
            .property("accessible-role", gtk::AccessibleRole::Button)
            .property("focusable", true)
            .build()
    }

    pub fn text(&self) -> String {
        self.imp().text.borrow().clone()
    }

    pub fn set_text(&self, text: &str) {
        if *self.imp().text.borrow() != text {
            self.imp().text.replace(text.to_string());
            self.queue_resize();
        }
    }

    /// The font at its largest; it shrinks to fit down to `min_size`.
    pub fn set_font(&self, font: &Font, min_size: f32) {
        let imp = self.imp();
        if imp.font.borrow().as_ref() != Some(font) || imp.min_size.get() != min_size {
            imp.font.replace(Some(font.clone()));
            imp.min_size.set(min_size);
            self.queue_resize();
        }
    }

    /// The size the text is set at in `width` (its font's own if it fits).
    fn fitted_size(&self, width: f32) -> f32 {
        let imp = self.imp();
        let Some(size) = imp.font.borrow().as_ref().map(|f| f.size) else {
            return 15.0;
        };
        let text = imp.text.borrow();
        let full = self.layout_at_size(size, &text).pixel_size().0 as f32;
        if full > width && full > 0.0 {
            (size * width / full).max(imp.min_size.get())
        } else {
            size
        }
    }

    fn layout_at(&self, width: Option<f32>) -> pango::Layout {
        let size = match width {
            Some(w) => self.fitted_size(w),
            None => self.imp().font.borrow().as_ref().map_or(15.0, |f| f.size),
        };
        let text = self.imp().text.borrow().clone();
        self.layout_at_size(size, &text)
    }

    fn layout_at_size(&self, size: f32, text: &str) -> pango::Layout {
        let layout = self.create_pango_layout(Some(text));
        if let Some(font) = self.imp().font.borrow().as_ref() {
            let (fd, attrs) = pango_font(font, size);
            layout.set_font_description(Some(&fd));
            layout.set_attributes(Some(&attrs));
        }
        layout
    }
}

mod max_imp {
    use super::*;

    #[derive(Default)]
    pub struct MaxWidth {
        pub max: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MaxWidth {
        const NAME: &'static str = "GmnbMaxWidth";
        type Type = super::MaxWidth;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for MaxWidth {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for MaxWidth {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(child) = self.obj().first_child() else {
                return (0, 0, -1, -1);
            };
            let max = self.max.get();
            if orientation == gtk::Orientation::Horizontal {
                // Never less than the child needs.
                let (min, nat, _, _) = child.measure(orientation, for_size);
                (min, nat.min(max).max(min), -1, -1)
            } else {
                let for_width = if for_size < 0 {
                    -1
                } else {
                    for_size.min(max.max(child.measure(gtk::Orientation::Horizontal, -1).0))
                };
                let (min, nat, _, _) = child.measure(orientation, for_width);
                (min, nat, -1, -1)
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.obj().first_child() {
                let least = child.measure(gtk::Orientation::Horizontal, -1).0;
                let w = width.min(self.max.get()).max(least);
                child.allocate(w, height, baseline, None);
            }
        }
    }
}

glib::wrapper! {
    pub struct MaxWidth(ObjectSubclass<max_imp::MaxWidth>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl MaxWidth {
    pub fn new(child: &impl IsA<gtk::Widget>, max: i32) -> Self {
        let bin: Self = glib::Object::new();
        bin.imp().max.set(max);
        child.set_parent(&bin);
        bin
    }

    pub fn set_max(&self, max: i32) {
        if self.imp().max.replace(max) != max {
            self.queue_resize();
        }
    }
}
