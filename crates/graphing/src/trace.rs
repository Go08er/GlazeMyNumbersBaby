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
}

fn project(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> (f64, (f64, f64)) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = (a.0 + t * dx, a.1 + t * dy);
    let d = ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
    (d, q)
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
            let (d, q) = project((px, py), a, b);
            if best.is_none_or(|(bd, _)| d < bd) {
                let (wx, wy) = vp.to_world(q.0, q.1);
                best = Some((d, Point::new(wx, wy)));
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
    let offer = |index: usize, x: f64, y: f64, best: &mut Option<TracePoint>| {
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
            });
        }
    };
    for (index, (eq, plot)) in curves.iter().enumerate() {
        match &eq.form {
            CompiledForm::Explicit { axis, f } => {
                let (cx, cy) = vp.to_world(px, py);
                let snap_eval = |t: f64| {
                    let t = round_to(t, precision);
                    let d = f.eval(t, 0.0);
                    match axis {
                        Axis::X => (t, d),
                        Axis::Y => (d, t),
                    }
                };
                // Directly above/below (or beside) the pointer.
                let (x, y) = snap_eval(if *axis == Axis::X { cx } else { cy });
                offer(index, x, y, &mut best);
                // Nearest along the drawn curve (steep parts, asymptotes).
                if let Some((_, q)) = nearest_on_polylines(vp, &plot.curves, px, py) {
                    let (x, y) = snap_eval(if *axis == Axis::X { q.x } else { q.y });
                    offer(index, x, y, &mut best);
                }
            }
            CompiledForm::Implicit { .. } | CompiledForm::Inequality { .. } => {
                if let Some((d, q)) = nearest_on_polylines(vp, &plot.curves, px, py)
                    && d <= radius_px + 3.0
                    && let Some(r) = newton_project(eq, q, vp)
                {
                    offer(index, r.x, r.y, &mut best);
                }
            }
        }
    }
    best
}

/// Formats a traced point as `(x, y)` with as many decimals as the tracing
/// precision (`10^(floor(log10(xMax − xMin)) − 3)`), e.g. `(1.25, -0.84)`.
pub fn format_trace_value(x: f64, y: f64, precision: f64) -> String {
    let decimals = if precision > 0.0 && precision.is_finite() {
        (-precision.log10().floor()).clamp(0.0, 15.0) as usize
    } else {
        6
    };
    let f = |v: f64| {
        let s = format!("{:.*}", decimals, round_to(v, precision));
        if s.trim_start_matches('-')
            .chars()
            .all(|c| c == '0' || c == '.')
        {
            s.trim_start_matches('-').to_string()
        } else {
            s
        }
    };
    format!("({}, {})", f(x), f(y))
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
    }
}
