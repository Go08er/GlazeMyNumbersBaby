//! The tokens (docs/theming.md, "Reference: tokens"): the few values most
//! of the look derives from, and `ctx`, what they are worked out for.
//!
//! `colors`, `fonts`, `text` and `shape` are a struct here, because every
//! element's base value is written in terms of them. The other token
//! tables (`icons`, `motion`, `layout`, `stock`, `native`) derive nothing,
//! so they are plain rows of the schema.

use std::collections::BTreeMap;

use appcore::metrics;
use appcore::palette::Palette;

use crate::ThemeValue;
use crate::check::{self, Notes, join};
use crate::types::*;

/// A theme option's value.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionValue {
    Text(String),
    Number(f64),
    Bool(bool),
}

/// What a theme is told (docs/theming.md, "`ctx`"), and what the base look
/// is worked out for.
#[derive(Clone, Debug, PartialEq)]
pub struct Ctx {
    pub dark: bool,
    /// The desktop's accent colour, if it shares one and the user lets the
    /// app use it.
    pub accent: Option<[f32; 3]>,
    /// The Background opacity setting, 0.1 to 1.
    pub opacity: f32,
    /// This theme's options by id: always complete.
    pub options: BTreeMap<String, OptionValue>,
    /// `false` when the desktop asks for reduced motion.
    pub motion: bool,
    /// The desktop's interface font: family, and size in pixels.
    pub font: (String, f32),
}

impl Default for Ctx {
    fn default() -> Self {
        Ctx {
            dark: false,
            accent: None,
            opacity: 1.0,
            options: BTreeMap::new(),
            motion: true,
            font: ("sans-serif".into(), 15.0),
        }
    }
}

impl Ctx {
    /// The opacity as one to draw with: 0.1 to 1.
    pub fn opacity(&self) -> f32 {
        if self.opacity.is_finite() {
            self.opacity.clamp(0.1, 1.0)
        } else {
            1.0
        }
    }

