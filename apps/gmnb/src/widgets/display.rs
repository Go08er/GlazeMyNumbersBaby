//! `Display` — a readout: the calculator's, and the same widget for the
//! converter's two values and the date page's results (a *line*:
//! [`Display::line`]).
//!
//! The app lays the value out: it picks the size that fits, shapes the
//! text in the style's font and places each character, as DGMNB sets a
//! line (`widgets/fit.rs`: unhinted, the capitals centred with the
//! baseline on a whole pixel, each glyph at the nearest quarter pixel).
//! Who paints it depends on the theme (docs/theming.md, "The display
//! layer"):
//!
//! * Without a `display` layer the value is painted the base way, from
//!   the style: `display.value.color` (a gradient is spread over the
//!   value's band), `error_color`, `glow`. In the base look that is
//!   DGMNB's readout: one colour, no animation. A line is painted in the
//!   colour the stylesheet gives it.
//! * With one, the theme's `draw` paints the characters, and is told of
//!   each change of the value while it shows it. If the hook fails, the
//!   base painting stands in: a number is never missing.
//!
//! The expression line and the halo around the readout on a see-through
//! window are the app's either way, and so is what assistive technology
//! hears: the expression is drawn by hand, so it gets a separate,
//! invisible child placed over it ("Expression is 7 +", as upstream names
//! its CalculatorExpression text and DGMNB its node).
//!
//! A line is a [`super::fit::FitLabel`] to its page: its font, the least
//! size it shrinks to, its halo and what is set before it (a currency's
//! symbol) are the page's to give, it is as wide as its text and it has
//! no expression line, no padding and no panel behind it. To a theme it
//! is a readout like the calculator's, with `role` `"converter"` or
//! `"date"`.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use appcore::metrics;
use gmnb_theme::Style;
use gmnb_theme::events::Page;
use gmnb_theme::layer::{ChangeKind, DisplayValue, GlyphBox, Glyphs, LayerKind, Role};
use gmnb_theme::types::{Font, Paint, Rgba, Shadow, Value};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk, pango};

use super::fit::{self, Prefix};
use crate::layers::{self, Extra, Instance, replay};
use crate::theme::Hub;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Typing,
    Result,
    Replace,
    Error,
    None,
}

/// What assistive technology hears for the pending expression (upstream's
/// Format_CalculatorExpression).
pub fn expression_label(expression: &str) -> String {
    format!("Expression is {expression}")
}

/// What assistive technology hears for the value shown (upstream's
/// Format_DisplayValue). A page with a name of its own for its display
/// (the date results, the converter's values) sets that after each value.
pub fn display_label(value: &str) -> String {
    format!("Display is {value}")
}

mod expr_imp {
    use super::*;

    /// The expression line's accessible stand-in: draws nothing, takes no
    /// input, sits over the drawn line so its extents match.
    #[derive(Default)]
    pub struct Expression;

    #[glib::object_subclass]
    impl ObjectSubclass for Expression {
        const NAME: &'static str = "GmnbDisplayExpression";
        type Type = super::Expression;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_accessible_role(gtk::AccessibleRole::Label);
        }
    }

    impl ObjectImpl for Expression {}
    impl WidgetImpl for Expression {}
}

glib::wrapper! {
    pub struct Expression(ObjectSubclass<expr_imp::Expression>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

/// The calculator's modes a readout is sized for (`display.sizes`,
/// `display.heights`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Standard,
    Scientific,
    Programmer,
    Compact,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::Standard => "standard",
            Mode::Scientific => "scientific",
            Mode::Programmer => "programmer",
            Mode::Compact => "compact",
        }
    }
}

/// What a readout takes from the style. Before any style arrives it is
/// the base look's, which is DGMNB's.
struct Look {
    /// The space left and right of the text.
    pad_x: f32,
    /// Where shrinking the value stops.
    min_size: f32,
    /// The value's font (its size comes from the mode, or from the page).
    font: Font,
    color: Paint,
    error_color: Paint,
    /// An error's size as a fraction of the mode's.
    error_scale: f32,
    /// A blurred copy under the value: its colour, and its blur as a
    /// fraction of the font size.
    glow: Option<(Rgba, f32)>,
    expression: Font,
    expression_color: Rgba,
    expression_min: f32,
    /// Shadows around the whole readout.
    halo: Vec<Shadow>,
}

