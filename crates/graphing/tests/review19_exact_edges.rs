//! Review 19, R19-M-04 and its family: an x-free value known exactly (a
//! typed decimal, arithmetic on literals and sliders) whose exact value
//! and whose double fall on different sides of a discontinuity or of a
//! domain's end. The exact value decides, in every view of the function:
//! the compiled program, the reference evaluator and the interval form.

use graphing::analysis::truth::{R, bind_variables, reval_typed};
use graphing::ast::{Expr, Func, Lit};
use graphing::compile::{CompileOptions, Program, Sliders};
use graphing::interval::{Ctx, Dec, Interval, enclose};
use graphing::lexer::ParseOptions;
use graphing::plot::{PlotOptions, plot};
use graphing::{Equation, TrigUnit, Viewport};

/// What a view makes of a function at a point.
#[derive(Clone, Copy, Debug, PartialEq)]
enum V {
    /// A value (±∞ for one past the largest double).
    Is(f64),
    /// Undefined there.
    Undefined,
    /// Not known: too long to carry out.
    Unknown,
    /// The function doesn't compile (a division by an exact 0).
    Refused,
}

/// Number precision "Off": each number is exactly the decimal typed.
const OFF: Option<u8> = None;
/// The apps' default: each number rounded to 14 significant digits.
const TI: Option<u8> = Some(14);

struct Views {
    compiled: V,
    reference: V,
    /// f's enclosure at the point, on the tree as typed: (lo, hi, proven
    /// defined), or `None` where it is empty (proven undefined).
    enclosure: Option<(f64, f64, bool)>,
}

/// `y = text` at `x` under a number precision and an angle unit, a slider
/// `a` (if any) at `a`.
fn views_at(text: &str, digits: Option<u8>, unit: TrigUnit, x: f64, a: f64) -> Views {
    let parse = ParseOptions {
        decimal_comma: false,
        literal_digits: digits,
    };
    let eq = Equation::parse_with(&format!("y={text}"), parse)
        .unwrap_or_else(|e| panic!("{text} doesn't parse: {e:?}"));
    let given = [("a", a)];
    let sliders = Sliders::of(&eq, &given);
    let opts = CompileOptions {
        trig_unit: unit,
        variables: &sliders,
    };
    let (_, ast) = eq.explicit().expect("y = f(x)");
    let compiled = match Program::compile_typed(ast, &opts) {
        Err(_) => V::Refused,
        Ok(p) => {
            let c = p.eval(x, 0.0);
            let mut batch = [0.0];
            p.eval_batch(
                graphing::compile::Input::Slice(&[x]),
                graphing::compile::Input::Scalar(0.0),
                &mut batch,
            );
            assert!(
                batch[0].to_bits() == c.to_bits() || (batch[0].is_nan() && c.is_nan()),
                "{text}: scalar {c:e}, batched {:e}",
                batch[0]
            );
            if !c.is_nan() {
                V::Is(c)
            } else if p.undecided(x, 0.0) {
                V::Unknown
            } else {
                V::Undefined
            }
        }
    };
    let reference = match reval_typed(&bind_variables(ast, &opts), x, unit) {
        R::V(v) if v.huge() => V::Is(f64::INFINITY.copysign(v.sign())),
        R::V(v) => V::Is(v.f()),
        R::Undef => V::Undefined,
        R::Unknown => V::Unknown,
    };
    let e = enclose(ast, Interval::point(x), &Ctx::new(opts));
    assert_ne!(e.dec, Dec::Ill, "{text}");
    let enclosure = (!e.is_empty()).then(|| (e.lo(), e.hi(), e.dec >= Dec::Def));
    Views {
        compiled,
        reference,
        enclosure,
    }
}

fn close(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 4.0 * f64::EPSILON * a.abs().max(b.abs())
}

/// The program and the reference both make `want` of `y = text` at x = 0
/// (a value: the program's to four ulps of it, the reference's to four of
/// the program's), and the interval form on the tree as typed, which
/// encloses each literal alone, doesn't speak against it: a value lies in
/// its enclosure, and where f is undefined it doesn't prove f defined.
#[track_caller]
fn check_at(text: &str, digits: Option<u8>, unit: TrigUnit, x: f64, want: V) {
    let v = views_at(text, digits, unit, x, 1.0);
    let label = format!("{text} at {x} ({digits:?}, {unit:?})");
    match (want, v.compiled) {
        (V::Is(w), V::Is(c)) => assert!(close(c, w), "{label}: compiled {c:e}, not {w:e}"),
        (w, c) => assert_eq!(c, w, "{label}: compiled"),
    }
    match (v.compiled, v.reference) {
        (V::Is(c), V::Is(r)) => assert!(close(c, r), "{label}: compiled {c:e}, reference {r:e}"),
        // (The program refuses a division by an exact 0; to the reference
        // it is undefined.)
        (V::Refused, r) => assert_eq!(r, V::Undefined, "{label}: reference"),
        // (Past the largest double the reference may not size a value.)
        (V::Is(c), V::Unknown) if c.is_infinite() => {}
        (c, r) => assert_eq!(r, c, "{label}: reference"),
    }
    match (want, v.enclosure) {
        (V::Is(w), Some((lo, hi, _))) if w.is_finite() => {
            let slack = 4.0 * f64::EPSILON * w.abs();
            assert!(
                lo - slack <= w && w <= hi + slack,
                "{label}: {w:e} outside the enclosure [{lo:e}, {hi:e}]"
            );
        }
        (V::Is(w), Some((lo, hi, _))) => {
            assert!(
                (w > 0.0 && hi >= f64::MAX) || (w < 0.0 && lo <= -f64::MAX),
                "{label}: {w:e} outside the enclosure [{lo:e}, {hi:e}]"
            );
        }
        (V::Is(w), None) => panic!("{label}: {w:e}, but the enclosure is empty"),
        (V::Undefined | V::Refused, Some((lo, hi, defined))) => {
            assert!(
                !defined,
                "{label}: undefined, but enclosed as defined in [{lo:e}, {hi:e}]"
            );
        }
        _ => {}
    }
}

