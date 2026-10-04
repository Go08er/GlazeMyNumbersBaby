//! Function analysis checked against known answers.
//!
//! The panel shows the certified analysis only (`graphing::certify`): where
//! the certifier proves no answer the row is unknown (its too-complex
//! flag), never a guess; a list proven correct but not complete carries a
//! note (its partial flag).

use graphing::analysis::{
    AnalysisError, KeyGraphFeatures, Monotonicity, Parity, Periodicity, analyze, analyze_str, flags,
};
use graphing::compile::CompileOptions;
use graphing::equation::Equation;
use graphing::functions::TrigUnit;
use std::f64::consts::{E, FRAC_PI_2, PI};

use Monotonicity::{Constant, Decreasing, Increasing};

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

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

#[test]
fn quadratic() {
    let r = k("y = x^2");
    assert_eq!(r.domain, "x ∈ ℝ");
    assert_eq!(r.range, "y ∈ [0, ∞)");
    assert_eq!(r.x_intercept, "0");
    assert_eq!(r.y_intercept, "0");
    assert_eq!(r.minima, ["(0, 0)"]);
    assert!(r.maxima.is_empty() && r.inflection_points.is_empty());
    assert!(
        r.vertical_asymptotes.is_empty()
            && r.horizontal_asymptotes.is_empty()
            && r.oblique_asymptotes.is_empty()
    );
    assert_eq!(r.parity, Parity::Even);
    assert_eq!(r.periodicity_direction, Periodicity::NotPeriodic);
    assert_eq!(mono(&r), [("(−∞, 0)", Decreasing), ("(0, ∞)", Increasing)]);
    assert_eq!(r.too_complex_features, 0);
}

#[test]
fn cubic() {
    let r = k("x^3");
    assert_eq!(r.range, "y ∈ ℝ");
    assert_eq!(r.inflection_points, ["(0, 0)"]);
    assert_eq!(r.parity, Parity::Odd);
    assert_eq!(mono(&r), [("(−∞, ∞)", Increasing)]);

    let r = k("x^3 - 3x");
    assert_eq!(r.x_intercept, "−√3, 0, √3");
    assert_eq!(r.minima, ["(1, −2)"]);
    assert_eq!(r.maxima, ["(−1, 2)"]);
    assert_eq!(r.inflection_points, ["(0, 0)"]);
    assert_eq!(
        mono(&r),
        [
            ("(−∞, −1)", Increasing),
            ("(−1, 1)", Decreasing),
            ("(1, ∞)", Increasing)
        ]
    );

    let r = k("f(x) = x^3 - 6x^2 + 9x");
    // The double zero at 3 (a touch, no sign change) is not proven by the
    // zeros row within the budget; the minimum (3, 0) shows it is a zero,
    // but not that there are no others: some of them.
    assert_eq!(r.x_intercept, "0, 3");
    assert!(r.partial_features & flags::ZEROS != 0);
    assert_eq!(r.minima, ["(3, 0)"]);
    assert_eq!(r.maxima, ["(1, 4)"]);
    assert_eq!(r.inflection_points, ["(2, 2)"]);
    assert_eq!(r.parity, Parity::Neither);
}

#[test]
fn quartic() {
    let r = k("x^4 - 5x^2 + 4");
    assert_eq!(r.x_intercept, "−2, −1, 1, 2");
    assert_eq!(r.y_intercept, "4");
    assert_eq!(r.range, "y ∈ [−9/4, ∞)");
    assert_eq!(r.minima, ["(−√10/2, −9/4)", "(√10/2, −9/4)"]);
    assert_eq!(r.maxima, ["(0, 4)"]);
    assert_eq!(r.inflection_points, ["(−√30/6, 19/36)", "(√30/6, 19/36)"]);
    assert_eq!(r.parity, Parity::Even);
}

#[test]
fn reciprocal() {
    let r = k("1/x");
    assert_eq!(r.domain, "x ∈ ℝ \\ {0}");
    assert_eq!(r.range, "y ∈ ℝ \\ {0}");
    assert_eq!(r.vertical_asymptotes, ["x = 0"]);
    assert_eq!(r.horizontal_asymptotes, ["y = 0"]);
    assert!(r.x_intercept.is_empty() && r.y_intercept.is_empty());
    assert_eq!(r.parity, Parity::Odd);
    assert_eq!(mono(&r), [("(−∞, 0)", Decreasing), ("(0, ∞)", Decreasing)]);

    let r = k("1/x^2");
    assert_eq!(r.range, "y ∈ (0, ∞)");
    assert_eq!(r.parity, Parity::Even);
    assert_eq!(mono(&r), [("(−∞, 0)", Increasing), ("(0, ∞)", Decreasing)]);
}

