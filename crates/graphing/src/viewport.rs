//! The visible window of the graph (world ranges + pixel size) and the
//! zoom / pan operations of the original renderer
//! (`IGraphRenderer::ScaleRange`, `ChangeRange`, `MoveRangeByRatio`,
//! `ResetRange`, `Get/SetDisplayRanges`) and `Grapher`/`GraphingCalculator`.

/// Default half-extent of the reset view: the square [−10, 10]² is fitted
/// into the graph's aspect ratio.
pub const DEFAULT_HALF_RANGE: f64 = 10.0;

/// Scale applied by the zoom-in button (`GraphingCalculator.zoomInScale`).
pub const ZOOM_IN_SCALE: f64 = 1.0 / 1.0625;
/// Scale applied by the zoom-out button (`GraphingCalculator.zoomOutScale`).
pub const ZOOM_OUT_SCALE: f64 = 1.0625;
/// Mouse wheel damping (`Grapher::OnPointerWheelChanged`).
pub const WHEEL_SCROLL_DAMPER: f64 = 0.15;
/// One wheel notch (`WHEEL_DELTA`).
pub const WHEEL_DELTA: f64 = 120.0;

/// Range-changing actions (`Graphing::Renderer::ChangeRangeAction`, 2D
/// subset). The engine's predefined ratios are not public; these use
/// 1.25× for zoom/widen/shrink, the button ratio 1.0625× for smooth/pinch
/// zoom, and 10% of the span for moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum ChangeRangeAction {
    ZoomIn,
    ZoomOut,
    WidenX,
    ShrinkX,
    WidenY,
    ShrinkY,
    MoveNegativeX,
    MovePositiveX,
    MoveNegativeY,
    MovePositiveY,
    SmoothZoomIn,
    SmoothZoomOut,
    PinchZoomIn,
    PinchZoomOut,
}

/// Error from [`Viewport::set_display_ranges`], mirroring the
/// `XError`/`YError` flags of `GraphingSettingsViewModel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeError {
    /// A bound is not a finite number.
    NotFinite,
    /// x min ≥ x max.
    XMinNotLessThanMax,
    /// y min ≥ y max.
    YMinNotLessThanMax,
    /// The span is too large or too small to map onto the graph's pixels.
    OutOfRange,
}

/// World-coordinate ranges together with the pixel size of the graph area.
///
/// Screen coordinates have their origin at the top-left corner with y
/// growing downwards; world coordinates are the usual math orientation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    /// Width of the graph area in pixels.
    pub width: f64,
    /// Height of the graph area in pixels.
    pub height: f64,
}

impl Viewport {
    /// A viewport with explicit ranges.
    pub fn new(
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
        width: f64,
        height: f64,
    ) -> Viewport {
        Viewport {
            x_min,
            x_max,
            y_min,
            y_max,
            width: width.max(1.0),
            height: height.max(1.0),
        }
    }

    /// The reset view ("Reset view" button, `ResetRange`): [−10, 10] on the
    /// shorter axis, the longer axis extended so one unit has the same
    /// number of pixels on both axes.
    pub fn default_for_size(width: f64, height: f64) -> Viewport {
        Viewport::fit_square(0.0, 0.0, DEFAULT_HALF_RANGE, width, height)
    }

    /// A view centred on `(cx, cy)` showing at least `half` units in every
    /// direction with square units.
    pub fn fit_square(cx: f64, cy: f64, half: f64, width: f64, height: f64) -> Viewport {
        let width = width.max(1.0);
        let height = height.max(1.0);
        let (hx, hy) = if width >= height {
            (half * width / height, half)
        } else {
            (half, half * height / width)
        };
        Viewport::new(cx - hx, cx + hx, cy - hy, cy + hy, width, height)
    }

    /// Same ranges, new pixel size.
    pub fn with_size(mut self, width: f64, height: f64) -> Viewport {
        self.width = width.max(1.0);
        self.height = height.max(1.0);
        self
    }

    /// Width of the x range.
    pub fn x_span(&self) -> f64 {
        self.x_max - self.x_min
    }

    /// Height of the y range.
    pub fn y_span(&self) -> f64 {
        self.y_max - self.y_min
    }