#[track_caller]
fn check(text: &str, digits: Option<u8>, want: V) {
    check_at(text, digits, TrigUnit::Radians, 0.0, want);
}

/// `y = text` plotted at the apps' reset view, 760×500, is one whole
/// curve along `y = f(x)`: nothing missing, and it spans the view.
#[track_caller]
fn plots_as(text: &str, digits: Option<u8>, unit: TrigUnit, f: impl Fn(f64) -> f64) {
    let parse = ParseOptions {
        decimal_comma: false,
        literal_digits: digits,
    };
    let eq = Equation::parse_with(&format!("y={text}"), parse).expect("parses");
    let opts = CompileOptions {
        trig_unit: unit,
        variables: &(),
    };
    let compiled = eq
        .compile(&opts)
        .unwrap_or_else(|e| panic!("{text}: {e:?}"));
    let vp = Viewport::default_for_size(760.0, 500.0);
    let p = plot(&compiled, &vp, &PlotOptions::default());
    let label = format!("{text} ({digits:?}, {unit:?})");
    assert!(!p.has_missing_data, "{label}: missing data");
    assert_eq!(p.curves.len(), 1, "{label}: pieces");
    let line = &p.curves[0];
    let px = vp.x_span() / vp.width;
    assert!(
        line[0].x <= vp.x_min + px && line[line.len() - 1].x >= vp.x_max - px,
        "{label}: drawn from {} to {}",
        line[0].x,
        line[line.len() - 1].x
    );
    for q in line {
        let want = f(q.x);
        assert!(
            (q.y - want).abs() <= 1e-9 * (1.0 + want.abs()),
            "{label}: ({}, {}) drawn, f there is {want}",
            q.x,
            q.y
        );
    }
}

/// A step of a value proven beyond the doubles is decided by where that
/// value lies: its sign is ±1, a step of one nearer 0 than the smallest
/// double is 0 or ±1, and past the largest double a step leaves it there.
/// (Folding the steps had made each unknown, as arithmetic on such a value
/// is; ⌊10⁵⁰⁰⁰⌋ had been +∞ and sign(10⁵⁰⁰⁰) 1.)
#[test]
fn a_step_of_a_value_beyond_the_doubles_is_decided_by_where_it_lies() {
    for digits in [OFF, TI] {
        check("floor(10^5000)", digits, V::Is(f64::INFINITY));
        check("ceil(-(10^5000))", digits, V::Is(f64::NEG_INFINITY));
        check("round(2000!)", digits, V::Is(f64::INFINITY));
        check("sign(10^5000)", digits, V::Is(1.0));
        check("sign(-(10^5000))", digits, V::Is(-1.0));
        check("sign(abs(-(10^5000)))", digits, V::Is(1.0));
        check("sign((-10)^5001)", digits, V::Is(-1.0));
        check("sign((-10)^5000)", digits, V::Is(1.0));
        check("sign(2^(-100000))", digits, V::Is(1.0));
        check("sign(-(2^(-100000)))", digits, V::Is(-1.0));
        check("sign(10^(-3000)*(-(10^(-3000))))", digits, V::Is(-1.0));
        // Whole numbers from there on, to compute with.
        check("floor(2^(-100000))+5", digits, V::Is(5.0));
        check("floor(-(2^(-100000)))+5", digits, V::Is(4.0));
        check("ceil(2^(-100000))+5", digits, V::Is(6.0));
        check("ceil(-(2^(-100000)))+5", digits, V::Is(5.0));
        check("round(-(2^(-100000)))+5", digits, V::Is(5.0));
        check("3*sign(10^5000)+4", digits, V::Is(7.0));
        // Past the largest double it stays a value not to be computed
        // with: never the 0 that rounding step by step would make.
        check("floor(10^5000)-10^5000", digits, V::Unknown);
        check("floor(1.5^100000)-1.5^100000", digits, V::Unknown);
        check("sign(10^5000)+10^5000-10^5000", digits, V::Unknown);
        // A whole number keeps its parity through a step (for a power of
        // a negative base); a fraction's step is a whole number of a
        // parity not known.
        let at = |text: &str, want: V| check_at(text, digits, TrigUnit::Radians, -1.0, want);
        at("x^floor(10^5000)", V::Is(1.0));
        at("x^ceil(3^20000)", V::Is(-1.0));
        at("x^floor(1.5^100000)", V::Unknown);
    }
    // A decimal of more digits than are carried (no typed equation is
    // that long, but a tree may be): which side of a whole number it lies
    // on isn't known, so its step isn't either (its double is 1, and the
    // step of that would be 1 for the 2 it is).
    let digits = format!("1.{}1", "0".repeat(5000));
    let long = Expr::Num(1.0, Lit::Decimal(digits.as_str().into()));
    for f in [Func::Ceil, Func::Floor, Func::Round] {
        for e in [
            Expr::call1(f, long.clone()),
            Expr::call1(f, Expr::Neg(Box::new(long.clone()))),
        ] {
            let p = Program::compile_typed(&e, &CompileOptions::default()).expect("compiles");
            assert!(p.eval(0.0, 0.0).is_nan() && p.undecided(0.0, 0.0), "{f:?}");
            assert!(matches!(
                reval_typed(&e, 0.0, TrigUnit::Radians),
                R::Unknown
            ));
        }
    }
}

