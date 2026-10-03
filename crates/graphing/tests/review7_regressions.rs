//! Round-7 review: features at the very end of the search, many far
//! polynomial roots, tiny range bounds and far zeros must not turn into
//! false global claims.
//!
//! - R7-M-01: a zero or pole exactly at ±10¹⁵ is found, and a polynomial's
//!   far roots aren't dropped as "oscillation".
//! - R7-M-02: a tiny range bound keeps its value and sign, so a gap between
//!   a negative maximum and a positive branch stays open.
//! - Question 1: functions of tiny size keep their own scale; a zero beyond
//!   the search is unknown, not denied; slowly converging tails don't fake
//!   a range bound or deny an asymptote.

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

/// R7-M-01: x − 10¹⁵ is exactly 0 at the last probe sample, with nothing
/// beyond to bracket it.
#[test]
fn a_zero_at_the_end_of_the_search() {
    let r = k("y = x - 1000000000000000");
    assert_eq!(r.x_intercept, "1000000000000000");
    assert_eq!(r.range, "y ∈ ℝ");
    assert_eq!(r.parity, Parity::Neither);
    assert_eq!(mono(&r), [("(−∞, ∞)", Increasing)]);
    assert_eq!(r.too_complex_features, 0);
}

#[test]
fn a_pole_at_the_end_of_the_search() {
    let r = k("y = 1/(x - 1000000000000000)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {1000000000000000}");
    assert_eq!(r.range, "y ∈ ℝ \\ {0}");
    assert_eq!(r.vertical_asymptotes, ["x = 1000000000000000"]);
    assert_eq!(r.horizontal_asymptotes, ["y = 0"]);
    assert_eq!(r.parity, Parity::Neither);
    assert_eq!(
        mono(&r),
        [
            ("(−∞, 1000000000000000)", Decreasing),
            ("(1000000000000000, ∞)", Decreasing)
        ]
    );
    assert_eq!(r.too_complex_features, 0);
    // The neighbours: one inside the search, one beyond it (unknown).
    let r = k("y = 1/(x - 999999999999999)");
    assert_eq!(r.vertical_asymptotes, ["x = 999999999999999"]);
    let r = k("y = 1/(x - 1000000000000001)");
    assert_ne!(r.too_complex_features & flags::VERTICAL_ASYMPTOTES, 0);
    assert_eq!(r.parity, Parity::Unknown);
}

/// R7-M-01: nine double roots between 2·10⁶ and 10⁷ are a polynomial's
/// roots, not oscillation to give up on.
#[test]
fn many_far_roots_of_a_polynomial() {
    let r = k(
        "y = ((x-2000000)*(x-3000000)*(x-4000000)*(x-5000000)*(x-6000000)\
               *(x-7000000)*(x-8000000)*(x-9000000)*(x-10000000))^2",
    );
    assert_eq!(
        r.x_intercept,
        "2000000, 3000000, 4000000, 5000000, 6000000, 7000000, 8000000, 9000000, 10000000"
    );
    let roots = [2, 3, 4, 5, 6, 7, 8, 9, 10].map(|m| format!("({m}000000, 0)"));
    assert_eq!(r.minima, roots);
    assert_eq!(r.maxima.len(), 8);
    assert_eq!(r.range, "y ∈ [0, ∞)");
    assert_eq!(r.parity, Parity::Neither);
    let m = mono(&r);
    assert_eq!(m.len(), 18);
    assert_eq!(m[0], ("(−∞, 2000000)", Decreasing));
    assert_eq!(m[17], ("(10000000, ∞)", Increasing));
    assert_eq!(r.too_complex_features, 0);
}

/// R7-M-02: the maximum −2.5·10⁻⁹ isn't rounded to 0 and merged with the
/// positive branch into ℝ.
#[test]
fn a_tiny_negative_maximum_keeps_the_gap() {
    let r = k("y = 1/(x^2 - 400000000)");
    assert_eq!(r.range, "y ∈ (−∞, −2.5×10⁻⁹] ∪ (0, ∞)");
    assert_eq!(r.maxima, ["(0, −2.5×10⁻⁹)"]);
    assert_eq!(r.y_intercept, "−2.5×10⁻⁹");
    // The round-6 fixture, now with its range and maximum checked too.
    let r = k("y = 1/(x^2 - 4000000000000)");
    assert_eq!(r.range, "y ∈ (−∞, −2.5×10⁻¹³] ∪ (0, ∞)");
    assert_eq!(r.maxima, ["(0, −2.5×10⁻¹³)"]);
    // The everyday version is unchanged.
    let r = k("y = 1/(x^2 - 4)");
    assert_eq!(r.range, "y ∈ (−∞, −1/4] ∪ (0, ∞)");
    assert_eq!(r.maxima, ["(0, −1/4)"]);
}

/// Question 1: rationals with nine poles, tiny everywhere (10⁻³³ and
/// 10⁻⁶¹ near 0): their extrema are extrema, not zeros, and the range is
/// a set, not `ℝ \ {0, ∞, ∞, ∞, ∞}`.
#[test]
fn nine_poles_and_a_tiny_function() {
    for (src, first_min) in [
        (
            "y = 1/((x-1000)*(x-2000)*(x-3000)*(x-4000)*(x-5000)*(x-6000)*(x-7000)*(x-8000)*(x-9000))",
            "(1300.62, 2.02874×10⁻³¹)",
        ),
        (
            "y = 1/((x-2000000)*(x-3000000)*(x-4000000)*(x-5000000)*(x-6000000)*(x-7000000)\
             *(x-8000000)*(x-9000000)*(x-10000000))",
            "(2300620.003, 2.02874×10⁻⁵⁸)",
        ),
    ] {
        let r = k(src);
        assert_eq!(r.range, "y ∈ ℝ \\ {0}", "{src}");
        assert_eq!(r.x_intercept, "", "{src}");
        assert_eq!(r.vertical_asymptotes.len(), 9, "{src}");
        assert_eq!(r.minima.len(), 4, "{src}");
        assert_eq!(r.maxima.len(), 4, "{src}");
        assert_eq!(r.minima[0], first_min, "{src}");
        assert_eq!(r.parity, Parity::Neither, "{src}");
        assert_eq!(r.too_complex_features & flags::RANGE, 0, "{src}");
        assert_eq!(r.too_complex_features & flags::ZEROS, 0, "{src}");
    }
}