#[test]
fn rational_functions() {
    let r = k("(2x+1)/(x-3)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {3}");
    assert_eq!(r.range, "y ∈ ℝ \\ {2}");
    assert_eq!(r.x_intercept, "−1/2");
    assert_eq!(r.y_intercept, "−1/3");
    assert_eq!(r.vertical_asymptotes, ["x = 3"]);
    assert_eq!(r.horizontal_asymptotes, ["y = 2"]);

    let r = k("x + 1/x");
    assert_eq!(r.range, "y ∈ (−∞, −2] ∪ [2, ∞)");
    assert_eq!(r.minima, ["(1, 2)"]);
    assert_eq!(r.maxima, ["(−1, −2)"]);
    assert_eq!(r.oblique_asymptotes, ["y = x"]);
    assert_eq!(r.vertical_asymptotes, ["x = 0"]);

    let r = k("x^2/(x+1)");
    assert_eq!(r.oblique_asymptotes, ["y = x − 1"]);
    // f′'s sign is not decided near the pole within the budget: the range
    // and monotonicity are unknown, the extrema found are some of them.
    assert!(r.too_complex_features & flags::RANGE != 0);
    assert!(r.range.is_empty());
    assert_eq!(r.maxima, ["(−2, −4)"]);
    assert!(r.partial_features & flags::MAXIMA != 0);

    let r = k("1/(x^2-4)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {−2, 2}");
    assert_eq!(r.vertical_asymptotes, ["x = −2", "x = 2"]);
    assert_eq!(r.range, "y ∈ (−∞, −1/4] ∪ (0, ∞)");
    assert_eq!(r.maxima, ["(0, −1/4)"]);

    let r = k("(x^2+1)/(x^2-1)");
    assert_eq!(r.horizontal_asymptotes, ["y = 1"]);
    assert_eq!(r.range, "y ∈ (−∞, −1] ∪ (1, ∞)");

    // Even-order pole found from a non-sign-changing denominator.
    let r = k("1/(x-1)^2");
    assert_eq!(r.vertical_asymptotes, ["x = 1"]);
    assert_eq!(r.range, "y ∈ (0, ∞)");
}

#[test]
fn removable_discontinuity_is_not_an_asymptote() {
    let r = k("(x^2-1)/(x-1)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {1}");
    assert!(r.vertical_asymptotes.is_empty());
    assert_eq!(r.range, "y ∈ ℝ \\ {2}");
    assert_eq!(r.x_intercept, "−1");
    assert!(r.inflection_points.is_empty());
    assert!(
        r.oblique_asymptotes.is_empty(),
        "the function is the line itself: {:?}",
        r.oblique_asymptotes
    );
    // So is a constant: no horizontal asymptote.
    assert!(k("x/x").horizontal_asymptotes.is_empty());
    assert!(k("5").horizontal_asymptotes.is_empty());
}

