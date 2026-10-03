//! Round-10 review.
//!
//! R10-M-01: an intermediate that overflows or underflows a double can
//! mislead later operations even when the final double is finite:
//! √(−e^(−800)) took −0 for −(something tiny) and gave −0, so
//! 1 + √(−e^(−800)) was 1 rather than undefined; √(e^(−800))·e^400 was 0
//! rather than 1. Extended range was only tried for a NaN or ±∞ result.
//! Sums can overflow too ((x + 2¹⁰²³) + (x + 2¹⁰²³)).
//!
//! R10-L-01: the reference evaluator (the analysis gate's truth) ordered
//! values beyond the doubles by the doubles standing in for them
//! (max(e^1000, e^2000) was e^1000), and gave every e^t below its exponent
//! range one shared stand-in, so e^(−x²)/e^(−2x²) was 1 at x = 10¹⁰.

use graphing::TrigUnit;
use graphing::analysis::truth::{R, reval};
use graphing::analysis::{AnalysisError, analyze_str};
use graphing::compile::{Input, compile_str};
use graphing::parser::parse_expression;

fn at(src: &str, x: f64) -> f64 {
    compile_str(src, TrigUnit::Radians).unwrap().eval_x(x)
}

fn batch(src: &str, xs: &[f64]) -> Vec<f64> {
    let p = compile_str(src, TrigUnit::Radians).unwrap();
    let mut out = vec![0.0; xs.len()];
    p.eval_batch(Input::Slice(xs), Input::Scalar(0.0), &mut out);
    out
}

fn close(v: f64, want: f64) -> bool {
    v == want || (v - want).abs() <= 1e-12 * want.abs().max(1.0)
}

#[test]
fn a_finite_result_from_an_undefined_operand_is_undefined() {
    // −e^(−(x+800)) is negative wherever it is defined, so its square root
    // is undefined everywhere, however small it is.
    let xs = [-1000.0, -100.0, -54.0, 0.0, 100.0, 1e6];
    for src in ["1+sqrt(-exp(-(x+800)))", "sqrt(-exp(-(x+800)))^0"] {
        for &x in &xs {
            assert!(at(src, x).is_nan(), "{src} at {x}: {}", at(src, x));
        }
        assert!(batch(src, &xs).iter().all(|v| v.is_nan()), "{src} batch");
        // Nothing to analyse: no domain, no value, no curve.
        let k = analyze_str(&format!("y={src}"));
        assert!(
            k.analysis_error != AnalysisError::NoError || k.data.domain.is_empty(),
            "{src}: {:?} {:?}",
            k.domain,
            k.y_intercept
        );
    }
}

#[test]
fn a_finite_result_keeps_the_value_an_underflow_hid() {
    let xs = [-100.0, 0.0, 500.0, 1000.0];
    for &x in &xs {
        let v = at("sqrt(exp(-(x+800)))*exp((x+800)/2)", x);
        assert!(close(v, 1.0), "at {x}: {v}");
    }
    let vs = batch("sqrt(exp(-(x+800)))*exp((x+800)/2)", &xs);
    assert!(vs.iter().all(|&v| close(v, 1.0)), "{vs:?}");
    // e^(−1000) is a constant beyond the doubles, not 0.
    for x in [-3.0, -1.0, 0.0, 2.0, 1e6] {
        let v = at("exp(-1000)*(x+1)*exp(500)*exp(500)", x);
        assert!(close(v, x + 1.0), "at {x}: {v}");
    }
    let k = analyze_str("y=sqrt(exp(-(x+800)))*exp((x+800)/2)");
    if let Some(y0) = k.data.y_intercept {
        assert!(close(y0, 1.0), "y-intercept {y0}");
    }
}

#[test]
fn a_sum_that_overflows_is_redone() {
    let src = "((x+2^1023)+(x+2^1023))/(x+2^1023)";
    for x in [0.0, 1.0, -5.0] {
        assert!(close(at(src, x), 2.0), "at {x}: {}", at(src, x));
    }
    assert!(batch(src, &[0.0, 1.0]).iter().all(|&v| close(v, 2.0)));
    assert!(close(at("(x+10^308)+(x+10^308)-10^308", 0.0), 1e308));
}

