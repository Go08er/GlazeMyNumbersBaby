//! Exact factorial domains and lost extended magnitudes retain their meaning.
use graphing::analysis::truth::{R, reval_typed};
use graphing::compile::{CompileOptions, Input, Program};
use graphing::lexer::ParseOptions;
use graphing::{Equation, TrigUnit};

fn program(text: &str) -> (Equation, Program) {
    let eq = Equation::parse_with(
        &format!("y={text}"),
        ParseOptions {
            literal_digits: None,
            decimal_comma: false,
        },
    )
    .expect("parses");
    let p = Program::compile_typed(eq.explicit().unwrap().1, &CompileOptions::default())
        .expect("compiles");
    (eq, p)
}

#[test]
fn an_exact_noninteger_has_no_double_factorial_beyond_binary64() {
    for text in [
        "(10^400+0.1)!!",
        "1/((10^400+0.1)!!)",
        "(-10^400+0.1)!!",
        "(10^400+0.1)!!+x",
    ] {
        let (eq, p) = program(text);
        assert!(p.eval(0.0, 0.0).is_nan(), "{text}");
        assert!(!p.undecided(0.0, 0.0), "{text}: known undefined");
        let mut batch = [0.0; 2];
        p.eval_batch(Input::Slice(&[0.0, 1.0]), Input::Scalar(0.0), &mut batch);
        assert!(batch.iter().all(|v| v.is_nan()), "{text}: batch");
        assert!(
            matches!(
                reval_typed(eq.explicit().unwrap().1, 0.0, TrigUnit::Radians),
                R::Undef
            ),
            "{text}: reference"
        );
    }
    // The same magnitude, now an integer: its double factorial is a
    // positive value beyond the doubles, rather than undefined.
    let (_, p) = program("(10^400)!!");
    assert_eq!(p.eval(0.0, 0.0), f64::INFINITY);
    assert!(!p.undecided(0.0, 0.0));
}

#[test]
fn a_factorial_pole_is_decided_by_exact_integrality() {
    for text in ["(-9007199254740993)!", "(-10^400)!"] {
        let (eq, p) = program(text);
        assert!(p.eval(0.0, 0.0).is_nan(), "{text}");
        assert!(!p.undecided(0.0, 0.0), "{text}: known pole");
        assert!(
            matches!(
                reval_typed(eq.explicit().unwrap().1, 0.0, TrigUnit::Radians),
                R::Undef
            ),
            "{text}: reference"
        );
    }
    // Beside the pole it has a value, but the exact-distance formula is
    // deliberately bounded. Report unknown, never a proven pole.
    let (eq, p) = program("(-10^400+0.1)!");
    assert!(p.eval(0.0, 0.0).is_nan());
    assert!(p.undecided(0.0, 0.0));
    assert!(matches!(
        reval_typed(eq.explicit().unwrap().1, 0.0, TrigUnit::Radians),
        R::Unknown
    ));
}

#[test]
fn lost_magnitudes_do_not_cancel_to_an_exact_zero_or_a_drawn_line() {
    use graphing::Viewport;
    use graphing::plot::{PlotOptions, plot};
    // These distinct, ordered arguments remain distinct after rounding
    // to doubles. Their quotients by ln 2 round alike: beyond 2^53
    // binades the extended representation keeps no useful mantissa.
    // The first sign is -1, the second +1; an honest unknown is allowed.
    let (_, a) = program("10^300+2*10^284");
    let (_, b) = program("10^300+3*10^284");
    assert!(a.eval(0.0, 0.0) < b.eval(0.0, 0.0));
    for text in [
        "sign(exp(10^300+2*10^284)-exp(10^300+3*10^284))",
        "sign(exp(-(10^300+2*10^284))-exp(-(10^300+3*10^284)))",
        "exp(10^300+2*10^284)-exp(10^300+3*10^284)",
    ] {
        let eq = Equation::parse_with(
            &format!("y={text}"),
            ParseOptions {
                literal_digits: Some(14),
                decimal_comma: false,
            },
        )
        .expect("parses at default 14 digits");
        let p = Program::compile_typed(eq.explicit().unwrap().1, &CompileOptions::default())
            .expect("compiles");
        assert!(p.eval(0.0, 0.0).is_nan(), "{text}");
        assert!(p.undecided(0.0, 0.0), "{text}: unknown, not undefined");
        let mut batch = [0.0; 2];
        p.eval_batch(Input::Slice(&[0.0, 1.0]), Input::Scalar(0.0), &mut batch);
        assert!(batch.iter().all(|v| v.is_nan()), "{text}: batch");
        let compiled = eq.compile(&CompileOptions::default()).expect("compiles");
        let drawn = plot(
            &compiled,
            &Viewport::default_for_size(760.0, 500.0),
            &PlotOptions::default(),
        );
        assert!(
            drawn.curves.is_empty(),
            "{text}: false curve {:?}",
            drawn.curves
        );
        assert!(drawn.has_missing_data, "{text}: the value is unresolved");
    }
    // A true zero factor or numerator is independent of the lost
    // magnitude, and ordinary extended values can still cancel exactly.
    for text in ["0*exp(10^300)", "0/exp(10^300)", "exp(1000)-exp(1000)"] {
        let (_, p) = program(text);
        assert_eq!(p.eval(0.0, 0.0), 0.0, "{text}");
        assert!(!p.undecided(0.0, 0.0), "{text}");
    }
}
