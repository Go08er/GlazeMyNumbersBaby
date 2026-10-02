//! Plot geometry for a viewport, in world (graph) coordinates.
//!
//! * Explicit curves (`y = f(x)`, `x = g(y)`) are sampled adaptively with
//!   discontinuity detection, so polylines break at poles and jumps
//!   (`tan x`, `1/x`, `floor x`) instead of drawing vertical connectors,
//!   while steep-but-continuous parts (`atan(10⁶x)`) stay connected.
//! * Implicit relations are contoured with marching squares on a
//!   two-level lattice (coarse blocks, refined where the sign changes).
//! * Inequalities produce fill polygons plus the boundary curve; strict
//!   inequalities report a dashed boundary.
//!
//! All coordinates are finite: curves are clipped to a band one viewport
//! tall above and below the visible area, so they can be stroked directly.

mod explicit;
mod implicit;

use crate::equation::{Axis, CompiledEquation, CompiledForm, EquationKind};
use crate::viewport::Viewport;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) use explicit::ExplicitSampler;

/// A point in world coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    /// Creates a point.
    pub fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }
}

/// A connected run of points, to be stroked as one path.
pub type Polyline = Vec<Point>;

/// Tuning knobs for plotting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlotOptions {
    /// Initial sample spacing for explicit curves, in pixels.
    pub seed_px: f64,
    /// Maximum deviation from a straight segment before refining, in pixels.
    pub tolerance_px: f64,
    /// Maximum refinement depth below the seed spacing (2^depth subdivisions).
    pub max_depth: u32,
    /// Evaluation budget per explicit curve; when exhausted the remaining
    /// segments are drawn unrefined and `has_missing_data` is set.
    pub max_evals: usize,
    /// Size of a fine marching-squares cell for implicit relations, in pixels.
    pub implicit_cell_px: f64,
    /// Number of fine cells per coarse block side.
    pub implicit_block: usize,
    /// Most fine cells refined for one implicit relation (where its sign
    /// changes). A relation that changes sign nearly everywhere would need
    /// the whole lattice; past this the cells grow coarser instead.
    /// [`crate::Graph`] lowers it to share [`crate::graph::MAX_IMPLICIT_WORK`]
    /// between the relations plotted together.
    pub implicit_max_refined: f64,
}

impl Default for PlotOptions {
    fn default() -> Self {
        PlotOptions {
            seed_px: 1.0,
            tolerance_px: 0.25,
            max_depth: 6,
            max_evals: 200_000,
            implicit_cell_px: 2.0,
            implicit_block: 8,
            implicit_max_refined: 4.0e6,
        }
    }
}

/// Geometry for one equation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plot {
    /// Curves to stroke (for inequalities: the region boundary).
    pub curves: Vec<Polyline>,
    /// Closed polygons (counter-clockwise, non-overlapping) to fill for an
    /// inequality's region. Fill them all as one path.
    pub fill: Vec<Polyline>,
    /// True for strict inequalities (`<`, `>`): the boundary is not part of
    /// the region and is conventionally drawn dashed.
    pub boundary_dashed: bool,
    /// True if some part could not be resolved within the evaluation
    /// budget (`hasSomeMissingData` in the original renderer).
    pub has_missing_data: bool,
}

impl Plot {
    /// Total number of curve points (diagnostics).
    pub fn point_count(&self) -> usize {
        self.curves.iter().map(|c| c.len()).sum()
    }
}

/// Computes the geometry of a compiled equation for a viewport.
pub fn plot(eq: &CompiledEquation, vp: &Viewport, opts: &PlotOptions) -> Plot {
    plot_with(eq, vp, opts, &Cancel(None))
}

/// [`plot`], polling `cancel`: returns `None` soon after the flag becomes
/// true, so a worker thread can drop work for a viewport that is gone.
pub fn plot_cancellable(
    eq: &CompiledEquation,
    vp: &Viewport,
    opts: &PlotOptions,
    cancel: &AtomicBool,
) -> Option<Plot> {
    let c = Cancel(Some(cancel));
    let p = plot_with(eq, vp, opts, &c);
    (!c.is_set()).then_some(p)
}