#[test]
fn genuinely_undefined_and_ordinary_values_stay() {
    assert!(at("exp(1/x)", 0.0).is_nan());
    assert!(at("0^(-x)", 1.0).is_nan());
    assert!(at("ln(x)", 0.0).is_nan());
    assert!(at("sqrt(x-1)+1", 0.0).is_nan());
    assert_eq!(at("1/(1+e^1000)", 3.0), 0.0);
    assert_eq!(at("1/(1+exp(x))", 800.0), 0.0);
    assert!(close(
        at("atan(exp(x))", 1000.0),
        std::f64::consts::FRAC_PI_2
    ));
    assert!(close(at("exp(-x^2)*cos(4x)+1", 1e8), 1.0));
    assert_eq!(at("x*0+1", 5.0), 1.0);
    assert_eq!(at("floor(x)*x+1", 0.5), 1.0);
    // Exactly 0 is still exactly 0.
    assert_eq!(at("(x-x)*exp(x)+0", 3.0), 0.0);
}

fn reference(src: &str, x: f64) -> R {
    reval(&parse_expression(src).unwrap(), x, TrigUnit::Radians)
}

/// ln of the reference value, if it has one.
fn ln_ref(src: &str, x: f64) -> Option<f64> {
    match reference(src, x) {
        R::V(v) if !v.lost() && v.sign() > 0.0 => Some(v.ln()),
        _ => None,
    }
}

#[test]
fn reference_orders_values_beyond_the_doubles() {
    for (src, want) in [
        ("max(exp(1000),exp(2000))", 2000.0),
        ("max(exp(2000),exp(1000))", 2000.0),
        ("min(exp(1000),exp(2000))", 1000.0),
        ("min(exp(2000),exp(1000))", 1000.0),
        ("max(exp(-1000),exp(-2000))", -1000.0),
        ("min(exp(-2000),exp(-1000))", -2000.0),
    ] {
        let l = ln_ref(src, 0.0).unwrap_or(f64::NAN);
        assert!((l - want).abs() < 1e-9 * want.abs(), "{src}: ln = {l}");
    }
    // The compiled program agrees.
    assert!(close(at("ln(max(exp(1000),exp(2000)))", 0.0), 2000.0));
    assert!(close(at("ln(min(exp(1000),exp(2000)))", 0.0), 1000.0));
    // And the reference's own ln of it.
    match reference("ln(max(exp(1000),exp(2000)))", 0.0) {
        R::V(v) => assert!(close(v.f(), 2000.0), "{v:?}"),
        r => panic!("{r:?}"),
    }
}

#[test]
fn reference_never_makes_up_a_size_it_lost() {
    // Below the exponent range e^(−x²) is only known to be tinier than
    // anything held exactly: what depends on its size is unknown, not made
    // up.
    let x = 1e10;
    match reference("ln(exp(-x^2))", x) {
        R::Unknown => {}
        R::V(v) => assert!(close(v.f(), -1e20), "{v:?}"),
        R::Undef => panic!("undefined"),
    }
    match reference("exp(-x^2)/exp(-2*x^2)", x) {
        R::Unknown => {}
        R::V(v) => assert!(v.huge() || v.lost(), "made up {v:?}"),
        R::Undef => panic!("undefined"),
    }
    // (Two lost values of opposite sign: not known to cancel.)
    if let R::V(v) = reference("exp(-x^2)-exp(-2*x^2)", x) {
        assert!(!v.is_zero(), "{v:?}");
    }
    // What doesn't need the size keeps working.
    match reference("exp(-x^2)+1", x) {
        R::V(v) => assert_eq!(v.f(), 1.0),
        r => panic!("{r:?}"),
    }
    match reference("cos(exp(-x^2))", x) {
        R::V(v) => assert_eq!(v.f(), 1.0),
        r => panic!("{r:?}"),
    }
    match reference("exp(-x^2)*x", x) {
        R::V(v) => assert!(v.lost() && v.sign() > 0.0, "{v:?}"),
        r => panic!("{r:?}"),
    }
    // Where the size is held exactly, it is used.
    let l = ln_ref("exp(-x^2)/exp(-2*x^2)", 100.0).unwrap();
    assert!((l - 10000.0).abs() < 1e-9 * 10000.0, "{l}");
    let l = match reference("ln(exp(-x^2))", 1e6) {
        R::V(v) => v.f(),
        r => panic!("{r:?}"),
    };
    assert!(close(l, -1e12), "{l}");
}
