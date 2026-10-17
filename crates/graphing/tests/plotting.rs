//! Plot geometry through the public API.

use graphing::compile::CompileOptions;
use graphing::equation::Equation;
use graphing::functions::TrigUnit;
use graphing::grid::Grid;
use graphing::plot::{PlotOptions, Polyline, plot, total_area, total_length};
use graphing::{Graph, Viewport};
use std::f64::consts::PI;

fn vp() -> Viewport {
    Viewport::new(-10.0, 10.0, -10.0, 10.0, 1000.0, 1000.0)
}

fn geometry(src: &str, v: &Viewport) -> graphing::Plot {
    let eq = Equation::parse(src)
        .unwrap()
        .compile(&CompileOptions::default())
        .unwrap();
    plot(&eq, v, &PlotOptions::default())
}

fn all_finite(lines: &[Polyline]) -> bool {
    lines
        .iter()
        .all(|l| l.iter().all(|p| p.x.is_finite() && p.y.is_finite()))
}

/// Largest vertical jump (in pixels) of any stroked segment that is less
/// than a pixel wide and crosses the visible area.
fn max_vertical_connector(lines: &[Polyline], v: &Viewport) -> f64 {
    let mut worst: f64 = 0.0;
    for l in lines {
        for w in l.windows(2) {
            let dx = (w[1].x - w[0].x).abs() / v.x_per_px();
            let lo = w[0].y.min(w[1].y).max(v.y_min);
            let hi = w[0].y.max(w[1].y).min(v.y_max);
            if dx < 1.0 && hi > lo {
                worst = worst.max((hi - lo) / v.y_per_px());
            }
        }
    }
    worst
}

#[test]
fn tan_has_no_connectors_across_asymptotes() {
    let v = vp();
    let p = geometry("y = tan(x)", &v);
    assert!(all_finite(&p.curves));
    assert_eq!(p.curves.len(), 7);
    // Each branch crosses the view top to bottom, but within a branch the
    // steepest sub-pixel segment stays far from a full-height connector.
    assert!(
        max_vertical_connector(&p.curves, &v) < 300.0,
        "{}",
        max_vertical_connector(&p.curves, &v)
    );
}

#[test]
fn reciprocal_and_shifted_poles_break() {
    let v = vp();
    for src in ["1/x", "1/(x - 0.0137)", "1/(x+3)^2", "x/(x^2-1)"] {
        let p = geometry(src, &v);
        assert!(all_finite(&p.curves), "{src}");
        assert!(max_vertical_connector(&p.curves, &v) < 500.0, "{src}");
    }
    assert_eq!(geometry("1/x", &v).curves.len(), 2);
    assert_eq!(geometry("x/(x^2-1)", &v).curves.len(), 3);
}

#[test]
fn steep_continuous_functions_stay_connected() {
    let v = vp();
    assert_eq!(geometry("arctan(10000000x)", &v).curves.len(), 1);
    assert_eq!(geometry("x^(1/3)", &v).curves.len(), 1);
    assert_eq!(geometry("100x", &v).curves.len(), 1);
}

#[test]
fn domain_edges_are_reached() {
    let v = vp();
    let p = geometry("sqrt(x)", &v);
    let first = p.curves[0][0];
    assert!(first.x.abs() < 1e-6 && first.y.abs() < 1e-3);
    let p = geometry("ln(x)", &v);
    assert_eq!(p.curves.len(), 1);
    // ln goes off the bottom of the view near 0.
    assert!(p.curves[0].iter().any(|q| q.y < -10.0));
    let p = geometry("sqrt(9 - x^2)", &v);
    let l = &p.curves[0];
    assert!((l[0].x + 3.0).abs() < 1e-6 && (l[l.len() - 1].x - 3.0).abs() < 1e-6);
}

#[test]
fn jumps_break_lines() {
    let v = vp();
    assert_eq!(geometry("floor(x)", &v).curves.len(), 22);
    assert_eq!(geometry("sign(x)", &v).curves.len(), 2);
    let p = geometry("x mod 3", &v);
    assert_eq!(p.curves.len(), 8);
    assert!(max_vertical_connector(&p.curves, &v) < 2.0);
}

