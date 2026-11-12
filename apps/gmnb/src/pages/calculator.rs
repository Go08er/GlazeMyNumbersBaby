//! Standard / Scientific / Programmer (upstream Calculator.xaml + friends).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use calcvm::{
    AngleUnit, Button as B, CalcMode, CalculatorViewModel, Event, Radix, ShiftMode, WordSize,
};
use gtk::{gdk, glib};

use super::{Ctx, Page};
use crate::widgets::bitflip::BitFlip;
use crate::widgets::calc_panel::{CalcPanel, MemOp};
use crate::widgets::display::{Change, Display, Mode};
use crate::widgets::fit::{self, FitLabel};
use crate::widgets::icon::{PathIcon, icon_toggle, paths};
use crate::widgets::keypad::Keypad;
use crate::widgets::width_bin::{Cells, FixedBin, Strip, WidthBin};
use appcore::KeyPress;
use appcore::input::{self, Action};
use appcore::keys::{self, KEY_HYP, KEY_SECOND, KEY_TRIG_SECOND};
use appcore::modes::ViewMode;
use gmnb_theme::Style;
use gmnb_theme::types::{Spacing, Value};

/// Width at which history/memory docks beside the keypad.
const WIDE_PX: i32 = appcore::metrics::calc::WIDE as i32;

pub struct CalculatorPage {
    ctx: Rc<Ctx>,
    vm: RefCell<CalculatorViewModel>,
    root: WidthBin,
    /// The display and keypad: what keys and pastes change, and what the
    /// History sheet covers.
    column: gtk::Box,
    display: Display,
    /// Holds the readout at its mode's height.
    display_bin: FixedBin,
    modes: gtk::Stack,
    keypads: RefCell<HashMap<CalcMode, Keypad>>,
    flyouts: RefCell<Vec<Keypad>>,
    trig_pad: RefCell<Option<Keypad>>,
    mem_buttons: RefCell<Vec<(B, gtk::Button)>>,
    mem_toggles: RefCell<Vec<gtk::Button>>,
    /// The programmer bar's M▾, shown wide or narrow.
    prog_mem_toggle: RefCell<Option<gtk::Button>>,
    angle_btn: RefCell<Option<gtk::Button>>,
    fe_btn: RefCell<Option<gtk::ToggleButton>>,
    second: Cell<bool>,
    trig_inv: Cell<bool>,
    hyp: Cell<bool>,
    radix_rows: RefCell<Vec<(Radix, gtk::Button, FitLabel)>>,
    word_btn: RefCell<Option<gtk::Button>>,
    prog_stack: RefCell<Option<gtk::Stack>>,
    bitflip: RefCell<Option<Rc<BitFlip>>>,
    shift_radios: RefCell<Vec<(ShiftMode, gtk::CheckButton)>>,
    panel: Rc<CalcPanel>,
    side: gtk::Box,
    side_bin: FixedBin,
    sheet: adw::BottomSheet,
    sheet_holder: gtk::Box,
    history_btn: gtk::Button,
    wide: Cell<bool>,
    /// Set while pushing VM state into widgets, so their change signals
    /// don't feed back into the VM.
    syncing: Cell<bool>,
    programmer: Cell<bool>,
    compact_btn: gtk::ToggleButton,
    compact: Cell<bool>,
    mem_rows: RefCell<Vec<Cells>>,
    /// Another page was shown since the calculator was (see `switch_mode`).
    left: Cell<bool>,
}

fn text_button(label: &str, tip: &str) -> gtk::Button {
    let b = gtk::Button::builder()
        .child(&button_text(label, "text_button"))
        .css_classes(["wc-mem"])
        .tooltip_text(tip)
        .build();
    // Called what it shows unless it is named (a11y::name_button).
    b.update_property(&[gtk::accessible::Property::Label(label)]);
    b
}

/// A text button's label, set as DGMNB's `button` sets it: centred and
/// shrunk to fit its button (down to `BUTTON_MIN_SIZE`), in `path`'s
/// font, unhinted, each glyph on a quarter pixel. Drawn only: its button
/// is named. Its text is on the window (these buttons have no fill at
/// rest), so it has the window's halo.
fn button_text(text: &str, path: &'static str) -> FitLabel {
    let l = FitLabel::drawn();
    l.set_text(text);
    l.set_align(fit::Align::Center);
    fit::follow(
        &l,
        path,
        appcore::metrics::text::BUTTON_MIN_SIZE,
        Some("window.text_halo"),
    );
    l
}

/// A button that opens a flyout, DGMNB's: one label ending in "▾" (read
/// out without it), lit while the flyout is open. `class`: its look,
/// `wc-flyout-button` (Scientific's, `flyout_button`) or `wc-text-flyout`
/// (the programmer bar's, `text_button`).
fn chevron_menu(ctx: &Ctx, label: &str, class: &str, popover: &gtk::Popover) -> gtk::MenuButton {
    // The arrow (from a fallback font) in a line box of its own, a pixel
    // high: the label's line keeps Inter's height (Pango reads a
    // `line_height` above 1024 as Pango units).
    let path = if class == "wc-flyout-button" {
        "flyout_button"
    } else {
        "text_button"
    };
    let text = button_text(&format!("{label} ▾"), path);
    let button = gtk::MenuButton::builder()
        .child(&text)
        .popover(popover)
        .css_classes([class])
        .build();
    crate::a11y::name_menu_button(&button, label);
    // Its keys take the focus on a click, as the keypad's, without the
    // ring, and the keys typed in it go the window's way (Ctx::flyout).
    ctx.flyout(popover);
    button
}

