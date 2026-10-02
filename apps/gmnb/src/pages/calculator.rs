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
use crate::widgets::display::{Change, Display};
use crate::widgets::icon::{PathIcon, icon_toggle, paths};
use crate::widgets::keypad::Keypad;
use crate::widgets::width_bin::WidthBin;
use appcore::KeyPress;
use appcore::input::{self, Action};
use appcore::keys::{self, KEY_HYP, KEY_SECOND, KEY_TRIG_SECOND};
use appcore::modes::ViewMode;

/// Width at which history/memory docks beside the keypad.
const WIDE_PX: i32 = 620;

pub struct CalculatorPage {
    ctx: Rc<Ctx>,
    vm: RefCell<CalculatorViewModel>,
    root: WidthBin,
    display: Display,
    modes: gtk::Stack,
    keypads: RefCell<HashMap<CalcMode, Keypad>>,
    flyouts: RefCell<Vec<Keypad>>,
    trig_pad: RefCell<Option<Keypad>>,
    mem_buttons: RefCell<Vec<(B, gtk::Button)>>,
    mem_toggles: RefCell<Vec<gtk::Button>>,
    angle_btn: RefCell<Option<gtk::Button>>,
    fe_btn: RefCell<Option<gtk::ToggleButton>>,
    second: Cell<bool>,
    trig_inv: Cell<bool>,
    hyp: Cell<bool>,
    radix_rows: RefCell<Vec<(Radix, gtk::Button, gtk::Label)>>,
    word_btn: RefCell<Option<gtk::Button>>,
    prog_stack: RefCell<Option<gtk::Stack>>,
    bitflip: RefCell<Option<Rc<BitFlip>>>,
    shift_radios: RefCell<Vec<(ShiftMode, gtk::CheckButton)>>,
    panel: Rc<CalcPanel>,
    side: gtk::Box,
    sheet: adw::BottomSheet,
    sheet_holder: gtk::Box,
    history_btn: gtk::ToggleButton,
    wide: Cell<bool>,
    /// Set while pushing VM state into widgets, so their change signals
    /// don't feed back into the VM.
    syncing: Cell<bool>,
    programmer: Cell<bool>,
    compact_btn: gtk::ToggleButton,
    compact: Cell<bool>,
    mem_rows: RefCell<Vec<gtk::Box>>,
}

fn text_button(label: &str, tip: &str) -> gtk::Button {
    let b = gtk::Button::with_label(label);
    b.add_css_class("wc-mem");
    b.set_focus_on_click(false);
    b.set_tooltip_text(Some(tip));
    b
}

fn chevron_menu(label: &str, icon: &str, popover: &gtk::Popover) -> gtk::MenuButton {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.append(&PathIcon::new(icon, 16));
    content.append(&gtk::Label::new(Some(label)));
    content.append(&PathIcon::new(paths::CHEVRON_DOWN, 12));
    gtk::MenuButton::builder()
        .child(&content)
        .popover(popover)
        .css_classes(["wc-mem", "wc-flyout-button"])
        .focus_on_click(false)
        .build()
}

