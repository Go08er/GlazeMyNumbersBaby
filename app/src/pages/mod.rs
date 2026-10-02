//! Pages hosted in the main window's stack.

pub mod calc_keys;
pub mod calculator;
pub mod converter;
pub mod date;
pub mod graphing;

use std::rc::Rc;

use gtk::gdk;
use gtk::graphene;
use gtk::prelude::*;

use crate::modes::ViewMode;
use crate::settings::Store;
use crate::theme::Hub;
use crate::widgets::aurora::Aurora;

/// Services every page can use.
pub struct Ctx {
    pub hub: Rc<Hub>,
    pub aurora: Aurora,
    pub toasts: adw::ToastOverlay,
    pub store: Rc<Store>,
    /// Set by the window: enter/leave the compact "keep on top" chrome.
    pub compact: std::cell::RefCell<Option<Box<dyn Fn(bool)>>>,
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

    /// Header widgets to show at the end of the title bar while active.
    fn header_end(&self) -> Vec<gtk::Widget> {
        Vec::new()
    }

    /// Keyboard input; return true if handled.
    fn key_pressed(&self, _key: gdk::Key, _mods: gdk::ModifierType) -> bool {
        false
    }

    fn copy(&self) -> Option<String> {
        None
    }

    fn paste(&self, _text: &str) {}

    /// Persist page state into the store (called on close).
    fn save(&self) {}
}
