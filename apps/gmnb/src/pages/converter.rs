//! Unit / currency converter (upstream UnitConverter.xaml).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use unitconv::{ConverterMode, NetworkAccessBehavior, UnitConverterViewModel};

use super::{Ctx, Page};
use crate::widgets::display::{Change, Display};
use crate::widgets::icon::{PathIcon, paths};
use crate::widgets::keypad::Keypad;
use crate::widgets::width_bin::WidthBin;
use appcore::KeyPress;
use appcore::input;
use appcore::keys::{self, conv};
use appcore::modes::ViewMode;

const WIDE_PX: i32 = 700;

struct Field {
    frame: gtk::Box,
    display: Display,
    dropdown: gtk::DropDown,
    symbol: gtk::Label,
}

pub struct ConverterPage {
    ctx: Rc<Ctx>,
    vm: RefCell<UnitConverterViewModel>,
    root: WidthBin,
    layout: gtk::Box,
    values: gtk::Box,
    f1: Field,
    f2: Field,
    supp: gtk::FlowBox,
    supp_header: gtk::Label,
    currency_box: gtk::Box,
    ratio: gtk::Label,
    timestamp: gtk::Label,
    status: gtk::Label,
    refresh: gtk::Button,
    spinner: adw::Spinner,
    keypad: Keypad,
    syncing: Cell<bool>,
    unit_ids: RefCell<Vec<i32>>,
    wide: Cell<bool>,
}

fn field(label: &str) -> Field {
    let display = Display::new(44.0);
    display.set_show_expression(false);
    display.set_align_end(false);
    display.set_weight(300);
    let symbol = gtk::Label::new(None);
    symbol.add_css_class("wc-currency-symbol");
    symbol.set_valign(gtk::Align::Center);
    symbol.set_visible(false);
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    top.append(&symbol);
    top.append(&display);
    let strings = gtk::StringList::new(&[]);
    let dropdown = gtk::DropDown::builder()
        .model(&strings)
        .enable_search(true)
        .expression(gtk::PropertyExpression::new(
            gtk::StringObject::static_type(),
            None::<&gtk::Expression>,
            "string",
        ))
        .halign(gtk::Align::Start)
        .css_classes(["wc-unit-dropdown"])
        .build();
    dropdown.set_search_match_mode(gtk::StringFilterMatchMode::Substring);
    dropdown.update_property(&[gtk::accessible::Property::Label(label)]);
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 2);
    frame.add_css_class("wc-conv-field");
    frame.append(&top);
    frame.append(&dropdown);
    Field {
        frame,
        display,
        dropdown,
        symbol,
    }
}

/// Map GIO's view of connectivity onto the loader's behaviour enum.
fn network_behavior(m: &gio::NetworkMonitor) -> NetworkAccessBehavior {
    appcore::converter::network_behavior(m.is_network_available(), m.is_network_metered())
}

