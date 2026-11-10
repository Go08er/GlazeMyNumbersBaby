//! DGMNB's look: flat surfaces in light or dark, one accent colour (the
//! desktop's, if it shares one, else a calm blue), nothing animated.
//!
//! The colours themselves are computed by `appcore::palette`, shared with
//! GMNB; this wraps them in DGMNB's drawing colour type.

use appcore::palette::Palette;

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

impl From<Palette> for Theme {
    fn from(p: Palette) -> Theme {
        Theme {
            dark: p.dark,
            bg: Color(p.bg),
            surface: Color(p.surface),
            surface2: Color(p.surface2),
            border: Color(p.border),
            fg: Color(p.fg),
            fg_dim: Color(p.fg_dim),
            fg_faint: Color(p.fg_faint),
            accent: Color(p.accent),
            on_accent: Color(p.on_accent),
            accent_text: Color(p.accent_text),
            hover: Color(p.hover),
            press: Color(p.press),
            danger: Color(p.danger),
            series: p.series.map(Color),
        }
    }
}

impl Theme {
    pub fn new(dark: bool, accent: Option<[f32; 3]>) -> Theme {
        Palette::new(dark, accent).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore::color::hex;

    #[test]
    fn wrapper_matches_palette() {
        for accent in [None, Some(hex(0xffff00)), Some([0.894, 0.651, 0.404])] {
            for dark in [false, true] {
                let p = Palette::new(dark, accent);
                let t = Theme::new(dark, accent);
                assert_eq!(t.dark, dark);
                let got = [
                    t.bg,
                    t.surface,
                    t.surface2,
                    t.border,
                    t.fg,
                    t.fg_dim,
                    t.fg_faint,
                    t.accent,
                    t.on_accent,
                    t.accent_text,
                    t.hover,
                    t.press,
                    t.danger,
                    t.series[0],
                    t.series[1],
                    t.series[2],
                    t.series[3],
                    t.series[4],
                    t.series[5],
                ]
                .map(|c| c.0);
                assert_eq!(got, p.all(), "{accent:?} {dark}");
            }
        }
    }
}