impl CalculatorPage {
    pub fn new(ctx: Rc<Ctx>) -> Rc<Self> {
        let display = Display::new(64.0);
        // The readout is exactly `display.heights` high (`sync_display`).
        let display_bin = FixedBin::new(&display);

        let modes = gtk::Stack::new();
        // `motion.mode` ms (`apply_style`; none in the base look).
        modes.set_transition_type(gtk::StackTransitionType::Crossfade);
        modes.set_vhomogeneous(false);
        modes.set_vexpand(true);

        // DGMNB's page: the readout, then the mode's rows and its keypad,
        // inside `layout.calculator.margin` (`apply_style`).
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        column.append(&display_bin);
        column.append(&modes);

        let panel = CalcPanel::new(&ctx.hub);
        let side = gtk::Box::new(gtk::Orientation::Vertical, 0);
        side.add_css_class("wc-side-panel");
        // Exactly `panel_width` wide, with its margins (`apply_style`).
        let side_bin = FixedBin::new(&side);
        side_bin.set_visible(false);

        // The narrow window's History and Memory, in a sheet DGMNB's
        // share of the page high (`fit_sheet`).
        let sheet_holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sheet_holder.add_css_class("wc-sheet");
        let sheet = adw::BottomSheet::builder()
            .content(&column)
            .sheet(&sheet_holder)
            .show_drag_handle(true)
            .build();
        sheet_holder.append(&panel.root);

        {
            // An open sheet covers the calculator (it is modal by default).
            let weak = sheet.downgrade();
            ctx.layers
                .scrim(&column, &sheet, &["open", "modal"], move || {
                    weak.upgrade().is_some_and(|s| s.is_open() && s.is_modal())
                });
        }
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        sheet.set_hexpand(true);
        row.append(&sheet);
        row.append(&side_bin);

        let root = WidthBin::new(&row);
        // A button, as DGMNB's (not a toggle, lit while the sheet is open):
        // it opens the sheet, or closes the open one.
        let history_btn = crate::window::icon_button(paths::HISTORY, "History (Ctrl+H)");
        crate::window::follow_icon(&ctx.hub, &history_btn, "header.button");
        {
            // The button names History: clicked, it shows that list,
            // whichever the sheet showed last (M▾ leaves it on Memory, and
            // the sheet then opened on Memory from this button too:
            // R19-L-01's other half).
            let panel = panel.clone();
            let sheet = sheet.downgrade();
            history_btn.connect_clicked(move |_| {
                let Some(sheet) = sheet.upgrade() else { return };
                if sheet.is_open() {
                    sheet.set_open(false);
                } else {
                    panel.show_tab("history");
                    sheet.set_open(true);
                }
            });
        }

        let page = Rc::new(CalculatorPage {
            ctx: ctx.clone(),
            vm: RefCell::new(CalculatorViewModel::new()),
            root: root.clone(),
            column: column.clone(),
            display: display.clone(),
            display_bin,
            modes: modes.clone(),
            keypads: RefCell::default(),
            flyouts: RefCell::default(),
            trig_pad: RefCell::default(),
            mem_buttons: RefCell::default(),
            mem_toggles: RefCell::default(),
            prog_mem_toggle: RefCell::default(),
            angle_btn: RefCell::default(),
            fe_btn: RefCell::default(),
            second: Cell::new(false),
            trig_inv: Cell::new(false),
            hyp: Cell::new(false),
            radix_rows: RefCell::default(),
            word_btn: RefCell::default(),
            prog_stack: RefCell::default(),
            bitflip: RefCell::default(),
            shift_radios: RefCell::default(),
            panel: panel.clone(),
            side,
            side_bin,
            sheet,
            sheet_holder,
            history_btn,
            wide: Cell::new(false),
            syncing: Cell::new(false),
            programmer: Cell::new(false),
            compact_btn: icon_toggle(paths::KEEP_ON_TOP, "Keep on top (Alt+Up)"),
            compact: Cell::new(false),
            mem_rows: RefCell::default(),
            left: Cell::new(false),
        });
        {
            let weak = Rc::downgrade(&page);
            page.compact_btn.connect_toggled(move |b| {
                if let Some(p) = weak.upgrade() {
                    p.set_compact(b.is_active());
                }
            });
        }
        crate::window::follow_icon(&ctx.hub, &page.compact_btn, "header.button");
        {
            let weak = Rc::downgrade(&page);
            ctx.hub.subscribe_style(&page.root, move |_, style| {
                if let Some(p) = weak.upgrade() {
                    p.apply_style(style);
                }
            });
        }
        {
            // The sheet is DGMNB's share of the page's height.
            let weak = Rc::downgrade(&page);
            page.root.connect_height(move |h| {
                if let Some(p) = weak.upgrade() {
                    p.fit_sheet(h);
                }
            });
        }

        // Scientific and Programmer (≈150 more widgets incl. the 64-bit flip
        // panel) are built on first visit, not up front.
        page.ensure_mode_built(CalcMode::Standard);
        page.wire_panel();
        page.install_display_menu();

        let weak = Rc::downgrade(&page);
        root.connect_width(move |w| {
            if let Some(p) = weak.upgrade() {
                let wide = w >= WIDE_PX;
                if wide != p.wide.get() {
                    p.set_wide(wide);
                }
            }
        });

        if let Some(state) = ctx
            .store
            .page_state(appcore::settings::CALCULATOR_PAGE)
            .and_then(|v| v.as_str().map(String::from))
        {
            page.vm.borrow_mut().restore_state(&state);
        }
        // What a restored session shows at the start isn't told as news.
        page.vm.borrow_mut().take_announcements();
        page.sync_all(Change::None);
        page
    }

    // -------------------------------------------------------------- style

    /// The page's spaces from the style (`layout.calculator`), DGMNB's in
    /// the base look; the readout's size.
    fn apply_style(&self, style: &Style) {
        set_margins(&self.column, spacing(style, "layout.calculator.margin"));
        self.column
            .set_spacing(style.number("layout.calculator.gap").round() as i32);
        let margin = spacing(style, "layout.calculator.panel_margin");
        set_margins(&self.side, margin);
        let width = style.number("layout.calculator.panel_width") + margin.left + margin.right;
        self.side_bin.set_width(width.round() as i32);
        self.modes
            .set_transition_duration(style.number("motion.mode").round() as u32);
        if let Some(stack) = self.prog_stack.borrow().as_ref() {
            stack.set_transition_duration(style.number("motion.flip").round() as u32);
        }
        let gap = style.number("text_button_gap");
        for row in self.mem_rows.borrow().iter() {
            row.set_gap(gap);
        }
        self.sync_display();
    }

    /// The readout's mode, the one shown: the readout takes its value's
    /// largest size and its height from the style by it (`display.sizes`,
    /// `display.heights`), and its bin holds it at that height.
    fn sync_display(&self) {
        let mode = if self.compact.get() {
            Mode::Compact
        } else {
            match self.vm.try_borrow().map(|vm| vm.mode()) {
                Ok(CalcMode::Scientific) => Mode::Scientific,
                Ok(CalcMode::Programmer) => Mode::Programmer,
                _ => Mode::Standard,
            }
        };
        self.display.set_mode(mode);
        self.display_bin.set_height(self.display.height_wanted());
    }

    /// The narrow window's sheet, DGMNB's: its share of the page's height,
    /// at least `SHEET_MIN_H`, leaving `SHEET_VERTICAL_MARGIN` above it.
    fn fit_sheet(&self, page_height: i32) {
        use appcore::metrics::calc::{SHEET_MIN_H, SHEET_SHARE, SHEET_VERTICAL_MARGIN};
        let h = page_height as f32;
        let sheet = (h * SHEET_SHARE)
            .max(SHEET_MIN_H)
            .min(h - SHEET_VERTICAL_MARGIN);
        self.sheet_holder
            .set_height_request(sheet.max(0.0).round() as i32);
    }

    // ------------------------------------------------------------ building

    fn add_keypad(self: &Rc<Self>, layout: keys::Layout, main_for: Option<CalcMode>) -> Keypad {
        let pad = Keypad::new();
        for (key, r, c) in layout {
            pad.add(key, r, c, 1, 1);
        }
        let weak = Rc::downgrade(self);
        let pad_ = pad.clone();
        pad.connect_pressed(move |id, x, y| {
            if let Some(p) = weak.upgrade() {
                p.on_key(&pad_, id, x, y);
            }
        });
        match main_for {
            Some(mode) => {
                self.keypads.borrow_mut().insert(mode, pad.clone());
            }
            None => self.flyouts.borrow_mut().push(pad.clone()),
        }
        self.ctx
            .hub
            .subscribe_style(&pad, |pad, style| pad.set_style(style));
        pad
    }

    /// Upstream's OpenParenthesisButton_GotFocus (Scientific and
    /// Programmer): each time `pad`'s "(" gets the focus (Tab, a click),
    /// screen readers are told how many parentheses are open ("Open
    /// parenthesis count 2"), after the key's own name.
    fn announce_open_parens(self: &Rc<Self>, pad: &Keypad) {
        let Some(open) = pad.button(B::OpenParenthesis.id()) else {
            return;
        };
        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(self);
        focus.connect_enter(move |c| {
            if let (Some(p), Some(key)) = (weak.upgrade(), c.widget())
                && let Ok(vm) = p.vm.try_borrow()
            {
                let count = vm.open_parens();
                drop(vm);
                key.announce(
                    &keys::open_parenthesis_count_announcement(count),
                    gtk::AccessibleAnnouncementPriority::Medium,
                );
            }
        });
        open.add_controller(focus);
    }