impl ConverterPage {
    pub fn new(ctx: Rc<Ctx>) -> Rc<Self> {
        let vm = appcore::converter::view_model(crate::DATA_DIR, ctx.store.page_state("converter"));

        let f1 = field("Input unit");
        let f2 = field("Output unit");
        let swap = gtk::Button::builder()
            .child(&PathIcon::new(paths::SWAP, 18))
            .tooltip_text("Swap units")
            .css_classes(["wc-swap"])
            .halign(gtk::Align::Start)
            .build();

        let supp_header = gtk::Label::new(Some("About equal to"));
        supp_header.add_css_class("wc-section");
        supp_header.set_xalign(0.0);
        let supp = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .max_children_per_line(4)
            .row_spacing(6)
            .column_spacing(6)
            .css_classes(["wc-supp"])
            .build();

        let ratio = gtk::Label::new(None);
        ratio.add_css_class("wc-ratio");
        ratio.set_xalign(0.0);
        let timestamp = gtk::Label::new(None);
        timestamp.add_css_class("wc-timestamp");
        timestamp.set_xalign(0.0);
        let status = gtk::Label::new(None);
        status.add_css_class("wc-timestamp");
        status.set_xalign(0.0);
        status.set_wrap(true);
        let refresh = gtk::Button::builder()
            .label("Update rates")
            .css_classes(["flat", "wc-link"])
            .halign(gtk::Align::Start)
            .build();
        let spinner = adw::Spinner::new();
        spinner.set_visible(false);
        let ts_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        ts_row.append(&timestamp);
        ts_row.append(&refresh);
        ts_row.append(&spinner);
        let currency_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        currency_box.append(&ratio);
        currency_box.append(&ts_row);
        currency_box.append(&status);

        let values = gtk::Box::new(gtk::Orientation::Vertical, 6);
        values.set_vexpand(true);
        values.set_valign(gtk::Align::Start);
        values.append(&f1.frame);
        values.append(&swap);
        values.append(&f2.frame);
        values.append(&currency_box);
        values.append(&supp_header);
        values.append(&supp);

        let keypad = Keypad::new();
        for (key, r, c, rs, cs) in keys::converter() {
            keypad.add(key, r, c, rs, cs);
        }
        keypad.set_vexpand(true);
        keypad.set_size_request(-1, 280);

        let layout = gtk::Box::new(gtk::Orientation::Vertical, 10);
        layout.set_margin_start(12);
        layout.set_margin_end(12);
        layout.set_margin_bottom(10);
        layout.append(&values);
        layout.append(&keypad);
        let root = WidthBin::new(&layout);

        let page = Rc::new(ConverterPage {
            ctx: ctx.clone(),
            vm: RefCell::new(vm),
            root: root.clone(),
            layout,
            values,
            f1,
            f2,
            supp,
            supp_header,
            currency_box,
            ratio,
            timestamp,
            status,
            refresh: refresh.clone(),
            spinner,
            keypad: keypad.clone(),
            syncing: Cell::new(false),
            unit_ids: RefCell::default(),
            wide: Cell::new(false),
        });

        {
            let weak = Rc::downgrade(&page);
            ctx.hub.subscribe(move |s| {
                if let Some(p) = weak.upgrade() {
                    p.f1.display.set_scheme(*s);
                    p.f2.display.set_scheme(*s);
                    p.keypad.set_scheme(*s);
                }
            });
        }
        let weak = Rc::downgrade(&page);
        keypad.connect_pressed(move |id, x, y| {
            if let Some(p) = weak.upgrade() {
                let s = p.ctx.hub.scheme();
                p.ctx
                    .pulse_at(&p.keypad, x, y, s.blobs[(id as usize * 5) % 4], 0.55);
                p.press(id);
            }
        });
        for (which, f) in [(1, &page.f1), (2, &page.f2)] {
            let click = gtk::GestureClick::new();
            let weak = Rc::downgrade(&page);
            click.connect_released(move |_, _, _, _| {
                if let Some(p) = weak.upgrade() {
                    {
                        let mut vm = p.vm.borrow_mut();
                        if which == 1 {
                            vm.activate_value1();
                        } else {
                            vm.activate_value2();
                        }
                    }
                    p.sync(Change::None, Change::None);
                }
            });
            f.display.add_controller(click);
            let weak = Rc::downgrade(&page);
            f.dropdown.connect_selected_notify(move |d| {
                let Some(p) = weak.upgrade() else { return };
                if p.syncing.get() {
                    return;
                }
                let Some(&id) = p.unit_ids.borrow().get(d.selected() as usize) else {
                    return;
                };
                {
                    let mut vm = p.vm.borrow_mut();
                    if which == 1 {
                        vm.set_unit1(id);
                    } else {
                        vm.set_unit2(id);
                    }
                }
                p.sync(Change::Replace, Change::Result);
            });
        }
        {
            let weak = Rc::downgrade(&page);
            swap.connect_clicked(move |b| {
                if let Some(p) = weak.upgrade() {
                    p.vm.borrow_mut().swap_units();
                    let s = p.ctx.hub.scheme();
                    p.ctx.pulse_at(
                        b,
                        b.width() as f32 / 2.0,
                        b.height() as f32 / 2.0,
                        s.hot_a,
                        1.0,
                    );
                    p.sync(Change::Result, Change::Result);
                }
            });
        }
        {
            let weak = Rc::downgrade(&page);
            refresh.connect_clicked(move |_| {
                if let Some(p) = weak.upgrade() {
                    let started = p.vm.borrow_mut().start_currency_refresh();
                    if started {
                        p.fetch_currency();
                    }
                }
            });
        }
        {
            // Upstream registers for network-behaviour changes; do the same so
            // a failed fetch is retried when connectivity returns, and metered
            // connections only fetch when asked.
            let monitor = gio::NetworkMonitor::default();
            page.vm
                .borrow_mut()
                .set_network_behavior(network_behavior(&monitor));
            let weak = Rc::downgrade(&page);
            monitor.connect_network_changed(move |m, _| {
                let Some(p) = weak.upgrade() else { return };
                let behavior = network_behavior(m);
                p.vm.borrow_mut().set_network_behavior(behavior);
                let in_currency = p.vm.borrow().is_currency_current_category();
                if behavior == NetworkAccessBehavior::Normal && in_currency {
                    let started = p.vm.borrow_mut().start_automatic_currency_fetch();
                    if started {
                        p.fetch_currency();
                    }
                }
                p.sync(Change::None, Change::None);
            });
        }
        {
            let weak = Rc::downgrade(&page);
            root.connect_width(move |w| {
                if let Some(p) = weak.upgrade() {
                    p.set_wide(w >= WIDE_PX);
                }
            });
        }
        page
    }

