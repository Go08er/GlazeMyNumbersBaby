//! Review 20: exact factorial domains past the doubles, values beyond the
//! doubles that carry only their sign and side, and exact products proven
//! in integers.
use graphing::analysis::truth::{R, reval_typed};
use graphing::compile::{CompileOptions, Input, Program};
use graphing::lexer::ParseOptions;
use graphing::plot::{PlotOptions, plot};
use graphing::{Equation, TrigUnit, Viewport};

fn parse(text: &str, digits: Option<u8>) -> Equation {
    Equation::parse_with(
        &format!("y={text}"),
        ParseOptions {
            literal_digits: digits,
            decimal_comma: false,
        },
    )
    .unwrap_or_else(|e| panic!("{text}: {e:?}"))
}

fn program(text: &str) -> (Equation, Program) {
    let eq = parse(text, None);
    let p = Program::compile_typed(eq.explicit().unwrap().1, &CompileOptions::default())
        .expect("compiles");
    (eq, p)
}

fn reference(eq: &Equation, x: f64) -> R {
    reval_typed(eq.explicit().unwrap().1, x, TrigUnit::Radians)
}

/// "Can't tell" at the default 14 digits (as typed, after "off|"),
/// one point and a batch, and so no curve at the reset view, only a plot
/// marked partial.
fn unknown_everywhere(text: &str) {
    let (eq, text) = match text.strip_prefix("off|") {
        Some(t) => (parse(t, None), t),
        None => (parse(text, Some(14)), text),
    };
    let p = Program::compile_typed(eq.explicit().unwrap().1, &CompileOptions::default())
        .expect("compiles");
    assert!(p.eval(0.0, 0.0).is_nan(), "{text}: {}", p.eval(0.0, 0.0));
    assert!(p.undecided(0.0, 0.0), "{text}: unknown, not undefined");
    let mut batch = [0.0; 2];
    p.eval_batch(Input::Slice(&[0.0, 1.0]), Input::Scalar(0.0), &mut batch);
    assert!(batch.iter().all(|v| v.is_nan()), "{text}: batch {batch:?}");
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

/// Decided as `want` at the default 14 digits (NaN: known undefined).
fn decided(text: &str, want: f64) {
    let eq = parse(text, Some(14));
    let p = Program::compile_typed(eq.explicit().unwrap().1, &CompileOptions::default())
        .expect("compiles");
    let v = p.eval(0.0, 0.0);
    assert!(
        v.to_bits() == want.to_bits() || (v.is_nan() && want.is_nan()) || v == want,
        "{text}: {v} for {want}"
    );
    assert!(!p.undecided(0.0, 0.0), "{text}: decided");
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
        assert!(matches!(reference(&eq, 0.0), R::Undef), "{text}: reference");
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
        assert!(matches!(reference(&eq, 0.0), R::Undef), "{text}: reference");
    }
    // Beside the pole it has a value, but the exact-distance formula is
    // deliberately bounded. Report unknown, never a proven pole.
    let (eq, p) = program("(-10^400+0.1)!");
    assert!(p.eval(0.0, 0.0).is_nan());
    assert!(p.undecided(0.0, 0.0));
    assert!(matches!(reference(&eq, 0.0), R::Unknown));
}

