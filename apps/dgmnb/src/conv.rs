//! Unit and currency converters (upstream UnitConverter.xaml).

use std::time::Duration;

use appcore::KeyPress;
use appcore::dbus;
use appcore::input;
use appcore::keys::{self, conv};
use unitconv::{ConverterMode, UnitConverterViewModel};
use winit::event_loop::EventLoopProxy;

use crate::app::{Cx, DATA_DIR, Msg as AppMsg, UserEvent};
use crate::edit::TextEdit;
use crate::gfx::Rect;
use crate::ui::{Align, BODY, CAPTION, Frame, KeyLook, SMALL, Style, id};

const WIDE: f32 = 700.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Key(u32),
    Activate(u8),
    Units(Option<u8>),
    Pick(u8, i32),
    Swap,
    Refresh,
}

pub struct ConvPage {
    vm: UnitConverterViewModel,
    /// Which field's unit list is open.
    picker: Option<u8>,
    search: TextEdit,
    fetching: bool,
    proxy: Option<EventLoopProxy<UserEvent>>,
}

fn msg(m: Msg) -> AppMsg {
    AppMsg::Conv(m)
}

/// Connectivity from the network portal: (available, metered).
fn network_status() -> Option<(bool, bool)> {
    let mut c = dbus::Connection::open(dbus::Bus::Session, Duration::from_millis(300)).ok()?;
    let mut get = |m: &str| {
        c.call(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.NetworkMonitor",
            m,
            &[],
        )
        .ok()?
        .first()?
        .as_bool()
    };
    Some((get("GetAvailable")?, get("GetMetered")?))
}

pub fn search_id() -> crate::ui::Id {
    id("unit-search")
}

impl ConvPage {
    pub fn new(saved: Option<serde_json::Value>) -> ConvPage {
        let mut vm = appcore::converter::view_model(DATA_DIR, saved);
        if let Some((available, metered)) = network_status() {
            vm.set_network_behavior(appcore::converter::network_behavior(available, metered));
        }
        ConvPage {
            vm,
            picker: None,
            search: TextEdit::new("", 60),
            fetching: false,
            proxy: None,
        }
    }

    pub fn save(&self) -> serde_json::Value {
        serde_json::to_value(self.vm.preferences()).unwrap_or_default()
    }

    pub fn activate(&mut self, mode: ConverterMode, proxy: &EventLoopProxy<UserEvent>) {
        self.proxy = Some(proxy.clone());
        self.picker = None;
        self.vm.set_current_mode(mode);
        if mode == ConverterMode::Currency && self.vm.start_automatic_currency_fetch() {
            self.fetch();
        }
    }

    fn fetch(&mut self) {
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        self.fetching = true;
        let _ = std::thread::Builder::new()
            .name("currency".into())
            .spawn(move || {
                let _ = proxy.send_event(UserEvent::Currency(Box::new(
                    unitconv::currency::fetch_latest(),
                )));
            });
    }

    pub fn currency_fetched(
        &mut self,
        result: Result<unitconv::CurrencySnapshot, unitconv::CurrencyError>,
    ) {
        self.fetching = false;
        self.vm.finish_currency_fetch(result);
    }

    pub fn update(&mut self, m: Msg, cx: &mut Cx) {
        match m {
            Msg::Key(k) => self.press(k),
            Msg::Activate(1) => self.vm.activate_value1(),
            Msg::Activate(_) => self.vm.activate_value2(),
            Msg::Units(which) => {
                self.picker = which;
                self.search.set_text("");
                *cx.focus = which.map(|_| search_id());
            }
            Msg::Pick(which, unit) => {
                if which == 1 {
                    self.vm.set_unit1(unit);
                } else {
                    self.vm.set_unit2(unit);
                }
                self.picker = None;
                *cx.focus = None;
            }
            Msg::Swap => self.vm.swap_units(),
            Msg::Refresh => {
                if self.vm.start_currency_refresh() {
                    self.fetch();
                }
            }
        }
    }