/// The steps folded on exact values: the bound they keep, 0's sign, the
/// halves (away from zero), negatives, composition, and that a step of
/// something with x in it is not folded.
#[test]
fn exact_steps_within_the_bound() {
    for digits in [OFF, TI] {
        check("sign(0)", digits, V::Is(0.0));
        check("sign(0.1+0.2-0.3)", digits, V::Is(0.0));
        check("sign(-(10^(-400)))", digits, V::Is(-1.0));
        for (text, want) in [
            ("round(0.5)", 1.0),
            ("round(-0.5)", -1.0),
            ("round(1.5)", 2.0),
            ("round(-1.5)", -2.0),
            ("round(2.5)", 3.0),
            ("round(-2.5)", -3.0),
            ("round(1/2)", 1.0),
            ("round(-7/2)", -4.0),
            ("round(1/3)", 0.0),
            ("round(-2/3)", -1.0),
            ("floor(-0.5)", -1.0),
            ("ceil(-0.5)", 0.0),
            ("floor(-7)", -7.0),
            ("ceil(-7)", -7.0),
            ("floor(-7/2)", -4.0),
            ("ceil(-7/2)", -3.0),
            // 10⁴⁰⁰⁰ is 1 more than a multiple of 3: inside the bound,
            // the step of a number of four thousand digits is exact.
            ("floor(10^4000/3)*3-10^4000", -1.0),
            ("ceil(10^4000/3)*3-10^4000", 2.0),
            ("round(10^4000/3)*3-10^4000", -1.0),
            ("round(-(10^4000)/3)*3+10^4000", 1.0),
            ("sign(-(10^4000)/3)", -1.0),
        ] {
            check(text, digits, V::Is(want));
        }
        // Past the bound nothing is folded: a value proven beyond the
        // doubles is +∞ still, under a step too, and arithmetic on it
        // unknown.
        check("floor(10^3000*10^3000)", digits, V::Is(f64::INFINITY));
        check("floor(10^5000/3)", digits, V::Unknown);
        check("floor(10^3000*10^3000)-10^6000", digits, V::Unknown);
        // Too long to carry and not beyond the doubles: unknown.
        check("floor((1+2^(-20))^800)", digits, V::Unknown);
        // A division by an exact 0 under a step is still one.
        check("floor(1/(0.1+0.2-0.3))", digits, V::Refused);
    }
    // Composition, each step on the exact value of the one inside.
    check(
        "floor(ceil(1.00000000000000000001)+0.99999999999999999999)",
        OFF,
        V::Is(2.0),
    );
    check(
        "ceil(floor(0.99999999999999999999)+0.00000000000000000001)",
        OFF,
        V::Is(1.0),
    );
    check("round(sign(-(10^(-400)))*0.5)", OFF, V::Is(-1.0));
    check("sign(round(-0.49999999999999999999))", OFF, V::Is(0.0));
    check(
        "floor(ceil(1.00000000000000000001)+0.99999999999999999999)",
        TI,
        V::Is(2.0),
    );
    // With x in it a step is the program's, in doubles: 1 + 10⁻²⁰ is 1
    // there, and the step moves with x.
    let at = |x: f64| {
        views_at(
            "ceil(x+0.00000000000000000001)",
            OFF,
            TrigUnit::Radians,
            x,
            1.0,
        )
    };
    assert_eq!(at(1.0).compiled, V::Is(1.0));
    assert_eq!(at(1.5).compiled, V::Is(2.0));
    // A slider alone is a number like one typed: its value's step, again
    // when it moves.
    for (a, want) in [(1.0, 2.0), (2.0, 3.0), (-1.0, 0.0), (0.5, 1.0)] {
        let v = views_at(
            "ceil(a+0.00000000000000000001)",
            OFF,
            TrigUnit::Radians,
            0.0,
            a,
        );
        assert_eq!((v.compiled, v.reference), (V::Is(want), V::Is(want)), "{a}");
    }
    // Under a digit limit a slider is the decimal it is rounded to: one
    // set a hair past 0.3 is 3/10, so 10·a is exactly 3.
    for (digits, want) in [(TI, 3.0), (OFF, 4.0)] {
        let v = views_at(
            "ceil(10*a)",
            digits,
            TrigUnit::Radians,
            0.0,
            0.30000000000000004,
        );
        assert_eq!((v.compiled, v.reference), (V::Is(want), V::Is(want)));
    }
}