/// An optional cancel flag, polled by the samplers.
#[derive(Clone, Copy)]
pub(crate) struct Cancel<'a>(pub(crate) Option<&'a AtomicBool>);

impl Cancel<'_> {
    #[inline]
    pub(crate) fn is_set(&self) -> bool {
        self.0.is_some_and(|c| c.load(Ordering::Relaxed))
    }
}

/// For an equation plotted by contouring a scalar field (marching squares)
/// rather than by sampling an explicit curve: the field's evaluation cost.
pub(crate) fn contour_cost(eq: &CompiledEquation) -> Option<usize> {
    match &eq.form {
        CompiledForm::Explicit { .. } => None,
        CompiledForm::Implicit { f } => Some(f.cost()),
        CompiledForm::Inequality { bound: Some(_), .. } => None,
        CompiledForm::Inequality { field, .. } => Some(field.cost()),
    }
}

pub(crate) fn plot_with(
    eq: &CompiledEquation,
    vp: &Viewport,
    opts: &PlotOptions,
    cancel: &Cancel<'_>,
) -> Plot {
    match &eq.form {
        CompiledForm::Explicit { axis, f } => {
            let mut s = ExplicitSampler::new(f, *axis, vp, opts);
            s.set_cancel(*cancel);
            s.run();
            Plot {
                curves: s.stroke_polylines(),
                fill: Vec::new(),
                boundary_dashed: false,
                has_missing_data: s.exhausted(),
            }
        }
        CompiledForm::Implicit { f } => {
            let r = implicit::contour(f, vp, opts, false, false, cancel);
            Plot {
                curves: r.curves,
                fill: Vec::new(),
                boundary_dashed: false,
                has_missing_data: r.missing,
            }
        }
        CompiledForm::Inequality {
            field,
            bound,
            strict,
            ..
        } => {
            if let Some(b) = bound {
                let mut s = ExplicitSampler::new(&b.f, b.axis, vp, opts);
                s.set_cancel(*cancel);
                s.run();
                Plot {
                    curves: s.stroke_polylines(),
                    fill: s.fill_polygons(b.greater),
                    boundary_dashed: *strict,
                    has_missing_data: s.exhausted(),
                }
            } else {
                let r = implicit::contour(field, vp, opts, true, *strict, cancel);
                Plot {
                    curves: r.curves,
                    fill: r.fill,
                    boundary_dashed: *strict,
                    has_missing_data: r.missing,
                }
            }
        }
    }
}

/// Signed area (positive for counter-clockwise in world coordinates).
pub(crate) fn signed_area(poly: &[Point]) -> f64 {
    let n = poly.len();
    let mut a = 0.0;
    for i in 0..n {
        let p = poly[i];
        let q = poly[(i + 1) % n];
        a += p.x * q.y - q.x * p.y;
    }
    0.5 * a
}

/// Total absolute area of a set of polygons (diagnostics / tests).
pub fn total_area(polys: &[Polyline]) -> f64 {
    polys.iter().map(|p| signed_area(p).abs()).sum()
}

/// Total length of a set of polylines (diagnostics / tests).
pub fn total_length(lines: &[Polyline]) -> f64 {
    lines
        .iter()
        .map(|l| {
            l.windows(2)
                .map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt())
                .sum::<f64>()
        })
        .sum()
}

/// Convenience for [`EquationKind`] users: whether geometry of this kind can
/// have a fill.
pub fn kind_has_fill(kind: EquationKind) -> bool {
    kind == EquationKind::Inequality
}

pub(crate) fn axis_point(axis: Axis, t: f64, d: f64) -> Point {
    match axis {
        Axis::X => Point { x: t, y: d },
        Axis::Y => Point { x: d, y: t },
    }
}