    fn set_wide(&self, wide: bool) {
        if wide == self.wide.get() {
            return;
        }
        self.wide.set(wide);
        self.layout.set_orientation(if wide {
            gtk::Orientation::Horizontal
        } else {
            gtk::Orientation::Vertical
        });
        self.layout.set_spacing(if wide { 24 } else { 10 });
        self.values.set_hexpand(wide);
        self.keypad.set_hexpand(wide);
        self.keypad
            .set_size_request(if wide { 300 } else { -1 }, 280);
    }

    fn press(self: &Rc<Self>, id: u32) {
        let Some(cmd) = keys::converter_command(id) else {
            return;
        };
        self.vm.borrow_mut().button_pressed(cmd);
        self.sync(Change::Typing, Change::Replace);
    }

    fn fetch_currency(self: &Rc<Self>) {
        self.spinner.set_visible(true);
        self.refresh.set_sensitive(false);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(unitconv::currency::fetch_latest)
                .await
                .unwrap_or_else(|_| {
                    Err(unitconv::CurrencyError::Http(
                        "fetch thread panicked".into(),
                    ))
                });
            if let Some(p) = weak.upgrade() {
                p.vm.borrow_mut().finish_currency_fetch(result);
                p.spinner.set_visible(false);
                p.refresh.set_sensitive(true);
                p.rebuild_units();
                p.sync(Change::Replace, Change::Result);
            }
        });
    }

    fn rebuild_units(&self) {
        let vm = self.vm.borrow();
        let units = vm.units();
        let names: Vec<String> = units.iter().map(|u| u.name.clone()).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        self.unit_ids.replace(units.iter().map(|u| u.id).collect());
        self.syncing.set(true);
        for f in [&self.f1, &self.f2] {
            f.dropdown.set_model(Some(&gtk::StringList::new(&refs)));
        }
        self.syncing.set(false);
    }

    /// Push VM state into widgets. `active`/`other` pick the animation for
    /// the field being edited and the computed one.
    fn sync(&self, active: Change, other: Change) {
        let vm = self.vm.borrow();
        self.syncing.set(true);
        let ids = self.unit_ids.borrow();
        for (f, unit, value, is_active, sym) in [
            (
                &self.f1,
                vm.unit1(),
                vm.value1(),
                vm.value1_active(),
                vm.currency_symbol1(),
            ),
            (
                &self.f2,
                vm.unit2(),
                vm.value2(),
                vm.value2_active(),
                vm.currency_symbol2(),
            ),
        ] {
            f.display
                .set_value(value, if is_active { active } else { other }, false);
            if is_active {
                f.frame.add_css_class("wc-active");
            } else {
                f.frame.remove_css_class("wc-active");
            }
            if let Some(u) = unit {
                if let Some(pos) = ids.iter().position(|&id| id == u.id)
                    && f.dropdown.selected() != pos as u32
                {
                    f.dropdown.set_selected(pos as u32);
                }
                f.display
                    .update_property(&[gtk::accessible::Property::Label(&format!(
                        "{value} {}",
                        u.accessible_name
                    ))]);
            }
            f.symbol
                .set_visible(vm.currency_symbol_visible() && !sym.is_empty());
            f.symbol.set_text(sym);
        }
        drop(ids);
        self.syncing.set(false);

        let currency = vm.is_currency_current_category();
        self.currency_box.set_visible(currency);
        if currency {
            self.ratio.set_text(vm.currency_ratio_equality());
            self.timestamp.set_text(vm.currency_timestamp());
            let status = vm.currency_status();
            let text = status.text();
            self.status.set_visible(!text.is_empty());
            self.status.set_text(&text);
            self.refresh
                .set_visible(status.refresh_visible() || !vm.is_currency_fetch_in_flight());
        }

        while let Some(c) = self.supp.first_child() {
            self.supp.remove(&c);
        }
        let results = vm.supplementary_results();
        let show = vm.supplementary_visible() && !results.is_empty();
        self.supp_header.set_visible(show);
        self.supp.set_visible(show);
        for r in results {
            let chip = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            chip.add_css_class("wc-chip");
            if r.is_whimsical() {
                chip.add_css_class("wc-chip-whimsy");
                chip.append(&PathIcon::new(paths::SPARKLE, 14));
            }
            let l = gtk::Label::new(Some(&format!("{} {}", r.value, r.unit.abbreviation)));
            l.set_tooltip_text(Some(&r.localized_automation_name()));
            chip.append(&l);
            chip.update_property(&[gtk::accessible::Property::Label(
                &r.localized_automation_name(),
            )]);
            self.supp.append(&chip);
        }

        let negate = vm.current_category().is_some_and(|c| c.negate_visible());
        self.keypad.set_key_sensitive(conv::NEGATE, negate);
        self.keypad
            .set_key_sensitive(conv::DECIMAL, vm.is_decimal_enabled());
    }
}

