//! Round-14 review: typed integers past the doubles, and limits.
//!
//! R14-M-02: a constant exponent exactly an odd integer past 2⁵³ (a typed
//! 9007199254740993) was read as its double, the even 2⁵³: x^n was +1 at
//! −1. Its parity is now the exact value's at any size, and where that
//! isn't carried (an exponent too long to fold) a negative base's power
//! is unknown, unless the parity is known otherwise.
//!
//! R14-M-04: a root degree box far below −10³⁰⁰ was taken to hold no odd
//! integer (its lower end was clipped to −10³⁰⁰), so root(−8, n) over it
//! was empty, and a degree typed as the odd −(10³⁰¹ + 1) left the panel
//! with no y-intercept.

use graphing::Graph;
use graphing::TrigUnit;
use graphing::analysis::truth::{R, reval_typed};
use graphing::compile::{CompileOptions, Input, compile_str};
use graphing::equation::Equation;
use graphing::interval::{Ctx, Dec, Interval, taylor};
use graphing::lexer::ParseOptions;

/// The value the app computes for `y=src` at x under the digit limit
/// `digits`, and its panel.
fn app(src: &str, x: f64, digits: Option<u8>) -> (f64, graphing::analysis::KeyGraphFeatures) {
    let mut g = Graph::new();
    g.set_literal_digits(digits);
    let id = g.add_equation(&format!("y={src}"));
    (g.evaluate(id, x).unwrap_or(f64::NAN), g.analyze(id))
}

/// The value the app computes for `y=src` at x under `digits`.
fn value(src: &str, x: f64, digits: Option<u8>) -> f64 {
    let mut g = Graph::new();
    g.set_literal_digits(digits);
    let id = g.add_equation(&format!("y={src}"));
    g.evaluate(id, x).unwrap_or(f64::NAN)
}

/// The typed reference for `y=src` at x under `digits`.
fn reference(src: &str, x: f64, digits: Option<u8>) -> R {
    let opts = ParseOptions {
        literal_digits: digits,
        ..Default::default()
    };
    let eq = Equation::parse_with(&format!("y={src}"), opts).unwrap();
    reval_typed(eq.explicit().unwrap().1, x, TrigUnit::Radians)
}

/// Precisions at which a typed integer with more than 16 digits is as
/// typed (`None`, off) or rounded (the rest).
const PRECISIONS: [Option<u8>; 4] = [None, Some(16), Some(20), Some(14)];

#[test]
fn an_odd_exponent_past_the_doubles_is_odd() {
    // Off, 16 and 20 digits: the exponent is the odd 2⁵³ + 1; to 14 the
    // literal is the even 9007199254741000 (and the sum the odd
    // 9007199254741000 + 1).
    for digits in PRECISIONS {
        for (src, odd_at_14) in [
            ("x^9007199254740993", false),
            ("x^(9007199254740992+1)", true),
        ] {
            let odd = digits != Some(14) || odd_at_14;
            let sign = if odd { -1.0 } else { 1.0 };
            assert_eq!(value(src, -1.0, digits), sign, "{src} at −1 ({digits:?})");
            assert!(
                matches!(reference(src, -1.0, digits), R::V(v) if v.f() == sign),
                "{src} ({digits:?}): {:?}",
                reference(src, -1.0, digits)
            );
            assert_eq!(value(src, 1.0, digits), 1.0);
            assert_eq!(value(src, 0.0, digits), 0.0);
            // Beyond the doubles: the odd power is −∞ below −1, −0 above.
            assert_eq!(value(src, -1.0000001, digits), sign * f64::INFINITY);
            assert_eq!(value(src, -0.5, digits), 0.0);
        }
        let sign = if digits == Some(14) { 1.0 } else { -1.0 };
        assert_eq!(value("x^(-9007199254740993)", -1.0, digits), sign);
        // A constant base, folded or not.
        assert_eq!(value("(-1)^9007199254740993", 0.0, digits), sign);
        assert_eq!(
            value("(-1.0000001)^9007199254740993", 0.0, digits),
            sign * f64::INFINITY
        );
    }
    // Even as typed: +1.
    assert_eq!(value("x^9007199254740992", -1.0, None), 1.0);
    assert_eq!(value("x^18014398509481986", -1.0, None), 1.0);
    // The batch evaluator agrees with the scalar.
    let p = compile_str("x^9007199254740993+x^(3^20000)", TrigUnit::Radians).unwrap();
    let xs: Vec<f64> = (0..400).map(|i| -2.0 + i as f64 * 0.01).collect();
    let mut out = vec![0.0; xs.len()];
    p.eval_batch(Input::Slice(&xs), Input::Scalar(0.0), &mut out);
    for (x, o) in xs.iter().zip(&out) {
        let s = p.eval(*x, 0.0);
        assert!((s.is_nan() && o.is_nan()) || s == *o, "{x}: {s} vs {o}");
    }
    assert_eq!(p.eval(-1.0, 0.0), -2.0);
}