/// A value proven past the doubles but too long to carry exactly (3²⁰⁰⁰⁰,
/// 1.5¹⁰⁰⁰⁰⁰) has an exact parity, so whether it is a whole number is
/// known though its double isn't (every double past 2⁵³ is even): a pole
/// of n!, n!! of no whole number, are decided by it. Each of these was
/// "can't tell"; where only the extended-range value is known (e^(10³⁰⁰),
/// whose made-up mantissa says nothing of it), the pole or n!! stays so.
#[test]
fn a_factorial_past_the_doubles_is_decided_by_exact_parity() {
    for text in [
        "(-(3^20000))!",
        "1/((-(3^20000))!)",
        "(-(3^20000))!!",
        "(1.5^100000)!!",
        "(-(1.5^100000))!!",
        "(2^-100000)!!",
        "x+(1.5^100000)!!",
    ] {
        let (eq, p) = program(text);
        assert!(p.eval(0.0, 0.0).is_nan(), "{text}");
        assert!(!p.undecided(0.0, 0.0), "{text}: known undefined");
        assert!(matches!(reference(&eq, 0.0), R::Undef), "{text}: reference");
    }
    for (text, want) in [
        ("(3^20000)!", f64::INFINITY),
        ("(1.5^100000)!", f64::INFINITY),
        ("(3^20000)!!", f64::INFINITY),
        ("(2^-100000)!", 1.0),
        ("(-(2^-100000))!", 1.0),
    ] {
        let (_, p) = program(text);
        assert_eq!(p.eval(0.0, 0.0), want, "{text}");
        assert!(!p.undecided(0.0, 0.0), "{text}");
    }
    // Beside a pole past the factor bound, and of computed values past
    // 2⁵³ binades: unknown, never decided.
    for text in [
        "(-(1.5^100000))!",
        "(3^20000*1.5^100000)!!",
        "(-exp(10^300))!",
        "(exp(10^300))!!",
        "(3^20000)!-(3^20000)!",
    ] {
        let (_, p) = program(text);
        assert!(p.eval(0.0, 0.0).is_nan(), "{text}");
        assert!(p.undecided(0.0, 0.0), "{text}: unknown");
    }
    // Γ(1 + w) of a positive w past the range is past it, whole or not.
    let (_, p) = program("(exp(10^300))!");
    assert_eq!(p.eval(0.0, 0.0), f64::INFINITY);
    assert!(!p.undecided(0.0, 0.0));
}

#[test]
fn lost_magnitudes_do_not_cancel_to_an_exact_zero_or_a_drawn_line() {
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
        unknown_everywhere(text);
    }
    // A true zero factor or numerator is independent of the lost
    // magnitude, and ordinary extended values can still cancel exactly.
    for text in ["0*exp(10^300)", "0/exp(10^300)", "exp(1000)-exp(1000)"] {
        let (_, p) = program(text);
        assert_eq!(p.eval(0.0, 0.0), 0.0, "{text}");
        assert!(!p.undecided(0.0, 0.0), "{text}");
    }
}

