//! Powers in graphing follow the TI-84 Plus CE where it does better than
//! Windows (docs/ti-conventions.md):
//!
//! * an exponent that varies with x needs a positive base (or a zero base
//!   to a positive power), as in the TI's Real mode and IEEE 1788's pow, so
//!   x^x is defined for x > 0 only and (−2)^x nowhere;
//! * 0⁰ is undefined (the TI's ERR:DOMAIN), so x⁰ has a hole at 0;
//! * a constant exponent keeps real powers: integer powers of negative
//!   bases, and odd roots ((−8)^(1/3) = −2).
//!
//! The calculator modes keep Windows' 0⁰ = 1; these are graphing only.

use graphing::TrigUnit;
use graphing::analysis::truth::{R, reval};
use graphing::analysis::{AnalysisError, analyze_str};
use graphing::compile::{Input, compile_str};
use graphing::parser::parse_expression;

fn at(src: &str, x: f64) -> f64 {
    compile_str(src, TrigUnit::Radians).unwrap().eval_x(x)
}

/// The value at x by every evaluator: one point, a batch, and the
/// analysis' reference (None where that says undefined).
fn all(src: &str, x: f64) -> Option<f64> {
    let p = compile_str(src, TrigUnit::Radians).unwrap();
    let one = p.eval_x(x);
    let mut out = [0.0];
    p.eval_batch(Input::Slice(&[x]), Input::Scalar(0.0), &mut out);
    let reference = match reval(&parse_expression(src).unwrap(), x, TrigUnit::Radians) {
        R::V(v) => Some(v.f()),
        R::Undef => None,
        R::Unknown => panic!("{src} at {x}: reference unknown"),
    };
    assert!(
        one.to_bits() == out[0].to_bits() || (one.is_nan() && out[0].is_nan()),
        "{src} at {x}: point {one} vs batch {}",
        out[0]
    );
    assert_eq!(
        one.is_nan(),
        reference.is_none(),
        "{src} at {x}: {one} vs {reference:?}"
    );
    (!one.is_nan()).then_some(one)
}

#[test]
fn a_varying_exponent_needs_a_positive_base() {
    assert_eq!(all("x^x", 2.0), Some(4.0));
    assert_eq!(all("x^x", 0.0), None, "0⁰");
    assert_eq!(all("x^x", -1.0), None, "(−1)^(−1): a negative base");
    assert_eq!(all("x^x", -2.0), None);
    assert_eq!(all("(-2)^x", 2.0), None, "even at whole x");
    assert_eq!(all("(-2)^x", 0.5), None);
    assert_eq!(all("2^x", -3.0), Some(0.125));
    assert_eq!(all("0^x", 2.0), Some(0.0));
    assert_eq!(all("0^x", 0.0), None, "0⁰");
    assert_eq!(all("0^x", -1.0), None);
    assert_eq!(all("(x-1)^x", 3.0), Some(8.0));
    assert_eq!(all("(x-1)^x", 1.0), Some(0.0), "0 to a positive power");
    assert_eq!(all("(x-1)^x", 0.5), None);
}

#[test]
fn zero_to_the_zero_is_undefined_in_graphing() {
    assert_eq!(all("x^0", 3.0), Some(1.0));
    assert_eq!(all("x^0", -3.0), Some(1.0));
    assert_eq!(all("x^0", 0.0), None);
    assert_eq!(all("(x+1)^0", -1.0), None);
}

#[test]
fn a_constant_exponent_keeps_real_powers() {
    assert_eq!(at("(-8)^(1/3)", 0.0), -2.0);
    assert_eq!(all("x^(1/3)", -8.0), Some(-2.0));
    assert_eq!(all("x^3", -2.0), Some(-8.0));
    assert_eq!(all("x^0.5", 4.0), Some(2.0));
    assert_eq!(all("x^0.5", -1.0), None);
    assert_eq!(all("x^0.5", 0.0), Some(0.0));
    // A constant written as an expression (and a slider's value) is still a
    // constant: (−3)^(1+1) = 9.
    assert_eq!(all("x^(1+1)", -3.0), Some(9.0));
}

#[test]
fn the_panel_follows_the_ti_rule() {
    let k = analyze_str("y=x^x");
    assert_eq!(k.domain, "x ∈ (0, ∞)");
    assert!(k.y_intercept.is_empty(), "{}", k.y_intercept);
    assert!(k.vertical_asymptotes.is_empty());
    assert_eq!(k.range, "y ∈ [0.692201, ∞)");

    let k = analyze_str("y=x^0");
    assert_eq!(k.domain, "x ∈ ℝ \\ {0}");
    assert_eq!(k.range, "y ∈ {1}");
    assert!(k.y_intercept.is_empty());

    let k = analyze_str("y=0^x");
    assert_eq!(k.domain, "x ∈ (0, ∞)");
    // {0}, or unknown where the certifier doesn't prove the value.
    assert!(k.range == "y ∈ {0}" || k.range.is_empty(), "{}", k.range);

    assert_eq!(
        analyze_str("y=(-2)^x").analysis_error,
        AnalysisError::AnalysisCouldNotBePerformed,
        "defined nowhere"
    );

    let k = analyze_str("y=(-8)^(1/3)");
    assert_eq!(k.domain, "x ∈ ℝ");
    assert_eq!(k.range, "y ∈ {−2}");

    // ℝ; its range is unknown to the certifier (no f′ at 0 to join the
    // two monotone halves).
    let k = analyze_str("y=x^(1/3)");
    assert_eq!(k.domain, "x ∈ ℝ");
    assert!(k.range == "y ∈ ℝ" || k.range.is_empty(), "{}", k.range);

    assert_eq!(analyze_str("y=x^0.5").domain, "x ∈ [0, ∞)");
    assert_eq!(analyze_str("y=(x-1)^x").domain, "x ∈ [1, ∞)");
}
