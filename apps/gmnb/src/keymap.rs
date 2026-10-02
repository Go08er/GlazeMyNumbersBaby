//! GDK key events → toolkit-neutral [`KeyPress`] (see `appcore::input`).

use appcore::{Key, KeyPress, Named};
use gtk::gdk;
use gtk::glib::translate::IntoGlib;

pub fn key_press(key: gdk::Key, mods: gdk::ModifierType) -> Option<KeyPress> {
    use gdk::Key as K;
    let named = match key {
        K::Return | K::KP_Enter | K::ISO_Enter => Some(Named::Enter),
        K::Escape => Some(Named::Escape),
        K::BackSpace => Some(Named::Backspace),
        K::Delete | K::KP_Delete => Some(Named::Delete),
        K::Insert | K::KP_Insert => Some(Named::Insert),
        K::Tab | K::ISO_Left_Tab | K::KP_Tab => Some(Named::Tab),
        K::Home | K::KP_Home => Some(Named::Home),
        K::End | K::KP_End => Some(Named::End),
        K::Page_Up | K::KP_Page_Up => Some(Named::PageUp),
        K::Page_Down | K::KP_Page_Down => Some(Named::PageDown),
        K::Up | K::KP_Up => Some(Named::Up),
        K::Down | K::KP_Down => Some(Named::Down),
        K::Left | K::KP_Left => Some(Named::Left),
        K::Right | K::KP_Right => Some(Named::Right),
        k => {
            let v = k.into_glib();
            // F1…F24 are consecutive keysyms.
            (K::F1.into_glib()..=K::F24.into_glib())
                .contains(&v)
                .then(|| Named::F((v - K::F1.into_glib() + 1) as u8))
        }
    };
    let key_value = match named {
        Some(n) => Key::Named(n),
        None => Key::Char(key.to_unicode().filter(|c| !c.is_control())?),
    };
    Some(KeyPress {
        key: key_value,
        ctrl: mods.contains(gdk::ModifierType::CONTROL_MASK),
        shift: mods.contains(gdk::ModifierType::SHIFT_MASK),
        alt: mods.contains(gdk::ModifierType::ALT_MASK),
        keypad: key.name().is_some_and(|n| n.starts_with("KP_")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gdk_keys_translate() {
        let none = gdk::ModifierType::empty();
        assert_eq!(
            key_press(gdk::Key::Return, none),
            Some(KeyPress::named(Named::Enter))
        );
        assert_eq!(
            key_press(gdk::Key::F9, none),
            Some(KeyPress::named(Named::F(9)))
        );
        assert_eq!(
            key_press(gdk::Key::R, gdk::ModifierType::SHIFT_MASK),
            Some(KeyPress::char('R'))
        );
        let kp = key_press(gdk::Key::KP_Decimal, none).unwrap();
        assert!(kp.keypad && kp.key == Key::Char('.'));
        assert_eq!(
            key_press(gdk::Key::_3, gdk::ModifierType::ALT_MASK),
            Some(KeyPress::char('3').alt())
        );
        assert_eq!(key_press(gdk::Key::Shift_L, none), None);
    }
}