/// R20-M-01's whole family. A value past 2⁵³ binades (lost), or beyond
/// even the extended range (an infinite exponent), is known by its sign and
/// its side of the doubles alone: its exponent is rounded, so two of them
/// on one side can't be put in order, and nothing of its logarithm is held.
/// Each of these was decided on main, most as a whole curve at the reset
/// view, and all but the few marked at a value the true one contradicts
/// (in brackets: what main gave, and the truth, from the doubles the
/// literals are; the MPFR side is in `crossfuzz_found`):
#[test]
fn far_values_decide_nothing_but_their_sign_and_side() {
    for text in [
        // e^A·e^B/e^C with A + B = C − 2⁹⁴⁴, all exact doubles: the
        // exponents rounded up, then down [π/2, and 1; e^(−2⁹⁴⁴): 0, −1].
        "off|atan(exp(7801888238375471*2^944)*exp(6783390357013892*2^944)/exp(7292639297694682*2^945))",
        "off|sign(exp(7801888238375471*2^944)*exp(6783390357013892*2^944)-exp(7292639297694682*2^945))",
        // ln of a value beyond even the extended range was +∞ [π/2; 0].
        "atan(ln(ln(exp(exp(1000))))-1000)",
        "atan(ln(exp(1.3*10^308))-1.3*10^308)",
        "atan(asinh(exp(1.3*10^308))-1.3*10^308)",
        "atan(log(2,exp(exp(1000)))-exp(1000)/ln(2))",
        // Values made beyond the extended range that aren't (2^1001)!
        // has 2^1010.4 binades; 1.0000001^(10^309), 2^302.4; √e^(1.3·10³⁰⁸),
        // 2^(0.94·10³⁰⁸)), times smaller values below the doubles
        // [π/2 and 1; 0 and −1].
        "atan((2^1001)!*exp(-2^1011))",
        "sign((2^1001)!*exp(-2^1011)-1)",
        "atan(1.0000001^(10^309)*exp(-10^305))",
        "atan(sqrt(exp(1.3*10^308))/exp(0.7*10^308))",
        // A small power of one: (e^(1.3·10³⁰⁸))^(10⁻³⁰⁶) is e¹³⁰ [1; −1].
        "sign(exp(1.3*10^308)^(10^-306)-10^60)",
        // A count whose logarithm passed 2⁴⁷ keeps only its side, past
        // 2⁴⁷·log₂e binades, not past every value held in full:
        // (5·10¹²)! is e^(1.41·10¹⁴), below e^(2·10¹⁴) [1, π/2, 1, π/2;
        // −1, 0, −1, 0].
        "sign((5*10^12)!-exp(2*10^14))",
        "atan((5*10^12)!/exp(2*10^14))",
        "sign((5*10^12)!*exp(-2*10^14)-1)",
        "atan(sqrt((5*10^12)!)/exp(10^14))",
        // Nor does it follow a product back toward 1, so it is never
        // taken far back: e^(−4.8·10¹³) is within 2⁴⁶ binades of 1, but
        // three of them bring (5·10¹²)! below 1 [1; −1].
        "sign((5*10^12)!*exp(-4.8*10^13)*exp(-4.8*10^13)*exp(-4.8*10^13)-1)",
        // Rounded exponents taken as ordered. Main had these right, its
        // roundings happening to keep the order, but nothing proves it:
        // the same shapes rounded the other way were wrong (above).
        "sign(exp(10^300)*exp(-10^300+10^290)-1)",
        "atan(exp(10^300)/exp(10^300-10^285))",
        "floor(exp(10^300)-exp(10^300+3*10^284))",
        "mod(-exp(10^300),exp(10^300+10^285))",
        "sign(max(exp(10^300+2*10^284),exp(10^300+3*10^284))-exp(10^300+2*10^284))",
        // A negative base to an exponent whose parity is made up [+∞].
        "(-2)^(exp(10^300))",
    ] {
        unknown_everywhere(text);
    }
    // What a far value's sign and side decide stays decided.
    for (text, want) in [
        ("max(exp(10^300),exp(10^300))", f64::INFINITY),
        ("min(-exp(10^300),-exp(10^299))", f64::NEG_INFINITY),
        ("sign(exp(10^300)-exp(10^10))", 1.0),
        ("sign(exp(-10^300)-exp(-10^10))", -1.0),
        ("sign(-exp(10^300)+exp(-10^300))", -1.0),
        ("sign((5*10^12)!-exp(10^13))", 1.0),
        ("exp(-10^300)+1", 1.0),
        ("1/sqrt(exp(10^300))-1", -1.0),
        ("atan(exp(-10^300)/exp(10^299))", 0.0),
        ("mod(-exp(-10^300),3)", 3.0),
        ("floor(-exp(-10^300))", -1.0),
        ("(exp(10^300))!", f64::INFINITY),
        ("exp(1000)-exp(1000)", 0.0),
        // Undefined by its sign alone (it was "can't tell").
        ("ln(-exp(10^300))", f64::NAN),
        ("sqrt(-exp(10^300))", f64::NAN),
    ] {
        decided(text, want);
    }
    // With x: e^x/e^x is 1 wherever e^x is held, past the doubles too.
    let (_, p) = program("exp(x)/exp(x)");
    assert_eq!(p.eval(1000.0, 0.0), 1.0);
    let (_, p) = program("exp(1000+x)/exp(1000+x)");
    assert_eq!(p.eval(1.0, 0.0), 1.0);
}
