//! Pages hosted in the main window's stack.

pub mod calculator;
pub mod converter;
pub mod date;
pub mod graphing;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::gdk;
use gtk::graphene;
use gtk::prelude::*;

use crate::settings::Store;
use crate::theme::Hub;
use crate::widgets::aurora::Aurora;
use appcore::KeyPress;
use appcore::modes::ViewMode;

/// Enter/leave the compact window chrome.
pub type CompactHook = Box<dyn Fn(bool)>;

/// Give a page's flyout the window's key routing and pointer handling.
pub type FlyoutHook = Box<dyn Fn(&gtk::Popover)>;

/// A setting pages follow live: Settings sets it, a page reads it when it
/// is made and is told of every change after.
pub struct Followed<T> {
    value: Cell<T>,
    followers: RefCell<Vec<Follower<T>>>,
}

type Follower<T> = Box<dyn Fn(T)>;

impl<T: Copy + PartialEq> Followed<T> {
    pub fn new(value: T) -> Self {
        Followed {
            value: Cell::new(value),
            followers: RefCell::default(),
        }
    }

    pub fn get(&self) -> T {
        self.value.get()
    }

    /// Changes it, telling every follower if it changed.
    pub fn set(&self, value: T) {
        if self.value.replace(value) != value {
            for f in self.followers.borrow().iter() {
                f(value);
            }
        }
    }

    pub fn follow(&self, f: impl Fn(T) + 'static) {
        self.followers.borrow_mut().push(Box::new(f));
    }
}

/// Services every page can use.
pub struct Ctx {
    pub hub: Rc<Hub>,
    pub aurora: Aurora,
    pub toasts: adw::ToastOverlay,
    pub store: Rc<Store>,
    /// Set by the window: enter/leave the compact "keep on top" chrome.
    pub compact: std::cell::RefCell<Option<CompactHook>>,
    /// Set by the window: what a flyout of a page's own keys needs
    /// ([`Ctx::flyout`]).
    pub flyouts: std::cell::RefCell<Option<FlyoutHook>>,
    /// What covers what in the window, for assistive technology; a page
    /// registers the layers it opens over itself.
    pub layers: Rc<crate::inert::Layers>,
    /// Graphing's number precision (Settings).
    pub precision: Followed<appcore::graph::NumberPrecision>,
}

impl Ctx {
    /// Bloom a pulse in the background at a point given in `widget` coords.
    pub fn pulse_at(
        &self,
        widget: &impl IsA<gtk::Widget>,
        x: f32,
        y: f32,
        color: [f32; 3],
        strength: f32,
    ) {
        if let Some(p) = widget.compute_point(&self.aurora, &graphene::Point::new(x, y)) {
            self.aurora.pulse(p.x(), p.y(), color, strength);
        }
    }

    /// A flyout of the page's own keys, an autohide popover, which holds
    /// a grab: its keys go the window's way (Enter on a key is "=", what
    /// is typed is the calculator's), and a click in it draws no focus
    /// ring, as in the window.
    pub fn flyout(&self, popover: &gtk::Popover) {
        if let Some(f) = self.flyouts.borrow().as_ref() {
            f(popover);
        }
    }

    pub fn set_compact(&self, on: bool) {
        if let Some(f) = self.compact.borrow().as_ref() {
            f(on);
        }
    }

    pub fn toast(&self, text: &str) {
        let toast = adw::Toast::new(text);
        toast.set_timeout(2);
        self.toasts.add_toast(toast);
    }

    pub fn copy_to_clipboard(&self, text: &str) {
        if let Some(display) = gdk::Display::default() {
            display.clipboard().set_text(text);
            self.toast("Copied to clipboard");
        }
    }
}

pub trait Page {
    fn widget(&self) -> gtk::Widget;

    /// The page is being shown for `mode` (several modes can share a page).
    fn activate(&self, mode: ViewMode);

    /// The window is switching to a different page.
    fn deactivate(&self) {}

    /// Header widgets to show at the end of the title bar while active.
    fn header_end(&self) -> Vec<gtk::Widget> {
        Vec::new()
    }

    /// What the page's keys and pastes change: the calculation, which a
    /// layer of the page's own can cover (the calculator's History sheet
    /// covers its display and keypad, not the page). The page by default.
    /// While it is covered ([`crate::inert::Layers::covers`]) only
    /// [`Page::layer_key_pressed`] gets keys, and nothing is pasted.
    fn target(&self) -> gtk::Widget {
        self.widget()
    }

    /// Keyboard input; return true if handled.
    fn key_pressed(&self, _kp: &KeyPress) -> bool {
        false
    }

    /// Whether `widget` is one of the page's own keys, upstream's
    /// `CalculatorButton`s (the calculator's, the converter's and the
    /// graph's keypads) and bit `FlipButtons`, which ignore Enter when
    /// focused: it is [`Page::key_pressed`]'s ("=" on the calculator, or
    /// nothing); any other focused control activates on Enter itself
    /// (`crate::window::focus_takes`, `crate::window::focus_ignores`).
    fn is_calculator_key(&self, _widget: &gtk::Widget) -> bool {
        false
    }

    /// Keyboard input while a layer of the page's own covers
    /// [`Page::target`]: only that layer's keys (the History sheet's
    /// Ctrl+H and Ctrl+Shift+D); return true if handled.
    fn layer_key_pressed(&self, _kp: &KeyPress) -> bool {
        false
    }

    fn copy(&self) -> Option<String> {
        None
    }

    fn paste(&self, _text: &str) {}

    /// Persist page state into the store (called on close).
    fn save(&self) {}
}
