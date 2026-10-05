//! The simplifier on the cases that motivated it and on the 40-function
//! comparison set: simplified forms, the conditions that keep holes,
//! parity and period proofs, rational forms and limits.

use graphing::ast::Expr;
use graphing::compile::{CompileOptions, Program};
use graphing::simplify::{
    Cond, Dir, Limit, Parity, PiQ, Q, Settings, limit_at, prove_parity, prove_period,
    rational_form, side::Trig, simplify,
};
use graphing::{Equation, TrigUnit};

struct Case {
    f: Expr,
}

fn case(s: &str) -> Case {
    let text = format!("y={s}");
    let eq = Equation::parse(&text).unwrap();
    Case {
        f: eq.explicit().unwrap().1.clone(),
    }
}

fn opts(unit: TrigUnit) -> CompileOptions<'static> {
    CompileOptions {
        trig_unit: unit,
        variables: &(),
    }
}

fn simplified(s: &str) -> (String, Vec<Cond>) {
    let c = case(s);
    let o = opts(TrigUnit::Radians);
    let r = simplify(&c.f, &Settings::new(&o)).unwrap();
    (r.expr.to_string(), r.conditions)
}

fn parity(s: &str) -> Option<Parity> {
    let c = case(s);
    let o = opts(TrigUnit::Radians);
    prove_parity(&c.f, &Settings::new(&o))
}

fn period(s: &str, unit: TrigUnit) -> Option<PiQ> {
    let c = case(s);
    let o = opts(unit);
    prove_period(&c.f, &Settings::new(&o)).map(|p| p.value)
}

fn pi(n: i128, d: i128) -> Option<PiQ> {
    Some(PiQ {
        q: Q::new(n, d).unwrap(),
        k: 1,
    })
}

#[test]
fn holes_survive_simplification() {
    let (e, c) = simplified("tan(x)*cos(x)");
    assert_eq!(e, "sin(x)");
    assert_eq!(
        c,
        vec![Cond::TrigNonZero {
            f: Trig::Cos,
            arg: Expr::X
        }]
    );
    let (e, c) = simplified("cos(x)^2/cos(x)^2");
    assert_eq!(e, "1");
    assert_eq!(c.len(), 1, "cos²x ≠ 0 kept: {c:?}");
    let (e, c) = simplified("x/x");
    assert_eq!(e, "1");
    assert_eq!(c, vec![Cond::NonZero(Expr::X)]);
    let (e, c) = simplified("(x^2-1)/(x-1)");
    assert_eq!(e, "x+1");
    assert_eq!(c.len(), 1);
    // A boundary, not a hole: x > 0 stays a condition.
    let (e, c) = simplified("exp(ln(x))");
    assert_eq!(e, "x");
    assert_eq!(c, vec![Cond::Positive(Expr::X)]);
    assert_eq!(simplified("sin(x)^2+cos(x)^2").0, "1");
}

#[test]
fn extreme_constants_are_exact() {
    assert_eq!(
        simplified("ln(exp(1000000000000000)^4)-4000000000000000").0,
        "0"
    );
    // acot(10¹⁵)·10¹⁵ = 1: the stable form evaluates to it.
    let c = case("acot(1000000000000000)*1000000000000000");
    let o = opts(TrigUnit::Radians);
    let r = simplify(&c.f, &Settings::new(&o)).unwrap();
    assert!(
        !r.expr
            .any(&|n| matches!(n, Expr::Call(graphing::ast::Func::Acot, _)))
    );
    let v = Program::compile(&r.expr, &o).unwrap().eval(0.0, 0.0);
    assert!((v - 1.0).abs() < 4e-16, "{} = {v}", r.expr);
}

#[test]
fn stable_forms_are_chosen() {
    assert_eq!(simplified("sqrt(x+1)-sqrt(x)").0, "1/(sqrt(x+1)+sqrt(x))");
    assert!(simplified("1-cos(x)").0.contains("sin"));
}

#[test]
fn parity_is_proven() {
    assert_eq!(parity("sin(x)+sin(sqrt(2)*x)"), Some(Parity::Odd));
    assert_eq!(parity("tan(x)*cos(x)"), Some(Parity::Odd));
    assert_eq!(parity("cos(x)^2/cos(x)^2"), Some(Parity::Even));
    assert_eq!(parity("x*sin(x)"), Some(Parity::Even));
    assert_eq!(parity("x^2+1/x^2"), Some(Parity::Even));
    assert_eq!(parity("x^3-2x"), Some(Parity::Odd));
    // Not proven (and not true): an asymmetric domain or shape.
    assert_eq!(parity("ln(x)"), None);
    assert_eq!(parity("sqrt(x)*0+1"), None);
    assert_eq!(parity("x^2+x"), None);
    assert_eq!(parity("exp(-(x-100)^2)+exp(-x^2)"), None);
}

