//! What the random cross-checking tester (`tests/crossfuzz.rs`) found
//! and was fixed: each case as the tester minimised it, checked directly.

use graphing::Equation;
use graphing::analysis::truth::{R, reval_typed};
use graphing::compile::{CompileOptions, Program};
use graphing::functions::{TrigUnit, ncr, npr};
use graphing::interval::{Ctx, Dec, DecInterval, Interval, enclose};

fn ast(src: &str) -> graphing::ast::Expr {
    let eq = Equation::parse(&format!("y={src}")).unwrap_or_else(|e| panic!("{src}: {e:?}"));
    eq.explicit().expect("a function of x").1.clone()
}

fn scalar(src: &str, x: f64) -> f64 {
    Program::compile_typed(&ast(src), &CompileOptions::default())
        .expect("compiles")
        .eval(x, 0.0)
}

fn reference(src: &str, x: f64) -> R {
    reval_typed(&ast(src), x, TrigUnit::Radians)
}

fn enclosure(src: &str, lo: f64, hi: f64) -> DecInterval {
    enclose(
        &ast(src),
        Interval::new(lo, hi),
        &Ctx::new(CompileOptions::default()),
    )
}

/// The reference evaluator called a power undefined where y·ln|a| leaves
/// the doubles: the product overflowed to a NaN, read as "undefined". The
/// compiled program had it right both times.
#[test]
fn a_power_whose_logarithm_overflows_is_not_undefined_to_the_reference() {
    // 0.2913^(143^143) is 10^(−10³⁰⁷): far below the doubles, and positive.
    let f = "tanh(0.3)^(x^x)+(x+x)";
    assert_eq!(scalar(f, 143.0), 286.0);
    match reference(f, 143.0) {
        R::V(v) => assert_eq!(v.f(), 286.0),
        r => panic!("{f} at 143: {r:?}"),
    }
    match reference("tanh(0.3)^(x^x)", 143.0) {
        R::V(v) => assert!(v.sign() > 0.0 && v.f() == 0.0, "{v:?}"),
        r => panic!("{r:?}"),
    }
    // (−6.8·10¹⁰)^(170!): 170! is an even integer, so the power is beyond
    // the largest double: +∞ compiled, and no value the reference can
    // hold — but not undefined.
    let g = "x^(170!)+cos(x)";
    assert_eq!(scalar(g, -68129206905.79622), f64::INFINITY);
    assert!(
        matches!(reference(g, -68129206905.79622), R::Unknown),
        "{:?}",
        reference(g, -68129206905.79622)
    );
    // A base below 1 to that power is below every double; a non-integer
    // exponent of a negative base stays undefined.
    assert!(matches!(reference("0.5^(170!)", 0.0), R::V(v) if v.sign() > 0.0 && v.f() == 0.0));
    assert!(matches!(
        reference("x^(10^308+0.5)", -2.0),
        R::Undef | R::Unknown
    ));
}

/// The reference evaluator, as its powers: a root whose ln|x|/n leaves the
/// doubles is beyond them, not undefined.
#[test]
fn a_root_of_a_degree_near_zero_is_not_undefined_to_the_reference() {
    let f = "root(2*x,2*x)";
    let x = f64::MIN_POSITIVE;
    assert_eq!(scalar(f, x), 0.0);
    assert!(
        matches!(reference(f, x), R::V(v) if v.sign() > 0.0 && v.f() == 0.0),
        "{:?}",
        reference(f, x)
    );
}

/// The reference evaluator: a root of a degree beyond the doubles is 1
/// for a positive x a double holds (1/n is below every double), not
/// undefined. root(u, 6 + xˣ) from x = 144 on, where xˣ overflows.
#[test]
fn a_root_of_a_degree_beyond_the_doubles_is_not_undefined_to_the_reference() {
    let f = "root(x^(3/2),6+x^x)+(9+x)";
    match reference(f, 170.0) {
        R::V(v) => assert_eq!(v.f(), 180.0),
        r => panic!("{f} at 170: {r:?}"),
    }
    let e = enclosure(f, 170.0, 170.0);
    assert!(e.lo() <= 180.0 && 180.0 <= e.hi() && e.dec >= Dec::Def);
    // A negative x there: the degree's parity isn't known.
    assert!(matches!(
        reference("root(x,6+(-x)^(-x))", -170.0),
        R::Unknown
    ));
}

