//! Unit / currency converter (upstream UnitConverter.xaml).
//!
//! Laid out as DGMNB's page (`apps/dgmnb/src/conv.rs`): the values from
//! the top left, each part at the height and the distance
//! `appcore::metrics::converter` gives it; the keypad on the right in a
//! wide window, below in a narrow one.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use appcore::metrics::{controls as ctl, converter as mc, text as txt};
use gmnb_theme::Style;
use gmnb_theme::types::{Font, Spacing, Value};
use gtk::{gio, glib};
use unitconv::{ConverterMode, NetworkAccessBehavior, UnitConverterViewModel};

use super::{Ctx, Page};
use crate::widgets::fit::{FitLabel, Prefix, SizeBin, pango_font};
use crate::widgets::icon::{PathIcon, paths};
use crate::widgets::keypad::Keypad;
use appcore::KeyPress;
use appcore::input;
use appcore::keys::{self, conv};
use appcore::modes::ViewMode;

struct Field {
    /// The value's 58 px strip (`converter.field`; `checked` while it is
    /// the one typed into), with the marker under its value.
    frame: gtk::Box,
    /// The value, after the currency's symbol (its prefix) when there
    /// is one.
    value: FitLabel,
    dropdown: gtk::DropDown,
}

pub struct ConverterPage {
    ctx: Rc<Ctx>,
    vm: RefCell<UnitConverterViewModel>,
    root: SizeBin,
    layout: gtk::Box,
    f1: Field,
    f2: Field,
    supp: gtk::FlowBox,
    supp_header: gtk::Label,
    /// The chips' sparkle (`converter.whimsy_icon`).
    whimsy_icon: Cell<bool>,
    currency_box: gtk::Box,
    ratio: gtk::Label,
    timestamp: gtk::Label,
    status: gtk::Label,
    refresh: gtk::Button,
    spinner: adw::Spinner,
    keypad: Keypad,
    /// The style the page was last laid out for.
    style: RefCell<Option<Rc<Style>>>,
    syncing: Cell<bool>,
    /// What the second value said when it was last told ([`Self::sync`]).
    told: RefCell<Option<String>>,
    unit_ids: RefCell<Vec<i32>>,
    /// Each unit's spoken name by its shown one ("United States Dollar"
    /// for "United States - Dollar"), for the drop-downs' lists.
    spoken: Rc<RefCell<HashMap<String, String>>>,
    wide: Cell<bool>,
    /// The converter is the page on screen (between activate and
    /// deactivate).
    showing: Cell<bool>,
}

fn field(label: &str) -> Field {
    // A control too: picked (pointer or assistive technology), it is the
    // field typed into.
    let value = FitLabel::button();
    value.add_css_class("wc-conv-value");
    value.set_hexpand(true);
    // The marker: a bar at the bottom left, drawn as a sized background
    // of the strip's inner box while the field is the one typed into.
    let marker = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    marker.add_css_class("wc-conv-marker");
    let inner = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    inner.set_margin_start(mc::FIELD_PAD_X as i32);
    inner.set_margin_end(mc::FIELD_PAD_X as i32);
    inner.set_hexpand(true);
    inner.append(&value);
    marker.append(&inner);
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
    dropdown.set_size_request(-1, mc::UNIT_H as i32);
    // GTK names the drop-down after the unit it shows (a labelled-by
    // relation, which outranks a label), as upstream's reads; which field
    // it is goes in its description.
    dropdown.update_property(&[gtk::accessible::Property::Description(label)]);
    unit_button(&dropdown);
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.add_css_class("wc-conv-field");
    frame.set_size_request(-1, mc::FIELD_H as i32);
    marker.set_vexpand(true);
    frame.append(&marker);
    Field {
        frame,
        value,
        dropdown,
    }
}