impl Look {
    fn base() -> Look {
        let ctx = gmnb_theme::Ctx::default();
        Look::of(&Style::base(&ctx))
    }

    fn of(style: &Style) -> Look {
        let t = &style.tokens;
        let value = metrics::text::DISPLAY_VALUE;
        let glow = match style.get("display.value.glow") {
            Some(Value::Record(parts)) => match (parts.get("color"), parts.get("blur")) {
                (Some(Value::Color(c)), Some(Value::Number(blur))) => Some((*c, *blur as f32)),
                _ => None,
            },
            _ => None,
        };
        let expression = style.text("display.expression");
        Look {
            pad_x: style.number("display.padding_x"),
            min_size: style.number("display.min_size").max(1.0),
            font: style
                .font("display.value.font")
                .cloned()
                .unwrap_or_else(|| Font::new(&t.fonts.display, value.size, value.weight)),
            color: style
                .paint("display.value.color")
                .cloned()
                .unwrap_or(Paint::Color(t.colors.fg)),
            error_color: style
                .paint("display.value.error_color")
                .cloned()
                .unwrap_or(Paint::Color(t.colors.fg)),
            error_scale: match style.number("display.value.error_scale") {
                n if n > 0.0 => n,
                _ => 1.0,
            },
            glow,
            expression: expression.map_or_else(|| t.text.small.clone(), |e| e.font.clone()),
            expression_color: expression.map_or(t.colors.fg_dim, |e| e.color),
            expression_min: style.number("display.expression_min_size").max(1.0),
            halo: style.shadows("display.halo").to_vec(),
        }
    }
}

/// What a page gives a line of its own ([`Display::line`]).
struct Line {
    /// The font at its largest, and where shrinking it stops.
    font: Font,
    min_size: f32,
    /// Shadows round the text (the window's halo on a see-through
    /// window).
    halo: Vec<Shadow>,
    /// Set before the value, in a font and colour of its own.
    prefix: Option<Prefix>,
}

/// A value laid out in the readout.
struct Laid {
    /// The whole value as one layout, at its place: what the base
    /// painting draws.
    full: pango::Layout,
    /// Where its baseline goes.
    baseline: f32,
    /// A line's prefix: its layout, its baseline and its colour. It
    /// starts at the line's start.
    prefix: Option<(pango::Layout, f32, gdk::RGBA)>,
    /// Each character as a layout of its own, at its place: what a
    /// theme's `draw:glyph` draws.
    glyphs: Vec<replay::Glyph>,
    /// The same, as a frame is told it.
    told: Glyphs,
    /// The strip the value sits in, as wide as the readout.
    band: [f32; 4],
}

mod imp {
    use super::*;

    pub struct Display {
        pub value: RefCell<String>,
        pub expression: RefCell<String>,
        /// The expression line for assistive technology.
        pub expression_node: super::Expression,
        pub(super) laid: RefCell<Option<Rc<Laid>>>,
        /// The old value, while a change is shown.
        pub(super) previous: RefCell<Option<Rc<Laid>>>,
        /// The change being shown: its kind, when it started and the
        /// first character that differs (from 1).
        pub change: Cell<Option<(ChangeKind, f64, u32)>>,
        /// Counts what a display layer's frame is told of the value.
        pub version: Cell<u64>,
        /// The value's largest size until the page gives the readout a
        /// mode ([`super::Display::set_mode`]): then it is the style's
        /// for that mode.
        pub max_size: Cell<f32>,
        pub mode: Cell<Option<Mode>>,
        /// A page's own line, and what the page gave it.
        pub(super) line: RefCell<Option<Line>>,
        pub(super) look: RefCell<Look>,
        /// `display.sizes` and `display.heights` (`None`: `"auto"`), by
        /// mode.
        pub sizes: Cell<[f32; 4]>,
        pub heights: Cell<[Option<f32>; 4]>,
        pub error: Cell<bool>,
        pub show_expression: Cell<bool>,
        pub align_end: Cell<bool>,
        /// The theme's `display` layer here, and this readout's number
        /// among the window's.
        pub layer: OnceCell<Rc<Instance>>,
        pub readout: Cell<u64>,
        /// `GMNB_DISPLAY_RECT`: where the readout last said it was.
        pub reported: Cell<Option<(f32, f32, f32, f32)>>,
    }