    /// The memory buttons in a row of their own (`text_button`s, as
    /// DGMNB's): MC MR M+ M− MS, and M▾ while narrow. `full`: false for
    /// the programmer bar's MS and M▾, which are its own.
    fn memory_row(self: &Rc<Self>, full: bool) -> Cells {
        let row = Cells::new(appcore::metrics::controls::TEXT_BUTTON_GAP);
        // Text straight on the backdrop: the scrim of a see-through window.
        row.add_css_class("wc-plain");
        // Named as upstream's (ClearMemoryButton "Clear all memory", ...),
        // their tooltips read as descriptions.
        let all: &[(B, &str, &str, &str)] = &[
            (
                B::MemoryClear,
                "MC",
                "Clear all memory",
                "Clear all memory (Ctrl+L)",
            ),
            (
                B::MemoryRecall,
                "MR",
                "Memory recall",
                "Memory recall (Ctrl+R)",
            ),
            (B::MemoryAdd, "M+", "Memory add", "Memory add (Ctrl+P)"),
            (
                B::MemorySubtract,
                "M−",
                "Memory subtract",
                "Memory subtract (Ctrl+Q)",
            ),
            (B::Memory, "MS", "Memory store", "Memory store (Ctrl+M)"),
        ];
        for (b, label, name, tip) in all {
            if !full && *b != B::Memory {
                continue;
            }
            let button = text_button(label, tip);
            crate::a11y::name_button(&button, name);
            let weak = Rc::downgrade(self);
            let id = *b;
            button.connect_clicked(move |w| {
                if let Some(p) = weak.upgrade() {
                    p.press_from(w, id);
                }
            });
            row.append(&button);
            self.mem_buttons.borrow_mut().push((*b, button));
        }
        let toggle = text_button("M▾", "Memory");
        crate::a11y::name_button(&toggle, "Memory");
        let weak = Rc::downgrade(self);
        toggle.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                p.panel.show_tab("memory");
                if !p.wide.get() {
                    p.sheet.set_open(true);
                }
            }
        });
        row.append(&toggle);
        if full {
            // The programmer bar's M▾ is always there, as DGMNB's.
            self.mem_toggles.borrow_mut().push(toggle);
        } else {
            self.prog_mem_toggle.replace(Some(toggle));
        }
        self.mem_rows.borrow_mut().push(row.clone());
        row
    }

    /// Upstream "Keep on top": display + standard keypad only.
    fn set_compact(&self, on: bool) {
        if self.compact.replace(on) == on {
            return;
        }
        if self.compact_btn.is_active() != on {
            self.compact_btn.set_active(on);
        }
        let icon = self.compact_btn.child().and_downcast::<PathIcon>();
        if let Some(icon) = icon {
            icon.set_path(if on {
                paths::BACK_TO_FULL
            } else {
                paths::KEEP_ON_TOP
            });
        }
        self.compact_btn.set_tooltip_text(Some(if on {
            "Back to full view (Alt+Down)"
        } else {
            "Keep on top (Alt+Up)"
        }));
        for r in self.mem_rows.borrow().iter() {
            r.set_visible(!on);
        }
        self.history_btn
            .set_visible(!on && !self.wide.get() && !self.programmer.get());
        self.side_bin.set_visible(!on && self.wide.get());
        if let Some(pad) = self.keypads.borrow().get(&CalcMode::Standard) {
            pad.set_compact(on);
        }
        self.sync_display();
        self.ctx.set_compact(on);
    }

    /// A mode's keypad, `KEYPAD_PAD_Y` clear of what is above and below it.
    fn keypad_slot(pad: &Keypad) {
        let pad_y = appcore::metrics::calc::KEYPAD_PAD_Y as i32;
        pad.set_vexpand(true);
        pad.set_margin_top(pad_y);
        pad.set_margin_bottom(pad_y);
    }

    fn build_standard(self: &Rc<Self>) -> gtk::Widget {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
        b.append(&self.memory_row(true));
        let pad = self.add_keypad(keys::standard(), Some(CalcMode::Standard));
        Self::keypad_slot(&pad);
        b.append(&pad);
        b.upcast()
    }

    fn build_scientific(self: &Rc<Self>) -> gtk::Widget {
        use appcore::metrics::calc as mc;
        let b = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // DEG and F-E, in a row of their own.
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        top.add_css_class("wc-plain");
        top.add_css_class("wc-short-row");
        top.set_height_request(mc::ANGLE_ROW_H as i32);
        let angle = text_button("DEG", "Switch angle unit (F3/F4/F5)");
        angle.set_width_request(mc::ANGLE_W as i32);
        let weak = Rc::downgrade(self);
        angle.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                let next = p.vm.borrow().angle_unit().next();
                p.set_angle(next);
            }
        });
        let fe = gtk::ToggleButton::builder()
            .child(&button_text("F-E", "text_button"))
            .css_classes(["wc-mem"])
            .build();
        fe.update_property(&[gtk::accessible::Property::Label("F-E")]);
        fe.set_width_request(mc::FE_W as i32);
        fe.set_tooltip_text(Some("Scientific notation (V)"));
        let weak = Rc::downgrade(self);
        fe.connect_clicked(move |w| {
            if let Some(p) = weak.upgrade()
                && w.is_active() != p.vm.borrow().is_fe()
            {
                p.press_from(w, B::FToE);
            }
        });
        // Named as upstream's ("Degrees toggle", "Scientific notation"):
        // they show "DEG" and "F-E".
        crate::a11y::name_button(
            &angle,
            appcore::keys::angle_button_name(self.vm.borrow().angle_unit()),
        );
        crate::a11y::name_button(fe.upcast_ref(), appcore::keys::FTOE_BUTTON_NAME);
        top.append(&angle);
        top.append(&fe);
        self.angle_btn.replace(Some(angle));
        self.fe_btn.replace(Some(fe));
        b.append(&top);
        b.append(&self.memory_row(true));

        // Flyouts.
        let trig = self.add_keypad(keys::trig(), None);
        self.flyout_size(&trig, mc::TRIG_FLYOUT);
        self.trig_pad.replace(Some(trig.clone()));
        let trig_pop = gtk::Popover::builder()
            .child(&trig)
            .css_classes(["wc-flyout"])
            .build();
        let func = self.add_keypad(keys::functions(), None);
        self.flyout_size(&func, mc::FUNCTION_FLYOUT);
        let func_pop = gtk::Popover::builder()
            .child(&func)
            .css_classes(["wc-flyout"])
            .build();
        // Trigonometry ▾ and Function ▾: their row is 36 high, the buttons
        // 3 in from its top and bottom; Function 4 in from its sides.
        let flyrow = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        flyrow.add_css_class("wc-plain");
        flyrow.set_height_request(mc::FLYOUT_ROW_H as i32);
        let trig_btn = chevron_menu(&self.ctx, "Trigonometry", "wc-flyout-button", &trig_pop);
        trig_btn.set_width_request(mc::TRIG_W as i32);
        let func_btn = chevron_menu(&self.ctx, "Function", "wc-flyout-button", &func_pop);
        func_btn.set_width_request((mc::FUNCTION_W - 2.0 * mc::FUNCTION_INSET_X) as i32);
        func_btn.set_margin_start(mc::FUNCTION_INSET_X as i32);
        for button in [&trig_btn, &func_btn] {
            button.set_valign(gtk::Align::Center);
            flyrow.append(button);
        }
        b.append(&flyrow);

        let pad = self.add_keypad(keys::scientific(), Some(CalcMode::Scientific));
        self.announce_open_parens(&pad);
        // Upstream's 2nd alone isn't focused by a click
        // (AllowFocusOnInteraction="False"): Enter after it is still the
        // focused control's, "=" after clicking a key. The flyout's 2nd
        // and hyp are, as every other key.
        if let Some(second) = pad.button(KEY_SECOND) {
            second.set_focus_on_click(false);
        }
        Self::keypad_slot(&pad);
        b.append(&pad);
        b.upcast()
    }

    /// A flyout's keypad: DGMNB's card, `(width, height)` with its inset.
    fn flyout_size(&self, pad: &Keypad, (w, h): (f32, f32)) {
        let inset = appcore::metrics::calc::FLYOUT_INSET;
        pad.set_size_request((w - 2.0 * inset) as i32, (h - 2.0 * inset) as i32);
    }

    fn build_programmer(self: &Rc<Self>) -> gtk::Widget {
        use appcore::metrics::programmer as mpg;
        let b = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // The radix list: four rows and a little room under them.
        let radix_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        radix_box.add_css_class("wc-radix-list");
        radix_box.add_css_class("wc-plain");
        radix_box
            .set_height_request((mpg::RADIX_ROWS * mpg::RADIX_ROW_H + mpg::RADIX_LIST_PAD) as i32);
        for radix in Radix::ALL {
            let name = gtk::Label::new(Some(radix.label()));
            name.add_css_class("wc-radix-name");
            name.set_width_request(mpg::RADIX_NAME_W as i32);
            name.set_xalign(0.0);
            // DGMNB's `label_fit`: a long value shrinks to its room (a 64
            // digit binary one to 9 px), it isn't cut at its start.
            let value = FitLabel::drawn();
            value.set_text("0");
            value.add_css_class("wc-radix-value");
            value.set_hexpand(true);
            fit::follow(
                &value,
                "programmer.radix.value",
                appcore::metrics::text::RADIX_VALUE_MIN_SIZE,
                None,
            );
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            content.append(&name);
            content.append(&value);
            // One of four, as upstream's radix RadioButtons: assistive
            // technology hears which base is current ("checked").
            let button = gtk::Button::builder()
                .child(&content)
                .css_classes(["wc-radix-row", "flat"])
                .accessible_role(gtk::AccessibleRole::Radio)
                .build();
            button.update_state(&[gtk::accessible::State::Checked(
                gtk::AccessibleTristate::False,
            )]);
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(p) = weak.upgrade() {
                    p.set_radix(radix);
                }
            });
            radix_box.append(&button);
            self.radix_rows.borrow_mut().push((radix, button, value));
        }
        b.append(&radix_box);

        // The bar, DGMNB's: the two keypads, word size, Bitwise ▾ and Bit
        // shift ▾; MS and M▾ at its end. 36 high, its buttons 2 in.
        let bar = Strip::new();
        bar.add_css_class("wc-plain");
        bar.add_css_class("wc-short-row");
        bar.set_height_request(mpg::BAR_H as i32);
        let in_bar = |w: &gtk::Widget, width: f32, inset: f32| {
            w.set_margin_top(mpg::BAR_PAD_Y as i32);
            w.set_margin_bottom(mpg::BAR_PAD_Y as i32);
            bar.append(w, false, width, inset);
        };
        let full = icon_toggle(paths::KEYBOARD, "Full keypad");
        let bits = icon_toggle(paths::BITS, "Bit toggling keypad");
        bits.set_group(Some(&full));
        full.set_active(true);
        for t in [&full, &bits] {
            crate::window::follow_icon(&self.ctx.hub, t, "icon_button");
            in_bar(t.upcast_ref(), mpg::BAR_SWITCH_W, 0.0);
        }
        let word = text_button("QWORD", "Word size (F2/F3/F4/F12)");
        let weak = Rc::downgrade(self);
        word.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                let next = p.vm.borrow().word_size().next();
                p.set_word(next);
            }
        });
        in_bar(word.upcast_ref(), mpg::WORD_W, mpg::WORD_INSET);
        // Named as upstream's ("Quadruple Word toggle"): it shows "QWORD".
        crate::a11y::name_button(
            &word,
            appcore::keys::word_size_button_name(self.vm.borrow().word_size()),
        );
        self.word_btn.replace(Some(word));

        // Bitwise / shift flyouts.
        let bitwise = self.add_keypad(keys::bitwise(), None);
        self.flyout_size(&bitwise, appcore::metrics::calc::BITWISE_FLYOUT);
        let bw_pop = gtk::Popover::builder()
            .child(&bitwise)
            .css_classes(["wc-flyout"])
            .build();
        let shift_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        shift_box.set_margin_top(6);
        shift_box.set_margin_bottom(6);
        shift_box.set_margin_start(6);
        shift_box.set_margin_end(6);
        let mut group: Option<gtk::CheckButton> = None;
        for (mode, label) in [
            (ShiftMode::Arithmetic, "Arithmetic shift"),
            (ShiftMode::Logical, "Logical shift"),
            (ShiftMode::Rotate, "Rotate circular shift"),
            (
                ShiftMode::RotateThroughCarry,
                "Rotate through carry circular shift",
            ),
        ] {
            let c = gtk::CheckButton::with_label(label);
            c.set_group(group.as_ref());
            group.get_or_insert(c.clone());
            let weak = Rc::downgrade(self);
            c.connect_toggled(move |c| {
                if c.is_active()
                    && let Some(p) = weak.upgrade()
                    && !p.syncing.get()
                {
                    p.set_shift(mode);
                    // Another mode closes the flyout, as upstream's
                    // (BitshiftFlyout_Checked); the one already chosen
                    // toggles nothing and leaves it open.
                    if let Some(flyout) = c
                        .ancestor(gtk::Popover::static_type())
                        .and_downcast::<gtk::Popover>()
                    {
                        flyout.popdown();
                    }
                }
            });
            shift_box.append(&c);
            self.shift_radios.borrow_mut().push((mode, c));
        }
        let shift_pop = gtk::Popover::builder()
            .child(&shift_box)
            .css_classes(["wc-flyout"])
            .build();
        let bw_btn = chevron_menu(&self.ctx, "Bitwise", "wc-text-flyout", &bw_pop);
        in_bar(bw_btn.upcast_ref(), mpg::BITWISE_W, mpg::BAR_BUTTON_INSET);
        let shift_btn = chevron_menu(&self.ctx, "Bit shift", "wc-text-flyout", &shift_pop);
        in_bar(shift_btn.upcast_ref(), mpg::SHIFT_W, mpg::BAR_BUTTON_INSET);
        // MS and M▾ from the end: M▾ last, as DGMNB's.
        let mem = self.memory_row(false);
        let mut buttons = Vec::new();
        let mut child = mem.first_child();
        while let Some(button) = child {
            child = button.next_sibling();
            mem.remove(&button);
            buttons.push(button);
        }
        for button in buttons.iter().rev() {
            button.set_margin_top(mpg::BAR_PAD_Y as i32);
            button.set_margin_bottom(mpg::BAR_PAD_Y as i32);
            bar.append(button, true, mpg::MEMORY_BUTTON_W, 0.0);
        }
        self.mem_rows.borrow_mut().retain(|r| r != &mem);
        b.append(&bar);

        let stack = gtk::Stack::new();
        // Keypad and bits slide over each other for `motion.flip` ms.
        stack.set_transition_type(gtk::StackTransitionType::SlideUpDown);
        stack.set_transition_duration(self.ctx.hub.style().number("motion.flip").round() as u32);
        let pad = self.add_keypad(keys::programmer(), Some(CalcMode::Programmer));
        self.announce_open_parens(&pad);
        Self::keypad_slot(&pad);
        stack.add_named(&pad, Some("full"));
        let flip = BitFlip::new(&self.ctx.hub);
        let weak = Rc::downgrade(self);
        flip.connect_flip(move |i| {
            if let Some(p) = weak.upgrade() {
                p.vm.borrow_mut().flip_bit(i);
                p.sync(Some(Change::Typing));
            }
        });
        // In a box that fills the stack: GTK 4.22 counts a stack child's
        // place in the stack twice in what it tells assistive technology
        // (the child's bounds, then its GtkStackPage's, which are the
        // same), so a child placed inside it was reported that far below
        // where it is drawn, and a click there missed.
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        flip.root.set_vexpand(true);
        let pad_y = appcore::metrics::calc::KEYPAD_PAD_Y as i32;
        flip.root.set_margin_top(pad_y);
        flip.root.set_margin_bottom(pad_y);
        holder.append(&flip.root);
        stack.add_named(&holder, Some("bits"));
        stack.set_vexpand(true);
        {
            let stack = stack.clone();
            bits.connect_toggled(move |t| {
                stack.set_visible_child_name(if t.is_active() { "bits" } else { "full" })
            });
        }
        if std::env::var("GMNB_BITS").as_deref() == Ok("1") {
            bits.set_active(true);
        }
        self.bitflip.replace(Some(flip));
        self.prog_stack.replace(Some(stack.clone()));
        b.append(&stack);
        b.upcast()
    }

    /// Right-click on the display: Copy / Paste (upstream context menu).
    fn install_display_menu(self: &Rc<Self>) {
        let click = gtk::GestureClick::builder().button(3).build();
        let weak = Rc::downgrade(self);
        click.connect_pressed(move |_, _, x, y| {
            let Some(p) = weak.upgrade() else { return };
            let copy = gtk::Button::with_label("Copy");
            let paste = gtk::Button::with_label("Paste");
            for b in [&copy, &paste] {
                b.add_css_class("flat");
            }
            let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
            col.append(&copy);
            col.append(&paste);
            let pop = gtk::Popover::builder().child(&col).has_arrow(false).build();
            crate::window::own_keys(&pop);
            pop.set_parent(&p.display);
            pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            // Weak popover: it contains these buttons.
            let (pop_, p_) = (pop.downgrade(), Rc::downgrade(&p));
            copy.connect_clicked(move |_| {
                if let Some(pop) = pop_.upgrade() {
                    pop.popdown();
                }
                if let Some(p) = p_.upgrade() {
                    p.copy_value();
                }
            });
            let (pop_, p_) = (pop.downgrade(), Rc::downgrade(&p));
            paste.connect_clicked(move |_| {
                if let Some(pop) = pop_.upgrade() {
                    pop.popdown();
                }
                let Some(p_) = p_.upgrade() else { return };
                if let Some(root) = p_.display.root().and_downcast::<gtk::Window>() {
                    // Route through the window so Ctrl+V and the menu share one path.
                    let _ = gtk::prelude::WidgetExt::activate_action(&root, "win.paste", None);
                }
            });
            pop.connect_closed(|p| {
                let p = p.clone();
                glib::idle_add_local_once(move || p.unparent());
            });
            pop.popup();
        });
        self.display.add_controller(click);
    }

    fn wire_panel(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.panel.connect_history_recall(move |i| {
            if let Some(p) = weak.upgrade() {
                p.vm.borrow_mut().history_recall(i);
                p.sync(None);
                if !p.wide.get() {
                    p.sheet.set_open(false);
                }
            }
        });
        let weak = Rc::downgrade(self);
        self.panel.connect_history_delete(move |i| {
            if let Some(p) = weak.upgrade() {
                p.vm.borrow_mut().history_remove(i);
                p.sync(None);
            }
        });
        let weak = Rc::downgrade(self);
        self.panel.connect_history_clear(move || {
            if let Some(p) = weak.upgrade() {
                p.vm.borrow_mut().history_clear();
                p.sync(None);
            }
        });
        let weak = Rc::downgrade(self);
        self.panel.connect_memory_op(move |i, op, button| {
            if let Some(p) = weak.upgrade() {
                // MC, M+ and M- on a stored value: a memory press, told
                // before the list is made anew without that button.
                if let Some(button) = button {
                    use crate::layers::{Action as Does, Source};
                    let layers = p.ctx.hub.layers();
                    layers.pressed(button, Source::Button, Some(Does::Memory));
                }
                {
                    let mut vm = p.vm.borrow_mut();
                    match op {
                        MemOp::Recall => vm.memory_recall(i),
                        MemOp::Add => vm.memory_add(i),
                        MemOp::Subtract => vm.memory_subtract(i),
                        MemOp::Clear => vm.memory_clear(i),
                    }
                }
                p.sync(None);
                if op == MemOp::Recall && !p.wide.get() {
                    p.sheet.set_open(false);
                }
            }
        });
        let weak = Rc::downgrade(self);
        self.panel.connect_memory_clear_all(move || {
            if let Some(p) = weak.upgrade() {
                p.vm.borrow_mut().press(B::MemoryClear);
                p.sync(None);
            }
        });
    }

    fn set_wide(&self, wide: bool) {
        self.wide.set(wide);
        let root = &self.panel.root;
        if let Some(parent) = root.parent().and_downcast::<gtk::Box>() {
            parent.remove(root);
        }
        if wide {
            self.sheet.set_open(false);
            self.side.append(root);
        } else {
            self.sheet_holder.append(root);
        }
        self.side_bin.set_visible(wide && !self.compact.get());
        self.history_btn
            .set_visible(!wide && !self.programmer.get() && !self.compact.get());
        for t in self.mem_toggles.borrow().iter() {
            t.set_visible(!wide);
        }
    }

    // ------------------------------------------------------------- input

    fn press_from(self: &Rc<Self>, widget: &impl IsA<gtk::Widget>, b: B) {
        use crate::layers::{Action as Does, Source};
        let memory = (b != B::FToE).then_some(Does::Memory);
        self.ctx
            .hub
            .layers()
            .pressed(widget, Source::Button, memory);
        self.vm.borrow_mut().press(b);
        self.sync(None);
    }

    fn on_key(self: &Rc<Self>, pad: &Keypad, id: u32, x: f32, y: f32) {
        self.ctx.hub.layers().key_pressed(pad, id, Some((x, y)));
        match id {
            KEY_SECOND => {
                self.second.set(!self.second.get());
                self.relabel();
                return;
            }
            KEY_TRIG_SECOND => {
                self.trig_inv.set(!self.trig_inv.get());
                self.relabel();
                return;
            }
            KEY_HYP => {
                self.hyp.set(!self.hyp.get());
                self.relabel();
                return;
            }
            _ => {}
        }
        let Some(button) = button_from_id(id) else {
            return;
        };
        // One of the keys 2nd turned into its second function (x³, ³√x,
        // ʸ√x, 2ˣ, logᵧx, eˣ), upstream's InvRow1 buttons, whose Click is
        // ShiftButton_Uncheck (CalculatorScientificOperators.xaml:1089-1140,
        // .xaml.cs:65): 2nd is unchecked and takes the focus (a click, Space,
        // assistive technology; a typed shortcut runs the key's command
        // alone, there as here: KeyboardShortcutManager.RunButtonCommand).
        let second_function = self.second.get()
            && self.vm.borrow().mode() == CalcMode::Scientific
            && keys::SECOND_FLIPS.iter().any(|f| f.normal == button);
        self.press_resolved(button);
        if second_function {
            self.second.set(false);
            self.relabel();
            if let Some(second) = pad.button(KEY_SECOND) {
                second.grab_focus();
            }
        }
        // A flyout closes when one of its keys is pressed (a click, Space,
        // assistive technology; not the 2nd and hyp toggles), as
        // upstream's (FlyoutButton_Clicked). GTK gives the focus back to
        // the button that opened it: Enter after the click opens it again.
        if let Some(flyout) = pad
            .ancestor(gtk::Popover::static_type())
            .and_downcast::<gtk::Popover>()
        {
            // Upstream's FlyoutButton_Clicked
            // (CalculatorScientificOperators.xaml.cs:82), every key of the
            // Trigonometry and Function flyouts': the trig flyout's 2nd
            // and hyp are unchecked, whichever key it was (a trig function
            // or not). (Programmer's Bitwise flyout keeps its own handler,
            // which leaves them be.)
            if self.vm.borrow().mode() == CalcMode::Scientific {
                self.trig_inv.set(false);
                self.hyp.set(false);
                self.relabel();
            }
            flyout.popdown();
        }
    }

    /// Apply 2nd/hyp/shift-mode/C-CE remapping, then press.
    fn press_resolved(self: &Rc<Self>, button: B) {
        let mode = self.vm.borrow().mode();
        let mut b = button;
        if mode == CalcMode::Scientific {
            b = keys::resolve_second(b, self.second.get());
            b = keys::resolve_trig(b, self.trig_inv.get(), self.hyp.get());
        }
        if mode == CalcMode::Programmer {
            let (l, r) = keys::shift_keys(self.vm.borrow().shift_mode());
            if b == B::Lsh {
                b = l.0;
            } else if b == B::Rsh {
                b = r.0;
            }
        }
        if b == B::Clear && mode != CalcMode::Standard && self.vm.borrow().shows_clear_entry() {
            b = B::ClearEntry;
        }
        self.vm.borrow_mut().press(b);
        self.sync(None);
    }

    fn set_angle(self: &Rc<Self>, unit: AngleUnit) {
        self.vm.borrow_mut().set_angle_unit(unit);
        self.sync(None);
    }

    fn set_radix(self: &Rc<Self>, radix: Radix) {
        self.vm.borrow_mut().set_radix(radix);
        self.sync(Some(Change::Replace));
    }

    fn set_word(self: &Rc<Self>, w: WordSize) {
        self.vm.borrow_mut().set_word_size(w);
        self.sync(Some(Change::Replace));
    }

    fn set_shift(self: &Rc<Self>, s: ShiftMode) {
        self.vm.borrow_mut().set_shift_mode(s);
        self.relabel();
    }

    fn relabel(&self) {
        let mode = self.vm.borrow().mode();
        if let Some(pad) = self.keypads.borrow().get(&CalcMode::Scientific) {
            let second = self.second.get();
            for f in &keys::SECOND_FLIPS {
                pad.set_key(f.normal.id(), f.label(second), f.tip(second));
            }
            if let Some(b) = pad.button(KEY_SECOND) {
                set_on(&b, self.second.get());
            }
        }
        if let Some(trig) = self.trig_pad.borrow().as_ref() {
            let (inv, hyp) = (self.trig_inv.get(), self.hyp.get());
            for t in keys::TRIG {
                let label = keys::trig_label(t.4, inv, hyp);
                trig.set_key(t.0.id(), &label, &keys::trig_tip(t.4, inv, hyp));
            }
            if let Some(b) = trig.button(KEY_TRIG_SECOND) {
                set_on(&b, self.trig_inv.get());
            }
            if let Some(b) = trig.button(KEY_HYP) {
                set_on(&b, self.hyp.get());
            }
        }
        if let Some(pad) = self.keypads.borrow().get(&CalcMode::Programmer) {
            let shift = self.vm.borrow().shift_mode();
            let ((_, l), (_, r)) = keys::shift_keys(shift);
            let (lt, rt) = keys::shift_tips(shift);
            pad.set_key(B::Lsh.id(), l, lt);
            pad.set_key(B::Rsh.id(), r, rt);
        }
        let _ = mode;
    }

    // -------------------------------------------------------------- sync

    fn sync_all(self: &Rc<Self>, change: Change) {
        let vm = self.vm.borrow();
        let history_emptied = self.panel.set_history(&vm.history());
        let memory_emptied = self.panel.set_memory(&vm.memory());
        drop(vm);
        self.relabel();
        self.sync(Some(change));
        // Another mode's lists, shown with the focus in the one before's.
        self.focus_after_emptied(history_emptied, memory_emptied);
    }

    /// Pull state from the VM into the widgets. `force` overrides the change
    /// kind derived from VM events.
    fn sync(self: &Rc<Self>, force: Option<Change>) {
        let (events, said) = {
            let mut vm = self.vm.borrow_mut();
            (vm.take_events(), vm.take_announcements())
        };
        self.syncing.set(true);
        let vm = self.vm.borrow();
        let mode = vm.mode();
        let change = force.unwrap_or_else(|| {
            if events.contains(&Event::Error) {
                Change::Error
            } else if events.contains(&Event::Result) {
                Change::Result
            } else if events.contains(&Event::Replace) {
                Change::Replace
            } else if events.contains(&Event::Typing) {
                Change::Typing
            } else {
                Change::None
            }
        });
        self.display
            .set_value(&vm.display_value(), change, vm.is_error());
        self.display.set_expression(&vm.expression());

        // The focus was in a list (or on what clears it) that is now empty:
        // it goes where upstream's does, below.
        let history_emptied =
            events.contains(&Event::HistoryChanged) && self.panel.set_history(&vm.history());
        let mut memory_emptied =
            events.contains(&Event::MemoryChanged) && self.panel.set_memory(&vm.memory());
        let has_memory = !vm.memory().is_empty();
        for (b, w) in self.mem_buttons.borrow().iter() {
            let enabled = vm.is_enabled(*b) && !vm.is_error();
            // MC or MR focused as the memory empties: disabled, it would
            // keep the focus no key can use.
            memory_emptied |= !enabled && !has_memory && w.is_focus();
            w.set_sensitive(enabled);
        }
        for t in self.mem_toggles.borrow().iter() {
            t.set_sensitive(has_memory);
        }
        if let Some(t) = self.prog_mem_toggle.borrow().as_ref() {
            t.set_sensitive(has_memory);
        }

        if let Some(pad) = self.keypads.borrow().get(&mode)
            && mode != CalcMode::Standard
        {
            pad.set_key_label(
                B::Clear.id(),
                if vm.shows_clear_entry() { "CE" } else { "C" },
            );
            let n = vm.open_parens();
            let open = if n > 0 {
                format!("(<sub>{n}</sub>")
            } else {
                "(".to_string()
            };
            pad.set_key_label(B::OpenParenthesis.id(), &open);
        }
        // Upstream enablement rules (radix digits, decimal, operators while
        // an error is shown, …) for every engine key, flyouts included.
        let enable = |pad: &Keypad| {
            for id in pad.ids() {
                if let Some(b) = button_from_id(id) {
                    let b = keys::resolve_second(b, self.second.get());
                    pad.set_key_sensitive(id, vm.is_enabled(b));
                }
            }
        };
        if let Some(pad) = self.keypads.borrow().get(&mode) {
            enable(pad);
        }
        for pad in self.flyouts.borrow().iter() {
            enable(pad);
        }

        match mode {
            CalcMode::Scientific => {
                if let Some(a) = self.angle_btn.borrow().as_ref() {
                    set_text(a, vm.angle_unit().label());
                    crate::a11y::name_button(a, appcore::keys::angle_button_name(vm.angle_unit()));
                }
                if let Some(fe) = self.fe_btn.borrow().as_ref() {
                    fe.set_active(vm.is_fe());
                }
            }
            CalcMode::Programmer => {
                for (radix, button, label) in self.radix_rows.borrow().iter() {
                    label.set_text(&vm.radix_value(*radix));
                    let current = *radix == vm.radix();
                    if current {
                        button.add_css_class("wc-radix-active");
                    } else {
                        button.remove_css_class("wc-radix-active");
                    }
                    button.update_state(&[gtk::accessible::State::Checked(if current {
                        gtk::AccessibleTristate::True
                    } else {
                        gtk::AccessibleTristate::False
                    })]);
                    button.update_property(&[gtk::accessible::Property::Label(&format!(
                        "{} {}",
                        radix.label(),
                        vm.radix_value(*radix)
                    ))]);
                }
                if let Some(w) = self.word_btn.borrow().as_ref() {
                    set_text(w, vm.word_size().label());
                    crate::a11y::name_button(
                        w,
                        appcore::keys::word_size_button_name(vm.word_size()),
                    );
                }
                for (m, c) in self.shift_radios.borrow().iter() {
                    if *m == vm.shift_mode() && !c.is_active() {
                        c.set_active(true);
                    }
                }
                if let Some(flip) = self.bitflip.borrow().as_ref() {
                    flip.sync(|i| vm.bit(i), vm.word_size().bits());
                }
            }
            CalcMode::Standard => {}
        }
        drop(vm);
        self.syncing.set(false);

        // What upstream's view model tells a screen reader, as it does
        // (calcvm::Announcement, each `Announcement = …` of
        // StandardCalculatorViewModel, which its NarratorNotifier says):
        // the display each time the value shown changes, typed digits too,
        // and after a binary operator ("Display is 7 plus"); the
        // open-parenthesis count each time it changes, in Scientific and
        // Programmer; a ")" with none open. In the order they came: a
        // paste's "Open parenthesis count 1", "Display is 1", "Open
        // parenthesis count 0".
        for a in &said {
            self.display.announce(
                &keys::announcement(a),
                gtk::AccessibleAnnouncementPriority::Medium,
            );
        }

        self.focus_after_emptied(history_emptied, memory_emptied);
    }

    /// The focus was in a list (or on what clears it) that is now empty.
    /// History: the focus goes to the panel's History tab, as upstream's
    /// docked panel's (`OnHistoryEmptied`, Calculator.xaml.cs:714-730:
    /// `DockPivot.Focus`); with no tabs to take it (Programmer shows
    /// Memory alone), and for Memory, to M+.
    fn focus_after_emptied(&self, history: bool, memory: bool) {
        if memory || (history && !self.panel.focus_tabs()) {
            self.focus_after_memory_emptied();
        }
    }

    /// The memory was emptied with the focus in its list, on what clears
    /// it or on MC or MR, which are disabled now: the focus goes to M+, as
    /// upstream's (`OnMemoryEmptied`, Calculator.xaml.cs:732-744:
    /// `MemPlus.Focus`, the Memory flyout closed first while narrow, the
    /// keys being under it).
    fn focus_after_memory_emptied(&self) {
        if !self.wide.get() {
            self.sheet.set_open(false);
        }
        let add = self
            .mem_buttons
            .borrow()
            .iter()
            .find(|(b, w)| *b == B::MemoryAdd && w.is_mapped())
            .map(|(_, w)| w.clone());
        // Once the closing sheet has uncovered it, if it hasn't yet.
        if let Some(add) = add
            && !add.grab_focus()
        {
            glib::idle_add_local_once(move || {
                add.grab_focus();
            });
        }
    }

    fn flash(self: &Rc<Self>, b: B) {
        let mode = self.vm.borrow().mode();
        let id = match b {
            // Keys whose cell shows another id.
            B::ClearEntry if mode != CalcMode::Standard => B::Clear.id(),
            other => keys::SECOND_FLIPS
                .iter()
                .find(|f| f.second == other)
                .map(|f| f.normal.id())
                .unwrap_or(other.id()),
        };
        if let Some(pad) = self.keypads.borrow().get(&mode).cloned()
            && pad.flash(id).is_some()
        {
            self.ctx.hub.layers().key_pressed(&pad, id, None);
        }
    }
}

