//! Text: embedded fonts, shaping (swash), per-character font fallback, bidi
//! ordering for the few right-to-left currency symbols, the `<sup>`/`<sub>`
//! label markup, and a bounded glyph cache.

use std::collections::HashMap;

use swash::scale::{Render, ScaleContext, Source};
use swash::shape::{Direction, ShapeContext};
use swash::text::Script;
use swash::zeno::{Format, Vector};
use swash::{FontRef, Setting};

use crate::gfx::{Canvas, Color};

static FONTS: [&[u8]; 6] = [
    include_bytes!("../assets/fonts/Inter.ttf"),
    include_bytes!("../assets/fonts/NotoSansMath-subset.otf"),
    include_bytes!("../assets/fonts/NotoSansArabic-subset.ttf"),
    include_bytes!("../assets/fonts/NotoSansArmenian-subset.ttf"),
    include_bytes!("../assets/fonts/NotoSansBengali-subset.ttf"),
    include_bytes!("../assets/fonts/NotoSansKhmer-subset.ttf"),
];
const ARABIC: u8 = 2;

/// Glyphs kept rasterised before the cache is flushed.
const CACHE_LIMIT: usize = 3000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    /// Logical pixels.
    pub size: f32,
    pub weight: f32,
    /// Tabular (fixed-width) digits.
    pub tabular: bool,
}

impl Style {
    pub const fn new(size: f32, weight: f32) -> Style {
        Style {
            size,
            weight,
            tabular: false,
        }
    }
    pub const fn tabular(mut self) -> Style {
        self.tabular = true;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub font: u8,
    pub id: u16,
    pub x: f32,
    /// Baseline shift (negative is up), for superscripts.
    pub y: f32,
    pub size: f32,
    pub weight: f32,
}

/// A shaped single line, in logical pixels relative to its origin on the
/// baseline.
#[derive(Clone, Debug, Default)]
pub struct Line {
    pub glyphs: Vec<Glyph>,
    pub width: f32,
    /// Height of capital letters above the baseline (for centring).
    pub cap: f32,
    /// (byte offset, x) at every cluster boundary, plus the end.
    pub carets: Vec<(usize, f32)>,
}

impl Line {
    /// x of the caret before byte `offset`.
    pub fn caret_x(&self, offset: usize) -> f32 {
        self.carets
            .iter()
            .filter(|(o, _)| *o <= offset)
            .map(|(_, x)| *x)
            .next_back()
            .unwrap_or(0.0)
    }

