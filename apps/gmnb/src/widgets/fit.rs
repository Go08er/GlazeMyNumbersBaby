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
    let mut variations: Vec<String> = sized
        .effective_variations()
        .iter()
        .map(|(tag, v)| format!("{tag}={v}"))
        .collect();
    // The weight as an axis too: a named instance of the font that
    // fontconfig picks for it (Inter's "Light") would otherwise bring its
    // own optical size.
    if !font.variations.iter().any(|(tag, _)| tag == "wght") {
        variations.push(format!("wght={}", font.weight.round()));
    }
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

/// Where a [`FitLabel`]'s line sits across its width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    Center,
}

/// Text set before the line in a font and colour of its own (the
/// converter's currency symbol), and the space after it.
#[derive(Clone, Debug, PartialEq)]
pub struct Prefix {
    pub text: String,
    pub font: Font,
    pub color: gmnb_theme::types::Rgba,
    pub gap: f32,
}

/// Pixels from Pango units.
fn px(units: i32) -> f32 {
    units as f32 / pango::SCALE as f32
}

mod imp {
    use super::*;

    pub struct FitLabel {
        pub text: RefCell<String>,
        pub font: RefCell<Option<Font>>,
        pub min_size: Cell<f32>,
        pub align: Cell<Align>,
        pub prefix: RefCell<Option<Prefix>>,
        /// Shadows round the text (a see-through window's halo).
        pub halo: RefCell<Vec<gmnb_theme::types::Shadow>>,
    }

    impl Default for FitLabel {
        fn default() -> Self {
            Self {
                text: RefCell::default(),
                font: RefCell::default(),
                min_size: Cell::new(1.0),
                align: Cell::new(Align::Start),
                prefix: RefCell::default(),
                halo: RefCell::default(),
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
            let obj = self.obj();
            let layout = obj.layout_at(None);
            let (_, logical) = layout.extents();
            if orientation == gtk::Orientation::Horizontal {
                // It shrinks to whatever width it is given.
                let natural = obj.prefix_width() + px(logical.width());
                (0, natural.ceil() as i32, -1, -1)
            } else {
                let natural = px(logical.height()).ceil() as i32;
                (natural, natural, -1, -1)
            }
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            let obj = self.obj();
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            let lead = obj.prefix_width();
            let size = obj.fitted_size(w - lead);
            let layout = obj.layout_at_size(size, &self.text.borrow(), None);
            // The capitals centred, on a whole pixel, as DGMNB sets a line
            // (`H`'s ink is the cap height).
            let baseline = |font: Option<&Font>, size: f32| {
                let probe = obj.layout_at_size(size, "H", font);
                let (ink, _) = probe.extents();
                (h / 2.0 + px(ink.height()) / 2.0).round()
            };
            let (_, logical) = layout.extents();
            let x = match self.align.get() {
                Align::Start => lead,
                Align::Center => (w - lead - px(logical.width())) / 2.0 + lead,
            };
            let halo = self.halo.borrow();
            if !halo.is_empty() {
                let shadows: Vec<gtk::gsk::Shadow> = halo
                    .iter()
                    .map(|h| gtk::gsk::Shadow::new(rgba(h.color), h.x, h.y, h.blur))
                    .collect();
                s.push_shadow(&shadows);
            }
            if let Some(p) = self.prefix.borrow().as_ref() {
                let pl = obj.layout_at_size(p.font.size, &p.text, Some(&p.font));
                glyphs(
                    s,
                    &pl,
                    0.0,
                    baseline(Some(&p.font), p.font.size),
                    &rgba(p.color),
                );
            }
            glyphs(s, &layout, x, baseline(None, size), &obj.color());
            if !halo.is_empty() {
                s.pop();
            }
        }
    }
}

fn rgba(c: gmnb_theme::types::Rgba) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(c[0], c[1], c[2], c[3])
}