impl CalculatorPage {
    pub fn new(ctx: Rc<Ctx>) -> Rc<Self> {
        let display = Display::new(64.0);
        display.set_vexpand(true);
        display.set_valign(gtk::Align::End);

        let modes = gtk::Stack::new();
        modes.set_transition_type(gtk::StackTransitionType::Crossfade);
        modes.set_transition_duration(180);
        modes.set_vhomogeneous(false);
        modes.set_interpolate_size(true);

        let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
        column.set_margin_start(8);
        column.set_margin_end(8);
        column.set_margin_bottom(8);
        column.append(&display);
        column.append(&modes);

        let panel = CalcPanel::new();
        let side = gtk::Box::new(gtk::Orientation::Vertical, 0);
        side.add_css_class("wc-glass-panel");
        side.add_css_class("wc-side-panel");
        side.set_width_request(300);
        side.set_margin_end(10);
        side.set_margin_bottom(10);
        side.set_visible(false);

        let sheet_holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sheet_holder.set_height_request(360);
        sheet_holder.add_css_class("wc-sheet");
        let sheet = adw::BottomSheet::builder()
            .content(&column)
            .sheet(&sheet_holder)
            .show_drag_handle(true)
            .build();
        sheet_holder.append(&panel.root);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        sheet.set_hexpand(true);
        row.append(&sheet);
        row.append(&side);

        let root = WidthBin::new(&row);
        let history_btn = icon_toggle(paths::HISTORY, "History (Ctrl+H)");
        history_btn
            .bind_property("active", &sheet, "open")
            .bidirectional()
            .sync_create()
            .build();

        let page = Rc::new(CalculatorPage {
            ctx: ctx.clone(),
            vm: RefCell::new(CalculatorViewModel::new()),
            root: root.clone(),
            display: display.clone(),
            modes: modes.clone(),
            keypads: RefCell::default(),
            flyouts: RefCell::default(),
            trig_pad: RefCell::default(),
            mem_buttons: RefCell::default(),
            mem_toggles: RefCell::default(),
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
            sheet,
            sheet_holder,
            history_btn,
            wide: Cell::new(false),
            syncing: Cell::new(false),
            programmer: Cell::new(false),
            compact_btn: icon_toggle(paths::KEEP_ON_TOP, "Keep on top (Alt+Up)"),
            compact: Cell::new(false),
            mem_rows: RefCell::default(),
        });
        {
            let weak = Rc::downgrade(&page);
            page.compact_btn.connect_toggled(move |b| {
                if let Some(p) = weak.upgrade() {
                    p.set_compact(b.is_active());
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

        {
            let weak = Rc::downgrade(&page);
            ctx.hub.subscribe(move |s| {
                if let Some(p) = weak.upgrade() {
                    p.display.set_scheme(*s);
                    for k in p.keypads.borrow().values() {
                        k.set_scheme(*s);
                    }
                    for k in p.flyouts.borrow().iter() {
                        k.set_scheme(*s);
                    }
                }
            });
        }

        if let Some(state) = ctx
            .store
            .page_state("calculator")
            .and_then(|v| v.as_str().map(String::from))
        {
            page.vm.borrow_mut().restore_state(&state);
        }
        page.sync_all(Change::None);
        page
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
        pad.set_scheme(self.ctx.hub.scheme());
        pad
    }

    fn memory_row(self: &Rc<Self>, full: bool) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        row.set_homogeneous(full);
        let all: &[(B, &str, &str)] = &[
            (B::MemoryClear, "MC", "Clear all memory (Ctrl+L)"),
            (B::MemoryRecall, "MR", "Memory recall (Ctrl+R)"),
            (B::MemoryAdd, "M+", "Memory add (Ctrl+P)"),
            (B::MemorySubtract, "M−", "Memory subtract (Ctrl+Q)"),
            (B::Memory, "MS", "Memory store (Ctrl+M)"),
        ];
        for (b, label, tip) in all {
            if !full && *b != B::Memory {
                continue;
            }
            let button = text_button(label, tip);
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
        self.mem_toggles.borrow_mut().push(toggle);
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
        self.side.set_visible(!on && self.wide.get());
        self.display.set_max_size(if on { 44.0 } else { 64.0 });
        if let Some(pad) = self.keypads.borrow().get(&CalcMode::Standard) {
            pad.set_spacing(if on { 3 } else { 5 });
        }
        self.ctx.set_compact(on);
    }

    fn build_standard(self: &Rc<Self>) -> gtk::Widget {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
        b.append(&self.memory_row(true));
        let pad = self.add_keypad(keys::standard(), Some(CalcMode::Standard));
        pad.set_vexpand(true);
        pad.set_size_request(-1, 300);
        b.append(&pad);
        b.upcast()
    }

    fn build_scientific(self: &Rc<Self>) -> gtk::Widget {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 4);

        let top = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        let angle = text_button("DEG", "Switch angle unit (F3/F4/F5)");
        let weak = Rc::downgrade(self);
        angle.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                let next = p.vm.borrow().angle_unit().next();
                p.set_angle(next);
            }
        });
        let fe = gtk::ToggleButton::with_label("F-E");
        fe.add_css_class("wc-mem");
        fe.set_focus_on_click(false);
        fe.set_tooltip_text(Some("Scientific notation (V)"));
        let weak = Rc::downgrade(self);
        fe.connect_clicked(move |w| {
            if let Some(p) = weak.upgrade()
                && w.is_active() != p.vm.borrow().is_fe()
            {
                p.press_from(w, B::FToE);
            }
        });
        top.append(&angle);
        top.append(&fe);
        self.angle_btn.replace(Some(angle));
        self.fe_btn.replace(Some(fe));
        b.append(&top);
        b.append(&self.memory_row(true));

        // Flyouts.
        let trig = self.add_keypad(keys::trig(), None);
        trig.set_size_request(300, 104);
        self.trig_pad.replace(Some(trig.clone()));
        let trig_pop = gtk::Popover::builder()
            .child(&trig)
            .css_classes(["wc-flyout"])
            .build();
        let func = self.add_keypad(keys::functions(), None);
        func.set_size_request(240, 104);
        let func_pop = gtk::Popover::builder()
            .child(&func)
            .css_classes(["wc-flyout"])
            .build();
        let flyrow = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        flyrow.append(&chevron_menu("Trigonometry", paths::ANGLE, &trig_pop));
        flyrow.append(&chevron_menu("Function", paths::FUNCTION, &func_pop));
        b.append(&flyrow);

        let pad = self.add_keypad(keys::scientific(), Some(CalcMode::Scientific));
        pad.set_vexpand(true);
        pad.set_size_request(-1, 330);
        b.append(&pad);
        b.upcast()
    }

