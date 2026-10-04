//! Tracing: the point on a curve nearest to the pointer
//! (`IGraphRenderer::GetClosePointData`) and its display text.
//!
//! On an explicit curve the traced coordinate is a decimal (the pointer's,
//! rounded to the tracing step) and the curve's value there comes from its
//! interval form ([`crate::plot::IntervalFn`]) evaluated at that decimal
//! exactly: proven defined, with as many digits as the enclosure fixes;
//! proven undefined (a hole, `(x²−1)/(x−1)` at 1); or unknown. The label
//! never shows a digit the enclosure leaves open. Implicit curves and
//! inequality boundaries are traced in floating point alone, and say so.

use crate::analysis::MIN_SHOWN_DIGITS;
use crate::equation::{Axis, CompiledEquation, CompiledForm};
use crate::interval::{Dec, derivs_valid};
use crate::plot::{IntervalFn, Plot, Point};
use crate::simplify::Q;
use crate::strings as s;
use crate::viewport::Viewport;

/// The minus sign in trace texts, as in the analysis panel (U+2212).
const MINUS: &str = "−";

/// Default distance (pixels) within which the pointer snaps to a curve.
pub const DEFAULT_TRACE_RADIUS_PX: f64 = 50.0;
/// Active tracing (keyboard) moves the cursor this many pixels per tick
/// (`Grapher::HandleTracingMovementTick`).
pub const ACTIVE_TRACE_STEP_PX: f64 = 5.0;
/// … or this many with Shift held.
pub const ACTIVE_TRACE_FINE_STEP_PX: f64 = 1.0;
/// Tick interval of active tracing.
pub const ACTIVE_TRACE_TICK_MS: u64 = 100;
/// A hole within this many pixels of the pointer is traced in preference
/// to the curve around it.
pub const HOLE_SNAP_PX: f64 = 4.0;
/// The end of a curve at a domain edge within this many pixels of the
/// pointer is traced in preference to the curve's nearest point when the
/// pointer is past the edge.
pub const EDGE_SNAP_PX: f64 = 50.0;
/// Initial active tracing cursor offset from the centre of the graph
/// (`RenderMain`: 40 px right of and 40 px above the centre).
pub const ACTIVE_TRACE_START_OFFSET_PX: (f64, f64) = (40.0, -40.0);

/// What tracing knows of a curve's value at the traced coordinate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TraceValue {
    /// Proven defined there, with its value in `[lo, hi]`, which fixes
    /// the digits shown.
    Defined { lo: f64, hi: f64 },
    /// Proven undefined there: a hole in the curve (drawn as an open
    /// circle; the point sits on it).
    Undefined,
    /// Neither: the enclosure is too wide to tell, or to fix the digits.
    Unknown,
    /// A point found in floating point alone (implicit curves, inequality
    /// boundaries): both coordinates are approximate.
    Approximate,
}

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
    /// The traced coordinate's axis on an explicit curve (`Axis::X` for
    /// y = f(x)): that coordinate is exactly the decimal shown, and
    /// [`TracePoint::value`] is the other one's.
    pub axis: Axis,
    pub value: TraceValue,
}

impl TracePoint {
    /// `(x, y)` as shown. On an explicit curve the traced coordinate is
    /// written to its step (the tracing precision, or finer where the
    /// curve is too steep for it) and the value to the digits its
    /// enclosure fixes, "≈" unless exact, or "undefined"/"unknown"; on
    /// other curves both are written to their steps, "≈".
    pub fn text(&self) -> String {
        let (x, y) = match self.value {
            TraceValue::Approximate => (
                format!("≈{}", format_coordinate(self.x, self.x_step)),
                format!("≈{}", format_coordinate(self.y, self.y_step)),
            ),
            value => {
                let (t, t_step, d_step) = match self.axis {
                    Axis::X => (self.x, self.x_step, self.y_step),
                    Axis::Y => (self.y, self.y_step, self.x_step),
                };
                let traced = Decimal::round(t, exponent_of(t_step)).0.text();
                let shown = match value {
                    TraceValue::Defined { lo, hi } => value_text(lo, hi, exponent_of(d_step))
                        .unwrap_or_else(|| s::TRACE_UNKNOWN.into()),
                    TraceValue::Undefined => s::TRACE_UNDEFINED.into(),
                    _ => s::TRACE_UNKNOWN.into(),
                };
                match self.axis {
                    Axis::X => (traced, shown),
                    Axis::Y => (shown, traced),
                }
            }
        };
        format!("({x}, {y})")
    }

    /// [`TracePoint::text`] for a screen reader: "≈" read as
    /// "approximately" (see [`spoken`]).
    pub fn spoken_text(&self) -> String {
        spoken(&self.text())
    }
}

