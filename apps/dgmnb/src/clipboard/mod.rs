//! The clipboard: copy text or images (the graph) and paste text, on
//! Wayland or X11, behind one interface.

mod pipe;
mod wayland;
mod x11;

use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use winit::window::Window;

/// Largest clipboard text accepted when pasting (bigger is rejected, not
/// truncated).
pub const MAX_PASTE: usize = 1 << 20;

pub enum Clipboard {
    Wayland(wayland::Clipboard),
    X11(Box<x11::Clipboard>),
}

impl Clipboard {
    /// The clipboard for `window`'s display server.
    ///
    /// # Safety
    /// On Wayland the clipboard borrows winit's display connection: call
    /// [`Clipboard::shutdown`] (or drop it) while the event loop is still
    /// alive, e.g. from `ApplicationHandler::exiting`.
    pub unsafe fn for_window(window: &Window) -> Option<Clipboard> {
        match window.display_handle().ok()?.as_raw() {
            // SAFETY: forwarded from the caller.
            RawDisplayHandle::Wayland(h) => unsafe {
                wayland::Clipboard::new(h.display.as_ptr()).map(Clipboard::Wayland)
            },
            RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_) => {
                x11::Clipboard::new().map(|c| Clipboard::X11(Box::new(c)))
            }
            _ => None,
        }
    }

    pub fn copy_text(&self, text: &str) {
        match self {
            Clipboard::Wayland(c) => c.copy_text(text),
            Clipboard::X11(c) => c.copy_text(text),
        }
    }

    pub fn copy_png(&self, png: Vec<u8>) {
        match self {
            Clipboard::Wayland(c) => c.copy_png(png),
            Clipboard::X11(c) => c.copy_png(png),
        }
    }

    /// Clipboard text, if there is any (waits up to a second for its owner).
    pub fn paste_text(&self) -> Option<String> {
        match self {
            Clipboard::Wayland(c) => c.paste_text(),
            Clipboard::X11(c) => c.paste_text(),
        }
    }

    /// Stop the backend's worker (and, on Wayland, release everything it
    /// holds on the display) before the display goes away.
    pub fn shutdown(&mut self) {
        if let Clipboard::Wayland(c) = self {
            c.shutdown();
        }
    }
}
