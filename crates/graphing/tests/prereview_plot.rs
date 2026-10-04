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

/// The range of the drawn curves along the dependent axis, clipped to the
/// view.
fn drawn_extent(p: &graphing::Plot, vp: &Viewport) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for c in &p.curves {
        if c.len() < 2 {
            continue;
        }
        for q in c {
            lo = lo.min(q.y.clamp(vp.y_min, vp.y_max));
            hi = hi.max(q.y.clamp(vp.y_min, vp.y_max));
        }
    }
    (lo, hi)
}

/// PREREVIEW_C M3, PREREVIEW_B B-M3: lines steeper than about 10¹⁹ per
/// pixel collapsed to a point at most canvas sizes (both band crossings
/// rounded to the same spot of the chord), and so did overflowing ones.
#[test]
fn very_steep_lines_cross_the_view_at_every_size() {
    let srcs = [
        "y=10^17*x",
        "y=5*10^18*x",
        "y=10^19*x",
        "y=10^20*x",
        "y=x*10^20",
        "y=-10^20*x",
        "y=10^20*x+1",
        "y=10^21*x",
        "y=10^25*x",
        "y=10^30*x",
        "y=10^300*x",
        "y=sinh(10^20*x)",
        "y=10^300*x^3",
    ];
    let mut g = Graph::new();
    let ids: Vec<_> = srcs.iter().map(|s| g.add_equation(s)).collect();
    let mut sizes = 0;
    for w in (300..=1900).step_by(67) {
        for h in (300..=1100).step_by(43) {
            let vp = Viewport::default_for_size(w as f64, h as f64);
            sizes += 1;
            for (src, &id) in srcs.iter().zip(&ids) {
                let p = g.plot_equation(id, &vp).unwrap();
                let (lo, hi) = drawn_extent(&p, &vp);
                assert!(
                    lo <= vp.y_min && hi >= vp.y_max,
                    "{src} at {w}x{h}: drawn over [{lo}, {hi}] of {:?}",
                    (vp.y_min, vp.y_max)
                );
            }
        }
    }
    assert!(sizes > 400);
    // x = g(y) alike.
    let mut g = Graph::new();
    let id = g.add_equation("x=10^25*y");
    let vp = Viewport::default_for_size(761.0, 700.0);
    let p = g.plot_equation(id, &vp).unwrap();
    let (lo, hi) = p
        .curves
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), q| {
            (a.min(q.x), b.max(q.x))
        });
    assert!(lo <= vp.x_min && hi >= vp.x_max, "[{lo}, {hi}]");
}

/// M3 tracing: Shift+Up (1 px) and Up (5 px) from the centre move the
/// traced point up a very steep line by about the step, as the apps'
/// keyboard tracing does, at several canvas sizes.
#[test]
fn keyboard_tracing_climbs_very_steep_lines() {
    for src in ["y=10^20*x", "y=10^25*x", "y=10^300*x", "y=-10^20*x+3"] {
        let mut g = Graph::new();
        g.add_equation(src);
        for (w, h) in [
            (378.0, 644.0),
            (760.0, 700.0),
            (761.0, 700.0),
            (1000.0, 700.0),
            (1919.0, 1080.0),
        ] {
            let vp = Viewport::default_for_size(w, h);
            let plots = g.plot_parallel(&vp);
            for step in [1.0, 5.0] {
                let (mut px, mut py) = (w / 2.0, h / 2.0);
                let mut ys = Vec::new();
                for _ in 0..21 {
                    py -= step;
                    let (_, t) = g
                        .trace(&vp, &plots, px, py, 50.0)
                        .unwrap_or_else(|| panic!("{src} {w}x{h}: lost the curve"));
                    assert!(t.distance_px < 2.0, "{src} {w}x{h}: {t:?}");
                    assert!(!t.text().contains("unknown"), "{src} {w}x{h}: {}", t.text());
                    px = t.screen_x;
                    ys.push(t.screen_y);
                }
                assert!(ys.windows(2).all(|p| p[1] < p[0]), "{src} {w}x{h} {step}: {ys:?}");
                let travelled = ys[0] - ys[20];
                assert!(
                    (travelled - 20.0 * step).abs() < 3.0,
                    "{src} {w}x{h} {step}: {travelled}"
                );
            }
        }
    }
}

