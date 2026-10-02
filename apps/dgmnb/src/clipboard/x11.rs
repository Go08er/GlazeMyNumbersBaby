//! X11 clipboard (the CLIPBOARD selection) via x11-clipboard, which keeps
//! its own X connections and serving thread; dropping it stops both.

use std::time::Duration;

use x11_clipboard::Atom;

use super::MAX_PASTE;

pub struct Clipboard {
    inner: x11_clipboard::Clipboard,
    png: Atom,
}

impl Clipboard {
    pub fn new() -> Option<Clipboard> {
        let inner = x11_clipboard::Clipboard::new().ok()?;
        let png = inner.setter.get_atom("image/png").ok()?;
        Some(Clipboard { inner, png })
    }

    pub fn copy_text(&self, text: &str) {
        let a = &self.inner.setter.atoms;
        let _ = self
            .inner
            .store(a.clipboard, a.utf8_string, text.as_bytes());
    }

    pub fn copy_png(&self, png: Vec<u8>) {
        let a = &self.inner.setter.atoms;
        let _ = self.inner.store(a.clipboard, self.png, png);
    }

    pub fn paste_text(&self) -> Option<String> {
        let a = &self.inner.getter.atoms;
        let bytes = self
            .inner
            .load(
                a.clipboard,
                a.utf8_string,
                a.property,
                Duration::from_secs(1),
            )
            .ok()?;
        (bytes.len() <= MAX_PASTE).then(|| String::from_utf8_lossy(&bytes).replace("\r\n", "\n"))
    }
}