/// The floored remainder of two exact values is exact: which multiple of
/// b lies below a is decided by them, never by their doubles. At the
/// default 14 digits too: `mod(0.3, 0.1)` was 0.1 less an ulp for the 0 it
/// is, 0.3's double being below three times 0.1's.
#[test]
fn the_remainder_of_exact_values_is_exact() {
    for digits in [OFF, TI] {
        for (text, want) in [
            ("mod(0.3,0.1)", 0.0),
            ("mod(0.7,0.1)", 0.0),
            ("mod(1,0.1)", 0.0),
            ("mod(-0.3,0.1)", 0.0),
            ("mod(1,0.3)", 0.1),
            ("mod(5.5,2)", 1.5),
            ("mod(-7,3)", 2.0),
            ("mod(7,-3)", -2.0),
            ("mod(-7,-3)", -1.0),
            // 10³⁰⁰ is 1 more than a multiple of 3 (its double is not).
            ("mod(10^300,3)", 1.0),
            // Below every double and not 0: a hair under 3, and 0's side.
            ("mod(-(10^(-400)),3)", 3.0),
            ("floor(mod(-(10^(-400)),3))", 2.0),
            ("mod(10^(-400),-3)", -3.0),
            ("sign(mod(10^(-400),3))", 1.0),
            ("mod(7,10^(-400))", 0.0),
            ("10^400*mod(7,3*10^(-400))", 1.0),
        ] {
            check(text, digits, V::Is(want));
        }
        // By an exact 0: undefined, as before (not a division to refuse).
        check("mod(7,0.1+0.2-0.3)", digits, V::Undefined);
        check("mod(7,0)", digits, V::Undefined);
        // With x in it the remainder is the program's, in doubles.
        let v = views_at("mod(x,0.1)", digits, TrigUnit::Radians, 0.3, 1.0);
        assert_eq!(v.compiled, V::Is(0.3 % 0.1));
        assert_eq!(v.reference, V::Is(0.3 % 0.1));
    }
    // Off, a number typed a hair off a multiple keeps its side of it.
    for (text, off, ti) in [
        ("mod(1.00000000000000000001,1)", 1e-20, 0.0),
        ("mod(0.99999999999999999999,1)", 1.0, 0.0),
        ("floor(mod(0.99999999999999999999,1))", 0.0, 0.0),
        ("mod(5,1.00000000000000000001)", 1.0, 0.0),
        ("floor(mod(5,1.00000000000000000001))", 0.0, 0.0),
        ("mod(5,0.99999999999999999999)", 5e-20, 0.0),
        ("mod(-1.00000000000000000001,1)", 1.0, 0.0),
    ] {
        check(text, OFF, V::Is(off));
        check(text, TI, V::Is(ti));
    }
    // A slider is a number: under a digit limit the decimal it is rounded
    // to (0.3, three times 0.1), off the double it holds (below 0.3).
    let at = |digits| views_at("mod(a,0.1)", digits, TrigUnit::Radians, 0.0, 0.3);
    assert_eq!(
        (at(TI).compiled, at(TI).reference),
        (V::Is(0.0), V::Is(0.0))
    );
    let V::Is(c) = at(OFF).compiled else {
        panic!("a value")
    };
    assert!(c > 0.09 && c < 0.1, "{c:e}");
    assert_eq!(at(OFF).reference, V::Is(c));
}

/// The least and the greatest of exact values are chosen by their own
/// order: two numbers that share a double are still apart, and a tie is
/// one only when they are one number.
#[test]
fn the_least_and_greatest_of_exact_values_are_exact() {
    for (text, off, ti) in [
        ("ceil(max(1.00000000000000000001,1))", 2.0, 1.0),
        ("ceil(max(1,1.00000000000000000001))", 2.0, 1.0),
        ("ceil(max(1,0.5,1.00000000000000000001,-3))", 2.0, 1.0),
        ("floor(min(0.99999999999999999999,1))", 0.0, 1.0),
        ("floor(min(1,2,0.99999999999999999999))", 0.0, 1.0),
        ("10^20*(max(1.00000000000000000001,1)-1)", 1.0, 0.0),
        ("10^20*(min(1.00000000000000000001,1)-1)", 0.0, 0.0),
        ("10^20*(1-min(0.99999999999999999999,1))", 1.0, 0.0),
    ] {
        check(text, OFF, V::Is(off));
        check(text, TI, V::Is(ti));
    }
    for digits in [OFF, TI] {
        for (text, want) in [
            // Below every double and not 0, against 0.
            ("sign(min(10^(-400),0))", 0.0),
            ("sign(max(10^(-400),0))", 1.0),
            ("sign(min(-(10^(-400)),0))", -1.0),
            ("sign(max(-(10^(-400)),0))", 0.0),
            // A true tie: 0.1 + 0.2 is 0.3.
            ("10^17*(min(0.1+0.2,0.3)-0.3)", 0.0),
            ("10^17*(max(0.1+0.2,0.3)-0.3)", 0.0),
            ("max(3)", 3.0),
            ("min(-2,5)", -2.0),
            ("max(-2,5,7/2)", 5.0),
            // Any other argument, as before.
            ("min(10^5000,3)", 3.0),
            ("max(-(10^5000),3)", 3.0),
            ("max(sin(1),2)", 2.0),
        ] {
            check(text, digits, V::Is(want));
        }
        // With x in it the choice is the program's, in doubles.
        let v = views_at(
            "ceil(max(x,1.00000000000000000001))",
            digits,
            TrigUnit::Radians,
            1.0,
            1.0,
        );
        assert_eq!(v.compiled, V::Is(1.0));
    }
    // A slider alone is a number.
    for (a, want) in [(1.0, 2.0), (1.5, 2.0), (2.0, 2.0), (2.5, 3.0)] {
        let v = views_at(
            "ceil(max(a,1.00000000000000000001))",
            OFF,
            TrigUnit::Radians,
            0.0,
            a,
        );
        assert_eq!((v.compiled, v.reference), (V::Is(want), V::Is(want)), "{a}");
    }
}

