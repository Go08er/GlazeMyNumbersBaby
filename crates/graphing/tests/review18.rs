//! Round-18 review: numbers narrowed on their way into a proof.
//!
//! The review's findings in graphing were in the independent replay
//! (`tests/replay/`, with their plants in `certify_replay.rs`): a root's
//! degree cast to u32 (R18-M-02), an exponent negated as an i64 (R18-L-01).
//! Its audit of every integer narrowing and sign conversion here, in the
//! library, found these of the same family, each reachable from a typed
//! expression:
//!
//! * a path to a sub-expression names each child by a byte, and the index
//!   of a min's or max's argument was cast to one: the 257th argument's
//!   condition was looked up under the first. `max(x + 1, 0, …, 0,
//!   x^(x − 5))` had the certified domain [0, ∞): at 0 the base x is 0,
//!   allowed where the exponent is positive, and the exponent read was the
//!   "1" of `x + 1`, not x − 5. (The replay rejected that certificate; the
//!   panel showed it.)
//! * the order of a power's expansion in t = 1/x (base order × exponent)
//!   was cast to i32: (x^999999)^999999, of order −999998000001, expanded
//!   as t^729379967, a term that tends to 0, and x + (x^999999)^999999 as
//!   the line y = x with a remainder. (The oblique row asks the tree's
//!   structure and f″'s sign first, which caught these; the expansion
//!   itself was wrong.)
//! * an exact value q·πᵏ was enclosed by one product per power of π, and k
//!   is an i32 the simplifier multiplies up: ((((π^64)^64)^64)^64)^64 has k
//!   = 2³⁰, over a minute of products for one limit (and as many tree nodes
//!   where a slope was written out).

use graphing::certify::{self, DEFAULT_BUDGET, Row, certify_text};
use graphing::compile::CompileOptions;
use graphing::equation::Equation;
use std::time::{Duration, Instant};

fn certified(src: &str) -> certify::Analysis {
    certify_text(src, CompileOptions::default(), DEFAULT_BUDGET, None).expect("analysed")
}

/// `max(first, 0, …, 0, last)`: 257 arguments, the last at index 256.
fn max_of_257(first: &str, last: &str) -> String {
    format!("max({first},{}{last})", "0,".repeat(255))
}

/// The domain row as JSON text (its pieces and their ends).
fn domain(a: &certify::Analysis) -> String {
    serde_json::to_string(&a.domain).unwrap()
}

#[test]
fn the_257th_argument_keeps_its_own_conditions() {
    // 0^(−5) is undefined: 0 is no point of the domain, whichever argument
    // the power is.
    let closed_at_0 = r#""lo":{"At":{"x":{"lo":0.0,"hi":0.0},"closed":true}}"#;
    let open_at_0 = r#""lo":{"At":{"x":{"lo":0.0,"hi":0.0},"closed":false}}"#;
    let first = certified(&max_of_257("x^(x-5)", "x+1"));
    assert!(
        first.domain.is_certified() && domain(&first).contains(open_at_0),
        "the power first: {}",
        domain(&first)
    );
    let last = certified(&max_of_257("x+1", "x^(x-5)"));
    assert!(
        !domain(&last).contains(closed_at_0),
        "the power last: {}",
        domain(&last)
    );
    // Past the children a path can name the conditions are not gathered at
    // all: the domain is not decided, and no row rests on it.
    assert!(
        matches!(&last.domain, Row::Unknown { reason } if reason.contains("more arguments")),
        "{}",
        domain(&last)
    );
    // The same for a condition the certificate would only have mislabelled
    // (ln's argument under the 257th child, written under the first).
    let ln = certified(&max_of_257("ln(x+100)", "ln(x)"));
    assert!(matches!(&ln.domain, Row::Unknown { .. }), "{}", domain(&ln));
    // 256 arguments are all named: nothing changes there.
    let named = format!("max(x+1,{}x^(x-5))", "0,".repeat(254));
    let a = certified(&named);
    assert!(
        a.domain.is_certified() && domain(&a).contains(open_at_0),
        "256 arguments: {}",
        domain(&a)
    );
    // And the panel says nothing of a domain it can't decide.
    let eq = Equation::parse(&format!("y={}", max_of_257("x+1", "x^(x-5)"))).unwrap();
    let shown = graphing::analysis::analyze(&eq, &CompileOptions::default());
    assert!(
        !format!("{:?}", shown.domain).contains("[0"),
        "{:?}",
        shown.domain
    );
}

