//! The pieces the pages lay out and set their text with as DGMNB does.
//!
//! * [`FitLabel`]: one line of text in a style's font, shrunk to fit its
//!   width (down to a least size) and set as DGMNB sets a line: its
//!   capitals centred between its top and bottom on a whole pixel, each
//!   glyph at the nearest quarter pixel, unhinted. The pages' results,
//!   values, captions and keys. Its colour is the CSS `color` it gets; its
//!   font and halo come from the style ([`Texts`]).
//! * [`MaxWidth`]: a bin that gives its child the width it has, but no more
//!   than a most, at its start (DGMNB's columns and buttons are
//!   `w.min(MAX)` from the left; `AdwClamp` centres).
//! * [`SizeBin`]: a bin that tells its size, for layouts that follow it.
//!
//! How a line is set is defined once, here, as free functions, for the
//! readout (`widgets/display.rs`) to set its value the same way:
//! [`unhinted`], [`line_layout`], [`fitted_size`], [`cap_height`] and
//! [`draw_line`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;

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

/// A cell of a grid a FitLabel's line is set about. The grid is a
/// `gtk::Grid`, a keypad, or any widget laid out by a `gtk::GridLayout`.
pub struct CellAt {
    grid: glib::WeakRef<gtk::Widget>,
    key: glib::WeakRef<gtk::Widget>,
    /// The grid's rows and columns: shared, for a grid that learns them
    /// as its cells are added.
    cells: Rc<Cell<(usize, usize)>>,
    at: (usize, usize),
    /// The rows and columns the cell is over (a key two cells wide).
    span: (usize, usize),
}

impl CellAt {
    /// How far DGMNB's cell middle lies from the key's allocated one.
    fn shift(&self) -> (f32, f32) {
        let (Some(grid), Some(key)) = (self.grid.upgrade(), self.key.upgrade()) else {
            return (0.0, 0.0);
        };
        let Some(at) = key.compute_bounds(&grid) else {
            return (0.0, 0.0);
        };
        let cells = self.cells.get();
        if cells.0 == 0 || cells.1 == 0 {
            return (0.0, 0.0);
        }
        let gap = if let Some(g) = grid.downcast_ref::<gtk::Grid>() {
            g.row_spacing() as i32
        } else if let Some(k) = grid.downcast_ref::<super::keypad::Keypad>() {
            k.gap()
        } else {
            grid.layout_manager()
                .and_downcast::<gtk::GridLayout>()
                .map_or(0, |l| l.row_spacing() as i32)
        };
        let size = (grid.width() as f32, grid.height() as f32);
        // Over several cells: midway between the first's middle and the
        // last's.
        let last = (
            self.at.0 + self.span.0.max(1) - 1,
            self.at.1 + self.span.1.max(1) - 1,
        );
        let first = cell_middle(size, cells, self.at, gap as f32);
        let last = cell_middle(size, cells, last, gap as f32);
        let (x, y) = ((first.0 + last.0) / 2.0, (first.1 + last.1) / 2.0);
        (
            x - (at.x() + at.width() / 2.0),
            y - (at.y() + at.height() / 2.0),
        )
    }
}

/// A superscript or subscript in a key's label is this much of the
/// label's size, raised or lowered by these parts of it (DGMNB's
/// `layout_markup`).
pub const SCRIPT_SCALE: f32 = 0.62;
pub const SUP_RISE: f32 = 0.36;
pub const SUB_DROP: f32 = 0.16;

/// A key label's markup: `<sup>…</sup>` and `<sub>…</sub>` spans, as
/// DGMNB reads it (any other `<` is text; `<small>` is taken out, the
/// span it is in being small already). The text without its tags, and
/// each span's bytes in it with whether it is raised.
pub fn parse_markup(markup: &str) -> (String, Vec<(usize, usize, bool)>) {
    const TAGS: [(&str, Option<Option<bool>>); 6] = [
        ("<sup>", Some(Some(true))),
        ("</sup>", Some(None)),
        ("<sub>", Some(Some(false))),
        ("</sub>", Some(None)),
        ("<small>", None),
        ("</small>", None),
    ];
    let mut plain = String::with_capacity(markup.len());
    let mut spans = Vec::new();
    let mut shift: Option<bool> = None;
    let mut rest = markup;
    while !rest.is_empty() {
        if let Some((tag, change)) = TAGS.iter().find(|(tag, _)| rest.starts_with(tag)) {
            if let Some(to) = change {
                shift = *to;
            }
            rest = &rest[tag.len()..];
            continue;
        }
        let next = TAGS
            .iter()
            .filter_map(|(tag, _)| rest.find(tag))
            .min()
            .unwrap_or(rest.len());
        let start = plain.len();
        plain.push_str(&rest[..next]);
        if let Some(sup) = shift {
            spans.push((start, plain.len(), sup));
        }
        rest = &rest[next..];
    }
    (plain, spans)
}