#[test]
fn trigonometric() {
    let r = k("sin(x)");
    assert_eq!(r.periodicity_direction, Periodicity::Periodic);
    assert_eq!(r.periodicity_expression, "2π");
    assert_eq!(r.range, "y ∈ [−1, 1]");
    assert_eq!(r.x_intercept, "kπ, k ∈ ℤ");
    assert_eq!(r.maxima, ["(π/2 + 2kπ, 1), k ∈ ℤ"]);
    assert_eq!(r.minima, ["(−π/2 + 2kπ, −1), k ∈ ℤ"]);
    assert_eq!(r.inflection_points, ["(kπ, 0), k ∈ ℤ"]);
    assert_eq!(r.parity, Parity::Odd);
    assert_eq!(
        mono(&r),
        [
            ("(−π/2 + 2kπ, π/2 + 2kπ), k ∈ ℤ", Increasing),
            ("(π/2 + 2kπ, 3π/2 + 2kπ), k ∈ ℤ", Decreasing)
        ]
    );
    // Numeric data for markers.
    let z = &r.data.zeros;
    assert_eq!(z.len(), 1);
    assert!(z[0].contains(3.0 * PI, 1e-9) && !z[0].contains(1.0, 1e-9));

    let r = k("cos(x)");
    assert_eq!(r.parity, Parity::Even);
    assert_eq!(r.x_intercept, "π/2 + kπ, k ∈ ℤ");
    assert_eq!(r.y_intercept, "1");

    let r = k("tan(x)");
    assert_eq!(r.periodicity_expression, "π");
    assert_eq!(r.domain, "x ∈ ℝ \\ {π/2 + kπ | k ∈ ℤ}");
    assert_eq!(r.vertical_asymptotes, ["x = π/2 + kπ, k ∈ ℤ"]);
    assert_eq!(r.range, "y ∈ ℝ");
    assert_eq!(mono(&r), [("(−π/2 + kπ, π/2 + kπ), k ∈ ℤ", Increasing)]);

    let r = k("sin(2x)");
    assert_eq!(r.periodicity_expression, "π");
    assert_eq!(r.x_intercept, "kπ/2, k ∈ ℤ");

    let r = k("sin(x) + cos(x)");
    assert_eq!(r.range, "y ∈ [−√2, √2]");
    assert_eq!(r.maxima, ["(π/4 + 2kπ, √2), k ∈ ℤ"]);

    // |sin x| and sin²x repeat every π, but their least period is not
    // proven: the row says it is unknown (not shown as 2π, a period but
    // not the least).
    for f in ["|sin(x)|", "sin(x)^2"] {
        let r = k(f);
        assert!(r.periodicity_expression.is_empty(), "{f}");
        assert!(r.too_complex_features & flags::PERIODICITY != 0, "{f}");
    }
    let r = k("sin(x)^2");
    assert_eq!(r.range, "y ∈ [0, 1]");
    assert_eq!(r.minima, ["(kπ, 0), k ∈ ℤ"]);
    assert_eq!(r.inflection_points, ["(π/4 + kπ/2, 1/2), k ∈ ℤ"]);
    let r = k("sec(x)");
    assert_eq!(r.range, "y ∈ (−∞, −1] ∪ [1, ∞)");
    let r = k("cot(x)");
    assert_eq!(r.vertical_asymptotes, ["x = kπ, k ∈ ℤ"]);
}

#[test]
fn degrees_mode() {
    let eq = Equation::parse("y = sin(x)").unwrap();
    let r = analyze(
        &eq,
        &CompileOptions {
            trig_unit: TrigUnit::Degrees,
            variables: &(),
        },
    );
    assert_eq!(r.periodicity_expression, "360");
    assert_eq!(r.x_intercept, "180k, k ∈ ℤ");
    assert_eq!(r.maxima, ["(90 + 360k, 1), k ∈ ℤ"]);
    let r = analyze(
        &eq,
        &CompileOptions {
            trig_unit: TrigUnit::Grads,
            variables: &(),
        },
    );
    assert_eq!(r.periodicity_expression, "400");
}

#[test]
fn exponential_and_logarithmic() {
    let r = k("e^x");
    assert_eq!(r.domain, "x ∈ ℝ");
    assert_eq!(r.range, "y ∈ (0, ∞)");
    assert_eq!(r.horizontal_asymptotes, ["y = 0"]);
    assert_eq!(r.y_intercept, "1");
    assert!(r.x_intercept.is_empty());
    assert_eq!(mono(&r), [("(−∞, ∞)", Increasing)]);
    assert_eq!(r.too_complex_features, 0);

    let r = k("2^x - 4");
    assert_eq!(r.x_intercept, "2");
    assert_eq!(r.range, "y ∈ (−4, ∞)");
    assert_eq!(r.horizontal_asymptotes, ["y = −4"]);

    let r = k("ln(x)");
    assert_eq!(r.domain, "x ∈ (0, ∞)");
    assert_eq!(r.range, "y ∈ ℝ");
    assert_eq!(r.x_intercept, "1");
    assert_eq!(r.vertical_asymptotes, ["x = 0"]);
    assert!(r.y_intercept.is_empty());

    let r = k("log(2, x)");
    assert_eq!(r.domain, "x ∈ (0, ∞)");
    assert_eq!(r.vertical_asymptotes, ["x = 0"]);

    // 1/e exactly: f′(1/e) = ln(1/e) + 1 = 0 in exact arithmetic.
    let r = k("x ln(x)");
    assert_eq!(r.minima, ["(1/e, −1/e)"]);
    assert_eq!(r.range, "y ∈ [−1/e, ∞)");

    let r = k("ln(ln(x))");
    assert_eq!(r.domain, "x ∈ (1, ∞)");
    assert_eq!(r.x_intercept, "e");

    let r = k("e^(-x^2)");
    assert_eq!(r.maxima, ["(0, 1)"]);
    assert_eq!(r.range, "y ∈ (0, 1]");
    assert_eq!(r.horizontal_asymptotes, ["y = 0"]);
    assert_eq!(r.inflection_points.len(), 2);
    let (fam, y) = r.data.inflection_points[1];
    assert!(close(fam.x, 0.5f64.sqrt()) && close(y, (-0.5f64).exp()));

    let r = k("1/(1+e^-x)");
    assert_eq!(r.range, "y ∈ (0, 1)");
    assert_eq!(r.inflection_points, ["(0, 1/2)"]);
    assert_eq!(r.horizontal_asymptotes, ["y = 1", "y = 0"]);

    let r = k("e^(1/x)");
    assert_eq!(r.domain, "x ∈ ℝ \\ {0}");
    assert_eq!(r.vertical_asymptotes, ["x = 0"]);
    assert_eq!(r.horizontal_asymptotes, ["y = 1"]);
    assert_eq!(r.range, "y ∈ (0, 1) ∪ (1, ∞)");
    let _ = E;
}

