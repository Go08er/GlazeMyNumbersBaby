//! Review 21: a far value's exponent must describe the value it stands
//! for (`wide::Wide::far`'s invariant). Min/max of two far values of one
//! sign and side lent its first operand's exponent to the result, a sum of
//! a lost value and one beyond even the extended range kept the lost one's,
//! and an exponent of exactly 2⁵³ was taken as exact; a later small power,
//! root or ratio then drew a false curve.
use graphing::compile::{CompileOptions, Input, Program};
use graphing::lexer::ParseOptions;
use graphing::plot::{PlotOptions, plot};
use graphing::{Equation, Viewport};

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

fn program(eq: &Equation) -> Program {
    Program::compile_typed(eq.explicit().unwrap().1, &CompileOptions::default()).unwrap()
}

/// "Can't tell", with literals off and at the default 14 digits: one point
/// and a batch, and no curve at the reset view, only a plot marked partial.
fn unknown_everywhere(text: &str) {
    for digits in [None, Some(14)] {
        let eq = parse(text, digits);
        let p = program(&eq);
        assert!(p.eval(0.0, 0.0).is_nan(), "{text}: {}", p.eval(0.0, 0.0));
        assert!(p.undecided(0.0, 0.0), "{text}: unknown, not undefined");
        let mut batch = [0.0; 2];
        p.eval_batch(Input::Slice(&[0.0, 1.0]), Input::Scalar(0.0), &mut batch);
        assert!(batch.iter().all(|v| v.is_nan()), "{text}: {batch:?}");
        let c = eq.compile(&CompileOptions::default()).unwrap();
        let drawn = plot(
            &c,
            &Viewport::default_for_size(760.0, 500.0),
            &PlotOptions::default(),
        );
        assert!(drawn.curves.is_empty(), "{text}: false curve");
        assert!(drawn.has_missing_data, "{text}: the value is unresolved");
    }
}

/// Decided, within `tol` of `want`, with literals off and at 14 digits.
fn decided(text: &str, want: f64, tol: f64) {
    for digits in [None, Some(14)] {
        let p = program(&parse(text, digits));
        let v = p.eval(0.0, 0.0);
        assert!((v - want).abs() <= tol, "{text}: {v} for {want}");
        assert!(!p.undecided(0.0, 0.0), "{text}: decided");
    }
}

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
            unknown_everywhere(&format!("sign(abs({op}({a},{b}))^(10^-16)-{bound})"));
        }
    }
    // A root, too: √e^(10¹⁶) = e^(5·10¹⁵) < e^(5.1·10¹⁵), +1 on main.
    for args in ["exp(10^300),exp(10^16)", "exp(10^16),exp(10^300)"] {
        unknown_everywhere(&format!("sign(sqrt(min({args}))-exp(5.1*10^15))"));
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
        decided(src, want, 0.0);
    }
}

/// Of two lost values the selected one's exponent is kept, whichever order
/// they come in: min(e^(10³⁰⁰), e^(10²⁰))^(10⁻⁴) is e^(10¹⁶), past
/// e^(10¹⁵). (The proposal kept sign and side alone: "can't tell".)
#[test]
fn far_minmax_of_two_lost_values_keeps_the_selected_ones_size() {
    // (op, the two arguments, the power and what it is compared with.)
    let above = "^(10^-4)-exp(10^15)";
    let below = "^(10^-4)-exp(-10^15)";
    for (op, a, b, rest, want) in [
        ("min", "exp(10^300)", "exp(10^20)", above, 1.0),
        ("max", "exp(-10^300)", "exp(-10^20)", below, -1.0),
        ("max", "-exp(10^300)", "-exp(10^20)", above, 1.0),
        ("min", "-exp(-10^300)", "-exp(-10^20)", below, -1.0),
        (
            "min",
            "exp(10^20)",
            "exp(10^19)",
            "^(10^-3)-exp(10^15)",
            1.0,
        ),
    ] {
        for (a, b) in [(a, b), (b, a)] {
            decided(&format!("sign(abs({op}({a},{b})){rest})"), want, 0.0);
        }
    }
}