/// Question 1: ln x − 40 crosses 0 at e⁴⁰ ≈ 2.4·10¹⁷, beyond the search.
/// That zero is unknown, not denied, and the range follows the divergence.
#[test]
fn a_zero_beyond_the_search_is_not_denied() {
    for (src, range) in [
        ("y = ln(x) - 40", "y ∈ ℝ"),
        ("y = sqrt(x) - 100000000", "y ∈ [−100000000, ∞)"),
        ("y = ln(x) - 800", "y ∈ ℝ"),
    ] {
        let r = k(src);
        assert_ne!(r.too_complex_features & flags::ZEROS, 0, "{src}");
        assert_eq!(r.x_intercept, "", "{src}");
        assert_eq!(r.range, range, "{src}");
        assert_eq!(r.horizontal_asymptotes, Vec::<String>::new(), "{src}");
    }
    // Crossing inside the search: listed (e³⁰ = 10686474581524.46, to six
    // digits; not as the integer it is 0.46 from).
    let r = k("y = ln(x) - 30");
    assert_eq!(r.x_intercept, "1.06865×10¹³");
    assert_eq!(r.range, "y ∈ ℝ");
    assert_eq!(r.too_complex_features, 0);
}

/// Question 1: slowly decaying tails. A power tail has an exact limit; a
/// logarithmic one creeps too slowly to pin down, so its range and
/// asymptotes are unknown rather than a bound taken from x ≈ 10⁴.
#[test]
fn slow_tails() {
    for src in ["y = x^-0.1", "y = x^-0.01"] {
        let r = k(src);
        assert_eq!(r.range, "y ∈ (0, ∞)", "{src}");
        assert_eq!(r.horizontal_asymptotes, ["y = 0"], "{src}");
        assert_eq!(r.too_complex_features, 0, "{src}");
    }
    // Its range creeps (toward 0 from below as x → 0⁺), so stays unknown;
    // its form shows the limit at +∞ exactly (ln x grows without bound).
    let r = k("y = 1/ln(x)");
    assert_ne!(r.too_complex_features & flags::RANGE, 0);
    assert_eq!(r.range, "");
    assert_eq!(r.horizontal_asymptotes, ["y = 0"]);
}

/// What was right stays right: oscillation beyond the window doesn't
/// cost an observed symmetry or range, and rounding noise in f″ of a
/// function linear in disguise isn't a far inflection.
#[test]
fn ordinary_results_survive() {
    let r = k("y = sin(x)/x");
    assert_eq!(r.parity, Parity::Even);
    assert_eq!(r.range, "y ∈ [−0.217234, 1)");
    let r = k("y = sin(x^2)");
    assert_eq!(r.parity, Parity::Even);
    // (Round 9: right, but sampling can't show its bounds are global, so
    // the gate leaves it unknown; see tests/analysis.rs.)
    assert!(r.range == "y ∈ [−1, 1]" || r.too_complex_features & flags::RANGE != 0);
    let r = k("y = x*sin(x)");
    assert_eq!(r.parity, Parity::Even);
    assert_eq!(r.range, "y ∈ ℝ");
    let r = k("y = (x^2-1)/(x-1)");
    assert_eq!(r.range, "y ∈ ℝ \\ {2}");
    assert_eq!(r.too_complex_features, 0);
}

/// A point where an intermediate is undefined (division by exactly zero,
/// the log of zero) is outside the domain even if an outer function would
/// turn the resulting ±∞ into a finite value.
#[test]
fn undefined_intermediates_stay_undefined() {
    use graphing::functions;
    assert!(functions::div(1.0, 0.0).is_nan());
    assert!(functions::ln(0.0).is_nan());
    assert!(functions::log10(-0.0).is_nan());
    assert!(functions::log_base(2.0, 0.0).is_nan());
    // Overflow of a finite value still gives ∞: 1/(1 + e^1000) is 0.
    assert_eq!(functions::div(1.0, 1.0 + 1000f64.exp()), 0.0);

    for (expr, domain) in [
        ("y=atan(1/x)", "x ∈ ℝ \\ {0}"),
        ("y=e^(-1/x^2)", "x ∈ ℝ \\ {0}"),
        ("y=e^(ln(x))", "x ∈ (0, ∞)"),
        ("y=1/ln(x)", "x ∈ (0, 1) ∪ (1, ∞)"),
    ] {
        let mut g = graphing::Graph::new();
        let id = g.add_equation(expr);
        let k = g.analyze(id);
        let items = k.items();
        let shown = items
            .iter()
            .find(|i| i.title == graphing::strings::DOMAIN)
            .map(|i| i.display_items.join(" "))
            .unwrap_or_default();
        assert_eq!(shown, domain, "{expr}");
        let y = items
            .iter()
            .find(|i| i.title == graphing::strings::Y_INTERCEPT)
            .map(|i| i.display_items.join(" "))
            .unwrap_or_default();
        assert_eq!(y, graphing::strings::KGF_Y_INTERCEPT_NONE, "{expr}");
    }
}