/// In degrees and grads an exact angle a hair off a quarter turn is not
/// the quarter turn its double is: the tangent beside its pole is a
/// (large) number, not undefined, and the sine beside its zero is not 0.
/// Each is computed from the angle's exact distance to the quarter turn.
#[test]
fn an_exact_angle_beside_a_quarter_turn_is_not_on_it() {
    use TrigUnit::{Degrees, Grads, Radians};
    use std::f64::consts::PI;
    // The sine of an angle of 10⁻²⁰ and of 10⁻³⁰, in each unit.
    let (d20, g20) = (1e-20 * PI / 180.0, 1e-20 * PI / 200.0);
    let d30 = 1e-30 * PI / 180.0;
    let at = |text: &str, digits, unit, want| check_at(text, digits, unit, 0.0, want);
    for (text, unit, off) in [
        ("tan(90.00000000000000000001)", Degrees, -1.0 / d20),
        ("tan(89.99999999999999999999)", Degrees, 1.0 / d20),
        ("tan(-90.00000000000000000001)", Degrees, 1.0 / d20),
        ("tan(270.00000000000000000001)", Degrees, -1.0 / d20),
        ("tan(90.00000000000000000001°)", Degrees, -1.0 / d20),
        ("1/tan(90.00000000000000000001)", Degrees, -d20),
        ("sec(90.00000000000000000001)", Degrees, -1.0 / d20),
        ("sec(269.99999999999999999999)", Degrees, -1.0 / d20),
        ("cot(180.00000000000000000001)", Degrees, 1.0 / d20),
        ("cot(359.99999999999999999999)", Degrees, -1.0 / d20),
        ("csc(359.99999999999999999999)", Degrees, -1.0 / d20),
        ("csc(180.00000000000000000001)", Degrees, -1.0 / d20),
        ("tan(100.00000000000000000001)", Grads, -1.0 / g20),
        ("sec(299.99999999999999999999)", Grads, -1.0 / g20),
        ("cot(199.99999999999999999999)", Grads, -1.0 / g20),
    ] {
        at(text, OFF, unit, V::Is(off));
        // To 14 digits the angle is the quarter turn: a pole.
        at(text, TI, unit, V::Undefined);
    }
    for (text, unit, off, ti) in [
        ("sin(180.00000000000000000001)", Degrees, -d20, 0.0),
        ("sin(359.99999999999999999999)", Degrees, -d20, 0.0),
        ("cos(90.00000000000000000001)", Degrees, -d20, 0.0),
        ("cos(270.00000000000000000001)", Degrees, d20, 0.0),
        ("cos(-90.00000000000000000001)", Degrees, -d20, 0.0),
        ("sin(200.00000000000000000001)", Grads, -g20, 0.0),
        ("cos(99.99999999999999999999)", Grads, g20, 0.0),
        // What reads the sine's side of 0.
        ("sign(sin(180.00000000000000000001))", Degrees, -1.0, 0.0),
        ("5+floor(sin(180.00000000000000000001))", Degrees, 4.0, 5.0),
        ("ceil(cos(270.00000000000000000001))", Degrees, 1.0, 0.0),
        // Beside a quarter turn where the function is ±1 it rounds to it.
        ("sin(90.00000000000000000001)", Degrees, 1.0, 1.0),
        ("cos(180.00000000000000000001)", Degrees, -1.0, -1.0),
        ("sec(360.00000000000000000001)", Degrees, 1.0, 1.0),
        ("tan(180.00000000000000000001)", Degrees, d20, 0.0),
        ("cot(90.00000000000000000001)", Degrees, -d20, 0.0),
    ] {
        at(text, OFF, unit, V::Is(off));
        at(text, TI, unit, V::Is(ti));
    }
    for digits in [OFF, TI] {
        // Arithmetic on short numbers lands beside a quarter turn at any
        // setting.
        at("tan(90+10^(-30))", digits, Degrees, V::Is(-1.0 / d30));
        at("1/sin(180-10^(-30))", digits, Degrees, V::Is(1.0 / d30));
        at("sign(cos(90+10^(-30)))", digits, Degrees, V::Is(-1.0));
        at("sin(3*60+10^(-30))", digits, Degrees, V::Is(-d30));
        // On the quarter turn itself nothing changes: a pole, an exact 0.
        at("tan(90)", digits, Degrees, V::Undefined);
        at("tan(45+45)", digits, Degrees, V::Undefined);
        at("cot(0.1+0.2-0.3)", digits, Degrees, V::Undefined);
        at("sin(180)", digits, Degrees, V::Is(0.0));
        at("sec(300)", digits, Grads, V::Undefined);
        // Nor below every double, where the angle keeps its side of 0.
        at("sign(sin(10^(-400)))", digits, Degrees, V::Is(1.0));
        at("sign(tan(-(10^(-400))))", digits, Grads, V::Is(-1.0));
        // With x in the angle it is the program's, in doubles.
        let v = views_at("tan(x+0.00000000000000000001)", digits, Degrees, 90.0, 1.0);
        assert_eq!(v.compiled, V::Undefined);
    }
    // A whole number of quarter turns no double holds past 2⁵³: the
    // double beside it is one (2⁵³ + 58 is 90 more than a whole turn).
    at(
        "tan(9007199254741050.5)",
        OFF,
        Degrees,
        V::Is(-1.0 / 0.5f64.to_radians().tan()),
    );
    // In radians no number typed is a quarter turn: nothing changes.
    let v = views_at(
        "tan(1.5707963267948966192313216916397514420985846996875)",
        OFF,
        Radians,
        0.0,
        1.0,
    );
    assert!(
        matches!(v.compiled, V::Is(c) if close(c, 1.633123935319537e16)),
        "{:?}",
        v.compiled
    );
    // A slider alone is a number.
    let v = views_at("tan(a+0.00000000000000000001)", OFF, Degrees, 0.0, 90.0);
    assert!(
        matches!(v.compiled, V::Is(c) if close(c, -1.0 / d20)),
        "{:?}",
        v.compiled
    );
    assert_eq!(v.reference, v.compiled);
    // The curves at the reset view: both were missing.
    plots_as("x+1/tan(90.00000000000000000001)", OFF, Degrees, |x| x);
    plots_as(
        "5+floor(sin(180.00000000000000000001))",
        OFF,
        Degrees,
        |_| 4.0,
    );
    plots_as("x*sign(cos(90+10^(-30)))", TI, Degrees, |x| -x);
}