/// The drop-down's button as DGMNB's unit button: "Centimeters ▾",
/// centred (the toolkit's own list, with its marks, stays the popup's).
/// The arrow is drawn, not read: the unit alone names it.
fn unit_button(dropdown: &gtk::DropDown) {
    if let Some(list) = dropdown.factory() {
        dropdown.set_list_factory(Some(&list));
    }
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let name = gtk::Label::new(None);
        let arrow = gtk::Label::builder()
            .label(" ▾")
            .accessible_role(gtk::AccessibleRole::Presentation)
            .build();
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.set_halign(gtk::Align::Center);
        row.set_hexpand(true);
        row.append(&name);
        row.append(&arrow);
        item.set_child(Some(&row));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let name = item
            .child()
            .and_then(|row| row.first_child())
            .and_downcast::<gtk::Label>();
        if let (Some(name), Some(unit)) = (name, item.item().and_downcast::<gtk::StringObject>()) {
            name.set_text(&unit.string());
        }
    });
    dropdown.set_factory(Some(&factory));
    // And not the toolkit's arrow beside it.
    let mut todo = vec![dropdown.clone().upcast::<gtk::Widget>()];
    while let Some(w) = todo.pop() {
        if w.css_name() == "arrow" && w.ancestor(gtk::Popover::static_type()).is_none() {
            w.set_visible(false);
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            todo.push(c);
        }
    }
}