/// Segments that join across a point where f is undefined, or (wider than
/// a pixel) stray far from f between their ends: reviewer B's probe.
fn suspicious_joins(
    g: &Graph,
    id: graphing::EquationId,
    p: &graphing::Plot,
    vp: &Viewport,
) -> Vec<String> {
    let mut out = Vec::new();
    let (xpp, ypp) = (vp.x_per_px(), vp.y_per_px());
    for c in &p.curves {
        for w in c.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a.x == b.x {
                continue;
            }
            let wide = (b.x - a.x).abs() > xpp;
            for k in 1..64 {
                let x = a.x + (b.x - a.x) * k as f64 / 64.0;
                let y = g.evaluate(id, x).unwrap_or(f64::NAN);
                let lin = a.y + (b.y - a.y) * (x - a.x) / (b.x - a.x);
                let far = (y < a.y.min(b.y) - 50.0 * ypp || y > a.y.max(b.y) + 50.0 * ypp)
                    && (y - lin).abs() > 50.0 * ypp
                    && y.abs() < 1e300;
                if !y.is_finite() || (wide && far) {
                    out.push(format!("{a:?} -> {b:?}: f({x}) = {y}"));
                    break;
                }
            }
        }
    }
    out
}

/// PREREVIEW_C M2: removable holes at the default view of many canvas
/// sizes (the per-curve budget ran out at most of them and the fallback
/// joined across the hole, unmarked).
#[test]
fn holes_marked_and_unjoined_at_every_size() {
    for (src, hole, val) in [
        ("y=x/x", 0.0, 1.0),
        ("y=(x^2-1)/(x-1)", 1.0, 2.0),
        ("y=sin(x)/x", 0.0, 1.0),
        ("y=x^2/x^2", 0.0, 1.0),
        ("y=(x^3-8)/(x-2)", 2.0, 12.0),
    ] {
        let mut g = Graph::new();
        let id = g.add_equation(src);
        let mut sizes = 0;
        for w in (300..=1900).step_by(53) {
            for h in (300..=1100).step_by(61) {
                let vp = Viewport::default_for_size(w as f64, h as f64);
                if !vp.contains(hole, val) {
                    continue;
                }
                sizes += 1;
                let p = g.plot_equation(id, &vp).unwrap();
                assert!(
                    p.holes
                        .iter()
                        .any(|q| q.x == hole && (q.y - val).abs() < 1e-4),
                    "{src} at {w}x{h}: holes {:?}",
                    p.holes
                );
                for c in &p.curves {
                    for s in c.windows(2) {
                        let (a, b) = (s[0].x.min(s[1].x), s[0].x.max(s[1].x));
                        assert!(!(a < hole && hole < b), "{src} at {w}x{h}: {s:?}");
                    }
                }
                let bad = suspicious_joins(&g, id, &p, &vp);
                assert!(bad.is_empty(), "{src} at {w}x{h}: {bad:?}");
            }
        }
        assert!(sizes > 50, "{src}: {sizes}");
    }
}

