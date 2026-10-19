//! Far exponents must not be formed by cancelling a rounded log2(m)
//! against -1 when a power's base is just below 1.
use graphing::compile::{CompileOptions, Input, Program};
use graphing::lexer::ParseOptions;
use graphing::plot::{PlotOptions, plot};
use graphing::{Equation, Viewport};

#[test]
fn a_far_power_just_below_one_keeps_its_logarithm() {
    // Each base 1 - 2^-k is exactly a double, cos(0) exactly 1. The
    // powers' true natural logarithms are about -6.243e15, above the
    // comparison's -6.2432e15: every function is +1. Rounded log2(m) - 1
    // gave a far exponent too large in magnitude, so these drew -1.
    // A correct exponent lands back within reach here: Unknown, not a
    // mantissa recovered from the earlier lost value.
    for (k, scale) in [
        (44, "1.0982801747529*10^-271"),
        (46, "4.3931206990115*10^-271"),
        (47, "8.7862413980231*10^-271"),
        (50, "7.0289931184185*10^-270"),
    ] {
        let src = format!("y=sign(((cos(0)-2^-{k})^(10^300))^({scale})-exp(-6.2432*10^15))");
        for digits in [None, Some(14)] {
            let eq = Equation::parse_with(
                &src,
                ParseOptions {
                    literal_digits: digits,
                    decimal_comma: false,
                },
            )
            .unwrap();
            let opts = CompileOptions::default();
            let p = Program::compile_typed(eq.explicit().unwrap().1, &opts).unwrap();
            assert!(p.eval(0.0, 0.0).is_nan(), "{src}: {}", p.eval(0.0, 0.0));
            assert!(p.undecided(0.0, 0.0), "{src}");
            let mut batch = [0.0; 2];
            p.eval_batch(Input::Slice(&[0.0, 1.0]), Input::Scalar(0.0), &mut batch);
            assert!(batch.iter().all(|v| v.is_nan()), "{src}: {batch:?}");
            let drawn = plot(
                &eq.compile(&opts).unwrap(),
                &Viewport::default_for_size(760.0, 500.0),
                &PlotOptions::default(),
            );
            assert!(drawn.curves.is_empty(), "{src}: false curve");
            assert!(drawn.has_missing_data, "{src}");
        }
    }
}

#[test]
fn ordinary_powers_below_one_keep_their_values() {
    for src in [
        "((cos(0)-0.5)^1000)/(2^-1000)",
        "((cos(0)-0.25)^1000)/(0.75^1000)",
        "sign((cos(0)-2^-44)^(10^300))",
    ] {
        let eq = Equation::parse(&format!("y={src}")).unwrap();
        let p =
            Program::compile_typed(eq.explicit().unwrap().1, &CompileOptions::default()).unwrap();
        assert!((p.eval(0.0, 0.0) - 1.0).abs() < 1e-12, "{src}");
        assert!(!p.undecided(0.0, 0.0), "{src}");
    }
}
