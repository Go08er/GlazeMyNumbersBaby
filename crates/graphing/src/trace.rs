//! Tracing: the point on a curve nearest to the pointer
//! (`IGraphRenderer::GetClosePointData`) and its display text.

use crate::equation::{Axis, CompiledEquation, CompiledForm};
use crate::plot::{Plot, Point};
use crate::viewport::Viewport;

/// Default distance (pixels) within which the pointer snaps to a curve.
pub const DEFAULT_TRACE_RADIUS_PX: f64 = 50.0;
/// Active tracing (keyboard) moves the cursor this many pixels per tick
/// (`Grapher::HandleTracingMovementTick`).
pub const ACTIVE_TRACE_STEP_PX: f64 = 5.0;
/// … or this many with Shift held.
pub const ACTIVE_TRACE_FINE_STEP_PX: f64 = 1.0;
/// Tick interval of active tracing.
pub const ACTIVE_TRACE_TICK_MS: u64 = 100;
/// Initial active tracing cursor offset from the centre of the graph
/// (`RenderMain`: 40 px right of and 40 px above the centre).
pub const ACTIVE_TRACE_START_OFFSET_PX: (f64, f64) = (40.0, -40.0);

/// A traced point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TracePoint {
    /// Index of the equation in the slice passed to [`nearest_point`].
    pub index: usize,
    /// World coordinates of the point on the curve.
    pub x: f64,
    pub y: f64,
    /// Screen position of the point (pixels).
    pub screen_x: f64,
    pub screen_y: f64,
    /// Distance from the pointer in pixels.
    pub distance_px: f64,
    /// The steps `x` and `y` are shown to (see [`TracePoint::text`]).
    pub x_step: f64,
    pub y_step: f64,
}

impl TracePoint {
    /// `(x, y)` as shown: each coordinate to its step, which is the
    /// tracing precision unless the curve is too steep for it.
    pub fn text(&self) -> String {
        format!(
            "({}, {})",
            format_coordinate(self.x, self.x_step),
            format_coordinate(self.y, self.y_step)
        )
    }
}

/// The point of segment a–b nearest to p: its distance from p and its
/// position along the segment (0 at a, 1 at b).
fn project(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = (a.0 + t * dx, a.1 + t * dy);
    let d = ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
    (d, t)
}

/// Nearest point on any of the polylines, in screen space.
fn nearest_on_polylines(
    vp: &Viewport,
    lines: &[Vec<Point>],
    px: f64,
    py: f64,
) -> Option<(f64, Point)> {
    let mut best: Option<(f64, Point)> = None;
    for l in lines {
        for w in l.windows(2) {
            let a = vp.to_screen(w[0].x, w[0].y);
            let b = vp.to_screen(w[1].x, w[1].y);
            let (d, t) = project((px, py), a, b);
            if best.is_none_or(|(bd, _)| d < bd) {
                // Along the segment in graph coordinates: going back from
                // the screen would round x to the view's own float spacing
                // (about 2×10⁻¹⁵ in a ±10 view), far coarser than a steep
                // curve's.
                let (p, q) = (w[0], w[1]);
                best = Some((d, Point::new(p.x + t * (q.x - p.x), p.y + t * (q.y - p.y))));
            }
        }
        if l.len() == 1 {
            let a = vp.to_screen(l[0].x, l[0].y);
            let d = ((px - a.0).powi(2) + (py - a.1).powi(2)).sqrt();
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, l[0]));
            }
        }
    }
    best
}

/// The spacing of floats at `v`.
fn ulp(v: f64) -> f64 {
    let a = v.abs();
    if a.is_finite() {
        f64::from_bits(a.to_bits() + 1) - a
    } else {
        f64::NAN
    }
}

/// The smallest power of ten at least `v` (`v` > 0).
fn decade_at_least(v: f64) -> f64 {
    let p = 10f64.powi(v.log10().ceil().clamp(-320.0, 300.0) as i32);
    if p < v { p * 10.0 } else { p }
}

fn round_to(v: f64, precision: f64) -> f64 {
    if precision > 0.0 && precision.is_finite() {
        let r = (v / precision).round() * precision;
        if r == 0.0 { 0.0 } else { r }
    } else {
        v
    }
}

