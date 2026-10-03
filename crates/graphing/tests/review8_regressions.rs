//! Round-8 review: what analysis claims must hold for the function, judged
//! on its own terms.
//!
//! - R8-M-01: a range bound is noise only against f near where it is
//!   attained, never against f's size elsewhere.
//! - R8-M-02: a slow power tail converges; it is not taken for divergence.
//! - R8-M-03: features at the edge of the search, clustered or beyond it
//!   make what depends on them unknown, not wrong.
//! - R8-M-04: undefined operands of powers and exact poles stay undefined.
//! - R8-L-01: a feature determined only in part is unknown, not listed
//!   as if complete.
//!
//! The second half is a metamorphic sweep: shifting f along x, offsetting
//! or scaling it must move every feature accordingly, and every value of f
//! (sampled densely, and on ladders closing in on 0, domain ends and each
//! listed feature) must lie in the claimed range and agree with the
//! claimed monotonicity and intercepts. The full matrix is `#[ignore]`d:
//! `cargo test -p graphing --test review8_regressions -- --include-ignored`.

use graphing::analysis::{
    AnalysisError, Interval, KeyGraphFeatures, Monotonicity, analyze, analyze_str, flags,
};
use graphing::compile::{CompileOptions, Program, compile_str};
use graphing::equation::Equation;
use graphing::functions::{self as fns, TrigUnit};
use graphing::strings as s;

const INF: f64 = f64::INFINITY;

fn k(src: &str) -> KeyGraphFeatures {
    let r = analyze_str(src);
    assert_eq!(r.analysis_error, AnalysisError::NoError, "{src}");
    r
}

fn near(a: f64, b: f64, rel: f64) -> bool {
    a == b || (a - b).abs() <= rel * a.abs().max(b.abs())
}

/// The range as (lo, lo closed, hi, hi closed) per interval.
fn range(r: &KeyGraphFeatures) -> Vec<(f64, bool, f64, bool)> {
    r.data
        .range
        .iter()
        .map(|i| (i.lo.value, i.lo.closed, i.hi.value, i.hi.closed))
        .collect()
}

fn assert_range(src: &str, r: &KeyGraphFeatures, want: &[(f64, bool, f64, bool)]) {
    assert_eq!(
        r.too_complex_features & flags::RANGE,
        0,
        "{src}: range flagged"
    );
    let got = range(r);
    assert_eq!(got.len(), want.len(), "{src}: {got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!(
            near(g.0, w.0, 1e-8) && g.1 == w.1 && near(g.2, w.2, 1e-8) && g.3 == w.3,
            "{src}: range {got:?}, want {want:?}"
        );
    }
}

fn zeros(r: &KeyGraphFeatures) -> Vec<f64> {
    r.data.zeros.iter().map(|z| z.x).collect()
}

fn points(p: &[(graphing::analysis::Family, f64)]) -> Vec<(f64, f64)> {
    p.iter().map(|(f, y)| (f.x, *y)).collect()
}

// ---------------------------------------------------------------------------
// R8-M-01

/// x² + 10⁻⁷ is 25 on average near the origin; its minimum 10⁻⁷ is still
/// the bottom of its range, not 0.
#[test]
fn tiny_offsets_keep_their_range_bounds() {
    let src = "y=x^2+0.0000001";
    let r = k(src);
    assert_range(src, &r, &[(1e-7, true, INF, false)]);
    assert_eq!(points(&r.data.minima), [(0.0, 1e-7)]);
    assert_eq!(r.data.y_intercept, Some(1e-7));
    assert!(zeros(&r).is_empty() && r.too_complex_features & flags::ZEROS == 0);

    let src = "y=-x^2-0.0000001";
    let r = k(src);
    assert_range(src, &r, &[(-INF, false, -1e-7, true)]);
    assert_eq!(points(&r.data.maxima), [(0.0, -1e-7)]);

    // Periodic: the lower bound is 10⁻⁹, the upper 1 + 10⁻⁹ (shown as 1).
    let src = "y=sin(x)^2+0.000000001";
    let r = k(src);
    assert_range(src, &r, &[(1e-9, true, 1.0, true)]);
    assert!(zeros(&r).is_empty() && r.too_complex_features & flags::ZEROS == 0);
    assert!(r.data.minima.iter().all(|m| m.1 == 1e-9));
}

