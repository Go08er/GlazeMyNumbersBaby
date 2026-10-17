//! The clipboard: copy text or images (the graph) and paste text, on
//! Wayland.

mod pipe;
mod wayland;

use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use winit::window::Window;

/// Largest clipboard text accepted when pasting (bigger is rejected, not
/// truncated).
pub const MAX_PASTE: usize = 1 << 20;

pub struct Clipboard(wayland::Clipboard);

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
                wayland::Clipboard::new(h.display.as_ptr()).map(Clipboard)
            },
            _ => None,
        }
    }

    pub fn copy_text(&self, text: &str) {
        self.0.copy_text(text)
    }

    pub fn copy_png(&self, png: Vec<u8>) {
        self.0.copy_png(png)
    }

    /// Clipboard text, if there is any (waits up to a second for its owner).
    pub fn paste_text(&self) -> Option<String> {
        self.0.paste_text()
    }

    /// Stop the worker and release everything it holds on the display
    /// before the display goes away.
    ///
    /// False if the worker didn't stop in time (see
    /// `wayland::Clipboard::shutdown`).
    pub fn shutdown(&mut self) -> bool {
        self.0.shutdown()
    }
}