    impl Default for Display {
        fn default() -> Self {
            use metrics::calc::*;
            let (full, compact) = (
                metrics::text::DISPLAY_VALUE.size,
                metrics::text::DISPLAY_VALUE_COMPACT.size,
            );
            Self {
                value: RefCell::new("0".into()),
                expression: RefCell::new(String::new()),
                expression_node: glib::Object::new(),
                laid: RefCell::new(None),
                previous: RefCell::new(None),
                change: Cell::new(None),
                version: Cell::new(1),
                max_size: Cell::new(full),
                mode: Cell::new(None),
                line: RefCell::new(None),
                look: RefCell::new(Look::base()),
                sizes: Cell::new([full, full, full, compact]),
                heights: Cell::new([
                    Some(DISPLAY_H_STANDARD),
                    Some(DISPLAY_H_SCIENTIFIC),
                    Some(DISPLAY_H_PROGRAMMER),
                    Some(DISPLAY_H_COMPACT),
                ]),
                error: Cell::new(false),
                show_expression: Cell::new(true),
                align_end: Cell::new(true),
                layer: OnceCell::new(),
                readout: Cell::new(0),
                reported: Cell::new(None),
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
            let obj = self.obj();
            obj.set_hexpand(true);
            // Named from the start: the first value set is often the "0"
            // already shown, which changes nothing.
            obj.update_property(&[gtk::accessible::Property::Label(&display_label(
                &self.value.borrow(),
            ))]);
            let node = &self.expression_node;
            node.set_can_target(false);
            node.set_can_focus(false);
            node.set_visible(false);
            node.set_parent(&*obj);
            // The readout follows the style by itself.
            if let Some(hub) = Hub::current() {
                hub.subscribe_style(&*obj, |d, style| d.set_style(style));
            }
        }

        fn dispose(&self) {
            self.expression_node.unparent();
        }
    }

