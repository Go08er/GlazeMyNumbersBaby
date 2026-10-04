//! Certified explicit plotting: no join across a discontinuity, no
//! missing piece of a visible curve, every chord within the tolerance of
//! the curve, and proven holes reported, checked against densely sampled
//! values and the functions' known discontinuities.

use graphing::compile::CompileOptions;
use graphing::equation::Equation;
use graphing::plot::{PlotOptions, Polyline, plot};
use graphing::{Plot, Viewport};
use std::f64::consts::{FRAC_PI_2, PI};

fn vp() -> Viewport {
    Viewport::new(-10.0, 10.0, -10.0, 10.0, 800.0, 800.0)
}

fn geometry(src: &str, v: &Viewport) -> Plot {
    let eq = Equation::parse(src)
        .unwrap()
        .compile(&CompileOptions::default())
        .unwrap();
    plot(&eq, v, &PlotOptions::default())
}

/// No stroked segment spans one of `xs` (a pole or a jump).
fn no_join_across(lines: &[Polyline], xs: &[f64], src: &str) {
    for l in lines {
        for w in l.windows(2) {
            let (a, b) = (w[0].x.min(w[1].x), w[0].x.max(w[1].x));
            for &x in xs {
                assert!(
                    !(a < x && x < b),
                    "{src}: segment {:?} -> {:?} joins across x = {x}",
                    w[0],
                    w[1]
                );
            }
        }
    }
}

/// Every segment's chord is within `tol` pixels of f (sampled densely),
/// vertically, or for a segment under a pixel wide, f stays within its
/// ends' values ± tol.
fn chords_follow(lines: &[Polyline], f: impl Fn(f64) -> f64, v: &Viewport, src: &str) {
    let tol = 0.25 + 0.05; // the tolerance, and rounding
    for l in lines {
        for w in l.windows(2) {
            let (p, q) = (w[0], w[1]);
            let wide_px = (q.x - p.x).abs() / v.x_per_px();
            for k in 1..16 {
                let s = k as f64 / 16.0;
                let x = p.x + s * (q.x - p.x);
                let y = f(x);
                if !y.is_finite() {
                    continue;
                }
                // Off the visible area there's nothing to see.
                if y.max(p.y.max(q.y)) < v.y_min || y.min(p.y.min(q.y)) > v.y_max {
                    continue;
                }
                if wide_px <= 1.0 {
                    let (lo, hi) = (p.y.min(q.y), p.y.max(q.y));
                    let out = if y < lo {
                        lo - y
                    } else if y > hi {
                        y - hi
                    } else {
                        0.0
                    };
                    assert!(
                        out / v.y_per_px() <= tol,
                        "{src}: f({x}) = {y} outside the steep chord {p:?} -> {q:?}"
                    );
                } else {
                    let chord = p.y + s * (q.y - p.y);
                    assert!(
                        (y - chord).abs() / v.y_per_px() <= tol,
                        "{src}: f({x}) = {y}, chord {chord} ({p:?} -> {q:?})"
                    );
                }
            }
        }
    }
}

/// Every visible stretch where f is defined is covered by some segment,
/// apart from `skip` (within a pixel of a discontinuity).
fn covers_visible(
    lines: &[Polyline],
    f: impl Fn(f64) -> f64,
    v: &Viewport,
    skip: &[f64],
    src: &str,
) {
    let mut spans: Vec<(f64, f64)> = lines
        .iter()
        .flat_map(|l| {
            l.windows(2)
                .map(|w| (w[0].x.min(w[1].x), w[0].x.max(w[1].x)))
        })
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let px = v.x_per_px();
    let n = 4000;
    for i in 0..=n {
        let x = v.x_min + (v.x_max - v.x_min) * i as f64 / n as f64;
        let y = f(x);
        if !y.is_finite() || y < v.y_min || y > v.y_max {
            continue;
        }
        if skip.iter().any(|&s| (x - s).abs() < px) {
            continue;
        }
        assert!(
            spans.iter().any(|&(a, b)| a <= x && x <= b),
            "{src}: the curve at x = {x} (y = {y}) isn't drawn"
        );
    }
}

#[test]
fn tan_breaks_at_every_pole_and_nowhere_else() {
    let v = vp();
    let p = geometry("y = tan(x)", &v);
    let poles: Vec<f64> = (-4..=3).map(|k| FRAC_PI_2 + k as f64 * PI).collect();
    no_join_across(&p.curves, &poles, "tan");
    chords_follow(&p.curves, f64::tan, &v, "tan");
    covers_visible(&p.curves, f64::tan, &v, &poles, "tan");
    assert!(p.holes.is_empty());
}

#[test]
fn reciprocal_and_shifted_poles() {
    let v = vp();
    for (src, pole) in [("y = 1/x", 0.0), ("y = 1/(x-0.0123)", 0.0123)] {
        let p = geometry(src, &v);
        let f = |x: f64| 1.0 / (x - pole);
        no_join_across(&p.curves, &[pole], src);
        chords_follow(&p.curves, f, &v, src);
        covers_visible(&p.curves, f, &v, &[pole], src);
    }
}

#[test]
fn floor_breaks_at_every_integer() {
    let v = vp();
    let p = geometry("y = floor(x)", &v);
    let ints: Vec<f64> = (-10..=10).map(f64::from).collect();
    no_join_across(&p.curves, &ints, "floor");
    chords_follow(&p.curves, f64::floor, &v, "floor");
    covers_visible(&p.curves, f64::floor, &v, &ints, "floor");
}