    fn press(&mut self, id: u32) {
        if (id == conv::NEGATE && !self.negate_enabled())
            || (id == conv::DECIMAL && !self.vm.is_decimal_enabled())
        {
            return;
        }
        if let Some(cmd) = keys::converter_command(id) {
            self.vm.button_pressed(cmd);
        }
    }

    fn negate_enabled(&self) -> bool {
        self.vm
            .current_category()
            .is_some_and(|c| c.negate_visible())
    }

    pub fn key(&mut self, kp: &KeyPress) -> bool {
        match input::converter_shortcut(kp) {
            Some(id) => {
                self.press(id);
                true
            }
            None => false,
        }
    }

    pub fn copy_text(&self) -> String {
        self.vm.copy_text().to_string()
    }

    pub fn paste(&mut self, text: &str) {
        self.vm.paste(text);
    }

    pub fn field(&mut self, fid: crate::ui::Id) -> Option<&mut TextEdit> {
        (self.picker.is_some() && fid == search_id()).then_some(&mut self.search)
    }

    pub fn field_changed(&mut self, _id: crate::ui::Id) {}

    pub fn close_popup(&mut self) -> bool {
        self.picker.take().is_some()
    }

    fn matches(&self) -> Vec<(i32, String, String)> {
        let q = self.search.text.trim().to_lowercase();
        self.vm
            .units()
            .iter()
            .filter(|u| {
                q.is_empty()
                    || u.name.to_lowercase().contains(&q)
                    || u.abbreviation.to_lowercase().contains(&q)
            })
            .map(|u| (u.id, u.name.clone(), u.abbreviation.clone()))
            .collect()
    }

    // ------------------------------------------------------------ view

    pub fn view(&mut self, f: &mut Frame, area: Rect) {
        let wide = area.w >= WIDE;
        let area = area.inset_xy(12.0, 6.0);
        let (values, pad) = if wide {
            let (p, v) = area.take_right(300.0);
            (v.take_left(v.w - 20.0).0, p)
        } else {
            // The keypad gives up height (down to a minimum) before the
            // values do; anything still left over scrolls.
            let needed = if self.vm.is_currency_current_category() {
                320.0
            } else {
                250.0
            };
            let pad_h = (area.h - needed).min(area.h * 0.5).clamp(168.0, 330.0);
            let (p, v) = area.take_bottom(pad_h);
            (v.take_top(v.h - 6.0).0, p)
        };
        self.values(f, values);
        self.keypad(f, pad);
    }

