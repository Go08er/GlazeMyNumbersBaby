//! `Display` — the calculator readout.
//!
//! The value is drawn glyph-by-glyph (so each digit can animate on its own)
//! through a gradient mask with a soft glow behind it, auto-fitting its size
//! to the available width. The expression line sits above it.
//!
//! Animations by change kind:
//! * `Typing`  – newly appended characters pop in with a little overshoot.
//! * `Result`  – the whole number rises in as a staggered wave + shimmer sweep.
//! * `Replace` – old value drifts up and fades while the new one settles in.
//! * `Error`   – a damped shake.

use std::cell::{Cell, RefCell};
use std::time::Instant;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene, gsk, pango};

use super::animations_enabled;
use crate::theme::{Scheme, rgba};

/// The bundled display face.
pub const DISPLAY_FONT: Option<&str> = Some("Outfit");

/// A font description starting from the widget's own (system) font.
pub fn display_font(
    widget: &impl IsA<gtk::Widget>,
    size: f32,
    weight: i32,
) -> pango::FontDescription {
    let mut fd = widget
        .pango_context()
        .font_description()
        .unwrap_or_default();
    if let Some(family) = DISPLAY_FONT {
        fd.set_family(family);
    }
    fd.set_weight(pango::Weight::__Unknown(weight));
    fd.set_absolute_size(size as f64 * pango::SCALE as f64);
    fd
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Typing,
    Result,
    Replace,
    Error,
    None,
}

#[derive(Clone)]
pub struct Glyphs {
    size: f32,
    width: f32,
    height: f32,
    chars: Vec<(pango::Layout, f32)>,
}

mod imp {
    use super::*;

    pub struct Display {
        pub value: RefCell<String>,
        pub prev: RefCell<Option<Glyphs>>,
        pub expression: RefCell<String>,
        pub glyphs: RefCell<Option<Glyphs>>,
        pub max_size: Cell<f32>,
        pub min_size: Cell<f32>,
        pub expr_size: Cell<f32>,
        pub weight: Cell<i32>,
        pub change: Cell<Change>,
        pub changed_from: Cell<usize>,
        pub anim_start: Cell<Option<Instant>>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
        pub scheme: Cell<Option<Scheme>>,
        pub error: Cell<bool>,
        pub show_expression: Cell<bool>,
        pub align_end: Cell<bool>,
    }

    impl Default for Display {
        fn default() -> Self {
            Self {
                value: RefCell::new("0".into()),
                prev: RefCell::new(None),
                expression: RefCell::new(String::new()),
                glyphs: RefCell::new(None),
                max_size: Cell::new(64.0),
                min_size: Cell::new(18.0),
                expr_size: Cell::new(15.0),
                weight: Cell::new(250),
                change: Cell::new(Change::None),
                changed_from: Cell::new(0),
                anim_start: Cell::new(None),
                tick: RefCell::new(None),
                scheme: Cell::new(None),
                error: Cell::new(false),
                show_expression: Cell::new(true),
                align_end: Cell::new(true),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Display {
        const NAME: &'static str = "GmnbDisplay";
        type Type = super::Display;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("calcdisplay");
            klass.set_accessible_role(gtk::AccessibleRole::Label);
        }
    }

    impl ObjectImpl for Display {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_hexpand(true);
        }
        fn dispose(&self) {
            if let Some(id) = self.tick.take() {
                id.remove();
            }
        }
    }