#[test]
fn inverse_function_x_of_y() {
    let v = vp();
    let p = geometry("x = y^2 - 4", &v);
    assert_eq!(p.curves.len(), 1);
    // Interior points lie on the curve (the ends are clipped to the band).
    let l = &p.curves[0];
    for q in &l[1..l.len() - 1] {
        assert!((q.x - (q.y * q.y - 4.0)).abs() < 1e-6, "{q:?}");
    }
    let p = geometry("x = 3", &v);
    assert_eq!(p.curves.len(), 1);
    assert!(p.curves[0].iter().all(|q| q.x == 3.0));
}

#[test]
fn implicit_circle() {
    let v = vp();
    let p = geometry("x^2 + y^2 = 25", &v);
    assert_eq!(p.curves.len(), 1);
    let len = total_length(&p.curves);
    assert!((len - 10.0 * PI).abs() < 0.05, "{len}");
    for q in &p.curves[0] {
        assert!(((q.x * q.x + q.y * q.y).sqrt() - 5.0).abs() < 1e-3);
    }
    // Ellipse and hyperbola.
    let p = geometry("x^2/16 + y^2/4 = 1", &v);
    assert_eq!(p.curves.len(), 1);
    let p = geometry("x^2 - y^2 = 4", &v);
    assert_eq!(p.curves.len(), 2);
    // A heart-like relation with a cusp still produces a closed outline.
    let p = geometry(
        "(x^2 + y^2 - 1)^3 = x^2 y^3",
        &Viewport::new(-2.0, 2.0, -2.0, 2.0, 800.0, 800.0),
    );
    assert!(!p.curves.is_empty());
}

#[test]
fn inequality_regions() {
    let v = vp();
    // Disc: area 9π, dashed boundary (strict).
    let p = geometry("x^2 + y^2 < 9", &v);
    assert!(p.boundary_dashed);
    let area = total_area(&p.fill);
    assert!((area - 9.0 * PI).abs() / (9.0 * PI) < 0.01, "{area}");
    assert!((total_length(&p.curves) - 6.0 * PI).abs() < 0.1);
    // Non-strict: solid boundary.
    let p = geometry("x^2 + y^2 <= 9", &v);
    assert!(!p.boundary_dashed);

    // Explicit: y < x over [-10, 10]² is half the square (inside the view).
    let p = geometry("y < x", &v);
    assert!(p.boundary_dashed);
    let clipped: f64 = clip_area(&p.fill, &v);
    assert!((clipped - 200.0).abs() < 1.0, "{clipped}");
    // y ≥ x² region between the parabola and the top edge.
    let p = geometry("y >= x^2", &v);
    let a = clip_area(&p.fill, &v);
    let exact = 2.0 * (10.0 * 10f64.sqrt() - 10f64.powf(1.5) / 3.0);
    assert!((a - exact).abs() / exact < 0.01, "{a} vs {exact}");
    // Chained: 1 < y < 3 is a band of area 2·20.
    let p = geometry("1 < y < 3", &v);
    let a = clip_area(&p.fill, &v);
    assert!((a - 40.0).abs() < 0.5, "{a}");
    // x > sin(y): right of a curve.
    let p = geometry("x > sin(y)", &v);
    let a = clip_area(&p.fill, &v);
    assert!((a - 200.0).abs() < 1.0, "{a}");
    // Fill polygons are counter-clockwise.
    for poly in &p.fill {
        let mut s = 0.0;
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            s += a.x * b.y - b.x * a.y;
        }
        assert!(s > 0.0);
    }
}

/// Area of polygons clipped to the viewport (Sutherland–Hodgman).
fn clip_area(polys: &[Polyline], v: &Viewport) -> f64 {
    let mut total = 0.0;
    for p in polys {
        let mut pts: Vec<(f64, f64)> = p.iter().map(|q| (q.x, q.y)).collect();
        let edges: [(f64, u8, bool); 4] = [
            (v.x_min, 0, true),
            (v.x_max, 0, false),
            (v.y_min, 1, true),
            (v.y_max, 1, false),
        ];
        for (c, axis, keep_greater) in edges {
            let inside = |q: (f64, f64)| {
                let val = if axis == 0 { q.0 } else { q.1 };
                if keep_greater { val >= c } else { val <= c }
            };
            let mut out = Vec::new();
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                let (ia, ib) = (inside(a), inside(b));
                if ia {
                    out.push(a);
                }
                if ia != ib {
                    let (va, vb) = if axis == 0 { (a.0, b.0) } else { (a.1, b.1) };
                    let t = (c - va) / (vb - va);
                    out.push((a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)));
                }
            }
            pts = out;
            if pts.is_empty() {
                break;
            }
        }
        let mut s = 0.0;
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            s += a.0 * b.1 - b.0 * a.1;
        }
        total += 0.5 * s.abs();
    }
    total
}

