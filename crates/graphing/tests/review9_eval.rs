//! Round-9 review, R9-M-04: a value beyond a double's range is still a
//! value. It rounds to 0 or ±∞, and an operation on that rounding gave NaN,
//! which stands for "undefined": (e^x)^−1 was 0^−1 below −745.1, e^x/e^x
//! was ∞/∞ beyond 709.8, ln(e^x) was ln 0. Analysis took those for gaps in
//! the domain. Genuinely undefined points must stay undefined: e^(1/x) at
//! 0, 1/0, 0^−1, ln 0.
//!
//! Also: a form that is 0 wherever it is defined (0·x, 0/x) has every
//! point of its domain as an x-intercept; sampling alone said none.

use graphing::TrigUnit;
use graphing::analysis::{AnalysisError, KeyGraphFeatures, Monotonicity, analyze_str, flags};
use graphing::compile::{CompileOptions, Input, Program, compile_str};
use graphing::error::{ErrorCode, EvaluationErrorCode};

fn k(src: &str) -> KeyGraphFeatures {
    let r = analyze_str(src);
    assert_eq!(r.analysis_error, AnalysisError::NoError, "{src}");
    r
}

fn at(src: &str, x: f64) -> f64 {
    compile_str(src, TrigUnit::Radians).unwrap().eval_x(x)
}

fn known(r: &KeyGraphFeatures, which: u32) -> bool {
    r.too_complex_features & which == 0
}

#[test]
fn values_beyond_a_double_are_values() {
    // Defined everywhere, whatever a double makes of the parts.
    for (src, x, want) in [
        ("exp(x)/exp(x)", 800.0, 1.0),
        ("exp(x)/exp(x)", -800.0, 1.0),
        ("exp(x)*exp(-x)", 800.0, 1.0),
        ("exp(x)*exp(-x)", -800.0, 1.0),
        ("0/exp(x)", -800.0, 0.0),
        ("0/exp(1/x)", -0.001, 0.0),
        ("ln(exp(x))", -800.0, -800.0),
        ("ln(exp(x))", 800.0, 800.0),
        ("atan(exp(x)^-1)", -800.0, std::f64::consts::FRAC_PI_2),
        ("exp(x)^-1", -800.0, f64::INFINITY),
        ("exp(x)^-1", 800.0, 0.0),
        ("x^400/x^400", 1e-10, 1.0),
        ("exp(x)-exp(x)", 800.0, 0.0),
        ("sqrt(exp(x))^-1", -2000.0, f64::INFINITY),
    ] {
        let v = at(src, x);
        assert!(
            (v - want).abs() <= 1e-12 * want.abs() || v == want,
            "{src} at {x}: {v}, want {want}"
        );
    }
    // A batch gives the same values.
    let p = compile_str("exp(x)/exp(x)", TrigUnit::Radians).unwrap();
    let xs = [-1e6, -800.0, 0.0, 800.0, 1e300];
    let mut out = [0.0; 5];
    p.eval_batch(Input::Slice(&xs), Input::Scalar(0.0), &mut out);
    assert_eq!(out, [1.0; 5]);
}

#[test]
fn undefined_points_stay_undefined() {
    for (src, x) in [
        ("exp(1/x)", 0.0),
        ("0/exp(1/x)", 0.0),
        ("1/x", 0.0),
        ("x^-1", 0.0),
        ("1/(1/x)", 0.0),
        ("ln(x)", 0.0),
        ("log(x)", 0.0),
        ("0^(-x)", 1.0),
        ("(1/x)^0", 0.0),
        ("1^(1/x)", 0.0),
        ("atanh(x)", 1.0),
        ("acoth(x)", 1.0),
        ("sqrt(x)*exp(x)", -1.0),
        ("(-2)^x", 0.5),
        ("exp(x)/(exp(x)-exp(x))", 800.0),
        ("ln(exp(x)-exp(x))", 800.0),
    ] {
        let v = at(src, x);
        assert!(v.is_nan(), "{src} at {x}: {v}");
    }
}