#[test]
fn a_power_s_expansion_keeps_its_order_or_is_not_made() {
    let line = |src: &str, right: bool| {
        let eq = Equation::parse(&format!("y={src}")).unwrap();
        let (_, e) = eq.explicit().unwrap();
        let f = certify::fun::Fun::new(e, CompileOptions::default(), DEFAULT_BUDGET, None);
        certify::expand::line(&f, right, 16.0)
    };
    // A line with a remainder: found, with the slope and the intercept.
    for right in [false, true] {
        let (m, b, _) = line("x+1+1/x", right).expect("x + 1 + 1/x expands");
        assert!(m.lo() == 1.0 && m.hi() == 1.0 && b.lo() == 1.0 && b.hi() == 1.0);
        assert!(line("x+1+(x^(-999999))^3", right).is_some());
    }
    // No line: the power's order is −999998000001 (cast to i32 it was
    // +729379967, and the sum expanded as x + a remainder), or its base's
    // order times the exponent leaves the i32s altogether.
    for src in [
        "x+(x^999999)^999999",
        "x+(x^999999)^999998",
        "x+1/(x^(-999999))^999999",
        "x+((x^65536)^65536)^3",
        "x+(x^999999)^17",
        "x+x^999999*x^999999*x^999999*x^999999",
    ] {
        for right in [false, true] {
            assert!(
                line(src, right).is_none(),
                "{src} expanded as a line on the {} tail: {:?}",
                if right { "right" } else { "left" },
                line(src, right)
            );
        }
    }
    // And the row: no oblique asymptote for any of them.
    for src in ["x+(x^999999)^999999", "sqrt(x^2+1)+(x^999999)^999999"] {
        let a = certified(src);
        if let Row::Certified { value, .. } | Row::Partial { value, .. } = &a.oblique {
            assert!(value.is_empty(), "{src}: {value:?}");
        }
    }
}

#[test]
fn a_huge_power_of_pi_is_not_multiplied_out() {
    use graphing::simplify::{PiQ, Q};
    let piq = |k: i32| PiQ { q: Q::ONE, k };
    // Small powers are enclosed as before.
    let two = certify::rows::piq_interval(piq(2)).expect("π²");
    assert!(two.lo() <= 9.869604401089358 && 9.869604401089358 <= two.hi());
    assert!(two.hi() - two.lo() < 1e-13);
    let inv = certify::rows::piq_interval(piq(-3)).expect("π⁻³");
    assert!(inv.lo() <= 0.03225153443319949 && 0.03225153443319949 <= inv.hi());
    assert!(inv.hi() - inv.lo() < 1e-15);
    // Past the bound: no enclosure, at once.
    let t = Instant::now();
    for k in [1 << 30, -(1 << 30), i32::MAX, i32::MIN, 257, -257] {
        assert!(certify::rows::piq_interval(piq(k)).is_none(), "π^{k}");
    }
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    // Typed: each analysis took over a minute (2³⁰ products for the limit
    // at each tail), and with the power in a slope would have built a tree
    // of 2³⁰ nodes.
    let t = Instant::now();
    for src in [
        "((((pi^64)^64)^64)^64)^64+1/x",
        "1/((((pi^64)^64)^64)^64)^64+1/x",
        "x/((((pi^64)^64)^64)^64)^64+sin(x)",
    ] {
        let _ = certified(src);
    }
    assert!(t.elapsed() < Duration::from_secs(120), "{:?}", t.elapsed());
}

/// The follow-up's finding in the library: a row decided over one period
/// of f covers a window [s, e], and speaks for the line because e − s is a
/// whole period. e was s + P (P the upper end of the period's enclosure)
/// rounded to nearest, which can be half an ulp less: for
/// sin(2π·10¹²·x), cos(2π·10¹⁵·x) and tan(2π·10¹²·x) the window was
/// 5·10⁻¹⁷ of a period short of the enclosure's upper end. The replay
/// accepted any window; asked for one no shorter than the period it
/// refused these three. The end is now rounded up.
#[test]
fn a_window_over_one_period_is_no_shorter_than_the_period() {
    // b − a ≥ p exactly: the double difference, and the sign of what its
    // rounding left out (TwoSum).
    let at_least = |a: f64, b: f64, p: f64| {
        let w = b - a;
        let bb = w - b;
        let err = (b - (w - bb)) + (-a - bb);
        w > p || (w == p && err >= 0.0)
    };
    let mut seen = 0;
    for src in [
        "sin(x)",
        "tan(x)",
        "sin(6*pi*x)",
        "sin(2*pi*10^12*x)",
        "cos(2*pi*10^15*x)",
        "tan(2*pi*10^12*x)",
        "sin(x/7)+cos(x/3)",
    ] {
        let a = certified(src);
        let period = a.period.value().and_then(|p| p.as_ref());
        let v: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        for row in [
            "x_intercepts",
            "extrema",
            "inflections",
            "monotonicity",
            "range",
            "vertical",
        ] {
            let Some(body) = v[row].as_object().and_then(|o| o.values().next()) else {
                continue;
            };
            let w = &body["cert"]["covers"]["Period"];
            let (Some(s), Some(e)) = (w["a"].as_f64(), w["b"].as_f64()) else {
                continue;
            };
            let p = period.unwrap_or_else(|| panic!("{src}: a period window with no period"));
            assert!(
                at_least(s, e, p.hi.0),
                "{src}, {row}: the window [{s:e}, {e:e}] is shorter than the period's \
                 enclosure [{:e}, {:e}]",
                p.lo.0,
                p.hi.0
            );
            seen += 1;
        }
    }
    assert!(seen >= 12, "only {seen} rows over one period seen");
}
