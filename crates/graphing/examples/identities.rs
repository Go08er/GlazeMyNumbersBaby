//! Algebraic identities at extreme arguments: `cargo run --release -p
//! graphing --example identities [-- FILTER]`. Exits 1 on a disagreement.
//!
//! The analysis gate, the sweep and the oracle all evaluate through the same
//! built-in functions (`functions.rs`, `dd.rs`, `wide.rs`), so their
//! agreement says nothing about those. Two spellings of one function, both
//! evaluated by the program the app plots, are independent evidence: here
//! each pair must read the same in the panel's own formatting
//! (`format_nonzero`), or one side must honestly have no value: the
//! reference evaluator says it can't be known (`R::Unknown`). A value
//! against "undefined", or two different values, is a disagreement: one
//! spelling is wrong. Scalar and batch evaluation must also agree, bit for
//! bit.
//!
//! The identities are written in numerically stable forms (no
//! cosh² − sinh², no 90 − x, no 1 − cos x), so that a disagreement points
//! at a primitive rather than at cancellation in the identity itself, and
//! each is checked only where both sides are defined in the reals.
//!
//! Arguments: 0, ±{5·10⁻³²⁴, 10⁻³¹⁰, 10⁻³⁰⁰ … 10³⁰⁰}, points just either side
//! of ±1, and 700–745 (the edge of e^x), in every angle unit for the
//! trigonometric pairs.

use graphing::analysis::format::format_nonzero;
use graphing::analysis::truth::{R, reval};
use graphing::ast::Expr;
use graphing::compile::{CompileOptions, Input, Program};
use graphing::equation::Equation;
use graphing::functions::TrigUnit;

/// Where a pair is checked: both sides defined in the reals, and the
/// identity's own form well conditioned (ln(1 + x) for a tiny x, or
/// arcsin near 1, would lose the value whatever the primitives did).
type Dom = fn(f64) -> bool;

/// (left, right, where, trigonometric: checked in every angle unit)
const PAIRS: &[(&str, &str, Dom, bool)] = &[
    // Inverse circular functions against their reciprocal forms.
    ("acot(abs(x))", "atan(1/abs(x))", |x| x != 0.0, true),
    ("acot(x)+acot(-x)", "2*acot(0)", |_| true, true),
    ("asec(x)", "acos(1/x)", |x| x.abs() >= 1.001, true),
    ("acsc(x)", "asin(1/x)", |x| x.abs() >= 1.001, true),
    ("asin(x)", "atan(x/sqrt(1-x^2))", |x| x.abs() <= 0.999, true),
    (
        "acos(x)",
        "2*atan(sqrt((1-x)/(1+x)))",
        |x| x.abs() <= 0.999,
        true,
    ),
    ("atan(x)", "asin(x/sqrt(1+x^2))", |x| x.abs() <= 1e3, true),
    ("atan(x)", "-atan(-x)", |_| true, true),
    // Circular functions.
    ("tan(x)", "sin(x)/cos(x)", |_| true, true),
    ("cot(x)", "cos(x)/sin(x)", |_| true, true),
    ("sec(x)", "1/cos(x)", |_| true, true),
    ("csc(x)", "1/sin(x)", |_| true, true),
    ("sin(x)^2+cos(x)^2", "1", |_| true, true),
    ("sin(2*x)", "2*sin(x)*cos(x)", |_| true, true),
    ("sin(-x)", "-sin(x)", |_| true, true),
    ("cos(-x)", "cos(x)", |_| true, true),
    // Inverse hyperbolic functions.
    (
        "asinh(abs(x))",
        "ln(abs(x)+sqrt(x^2+1))",
        |x| x.abs() >= 1e-3,
        false,
    ),
    ("acosh(x)", "ln(x+sqrt((x-1)*(x+1)))", |x| x >= 1.0, false),
    (
        "atanh(x)",
        "ln((1+x)/(1-x))/2",
        |x| (1e-3..1.0).contains(&x.abs()),
        false,
    ),
    ("atanh(x)", "-atanh(-x)", |x| x.abs() < 1.0, false),
    ("asech(x)", "acosh(1/x)", |x| x > 0.0 && x <= 0.999, false),
    ("acsch(x)", "asinh(1/x)", |x| x != 0.0, false),
    ("acoth(x)", "atanh(1/x)", |x| x.abs() >= 1.001, false),
    ("acoth(x)", "-acoth(-x)", |x| x.abs() > 1.0, false),
    // Hyperbolic functions.
    ("sech(x)", "1/cosh(x)", |_| true, false),
    ("csch(x)", "1/sinh(x)", |x| x != 0.0, false),
    ("coth(x)", "cosh(x)/sinh(x)", |x| x != 0.0, false),
    ("tanh(x)", "sinh(x)/cosh(x)", |_| true, false),
    // Exponentials, logarithms and powers. (e^x rounds to 1 below 10⁻¹⁶,
    // and e^(ln x) magnifies ln x's last bit for large x: both are the
    // identity's conditioning, not an error.)
    ("ln(exp(x))", "x", |x| x.abs() >= 1e-3, false),
    ("exp(ln(x))", "x", |x| x > 0.0 && x <= 1e8, false),
    ("ln(exp(x)^2)", "2*x", |x| x.abs() >= 1e-3, false),
    ("ln(exp(x)^4)", "4*x", |x| x.abs() >= 1e-3, false),
    ("ln(exp(x)^8)", "8*x", |x| x.abs() >= 1e-3, false),
    ("ln(exp(x)^100)", "100*x", |x| x.abs() >= 1e-3, false),
    ("exp(x)^4", "exp(4*x)", |_| true, false),
    ("exp(x)*exp(-x)", "1", |_| true, false),
    ("exp(x)/exp(x)", "1", |_| true, false),
    ("sqrt(x^2)", "abs(x)", |_| true, false),
    ("(x^3)^(1/3)", "x", |_| true, false),
    ("sqrt(x)^2", "x", |x| x > 0.0, false),
    ("x^0.5", "sqrt(x)", |x| x > 0.0, false),
    ("x/x", "1", |x| x != 0.0, false),
    ("x-x", "0", |_| true, false),
    ("log(x)", "ln(x)/ln(10)", |x| x > 0.0, false),
    ("10^log(x)", "x", |x| x > 0.0 && x <= 1e8, false),
    ("2^x", "exp(x*ln(2))", |x| x.abs() <= 1e3, false),
    ("x^-1", "1/x", |x| x != 0.0, false),
    ("(1/x)^-1", "x", |x| x != 0.0, false),
];