    impl WidgetImpl for Display {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk::Orientation::Vertical {
                let h = self.obj().natural_height();
                (h, h, -1, -1)
            } else {
                (40, 200, -1, -1)
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            self.glyphs.replace(None);
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            self.obj().draw(s);
        }
    }
}

glib::wrapper! {
    pub struct Display(ObjectSubclass<imp::Display>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Display {
    fn default() -> Self {
        glib::Object::new()
    }
}

const PAD_X: f32 = 14.0;

impl Display {
    pub fn new(max_size: f32) -> Self {
        let d = Self::default();
        d.imp().max_size.set(max_size);
        d
    }

    pub fn set_max_size(&self, size: f32) {
        if self.imp().max_size.get() != size {
            self.imp().max_size.set(size);
            self.imp().glyphs.replace(None);
            self.queue_resize();
        }
    }

    pub fn set_show_expression(&self, show: bool) {
        self.imp().show_expression.set(show);
        self.queue_resize();
    }

    pub fn set_align_end(&self, end: bool) {
        self.imp().align_end.set(end);
        self.imp().glyphs.replace(None);
        self.queue_draw();
    }

    pub fn set_weight(&self, weight: i32) {
        self.imp().weight.set(weight);
        self.imp().glyphs.replace(None);
        self.queue_draw();
    }

    fn natural_height(&self) -> i32 {
        let imp = self.imp();
        let expr = if imp.show_expression.get() {
            imp.expr_size.get() * 1.6 + 4.0
        } else {
            0.0
        };
        (expr + imp.max_size.get() * 1.28 + 8.0).ceil() as i32
    }

    pub fn set_scheme(&self, scheme: Scheme) {
        self.imp().scheme.set(Some(scheme));
        self.queue_draw();
    }

    pub fn value(&self) -> String {
        self.imp().value.borrow().clone()
    }

    pub fn set_expression(&self, text: &str) {
        if *self.imp().expression.borrow() != text {
            self.imp().expression.replace(text.to_string());
            self.queue_draw();
        }
    }

    pub fn set_value(&self, text: &str, change: Change, is_error: bool) {
        let imp = self.imp();
        let old = imp.value.borrow().clone();
        if old == text && imp.error.get() == is_error && change != Change::Result {
            return;
        }
        let common = old
            .chars()
            .zip(text.chars())
            .take_while(|(a, b)| a == b)
            .count();
        imp.changed_from.set(common);
        imp.prev.replace(imp.glyphs.borrow().clone());
        imp.value.replace(text.to_string());
        imp.error.set(is_error);
        imp.glyphs.replace(None);
        self.update_property(&[gtk::accessible::Property::Label(&format!(
            "Display is {text}"
        ))]);

        let change = if is_error { Change::Error } else { change };
        imp.change.set(change);
        if change != Change::None && animations_enabled() && self.is_mapped() {
            imp.anim_start.set(Some(Instant::now()));
            self.ensure_ticking();
        } else {
            imp.anim_start.set(None);
        }
        self.queue_draw();
    }

    fn anim_len(&self) -> f32 {
        let n = self.imp().value.borrow().chars().count() as f32;
        match self.imp().change.get() {
            Change::Typing => 0.26,
            Change::Result => 0.5 + n * 0.028 + 0.35,
            Change::Replace => 0.32,
            Change::Error => 0.5,
            Change::None => 0.0,
        }
    }

    fn ensure_ticking(&self) {
        if self.imp().tick.borrow().is_some() {
            return;
        }
        let id = self.add_tick_callback(|d, _| {
            let imp = d.imp();
            let done = imp
                .anim_start
                .get()
                .is_none_or(|t| t.elapsed().as_secs_f32() > d.anim_len());
            d.queue_draw();
            if done {
                imp.anim_start.set(None);
                imp.prev.replace(None);
                imp.tick.replace(None);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        self.imp().tick.replace(Some(id));
    }

    fn font(&self, size: f32, weight: i32) -> pango::FontDescription {
        display_font(self, size, weight)
    }

    fn layout(&self, text: &str, size: f32, weight: i32) -> pango::Layout {
        let layout = self.create_pango_layout(Some(text));
        layout.set_font_description(Some(&self.font(size, weight)));
        let attrs = pango::AttrList::new();
        attrs.insert(pango::AttrFontFeatures::new("tnum 1, lnum 1"));
        layout.set_attributes(Some(&attrs));
        layout
    }

    fn build_glyphs(&self) -> Glyphs {
        let imp = self.imp();
        let text = imp.value.borrow().clone();
        let avail = (self.width() as f32 - PAD_X * 2.0).max(10.0);
        let weight = imp.weight.get();
        let max = if imp.error.get() {
            imp.max_size.get() * 0.62
        } else {
            imp.max_size.get()
        };
        let probe = self.layout(&text, max, weight);
        let (w, _) = probe.pixel_size();
        let size = if w as f32 > avail {
            (max * avail / w as f32).max(imp.min_size.get())
        } else {
            max
        };
        let full = self.layout(&text, size, weight);
        let (fw, fh) = full.pixel_size();
        let mut chars = Vec::new();
        for (byte, ch) in text.char_indices() {
            let pos = full.index_to_pos(byte as i32);
            let x = pos.x() as f32 / pango::SCALE as f32;
            chars.push((self.layout(&ch.to_string(), size, weight), x));
        }
        Glyphs {
            size,
            width: fw as f32,
            height: fh as f32,
            chars,
        }
    }

    fn draw(&self, s: &gtk::Snapshot) {
        let imp = self.imp();
        let Some(scheme) = imp.scheme.get() else {
            return;
        };
        let w = self.width() as f32;
        let h = self.height() as f32;

        // Expression line.
        let mut top = 4.0;
        if imp.show_expression.get() {
            let expr = imp.expression.borrow();
            if !expr.is_empty() {
                let layout = self.create_pango_layout(Some(&expr));
                layout.set_font_description(Some(&display_font(self, imp.expr_size.get(), 400)));
                layout.set_ellipsize(pango::EllipsizeMode::Start);
                layout.set_width(((w - PAD_X * 2.0) * pango::SCALE as f32) as i32);
                layout.set_alignment(if imp.align_end.get() {
                    pango::Alignment::Right
                } else {
                    pango::Alignment::Left
                });
                s.save();
                s.translate(&graphene::Point::new(PAD_X, top));
                s.append_layout(&layout, &rgba(scheme.fg, 0.55));
                s.restore();
            }
            top += imp.expr_size.get() * 1.6 + 4.0;
        }

        if imp.glyphs.borrow().is_none() {
            let g = self.build_glyphs();
            imp.glyphs.replace(Some(g));
        }
        let g = imp.glyphs.borrow().clone().unwrap();
        let elapsed = imp.anim_start.get().map(|t| t.elapsed().as_secs_f32());
        let change = if elapsed.is_some() {
            imp.change.get()
        } else {
            Change::None
        };
        let line_h = imp.max_size.get() * 1.28;
        let base_y = top + (line_h - g.height) / 2.0 + (h - top - line_h - 4.0).max(0.0);

        let origin_x = |g: &Glyphs| {
            if imp.align_end.get() {
                w - PAD_X - g.width
            } else {
                PAD_X
            }
        };

        // Shake for errors.
        let shake = match (change, elapsed) {
            (Change::Error, Some(t)) => {
                let p = (t / 0.5).min(1.0);
                (t * 46.0).sin() * 9.0 * (1.0 - p).powi(2)
            }
            _ => 0.0,
        };

        // Old value drifting away (Replace).
        if let (Change::Replace, Some(t), Some(prev)) =
            (change, elapsed, imp.prev.borrow().as_ref())
        {
            let p = (t / 0.22).min(1.0);
            s.save();
            s.translate(&graphene::Point::new(origin_x(prev), base_y - 14.0 * p));
            s.push_opacity((1.0 - p) as f64 * 0.8);
            for (layout, x) in &prev.chars {
                s.save();
                s.translate(&graphene::Point::new(*x, 0.0));
                s.append_layout(layout, &rgba(scheme.fg, 1.0));
                s.restore();
            }
            s.pop();
            s.restore();
        }

        let ox = origin_x(&g) + shake;
        let changed_from = imp.changed_from.get();
        let n = g.chars.len();
        // Per-glyph (dy, opacity, scale) for the current animation.
        let glyph_state = |i: usize| -> (f32, f32, f32) {
            let Some(t) = elapsed else {
                return (0.0, 1.0, 1.0);
            };
            match change {
                Change::Typing if i >= changed_from => {
                    let p = (t / 0.26).min(1.0);
                    let back = 1.0 + 2.7 * (p - 1.0).powi(3) + 1.7 * (p - 1.0).powi(2);
                    (0.0, p.min(1.0).powf(0.5), 0.55 + 0.45 * back)
                }
                Change::Result => {
                    let delay = i as f32 * 0.028;
                    let p = ((t - delay) / 0.5).clamp(0.0, 1.0);
                    let e = 1.0 - (1.0 - p).powi(4);
                    ((1.0 - e) * g.size * 0.45, e, 1.0)
                }
                Change::Replace => {
                    let p = (t / 0.32).min(1.0);
                    let e = 1.0 - (1.0 - p).powi(3);
                    ((1.0 - e) * 12.0, e, 1.0)
                }
                _ => (0.0, 1.0, 1.0),
            }
        };

        let draw_glyphs = |s: &gtk::Snapshot, color: gtk::gdk::RGBA| {
            for (i, (layout, x)) in g.chars.iter().enumerate() {
                let (dy, alpha, scale) = glyph_state(i);
                if alpha <= 0.001 {
                    continue;
                }
                let (cw, ch) = layout.pixel_size();
                s.save();
                s.translate(&graphene::Point::new(
                    ox + x + cw as f32 / 2.0,
                    base_y + dy + ch as f32 / 2.0,
                ));
                s.scale(scale, scale);
                s.translate(&graphene::Point::new(
                    -(cw as f32) / 2.0,
                    -(ch as f32) / 2.0,
                ));
                let mut c = color;
                c.set_alpha(c.alpha() * alpha);
                s.append_layout(layout, &c);
                s.restore();
            }
        };

        let value_rect = graphene::Rect::new(0.0, base_y - 4.0, w, g.height + 8.0);
        let (tint_top, tint_bottom) = if imp.error.get() {
            (rgba(scheme.fg, 0.92), rgba(scheme.hot_a, 0.95))
        } else if scheme.dark {
            (
                rgba(scheme.fg, 1.0),
                rgba(mix(scheme.fg, scheme.accent, 0.55), 1.0),
            )
        } else {
            (
                rgba(scheme.fg, 1.0),
                rgba(mix(scheme.fg, scheme.accent, 0.45), 1.0),
            )
        };

        // Glow.
        if scheme.dark && n > 0 {
            s.push_blur((g.size * 0.30) as f64);
            draw_glyphs(s, rgba(scheme.accent, 0.42));
            s.pop();
        }

        // Gradient-filled glyphs.
        s.push_mask(gsk::MaskMode::Alpha);
        draw_glyphs(s, rgba([1.0, 1.0, 1.0], 1.0));
        s.pop();
        s.append_linear_gradient(
            &value_rect,
            &graphene::Point::new(0.0, value_rect.y()),
            &graphene::Point::new(0.0, value_rect.y() + value_rect.height()),
            &[
                gsk::ColorStop::new(0.15, tint_top),
                gsk::ColorStop::new(1.0, tint_bottom),
            ],
        );
        if let (Change::Result, Some(t)) = (change, elapsed) {
            // A band of light sweeping across after the wave lands.
            let start = 0.25 + n as f32 * 0.028;
            let p = ((t - start) / 0.55).clamp(0.0, 1.0);
            if p > 0.0 && p < 1.0 {
                let span = g.width + 160.0;
                let cx = ox - 80.0 + span * p;
                let band =
                    graphene::Rect::new(cx - 70.0, value_rect.y(), 140.0, value_rect.height());
                s.append_linear_gradient(
                    &band,
                    &graphene::Point::new(band.x(), band.y()),
                    &graphene::Point::new(band.x() + band.width(), band.y() + band.height() * 0.4),
                    &[
                        gsk::ColorStop::new(0.0, rgba(scheme.hot_a, 0.0)),
                        gsk::ColorStop::new(0.5, rgba([1.0, 1.0, 1.0], 0.95)),
                        gsk::ColorStop::new(1.0, rgba(scheme.hot_b, 0.0)),
                    ],
                );
            }
        }
        s.pop();
    }
}

pub fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}