#[test]
fn folded_constants_keep_their_value() {
    let c = |src: &str| compile_str(src, TrigUnit::Radians).map(|p| p.as_constant());
    let pi2 = std::f64::consts::FRAC_PI_2;
    assert_eq!(c("atan(1/exp(-1000))").unwrap(), Some(pi2));
    assert_eq!(
        c("exp(-1000)*exp(1000)").unwrap().map(f64::round),
        Some(1.0)
    );
    // A double overflow of a finite constant is still 0 (disclosed).
    assert_eq!(c("1/(1+e^1000)").unwrap(), Some(0.0));
    // Literal divisions by zero are still errors.
    for src in ["1/0", "0^-1", "x/0", "1/(1-1)", "x/(2-2)"] {
        let e = compile_str(src, TrigUnit::Radians).unwrap_err();
        assert_eq!(
            e.code,
            ErrorCode::Evaluation(EvaluationErrorCode::DivideByZero),
            "{src}"
        );
    }
    // A constant beyond a double inside a program keeps its value there.
    let p = Program::compile(
        &graphing::parser::parse_expression("x/exp(-1000)*exp(-1000)").unwrap(),
        &CompileOptions::default(),
    )
    .unwrap();
    assert!((p.eval_x(3.0) - 3.0).abs() < 1e-12, "{}", p.eval_x(3.0));
}

#[test]
fn degrees_of_a_tiny_negative_angle() {
    // rem_euclid rounds −10⁻²⁰ up to a whole turn: that is 0°, not 270°.
    use graphing::functions::{cos_u, sec_u, sin_u, tan_u};
    for unit in [TrigUnit::Radians, TrigUnit::Degrees, TrigUnit::Grads] {
        for x in [-1e-20, -3e-322, -0.0] {
            let (s, c, t) = (sin_u(x, unit), cos_u(x, unit), tan_u(x, unit));
            assert!(s.abs() < 1e-15 && s <= 0.0, "{unit:?} sin({x}) = {s}");
            assert_eq!(c, 1.0, "{unit:?} cos({x})");
            assert!(t.abs() < 1e-15, "{unit:?} tan({x}) = {t}");
            assert_eq!(sec_u(x, unit), 1.0, "{unit:?} sec({x})");
        }
        for src in ["atan(tan(x))", "atan(sec(x))", "sin(x)/cos(x)"] {
            let v = compile_str(src, unit).unwrap().eval_x(-3e-322);
            assert!(v.is_finite(), "{unit:?} {src} at −3·10⁻³²²: {v}");
        }
    }
}

#[test]
fn a_reciprocal_power_of_exp_is_defined_everywhere() {
    for src in [
        "y=exp(x)^-1",
        "y=1/exp(x)",
        "y=exp(x)^-2",
        "y=1/(exp(x)^-1)",
    ] {
        let r = k(src);
        assert_eq!(r.domain, "x ∈ ℝ", "{src}");
        assert!(r.vertical_asymptotes.is_empty(), "{src}: {r:?}");
        assert!(known(&r, flags::VERTICAL_ASYMPTOTES), "{src}");
    }
    assert_eq!(k("y=exp(x)^-1").horizontal_asymptotes, ["y = 0"]);
    for src in ["y=atan(exp(x)^-1)", "y=atan(1/exp(x))"] {
        let r = k(src);
        assert_eq!(r.domain, "x ∈ ℝ", "{src}");
        assert_eq!(r.horizontal_asymptotes, ["y = 0", "y = π/2"], "{src}");
    }
}

#[test]
fn quotients_and_logs_of_exp_have_no_cutoff() {
    let r = k("y=exp(x)/exp(x)");
    assert_eq!(r.domain, "x ∈ ℝ");
    assert_eq!(r.range, "y ∈ {1}");
    assert_eq!(r.monotonicity, [("(−∞, ∞)".into(), Monotonicity::Constant)]);
    for src in ["y=ln(exp(x))", "y=ln(exp(-x))", "y=log(exp(x))"] {
        let r = k(src);
        assert_eq!(r.domain, "x ∈ ℝ", "{src}");
        assert_eq!(r.x_intercept, "0", "{src}");
    }
    assert_eq!(k("y=ln(exp(x))").oblique_asymptotes, ["y = x"]);
    let r = k("y=exp(x)*exp(-x)");
    assert_eq!(r.domain, "x ∈ ℝ");
    assert!(r.vertical_asymptotes.is_empty() && known(&r, flags::VERTICAL_ASYMPTOTES));
}

#[test]
fn a_folded_tiny_denominator_is_not_a_division_by_zero() {
    let r = k("y=atan(1/exp(-1000))");
    assert_eq!(r.range, "y ∈ {π/2}");
    let r = k("y=atan(1/exp(-1000))+x");
    assert_eq!(r.x_intercept, "−π/2");
    assert_eq!(r.y_intercept, "π/2");
    assert_eq!(r.range, "y ∈ ℝ");
}