/// A logarithm turns at 1: of an exact value a hair off 1 it is that
/// hair, not the 0 of the value's double, and as a base such a value is a
/// base, not the 1 no logarithm has.
#[test]
fn a_logarithm_of_an_exact_value_beside_one() {
    use std::f64::consts::{LN_2, LN_10};
    let ln5 = 5f64.ln();
    for (text, x, off, ti) in [
        ("ln(1.00000000000000000001)", 0.0, V::Is(1e-20), V::Is(0.0)),
        ("ln(0.99999999999999999999)", 0.0, V::Is(-1e-20), V::Is(0.0)),
        (
            "10^20*ln(1.00000000000000000001)",
            0.0,
            V::Is(1.0),
            V::Is(0.0),
        ),
        (
            "log(0.99999999999999999999)",
            0.0,
            V::Is(-1e-20 / LN_10),
            V::Is(0.0),
        ),
        (
            "log(1.00000000000000000001)",
            0.0,
            V::Is(1e-20 / LN_10),
            V::Is(0.0),
        ),
        (
            "sign(ln(1.00000000000000000001))",
            0.0,
            V::Is(1.0),
            V::Is(0.0),
        ),
        (
            "floor(ln(0.99999999999999999999))",
            0.0,
            V::Is(-1.0),
            V::Is(0.0),
        ),
        // 1 over it was a division by zero.
        ("1/ln(1.00000000000000000001)", 0.0, V::Is(1e20), V::Refused),
        (
            "1/log(0.99999999999999999999)",
            0.0,
            V::Is(-1e20 * LN_10),
            V::Refused,
        ),
        // As a base: log(b, x) is ln x/ln b.
        (
            "log(1.00000000000000000001,x)",
            2.0,
            V::Is(LN_2 * 1e20),
            V::Undefined,
        ),
        (
            "log(0.99999999999999999999,x)",
            2.0,
            V::Is(-LN_2 * 1e20),
            V::Undefined,
        ),
        (
            "log(0.99999999999999999999,5)",
            0.0,
            V::Is(-ln5 * 1e20),
            V::Undefined,
        ),
        (
            "log(1.00000000000000000001,1)",
            0.0,
            V::Is(0.0),
            V::Undefined,
        ),
        (
            "log(1.00000000000000000001,-3)",
            0.0,
            V::Undefined,
            V::Undefined,
        ),
        (
            "log(1.00000000000000000001,0)",
            0.0,
            V::Undefined,
            V::Undefined,
        ),
        (
            "log(1.00000000000000000001,1.00000000000000000001)",
            0.0,
            V::Is(1.0),
            V::Undefined,
        ),
        // Under one: 10⁻²⁰ over the base's logarithm.
        (
            "log(x,1.00000000000000000001)",
            2.0,
            V::Is(1e-20 / LN_2),
            V::Is(0.0),
        ),
        (
            "log(10,1.00000000000000000001)",
            0.0,
            V::Is(1e-20 / LN_10),
            V::Is(0.0),
        ),
        (
            "log(2,0.99999999999999999999)",
            0.0,
            V::Is(-1e-20 / LN_2),
            V::Is(0.0),
        ),
        (
            "1/log(2,1.00000000000000000001)",
            0.0,
            V::Is(LN_2 * 1e20),
            V::Refused,
        ),
        // The base's own rules still hold: no base 1, 0 or below.
        (
            "log(1,1.00000000000000000001)",
            0.0,
            V::Undefined,
            V::Undefined,
        ),
        (
            "log(0,1.00000000000000000001)",
            0.0,
            V::Undefined,
            V::Undefined,
        ),
        (
            "log(-2,1.00000000000000000001)",
            0.0,
            V::Undefined,
            V::Undefined,
        ),
    ] {
        check_at(text, OFF, TrigUnit::Radians, x, off);
        check_at(text, TI, TrigUnit::Radians, x, ti);
    }
    for digits in [OFF, TI] {
        // Arithmetic on short numbers lands beside 1 at any setting.
        check("ln(1+10^(-30))", digits, V::Is(1e-30));
        check("10^30*ln(1-10^(-30))", digits, V::Is(-1.0));
        check_at(
            "log(1+10^(-30),x)",
            digits,
            TrigUnit::Radians,
            2.0,
            V::Is(LN_2 * 1e30),
        );
        // On 1 itself, and away from it, nothing changes.
        check("ln(1)", digits, V::Is(0.0));
        check("ln(0.5+0.5)", digits, V::Is(0.0));
        check("1/ln(0.1+0.9)", digits, V::Refused);
        check("log(1,5)", digits, V::Undefined);
        check("ln(1.5)", digits, V::Is(1.5f64.ln()));
        // Nor at 0, where a value below every double keeps its side.
        check("ln(10^(-400))", digits, V::Is(-400.0 * LN_10));
        check("ln(-(10^(-400)))", digits, V::Undefined);
        check("ln(0.1+0.2-0.3)", digits, V::Undefined);
        check("log(10^(-400),5)", digits, V::Is(ln5 / (-400.0 * LN_10)));
        check("log(5,10^(-400))", digits, V::Is(-400.0 * LN_10 / ln5));
        check("log(-(10^(-400)),5)", digits, V::Undefined);
        // With x in it the logarithm is the program's, in doubles.
        let v = views_at(
            "ln(x+0.00000000000000000001)",
            digits,
            TrigUnit::Radians,
            1.0,
            1.0,
        );
        assert_eq!(v.compiled, V::Is(0.0));
    }
    plots_as(
        "10^20*ln(1.00000000000000000001)+sin(x)",
        OFF,
        TrigUnit::Radians,
        |x| 1.0 + x.sin(),
    );
    plots_as(
        "x+log(1.00000000000000000001,1.00000000000000000001)",
        OFF,
        TrigUnit::Radians,
        |x| x + 1.0,
    );
    plots_as("x+10^(-30)/ln(1+10^(-30))", TI, TrigUnit::Radians, |x| {
        x + 1.0
    });
}

