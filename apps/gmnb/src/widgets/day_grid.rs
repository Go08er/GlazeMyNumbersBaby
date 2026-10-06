//! `DayGrid` — a month of days to pick a date from, in the date pickers'
//! popovers.
//!
//! GTK's calendar draws its days itself: assistive technology gets one
//! unnamed widget and never hears which day the keys are on. Here each day
//! is a button named by its date ("Tuesday, October 6, 2026", the
//! original's long date, as DGMNB's), the chosen one selected and today
//! described so, under a heading that names the month ("October 2026")
//! between "Previous month"/"Next month" (and year) buttons. Sunday comes
//! first, as in upstream's en-US picker and DGMNB's.
//!
//! Keys: Tab reaches the four buttons and one day (the one the keys are
//! on); the arrow keys move a day or a week, Page Up/Down a month (with
//! Shift a year), Home/End to the week's ends, crossing into the months
//! around as they go; Space or Enter picks the day, as a click does. The
//! focus, the keys' day and what the focused day is called stay in step:
//! the month buttons move the focus with the keys when a day had it (an
//! assistive technology's click), and each opening starts again on the
//! date chosen ([`DayGrid::show_chosen`]).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::{Datelike, Days, Local, Months, NaiveDate};
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;

use super::icon::{PathIcon, paths};

const CHEVRON_RIGHT: &str = "M9 6l6 6-6 6";
const DOUBLE_LEFT: &str = "M17 6l-6 6 6 6M11 6l-6 6 6 6";
const DOUBLE_RIGHT: &str = "M7 6l6 6-6 6M13 6l6 6-6 6";
/// Told of each date picked.
type Picked = Box<dyn Fn(NaiveDate)>;

const WEEKDAYS: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

pub struct DayGrid {
    root: gtk::Box,
    title: gtk::Label,
    /// Previous year, previous month, next month, next year.
    nav: [gtk::Button; 4],
    days: Vec<gtk::Button>,
    /// The first of the month shown.
    shown: Cell<NaiveDate>,
    /// The date chosen.
    selected: Cell<NaiveDate>,
    /// The day the keys are on: the one day Tab reaches.
    cursor: Cell<NaiveDate>,
    picked: RefCell<Vec<Picked>>,
}

fn nav_button(icon: &str, name: &str) -> gtk::Button {
    let b = gtk::Button::builder()
        .child(&PathIcon::new(icon, 16))
        .tooltip_text(name)
        .css_classes(["flat", "circular", "wc-days-nav"])
        .build();
    b.update_property(&[gtk::accessible::Property::Label(name)]);
    b
}

/// The days the grid shows for the month starting `first`: from the Sunday
/// on or before it, six weeks.
fn first_cell(first: NaiveDate) -> NaiveDate {
    first - Days::new(u64::from(first.weekday().num_days_from_sunday()))
}

fn clamp(d: NaiveDate) -> NaiveDate {
    d.clamp(datecalc::picker_min_date(), datecalc::picker_max_date())
}

fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

/// `d` moved by `months` (whole months, the day kept where the month has
/// it, else its last), within the pickers' range.
fn add_months(d: NaiveDate, months: i32) -> NaiveDate {
    let moved = if months >= 0 {
        d.checked_add_months(Months::new(months.unsigned_abs()))
    } else {
        d.checked_sub_months(Months::new(months.unsigned_abs()))
    };
    clamp(moved.unwrap_or(d))
}

