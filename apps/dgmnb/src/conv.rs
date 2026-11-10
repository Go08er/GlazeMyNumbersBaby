//! Unit and currency converters (upstream UnitConverter.xaml).

use std::time::Duration;

use appcore::KeyPress;
use appcore::dbus;
use appcore::input;
use appcore::keys::{self, conv};
use appcore::metrics::{controls as ctl, converter as mc, shape, text as txt};
use unitconv::{ConverterMode, UnitConverterViewModel};
use winit::event_loop::EventLoopProxy;

use crate::app::{Cx, DATA_DIR, Msg as AppMsg, UserEvent};
use crate::edit::TextEdit;
use crate::gfx::Rect;
use crate::ui::{Align, BODY, CAPTION, Frame, KeyLook, SMALL, Style, id, style};

const WIDE: f32 = mc::WIDE;

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
    /// What the second value said when the page was last drawn: it is told
    /// when it says something else ([`ConvPage::view`]).
    told: Option<String>,
    /// A network-status query is under way.
    checking: bool,
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
        ConvPage {
            vm: appcore::converter::view_model(DATA_DIR, saved),
            picker: None,
            search: TextEdit::new("", 60),
            fetching: false,
            told: None,
            checking: false,
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
        if mode == ConverterMode::Currency {
            self.check_network();
        }
    }

    /// Ask the network portal (off the UI thread) whether we're online and
    /// metered; the answer comes back as `UserEvent::Network`, and decides
    /// whether rates may be fetched without asking. Done each time Currency
    /// opens, so the policy follows connection changes.
    fn check_network(&mut self) {
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        if self.checking {
            return;
        }
        self.checking = true;
        let spawned = std::thread::Builder::new()
            .name("network".into())
            .spawn(move || {
                let _ = proxy.send_event(UserEvent::Network(network_status()));
            });
        if spawned.is_err() {
            self.network(None, true);
        }
    }

    /// The portal's answer (`None`: no portal), then the automatic fetch if
    /// Currency is still `showing` and the policy allows it. (The view model
    /// keeps its Currency mode while another page is open, so the app says.)
    pub fn network(&mut self, status: Option<(bool, bool)>, showing: bool) {
        self.checking = false;
        if let Some((available, metered)) = status {
            self.vm
                .set_network_behavior(appcore::converter::network_behavior(available, metered));
        }
        if showing
            && self.vm.current_mode() == Some(ConverterMode::Currency)
            && self.vm.start_automatic_currency_fetch()
        {
            self.fetch();
        }
    }

    fn fetch(&mut self) {
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        self.fetching = true;
        let spawned = std::thread::Builder::new()
            .name("currency".into())
            .spawn(move || {
                let _ = proxy.send_event(UserEvent::Currency(Box::new(
                    unitconv::currency::fetch_latest(),
                )));
            });
        if let Err(e) = spawned {
            self.currency_fetched(Err(unitconv::CurrencyError::Http(e.to_string())));
        }
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
        appcore::converter::paste(&mut self.vm, text);
    }

    pub fn field(&mut self, fid: crate::ui::Id) -> Option<&mut TextEdit> {
        (self.picker.is_some() && fid == search_id()).then_some(&mut self.search)
    }

    pub fn field_changed(&mut self, _id: crate::ui::Id) {}

    /// Whether a unit picker is open.
    pub fn picker_open(&self) -> bool {
        self.picker.is_some()
    }

    pub fn close_popup(&mut self) -> bool {
        self.picker.take().is_some()
    }

    /// The units the search leaves: id, name shown, name spoken (as
    /// upstream's Unit.AccessibleName: a currency without the dash),
    /// abbreviation.
    fn matches(&self) -> Vec<(i32, String, String, String)> {
        let q = self.search.text.trim().to_lowercase();
        self.vm
            .units()
            .iter()
            .filter(|u| {
                q.is_empty()
                    || u.name.to_lowercase().contains(&q)
                    || u.abbreviation.to_lowercase().contains(&q)
            })
            .map(|u| {
                (
                    u.id,
                    u.name.clone(),
                    u.accessible_name.clone(),
                    u.abbreviation.clone(),
                )
            })
            .collect()
    }

    // ------------------------------------------------------------ view

    pub fn view(&mut self, f: &mut Frame, area: Rect) {
        let wide = area.w >= WIDE;
        let area = area.inset_xy(mc::MARGIN_X, mc::MARGIN_Y);
        let (values, pad) = if wide {
            let (p, v) = area.take_right(mc::KEYPAD_W);
            (v.take_left(v.w - mc::WIDE_GAP).0, p)
        } else {
            // The keypad gives up height (down to a minimum) before the
            // values do; anything still left over scrolls.
            let needed = if self.vm.is_currency_current_category() {
                mc::NEEDED_CURRENCY
            } else {
                mc::NEEDED
            };
            let pad_h = (area.h - needed)
                .min(area.h * mc::PAD_SHARE)
                .clamp(mc::PAD_MIN_H, mc::PAD_MAX_H);
            let (p, v) = area.take_bottom(pad_h);
            (v.take_top(v.h - mc::NARROW_GAP).0, p)
        };
        self.values(f, values);
        self.keypad(f, pad);
    }

    fn values(&mut self, f: &mut Frame, r: Rect) {
        let t = f.t;
        let sid = id("conv-values");
        // The second value is told when what it says changes, as
        // upstream's is a polite live region (Value2,
        // UnitConverter.xaml:595-599; the first is not): its node is a
        // live one in the frame that shows another text than the frame
        // before, whichever of the two values is typed into. Not in the
        // first frame of the page (see DatePage::view).
        let name2 = self.vm.value2_automation_name();
        let changed = self.told.as_deref().is_some_and(|was| was != name2);
        self.told = Some(name2);
        let off = f.scroll_begin(sid, r, "Conversion");
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
            let field = Rect::new(r.x, y, r.w, mc::FIELD_H);
            let fid = id(("conv-value", which));
            if active {
                f.cv.fill_rect(
                    Rect::new(
                        field.x,
                        field.bottom() - mc::MARKER_H,
                        mc::MARKER_W,
                        mc::MARKER_H,
                    ),
                    t.accent,
                );
            }
            let mut vr = field.inset_xy(mc::FIELD_PAD_X, 0.0);
            if self.vm.currency_symbol_visible() && !sym.is_empty() {
                let w = f.label(
                    vr,
                    sym,
                    style(txt::CONVERTER_SYMBOL),
                    t.fg_dim,
                    Align::Start,
                );
                vr = vr.take_left(w + mc::SYMBOL_GAP).1;
            }
            f.label_fit(
                vr,
                value,
                Style::new(
                    txt::CONVERTER_VALUE.size,
                    if active {
                        txt::CONVERTER_VALUE_CHECKED_WEIGHT
                    } else {
                        txt::CONVERTER_VALUE.weight
                    },
                )
                .tabular(),
                txt::CONVERTER_VALUE_MIN_SIZE,
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
                n.live = which == 2 && changed;
            }
            y += mc::FIELD_STEP;
            let unit_name = unit.map(|u| u.name.clone()).unwrap_or_default();
            let ur = Rect::new(
                r.x,
                y,
                (f.text.width(&unit_name, BODY) + mc::UNIT_ARROW).min(r.w),
                mc::UNIT_H,
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
            // Named after the unit it shows, as GMNB's drop-down (and
            // upstream's ComboBox): which field it is, its description.
            let spoken = unit.map(|u| u.accessible_name.clone()).unwrap_or_default();
            if let Some(n) = f.nodes.as_mut().and_then(|v| v.last_mut()) {
                n.label = spoken;
                n.description = Some((
                    if which == 1 {
                        "Input unit"
                    } else {
                        "Output unit"
                    }
                    .into(),
                    None,
                ));
            }
            y += mc::UNIT_STEP;
            if which == 1 {
                let sw = Rect::new(r.x, y + mc::SWAP_TOP, ctl::ICON_BUTTON, ctl::ICON_BUTTON);
                f.icon_button(
                    id("swap"),
                    sw,
                    appcore::icons::SWAP,
                    "Swap units",
                    msg(Msg::Swap),
                    true,
                    None,
                );
                y += mc::SWAP_STEP;
            }
        }
        y += mc::SECTION_GAP;
        if self.vm.is_currency_current_category() {
            // Each line is also a label for assistive technology (as the
            // original's text blocks are), the changing ones live.
            let spoken = |f: &mut Frame, key: &str, text: &str, r: Rect, live: bool| {
                if !text.is_empty()
                    && let Some(n) =
                        f.node(id(("currency-line", key)), accesskit::Role::Label, text, r)
                {
                    n.live = live;
                }
            };
            let line = Rect::new(r.x, y, r.w, mc::RATIO_H);
            f.label(
                line,
                self.vm.currency_ratio_equality(),
                SMALL,
                t.fg,
                Align::Start,
            );
            spoken(
                f,
                "ratio",
                self.vm.currency_ratio_equality_automation_name(),
                line,
                false,
            );
            y += mc::RATIO_H;
            let ts = self.vm.currency_timestamp().to_string();
            let line = Rect::new(r.x, y, r.w, mc::TIMESTAMP_H);
            let w = f.label(line, &ts, SMALL, t.fg_dim, Align::Start);
            spoken(f, "updated", &ts, line, true);
            let status = self.vm.currency_status();
            if self.fetching {
                let line = Rect::new(
                    r.x + w + mc::UPDATING_GAP,
                    y,
                    mc::UPDATING_W,
                    mc::TIMESTAMP_H,
                );
                f.label(line, "Updating…", SMALL, t.fg_dim, Align::Start);
                spoken(f, "updating", "Updating rates", line, true);
            } else if status.refresh_visible() || !self.vm.is_currency_fetch_in_flight() {
                let b = Rect::new(r.x + w + mc::REFRESH_GAP, y, mc::REFRESH_W, mc::TIMESTAMP_H);
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
            y += mc::TIMESTAMP_STEP;
            let text = status.text();
            if !text.is_empty() {
                let line = Rect::new(r.x, y, r.w, mc::STATUS_H);
                f.label_fit(
                    line,
                    &text,
                    CAPTION,
                    txt::STATUS_MIN_SIZE,
                    t.fg_dim,
                    Align::Start,
                );
                spoken(f, "status", &text, line, true);
                y += mc::STATUS_STEP;
            }
        }
        let results = self.vm.supplementary_results();
        if self.vm.supplementary_visible() && !results.is_empty() {
            f.label(
                Rect::new(r.x, y, r.w, mc::CAPTION_H),
                "About equal to",
                CAPTION,
                t.fg_dim,
                Align::Start,
            );
            y += mc::CAPTION_STEP;
            let mut x = r.x;
            for (i, s) in results.iter().enumerate() {
                let label = format!("{} {}", s.value, s.unit.abbreviation);
                let w = f.text.width(&label, SMALL) + mc::CHIP_PAD_X * 2.0;
                if x + w > r.right() {
                    x = r.x;
                    y += mc::CHIP_ROW_STEP;
                }
                let chip = Rect::new(x, y, w, mc::CHIP_H);
                f.cv.rounded(chip, shape::CHIP_RADIUS, t.surface2);
                f.label(chip, &label, SMALL, t.fg, Align::Center);
                if let Some(n) = f.node(
                    id(("supp", i)),
                    accesskit::Role::Label,
                    &s.localized_automation_name(),
                    chip,
                ) {
                    n.focusable = false;
                }
                x += w + mc::CHIP_GAP;
            }
        }
        f.scroll_end(sid, r, y + mc::BOTTOM_PAD + off - r.y);
    }

    fn keypad(&mut self, f: &mut Frame, r: Rect) {
        let layout = keys::converter();
        f.group(id("conv-pad"), accesskit::Role::Group, "Keypad", r);
        for (k, row, col, rs, cs) in layout {
            let cell = r.cell_span(
                mc::KEY_ROWS,
                mc::KEY_COLS,
                row as usize,
                col as usize,
                rs as usize,
                cs as usize,
                mc::KEY_GAP,
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
            let kid = id(("conv-key", k.id));
            f.key(
                kid,
                cell,
                &k.label,
                k.icon,
                look,
                &k.accessible_name(),
                msg(Msg::Key(k.id)),
                enabled,
            );
            // Upstream's converter keys are CalculatorButtons, which
            // ignore Enter (the converter has none of its own).
            f.calculator_key(kid);
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
        let w = area.w.min(mc::LIST_MAX_W) - mc::LIST_EDGE * 2.0;
        let x = anchor
            .x
            .min(area.right() - w - mc::LIST_EDGE)
            .max(area.x + mc::LIST_EDGE);
        let top = (anchor.bottom() + mc::LIST_GAP).min(area.bottom() - mc::LIST_MIN_BELOW);
        let h = (area.bottom() - top - mc::LIST_EDGE).min(mc::LIST_MAX_H);
        let card = Rect::new(x, top, w, h);
        f.card(card, shape::POPUP_RADIUS);
        let inner = card.inset(mc::LIST_PAD);
        let (sr, list) = inner.take_top(mc::SEARCH_H);
        f.text_field(
            search_id(),
            sr,
            &self.search,
            "Search units",
            false,
            "Search units",
        );
        let list = list.take_top(list.h).0.inset_xy(0.0, mc::LIST_INNER_PAD_Y);
        let current = if which == 1 {
            self.vm.unit1()
        } else {
            self.vm.unit2()
        }
        .map(|u| u.id);
        let items = self.matches();
        let sid = id(("unit-scroll", which));
        let off = f.scroll_begin(sid, list, "Units");
        let row_h = mc::UNIT_ROW_STEP;
        if let Some(pos) = items.iter().position(|i| Some(i.0) == current)
            && self.search.text.is_empty()
            && f.scrolls.get(&sid).is_none_or(|s| s.content == 0.0)
        {
            f.reveal(sid, pos as f32 * row_h, (pos as f32 + 1.0) * row_h);
        }
        let first = (off / row_h).floor().max(0.0) as usize;
        let last = ((off + list.h) / row_h).ceil() as usize + 1;
        for (i, (uid, name, spoken, abbr)) in items.iter().enumerate().take(last).skip(first) {
            let row = Rect::new(
                list.x,
                list.y - off + i as f32 * row_h,
                list.w,
                row_h - mc::UNIT_ROW_GAP,
            );
            let sel = Some(*uid) == current;
            f.row(
                id(("unit", which, *uid)),
                row,
                msg(Msg::Pick(which, *uid)),
                sel,
                spoken,
            );
            let (a, n) = row
                .inset_xy(mc::UNIT_ROW_PAD_X, 0.0)
                .take_right(mc::UNIT_ABBR_W);
            f.label_fit(
                n,
                name,
                BODY,
                txt::UNIT_ROW_MIN_SIZE,
                if sel { t.accent_text } else { t.fg },
                Align::Start,
            );
            f.label_fit(
                a,
                abbr,
                SMALL,
                txt::UNIT_ABBR_MIN_SIZE,
                t.fg_dim,
                Align::End,
            );
        }
        if items.is_empty() {
            f.label(
                list.take_top(mc::EMPTY_H).0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{Frame, Icons, Input};

    /// Pre-review: the currency rate line, its age and status reach screen
    /// readers (they were drawn only). Uses the bundled rates, not a cache.
    #[test]
    fn currency_lines_are_exported() {
        let mut page = ConvPage {
            vm: UnitConverterViewModel::new(unitconv::ViewModelConfig::default()),
            picker: None,
            search: TextEdit::new("", 60),
            fetching: true,
            told: None,
            checking: false,
            proxy: None,
        };
        page.vm.set_current_mode(ConverterMode::Currency);
        let mut pm = tiny_skia::Pixmap::new(760, 700).unwrap();
        let (mut text, mut icons, input) =
            (crate::text::Text::new(), Icons::default(), Input::default());
        let mut scrolls = std::collections::HashMap::new();
        let mut f = Frame::new(
            crate::gfx::Canvas::new(pm.as_mut(), 1.0, false),
            &mut text,
            &mut icons,
            crate::theme::Theme::new(false, None),
            &input,
            &mut scrolls,
            true,
        );
        page.view(&mut f, Rect::new(0.0, 46.0, 760.0, 654.0));
        let labels: Vec<String> = crate::a11y::exported(f.nodes.as_ref().unwrap())
            .into_iter()
            .filter(|(r, ..)| *r == accesskit::Role::Label)
            .map(|(_, n, _)| n)
            .collect();
        assert!(
            labels.iter().any(|l| l.contains("United States Dollar")),
            "{labels:?}"
        );
        assert!(
            labels.iter().any(|l| l.starts_with("Updated")),
            "{labels:?}"
        );
        assert!(labels.iter().any(|l| l == "Updating rates"), "{labels:?}");
    }
}
