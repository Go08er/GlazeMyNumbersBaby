//! Date calculation (upstream DateCalculator.xaml).

use appcore::input::{Key, KeyPress, Named};
use chrono::{Datelike, Days, Local, Months, NaiveDate};
use datecalc::{DateCalculatorState, strings as S};

use crate::app::{Cx, Msg as AppMsg};
use crate::edit::TextEdit;
use crate::gfx::Rect;
use crate::ui::{Align, BODY, CAPTION, Frame, SMALL, STRONG, Style, id};

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    DiffMode(bool),
    Add(bool),
    Step(u8, i32),
    Calendar(Option<u8>),
    Month(i32),
    Pick(u8, NaiveDate),
}

pub struct DatePage {
    state: DateCalculatorState,
    /// Which picker is open: 0 from, 1 to, 2 start.
    calendar: Option<u8>,
    /// First day of the month shown in the open calendar.
    shown: NaiveDate,
    /// The day the keys are on in the open calendar (the one day Tab
    /// reaches, and where the focus goes when it opens).
    cursor: NaiveDate,
    offsets: [TextEdit; 3],
}

fn msg(m: Msg) -> AppMsg {
    AppMsg::Date(m)
}

fn offset_id(i: usize) -> crate::ui::Id {
    id(("date-offset", i))
}

fn day_id(d: NaiveDate) -> crate::ui::Id {
    id(("cal-day", d.num_days_from_ce()))
}

fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

fn clamp(d: NaiveDate) -> NaiveDate {
    d.clamp(datecalc::picker_min_date(), datecalc::picker_max_date())
}

/// `d` moved by whole months (the day kept where the month has it, else
/// its last), within the pickers' range.
fn add_months(d: NaiveDate, months: i32) -> NaiveDate {
    let moved = if months >= 0 {
        d.checked_add_months(Months::new(months.unsigned_abs()))
    } else {
        d.checked_sub_months(Months::new(months.unsigned_abs()))
    };
    clamp(moved.unwrap_or(d))
}