    pub fn see_through(&self) -> bool {
        self.opacity() < 1.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Colors {
    pub bg: Rgba,
    pub surface: Rgba,
    pub surface2: Rgba,
    pub border: Rgba,
    pub fg: Rgba,
    pub fg_dim: Rgba,
    pub fg_faint: Rgba,
    pub accent: Rgba,
    pub on_accent: Rgba,
    pub accent_text: Rgba,
    pub hover: Rgba,
    pub press: Rgba,
    pub danger: Rgba,
    pub series: [Rgba; 6],
}

impl Colors {
    /// The base colours: `appcore`'s, the ones DGMNB draws with.
    pub fn base(dark: bool, accent: Option<[f32; 3]>) -> Colors {
        Colors::from(&Palette::new(dark, accent))
    }

    /// The colour tokens by name, in the reference's order (`series` apart).
    pub fn named(&self) -> [(&'static str, Rgba); 13] {
        [
            ("bg", self.bg),
            ("surface", self.surface),
            ("surface2", self.surface2),
            ("border", self.border),
            ("fg", self.fg),
            ("fg_dim", self.fg_dim),
            ("fg_faint", self.fg_faint),
            ("accent", self.accent),
            ("on_accent", self.on_accent),
            ("accent_text", self.accent_text),
            ("hover", self.hover),
            ("press", self.press),
            ("danger", self.danger),
        ]
    }

    fn slot(&mut self, name: &str) -> Option<&mut Rgba> {
        Some(match name {
            "bg" => &mut self.bg,
            "surface" => &mut self.surface,
            "surface2" => &mut self.surface2,
            "border" => &mut self.border,
            "fg" => &mut self.fg,
            "fg_dim" => &mut self.fg_dim,
            "fg_faint" => &mut self.fg_faint,
            "accent" => &mut self.accent,
            "on_accent" => &mut self.on_accent,
            "accent_text" => &mut self.accent_text,
            "hover" => &mut self.hover,
            "press" => &mut self.press,
            "danger" => &mut self.danger,
            _ => return None,
        })
    }

    /// What goes under text on a see-through window, for these colours
    /// (`appcore::palette::text_scrim`): transparent at full opacity.
    ///
    /// It is also the least that lets `danger` (an error's text on the
    /// graph's canvas) reach 3 over both desktops: `appcore`'s is worked
    /// out for the text, its dim and the accent only, and DGMNB draws
    /// with it as it is.
    pub fn text_scrim(&self, opacity: f32) -> Rgba {
        let mut scrim =
            appcore::palette::text_scrim(self.bg, self.fg, self.fg_dim, self.accent_text, opacity);
        let opacity = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        if opacity >= 1.0 {
            return scrim;
        }
        let danger_reads = |k: f32| {
            [appcore::color::BLACK, appcore::color::WHITE]
                .into_iter()
                .all(|desktop| {
                    let window = [self.bg[0], self.bg[1], self.bg[2], self.bg[3] * opacity];
                    let behind =
                        appcore::color::over(window, [desktop[0], desktop[1], desktop[2], 1.0]);
                    let b = appcore::color::over([self.bg[0], self.bg[1], self.bg[2], k], behind);
                    let b3 = [b[0], b[1], b[2]];
                    let d = appcore::color::over(self.danger, b);
                    appcore::color::contrast([d[0], d[1], d[2]], b3) >= appcore::color::UI_CONTRAST
                })
        };
        if !danger_reads(scrim[3]) {
            // The background only gets closer to `bg` as k grows.
            let (mut lo, mut hi) = (scrim[3], 1.0f32);
            for _ in 0..20 {
                let mid = (lo + hi) / 2.0;
                if danger_reads(mid) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            scrim[3] = hi;
        }
        scrim
    }

    /// Faint text straight on a see-through window, on that scrim
    /// (`appcore::palette::faint_text`): `fg_faint` at full opacity.
    pub fn faint_text(&self, opacity: f32) -> Rgba {
        let scrim = self.text_scrim(opacity);
        appcore::palette::faint_text(self.bg, self.fg_faint, self.fg_dim, scrim, opacity)
    }
}

impl From<&Palette> for Colors {
    fn from(p: &Palette) -> Colors {
        Colors {
            bg: p.bg,
            surface: p.surface,
            surface2: p.surface2,
            border: p.border,
            fg: p.fg,
            fg_dim: p.fg_dim,
            fg_faint: p.fg_faint,
            accent: p.accent,
            on_accent: p.on_accent,
            accent_text: p.accent_text,
            hover: p.hover,
            press: p.press,
            danger: p.danger,
            series: p.series,
        }
    }
}

/// The families after Inter in `fonts.ui`: the scripts DGMNB bundles
/// fonts for. After them, the system's fonts.
pub const FALLBACK_FAMILIES: [&str; 5] = [
    "Noto Sans Math",
    "Noto Sans Arabic",
    "Noto Sans Armenian",
    "Noto Sans Bengali",
    "Noto Sans Khmer",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Fonts {
    /// Everything but the readout.
    pub ui: Vec<String>,
    /// The readout, and the converter's and date page's values.
    pub display: Vec<String>,
}

/// The six sizes of text the base look is set in.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyles {
    pub body: Font,
    pub strong: Font,
    pub small: Font,
    pub caption: Font,
    pub key: Font,
    pub title: Font,
}

impl TextStyles {
    pub fn named(&self) -> [(&'static str, &Font); 6] {
        [
            ("body", &self.body),
            ("strong", &self.strong),
            ("small", &self.small),
            ("caption", &self.caption),
            ("key", &self.key),
            ("title", &self.title),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    /// Keys, buttons, date and unit buttons.
    pub radius: f32,
    /// List rows, text fields.
    pub row_radius: f32,
    /// Panels, cards, popovers, the graph.
    pub card_radius: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tokens {
    pub colors: Colors,
    pub fonts: Fonts,
    pub text: TextStyles,
    pub shape: Shape,
}

/// The top-level keys of a style that are token tables handled here.
pub const TOKEN_TABLES: [&str; 4] = ["colors", "fonts", "text", "shape"];

impl Tokens {
    /// `T0`: the base tokens for `ctx`.
    pub fn base(ctx: &Ctx) -> Tokens {
        let ui: Vec<String> = std::iter::once(INTER)
            .chain(FALLBACK_FAMILIES)
            .map(str::to_string)
            .collect();
        let text = text_styles(&ui, None, &mut Notes::default());
        Tokens {
            colors: Colors::base(ctx.dark, ctx.accent),
            fonts: Fonts {
                display: ui.clone(),
                ui,
            },
            text,
            shape: Shape {
                radius: metrics::shape::RADIUS,
                row_radius: metrics::shape::ROW_RADIUS,
                card_radius: metrics::shape::CARD_RADIUS,
            },
        }
    }

    /// `T`: the theme's token tables merged over the base ones.
    pub fn merged(ctx: &Ctx, style: &ThemeValue, notes: &mut Notes) -> Tokens {
        let mut t = Tokens::base(ctx);
        if let Some(v) = style.get("colors") {
            merge_colors(&mut t.colors, v, notes);
        }
        if let Some(v) = style.get("fonts") {
            match v {
                ThemeValue::Map(m) => {
                    for (key, value) in m {
                        let path = join("fonts", key);
                        let slot = match key.as_str() {
                            "ui" => &mut t.fonts.ui,
                            "display" => &mut t.fonts.display,
                            _ => {
                                notes.push(&path, "unknown key");
                                continue;
                            }
                        };
                        if let Some(names) = check::families(value, &path, notes) {
                            *slot = names;
                        }
                    }
                    // `fonts.display` is `fonts.ui` unless the theme names one.
                    if !m.contains_key("display") {
                        t.fonts.display = t.fonts.ui.clone();
                    }
                }
                other => notes.push(
                    "fonts",
                    format!("expected a table, got {}", other.type_name()),
                ),
            }
        }
        // A text style without a family takes `fonts.ui`.
        t.text = text_styles(&t.fonts.ui, style.get("text"), notes);
        if let Some(v) = style.get("shape") {
            match v {
                ThemeValue::Map(m) => {
                    for (key, value) in m {
                        let path = join("shape", key);
                        let slot = match key.as_str() {
                            "radius" => &mut t.shape.radius,
                            "row_radius" => &mut t.shape.row_radius,
                            "card_radius" => &mut t.shape.card_radius,
                            _ => {
                                notes.push(&path, "unknown key");
                                continue;
                            }
                        };
                        if let Some(n) = check::number(value, &path, 0.0, 9999.0, notes) {
                            *slot = n as f32;
                        }
                    }
                }
                other => notes.push(
                    "shape",
                    format!("expected a table, got {}", other.type_name()),
                ),
            }
        }
        t
    }
}

/// The base text styles in `family`, with the theme's `text` table (if
/// any) merged over them.
fn text_styles(family: &[String], theme: Option<&ThemeValue>, notes: &mut Notes) -> TextStyles {
    use metrics::text as m;
    let base = |s: metrics::TextStyle| Font::new(family, s.size, s.weight);
    let mut t = TextStyles {
        body: base(m::BODY),
        strong: base(m::STRONG),
        small: base(m::SMALL),
        caption: base(m::CAPTION),
        key: base(m::KEY),
        title: base(m::TITLE),
    };
    match theme {
        None => {}
        Some(ThemeValue::Map(given)) => {
            for (key, value) in given {
                let path = join("text", key);
                let slot = match key.as_str() {
                    "body" => &mut t.body,
                    "strong" => &mut t.strong,
                    "small" => &mut t.small,
                    "caption" => &mut t.caption,
                    "key" => &mut t.key,
                    "title" => &mut t.title,
                    _ => {
                        notes.push(&path, "unknown key");
                        continue;
                    }
                };
                *slot = check::font(value, slot, &path, notes);
            }
        }
        Some(other) => notes.push(
            "text",
            format!("expected a table, got {}", other.type_name()),
        ),
    }
    t
}

fn merge_colors(colors: &mut Colors, v: &ThemeValue, notes: &mut Notes) {
    let ThemeValue::Map(m) = v else {
        notes.push("colors", format!("expected a table, got {}", v.type_name()));
        return;
    };
    for (key, value) in m {
        let path = join("colors", key);
        if key == "series" {
            let six = check::list(value).filter(|l| l.len() == 6);
            let Some(six) = six else {
                notes.push(&path, "expected a list of six colours");
                continue;
            };
            let mut series = colors.series;
            let mut ok = true;
            for (i, c) in six.iter().enumerate() {
                match check::color(c, &format!("{path}[{}]", i + 1), notes) {
                    Some(c) => series[i] = c,
                    None => ok = false,
                }
            }
            if ok {
                colors.series = series;
            }
            continue;
        }
        match colors.slot(key) {
            Some(slot) => {
                if let Some(c) = check::color(value, &path, notes) {
                    *slot = c;
                }
            }
            None => notes.push(&path, "unknown key"),
        }
    }
}