/// Q1: a tiny constant is that constant, not 0 with every x an intercept;
/// rounding residue of a computed constant still is 0.
#[test]
fn a_tiny_constant_is_not_zero() {
    for src in ["y=10^-10", "y=0.0000000001"] {
        let r = k(src);
        assert_eq!(r.range, "y ∈ {1×10⁻¹⁰}", "{src}");
        assert_eq!(r.data.range[0].lo.value, 1e-10, "{src}");
        assert_eq!(r.x_intercept, "", "{src}");
        assert_eq!(r.y_intercept, "1×10⁻¹⁰", "{src}");
    }
    let r = k("y=sin(pi)");
    assert_eq!(r.range, "y ∈ {0}");
    assert_eq!(r.x_intercept, "x ∈ ℝ");
}

// ---------------------------------------------------------------------------
// R8-M-02

/// x^p for p < 0 eases off by 10^p per decade, however close to 1: it
/// converges to 0, from above, and never reaches it.
#[test]
fn slow_power_tails_tend_to_zero() {
    for p in ["0.1", "0.01", "0.001", "0.0001", "0.00001"] {
        let src = format!("y=x^-{p}");
        let r = k(&src);
        assert_range(&src, &r, &[(0.0, false, INF, false)]);
        assert_eq!(r.data.horizontal_asymptotes.len(), 1, "{src}");
        assert_eq!(r.data.horizontal_asymptotes[0].0, 0.0, "{src}");
        assert!(zeros(&r).is_empty(), "{src}");
        assert_eq!(r.too_complex_features, 0, "{src}");
        assert_eq!(
            r.data.monotonicity.iter().map(|m| m.1).collect::<Vec<_>>(),
            [Monotonicity::Decreasing],
            "{src}"
        );

        let src = format!("y=-x^-{p}");
        let r = k(&src);
        assert_range(&src, &r, &[(-INF, false, 0.0, false)]);
        assert_eq!(r.too_complex_features, 0, "{src}");
    }
    // Shifted and offset: the limit is the offset.
    let src = "y=(x+1)^-0.0001+3";
    let r = k(src);
    assert_range(src, &r, &[(3.0, false, INF, false)]);
    assert_eq!(r.data.horizontal_asymptotes[0].0, 3.0);
    // Genuine slow growth is still growth.
    for src in ["y=ln(x)", "y=x^0.0001", "y=log(x-5000000)"] {
        let r = k(src);
        assert_eq!(r.data.range.last().unwrap().hi.value, INF, "{src}");
        assert!(r.data.horizontal_asymptotes.is_empty(), "{src}");
    }
}

// ---------------------------------------------------------------------------
// R8-M-03

/// A minimum exactly at the base window's edge.
#[test]
fn a_minimum_at_the_window_edge() {
    let src = "y=(x-1000000)^2+1";
    let r = k(src);
    assert_range(src, &r, &[(1.0, true, INF, false)]);
    assert_eq!(points(&r.data.minima), [(1e6, 1.0)]);
    assert_eq!(
        r.data
            .monotonicity
            .iter()
            .map(|(i, m)| (i.lo.value, i.hi.value, *m))
            .collect::<Vec<_>>(),
        [
            (-INF, 1e6, Monotonicity::Decreasing),
            (1e6, INF, Monotonicity::Increasing)
        ]
    );
    assert_eq!(r.too_complex_features, 0);
}

/// Nine double roots of a sqrt expression are too many critical points to
/// follow: whatever depends on them is unknown, the range included.
#[test]
fn unfollowed_critical_points_make_the_range_unknown() {
    for roots in [
        [2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000],
        [
            2000, 4000, 8000, 16000, 32000, 64000, 128000, 256000, 512000,
        ],
    ] {
        let src = format!(
            "y=({})^2",
            roots
                .iter()
                .map(|r| format!("(sqrt(x)-{r})"))
                .collect::<Vec<_>>()
                .join("*")
        );
        let r = k(&src);
        let need =
            flags::RANGE | flags::ZEROS | flags::MINIMA | flags::MAXIMA | flags::MONOTONE_INTERVALS;
        assert_eq!(r.too_complex_features & need, need, "{src}");
        assert_eq!(r.range, "", "{src}");
        assert_eq!(r.domain, "x ∈ [0, ∞)", "{src}");
    }
}