/// `context` set as DGMNB sets text: outlines unhinted, in grey levels,
/// their widths unrounded. What `window.rs` `match_text_rendering` asks of
/// the toolkit for the whole app, said here too for the text the app
/// places glyph by glyph: it must not move with a setting.
pub fn set_unhinted(context: &pango::Context) {
    let mut options = pangocairo::cairo::FontOptions::new().ok();
    if let Some(o) = options.as_mut() {
        o.set_antialias(pangocairo::cairo::Antialias::Gray);
        o.set_hint_style(pangocairo::cairo::HintStyle::None);
        o.set_hint_metrics(pangocairo::cairo::HintMetrics::Off);
    }
    pangocairo::functions::context_set_font_options(context, options.as_ref());
}

/// `widget`'s Pango context, set as DGMNB sets text ([`set_unhinted`]).
pub fn unhinted(widget: &impl IsA<gtk::Widget>) -> pango::Context {
    let context = widget.pango_context();
    set_unhinted(&context);
    context
}

/// One line of `text` in `font` at `size` pixels. `markup`: `text` is a
/// key label's ([`parse_markup`]).
pub fn line_layout(
    context: &pango::Context,
    text: &str,
    font: &Font,
    size: f32,
    markup: bool,
) -> pango::Layout {
    let layout = pango::Layout::new(context);
    let (fd, attrs) = pango_font(font, size);
    layout.set_font_description(Some(&fd));
    if markup {
        let (plain, spans) = parse_markup(text);
        layout.set_text(&plain);
        for (start, end, sup) in spans {
            let (small, _) = pango_font(font, size * SCRIPT_SCALE);
            let mut desc = pango::AttrFontDesc::new(&small);
            desc.set_start_index(start as u32);
            desc.set_end_index(end as u32);
            attrs.insert(desc);
            let by = if sup { SUP_RISE } else { -SUB_DROP } * size;
            let mut rise = pango::AttrInt::new_rise((by * pango::SCALE as f32).round() as i32);
            rise.set_start_index(start as u32);
            rise.set_end_index(end as u32);
            attrs.insert(rise);
        }
    } else {
        layout.set_text(text);
    }
    layout.set_attributes(Some(&attrs));
    layout
}

/// The size `text` is set at in `width`: `font`'s own if it fits, else
/// smaller by as much as it is too wide, down to `min`.
pub fn fitted_size(
    context: &pango::Context,
    text: &str,
    font: &Font,
    width: f32,
    min: f32,
    markup: bool,
) -> f32 {
    let (_, logical) = line_layout(context, text, font, font.size, markup).extents();
    let full = px(logical.width());
    if full > width && full > 0.0 {
        (font.size * width / full).max(min)
    } else {
        font.size
    }
}

/// The height of `font`'s capitals at `size`: the ink of an `H`. DGMNB
/// centres a line on it.
pub fn cap_height(context: &pango::Context, font: &Font, size: f32) -> f32 {
    let (ink, _) = line_layout(context, "H", font, size, false).extents();
    px(ink.height())
}

/// Pixels from Pango units.
fn px(units: i32) -> f32 {
    units as f32 / pango::SCALE as f32
}

mod imp {
    use super::*;

    pub struct FitLabel {
        pub text: RefCell<String>,
        /// The text is a key label's markup ([`parse_markup`]).
        pub markup: Cell<bool>,
        pub font: RefCell<Option<Font>>,
        pub min_size: Cell<f32>,
        pub align: Cell<Align>,
        pub prefix: RefCell<Option<Prefix>>,
        /// Shadows round the text (a see-through window's halo).
        pub halo: RefCell<Vec<gmnb_theme::types::Shadow>>,
        /// The cell of a grid whose middle the line is set about
        /// (DGMNB's keys sit in cells of fractional size).
        pub cell: RefCell<Option<CellAt>>,
        /// It asks for no least height ([`super::FitLabel::set_loose`]).
        pub loose: Cell<bool>,
        /// How far its line is moved from where its alignment puts it
        /// ([`super::FitLabel::set_shift`]), worked out as it is drawn.
        pub shift: RefCell<Option<ShiftFn>>,
        /// A text field's line ([`super::FitLabel::set_field`]): never
        /// shrunk, cut at its box, and moved left to keep this byte of
        /// the text (the caret) in view.
        pub field: Cell<Option<usize>>,
    }