/// PREREVIEW_B B-M1: no open circle inside an interval where f is
/// undefined (a run of unproven boxes as wide as the gap in sqrt(x²−4),
/// or one with a box proven undefined), nor at a pole (x! at −13, where
/// the values either side meet a hundred-thousandth of a pixel out); the
/// true holes stay, one per integer for (x − ⌊x⌋)/(x − ⌊x⌋).
#[test]
fn holes_only_where_proven() {
    let none = [
        "y=sqrt(sin(x))",
        "y=sqrt(x^2-4)+3",
        "y=floor(x)/floor(x)-5",
        "y=sqrt(cos(x))",
        "y=asin(2*sin(x))",
        "y=sqrt(sin(2*x))",
        "y=x!",
        "y=1/x",
    ];
    let sizes = [
        (378.0, 644.0),
        (760.0, 700.0),
        (1000.0, 700.0),
        (1920.0, 1080.0),
        (882.0, 300.0),
    ];
    for src in none {
        let mut g = Graph::new();
        let id = g.add_equation(src);
        for (w, h) in sizes {
            let vp = Viewport::default_for_size(w, h);
            let p = g.plot_equation(id, &vp).unwrap();
            assert!(p.holes.is_empty(), "{src} at {w}x{h}: {:?}", p.holes);
        }
    }
    let mut g = Graph::new();
    let id = g.add_equation("y=(x-floor(x))/(x-floor(x))");
    for (w, h) in sizes {
        let vp = Viewport::default_for_size(w, h);
        let p = g.plot_equation(id, &vp).unwrap();
        let ints: Vec<f64> = ((vp.x_min.ceil() as i64)..=(vp.x_max.floor() as i64))
            .map(|k| k as f64)
            .collect();
        let mut xs: Vec<f64> = p.holes.iter().map(|q| q.x).collect();
        xs.sort_by(f64::total_cmp);
        assert_eq!(xs, ints, "at {w}x{h}");
        assert!(p.holes.iter().all(|q| q.y == 1.0), "{:?}", p.holes);
    }
    // x! far out: between its poles it underflows to ±0 in doubles, but
    // the poles are poles, not holes.
    let mut g = Graph::new();
    let id = g.add_equation("y=x!");
    for half in [100.0, 1000.0] {
        let vp = Viewport::new(-half, half, -half, half, 380.0, 645.0);
        let p = g.plot_equation(id, &vp).unwrap();
        assert!(p.holes.is_empty(), "±{half}: {:?}", p.holes);
    }
    // nCr(x, 2) is undefined at the negative integers (Γ at a pole in its
    // definition) while x(x − 1)/2 isn't: holes there.
    let mut g = Graph::new();
    let id = g.add_equation("y=nCr(x,2)");
    let p = g
        .plot_equation(id, &Viewport::default_for_size(760.0, 700.0))
        .unwrap();
    for k in 1..=4 {
        let x = -(k as f64);
        let want = x * (x - 1.0) / 2.0;
        assert!(
            p.holes.iter().any(|q| q.x == x && (q.y - want).abs() < 1e-3),
            "{x}: {:?}",
            p.holes
        );
    }
    // 1/x! is undefined at the negative integers (Γ's poles) with 0 the
    // limit: holes, where its slope lets the values either side meet.
    let mut g = Graph::new();
    let id = g.add_equation("y=1/x!");
    let p = g
        .plot_equation(id, &Viewport::default_for_size(760.0, 700.0))
        .unwrap();
    assert!(!p.holes.is_empty());
    for q in &p.holes {
        assert!(q.x < 0.0 && q.x == q.x.trunc() && q.y.abs() < 1e-3, "{q:?}");
    }
}