#[test]
fn zero_wherever_defined() {
    for (src, domain) in [
        ("y=0*x", "x ∈ ℝ"),
        ("y=0*e^x", "x ∈ ℝ"),
        ("y=x*0", "x ∈ ℝ"),
        ("y=0/exp(x)", "x ∈ ℝ"),
        ("y=0*sin(x)", "x ∈ ℝ"),
        ("y=x-x", "x ∈ ℝ"),
        ("y=0*(x-5)", "x ∈ ℝ"),
        ("y=0/x", "x ∈ ℝ \\ {0}"),
        ("y=0/x^2", "x ∈ ℝ \\ {0}"),
        ("y=0/exp(1/x)", "x ∈ ℝ \\ {0}"),
        ("y=0/(x-1000)", "x ∈ ℝ \\ {1000}"),
        ("y=0*ln(x)", "x ∈ (0, ∞)"),
    ] {
        let r = k(src);
        assert_eq!(r.domain, domain, "{src}");
        assert_eq!(r.x_intercept, domain, "{src}");
        assert_eq!(r.range, "y ∈ {0}", "{src}");
        assert!(r.minima.is_empty() && r.maxima.is_empty(), "{src}");
        assert!(r.vertical_asymptotes.is_empty(), "{src}");
        assert_eq!(r.too_complex_features, 0, "{src}");
        assert!(
            r.monotonicity
                .iter()
                .all(|(_, m)| *m == Monotonicity::Constant),
            "{src}"
        );
    }
    // The same for any constant wherever defined.
    let r = k("y=x^2/x^2");
    assert_eq!(r.domain, "x ∈ ℝ \\ {0}");
    assert_eq!(r.range, "y ∈ {1}");
    assert_eq!(r.x_intercept, "");
}

#[test]
fn genuine_holes_and_controls_are_unchanged() {
    let r = k("y=exp(1/x)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {0}");
    assert_eq!(r.vertical_asymptotes, ["x = 0"]);
    let r = k("y=1/(1+e^1000)");
    assert_eq!(r.range, "y ∈ {0}");
    assert_eq!(r.x_intercept, "x ∈ ℝ");
    let r = k("y=1/exp(1/x)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {0}");
    assert_eq!(r.range, "y ∈ (0, 1) ∪ (1, ∞)");
    let r = k("y=exp(1/x)^-1");
    assert_eq!(r.domain, "x ∈ ℝ \\ {0}");
    assert_eq!(r.vertical_asymptotes, ["x = 0"]);
    assert_eq!(k("y=x/x").range, "y ∈ {1}");
    assert_eq!(k("y=0^(-x)").domain, "x ∈ (−∞, 0]");
}

#[test]
fn unresolvable_values_are_unknown_not_a_gap() {
    // sin(e^x) beyond x ≈ 709.8 is the sine of a number beyond a double:
    // defined, but nothing can say where in its period it falls.
    for src in ["y=sin(exp(x))", "y=sin(x^200)"] {
        let r = k(src);
        assert!(!known(&r, flags::DOMAIN), "{src}: {}", r.domain);
        assert!(r.domain.is_empty(), "{src}: {}", r.domain);
    }
    // Far beyond the search it changes nothing.
    let r = k("y=sin(x^2)");
    assert_eq!(r.domain, "x ∈ ℝ");
    assert_eq!(r.range, "y ∈ [−1, 1]");
}

#[test]
fn a_stretch_of_zeros_between_jumps_is_not_none() {
    // floor(x) is 0 on [0, 1), a piece of its own: not "no x-intercepts".
    for src in [
        "y=floor(x)",
        "y=floor(x-3)",
        "y=floor(x)+1",
        "y=floor(x/1000)",
        "y=ceil(x)",
        "y=round(x)",
    ] {
        let r = k(src);
        assert!(!known(&r, flags::ZEROS), "{src}: {}", r.x_intercept);
    }
    // Underflow is not such a stretch.
    let r = k("y=e^x");
    assert!(known(&r, flags::ZEROS) && r.x_intercept.is_empty());
}

#[test]
fn a_y_intercept_beyond_a_double_is_unknown_not_none() {
    for src in [
        "y=(x-1000)*e^(-(x-1000))",
        "y=e^(x+1000)",
        "y=-e^(1000-x^2)",
    ] {
        let r = k(src);
        assert!(!known(&r, flags::Y_INTERCEPT), "{src}: {}", r.y_intercept);
    }
    // An undefined f(0) is still none.
    let r = k("y=1/x");
    assert!(known(&r, flags::Y_INTERCEPT) && r.y_intercept.is_empty());
}
