//! Round-13 review: typed numbers.
//!
//! R13-L-05: arithmetic on literals alone is done exactly and rounded once,
//! but an intermediate too long to carry (2¹⁴ bits) made the exact fold
//! give up, and the program then rounded step by step:
//! (10⁵⁰⁰⁰ + 1) − 10⁵⁰⁰⁰ was 0, not 1. Now such arithmetic isn't done: the
//! value is unknown (NaN, and the reference says unknown, not undefined),
//! so nothing is drawn or traced there. A value too long to carry that is
//! proven beyond the doubles alone (10⁵⁰⁰⁰, 171!) still rounds to ±∞ or 0.

use graphing::TrigUnit;
use graphing::analysis::analyze_str;
use graphing::analysis::truth::{R, reval_typed};
use graphing::compile::{CompileOptions, compile_str};
use graphing::equation::Equation;
use graphing::interval::Literals;
use graphing::plot::{PlotOptions, plot};
use graphing::{Viewport, lexer::ParseOptions};

fn at(src: &str, x: f64) -> f64 {
    compile_str(src, TrigUnit::Radians).unwrap().eval_x(x)
}

/// The reference for the program as the app compiles it.
fn reference(src: &str, x: f64) -> R {
    let text = format!("y={src}");
    let eq = Equation::parse(&text).unwrap();
    let lits = Literals::of(&text, ParseOptions::default()).unwrap();
    reval_typed(eq.explicit().unwrap().1, x, TrigUnit::Radians, &lits)
}

/// The heights of the curve's vertices in [−10, 10]².
fn curve_ys(src: &str) -> Vec<f64> {
    let eq = Equation::parse(src)
        .unwrap()
        .compile(&CompileOptions::default())
        .unwrap();
    let v = Viewport::new(-10.0, 10.0, -10.0, 10.0, 400.0, 400.0);
    let p = plot(&eq, &v, &PlotOptions::default());
    p.curves.iter().flatten().map(|p| p.y).collect()
}

#[test]
fn arithmetic_too_long_to_fold_is_unknown_not_rounded() {
    let beyond = [
        "(10^5000+1)-10^5000",
        "(10^5000+1)-10^5000+x",
        "2^20000/2^19999",
        "(171!+1)-171!",
        "10^5000*(10^-5000)",
    ];
    for src in beyond.iter().chain(&["(1+1/2^20)^800"]) {
        for x in [-1.0, 0.0, 2.5] {
            let v = at(src, x);
            assert!(v.is_nan(), "{src} at {x}: {v}");
            assert!(matches!(reference(src, x), R::Unknown), "{src} at {x}");
        }
    }
    // Not drawn where nothing places the curve (its enclosure is as wide
    // as the doubles)...
    for src in beyond {
        assert!(curve_ys(&format!("y={src}")).is_empty(), "{src}");
    }
    // ...and drawn only where f's enclosure places it, within a quarter
    // pixel (the plotter's rule for an unknown scalar, `plot::drawn_value`).
    let exact = (800.0 * (2f64.powi(-20)).ln_1p()).exp();
    let ys = curve_ys("y=(1+1/2^20)^800");
    assert!(!ys.is_empty());
    assert!(
        ys.iter().all(|y| (y - exact).abs() < 20.0 / 400.0 / 4.0),
        "{ys:?}"
    );
    // The panel still says what it proves exactly.
    let k = analyze_str("y=(10^5000+1)-10^5000");
    assert_eq!(k.data.y_intercept, Some(1.0), "{:?}", k.y_intercept);
}

/// Review 13's three functions, analysed with the digit limit `digits`
/// (the slider `a` at 1).
fn panel(src: &str, digits: Option<u8>) -> graphing::analysis::KeyGraphFeatures {
    let mut g = graphing::Graph::new();
    g.set_literal_digits(digits);
    let id = g.add_equation(src);
    g.analyze(id)
}

fn is_singleton(k: &graphing::analysis::KeyGraphFeatures) -> bool {
    k.range.contains('{')
}