#[test]
fn degrees_mode_changes_geometry() {
    let v = Viewport::new(-360.0, 360.0, -2.0, 2.0, 1000.0, 400.0);
    let eq = Equation::parse("y = sin(x)").unwrap();
    let c = eq
        .compile(&CompileOptions {
            trig_unit: TrigUnit::Degrees,
            variables: &(),
        })
        .unwrap();
    let p = plot(&c, &v, &PlotOptions::default());
    assert_eq!(p.curves.len(), 1);
    let top = p.curves[0].iter().map(|q| q.y).fold(f64::MIN, f64::max);
    assert!((top - 1.0).abs() < 1e-3);
    let at90 = p.curves[0]
        .iter()
        .min_by(|a, b| (a.x - 90.0).abs().total_cmp(&(b.x - 90.0).abs()))
        .unwrap();
    assert!((at90.y - 1.0).abs() < 1e-3);
}

#[test]
fn sliders_change_the_graph() {
    let mut g = Graph::new();
    let id = g.add_equation("y = a x + b");
    let v = vp();
    g.set_variable("a", 0.0);
    g.set_variable("b", 2.0);
    let p = g.plot_equation(id, &v).unwrap();
    assert!(p.curves[0].iter().all(|q| (q.y - 2.0).abs() < 1e-12));
}

#[test]
fn grid_for_default_view() {
    let v = Viewport::default_for_size(1000.0, 500.0);
    let g = Grid::for_viewport(&v);
    assert_eq!(g.y.major_step, 5.0);
    assert!(g.x.major.iter().any(|t| t.label == "0"));
    assert!(g.x.major.iter().all(|t| !t.label.contains('e')));
}

#[test]
fn pathological_inputs_stay_bounded() {
    let v = vp();
    for src in [
        "sin(1/x)",
        "sin(1000x)",
        "tan(x^2)",
        "x!",
        "floor(1/x)",
        "sqrt(-1)",
        "1/0x",
    ] {
        let Ok(eq) = Equation::parse(src) else {
            continue;
        };
        let Ok(c) = eq.compile(&CompileOptions::default()) else {
            continue;
        };
        let t = std::time::Instant::now();
        let p = plot(&c, &v, &PlotOptions::default());
        assert!(all_finite(&p.curves), "{src}");
        assert!(
            t.elapsed().as_secs_f64() < if cfg!(debug_assertions) { 5.0 } else { 0.5 },
            "{src}"
        );
    }
}

/// Review 12, R12-M-05: 0.1 + 0.2 − 0.3 is exactly 0 as typed, so
/// 10^17·(0.1 + 0.2 − 0.3) + x is the line y = x. The curve drawn went
/// through (0, ≈5.55) (each operation rounded), and the trace, from an
/// enclosure 16 wide, read "unknown": both are the analysis's y = x now.
#[test]
fn literal_arithmetic_is_drawn_and_traced_exactly() {
    use graphing::trace::TraceValue;
    let mut g = Graph::new();
    let id = g.add_equation("y=10^17*(0.1+0.2-0.3)+x");
    let v = vp();
    let plots = g.plot(&v);
    let p = &plots.iter().find(|p| p.id == id).expect("plotted").plot;
    // Every vertex on y = x, so the line passes through (0, 0).
    let vertices: Vec<_> = p.curves.iter().flatten().collect();
    assert!(!vertices.is_empty());
    assert!(vertices.iter().all(|q| q.y == q.x), "{vertices:?}");
    assert!(vertices.iter().any(|q| q.x <= 0.0) && vertices.iter().any(|q| q.x >= 0.0));
    // Traced anywhere: y = x to every digit shown.
    for wx in [0.0, 1.25, -3.5, 7.0] {
        let (px, py) = v.to_screen(wx, wx);
        let (eq, t) = g.trace(&v, &plots, px, py, 20.0).expect("traced");
        assert_eq!(eq, id);
        match t.value {
            TraceValue::Defined { lo, hi } => {
                assert!(
                    lo <= t.x && t.x <= hi && hi - lo <= 4.0 * f64::EPSILON * t.x.abs(),
                    "{t:?}"
                );
            }
            other => panic!("traced at {wx}: {other:?}, {}", t.text()),
        }
        let text = t.text();
        let (x, y) = text
            .trim_matches(|c| c == '(' || c == ')')
            .split_once(", ")
            .expect("(x, y)");
        assert_eq!(x, y.trim_start_matches('≈'), "{text}");
    }
}

