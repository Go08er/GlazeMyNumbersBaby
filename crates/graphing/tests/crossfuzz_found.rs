//! What the random cross-checking tester (`tests/crossfuzz.rs`) found
//! and was fixed: each case as the tester minimised it, checked directly.

use graphing::Equation;
use graphing::analysis::truth::{R, reval_typed};
use graphing::compile::{CompileOptions, Program};
use graphing::functions::TrigUnit;
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
