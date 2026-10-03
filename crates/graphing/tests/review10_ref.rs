//! Round-10 follow-up: the reference evaluator (`analysis::truth`) and the
//! compiled program's extended range both hold their values to a double's
//! last bits where results leave the doubles. The expected values are the
//! expressions' exact values (with their constants as the doubles they
//! parse to), from Python's `decimal` at 80 digits.

use graphing::analysis::truth::{R, reval};
use graphing::compile::{CompileOptions, Program};
use graphing::equation::Equation;
use graphing::functions::TrigUnit;

fn parse(src: &str) -> (graphing::ast::Expr, Program) {
    let eq = Equation::parse(&format!("y={src}")).unwrap();
    let (_, ast) = eq.explicit().unwrap();
    let p = Program::compile(ast, &CompileOptions::default()).unwrap();
    (ast.clone(), p)
}

fn ulps(got: f64, want: f64) -> f64 {
    let u = f64::from_bits(want.abs().to_bits() + 1) - want.abs();
    (got - want).abs() / u
}

/// Both evaluators within `n` ulps of `want` at `x`.
fn both_within(src: &str, x: f64, want: f64, n: f64) {
    let (ast, p) = parse(src);
    let c = p.eval(x, 0.0);
    assert!(
        ulps(c, want) <= n,
        "{src} at {x:e}: compiled {c:e} vs {want:e}"
    );
    match reval(&ast, x, TrigUnit::Radians) {
        R::V(v) => {
            let r = v.f();
            assert!(
                ulps(r, want) <= n,
                "{src} at {x:e}: reference {r:e} vs {want:e}"
            );
        }
        r => panic!("{src} at {x:e}: reference {r:?}"),
    }
}

#[test]
fn subnormal_intermediates_are_kept_exact() {
    // x/1000 at x = 2.0237e-320 is below the normal doubles: rounded to
    // the subnormal grid it would be 2% off.
    let x = 2.0237e-320;
    both_within("(x/1000)^0.9", x, 3.762_964_700_878_684e-291, 2.0);
    both_within("sqrt(x/1000)", x, 4.498_547_415_961_896e-162, 2.0);
    both_within("ln(x/1000)", x, -743.030_061_033_644_1, 2.0);
    both_within(
        "(x/1000)^(1/3)",
        -5.180654e-318,
        -1.730_327_021_892_424_8e-107,
        2.0,
    );
    both_within(
        "1000000000000*(x*ln(x))",
        4e-323,
        -2.934_199_074_365_296_6e-308,
        2.0,
    );
}

#[test]
fn powers_far_out_of_range_keep_their_last_bits() {
    // e^(−725) as a power of the double e: its exponent t·ln b is about
    // −725, where a double's rounding alone costs hundreds of ulps.
    both_within("1000000*(x*e^(-x))", 725.0, 9.927_470_991_567_15e-307, 2.0);
    both_within(
        "1000000000000*(e^x)",
        -727.0,
        1.853_154_618_575_167_6e-304,
        2.0,
    );
    // (e^x)² is below the doubles; divided by e^x it is e^x again.
    both_within("exp(x)^2/exp(x)", -365.0, 3.037_484_743_850_101e-159, 2.0);
    // e^(4.2·10¹⁴) and e^(−2.1·10¹⁴), each far out, multiply back to 1.
    both_within(
        "sqrt(exp(-(x+800)))*exp((x+800)/2)",
        -8.254041852680173e14,
        1.0,
        2.0,
    );
}

#[test]
fn a_tiny_exponent_is_no_integer() {
    // (x/1000)^(x/1000) at x = −1.5·10⁻³²³: a negative base to a nonzero
    // power far below the doubles, so no integer: undefined in the reals.
    let (ast, p) = parse("(x/1000)^(x/1000)");
    let x = -1.5e-323;
    assert!(p.eval(x, 0.0).is_nan());
    assert!(matches!(reval(&ast, x, TrigUnit::Radians), R::Undef));
    // A positive base to it is 1 to a double's precision.
    let (ast, p) = parse("2^(x/1000)");
    assert_eq!(p.eval(x, 0.0), 1.0);
    assert!(matches!(reval(&ast, x, TrigUnit::Radians), R::V(v) if v.f() == 1.0));
}