fn args() -> Vec<f64> {
    let mut v = vec![0.0];
    let mut m = vec![
        5e-324, 1e-310, 1e-300, 1e-200, 1e-100, 1e-20, 1e-8, 1e-3, 0.1, 0.5,
    ];
    m.extend([
        1.0 - 2f64.powi(-53),
        1.0 - 1e-10,
        1.0,
        1.0 + 2f64.powi(-52),
        1.0 + 1e-10,
        1.5,
        2.0,
        3.0,
        10.0,
        100.0,
        354.0,
        700.0,
        709.0,
        710.0,
        745.0,
        746.0,
        1e3,
        1e8,
        1e15,
        1e17,
        1e100,
        1e154,
        1e300,
    ]);
    for a in m {
        v.push(a);
        v.push(-a);
    }
    v
}

/// The program the app evaluates (its literals as typed).
fn compile(src: &str, unit: TrigUnit) -> Option<(Expr, Program)> {
    let text = format!("y={src}");
    let eq = Equation::parse(&text).ok()?;
    let (_, ast) = eq.explicit()?;
    let opts = CompileOptions {
        trig_unit: unit,
        ..CompileOptions::default()
    };
    Some((ast.clone(), Program::compile_typed(ast, &opts).ok()?))
}

fn within_ulps(a: f64, b: f64, n: f64) -> bool {
    if !(a.is_finite() && b.is_finite()) {
        return false;
    }
    let u = (f64::from_bits(a.abs().max(b.abs()).to_bits() + 1) - a.abs().max(b.abs()))
        .max(f64::from_bits(1));
    (a - b).abs() <= n * u
}

fn shown(v: f64) -> String {
    if v.is_nan() {
        "NaN".into()
    } else if v.is_infinite() {
        if v > 0.0 {
            "∞".into()
        } else {
            "−∞".into()
        }
    } else {
        format_nonzero(v)
    }
}

fn main() {
    let filter = std::env::args().nth(1);
    let xs = args();
    let mut checked = 0usize;
    let mut unknown = 0usize;
    let mut bad = Vec::new();
    for &(l, r, dom, trig) in PAIRS {
        if filter
            .as_deref()
            .is_some_and(|f| !l.contains(f) && !r.contains(f))
        {
            continue;
        }
        let units: &[TrigUnit] = if trig {
            &[TrigUnit::Radians, TrigUnit::Degrees, TrigUnit::Grads]
        } else {
            &[TrigUnit::Radians]
        };
        for &u in units {
            let (Some((la, lp)), Some((ra, rp))) = (compile(l, u), compile(r, u)) else {
                bad.push(format!("{l} = {r} [{u:?}]: does not compile"));
                continue;
            };
            for (src, p) in [(l, &lp), (r, &rp)] {
                let mut out = vec![0.0; xs.len()];
                p.eval_batch(Input::Slice(&xs), Input::Scalar(0.0), &mut out);
                for (&x, &b) in xs.iter().zip(&out) {
                    let s = p.eval(x, 0.0);
                    if s.to_bits() != b.to_bits() && !(s.is_nan() && b.is_nan()) {
                        bad.push(format!("{src} [{u:?}] at {x:e}: scalar {s:e}, batch {b:e}"));
                    }
                }
            }
            for &x in xs.iter().filter(|&&x| dom(x)) {
                checked += 1;
                let (a, b) = (lp.eval(x, 0.0), rp.eval(x, 0.0));
                let (sa, sb) = (shown(a), shown(b));
                // The same text, or within 4 ulps: the identity's own forms
                // round two or three times (√x is rounded before it is
                // squared), which shows only where the panel writes a whole
                // number out in full (999999999999999.9 against 10¹⁵).
                if sa == sb || within_ulps(a, b, 4.0) {
                    continue;
                }
                // One side without a value: fine only if it honestly can't
                // be known.
                let honest =
                    |v: f64, ast: &Expr| v.is_nan() && matches!(reval(ast, x, u), R::Unknown);
                if honest(a, &la) || honest(b, &ra) {
                    unknown += 1;
                    continue;
                }
                bad.push(format!(
                    "{l} = {r} [{u:?}] at {x:e}: {sa} vs {sb} ({a:e} vs {b:e})"
                ));
            }
        }
    }
    println!(
        "{checked} identity checks over {} pairs; {unknown} honestly unknown; {} disagreements",
        PAIRS.len(),
        bad.len()
    );
    for b in &bad {
        println!("  {b}");
    }
    if !bad.is_empty() {
        std::process::exit(1);
    }
}