/// Names each unit in `dropdown`'s list as upstream's does, by its spoken
/// name (`spoken`): GTK's own list items have none. And its search field.
fn name_units(dropdown: &gtk::DropDown, spoken: &Rc<RefCell<HashMap<String, String>>>) {
    // GTK's own factory, which shows the unit and marks the chosen one
    // (the list's: `unit_button`); this runs after it binds each row.
    if let Some(factory) = dropdown
        .list_factory()
        .and_downcast::<gtk::SignalListItemFactory>()
    {
        let spoken = Rc::downgrade(spoken);
        factory.connect_bind(move |_, item| {
            let (Some(item), Some(spoken)) =
                (item.downcast_ref::<gtk::ListItem>(), spoken.upgrade())
            else {
                return;
            };
            if let Some(unit) = item.item().and_downcast::<gtk::StringObject>() {
                let shown = unit.string();
                let spoken = spoken.borrow();
                let name = spoken
                    .get(shown.as_str())
                    .map_or(shown.as_str(), String::as_str);
                item.set_accessible_label(name);
            }
        });
    }
    let mut child = dropdown.first_child();
    let mut todo = Vec::new();
    while let Some(c) = child {
        child = c.next_sibling();
        todo.push(c);
    }
    let (mut search, mut popovers) = (None, Vec::new());
    // DGMNB's list has no arrow pointing at its button.
    while let Some(w) = todo.pop() {
        if let Some(entry) = w.downcast_ref::<gtk::SearchEntry>() {
            w.update_property(&[gtk::accessible::Property::Label("Search units")]);
            // Undo from appcore's history, as DGMNB's unit search
            // (crate::undo).
            search = Some(crate::undo::attach(entry));
        }
        if let Some(popover) = w.downcast_ref::<gtk::Popover>() {
            popover.set_has_arrow(false);
            popovers.push(popover.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            todo.push(c);
        }
    }
    // GTK clears the search as the list closes (gtkdropdown.c
    // `popover_closed`, before this): the program's text, which ends the
    // history, as DGMNB's search starts afresh each time its list opens.
    if let Some(search) = search {
        for popover in popovers {
            let search = search.clone();
            popover.connect_closed(move |_| search.forget());
        }
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
        let swap_icon = PathIcon::new(paths::SWAP, ctl::ICON_BUTTON_ICON as i32);
        let swap = gtk::Button::builder()
            .child(&swap_icon)
            .tooltip_text("Swap units")
            .css_classes(["wc-swap"])
            .halign(gtk::Align::Start)
            .build();
        crate::a11y::name_button(&swap, "Swap units");
        let unit_gap = (mc::UNIT_STEP - mc::UNIT_H) as i32;
        f1.frame
            .set_margin_bottom((mc::FIELD_STEP - mc::FIELD_H) as i32);
        f2.frame
            .set_margin_bottom((mc::FIELD_STEP - mc::FIELD_H) as i32);
        f1.dropdown.set_margin_bottom(unit_gap);
        f2.dropdown.set_margin_bottom(unit_gap);
        swap.set_margin_top(mc::SWAP_TOP as i32);
        swap.set_margin_bottom((mc::SWAP_STEP - mc::SWAP_TOP - ctl::ICON_BUTTON) as i32);

        let supp_header = gtk::Label::new(Some("About equal to"));
        supp_header.add_css_class("wc-conv-caption");
        supp_header.set_xalign(0.0);
        supp_header.set_size_request(-1, mc::CAPTION_H as i32);
        supp_header.set_margin_bottom((mc::CAPTION_STEP - mc::CAPTION_H) as i32);
        let supp = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .max_children_per_line(64)
            .row_spacing((mc::CHIP_ROW_STEP - mc::CHIP_H) as u32)
            .column_spacing(mc::CHIP_GAP as u32)
            .halign(gtk::Align::Start)
            .css_classes(["wc-supp"])
            .build();

        let line = |class: &str, height: f32| {
            let l = gtk::Label::new(None);
            l.add_css_class(class);
            l.set_xalign(0.0);
            l.set_size_request(-1, height as i32);
            l
        };
        let ratio = line("wc-ratio", mc::RATIO_H);
        let timestamp = line("wc-timestamp", mc::TIMESTAMP_H);
        let status = line("wc-status", mc::STATUS_H);
        status.set_ellipsize(gtk::pango::EllipsizeMode::End);
        status.set_margin_bottom((mc::STATUS_STEP - mc::STATUS_H) as i32);
        let refresh = gtk::Button::builder()
            .label("Update rates")
            .css_classes(["wc-link"])
            .halign(gtk::Align::Start)
            .build();
        refresh.set_size_request(mc::REFRESH_W as i32, mc::TIMESTAMP_H as i32);
        refresh.set_margin_start(mc::REFRESH_GAP as i32);
        let spinner = adw::Spinner::new();
        spinner.set_visible(false);
        spinner.set_margin_start(mc::UPDATING_GAP as i32);
        let ts_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        ts_row.append(&timestamp);
        ts_row.append(&refresh);
        ts_row.append(&spinner);
        ts_row.set_margin_bottom((mc::TIMESTAMP_STEP - mc::TIMESTAMP_H) as i32);
        let currency_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        currency_box.append(&ratio);
        currency_box.append(&ts_row);
        currency_box.append(&status);

        let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        section.set_margin_top(mc::SECTION_GAP as i32);
        section.set_margin_bottom(mc::BOTTOM_PAD as i32);
        section.append(&currency_box);
        section.append(&supp_header);
        section.append(&supp);

        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.add_css_class("wc-plain");
        column.set_valign(gtk::Align::Start);
        column.append(&f1.frame);
        column.append(&f1.dropdown);
        column.append(&swap);
        column.append(&f2.frame);
        column.append(&f2.dropdown);
        column.append(&section);
        let values = gtk::ScrolledWindow::builder()
            .child(&column)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .build();

        let keypad = Keypad::new();
        for (key, r, c, rs, cs) in keys::converter() {
            keypad.add(key, r, c, rs, cs);
        }

        let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
        layout.append(&values);
        layout.append(&keypad);
        let root = SizeBin::new(&layout);

        let page = Rc::new(ConverterPage {
            ctx: ctx.clone(),
            vm: RefCell::new(vm),
            root: root.clone(),
            layout,
            f1,
            f2,
            supp,
            supp_header,
            whimsy_icon: Cell::new(false),
            currency_box,
            ratio,
            timestamp,
            status,
            refresh: refresh.clone(),
            spinner,
            keypad: keypad.clone(),
            style: RefCell::new(None),
            syncing: Cell::new(false),
            told: RefCell::new(None),
            unit_ids: RefCell::default(),
            spoken: Rc::default(),
            wide: Cell::new(true),
            showing: Cell::new(false),
        });

        {
            // The keypad's own effects (TRANSITION: the layers' business).
            let weak = Rc::downgrade(&page);
            ctx.hub.subscribe(move |s| {
                if let Some(p) = weak.upgrade() {
                    p.keypad.set_scheme(*s);
                }
            });
            ctx.hub
                .subscribe_style(&page.keypad, |pad, style| pad.set_style(style));
            let weak = Rc::downgrade(&page);
            let swap_icon = swap_icon.clone();
            ctx.hub.subscribe_style(&page.root, move |_, style| {
                if let Some(p) = weak.upgrade() {
                    if let Some(icon) = style.control("converter.swap").map(|c| c.icon) {
                        let stroke = icon.stroke.unwrap_or(style.icon_stroke(icon.size));
                        swap_icon.set_style(icon.size, stroke);
                    }
                    p.whimsy_icon.set(style.flag("converter.whimsy_icon"));
                    for f in [&p.f1, &p.f2] {
                        f.value.set_halo(style.shadows("window.text_halo"));
                    }
                    p.style.replace(Some(style.clone()));
                    p.lay_out();
                    p.sync();
                }
            });
        }
        let weak = Rc::downgrade(&page);
        keypad.connect_pressed(move |id, x, y| {
            if let Some(p) = weak.upgrade() {
                p.ctx.pulse_at(&p.keypad, x, y, accent(&p.ctx), 0.55);
                p.press(id);
            }
        });
        for (which, f) in [(1, &page.f1), (2, &page.f2)] {
            ctx.layers.hold_list(&f.dropdown);
            name_units(&f.dropdown, &page.spoken);
            let pick = {
                let weak = Rc::downgrade(&page);
                Rc::new(move || {
                    if let Some(p) = weak.upgrade() {
                        {
                            let mut vm = p.vm.borrow_mut();
                            if which == 1 {
                                vm.activate_value1();
                            } else {
                                vm.activate_value2();
                            }
                        }
                        p.sync();
                    }
                })
            };
            let click = gtk::GestureClick::new();
            {
                let pick = pick.clone();
                click.connect_released(move |_, _, _, _| pick());
            }
            f.value.add_controller(click);
            // The same from the keyboard, once Tab has brought the focus
            // here: Space, as upstream's OnValueKeyDown, or Enter. (The
            // window lets both through: neither is a converter key.)
            // With Shift or Ctrl held too, as any control's
            // (input::control_key): upstream's compares the key alone.
            let keys = gtk::EventControllerKey::new();
            {
                let pick = pick.clone();
                keys.connect_key_pressed(move |_, key, _, mods| {
                    let activates = crate::keymap::key_press(key, mods).is_some_and(|kp| {
                        input::control_key(&kp) == Some(input::ControlKey::Activate)
                    });
                    if activates {
                        pick();
                        glib::Propagation::Stop
                    } else {
                        glib::Propagation::Proceed
                    }
                });
            }
            f.value.add_controller(keys);
            // And for assistive technology.
            crate::a11y::operable(&f.value, "value", "activate", move |_| pick());
            let weak = Rc::downgrade(&page);
            f.dropdown.connect_selected_notify(move |d| {
                let Some(p) = weak.upgrade() else { return };
                if p.syncing.get() {
                    return;
                }
                if p.ctx.layers.covers(d) {
                    // Covered, the unit stays (R15-M-02): back to the VM's.
                    p.sync();
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
                p.sync();
            });
        }
        {
            let weak = Rc::downgrade(&page);
            swap.connect_clicked(move |b| {
                if let Some(p) = weak.upgrade() {
                    p.vm.borrow_mut().swap_units();
                    p.ctx.pulse_at(
                        b,
                        b.width() as f32 / 2.0,
                        b.height() as f32 / 2.0,
                        accent(&p.ctx),
                        1.0,
                    );
                    p.sync();
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
                // Only for Currency on screen, as DGMNB does: going back to
                // it fetches anyway.
                let in_currency = p.showing.get() && p.vm.borrow().is_currency_current_category();
                if behavior == NetworkAccessBehavior::Normal && in_currency {
                    let started = p.vm.borrow_mut().start_automatic_currency_fetch();
                    if started {
                        p.fetch_currency();
                    }
                }
                p.sync();
            });
        }
        {
            let weak = Rc::downgrade(&page);
            root.connect_size(move |_, _| {
                if let Some(p) = weak.upgrade() {
                    p.lay_out();
                }
            });
        }
        page
    }

    /// DGMNB's layout for the page's size and the style's `layout`: wide,
    /// the keypad on the right at its width; narrow, below, giving up
    /// height (down to a least) before the values do.
    fn lay_out(&self) {
        let Some(style) = self.style.borrow().clone() else {
            return;
        };
        let (w, h) = self.root.size();
        let m = match style.get("layout.converter.margin") {
            Some(Value::Spacing(s)) => *s,
            _ => Spacing::default(),
        };
        self.layout.set_margin_top(m.top.round() as i32);
        self.layout.set_margin_end(m.right.round() as i32);
        self.layout.set_margin_bottom(m.bottom.round() as i32);
        self.layout.set_margin_start(m.left.round() as i32);
        let wide = w as f32 >= mc::WIDE;
        self.wide.set(wide);
        if wide {
            self.layout.set_orientation(gtk::Orientation::Horizontal);
            self.layout
                .set_spacing(style.number("layout.converter.gap_wide").round() as i32);
            let pad_w = style.number("layout.converter.keypad_width").round() as i32;
            self.keypad.set_size_request(pad_w, -1);
            self.keypad.set_vexpand(true);
        } else {
            self.layout.set_orientation(gtk::Orientation::Vertical);
            self.layout
                .set_spacing(style.number("layout.converter.gap").round() as i32);
            let area = h as f32 - m.top - m.bottom;
            let needed = if self.vm.borrow().is_currency_current_category() {
                mc::NEEDED_CURRENCY
            } else {
                mc::NEEDED
            };
            let pad_h = (area - needed)
                .min(area * mc::PAD_SHARE)
                .clamp(mc::PAD_MIN_H, mc::PAD_MAX_H);
            self.keypad.set_size_request(-1, pad_h.round() as i32);
            self.keypad.set_vexpand(false);
        }
    }

    fn press(self: &Rc<Self>, id: u32) {
        let Some(cmd) = keys::converter_command(id) else {
            return;
        };
        self.vm.borrow_mut().button_pressed(cmd);
        self.sync();
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
                p.sync();
            }
        });
    }

    fn rebuild_units(&self) {
        let vm = self.vm.borrow();
        let units = vm.units();
        let names: Vec<String> = units.iter().map(|u| u.name.clone()).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        self.unit_ids.replace(units.iter().map(|u| u.id).collect());
        self.spoken.replace(
            units
                .iter()
                .map(|u| (u.name.clone(), u.accessible_name.clone()))
                .collect(),
        );
        self.syncing.set(true);
        for f in [&self.f1, &self.f2] {
            f.dropdown.set_model(Some(&gtk::StringList::new(&refs)));
        }
        self.syncing.set(false);
    }

    /// Push VM state into widgets.
    fn sync(&self) {
        let style = self.style.borrow().clone();
        let value_font = style.as_deref().and_then(value_font);
        let unit_font = style
            .as_deref()
            .and_then(|s| s.control("converter.unit"))
            .map(|c| c.font.clone());
        let vm = self.vm.borrow();
        self.syncing.set(true);
        let ids = self.unit_ids.borrow();
        for (f, unit, value, is_active, sym, name) in [
            (
                &self.f1,
                vm.unit1(),
                vm.value1(),
                vm.value1_active(),
                vm.currency_symbol1(),
                vm.value1_automation_name(),
            ),
            (
                &self.f2,
                vm.unit2(),
                vm.value2(),
                vm.value2_active(),
                vm.currency_symbol2(),
                vm.value2_automation_name(),
            ),
        ] {
            f.value.set_text(value);
            if let Some((font, checked_weight)) = &value_font {
                let font = Font {
                    weight: if is_active {
                        *checked_weight
                    } else {
                        font.weight
                    },
                    ..font.clone()
                };
                f.value.set_font(&font, txt::CONVERTER_VALUE_MIN_SIZE);
            }
            if is_active {
                f.frame.add_css_class("wc-active");
            } else {
                f.frame.remove_css_class("wc-active");
            }
            // DGMNB's unit button: the unit's width and room for the arrow.
            if let (Some(u), Some(font)) = (unit, &unit_font) {
                let (fd, attrs) = pango_font(font, font.size);
                let layout = f.dropdown.create_pango_layout(Some(&u.name));
                layout.set_font_description(Some(&fd));
                layout.set_attributes(Some(&attrs));
                let (_, logical) = layout.extents();
                let text = logical.width() as f32 / gtk::pango::SCALE as f32;
                f.dropdown
                    .set_size_request((text + mc::UNIT_ARROW).ceil() as i32, mc::UNIT_H as i32);
            }
            if let Some(u) = unit
                && let Some(pos) = ids.iter().position(|&id| id == u.id)
                && f.dropdown.selected() != pos as u32
            {
                f.dropdown.set_selected(pos as u32);
            }
            // Named as upstream's (and DGMNB's) fields, "Convert from 5
            // Centimeters", "Converts into 1.97 Inches"; selected while
            // it is the one typed into.
            f.value
                .update_property(&[gtk::accessible::Property::Label(&name)]);
            f.value
                .update_state(&[gtk::accessible::State::Selected(Some(is_active))]);
            // The second value is told when what it says changes, as
            // upstream's is a polite live region (Value2,
            // UnitConverter.xaml:595-599; the first is not) and DGMNB's
            // node: whichever of the two is typed into, and not what the
            // page shows as it opens.
            if std::ptr::eq(f, &self.f2) {
                let was = self.told.replace(Some(name.clone()));
                if was.is_some_and(|was| was != name) {
                    f.value
                        .announce(&name, gtk::AccessibleAnnouncementPriority::Medium);
                }
            }
            // The symbol before the value, in its own text style, the
            // value after its width and the gap (DGMNB's arithmetic).
            let prefix = match style.as_deref().and_then(|s| s.get("converter.symbol")) {
                Some(Value::Text(t)) if vm.currency_symbol_visible() && !sym.is_empty() => {
                    Some(Prefix {
                        text: sym.to_string(),
                        font: t.font.clone(),
                        color: t.color,
                        gap: mc::SYMBOL_GAP,
                    })
                }
                _ => None,
            };
            f.value.set_prefix(prefix);
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
            chip.set_size_request(-1, mc::CHIP_H as i32);
            if r.is_whimsical() {
                chip.add_css_class("wc-chip-whimsy");
                if self.whimsy_icon.get() {
                    chip.append(&PathIcon::new(paths::SPARKLE, 14));
                }
            }
            let l = gtk::Label::new(Some(&format!("{} {}", r.value, r.unit.abbreviation)));
            l.set_hexpand(true);
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
        p.showing.set(true);
        p.vm.borrow_mut().set_current_mode(cm);
        p.rebuild_units();
        p.sync();
        p.lay_out();
        p.keypad.cascade();
        if cm == ConverterMode::Currency {
            let started = p.vm.borrow_mut().start_automatic_currency_fetch();
            if started {
                p.fetch_currency();
            }
        }
    }

    fn deactivate(&self) {
        self.0.showing.set(false);
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
            p.ctx.pulse_at(&p.keypad, x, y, accent(&p.ctx), 0.5);
        }
        p.press(id);
        true
    }

    /// The keypad's keys: upstream's converter keys are `CalculatorButton`s,
    /// which ignore Enter (and the converter has no Enter of its own).
    fn is_calculator_key(&self, widget: &gtk::Widget) -> bool {
        widget.is_ancestor(&self.0.keypad)
    }

    fn copy(&self) -> Option<String> {
        let text = self.0.vm.borrow().copy_text().to_string();
        self.0.ctx.copy_to_clipboard(&text);
        Some(text)
    }

    fn paste(&self, text: &str) {
        appcore::converter::paste(&mut self.0.vm.borrow_mut(), text);
        self.0.sync();
    }

    fn save(&self) {
        if let Ok(v) = serde_json::to_value(self.0.vm.borrow().preferences()) {
            self.0.ctx.store.set_page_state("converter", v);
        }
    }
}

/// The values' font at their largest, and its weight in the field typed
/// into (`converter.value`).
fn value_font(style: &Style) -> Option<(Font, f32)> {
    let Some(Value::Record(r)) = style.get("converter.value") else {
        return None;
    };
    let (Some(Value::Number(size)), Some(Value::Font(font)), Some(Value::Number(weight))) =
        (r.get("size"), r.get("font"), r.get("checked_weight"))
    else {
        return None;
    };
    let font = Font {
        size: *size as f32,
        ..font.clone()
    };
    Some((font, *weight as f32))
}

/// The accent, for a press's event (the layers' business).
fn accent(ctx: &Ctx) -> [f32; 3] {
    let a = ctx.hub.style().tokens.colors.accent;
    [a[0], a[1], a[2]]
}

impl ConverterPage {
    pub fn handle(ctx: Rc<Ctx>) -> Rc<dyn Page> {
        Rc::new(ConverterHandle(Self::new(ctx)))
    }
}