/// A text button's label set anew ([`button_text`]).
fn set_text(button: &gtk::Button, text: &str) {
    if let Some(label) = button.child().and_downcast::<FitLabel>() {
        label.set_text(text);
    }
}

/// The spacing at `path` (none if there is none).
fn spacing(style: &Style, path: &str) -> Spacing {
    match style.get(path) {
        Some(Value::Spacing(s)) => *s,
        _ => Spacing::default(),
    }
}

/// `w`'s margins, in whole pixels.
fn set_margins(w: &impl IsA<gtk::Widget>, m: Spacing) {
    w.set_margin_top(m.top.round() as i32);
    w.set_margin_end(m.right.round() as i32);
    w.set_margin_bottom(m.bottom.round() as i32);
    w.set_margin_start(m.left.round() as i32);
}

fn set_on(b: &gtk::Button, on: bool) {
    if on {
        b.add_css_class("wc-on");
    } else {
        b.remove_css_class("wc-on");
    }
    b.update_state(&[gtk::accessible::State::Pressed(if on {
        gtk::AccessibleTristate::True
    } else {
        gtk::AccessibleTristate::False
    })]);
}

/// Map a keypad id back to the `Button` enum.
fn button_from_id(id: u32) -> Option<B> {
    B::from_id(id)
}

