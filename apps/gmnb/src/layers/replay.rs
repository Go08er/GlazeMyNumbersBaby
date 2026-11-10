//! Replaying a theme's display list into a `GtkSnapshot`: each recorded
//! operation becomes the GtkSnapshot call (or the fixed few) the
//! contract's table names for it (docs/theming.md, "The draw API").
//!
//! The list is balanced and finite by construction (`gmnb_theme::draw`).
//! What the toolkit itself refuses is kept from it here: an empty
//! rectangle, a circle or a gradient of no radius, an SVG path it can't
//! parse. Such an operation draws nothing; a push that can't be made
//! still pushes (a group at full opacity), so that its pop finds it.

use std::cell::RefCell;
use std::collections::HashMap;

use gmnb_theme::draw::{self, BlendMode, DisplayList, Filter, MaskMode, Op};
use gmnb_theme::path::{Path, Segment};
use gmnb_theme::picture::{ImageId, Pixels};
use gmnb_theme::theme::Theme;
use gmnb_theme::types::*;
use gtk::prelude::*;
use gtk::{gdk, glib, graphene, gsk, pango};

fn rgba(c: Rgba) -> gdk::RGBA {
    gdk::RGBA::new(c[0], c[1], c[2], c[3])
}

fn rect(r: draw::Rect) -> graphene::Rect {
    graphene::Rect::new(r[0], r[1], r[2], r[3])
}

fn empty(r: draw::Rect) -> bool {
    r[2] <= 0.0 || r[3] <= 0.0
}

/// A rounded rectangle: corners too large for the box are scaled down
/// together, as CSS does (so 999 is "fully round").
fn rounded(r: draw::Rect, radius: &Radius) -> gsk::RoundedRect {
    let corner = |i: usize| graphene::Size::new(radius.0[i], radius.0[i]);
    let mut out = gsk::RoundedRect::new(rect(r), corner(0), corner(1), corner(2), corner(3));
    out.normalize();
    out
}

/// A font as Pango describes one. What a description can't say (letter
/// spacing, OpenType features) goes on the layout ([`attributes`]).
pub fn description(font: &Font) -> pango::FontDescription {
    let mut d = pango::FontDescription::new();
    d.set_family(&font.family.join(","));
    d.set_absolute_size(f64::from(font.size) * f64::from(pango::SCALE));
    d.set_weight(pango::Weight::__Unknown(font.weight.round() as i32));
    if font.italic {
        d.set_style(pango::Style::Italic);
    }
    let axes = font.effective_variations();
    if !axes.is_empty() {
        let text: Vec<String> = axes.iter().map(|(tag, v)| format!("{tag}={v}")).collect();
        d.set_variations(Some(&text.join(",")));
    }
    d
}

/// The attributes a layout in `font` needs beside its description.
pub fn attributes(font: &Font) -> Option<pango::AttrList> {
    if font.features.is_empty() && font.spacing == 0.0 {
        return None;
    }
    let attrs = pango::AttrList::new();
    if !font.features.is_empty() {
        let text: Vec<String> = font
            .features
            .iter()
            .map(|(tag, v)| format!("\"{tag}\" {v}"))
            .collect();
        attrs.insert(pango::AttrFontFeatures::new(&text.join(", ")));
    }
    if font.spacing != 0.0 {
        let units = font.spacing * font.size * pango::SCALE as f32;
        attrs.insert(pango::AttrInt::new_letter_spacing(units.round() as i32));
    }
    Some(attrs)
}

/// A layout of `text` in `font`.
pub fn layout(context: &pango::Context, text: &str, font: &Font) -> pango::Layout {
    let layout = pango::Layout::new(context);
    layout.set_text(text);
    layout.set_font_description(Some(&description(font)));
    layout.set_attributes(attributes(font).as_ref());
    layout
}

/// `draw:text_size`, measured by Pango.
pub struct Measure(pub pango::Context);

impl gmnb_theme::TextMeasure for Measure {
    fn measure(&self, text: &str, font: &Font) -> (f32, f32) {
        let (w, h) = layout(&self.0, text, font).pixel_size();
        (w as f32, h as f32)
    }
}

/// One laid-out character of a readout's value, and where it goes.
pub struct Glyph {
    pub layout: pango::Layout,
    pub x: f32,
    pub y: f32,
}