/// A trace or value text for a screen reader: "≈" (a value known to its
/// digits only) read as "approximately".
pub fn spoken(text: &str) -> String {
    text.replace('≈', "approximately ")
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

/// 10ⁿ, correctly rounded (subnormal for n < −307).
fn pow10(n: i32) -> f64 {
    format!("1e{n}").parse().unwrap_or(f64::NAN)
}

/// The n of a step 10ⁿ.
fn exponent_of(step: f64) -> i32 {
    if step > 0.0 && step.is_finite() {
        step.log10().round() as i32
    } else {
        -6
    }
}

/// The least n with 10ⁿ ≥ `v` (`v` > 0).
fn exponent_at_least(v: f64) -> i32 {
    let n = (v.log10().ceil() as i32).clamp(-323, 308);
    if pow10(n) < v { n + 1 } else { n }
}

fn round_to(v: f64, precision: f64) -> f64 {
    if precision > 0.0 && precision.is_finite() {
        let r = (v / precision).round() * precision;
        if r == 0.0 { 0.0 } else { r }
    } else {
        v
    }
}

/// A decimal ±m·10ⁿ.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Decimal {
    neg: bool,
    /// The digits, without leading zeros ("0" for zero).
    m: String,
    n: i32,
}

impl Decimal {
    /// `v` (finite) rounded to a multiple of 10ⁿ, half away from 0, from
    /// its exact value; and whether that is `v` itself.
    fn round(v: f64, n: i32) -> (Decimal, bool) {
        debug_assert!(v.is_finite());
        // A double's exact expansion has at most 1074 fractional digits.
        let full = format!("{:.1074}", v.abs());
        let (int, frac) = full.split_once('.').unwrap_or((&full, ""));
        let digits = [int.as_bytes(), frac.as_bytes()].concat();
        // digits[i] is the digit of 10^(int.len() − 1 − i): those before
        // `cut` are multiples of 10ⁿ.
        let cut = int.len() as i64 - n as i64;
        let (keep, rest, up) = if cut < 0 {
            // Below a tenth of 10ⁿ.
            (&[][..], &digits[..], false)
        } else {
            let c = (cut as usize).min(digits.len());
            let (k, r) = digits.split_at(c);
            (k, r, r.first().is_some_and(|&d| d >= b'5'))
        };
        let exact = rest.iter().all(|&d| d == b'0');
        let mut m = keep.to_vec();
        if up {
            let mut i = m.len();
            loop {
                if i == 0 {
                    m.insert(0, b'1');
                    break;
                }
                i -= 1;
                if m[i] == b'9' {
                    m[i] = b'0';
                } else {
                    m[i] += 1;
                    break;
                }
            }
        }
        let m = String::from_utf8_lossy(&m);
        let m = match m.trim_start_matches('0') {
            "" => "0".to_string(),
            t => t.to_string(),
        };
        let neg = v < 0.0 && m != "0";
        (Decimal { neg, m, n }, exact)
    }

    /// It as a rational, if that fits.
    fn to_q(&self) -> Option<Q> {
        let m: i128 = self.m.parse().ok()?;
        let m = if self.neg { -m } else { m };
        let p = 10i128.checked_pow(self.n.unsigned_abs())?;
        if self.n >= 0 {
            Q::new(m.checked_mul(p)?, 1)
        } else {
            Q::new(m, p)
        }
    }

    /// The double nearest to it.
    fn to_f64(&self) -> f64 {
        let sign = if self.neg { "-" } else { "" };
        format!("{sign}{}e{}", self.m, self.n)
            .parse()
            .unwrap_or(f64::NAN)
    }

    /// Significant digits (0 for zero).
    fn significant(&self) -> i32 {
        if self.m == "0" {
            0
        } else {
            self.m.len() as i32
        }
    }

    /// With as many decimals as 10ⁿ has; past 15, as m×10ᵉ with its
    /// digits.
    fn text(&self) -> String {
        let sign = if self.neg { MINUS } else { "" };
        if self.m == "0" && (self.n >= 0 || self.n < -15) {
            return "0".into();
        }
        if self.n < -15 {
            let e = self.m.len() as i32 - 1 + self.n;
            let (head, tail) = self.m.split_at(1);
            let tail = tail.trim_end_matches('0');
            let mantissa = if tail.is_empty() {
                head.to_string()
            } else {
                format!("{head}.{tail}")
            };
            return format!(
                "{sign}{mantissa}×10{}",
                crate::analysis::format::superscript(e)
            );
        }
        if self.n >= 0 {
            return format!("{sign}{}{}", self.m, "0".repeat(self.n as usize));
        }
        let d = (-self.n) as usize;
        let padded = format!("{:0>w$}", self.m, w = d + 1);
        let (int, frac) = padded.split_at(padded.len() - d);
        format!("{sign}{int}.{frac}")
    }
}

