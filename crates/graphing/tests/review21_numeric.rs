//! A min/max of far values preserves sign and side, not an arbitrary
//! argument's magnitude: subsequent small powers must remain undecided.
use graphing::compile::{CompileOptions, Input, Program};
use graphing::plot::{PlotOptions, plot};
use graphing::{Equation, Viewport};

#[test]
fn far_minmax_does_not_lend_an_arguments_exponent_to_its_result() {
    // The first and third functions are -1 (e < 10); the others +1
    // (1/e > 0.1). Retaining the first argument's exponent drew the
    // opposite answer, with no missing-data flag. Both argument orders
    // must be equally conservative, for positive and negative operands.
    for (op, a, b, bound) in [
        ("min", "exp(10^300)", "exp(10^16)", "10"),
        ("max", "exp(-10^300)", "exp(-10^16)", "0.1"),
        ("max", "-exp(10^300)", "-exp(10^16)", "10"),
        ("min", "-exp(-10^300)", "-exp(-10^16)", "0.1"),
    ] {
        for (a, b) in [(a, b), (b, a)] {
            let src = format!("y=sign(abs({op}({a},{b}))^(10^-16)-{bound})");
            let eq = Equation::parse(&src).unwrap();
            let opts = CompileOptions::default();
            let p = Program::compile_typed(eq.explicit().unwrap().1, &opts).unwrap();
            assert!(p.eval(0.0, 0.0).is_nan(), "{src}");
            assert!(p.undecided(0.0, 0.0), "{src}: unknown, not undefined");
            let mut batch = [0.0; 2];
            p.eval_batch(Input::Slice(&[0.0, 1.0]), Input::Scalar(0.0), &mut batch);
            assert!(batch.iter().all(|v| v.is_nan()), "{src}: {batch:?}");
            let c = eq.compile(&opts).unwrap();
            let drawn = plot(
                &c,
                &Viewport::default_for_size(760.0, 500.0),
                &PlotOptions::default(),
            );
            assert!(drawn.curves.is_empty(), "{src}");
            assert!(drawn.has_missing_data, "{src}");
        }
    }
}

#[test]
fn far_minmax_still_decides_signs_and_ordinary_minmax_keeps_magnitudes() {
    for (src, want) in [
        ("sign(min(exp(10^300),exp(10^16)))", 1.0),
        ("sign(max(-exp(10^300),-exp(10^16)))", -1.0),
        ("min(exp(1000),exp(1100))/exp(1000)", 1.0),
        ("max(exp(-1000),exp(-1100))/exp(-1000)", 1.0),
        ("min(-2,3)", -2.0),
        ("max(-2,3)", 3.0),
    ] {
        let eq = Equation::parse(&format!("y={src}")).unwrap();
        let p =
            Program::compile_typed(eq.explicit().unwrap().1, &CompileOptions::default()).unwrap();
        assert_eq!(p.eval(0.0, 0.0), want, "{src}");
        assert!(!p.undecided(0.0, 0.0), "{src}");
    }
}