/// R13-M-01: two decimals that share a double are two numbers, each
/// occurrence its own (`ast::Lit`), with the digit limit off: at the
/// scalar, the panel and the certificate. `10^16*(1.0000000000000001-1)+x`
/// is x + 1; 1.0000000000000001·x − 1·x is 10⁻¹⁶·x; beside a slider a at
/// 1, a·x − 1.0000000000000001·x is −10⁻¹⁶·x. None is constant, none is 0
/// everywhere, and the first isn't odd.
#[test]
fn literals_sharing_a_double_are_each_their_own_number() {
    use graphing::analysis::{Monotonicity, Parity};
    // The scalar the app plots, and its reference: x + 1.
    let first = "10^16*(1.0000000000000001-1)+x";
    assert_eq!(at(first, 0.0), 1.0);
    assert_eq!(at(first, -1.0), 0.0);
    assert!(matches!(reference(first, 0.0), R::V(v) if v.f() == 1.0));
    let k = panel(&format!("y={first}"), None);
    assert_eq!(k.data.y_intercept, Some(1.0), "{}", k.y_intercept);
    assert!(
        k.data
            .zeros
            .iter()
            .all(|z| z.x == -1.0 && z.period.is_none()),
        "{}",
        k.x_intercept
    );
    assert!(!matches!(
        k.parity,
        Parity::Odd | Parity::Even | Parity::Both
    ));
    for src in ["y=1.0000000000000001*x-1*x", "y=a*x-1.0000000000000001*x"] {
        let k = panel(src, None);
        assert!(!is_singleton(&k), "{src}: range {}", k.range);
        assert!(
            !matches!(k.parity, Parity::Even | Parity::Both),
            "{src}: {:?}",
            k.parity
        );
        assert!(
            k.monotonicity
                .iter()
                .all(|(_, m)| *m != Monotonicity::Constant),
            "{src}: {:?}",
            k.monotonicity
        );
        assert!(!k.x_intercept.contains('ℝ'), "{src}: {}", k.x_intercept);
        assert!(k.data.zeros.iter().all(|z| z.x == 0.0), "{src}");
    }
    // The certificate: no row says otherwise.
    let a = graphing::certify::certify_text(
        "1.0000000000000001*x-1*x",
        CompileOptions::default(),
        graphing::certify::DEFAULT_BUDGET,
        None,
    )
    .unwrap();
    assert!(
        !matches!(
            a.parity,
            graphing::certify::Row::Certified {
                value: graphing::certify::Parity::Even,
                ..
            }
        ),
        "{:?}",
        a.parity
    );
    // To 14 digits (the apps' default) each decimal is 1: x, 0 and 0.
    let k = panel(&format!("y={first}"), Some(14));
    assert_eq!(k.data.y_intercept, Some(0.0));
    assert_eq!(k.parity, Parity::Odd);
    for src in ["y=1.0000000000000001*x-1*x", "y=a*x-1.0000000000000001*x"] {
        let k = panel(src, Some(14));
        assert_eq!(k.range, "y ∈ {0}", "{src}");
        assert_eq!(k.parity, Parity::Both, "{src}");
    }
}