    fn build_programmer(self: &Rc<Self>) -> gtk::Widget {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 4);

        let radix_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        radix_box.add_css_class("wc-radix-list");
        for radix in Radix::ALL {
            let name = gtk::Label::new(Some(radix.label()));
            name.add_css_class("wc-radix-name");
            name.set_width_chars(4);
            name.set_xalign(0.0);
            let value = gtk::Label::new(Some("0"));
            value.add_css_class("wc-radix-value");
            value.set_xalign(0.0);
            value.set_hexpand(true);
            value.set_ellipsize(gtk::pango::EllipsizeMode::Start);
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            content.append(&name);
            content.append(&value);
            let button = gtk::Button::builder()
                .child(&content)
                .css_classes(["wc-radix-row", "flat"])
                .focus_on_click(false)
                .build();
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

        // Toolbar: keypad kind, word size, memory.
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        let full = icon_toggle(paths::KEYBOARD, "Full keypad");
        let bits = icon_toggle(paths::BITS, "Bit toggling keypad");
        bits.set_group(Some(&full));
        full.set_active(true);
        for t in [&full, &bits] {
            t.add_css_class("wc-mem");
        }
        let word = text_button("QWORD", "Word size (F2/F3/F4/F12)");
        let weak = Rc::downgrade(self);
        word.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                let next = p.vm.borrow().word_size().next();
                p.set_word(next);
            }
        });
        bar.append(&full);
        bar.append(&bits);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        bar.append(&spacer);
        bar.append(&word);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        bar.append(&spacer);
        bar.append(&self.memory_row(false));
        self.word_btn.replace(Some(word));
        b.append(&bar);

        // Bitwise / shift flyouts.
        let bitwise = self.add_keypad(keys::bitwise(), None);
        bitwise.set_size_request(260, 104);
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
                }
            });
            shift_box.append(&c);
            self.shift_radios.borrow_mut().push((mode, c));
        }
        let shift_pop = gtk::Popover::builder()
            .child(&shift_box)
            .css_classes(["wc-flyout"])
            .build();
        let flyrow = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        flyrow.append(&chevron_menu("Bitwise", paths::BITS, &bw_pop));
        flyrow.append(&chevron_menu("Bit shift", paths::SWAP, &shift_pop));
        b.append(&flyrow);

        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::SlideUpDown);
        stack.set_transition_duration(240);
        let pad = self.add_keypad(keys::programmer(), Some(CalcMode::Programmer));
        pad.set_size_request(-1, 290);
        stack.add_named(&pad, Some("full"));
        let flip = BitFlip::new();
        let weak = Rc::downgrade(self);
        flip.connect_flip(move |i| {
            if let Some(p) = weak.upgrade() {
                p.vm.borrow_mut().flip_bit(i);
                p.sync(Some(Change::Typing));
            }
        });
        stack.add_named(&flip.root, Some("bits"));
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
        self.panel.connect_memory_op(move |i, op| {
            if let Some(p) = weak.upgrade() {
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
        self.side.set_visible(wide && !self.compact.get());
        self.history_btn
            .set_visible(!wide && !self.programmer.get() && !self.compact.get());
        for t in self.mem_toggles.borrow().iter() {
            t.set_visible(!wide);
        }
    }

    // ------------------------------------------------------------- input

    fn press_from(self: &Rc<Self>, widget: &impl IsA<gtk::Widget>, b: B) {
        let (w, h) = (widget.width() as f32, widget.height() as f32);
        let s = self.ctx.hub.scheme();
        self.ctx.pulse_at(widget, w / 2.0, h / 2.0, s.accent, 0.5);
        self.vm.borrow_mut().press(b);
        self.sync(None);
    }

    fn on_key(self: &Rc<Self>, pad: &Keypad, id: u32, x: f32, y: f32) {
        let s = self.ctx.hub.scheme();
        let hot = id == B::Equals.id();
        let color = if hot {
            s.hot_a
        } else {
            s.blobs[(id as usize * 7) % 4]
        };
        self.ctx
            .pulse_at(pad, x, y, color, if hot { 1.4 } else { 0.55 });
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
        self.press_resolved(button);
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
        // Upstream: using a trig function or the 2nd variant resets the toggles.
        if keys::TRIG.iter().any(|t| [t.1, t.2, t.3].contains(&b)) {
            self.trig_inv.set(false);
            self.hyp.set(false);
            self.relabel();
        }
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
                trig.set_key(
                    t.0.id(),
                    &keys::trig_label(t.4, inv, hyp),
                    &keys::trig_tip(t.4, inv, hyp),
                );
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
        self.panel.set_history(&vm.history());
        self.panel.set_memory(&vm.memory());
        drop(vm);
        self.relabel();
        self.sync(Some(change));
    }

    /// Pull state from the VM into the widgets. `force` overrides the change
    /// kind derived from VM events.
    fn sync(self: &Rc<Self>, force: Option<Change>) {
        let events = self.vm.borrow_mut().take_events();
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

        if events.contains(&Event::HistoryChanged) {
            self.panel.set_history(&vm.history());
        }
        if events.contains(&Event::MemoryChanged) {
            self.panel.set_memory(&vm.memory());
        }
        for (b, w) in self.mem_buttons.borrow().iter() {
            w.set_sensitive(vm.is_enabled(*b) && !vm.is_error());
        }
        let has_memory = !vm.memory().is_empty();
        for t in self.mem_toggles.borrow().iter() {
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
            pad.set_key_label(
                B::OpenParenthesis.id(),
                &if n > 0 {
                    format!("(<sub><small>{n}</small></sub>")
                } else {
                    "(".to_string()
                },
            );
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
                    a.set_label(vm.angle_unit().label());
                }
                if let Some(fe) = self.fe_btn.borrow().as_ref() {
                    fe.set_active(vm.is_fe());
                }
            }
            CalcMode::Programmer => {
                for (radix, button, label) in self.radix_rows.borrow().iter() {
                    label.set_text(&vm.radix_value(*radix));
                    if *radix == vm.radix() {
                        button.add_css_class("wc-radix-active");
                    } else {
                        button.remove_css_class("wc-radix-active");
                    }
                    button.update_property(&[gtk::accessible::Property::Label(&format!(
                        "{} {}",
                        radix.label(),
                        vm.radix_value(*radix)
                    ))]);
                }
                if let Some(w) = self.word_btn.borrow().as_ref() {
                    w.set_label(vm.word_size().label());
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

        if matches!(change, Change::Result | Change::Error) {
            // Upstream NarratorNotifier: announce computed results.
            let text = format!("Display is {}", self.display.value());
            self.display
                .announce(&text, gtk::AccessibleAnnouncementPriority::Medium);
            let s = self.ctx.hub.scheme();
            let (w, h) = (self.display.width() as f32, self.display.height() as f32);
            let color = if change == Change::Error {
                s.hot_a
            } else {
                s.hot_b
            };
            self.ctx
                .pulse_at(&self.display, w * 0.8, h * 0.65, color, 1.1);
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
            && let Some((x, y)) = pad.flash(id)
        {
            let s = self.ctx.hub.scheme();
            self.ctx.pulse_at(
                &pad,
                x,
                y,
                if b == B::Equals { s.hot_a } else { s.accent },
                0.6,
            );
        }
    }
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
        self.vm.borrow_mut().set_mode(calc_mode);
        let programmer = calc_mode == CalcMode::Programmer;
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
        self.display.set_max_size(match calc_mode {
            CalcMode::Standard => 64.0,
            CalcMode::Scientific => 54.0,
            CalcMode::Programmer => 46.0,
        });
        if let Some(pad) = self.keypads.borrow().get(&calc_mode) {
            pad.cascade();
        }
    }

    fn copy_value(&self) -> String {
        let text = self.vm.borrow().copy_text();
        self.ctx.copy_to_clipboard(&text);
        text
    }

    fn save_state(&self) {
        let state = self.vm.borrow().save_state();
        self.ctx
            .store
            .set_page_state("calculator", serde_json::Value::String(state));
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
            Action::ToggleHistory => {
                if p.wide.get() {
                    p.panel.show_tab("history");
                } else {
                    p.panel.show_tab("history");
                    p.sheet.set_open(!p.sheet.is_open());
                }
            }
            Action::ClearHistory => {
                p.vm.borrow_mut().history_clear();
                p.sync(None);
            }
            Action::Angle(u) => p.set_angle(u),
            Action::Radix(r) => p.set_radix(r),
            Action::Word(w) => p.set_word(w),
        }
        true
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