impl CalculatorPage {
    fn ensure_mode_built(self: &Rc<Self>, mode: CalcMode) {
        let name = match mode {
            CalcMode::Standard => "standard",
            CalcMode::Scientific => "scientific",
            CalcMode::Programmer => "programmer",
        };
        if self.modes.child_by_name(name).is_some() {
            return;
        }
        let widget = match mode {
            CalcMode::Standard => self.build_standard(),
            CalcMode::Scientific => self.build_scientific(),
            CalcMode::Programmer => self.build_programmer(),
        };
        crate::window::exact_text(&widget);
        self.modes.add_named(&widget, Some(name));
        self.relabel();
    }

    fn switch_mode(self: &Rc<Self>, mode: ViewMode) {
        let calc_mode = match mode {
            ViewMode::Scientific => CalcMode::Scientific,
            ViewMode::Programmer => CalcMode::Programmer,
            _ => CalcMode::Standard,
        };
        self.ensure_mode_built(calc_mode);
        // Like upstream's `ApplicationViewModel.Mode` setter: the calculator
        // is set up again (`SetCalculatorType`) when its mode changes or you
        // come back to it from another page, not when the mode shown is
        // chosen again (nor after the saved session was restored at start).
        let returning = self.left.replace(false);
        let switching = self.vm.borrow().mode() != calc_mode;
        let programmer = calc_mode == CalcMode::Programmer;
        // Upstream's checkDefaultBitShift
        // (CalculatorProgrammerRadixOperators.xaml.cs:44-58), which
        // OnIsProgrammerPropertyChanged runs (Calculator.xaml.cs:408, by
        // EnsureProgrammer and OperatorsPanel.EnsureProgrammerRadixOps)
        // when the calculator turns to Programmer from Standard or
        // Scientific: it starts on Arithmetic shift. Not when Programmer is
        // shown again, after another page, nor when a restored session's
        // Programmer is shown first: that session goes on with its shift
        // mode, as it would have uninterrupted.
        if switching && programmer {
            self.set_shift(ShiftMode::Arithmetic);
        }
        if returning || switching {
            self.vm.borrow_mut().set_mode(calc_mode);
        }
        self.programmer.set(programmer);
        if calc_mode != CalcMode::Standard {
            self.set_compact(false);
        }
        self.compact_btn
            .set_visible(calc_mode == CalcMode::Standard);
        self.panel.set_history_available(!programmer);
        self.history_btn
            .set_visible(!self.wide.get() && !programmer);
        self.modes.set_visible_child_name(match calc_mode {
            CalcMode::Standard => "standard",
            CalcMode::Scientific => "scientific",
            CalcMode::Programmer => "programmer",
        });
        self.sync_display();
        if let Some(pad) = self.keypads.borrow().get(&calc_mode) {
            pad.cascade();
        }
    }

