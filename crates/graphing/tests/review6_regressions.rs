//! Round-6 review (R6-M-08): the analysis scans [−10⁶, 10⁶] closely, but a
//! feature beyond that window must not be extrapolated away into a false
//! global claim. Features out to 10¹⁵ are found. Past that, a polynomial
//! whose root bound reaches further, or a singularity that may lie further
//! out, makes the affected items unknown rather than wrong.

use graphing::analysis::{
    AnalysisError, KeyGraphFeatures, Monotonicity, Parity, analyze_str, flags,
};

use Monotonicity::{Decreasing, Increasing};

fn k(src: &str) -> KeyGraphFeatures {
    let r = analyze_str(src);
    assert_eq!(r.analysis_error, AnalysisError::NoError, "{src}");
    r
}

fn mono(r: &KeyGraphFeatures) -> Vec<(&str, Monotonicity)> {
    r.monotonicity
        .iter()
        .map(|(s, m)| (s.as_str(), *m))
        .collect()
}

/// The review's example: a pole at 2·10⁶ is a pole, not a domain of ℝ.
#[test]
fn a_pole_beyond_the_window() {
    let r = k("y = 1/(x - 2000000)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {2000000}");
    assert_eq!(r.range, "y ∈ ℝ \\ {0}");
    assert_eq!(r.vertical_asymptotes, ["x = 2000000"]);
    assert_eq!(
        mono(&r),
        [("(−∞, 2000000)", Decreasing), ("(2000000, ∞)", Decreasing)]
    );
    assert_eq!(r.too_complex_features, 0);
}

#[test]
fn poles_of_a_polynomial_denominator_beyond_the_window() {
    let r = k("y = 1/(x^2 - 4000000000000)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {−2000000, 2000000}");
    assert_eq!(r.vertical_asymptotes, ["x = −2000000", "x = 2000000"]);
    assert_eq!(r.parity, Parity::Even);
    let r = k("y = 1/(x - 2000000)^2");
    assert_eq!(r.domain, "x ∈ ℝ \\ {2000000}");
    assert_eq!(r.range, "y ∈ (0, ∞)");
}

#[test]
fn zeros_and_extrema_beyond_the_window() {
    let r = k("y = x - 2000000");
    assert_eq!(r.x_intercept, "2000000");
    let r = k("y = (x - 2000000)^2");
    assert_eq!(r.x_intercept, "2000000");
    assert_eq!(r.minima, ["(2000000, 0)"]);
    assert_eq!(r.range, "y ∈ [0, ∞)");
    assert_eq!(
        mono(&r),
        [("(−∞, 2000000)", Decreasing), ("(2000000, ∞)", Increasing)]
    );
    // Turning points a step either side of a far pole.
    let r = k("y = x + 1/(x - 2000000)");
    assert_eq!(r.maxima, ["(1999999, 1999998)"]);
    assert_eq!(r.minima, ["(2000001, 2000002)"]);
    assert_eq!(r.range, "y ∈ (−∞, 1999998] ∪ [2000002, ∞)");
}

#[test]
fn domains_that_start_beyond_the_window() {
    let r = k("y = log(x - 5000000)");
    assert_eq!(r.domain, "x ∈ (5000000, ∞)");
    assert_eq!(r.range, "y ∈ ℝ");
    assert_eq!(r.vertical_asymptotes, ["x = 5000000"]);
    let r = k("y = sqrt(x - 10000000)");
    assert_eq!(r.domain, "x ∈ [10000000, ∞)");
    assert_eq!(r.range, "y ∈ [0, ∞)");
    assert_eq!(r.x_intercept, "10000000");
    let r = k("y = 1/(sqrt(x) - 2000)");
    assert_eq!(r.vertical_asymptotes, ["x = 4000000"]);
}

/// A transcendental zero out at e³⁰ ≈ 1.07·10¹³, and growth through zero
/// at the far end, which used to cut the range off.
#[test]
fn a_logarithm_crossing_zero_far_out() {
    let r = k("y = ln(x) - 30");
    let z = r.data.zeros[0].x;
    assert!((z - 30f64.exp()).abs() <= 1e-6 * z, "{z}");
    assert_eq!(r.range, "y ∈ ℝ");
}

/// Values near 10⁻³⁰ are tiny, not constant, and not even.
#[test]
fn tiny_values_are_still_analysed() {
    let r = k("y = 1/(x^3 - 1000000000000000000000000000000)");
    assert_eq!(r.vertical_asymptotes, ["x = 10000000000"]);
    assert_eq!(r.parity, Parity::Neither);
    assert_eq!(
        mono(&r),
        [
            ("(−∞, 10000000000)", Decreasing),
            ("(10000000000, ∞)", Decreasing)
        ]
    );
}

/// Beyond 10¹⁵ nothing is searched: what may lie there is unknown, and no
/// symmetry is claimed.
#[test]
fn what_lies_beyond_the_search_is_unknown() {
    let r = k("y = 1/(x - 100000000000000000000)");
    let singular =
        flags::DOMAIN | flags::VERTICAL_ASYMPTOTES | flags::RANGE | flags::MONOTONE_INTERVALS;
    assert_eq!(r.too_complex_features & singular, singular);
    assert!(r.domain.is_empty() && r.vertical_asymptotes.is_empty());
    assert_eq!(r.parity, Parity::Unknown);
    let r = k("y = x - 100000000000000000000");
    assert_ne!(r.too_complex_features & flags::ZEROS, 0);
    assert!(r.x_intercept.is_empty());
    // Not polynomial, still heading for zero at 10¹⁵ (it gets there at e¹⁰⁰).
    let r = k("y = 1/(ln(x) - 100)");
    assert_ne!(r.too_complex_features & flags::DOMAIN, 0);
    assert!(r.domain.is_empty());
}

/// Ordinary functions don't trip any of this: same answers, and none of
/// the new "unknown" outcomes (some flag other features already, e.g.
/// sin(x²)'s endless zeros).
#[test]
fn ordinary_functions_are_unaffected() {
    for src in [
        "y = 1/(x - 2)",
        "y = x^0.9",
        "y = 1/ln(x)",
        "y = x/ln(x)",
        "y = ln(x)^2",
        "y = x*e^(-x)",
        "y = sin(x^2)",
        "y = sqrt(x^2 + 1)",
    ] {
        let r = analyze_str(src);
        assert_eq!(r.analysis_error, AnalysisError::NoError, "{src}");
        let new = r.too_complex_features & (flags::DOMAIN | flags::VERTICAL_ASYMPTOTES);
        assert_eq!(new, 0, "{src}");
        assert_ne!(r.parity, Parity::Unknown, "{src}");
    }
    let r = k("y = 1/(x - 2)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {2}");
    assert_eq!(r.vertical_asymptotes, ["x = 2"]);
}