/// Roots a unit apart at 2·10⁶: two zeros and minima, the maximum between,
/// four monotone runs.
#[test]
fn clustered_far_roots() {
    let src = "y=((x-2000000)*(x-2000001))^2";
    let r = k(src);
    assert_eq!(zeros(&r), [2e6, 2e6 + 1.0]);
    assert_eq!(points(&r.data.minima), [(2e6, 0.0), (2e6 + 1.0, 0.0)]);
    let max = points(&r.data.maxima);
    assert_eq!(max.len(), 1);
    assert!(near(max[0].0, 2e6 + 0.5, 1e-12) && near(max[0].1, 1.0 / 16.0, 1e-9));
    assert_eq!(r.maxima, ["(2000000.5, 1/16)"]);
    assert_eq!(r.data.monotonicity.len(), 4);
    assert_range(src, &r, &[(0.0, true, INF, false)]);
    assert_eq!(r.too_complex_features, 0);
}

/// Q1: (ln x − 40)(ln x − 50) turns at e⁴⁵ ≈ 3.5·10¹⁹ and (ln x − 40)²
/// touches 0 at e⁴⁰, both beyond the search: what depends on them is
/// unknown, not "no minimum" and "decreasing".
#[test]
fn turns_beyond_the_search_are_unknown() {
    for src in ["y=(ln(x)-40)*(ln(x)-50)", "y=(ln(x)-40)^2"] {
        let r = k(src);
        let need = flags::ZEROS
            | flags::MINIMA
            | flags::MAXIMA
            | flags::MONOTONE_INTERVALS
            | flags::RANGE
            | flags::INFLECTION_POINTS;
        assert_eq!(r.too_complex_features & need, need, "{src}");
        assert!(r.minima.is_empty() && r.monotonicity.is_empty(), "{src}");
        assert_eq!(r.vertical_asymptotes, ["x = 0"], "{src}");
    }
    // One crossing out there only adds a zero.
    let r = k("y=ln(x)-40");
    assert_eq!(r.range, "y ∈ ℝ");
    assert_eq!(r.too_complex_features, flags::ZEROS);
}

/// A pole possibly beyond the search hides f's behaviour towards ±∞.
#[test]
fn a_pole_beyond_the_search_hides_the_asymptotes() {
    for src in ["y=1/(x-100000000000000000000)", "y=1/(ln(x)-100)"] {
        let r = k(src);
        let need = flags::HORIZONTAL_ASYMPTOTES | flags::OBLIQUE_ASYMPTOTES;
        assert_eq!(r.too_complex_features & need, need, "{src}");
        assert!(r.horizontal_asymptotes.is_empty(), "{src}");
    }
}

// ---------------------------------------------------------------------------
// R8-M-04

#[test]
fn powers_of_undefined_operands_stay_undefined() {
    assert!(fns::pow(0.0, -1.0).is_nan());
    assert!(fns::pow(0.0, -0.5).is_nan());
    assert!(fns::pow(f64::NAN, 0.0).is_nan());
    assert!(fns::pow(1.0, f64::NAN).is_nan());
    assert_eq!(fns::pow(0.0, 0.5), 0.0);
    assert_eq!(fns::pow(2.0, -1.0), 0.5);
    assert!(fns::pow_int(0.0, -2).is_nan());
    assert_eq!(fns::pow_int(-2.0, 3), -8.0);
    let at = |src: &str, x: f64| compile_str(src, TrigUnit::Radians).unwrap().eval_x(x);
    for (src, x) in [
        ("x^-1", 0.0),
        ("x^(-1/3)", 0.0),
        ("1/(x^-1)", 0.0),
        ("(1/x)^0", 0.0),
        ("1^(1/x)", 0.0),
        ("1^(ln(x))", -1.0),
        ("root(x,-1)", 0.0),
        ("root(x,-3)", 0.0),
        ("atanh(x)", 1.0),
        ("atanh(x)", -1.0),
        ("asech(x)", 0.0),
        ("acsch(x)", 0.0),
        ("acoth(x)", 1.0),
        ("acoth(x)", -1.0),
        ("exp(-x^-2)", 0.0),
    ] {
        assert!(at(src, x).is_nan(), "{src} at {x}: {}", at(src, x));
    }
    // Finite overflow is unchanged.
    assert_eq!(at("1/(1+e^1000)", 0.0), 0.0);
    assert_eq!(at("1/(1+e^x)", 1000.0), 0.0);
    // Exact tangent poles in degrees and gradians.
    assert!(fns::tan_u(90.0, TrigUnit::Degrees).is_nan());
    assert!(fns::tan_u(-270.0, TrigUnit::Degrees).is_nan());
    assert!(fns::tan_u(100.0, TrigUnit::Grads).is_nan());
    assert!(near(fns::tan_u(45.0, TrigUnit::Degrees), 1.0, 1e-15));
    let deg = CompileOptions {
        trig_unit: TrigUnit::Degrees,
        variables: &(),
    };
    let eq = Equation::parse("y=1/tan(x)").unwrap();
    let r = analyze(&eq, &deg);
    assert_eq!(r.analysis_error, AnalysisError::NoError);
    assert!(!r.x_intercept.contains("90"), "{}", r.x_intercept);
}