#[test]
fn periods_are_proven() {
    use TrigUnit::*;
    assert_eq!(period("sin(x)", Radians), pi(2, 1));
    assert_eq!(period("tan(x)", Radians), pi(1, 1));
    assert_eq!(period("sin(2x)+cos(3x)", Radians), pi(2, 1));
    assert_eq!(period("sin(x)+sin(sqrt(2)*x)", Radians), None);
    assert_eq!(
        period("sin(x)", Degrees),
        Some(PiQ {
            q: Q::int(360),
            k: 0
        })
    );
    // A hole outside every trig function breaks the period.
    assert_eq!(period("sin(x)+x/x", Radians), None);
}

#[test]
fn rational_forms_and_limits() {
    let c = case("(x^2-1)/(x-1)");
    let f = rational_form(&c.f).unwrap();
    assert_eq!(f.reduced.oblique(), Some((Q::ONE, Q::ONE)));
    let o = opts(TrigUnit::Radians);
    let lim = |s: &str, d: Dir| {
        let c = case(s);
        limit_at(&c.f, d, &Settings::new(&o))
    };
    assert_eq!(
        lim("atan(x)", Dir::PosInf),
        Limit::Exact(PiQ {
            q: Q::new(1, 2).unwrap(),
            k: 1
        })
    );
    assert_eq!(
        lim("1/(1+e^x)", Dir::NegInf),
        Limit::Exact(PiQ { q: Q::ONE, k: 0 })
    );
    assert_eq!(
        lim("sin(x)/x", Dir::PosInf),
        Limit::Exact(PiQ { q: Q::ZERO, k: 0 })
    );
}

/// The 40-function comparison set: each simplifies within the limits, and
/// wherever the input is defined the simplified form is defined and agrees.
#[test]
fn the_forty_simplify_soundly() {
    const SET: &[&str] = &[
        "x^2",
        "x^3-2x+1/(x-1)",
        "sin(x)",
        "tan(x)",
        "1/x",
        "ln(x)",
        "log(x)",
        "sqrt(x)",
        "e^x",
        "(x^2-1)/(x-1)",
        "x^0.9",
        "x/ln(x)",
        "1/ln(x)",
        "sin(x^2)",
        "floor(x)",
        "atan(x)",
        "x*e^(-x)",
        "sin(x)/x",
        "x*sin(x)",
        "1/(x-2)",
        "1/(x^2-4)",
        "cot(x)",
        "csc(x)",
        "sec(x)",
        "coth(x)",
        "csch(x)",
        "atan(1/x)",
        "e^(-1/x^2)",
        "e^(ln(x))",
        "1/(1+e^x)",
        "x*ln(x)",
        "x/x",
        "1/x^2",
        "log(x-5000000)",
        "1/(x^2-400000000)",
        "tan(x)^2",
        "1/sin(x)",
        "ln(abs(x))",
        "sqrt(1/x)",
        "x^x",
    ];
    let o = opts(TrigUnit::Radians);
    let xs: Vec<f64> = (-40..=40)
        .map(|i| i as f64 * 0.37 + 0.011)
        .chain([1e-3, -1e-3, 0.5, 2.5, 7.0, 1e3, -1e3, 5000001.0, 20001.0])
        .collect();
    for s in SET {
        let c = case(s);
        let t = std::time::Instant::now();
        let r = simplify(&c.f, &Settings::new(&o)).unwrap();
        let ms = t.elapsed().as_secs_f64() * 1e3;
        assert!(ms < 500.0, "{s}: {ms} ms");
        let a = Program::compile(&c.f, &o).unwrap();
        let b = Program::compile(&r.expr, &o).unwrap();
        for &x in &xs {
            let (va, vb) = (a.eval(x, 0.0), b.eval(x, 0.0));
            if va.is_nan() {
                continue;
            }
            assert!(!vb.is_nan(), "{s} → {}: undefined at {x} ({va})", r.expr);
            let tol = 1e-9 * va.abs().max(vb.abs()).max(1e-300);
            assert!(
                (va - vb).abs() <= tol || (va.is_infinite() && va == vb),
                "{s} → {}: {va} vs {vb} at {x}",
                r.expr
            );
        }
    }
}