/// The inverse functions that turn at ±1 (the end of a domain, or a
/// pole): an exact value a hair inside or outside is on its own side.
#[test]
fn an_inverse_function_of_an_exact_value_beside_one() {
    use TrigUnit::{Degrees, Radians};
    use std::f64::consts::{FRAC_PI_2, PI};
    let (und, is) = (V::Undefined, V::Is);
    // arccos(1 − 10⁻²⁰) = √(2·10⁻²⁰), to its square; artanh of it.
    let s = 2e-20f64.sqrt();
    let t = 0.5 * 2e20f64.ln();
    for (text, off, ti) in [
        ("asin(1.00000000000000000001)", und, is(FRAC_PI_2)),
        (
            "asin(0.99999999999999999999)",
            is(FRAC_PI_2 - s),
            is(FRAC_PI_2),
        ),
        ("asin(-1.00000000000000000001)", und, is(-FRAC_PI_2)),
        (
            "asin(-0.99999999999999999999)",
            is(s - FRAC_PI_2),
            is(-FRAC_PI_2),
        ),
        ("acos(1.00000000000000000001)", und, is(0.0)),
        ("acos(0.99999999999999999999)", is(s), is(0.0)),
        ("acos(-1.00000000000000000001)", und, is(PI)),
        ("acos(-0.99999999999999999999)", is(PI - s), is(PI)),
        ("10^10*acos(0.99999999999999999999)", is(1e10 * s), is(0.0)),
        ("acosh(0.99999999999999999999)", und, is(0.0)),
        ("acosh(1.00000000000000000001)", is(s), is(0.0)),
        ("acosh(-1.00000000000000000001)", und, und),
        ("atanh(0.99999999999999999999)", is(t), und),
        ("atanh(-0.99999999999999999999)", is(-t), und),
        ("atanh(1.00000000000000000001)", und, und),
        ("atanh(-1.00000000000000000001)", und, und),
        ("acoth(1.00000000000000000001)", is(t), und),
        ("acoth(-1.00000000000000000001)", is(-t), und),
        ("acoth(0.99999999999999999999)", und, und),
        ("asec(0.99999999999999999999)", und, is(0.0)),
        ("asec(1.00000000000000000001)", is(s), is(0.0)),
        ("asec(-1.00000000000000000001)", is(PI - s), is(PI)),
        ("asec(-0.99999999999999999999)", und, is(PI)),
        ("acsc(0.99999999999999999999)", und, is(FRAC_PI_2)),
        (
            "acsc(1.00000000000000000001)",
            is(FRAC_PI_2 - s),
            is(FRAC_PI_2),
        ),
        (
            "acsc(-1.00000000000000000001)",
            is(s - FRAC_PI_2),
            is(-FRAC_PI_2),
        ),
        ("asech(1.00000000000000000001)", und, is(0.0)),
        ("asech(0.99999999999999999999)", is(s), is(0.0)),
        ("asech(-0.99999999999999999999)", und, und),
        // What reads the side.
        ("sign(acos(0.99999999999999999999))", is(1.0), is(0.0)),
        ("1/acosh(1.00000000000000000001)", is(1.0 / s), V::Refused),
    ] {
        check_at(text, OFF, Radians, 0.0, off);
        check_at(text, TI, Radians, 0.0, ti);
    }
    // An angle comes out in the unit.
    for (text, off, ti) in [
        ("acos(0.99999999999999999999)", is(s.to_degrees()), is(0.0)),
        (
            "asin(0.99999999999999999999)",
            is(90.0 - s.to_degrees()),
            is(90.0),
        ),
        (
            "asec(-1.00000000000000000001)",
            is(180.0 - s.to_degrees()),
            is(180.0),
        ),
        (
            "acsc(-1.00000000000000000001)",
            is(s.to_degrees() - 90.0),
            is(-90.0),
        ),
        ("asin(1.00000000000000000001)", und, is(90.0)),
    ] {
        check_at(text, OFF, Degrees, 0.0, off);
        check_at(text, TI, Degrees, 0.0, ti);
    }
    for digits in [OFF, TI] {
        // Arithmetic on short numbers lands beside ±1 at any setting.
        check("acos(1-10^(-30))", digits, is(2e-30f64.sqrt()));
        check("asin(1+10^(-30))", digits, und);
        check("atanh(10^(-30)-1)", digits, is(-0.5 * 2e30f64.ln()));
        check("acosh(1-10^(-30))", digits, und);
        // On ±1 itself, away from it, and beside 0, nothing changes.
        check("asin(1)", digits, is(FRAC_PI_2));
        check("acos(0.5+0.5)", digits, is(0.0));
        check("atanh(1)", digits, und);
        check("acosh(1)", digits, is(0.0));
        check("asin(0.5)", digits, is(0.5f64.asin()));
        check(
            "asech(10^(-400))",
            digits,
            is((2.0f64).ln() + 400.0 * 10f64.ln()),
        );
        check("asech(-(10^(-400)))", digits, und);
        check("acsc(10^(-400))", digits, und);
        // With x in it the function is the program's, in doubles.
        let v = views_at("acos(x-0.00000000000000000001)", digits, Radians, 1.0, 1.0);
        assert_eq!(v.compiled, is(0.0));
    }
    plots_as(
        "10^10*acos(0.99999999999999999999)+sin(x)",
        OFF,
        Radians,
        |x| 1e10 * s + x.sin(),
    );
    plots_as(
        "atanh(0.99999999999999999999)-20+sin(x)",
        OFF,
        Radians,
        |x| t - 20.0 + x.sin(),
    );
}