/// What a replay draws with, kept between frames: the theme's pictures
/// as textures, and the layouts of the texts it drew.
#[derive(Default)]
pub struct Cache {
    /// `None`: the picture has no pixels to show.
    textures: RefCell<HashMap<ImageId, Option<gdk::Texture>>>,
    layouts: RefCell<HashMap<String, pango::Layout>>,
}

/// Layouts kept at once: more, and the cache starts again.
const LAYOUTS: usize = 256;

impl Cache {
    pub fn clear(&self) {
        self.textures.borrow_mut().clear();
        self.layouts.borrow_mut().clear();
    }

    /// How many textures are held (for the gates).
    pub fn textures(&self) -> usize {
        self.textures.borrow().values().flatten().count()
    }

    /// The texture of a theme's picture: made the first time it is drawn.
    fn texture(&self, theme: &Theme, id: ImageId) -> Option<gdk::Texture> {
        if let Some(known) = self.textures.borrow().get(&id) {
            return known.clone();
        }
        let made = match theme.api().pictures().pixels(id) {
            Some(Pixels::Rgba8 {
                width,
                height,
                bytes,
            }) if width > 0 && height > 0 && bytes.len() == (width * height * 4) as usize => {
                let bytes = glib::Bytes::from_owned(bytes);
                Some(
                    gdk::MemoryTexture::new(
                        width as i32,
                        height as i32,
                        gdk::MemoryFormat::R8g8b8a8Premultiplied,
                        &bytes,
                        width as usize * 4,
                    )
                    .upcast(),
                )
            }
            // An image file: decoding them is the app's own decoders'
            // work (docs/theming-media.md, "Decoding stills"), which
            // arrive with the media slice. Until then a file draws
            // nothing; the toolkit's loaders are never given a theme's
            // file.
            _ => None,
        };
        self.textures.borrow_mut().insert(id, made.clone());
        made
    }

    fn layout(&self, context: &pango::Context, text: &str, font: &Font) -> pango::Layout {
        let key = format!("{text}\u{0}{font:?}");
        if let Some(known) = self.layouts.borrow().get(&key) {
            return known.clone();
        }
        let made = layout(context, text, font);
        let mut layouts = self.layouts.borrow_mut();
        if layouts.len() >= LAYOUTS {
            layouts.clear();
        }
        layouts.insert(key, made.clone());
        made
    }
}

/// What one replay needs beside the list.
pub struct Scene<'a> {
    pub cache: &'a Cache,
    /// The theme the list came from (its pictures).
    pub theme: Option<&'a Theme>,
    /// Lays out `draw:text`.
    pub pango: &'a pango::Context,
    /// The display layer: the value's characters, and the old value's.
    pub glyphs: [&'a [Glyph]; 2],
}

fn stops(g: &Gradient) -> Vec<gsk::ColorStop> {
    g.stops
        .iter()
        .zip(&g.colors)
        .map(|(offset, color)| gsk::ColorStop::new(*offset, rgba(*color)))
        .collect()
}

/// The smallest radius a gradient is given: the toolkit refuses 0.
const MIN_RADIUS: f32 = 0.001;

