//! Round-11 review: evaluation.
//!
//! R11-M-01: arccot cancelled (π/2 − arctan x), so acot(10¹⁵)·10¹⁵ was a
//! definite 0.888 everywhere, plotted as such, and 0 past 10¹⁶.
//!
//! R11-M-02: extended range kept a made-up mantissa for a power beyond 2⁵²
//! binades, so ln((e^(10¹⁵))⁴) − 4·10¹⁵ folded to a definite ½ (it is 0).
//! Powers and exponentials are now exact up to 2⁵³ binades; beyond, a value
//! is "lost": still certainly beyond the doubles, but nothing that would
//! bring it back into range (a logarithm, a root, a ratio) is known.
//!
//! R11-L-05: in degrees, the sine of an angle so small that its value in
//! radians is below the doubles was the smallest double itself, so
//! sin(x + 2⁻¹⁰⁷⁴)/(x + 2⁻¹⁰⁷⁴) was 1 at 0, not π/180.

use graphing::TrigUnit;
use graphing::analysis::{Parity, analyze_str};
use graphing::compile::{CompileOptions, Program};
use graphing::equation::Equation;

fn prog(src: &str, unit: TrigUnit) -> Program {
    let eq = Equation::parse(&format!("y={src}")).unwrap();
    let (_, ast) = eq.explicit().unwrap();
    Program::compile(
        ast,
        &CompileOptions {
            trig_unit: unit,
            ..CompileOptions::default()
        },
    )
    .unwrap()
}

fn at(src: &str, x: f64) -> f64 {
    prog(src, TrigUnit::Radians).eval(x, 0.0)
}

#[test]
fn arccot_of_a_large_argument_is_its_reciprocal() {
    for (src, x) in [
        ("acot(1000000000000000)*1000000000000000", 0.0),
        ("acot(100000000000000000)*100000000000000000", 0.0),
        ("acot(x)*x", 1e15),
        ("acot(x)*x", 1e17),
        ("acot(x)*x", 1e300),
    ] {
        let v = at(src, x);
        assert!((v - 1.0).abs() < 1e-15, "{src} at {x:e}: {v}");
    }
    // Below 0 the continuous branch (0, π): arccot(−10¹⁵) is π − 10⁻¹⁵,
    // two doubles below π's.
    assert_eq!(at("acot(x)", -1e15), 3.1415926535897922);
    // The panel and the plot: 1, not 0.888178.
    let k = analyze_str("y=acot(1000000000000000)*1000000000000000");
    assert_eq!(k.data.y_intercept, Some(1.0), "{:?}", k.y_intercept);
    assert_eq!(k.range, "y ∈ {1}", "{:?}", k.range);
}

#[test]
fn a_power_far_beyond_the_doubles_keeps_its_value_or_says_it_cannot() {
    // Within 2⁵³ binades the value is exact: ln((e^(10¹⁵))⁴) is 4·10¹⁵.
    let v = at("ln(exp(1000000000000000)^4)-4000000000000000", 0.0);
    assert_eq!(v, 0.0);
    let v = at("ln(exp(x)^4)-4*x", 1e15);
    assert_eq!(v, 0.0);
    let k = analyze_str("y=ln(exp(1000000000000000)^4)-4000000000000000");
    assert_ne!(k.data.y_intercept, Some(0.5), "{:?}", k.y_intercept);
    assert!(
        !k.range.contains("1/2") && !k.range.contains("0.5"),
        "{}",
        k.range
    );
    // Beyond 2⁵³ binades nothing in range can be known of it: no definite
    // value at all, rather than a made-up one.
    for src in [
        "ln(exp(1000000000000000)^8)-8000000000000000",
        "ln(exp(1000000000000000)^8)",
        "sqrt(sqrt(exp(1000000000000000)^16))/exp(1000000000000000)^4",
    ] {
        let v = at(src, 0.0);
        assert!(v.is_nan(), "{src}: {v}");
        let k = analyze_str(&format!("y={src}"));
        assert!(
            k.data.y_intercept.is_none() && k.data.range.is_empty(),
            "{src}: {:?} {:?}",
            k.y_intercept,
            k.range
        );
    }
    // Its size is still certain: a lost value is beyond the doubles.
    assert_eq!(at("exp(1000000000000000)^8", 0.0), f64::INFINITY);
    assert_eq!(at("1/exp(1000000000000000)^8", 0.0), 0.0);
    assert_eq!(
        at("atan(exp(1000000000000000)^8)", 0.0),
        std::f64::consts::FRAC_PI_2
    );
}

#[test]
fn a_tiny_angle_in_degrees_keeps_its_sine() {
    let deg = std::f64::consts::PI / 180.0;
    let grad = std::f64::consts::PI / 200.0;
    for (unit, k) in [(TrigUnit::Degrees, deg), (TrigUnit::Grads, grad)] {
        for src in [
            "sin(x+2^-1074)/(x+2^-1074)",
            "tan(x+2^-1074)/(x+2^-1074)",
            "sin(x+10^-310)/(x+10^-310)",
        ] {
            let v = prog(src, unit).eval(0.0, 0.0);
            assert!(
                (v - k).abs() <= 2.0 * f64::EPSILON * k,
                "{src} [{unit:?}] at 0: {v} vs {k}"
            );
        }
    }
    // Still odd, and still a pole for its reciprocal only at 0 itself.
    let p = prog("sin(x)", TrigUnit::Degrees);
    assert_eq!(p.eval(-5e-324, 0.0), -p.eval(5e-324, 0.0));
    let k = analyze_str("y=sin(x)");
    assert_eq!(k.parity, Parity::Odd);
}