pub struct ConverterHandle(pub Rc<ConverterPage>);

impl Page for ConverterHandle {
    fn widget(&self) -> gtk::Widget {
        self.0.root.clone().upcast()
    }

    fn activate(&self, mode: ViewMode) {
        let p = &self.0;
        let Some(cm) = mode.converter_mode() else {
            return;
        };
        p.vm.borrow_mut().set_current_mode(cm);
        p.rebuild_units();
        p.sync(Change::Replace, Change::Replace);
        p.keypad.cascade();
        if cm == ConverterMode::Currency {
            let started = p.vm.borrow_mut().start_automatic_currency_fetch();
            if started {
                p.fetch_currency();
            }
        }
    }

    fn key_pressed(&self, kp: &KeyPress) -> bool {
        let Some(id) = input::converter_shortcut(kp) else {
            return false;
        };
        let p = &self.0;
        if let Some(b) = p.keypad.button(id)
            && !b.is_sensitive()
        {
            return true;
        }
        if let Some((x, y)) = p.keypad.flash(id) {
            let s = p.ctx.hub.scheme();
            p.ctx.pulse_at(&p.keypad, x, y, s.accent, 0.5);
        }
        p.press(id);
        true
    }

    fn copy(&self) -> Option<String> {
        let text = self.0.vm.borrow().copy_text().to_string();
        self.0.ctx.copy_to_clipboard(&text);
        Some(text)
    }

    fn paste(&self, text: &str) {
        self.0.vm.borrow_mut().paste(text);
        self.0.sync(Change::Replace, Change::Result);
    }

    fn save(&self) {
        if let Ok(v) = serde_json::to_value(self.0.vm.borrow().preferences()) {
            self.0.ctx.store.set_page_state("converter", v);
        }
    }
}

impl ConverterPage {
    pub fn handle(ctx: Rc<Ctx>) -> Rc<dyn Page> {
        Rc::new(ConverterHandle(Self::new(ctx)))
    }
}