/// PREREVIEW_B B-M2: factorials, nCr/nPr in x and dense poles are drawn
/// without a join across a pole or a point where f is undefined, and with
/// their shapes (x! was a few segments zig-zagging over its poles).
#[test]
fn factorials_and_dense_poles_never_join_across() {
    let cases: [(&str, f64, f64, f64); 14] = [
        ("y=x!", 10.0, 380.0, 645.0),
        ("y=1/x!", 10.0, 380.0, 645.0),
        ("y=(x/2)!", 10.0, 1000.0, 700.0),
        ("y=nCr(x,2)", 10.0, 380.0, 645.0),
        ("y=nPr(x,2)", 10.0, 1000.0, 700.0),
        ("y=tan(30*x)", 10.0, 380.0, 645.0),
        ("y=tan(50*x)", 10.0, 1000.0, 700.0),
        ("y=tan(100*x)", 10.0, 1000.0, 700.0),
        ("y=1/sin(30*x)", 10.0, 1000.0, 700.0),
        ("y=tan(x)", 1000.0, 1920.0, 1080.0),
        ("y=tan(10*x)", 100.0, 1000.0, 700.0),
        ("y=tan(x^2)", 100.0, 1000.0, 700.0),
        ("y=tan(30*x)", 1000.0, 1000.0, 700.0),
        ("y=x!", 100.0, 1000.0, 700.0),
    ];
    for (src, half, w, h) in cases {
        let mut g = Graph::new();
        let id = g.add_equation(src);
        let yh = half * h / w;
        let vp = Viewport::new(-half, half, -yh, yh, w, h);
        let p = g.plot_equation(id, &vp).unwrap();
        let bad = suspicious_joins(&g, id, &p, &vp);
        assert!(bad.is_empty(), "{src} ±{half} {w}x{h}: {bad:?}");
        assert!(p.point_count() > 200, "{src}: {} points", p.point_count());
    }
    // x! between its poles is resolved, and traces as a value, not
    // "unknown".
    let mut g = Graph::new();
    g.add_equation("y=x!");
    let vp = Viewport::default_for_size(760.0, 700.0);
    let plots = g.plot_parallel(&vp);
    assert!(!plots[0].plot.has_missing_data);
    // 1.32! = Γ(2.32) ≈ 1.181.
    let (sx, sy) = vp.to_screen(1.32, 1.181);
    let (_, t) = g.trace(&vp, &plots, sx, sy, 50.0).unwrap();
    assert!(t.text().starts_with("(1.32, ") && !t.text().contains("unknown"), "{}", t.text());
}

/// PREREVIEW_B Lows: keyboard Left along √x reaches its end (0, 0) (the
/// cursor keeps the traced point's height, as the apps' keyboard tracing
/// does); exactly known values carry no "≈"; minus signs are U+2212; the
/// text for a screen reader says "approximately".
#[test]
fn keyboard_tracing_reaches_domain_ends_and_texts_are_exact() {
    for (w, h) in [(378.0, 644.0), (760.0, 700.0), (1000.0, 700.0)] {
        let mut g = Graph::new();
        g.add_equation("y=sqrt(x)");
        let vp = Viewport::default_for_size(w, h);
        let plots = g.plot_parallel(&vp);
        let (mut px, mut py) = vp.to_screen(1.0, 1.0);
        let mut last = String::new();
        for _ in 0..60 {
            px = (px - 5.0).max(0.0);
            if let Some((_, t)) = g.trace(&vp, &plots, px, py, 50.0) {
                py = t.screen_y;
                last = t.text();
            }
        }
        assert_eq!(last, "(0.00, 0.00)", "at {w}x{h}");
    }
    let vp = Viewport::default_for_size(760.0, 700.0);
    for (src, x, text, spoken) in [
        ("y=x/x-1", 0.26, "(0.26, 0.00)", "(0.26, 0.00)"),
        ("y=x-1", 0.5, "(0.50, −0.50)", "(0.50, −0.50)"),
        ("y=-x/3", 0.5, "(0.50, ≈−0.17)", "(0.50, approximately −0.17)"),
    ] {
        let mut g = Graph::new();
        let id = g.add_equation(src);
        let plots = g.plot_parallel(&vp);
        let y = g.evaluate(id, x).unwrap();
        let (sx, sy) = vp.to_screen(x, y);
        let (_, t) = g.trace(&vp, &plots, sx, sy, 50.0).unwrap();
        assert_eq!(t.text(), text, "{src}");
        assert_eq!(t.spoken_text(), spoken, "{src}");
    }
}
