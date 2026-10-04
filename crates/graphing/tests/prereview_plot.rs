//! Regressions from the v0.2.0 pre-reviews (plotting and graph UI).

use graphing::viewport::{ChangeRangeAction, MAX_COORD, RangeError};
use graphing::{Graph, Viewport};

fn finite_plot(g: &Graph, vp: &Viewport) {
    for p in g.plot_parallel(vp) {
        for l in p.plot.curves.iter().chain(&p.plot.fill) {
            for q in l {
                assert!(q.x.is_finite() && q.y.is_finite(), "{q:?} in {vp:?}");
            }
        }
    }
}

/// PREREVIEW_C M1: panning a view near the largest double made it
/// infinite, then NaN, and the inequality fill panicked on NaN bounds.
#[test]
fn pans_near_the_float_limit_stay_finite() {
    let mut vp = Viewport::default_for_size(760.0, 700.0);
    let before = vp;
    assert_eq!(
        vp.set_display_ranges(-10.0, 10.0, 1.797693134e308, 1.7976931348e308),
        Err(RangeError::OutOfRange)
    );
    assert_eq!(vp, before);
    let mut g = Graph::new();
    for src in ["y<x", "y>x^2", "y<=sin(x)", "x<y^2"] {
        g.add_equation(src);
    }
    // The largest accepted views, on either axis, dragged outwards.
    let near = MAX_COORD * (1.0 - 1e-9);
    for (x, y) in [
        ((-10.0, 10.0), (near - 1e296, near)),
        ((near - 1e296, near), (-10.0, 10.0)),
        ((-10.0, 10.0), (-near, -near + 1e296)),
        ((-near, -near + 1e296), (-10.0, 10.0)),
    ] {
        let mut vp = Viewport::default_for_size(760.0, 700.0);
        vp.set_display_ranges(x.0, x.1, y.0, y.1).unwrap();
        for (dx, dy) in [(0.0, 100.0), (5.0, 0.0), (-100.0, 0.0), (0.0, -100.0)] {
            for _ in 0..3 {
                vp.pan_pixels(dx, dy);
                assert!(vp.is_sane(), "{vp:?}");
                finite_plot(&g, &vp);
            }
        }
        for a in [
            ChangeRangeAction::MovePositiveY,
            ChangeRangeAction::MoveNegativeY,
            ChangeRangeAction::MovePositiveX,
            ChangeRangeAction::MoveNegativeX,
            ChangeRangeAction::ZoomOut,
        ] {
            for _ in 0..40 {
                vp.apply(a);
            }
            assert!(vp.is_sane(), "{a:?}: {vp:?}");
            finite_plot(&g, &vp);
        }
    }
    // A viewport built field by field without the checks plots nothing.
    for vp in [
        Viewport::new(-10.0, 10.0, f64::NAN, f64::INFINITY, 760.0, 700.0),
        Viewport::new(-10.0, 10.0, 1.7e308, f64::MAX, 760.0, 700.0),
        Viewport::new(f64::NEG_INFINITY, 10.0, -1.0, 1.0, 760.0, 700.0),
    ] {
        assert!(!vp.is_sane());
        for p in g.plot_parallel(&vp) {
            assert!(p.plot.curves.is_empty() && p.plot.fill.is_empty());
            assert!(p.plot.has_missing_data);
        }
    }
}

/// M1 fuzz: random pans, moves, zooms and wheel steps from views across
/// the whole range never leave a sane viewport, and every plot of an
/// inequality on either axis is finite.
#[test]
fn navigation_fuzz_keeps_views_sane() {
    let mut g = Graph::new();
    for src in ["y<x", "x>y^2", "y>=1/x"] {
        g.add_equation(src);
    }
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    for start in [1.0, 1e100, 1e250, 1e299, 1e305, MAX_COORD * 0.999] {
        let mut vp = Viewport::default_for_size(640.0, 480.0);
        let span = (start * 1e-7).max(1.0);
        let c = start * if rnd() < 0.5 { -1.0 } else { 1.0 };
        if vp.set_display_ranges(c - span, c + span, -c - span, -c + span).is_err() {
            vp.set_display_ranges(-span, span, c - span, c + span).unwrap();
        }
        for _ in 0..200 {
            let r = rnd();
            if r < 0.4 {
                vp.pan_pixels((rnd() - 0.5) * 4000.0, (rnd() - 0.5) * 4000.0);
            } else if r < 0.6 {
                vp.wheel_zoom(rnd() * 640.0, rnd() * 480.0, (rnd() - 0.5) * 2400.0);
            } else if r < 0.8 {
                vp.move_by_ratio((rnd() - 0.5) * 8.0, (rnd() - 0.5) * 8.0);
            } else {
                vp.zoom_about_pixel(rnd() * 640.0, rnd() * 480.0, 0.01 + rnd() * 100.0);
            }
            assert!(vp.is_sane(), "{vp:?}");
        }
        finite_plot(&g, &vp);
    }
}