/// Appends `paint` over `bounds`, a gradient placed in `frame` (the box
/// its fractions and its angle refer to).
fn append_paint(s: &gtk::Snapshot, bounds: &graphene::Rect, frame: draw::Rect, paint: &Paint) {
    let g = match paint {
        Paint::Color(c) => {
            if c[3] > 0.0 {
                s.append_color(&rgba(*c), bounds);
            }
            return;
        }
        Paint::Gradient(g) => g,
    };
    let [x, y, w, h] = frame;
    let stops = stops(g);
    if stops.len() < 2 {
        return;
    }
    let center = || {
        let [cx, cy] = g
            .pixels
            .center
            .unwrap_or([x + g.at[0] * w, y + g.at[1] * h]);
        graphene::Point::new(cx, cy)
    };
    match g.kind {
        GradientKind::Linear => {
            let (from, to) = match (g.pixels.from, g.pixels.to) {
                (Some(a), Some(b)) => (a, b),
                // CSS's angle: 0 points up, 90 right; the line runs
                // through the box's centre, corner to corner.
                _ => {
                    let a = g.angle.to_radians();
                    let (dx, dy) = (a.sin(), -a.cos());
                    let len = (w * dx).abs() + (h * dy).abs();
                    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
                    (
                        [cx - dx * len / 2.0, cy - dy * len / 2.0],
                        [cx + dx * len / 2.0, cy + dy * len / 2.0],
                    )
                }
            };
            if from == to {
                // No line to run along: the last colour, as CSS has it.
                if let Some(last) = g.colors.last() {
                    s.append_color(&rgba(*last), bounds);
                }
                return;
            }
            s.append_linear_gradient(
                bounds,
                &graphene::Point::new(from[0], from[1]),
                &graphene::Point::new(to[0], to[1]),
                &stops,
            );
        }
        GradientKind::Radial => {
            let c = center();
            let (rx, ry) = match g.pixels.radius {
                Some(r) => (r, g.pixels.radius_y.unwrap_or(r)),
                // CSS's ellipse for the extent keyword.
                None => {
                    let (l, r) = ((c.x() - x).abs(), (x + w - c.x()).abs());
                    let (t, b) = ((c.y() - y).abs(), (y + h - c.y()).abs());
                    let corner = std::f32::consts::SQRT_2;
                    match g.extent {
                        Extent::ClosestSide => (l.min(r), t.min(b)),
                        Extent::FarthestSide => (l.max(r), t.max(b)),
                        Extent::ClosestCorner => (l.min(r) * corner, t.min(b) * corner),
                        Extent::FarthestCorner => (l.max(r) * corner, t.max(b) * corner),
                    }
                }
            };
            s.append_radial_gradient(
                bounds,
                &c,
                rx.max(MIN_RADIUS),
                ry.max(MIN_RADIUS),
                0.0,
                1.0,
                &stops,
            );
        }
        GradientKind::Conic => s.append_conic_gradient(bounds, &center(), g.angle, &stops),
    }
}

fn to_path(path: &Path) -> gsk::Path {
    let b = gsk::PathBuilder::new();
    for segment in &path.segments {
        match segment {
            Segment::MoveTo(x, y) => b.move_to(*x, *y),
            Segment::LineTo(x, y) => b.line_to(*x, *y),
            Segment::QuadTo(cx, cy, x, y) => b.quad_to(*cx, *cy, *x, *y),
            Segment::CurveTo(ax, ay, bx, by, x, y) => b.cubic_to(*ax, *ay, *bx, *by, *x, *y),
            Segment::Close => b.close(),
            Segment::Rect(x, y, w, h) => b.add_rect(&graphene::Rect::new(*x, *y, *w, *h)),
            Segment::RoundedRect(x, y, w, h, radius) => {
                b.add_rounded_rect(&rounded([*x, *y, *w, *h], radius));
            }
            Segment::Circle(x, y, r) => {
                if *r > 0.0 {
                    b.add_circle(&graphene::Point::new(*x, *y), *r);
                }
            }
            Segment::Svg(d) => {
                if let Ok(parsed) = gsk::Path::parse(d) {
                    b.add_path(&parsed);
                }
            }
        }
    }
    b.to_path()
}

fn to_stroke(stroke: &draw::Stroke) -> gsk::Stroke {
    let s = gsk::Stroke::new(stroke.width.max(0.01));
    s.set_line_cap(match stroke.cap {
        draw::LineCap::Butt => gsk::LineCap::Butt,
        draw::LineCap::Round => gsk::LineCap::Round,
        draw::LineCap::Square => gsk::LineCap::Square,
    });
    s.set_line_join(match stroke.join {
        draw::LineJoin::Miter => gsk::LineJoin::Miter,
        draw::LineJoin::Round => gsk::LineJoin::Round,
        draw::LineJoin::Bevel => gsk::LineJoin::Bevel,
    });
    if !stroke.dash.is_empty() {
        s.set_dash(&stroke.dash);
        s.set_dash_offset(stroke.dash_offset);
    }
    s
}

fn bounds_of(r: &graphene::Rect) -> draw::Rect {
    [r.x(), r.y(), r.width(), r.height()]
}

