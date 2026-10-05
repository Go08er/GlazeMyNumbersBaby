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