    /// Ctrl+H (the history, in the sheet while narrow) and Ctrl+Shift+D
    /// (clear it).
    fn history_key(self: &Rc<Self>, action: Action) {
        match action {
            Action::ToggleHistory => {
                // The sheet showing History closes; closed, or showing
                // Memory, it shows History, as upstream's key shows the
                // History flyout unless that is the one open
                // (ToggleHistoryFlyout) and as DGMNB's does.
                let showing = self.sheet.is_open() && self.panel.current_tab() == "history";
                self.panel.show_tab("history");
                if !self.wide.get() {
                    self.sheet.set_open(!showing);
                }
            }
            Action::ClearHistory => {
                self.vm.borrow_mut().history_clear();
                self.sync(None);
            }
            _ => {}
        }
    }

    fn copy_value(&self) -> String {
        let text = self.vm.borrow().copy_text();
        self.ctx.copy_to_clipboard(&text);
        text
    }

    fn save_state(&self) {
        let state = self.vm.borrow().save_state();
        self.ctx.store.set_page_state(
            appcore::settings::CALCULATOR_PAGE,
            serde_json::Value::String(state),
        );
    }
}

/// `Page` methods take `&self`, but our handlers need `Rc<Self>`; the window
/// keeps pages as `Rc<dyn Page>`, so route through this adapter instead.
pub struct CalculatorHandle(pub Rc<CalculatorPage>);

