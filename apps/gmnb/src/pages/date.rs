//! Date calculation: "Difference between dates" and "Add or subtract days".

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use chrono::{Datelike, Local, NaiveDate};
use datecalc::{DateCalculatorState, strings as S};
use gtk::glib;

use super::{Ctx, Page};
use crate::widgets::display::{Change, Display};
use crate::widgets::icon::{PathIcon, paths};
use appcore::modes::ViewMode;

pub struct DatePage {
    root: gtk::Widget,
    ctx: Rc<Ctx>,
    state: Rc<RefCell<DateCalculatorState>>,
}

struct DateButton {
    button: gtk::MenuButton,
    label: gtk::Label,
    calendar: gtk::Calendar,
}

fn date_button() -> DateButton {
    let label = gtk::Label::new(None);
    label.set_xalign(0.0);
    label.set_hexpand(true);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    content.append(&label);
    content.append(&PathIcon::new(paths::DATE, 18));
    let calendar = gtk::Calendar::new();
    let popover = gtk::Popover::builder()
        .child(&calendar)
        .css_classes(["wc-calendar-popover"])
        .build();
    let button = gtk::MenuButton::builder()
        .child(&content)
        .popover(&popover)
        .css_classes(["wc-field"])
        .build();
    DateButton {
        button,
        label,
        calendar,
    }
}

fn to_glib(d: NaiveDate) -> glib::DateTime {
    glib::DateTime::from_local(d.year(), d.month() as i32, d.day() as i32, 12, 0, 0.0)
        .expect("valid date")
}

fn from_glib(d: &glib::DateTime) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(d.year(), d.month() as u32, d.day_of_month() as u32)
}

fn section_label(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.add_css_class("wc-section");
    l
}

fn spin(label: &str) -> (gtk::Box, gtk::SpinButton) {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 4);
    b.append(&section_label(label));
    let spin = gtk::SpinButton::with_range(0.0, datecalc::MAX_OFFSET_VALUE as f64, 1.0);
    spin.set_numeric(true);
    spin.add_css_class("wc-spin");
    spin.update_property(&[gtk::accessible::Property::Label(label)]);
    b.append(&spin);
    (b, spin)
}