    /// Byte offset of the caret nearest to `x`.
    pub fn offset_at(&self, x: f32) -> usize {
        self.carets
            .iter()
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map(|c| c.0)
            .unwrap_or(0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    font: u8,
    id: u16,
    ppem4: u16,
    weight: u16,
    subpx: u8,
}

struct Raster {
    left: i32,
    top: i32,
    w: u32,
    h: u32,
    data: Vec<u8>,
}

pub struct Text {
    fonts: Vec<FontRef<'static>>,
    shape: ShapeContext,
    scale: ScaleContext,
    cache: HashMap<Key, Option<Raster>>,
    /// Which font draws a character (memoised).
    pick: HashMap<char, u8>,
}

impl Default for Text {
    fn default() -> Self {
        Self::new()
    }
}

fn is_rtl(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

impl Text {
    pub fn new() -> Text {
        Text {
            fonts: FONTS
                .iter()
                .map(|d| FontRef::from_index(d, 0).expect("embedded font"))
                .collect(),
            shape: ShapeContext::new(),
            scale: ScaleContext::new(),
            cache: HashMap::new(),
            pick: HashMap::new(),
        }
    }

    fn font_for(&mut self, c: char) -> u8 {
        if c.is_ascii() || c.is_whitespace() {
            return 0;
        }
        if let Some(&f) = self.pick.get(&c) {
            return f;
        }
        let f = self
            .fonts
            .iter()
            .position(|f| f.charmap().map(c) != 0)
            .unwrap_or(0) as u8;
        self.pick.insert(c, f);
        f
    }

    fn variations(&self, font: u8, size: f32, weight: f32) -> Vec<Setting<f32>> {
        if font == 0 {
            vec![
                Setting::from(("wght", weight.clamp(100.0, 900.0))),
                Setting::from(("opsz", size.clamp(14.0, 32.0))),
            ]
        } else {
            vec![Setting::from(("wght", weight.clamp(100.0, 900.0)))]
        }
    }

    /// Shape `text` (one line).
    pub fn layout(&mut self, text: &str, style: Style) -> Line {
        let mut line = self.empty_line(style);
        self.append(&mut line, text, 0, style, 0.0);
        line
    }

    /// Shape a label with `<sup>…</sup>` / `<sub>…</sub>` spans.
    pub fn layout_markup(&mut self, markup: &str, style: Style) -> Line {
        if !markup.contains("<s") && !markup.contains("</s") {
            return self.layout(markup, style);
        }
        let mut line = self.empty_line(style);
        let mut rest = markup;
        let mut shift: Option<f32> = None;
        let mut offset = 0;
        const TAGS: [&str; 4] = ["<sup>", "</sup>", "<sub>", "</sub>"];
        while !rest.is_empty() {
            // Only the four known tags are markup; any other '<' is text.
            let next_tag = TAGS
                .iter()
                .filter_map(|t| rest.find(t).map(|i| (i, *t)))
                .min_by_key(|(i, _)| *i);
            let (chunk, tail) = match next_tag {
                Some((0, tag)) => {
                    shift = match tag {
                        "<sup>" => Some(-0.36),
                        "<sub>" => Some(0.16),
                        _ => None,
                    };
                    offset += tag.len();
                    rest = &rest[tag.len()..];
                    continue;
                }
                Some((i, _)) => (&rest[..i], &rest[i..]),
                None => (rest, ""),
            };
            let st = match shift {
                Some(_) => Style {
                    size: style.size * 0.62,
                    ..style
                },
                None => style,
            };
            let dy = shift.map_or(0.0, |s| s * style.size);
            self.append(&mut line, chunk, offset, st, dy);
            offset += chunk.len();
            rest = tail;
        }
        line
    }

    fn empty_line(&self, style: Style) -> Line {
        let m = self.fonts[0].metrics(&[]).scale(style.size);
        Line {
            cap: m.cap_height,
            ..Line::default()
        }
    }

    /// Shape `text` onto the end of `line`; `base` is its byte offset in the
    /// caller's string, `dy` a baseline shift.
    fn append(&mut self, line: &mut Line, text: &str, base: usize, style: Style, dy: f32) {
        if text.is_empty() {
            line.carets.push((base, line.width));
            return;
        }
        // Visual runs: plain left-to-right unless right-to-left text appears.
        let runs: Vec<(std::ops::Range<usize>, bool)> = if text.chars().any(is_rtl) {
            let info = unicode_bidi::BidiInfo::new(text, None);
            let mut out = Vec::new();
            for para in &info.paragraphs {
                let (levels, runs) = info.visual_runs(para, para.range.clone());
                for r in runs {
                    let rtl = levels[r.start].is_rtl();
                    out.push((r, rtl));
                }
            }
            out
        } else {
            vec![(0..text.len(), false)]
        };
        for (range, rtl) in runs {
            // Split the run further wherever the drawing font changes.
            let mut segs: Vec<(std::ops::Range<usize>, u8)> = Vec::new();
            for (i, c) in text[range.clone()].char_indices() {
                let i = i + range.start;
                let f = if unicode_mark(c) {
                    segs.last().map_or(0, |s| s.1)
                } else {
                    self.font_for(c)
                };
                match segs.last_mut() {
                    Some(s) if s.1 == f => s.0.end = i + c.len_utf8(),
                    _ => segs.push((i..i + c.len_utf8(), f)),
                }
            }
            if rtl {
                segs.reverse();
            }
            for (seg, font) in segs {
                self.shape_seg(
                    line,
                    &text[seg.clone()],
                    base + seg.start,
                    font,
                    style,
                    dy,
                    rtl,
                );
            }
        }
        line.carets.push((base + text.len(), line.width));
    }

    #[allow(clippy::too_many_arguments)]
    fn shape_seg(
        &mut self,
        line: &mut Line,
        s: &str,
        base: usize,
        font: u8,
        style: Style,
        dy: f32,
        rtl: bool,
    ) {
        let vars = self.variations(font, style.size, style.weight);
        let fref = self.fonts[font as usize];
        let mut features: Vec<Setting<u16>> = Vec::new();
        if style.tabular && font == 0 {
            features.push(Setting::from(("tnum", 1u16)));
        }
        let mut shaper = self
            .shape
            .builder(fref)
            .script(if font == ARABIC {
                Script::Arabic
            } else {
                Script::Latin
            })
            .direction(if rtl {
                Direction::RightToLeft
            } else {
                Direction::LeftToRight
            })
            .size(style.size)
            .features(features)
            .variations(vars)
            .build();
        shaper.add_str(s);
        // (source offset, glyphs as (id, x, y, advance))
        type Cluster = (usize, Vec<(u16, f32, f32, f32)>);
        let mut clusters: Vec<Cluster> = Vec::new();
        shaper.shape_with(|c| {
            clusters.push((
                c.source.start as usize,
                c.glyphs
                    .iter()
                    .map(|g| (g.id, g.x, g.y, g.advance))
                    .collect(),
            ));
        });
        if rtl
            && clusters
                .first()
                .is_some_and(|f| f.0 < clusters.last().map_or(0, |l| l.0))
        {
            clusters.reverse();
        }
        for (start, glyphs) in clusters {
            if !rtl {
                line.carets.push((base + start, line.width));
            }
            for (id, gx, gy, adv) in glyphs {
                line.glyphs.push(Glyph {
                    font,
                    id,
                    x: line.width + gx,
                    y: dy - gy,
                    size: style.size,
                    weight: style.weight,
                });
                line.width += adv;
            }
        }
    }

    pub fn width(&mut self, text: &str, style: Style) -> f32 {
        self.layout(text, style).width
    }

    /// Draw `line` with its origin (left end of the baseline) at `(x, y)`.
    pub fn draw(&mut self, canvas: &mut Canvas, line: &Line, x: f32, y: f32, color: Color) {
        let s = canvas.scale;
        for g in &line.glyphs {
            let px = (x + g.x) * s;
            let py = ((y + g.y) * s).round();
            // Quarter-pixel horizontal positioning.
            let q = ((px - px.floor()) * 4.0).round() as i32;
            let ix = px.floor() as i32 + q / 4;
            let subpx = (q % 4) as u8;
            let key = Key {
                font: g.font,
                id: g.id,
                ppem4: (g.size * s * 4.0).round().clamp(1.0, 65535.0) as u16,
                weight: (g.weight / 10.0).round() as u16,
                subpx,
            };
            if !self.cache.contains_key(&key) {
                if self.cache.len() >= CACHE_LIMIT {
                    self.cache.clear();
                }
                let r = self.rasterize(key);
                self.cache.insert(key, r);
            }
            if let Some(Some(r)) = self.cache.get(&key) {
                canvas.blit_mask(ix + r.left, py as i32 - r.top, r.w, r.h, &r.data, color);
            }
        }
    }

    fn rasterize(&mut self, key: Key) -> Option<Raster> {
        let ppem = key.ppem4 as f32 / 4.0;
        let weight = key.weight as f32 * 10.0;
        let vars = self.variations(key.font, ppem, weight);
        let fref = self.fonts[key.font as usize];
        let mut scaler = self
            .scale
            .builder(fref)
            .size(ppem)
            .hint(false)
            .variations(vars)
            .build();
        let img = Render::new(&[Source::Outline])
            .format(Format::Alpha)
            .offset(Vector::new(key.subpx as f32 / 4.0, 0.0))
            .render(&mut scaler, key.id)?;
        let p = img.placement;
        (p.width > 0 && p.height > 0).then_some(Raster {
            left: p.left,
            top: p.top,
            w: p.width,
            h: p.height,
            data: img.data,
        })
    }
}

fn unicode_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x064B..=0x065F | 0x0670 | 0x20D0..=0x20FF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ui_glyph_has_a_font() {
        let mut t = Text::new();
        let sample = "▾…‹—→©0123456789+−×÷=.,%√∛π∞≤≥≪≫⌊⌋⌈⌉ℝℤ¹²³⁻ᵣ₀₉€£¥₹₽₩₪₫₴₸₺₼₾₡₦₱₲₵₭₮৳֏៛﷼؋د.إ лв ден КМ ÉßÞþðłőűșț";
        for c in sample.chars().filter(|c| !c.is_whitespace()) {
            let f = t.font_for(c);
            assert_ne!(
                t.fonts[f as usize].charmap().map(c),
                0,
                "no glyph for {c:?}"
            );
        }
    }

    #[test]
    fn markup_shifts_and_shrinks() {
        let mut t = Text::new();
        let st = Style::new(20.0, 400.0);
        let plain = t.layout("x2", st);
        let sup = t.layout_markup("x<sup>2</sup>", st);
        assert_eq!(plain.glyphs.len(), 2);
        assert_eq!(sup.glyphs.len(), 2);
        assert!(sup.glyphs[1].y < 0.0);
        assert!(sup.glyphs[1].size < st.size);
        assert!(sup.width < plain.width);
    }

    #[test]
    fn plain_angle_brackets_are_text() {
        let mut t = Text::new();
        let st = Style::new(20.0, 400.0);
        assert_eq!(t.layout_markup("<", st).glyphs.len(), 1);
        assert_eq!(t.layout_markup("a<b>c", st).glyphs.len(), 5);
        assert_eq!(t.layout_markup("<sup>2</sup>√x", st).glyphs.len(), 3);
    }

    #[test]
    fn carets_cover_every_boundary() {
        let mut t = Text::new();
        let l = t.layout("sin(x)", Style::new(16.0, 400.0));
        assert_eq!(l.carets.first().map(|c| c.0), Some(0));
        assert_eq!(l.carets.last().map(|c| c.0), Some(6));
        assert!(l.caret_x(3) > l.caret_x(1));
        assert_eq!(l.offset_at(-5.0), 0);
        assert_eq!(l.offset_at(1e4), 6);
    }

    #[test]
    fn tabular_digits_have_equal_advances() {
        let mut t = Text::new();
        let st = Style::new(30.0, 300.0).tabular();
        assert!((t.width("1111", st) - t.width("8888", st)).abs() < 0.01);
    }

    #[test]
    fn right_to_left_symbols_are_reordered() {
        let mut t = Text::new();
        // Dal, full stop, alef with hamza below (the hamza may be a mark glyph).
        let l = t.layout("د.إ", Style::new(20.0, 400.0));
        assert!(l.glyphs.len() >= 3);
        assert!(l.glyphs.iter().all(|g| g.id != 0));
        assert!(l.width > 0.0);
    }
}