    impl WidgetImpl for Display {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            if let Some((w, h)) = self.obj().line_size() {
                // A line is as wide as its text, and shrinks to whatever
                // width it is given.
                return if orientation == gtk::Orientation::Horizontal {
                    (0, w, -1, -1)
                } else {
                    (h, h, -1, -1)
                };
            }
            if orientation == gtk::Orientation::Vertical {
                // The style's height for its mode; held at another (the
                // calculator page's bin), it sets its text in that.
                (0, self.obj().height_wanted(), -1, -1)
            } else {
                (40, 200, -1, -1)
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            self.obj().relayout();
            // Over the drawn expression line (see `draw`).
            let node = &self.expression_node;
            if node.is_visible() {
                let _ = node.measure(gtk::Orientation::Horizontal, -1);
                let _ = node.measure(gtk::Orientation::Vertical, -1);
                let pad = self.look.borrow().pad_x;
                let w = (width - 2 * pad as i32).max(0);
                let h = self.obj().expression_height().ceil() as i32;
                let at = gsk::Transform::new()
                    .translate(&graphene::Point::new(pad, 0.0))
                    .into();
                node.allocate(w, h.min(height).max(0), -1, at);
            }
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

/// The longest a change of the value is shown, in seconds.
const CHANGE_MAX: f64 = 3.0;

/// The context the readout's text is laid out in: the widget's, with
/// glyphs unhinted and placed by their true widths. DGMNB's rasteriser
/// hints nothing; left to the desktop's setting, small text here came out
/// crisper and brighter than DGMNB's. A context of its own, not the
/// widget's with its options changed (`fit::unhinted`): the readout keeps
/// its layouts from one frame to the next, and the toolkit puts its own
/// options back into a widget's context when its style changes.
fn text_context(widget: &impl IsA<gtk::Widget>) -> pango::Context {
    let context = widget.create_pango_context();
    fit::set_unhinted(&context);
    context
}

fn px(units: i32) -> f32 {
    units as f32 / pango::SCALE as f32
}

fn rgba(c: Rgba) -> gdk::RGBA {
    gdk::RGBA::new(c[0], c[1], c[2], c[3])
}

impl Display {
    /// A readout whose value is at most `max_size` pixels tall, until
    /// the page gives it a mode ([`Display::set_mode`]).
    pub fn new(max_size: f32) -> Self {
        let d = Self::default();
        d.imp().max_size.set(max_size);
        d
    }

    /// A page's own line of a value (a date result): see the module's
    /// notes. It starts empty.
    pub fn line() -> Self {
        let d = Self::default();
        d.make_line();
        d
    }

    /// A line that is also a control (a converter value, picked to type
    /// into): a button to assistive technology, not a label, and a stop
    /// for the keyboard's focus (Tab), ringed while it has it
    /// (style.css).
    pub fn line_button() -> Self {
        let d: Self = glib::Object::builder()
            .property("accessible-role", gtk::AccessibleRole::Button)
            .property("focusable", true)
            .build();
        d.make_line();
        d
    }

    fn make_line(&self) {
        let imp = self.imp();
        let font = Font {
            size: 15.0,
            ..imp.look.borrow().font.clone()
        };
        imp.line.replace(Some(Line {
            font,
            min_size: 1.0,
            halo: Vec::new(),
            prefix: None,
        }));
        imp.value.replace(String::new());
        imp.show_expression.set(false);
        imp.align_end.set(false);
        self.set_hexpand(false);
        // No panel behind it (style.css).
        self.add_css_class("wc-line");
        self.update_property(&[gtk::accessible::Property::Label("")]);
    }

    /// A line's font at its largest; it shrinks to fit down to
    /// `min_size`.
    pub fn set_font(&self, font: &Font, min_size: f32) {
        {
            let mut line = self.imp().line.borrow_mut();
            let Some(line) = line.as_mut() else { return };
            if line.font == *font && line.min_size == min_size {
                return;
            }
            line.font = font.clone();
            line.min_size = min_size;
        }
        self.relayout();
        self.queue_resize();
    }

    /// Shadows round a line's text: what `text-shadow` is for a label
    /// (the window's halo on a see-through window).
    pub fn set_halo(&self, halo: &[Shadow]) {
        {
            let mut line = self.imp().line.borrow_mut();
            let Some(line) = line.as_mut() else { return };
            if line.halo == halo {
                return;
            }
            line.halo = halo.to_vec();
        }
        self.queue_draw();
    }

    /// Text set before a line's value in a font and colour of its own
    /// (the converter's currency symbol).
    pub fn set_prefix(&self, prefix: Option<Prefix>) {
        {
            let mut line = self.imp().line.borrow_mut();
            let Some(line) = line.as_mut() else { return };
            if line.prefix == prefix {
                return;
            }
            line.prefix = prefix;
        }
        self.relayout();
        self.queue_resize();
    }

    /// A line's natural size: its prefix and its text at the font's own
    /// size. `None`: this is not a line.
    fn line_size(&self) -> Option<(i32, i32)> {
        let imp = self.imp();
        let line = imp.line.borrow();
        let line = line.as_ref()?;
        let context = text_context(self);
        let lead = line.prefix.as_ref().map_or(0.0, |p| {
            let layout = fit::line_layout(&context, &p.text, &p.font, p.font.size, false);
            px(layout.extents().1.width()) + p.gap
        });
        let text = imp.value.borrow();
        let layout = fit::line_layout(&context, &text, &line.font, line.font.size, false);
        let (_, logical) = layout.extents();
        Some((
            (lead + px(logical.width())).ceil() as i32,
            px(logical.height()).ceil() as i32,
        ))
    }

    /// The calculator's readout: its sizes and heights are the style's
    /// for `mode` (`display.sizes`, `display.heights`).
    pub fn set_mode(&self, mode: Mode) {
        if self.imp().mode.replace(Some(mode)) != Some(mode) {
            self.relayout();
            self.queue_resize();
        }
    }

    /// What the readout takes from the style.
    pub fn set_style(&self, style: &Rc<Style>) {
        let imp = self.imp();
        imp.look.replace(Look::of(style));
        let modes = [
            Mode::Standard,
            Mode::Scientific,
            Mode::Programmer,
            Mode::Compact,
        ];
        let mut sizes = imp.sizes.get();
        let mut heights = imp.heights.get();
        for (i, mode) in modes.iter().enumerate() {
            if let Some(Value::Number(n)) = style.part("display.sizes", mode.name()) {
                sizes[i] = *n as f32;
            }
            match style.part("display.heights", mode.name()) {
                Some(Value::Number(n)) => heights[i] = Some(*n as f32),
                Some(Value::Word(_)) => heights[i] = None,
                _ => {}
            }
        }
        imp.sizes.set(sizes);
        imp.heights.set(heights);
        self.relayout();
        self.queue_resize();
    }

    fn slot(mode: Mode) -> usize {
        match mode {
            Mode::Standard => 0,
            Mode::Scientific => 1,
            Mode::Programmer => 2,
            Mode::Compact => 3,
        }
    }

    /// The value's largest size.
    fn size(&self) -> f32 {
        let imp = self.imp();
        match imp.mode.get() {
            Some(mode) => imp.sizes.get()[Self::slot(mode)],
            None => imp.max_size.get(),
        }
    }

    /// How tall the expression line's strip is: DGMNB's 30 px (24 in the
    /// compact window); nothing when the readout has no such line.
    fn expression_height(&self) -> f32 {
        let imp = self.imp();
        if !imp.show_expression.get() {
            0.0
        } else if imp.mode.get() == Some(Mode::Compact) {
            metrics::calc::EXPRESSION_H_COMPACT
        } else {
            metrics::calc::EXPRESSION_H
        }
    }

    /// How tall the readout is by the style: `display.heights` for its
    /// mode, in whole pixels.
    pub fn height_wanted(&self) -> i32 {
        let imp = self.imp();
        let auto = || {
            // `"auto"`: 1.6 × the expression's size + 1.28 × the mode's
            // size + 12 (0.2's readout); a readout with no expression
            // line is its value and 8 px.
            let expression = if imp.show_expression.get() {
                imp.look.borrow().expression.size * 1.6 + 4.0
            } else {
                0.0
            };
            expression + self.size() * 1.28 + 8.0
        };
        let fixed = imp
            .mode
            .get()
            .and_then(|m| imp.heights.get()[Self::slot(m)]);
        fixed.unwrap_or_else(auto).ceil() as i32
    }

    pub fn value(&self) -> String {
        self.imp().value.borrow().clone()
    }

    /// The pending expression ("7 +"). Assistive technology hears every
    /// change, also one that leaves the value as it was (7 + → 7 −).
    pub fn set_expression(&self, text: &str) {
        if *self.imp().expression.borrow() != text {
            self.imp().expression.replace(text.to_string());
            self.sync_expression_node();
            self.queue_draw();
        }
    }

    /// Name the expression line's accessible node, present only while the
    /// line shows something (as DGMNB's).
    fn sync_expression_node(&self) {
        let imp = self.imp();
        let text = imp.expression.borrow();
        let node = &imp.expression_node;
        node.update_property(&[gtk::accessible::Property::Label(&expression_label(&text))]);
        node.set_visible(imp.show_expression.get() && !text.is_empty());
    }

    /// The theme's `display` layer drawn here.
    fn layer(&self) -> Option<Rc<Instance>> {
        let imp = self.imp();
        if let Some(layer) = imp.layer.get() {
            return Some(layer.clone());
        }
        let hub = Hub::current()?;
        let all = hub.layers();
        imp.readout.set(all.next_id());
        let layer = all.instance(LayerKind::Display, self);
        Some(imp.layer.get_or_init(|| layer).clone())
    }

    /// Whether a theme's `display` layer paints the value.
    fn layered(&self) -> bool {
        Hub::current().is_some_and(|h| h.layers().draws(LayerKind::Display))
    }

    /// What a display layer is told of the value changed: it is drawn
    /// again.
    fn touch(&self) {
        let imp = self.imp();
        imp.version.set(imp.version.get() + 1);
        if let Some(layer) = self.layer() {
            layer.invalidate();
        }
        self.queue_draw();
    }

    /// The value must be laid out again (its text, its font, its size or
    /// the readout's changed).
    fn relayout(&self) {
        self.imp().laid.replace(None);
        self.touch();
    }

    /// A change that was being shown is over.
    fn end_change(&self) {
        let imp = self.imp();
        if imp.change.take().is_some() || imp.previous.borrow().is_some() {
            imp.previous.replace(None);
            self.touch();
        }
    }

    pub fn set_value(&self, text: &str, change: Change, is_error: bool) {
        let imp = self.imp();
        // What a calculation came to is news for the theme's layers,
        // whether or not it changes what the readout shows. Not from a
        // readout that isn't on screen (another page's, or the date
        // page's other result).
        if self.is_mapped()
            && let Some(hub) = Hub::current()
        {
            match change {
                Change::Error => hub.layers().outcome(self, true),
                Change::Result if !is_error => hub.layers().outcome(self, false),
                _ => {}
            }
        }
        let old = imp.value.borrow().clone();
        if old == text && imp.error.get() == is_error && change != Change::Result {
            return;
        }
        let common = old
            .chars()
            .zip(text.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let was = imp.laid.borrow().clone();
        imp.value.replace(text.to_string());
        imp.error.set(is_error);
        // A line is named by what it says (its page names it afterwards).
        let is_line = imp.line.borrow().is_some();
        if is_line {
            self.update_property(&[gtk::accessible::Property::Label(text)]);
            self.queue_resize();
        } else {
            self.update_property(&[gtk::accessible::Property::Label(&display_label(text))]);
        }

        // A change is shown by a theme's display layer, and only where
        // motion is allowed.
        let kind = match (is_error, change) {
            (true, _) | (_, Change::Error) => Some(ChangeKind::Error),
            (_, Change::Typing) => Some(ChangeKind::Typing),
            (_, Change::Result) => Some(ChangeKind::Result),
            (_, Change::Replace) => Some(ChangeKind::Replace),
            (_, Change::None) => None,
        };
        let moving = self.is_mapped()
            && self.layered()
            && Hub::current().is_some_and(|h| h.layers().motion());
        match kind.filter(|_| moving) {
            Some(kind) => {
                imp.change
                    .set(Some((kind, layers::now(), common as u32 + 1)));
                imp.previous.replace(was);
            }
            None => {
                imp.change.set(None);
                imp.previous.replace(None);
            }
        }
        self.relayout();
    }

    /// Lays the value out: fitted to the readout's width, set against
    /// its end or its start, its capitals' height centred in the room
    /// under the expression line (as DGMNB places it). A line: after its
    /// prefix, in the middle of its height.
    fn lay(&self) -> Laid {
        let imp = self.imp();
        let look = imp.look.borrow();
        let context = text_context(self);
        let text = imp.value.borrow().clone();
        let (w, h) = (self.width() as f32, self.height() as f32);
        // The font it is set in, where it starts, the space kept at its
        // end, and the height its capitals are centred on.
        let mut prefix = None;
        let (font, start, pad, middle) = match imp.line.borrow().as_ref() {
            Some(line) => {
                let mut lead = 0.0;
                if let Some(p) = &line.prefix {
                    let layout = fit::line_layout(&context, &p.text, &p.font, p.font.size, false);
                    lead = px(layout.extents().1.width()) + p.gap;
                    let cap = fit::cap_height(&context, &p.font, p.font.size);
                    prefix = Some((layout, h / 2.0 + cap / 2.0, rgba(p.color)));
                }
                let size =
                    fit::fitted_size(&context, &text, &line.font, w - lead, line.min_size, false);
                let font = Font {
                    size,
                    ..line.font.clone()
                };
                (font, lead, 0.0, h / 2.0)
            }
            None => {
                let avail = (w - look.pad_x * 2.0).max(10.0);
                let max = if imp.error.get() {
                    self.size() * look.error_scale
                } else {
                    self.size()
                };
                let mut font = look.font.clone();
                font.size = max;
                font.size = fit::fitted_size(&context, &text, &font, avail, look.min_size, false);
                let top = self.expression_height();
                (font, look.pad_x, look.pad_x, top + (h - top) / 2.0)
            }
        };
        let size = font.size;
        let full = replay::layout(&context, &text, &font);
        // The text's width as laid out, not rounded up to whole pixels:
        // its end is set against the readout's.
        let (_, logical) = full.extents();
        let (fw, fh) = (px(logical.width()), px(logical.height()));
        let x = if imp.align_end.get() {
            w - pad - fw
        } else {
            start
        };
        // The capitals centred on `middle`, the baseline on a whole
        // pixel: DGMNB's placing of a line of text.
        let baseline = (middle + fit::cap_height(&context, &font, size) / 2.0).round();
        let y = baseline - px(full.baseline());
        let mut glyphs = Vec::new();
        let mut boxes = Vec::new();
        for (byte, ch) in text.char_indices() {
            let pos = full.index_to_pos(byte as i32);
            let gx = x + px(pos.x());
            let layout = replay::layout(&context, ch.encode_utf8(&mut [0; 4]), &font);
            let (cw, _) = layout.pixel_size();
            boxes.push(GlyphBox {
                x: gx,
                y,
                w: cw as f32,
                h: fh,
            });
            glyphs.push(replay::Glyph { layout, x: gx, y });
        }
        Laid {
            full,
            baseline,
            prefix,
            glyphs,
            told: Glyphs {
                glyphs: boxes,
                size,
                rect: [x, y, fw, fh],
            },
            band: [0.0, y - 4.0, w, fh + 8.0],
        }
    }

    /// The expression line: in its strip at the top, shrunk to fit, then
    /// cut at its start.
    fn draw_expression(&self, s: &gtk::Snapshot) {
        let imp = self.imp();
        let expr = imp.expression.borrow();
        if !imp.show_expression.get() || expr.is_empty() {
            return;
        }
        let look = imp.look.borrow();
        let context = text_context(self);
        let avail = (self.width() as f32 - look.pad_x * 2.0).max(10.0);
        let mut font = look.expression.clone();
        font.size = fit::fitted_size(&context, &expr, &font, avail, look.expression_min, false);
        let layout = replay::layout(&context, &expr, &font);
        layout.set_ellipsize(pango::EllipsizeMode::Start);
        layout.set_width((avail * pango::SCALE as f32) as i32);
        layout.set_alignment(if imp.align_end.get() {
            pango::Alignment::Right
        } else {
            pango::Alignment::Left
        });
        let cap = fit::cap_height(&context, &font, font.size);
        let baseline = self.expression_height() / 2.0 + cap / 2.0;
        fit::draw_line(
            s,
            &layout,
            look.pad_x,
            baseline,
            &rgba(look.expression_color),
        );
    }

    /// The value the base way: the style's colour (a gradient spread over
    /// the value's band), with its glow under it if the style has one.
    fn draw_base(&self, s: &gtk::Snapshot, laid: &Laid) {
        let imp = self.imp();
        let look = imp.look.borrow();
        let [x, _, _, _] = laid.told.rect;
        let append = |color: &gdk::RGBA| fit::draw_line(s, &laid.full, x, laid.baseline, color);
        // A line: in the colour the stylesheet gives it.
        if imp.line.borrow().is_some() {
            append(&self.color());
            return;
        }
        if let Some((c, blur)) = look.glow
            && !laid.told.glyphs.is_empty()
        {
            s.push_blur(f64::from((blur * laid.told.size).clamp(0.0, 100.0)));
            append(&rgba(c));
            s.pop();
        }
        let paint = if imp.error.get() {
            &look.error_color
        } else {
            &look.color
        };
        match paint {
            Paint::Color(c) => append(&rgba(*c)),
            gradient => {
                let band = gmnb_theme::DisplayList {
                    ops: vec![gmnb_theme::Op::Rect {
                        rect: laid.band,
                        paint: gradient.clone(),
                    }],
                };
                s.push_mask(gsk::MaskMode::Alpha);
                append(&gdk::RGBA::WHITE);
                s.pop();
                layers::paint(s, self, &band, [&[], &[]]);
                s.pop();
            }
        }
    }

    /// Dev hook, for comparing a readout with DGMNB's: with
    /// `GMNB_DISPLAY_RECT` set, each readout says where in the window it
    /// is, once, when it is first drawn at a size.
    fn report_rect(&self) {
        thread_local! {
            static ON: bool = std::env::var_os("GMNB_DISPLAY_RECT").is_some();
        }
        if !ON.with(|on| *on) {
            return;
        }
        let Some(root) = self.root() else { return };
        if let Some(b) = self.compute_bounds(&root) {
            let rect = (b.x(), b.y(), b.width(), b.height());
            if self.imp().reported.replace(Some(rect)) != Some(rect) {
                eprintln!(
                    "display rect: {} {} {} {} value {:?}",
                    rect.0,
                    rect.1,
                    rect.2,
                    rect.3,
                    self.imp().value.borrow()
                );
            }
        }
    }

    fn draw(&self, s: &gtk::Snapshot) {
        let imp = self.imp();
        self.report_rect();
        if imp.laid.borrow().is_none() {
            imp.laid.replace(Some(Rc::new(self.lay())));
        }
        let Some(laid) = imp.laid.borrow().clone() else {
            return;
        };
        // Over a see-through window the desktop can be anything: the
        // style's halo (a line's: its page's) keeps the numbers legible.
        let halo: Vec<gsk::Shadow> = {
            let (look, line) = (imp.look.borrow(), imp.line.borrow());
            line.as_ref()
                .map_or(&look.halo, |l| &l.halo)
                .iter()
                .map(|h| gsk::Shadow::new(rgba(h.color), h.x, h.y, h.blur))
                .collect()
        };
        if !halo.is_empty() {
            s.push_shadow(&halo);
        }
        self.draw_expression(s);
        // A line's prefix is the app's, whoever paints the value.
        if let Some((layout, baseline, color)) = &laid.prefix {
            fit::draw_line(s, layout, 0.0, *baseline, color);
        }
        if !self.draw_layer(s, &laid) {
            self.draw_base(s, &laid);
        }
        if !halo.is_empty() {
            s.pop();
        }
    }

    /// Has the theme's `display` layer paint the value. `false`: there
    /// is none, or its hook failed.
    fn draw_layer(&self, s: &gtk::Snapshot, laid: &Rc<Laid>) -> bool {
        let imp = self.imp();
        if !self.layered() {
            return false;
        }
        let Some(layer) = self.layer() else {
            return false;
        };
        let size = (self.width() as f32, self.height() as f32);
        let role = match layers::page_of(self.upcast_ref()) {
            Page::Converter => Role::Converter,
            Page::Date => Role::Date,
            _ => Role::Calculator,
        };
        let text = imp.value.borrow().clone();
        // Twice at most: the frame that ends a change, then the picture
        // at rest.
        for _ in 0..2 {
            let now = layers::now();
            if imp
                .change
                .get()
                .is_some_and(|(_, started, _)| now - started > CHANGE_MAX)
            {
                self.end_change();
            }
            let previous = imp.previous.borrow().clone();
            let change = imp.change.get();
            let value = DisplayValue {
                readout: imp.readout.get(),
                version: imp.version.get(),
                text: &text,
                glyphs: &laid.told,
                band: laid.band,
                error: imp.error.get(),
                align_end: imp.align_end.get(),
                role,
                change: change.map(|(kind, started, from)| (kind, (now - started).max(0.0), from)),
                previous: change.and(previous.as_ref()).map(|p| &p.told),
            };
            let extra = Extra {
                keys: None,
                value: Some(value),
            };
            let Some(picture) = layer.picture(size, extra) else {
                return false;
            };
            // A change ends when `draw` returns nil: one more frame, for
            // the picture at rest.
            if change.is_some() && picture.again.is_none() {
                self.end_change();
                continue;
            }
            let old = previous.as_ref().map_or(&[][..], |p| &p.glyphs[..]);
            layers::paint(s, self, &picture.list, [&laid.glyphs, old]);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    /// Upstream's Format_CalculatorExpression, as DGMNB words it too.
    #[test]
    fn the_expression_is_named_as_upstream_names_it() {
        assert_eq!(super::expression_label("7 + "), "Expression is 7 + ");
    }

    /// Upstream's Format_DisplayValue, as DGMNB words it too; a new
    /// display is named after the value it starts with.
    #[test]
    fn the_display_is_named_as_upstream_names_it() {
        assert_eq!(super::display_label("0"), "Display is 0");
        assert_eq!(super::display_label("1,234.5"), "Display is 1,234.5");
    }
}
