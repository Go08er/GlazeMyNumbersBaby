//! Drawing onto the window buffer with tiny-skia.
//!
//! The canvas draws straight into the buffer the compositor reads (no
//! intermediate pixmap and no per-frame copy). That buffer is 0RGB in native
//! (little-endian) order, i.e. B,G,R,X bytes, so colours are handed to
//! tiny-skia with red and blue swapped when `bgra` is set. Screenshots draw
//! into an ordinary RGBA pixmap with `bgra` off.

use tiny_skia::{
    FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, PixmapMut, Stroke, StrokeDash,
    Transform,
};

/// Straight (non-premultiplied) RGBA, 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color(pub [f32; 4]);

impl Color {
    pub const fn rgb(c: [f32; 3]) -> Color {
        Color([c[0], c[1], c[2], 1.0])
    }
    pub fn alpha(self, a: f32) -> Color {
        let [r, g, b, a0] = self.0;
        Color([r, g, b, a0 * a])
    }
    #[cfg(test)]
    pub fn rgb3(self) -> [f32; 3] {
        [self.0[0], self.0[1], self.0[2]]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn cx(&self) -> f32 {
        self.x + self.w / 2.0
    }
    pub fn cy(&self) -> f32 {
        self.y + self.h / 2.0
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
    pub fn inset(&self, d: f32) -> Rect {
        self.inset_xy(d, d)
    }
    pub fn inset_xy(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(
            self.x + dx,
            self.y + dy,
            (self.w - 2.0 * dx).max(0.0),
            (self.h - 2.0 * dy).max(0.0),
        )
    }
    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let (x0, y0) = (self.x.max(o.x), self.y.max(o.y));
        let (x1, y1) = (self.right().min(o.right()), self.bottom().min(o.bottom()));
        (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
    }
    /// Take `h` off the top: (top, rest).
    pub fn take_top(&self, h: f32) -> (Rect, Rect) {
        let h = h.min(self.h);
        (
            Rect::new(self.x, self.y, self.w, h),
            Rect::new(self.x, self.y + h, self.w, self.h - h),
        )
    }
    pub fn take_bottom(&self, h: f32) -> (Rect, Rect) {
        let h = h.min(self.h);
        (
            Rect::new(self.x, self.bottom() - h, self.w, h),
            Rect::new(self.x, self.y, self.w, self.h - h),
        )
    }
    pub fn take_left(&self, w: f32) -> (Rect, Rect) {
        let w = w.min(self.w);
        (
            Rect::new(self.x, self.y, w, self.h),
            Rect::new(self.x + w, self.y, self.w - w, self.h),
        )
    }
    pub fn take_right(&self, w: f32) -> (Rect, Rect) {
        let w = w.min(self.w);
        (
            Rect::new(self.right() - w, self.y, w, self.h),
            Rect::new(self.x, self.y, self.w - w, self.h),
        )
    }
    /// Cell (row, col) of an evenly divided grid with `gap` between cells.
    pub fn cell(&self, rows: usize, cols: usize, row: usize, col: usize, gap: f32) -> Rect {
        self.cell_span(rows, cols, row, col, 1, 1, gap)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn cell_span(
        &self,
        rows: usize,
        cols: usize,
        row: usize,
        col: usize,
        row_span: usize,
        col_span: usize,
        gap: f32,
    ) -> Rect {
        let cw = (self.w - gap * (cols as f32 - 1.0)) / cols as f32;
        let ch = (self.h - gap * (rows as f32 - 1.0)) / rows as f32;
        Rect::new(
            self.x + col as f32 * (cw + gap),
            self.y + row as f32 * (ch + gap),
            cw * col_span as f32 + gap * (col_span as f32 - 1.0),
            ch * row_span as f32 + gap * (row_span as f32 - 1.0),
        )
    }

    /// Split horizontally into `n` equal parts.
    pub fn columns(&self, n: usize, gap: f32) -> Vec<Rect> {
        (0..n).map(|i| self.cell(1, n, 0, i, gap)).collect()
    }
}

pub struct Canvas<'a> {
    pub pm: PixmapMut<'a>,
    /// Physical pixels per logical pixel.
    pub scale: f32,
    pub bgra: bool,
    clip: Option<Rect>,
    mask: Option<Mask>,
}

impl<'a> Canvas<'a> {
    pub fn new(pm: PixmapMut<'a>, scale: f32, bgra: bool) -> Self {
        Canvas {
            pm,
            scale,
            bgra,
            clip: None,
            mask: None,
        }
    }

    pub fn width(&self) -> f32 {
        self.pm.width() as f32 / self.scale
    }

    pub fn height(&self) -> f32 {
        self.pm.height() as f32 / self.scale
    }

    fn sk(&self, c: Color) -> tiny_skia::Color {
        let [r, g, b, a] = c.0;
        let (r, b) = if self.bgra { (b, r) } else { (r, b) };
        tiny_skia::Color::from_rgba(
            r.clamp(0.0, 1.0),
            g.clamp(0.0, 1.0),
            b.clamp(0.0, 1.0),
            a.clamp(0.0, 1.0),
        )
        .unwrap_or(tiny_skia::Color::BLACK)
    }

    fn paint(&self, c: Color) -> Paint<'static> {
        let mut p = Paint::default();
        p.set_color(self.sk(c));
        p.anti_alias = true;
        p
    }

    fn transform(&self) -> Transform {
        Transform::from_scale(self.scale, self.scale)
    }

    /// Restrict drawing to `r` (logical); `None` lifts the clip.
    pub fn set_clip(&mut self, r: Option<Rect>) {
        self.clip = r;
        let Some(r) = r else {
            return;
        };
        let (w, h) = (self.pm.width(), self.pm.height());
        if self
            .mask
            .as_ref()
            .is_none_or(|m| m.width() != w || m.height() != h)
        {
            self.mask = Mask::new(w, h);
        }
        if let Some(mask) = self.mask.as_mut() {
            mask.data_mut().fill(0);
            let s = self.scale;
            let x0 = (r.x * s).round().clamp(0.0, w as f32) as usize;
            let x1 = (r.right() * s).round().clamp(0.0, w as f32) as usize;
            let y0 = (r.y * s).round().clamp(0.0, h as f32) as usize;
            let y1 = (r.bottom() * s).round().clamp(0.0, h as f32) as usize;
            let data = mask.data_mut();
            for y in y0..y1 {
                data[y * w as usize + x0..y * w as usize + x1].fill(255);
            }
        }
    }

    pub fn clip(&self) -> Option<Rect> {
        self.clip
    }

    pub fn fill(&mut self, c: Color) {
        let col = self.sk(c);
        self.pm.fill(col);
    }

    pub fn fill_rect(&mut self, r: Rect, c: Color) {
        if let Some(rect) = tiny_skia::Rect::from_xywh(r.x, r.y, r.w, r.h) {
            let paint = self.paint(c);
            let t = self.transform();
            let mask = self.clip.and(self.mask.as_ref());
            self.pm.fill_rect(rect, &paint, t, mask);
        }
    }

    pub fn fill_path(&mut self, path: &Path, c: Color) {
        let paint = self.paint(c);
        let t = self.transform();
        let mask = self.clip.and(self.mask.as_ref());
        self.pm.fill_path(path, &paint, FillRule::Winding, t, mask);
    }

    pub fn stroke_path(&mut self, path: &Path, c: Color, width: f32, dash: Option<&[f32]>) {
        self.stroke_path_t(path, c, width, dash, Transform::identity());
    }

    /// Stroke `path` given in its own units, mapped by `t` (then the scale).
    pub fn stroke_path_t(
        &mut self,
        path: &Path,
        c: Color,
        width: f32,
        dash: Option<&[f32]>,
        t: Transform,
    ) {
        let paint = self.paint(c);
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            dash: dash.and_then(|d| StrokeDash::new(d.to_vec(), 0.0)),
            ..Stroke::default()
        };
        let t = t.post_concat(self.transform());
        let mask = self.clip.and(self.mask.as_ref());
        self.pm.stroke_path(path, &paint, &stroke, t, mask);
    }

