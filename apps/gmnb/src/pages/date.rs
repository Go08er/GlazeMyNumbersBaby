//! Date calculation: "Difference between dates" and "Add or subtract days".
//!
//! Laid out as DGMNB's page (`apps/dgmnb/src/date.rs`): a column from the
//! left, at most `layout.date.max_width` wide, every part at the height
//! and the distance `appcore::metrics::date` gives it.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use appcore::metrics::{controls as ctl, date as md, text as txt};
use chrono::{Local, NaiveDate};
use datecalc::{DateCalculatorState, strings as S};
use gmnb_theme::Style;
use gmnb_theme::types::{Spacing, Value};
use gtk::glib;

use super::{Ctx, Page};
use crate::widgets::day_grid::DayGrid;
use crate::widgets::display::{Change, Display};
use crate::widgets::fit::{Align, FitLabel, MaxWidth, Texts, rest_line};
use crate::widgets::icon::PathIcon;
use appcore::modes::ViewMode;

/// The year, month and day boxes' step buttons (DGMNB's).
const MINUS: &str = "M7 12h10";
const PLUS: &str = "M12 7v10M7 12h10";

pub struct DatePage {
    root: gtk::Widget,
    ctx: Rc<Ctx>,
    state: Rc<RefCell<DateCalculatorState>>,
    /// The pickers' days (their widgets hold them only weakly).
    _days: [Rc<DayGrid>; 3],
}

struct DateButton {
    /// The button at its most width, from the left.
    bin: MaxWidth,
    button: gtk::MenuButton,
    /// "Tuesday, November 10, 2026  ▾", drawn: the button is named by its
    /// caption and the date ([`DateButton::shower`]).
    label: FitLabel,
    /// The caption over it ("From").
    caption: Rc<RefCell<String>>,
    days: Rc<DayGrid>,
}

fn date_button(texts: &Texts) -> DateButton {
    // "Tuesday, November 10, 2026  ▾", centred and shrunk to fit, as
    // DGMNB sets it: the arrow is drawn, not read.
    let label = FitLabel::drawn();
    label.set_align(Align::Center);
    label.set_hexpand(true);
    texts.add(&label, "date.field", txt::BUTTON_MIN_SIZE);
    // Not GTK's calendar, which assistive technology can't read a day of:
    // each day a named button (DayGrid).
    let days = DayGrid::new();
    let popover = gtk::Popover::builder()
        .child(&days.widget())
        .css_classes(["wc-calendar-popover"])
        .has_arrow(false)
        .build();
    // A day is picked by Enter or Space with Shift or Ctrl held too, as
    // in the window.
    crate::window::own_keys(&popover);
    {
        // Opened, the keys (and so the screen reader) start on the date
        // chosen ("Tuesday, October 6, 2026, selected"), in its month,
        // whatever was browsed the last time (R16-L-02).
        let days = Rc::downgrade(&days);
        popover.connect_show(move |_| {
            if let Some(d) = days.upgrade() {
                d.show_chosen();
            }
            let days = days.clone();
            glib::idle_add_local_once(move || {
                if let Some(d) = days.upgrade() {
                    d.focus_day();
                }
            });
        });
    }
    let button = gtk::MenuButton::builder()
        .child(&label)
        .popover(&popover)
        .css_classes(["wc-field"])
        .build();
    button.set_size_request(-1, md::FIELD_H as i32);
    let bin = MaxWidth::new(&button, md::FIELD_MAX_W as i32);
    DateButton {
        bin,
        button,
        label,
        caption: Rc::default(),
        days,
    }
}

impl DateButton {
    /// The picker is named by its caption and the date it shows ("From
    /// Sunday, October 4, 2026"), as upstream's CalendarDatePicker reads
    /// its header and date (and DGMNB's node).
    fn labelled_by(&self, caption: &FitLabel) {
        self.caption.replace(caption.text());
    }