#[test]
fn removable_holes_are_gaps_with_markers() {
    let v = vp();
    let p = geometry("y = (x^2-1)/(x-1)", &v);
    no_join_across(&p.curves, &[1.0], "hole");
    chords_follow(&p.curves, |x| (x * x - 1.0) / (x - 1.0), &v, "hole");
    assert_eq!(p.holes.len(), 1, "{:?}", p.holes);
    assert_eq!(p.holes[0].x, 1.0);
    assert!((p.holes[0].y - 2.0).abs() < 1e-6, "{:?}", p.holes);

    let p = geometry("y = x/x", &v);
    no_join_across(&p.curves, &[0.0], "x/x");
    assert_eq!(p.holes.len(), 1, "{:?}", p.holes);
    assert_eq!((p.holes[0].x, p.holes[0].y), (0.0, 1.0));

    // A pole is not a hole.
    assert!(geometry("y = 1/x", &v).holes.is_empty());
}

#[test]
fn domain_edges_are_reached() {
    let v = vp();
    let p = geometry("y = sqrt(x)", &v);
    assert_eq!(p.curves.len(), 1);
    let first = p.curves[0][0];
    assert!(first.x >= 0.0 && first.x < 1e-6, "{first:?}");
    chords_follow(&p.curves, f64::sqrt, &v, "sqrt");
    covers_visible(&p.curves, f64::sqrt, &v, &[0.0], "sqrt");

    let p = geometry("y = sqrt(4 - x^2)", &v);
    assert_eq!(p.curves.len(), 1);
    let l = &p.curves[0];
    assert!((l[0].x + 2.0).abs() < 1e-6 && (l[l.len() - 1].x - 2.0).abs() < 1e-6);
}

#[test]
fn steep_continuous_stays_joined() {
    let v = vp();
    for src in ["y = arctan(1000000x)", "y = x^(1/3)", "y = 1000000000*x"] {
        let p = geometry(src, &v);
        let visible: Vec<_> = p.curves.iter().filter(|l| l.len() >= 2).collect();
        assert_eq!(visible.len(), 1, "{src}: {} pieces", visible.len());
    }
    let p = geometry("y = arctan(1000000x)", &v);
    chords_follow(&p.curves, |x| (1e6 * x).atan(), &v, "atan");
}

#[test]
fn smooth_curves_are_within_tolerance() {
    let v = vp();
    for (src, f) in [
        ("y = sin(x)", f64::sin as fn(f64) -> f64),
        ("y = e^(-x^2)*cos(4x)", |x: f64| {
            (-x * x).exp() * (4.0 * x).cos()
        }),
        ("y = x^3 - 3x", |x: f64| x * x * x - 3.0 * x),
    ] {
        let p = geometry(src, &v);
        assert_eq!(p.curves.len(), 1, "{src}");
        assert!(!p.has_missing_data, "{src}");
        chords_follow(&p.curves, f, &v, src);
        covers_visible(&p.curves, f, &v, &[], src);
    }
}

#[test]
fn endless_oscillation_breaks_at_its_pole_and_fills_its_columns() {
    let v = vp();
    let p = geometry("y = sin(1/x)", &v);
    no_join_across(&p.curves, &[0.0], "sin(1/x)");
    // Near 0 it oscillates faster than a pixel: each column is stroked
    // through its true extremes, ±1 (to the tolerance), rather than through
    // whatever two samples happened to land on.
    for side in [-1.0, 1.0] {
        let near: Vec<f64> = p
            .curves
            .iter()
            .flatten()
            .filter(|q| q.x * side > 0.0 && q.x.abs() < 0.05)
            .map(|q| q.y)
            .collect();
        let (lo, hi) = near
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &y| {
                (a.min(y), b.max(y))
            });
        assert!(lo < -0.99 && hi > 0.99, "{side}: [{lo}, {hi}]");
    }
    // Away from 0 it's drawn and accurate.
    let far: Vec<Polyline> = p
        .curves
        .iter()
        .map(|l| {
            l.iter()
                .copied()
                .filter(|q| q.x.abs() > 0.5)
                .collect::<Vec<_>>()
        })
        .filter(|l| l.len() >= 2)
        .collect();
    chords_follow(&far, |x| (1.0 / x).sin(), &v, "sin(1/x)");
}

#[test]
fn a_spike_between_samples_is_drawn() {
    // 0.0004 units wide: far narrower than a pixel (0.025), so point
    // samples a pixel apart never see it; the enclosures do.
    let v = vp();
    let src = "y = 5*e^(-((x-0.30007)/0.0001)^2)";
    let p = geometry(src, &v);
    let top = p
        .curves
        .iter()
        .flatten()
        .map(|q| q.y)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(top > 4.9, "{src}: top {top}");
}

#[test]
fn inequality_bounds_are_certified_too() {
    let v = vp();
    let p = geometry("y < tan(x)", &v);
    let poles: Vec<f64> = (-4..=3).map(|k| FRAC_PI_2 + k as f64 * PI).collect();
    no_join_across(&p.curves, &poles, "y < tan(x)");
    assert!(!p.fill.is_empty());
}