    pub fn rounded(&mut self, r: Rect, radius: f32, c: Color) {
        if let Some(p) = rounded_rect(r, radius) {
            self.fill_path(&p, c);
        }
    }

    /// A 1px (logical) border inside `r`.
    pub fn rounded_border(&mut self, r: Rect, radius: f32, c: Color, width: f32) {
        if let Some(p) = rounded_rect(r.inset(width / 2.0), (radius - width / 2.0).max(0.0)) {
            self.stroke_path(&p, c, width, None);
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, c: Color) {
        if let Some(p) = PathBuilder::from_circle(cx, cy, radius) {
            self.fill_path(&p, c);
        }
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color, width: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(x0, y0);
        pb.line_to(x1, y1);
        if let Some(p) = pb.finish() {
            self.stroke_path(&p, c, width, None);
        }
    }

    /// Blend an 8-bit coverage mask (glyph) at physical pixel (px, py).
    pub fn blit_mask(&mut self, px: i32, py: i32, w: u32, h: u32, alpha: &[u8], c: Color) {
        let (pw, ph) = (self.pm.width() as i32, self.pm.height() as i32);
        let s = self.scale;
        let (cx0, cy0, cx1, cy1) = match self.clip {
            Some(r) => (
                (r.x * s).round() as i32,
                (r.y * s).round() as i32,
                (r.right() * s).round() as i32,
                (r.bottom() * s).round() as i32,
            ),
            None => (0, 0, pw, ph),
        };
        let (x0, y0) = (px.max(cx0).max(0), py.max(cy0).max(0));
        let (x1, y1) = (
            (px + w as i32).min(cx1).min(pw),
            (py + h as i32).min(cy1).min(ph),
        );
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let [r, g, b, a] = c.0;
        let (r, b) = if self.bgra { (b, r) } else { (r, b) };
        let src = [r, g, b].map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32);
        let ca = (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
        let data = self.pm.data_mut();
        for y in y0..y1 {
            let row = ((y - py) as u32 * w) as usize;
            let out = (y * pw) as usize * 4;
            for x in x0..x1 {
                let cov = alpha[row + (x - px) as usize] as u32 * ca / 255;
                if cov == 0 {
                    continue;
                }
                let i = out + x as usize * 4;
                let inv = 255 - cov;
                for k in 0..3 {
                    data[i + k] = ((src[k] * cov + data[i + k] as u32 * inv + 127) / 255) as u8;
                }
                data[i + 3] = (cov + data[i + 3] as u32 * inv / 255) as u8;
            }
        }
    }
}

pub fn rounded_rect(r: Rect, radius: f32) -> Option<Path> {
    let rad = radius.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    let (x, y, w, h) = (r.x, r.y, r.w, r.h);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    // Cubic approximation of a quarter circle.
    let k = rad * 0.552_284_8;
    let mut pb = PathBuilder::new();
    pb.move_to(x + rad, y);
    pb.line_to(x + w - rad, y);
    pb.cubic_to(x + w - rad + k, y, x + w, y + rad - k, x + w, y + rad);
    pb.line_to(x + w, y + h - rad);
    pb.cubic_to(
        x + w,
        y + h - rad + k,
        x + w - rad + k,
        y + h,
        x + w - rad,
        y + h,
    );
    pb.line_to(x + rad, y + h);
    pb.cubic_to(x + rad - k, y + h, x, y + h - rad + k, x, y + h - rad);
    pb.line_to(x, y + rad);
    pb.cubic_to(x, y + rad - k, x + rad - k, y, x + rad, y);
    pb.close();
    pb.finish()
}