    /// What shows a date ("Sunday, October 4, 2026") on the button and
    /// names the picker by it: the menu button, and the toggle inside it
    /// that takes focus. (The button weakly: it owns the popover that
    /// owns the days, whose callbacks hold this.)
    fn shower(&self) -> impl Fn(&str) + use<> {
        let (label, caption) = (self.label.clone(), self.caption.clone());
        let button = self.button.downgrade();
        move |date| {
            label.set_text(&format!("{date}  ▾"));
            if let Some(button) = button.upgrade() {
                let name = format!("{} {date}", caption.borrow());
                crate::a11y::name_menu_button(&button, &name);
            }
        }
    }
}

/// A caption ("From", "Difference"), in its 22 px line.
fn section_label(text: &str, texts: &Texts) -> FitLabel {
    let l = FitLabel::new();
    l.set_text(text);
    l.add_css_class("wc-date-label");
    l.set_size_request(-1, md::FIELD_LABEL_H as i32);
    texts.add(&l, "date.label", 1.0);
    l
}

/// A switch's choice: its label set as DGMNB sets it.
fn toggle(name: &str, label: &str, texts: &Texts) -> adw::Toggle {
    let text = FitLabel::drawn();
    text.set_text(label);
    text.set_align(Align::Center);
    texts.add(
        &text,
        "segmented.item",
        appcore::metrics::text::BUTTON_MIN_SIZE,
    );
    adw::Toggle::builder().name(name).child(&text).build()
}

/// The buttons among `w`'s children (a switch's choices).
fn buttons(w: &gtk::Widget) -> impl Iterator<Item = gtk::Button> {
    let mut all = Vec::new();
    let mut child = w.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        if let Ok(b) = c.downcast::<gtk::Button>() {
            all.push(b);
        }
    }
    all.into_iter()
}

/// The space between a part and the next.
fn below(w: &impl IsA<gtk::Widget>, px: f32) {
    w.set_margin_bottom(px.round() as i32);
}

/// A step button of a year, month or day box: DGMNB's icon in place of
/// the toolkit's, at DGMNB's size.
fn step_button(button: &gtk::Widget, icon: &str) -> Option<PathIcon> {
    let button = button.downcast_ref::<gtk::Button>()?;
    let icon = PathIcon::new(icon, ctl::ICON_BUTTON_ICON as i32);
    button.set_child(Some(&icon));
    let inset = md::OFFSET_STEP_INSET as i32;
    let side = (md::OFFSET_STEP_W - 2.0 * md::OFFSET_STEP_INSET) as i32;
    let tall = (md::OFFSET_LINE_H - 2.0 * md::OFFSET_STEP_INSET) as i32;
    button.set_size_request(side, tall);
    button.set_margin_start(inset);
    button.set_margin_end(inset);
    button.set_margin_top(inset);
    button.set_margin_bottom(inset);
    Some(icon)
}

/// A year, month or day box: its caption over DGMNB's [−][box][+]; and
/// the line its number is set in at rest.
fn spin(
    label: &str,
    texts: &Texts,
) -> (gtk::Box, gtk::SpinButton, Vec<PathIcon>, Option<FitLabel>) {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let caption = section_label(label, texts);
    below(&caption, md::OFFSET_LINE_TOP - md::OFFSET_LABEL_H);
    b.append(&caption);
    let spin = gtk::SpinButton::with_range(0.0, datecalc::MAX_OFFSET_VALUE as f64, 1.0);
    spin.set_numeric(true);
    // As narrow as the column makes it (DGMNB's box takes its cell).
    spin.set_width_chars(1);
    spin.set_hexpand(true);
    spin.add_css_class("wc-spin");
    spin.set_size_request(-1, md::OFFSET_LINE_H as i32);
    // The toolkit puts both buttons after the box; DGMNB has − before it.
    // (The spin button lays its parts out in their order.)
    let mut text = None;
    let mut buttons = Vec::new();
    let mut child = spin.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        if c.is::<gtk::Text>() {
            text = Some(c);
        } else if c.is::<gtk::Button>() {
            buttons.push(c);
        }
    }
    let mut icons = Vec::new();
    if let (Some(text), [down, up]) = (&text, buttons.as_slice()) {
        down.insert_before(&spin, Some(text));
        let inset = md::OFFSET_FIELD_INSET as i32;
        text.set_margin_start(inset);
        text.set_margin_end(inset);
        icons.extend(step_button(down, MINUS));
        icons.extend(step_button(up, PLUS));
    }
    // Undo from appcore's history, as DGMNB's offset fields (crate::undo).
    crate::undo::attach(&spin);
    // Upstream: LabeledBy the caption above it.
    spin.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
        caption.upcast_ref()
    ])]);
    // At rest its number is set as DGMNB's field sets it (fit.rs
    // `rest_line`); the box's own shows while it has the focus.
    let line = match text.and_downcast::<gtk::Text>() {
        Some(text) => {
            let rest = rest_line(&spin, &text, &text, "", ctl::FIELD_PAD_X, "wc-spin-rest");
            texts.add(&rest.line, "field", 1.0);
            b.append(&rest.overlay);
            Some(rest.line)
        }
        None => {
            b.append(&spin);
            None
        }
    };
    (b, spin, icons, line)
}