/// `layout`'s line at `x`, its baseline at `baseline`, each glyph placed
/// at the nearest quarter pixel, as DGMNB places its glyphs (the
/// toolkit's own rounding of a position put some a quarter pixel off).
fn glyphs(
    s: &gtk::Snapshot,
    layout: &pango::Layout,
    x: f32,
    baseline: f32,
    color: &gtk::gdk::RGBA,
) {
    let quarter = |v: f32| (v * 4.0).round() / 4.0;
    let mut iter = layout.iter();
    loop {
        if let Some(run) = iter.run_readonly() {
            let (_, logical) = iter.run_extents();
            let origin = quarter(x + px(logical.x()));
            let font = run.item().analysis().font();
            let mut glyphs = run.glyph_string();
            let mut pen = 0;
            for g in glyphs.glyph_info_mut() {
                let geo = g.geometry_mut();
                let at = x + px(logical.x()) + px(pen + geo.x_offset());
                let want = quarter(at) - origin;
                geo.set_x_offset((want * pango::SCALE as f32).round() as i32 - pen);
                pen += geo.width();
            }
            let at = gtk::graphene::Point::new(origin, baseline);
            if let Some(node) = gtk::gsk::TextNode::new(&font, &glyphs, color, &at) {
                s.append_node(&node);
            }
        }
        if !iter.next_run() {
            break;
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

    pub fn set_align(&self, align: Align) {
        if self.imp().align.replace(align) != align {
            self.queue_draw();
        }
    }

    /// Shadows round the text: what `text-shadow` is for a label (the
    /// window's halo on a see-through window).
    pub fn set_halo(&self, halo: &[gmnb_theme::types::Shadow]) {
        if *self.imp().halo.borrow() != halo {
            self.imp().halo.replace(halo.to_vec());
            self.queue_draw();
        }
    }

    pub fn set_prefix(&self, prefix: Option<Prefix>) {
        if *self.imp().prefix.borrow() != prefix {
            self.imp().prefix.replace(prefix);
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

    /// The width the prefix takes with its gap.
    fn prefix_width(&self) -> f32 {
        let prefix = self.imp().prefix.borrow();
        let Some(p) = prefix.as_ref() else {
            return 0.0;
        };
        let (_, logical) = self
            .layout_at_size(p.font.size, &p.text, Some(&p.font))
            .extents();
        px(logical.width()) + p.gap
    }

    /// The size the text is set at in `width` (its font's own if it fits).
    fn fitted_size(&self, width: f32) -> f32 {
        let imp = self.imp();
        let Some(size) = imp.font.borrow().as_ref().map(|f| f.size) else {
            return 15.0;
        };
        let text = imp.text.borrow();
        let (_, logical) = self.layout_at_size(size, &text, None).extents();
        let full = px(logical.width());
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
        self.layout_at_size(size, &text, None)
    }

    /// `text` at `size` in `font` (the label's own if none). DGMNB's text
    /// is unhinted: so is this.
    fn layout_at_size(&self, size: f32, text: &str, font: Option<&Font>) -> pango::Layout {
        let context = self.pango_context();
        let mut options = pangocairo::cairo::FontOptions::new().ok();
        if let Some(o) = options.as_mut() {
            o.set_hint_style(pangocairo::cairo::HintStyle::None);
            o.set_hint_metrics(pangocairo::cairo::HintMetrics::Off);
        }
        pangocairo::functions::context_set_font_options(&context, options.as_ref());
        let layout = pango::Layout::new(&context);
        layout.set_text(text);
        let own = self.imp().font.borrow();
        if let Some(font) = font.or(own.as_ref()) {
            let (fd, attrs) = pango_font(font, size);
            layout.set_font_description(Some(&fd));
            layout.set_attributes(Some(&attrs));
        }
        layout
    }
}

type SizeFn = Box<dyn Fn(i32, i32)>;

mod size_imp {
    use super::*;

    #[derive(Default)]
    pub struct SizeBin {
        pub last: Cell<(i32, i32)>,
        pub pending: Cell<bool>,
        pub callback: RefCell<Option<SizeFn>>,
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

    // No layout manager: with one, GTK never calls size_allocate.
    impl WidgetImpl for SizeBin {
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
            if (width, height) != self.last.get() {
                self.last.set((width, height));
                // Told on an idle: the tree isn't changed mid-allocation.
                if !self.pending.replace(true) {
                    let weak = self.obj().downgrade();
                    glib::idle_add_local_once(move || {
                        if let Some(bin) = weak.upgrade() {
                            let imp = bin.imp();
                            imp.pending.set(false);
                            let (w, h) = imp.last.get();
                            if let Some(f) = imp.callback.borrow().as_ref() {
                                f(w, h);
                            }
                        }
                    });
                }
            }
        }
    }
}

glib::wrapper! {
    /// A bin that tells its allocated size (on an idle), for a page whose
    /// layout follows its height as well as its width.
    pub struct SizeBin(ObjectSubclass<size_imp::SizeBin>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl SizeBin {
    pub fn new(child: &impl IsA<gtk::Widget>) -> Self {
        let bin: Self = glib::Object::new();
        child.set_parent(&bin);
        bin
    }

    pub fn connect_size(&self, f: impl Fn(i32, i32) + 'static) {
        self.imp().callback.replace(Some(Box::new(f)));
    }

    pub fn size(&self) -> (i32, i32) {
        self.imp().last.get()
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