/// n! is Γ(n + 1), with a pole at each whole number below 0: an exact
/// value beside one is beside the pole, not on it, and the counts nCr and
/// nPr, quotients of such factorials, likewise. n!! is of whole numbers
/// alone: a value that only rounds onto one has none.
#[test]
fn a_factorial_of_an_exact_value_beside_a_pole() {
    let (und, is) = (V::Undefined, V::Is);
    for (text, off, ti) in [
        // Γ(−10⁻²⁰) and Γ(10⁻²⁰): ∓10²⁰.
        ("(-1.00000000000000000001)!", is(-1e20), und),
        ("(-0.99999999999999999999)!", is(1e20), und),
        ("1/(-1.00000000000000000001)!", is(-1e-20), und),
        // Γ(−2 ∓ 10⁻²⁰) = ∓1/(2·10⁻²⁰), Γ(−3 − 10⁻²⁰) = 1/(6·10⁻²⁰).
        ("(-3.00000000000000000001)!", is(-5e19), und),
        ("(-2.99999999999999999999)!", is(5e19), und),
        ("(-4.00000000000000000001)!", is(1e20 / 6.0), und),
        ("sign((-170.00000000000000000001)!)", is(1.0), und),
        ("sign((-171.00000000000000000001)!)", is(-1.0), und),
        // Away from a pole a factorial is the double's, as before.
        ("(3.00000000000000000001)!", is(6.0), is(6.0)),
        (
            "(-0.50000000000000000001)!",
            is(std::f64::consts::PI.sqrt()),
            is(std::f64::consts::PI.sqrt()),
        ),
        // n!! has no value beside a whole number.
        ("(3.00000000000000000001)!!", und, is(3.0)),
        ("(-0.99999999999999999999)!!", und, is(1.0)),
        ("(6.99999999999999999999)!!", und, is(105.0)),
        // The counts, where one of n!, k!, (n − k)! is beside a pole.
        ("nCr(-1.00000000000000000001,2)", is(1.0), und),
        ("nPr(-1.00000000000000000001,1)", is(-1.0), und),
        ("nCr(4.99999999999999999999,6)", is(-1e-20 / 6.0), is(0.0)),
        ("nCr(5,6.00000000000000000001)", is(-1e-20 / 6.0), is(0.0)),
        ("nCr(5,-1.00000000000000000001)", is(-1e-20 / 6.0), is(0.0)),
        ("nPr(5,6.00000000000000000001)", is(-1.2e-18), is(0.0)),
        ("sign(nCr(5,6.00000000000000000001))", is(-1.0), is(0.0)),
        ("1/nCr(5,6.00000000000000000001)", is(-6e20), V::Refused),
        // Away from the poles a count is the doubles', as before.
        ("nCr(5.00000000000000000001,2)", is(10.0), is(10.0)),
        ("nCr(5,2.00000000000000000001)", is(10.0), is(10.0)),
    ] {
        check(text, OFF, off);
        check(text, TI, ti);
    }
    for digits in [OFF, TI] {
        // Arithmetic on short numbers lands beside a pole at any setting.
        check("(10^(-30)-1)!", digits, is(1e30));
        check("10^30*nCr(5,6+10^(-30))", digits, is(-1.0 / 6.0));
        check("(3+10^(-30))!!", digits, und);
        // On a pole itself nothing changes, nor for whole numbers.
        check("(-1)!", digits, und);
        check("(0.5-1.5)!", digits, und);
        check("nCr(-1,2)", digits, und);
        check("nCr(5,6)", digits, is(0.0));
        check("nCr(5.5,2)", digits, is(12.375));
        check("7!!", digits, is(105.0));
        check("(2.5)!!", digits, und);
        // With x in it a factorial is the program's, in doubles.
        let v = views_at(
            "(x-0.00000000000000000001)!",
            digits,
            TrigUnit::Radians,
            -1.0,
            1.0,
        );
        assert_eq!(v.compiled, und);
    }
    // Beside a pole so far down that the product to it is past the bound
    // every exact value keeps: not known (never "undefined").
    check("(-350.00000000000000000001)!", OFF, V::Unknown);
    check("(-500.00000000000000000001)!", OFF, V::Unknown);
    // The line y = x, which had no curve.
    plots_as(
        "x+1/(-1.00000000000000000001)!",
        OFF,
        TrigUnit::Radians,
        |x| x,
    );
    plots_as(
        "x*nCr(-1.00000000000000000001,2)",
        OFF,
        TrigUnit::Radians,
        |x| x,
    );
}
