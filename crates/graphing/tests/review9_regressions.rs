//! Follow-up to round 8: the remaining false-claim classes the adversarial
//! sweep found, as shown in the analysis panel.

use graphing::Graph;
use graphing::strings as s;

/// The panel's text for each feature title.
fn panel(expr: &str) -> std::collections::HashMap<String, String> {
    let mut g = Graph::new();
    let id = g.add_equation(expr);
    g.analyze(id)
        .items()
        .into_iter()
        .map(|i| {
            let mut text = i.display_items.join(" | ");
            for row in &i.grid_items {
                text.push_str(&format!(" | {} {}", row.expression, row.direction));
            }
            (i.title, text)
        })
        .collect()
}

#[test]
fn a_large_offset_keeps_tans_period_and_range() {
    let p = panel("y=tan(x)+1000000");
    assert_eq!(p[s::PERIODICITY], "π");
    // ℝ, or (where the certifier can't join the branches) unknown.
    assert!(p[s::RANGE] == "y ∈ ℝ" || p[s::RANGE] == s::KGF_RANGE_NONE);
    assert_eq!(p[s::DOMAIN], "x ∈ ℝ \\ {π/2 + kπ | k ∈ ℤ}");
}

#[test]
fn a_flat_minimum_is_found_exactly() {
    let p = panel("y=x^4-4x^3+6x^2-4x+1");
    assert_eq!(p[s::X_INTERCEPT], "1");
    assert_eq!(p[s::MINIMA], "(1, 0)");
    assert_eq!(p[s::RANGE], "y ∈ [0, ∞)");
}

#[test]
fn an_unrepresentable_zero_is_unknown_not_absent() {
    let p = panel("y=ln(x)+1000000");
    assert_eq!(p[s::RANGE], "y ∈ ℝ");
    assert_eq!(p[s::X_INTERCEPT], s::KGF_X_INTERCEPT_UNKNOWN);
}

#[test]
fn a_slow_power_tail_keeps_its_offset() {
    // The limit 10⁶ is not borne out by f's enclosures far out (x^−0.0001
    // is 0.93 at 10³⁰⁰): unknown, never another value.
    let p = panel("y=x^-0.0001+1000000");
    assert!(
        p[s::RANGE] == "y ∈ (1000000, ∞)" || p[s::RANGE] == s::KGF_RANGE_NONE,
        "{}",
        p[s::RANGE]
    );
    assert!(
        p[s::HORIZONTAL_ASYMPTOTES] == "y = 1000000"
            || p[s::HORIZONTAL_ASYMPTOTES] == s::KGF_HORIZONTAL_ASYMPTOTES_UNKNOWN,
        "{}",
        p[s::HORIZONTAL_ASYMPTOTES]
    );
}

#[test]
fn overflow_is_unbounded_not_a_closed_bound() {
    let p = panel("y=1/e^(1/x)");
    assert_eq!(p[s::DOMAIN], "x ∈ ℝ \\ {0}");
    // Unknown where the certifier doesn't prove the pole side's limit;
    // never a closed bound.
    assert!(
        p[s::RANGE] == "y ∈ (0, 1) ∪ (1, ∞)" || p[s::RANGE] == s::KGF_RANGE_NONE,
        "{}",
        p[s::RANGE]
    );
}

#[test]
fn a_zero_beside_a_pole_is_shown_apart_from_it() {
    let p = panel("y=1/(x-2)+1000000");
    assert_eq!(p[s::X_INTERCEPT], "1.999999");
    assert_eq!(p[s::DOMAIN], "x ∈ ℝ \\ {2}");
}

#[test]
fn zero_to_a_negative_power_has_no_values_there() {
    // 0⁰ (at x = 0) is undefined too, as on the TI-84 Plus CE
    // (docs/ti-conventions.md): 0 to a positive power only.
    let p = panel("y=0^(-x)");
    assert_eq!(p[s::DOMAIN], "x ∈ (−∞, 0)");
    assert!(p[s::RANGE] == "y ∈ {0}" || p[s::RANGE] == s::KGF_RANGE_NONE);
}