impl DayGrid {
    pub fn new() -> Rc<Self> {
        let title = gtk::Label::builder()
            .accessible_role(gtk::AccessibleRole::Heading)
            .hexpand(true)
            .css_classes(["wc-days-title"])
            .build();
        title.update_property(&[gtk::accessible::Property::Level(2)]);
        let nav = [
            nav_button(DOUBLE_LEFT, "Previous year"),
            nav_button(paths::CHEVRON_LEFT, "Previous month"),
            nav_button(CHEVRON_RIGHT, "Next month"),
            nav_button(DOUBLE_RIGHT, "Next year"),
        ];
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        head.append(&nav[0]);
        head.append(&nav[1]);
        head.append(&title);
        head.append(&nav[2]);
        head.append(&nav[3]);

        let grid = gtk::Grid::builder()
            .row_homogeneous(true)
            .column_homogeneous(true)
            .row_spacing(2)
            .column_spacing(2)
            .css_classes(["wc-days-grid"])
            .build();
        // The weekdays are read with each day's date; drawn only.
        for (c, w) in WEEKDAYS.into_iter().enumerate() {
            let l = gtk::Label::builder()
                .label(w)
                .accessible_role(gtk::AccessibleRole::Presentation)
                .css_classes(["wc-days-weekday"])
                .build();
            grid.attach(&l, c as i32, 0, 1, 1);
        }
        // A list named by the month (the heading's text), so moving into
        // it says which month it is; each day a list item, as DGMNB's (GTK
        // reads an "option" as an option pane).
        let days_box = gtk::Grid::builder()
            .accessible_role(gtk::AccessibleRole::List)
            .row_homogeneous(true)
            .column_homogeneous(true)
            .row_spacing(2)
            .column_spacing(2)
            .build();
        days_box.update_relation(&[gtk::accessible::Relation::LabelledBy(&[title.upcast_ref()])]);
        let mut days = Vec::with_capacity(42);
        for i in 0..42 {
            let b = gtk::Button::builder()
                .accessible_role(gtk::AccessibleRole::ListItem)
                .css_classes(["flat", "circular", "wc-day"])
                .build();
            days_box.attach(&b, i % 7, i / 7, 1, 1);
            days.push(b);
        }
        grid.attach(&days_box, 0, 1, 7, 6);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.add_css_class("wc-days");
        root.append(&head);
        root.append(&grid);

        let today = Local::now().date_naive();
        let this = Rc::new(DayGrid {
            root,
            title,
            nav,
            days,
            shown: Cell::new(first_of_month(today)),
            selected: Cell::new(today),
            cursor: Cell::new(today),
            picked: RefCell::default(),
        });

        for (b, months) in this.nav.iter().zip([-12, -1, 1, 12]) {
            let weak = Rc::downgrade(&this);
            b.connect_clicked(move |_| {
                if let Some(g) = weak.upgrade() {
                    // Clicked from the keyboard, the button keeps the focus.
                    // Clicked by assistive technology while a day has it,
                    // the focus moves with the keys to the new month's day,
                    // rather than stay on a button now showing another
                    // date (R16-L-02).
                    let from = g.focused_day();
                    let c = from.unwrap_or(g.cursor.get());
                    g.move_cursor(add_months(c, months), from.is_some());
                    // The heading changed under a button that kept the
                    // focus: say the month.
                    g.title
                        .announce(&g.title.text(), gtk::AccessibleAnnouncementPriority::Medium);
                }
            });
        }
        for (i, b) in this.days.iter().enumerate() {
            let weak = Rc::downgrade(&this);
            b.connect_clicked(move |_| {
                if let Some(g) = weak.upgrade() {
                    let d = first_cell(g.shown.get()) + Days::new(i as u64);
                    g.pick(clamp(d));
                }
            });
        }
        let keys = gtk::EventControllerKey::new();
        {
            let weak = Rc::downgrade(&this);
            keys.connect_key_pressed(move |_, key, _, mods| {
                let Some(g) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                if mods.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK) {
                    return glib::Propagation::Proceed;
                }
                let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
                // From the day focused (always the keys' own, but should
                // they differ, the one announced).
                let c = g.focused_day().unwrap_or(g.cursor.get());
                let weekday = i64::from(c.weekday().num_days_from_sunday());
                let by_days = |n: i64| {
                    let moved = if n >= 0 {
                        c.checked_add_days(Days::new(n as u64))
                    } else {
                        c.checked_sub_days(Days::new(n.unsigned_abs()))
                    };
                    clamp(moved.unwrap_or(c))
                };
                use gdk::Key;
                let to = match key {
                    Key::Left | Key::KP_Left if !shift => by_days(-1),
                    Key::Right | Key::KP_Right if !shift => by_days(1),
                    Key::Up | Key::KP_Up if !shift => by_days(-7),
                    Key::Down | Key::KP_Down if !shift => by_days(7),
                    Key::Home | Key::KP_Home if !shift => by_days(-weekday),
                    Key::End | Key::KP_End if !shift => by_days(6 - weekday),
                    Key::Page_Up | Key::KP_Page_Up => add_months(c, if shift { -12 } else { -1 }),
                    Key::Page_Down | Key::KP_Page_Down => add_months(c, if shift { 12 } else { 1 }),
                    _ => return glib::Propagation::Proceed,
                };
                g.move_cursor(to, true);
                glib::Propagation::Stop
            });
        }
        days_box.add_controller(keys);
        this.render();
        this
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    /// Shows `d`'s month with `d` chosen and the keys on it.
    pub fn set_date(&self, d: NaiveDate) {
        let d = clamp(d);
        self.selected.set(d);
        self.cursor.set(d);
        self.shown.set(first_of_month(d));
        self.render();
    }