/// A value beyond even the extended range ((5·10¹²)!, about e^(1.4·10¹⁴))
/// is past 2⁴⁷ binades and no more: it may be nearer 1 than a lost one,
/// so neither its order against one nor a sum of the two below the doubles
/// takes the lost one's exponent.
#[test]
fn a_lost_value_is_not_ordered_against_one_beyond_the_extended_range() {
    // min((5·10¹²)!, e^(10²⁰)) is the factorial: its hundredth root is
    // e^(1.4·10¹²) < e^(2·10¹²), −1. Main drew +1.
    for args in ["exp(10^20),(5*10^12)!", "(5*10^12)!,exp(10^20)"] {
        unknown_everywhere(&format!("sign(min({args})^(10^-2)-exp(2*10^12))"));
        unknown_everywhere(&format!("sign(max({args})^(10^-2)-exp(2*10^12))"));
    }
    // e^(−10³⁰⁰) + e/(5·10¹²)! is about e^(−1.4·10¹⁴), and a 10⁻²⁸⁴th power
    // of it is 1 to 270 places: +1. Main took e^(−10³⁰⁰)'s exponent and
    // drew −1, in either order, and with x in it too.
    for sum in [
        "exp(-10^300)+e/((5*10^12)!)",
        "e/((5*10^12)!)+exp(-10^300)",
        "-exp(-10^300)-e/((5*10^12)!)",
    ] {
        unknown_everywhere(&format!("sign(abs({sum})^(10^-284)-0.5)"));
    }
    let eq = parse(
        "sign((exp(-10^300)+1/((x+5*10^12)!))^(10^-284)-0.5)",
        Some(14),
    );
    let p = program(&eq);
    for x in [-3.0, 0.0, 2.5] {
        assert!(p.undecided(x, 0.0), "at {x}: {}", p.eval(x, 0.0));
    }
    // Two lost values below the doubles still sum to the nearer 0:
    // (e^(−10³⁰⁰) + e^(−10²⁰))^(10⁻⁴) is e^(−10¹⁶) < e^(−10¹⁵).
    decided(
        "sign((exp(-10^300)+exp(-10^20))^(10^-4)-exp(-10^15))",
        -1.0,
        0.0,
    );
    // And the factorial alone keeps its sign and side.
    decided("sign(e/((5*10^12)!))", 1.0, 0.0);
}

/// An exponent of exactly 2⁵³ may be the rounding of 2⁵³ + 1, and a
/// value made there has no mantissa: it is lost (far), never within
/// reach. e^6243314768165359's exponent rounds to 2⁵³; main had
/// e^6243314768165359/e^6243314768165358 = 3.35 (truly e), and
/// e^6243314768165358·8 (exponent 2⁵³ + 1) over itself 4 (truly 8).
#[test]
fn an_exponent_of_two_to_the_53_is_lost() {
    unknown_everywhere("exp(6243314768165000+359)/exp(6243314768165000+358)");
    unknown_everywhere("sign(exp(6243314768165000+359)/exp(6243314768165000+358)-3)");
    unknown_everywhere("exp(6243314768165000+358)*8/exp(6243314768165000+358)");
    unknown_everywhere("exp(-6243314768165000-359)/exp(-6243314768165000-358)");
    // One binade inside, exponents are exact: e and 2.
    decided(
        "exp(6243314768165000+357)/exp(6243314768165000+356)",
        std::f64::consts::E,
        1e-12,
    );
    decided(
        "exp(6243314768165000+356)*2/exp(6243314768165000+356)",
        2.0,
        0.0,
    );
    // Still decided past the doubles, on its side.
    decided("sign(exp(6243314768165000+359)-10^300)", 1.0, 0.0);
}