/// The text of a value known to lie in `[lo, hi]` (bounded): to the step
/// 10ⁿ (the tracing precision) when every value in it rounds alike to
/// that; else to the nearest step that fixes it, up to three decades finer
/// (a value on a rounding boundary: 0.835 to 0.01) or coarser while at
/// least [`MIN_SHOWN_DIGITS`] significant digits remain. Marked "≈" unless
/// it is the value exactly. None when no step fixes it.
fn value_text(lo: f64, hi: f64, n: i32) -> Option<String> {
    if !(lo.is_finite() && hi.is_finite()) {
        return None;
    }
    let fixed = |n: i32| {
        let (a, exact) = Decimal::round(lo, n);
        let (b, _) = Decimal::round(hi, n);
        (a == b).then(|| {
            if exact && lo == hi {
                a.text()
            } else {
                format!("≈{}", a.text())
            }
        })
    };
    (0..=3).map(|k| n - k).find_map(fixed).or_else(|| {
        (n + 1..=n + 20)
            .take_while(|&m| Decimal::round(lo, m).0.significant() >= MIN_SHOWN_DIGITS)
            .find_map(fixed)
    })
}

/// What the interval form says of the curve at the number enclosed by
/// `[lo, hi]`, its value's digits to 10ⁿ.
fn value_at(iv: &IntervalFn, lo: f64, hi: f64, n: i32) -> TraceValue {
    let e = iv.enclose(lo, hi);
    if e.is_empty() {
        return TraceValue::Undefined;
    }
    if e.dec >= Dec::Def && e.iv.is_bounded() && value_text(e.lo(), e.hi(), n).is_some() {
        TraceValue::Defined {
            lo: e.lo(),
            hi: e.hi(),
        }
    } else {
        TraceValue::Unknown
    }
}

/// The n of the step 10ⁿ the traced coordinate is rounded to near `t`: the
/// tracing precision's `n0`, as in the original, unless one step of it
/// moves the point more than a pixel (a steep curve): then finer, so
/// tracing can still move along it pixel by pixel, by as many decades as
/// that movement is over a pixel, down to a few floats of `t` itself. The
/// movement is the step times f′ at the rounded point, from the interval
/// form (an enclosure of f′ over a whole step can be far wider than the
/// slope: (x−c)/(x−c) next to c); where f′ isn't known there (a pole, a
/// hole, a jump) the step stays. `t_px` and `d_px` are the traced and the
/// other coordinate's units per pixel.
fn step_exponent(iv: &IntervalFn, t: f64, n0: i32, t_px: f64, d_px: f64) -> i32 {
    let finest = exponent_at_least(4.0 * ulp(t));
    let slope = |c: f64| {
        let s = iv.series(c, c, 1);
        derivs_valid(&s, 1).then(|| s[1].iv.mig_mag().1)
    };
    let mut n = n0;
    for _ in 0..iterations(iv, 8) {
        if n <= finest {
            break;
        }
        let h = pow10(n);
        let Some(m) = slope(Decimal::round(t, n).0.to_f64()) else {
            break;
        };
        // log₁₀ of the pixels one step moves, the larger way (in logs: a
        // slope of 10³⁰⁰ on a fine view overflows the pixels themselves).
        let (a, b) = (-t_px.log10(), m.log10() - d_px.log10());
        let (big, small) = if a >= b { (a, b) } else { (b, a) };
        let moved = h.log10() + big + 0.5 * (10f64.powf(2.0 * (small - big))).ln_1p() / 10f64.ln();
        if !(moved > 0.0 && moved.is_finite()) {
            break;
        }
        let decades = moved.ceil().clamp(1.0, 300.0) as i32;
        n = (n - decades).max(finest);
    }
    n
}

/// The value at the decimal `t` exactly where the interval form allows
/// it (a rational function: `x/x − 1` is 0 at 0.26 exactly, though its
/// enclosure over the doubles either side of 0.26 is only ≈0), else
/// `value` (its enclosure there).
fn exact_value(iv: &IntervalFn, t: &Decimal, value: TraceValue) -> TraceValue {
    if let TraceValue::Defined { lo, hi } = value
        && let Some(v) = t.to_q().and_then(|q| iv.exact_at(q)).and_then(Q::exact_f64)
        && lo <= v
        && v <= hi
    {
        return TraceValue::Defined { lo: v, hi: v };
    }
    value
}