/// Moves a point onto `F(x, y) = 0` with a few Newton steps along the
/// gradient (for implicit curves and inequality boundaries).
fn newton_project(eq: &CompiledEquation, p: Point, vp: &Viewport) -> Option<Point> {
    let (hx, hy) = (vp.x_per_px() * 1e-3, vp.y_per_px() * 1e-3);
    let mut q = p;
    for _ in 0..8 {
        let f = eq.field(q.x, q.y);
        if !f.is_finite() {
            return None;
        }
        let gx = (eq.field(q.x + hx, q.y) - eq.field(q.x - hx, q.y)) / (2.0 * hx);
        let gy = (eq.field(q.x, q.y + hy) - eq.field(q.x, q.y - hy)) / (2.0 * hy);
        let g2 = gx * gx + gy * gy;
        if !(g2 > 0.0 && g2.is_finite()) {
            break;
        }
        let step = (f * gx / g2, f * gy / g2);
        q = Point::new(q.x - step.0, q.y - step.1);
        if (step.0 / vp.x_per_px()).abs() < 1e-3 && (step.1 / vp.y_per_px()).abs() < 1e-3 {
            break;
        }
    }
    // Stay close to where we started (avoid jumping to another branch).
    let (a, b) = (vp.to_screen(p.x, p.y), vp.to_screen(q.x, q.y));
    if ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() > 3.0 {
        Some(p)
    } else {
        Some(q)
    }
}

/// Finds the point nearest to the pointer `(px, py)` (pixels) on any of the
/// curves, within `radius_px`. `curves[i]` is an equation with its plot for
/// this viewport (the plot's polylines are used to locate candidates).
///
/// For explicit functions the result lies exactly on the curve: the x value
/// is rounded to the tracing precision (`Viewport::precision`) and y = f(x)
/// is evaluated, as when tracing in the original app; the point directly
/// above/below the pointer is also considered.
pub fn nearest_point(
    vp: &Viewport,
    curves: &[(&CompiledEquation, &Plot)],
    px: f64,
    py: f64,
    radius_px: f64,
) -> Option<TracePoint> {
    let precision = vp.precision();
    let mut best: Option<TracePoint> = None;
    let offer =
        |index: usize, (x, y): (f64, f64), steps: (f64, f64), best: &mut Option<TracePoint>| {
            if !(x.is_finite() && y.is_finite()) {
                return;
            }
            let (sx, sy) = vp.to_screen(x, y);
            let d = ((sx - px).powi(2) + (sy - py).powi(2)).sqrt();
            if d <= radius_px && best.is_none_or(|b| d < b.distance_px) {
                *best = Some(TracePoint {
                    index,
                    x,
                    y,
                    screen_x: sx,
                    screen_y: sy,
                    distance_px: d,
                    x_step: steps.0,
                    y_step: steps.1,
                });
            }
        };
    for (index, (eq, plot)) in curves.iter().enumerate() {
        match &eq.form {
            CompiledForm::Explicit { axis, f, .. } => {
                let (cx, cy) = vp.to_world(px, py);
                let point = |t: f64, d: f64| match axis {
                    Axis::X => (t, d),
                    Axis::Y => (d, t),
                };
                // The independent coordinate is rounded to the tracing
                // precision, as in the original, unless one step of it moves
                // the point more than a pixel (a steep curve): then to a
                // finer step, so tracing can still move along it pixel by
                // pixel. On a smooth curve a step's movement shrinks with
                // the step, so the step drops by as many decades as that
                // movement is over a pixel, down to a few floats of the
                // coordinate itself. A jump only counts on one side.
                let snap_eval = |t: f64| {
                    let at = |t: f64| {
                        let p = point(t, f.eval(t, 0.0));
                        (p, vp.to_screen(p.0, p.1))
                    };
                    let finest = 4.0 * ulp(t);
                    let mut step = precision;
                    for _ in 0..8 {
                        let (_, s) = at(round_to(t, step));
                        let quantum = [step, -step]
                            .map(|d| {
                                let (_, n) = at(round_to(t, step) + d);
                                ((n.0 - s.0).powi(2) + (n.1 - s.1).powi(2)).sqrt()
                            })
                            .into_iter()
                            .fold(f64::NAN, f64::min);
                        if quantum.is_nan() || quantum <= 1.0 || step <= finest {
                            break;
                        }
                        let decades = quantum.log10().ceil().clamp(1.0, 300.0) as i32;
                        step = (step / 10f64.powi(decades)).max(decade_at_least(finest));
                    }
                    let (p, _) = at(round_to(t, step));
                    let steps = match axis {
                        Axis::X => (step, precision),
                        Axis::Y => (precision, step),
                    };
                    (p, steps)
                };
                // Directly above/below (or beside) the pointer.
                let (p, steps) = snap_eval(if *axis == Axis::X { cx } else { cy });
                offer(index, p, steps, &mut best);
                // Nearest along the drawn curve (steep parts, asymptotes).
                if let Some((_, q)) = nearest_on_polylines(vp, &plot.curves, px, py) {
                    let (p, steps) = snap_eval(if *axis == Axis::X { q.x } else { q.y });
                    offer(index, p, steps, &mut best);
                }
            }
            CompiledForm::Implicit { .. } | CompiledForm::Inequality { .. } => {
                if let Some((d, q)) = nearest_on_polylines(vp, &plot.curves, px, py)
                    && d <= radius_px + 3.0
                    && let Some(r) = newton_project(eq, q, vp)
                {
                    offer(index, (r.x, r.y), (precision, precision), &mut best);
                }
            }
        }
    }
    best
}