impl Page for CalculatorHandle {
    fn widget(&self) -> gtk::Widget {
        self.0.root.clone().upcast()
    }

    fn deactivate(&self) {
        self.0.left.set(true);
        // Compact ("keep on top") is a Standard-only view; never strand the
        // window in compact chrome on another page.
        self.0.set_compact(false);
    }

    fn activate(&self, mode: ViewMode) {
        self.0.switch_mode(mode);
        self.0.sync(Some(Change::Replace));
    }

    fn header_end(&self) -> Vec<gtk::Widget> {
        vec![
            self.0.compact_btn.clone().upcast(),
            self.0.history_btn.clone().upcast(),
        ]
    }

    fn key_pressed(&self, kp: &KeyPress) -> bool {
        let p = &self.0;
        if p.vm.borrow().mode() == CalcMode::Standard
            && let Some(on) = input::compact_shortcut(kp)
        {
            p.set_compact(on);
            return true;
        }
        let (mode, shift) = {
            let vm = p.vm.borrow();
            (vm.mode(), vm.shift_mode())
        };
        let Some(action) = input::shortcut(mode, kp, shift) else {
            return false;
        };
        match action {
            Action::Press(b) => {
                if !p.vm.borrow().is_enabled(b) {
                    return true;
                }
                p.flash(b);
                p.vm.borrow_mut().press(b);
                p.sync(None);
            }
            Action::ToggleHistory | Action::ClearHistory => {
                p.history_key(action);
            }
            Action::Angle(u) => p.set_angle(u),
            Action::Radix(r) => p.set_radix(r),
            Action::Word(w) => p.set_word(w),
        }
        true
    }