    fn values(&mut self, f: &mut Frame, r: Rect) {
        let t = f.t;
        let sid = id("conv-values");
        let off = f.scroll_begin(sid, r);
        let mut y = r.y - off;
        for which in [1u8, 2] {
            let (value, unit, active, sym, name) = if which == 1 {
                (
                    self.vm.value1(),
                    self.vm.unit1(),
                    self.vm.value1_active(),
                    self.vm.currency_symbol1(),
                    self.vm.value1_automation_name(),
                )
            } else {
                (
                    self.vm.value2(),
                    self.vm.unit2(),
                    self.vm.value2_active(),
                    self.vm.currency_symbol2(),
                    self.vm.value2_automation_name(),
                )
            };
            let field = Rect::new(r.x, y, r.w, 58.0);
            let fid = id(("conv-value", which));
            if active {
                f.cv.fill_rect(
                    Rect::new(field.x, field.bottom() - 3.0, 36.0, 3.0),
                    t.accent,
                );
            }
            let mut vr = field.inset_xy(2.0, 0.0);
            if self.vm.currency_symbol_visible() && !sym.is_empty() {
                let w = f.label(vr, sym, Style::new(26.0, 400.0), t.fg_dim, Align::Start);
                vr = vr.take_left(w + 8.0).1;
            }
            f.label_fit(
                vr,
                value,
                Style::new(38.0, if active { 500.0 } else { 300.0 }).tabular(),
                14.0,
                t.fg,
                Align::Start,
            );
            f.hit(
                fid,
                field,
                crate::ui::Sense::Click,
                Some(msg(Msg::Activate(which))),
                true,
            );
            if let Some(n) = f.node(fid, accesskit::Role::Button, &name, field) {
                n.clickable = true;
                n.focusable = true;
                n.selected = Some(active);
            }
            y += 60.0;
            let unit_name = unit.map(|u| u.name.clone()).unwrap_or_default();
            let ur = Rect::new(
                r.x,
                y,
                (f.text.width(&unit_name, BODY) + 44.0).min(r.w),
                34.0,
            );
            f.button(
                id(("unit-btn", which)),
                ur,
                &format!("{unit_name} ▾"),
                BODY,
                msg(Msg::Units(Some(which))),
                self.vm.is_drop_down_enabled(),
                Some(self.picker == Some(which)),
                true,
            );
            if let Some(n) = f.nodes.as_mut().and_then(|v| v.last_mut()) {
                n.label = format!(
                    "{} unit, {unit_name}",
                    if which == 1 { "Input" } else { "Output" }
                );
            }
            y += 38.0;
            if which == 1 {
                let sw = Rect::new(r.x, y + 2.0, 36.0, 36.0);
                f.icon_button(
                    id("swap"),
                    sw,
                    appcore::icons::SWAP,
                    "Swap units",
                    msg(Msg::Swap),
                    true,
                    None,
                );
                y += 42.0;
            }
        }
        y += 6.0;
        if self.vm.is_currency_current_category() {
            f.label(
                Rect::new(r.x, y, r.w, 22.0),
                self.vm.currency_ratio_equality(),
                SMALL,
                t.fg,
                Align::Start,
            );
            y += 22.0;
            let ts = self.vm.currency_timestamp().to_string();
            let w = f.label(
                Rect::new(r.x, y, r.w, 26.0),
                &ts,
                SMALL,
                t.fg_dim,
                Align::Start,
            );
            let status = self.vm.currency_status();
            if self.fetching {
                f.label(
                    Rect::new(r.x + w + 10.0, y, 120.0, 26.0),
                    "Updating…",
                    SMALL,
                    t.fg_dim,
                    Align::Start,
                );
            } else if status.refresh_visible() || !self.vm.is_currency_fetch_in_flight() {
                let b = Rect::new(r.x + w + 6.0, y, 116.0, 26.0);
                f.button(
                    id("refresh"),
                    b,
                    "Update rates",
                    SMALL,
                    msg(Msg::Refresh),
                    true,
                    None,
                    false,
                );
            }
            y += 28.0;
            let text = status.text();
            if !text.is_empty() {
                f.label_fit(
                    Rect::new(r.x, y, r.w, 20.0),
                    &text,
                    CAPTION,
                    9.0,
                    t.fg_dim,
                    Align::Start,
                );
                y += 22.0;
            }
        }
        let results = self.vm.supplementary_results();
        if self.vm.supplementary_visible() && !results.is_empty() {
            f.label(
                Rect::new(r.x, y, r.w, 24.0),
                "About equal to",
                CAPTION,
                t.fg_dim,
                Align::Start,
            );
            y += 26.0;
            let mut x = r.x;
            for (i, s) in results.iter().enumerate() {
                let label = format!("{} {}", s.value, s.unit.abbreviation);
                let w = f.text.width(&label, SMALL) + 20.0;
                if x + w > r.right() {
                    x = r.x;
                    y += 32.0;
                }
                let chip = Rect::new(x, y, w, 28.0);
                f.cv.rounded(chip, 14.0, t.surface2);
                f.label(chip, &label, SMALL, t.fg, Align::Center);
                if let Some(n) = f.node(
                    id(("supp", i)),
                    accesskit::Role::Label,
                    &s.localized_automation_name(),
                    chip,
                ) {
                    n.focusable = false;
                }
                x += w + 6.0;
            }
        }
        f.scroll_end(sid, r, y + 32.0 + off - r.y);
    }