#[test]
fn literal_zero_to_a_negative_power_is_an_error() {
    assert!(compile_str("0^-1", TrigUnit::Radians).is_err());
    assert!(compile_str("1/(0^-1)", TrigUnit::Radians).is_err());
    assert!(compile_str("0^(-1/2)", TrigUnit::Radians).is_err());
    assert_eq!(
        compile_str("0^2", TrigUnit::Radians).unwrap().as_constant(),
        Some(0.0)
    );
    assert_ne!(
        analyze_str("y=1/(0^-1)").analysis_error,
        AnalysisError::NoError
    );
}

#[test]
fn analysis_keeps_holes_from_powers_and_poles() {
    // Same as the control 1/(1/x).
    let control = k("y=1/(1/x)");
    let r = k("y=1/(x^-1)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {0}");
    assert_eq!(r.range, control.range);
    assert_eq!(r.x_intercept, "");
    assert_eq!(r.data.y_intercept, None);
    for src in ["y=(1/x)^0", "y=1^(1/x)"] {
        let r = k(src);
        assert_eq!(r.domain, "x ∈ ℝ \\ {0}", "{src}");
        assert_eq!(r.range, "y ∈ {1}", "{src}");
        assert_eq!(r.data.y_intercept, None, "{src}");
    }
    let r = k("y=1^(ln(x))");
    assert_eq!(r.domain, "x ∈ (0, ∞)");
    assert_eq!(r.data.y_intercept, None);

    let r = k("y=exp(-x^-2)");
    assert!(r.data.minima.is_empty() && r.data.zeros.is_empty());
    assert_eq!(r.domain, "x ∈ ℝ \\ {0}");
    assert_eq!(r.range, "y ∈ (0, 1)");

    // 1/asech(x) → 0 only as x → 0, where it is undefined; how fast it gets
    // there can't be pinned down, so the range is unknown, not [0, ∞).
    let r = k("y=1/asech(x)");
    assert_eq!(r.domain, "x ∈ (0, 1)");
    assert!(r.data.zeros.is_empty());
    assert!(
        r.too_complex_features & flags::RANGE != 0
            || r.data
                .range
                .iter()
                .all(|i| i.lo.value > 0.0 || !i.lo.closed)
    );
    let r = k("y=1/acoth(x)");
    assert_eq!(r.domain, "x ∈ (−∞, −1) ∪ (1, ∞)");
    assert_eq!(r.x_intercept, "");
    assert_eq!(r.too_complex_features & flags::ZEROS, 0);
    let r = k("y=exp(-atanh(x))");
    assert_eq!(r.domain, "x ∈ (−1, 1)");
    assert_eq!(r.range, "y ∈ (0, ∞)");
    assert_eq!(r.x_intercept, "");
    assert_eq!(r.vertical_asymptotes, ["x = −1"]);
    // Finite overflow still gives a defined value.
    let r = k("y=1/(1+e^x)");
    assert_eq!(r.domain, "x ∈ ℝ");
    assert_eq!(r.range, "y ∈ (0, 1)");
}

// ---------------------------------------------------------------------------
// R8-L-01