/// nPr(x, 62) and x·nCr(7, 123456.7) at the default view, as the
/// cross-checking tester (`tests/crossfuzz.rs`) found them: no curve at
/// all. The compiled evaluator had no value for either (Γ's quotient in
/// doubles: x − 61 rounded onto Γ's pole next to 0, an overflow times an
/// underflow); nPr(x, 62) had no enclosure proving it continuous across
/// its zeros (Γ(x − 61) has its poles there), and the column at 0, where
/// it crosses the band from beyond 10⁸⁰ either side, was left unresolved.
/// Now each is drawn, nothing missing, at the program's values, inside
/// the enclosure.
#[test]
fn counts_far_out_and_next_to_zero_are_drawn() {
    use graphing::compile::Program;
    use graphing::functions::ncr;
    use graphing::interval::{Ctx, Dec, Interval, enclose};
    let v = Viewport::default_for_size(1280.0, 720.0);
    let opts = CompileOptions::default();
    let drawn = |text: &str| {
        let eq = Equation::parse(&format!("y={text}")).unwrap();
        let p = plot(&eq.compile(&opts).unwrap(), &v, &PlotOptions::default());
        let f = eq.explicit().unwrap().1.clone();
        let program = Program::compile_typed(&f, &opts).unwrap();
        (p, f, program)
    };
    let enclosure =
        |f: &graphing::ast::Expr, x: f64| enclose(f, Interval::point(x), &Ctx::new(opts));

    // nPr(x, 62): a stroke across the view at each zero 0, 1, …, 17, as
    // nPr(x, 60) has (the polynomial's enclosure).
    for text in ["nPr(x,62)", "nPr(x,60)"] {
        let (p, f, program) = drawn(text);
        assert!(!p.has_missing_data, "{text}: missing data");
        assert_eq!(p.curves.len(), 18, "{text}: {:?}", p.curves);
        for (k, line) in p.curves.iter().enumerate() {
            let k = k as f64;
            assert!(
                line.iter().all(|q| (q.x - k).abs() <= v.x_per_px()),
                "{text}: {line:?} not at {k}"
            );
            let (lo, hi) = line
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), q| {
                    (a.min(q.y), b.max(q.y))
                });
            assert!(lo <= v.y_min && hi >= v.y_max, "{text}: {line:?}");
            // At the zero and the doubles beside it: the program's value
            // inside the enclosure, which proves it continuous.
            for x in [k.next_down(), k, k.next_up()] {
                let (y, e) = (program.eval(x, 0.0), enclosure(&f, x));
                assert!(
                    e.dec >= Dec::Dac && e.lo() <= y && y <= e.hi(),
                    "{text} at {x:e}: {y:e} in {e:?}"
                );
            }
        }
    }

    // x·nCr(7, 123456.7) is −2.4·10⁻³⁸·x: one line along the axis.
    let (p, f, program) = drawn("x*nCr(7,123456.7)");
    assert!(!p.has_missing_data);
    assert_eq!(p.curves.len(), 1);
    let line = &p.curves[0];
    assert!(line[0].x <= v.x_min && line[line.len() - 1].x >= v.x_max);
    let c = ncr(7.0, 123_456.7);
    assert!(
        (c / -2.405_578_290_225_185_4e-38 - 1.0).abs() < 1e-15,
        "{c:e}"
    );
    for q in line {
        let y = program.eval(q.x, 0.0);
        let e = enclosure(&f, q.x);
        assert_eq!(q.y, y, "at {}", q.x);
        assert!((y - q.x * c).abs() <= 1e-15 * (q.x * c).abs(), "at {}", q.x);
        assert!(e.lo() <= y && y <= e.hi(), "at {}: {y:e} in {e:?}", q.x);
    }
}