    impl Default for FitLabel {
        fn default() -> Self {
            Self {
                text: RefCell::default(),
                markup: Cell::new(false),
                font: RefCell::default(),
                min_size: Cell::new(1.0),
                align: Cell::new(Align::Start),
                prefix: RefCell::default(),
                halo: RefCell::default(),
                cell: RefCell::default(),
                loose: Cell::new(false),
                shift: RefCell::default(),
                field: Cell::new(None),
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
                (if self.loose.get() { 0 } else { natural }, natural, -1, -1)
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
            // Worked out as it is drawn, from the allocations then.
            let (mut dx, mut dy) = self
                .cell
                .borrow()
                .as_ref()
                .map_or((0.0, 0.0), |c| c.shift());
            if let Some(f) = self.shift.borrow().as_ref() {
                let (x, y) = f(&obj);
                dx += x;
                dy += y;
            }
            // DGMNB's quarters and whole pixels are the window's: a label
            // placed to the fraction of a pixel (a row's cells, a strip)
            // sets its glyphs on those, not on its own.
            let (fx, fy) = obj
                .root()
                .and_then(|r| obj.compute_point(&r, &gtk::graphene::Point::new(0.0, 0.0)))
                .map_or((0.0, 0.0), |p| {
                    (p.x() - p.x().floor(), p.y() - p.y().floor())
                });
            let baseline = |font: Option<&Font>, size: f32| {
                let probe = obj.layout_at_size(size, "H", font);
                let (ink, _) = probe.extents();
                h / 2.0 + dy + px(ink.height()) / 2.0 + fy
            };
            let (_, logical) = layout.extents();
            let mut x = match self.align.get() {
                Align::Start => lead + dx,
                Align::Center => (w - lead - px(logical.width())) / 2.0 + lead + dx,
            };
            let field = self.field.get();
            if let Some(caret) = field {
                // DGMNB's `text_field`: the caret kept in view, a margin
                // before the box's end; the line cut at the box.
                let byte = caret.min(self.text.borrow().len()) as i32;
                let at = px(layout.index_to_pos(byte).x());
                x -= (at - (w - lead) + appcore::metrics::controls::CARET_MARGIN).max(0.0);
                s.push_clip(&gtk::graphene::Rect::new(0.0, 0.0, w, h));
            }
            s.save();
            s.translate(&gtk::graphene::Point::new(-fx, -fy));
            x += fx;
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
                draw_line(
                    s,
                    &pl,
                    fx,
                    baseline(Some(&p.font), p.font.size),
                    &rgba(p.color),
                );
            }
            draw_line(s, &layout, x, baseline(None, size), &obj.color());
            if !halo.is_empty() {
                s.pop();
            }
            s.restore();
            if field.is_some() {
                s.pop();
            }
        }
    }
}

/// Where a label's line goes from where its alignment puts it (x, y).
type ShiftFn = Box<dyn Fn(&FitLabel) -> (f32, f32)>;

fn rgba(c: gmnb_theme::types::Rgba) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(c[0], c[1], c[2], c[3])
}