/// A feature determined only in part (the asymptote y = 1/ln 2 on the left;
/// the right tail creeps to 0 too slowly to be pinned down) keeps none of
/// its values: they'd read as all there is. The row says it couldn't be
/// calculated, and the data agree.
#[test]
fn a_partly_determined_feature_is_unknown() {
    let r = k("y=1/ln(2+max(x,0))+1/(1+x^2)");
    assert_ne!(r.too_complex_features & flags::HORIZONTAL_ASYMPTOTES, 0);
    assert!(r.data.horizontal_asymptotes.is_empty());
    let items = r.items();
    let row = items
        .iter()
        .find(|i| i.title == s::HORIZONTAL_ASYMPTOTES)
        .unwrap();
    assert_eq!(row.display_items, [s::KGF_HORIZONTAL_ASYMPTOTES_UNKNOWN]);
    // Likewise the extrema of sin(x)/x, infinitely many: none listed.
    let r = k("y=sin(x)/x");
    assert_ne!(r.too_complex_features & flags::MINIMA, 0);
    assert!(r.minima.is_empty() && r.data.minima.is_empty());
    assert!(r.data.zeros.is_empty());
    // Every flagged feature, in general.
    for src in [
        "y=sin(x^2)",
        "y=x*sin(x)",
        "y=(ln(x)-40)^2",
        "y=1/(ln(x)-100)",
    ] {
        let r = k(src);
        let d = &r.data;
        for (flag, n) in [
            (flags::ZEROS, d.zeros.len()),
            (flags::MINIMA, d.minima.len()),
            (flags::MAXIMA, d.maxima.len()),
            (flags::INFLECTION_POINTS, d.inflection_points.len()),
            (flags::VERTICAL_ASYMPTOTES, d.vertical_asymptotes.len()),
            (flags::HORIZONTAL_ASYMPTOTES, d.horizontal_asymptotes.len()),
            (flags::OBLIQUE_ASYMPTOTES, d.oblique_asymptotes.len()),
            (flags::MONOTONE_INTERVALS, d.monotonicity.len()),
        ] {
            assert!(
                r.too_complex_features & flag == 0 || n == 0,
                "{src}: {flag}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Metamorphic sweep.

/// Base functions with well separated features of size about 1: a cubic,
/// a double root, a bump, rational functions with and without poles, a
/// domain end, an exponential, an arctangent.
const BASE: &[&str] = &[
    "X^3-3*X",
    "(X-1)^2*(X+2)",
    "X*exp(-X^2)",
    "1/(X^2+1)",
    "X/(X^2+1)",
    "(X^2-1)/(X^2+4)",
    "ln(X^2+1)",
    "atan(X)",
    "1/(X-1)",
    "sqrt(X)*(2-X)",
    "exp(X)-2",
    "X+1/X",
];

/// Sub-grid structure the sweep's dense check must also hold for: features
/// closer to a domain end, a kink or each other than the analysis grid
/// (≈ 7·10⁻⁴ near 0).
const FINE: &[&str] = &[
    "(x-0.0001)*(x-0.0002)",
    "(exp(x)-1.0001)*(exp(x)-1.0002)",
    "sqrt(x)-1000*x",
    "sqrt(x)*(x-0.001)",
    "x^2-0.001*abs(x)",
    "x^2-0.00001*abs(x)",
    "1/ln(2+abs(x))+1/(1+x^2)",
    "1/ln(2+max(x,0))+1/(1+x^2)",
    "x^3/3-0.00015*x^2+0.00000002*x",
];

fn subst(base: &str, x: &str) -> String {
    base.replace('X', x)
}

fn fun(src: &str) -> Program {
    let body = src.strip_prefix("y=").unwrap_or(src);
    compile_str(body, TrigUnit::Radians).unwrap()
}

/// Whether `v` lies in the union of intervals, to within `tol`.
fn in_set(set: &[Interval], v: f64, tol: f64) -> bool {
    set.iter().any(|i| {
        let lo_ok = if i.lo.closed || i.lo.value == -INF {
            v >= i.lo.value - tol
        } else {
            v > i.lo.value - tol && (tol > 0.0 || v > i.lo.value)
        };
        let hi_ok = if i.hi.closed || i.hi.value == INF {
            v <= i.hi.value + tol
        } else {
            v < i.hi.value + tol && (tol > 0.0 || v < i.hi.value)
        };
        lo_ok && hi_ok
    })
}

/// Points to check f at: a dense uniform grid over [−50, 50], ladders
/// closing in geometrically on 0, on each finite end of the domain and on
/// each listed zero, extremum and inflection, from both sides.
fn probe_points(r: &KeyGraphFeatures, centre: f64) -> Vec<f64> {
    let mut xs: Vec<f64> = (0..=40_000)
        .map(|i| centre - 50.0 + 100.0 * i as f64 / 40_000.0)
        .collect();
    let mut anchors = vec![0.0, centre];
    for i in &r.data.domain {
        for b in [i.lo.value, i.hi.value] {
            if b.is_finite() {
                anchors.push(b);
            }
        }
    }
    anchors.extend(r.data.zeros.iter().map(|z| z.x));
    for p in r
        .data
        .minima
        .iter()
        .chain(&r.data.maxima)
        .chain(&r.data.inflection_points)
    {
        anchors.push(p.0.x);
    }
    for a in anchors {
        for e in 1..=14 {
            let d = 10f64.powi(-e) * a.abs().max(1.0);
            for m in [1.0, 2.0, 5.0] {
                xs.push(a + m * d);
                xs.push(a - m * d);
            }
        }
    }
    xs.retain(|x| x.is_finite());
    xs.sort_by(|a, b| a.total_cmp(b));
    xs.dedup();
    xs
}

/// Checks what `r` claims against f sampled at many points: every value
/// lies in the range; on each claimed monotone interval f moves only in the
/// claimed direction (beyond rounding); within a connected domain interval
/// a sign change of f has a zero listed between. Returns the failures.
fn dense_check(src: &str, r: &KeyGraphFeatures, centre: f64) -> Vec<String> {
    let mut bad = Vec::new();
    let f = fun(src);
    let xs = probe_points(r, centre);
    let pts: Vec<(f64, f64)> = xs
        .iter()
        .map(|&x| (x, f.eval_x(x)))
        .filter(|p| p.1.is_finite())
        .collect();
    let noise = |x: f64, y: f64| {
        let u = 8.0 * f64::EPSILON * x.abs();
        let a = f.eval_x(x - u);
        let b = f.eval_x(x + u);
        let mut n = 64.0 * f64::EPSILON * y.abs();
        for z in [a, b] {
            if z.is_finite() {
                n += 4.0 * (z - y).abs();
            }
        }
        n
    };
    if r.too_complex_features & flags::RANGE == 0 && !r.range.is_empty() {
        for &(x, y) in &pts {
            if !in_set(&r.data.range, y, 1e-9 * y.abs() + noise(x, y)) {
                bad.push(format!("{src}: f({x:e}) = {y:e} outside range {}", r.range));
                break;
            }
        }
    }
    if r.too_complex_features & flags::MONOTONE_INTERVALS == 0 {
        for (iv, dir) in &r.data.monotonicity {
            let inside: Vec<&(f64, f64)> = pts
                .iter()
                .filter(|p| p.0 > iv.lo.value && p.0 < iv.hi.value)
                .collect();
            for w in inside.windows(2) {
                let ((x0, y0), (x1, y1)) = (*w[0], *w[1]);
                let tol = noise(x0, y0) + noise(x1, y1) + 1e-12 * y0.abs().max(y1.abs());
                let wrong = match dir {
                    Monotonicity::Increasing => y1 < y0 - tol,
                    Monotonicity::Decreasing => y1 > y0 + tol,
                    Monotonicity::Constant => (y1 - y0).abs() > tol,
                    Monotonicity::Unknown => false,
                };
                if wrong {
                    bad.push(format!(
                        "{src}: {dir:?} on {} but f({x0:e}) = {y0:e}, f({x1:e}) = {y1:e}",
                        iv.format()
                    ));
                    break;
                }
            }
        }
    }
    if r.too_complex_features & flags::ZEROS == 0 && !r.x_intercept.starts_with('x') {
        let zs = zeros(r);
        for iv in &r.data.domain {
            let inside: Vec<&(f64, f64)> = pts
                .iter()
                .filter(|p| in_set(std::slice::from_ref(iv), p.0, 0.0))
                .collect();
            for w in inside.windows(2) {
                let ((x0, y0), (x1, y1)) = (*w[0], *w[1]);
                let clear = |x: f64, y: f64| y.abs() > noise(x, y);
                if clear(x0, y0)
                    && clear(x1, y1)
                    && (y0 < 0.0) != (y1 < 0.0)
                    && !zs
                        .iter()
                        .any(|&z| z >= x0 - 1e-9 * x0.abs() && z <= x1 + 1e-9 * x1.abs())
                    && !r.data.excluded.iter().any(|e| e.x >= x0 && e.x <= x1)
                {
                    bad.push(format!(
                        "{src}: f changes sign between {x0:e} and {x1:e} with no zero listed"
                    ));
                    break;
                }
            }
        }
    }
    bad
}

/// The features of `r` as plain numbers, for comparison: zeros, extrema,
/// inflections, vertical asymptotes, monotone runs.
struct Shape {
    zeros: Vec<f64>,
    minima: Vec<(f64, f64)>,
    maxima: Vec<(f64, f64)>,
    infl: Vec<(f64, f64)>,
    poles: Vec<f64>,
    mono: Vec<(f64, f64, Monotonicity)>,
}

fn shape(r: &KeyGraphFeatures) -> Shape {
    Shape {
        zeros: zeros(r),
        minima: points(&r.data.minima),
        maxima: points(&r.data.maxima),
        infl: points(&r.data.inflection_points),
        poles: r.data.vertical_asymptotes.iter().map(|v| v.x).collect(),
        mono: r
            .data
            .monotonicity
            .iter()
            .map(|(i, m)| (i.lo.value, i.hi.value, *m))
            .collect(),
    }
}

/// Compares the x positions of a feature list, base shifted by `a`.
fn same_xs(what: &str, base: &[f64], got: &[f64], a: f64, tol: f64, bad: &mut Vec<String>) {
    let ok = base.len() == got.len()
        && base.iter().zip(got).all(|(b, g)| {
            (b + a == *g) || (g - a - b).abs() <= tol * b.abs().max(1.0) + 1e-15 * a.abs() * 64.0
        });
    if !ok {
        bad.push(format!(
            "{what}: base {base:?} shifted by {a:e}, got {got:?}"
        ));
    }
}

/// Compares (x, y) points: x shifted by `a`, y mapped by `fy`.
fn same_points(
    what: &str,
    base: &[(f64, f64)],
    got: &[(f64, f64)],
    a: f64,
    fy: &dyn Fn(f64) -> f64,
    bad: &mut Vec<String>,
) {
    let ok = base.len() == got.len()
        && base.iter().zip(got).all(|(b, g)| {
            let want = fy(b.1);
            (g.0 - a - b.0).abs() <= 1e-6 * b.0.abs().max(1.0) + 64.0 * f64::EPSILON * a.abs()
                && (g.1 == want
                    || (g.1 - want).abs()
                        <= 2e-9 * want.abs().max(g.1.abs()) + 1e-12 * (want - fy(0.0)).abs())
        });
    if !ok {
        bad.push(format!(
            "{what}: base {base:?} (x+{a:e}, y mapped), got {got:?}"
        ));
    }
}

/// f(x − a) against f: every feature moves by a.
fn shift_failures(base: &str, a: f64) -> Vec<String> {
    let mut bad = Vec::new();
    let b_src = format!("y={}", subst(base, "x"));
    let s_src = format!("y={}", subst(base, &format!("(x-{a})")));
    let rb = k(&b_src);
    let rs = k(&s_src);
    let (sb, ss) = (shape(&rb), shape(&rs));
    let tc = rs.too_complex_features;
    let tag = |w: &str| format!("{s_src} {w}");
    let tol = 1e-7;
    if tc & flags::ZEROS == 0 {
        same_xs(&tag("zeros"), &sb.zeros, &ss.zeros, a, tol, &mut bad);
    }
    if tc & flags::VERTICAL_ASYMPTOTES == 0 {
        same_xs(&tag("poles"), &sb.poles, &ss.poles, a, tol, &mut bad);
    }
    let id = |y: f64| y;
    if tc & flags::MINIMA == 0 {
        same_points(&tag("minima"), &sb.minima, &ss.minima, a, &id, &mut bad);
    }
    if tc & flags::MAXIMA == 0 {
        same_points(&tag("maxima"), &sb.maxima, &ss.maxima, a, &id, &mut bad);
    }
    if tc & flags::INFLECTION_POINTS == 0 {
        same_points(&tag("inflections"), &sb.infl, &ss.infl, a, &id, &mut bad);
    }
    if tc & flags::MONOTONE_INTERVALS == 0 {
        let ok = sb.mono.len() == ss.mono.len()
            && sb.mono.iter().zip(&ss.mono).all(|(b, g)| {
                b.2 == g.2
                    && [(b.0, g.0), (b.1, g.1)].iter().all(|&(u, v)| {
                        (u.is_infinite() && u == v)
                            || (v - a - u).abs()
                                <= 1e-6 * u.abs().max(1.0) + 64.0 * f64::EPSILON * a.abs()
                    })
            });
        if !ok {
            bad.push(format!(
                "{}: base {:?}, got {:?}",
                tag("monotonicity"),
                sb.mono,
                ss.mono
            ));
        }
    }
    if tc & flags::RANGE == 0 && rs.range != rb.range {
        bad.push(format!(
            "{}: base {}, got {}",
            tag("range"),
            rb.range,
            rs.range
        ));
    }
    if tc & flags::HORIZONTAL_ASYMPTOTES == 0
        && rs.horizontal_asymptotes != rb.horizontal_asymptotes
    {
        bad.push(format!(
            "{}: base {:?}, got {:?}",
            tag("horizontal asymptotes"),
            rb.horizontal_asymptotes,
            rs.horizontal_asymptotes
        ));
    }
    bad
}

/// k·f(x) + c against f: positions stay, values map.
fn map_failures(base: &str, scale: f64, offset: f64) -> Vec<String> {
    let mut bad = Vec::new();
    let b_src = format!("y={}", subst(base, "x"));
    let m_src = format!("y={scale}*({})+({offset})", subst(base, "x"));
    let rb = k(&b_src);
    let rm = k(&m_src);
    let (sb, sm) = (shape(&rb), shape(&rm));
    let tc = rm.too_complex_features;
    let tag = |w: &str| format!("{m_src} {w}");
    let fy = |y: f64| scale * y + offset;
    if tc & flags::VERTICAL_ASYMPTOTES == 0 {
        same_xs(&tag("poles"), &sb.poles, &sm.poles, 0.0, 1e-7, &mut bad);
    }
    if offset == 0.0 && tc & flags::ZEROS == 0 {
        same_xs(&tag("zeros"), &sb.zeros, &sm.zeros, 0.0, 1e-7, &mut bad);
    }
    if tc & flags::MINIMA == 0 {
        same_points(&tag("minima"), &sb.minima, &sm.minima, 0.0, &fy, &mut bad);
    }
    if tc & flags::MAXIMA == 0 {
        same_points(&tag("maxima"), &sb.maxima, &sm.maxima, 0.0, &fy, &mut bad);
    }
    if tc & flags::INFLECTION_POINTS == 0 {
        same_points(&tag("inflections"), &sb.infl, &sm.infl, 0.0, &fy, &mut bad);
    }
    if tc & flags::MONOTONE_INTERVALS == 0 {
        let ok = sb.mono.len() == sm.mono.len()
            && sb
                .mono
                .iter()
                .zip(&sm.mono)
                .all(|(b, g)| b.2 == g.2 && near(b.0, g.0, 1e-7) && near(b.1, g.1, 1e-7));
        if !ok {
            bad.push(format!(
                "{}: base {:?}, got {:?}",
                tag("monotonicity"),
                sb.mono,
                sm.mono
            ));
        }
    }
    if tc & flags::RANGE == 0 && rb.too_complex_features & flags::RANGE == 0 {
        let rb_r = range(&rb);
        let rm_r = range(&rm);
        let ok = rb_r.len() == rm_r.len()
            && rb_r.iter().zip(&rm_r).all(|(b, g)| {
                let lo = fy(b.0);
                let hi = fy(b.2);
                let close = |w: f64, v: f64| {
                    w == v
                        || (v - w).abs() <= 2e-9 * w.abs().max(v.abs()) + 1e-12 * (w - offset).abs()
                };
                b.1 == g.1 && b.3 == g.3 && close(lo, g.0) && close(hi, g.2)
            });
        if !ok {
            bad.push(format!("{}: base {:?}, got {:?}", tag("range"), rb_r, rm_r));
        }
    }
    bad
}

const SHIFTS: &[f64] = &[0.0, 1.0, 1e3, 1e6, 1e9];
const OFFSETS: &[f64] = &[0.0, 1e-9, -1e-9, 1e-7, -1e-7, 1.0, -1.0, 1e6, -1e6];
const SCALES: &[f64] = &[1e-12, 1e-6, 1.0, 1e6, 1e12];

fn report(bad: Vec<String>) {
    assert!(
        bad.is_empty(),
        "{} failures:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// The fast core: every base function shifted to 10³, offset by ±10⁻⁹ and
/// 10⁶, scaled by 10⁻¹² and 10¹², and densely checked; the sub-grid set
/// densely checked.
#[test]
fn metamorphic_core() {
    let mut bad = Vec::new();
    for b in BASE {
        bad.extend(shift_failures(b, 1e3));
        for c in [1e-9, -1e-9, 1e6] {
            bad.extend(map_failures(b, 1.0, c));
        }
        for s in [1e-12, 1e12] {
            bad.extend(map_failures(b, s, 0.0));
        }
        let src = format!("y={}", subst(b, "x"));
        bad.extend(dense_check(&src, &k(&src), 0.0));
    }
    for src in FINE {
        let src = format!("y={src}");
        bad.extend(dense_check(&src, &k(&src), 0.0));
    }
    report(bad);
}

/// The whole matrix: every shift, offset and scale, each densely checked.
#[test]
#[ignore = "slow: the full metamorphic matrix (run with --include-ignored)"]
fn metamorphic_full() {
    let mut bad = Vec::new();
    for b in BASE {
        for &a in SHIFTS {
            bad.extend(shift_failures(b, a));
            let src = format!("y={}", subst(b, &format!("(x-{a})")));
            bad.extend(dense_check(&src, &k(&src), a));
        }
        for &c in OFFSETS {
            for &s in SCALES {
                bad.extend(map_failures(b, s, c));
                let src = format!("y={s}*({})+({c})", subst(b, "x"));
                bad.extend(dense_check(&src, &k(&src), 0.0));
            }
        }
    }
    report(bad);
}
