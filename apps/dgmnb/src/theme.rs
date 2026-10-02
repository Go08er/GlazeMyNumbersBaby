//! DGMNB's look: flat surfaces in light or dark, one accent colour (the
//! desktop's, if it shares one, else a calm blue), nothing animated.

use appcore::color::{
    self, TEXT_CONTRAST, UI_CONTRAST, contrast, from_hsl, hex, to_hsl, with_contrast,
};

use crate::gfx::Color;

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub dark: bool,
    pub bg: Color,
    /// Raised surfaces: number keys, fields, popups.
    pub surface: Color,
    /// Function keys, the sidebar, cards.
    pub surface2: Color,
    pub border: Color,
    pub fg: Color,
    pub fg_dim: Color,
    pub fg_faint: Color,
    pub accent: Color,
    pub on_accent: Color,
    /// Accent used as text/lines on `bg`.
    pub accent_text: Color,
    pub hover: Color,
    pub press: Color,
    pub danger: Color,
    /// Graph colours.
    pub series: [Color; 6],
}

pub const DEFAULT_ACCENT: [f32; 3] = hex(0x3b6fd8);

impl Theme {
    pub fn new(dark: bool, accent: Option<[f32; 3]>) -> Theme {
        let accent = accent.unwrap_or(DEFAULT_ACCENT);
        let (bg, surface, surface2, border, fg) = if dark {
            (
                hex(0x1d1d20),
                hex(0x2c2c31),
                hex(0x25252a),
                hex(0x3a3a40),
                hex(0xf2f2f5),
            )
        } else {
            (
                hex(0xf6f6f8),
                hex(0xffffff),
                hex(0xececf0),
                hex(0xdcdce2),
                hex(0x1b1b1f),
            )
        };
        // The accent fills the equals key and marks selection; keep it a
        // solid mid-tone so text on it reads, whatever the desktop picked.
        let (h, s, _) = to_hsl(accent);
        let fill = from_hsl(h, s.min(0.85), if dark { 0.62 } else { 0.48 });
        let on_accent = if contrast(color::WHITE, fill) >= contrast(hex(0x101014), fill) {
            color::WHITE
        } else {
            hex(0x101014)
        };
        let fill = with_contrast(fill, on_accent, TEXT_CONTRAST);
        let accent_text = with_contrast(
            with_contrast(accent, bg, TEXT_CONTRAST),
            surface,
            TEXT_CONTRAST,
        );
        let series = [0.0, 180.0, 60.0, 240.0, 120.0, 300.0].map(|dh| {
            let c = from_hsl(h + dh, 0.7, if dark { 0.66 } else { 0.45 });
            Color::rgb(with_contrast(
                with_contrast(c, bg, UI_CONTRAST),
                surface,
                UI_CONTRAST,
            ))
        });
        let fg_c = Color::rgb(fg);
        Theme {
            dark,
            bg: Color::rgb(bg),
            surface: Color::rgb(surface),
            surface2: Color::rgb(surface2),
            border: Color::rgb(border),
            fg: fg_c,
            fg_dim: fg_c.alpha(0.66),
            fg_faint: fg_c.alpha(0.40),
            accent: Color::rgb(fill),
            on_accent: Color::rgb(on_accent),
            accent_text: Color::rgb(accent_text),
            hover: fg_c.alpha(if dark { 0.07 } else { 0.05 }),
            press: fg_c.alpha(if dark { 0.13 } else { 0.10 }),
            danger: Color::rgb(if dark { hex(0xff7b72) } else { hex(0xc62828) }),
            series,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_accent_stays_readable() {
        for accent in [
            None,
            Some(hex(0xffff00)),
            Some(hex(0x000000)),
            Some(hex(0xffffff)),
            Some(hex(0x808080)),
            Some(hex(0x0000ff)),
            Some([0.894, 0.651, 0.404]),
        ] {
            for dark in [false, true] {
                let t = Theme::new(dark, accent);
                let c = |a: Color, b: Color| contrast(a.rgb3(), b.rgb3());
                assert!(
                    c(t.on_accent, t.accent) >= TEXT_CONTRAST,
                    "{accent:?} {dark}"
                );
                assert!(c(t.accent_text, t.bg) >= TEXT_CONTRAST, "{accent:?} {dark}");
                assert!(c(t.fg, t.surface) >= 12.0);
                for s in t.series {
                    assert!(c(s, t.bg) >= UI_CONTRAST, "{accent:?} {dark}");
                }
            }
        }
    }
}
