//! Programmer-mode bit toggling keypad (upstream CalculatorProgrammerBitFlipPanel):
//! 64 bits in four rows of four nibbles, MSB first. Set bits glow.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

/// Called with the index of the bit the user toggled.
type FlipFn = Box<dyn Fn(u32)>;

pub struct BitFlip {
    pub root: gtk::Box,
    bits: Vec<gtk::ToggleButton>,
    on_flip: RefCell<Option<FlipFn>>,
    syncing: std::cell::Cell<bool>,
}

impl BitFlip {
    pub fn new() -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
        root.add_css_class("wc-bitflip");
        root.set_valign(gtk::Align::Center);
        let mut bits: Vec<Option<gtk::ToggleButton>> = vec![None; 64];
        for row in 0..4 {
            let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            row_box.set_homogeneous(true);
            for nibble in 0..4 {
                let col = gtk::Box::new(gtk::Orientation::Vertical, 2);
                let nb = gtk::Box::new(gtk::Orientation::Horizontal, 2);
                nb.set_homogeneous(true);
                let top_bit = 63 - (row * 16 + nibble * 4);
                for j in 0..4 {
                    let bit = top_bit - j;
                    let b = gtk::ToggleButton::with_label("0");
                    b.add_css_class("wc-bit");
                    b.set_focus_on_click(false);
                    b.set_tooltip_text(Some(&format!("Bit {bit}")));
                    b.update_property(&[gtk::accessible::Property::Label(&format!("Bit {bit}"))]);
                    nb.append(&b);
                    bits[bit as usize] = Some(b);
                }
                let label = gtk::Label::new(Some(&top_bit.to_string()));
                label.add_css_class("wc-bit-index");
                label.set_xalign(0.0);
                col.append(&nb);
                col.append(&label);
                row_box.append(&col);
            }
            root.append(&row_box);
        }
        let bits: Vec<gtk::ToggleButton> = bits
            .into_iter()
            .map(|b| b.expect("all bits built"))
            .collect();
        let this = Rc::new(BitFlip {
            root,
            bits,
            on_flip: RefCell::new(None),
            syncing: Default::default(),
        });
        for (i, b) in this.bits.iter().enumerate() {
            let weak = Rc::downgrade(&this);
            b.connect_toggled(move |_| {
                if let Some(t) = weak.upgrade()
                    && !t.syncing.get()
                    && let Some(f) = t.on_flip.borrow().as_ref()
                {
                    f(i as u32);
                }
            });
        }
        this
    }

    pub fn connect_flip(&self, f: impl Fn(u32) + 'static) {
        self.on_flip.replace(Some(Box::new(f)));
    }

    /// Reflect `bit(i)` for every bit and enable only those inside `width`.
    pub fn sync(&self, bit: impl Fn(u32) -> bool, width: u32) {
        self.syncing.set(true);
        for (i, b) in self.bits.iter().enumerate() {
            let i = i as u32;
            let on = i < width && bit(i);
            b.set_active(on);
            b.set_label(if on { "1" } else { "0" });
            b.set_sensitive(i < width);
        }
        self.syncing.set(false);
    }
}