/// A result (DGMNB's 28 px line, shrunk to fit): a readout to a theme's
/// `display` layer.
fn result_line() -> Display {
    let r = Display::line();
    r.add_css_class("wc-date-result");
    r.set_size_request(-1, md::RESULT_H as i32);
    r.set_hexpand(true);
    r
}

/// What holds a result: its caption, the result and the line under it,
/// one under the other as they were. `date.result_panel` styles it; in the
/// base look it is nothing and takes no room.
fn result_panel(parts: &[&gtk::Widget]) -> gtk::Box {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.add_css_class("wc-result-panel");
    for part in parts {
        panel.append(*part);
    }
    panel
}

/// The `{ size, font }` at `path`.
fn sized_font(style: &Style, path: &str) -> Option<gmnb_theme::types::Font> {
    let Some(Value::Record(r)) = style.get(path) else {
        return None;
    };
    let (Some(Value::Number(size)), Some(Value::Font(font))) = (r.get("size"), r.get("font"))
    else {
        return None;
    };
    let mut font = font.clone();
    font.size = *size as f32;
    Some(font)
}

fn spacing(style: &Style, path: &str) -> Spacing {
    match style.get(path) {
        Some(Value::Spacing(s)) => *s,
        _ => Spacing::default(),
    }
}

impl DatePage {
    pub fn new(ctx: Rc<Ctx>) -> Rc<Self> {
        let today = Local::now().date_naive();
        let state = Rc::new(RefCell::new(DateCalculatorState::with_today(today)));
        // Dev/screenshot hook: the "To" date (YYYY-MM-DD).
        if let Some(d) = std::env::var("GMNB_DATE_TO")
            .ok()
            .and_then(|v| NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d").ok())
        {
            state.borrow_mut().set_to_date(datecalc::utc_midnight(d));
        }

        // The page's texts, set in the style's fonts.
        let texts = Texts::default();
        // DGMNB's switch: two halves, the chosen one lit.
        let mode = adw::ToggleGroup::new();
        mode.add(toggle("diff", S::DATE_DIFFERENCE_OPTION, &texts));
        mode.add(toggle("add", S::DATE_ADD_SUBTRACT_OPTION, &texts));
        mode.set_active_name(Some("diff"));
        mode.set_homogeneous(true);
        mode.set_hexpand(true);
        mode.set_halign(gtk::Align::Fill);
        mode.set_size_request(-1, ctl::SEGMENTED_H as i32);
        below(&mode, md::SWITCH_GAP);
        // Each reads as upstream's option ("Difference between dates"), as
        // DGMNB's do.
        let options = [S::DATE_DIFFERENCE_OPTION, S::DATE_ADD_SUBTRACT_OPTION];
        for (b, name) in buttons(mode.upcast_ref()).zip(options) {
            crate::a11y::name_button(&b, name);
        }
        mode.add_css_class("wc-toggle-group");
        mode.upcast_ref::<gtk::Widget>()
            .update_property(&[gtk::accessible::Property::Label(
                S::DATE_CALCULATION_OPTION_AUTOMATION_NAME,
            )]);

        // The space under a caption, before its button or box.
        let caption_gap = md::FIELD_TOP - md::FIELD_LABEL_H;

        // --- difference mode
        let from = date_button(&texts);
        let to = date_button(&texts);
        let diff_result = result_line();
        let diff_days = gtk::Label::new(None);
        diff_days.set_xalign(0.0);
        diff_days.add_css_class("wc-subresult");
        diff_days.set_selectable(true);
        diff_days.set_size_request(-1, md::SUBRESULT_H as i32);

        let diff_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let from_caption = section_label(S::DATE_DIFF_FROM_HEADER, &texts);
        below(&from_caption, caption_gap);
        from.labelled_by(&from_caption);
        diff_box.append(&from_caption);
        diff_box.append(&from.bin);
        let to_caption = section_label(S::DATE_DIFF_TO_HEADER, &texts);
        below(&to_caption, caption_gap);
        to.labelled_by(&to_caption);
        diff_box.append(&to_caption);
        diff_box.append(&to.bin);
        below(&to.bin, md::STEP_TO_LABEL - md::FIELD_TOP - md::FIELD_H);
        let diff_caption = section_label(S::DATE_DIFFERENCE_LABEL, &texts);
        below(&diff_caption, md::STEP_LABEL_RESULT - md::FIELD_LABEL_H);
        below(&diff_result, md::STEP_RESULT_NEXT - md::RESULT_H);
        diff_box.append(&result_panel(&[
            diff_caption.upcast_ref(),
            diff_result.upcast_ref(),
            diff_days.upcast_ref(),
        ]));

        // --- add/subtract mode
        let start = date_button(&texts);
        below(&start.bin, md::STEP_FROM_OP - md::FIELD_TOP - md::FIELD_H);
        let op = adw::ToggleGroup::new();
        op.add(toggle("add", S::ADD_OPTION, &texts));
        op.add(toggle("sub", S::SUBTRACT_OPTION, &texts));
        for (b, name) in buttons(op.upcast_ref()).zip([S::ADD_OPTION, S::SUBTRACT_OPTION]) {
            crate::a11y::name_button(&b, name);
        }
        op.set_active_name(Some("add"));
        op.add_css_class("wc-toggle-group");
        op.add_css_class("wc-date-op");
        op.set_homogeneous(true);
        op.set_size_request(-1, md::OP_SWITCH_H as i32);
        let op_bin = MaxWidth::new(&op, md::OP_SWITCH_W as i32);
        below(&op_bin, md::STEP_OP_OFFSETS - md::OP_SWITCH_H);
        let (yb, years, yi, yl) = spin(S::YEARS_LABEL, &texts);
        let (mb, months, mi, ml) = spin(S::MONTHS_LABEL, &texts);
        let (db, days, di, dl) = spin(S::DAYS_LABEL, &texts);
        let spins = gtk::Box::new(gtk::Orientation::Horizontal, md::OFFSETS_GAP as i32);
        spins.set_homogeneous(true);
        spins.append(&yb);
        spins.append(&mb);
        spins.append(&db);
        // DGMNB's three cells fall on fractions of a pixel, the toolkit's
        // boxes on whole ones: each number is set where DGMNB's field puts
        // it, past its step button and the field's inset and padding.
        for (i, line) in [yl, ml, dl].into_iter().enumerate() {
            let (Some(line), row) = (line, spins.downgrade()) else {
                continue;
            };
            line.set_shift(move |l| {
                let Some(row) = row.upgrade() else {
                    return (0.0, 0.0);
                };
                let gap = md::OFFSETS_GAP;
                let cell = (row.width() as f32 - 2.0 * gap) / 3.0;
                let want = i as f32 * (cell + gap)
                    + md::OFFSET_STEP_W
                    + md::OFFSET_FIELD_INSET
                    + ctl::FIELD_PAD_X;
                let at = gtk::graphene::Point::new(0.0, 0.0);
                let have = l.compute_point(&row, &at).map_or(want, |p| p.x());
                (want - have, 0.0)
            });
        }
        // The toolkit's boxes need room for their largest value: in a
        // window narrower than that they are cut at the right rather than
        // widen the page.
        let spins_bin = gtk::ScrolledWindow::builder()
            .child(&spins)
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_width(true)
            .propagate_natural_height(true)
            .build();
        below(
            &spins_bin,
            md::STEP_OFFSETS_LABEL - md::OFFSET_LINE_TOP - md::OFFSET_LINE_H,
        );
        let step_icons: Vec<PathIcon> = yi.into_iter().chain(mi).chain(di).collect();
        let date_result = result_line();

        let add_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let start_caption = section_label(S::ADD_SUBTRACT_FROM_HEADER, &texts);
        below(&start_caption, caption_gap);
        start.labelled_by(&start_caption);
        add_box.append(&start_caption);
        add_box.append(&start.bin);
        add_box.append(&op_bin);
        add_box.append(&spins_bin);
        let date_caption = section_label(S::DATE_LABEL, &texts);
        below(&date_caption, md::STEP_LABEL_RESULT - md::FIELD_LABEL_H);
        add_box.append(&result_panel(&[
            date_caption.upcast_ref(),
            date_result.upcast_ref(),
        ]));

        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
        stack.set_vhomogeneous(false);
        stack.add_named(&diff_box, Some("diff"));
        stack.add_named(&add_box, Some("add"));

        let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
        col.add_css_class("wc-plain");
        col.append(&mode);
        col.append(&stack);
        let column = MaxWidth::new(&col, md::MAX_WIDTH as i32);
        let scroll = gtk::ScrolledWindow::builder()
            .child(&column)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();

        let page = Rc::new(DatePage {
            root: scroll.upcast(),
            ctx: ctx.clone(),
            state: state.clone(),
            _days: [from.days.clone(), to.days.clone(), start.days.clone()],
        });
        {
            let st = state.borrow();
            from.days.set_date(st.from_date().date_naive());
            to.days.set_date(st.to_date().date_naive());
            start.days.set_date(st.start_date().date_naive());
        }

        // The style: the column's margins, width and gap, the results'
        // font, the step buttons' icons, the switch's motion.
        {
            let (from_bin, stack) = (from.bin.clone(), stack.clone());
            let grids = [from.days.clone(), to.days.clone(), start.days.clone()];
            let (diff_result, date_result) = (diff_result.clone(), date_result.clone());
            ctx.hub.subscribe_style(&column, move |column, style| {
                texts.apply(style);
                let m = spacing(style, "layout.date.margin");
                column.set_margin_top(m.top.round() as i32);
                column.set_margin_end(m.right.round() as i32);
                column.set_margin_bottom(m.bottom.round() as i32);
                column.set_margin_start(m.left.round() as i32);
                column.set_max(style.number("layout.date.max_width").round() as i32);
                let gap = style.number("layout.date.gap");
                below(
                    &from_bin,
                    md::FIELD_BLOCK_H - md::FIELD_TOP - md::FIELD_H + gap,
                );
                stack.set_transition_duration(style.number("motion.date").round() as u32);
                for (result, path) in [
                    (&diff_result, "date.result"),
                    (&date_result, "date.add_result"),
                ] {
                    if let Some(font) = sized_font(style, path) {
                        result.set_font(&font, txt::DATE_RESULT_MIN_SIZE);
                    }
                    result.set_halo(style.shadows("window.text_halo"));
                }
                for g in &grids {
                    g.set_style(style);
                }
                let icon = style.control("icon_button").map(|c| c.icon);
                for i in &step_icons {
                    if let Some(icon) = icon {
                        i.set_style(
                            icon.size,
                            icon.stroke.unwrap_or(style.icon_stroke(icon.size)),
                        );
                    }
                }
            });
        }

        let refresh = {
            let state = state.clone();
            // What the shown result was when it was last told.
            let told = RefCell::new(None::<String>);
            let (from_b, to_b, start_b) = (from.shower(), to.shower(), start.shower());
            let (diff_result, diff_days, date_result) =
                (diff_result.clone(), diff_days.clone(), date_result.clone());
            // `change`: how the results have changed, for a theme's
            // `display` layer (and its `result` event).
            Rc::new(move |change: Change| {
                let st = state.borrow();
                from_b(&datecalc::format_long_date(&st.from_date()));
                to_b(&datecalc::format_long_date(&st.to_date()));
                start_b(&datecalc::format_long_date(&st.start_date()));
                diff_result.set_value(st.str_date_diff_result(), change, false);
                diff_result.update_property(&[gtk::accessible::Property::Label(
                    st.str_date_diff_result_automation_name(),
                )]);
                diff_days.set_text(st.str_date_diff_result_in_days());
                diff_days.set_visible(!st.is_diff_in_days());
                date_result.set_value(st.str_date_result(), change, st.is_out_of_bound());
                if st.is_out_of_bound() {
                    date_result.add_css_class("wc-error");
                } else {
                    date_result.remove_css_class("wc-error");
                }
                date_result.update_property(&[gtk::accessible::Property::Label(
                    st.str_date_result_automation_name(),
                )]);
                // The shown mode's result is told when it changes, as
                // upstream's two result labels are polite live regions
                // (DateDiffAllUnitsResultLabel and DateResultLabel,
                // DateCalculator.xaml:1089 and 1226) and as DGMNB's nodes
                // are told: once, the result alone (not the days under a
                // difference), and not what the page shows as it opens.
                let (shown, name) = if st.is_date_diff_mode() {
                    (&diff_result, st.str_date_diff_result_automation_name())
                } else {
                    (&date_result, st.str_date_result_automation_name())
                };
                let was = told.replace(Some(name.to_string()));
                if was.is_some_and(|was| was != name) {
                    shown.announce(name, gtk::AccessibleAnnouncementPriority::Medium);
                }
            })
        };
        refresh(Change::None);

        for (which, b) in [(0, &from), (1, &to), (2, &start)] {
            // Weak button: it owns the popover that owns these days.
            let (state, refresh, button) = (state.clone(), refresh.clone(), b.button.downgrade());
            let ctx = ctx.clone();
            b.days.connect_picked(move |d| {
                let Some(button) = button.upgrade() else {
                    return;
                };
                {
                    let mut st = state.borrow_mut();
                    match which {
                        0 => st.set_from_date(d),
                        1 => st.set_to_date(d),
                        _ => st.set_start_date(d),
                    }
                }
                refresh(Change::Result);
                button.popdown();
                let picked = crate::layers::Source::Date;
                ctx.hub.layers().pressed(&button, picked, None);
            });
        }

        {
            let (state, refresh, stack) = (state.clone(), refresh.clone(), stack.clone());
            mode.connect_active_name_notify(move |g| {
                let diff = g.active_name().as_deref() == Some("diff");
                state.borrow_mut().set_is_date_diff_mode(diff);
                stack.set_visible_child_name(if diff { "diff" } else { "add" });
                refresh(Change::Replace);
            });
        }
        // Dev/screenshot hook: open on "Add or subtract days".
        if std::env::var_os("GMNB_DATE_ADD").is_some() {
            mode.set_active_name(Some("add"));
        }
        {
            let (state, refresh) = (state.clone(), refresh.clone());
            op.connect_active_name_notify(move |g| {
                state
                    .borrow_mut()
                    .set_is_add_mode(g.active_name().as_deref() == Some("add"));
                refresh(Change::Result);
            });
        }
        for (which, spin) in [(0, &years), (1, &months), (2, &days)] {
            let (state, refresh) = (state.clone(), refresh.clone());
            spin.connect_value_changed(move |s| {
                let v = s.value_as_int();
                {
                    let mut st = state.borrow_mut();
                    match which {
                        0 => st.set_years_offset(v),
                        1 => st.set_months_offset(v),
                        _ => st.set_days_offset(v),
                    }
                }
                refresh(Change::Typing);
            });
        }
        page
    }
}

impl Page for DatePage {
    fn widget(&self) -> gtk::Widget {
        self.root.clone()
    }

    fn activate(&self, _mode: ViewMode) {}

    fn copy(&self) -> Option<String> {
        let text = self.state.borrow().copy_text().to_string();
        self.ctx.copy_to_clipboard(&text);
        Some(text)
    }
}