/// A layout at `(x, y)`, in `paint`: a colour directly, a gradient
/// through the layout as a mask, placed in the layout's own box.
fn append_layout(s: &gtk::Snapshot, layout: &pango::Layout, x: f32, y: f32, paint: &Paint) {
    let at = graphene::Point::new(x, y);
    match paint {
        Paint::Color(c) => {
            s.save();
            s.translate(&at);
            s.append_layout(layout, &rgba(*c));
            s.restore();
        }
        Paint::Gradient(_) => {
            let (w, h) = layout.pixel_size();
            // Ink can reach past the logical box (italics, a glow of the
            // theme's own): the gradient is laid a little wider.
            let frame = [x, y, w as f32, h as f32];
            let pad = h as f32 * 0.25;
            let bounds =
                graphene::Rect::new(x - pad, y - pad, w as f32 + 2.0 * pad, h as f32 + 2.0 * pad);
            s.push_mask(gsk::MaskMode::Alpha);
            s.save();
            s.translate(&at);
            s.append_layout(layout, &gdk::RGBA::WHITE);
            s.restore();
            s.pop();
            append_paint(s, &bounds, frame, paint);
            s.pop();
        }
    }
}

/// Replays `list` into `s`, in the coordinates `s` is in.
pub fn replay(s: &gtk::Snapshot, list: &DisplayList, scene: &Scene) {
    for op in &list.ops {
        match op {
            Op::Rect { rect: r, paint } => {
                if !empty(*r) {
                    append_paint(s, &rect(*r), *r, paint);
                }
            }
            Op::RoundedRect {
                rect: r,
                radius,
                paint,
            } => {
                if !empty(*r) {
                    s.push_rounded_clip(&rounded(*r, radius));
                    append_paint(s, &rect(*r), *r, paint);
                    s.pop();
                }
            }
            Op::Circle {
                center,
                radius,
                paint,
            } => {
                if *radius <= 0.0 {
                    continue;
                }
                let b = gsk::PathBuilder::new();
                b.add_circle(&graphene::Point::new(center[0], center[1]), *radius);
                let path = b.to_path();
                let frame = [
                    center[0] - radius,
                    center[1] - radius,
                    radius * 2.0,
                    radius * 2.0,
                ];
                match paint {
                    Paint::Color(c) => s.append_fill(&path, gsk::FillRule::Winding, &rgba(*c)),
                    gradient => {
                        s.push_fill(&path, gsk::FillRule::Winding);
                        append_paint(s, &rect(frame), frame, gradient);
                        s.pop();
                    }
                }
            }
            Op::Fill { path, paint, rule } => {
                let path = to_path(path);
                let rule = match rule {
                    draw::FillRule::Winding => gsk::FillRule::Winding,
                    draw::FillRule::EvenOdd => gsk::FillRule::EvenOdd,
                };
                match paint {
                    Paint::Color(c) => s.append_fill(&path, rule, &rgba(*c)),
                    gradient => {
                        if let Some(bounds) = path.bounds().filter(|b| !empty(bounds_of(b))) {
                            s.push_fill(&path, rule);
                            append_paint(s, &bounds, bounds_of(&bounds), gradient);
                            s.pop();
                        }
                    }
                }
            }
            Op::Stroke {
                path,
                stroke,
                paint,
            } => {
                let path = to_path(path);
                let stroke = to_stroke(stroke);
                match paint {
                    Paint::Color(c) => s.append_stroke(&path, &stroke, &rgba(*c)),
                    gradient => {
                        let bounds = path.stroke_bounds(&stroke);
                        if let Some(bounds) = bounds.filter(|b| !empty(bounds_of(b))) {
                            s.push_stroke(&path, &stroke);
                            append_paint(s, &bounds, bounds_of(&bounds), gradient);
                            s.pop();
                        }
                    }
                }
            }
            Op::Border {
                rect: r,
                radius,
                width,
                color,
            } => {
                if !empty(*r) && *width > 0.0 {
                    s.append_border(&rounded(*r, radius), &[*width; 4], &[rgba(*color); 4]);
                }
            }
            Op::Shadow {
                rect: r,
                radius,
                shadow,
            } => {
                if empty(*r) {
                    continue;
                }
                let outline = rounded(*r, radius);
                let color = rgba(shadow.color);
                let blur = shadow.blur.max(0.0);
                if shadow.inset {
                    s.append_inset_shadow(
                        &outline,
                        &color,
                        shadow.x,
                        shadow.y,
                        shadow.spread,
                        blur,
                    );
                } else {
                    s.append_outset_shadow(
                        &outline,
                        &color,
                        shadow.x,
                        shadow.y,
                        shadow.spread,
                        blur,
                    );
                }
            }
            Op::Text {
                text,
                at,
                font,
                paint,
            } => {
                if !text.is_empty() {
                    let layout = scene.cache.layout(scene.pango, text, font);
                    append_layout(s, &layout, at[0], at[1], paint);
                }
            }
            Op::Glyph {
                previous,
                index,
                paint,
            } => {
                let run = scene.glyphs[usize::from(*previous)];
                if let Some(g) = run.get(*index as usize) {
                    append_layout(s, &g.layout, g.x, g.y, paint);
                }
            }
            Op::Image {
                image,
                rect: r,
                filter,
            } => {
                if empty(*r) {
                    continue;
                }
                let texture = scene.theme.and_then(|t| scene.cache.texture(t, *image));
                if let Some(texture) = texture {
                    let filter = match filter {
                        Filter::Trilinear => gsk::ScalingFilter::Trilinear,
                        Filter::Linear => gsk::ScalingFilter::Linear,
                        Filter::Nearest => gsk::ScalingFilter::Nearest,
                    };
                    s.append_scaled_texture(&texture, filter, &rect(*r));
                }
            }
            Op::Tile { image, rect: r } => {
                if empty(*r) {
                    continue;
                }
                let texture = scene.theme.and_then(|t| scene.cache.texture(t, *image));
                if let Some(texture) = texture {
                    let tile = graphene::Rect::new(
                        r[0],
                        r[1],
                        texture.width() as f32,
                        texture.height() as f32,
                    );
                    s.push_repeat(&rect(*r), Some(&tile));
                    s.append_texture(&texture, &tile);
                    s.pop();
                }
            }
            Op::PushClip { rect: r, radius } => match radius {
                Some(radius) => s.push_rounded_clip(&rounded(*r, radius)),
                None => s.push_clip(&rect(*r)),
            },
            Op::PushOpacity(opacity) => s.push_opacity(f64::from(*opacity)),
            Op::PushBlur(radius) => s.push_blur(f64::from(radius.max(0.0))),
            Op::PushShadow(shadows) => {
                let list: Vec<gsk::Shadow> = shadows
                    .iter()
                    .map(|sh| gsk::Shadow::new(rgba(sh.color), sh.x, sh.y, sh.blur.max(0.0)))
                    .collect();
                if list.is_empty() {
                    s.push_opacity(1.0);
                } else {
                    s.push_shadow(&list);
                }
            }
            Op::PushMask(mode) => s.push_mask(match mode {
                MaskMode::Alpha => gsk::MaskMode::Alpha,
                MaskMode::InvertedAlpha => gsk::MaskMode::InvertedAlpha,
                MaskMode::Luminance => gsk::MaskMode::Luminance,
                MaskMode::InvertedLuminance => gsk::MaskMode::InvertedLuminance,
            }),
            Op::PushBlend(mode) => s.push_blend(match mode {
                BlendMode::Multiply => gsk::BlendMode::Multiply,
                BlendMode::Screen => gsk::BlendMode::Screen,
                BlendMode::Overlay => gsk::BlendMode::Overlay,
                BlendMode::Darken => gsk::BlendMode::Darken,
                BlendMode::Lighten => gsk::BlendMode::Lighten,
                BlendMode::ColorDodge => gsk::BlendMode::ColorDodge,
                BlendMode::ColorBurn => gsk::BlendMode::ColorBurn,
                BlendMode::HardLight => gsk::BlendMode::HardLight,
                BlendMode::SoftLight => gsk::BlendMode::SoftLight,
                BlendMode::Difference => gsk::BlendMode::Difference,
                BlendMode::Exclusion => gsk::BlendMode::Exclusion,
                BlendMode::Color => gsk::BlendMode::Color,
                BlendMode::Hue => gsk::BlendMode::Hue,
                BlendMode::Saturation => gsk::BlendMode::Saturation,
                BlendMode::Luminosity => gsk::BlendMode::Luminosity,
            }),
            Op::Pop => s.pop(),
            Op::Save => s.save(),
            Op::Restore => s.restore(),
            Op::Translate(dx, dy) => s.translate(&graphene::Point::new(*dx, *dy)),
            Op::Scale(sx, sy) => s.scale(*sx, *sy),
            Op::Rotate(degrees) => s.rotate(*degrees),
            // An operation this build doesn't know draws nothing. (None
            // can be a push: the list stays balanced.)
            _ => {}
        }
    }
}
