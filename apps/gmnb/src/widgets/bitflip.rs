//! Programmer-mode bit toggling keypad (upstream CalculatorProgrammerBitFlipPanel):
//! 64 bits in four rows of four nibbles, MSB first, laid out as DGMNB's
//! (`calc.rs` `bit_flip`): each row a quarter of the grid, each nibble its
//! four bits over the index of its first, the bits `BIT_INSET_Y` in from
//! the top and bottom of their room. DGMNB's cells fall on fractions of a
//! pixel; the grid places its buttons there, and their labels' baselines
//! on the whole pixel DGMNB's land on.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use appcore::metrics::programmer as mpg;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

/// Called with the index of the bit the user toggled.
type FlipFn = Box<dyn Fn(u32)>;

/// Half the bundled Inter's capitals, in em: DGMNB puts a line's baseline
/// that far below its place's middle.
const HALF_CAP_EM: f32 = 1490.0 / 2048.0 / 2.0;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct BitGrid {
        /// By bit, 0 to 63.
        pub bits: RefCell<Vec<gtk::ToggleButton>>,
        /// Each nibble's index, row by row.
        pub indexes: RefCell<Vec<gtk::Label>>,
        /// The bits' and the indexes' text sizes (px), from the style.
        pub sizes: Cell<(f32, f32)>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BitGrid {
        const NAME: &'static str = "GmnbBitGrid";
        type Type = super::BitGrid;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for BitGrid {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for BitGrid {
        /// No least size of its own: DGMNB's bits take whatever room the
        /// keypad's place has (its least window is 300 × 400), the bits
        /// and their indexes shrinking with it. What it would like is room
        /// for each part's own least size.
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let mut most = 0;
            let mut c = self.obj().first_child();
            while let Some(child) = c {
                most = most.max(child.measure(orientation, -1).0);
                c = child.next_sibling();
            }
            let n = if orientation == gtk::Orientation::Horizontal {
                16
            } else {
                8
            };
            (0, most * n, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let (bit_size, index_size) = self.sizes.get();
            let bits = self.bits.borrow();
            let indexes = self.indexes.borrow();
            let top = mpg::BITS_PAD_Y;
            let row_h = (height as f32 - 2.0 * top) / mpg::BITS_ROWS;
            let nibble_w = (width as f32 - 3.0 * mpg::NIBBLE_GAP) / 4.0;
            let bit_w = (nibble_w - 3.0 * mpg::BIT_GAP) / 4.0;
            for row in 0..4 {
                let y = top + row as f32 * row_h;
                for nibble in 0..4 {
                    let x = nibble as f32 * (nibble_w + mpg::NIBBLE_GAP);
                    let index_y = y + row_h - mpg::BIT_INDEX_H;
                    if let Some(label) = indexes.get(row * 4 + nibble) {
                        place(
                            label,
                            x,
                            index_y,
                            nibble_w,
                            mpg::BIT_INDEX_H,
                            index_size,
                            false,
                        );
                    }
                    let top_bit = 63 - (row * 16 + nibble * 4);
                    for j in 0..4 {
                        let bit = top_bit - j;
                        let bx = x + j as f32 * (bit_w + mpg::BIT_GAP);
                        let by = y + mpg::BIT_INSET_Y;
                        let bh = row_h - mpg::BIT_INDEX_H - 2.0 * mpg::BIT_INSET_Y;
                        if let Some(b) = bits.get(bit) {
                            place(b, bx, by, bit_w, bh, bit_size, true);
                        }
                    }
                }
            }
        }
    }

    /// Allocates `w` DGMNB's cell (`x`, `y`, `cw`, `ch`, fractional): a
    /// whole number of pixels wide, centred on the cell's middle (or at
    /// its start, `centre` false) to the fraction; an even number high, at
    /// the whole pixel that puts its text's baseline (text `size` px,
    /// centred in it: css.rs `text_line`) where DGMNB rounds it to.
    fn place(w: &impl IsA<gtk::Widget>, x: f32, y: f32, cw: f32, ch: f32, size: f32, centre: bool) {
        let w = w.as_ref();
        let width = (cw.round() as i32).max(w.measure(gtk::Orientation::Horizontal, -1).0);
        let height = ((ch / 2.0).round() as i32 * 2).max(0);
        let at_x = if centre {
            x + (cw - width as f32) / 2.0
        } else {
            x
        };
        let half_cap = size * HALF_CAP_EM;
        let baseline = (y + ch / 2.0 + half_cap).round();
        let at_y = baseline - half_cap.round() - (height / 2) as f32;
        let t = gtk::gsk::Transform::new().translate(&gtk::graphene::Point::new(at_x, at_y));
        w.allocate(width, height, -1, Some(t));
    }
}

glib::wrapper! {
    pub struct BitGrid(ObjectSubclass<imp::BitGrid>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

pub struct BitFlip {
    pub root: BitGrid,
    bits: Vec<gtk::ToggleButton>,
    on_flip: RefCell<Option<FlipFn>>,
    syncing: std::cell::Cell<bool>,
}

impl BitFlip {
    pub fn new(hub: &crate::theme::Hub) -> Rc<Self> {
        let root: BitGrid = glib::Object::new();
        root.add_css_class("wc-bitflip");
        root.add_css_class("wc-plain");
        let mut bits: Vec<Option<gtk::ToggleButton>> = vec![None; 64];
        let mut indexes = Vec::with_capacity(16);
        // In reading order, as DGMNB's: each nibble's bits, then its index.
        for row in 0..4 {
            for nibble in 0..4 {
                let top_bit = 63 - (row * 16 + nibble * 4);
                for j in 0..4 {
                    let bit = top_bit - j;
                    let b = gtk::ToggleButton::with_label("0");
                    b.add_css_class("wc-bit");
                    b.set_tooltip_text(Some(&format!("Bit {bit}")));
                    // Named by which bit it is (as DGMNB's, and upstream's
                    // "%1, value %2" puts the position first); its value,
                    // shown, is the toggle's pressed state.
                    crate::a11y::name_button(b.upcast_ref(), &format!("Bit {bit}"));
                    b.set_parent(&root);
                    bits[bit as usize] = Some(b);
                }
                let label = gtk::Label::new(Some(&top_bit.to_string()));
                label.add_css_class("wc-bit-index");
                label.set_xalign(0.0);
                label.set_parent(&root);
                indexes.push(label);
            }
        }
        let bits: Vec<gtk::ToggleButton> = bits
            .into_iter()
            .map(|b| b.expect("all bits built"))
            .collect();
        root.imp().bits.replace(bits.clone());
        root.imp().indexes.replace(indexes);
        hub.subscribe_style(&root, |root, style| {
            let size = |path: &str| match style.get(path) {
                Some(gmnb_theme::types::Value::Text(t)) => t.font.size,
                _ => style.control(path).map_or(16.0, |c| c.font.size),
            };
            root.imp()
                .sizes
                .set((size("programmer.bit"), size("programmer.bit_index")));
            root.queue_allocate();
        });
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
            let shown = if on { "1" } else { "0" };
            if b.label().as_deref() != Some(shown) {
                b.set_label(shown);
                crate::window::exact_text(b);
                // A new label names the button again (labelled-by, which
                // outranks the name given): the name stays "Bit N"
                // (R17-L-02), as the calendar's days keep theirs.
                b.reset_relation(gtk::AccessibleRelation::LabelledBy);
            }
            b.set_sensitive(i < width);
        }
        self.syncing.set(false);
    }
}