    /// World units per pixel horizontally.
    pub fn x_per_px(&self) -> f64 {
        self.x_span() / self.width
    }

    /// World units per pixel vertically.
    pub fn y_per_px(&self) -> f64 {
        self.y_span() / self.height
    }

    /// World → screen pixels.
    pub fn to_screen(&self, x: f64, y: f64) -> (f64, f64) {
        (
            (x - self.x_min) / self.x_per_px(),
            (self.y_max - y) / self.y_per_px(),
        )
    }

    /// Screen pixels → world.
    pub fn to_world(&self, px: f64, py: f64) -> (f64, f64) {
        (
            self.x_min + px * self.x_per_px(),
            self.y_max - py * self.y_per_px(),
        )
    }

    /// Converts a pointer position to the engine's normalised [−1, 1]
    /// coordinates (`PointerPositionToGraphPosition`).
    pub fn pointer_to_normalized(&self, px: f64, py: f64) -> (f64, f64) {
        (2.0 * px / self.width - 1.0, 1.0 - 2.0 * py / self.height)
    }

    /// `IGraphRenderer::ScaleRange(centerX, centerY, scale)`: scales both
    /// ranges by `scale` (> 1 zooms out) about a point given in normalised
    /// [−1, 1] graph coordinates (0, 0 = centre).
    pub fn scale_range(&mut self, center_nx: f64, center_ny: f64, scale: f64) {
        let cx = self.x_min + (center_nx + 1.0) * 0.5 * self.x_span();
        let cy = self.y_min + (center_ny + 1.0) * 0.5 * self.y_span();
        self.zoom_about(cx, cy, scale);
    }

    /// Scales both ranges by `scale` (> 1 zooms out) keeping the world point
    /// `(cx, cy)` fixed on screen.
    pub fn zoom_about(&mut self, cx: f64, cy: f64, scale: f64) {
        if !(scale.is_finite() && scale > 0.0) {
            return;
        }
        let new = Viewport {
            x_min: cx + (self.x_min - cx) * scale,
            x_max: cx + (self.x_max - cx) * scale,
            y_min: cy + (self.y_min - cy) * scale,
            y_max: cy + (self.y_max - cy) * scale,
            ..*self
        };
        if new.is_sane() {
            *self = new;
        }
    }

    /// Zooms keeping the world point under the pixel `(px, py)` fixed.
    pub fn zoom_about_pixel(&mut self, px: f64, py: f64, scale: f64) {
        let (cx, cy) = self.to_world(px, py);
        self.zoom_about(cx, cy, scale);
    }

    /// `Grapher::ZoomFromCenter`.
    pub fn zoom_from_center(&mut self, scale: f64) {
        self.scale_range(0.0, 0.0, scale);
    }

    /// The zoom-in button.
    pub fn zoom_in(&mut self) {
        self.zoom_from_center(ZOOM_IN_SCALE);
    }

    /// The zoom-out button.
    pub fn zoom_out(&mut self) {
        self.zoom_from_center(ZOOM_OUT_SCALE);
    }

    /// Scale factor for a mouse wheel delta (`MouseWheelDelta`, 120 per
    /// notch; positive = away from the user = zoom in), exactly as
    /// `Grapher::OnPointerWheelChanged`.
    pub fn wheel_scale(delta: f64) -> f64 {
        let scale = 1.0 + (delta.abs() / WHEEL_DELTA) * WHEEL_SCROLL_DAMPER;
        if delta >= 0.0 { 1.0 / scale } else { scale }
    }

    /// Applies a mouse wheel event at pixel `(px, py)`.
    pub fn wheel_zoom(&mut self, px: f64, py: f64, delta: f64) {
        let (nx, ny) = self.pointer_to_normalized(px, py);
        self.scale_range(nx, ny, Viewport::wheel_scale(delta));
    }

    /// `IGraphRenderer::MoveRangeByRatio`: +1 moves the view half a screen
    /// in the positive direction of the axis.
    pub fn move_by_ratio(&mut self, ratio_x: f64, ratio_y: f64) {
        let dx = ratio_x * 0.5 * self.x_span();
        let dy = ratio_y * 0.5 * self.y_span();
        self.x_min += dx;
        self.x_max += dx;
        self.y_min += dy;
        self.y_max += dy;
    }