    fn keypad(&mut self, f: &mut Frame, r: Rect) {
        let layout = keys::converter();
        f.group(id("conv-pad"), accesskit::Role::Group, "Keypad", r);
        for (k, row, col, rs, cs) in layout {
            let cell = r.cell_span(
                5,
                3,
                row as usize,
                col as usize,
                rs as usize,
                cs as usize,
                4.0,
            );
            let enabled = match k.id {
                conv::NEGATE => self.negate_enabled(),
                conv::DECIMAL => self.vm.is_decimal_enabled(),
                _ => true,
            };
            let look = if k.kind == appcore::keys::KeyKind::Number {
                KeyLook::Number
            } else {
                KeyLook::Function
            };
            f.key(
                id(("conv-key", k.id)),
                cell,
                &k.label,
                k.icon,
                look,
                &k.accessible_name(),
                msg(Msg::Key(k.id)),
                enabled,
            );
        }
        f.end_group();
    }

    pub fn overlay(&mut self, f: &mut Frame, area: Rect) {
        let Some(which) = self.picker else { return };
        let t = f.t;
        f.scrim(msg(Msg::Units(None)), false);
        let anchor = f
            .hits
            .iter()
            .find(|h| h.id == id(("unit-btn", which)))
            .map(|h| h.rect)
            .unwrap_or(area);
        let w = area.w.min(340.0) - 16.0;
        let x = anchor.x.min(area.right() - w - 8.0).max(area.x + 8.0);
        let top = (anchor.bottom() + 4.0).min(area.bottom() - 200.0);
        let h = (area.bottom() - top - 8.0).min(420.0);
        let card = Rect::new(x, top, w, h);
        f.card(card, 12.0);
        let inner = card.inset(8.0);
        let (sr, list) = inner.take_top(38.0);
        f.text_field(
            search_id(),
            sr,
            &self.search,
            "Search units",
            false,
            "Search units",
        );
        let list = list.take_top(list.h).0.inset_xy(0.0, 4.0);
        let current = if which == 1 {
            self.vm.unit1()
        } else {
            self.vm.unit2()
        }
        .map(|u| u.id);
        let items = self.matches();
        let sid = id(("unit-scroll", which));
        let off = f.scroll_begin(sid, list);
        let row_h = 36.0;
        if let Some(pos) = items.iter().position(|i| Some(i.0) == current)
            && self.search.text.is_empty()
            && f.scrolls.get(&sid).is_none_or(|s| s.content == 0.0)
        {
            f.reveal(sid, pos as f32 * row_h, (pos as f32 + 1.0) * row_h);
        }
        let first = (off / row_h).floor().max(0.0) as usize;
        let last = ((off + list.h) / row_h).ceil() as usize + 1;
        for (i, (uid, name, abbr)) in items.iter().enumerate().take(last).skip(first) {
            let row = Rect::new(list.x, list.y - off + i as f32 * row_h, list.w, row_h - 2.0);
            let sel = Some(*uid) == current;
            f.row(
                id(("unit", which, *uid)),
                row,
                msg(Msg::Pick(which, *uid)),
                sel,
                name,
            );
            let (a, n) = row.inset_xy(10.0, 0.0).take_right(70.0);
            f.label_fit(
                n,
                name,
                BODY,
                10.0,
                if sel { t.accent_text } else { t.fg },
                Align::Start,
            );
            f.label_fit(a, abbr, SMALL, 9.0, t.fg_dim, Align::End);
        }
        if items.is_empty() {
            f.label(
                list.take_top(40.0).0,
                "No matching units",
                SMALL,
                t.fg_dim,
                Align::Center,
            );
        }
        f.scroll_end(sid, list, items.len() as f32 * row_h);
    }

    /// Enter in the search field picks the first match.
    pub fn activate_search(&mut self) {
        if let (Some(which), Some(first)) = (self.picker, self.matches().first().map(|m| m.0)) {
            if which == 1 {
                self.vm.set_unit1(first);
            } else {
                self.vm.set_unit2(first);
            }
            self.picker = None;
        }
    }
}
