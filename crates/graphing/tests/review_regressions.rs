//! Regression tests for issues found in the external code review (REVIEW.md:
//! H-01, H-02, M-05, M-06, M-07). Each reproduces the reported input.

use std::time::{Duration, Instant};

use graphing::functions;
use graphing::graph::Graph;
use graphing::parser::parse_expression;
use graphing::viewport::{RangeError, Viewport};

/// H-01: deeply nested input must be an equation error, not a stack overflow.
#[test]
fn deep_nesting_is_an_error_not_a_crash() {
    let parens = format!("{}x{}", "(".repeat(20_000), ")".repeat(20_000));
    assert!(parse_expression(&parens).is_err());

    let powers = format!("x{}", "^x".repeat(20_000));
    assert!(parse_expression(&powers).is_err());

    let signs = format!("{}x", "-".repeat(20_000));
    assert!(parse_expression(&signs).is_err());

    let exp_signs = format!("x^{}x", "-".repeat(20_000));
    assert!(parse_expression(&exp_signs).is_err());

    // Through the full Graph path too (as the GUI does on every keystroke).
    let mut g = Graph::new();
    let id = g.add_equation(&format!("y={parens}"));
    assert!(g.error(id).is_some());
}

#[test]
fn reasonable_nesting_still_parses() {
    let e = format!("{}x{}", "(".repeat(50), ")".repeat(50));
    assert!(parse_expression(&e).is_ok());
    assert!(parse_expression("sin(cos(tan(((x+1)^2)^3)))").is_ok());
    assert!(parse_expression("--x").is_ok());
}

/// H-02: huge nCr / nPr arguments must finish promptly.
#[test]
fn huge_combinatorics_terminate_quickly() {
    let start = Instant::now();
    assert_eq!(functions::ncr(1e20, 1e19), f64::INFINITY);
    assert_eq!(functions::npr(1e20, 1e19), f64::INFINITY);
    let mut g = Graph::new();
    g.add_equation("y=nCr(10^20,10^19)");
    g.add_equation("y=nPr(10^20,10^19)");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "took {:?}",
        start.elapsed()
    );

    // Ordinary values are unchanged.
    assert_eq!(functions::ncr(5.0, 2.0), 10.0);
    assert_eq!(functions::ncr(52.0, 5.0), 2_598_960.0);
    assert_eq!(functions::npr(5.0, 2.0), 20.0);
    assert!(functions::ncr(f64::INFINITY, 2.0).is_nan());
}

/// M-06: coth must not lose large arguments to intermediate overflow.
#[test]
fn coth_large_arguments() {
    assert!((functions::coth(1000.0) - 1.0).abs() < 1e-12);
    assert!((functions::coth(-1000.0) + 1.0).abs() < 1e-12);
    assert!((functions::coth(1.0) - 1.0_f64.cosh() / 1.0_f64.sinh()).abs() < 1e-12);
    assert!(functions::coth(0.0).is_infinite());
}

/// M-07: huge even root degrees are even (no negative radicands).
#[test]
fn root_parity_for_large_degrees() {
    assert!(functions::root(-1.0, 1e20).is_nan());
    assert!(functions::root(-1.0, 2f64.powi(60)).is_nan());
    assert_eq!(functions::root(-8.0, 3.0), -2.0);
    assert!((functions::root(-32.0, 5.0) + 2.0).abs() < 1e-12);
    assert!(functions::root(-16.0, 4.0).is_nan());
}

/// M-05: manual ranges get the same sanity checks as zooming.
#[test]
fn manual_ranges_reject_infinite_spans() {
    let mut vp = Viewport::default_for_size(640.0, 480.0);
    let before = vp;
    assert_eq!(
        vp.set_display_ranges(-1e308, 1e308, -10.0, 10.0),
        Err(RangeError::OutOfRange)
    );
    assert_eq!(
        vp, before,
        "rejected ranges must leave the viewport untouched"
    );
    assert!(vp.set_display_ranges(-5.0, 5.0, -2.0, 2.0).is_ok());
    assert_eq!((vp.x_min, vp.x_max), (-5.0, 5.0));
}