    /// Pans so that the content follows a pointer drag of `(dx, dy)` pixels.
    pub fn pan_pixels(&mut self, dx: f64, dy: f64) {
        let wx = -dx * self.x_per_px();
        let wy = dy * self.y_per_px();
        self.x_min += wx;
        self.x_max += wx;
        self.y_min += wy;
        self.y_max += wy;
    }

    /// `IGraphRenderer::ChangeRange`.
    pub fn apply(&mut self, action: ChangeRangeAction) {
        use ChangeRangeAction::*;
        const ZOOM: f64 = 1.25;
        const SMOOTH: f64 = 1.0625;
        const MOVE: f64 = 0.2; // in half-screens => 10% of the span
        match action {
            ZoomIn => self.zoom_from_center(1.0 / ZOOM),
            ZoomOut => self.zoom_from_center(ZOOM),
            SmoothZoomIn | PinchZoomIn => self.zoom_from_center(1.0 / SMOOTH),
            SmoothZoomOut | PinchZoomOut => self.zoom_from_center(SMOOTH),
            WidenX | ShrinkX => {
                let s = if action == WidenX { ZOOM } else { 1.0 / ZOOM };
                let c = 0.5 * (self.x_min + self.x_max);
                let h = 0.5 * self.x_span() * s;
                let new = Viewport {
                    x_min: c - h,
                    x_max: c + h,
                    ..*self
                };
                if new.is_sane() {
                    *self = new;
                }
            }
            WidenY | ShrinkY => {
                let s = if action == WidenY { ZOOM } else { 1.0 / ZOOM };
                let c = 0.5 * (self.y_min + self.y_max);
                let h = 0.5 * self.y_span() * s;
                let new = Viewport {
                    y_min: c - h,
                    y_max: c + h,
                    ..*self
                };
                if new.is_sane() {
                    *self = new;
                }
            }
            MoveNegativeX => self.move_by_ratio(-MOVE, 0.0),
            MovePositiveX => self.move_by_ratio(MOVE, 0.0),
            MoveNegativeY => self.move_by_ratio(0.0, -MOVE),
            MovePositiveY => self.move_by_ratio(0.0, MOVE),
        }
    }

    /// `ResetRange`: back to the default view for the current pixel size.
    pub fn reset(&mut self) {
        *self = Viewport::default_for_size(self.width, self.height);
    }

    /// `SetDisplayRanges` with the validation of the graph settings panel.
    pub fn set_display_ranges(
        &mut self,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
    ) -> Result<(), RangeError> {
        if ![x_min, x_max, y_min, y_max].iter().all(|v| v.is_finite()) {
            return Err(RangeError::NotFinite);
        }
        if x_min >= x_max {
            return Err(RangeError::XMinNotLessThanMax);
        }
        if y_min >= y_max {
            return Err(RangeError::YMinNotLessThanMax);
        }
        // Same validation as zooming: finite spans with usable resolution
        // (e.g. -1e308..1e308 has an infinite span).
        let candidate = Viewport {
            x_min,
            x_max,
            y_min,
            y_max,
            ..*self
        };
        if !candidate.is_sane() {
            return Err(RangeError::OutOfRange);
        }
        *self = candidate;
        Ok(())
    }

    /// Tracing precision `10^(floor(log10(xMax − xMin)) − 3)`
    /// (`RenderMain::GetPrecision`).
    pub fn precision(&self) -> f64 {
        precision_for_span(self.x_span())
    }

    /// True if the point is inside the ranges.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x_min && x <= self.x_max && y >= self.y_min && y <= self.y_max
    }

    fn is_sane(&self) -> bool {
        let ok = |a: f64, b: f64| {
            a.is_finite() && b.is_finite() && b > a && (b - a) > 1e-300 && (b - a) < 1e300
        };
        // Keep at least ~12 significant digits of resolution across a pixel.
        let res = |a: f64, b: f64, px: f64| (b - a) / px > 1e-13 * a.abs().max(b.abs()).max(1e-300);
        ok(self.x_min, self.x_max)
            && ok(self.y_min, self.y_max)
            && res(self.x_min, self.x_max, self.width)
            && res(self.y_min, self.y_max, self.height)
    }
}