impl DatePage {
    pub fn new(ctx: Rc<Ctx>) -> Rc<Self> {
        let today = Local::now().date_naive();
        let state = Rc::new(RefCell::new(DateCalculatorState::with_today(today)));

        let mode = adw::ToggleGroup::new();
        mode.add(
            adw::Toggle::builder()
                .name("diff")
                .label("Difference")
                .tooltip(S::DATE_DIFFERENCE_OPTION)
                .build(),
        );
        mode.add(
            adw::Toggle::builder()
                .name("add")
                .label("Add or subtract")
                .tooltip(S::DATE_ADD_SUBTRACT_OPTION)
                .build(),
        );
        mode.set_active_name(Some("diff"));
        mode.add_css_class("wc-toggle-group");
        mode.upcast_ref::<gtk::Widget>()
            .update_property(&[gtk::accessible::Property::Label(
                S::DATE_CALCULATION_OPTION_AUTOMATION_NAME,
            )]);

        // --- difference mode
        let from = date_button();
        let to = date_button();
        let diff_result = Display::new(34.0);
        diff_result.set_show_expression(false);
        diff_result.set_align_end(false);
        diff_result.set_weight(300);
        let diff_days = gtk::Label::new(None);
        diff_days.set_xalign(0.0);
        diff_days.add_css_class("wc-subresult");
        diff_days.set_selectable(true);

        let diff_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        diff_box.append(&section_label(S::DATE_DIFF_FROM_HEADER));
        diff_box.append(&from.button);
        diff_box.append(&section_label(S::DATE_DIFF_TO_HEADER));
        diff_box.append(&to.button);
        let res_panel = gtk::Box::new(gtk::Orientation::Vertical, 2);
        res_panel.add_css_class("wc-glass-panel");
        res_panel.add_css_class("wc-result-panel");
        res_panel.append(&section_label(S::DATE_DIFFERENCE_LABEL));
        res_panel.append(&diff_result);
        res_panel.append(&diff_days);
        diff_box.append(&res_panel);

        // --- add/subtract mode
        let start = date_button();
        let op = adw::ToggleGroup::new();
        op.add(
            adw::Toggle::builder()
                .name("add")
                .label(S::ADD_OPTION)
                .build(),
        );
        op.add(
            adw::Toggle::builder()
                .name("sub")
                .label(S::SUBTRACT_OPTION)
                .build(),
        );
        op.set_active_name(Some("add"));
        op.add_css_class("wc-toggle-group");
        op.set_halign(gtk::Align::Start);
        let (yb, years) = spin(S::YEARS_LABEL);
        let (mb, months) = spin(S::MONTHS_LABEL);
        let (db, days) = spin(S::DAYS_LABEL);
        let spins = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        spins.set_homogeneous(true);
        spins.append(&yb);
        spins.append(&mb);
        spins.append(&db);
        let date_result = Display::new(30.0);
        date_result.set_show_expression(false);
        date_result.set_align_end(false);
        date_result.set_weight(300);

        let add_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        add_box.append(&section_label(S::ADD_SUBTRACT_FROM_HEADER));
        add_box.append(&start.button);
        add_box.append(&op);
        add_box.append(&spins);
        let res2 = gtk::Box::new(gtk::Orientation::Vertical, 2);
        res2.add_css_class("wc-glass-panel");
        res2.add_css_class("wc-result-panel");
        res2.append(&section_label(S::DATE_LABEL));
        res2.append(&date_result);
        add_box.append(&res2);

        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
        stack.set_transition_duration(260);
        stack.set_vhomogeneous(false);
        stack.add_named(&diff_box, Some("diff"));
        stack.add_named(&add_box, Some("add"));

        let col = gtk::Box::new(gtk::Orientation::Vertical, 14);
        col.set_margin_top(4);
        col.set_margin_start(16);
        col.set_margin_end(16);
        col.set_margin_bottom(16);
        col.append(&mode);
        col.append(&stack);
        let clamp = adw::Clamp::builder().maximum_size(560).child(&col).build();
        let scroll = gtk::ScrolledWindow::builder()
            .child(&clamp)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();

        for (b, d) in [(&from, today), (&to, today), (&start, today)] {
            b.calendar.select_day(&to_glib(d));
        }
        let page = Rc::new(DatePage {
            root: scroll.upcast(),
            ctx: ctx.clone(),
            state: state.clone(),
        });

        // Only while the displays live: the hub outlasts this page.
        ctx.hub
            .subscribe_while(&diff_result, |d, s| d.set_scheme(*s));
        ctx.hub
            .subscribe_while(&date_result, |d, s| d.set_scheme(*s));

        let refresh = {
            let state = state.clone();
            let (from_l, to_l, start_l) =
                (from.label.clone(), to.label.clone(), start.label.clone());
            let (diff_result, diff_days, date_result) =
                (diff_result.clone(), diff_days.clone(), date_result.clone());
            Rc::new(move |change: Change| {
                let st = state.borrow();
                from_l.set_text(&datecalc::format_long_date(&st.from_date()));
                to_l.set_text(&datecalc::format_long_date(&st.to_date()));
                start_l.set_text(&datecalc::format_long_date(&st.start_date()));
                diff_result.set_value(st.str_date_diff_result(), change, false);
                diff_result.update_property(&[gtk::accessible::Property::Label(
                    st.str_date_diff_result_automation_name(),
                )]);
                diff_days.set_text(st.str_date_diff_result_in_days());
                diff_days.set_visible(!st.is_diff_in_days());
                date_result.set_value(st.str_date_result(), change, st.is_out_of_bound());
                date_result.update_property(&[gtk::accessible::Property::Label(
                    st.str_date_result_automation_name(),
                )]);
            })
        };
        refresh(Change::None);

        let limit = |c: &gtk::Calendar| -> Option<NaiveDate> {
            let d = from_glib(&c.date())?;
            Some(d.clamp(datecalc::picker_min_date(), datecalc::picker_max_date()))
        };
        for (which, b) in [(0, &from), (1, &to), (2, &start)] {
            // Weak button: it owns the popover that owns this calendar.
            let (state, refresh, button) = (state.clone(), refresh.clone(), b.button.downgrade());
            let ctx = ctx.clone();
            b.calendar.connect_day_selected(move |c| {
                let (Some(d), Some(button)) = (limit(c), button.upgrade()) else {
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
                let s = ctx.hub.scheme();
                let (w, h) = (button.width() as f32, button.height() as f32);
                ctx.pulse_at(&button, w * 0.5, h * 0.5, s.hot_a, 0.9);
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