/// At most `most` interval evaluations of f's series for one refinement,
/// fewer for a long f, so a pointer move over 14 long curves stays within
/// a frame: about 3000 units of [`IntervalFn::cost`] (a sine is 8), at
/// least two.
fn iterations(iv: &IntervalFn, most: usize) -> usize {
    (3000 / iv.cost().max(1)).clamp(2, most)
}

/// Where the curve takes the value `d` (to within `tol`) near `t`: Newton
/// steps, with f and f′ from the interval form. None where it isn't proven
/// smooth. On a line steeper than 10¹⁶ each step can only cancel the
/// leading digits of t (10³⁰⁰·x from 10⁻¹⁷ to 10⁻³⁰² takes about twenty).
///
/// Only where the curve is steep there (`ratio`·|f′| over 2, `ratio` the
/// traced coordinate's units per pixel over the other's): elsewhere the
/// point in the pointer's column is as good, and a wandering Newton on a
/// long, wiggly f (79 terms of sin(x^k)) cost tens of milliseconds a
/// pointer move. Stops once a step no longer halves the miss.
fn solve_near(iv: &IntervalFn, t: f64, d: f64, tol: f64, ratio: f64) -> Option<f64> {
    let mut t = t;
    let mut miss = f64::INFINITY;
    for i in 0..iterations(iv, 32) {
        let s = iv.series(t, t, 1);
        if !derivs_valid(&s, 1) {
            return None;
        }
        let (v, dv) = (s[0].iv.mid(), s[1].iv.mid());
        if i == 0 && !(dv.abs() * ratio > 2.0) {
            return None;
        }
        if (v - d).abs() <= tol || (v - d).abs() > 0.5 * miss {
            break;
        }
        miss = (v - d).abs();
        let next = t - (v - d) / dv;
        if !next.is_finite() {
            return None;
        }
        if next == t {
            break;
        }
        t = next;
    }
    Some(t)
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

/// A candidate traced point: its world coordinates, the steps they are
/// shown to, the traced axis and what is known of the value.
type Candidate = ((f64, f64), (f64, f64), Axis, TraceValue);

/// Finds the point nearest to the pointer `(px, py)` (pixels) on any of the
/// curves, within `radius_px`. `curves[i]` is an equation with its plot for
/// this viewport (the plot's polylines are used to locate candidates).
///
/// On an explicit function the traced coordinate is the pointer's rounded
/// to the tracing precision (`Viewport::precision`, or finer on a steep
/// curve) and the point is the curve's there, as when tracing in the
/// original app; the point directly above/below the pointer and the one
/// nearest along the drawn curve are considered. Where the function is
/// proven undefined, only a hole has a point to trace (at its marker): a
/// pole or a domain edge is passed over for the drawn curve nearby.
pub fn nearest_point(
    vp: &Viewport,
    curves: &[(&CompiledEquation, &Plot)],
    px: f64,
    py: f64,
    radius_px: f64,
) -> Option<TracePoint> {
    let precision = vp.precision();
    let n0 = if precision > 0.0 && precision.is_finite() {
        exponent_of(precision)
    } else {
        (vp.x_span().log10().floor() - 3.0).clamp(-323.0, 308.0) as i32
    };
    // The best so far, by its distance less its bias.
    let mut best: Option<(f64, TracePoint)> = None;
    let offer = |index: usize, c: Candidate, bias: f64, best: &mut Option<(f64, TracePoint)>| {
        let ((x, y), (x_step, y_step), axis, value) = c;
        if !(x.is_finite() && y.is_finite()) {
            return;
        }
        let (sx, sy) = vp.to_screen(x, y);
        let d = ((sx - px).powi(2) + (sy - py).powi(2)).sqrt();
        let key = d - bias;
        if d <= radius_px && best.is_none_or(|(k, _)| key < k) {
            *best = Some((
                key,
                TracePoint {
                    index,
                    x,
                    y,
                    screen_x: sx,
                    screen_y: sy,
                    distance_px: d,
                    x_step,
                    y_step,
                    axis,
                    value,
                },
            ));
        }
    };
    for (index, (eq, plot)) in curves.iter().enumerate() {
        match &eq.form {
            CompiledForm::Explicit { axis, f, iv } => {
                let axis = *axis;
                let (cx, cy) = vp.to_world(px, py);
                let (t_px, d_px) = match axis {
                    Axis::X => (vp.x_per_px(), vp.y_per_px()),
                    Axis::Y => (vp.y_per_px(), vp.x_per_px()),
                };
                // (traced, other) coordinates to (x, y), and back.
                let point = |t: f64, d: f64| match axis {
                    Axis::X => (t, d),
                    Axis::Y => (d, t),
                };
                // A curve whose drawn polylines (within the tolerance of
                // it) and holes are all out of reach has no point to
                // trace: skip its interval work.
                let near = nearest_on_polylines(vp, &plot.curves, px, py);
                let reach = radius_px + 1.0;
                let hole_near = plot.holes.iter().any(|h| {
                    let (sx, sy) = vp.to_screen(h.x, h.y);
                    (sx - px).hypot(sy - py) <= reach
                });
                if !hole_near && near.is_none_or(|(d, _)| d > reach) {
                    continue;
                }
                let snap = |t: f64| -> Option<Candidate> {
                    if !t.is_finite() {
                        return None;
                    }
                    let n = match iv {
                        Some(iv) => step_exponent(iv, t, n0, t_px, d_px),
                        None => n0,
                    };
                    // The decimal shown, and its enclosure: the double
                    // itself when exact, else the doubles either side.
                    let shown = Decimal::round(t, n).0;
                    let t = shown.to_f64();
                    let (lo, hi) = if Decimal::round(t, n).1 {
                        (t, t)
                    } else {
                        (t.next_down(), t.next_up())
                    };
                    let value = match iv {
                        Some(iv) => exact_value(iv, &shown, value_at(iv, lo, hi, n0)),
                        None => TraceValue::Approximate,
                    };
                    let d = match value {
                        TraceValue::Undefined => {
                            let half = 0.5 * pow10(n);
                            plot.holes
                                .iter()
                                .map(|h| point(h.x, h.y))
                                .find(|&(ht, _)| (ht - t).abs() <= half)?
                                .1
                        }
                        TraceValue::Defined { lo, hi } => {
                            let d = f.eval(t, 0.0);
                            if d.is_finite() { d } else { 0.5 * (lo + hi) }
                        }
                        _ => f.eval(t, 0.0),
                    };
                    let steps = match axis {
                        Axis::X => (pow10(n), pow10(n0)),
                        Axis::Y => (pow10(n0), pow10(n)),
                    };
                    Some((point(t, d), steps, axis, value))
                };
                // Directly above/below (or beside) the pointer.
                let pt = if axis == Axis::X { cx } else { cy };
                let mut on_it = false;
                if let Some(c) = snap(pt) {
                    let (sx, sy) = vp.to_screen(c.0.0, c.0.1);
                    on_it = (sx - px).hypot(sy - py) <= 0.5;
                    offer(index, c, 0.0, &mut best);
                }
                // Past an end of f's domain (proven undefined in the
                // pointer's column, not a hole), the end of the curve near
                // it comes first, at the tracing step if f is defined
                // there: keyboard tracing reaches √x's (0, 0), where the
                // nearest point of the drawn curve to a pointer left of
                // it can be well up the steep part.
                if let Some(iv) = iv
                    && iv.enclose(pt, pt).is_empty()
                {
                    for l in &plot.curves {
                        for q in [l.first(), l.last()].into_iter().flatten() {
                            let (qt, _) = point(q.x, q.y);
                            let shown = Decimal::round(qt, n0).0;
                            let t = shown.to_f64();
                            let (lo, hi) = if Decimal::round(t, n0).1 {
                                (t, t)
                            } else {
                                (t.next_down(), t.next_up())
                            };
                            if let TraceValue::Defined { lo: a, hi: b } =
                                value_at(iv, lo, hi, n0)
                            {
                                let value = exact_value(iv, &shown, TraceValue::Defined { lo: a, hi: b });
                                let d = f.eval(t, 0.0);
                                let d = if d.is_finite() { d } else { 0.5 * (a + b) };
                                let c = (point(t, d), (pow10(n0), pow10(n0)), axis, value);
                                offer(index, c, EDGE_SNAP_PX, &mut best);
                            }
                        }
                    }
                }
                // A hole near the pointer, which could hardly land on its
                // exact coordinate otherwise.
                for h in &plot.holes {
                    if let Some(c) = snap(point(h.x, h.y).0)
                        && c.3 == TraceValue::Undefined
                    {
                        offer(index, c, HOLE_SNAP_PX, &mut best);
                    }
                }
                // Nearest along the drawn curve (steep parts, asymptotes).
                // (Not needed when the point in the pointer's column is
                // within half a pixel of it: nothing along the curve is nearer
                // by more than that.)
                if let Some((_, q)) = near.filter(|_| !on_it) {
                    let (qt, qd) = point(q.x, q.y);
                    if let Some(c) = snap(qt) {
                        offer(index, c, 0.0, &mut best);
                    }
                    // A chord a pixel wide can cross the whole view (1e20·x):
                    // where along it the curve itself meets that point's
                    // level.
                    if let Some(iv) = iv
                        && let Some(t) = solve_near(iv, qt, qd, 0.01 * d_px, t_px / d_px)
                        && let Some(c) = snap(t)
                    {
                        offer(index, c, 0.0, &mut best);
                    }
                }
            }
            CompiledForm::Implicit { .. } | CompiledForm::Inequality { .. } => {
                if let Some((d, q)) = nearest_on_polylines(vp, &plot.curves, px, py)
                    && d <= radius_px + 3.0
                    && let Some(r) = newton_project(eq, q, vp)
                {
                    let c = (
                        (r.x, r.y),
                        (precision, precision),
                        Axis::X,
                        TraceValue::Approximate,
                    );
                    offer(index, c, 0.0, &mut best);
                }
            }
        }
    }
    best.map(|(_, t)| t)
}

/// Formats a point as `(x, y)` with as many decimals as the tracing
/// precision (`10^(floor(log10(xMax − xMin)) − 3)`), e.g. `(1.25, −0.84)`:
/// the original's rounding, of the doubles as they are. A traced point's
/// own text is [`TracePoint::text`].
pub fn format_trace_value(x: f64, y: f64, precision: f64) -> String {
    format!(
        "({}, {})",
        format_coordinate(x, precision),
        format_coordinate(y, precision)
    )
}

/// `v` rounded to `step`, with as many decimals as the step has; past 15
/// decimals, as m×10ⁿ with the digits the step gives it. Negative with
/// U+2212, as the analysis panel writes them.
fn format_coordinate(v: f64, step: f64) -> String {
    let s = format_coordinate_ascii(v, step);
    match s.strip_prefix('-') {
        Some(rest) => format!("{MINUS}{rest}"),
        None => s,
    }
}

fn format_coordinate_ascii(v: f64, step: f64) -> String {
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

    fn vp() -> Viewport {
        Viewport::new(-10.0, 10.0, -10.0, 10.0, 800.0, 800.0)
    }

    /// The point traced with the pointer at world (x, y).
    fn trace_at(src: &str, x: f64, y: f64, radius: f64) -> Option<TracePoint> {
        let vp = vp();
        let (eq, p) = setup(src, &vp);
        let (px, py) = vp.to_screen(x, y);
        nearest_point(&vp, &[(&eq, &p)], px, py, radius)
    }

    #[test]
    fn traces_explicit_function_exactly() {
        // Slightly off the curve near (3, 2.25): x snaps to the 0.01 grid,
        // and f(3) = 2.25 exactly.
        let t = trace_at("y = x^2/4", 3.004, 2.2534, 100.0).unwrap();
        assert_eq!((t.x, t.y), (3.0, 2.25));
        assert_eq!(t.value, TraceValue::Defined { lo: 2.25, hi: 2.25 });
        assert_eq!(t.text(), "(3.00, 2.25)");
        assert_eq!(
            format_trace_value(t.x, t.y, vp().precision()),
            "(3.00, 2.25)"
        );
    }

    #[test]
    fn traced_values_show_the_digits_their_enclosure_fixes() {
        // Rounded: approximate.
        let t = trace_at("y = sin(x)", 1.0, 0.84, 100.0).unwrap();
        assert_eq!(t.text(), "(1.00, ≈0.84)");
        // At the decimal 0.10, not the double nearest it: f is enclosed
        // there, and the text says it's approximate.
        let t = trace_at("y = x", 0.1, 0.1, 100.0).unwrap();
        assert_eq!(t.text(), "(0.10, ≈0.10)");
        // Exactly halfway at the precision (1.67/2 = 0.835): neither 0.83
        // nor 0.84 is fixed; one more digit is.
        let t = trace_at("y = x/2", 1.67, 0.835, 100.0).unwrap();
        assert_eq!(t.text(), "(1.67, ≈0.835)");
        // Exact values are shown as such.
        let t = trace_at("y = x - 2", 2.0, 0.0, 100.0).unwrap();
        assert_eq!(t.text(), "(2.00, 0.00)");
        let t = trace_at("y = floor(x)", 2.0, 2.0, 100.0).unwrap();
        assert_eq!(t.text(), "(2.00, 2.00)");
    }

    #[test]
    fn holes_trace_as_undefined_at_their_markers() {
        for (src, x, y, slope) in [
            ("y = (x^2-1)/(x-1)", 1.0, 2.0, 1.0),
            ("y = x/x", 0.0, 1.0, 0.0),
        ] {
            let t = trace_at(src, x + 0.001, y + 0.001, 100.0).unwrap();
            assert_eq!(t.value, TraceValue::Undefined, "{src}: {t:?}");
            assert!(t.x == x && (t.y - y).abs() < 1e-6, "{src}: {t:?}");
            assert_eq!(t.text(), format!("({x:.2}, undefined)"), "{src}");
            // Within a few pixels of it, not on its exact coordinate.
            let t = trace_at(src, x + 0.06, y + 0.03, 100.0).unwrap();
            assert_eq!(t.value, TraceValue::Undefined, "{src}: {t:?}");
            // Further along it's defined again.
            let t = trace_at(src, x + 0.3, y + 0.3 * slope, 100.0).unwrap();
            assert!(
                matches!(t.value, TraceValue::Defined { .. }),
                "{src}: {t:?}"
            );
        }
    }

    #[test]
    fn holes_in_x_of_y_trace_as_undefined() {
        // x = g(y): y is traced, x is the value.
        let t = trace_at("x = (y^2-1)/(y-1)", 2.03, 1.06, 100.0).unwrap();
        assert_eq!(t.axis, Axis::Y);
        assert_eq!(t.value, TraceValue::Undefined, "{t:?}");
        assert!(t.y == 1.0 && (t.x - 2.0).abs() < 1e-6, "{t:?}");
        assert_eq!(t.text(), "(undefined, 1.00)");
        let t = trace_at("x = (y^2-1)/(y-1)", 3.0, 2.0, 100.0).unwrap();
        assert_eq!(t.text(), "(3.00, 2.00)");
    }

    #[test]
    fn holes_off_the_step_grid_are_passed_over() {
        // 0.125 isn't a multiple of this view's step, 0.01: the decimals
        // either side are traced, not the hole, and the value there is 1
        // exactly (PREREVIEW_B: "≈1.00").
        let src = "y = (x-0.125)/(x-0.125)";
        let (_, p) = setup(src, &vp());
        assert_eq!(p.holes.len(), 1, "{:?}", p.holes);
        let t = trace_at(src, 0.125, 1.0, 100.0).unwrap();
        assert!(matches!(t.value, TraceValue::Defined { .. }), "{t:?}");
        assert!(
            ["(0.12, 1.00)", "(0.13, 1.00)"].contains(&t.text().as_str()),
            "{t:?}"
        );
        // x/x − 1 at 0.26 (not a double) is 0 exactly too.
        let t = trace_at("y = x/x - 1", 0.26, 0.0, 100.0).unwrap();
        assert_eq!(t.text(), "(0.26, 0.00)");
        // Negative values with U+2212, as the panel writes them.
        let t = trace_at("y = x - 1", 0.5, -0.5, 100.0).unwrap();
        assert_eq!(t.text(), "(0.50, −0.50)");
        assert_eq!(t.spoken_text(), "(0.50, −0.50)");
        let t = trace_at("y = sin(x)", 0.5, 0.48, 100.0).unwrap();
        assert_eq!(t.text(), "(0.50, ≈0.48)");
        assert_eq!(t.spoken_text(), "(0.50, approximately 0.48)");
    }

    #[test]
    fn poles_and_domain_edges_are_passed_over() {
        // At the pole there's no point: the drawn curve nearby is traced.
        let t = trace_at("y = 1/x", 0.0, 8.0, 100.0).unwrap();
        assert!(
            t.x > 0.0 && matches!(t.value, TraceValue::Defined { .. }),
            "{t:?}"
        );
        assert!(trace_at("y = sqrt(x)", -3.0, 0.0, 20.0).is_none());
    }

    #[test]
    fn unknown_values_are_not_shown() {
        // At the decimal 0.3, 10·0.3 is 3, but the enclosure of 10x there
        // straddles 3 and floor's can't say which side it is: not "2" and
        // not "3".
        let t = trace_at("y = floor(10x)", 0.3, 3.0, 100.0).unwrap();
        assert_eq!(t.value, TraceValue::Unknown, "{t:?}");
        assert_eq!(t.text(), "(0.30, unknown)");
    }

    #[test]
    fn values_and_texts() {
        let d = |v: f64, n: i32| Decimal::round(v, n);
        assert_eq!(d(0.835, -2).0.text(), "0.83"); // the double is below
        assert_eq!(d(2.25, -2), (Decimal::round(2.25, -2).0, true));
        assert_eq!(d(2.25, -1).0.text(), "2.3");
        assert!(!d(0.1, -2).1);
        assert_eq!(d(-0.004, -2).0.text(), "0.00");
        assert_eq!(d(1234.5, 1).0.text(), "1230");
        assert_eq!(d(9.996, -2).0.text(), "10.00");
        assert_eq!(d(2.5e-21, -22).0.text(), "2.5×10⁻²¹");
        assert_eq!(d(-3.5e-15, -17).0.text(), "−3.5×10⁻¹⁵");
        assert_eq!(d(1e-20, -17).0.text(), "0");
        assert_eq!(d(0.1, -2).0.to_f64(), 0.1);

        assert_eq!(value_text(2.25, 2.25, -2).unwrap(), "2.25");
        assert_eq!(value_text(0.25f64.next_down(), 0.25, -2).unwrap(), "≈0.25");
        assert_eq!(value_text(-1e-17, 1e-17, -2).unwrap(), "≈0.00");
        // Finer, then coarser while three digits remain.
        assert_eq!(value_text(0.8349, 0.8351, -2).unwrap(), "≈0.835");
        assert_eq!(value_text(123.4549, 123.4551, -2).unwrap(), "≈123.455");
        assert_eq!(value_text(123.3, 123.4, -2).unwrap(), "≈123");
        assert_eq!(value_text(0.83, 0.84, -2), None);
    }

    #[test]
    fn traces_steep_curve_near_pointer() {
        let vp = vp();
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
        // tracing uses a finer step instead (from f′), and shows it.
        let vp = vp();
        let (eq, p) = setup("y = 1000*x", &vp);
        let (px, py) = vp.to_screen(0.0, 0.25);
        let t = nearest_point(&vp, &[(&eq, &p)], px, py, 50.0).unwrap();
        assert!(t.distance_px < 2.0, "{t:?}");
        // 1000 · 0.00025 is 0.25 exactly, though neither decimal is a
        // double.
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
    fn keyboard_steps_move_along_steep_lines() {
        // The arrows move the cursor 5 px, or 1 px with Shift (as the
        // apps do): up a steep line, every step moves the traced point
        // up, by about as much.
        let vp = vp();
        for src in [
            "y = 1000000000*x",
            "y = 1000000000000000*x",
            "y = 100000000000000000000*x",
        ] {
            let (eq, p) = setup(src, &vp);
            for px_step in [ACTIVE_TRACE_FINE_STEP_PX, ACTIVE_TRACE_STEP_PX] {
                let (mut px, mut py) = vp.to_screen(0.0, 0.5);
                let mut ys = Vec::new();
                for _ in 0..21 {
                    py -= px_step;
                    let t = nearest_point(&vp, &[(&eq, &p)], px, py, 50.0)
                        .unwrap_or_else(|| panic!("{src}: lost the curve"));
                    assert!(t.distance_px < 2.0, "{src} {px_step}: {t:?}");
                    assert!(matches!(t.value, TraceValue::Defined { .. }), "{t:?}");
                    // The cursor follows the traced point across the line.
                    px = t.screen_x;
                    ys.push(t.screen_y);
                }
                assert!(
                    ys.windows(2).all(|w| w[1] < w[0]),
                    "{src} {px_step}: {ys:?}"
                );
                let travelled = ys[0] - ys[20];
                assert!(
                    (travelled - 20.0 * px_step).abs() < 3.0,
                    "{src} {px_step}: {travelled}"
                );
            }
        }
    }

    #[test]
    fn traces_implicit_circle() {
        let vp = vp();
        let (eq, p) = setup("x^2 + y^2 = 25", &vp);
        let (px, py) = vp.to_screen(4.0, 4.0);
        let t = nearest_point(&vp, &[(&eq, &p)], px, py, 100.0).unwrap();
        assert!(((t.x * t.x + t.y * t.y).sqrt() - 5.0).abs() < 1e-6, "{t:?}");
        assert!((t.x - t.y).abs() < 0.05);
        assert_eq!(t.value, TraceValue::Approximate);
        assert_eq!(t.text(), "(≈3.54, ≈3.54)");
    }

    #[test]
    fn picks_the_closest_equation_and_respects_radius() {
        let vp = vp();
        let (a, pa) = setup("y = 1", &vp);
        let (b, pb) = setup("y = 3", &vp);
        let (px, py) = vp.to_screen(0.0, 2.6);
        let t = nearest_point(&vp, &[(&a, &pa), (&b, &pb)], px, py, 100.0).unwrap();
        assert_eq!(t.index, 1);
        assert!(nearest_point(&vp, &[(&a, &pa)], px, py, 10.0).is_none());
    }

    #[test]
    fn trace_text() {
        assert_eq!(format_trace_value(1.23456, -0.5, 0.01), "(1.23, −0.50)");
        assert_eq!(format_trace_value(-0.0001, 2.0, 0.01), "(0.00, 2.00)");
        assert_eq!(format_trace_value(1234.5, 2.0, 1.0), "(1235, 2)");
        assert_eq!(format_coordinate(3.55e-15, 1e-17), "3.55×10⁻¹⁵");
        assert_eq!(format_coordinate(-3.5e-15, 1e-17), "−3.5×10⁻¹⁵");
        assert_eq!(format_coordinate(1e-20, 1e-17), "0");
    }
}