    /// Shows the chosen date's month with the keys on it again, whatever
    /// was browsed since: each time the picker opens, it starts on the
    /// date chosen, shown and selected (R16-L-02).
    pub fn show_chosen(&self) {
        let d = self.selected.get();
        self.cursor.set(d);
        self.shown.set(first_of_month(d));
        self.render();
    }

    /// Puts the keyboard's focus on the day the keys are on.
    pub fn focus_day(&self) {
        if let Some(b) = self.button(self.cursor.get()) {
            b.grab_focus();
        }
    }

    /// The date of the day button that has the focus, if one has.
    fn focused_day(&self) -> Option<NaiveDate> {
        let i = self.days.iter().position(|b| b.is_focus())?;
        Some(clamp(first_cell(self.shown.get()) + Days::new(i as u64)))
    }

    /// Calls `f` with each date picked (clicked, or Space/Enter).
    pub fn connect_picked(&self, f: impl Fn(NaiveDate) + 'static) {
        self.picked.borrow_mut().push(Box::new(f));
    }

    fn pick(&self, d: NaiveDate) {
        self.selected.set(d);
        self.cursor.set(d);
        self.render();
        for f in self.picked.borrow().iter() {
            f(d);
        }
    }

    fn button(&self, d: NaiveDate) -> Option<&gtk::Button> {
        let i = (d - first_cell(self.shown.get())).num_days();
        usize::try_from(i).ok().and_then(|i| self.days.get(i))
    }

    /// Moves the keys to `d`, showing its month if it isn't shown; with
    /// `focus`, the focus too.
    fn move_cursor(&self, d: NaiveDate, focus: bool) {
        self.cursor.set(d);
        if first_of_month(d) != self.shown.get() {
            self.shown.set(first_of_month(d));
        }
        self.render();
        if focus {
            self.focus_day();
        }
    }

    fn render(&self) {
        let shown = self.shown.get();
        self.title.set_text(&shown.format("%B %Y").to_string());
        let (min, max) = (datecalc::picker_min_date(), datecalc::picker_max_date());
        let today = Local::now().date_naive();
        let start = first_cell(shown);
        for (i, b) in self.days.iter().enumerate() {
            let d = start + Days::new(i as u64);
            b.set_label(&d.day().to_string());
            // A button with a label is labelled by it: named by the date.
            b.reset_relation(gtk::AccessibleRelation::LabelledBy);
            b.update_property(&[gtk::accessible::Property::Label(
                &datecalc::format_long_date(&datecalc::utc_midnight(d)),
            )]);
            if d == today {
                b.update_property(&[gtk::accessible::Property::Description("Today")]);
            } else {
                b.reset_property(gtk::AccessibleProperty::Description);
            }
            let selected = d == self.selected.get();
            b.update_state(&[gtk::accessible::State::Selected(Some(selected))]);
            for (on, class) in [
                (d.month() != shown.month(), "wc-day-other"),
                (d == today, "wc-day-today"),
                (selected, "wc-day-selected"),
            ] {
                if on {
                    b.add_css_class(class);
                } else {
                    b.remove_css_class(class);
                }
            }
            b.set_sensitive(d >= min && d <= max);
            // One day takes Tab's focus: the one the keys are on.
            b.set_focusable(d == self.cursor.get());
        }
        let first = first_of_month(min);
        let last = first_of_month(max);
        self.nav[0].set_sensitive(add_months(shown, -12) >= first && shown > first);
        self.nav[1].set_sensitive(shown > first);
        self.nav[2].set_sensitive(shown < last);
        self.nav[3].set_sensitive(shown < last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Six weeks from the Sunday on or before the 1st; whole months keep
    /// the day where they have it, else take their last; nothing leaves
    /// the pickers' range (1601-2550).
    #[test]
    fn days_and_months_move_as_a_calendar_does() {
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        // October 2026 starts on a Thursday.
        assert_eq!(first_cell(d(2026, 10, 1)), d(2026, 9, 27));
        assert_eq!(first_cell(d(2026, 11, 1)), d(2026, 11, 1));
        assert_eq!(add_months(d(2026, 1, 31), 1), d(2026, 2, 28));
        assert_eq!(add_months(d(2024, 3, 31), -1), d(2024, 2, 29));
        assert_eq!(add_months(d(2026, 10, 6), -12), d(2025, 10, 6));
        assert_eq!(add_months(d(1601, 3, 1), -12), d(1601, 1, 1));
        assert_eq!(add_months(d(2550, 11, 30), 12), d(2550, 12, 31));
    }
}
