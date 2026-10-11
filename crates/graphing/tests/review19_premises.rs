//! Round 19: premises the certifier and the panel took without proof,
//! found by reading for them after R19-M-01 (a sampled value decided a
//! row). Each is now proven or gives no verdict.

use graphing::analysis::{analyze_str, flags};
use graphing::certify::{DEFAULT_BUDGET, Row, Tail, certify_text};
use graphing::compile::CompileOptions;

/// 3 + (x − c)·e⁻ˣ with c = 1 + 2·10⁻¹⁵ rises to its maximum at c + 1 and
/// falls to its limit 3 from above. f′ < 0 is proven from 16 on; its value
/// at 1, 3 − 7·10⁻¹⁶, was taken as an upper bound of the limit as if f
/// fell from 1 on, and the right asymptote was certified at
/// y ∈ [2.9999999999999996, 2.9999999999999996]. (`+(x-x)` keeps the
/// simplifier from giving the limit itself.)
#[test]
fn a_tail_falls_only_from_where_it_is_proven_to() {
    for src in [
        "3+(x-(1+2*10^(-15)))*e^(-x)+(x-x)",
        "3+(x-(1+3*10^(-15)))*e^(-x)+(x-x)",
        "-3-(x-(1+2*10^(-15)))*e^(-x)+(x-x)",
        "3-(x+(1+2*10^(-15)))*e^(x)+(x-x)",
    ] {
        let a = certify_text(src, CompileOptions::default(), DEFAULT_BUDGET, None)
            .unwrap_or_else(|e| panic!("{src}: {e}"));
        let limit = if src.starts_with('-') { -3.0 } else { 3.0 };
        let side = if src.contains("e^(x)") {
            Tail::Left
        } else {
            Tail::Right
        };
        if let Row::Certified { value, .. } | Row::Partial { value, .. } = &a.horizontal {
            for h in value.iter().filter(|h| h.side == side) {
                assert!(
                    h.y.contains(limit),
                    "{src}: the limit {limit} is not in {:?}",
                    h.y
                );
            }
        }
    }
}

/// tan(x + 10⁻³⁰) + tan(x + 2·10⁻³⁰) is undefined on two families of
/// points 10⁻³⁰ apart, which one enclosure holds: its domain read
/// ℝ \ {≈1.5708 + kπ}, the two taken for one because their enclosures
/// were equal. A family the function excludes twice over by the same
/// expression (tan x + sec x) is one, and is listed once.
#[test]
fn two_families_in_one_enclosure_are_not_one() {
    let k = analyze_str("y=tan(x+10^(-30))+tan(x+2*10^(-30))");
    assert!(
        k.too_complex_features & flags::DOMAIN != 0 || k.domain.matches("kπ").count() >= 2,
        "{}",
        k.domain
    );
    for src in [
        "y=tan(x)+sec(x)",
        "y=tan(x+sqrt(2))+1/cos(x+sqrt(2))",
        "y=tan(x+sqrt(2))+tan(x+sqrt(2))^2",
    ] {
        let k = analyze_str(src);
        assert_eq!(
            k.too_complex_features & flags::DOMAIN,
            0,
            "{src}: one family, known as one"
        );
        assert_eq!(k.domain.matches("kπ").count(), 1, "{src}: {}", k.domain);
    }
}

/// y = ln(1.00000000000000000001), its numbers as typed, is the constant
/// 10⁻²⁰. The panel wrote an exact constant whose form is unwieldy from
/// "its enclosure", which was the form's double with four doubles either
/// side: ln of the double beside 1, and the range read
/// {≈2.22045×10⁻¹⁶}. The enclosure is now one of the form itself.
#[test]
fn an_exact_constant_is_shown_by_an_enclosure_of_it() {
    for (src, wrong) in [
        ("y=ln(1.00000000000000000001)", "2.22045"),
        ("y=10^20*ln(1.00000000000000000001)", "22204"),
        ("y=ln(0.99999999999999999999)", "1.11022"),
        ("y=3+ln(1.00000000000000000001)-3", "2.22045"),
    ] {
        let k = analyze_str(src);
        assert!(!k.range.contains(wrong), "{src}: {}", k.range);
        if k.too_complex_features & flags::RANGE == 0 {
            assert!(
                k.range.contains('1') && k.range.starts_with("y ∈ {"),
                "{src}: {}",
                k.range
            );
        }
    }
    let k = analyze_str("y=ln(1.00000000000000000001)");
    assert_eq!(k.range, "y ∈ {≈1×10⁻²⁰}");
    let k = analyze_str("y=10^20*ln(1.00000000000000000001)");
    assert_eq!(k.range, "y ∈ {≈1}");
}
