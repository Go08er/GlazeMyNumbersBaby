//! Round-14 review: typed integers past the doubles, and limits.
//!
//! R14-M-04: a root degree box far below −10³⁰⁰ was taken to hold no odd
//! integer (its lower end was clipped to −10³⁰⁰), so root(−8, n) over it
//! was empty, and a degree typed as the odd −(10³⁰¹ + 1) left the panel
//! with no y-intercept.

use graphing::Graph;
use graphing::compile::CompileOptions;
use graphing::equation::Equation;
use graphing::interval::{Ctx, Dec, Interval, taylor};

/// The value the app computes for `y=src` at x under the digit limit
/// `digits`, and its panel.
fn app(src: &str, x: f64, digits: Option<u8>) -> (f64, graphing::analysis::KeyGraphFeatures) {
    let mut g = Graph::new();
    g.set_literal_digits(digits);
    let id = g.add_equation(&format!("y={src}"));
    (g.evaluate(id, x).unwrap_or(f64::NAN), g.analyze(id))
}

/// Precisions at which a typed integer with more than 16 digits is as
/// typed (`None`, off) or rounded (the rest).
const PRECISIONS: [Option<u8>; 4] = [None, Some(16), Some(20), Some(14)];

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