    /// Upstream's `CalculatorButton`s, which ignore Enter: every keypad's
    /// keys (the flyouts' too) but the 2nd and hyp toggles (ToggleButtons
    /// there, which Enter toggles), and the memory buttons MC, MR, M+, M−
    /// and MS; and the bit toggles (`FlipButtons`, which ignore it too).
    /// Not the angle, F-E, word size, radix, keypad kind or flyout buttons,
    /// nor M▾, the History or Memory items and their buttons: plain
    /// buttons, toggles, radio buttons and list items upstream.
    fn is_calculator_key(&self, widget: &gtk::Widget) -> bool {
        let p = &self.0;
        let key = widget.ancestor(Keypad::static_type()).is_some()
            && widget.accessible_role() != gtk::AccessibleRole::ToggleButton;
        key || p.mem_buttons.borrow().iter().any(|(_, b)| b == widget)
            || p.bitflip
                .borrow()
                .as_ref()
                .is_some_and(|f| widget.is_ancestor(&f.root))
    }

    fn target(&self) -> gtk::Widget {
        self.0.column.clone().upcast()
    }

    /// The History sheet's own keys, while it covers the calculator.
    fn layer_key_pressed(&self, kp: &KeyPress) -> bool {
        let p = &self.0;
        let (mode, shift) = {
            let vm = p.vm.borrow();
            (vm.mode(), vm.shift_mode())
        };
        match input::shortcut(mode, kp, shift) {
            Some(action @ (Action::ToggleHistory | Action::ClearHistory)) => {
                p.history_key(action);
                true
            }
            _ => false,
        }
    }

    fn copy(&self) -> Option<String> {
        Some(self.0.copy_value())
    }

    fn paste(&self, text: &str) {
        // The view model shows upstream's "Invalid input" itself on rejection.
        let _accepted = self.0.vm.borrow_mut().paste(text);
        self.0.sync(None);
    }

    fn save(&self) {
        self.0.save_state();
    }
}

impl CalculatorPage {
    pub fn handle(ctx: Rc<Ctx>) -> Rc<dyn Page> {
        Rc::new(CalculatorHandle(Self::new(ctx)))
    }
}