/// `layout`'s line at `x`, its baseline at `baseline`, each glyph placed
/// at the nearest quarter pixel across and each run's baseline on a
/// whole pixel, as DGMNB places its glyphs (the toolkit's own rounding of
/// a position put some a quarter pixel off). A raised or lowered run (a
/// key label's superscript) is rounded with its shift, as DGMNB's is.
pub fn draw_line(
    s: &gtk::Snapshot,
    layout: &pango::Layout,
    x: f32,
    baseline: f32,
    color: &gtk::gdk::RGBA,
) {
    let quarter = |v: f32| (v * 4.0).round() / 4.0;
    let mut iter = layout.iter();
    let line = iter.baseline();
    loop {
        if let Some(run) = iter.run_readonly() {
            let (_, logical) = iter.run_extents();
            let baseline = (baseline + px(iter.run_baseline() - line)).round();
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

    /// One drawn only: what it says is its control's name (a key's).
    pub fn drawn() -> Self {
        glib::Object::builder()
            .property("accessible-role", gtk::AccessibleRole::Presentation)
            .build()
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

    /// The text, which is also its name to assistive technology (a
    /// control that holds it names itself afterwards).
    pub fn set_text(&self, text: &str) {
        let imp = self.imp();
        if *imp.text.borrow() != text || imp.markup.get() {
            imp.text.replace(text.to_string());
            imp.markup.set(false);
            self.update_property(&[gtk::accessible::Property::Label(text)]);
            self.queue_resize();
        }
    }

    /// A key's label: text with `<sup>` and `<sub>` spans
    /// ([`parse_markup`]).
    pub fn set_markup(&self, markup: &str) {
        let imp = self.imp();
        if *imp.text.borrow() != markup || !imp.markup.get() {
            imp.text.replace(markup.to_string());
            imp.markup.set(true);
            self.queue_resize();
        }
    }

    /// A label that asks for no least height: its line is set about the
    /// middle of whatever it is given, past its edges if need be (a key's
    /// label: DGMNB's keys shrink below what they show).
    pub fn set_loose(&self, loose: bool) {
        if self.imp().loose.replace(loose) != loose {
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

    /// Sets the line about the middle of DGMNB's cell `at` = (row,
    /// column) of `grid` (a `gtk::Grid` of `cells` = (rows, columns),
    /// its own spacing apart), where `key` is the label's ancestor in it.
    pub fn set_cell(
        &self,
        grid: &impl IsA<gtk::Widget>,
        key: &impl IsA<gtk::Widget>,
        cells: (usize, usize),
        at: (usize, usize),
    ) {
        self.set_cell_of(grid, key, Rc::new(Cell::new(cells)), at, (1, 1));
    }

    /// [`FitLabel::set_cell`] for a grid whose rows and columns are only
    /// known once all its cells are in: `cells` is read as the label is
    /// drawn. The grid is a `gtk::Grid`, a keypad, or laid out by a
    /// `gtk::GridLayout`; `span`: the rows and columns the cell is over.
    pub fn set_cell_of(
        &self,
        grid: &impl IsA<gtk::Widget>,
        key: &impl IsA<gtk::Widget>,
        cells: Rc<Cell<(usize, usize)>>,
        at: (usize, usize),
        span: (usize, usize),
    ) {
        self.imp().cell.replace(Some(CellAt {
            grid: grid.upcast_ref::<gtk::Widget>().downgrade(),
            key: key.upcast_ref::<gtk::Widget>().downgrade(),
            cells,
            at,
            span,
        }));
        self.queue_draw();
    }

    /// Moves the line from where its alignment puts it by what `f` says
    /// as it is drawn (x, y in pixels): to the fraction of a pixel where
    /// DGMNB puts a line the toolkit's whole pixels can't (a button as
    /// wide as its text and a part more, a line placed after another's
    /// text).
    pub fn set_shift(&self, f: impl Fn(&FitLabel) -> (f32, f32) + 'static) {
        self.imp().shift.replace(Some(Box::new(f)));
        self.queue_draw();
    }

    /// A text field's line, set as DGMNB's `text_field` sets it: never
    /// shrunk, cut at the label's box, and moved left as far as keeps
    /// `caret` (a byte of the text) in view; `None`: a plain line.
    pub fn set_field(&self, caret: Option<usize>) {
        if self.imp().field.replace(caret) != caret {
            self.queue_draw();
        }
    }

    /// The width of its line (with its prefix) at its font's own size, to
    /// the fraction: what DGMNB measures a line at.
    pub fn text_width(&self) -> f32 {
        let (_, logical) = self.layout_at(None).extents();
        self.prefix_width() + px(logical.width())
    }

    /// The width of `text` in its font at its own size, to the fraction.
    pub fn measure(&self, text: &str) -> f32 {
        let size = self.imp().font.borrow().as_ref().map_or(15.0, |f| f.size);
        let (_, logical) = self.layout_at_size(size, text, None).extents();
        px(logical.width())
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
        let font = imp.font.borrow();
        let Some(font) = font.as_ref() else {
            return 15.0;
        };
        if imp.field.get().is_some() {
            return font.size;
        }
        fitted_size(
            &unhinted(self),
            &imp.text.borrow(),
            font,
            width,
            imp.min_size.get(),
            imp.markup.get(),
        )
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
        let context = unhinted(self);
        let own = self.imp().font.borrow();
        // Only the label's own text is ever markup: not the probe for its
        // capitals, nor its prefix.
        let markup =
            self.imp().markup.get() && font.is_none() && *text == *self.imp().text.borrow();
        match font.or(own.as_ref()) {
            Some(font) => line_layout(&context, text, font, size, markup),
            None => {
                let layout = pango::Layout::new(&context);
                layout.set_text(text);
                layout
            }
        }
    }
}

/// A toolkit text field whose text at rest is set as DGMNB sets a
/// field's ([`rest_line`]).
pub struct RestLine {
    /// The field, with the line over it: what goes in the page.
    pub overlay: gtk::Overlay,
    /// The line: its font is the caller's to give (a field's style).
    pub line: FitLabel,
}

/// `field` (a `GtkEntry`, a `GtkSpinButton`) in an overlay whose line sets
/// its text, while it doesn't have the focus, as DGMNB's `text_field`
/// does: in `area`'s box less `pad` each side (DGMNB's field and its
/// inner box), its capitals centred, unhinted, each glyph on a quarter
/// pixel, cut at the box and moved left to keep the caret in view; the
/// placeholder when there is no text, in the `wc-placeholder` colour.
///
/// The toolkit's own text (`text`, the field's `GtkText`) stays where it
/// is, hidden at rest (style.css, `.wc-rest`), and is what takes the
/// focus, is edited, shows the caret, a selection and what an input
/// method composes, and what assistive technology reads; as the field
/// takes the focus the line goes and the toolkit's text shows. The line
/// is drawn only (no node for assistive technology), and the pointer
/// goes through it to the field. `class` names the field's kind for the
/// line's colours (style.css).
pub fn rest_line(
    field: &impl IsA<gtk::Widget>,
    area: &impl IsA<gtk::Widget>,
    text: &gtk::Text,
    placeholder: &'static str,
    pad: f32,
    class: &str,
) -> RestLine {
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(field));
    overlay.add_css_class("wc-rest");
    overlay.add_css_class(class);
    let line = FitLabel::drawn();
    line.set_can_target(false);
    line.set_focusable(false);
    overlay.add_overlay(&line);
    overlay.set_clip_overlay(&line, true);
    {
        let (area, line) = (
            area.upcast_ref::<gtk::Widget>().downgrade(),
            line.downgrade(),
        );
        overlay.connect_get_child_position(move |o, child| {
            let (area, line) = (area.upgrade()?, line.upgrade()?);
            if child != line.upcast_ref::<gtk::Widget>() {
                return None;
            }
            let b = area.compute_bounds(o)?;
            Some(gtk::gdk::Rectangle::new(
                (b.x() + pad).round() as i32,
                b.y().round() as i32,
                (b.width() - 2.0 * pad).max(0.0).round() as i32,
                b.height().round() as i32,
            ))
        });
    }
    let show = {
        let line = line.downgrade();
        move |t: &gtk::Text| {
            let Some(line) = line.upgrade() else {
                return;
            };
            let shown = t.text();
            if shown.is_empty() {
                line.set_text(placeholder);
                line.set_field(Some(0));
                line.add_css_class("wc-placeholder");
            } else {
                // GTK counts the caret in characters; DGMNB's in bytes.
                let at = usize::try_from(t.position()).unwrap_or(0);
                let byte = shown.char_indices().nth(at).map_or(shown.len(), |(b, _)| b);
                line.set_text(&shown);
                line.set_field(Some(byte));
                line.remove_css_class("wc-placeholder");
            }
        }
    };
    show(text);
    let show = Rc::new(show);
    for property in ["text", "cursor-position"] {
        let show = show.clone();
        text.connect_notify_local(Some(property), move |t, _| show(t));
    }
    RestLine { overlay, line }
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

/// The middle of DGMNB's cell (`Rect::cell`) at `at` = (row, column) of a
/// `size` grid of `cells` = (rows, columns), `gap` apart: fractions the
/// toolkit's integer allocations round away.
pub fn cell_middle(
    size: (f32, f32),
    cells: (usize, usize),
    at: (usize, usize),
    gap: f32,
) -> (f32, f32) {
    let (rows, cols) = (cells.0 as f32, cells.1 as f32);
    let cw = (size.0 - gap * (cols - 1.0)) / cols;
    let ch = (size.1 - gap * (rows - 1.0)) / rows;
    (
        at.1 as f32 * (cw + gap) + cw / 2.0,
        at.0 as f32 * (ch + gap) + ch / 2.0,
    )
}

/// A page's FitLabels and the style path each takes its font from: a Text
/// (its font and halo) or a Control (its font, and the raised halo).
#[derive(Clone, Default)]
pub struct Texts(Rc<RefCell<Vec<Entry>>>);

/// A label, the path of its font, the least size it shrinks to, and the
/// path of its halo when it isn't the style's own.
type Entry = (
    glib::WeakRef<FitLabel>,
    &'static str,
    f32,
    Option<&'static str>,
);

impl Texts {
    /// `label` in `path`'s font, shrinking to fit down to `min` px. A Text
    /// brings its halo; a Control or a Field the raised one (its text is
    /// on its own fill).
    pub fn add(&self, label: &FitLabel, path: &'static str, min: f32) {
        self.0
            .borrow_mut()
            .push((label.downgrade(), path, min, None));
    }

    /// [`Texts::add`] with the halo at `halo` (a control with no fill of
    /// its own, its text on the window: `window.text_halo`).
    pub fn add_with_halo(
        &self,
        label: &FitLabel,
        path: &'static str,
        min: f32,
        halo: &'static str,
    ) {
        self.0
            .borrow_mut()
            .push((label.downgrade(), path, min, Some(halo)));
    }

    pub fn apply(&self, style: &gmnb_theme::Style) {
        self.0.borrow_mut().retain(|(l, ..)| l.upgrade().is_some());
        for (label, path, min, halo) in self.0.borrow().iter() {
            if let Some(label) = label.upgrade() {
                set_style(&label, style, path, *min, *halo);
            }
        }
    }
}

/// `label` in `path`'s font and halo ([`Texts::add`], [`Texts::add_with_halo`]).
fn set_style(
    label: &FitLabel,
    style: &gmnb_theme::Style,
    path: &str,
    min: f32,
    halo: Option<&str>,
) {
    use gmnb_theme::types::Value;
    let own = match style.get(path) {
        Some(Value::Text(t)) => {
            label.set_font(&t.font, min);
            t.shadow.as_slice()
        }
        Some(Value::Control(c)) => {
            label.set_font(&c.font, min);
            style.shadows("window.raised_text_halo")
        }
        Some(Value::Field(f)) => {
            label.set_font(&f.font, min);
            style.shadows("window.raised_text_halo")
        }
        _ => return,
    };
    label.set_halo(halo.map_or(own, |h| style.shadows(h)));
}

/// `label` in `path`'s font and halo from now on, as the hub's style
/// changes: for a label made where no page's [`Texts`] reaches (a
/// drop-down's button, made by its factory).
pub fn follow(label: &FitLabel, path: &'static str, min: f32, halo: Option<&'static str>) {
    if let Some(hub) = crate::theme::Hub::current() {
        hub.subscribe_style(label, move |l, style| set_style(l, style, path, min, halo));
    }
}

#[cfg(test)]
mod tests {
    use super::{cell_middle, parse_markup};

    /// A key's markup is read as DGMNB reads it (`text.rs`
    /// `layout_markup`): only `<sup>` and `<sub>` are markup, any other
    /// `<` is text.
    #[test]
    fn a_keys_markup_is_read_as_dgmnb_reads_it() {
        assert_eq!(
            parse_markup("x<sup>2</sup>"),
            ("x2".into(), vec![(1, 2, true)])
        );
        assert_eq!(
            parse_markup("<sup>2</sup>√x"),
            ("2√x".into(), vec![(0, 1, true)])
        );
        assert_eq!(
            parse_markup("<sup>1</sup>/<sub>x</sub>"),
            ("1/x".into(), vec![(0, 1, true), (2, 3, false)])
        );
        assert_eq!(parse_markup("<"), ("<".into(), vec![]));
        assert_eq!(parse_markup("a<b>c"), ("a<b>c".into(), vec![]));
        assert_eq!(parse_markup("CE"), ("CE".into(), vec![]));
    }

    /// `<small>` inside a script is taken out: the script is small
    /// already (the open-parenthesis count, as 0.2 wrote it).
    #[test]
    fn small_in_a_script_is_dropped() {
        assert_eq!(
            parse_markup("(<sub><small>2</small></sub>"),
            ("(2".into(), vec![(1, 2, false)])
        );
    }

    /// DGMNB's `Rect::cell`: five columns in 444 px, 4 apart, are 85.6
    /// wide.
    #[test]
    fn a_cells_middle_is_dgmnbs() {
        let (x, y) = cell_middle((444.0, 372.0), (7, 5), (0, 0), 4.0);
        assert!((x - 42.8).abs() < 1e-3 && (y - 24.857).abs() < 1e-2);
        let (x, _) = cell_middle((444.0, 372.0), (7, 5), (0, 4), 4.0);
        assert!((x - (4.0 * 89.6 + 42.8)).abs() < 1e-3);
    }
}