/// Formats a traced point as `(x, y)` with as many decimals as the tracing
/// precision (`10^(floor(log10(xMax − xMin)) − 3)`), e.g. `(1.25, -0.84)`.
pub fn format_trace_value(x: f64, y: f64, precision: f64) -> String {
    format!(
        "({}, {})",
        format_coordinate(x, precision),
        format_coordinate(y, precision)
    )
}

/// `v` rounded to `step`, with as many decimals as the step has; past 15
/// decimals, as m×10ⁿ with the digits the step gives it.
fn format_coordinate(v: f64, step: f64) -> String {
    let decimals = if step > 0.0 && step.is_finite() {
        (-step.log10().floor()).max(0.0)
    } else {
        6.0
    };
    let r = round_to(v, step);
    if decimals > 15.0 && r == 0.0 {
        return "0".into();
    }
    if decimals > 15.0 && r.is_finite() {
        let e = r.abs().log10().floor();
        let digits = (decimals + e).clamp(0.0, 16.0) as usize;
        let s = format!("{:.*e}", digits, r);
        if let Some((m, e)) = s.split_once('e')
            && let Ok(e) = e.parse::<i32>()
        {
            let m = m.trim_end_matches('0').trim_end_matches('.');
            return format!("{}×10{}", m, crate::analysis::format::superscript(e));
        }
    }
    let decimals = decimals.min(15.0) as usize;
    let s = format!("{:.*}", decimals, r);
    if s.trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        s.trim_start_matches('-').to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::CompileOptions;
    use crate::equation::Equation;
    use crate::plot::{PlotOptions, plot};

    fn setup(src: &str, vp: &Viewport) -> (CompiledEquation, Plot) {
        let eq = Equation::parse(src)
            .unwrap()
            .compile(&CompileOptions::default())
            .unwrap();
        let p = plot(&eq, vp, &PlotOptions::default());
        (eq, p)
    }

    #[test]
    fn traces_explicit_function_exactly() {
        let vp = Viewport::new(-10.0, 10.0, -10.0, 10.0, 800.0, 800.0);
        let (eq, p) = setup("y = x^2/4", &vp);
        // Slightly off the curve near (3, 2.25): x snaps to the 0.01 grid.
        let (px, py) = vp.to_screen(3.004, 2.2534);
        let t = nearest_point(&vp, &[(&eq, &p)], px, py, 100.0).unwrap();
        assert_eq!(t.x, 3.0);
        assert_eq!(t.y, 2.25);
        assert_eq!(format_trace_value(t.x, t.y, vp.precision()), "(3.00, 2.25)");
    }

    #[test]
    fn traces_steep_curve_near_pointer() {
        let vp = Viewport::new(-10.0, 10.0, -10.0, 10.0, 800.0, 800.0);
        let (eq, p) = setup("y = tan(x)", &vp);
        // Pointer right next to the steep branch but far above f(cursor x).
        let x0 = 1.5;
        let (px, py) = vp.to_screen(x0 + 0.02, 1.5f64.tan());
        let t = nearest_point(&vp, &[(&eq, &p)], px, py, 30.0).unwrap();
        assert!(t.distance_px < 2.0, "{t:?}");
    }

    #[test]
    fn steep_curves_trace_finer_than_the_precision() {
        // At this scale x rounds to 0.01, a 400 px jump in y on this line;
        // tracing uses a finer step instead, and shows it.
        let vp = Viewport::new(-10.0, 10.0, -10.0, 10.0, 800.0, 800.0);
        let (eq, p) = setup("y = 1000*x", &vp);
        let (px, py) = vp.to_screen(0.0, 0.25);
        let t = nearest_point(&vp, &[(&eq, &p)], px, py, 50.0).unwrap();
        assert!(t.distance_px < 2.0, "{t:?}");
        assert_eq!(t.text(), "(0.00025, 0.25)");
        // R10-M-06: as steep as the floats allow, not a fixed number of
        // decades finer.
        for (src, text) in [
            ("y = 1000000000*x", "(0.00000000025, 0.25)"),
            ("y = 1000000000000*x", "(0.00000000000025, 0.25)"),
            // R11-M-05/L-02: past what the view's own coordinates resolve,
            // and past 15 decimals.
            ("y = 1000000000000000*x", "(2.5×10⁻¹⁶, 0.25)"),
            ("y = 100000000000000000000*x", "(2.5×10⁻²¹, 0.25)"),
        ] {
            let (eq, p) = setup(src, &vp);
            let t = nearest_point(&vp, &[(&eq, &p)], px, py, 50.0).unwrap();
            assert!(t.distance_px < 2.0, "{src}: {t:?}");
            assert_eq!(t.text(), text, "{src}");
        }
        // Shallow curves keep the original's rounding.
        let (eq, p) = setup("y = x^2/4", &vp);
        let (px, py) = vp.to_screen(3.004, 2.2534);
        let t = nearest_point(&vp, &[(&eq, &p)], px, py, 100.0).unwrap();
        assert_eq!((t.x_step, t.text().as_str()), (0.01, "(3.00, 2.25)"));
    }

    #[test]
    fn traces_implicit_circle() {
        let vp = Viewport::new(-10.0, 10.0, -10.0, 10.0, 800.0, 800.0);
        let (eq, p) = setup("x^2 + y^2 = 25", &vp);
        let (px, py) = vp.to_screen(4.0, 4.0);
        let t = nearest_point(&vp, &[(&eq, &p)], px, py, 100.0).unwrap();
        assert!(((t.x * t.x + t.y * t.y).sqrt() - 5.0).abs() < 1e-6, "{t:?}");
        assert!((t.x - t.y).abs() < 0.05);
    }

    #[test]
    fn picks_the_closest_equation_and_respects_radius() {
        let vp = Viewport::new(-10.0, 10.0, -10.0, 10.0, 800.0, 800.0);
        let (a, pa) = setup("y = 1", &vp);
        let (b, pb) = setup("y = 3", &vp);
        let (px, py) = vp.to_screen(0.0, 2.6);
        let t = nearest_point(&vp, &[(&a, &pa), (&b, &pb)], px, py, 100.0).unwrap();
        assert_eq!(t.index, 1);
        assert!(nearest_point(&vp, &[(&a, &pa)], px, py, 10.0).is_none());
    }

    #[test]
    fn trace_text() {
        assert_eq!(format_trace_value(1.23456, -0.5, 0.01), "(1.23, -0.50)");
        assert_eq!(format_trace_value(-0.0001, 2.0, 0.01), "(0.00, 2.00)");
        assert_eq!(format_trace_value(1234.5, 2.0, 1.0), "(1235, 2)");
        assert_eq!(format_coordinate(3.55e-15, 1e-17), "3.55×10⁻¹⁵");
        assert_eq!(format_coordinate(-3.5e-15, 1e-17), "-3.5×10⁻¹⁵");
        assert_eq!(format_coordinate(1e-20, 1e-17), "0");
    }
}
