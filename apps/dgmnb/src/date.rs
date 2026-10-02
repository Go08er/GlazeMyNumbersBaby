//! Date calculation (upstream DateCalculator.xaml).

use chrono::{Datelike, Local, NaiveDate};
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
    offsets: [TextEdit; 3],
}

fn msg(m: Msg) -> AppMsg {
    AppMsg::Date(m)
}

fn offset_id(i: usize) -> crate::ui::Id {
    id(("date-offset", i))
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

    pub fn update(&mut self, m: Msg, _cx: &mut Cx) {
        match m {
            Msg::DiffMode(diff) => self.state.set_is_date_diff_mode(diff),
            Msg::Add(add) => self.state.set_is_add_mode(add),
            Msg::Step(i, d) => {
                let v = self.offset(i as usize) + d;
                self.set_offset(i as usize, v);
            }
            Msg::Calendar(which) => {
                self.calendar = which;
                if let Some(w) = which {
                    let d = self.date(w);
                    self.shown = d.with_day(1).unwrap_or(d);
                }
            }
            Msg::Month(delta) => {
                let m0 = self.shown.year() * 12 + self.shown.month0() as i32 + delta;
                if let Some(d) =
                    NaiveDate::from_ymd_opt(m0.div_euclid(12), m0.rem_euclid(12) as u32 + 1, 1)
                {
                    self.shown = d;
                }
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
                f.label(
                    Rect::new(col.x, y, col.w, 24.0),
                    self.state.str_date_diff_result_in_days(),
                    BODY,
                    t.fg_dim,
                    Align::Start,
                );
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
        f.label(
            head.inset_xy(40.0, 0.0),
            &title,
            STRONG,
            t.fg,
            Align::Center,
        );
        f.icon_button(
            id("cal-prev"),
            head.take_left(36.0).0,
            appcore::icons::CHEVRON_LEFT,
            "Previous month",
            msg(Msg::Month(-1)),
            true,
            None,
        );
        f.icon_button(
            id("cal-next"),
            head.take_right(36.0).0,
            "M9 6l6 6-6 6",
            "Next month",
            msg(Msg::Month(1)),
            true,
            None,
        );
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
        let (min, max) = (datecalc::picker_min_date(), datecalc::picker_max_date());
        for i in 0..42 {
            let d = start + chrono::Duration::days(i);
            let c = days.cell(6, 7, (i / 7) as usize, (i % 7) as usize, 2.0);
            let in_month = d.month() == self.shown.month();
            let enabled = d >= min && d <= max;
            let did = id(("cal-day", d.num_days_from_ce()));
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