/// The digit limit on typed numbers (`ParseOptions::literal_digits`): the
/// TI-84 Plus CE's 14 rounds `1.0000000000000001` to 1 on entry, for the
/// curve and the analysis alike, and a slider the same way.
#[test]
fn a_digit_limit_rounds_typed_numbers_and_sliders_on_entry() {
    use graphing::Graph;
    use graphing::lexer::TI84_DIGITS;
    let mut g = Graph::new();
    assert_eq!(g.literal_digits(), None);
    let id = g.add_equation("y=10^16*(1.0000000000000001-0.99999999999999999)+x");
    let b = g.add_equation("y=a*x-1.0000000000000001*x");
    g.set_variable("a", 0.1 + 0.2);
    g.set_literal_digits(Some(TI84_DIGITS));
    assert_eq!(g.literal_digits(), Some(14));
    // Both decimals are 1 to 14 digits: f is x.
    assert_eq!(g.evaluate(id, 0.0), Some(0.0));
    assert_eq!(g.evaluate(id, -1.0), Some(-1.0));
    let k = g.analyze(id);
    assert_eq!(k.data.y_intercept, Some(0.0), "{:?}", k.y_intercept);
    // The slider is 0.30000000000000004 rounded to 14 digits: 0.3, and
    // the literal 1; so f is (0.3 − 1)·x.
    assert_eq!(g.evaluate(b, 10.0), Some((0.3 - 1.0) * 10.0));
    // The stored value is the slider's own; only its reading is rounded.
    assert_eq!(g.variable("a").map(|v| v.value()), Some(0.1 + 0.2));
    // Clamped to 5..=20.
    g.set_literal_digits(Some(2));
    assert_eq!(g.literal_digits(), Some(5));
    g.set_literal_digits(Some(99));
    assert_eq!(g.literal_digits(), Some(20));
    // A certificate records the limit, for the replay to read alike.
    let parse = ParseOptions {
        literal_digits: Some(TI84_DIGITS),
        ..Default::default()
    };
    let a = graphing::certify::certify_text_with(
        "1.0000000000000001*x-1*x",
        parse,
        CompileOptions::default(),
        graphing::certify::DEFAULT_BUDGET,
        None,
    )
    .unwrap();
    assert_eq!(a.binding.as_ref().and_then(|b| b.literal_digits), Some(14));
}

/// R13-M-04 at the point evaluator (deferred by the interval work): a
/// root degree typed as the odd 9007199254740993 is odd, though both
/// doubles about it are even. The program and its reference give the real
/// root sign(a)·|a|^(1/n), as the curve and the panel do; typed even, or
/// to 14 digits (where it rounds to 9007199254741000), a negative base
/// has none.
#[test]
fn a_typed_odd_root_degree_past_the_doubles_is_odd() {
    let want = -(8f64.ln() / 9007199254740993.0).exp();
    for (src, x, y) in [
        ("root(-8,9007199254740993)", 0.0, want),
        ("root(x,9007199254740993)", -8.0, want),
        ("root(x,9007199254740993)", 8.0, -want),
        ("root(x,9007199254740993)", 0.0, 0.0),
        ("root(x,-9007199254740993)", -8.0, 1.0 / want),
    ] {
        let v = at(src, x);
        assert!((v - y).abs() <= 2.0 * f64::EPSILON, "{src} at {x}: {v}");
        assert!(
            matches!(reference(src, x), R::V(r) if (r.f() - y).abs() <= 2.0 * f64::EPSILON),
            "{src} at {x}"
        );
    }
    assert!(at("root(x,-9007199254740993)", 0.0).is_nan());
    assert!(at("root(x,18014398509481986)", -8.0).is_nan());
    let mut g = graphing::Graph::new();
    g.set_literal_digits(Some(14));
    let id = g.add_equation("y=root(x,9007199254740993)");
    assert!(g.evaluate(id, -8.0).is_some_and(f64::is_nan));
}

#[test]
fn a_value_beyond_the_doubles_alone_still_rounds_once() {
    // 10⁵⁰⁰⁰ and 171! are past the doubles however they are rounded: +∞,
    // as before; and functions of them are evaluated as before.
    assert_eq!(at("10^5000", 0.0), f64::INFINITY);
    assert_eq!(at("-10^5000", 0.0), f64::NEG_INFINITY);
    assert_eq!(at("171!", 0.0), f64::INFINITY);
    assert_eq!(at("(1/2)^100000", 0.0), 0.0);
    assert_eq!(at("10^5000*x", 2.0), f64::INFINITY);
    let ln = at("ln(10^5000)", 0.0);
    assert!((ln - 5000.0 * std::f64::consts::LN_10).abs() < 1e-9, "{ln}");
    // Within the cap nothing changed: exact, then rounded once.
    assert_eq!(at("(10^300+1)-10^300", 0.0), 1.0);
    assert_eq!(at("(-1)^100001+(10^300+1)-10^300", 0.0), 0.0);
    assert_eq!(at("1^100000+x", 2.0), 3.0);
}