/// An exponent too long to fold, beyond the doubles: its parity, where
/// known, decides a negative base's power (3^20000 is odd, 10^5000 and
/// 171! even, 1.5^100000 no integer); where it isn't (C(2000, 1000)), the
/// power is unknown, never the even one its extended-range value makes.
#[test]
fn an_exponent_too_long_to_fold_keeps_its_parity_or_abstains() {
    for (src, want) in [
        ("x^(3^20000)", Some(-1.0)),
        ("x^(-(3^20000))", Some(-1.0)),
        ("x^(10^5000)", Some(1.0)),
        ("x^(171!)", Some(1.0)),
        ("x^(1.5^100000)", None),
        ("x^(nCr(2000,1000))", None),
    ] {
        let v = value(src, -1.0, None);
        match want {
            Some(w) => assert_eq!(v, w, "{src}"),
            None => assert!(v.is_nan(), "{src}: {v}"),
        }
        assert_eq!(value(src, 1.0, None), 1.0, "{src} at 1");
    }
    // The reference: −1 for the odd one; no real power for no integer;
    // unknown where the parity isn't known (not undefined).
    assert!(matches!(reference("x^(3^20000)", -1.0, None), R::V(v) if v.f() == -1.0));
    assert!(matches!(reference("x^(1.5^100000)", -1.0, None), R::Undef));
    assert!(matches!(
        reference("x^(nCr(2000,1000))", -1.0, None),
        R::Unknown
    ));
}

#[test]
fn far_negative_root_degrees_hold_odd_integers() {
    let ctx = Ctx::new(CompileOptions::default());
    let eq = Equation::parse("y=root(-8,x)").unwrap();
    let (_, f) = eq.explicit().unwrap();
    for (lo, hi) in [
        (-1e302, -1e301),
        (-1e308, -1e307),
        (f64::NEG_INFINITY, -1e300),
        (-1e300, -1e299),
        (1e301, 1e302),
    ] {
        let s = taylor(f, Interval::new(lo, hi), 0, &ctx)[0];
        assert!(
            !s.is_empty() && s.lo() <= -0.9999999 && s.hi() >= -1.0000001 && s.dec <= Dec::Trv,
            "root(-8, x) on [{lo:e}, {hi:e}]: {s:?}"
        );
    }
    // The degree typed as one odd integer of 302 digits.
    let odd = format!("root(-8,-1{}1)", "0".repeat(300));
    for digits in PRECISIONS {
        let (v, k) = app(&odd, 0.0, digits);
        if digits.is_none() {
            // −8^(1/n) = −e^(ln 8/n), just above −1 (its double is −1).
            assert_eq!(v, -1.0);
            assert_eq!(k.y_intercept, "≈\u{2212}1");
            assert!(k.data.y_intercept.is_some_and(|y| (y + 1.0).abs() < 1e-9));
        } else {
            // Rounded to 16, 20 or 14 digits it is the even −10³⁰¹.
            assert!(v.is_nan(), "{digits:?}: {v}");
            assert!(
                k.data.y_intercept.is_none(),
                "{digits:?}: {}",
                k.y_intercept
            );
        }
    }
}