/// Distance in ulps (subnormal spacing below the normals).
fn ulps(got: f64, want: f64) -> f64 {
    let u = (f64::from_bits(want.abs().to_bits() + 1) - want.abs()).max(f64::from_bits(1));
    (got - want).abs() / u
}

#[test]
fn reciprocal_hyperbolics_stay_right_where_cosh_and_sinh_overflow() {
    // csch x = 2e^−|x|/(1 − e^−2|x|): a subnormal of the right sign, not −0
    // from 1/sinh(x) = 1/−∞. Exact values from mpmath.
    for (src, x, want) in [
        ("csch(x)", -712.0, -1.211_598_928_399_783e-309),
        ("csch(x-1)", -711.0, -1.211_598_928_399_783e-309),
        ("csch(x-1000)", 288.0, -1.211_598_928_399_783e-309),
        ("sech(x)", 712.0, 1.211_598_928_399_783e-309),
    ] {
        let v = at(src, x);
        assert!(
            v != 0.0 && ulps(v, want) <= 2.0,
            "{src} at {x}: {v:e} vs {want:e}"
        );
    }
    // And in their ordinary range, to a few ulps.
    assert!(ulps(at("csch(x)", 2.0), 0.2757205647717832) <= 2.0);
    assert!(ulps(at("sech(x)", 2.0), 0.2658022288340797) <= 2.0);
}

#[test]
fn arccot_agrees_with_its_reciprocal_form() {
    for x in [2.37e8, 4.2e9, 1e15, 3.0, 0.5, 1e-300] {
        assert_eq!(at("acot(x)", x), at("atan(1/x)", x), "{x}");
    }
    assert!((at("acot(x)*x", 4.2e9) - 1.0).abs() < 1e-15);
}

#[test]
fn the_reference_keeps_what_a_huge_exponent_leaves_known() {
    use graphing::analysis::truth::{R, reval};
    let eval = |src: &str, x: f64| {
        let eq = Equation::parse(&format!("y={src}")).unwrap();
        let (_, ast) = eq.explicit().unwrap();
        reval(ast, x, TrigUnit::Radians)
    };
    // 1 to a power beyond the doubles is exactly 1 (1/x at a subnormal x).
    for x in [5e-324, -5e-324, -2e-320, 1e-310] {
        match eval("1^(1/x)", x) {
            R::V(v) => assert_eq!(v.f(), 1.0, "{x}"),
            r => panic!("1^(1/x) at {x:e}: {r:?}"),
        }
        assert_eq!(at("1^(1/x)", x), 1.0, "{x}");
    }
    // ½ to it is a positive value below every double, not unknown.
    match eval("0.5^(1/x)", 5e-324) {
        R::V(v) => assert!(v.sign() > 0.0 && v.f() == 0.0),
        r => panic!("{r:?}"),
    }
    // A rational power of a quotient below the normals, to the last bit.
    for (src, want) in [
        ("(x/0.001)^(1/3)", 1.7031839360032603e-107),
        ("(x/0.001)^(2/3)", 2.900835519859558e-214),
    ] {
        match eval(src, 5e-324) {
            R::V(v) => assert!(ulps(v.f(), want) <= 1.0, "{src}: {:e}", v.f()),
            r => panic!("{src}: {r:?}"),
        }
        assert!(ulps(at(src, 5e-324), want) <= 1.0, "{src} compiled");
    }
}

#[test]
fn reciprocals_of_an_underflowing_sine_overflow_rather_than_vanish() {
    // In degrees the sine of 5·10⁻³²⁴ is 8.6·10⁻³²⁶, below every double:
    // it rounds to 0, but it is no zero of the sine, so its reciprocal is
    // ∞, and only a whole multiple of a half turn is a pole.
    let deg = |src: &str, x: f64| prog(src, TrigUnit::Degrees).eval(x, 0.0);
    assert_eq!(deg("sin(x)", 5e-324), 0.0);
    assert_eq!(deg("csc(x)", 5e-324), f64::INFINITY);
    assert_eq!(deg("csc(x)", -5e-324), f64::NEG_INFINITY);
    assert_eq!(deg("cot(x)", 5e-324), f64::INFINITY);
    assert_eq!(deg("1/sin(x)", 5e-324), f64::INFINITY);
    assert!(deg("csc(x)", 0.0).is_nan());
    assert!(deg("csc(x)", 180.0).is_nan());
    assert!(deg("tan(x)", 90.0).is_nan());
    assert!(deg("sec(x)", 90.0).is_nan());
    // Just off 90° the cosine is small but no zero: tan is finite.
    assert!(deg("tan(x)", 90.00000000000001).is_finite());
}