/// The extended-range power kept a mantissa of noise where t·log₂|b| is
/// past 2⁵³ through b's mantissa alone (a base in [1, 2), whose binary
/// exponent is 0): 1.5^x + 1 was ∞ at −10³⁰⁰ and 1 at 10³⁰⁰.
#[test]
fn a_power_far_beyond_the_doubles_keeps_its_side() {
    for (f, x, want) in [
        ("1.5^x+1", -1e300, 1.0),
        ("1.5^x+1", 1e300, f64::INFINITY),
        ("1.5^x+1", -1e17, 1.0),
        ("1.5^x+1", 1e17, f64::INFINITY),
        ("1.75^x+1", -1e300, 1.0),
        ("1.25^(-x)+1", -1e300, f64::INFINITY),
        ("1/(1.5^x)+1", 1e300, 1.0),
        ("0.75^x+1", 1e300, 1.0),
        ("0.75^x+1", -1e300, f64::INFINITY),
        ("2^x+1", -1e300, 1.0),
        ("1.5^x+1", -1e20, 1.0),
    ] {
        assert_eq!(scalar(f, x), want, "{f} at {x:e}");
    }
    assert_eq!(scalar("1.5^x", -1e300), 0.0);
}

/// nCr(n, k) and nPr(n, k) of a whole 0 ≤ k ≤ 60 are polynomials in n.
/// Γ's quotient lost them in doubles: undefined beside a whole n, wrong
/// in the twelfth digit at −172.5, 2.15 times too large at 5.6·10¹⁴.
#[test]
fn counts_of_a_small_whole_r_are_the_polynomial_in_n() {
    // n − k + 1 rounded onto Γ's pole.
    let n = 1.0 - f64::EPSILON / 2.0;
    assert_eq!(ncr(n, 2.0), n * (n - 1.0) / 2.0);
    assert!(scalar("nCr(abs(x),2)+sign(0.3)", -n).is_finite());
    // nCr(n, 1) is n.
    for n in [-172.5, -171.5, -0.5, 0.5, 1e-300, -5.5e-17, 12345.678] {
        assert_eq!(ncr(n, 1.0), n, "nCr({n}, 1)");
        assert_eq!(npr(n, 1.0), n, "nPr({n}, 1)");
        assert_eq!(ncr(n, 0.0), 1.0);
        assert_eq!(npr(n, 0.0), 1.0);
    }
    // Far out: n(n − 1)/2 to the last digits, not e^(a difference of two
    // logarithms of Γ near 10¹⁶).
    let n = 562_341_325_190_349.06_f64;
    let want = n * (n - 1.0) / 2.0;
    assert_eq!(ncr(n, 2.0), want);
    assert!((ncr(-n, 2.0) - (-n) * (-n - 1.0) / 2.0).abs() <= want * 1e-15);
    // A negative integer n is Γ's pole still: undefined.
    for k in [0.0, 1.0, 2.0, 60.0] {
        assert!(ncr(-3.0, k).is_nan() && npr(-3.0, k).is_nan());
    }
    // Whole n ≥ 0 are counted exactly, as before.
    assert_eq!(ncr(61.0, 62.0), 0.0);
    assert_eq!(ncr(10.0, 3.0), 120.0);
    assert_eq!(npr(10.0, 3.0), 720.0);
    // A product that leaves the doubles before k! brings it back.
    let big = ncr(1e30 + 0.5e15, 11.0);
    assert!(big.is_infinite() || big > 1e300, "{big:e}");
    let fits = ncr(1.5e27, 11.0);
    assert!(
        fits.is_finite() && (fits / (1.5e27f64.powi(11) / 39916800.0) - 1.0).abs() < 1e-12,
        "{fits:e}"
    );

    // The value lies in the interval form's point enclosure, whatever n:
    // the same operations in the same order.
    for (f, perm) in [("nCr", false), ("nPr", true)] {
        for k in [0u32, 1, 2, 3, 7, 20, 60] {
            let src = format!("{f}(x,{k})");
            for i in -400..=400 {
                let x = f64::from(i) * 0.37 + 0.013;
                let v = if perm {
                    npr(x, f64::from(k))
                } else {
                    ncr(x, f64::from(k))
                };
                let e = enclosure(&src, x, x);
                assert!(
                    v.is_finite() && e.lo() <= v && v <= e.hi(),
                    "{src} at {x}: {v:e} outside [{:e}, {:e}]",
                    e.lo(),
                    e.hi()
                );
                assert_eq!(scalar(&src, x), v);
            }
        }
    }
}