#[test]
fn inverse_trig_and_hyperbolic() {
    let r = k("arctan(x)");
    assert_eq!(r.range, "y ∈ (−π/2, π/2)");
    assert_eq!(r.horizontal_asymptotes, ["y = π/2", "y = −π/2"]);
    assert!(close(r.data.horizontal_asymptotes[0].0, FRAC_PI_2));
    let r = k("arcsin(x)");
    assert_eq!(r.domain, "x ∈ [−1, 1]");
    assert_eq!(r.range, "y ∈ [−π/2, π/2]");
    let r = k("cosh(x)");
    assert_eq!(r.range, "y ∈ [1, ∞)");
    assert_eq!(r.minima, ["(0, 1)"]);
    let r = k("tanh(x)");
    assert_eq!(r.range, "y ∈ (−1, 1)");
}

#[test]
fn roots_and_absolute_values() {
    let r = k("sqrt(x)");
    assert_eq!(r.domain, "x ∈ [0, ∞)");
    assert_eq!(r.range, "y ∈ [0, ∞)");
    assert_eq!(r.x_intercept, "0");
    let r = k("sqrt(1-x^2)");
    assert_eq!(r.domain, "x ∈ [−1, 1]");
    assert_eq!(r.range, "y ∈ [0, 1]");
    assert_eq!(r.maxima, ["(0, 1)"]);
    // |x| turns at a corner, placed exactly at 0 with f′ of each side's
    // form (−x, x) strictly signed there. x^(1/3) has a vertical tangent at
    // 0: the certifier proves inflections from f″ only, so that row is
    // unknown.
    let r = k("|x|");
    assert_eq!(r.too_complex_features & flags::MINIMA, 0);
    assert_eq!(r.minima, ["(0, 0)"]);
    assert_eq!(r.x_intercept, "0");
    let r = k("|x-3|");
    assert_eq!(r.minima, ["(3, 0)"]);
    assert_eq!(r.x_intercept, "3");
    let r = k("x^(1/3)");
    assert_eq!(r.domain, "x ∈ ℝ");
    assert!(r.too_complex_features & flags::INFLECTION_POINTS != 0);
    // The simplifier finds no limit of √(x² + 1)/x: unknown.
    let r = k("sqrt(x^2+1)");
    assert!(r.too_complex_features & flags::OBLIQUE_ASYMPTOTES != 0);
    assert_eq!(r.minima, ["(0, 1)"]);
    assert_eq!(r.range, "y ∈ [1, ∞)");
}

#[test]
fn piecewise_constant() {
    let r = k("y = 5");
    assert_eq!(r.range, "y ∈ {5}");
    assert_eq!(r.y_intercept, "5");
    assert_eq!(mono(&r), [("(−∞, ∞)", Constant)]);
    // Steps: the range is not proven (the certifier's range is the union
    // of monotone pieces' images, and a jump has no f′).
    let r = k("sign(x)");
    assert!(r.too_complex_features & flags::RANGE != 0);
    assert_eq!(r.x_intercept, "0");
    let r = k("x/|x|");
    assert_eq!(r.domain, "x ∈ ℝ \\ {0}");
    assert!(r.too_complex_features & flags::RANGE != 0);
    assert_eq!(r.horizontal_asymptotes, ["y = 1", "y = −1"]);
}

#[test]
fn parameters_use_current_values() {
    let eq = Equation::parse("y = a x^2 + b").unwrap();
    let vars = [("a", -2.0), ("b", 3.0)];
    let r = analyze(
        &eq,
        &CompileOptions {
            trig_unit: TrigUnit::Radians,
            variables: &vars,
        },
    );
    assert_eq!(r.range, "y ∈ (−∞, 3]");
    assert_eq!(r.maxima, ["(0, 3)"]);
}