/// `10^(floor(log10(span)) − 3)`.
pub fn precision_for_span(span: f64) -> f64 {
    let e = span.log10().floor() - 3.0;
    if e.is_finite() && e.abs() < 300.0 {
        10f64.powi(e as i32)
    } else {
        f64::NAN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * (1.0 + b.abs())
    }

    #[test]
    fn default_view_fits_square() {
        let v = Viewport::default_for_size(1600.0, 800.0);
        assert_eq!(
            (v.x_min, v.x_max, v.y_min, v.y_max),
            (-20.0, 20.0, -10.0, 10.0)
        );
        assert!(close(v.x_per_px(), v.y_per_px()));
        let v = Viewport::default_for_size(500.0, 1000.0);
        assert_eq!(
            (v.x_min, v.x_max, v.y_min, v.y_max),
            (-10.0, 10.0, -20.0, 20.0)
        );
    }

    #[test]
    fn zoom_and_pan() {
        let mut v = Viewport::new(-10.0, 10.0, -10.0, 10.0, 200.0, 200.0);
        v.zoom_in();
        assert!(close(v.x_max, 10.0 / 1.0625));
        v.zoom_out();
        assert!(close(v.x_max, 10.0));
        // Zoom about a point keeps it fixed on screen.
        let (px, py) = (50.0, 30.0);
        let before = v.to_world(px, py);
        v.zoom_about_pixel(px, py, 0.5);
        let after = v.to_world(px, py);
        assert!(close(before.0, after.0) && close(before.1, after.1));
        // Normalised scale_range: (1, 1) is the top-right corner.
        let mut w = Viewport::new(-10.0, 10.0, -10.0, 10.0, 200.0, 200.0);
        w.scale_range(1.0, 1.0, 0.5);
        assert!(close(w.x_max, 10.0) && close(w.y_max, 10.0) && close(w.x_min, 0.0));
        // Wheel: positive delta zooms in.
        assert!(Viewport::wheel_scale(120.0) < 1.0);
        assert!(close(Viewport::wheel_scale(-120.0), 1.15));
        // Move by ratio: +1 = half a screen.
        let mut m = Viewport::new(-10.0, 10.0, -10.0, 10.0, 200.0, 200.0);
        m.move_by_ratio(1.0, 0.0);
        assert!(close(m.x_min, 0.0) && close(m.x_max, 20.0));
        // Dragging right by 10 px moves the content right (view left).
        let mut d = Viewport::new(-10.0, 10.0, -10.0, 10.0, 200.0, 200.0);
        d.pan_pixels(10.0, 0.0);
        assert!(close(d.x_min, -11.0));
    }

    #[test]
    fn transforms_round_trip() {
        let v = Viewport::new(-3.0, 7.0, -2.0, 4.0, 640.0, 480.0);
        let (px, py) = v.to_screen(1.5, 0.25);
        let (x, y) = v.to_world(px, py);
        assert!(close(x, 1.5) && close(y, 0.25));
        assert_eq!(v.to_screen(-3.0, 4.0), (0.0, 0.0));
    }

    #[test]
    fn precision_matches_reference() {
        assert_eq!(precision_for_span(20.0), 0.01);
        assert_eq!(precision_for_span(9.0), 0.001);
        assert_eq!(precision_for_span(1000.0), 1.0);
    }

    #[test]
    fn display_range_validation() {
        let mut v = Viewport::default_for_size(100.0, 100.0);
        assert_eq!(
            v.set_display_ranges(1.0, 1.0, 0.0, 1.0),
            Err(RangeError::XMinNotLessThanMax)
        );
        assert_eq!(
            v.set_display_ranges(0.0, 1.0, 2.0, 1.0),
            Err(RangeError::YMinNotLessThanMax)
        );
        assert!(v.set_display_ranges(0.0, 1.0, 0.0, 1.0).is_ok());
    }
}