fn add_days(d: NaiveDate, days: i64) -> NaiveDate {
    let moved = if days >= 0 {
        d.checked_add_days(Days::new(days as u64))
    } else {
        d.checked_sub_days(Days::new(days.unsigned_abs()))
    };
    clamp(moved.unwrap_or(d))
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

impl DatePage {
    pub fn new() -> DatePage {
        let today = Local::now().date_naive();
        DatePage {
            state: DateCalculatorState::with_today(today),
            calendar: None,
            cursor: today,
            shown: today.with_day(1).unwrap_or(today),
            offsets: std::array::from_fn(|_| TextEdit::new("0", 3)),
        }
    }

    fn date(&self, which: u8) -> NaiveDate {
        let d = match which {
            0 => self.state.from_date(),
            1 => self.state.to_date(),
            _ => self.state.start_date(),
        };
        d.date_naive()
    }

    pub fn update(&mut self, m: Msg, cx: &mut Cx) {
        match m {
            Msg::DiffMode(diff) => self.state.set_is_date_diff_mode(diff),
            Msg::Add(add) => self.state.set_is_add_mode(add),
            Msg::Step(i, d) => {
                let v = self.offset(i as usize) + d;
                self.set_offset(i as usize, v);
            }
            Msg::Calendar(which) => {
                let was = self.calendar;
                self.calendar = which;
                if let Some(w) = which {
                    // Opened, the keys (and assistive technology) start on
                    // the date chosen.
                    let d = self.date(w);
                    self.shown = first_of_month(d);
                    self.cursor = d;
                    *cx.focus = Some(day_id(d));
                } else if let Some(w) = was {
                    *cx.focus = Some(id(("date-btn", w)));
                }
            }
            Msg::Month(delta) => {
                self.cursor = add_months(self.cursor, delta);
                self.shown = first_of_month(self.cursor);
            }
            Msg::Pick(which, d) => {
                let d = d.clamp(datecalc::picker_min_date(), datecalc::picker_max_date());
                let dt = datecalc::utc_midnight(d);
                match which {
                    0 => self.state.set_from_date(dt),
                    1 => self.state.set_to_date(dt),
                    _ => self.state.set_start_date(dt),
                }
                self.calendar = None;
                *cx.focus = Some(id(("date-btn", which)));
            }
        }
    }

    fn offset(&self, i: usize) -> i32 {
        match i {
            0 => self.state.years_offset(),
            1 => self.state.months_offset(),
            _ => self.state.days_offset(),
        }
    }

    fn set_offset(&mut self, i: usize, v: i32) {
        let v = v.clamp(0, datecalc::MAX_OFFSET_VALUE);
        match i {
            0 => self.state.set_years_offset(v),
            1 => self.state.set_months_offset(v),
            _ => self.state.set_days_offset(v),
        }
        self.offsets[i].set_text(&v.to_string());
    }

    pub fn field(&mut self, fid: crate::ui::Id) -> Option<&mut TextEdit> {
        (0..3)
            .find(|&i| offset_id(i) == fid)
            .map(|i| &mut self.offsets[i])
    }

    pub fn field_changed(&mut self, fid: crate::ui::Id) {
        if let Some(i) = (0..3).find(|&i| offset_id(i) == fid) {
            let digits: String = self.offsets[i]
                .text
                .chars()
                .filter(char::is_ascii_digit)
                .collect();
            if digits != self.offsets[i].text {
                self.offsets[i].set_text(&digits);
            }
            let v = digits
                .parse::<i32>()
                .unwrap_or(0)
                .min(datecalc::MAX_OFFSET_VALUE);
            match i {
                0 => self.state.set_years_offset(v),
                1 => self.state.set_months_offset(v),
                _ => self.state.set_days_offset(v),
            }
        }
    }

    /// The open calendar's keys: the arrows move a day or a week, Page
    /// Up/Down a month (with Shift a year), Home/End to the week's ends,
    /// into the months around as they go; the focus follows.
    pub fn key(&mut self, kp: &KeyPress, cx: &mut Cx) -> bool {
        if self.calendar.is_none() || kp.ctrl || kp.alt {
            return false;
        }
        let c = self.cursor;
        let weekday = i64::from(c.weekday().num_days_from_sunday());
        let Key::Named(n) = kp.key else {
            return false;
        };
        let to = match n {
            Named::Left if !kp.shift => add_days(c, -1),
            Named::Right if !kp.shift => add_days(c, 1),
            Named::Up if !kp.shift => add_days(c, -7),
            Named::Down if !kp.shift => add_days(c, 7),
            Named::Home if !kp.shift => add_days(c, -weekday),
            Named::End if !kp.shift => add_days(c, 6 - weekday),
            Named::PageUp => add_months(c, if kp.shift { -12 } else { -1 }),
            Named::PageDown => add_months(c, if kp.shift { 12 } else { 1 }),
            _ => return false,
        };
        self.cursor = to;
        self.shown = first_of_month(to);
        *cx.focus = Some(day_id(to));
        true
    }

    pub fn close_popup(&mut self) -> bool {
        self.calendar.take().is_some()
    }

    pub fn copy_text(&self) -> Option<String> {
        Some(self.state.copy_text().to_string())
    }

    fn date_button(&mut self, f: &mut Frame, r: Rect, which: u8, label: &str) {
        let text = datecalc::format_long_date(&match which {
            0 => self.state.from_date(),
            1 => self.state.to_date(),
            _ => self.state.start_date(),
        });
        let t = f.t;
        f.label(r.take_top(22.0).0, label, CAPTION, t.fg_dim, Align::Start);
        let b = Rect::new(r.x, r.y + 24.0, r.w.min(320.0), 38.0);
        f.button(
            id(("date-btn", which)),
            b,
            &format!("{text}  ▾"),
            BODY,
            msg(Msg::Calendar(Some(which))),
            true,
            Some(self.calendar == Some(which)),
            true,
        );
        if let Some(n) = f.nodes.as_mut().and_then(|v| v.last_mut()) {
            n.label = format!("{label} {text}");
        }
    }

    pub fn view(&mut self, f: &mut Frame, area: Rect) {
        let t = f.t;
        let col = area.inset_xy(16.0, 8.0);
        let col = Rect::new(col.x, col.y, col.w.min(480.0), col.h);
        let diff = self.state.is_date_diff_mode();
        let seg = Rect::new(col.x, col.y, col.w, 36.0);
        let (a, b) = (seg.cell(1, 2, 0, 0, 4.0), seg.cell(1, 2, 0, 1, 4.0));
        f.button(
            id("date-diff"),
            a,
            S::DATE_DIFFERENCE_OPTION,
            SMALL,
            msg(Msg::DiffMode(true)),
            true,
            Some(diff),
            true,
        );
        f.button(
            id("date-add"),
            b,
            S::DATE_ADD_SUBTRACT_OPTION,
            SMALL,
            msg(Msg::DiffMode(false)),
            true,
            Some(!diff),
            true,
        );
        let mut y = seg.bottom() + 16.0;
        if diff {
            self.date_button(
                f,
                Rect::new(col.x, y, col.w, 64.0),
                0,
                S::DATE_DIFF_FROM_HEADER,
            );
            y += 72.0;
            self.date_button(
                f,
                Rect::new(col.x, y, col.w, 64.0),
                1,
                S::DATE_DIFF_TO_HEADER,
            );
            y += 80.0;
            f.label(
                Rect::new(col.x, y, col.w, 22.0),
                S::DATE_DIFFERENCE_LABEL,
                CAPTION,
                t.fg_dim,
                Align::Start,
            );
            y += 24.0;
            let res = Rect::new(col.x, y, col.w, 44.0);
            f.label_fit(
                res,
                self.state.str_date_diff_result(),
                Style::new(28.0, 400.0),
                14.0,
                t.fg,
                Align::Start,
            );
            if let Some(n) = f.node(
                id("date-diff-result"),
                accesskit::Role::Label,
                self.state.str_date_diff_result_automation_name(),
                res,
            ) {
                n.live = true;
            }
            y += 46.0;
            if !self.state.is_diff_in_days() {
                let line = Rect::new(col.x, y, col.w, 24.0);
                let days = self.state.str_date_diff_result_in_days();
                f.label(line, days, BODY, t.fg_dim, Align::Start);
                if let Some(n) = f.node(id("date-diff-days"), accesskit::Role::Label, days, line) {
                    n.live = true;
                }
            }
        } else {
            self.date_button(
                f,
                Rect::new(col.x, y, col.w, 64.0),
                2,
                S::ADD_SUBTRACT_FROM_HEADER,
            );
            y += 76.0;
            let add = self.state.is_add_mode();
            let seg = Rect::new(col.x, y, 240.0f32.min(col.w), 34.0);
            f.button(
                id("date-op-add"),
                seg.cell(1, 2, 0, 0, 4.0),
                S::ADD_OPTION,
                SMALL,
                msg(Msg::Add(true)),
                true,
                Some(add),
                true,
            );
            f.button(
                id("date-op-sub"),
                seg.cell(1, 2, 0, 1, 4.0),
                S::SUBTRACT_OPTION,
                SMALL,
                msg(Msg::Add(false)),
                true,
                Some(!add),
                true,
            );
            y += 46.0;
            let row = Rect::new(col.x, y, col.w, 66.0);
            for (i, label) in [S::YEARS_LABEL, S::MONTHS_LABEL, S::DAYS_LABEL]
                .into_iter()
                .enumerate()
            {
                let c = row.cell(1, 3, 0, i, 10.0);
                f.label(c.take_top(22.0).0, label, CAPTION, t.fg_dim, Align::Start);
                let line = Rect::new(c.x, c.y + 24.0, c.w, 36.0);
                let (minus, rest) = line.take_left(30.0);
                let (plus, field) = rest.take_right(30.0);
                f.icon_button(
                    id(("date-minus", i)),
                    minus.inset(1.0),
                    "M7 12h10",
                    &format!("Fewer {}", label.to_lowercase()),
                    msg(Msg::Step(i as u8, -1)),
                    self.offset(i) > 0,
                    None,
                );
                f.text_field(
                    offset_id(i),
                    field.inset_xy(2.0, 0.0),
                    &self.offsets[i],
                    "0",
                    false,
                    label,
                );
                f.icon_button(
                    id(("date-plus", i)),
                    plus.inset(1.0),
                    "M12 7v10M7 12h10",
                    &format!("More {}", label.to_lowercase()),
                    msg(Msg::Step(i as u8, 1)),
                    self.offset(i) < datecalc::MAX_OFFSET_VALUE,
                    None,
                );
            }
            y += 82.0;
            f.label(
                Rect::new(col.x, y, col.w, 22.0),
                S::DATE_LABEL,
                CAPTION,
                t.fg_dim,
                Align::Start,
            );
            y += 24.0;
            let res = Rect::new(col.x, y, col.w, 44.0);
            let color = if self.state.is_out_of_bound() {
                t.danger
            } else {
                t.fg
            };
            f.label_fit(
                res,
                self.state.str_date_result(),
                Style::new(28.0, 400.0),
                14.0,
                color,
                Align::Start,
            );
            if let Some(n) = f.node(
                id("date-result"),
                accesskit::Role::Label,
                self.state.str_date_result_automation_name(),
                res,
            ) {
                n.live = true;
            }
        }
    }

    pub fn overlay(&mut self, f: &mut Frame, area: Rect) {
        let Some(which) = self.calendar else { return };
        let t = f.t;
        f.scrim(msg(Msg::Calendar(None)), false);
        let anchor = f
            .hits
            .iter()
            .find(|h| h.id == id(("date-btn", which)))
            .map(|h| h.rect)
            .unwrap_or(area);
        let (w, h) = (300.0, 316.0);
        let x = anchor.x.min(area.right() - w - 8.0).max(area.x + 8.0);
        let y = if anchor.bottom() + h + 8.0 < area.bottom() {
            anchor.bottom() + 4.0
        } else {
            (anchor.y - h - 4.0).max(area.y + 4.0)
        };
        let card = Rect::new(x, y, w, h);
        f.card(card, 12.0);
        let inner = card.inset(10.0);
        let (head, grid) = inner.take_top(36.0);
        let title = format!(
            "{} {}",
            MONTHS[self.shown.month0() as usize],
            self.shown.year()
        );
        let title_rect = head.inset_xy(76.0, 0.0);
        f.label(title_rect, &title, STRONG, t.fg, Align::Center);
        // The month, a heading (as GMNB's), said when it changes.
        if let Some(n) = f.node(
            id("cal-title"),
            accesskit::Role::Heading,
            &title,
            title_rect,
        ) {
            n.live = true;
        }
        let (min, max) = (datecalc::picker_min_date(), datecalc::picker_max_date());
        let (left, rest) = head.take_left(36.0);
        let (left2, _) = rest.take_left(36.0);
        let (right, rest) = head.take_right(36.0);
        let (right2, _) = rest.take_right(36.0);
        for (bid, r, icon, name, delta, enabled) in [
            (
                "cal-prev-year",
                left,
                "M17 6l-6 6 6 6M11 6l-6 6 6 6",
                "Previous year",
                -12,
                self.shown > first_of_month(min),
            ),
            (
                "cal-prev",
                left2,
                appcore::icons::CHEVRON_LEFT,
                "Previous month",
                -1,
                self.shown > first_of_month(min),
            ),
            (
                "cal-next",
                right2,
                "M9 6l6 6-6 6",
                "Next month",
                1,
                self.shown < first_of_month(max),
            ),
            (
                "cal-next-year",
                right,
                "M7 6l6 6-6 6M13 6l6 6-6 6",
                "Next year",
                12,
                self.shown < first_of_month(max),
            ),
        ] {
            f.icon_button(
                id(bid),
                r,
                icon,
                name,
                msg(Msg::Month(delta)),
                enabled,
                None,
            );
        }
        let (dow, days) = grid.take_top(26.0);
        for (i, d) in ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
            .into_iter()
            .enumerate()
        {
            f.label(
                dow.cell(1, 7, 0, i, 2.0),
                d,
                CAPTION,
                t.fg_dim,
                Align::Center,
            );
        }
        let selected = self.date(which);
        let today = Local::now().date_naive();
        let lead = self.shown.weekday().num_days_from_sunday() as i64;
        let start = self.shown - chrono::Duration::days(lead);
        for i in 0..42 {
            let d = start + chrono::Duration::days(i);
            let c = days.cell(6, 7, (i / 7) as usize, (i % 7) as usize, 2.0);
            let in_month = d.month() == self.shown.month();
            let enabled = d >= min && d <= max;
            let did = day_id(d);
            let sel = d == selected;
            let label = d.day().to_string();
            if sel {
                f.cv.rounded(c, c.h / 2.0, t.accent);
            } else if d == today {
                f.cv.rounded_border(c, c.h / 2.0, t.accent_text, 1.0);
            }
            if enabled {
                f.row(
                    did,
                    c,
                    msg(Msg::Pick(which, d)),
                    false,
                    &datecalc::format_long_date(&datecalc::utc_midnight(d)),
                );
                // Drawn as the accent circle above; tell AT which it is,
                // and which is today (as GMNB's). One day takes Tab: the
                // one the keys are on.
                let keys_here = d == self.cursor;
                if let Some(n) = f.nodes.as_mut().and_then(|v| v.last_mut()) {
                    n.selected = Some(sel);
                    n.focusable = keys_here;
                    if d == today {
                        n.description = Some(("Today".into(), None));
                    }
                }
                if let Some(h) = f.hits.last_mut() {
                    h.focusable = keys_here;
                }
            }
            let color = if sel {
                t.on_accent
            } else if !in_month || !enabled {
                t.fg_faint
            } else {
                t.fg
            };
            f.label(c, &label, SMALL, color, Align::Center);
        }
    }
}