#[test]
fn unsupported_and_too_complex() {
    assert_eq!(
        analyze_str("x = y^2").analysis_error,
        AnalysisError::VariableIsNotX
    );
    assert_eq!(
        analyze_str("x^2 + y^2 = 1").analysis_error,
        AnalysisError::AnalysisNotSupported
    );
    assert_eq!(
        analyze_str("y > x").analysis_error,
        AnalysisError::AnalysisNotSupported
    );
    assert_eq!(
        analyze_str("y = sqrt(-1 - x^2)").analysis_error,
        AnalysisError::AnalysisCouldNotBePerformed
    );
    let e = analyze_str("x = y^2");
    assert_eq!(
        e.analysis_error_string(),
        Some("Analysis is only supported for functions in the f(x) format. Example: y=x")
    );
    assert!(e.items().is_empty());

    // Infinitely many zeros, not periodic: some of them, with a note.
    let r = k("sin(x^2)");
    assert!(r.partial_features & flags::ZEROS != 0);
    assert!(r.too_complex_features & flags::ZEROS == 0);
    assert!(r.too_complex_features & flags::RANGE != 0);
    let r = k("sin(x)/x");
    assert!(r.partial_features & flags::ZEROS != 0);
    assert!(r.x_intercept.starts_with("−10π, −9π, "));
    assert_eq!(r.horizontal_asymptotes, ["y = 0"]);
    let r = k("x!");
    assert!(r.too_complex_features & flags::DOMAIN != 0);
}

#[test]
fn panel_items_follow_the_original_layout() {
    let r = k("y = x^2");
    let items = r.items();
    let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Domain",
            "Range",
            "X-Intercept",
            "Y-Intercept",
            "Minima",
            "Maxima",
            "Inflection points",
            "Vertical asymptotes",
            "Horizontal asymptotes",
            "Oblique asymptotes",
            "Parity",
            "Period",
            "Monotonicity"
        ]
    );
    assert_eq!(
        items[5].display_items,
        ["The function does not have any maxima points."]
    );
    assert!(items[5].is_text);
    assert!(!items[0].is_text);
    assert_eq!(items[10].display_items, ["The function is even."]);
    assert_eq!(items[11].display_items, ["The function is not periodic."]);
    assert_eq!(items[12].grid_items.len(), 2);
    assert_eq!(items[12].grid_items[0].direction, "Decreasing");

    let r = k("sin(x^2)");
    let items = r.items();
    let last = items.last().unwrap();
    assert_eq!(
        last.display_items[0],
        "These features are too complex for Calculator to calculate:"
    );
    assert!(last.display_items[1].contains("Range"));
    assert!(last.display_items[1].contains("Monotonicity"));
    // A partial list keeps its items and says so.
    let zeros = &items[2];
    assert_eq!(zeros.title, "X-Intercept");
    assert!(!zeros.is_text);
    assert_eq!(zeros.note, "These are some of them; there may be more.");
}

#[test]
fn analysis_is_reasonably_fast() {
    let t = std::time::Instant::now();
    for s in ["x^3-3x", "tan(x)", "1/(x^2-4)", "e^(-x^2)", "sin(x)/x"] {
        let _ = analyze_str(s);
    }
    let limit = if cfg!(debug_assertions) { 20.0 } else { 2.0 };
    assert!(t.elapsed().as_secs_f64() < limit, "{:?}", t.elapsed());
}

/// e, 1/e, eᵏ and ln forms, exactly where exact arithmetic proves them (f′
/// exactly 0 at the closed form, f's value there computed exactly).
#[test]
fn e_and_ln_closed_forms() {
    let r = k("x*e^(-x)");
    assert_eq!(r.maxima, ["(1, 1/e)"]);
    assert_eq!(r.range, "y ∈ (−∞, 1/e]");
    assert_eq!(r.inflection_points, ["(2, 2/e²)"]);
    let r = k("x*ln(x)");
    assert_eq!(r.minima, ["(1/e, −1/e)"]);
    let r = k("ln(x)/x");
    assert_eq!(r.maxima, ["(e, 1/e)"]);
    let r = k("e^x-2x");
    assert_eq!(r.minima, ["(ln(2), 2 − 2ln(2))"]);
    let r = k("e^(2x)-4x");
    assert_eq!(r.minima, ["(ln(2)/2, 2 − 2ln(2))"]);
    let r = k("e^(-x^2)");
    assert_eq!(r.inflection_points, ["(−√2/2, 1/√e)", "(√2/2, 1/√e)"]);
    // Fewer digits than six fixed: as many as are, marked "≈".
    let r = k("x*2^x");
    assert!(r.minima[0].starts_with("(≈−1.44"), "{:?}", r.minima);
}
