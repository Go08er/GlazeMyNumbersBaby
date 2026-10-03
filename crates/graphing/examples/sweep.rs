//! Adversarial invariant sweep over function analysis (not run in CI).
//!
//! `cargo run --release -p graphing --example sweep [-- FILTER] [--all] [--check-ref]`
//!
//! For a set of base functions it analyses shifted, offset, scaled and
//! stretched variants (alone and combined), plus hand-written families
//! (several close centres, mixed frames, discontinuous and aperiodic
//! functions, exactly-zero expressions, underflow and overflow cases) and
//! checks invariants that must hold whatever the function:
//!
//! * internal consistency (a reported minimum lies in the range, 0 is
//!   attained exactly when there are x-intercepts, ...);
//! * agreement with sampling: every claim is tested where it is made
//!   (domain, range with its open and closed ends, monotonicity, parity, the
//!   period over many cycles, asymptotes), and features the analysis didn't
//!   report are looked for (sign changes, exact zeros, poles, local
//!   extrema), each of which must match a distinct reported one;
//! * transformed and equivalent functions get correspondingly transformed
//!   or identical results, matched one to one;
//! * stated mathematical facts for the cases of review 9.
//!
//! The truth is the expression itself, evaluated by an independent reference
//! evaluator over the parsed tree: the same operations as the compiled
//! program, but an intermediate that underflows or overflows a double stays
//! a nonzero, finite number (e^(−800) is not 0, and 1/e^(−800) is defined).
//!
//! Every tolerance is local, and these are all of them:
//!
//! * `noise(x)`: f's change to its nearest other value at least two floats
//!   either side of x (a feature at the neighbouring float is as good as the
//!   floats allow; in a shifted frame f only changes every so many floats),
//!   plus `rounding(x)`, a first-order bound on the rounding of each of f's
//!   operations at x, propagated through the tree (the expanded (x − 1)⁴ is
//!   ±4·10⁻¹⁶ beside its zero though neighbouring floats agree). Values
//!   closer than that can't be told apart.
//! * `resolution(x)`: how far f's argument must move for f to change. Which
//!   side of a domain boundary x is on, and whether a sign change is a
//!   crossing, a pole or a jump, are judged at that scale.
//! * Points within four floats (or the rounding of a periodic family's
//!   copies, |k|·ulp(P)) of a reported end or excluded point: the precision
//!   of that end, not a claim about the point.
//! * `same_shown`: a value that reads the same in the panel's 6 significant
//!   digits is the same claim (the engine rounds values to what it shows).
//!   Only for values; shapes (pieces, open or closed, a point or an interval,
//!   constant or not) and positions get no such allowance.
//! * x + P is rounded and P stands for the true period to half an ulp: the
//!   period check compares f across that reach.
//!
//! Nothing is relative to the function's size elsewhere, to a shift or
//! offset, or to anything the analysis engine estimates.
//!
//! A feature the analysis marks as unknown is never counted as a failure; a
//! definite claim that contradicts the function is. A coverage table says
//! how many features were definite. Checks named `policy-…` are deliberate
//! choices of the calculator (0⁰ = 1, refusing to analyse a function that
//! is nowhere defined) rather than false claims; `unverifiable-…` ones are
//! claims no float can confirm or refute (a zero closer to a pole than f's
//! resolution). Exits with status 1 if any other check fails.

use std::collections::BTreeMap;
use std::f64::consts::{LN_2, LN_10, PI};

use graphing::analysis::format::format_decimal;
use graphing::analysis::{
    AnalysisData, AnalysisError, Family, Interval, KeyGraphFeatures, Monotonicity, Parity,
    Periodicity, analyze, flags,
};
use graphing::ast::{BinOp, Expr, Func};
use graphing::compile::{CompileOptions, DEFAULT_VARIABLE_VALUE};
use graphing::equation::CompiledEquation;
use graphing::functions as fns;
use graphing::{Equation, TrigUnit};

/// Base functions (the right-hand side of `y=`).
const BASES: &[&str] = &[
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
    "tan(x)^2",
    "1/sin(x)",
    "ln(abs(x))",
    "sqrt(1/x)",
    "x^x",
    // Extras.
    "x^-0.5",
    "x^-0.0001",
    "e^(-x^2)",
    "x^3-3x",
    "1/(x^2+1)",
    "sqrt(x^2+1)",
    "abs(x)",
    "x*abs(x)",
    "(x-1)*(x+2)",
    "(x-1)*(x-2)*(x+3)",
    "(x-1)^2*(x+1)",
    "(x-0.5)*(x+0.25)*(x-3)*(x+4)",
    "(x^2-1)*(x^2-4)*(x-0.5)",
    "(x-1)/((x-2)*(x+3))",
    "(x^2-4)/(x-2)",
    "(x+1)/(x^2-1)",
    "x^2/(x^2-1)",
    "(x^3-1)/(x-1)",
    "sin(x)+x/10",
    "ln(x^2+1)",
    "atan(x)+0.000000001",
    "1/(x^2+1)-0.5",
    "x^4-x^2",
    "x*e^x",
    "e^x-x",
    "x^(1/3)",
    "sqrt(4-x^2)",
    "ln(4-x^2)",
    "1/(sqrt(x)-1)",
    "x^2+1",
    "-x^2-1",
    "cos(x)^2",
    "sin(x)^2+1",
    "x+1/x",
    "e^(-x)",
    "2^x",
    "x^5-5x^3+4x",
    // More everyday functions.
    "(x-1)^(1/3)",
    "x^(2/3)",
    "1/(x^2+x+1)",
    "x^2*e^(-x)",
    "sin(x)*e^(-x/10)",
    "ln(x)/x",
    "x^3+x",
    "(x+1)^2/(x-1)",
    "sqrt(x)*ln(x)",
    "e^(1/x)",
    "e^(1/(x-1))",
    "x^2-2x+1",
    "2x+3",
    "x^4-4x^3+6x^2-4x+1",
    "(x^2+1)/(x^2-1)",
    "x/(x^2+1)",
    "atan(ln(abs(x)))",
    "cos(x)+sin(2x)",
    "x-sin(x)",
    "ln(x)^2",
    "sqrt(x^2-1)",
    "1/sqrt(1-x^2)",
    "x^2*ln(x)",
    "e^x/x",
    "x^3-6x^2+11x-6",
    "x^4-5x^2+4",
    "(x-3)^2-4",
    "100x^2",
    "0.01x^2",
    "x^2/100+x",
];

/// Expressions whose inner operations are undefined at known points while
/// an outer one could make IEEE arithmetic finite again, with those points
/// (which must be outside the claimed domain and never be intercepts). The
/// flag marks the calculator's deliberate 0⁰ = 1, under which the listed
/// points are defined after all: disagreements there are policy.
const UNDEFINED: &[(&str, TrigUnit, &[f64], bool)] = &[
    ("x^-1", TrigUnit::Radians, &[0.0], false),
    ("1/(x^-1)", TrigUnit::Radians, &[0.0], false),
    ("1/(1/x)", TrigUnit::Radians, &[0.0], false),
    ("x*(1/x)", TrigUnit::Radians, &[0.0], false),
    ("1^(1/x)", TrigUnit::Radians, &[0.0], false),
    ("(1/x)^0", TrigUnit::Radians, &[0.0], false),
    ("1^(ln(x))", TrigUnit::Radians, &[0.0, -1.0, -5.0], false),
    ("exp(-x^-2)", TrigUnit::Radians, &[0.0], false),
    ("1/asech(x)", TrigUnit::Radians, &[0.0], false),
    ("1/acoth(x)", TrigUnit::Radians, &[1.0, -1.0], false),
    ("exp(-atanh(x))", TrigUnit::Radians, &[1.0, -1.0], false),
    (
        "atan(tan(x))",
        TrigUnit::Degrees,
        &[90.0, -90.0, 270.0],
        false,
    ),
    (
        "atan(tan(x))",
        TrigUnit::Grads,
        &[100.0, -100.0, 300.0],
        false,
    ),
    ("atan(sec(x))", TrigUnit::Degrees, &[90.0, 270.0], false),
    ("atan(csc(x))", TrigUnit::Degrees, &[0.0, 180.0], false),
    ("atan(cot(x))", TrigUnit::Degrees, &[0.0, 180.0], false),
    ("atan(1/(x-x))", TrigUnit::Radians, &[0.0, 1.0, -3.0], false),
    ("ln(x-x)", TrigUnit::Radians, &[0.0, 1.0], false),
    ("e^(-1/abs(x))", TrigUnit::Radians, &[0.0], false),
    ("atan(1/sin(x))", TrigUnit::Radians, &[0.0], false),
    ("1/e^(1/x)", TrigUnit::Radians, &[0.0], false),
    ("(x-x)^0", TrigUnit::Radians, &[0.0, 1.0], true),
    ("atan(x^-2)", TrigUnit::Radians, &[0.0], false),
    ("atan(log(x))", TrigUnit::Radians, &[0.0], false),
    ("atan(ln(abs(x)))", TrigUnit::Radians, &[0.0], false),
    ("0^(-x)", TrigUnit::Radians, &[0.0, 1.0, 2.5], true),
    ("root(x,-2)", TrigUnit::Radians, &[0.0], false),
    ("1/root(x,-2)", TrigUnit::Radians, &[0.0], false),
    ("x^(-1/3)", TrigUnit::Radians, &[0.0], false),
    ("1/x^(-1/3)", TrigUnit::Radians, &[0.0], false),
    ("e^(1/(x-1))", TrigUnit::Radians, &[1.0], false),
    ("atan(1/(x^2-1))", TrigUnit::Radians, &[1.0, -1.0], false),
];

/// A hand-written case: the expression and where to sample densely (its
/// features sit near these centres). `wide` adds a long fine grid, for
/// bounded aperiodic sums whose extremes lie far out.
struct Special {
    expr: String,
    centres: Vec<f64>,
    wide: bool,
}

fn special(expr: impl Into<String>, centres: &[f64]) -> Special {
    Special {
        expr: expr.into(),
        centres: centres.to_vec(),
        wide: false,
    }
}

/// The hand-written families.
fn specials() -> Vec<Special> {
    let mut v = Vec::new();
    // Several distinct close centres, as products, nested products (the
    // same function: see `EQUIVALENT`), quotients and triples.
    for a in [1e6, 1e9, 1e12] {
        for d in [0.0625, 1.0, 1000.0] {
            let (b, c) = (a + d, a + 2.0 * d);
            let cs = [a, b, c];
            v.push(special(format!("(x-{a})*(x-{b})"), &cs));
            v.push(special(format!("(x-{a})*((x-{a})-{d})"), &cs));
            v.push(special(format!("(x-{a})/(x-{b})"), &cs));
            v.push(special(format!("(x-{b})/(x-{a})^2"), &cs));
            v.push(special(format!("(x-{a})*(x-{b})*(x-{c})"), &cs));
            v.push(special(format!("(x-{a})^2*(x-{b})"), &cs));
        }
    }
    // Two frames at once: a far centre and a slow quadratic about 0.
    for (a, s) in [(1e6, 1e9), (1e12, 1e15), (1e6, 1e3), (1e9, 1e9)] {
        for c in ["0.0000001", "1"] {
            let m = a * s * s / (s * s + 1.0);
            v.push(special(format!("(x-{a})^2+{c}+(x/{s})^2"), &[a, m, 0.0]));
        }
    }
    v.push(special("(x-1000000)^2+(x-1000001)^2", &[1e6, 1e6 + 1.0]));
    v.push(special("(x-1000000000)^2*(x+1000000000)^2", &[1e9, -1e9]));
    v.push(special(
        "(x-1000000000000)^2+0.0000001+((x-1000)/1000)^2",
        &[1e12, 1000.0],
    ));
    // Discontinuous functions of trig, with offsets and shifts.
    for e in [
        "floor(sin(x))",
        "ceil(sin(x))",
        "floor(cos(x))",
        "ceil(cos(x))",
        "round(sin(x))",
        "sign(sin(x))",
        "floor(2*sin(x))",
        "floor(sin(x))+0.5",
        "atan(tan(x))",
        "atan(tan(x))+1",
        "atan(tan(x))+10",
        "atan(tan(x))-1000000",
        "atan(tan(x-1000))",
        "floor(cos(x-1000000))",
        "x-floor(x)",
        "floor(x)",
        "floor(x)-0.5",
        "mod(x,1)",
        "1/(1+cos(x))",
    ] {
        v.push(special(e, &[0.0]));
    }
    // Bounded, not periodic.
    for e in [
        "sin(x)+sin(sqrt(2)*x)",
        "sin(x)*sin(sqrt(3)*x)",
        "cos(x)+cos(pi*x)",
    ] {
        v.push(Special {
            expr: e.into(),
            centres: vec![0.0],
            wide: true,
        });
    }
    // Exactly zero, everywhere or where defined.
    for e in [
        "0*x",
        "x-x",
        "sin(x)-sin(x)",
        "0*e^x",
        "(x-x)*x^2",
        "0/x",
        "0/x^2",
        "x^2/x^2",
    ] {
        v.push(special(e, &[0.0]));
    }
    // Small variation on a large value, and a tiny periodic difference.
    for e in [
        "sin(x)+10000000000000",
        "cos(x)+10000000000000",
        "sin(x)+1000000000000",
        "sin(x)-sin(x+0.000000001)",
        "tan(x)+1000000",
        "sin(x)/1000000000000+1",
    ] {
        v.push(special(e, &[0.0]));
    }
    // Underflow and overflow of intermediates (review 9, R9-M-04) and
    // everyday compositions that should be easy.
    for e in [
        "exp(x)^-1",
        "1/exp(x)",
        "atan(exp(x)^-1)",
        "atan(1/exp(x))",
        "0/exp(1/x)",
        "0/exp(x)",
        "exp(x)/exp(x)",
        "atan(1/exp(-1000))",
        "atan(1/exp(-1000))+x",
        "ln(exp(x))",
        "ln(1+exp(x))",
        "exp(x)^2/exp(x)",
        "1/(1+exp(-x))",
        "x^(1/10)+1",
    ] {
        v.push(special(e, &[0.0]));
    }
    v
}

/// Pairs of expressions for the same function: every definite result must
/// agree.
fn equivalent() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = Vec::new();
    for a in [1e6, 1e9, 1e12] {
        for d in [0.0625, 1.0, 1000.0] {
            let b = a + d;
            v.push((format!("(x-{a})*(x-{b})"), format!("(x-{a})*((x-{a})-{d})")));
        }
    }
    for (p, q) in [
        ("exp(x)^-1", "1/exp(x)"),
        ("atan(exp(x)^-1)", "atan(1/exp(x))"),
        ("ln(exp(x))", "x"),
        ("exp(x)/exp(x)", "1"),
        ("0/exp(x)", "0"),
        ("0*x", "0"),
        ("x-x", "0"),
        ("sin(x)-sin(x)", "0"),
        ("0*e^x", "0"),
        ("x^2/x^2", "x/x"),
        ("0/x^2", "0/x"),
        ("exp(x)^2/exp(x)", "exp(x)"),
        ("1/(1+exp(-x))", "exp(x)/(exp(x)+1)"),
    ] {
        v.push((p.into(), q.into()));
    }
    v
}

/// What the x-intercepts are, as a fact.
enum Zs {
    /// None at all.
    No,
    /// Every x where f is defined.
    All,
    /// Exactly these.
    At(Vec<f64>),
}

/// Mathematical facts about a function, stated independently of the
/// engine. Only definite claims are compared.
struct Known {
    expr: &'static str,
    /// The domain is ℝ without these points (nonperiodic claims only).
    domain_except: Option<Vec<f64>>,
    /// The range: (lo, lo closed, hi, hi closed) intervals.
    range: Option<Vec<(f64, bool, f64, bool)>>,
    /// `Some(None)`: not periodic; `Some(Some(p))`: fundamental period p.
    period: Option<Option<f64>>,
    zeros: Option<Zs>,
    /// No vertical asymptotes.
    no_poles: bool,
    /// The horizontal asymptotes' values, if stated.
    hasymptotes: Option<Vec<f64>>,
}

fn known(expr: &'static str) -> Known {
    Known {
        expr,
        domain_except: None,
        range: None,
        period: None,
        zeros: None,
        no_poles: false,
        hasymptotes: None,
    }
}

fn known_cases() -> Vec<Known> {
    let inf = f64::INFINITY;
    let all = (-inf, false, inf, false);
    let pt = |v: f64| (v, true, v, true);
    let (a12, s9, s15) = (1e12, 1e9, 1e15);
    // Minimum of (x − A)² + c + (x/S)²: A²/(S² + 1) + c.
    let mixed = |a: f64, s: f64, c: f64| a * a / (s * s + 1.0) + c;
    vec![
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![(-1.0 / 1024.0, true, inf, false)]),
            zeros: Some(Zs::At(vec![a12, a12 + 0.0625])),
            ..known("(x-1000000000000)*(x-1000000000000.0625)")
        },
        Known {
            domain_except: Some(vec![a12 + 0.0625]),
            range: Some(vec![(-inf, false, 1.0, false), (1.0, false, inf, false)]),
            zeros: Some(Zs::At(vec![a12])),
            ..known("(x-1000000000000)/(x-1000000000000.0625)")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![(-0.25, true, inf, false)]),
            zeros: Some(Zs::At(vec![a12, a12 + 1.0])),
            ..known("(x-1000000000000)*(x-1000000000001)")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![(mixed(1e6, s9, 1e-7), true, inf, false)]),
            zeros: Some(Zs::No),
            ..known("(x-1000000)^2+0.0000001+(x/1000000000)^2")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![(mixed(a12, s15, 1e-7), true, inf, false)]),
            zeros: Some(Zs::No),
            ..known("(x-1000000000000)^2+0.0000001+(x/1000000000000000)^2")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![(-2.0, false, 2.0, false)]),
            period: Some(None),
            ..known("sin(x)+sin(sqrt(2)*x)")
        },
        Known {
            range: Some(vec![(-PI / 2.0, false, PI / 2.0, false)]),
            period: Some(Some(PI)),
            ..known("atan(tan(x))")
        },
        Known {
            range: Some(vec![(1.0 - PI / 2.0, false, 1.0 + PI / 2.0, false)]),
            period: Some(Some(PI)),
            zeros: Some(Zs::No),
            ..known("atan(tan(x))+1")
        },
        Known {
            range: Some(vec![(10.0 - PI / 2.0, false, 10.0 + PI / 2.0, false)]),
            period: Some(Some(PI)),
            zeros: Some(Zs::No),
            ..known("atan(tan(x))+10")
        },
        Known {
            range: Some(vec![pt(-1.0), pt(0.0), pt(1.0)]),
            period: Some(Some(2.0 * PI)),
            ..known("floor(sin(x))")
        },
        Known {
            range: Some(vec![pt(-1.0), pt(0.0), pt(1.0)]),
            period: Some(Some(2.0 * PI)),
            ..known("ceil(sin(x))")
        },
        Known {
            range: Some(vec![pt(-1.0), pt(0.0), pt(1.0)]),
            period: Some(Some(2.0 * PI)),
            ..known("floor(cos(x))")
        },
        Known {
            range: Some(vec![pt(-1.0), pt(0.0), pt(1.0)]),
            period: Some(Some(2.0 * PI)),
            ..known("ceil(cos(x))")
        },
        Known {
            range: Some(vec![(0.5, true, inf, false)]),
            period: Some(Some(2.0 * PI)),
            zeros: Some(Zs::No),
            ..known("1/(1+cos(x))")
        },
        Known {
            range: Some(vec![(1e13 - 1.0, true, 1e13 + 1.0, true)]),
            period: Some(Some(2.0 * PI)),
            ..known("sin(x)+10000000000000")
        },
        Known {
            range: Some(vec![(1e13 - 1.0, true, 1e13 + 1.0, true)]),
            period: Some(Some(2.0 * PI)),
            ..known("cos(x)+10000000000000")
        },
        Known {
            range: Some(vec![(1e12 - 1.0, true, 1e12 + 1.0, true)]),
            period: Some(Some(2.0 * PI)),
            ..known("sin(x)+1000000000000")
        },
        Known {
            range: Some(vec![(
                -2.0 * 5e-10f64.sin(),
                true,
                2.0 * 5e-10f64.sin(),
                true,
            )]),
            period: Some(Some(2.0 * PI)),
            hasymptotes: Some(vec![]),
            ..known("sin(x)-sin(x+0.000000001)")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![(0.0, false, inf, false)]),
            no_poles: true,
            zeros: Some(Zs::No),
            ..known("exp(x)^-1")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![(0.0, false, PI / 2.0, false)]),
            no_poles: true,
            zeros: Some(Zs::No),
            hasymptotes: Some(vec![0.0, PI / 2.0]),
            ..known("atan(exp(x)^-1)")
        },
        Known {
            domain_except: Some(vec![0.0]),
            range: Some(vec![pt(0.0)]),
            zeros: Some(Zs::All),
            ..known("0/exp(1/x)")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![pt(0.0)]),
            zeros: Some(Zs::All),
            ..known("0/exp(x)")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![pt(1.0)]),
            zeros: Some(Zs::No),
            ..known("exp(x)/exp(x)")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![pt(PI / 2.0)]),
            zeros: Some(Zs::No),
            ..known("atan(1/exp(-1000))")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![all]),
            ..known("atan(1/exp(-1000))+x")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![all]),
            zeros: Some(Zs::At(vec![0.0])),
            ..known("ln(exp(x))")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![(0.0, false, inf, false)]),
            zeros: Some(Zs::No),
            ..known("ln(1+exp(x))")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![pt(0.0)]),
            zeros: Some(Zs::All),
            ..known("0*x")
        },
        Known {
            domain_except: Some(vec![]),
            range: Some(vec![pt(0.0)]),
            zeros: Some(Zs::All),
            ..known("x-x")
        },
        Known {
            domain_except: Some(vec![0.0]),
            range: Some(vec![pt(1.0)]),
            zeros: Some(Zs::No),
            ..known("x^2/x^2")
        },
        Known {
            domain_except: Some(vec![0.0]),
            range: Some(vec![pt(0.0)]),
            zeros: Some(Zs::All),
            ..known("0/x^2")
        },
        Known {
            domain_except: Some(vec![]),
            zeros: Some(Zs::All),
            ..known("0*e^x")
        },
    ]
}

// ---------------------------------------------------------------------------
// Floats.

/// Floats in order as integers (−0 and +0 coincide).
fn to_ord(x: f64) -> i64 {
    let b = x.to_bits() as i64;
    if b < 0 { -(b & i64::MAX) } else { b }
}

fn from_ord(o: i64) -> f64 {
    if o < 0 {
        f64::from_bits(o.unsigned_abs() | (1 << 63))
    } else {
        f64::from_bits(o as u64)
    }
}

/// The float `k` representable steps from x.
fn nudge(x: f64, k: i64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let y = from_ord(to_ord(x).saturating_add(k));
    if y.is_finite() { y } else { x }
}

/// The spacing of the floats at x.
fn ulp(x: f64) -> f64 {
    let a = x.abs();
    if !a.is_finite() {
        return f64::INFINITY;
    }
    let n = f64::from_bits(a.to_bits() + 1);
    if n.is_finite() {
        n - a
    } else {
        a - f64::from_bits(a.to_bits() - 1)
    }
}

/// Floats strictly between a and b, roughly (saturating).
fn floats_between(a: f64, b: f64) -> i64 {
    (to_ord(b) as i128 - to_ord(a) as i128)
        .unsigned_abs()
        .min(i64::MAX as u128) as i64
}

// ---------------------------------------------------------------------------
// The reference evaluator.

/// m · 2ᵉ with ½ ≤ |m| < 1 (or m = 0): a double with an unbounded exponent.
#[derive(Clone, Copy, Debug)]
struct Xf {
    m: f64,
    e: i64,
}

fn frexp(v: f64) -> (f64, i64) {
    if v == 0.0 || !v.is_finite() {
        return (v, 0);
    }
    let bits = v.to_bits();
    let ex = ((bits >> 52) & 0x7ff) as i64;
    if ex == 0 {
        let (m, e) = frexp(v * 18446744073709551616.0);
        return (m, e - 64);
    }
    (
        f64::from_bits((bits & !(0x7ffu64 << 52)) | (1022u64 << 52)),
        ex - 1022,
    )
}

fn ldexp(m: f64, e: i64) -> f64 {
    if m == 0.0 {
        return m;
    }
    if e > 2200 {
        return m.signum() * f64::INFINITY;
    }
    if e < -2200 {
        return m.signum() * 0.0;
    }
    let e = e as i32;
    let h = e / 2;
    m * 2f64.powi(h) * 2f64.powi(e - h)
}

impl Xf {
    const ZERO: Xf = Xf { m: 0.0, e: 0 };

    fn of(v: f64) -> Xf {
        let (m, e) = frexp(v);
        Xf { m, e }
    }
    fn norm(m: f64, e: i64) -> Xf {
        let (mm, ee) = frexp(m);
        Xf {
            m: mm,
            e: if mm == 0.0 { 0 } else { e + ee },
        }
    }
    fn f(self) -> f64 {
        ldexp(self.m, self.e)
    }
    fn is_zero(self) -> bool {
        self.m == 0.0
    }
    fn sign(self) -> f64 {
        if self.m == 0.0 { 0.0 } else { self.m.signum() }
    }
    /// Exactly a double (subnormals included): the compiled program's
    /// arithmetic applies, rounding and all.
    fn normal(self) -> bool {
        if self.m == 0.0 {
            return true;
        }
        let f = self.f();
        f.is_finite() && f != 0.0 && {
            let b = Xf::of(f);
            b.m == self.m && b.e == self.e
        }
    }
    /// Nonzero but below the doubles (or between subnormals).
    fn tiny(self) -> bool {
        self.m != 0.0 && self.e < 0 && !self.normal()
    }
    /// Beyond the largest double.
    fn huge(self) -> bool {
        self.e > 1024
    }
    fn neg(self) -> Xf {
        Xf {
            m: -self.m,
            e: self.e,
        }
    }
    fn abs(self) -> Xf {
        Xf {
            m: self.m.abs(),
            e: self.e,
        }
    }
    fn mul(self, o: Xf) -> Xf {
        Xf::norm(self.m * o.m, self.e + o.e)
    }
    fn div(self, o: Xf) -> Xf {
        Xf::norm(self.m / o.m, self.e - o.e)
    }
    fn add(self, o: Xf) -> Xf {
        if self.m == 0.0 {
            return o;
        }
        if o.m == 0.0 {
            return self;
        }
        let (a, b) = if self.e >= o.e { (self, o) } else { (o, self) };
        let d = a.e - b.e;
        if d > 1100 {
            return a;
        }
        Xf::norm(a.m + ldexp(b.m, -d), a.e)
    }
    /// ln of a positive number.
    fn ln(self) -> f64 {
        self.m.ln() + self.e as f64 * LN_2
    }
    fn exp(t: f64) -> R {
        if t.is_nan() {
            return R::Undef;
        }
        if (-700.0..=700.0).contains(&t) {
            return R::V(Xf::of(t.exp()));
        }
        let k = (t / LN_2).floor();
        if !k.is_finite() || k.abs() > 4e18 {
            return R::Unknown;
        }
        R::V(Xf::norm((t - k * LN_2).exp(), k as i64))
    }
    /// A double standing in for it: itself, or the smallest or largest
    /// double of its sign when it is beyond them.
    fn proxy(self) -> f64 {
        if self.normal() {
            return self.f();
        }
        if self.e < 0 {
            let f = self.f();
            return if f == 0.0 { self.sign() * 5e-324 } else { f };
        }
        self.sign() * f64::MAX
    }
}

/// The value of an expression at a point: a number (possibly beyond the
/// doubles), undefined there, or not something the reference can tell.
#[derive(Clone, Copy, Debug)]
enum R {
    V(Xf),
    Undef,
    Unknown,
}

/// An exponent written as an integer or a ratio of integers, as the
/// compiler reads it (`syntactic_rational`): real-root semantics apply.
fn rational(e: &Expr) -> Option<(i32, i32)> {
    fn int(e: &Expr) -> Option<i64> {
        match e {
            Expr::Num(v) if *v == v.trunc() && v.abs() < 1e6 => Some(*v as i64),
            Expr::Neg(a) => int(a).map(|v| -v),
            _ => None,
        }
    }
    let (p, q) = match e {
        Expr::Neg(a) => {
            let (p, q) = rational(a)?;
            return Some((-p, q));
        }
        Expr::Bin(BinOp::Div, a, b) => (int(a)?, int(b)?),
        _ => (int(e)?, 1),
    };
    if q == 0 {
        return None;
    }
    let mut g = (p.unsigned_abs(), q.unsigned_abs());
    while g.1 != 0 {
        g = (g.1, g.0 % g.1);
    }
    let g = g.0.max(1) as i64;
    let (mut p, mut q) = (p / g, q / g);
    if q < 0 {
        p = -p;
        q = -q;
    }
    Some((i32::try_from(p).ok()?, i32::try_from(q).ok()?))
}

fn one(r: f64) -> R {
    if r.is_nan() {
        R::Undef
    } else if r.is_infinite() {
        R::Unknown
    } else {
        R::V(Xf::of(r))
    }
}

fn reval(e: &Expr, x: f64, u: TrigUnit) -> R {
    match e {
        Expr::Num(v) => R::V(Xf::of(*v)),
        Expr::Const(c) => R::V(Xf::of(c.value())),
        Expr::X => R::V(Xf::of(x)),
        Expr::Y => R::Unknown,
        Expr::Var(_) => R::V(Xf::of(DEFAULT_VARIABLE_VALUE)),
        Expr::Degrees(a) => reval(a, x, u),
        Expr::Neg(a) => match reval(a, x, u) {
            R::V(v) => R::V(v.neg()),
            r => r,
        },
        Expr::Bin(BinOp::Pow, a, b) if let Some((p, q)) = rational(b) => {
            pow_rat(reval(a, x, u), p, q)
        }
        Expr::Bin(op, a, b) => match (reval(a, x, u), reval(b, x, u)) {
            (R::Undef, _) | (_, R::Undef) => R::Undef,
            (R::V(va), R::V(vb)) => bin(*op, va, vb),
            _ => R::Unknown,
        },
        Expr::Call(f, args) => {
            let mut vs = Vec::with_capacity(args.len());
            let mut unknown = false;
            for a in args {
                match reval(a, x, u) {
                    R::Undef => return R::Undef,
                    R::Unknown => unknown = true,
                    R::V(v) => vs.push(v),
                }
            }
            if unknown {
                R::Unknown
            } else {
                call(*f, &vs, u)
            }
        }
    }
}

fn bin(op: BinOp, a: Xf, b: Xf) -> R {
    match op {
        BinOp::Pow => return pow_real(a, b),
        BinOp::Div if b.is_zero() => return R::Undef,
        _ => {}
    }
    if a.normal() && b.normal() {
        let (x, y) = (a.f(), b.f());
        let r = match op {
            BinOp::Add => x + y,
            BinOp::Sub => x - y,
            BinOp::Mul => x * y,
            _ => x / y,
        };
        let exact0 = r == 0.0 && (matches!(op, BinOp::Add | BinOp::Sub) || x == 0.0 || y == 0.0);
        if r.is_finite() && (r != 0.0 || exact0) {
            return R::V(Xf::of(r));
        }
    }
    R::V(match op {
        BinOp::Add => a.add(b),
        BinOp::Sub => a.add(b.neg()),
        BinOp::Mul => a.mul(b),
        _ => a.div(b),
    })
}

/// b^(p/q), with the compiler's integer and real-root powers.
fn pow_rat(b: R, p: i32, q: i32) -> R {
    let b = match b {
        R::V(b) => b,
        r => return r,
    };
    if b.is_zero() {
        return match p {
            ..0 => R::Undef,
            0 => R::V(Xf::of(1.0)),
            _ => R::V(Xf::ZERO),
        };
    }
    if b.normal() {
        let bf = b.f();
        let r = if q == 1 {
            fns::pow_int(bf, p)
        } else {
            fns::pow_rational(bf, p, q)
        };
        if r.is_nan() {
            return R::Undef;
        }
        if r.is_finite() && r != 0.0 {
            return R::V(Xf::of(r));
        }
    }
    if b.sign() < 0.0 && q % 2 == 0 {
        return R::Undef;
    }
    let t = (p as f64 / q as f64) * b.abs().ln();
    match Xf::exp(t) {
        R::V(m) if b.sign() < 0.0 && p % 2 != 0 => R::V(m.neg()),
        r => r,
    }
}

/// a^b for a computed exponent (`fns::pow`, so 0⁰ = 1).
fn pow_real(a: Xf, b: Xf) -> R {
    if a.normal() && b.normal() {
        let (x, y) = (a.f(), b.f());
        let r = fns::pow(x, y);
        if r.is_nan() {
            return R::Undef;
        }
        if r.is_finite() && (r != 0.0 || x == 0.0) {
            return R::V(Xf::of(r));
        }
    }
    if a.is_zero() {
        return if b.sign() < 0.0 {
            R::Undef
        } else if b.is_zero() {
            R::V(Xf::of(1.0))
        } else {
            R::V(Xf::ZERO)
        };
    }
    let y = b.f();
    if !y.is_finite() {
        return R::Unknown;
    }
    if a.sign() < 0.0 {
        if y != y.trunc() {
            return R::Undef;
        }
        let odd = y.abs() < 9007199254740992.0 && y % 2.0 != 0.0;
        return match Xf::exp(y * a.abs().ln()) {
            R::V(m) if odd => R::V(m.neg()),
            r => r,
        };
    }
    Xf::exp(y * a.ln())
}

fn root_x(x: Xf, n: Xf) -> R {
    if x.normal() && n.normal() {
        let r = fns::root(x.f(), n.f());
        if r.is_nan() {
            return R::Undef;
        }
        if r.is_finite() && (r != 0.0 || x.is_zero()) {
            return R::V(Xf::of(r));
        }
    }
    let nf = n.f();
    if nf == 0.0 || !nf.is_finite() {
        return R::Undef;
    }
    if x.is_zero() {
        return if nf < 0.0 { R::Undef } else { R::V(Xf::ZERO) };
    }
    let odd = nf.fract() == 0.0 && nf.abs() < 9e15 && (nf % 2.0).abs() == 1.0;
    if x.sign() < 0.0 && !odd {
        return R::Undef;
    }
    match Xf::exp(x.abs().ln() / nf) {
        R::V(m) if x.sign() < 0.0 => R::V(m.neg()),
        r => r,
    }
}

fn trig_factor(u: TrigUnit) -> f64 {
    u.to_radians_factor()
}

fn call(f: Func, v: &[Xf], u: TrigUnit) -> R {
    use Func::*;
    let a = v[0];
    let c = trig_factor(u);
    match f {
        Exp => {
            let t = a.f();
            if t.is_finite() {
                Xf::exp(t)
            } else {
                R::Unknown
            }
        }
        Ln | Log => {
            if a.sign() <= 0.0 {
                return R::Undef;
            }
            if a.normal() {
                return one(if f == Ln {
                    fns::ln(a.f())
                } else {
                    fns::log10(a.f())
                });
            }
            let l = a.ln();
            R::V(Xf::of(if f == Ln { l } else { l / LN_10 }))
        }
        Sqrt => {
            if a.sign() < 0.0 {
                R::Undef
            } else if a.normal() {
                one(a.f().sqrt())
            } else {
                let (m, e) = if a.e % 2 != 0 {
                    (a.m * 2.0, a.e - 1)
                } else {
                    (a.m, a.e)
                };
                R::V(Xf::norm(m.sqrt(), e / 2))
            }
        }
        Cbrt => {
            if a.normal() {
                one(a.f().cbrt())
            } else {
                let r = a.e.rem_euclid(3);
                R::V(Xf::norm((a.m * 2f64.powi(r as i32)).cbrt(), (a.e - r) / 3))
            }
        }
        Abs => R::V(a.abs()),
        Sign => R::V(Xf::of(a.sign())),
        Floor | Ceil | Round => {
            if a.normal() {
                let t = a.f();
                return one(match f {
                    Floor => t.floor(),
                    Ceil => t.ceil(),
                    _ => fns::round(t),
                });
            }
            if a.huge() {
                return R::V(a);
            }
            R::V(Xf::of(match f {
                Floor if a.sign() < 0.0 => -1.0,
                Ceil if a.sign() > 0.0 => 1.0,
                _ => 0.0,
            }))
        }
        Sin | Tan | Cos | Sec | Csc | Cot => {
            if a.huge() {
                return R::Unknown;
            }
            if a.tiny() {
                let t = a.mul(Xf::of(c));
                return R::V(match f {
                    Sin | Tan => t,
                    Cos | Sec => Xf::of(1.0),
                    _ => Xf::of(1.0).div(t),
                });
            }
            let t = a.f();
            one(match f {
                Sin => fns::sin_u(t, u),
                Cos => fns::cos_u(t, u),
                Tan => fns::tan_u(t, u),
                Sec => fns::sec_u(t, u),
                Csc => fns::csc_u(t, u),
                _ => fns::cot_u(t, u),
            })
        }
        Sinh | Cosh | Tanh => {
            if a.tiny() {
                return R::V(if f == Cosh { Xf::of(1.0) } else { a });
            }
            let t = a.f();
            if t.abs() > 700.0 {
                if f == Tanh {
                    return R::V(Xf::of(a.sign()));
                }
                if !t.is_finite() {
                    return R::Unknown;
                }
                return match Xf::exp(t.abs()) {
                    R::V(m) => {
                        let h = Xf { m: m.m, e: m.e - 1 };
                        R::V(if f == Sinh && t < 0.0 { h.neg() } else { h })
                    }
                    r => r,
                };
            }
            one(match f {
                Sinh => t.sinh(),
                Cosh => t.cosh(),
                _ => t.tanh(),
            })
        }
        Sech | Csch => {
            if f == Csch && a.is_zero() {
                return R::Undef;
            }
            if a.tiny() {
                return R::V(if f == Sech {
                    Xf::of(1.0)
                } else {
                    Xf::of(1.0).div(a)
                });
            }
            let t = a.f();
            if t.abs() > 700.0 {
                if !t.is_finite() {
                    return R::Unknown;
                }
                return match Xf::exp(-t.abs()) {
                    R::V(m) => {
                        let d = Xf { m: m.m, e: m.e + 1 };
                        R::V(if f == Csch && t < 0.0 { d.neg() } else { d })
                    }
                    r => r,
                };
            }
            one(if f == Sech {
                fns::sech(t)
            } else {
                fns::csch(t)
            })
        }
        Coth => {
            if a.is_zero() {
                R::Undef
            } else if a.tiny() {
                R::V(Xf::of(1.0).div(a))
            } else {
                one(fns::coth(a.proxy()))
            }
        }
        Asinh | Acosh if a.huge() => {
            if f == Acosh && a.sign() < 0.0 {
                return R::Undef;
            }
            let l = LN_2 + a.abs().ln();
            R::V(Xf::of(if a.sign() < 0.0 { -l } else { l }))
        }
        Asin | Acos | Atan | Asec | Acsc | Acot | Asinh | Acosh | Atanh | Asech | Acsch | Acoth => {
            let p = a.proxy();
            one(match f {
                Asin => fns::asin_u(p, u),
                Acos => fns::acos_u(p, u),
                Atan => fns::atan_u(p, u),
                Asec => fns::asec_u(p, u),
                Acsc => fns::acsc_u(p, u),
                Acot => fns::acot_u(p, u),
                Asinh => p.asinh(),
                Acosh => p.acosh(),
                Atanh => fns::atanh(p),
                Asech => fns::asech(p),
                Acsch => fns::acsch(p),
                _ => fns::acoth(p),
            })
        }
        Factorial | DoubleFactorial => {
            if !a.normal() {
                return R::Unknown;
            }
            one(if f == Factorial {
                fns::factorial(a.f())
            } else {
                fns::double_factorial(a.f())
            })
        }
        Root => root_x(a, v[1]),
        LogBase => {
            let (b, t) = (a, v[1]);
            if b.normal() && t.normal() {
                one(fns::log_base(b.f(), t.f()))
            } else if b.sign() <= 0.0 || t.sign() <= 0.0 || (b.normal() && b.f() == 1.0) {
                R::Undef
            } else {
                R::V(Xf::of(t.ln() / b.ln()))
            }
        }
        Mod | NCr | NPr => {
            if !v.iter().all(|t| t.normal()) {
                return R::Unknown;
            }
            let (p, q) = (a.f(), v[1].f());
            one(match f {
                Mod => fns::modulo(p, q),
                NCr => fns::ncr(p, q),
                _ => fns::npr(p, q),
            })
        }
        Min | Max => {
            let mut best = v[0];
            for &t in &v[1..] {
                let (bp, tp) = (best.proxy(), t.proxy());
                if (f == Min && tp < bp) || (f == Max && tp > bp) {
                    best = t;
                }
            }
            R::V(best)
        }
    }
}

// ---------------------------------------------------------------------------
// Rounding bound.

/// The exact rounding error of the double sum s = a ⊕ b (Knuth's TwoSum).
fn two_sum_err(a: f64, b: f64, s: f64) -> f64 {
    let bv = s - a;
    let av = s - bv;
    ((a - av) + (b - bv)).abs()
}

/// g's change when its argument a moves by up to ea. a ± ea can round
/// back to a when ea is below a's own spacing (x − 1000 is off by
/// 5·10⁻¹⁴ but x ± that is x): then step a few floats and scale, which is
/// g′·ea for a smooth g. A jump (floor) is taken whole if within reach.
fn change(g: &dyn Fn(f64) -> f64, a: f64, ea: f64, r: f64, jumps: bool) -> f64 {
    if ea == 0.0 {
        return 0.0;
    }
    let h = ea.max(2.0 * ulp(a));
    let mut d = 0.0f64;
    for t in [a - h, a + h] {
        let v = g(t);
        if !v.is_finite() {
            return f64::INFINITY;
        }
        d = d.max((v - r).abs());
    }
    if jumps {
        return d;
    }
    // (An uncertain argument never makes a smooth result certain: the
    // change can underflow, (x − 1) + 1 squared beside 0, but is not 0.)
    let s = d * (ea / h);
    if s == 0.0 && (d > 0.0 || r == 0.0) {
        f64::from_bits(1)
    } else {
        s
    }
}

/// g(a) where a is known to within ea, and how far the double result can
/// be from g at the exact a: g's change across [a − ea, a + ea] plus the
/// rounding of g itself (k units in the last place, none if `exact`).
fn unary_err(
    g: &dyn Fn(f64) -> f64,
    a: f64,
    ea: f64,
    k: f64,
    exact: bool,
    jumps: bool,
) -> (f64, f64) {
    let r = g(a);
    if !r.is_finite() || !ea.is_finite() {
        return (r, f64::INFINITY);
    }
    // (Never rounded away: half the least subnormal is 0.)
    let round = if exact {
        0.0
    } else {
        (k * ulp(r)).max(f64::from_bits(1))
    };
    (r, change(g, a, ea, r, jumps) + round)
}

fn binary_err(
    g: &dyn Fn(f64, f64) -> f64,
    (a, ea): (f64, f64),
    (b, eb): (f64, f64),
    k: f64,
) -> (f64, f64) {
    let r = g(a, b);
    if !r.is_finite() || !ea.is_finite() || !eb.is_finite() {
        return (r, f64::INFINITY);
    }
    let d = change(&|s| g(s, b), a, ea, r, false) + change(&|t| g(a, t), b, eb, r, false);
    (
        r,
        d + (k * ulp(r)).max(if k > 0.0 { f64::from_bits(1) } else { 0.0 }),
    )
}

/// The compiled program's unary function (`compile::Fn1::apply`).
fn fn1(f: Func, u: TrigUnit) -> impl Fn(f64) -> f64 {
    use Func::*;
    move |x: f64| match f {
        Sin => fns::sin_u(x, u),
        Cos => fns::cos_u(x, u),
        Tan => fns::tan_u(x, u),
        Sec => fns::sec_u(x, u),
        Csc => fns::csc_u(x, u),
        Cot => fns::cot_u(x, u),
        Asin => fns::asin_u(x, u),
        Acos => fns::acos_u(x, u),
        Atan => fns::atan_u(x, u),
        Asec => fns::asec_u(x, u),
        Acsc => fns::acsc_u(x, u),
        Acot => fns::acot_u(x, u),
        Sinh => x.sinh(),
        Cosh => x.cosh(),
        Tanh => x.tanh(),
        Sech => fns::sech(x),
        Csch => fns::csch(x),
        Coth => fns::coth(x),
        Asinh => x.asinh(),
        Acosh => x.acosh(),
        Atanh => fns::atanh(x),
        Asech => fns::asech(x),
        Acsch => fns::acsch(x),
        Acoth => fns::acoth(x),
        Sqrt => x.sqrt(),
        Cbrt => x.cbrt(),
        Log => fns::log10(x),
        Ln => fns::ln(x),
        Exp => x.exp(),
        Abs => x.abs(),
        Floor => x.floor(),
        Ceil => x.ceil(),
        Round => fns::round(x),
        Sign => fns::sign(x),
        Factorial => fns::factorial(x),
        DoubleFactorial => fns::double_factorial(x),
        _ => f64::NAN,
    }
}

fn fn2(f: Func) -> impl Fn(f64, f64) -> f64 {
    move |a: f64, b: f64| match f {
        Func::Root => fns::root(a, b),
        Func::LogBase => fns::log_base(a, b),
        Func::Mod => fns::modulo(a, b),
        Func::NCr => fns::ncr(a, b),
        Func::NPr => fns::npr(a, b),
        Func::Min if !a.is_nan() && !b.is_nan() => a.min(b),
        Func::Max if !a.is_nan() && !b.is_nan() => a.max(b),
        _ => f64::NAN,
    }
}

/// Is the library result exact here (no rounding at all)? Special values
/// the functions return exactly (sin 0, cos 0, ln 1, quarter turns in
/// degrees and grads) and the functions that never round.
fn exact_at(f: Func, a: f64, r: f64, u: TrigUnit) -> bool {
    use Func::*;
    match f {
        Abs | Floor | Ceil | Round | Sign => true,
        Sqrt => r.mul_add(r, -a) == 0.0,
        Cbrt => r * r * r == a,
        Sin | Tan | Cos | Sec | Csc | Cot if u != TrigUnit::Radians => {
            a.rem_euclid(u.full_turn() / 4.0) == 0.0
        }
        Sin | Tan | Asin | Atan | Sinh | Tanh | Asinh | Atanh => a == 0.0 && r == 0.0,
        Cos | Cosh | Exp => a == 0.0 && r == 1.0,
        Ln | Log => a == 1.0 && r == 0.0,
        Acos => a == 1.0 && r == 0.0,
        _ => false,
    }
}

/// f's value at the float x as the compiled program computes it, and a
/// bound on how far that is from the expression's exact value at x: each
/// operation's rounding, propagated to first order through the tree. This
/// is what the rounding of f's terms can do at x (the expanded (x − 1)⁴ is
/// ±4·10⁻¹⁶ beside its zero though neighbouring floats all agree).
fn eb(e: &Expr, x: f64, u: TrigUnit) -> (f64, f64) {
    const INF: f64 = f64::INFINITY;
    match e {
        Expr::Num(v) => (*v, if v.fract() == 0.0 { 0.0 } else { 0.5 * ulp(*v) }),
        Expr::Const(c) => (c.value(), 0.5 * ulp(c.value())),
        Expr::X => (x, 0.0),
        Expr::Y => (f64::NAN, INF),
        Expr::Var(_) => (DEFAULT_VARIABLE_VALUE, 0.0),
        Expr::Degrees(a) => eb(a, x, u),
        Expr::Neg(a) => {
            let (v, err) = eb(a, x, u);
            (-v, err)
        }
        Expr::Bin(BinOp::Pow, a, b) if let Some((p, q)) = rational(b) => {
            let (va, ea) = eb(a, x, u);
            let g = move |t: f64| {
                if q == 1 {
                    fns::pow_int(t, p)
                } else {
                    fns::pow_rational(t, p, q)
                }
            };
            let r = g(va);
            // (An underflowed square is not exact, whatever fma says.)
            let exact = q == 1
                && (p == 0
                    || p == 1
                    || (p == 2 && (r != 0.0 || va == 0.0) && va.mul_add(va, -r) == 0.0));
            let k = if q == 1 && p.abs() <= 2 {
                0.5
            } else {
                p.unsigned_abs().max(2) as f64
            };
            unary_err(&g, va, ea, k, exact, false)
        }
        Expr::Bin(op, a, b) => {
            let ((va, ea), (vb, ebb)) = (eb(a, x, u), eb(b, x, u));
            if *op == BinOp::Pow {
                return binary_err(&|s, t| fns::pow(s, t), (va, ea), (vb, ebb), 2.0);
            }
            let r = match op {
                BinOp::Add => va + vb,
                BinOp::Sub => va - vb,
                BinOp::Mul => va * vb,
                _ => fns::div(va, vb),
            };
            if !r.is_finite() || !ea.is_finite() || !ebb.is_finite() {
                return (r, INF);
            }
            let err = match op {
                BinOp::Add => ea + ebb + two_sum_err(va, vb, r),
                BinOp::Sub => ea + ebb + two_sum_err(va, -vb, r),
                BinOp::Mul => {
                    let under = if r == 0.0 && va != 0.0 && vb != 0.0 {
                        f64::from_bits(1)
                    } else {
                        0.0
                    };
                    let e = va.abs() * ebb
                        + vb.abs() * ea
                        + ea * ebb
                        + va.mul_add(vb, -r).abs()
                        + under;
                    // (Nor does the bound itself: 10⁻⁶ times an underflowed
                    // e⁻¹⁰⁰⁰'s error is no reason to call the product exact.)
                    if e == 0.0 && ((ea > 0.0 && vb != 0.0) || (ebb > 0.0 && va != 0.0)) {
                        f64::from_bits(1)
                    } else {
                        e
                    }
                }
                _ => {
                    if ebb >= vb.abs() {
                        return (r, INF);
                    }
                    let under = if r == 0.0 && va != 0.0 {
                        f64::from_bits(1)
                    } else {
                        0.0
                    };
                    let e = (va.abs() * ebb + vb.abs() * ea) / (vb.abs() * (vb.abs() - ebb))
                        + ((-r).mul_add(vb, va) / vb).abs()
                        + under;
                    if e == 0.0 && (ea > 0.0 || (ebb > 0.0 && va != 0.0)) {
                        f64::from_bits(1)
                    } else {
                        e
                    }
                }
            };
            (r, err)
        }
        Expr::Call(f, args) => {
            let vs: Vec<(f64, f64)> = args.iter().map(|a| eb(a, x, u)).collect();
            match f {
                Func::Root
                | Func::LogBase
                | Func::Mod
                | Func::NCr
                | Func::NPr
                | Func::Min
                | Func::Max => {
                    let k = if matches!(f, Func::Min | Func::Max) {
                        0.0
                    } else if matches!(f, Func::NCr | Func::NPr) {
                        64.0
                    } else {
                        2.0
                    };
                    let g = fn2(*f);
                    let mut acc = vs[0];
                    for &next in &vs[1..] {
                        acc = binary_err(&g, acc, next, k);
                    }
                    acc
                }
                _ => {
                    let (a, ea) = vs[0];
                    let g = fn1(*f, u);
                    let r = g(a);
                    let k = if matches!(f, Func::Factorial | Func::DoubleFactorial) {
                        64.0
                    } else {
                        1.0
                    };
                    let jumps = matches!(f, Func::Floor | Func::Ceil | Func::Round | Func::Sign);
                    unary_err(&g, a, ea, k, exact_at(*f, a, r, u), jumps)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// An analysed function.

struct Analysed {
    expr: String,
    k: KeyGraphFeatures,
    f: Option<CompiledEquation>,
    ast: Expr,
    unit: TrigUnit,
}

impl Analysed {
    fn new(expr: &str, unit: TrigUnit) -> Option<Analysed> {
        let opts = CompileOptions {
            trig_unit: unit,
            ..CompileOptions::default()
        };
        let eq = Equation::parse(&format!("y={expr}")).ok()?;
        let ast = eq.explicit()?.1.clone();
        let f = eq.compile(&opts).ok();
        let k = analyze(&eq, &opts);
        Some(Analysed {
            expr: expr.to_string(),
            k,
            f,
            ast,
            unit,
        })
    }
    /// The compiled program's value.
    fn raw(&self, x: f64) -> f64 {
        match &self.f {
            Some(f) => f.eval_explicit(x).unwrap_or(f64::NAN),
            None => f64::NAN,
        }
    }
    fn reference(&self, x: f64) -> R {
        reval(&self.ast, x, self.unit)
    }
    /// f(x): the compiled value, unless that is 0 or not finite, where the
    /// reference decides: undefined (NaN), exactly 0, or a nonzero value
    /// beyond the doubles (then the smallest or largest double of its sign).
    fn eval(&self, x: f64) -> f64 {
        let y = self.raw(x);
        if y.is_finite() && y != 0.0 {
            return y;
        }
        match self.reference(x) {
            R::V(v) if v.is_zero() => 0.0,
            R::V(v) => v.proxy(),
            R::Undef => f64::NAN,
            // (A 0 the reference can't vouch for may be underflow.)
            R::Unknown if y.is_finite() && y != 0.0 => y,
            R::Unknown => f64::NAN,
        }
    }
    /// Whether f is defined at x (None: the reference can't tell).
    fn defined(&self, x: f64) -> Option<bool> {
        // (A 0 can be underflow of an undefined operand: √(x/1000) at
        // x = −3·10⁻³²².)
        let y = self.raw(x);
        if y.is_finite() && y != 0.0 {
            return Some(true);
        }
        match self.reference(x) {
            R::V(_) => Some(true),
            R::Undef => Some(false),
            R::Unknown if y.is_finite() => Some(true),
            R::Unknown => None,
        }
    }
    /// How far from x f first takes another value, or changes between
    /// defined and undefined, at least two floats out: the resolution of
    /// f's argument at x. Coarser than x's own spacing in a shifted frame
    /// ((x − 1)² − 1 is the same for every x below 10⁻¹⁶).
    fn resolution(&self, x: f64) -> f64 {
        let (y, d) = (self.raw(x), self.defined(x));
        let mut res = f64::INFINITY;
        for dir in [-1i64, 1] {
            let mut k = 2i64;
            while k <= 1 << 52 {
                let t = nudge(x, dir * k);
                let v = self.raw(t);
                if (v != y && !(v.is_nan() && y.is_nan())) || self.defined(t) != d {
                    res = res.min((t - x).abs());
                    break;
                }
                k *= 2;
            }
        }
        res
    }
    /// The bound on the rounding of f's operations at x alone (no
    /// neighbouring floats: across a jump they say nothing).
    fn rounding(&self, x: f64) -> f64 {
        let y = self.raw(x);
        if !y.is_finite() {
            return f64::INFINITY;
        }
        let (r, err) = eb(&self.ast, x, self.unit);
        err + if r.is_finite() { (r - y).abs() } else { 0.0 }
    }
    /// Certainly exactly 0 at x: no operation on the way rounded.
    fn exact_zero(&self, x: f64) -> bool {
        self.eval(x) == 0.0 && eb(&self.ast, x, self.unit).1 == 0.0
    }
    /// How far f's double value at x can be from the function's value
    /// there, as far as a claim can tell: f's change to its nearest other
    /// values either side, at least two floats out (a feature placed at a
    /// neighbouring float is as good as the floats allow; in a shifted
    /// frame, sin(x − 1000), f only changes every thousand floats of x),
    /// plus the bound on the rounding of f's operations at x. Local by
    /// construction.
    fn noise(&self, x: f64) -> f64 {
        let y = self.raw(x);
        let mut spread = 0.0f64;
        if y.is_finite() {
            for dir in [-1i64, 1] {
                let mut k = 1i64;
                while k <= 1 << 40 {
                    let v = self.raw(nudge(x, dir * k));
                    if !v.is_finite() {
                        break;
                    }
                    if v != y && k >= 2 {
                        spread = spread.max((v - y).abs());
                        break;
                    }
                    k *= 2;
                }
            }
        }
        // Where only the reference's extended arithmetic gives a value,
        // nothing bounds its rounding here: unknown.
        if !y.is_finite() {
            return f64::INFINITY;
        }
        let (r, err) = eb(&self.ast, x, self.unit);
        // The compiler folds and rewrites (a/bᵏ as a·b⁻ᵏ): its double can
        // differ from the plain tree's by their roundings.
        let lowering = if r.is_finite() { (r - y).abs() } else { 0.0 };
        spread + err + lowering
    }
    fn ok(&self) -> bool {
        self.k.analysis_error == AnalysisError::NoError
    }
    fn unknown(&self, flag: u32) -> bool {
        self.k.too_complex_features & flag != 0
    }
    fn d(&self) -> &AnalysisData {
        &self.k.data
    }
    fn period(&self) -> Option<f64> {
        self.d().period.filter(|p| p.is_finite() && *p > 0.0)
    }
    /// The x-intercepts are every x of the domain ("x ∈ ℝ").
    fn zero_everywhere(&self) -> bool {
        self.k.x_intercept == "x ∈ ℝ"
    }
    /// The x-intercepts are given as a set of intervals, not points.
    fn zero_set(&self) -> bool {
        self.k.x_intercept.starts_with("x ∈")
    }
    /// An x-intercept set this checker reads: ℝ, or ℝ without some points
    /// ("x ∈ ℝ \ {0}"); the points left out. None for any other set.
    fn zero_set_except(&self) -> Option<Vec<f64>> {
        let t = self.k.x_intercept.as_str();
        if t == "x ∈ ℝ" {
            return Some(Vec::new());
        }
        let list = t.strip_prefix("x ∈ ℝ \\ {")?.strip_suffix('}')?;
        list.split(", ")
            .map(|v| v.replace('−', "-").parse::<f64>().ok())
            .collect()
    }
    /// Copies of a feature to test: the point, or a few of its family.
    fn copies(&self, f: &Family) -> Vec<f64> {
        match f.period {
            Some(p) if p.is_finite() && p > 0.0 => {
                vec![f.x, f.x + p, f.x - p, f.x + 7.0 * p]
            }
            _ => vec![f.x],
        }
    }
}

/// The member of a family nearest v.
fn nearest(f: &Family, v: f64) -> f64 {
    match f.period {
        Some(p) if p.is_finite() && p > 0.0 => f.x + ((v - f.x) / p).round() * p,
        _ => f.x,
    }
}

// ---------------------------------------------------------------------------
// Sets.

fn iv_has(iv: &Interval, v: f64) -> bool {
    let lo = if iv.lo.closed {
        v >= iv.lo.value
    } else {
        v > iv.lo.value
    };
    let hi = if iv.hi.closed {
        v <= iv.hi.value
    } else {
        v < iv.hi.value
    };
    lo && hi
}

fn set_has(set: &[Interval], v: f64) -> bool {
    set.iter().any(|iv| iv_has(iv, v))
}

/// Distance from v to the set's closure.
fn set_dist(set: &[Interval], v: f64) -> f64 {
    set.iter()
        .map(|iv| {
            if v < iv.lo.value {
                iv.lo.value - v
            } else if v > iv.hi.value {
                v - iv.hi.value
            } else {
                0.0
            }
        })
        .fold(f64::INFINITY, f64::min)
}

/// The bound of the set nearest v.
fn nearest_bound(set: &[Interval], v: f64) -> f64 {
    set.iter()
        .flat_map(|iv| [iv.lo.value, iv.hi.value])
        .filter(|b| b.is_finite())
        .min_by(|p, q| (p - v).abs().total_cmp(&(q - v).abs()))
        .unwrap_or(f64::NAN)
}

/// The panel shows values to 6 significant digits (`format_decimal`): a
/// value that reads the same is the same claim. The engine rounds values
/// to what it shows (545843449.45 is reported as 545843449). Positions and
/// shapes (how many pieces, open or closed, constant or not, a point or an
/// interval) get no such allowance.
fn same_shown(a: f64, b: f64) -> bool {
    a == b || (a.is_finite() && b.is_finite() && format_decimal(a) == format_decimal(b))
}

fn fmt_set(set: &[Interval]) -> String {
    set.iter()
        .map(|iv| {
            format!(
                "{}{:?},{:?}{}",
                if iv.lo.closed { "[" } else { "(" },
                iv.lo.value,
                iv.hi.value,
                if iv.hi.closed { "]" } else { ")" }
            )
        })
        .collect::<Vec<_>>()
        .join("∪")
}

fn fmt_fams(f: &[Family]) -> String {
    let v: Vec<String> = f
        .iter()
        .take(6)
        .map(|f| match f.period {
            Some(p) => format!("{:?}+k·{p:?}", f.x),
            None => format!("{:?}", f.x),
        })
        .collect();
    let more = if f.len() > 6 {
        format!(" … ({} in all)", f.len())
    } else {
        String::new()
    };
    format!("[{}]{more}", v.join(", "))
}

// ---------------------------------------------------------------------------
// Sampling.

fn has_trig(expr: &str) -> bool {
    ["sin", "cos", "tan", "sec", "csc", "cot"]
        .iter()
        .any(|t| expr.contains(t))
}

/// Sample points: dense near the origin and each centre, log-spaced far
/// out, a long fine grid for trig, and the floats at and around every
/// point the analysis reports (and their periodic copies).
fn samples(a: &Analysed, centres: &[f64], wide: bool) -> Vec<f64> {
    let mut xs = Vec::with_capacity(20000);
    let mut cs = vec![0.0];
    cs.extend(centres.iter().copied().filter(|c| c.is_finite()));
    cs.sort_by(f64::total_cmp);
    cs.dedup();
    for &c in &cs {
        for i in -2000..=2000 {
            xs.push(c + i as f64 * 0.005);
        }
        for i in -400..=400 {
            xs.push(c + i as f64);
        }
        for e in -60..=150 {
            let m = 10f64.powf(e as f64 / 10.0);
            xs.push(c + m);
            xs.push(c - m);
        }
        for k in [-64, -8, -1, 1, 8, 64] {
            xs.push(nudge(c, k));
        }
    }
    if has_trig(&a.expr) || wide {
        let n = if wide { 400_000 } else { 40_000 };
        for i in -n..=n {
            xs.push(i as f64 * 0.05 + 0.0123);
        }
    }
    let d = a.d();
    let mut pts: Vec<Family> = Vec::new();
    pts.extend(d.zeros.iter().copied());
    pts.extend(
        d.minima
            .iter()
            .chain(&d.maxima)
            .chain(&d.inflection_points)
            .map(|m| m.0),
    );
    pts.extend(d.vertical_asymptotes.iter().copied());
    pts.extend(d.excluded.iter().copied());
    for iv in d.domain.iter().chain(d.monotonicity.iter().map(|m| &m.0)) {
        for v in [iv.lo.value, iv.hi.value] {
            if v.is_finite() {
                pts.push(Family {
                    x: v,
                    period: a.period(),
                });
            }
        }
    }
    for f in &pts {
        for c in a.copies(f) {
            if !c.is_finite() {
                continue;
            }
            for k in [0, -1, 1, -8, 8, -64, 64, -4096, 4096] {
                xs.push(nudge(c, k));
            }
            let s = cs
                .iter()
                .map(|z| (c - z).abs())
                .fold(f64::INFINITY, f64::min)
                .max(1.0);
            for h in [1e-9, 1e-6, 1e-3, 0.1] {
                xs.push(c + h * s);
                xs.push(c - h * s);
            }
        }
    }
    xs.retain(|x| x.is_finite());
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    xs
}

/// Golden-section search for the smallest (sign 1) or largest (sign −1)
/// value on [lo, hi]; undefined points count as worst.
fn golden(a: &Analysed, mut lo: f64, mut hi: f64, sign: f64) -> (f64, f64) {
    let g = |x: f64| {
        let y = a.eval(x);
        if y.is_finite() {
            sign * y
        } else {
            f64::INFINITY
        }
    };
    let phi = 0.5 * (5f64.sqrt() - 1.0);
    let (mut m1, mut m2) = (hi - phi * (hi - lo), lo + phi * (hi - lo));
    let (mut g1, mut g2) = (g(m1), g(m2));
    for _ in 0..160 {
        if floats_between(lo, hi) <= 2 {
            break;
        }
        if g1 <= g2 {
            hi = m2;
            m2 = m1;
            g2 = g1;
            m1 = hi - phi * (hi - lo);
            g1 = g(m1);
        } else {
            lo = m1;
            m1 = m2;
            g1 = g2;
            m2 = lo + phi * (hi - lo);
            g2 = g(m2);
        }
    }
    let x = if g1 <= g2 { m1 } else { m2 };
    (x, a.eval(x))
}

/// The k most extreme samples (smallest for sign 1), each refined between
/// its neighbours.
fn refine_extremes(a: &Analysed, xs: &[f64], ys: &[f64], sign: f64, k: usize) -> Vec<(f64, f64)> {
    let mut idx: Vec<usize> = (1..ys.len().saturating_sub(1))
        .filter(|&i| ys[i].is_finite())
        .collect();
    idx.sort_by(|&i, &j| (sign * ys[i]).total_cmp(&(sign * ys[j])));
    idx.truncate(k);
    idx.iter()
        .map(|&i| golden(a, xs[i - 1], xs[i + 1], sign))
        .filter(|(_, y)| y.is_finite())
        .collect()
}

// ---------------------------------------------------------------------------
// Report and coverage.

struct Report {
    failures: BTreeMap<&'static str, Vec<String>>,
}

impl Report {
    fn fail(&mut self, check: &'static str, expr: &str, detail: String) {
        self.failures
            .entry(check)
            .or_default()
            .push(format!("y={expr}    {detail}"));
    }
}

const FEATURES: [(u32, &str); 13] = [
    (flags::DOMAIN, "domain"),
    (flags::RANGE, "range"),
    (flags::PARITY, "parity"),
    (flags::PERIODICITY, "period"),
    (flags::ZEROS, "zeros"),
    (flags::Y_INTERCEPT, "y-int"),
    (flags::MINIMA, "minima"),
    (flags::MAXIMA, "maxima"),
    (flags::INFLECTION_POINTS, "inflect"),
    (flags::VERTICAL_ASYMPTOTES, "v-asym"),
    (flags::HORIZONTAL_ASYMPTOTES, "h-asym"),
    (flags::OBLIQUE_ASYMPTOTES, "o-asym"),
    (flags::MONOTONE_INTERVALS, "monotone"),
];

/// How many functions, how many the analysis refused, and per feature how
/// many answers were unknown (the rest of the analysed ones are definite).
#[derive(Default, Clone)]
struct Coverage {
    fns: usize,
    refused: usize,
    unknown: [usize; 13],
}

impl Coverage {
    fn add(&mut self, a: &Analysed) {
        self.fns += 1;
        if !a.ok() {
            self.refused += 1;
            return;
        }
        for (i, (flag, _)) in FEATURES.iter().enumerate() {
            if a.unknown(*flag) {
                self.unknown[i] += 1;
            }
        }
    }
    fn merge(&mut self, o: &Coverage) {
        self.fns += o.fns;
        self.refused += o.refused;
        for i in 0..13 {
            self.unknown[i] += o.unknown[i];
        }
    }
    fn unknown_total(&self) -> usize {
        self.unknown.iter().sum()
    }
}

// ---------------------------------------------------------------------------
// Checks on one function.

/// Checks on one analysed function on its own.
fn check_one(a: &Analysed, centres: &[f64], wide: bool, r: &mut Report) {
    if !a.ok() {
        return;
    }
    check_flagged(a, r);
    let xs = samples(a, centres, wide);
    let ys: Vec<f64> = xs.iter().map(|&x| a.eval(x)).collect();
    if !a.unknown(flags::RANGE) && !a.d().range.is_empty() {
        check_range(a, &xs, &ys, r);
    }
    check_zero_claims(a, r);
    check_missing_zeros(a, &xs, &ys, r);
    check_extremum_claims(a, r);
    check_missing_extrema(a, &xs, &ys, r);
    check_y_intercept(a, r);
    check_domain(a, &xs, r);
    check_monotonicity(a, &xs, &ys, r);
    check_vertical(a, r);
    check_horizontal(a, centres, r);
    check_parity(a, &xs, r);
    check_period(a, centres, r);
}

/// Rows flagged unknown keep no values.
fn check_flagged(a: &Analysed, r: &mut Report) {
    let d = a.d();
    for (flag, name, n) in [
        (flags::ZEROS, "zeros", d.zeros.len()),
        (flags::MINIMA, "minima", d.minima.len()),
        (flags::MAXIMA, "maxima", d.maxima.len()),
        (
            flags::INFLECTION_POINTS,
            "inflections",
            d.inflection_points.len(),
        ),
        (
            flags::VERTICAL_ASYMPTOTES,
            "vertical asymptotes",
            d.vertical_asymptotes.len(),
        ),
        (
            flags::HORIZONTAL_ASYMPTOTES,
            "horizontal asymptotes",
            d.horizontal_asymptotes.len(),
        ),
        (
            flags::OBLIQUE_ASYMPTOTES,
            "oblique asymptotes",
            d.oblique_asymptotes.len(),
        ),
    ] {
        if a.unknown(flag) && n > 0 {
            r.fail(
                "flagged-unknown-but-has-values",
                &a.expr,
                format!("{name}: {n} values kept"),
            );
        }
    }
}

fn check_range(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
    let d = a.d();
    let e = &a.expr;
    let range = &d.range;
    // Every value lies in the range, give or take its noise or what the
    // panel shows.
    // (Nearest the origin first: the plainest counterexample.)
    let mut outside: Vec<usize> = (0..xs.len())
        .filter(|&i| ys[i].is_finite() && !set_has(range, ys[i]))
        .collect();
    outside.sort_by(|&i, &j| xs[i].abs().total_cmp(&xs[j].abs()));
    for (x, y) in outside.into_iter().map(|i| (xs[i], ys[i])) {
        let dist = set_dist(range, y);
        if dist > 0.0 && dist > a.noise(x) && !same_shown(y, nearest_bound(range, y)) {
            r.fail(
                "sample-outside-range",
                e,
                format!("f({x:?})={y:?} not in claimed range {}", fmt_set(range)),
            );
            break;
        }
    }
    // A set of points is a shape: f taking resolvably different values
    // that all read as one claimed point contradicts it (a constant claimed
    // for 10⁶ + 10⁻⁶·sin x), however they read. (An offset that reads the
    // same, x/x + 10⁻¹² for {1}, is the same claim.)
    if range.iter().all(|iv| iv.lo.value == iv.hi.value) {
        let mut seen: Vec<Option<((f64, f64), (f64, f64))>> = vec![None; range.len()];
        for (&x, &y) in xs.iter().zip(ys) {
            if !y.is_finite() {
                continue;
            }
            let Some(j) = (0..range.len()).find(|&j| same_shown(y, range[j].lo.value)) else {
                continue;
            };
            let s = seen[j].get_or_insert(((x, y), (x, y)));
            if y < s.0.1 {
                s.0 = (x, y);
            }
            if y > s.1.1 {
                s.1 = (x, y);
            }
        }
        if let Some(((x1, y1), (x2, y2))) = seen
            .into_iter()
            .flatten()
            .find(|((x1, y1), (x2, y2))| y2 - y1 > a.noise(*x1) + a.noise(*x2))
        {
            r.fail(
                "range-points-but-varies",
                e,
                format!(
                    "f({x1:?})={y1:?} and f({x2:?})={y2:?} both read as one of {}",
                    fmt_set(range)
                ),
            );
        }
    }
    // So do the most extreme values, refined: a sample rarely lands on the
    // top of a peak.
    let lows = refine_extremes(a, xs, ys, 1.0, 32);
    let highs = refine_extremes(a, xs, ys, -1.0, 32);
    for &(x, y) in lows.iter().chain(&highs) {
        if !set_has(range, y)
            && set_dist(range, y) > a.noise(x)
            && !same_shown(y, nearest_bound(range, y))
        {
            r.fail(
                "refined-value-outside-range",
                e,
                format!("f({x:?})={y:?} not in claimed range {}", fmt_set(range)),
            );
            break;
        }
    }
    let near = |x: f64, y: f64, v: f64| {
        y.is_finite() && (y == v || same_shown(y, v) || (y - v).abs() <= a.noise(x))
    };
    for iv in range {
        for (b, is_lo) in [(iv.lo, true), (iv.hi, false)] {
            let v = b.value;
            if !v.is_finite() {
                continue;
            }
            if b.closed {
                // A closed bound is attained: at a reported point, a sample
                // or a refined extreme, to within the noise there.
                let claimed = d
                    .minima
                    .iter()
                    .chain(&d.maxima)
                    .chain(&d.inflection_points)
                    .any(|(fx, y)| *y == v && near(fx.x, a.eval(fx.x), v))
                    || d.y_intercept
                        .is_some_and(|y| y == v && near(0.0, a.eval(0.0), v));
                let refined = if is_lo { &lows } else { &highs };
                let mut closest: Vec<usize> =
                    (0..ys.len()).filter(|&i| ys[i].is_finite()).collect();
                closest.sort_by(|&i, &j| (ys[i] - v).abs().total_cmp(&(ys[j] - v).abs()));
                closest.truncate(64);
                let hit = claimed
                    || refined.iter().any(|&(x, y)| near(x, y, v))
                    || closest.iter().any(|&i| near(xs[i], ys[i], v));
                if !hit {
                    let best = refined.first().copied().unwrap_or((f64::NAN, f64::NAN));
                    r.fail(
                        "closed-range-bound-not-attained",
                        e,
                        format!(
                            "range {} has closed bound {v:?}; most extreme value found f({:?})={:?}",
                            fmt_set(range),
                            best.0,
                            best.1
                        ),
                    );
                }
            } else if let Some(i) = (0..ys.len()).find(|&i| ys[i] == v && a.noise(xs[i]) == 0.0) {
                // An open bound is not attained: f exactly at it, with no
                // rounding anywhere, attains it.
                r.fail(
                    "open-range-bound-attained",
                    e,
                    format!("range {} but f({:?})={v:?} exactly", fmt_set(range), xs[i]),
                );
            }
        }
    }
    // 0 attained ⟺ x-intercepts.
    if !a.unknown(flags::ZEROS) {
        let attains0 = set_has(range, 0.0);
        let zeros = !d.zeros.is_empty() || a.zero_set();
        if attains0 && !zeros {
            r.fail(
                "range-has-0-but-no-zeros",
                e,
                format!("range {} attains 0, no x-intercepts", fmt_set(range)),
            );
        }
        if zeros && !attains0 {
            r.fail(
                "zeros-but-range-lacks-0",
                e,
                format!(
                    "x-intercepts {}, range {}",
                    fmt_fams(&d.zeros),
                    fmt_set(range)
                ),
            );
        }
    }
    for (what, list) in [
        ("minimum", &d.minima),
        ("maximum", &d.maxima),
        ("inflection", &d.inflection_points),
    ] {
        for (fx, y) in list {
            if !set_has(range, *y)
                && set_dist(range, *y) > a.noise(fx.x)
                && !same_shown(*y, nearest_bound(range, *y))
            {
                r.fail(
                    "extremum-y-outside-range",
                    e,
                    format!("{what} ({:?}, {y:?}) but range {}", fx.x, fmt_set(range)),
                );
            }
        }
    }
    if let Some(y) = d.y_intercept
        && !set_has(range, y)
        && set_dist(range, y) > a.noise(0.0)
        && !same_shown(y, nearest_bound(range, y))
    {
        r.fail(
            "y-intercept-outside-range",
            e,
            format!("y-intercept {y:?}, range {}", fmt_set(range)),
        );
    }
}

/// What evaluation says about a claimed zero.
#[derive(PartialEq)]
enum Verdict {
    Holds,
    Fails,
    /// At or beside a pole, closer than f's resolution: 10⁶ + 10⁻⁶/(x + 10⁶)
    /// is 0 at −10⁶ − 10⁻¹², which no float can show.
    Unverifiable,
}

/// Is f indistinguishable from 0 at z or within a few steps of its own
/// resolution (f in a shifted frame changes only every so many floats),
/// without blowing up there (a pole is no zero)?
fn zero_ok(a: &Analysed, z: f64) -> Verdict {
    let y = a.eval(z);
    if y == 0.0 {
        return Verdict::Holds;
    }
    let res = a.resolution(z);
    let step = if res.is_finite() { res } else { ulp(z) };
    let h = (4096.0 * ulp(z)).max(64.0 * step);
    let side = a.eval(z - h).abs().max(a.eval(z + h).abs());
    if !y.is_finite() || (side.is_finite() && y.abs() > side) {
        return Verdict::Unverifiable;
    }
    for m in [1.0, 2.0, 4.0, 16.0] {
        let (l, r) = (a.eval(z - m * step), a.eval(z + m * step));
        if l.is_finite() && r.is_finite() && (l == 0.0 || r == 0.0 || l.signum() != r.signum()) {
            return Verdict::Holds;
        }
    }
    let near = (-4..=4).any(|k| {
        let t = z + k as f64 * step;
        let v = a.eval(t);
        v.is_finite() && v.abs() <= a.noise(t)
    });
    if near { Verdict::Holds } else { Verdict::Fails }
}

/// Are p and q the same zero: a few dozen floats apart (where a root
/// finder stops; 10⁷·(1 − 3·10⁻¹⁵) for 10⁷ is no claim anyone can see), or
/// with f indistinguishable from 0 all the way between?
fn same_root(a: &Analysed, p: f64, q: f64) -> bool {
    if floats_between(p, q) <= 64 {
        return true;
    }
    (1..8).all(|i| {
        let t = p + (q - p) * i as f64 / 8.0;
        let y = a.eval(t);
        y.is_finite() && (y == 0.0 || y.abs() <= a.noise(t))
    })
}

fn check_zero_claims(a: &Analysed, r: &mut Report) {
    if a.unknown(flags::ZEROS) {
        return;
    }
    let d = a.d();
    'z: for z in &d.zeros {
        for c in a.copies(z) {
            let (check, v) = match zero_ok(a, c) {
                Verdict::Holds => continue,
                Verdict::Fails => ("zero-not-a-zero", a.eval(c)),
                Verdict::Unverifiable => ("unverifiable-zero-beside-pole", a.eval(c)),
            };
            r.fail(
                check,
                &a.expr,
                format!("x-intercept {c:?} but f={v:?} (noise {:?})", a.noise(c)),
            );
            break 'z;
        }
    }
    // One zero reported once.
    let mut xs: Vec<f64> = d
        .zeros
        .iter()
        .filter(|z| z.period.is_none())
        .map(|z| z.x)
        .collect();
    xs.sort_by(f64::total_cmp);
    for w in xs.windows(2) {
        if same_root(a, w[0], w[1]) {
            r.fail(
                "zeros-duplicate",
                &a.expr,
                format!("x-intercepts {} and {} are one zero", w[0], w[1]),
            );
            break;
        }
    }
}

/// What a sign change between two samples turns out to be.
enum Found {
    Crossing(f64),
    Pole(f64),
    Jump,
    Gap,
}

/// Bisect a sign change down to neighbouring floats and say what is there:
/// a zero crossing (f there is as small as its change across a few floats),
/// a pole (|f| grows towards it, far beyond the samples), a jump, or an
/// undefined point (gap).
fn bisect(a: &Analysed, mut lo: f64, mut hi: f64) -> Found {
    let (mut ylo, mut yhi) = (a.eval(lo), a.eval(hi));
    let scale = ylo.abs().max(yhi.abs());
    let s = ylo.signum();
    for _ in 0..80 {
        if floats_between(lo, hi) <= 1 {
            break;
        }
        let mid = from_ord(to_ord(lo) + (to_ord(hi) - to_ord(lo)) / 2);
        let ym = a.eval(mid);
        if ym.is_nan() {
            return Found::Gap;
        }
        if ym == 0.0 {
            return Found::Crossing(mid);
        }
        if ym.signum() == s {
            lo = mid;
            ylo = ym;
        } else {
            hi = mid;
            yhi = ym;
        }
    }
    // Sixteen steps of f's own resolution out (in a shifted frame f is a
    // staircase over the floats of x).
    let step = |t: f64| {
        let res = a.resolution(t);
        if res.is_finite() { res } else { ulp(t) }
    };
    let (l2, h2) = (a.eval(lo - 16.0 * step(lo)), a.eval(hi + 16.0 * step(hi)));
    if !l2.is_finite() || !h2.is_finite() {
        return Found::Gap;
    }
    if ylo.abs() > l2.abs() && yhi.abs() > h2.abs() && ylo.abs().min(yhi.abs()) > 1e3 * scale {
        return Found::Pole(lo);
    }
    // A crossing: f beside it is small against its change across those
    // steps, steps no more than that, and goes one way through it. (Beside
    // a pole |f| is as big as that change and turns back; across a jump the
    // step is the jump.)
    let jump = (yhi - ylo).abs();
    let sides = (ylo - l2).abs().max((h2 - yhi).abs());
    let s = (yhi - ylo).signum();
    let through = l2 != ylo && h2 != yhi && (ylo - l2).signum() == s && (h2 - yhi).signum() == s;
    if through && ylo.abs().min(yhi.abs()) <= 0.25 * sides && jump <= 2.0 * sides {
        Found::Crossing(if ylo.abs() <= yhi.abs() { lo } else { hi })
    } else {
        Found::Jump
    }
}

/// Zeros the analysis didn't report: sign changes bisected to a crossing,
/// and exactly-zero samples (a run of them is an interval of zeros). Each
/// must match a distinct reported zero. Poles found the same way must be
/// reported too.
fn check_missing_zeros(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
    let d = a.d();
    let e = &a.expr;
    let n = xs.len();
    let mut roots: Vec<f64> = Vec::new();
    let mut runs: Vec<(f64, f64)> = Vec::new();
    let mut poles: Vec<f64> = Vec::new();
    let mut i = 0;
    while i < n {
        if ys[i] == 0.0 && a.exact_zero(xs[i]) {
            let start = i;
            // A run is an interval of zeros only if f is 0 between the
            // samples too (far, sparse samples can all happen to be 0).
            while i + 1 < n
                && ys[i + 1] == 0.0
                && a.exact_zero(xs[i + 1])
                && a.exact_zero(0.5 * (xs[i] + xs[i + 1]))
            {
                i += 1;
            }
            if i - start >= 2 {
                runs.push((xs[start], xs[i]));
            } else {
                roots.extend(&xs[start..=i]);
            }
        }
        i += 1;
    }
    for i in 0..n.saturating_sub(1) {
        let (y0, y1) = (ys[i], ys[i + 1]);
        if y0.is_finite()
            && y1.is_finite()
            && y0 != 0.0
            && y1 != 0.0
            && y0.signum() != y1.signum()
            // (Past f's resolution, trig of 10¹⁴, the signs say nothing.)
            && y0.abs() > a.noise(xs[i])
            && y1.abs() > a.noise(xs[i + 1])
        {
            match bisect(a, xs[i], xs[i + 1]) {
                Found::Crossing(x) => roots.push(x),
                Found::Pole(x) => poles.push(x),
                Found::Jump | Found::Gap => {}
            }
            if roots.len() + poles.len() > 4000 {
                break;
            }
        }
    }
    if !a.unknown(flags::ZEROS) {
        if a.zero_set() {
            // A set of x-intercepts (the zeros list can't hold one) is
            // checked as a set: f is 0 at every sample in it, and not 0
            // (undefined, or resolvably nonzero) at the points it leaves
            // out. A set this checker doesn't read is only checked to be
            // 0 somewhere, by the 0 ∈ range rule.
            if let Some(except) = a.zero_set_except() {
                let inside = |x: f64| !except.iter().any(|p| floats_between(*p, x) <= 4);
                if let Some(i) = (0..n).find(|&i| {
                    inside(xs[i])
                        && ys[i].is_finite()
                        && ys[i] != 0.0
                        && ys[i].abs() > a.noise(xs[i])
                }) {
                    r.fail(
                        "zero-set-wrong",
                        e,
                        format!(
                            "x-intercepts \"{}\" but f({:?})={:?}",
                            a.k.x_intercept, xs[i], ys[i]
                        ),
                    );
                } else if let Some(p) = except
                    .iter()
                    .find(|&&p| a.eval(p) == 0.0 && a.exact_zero(p))
                {
                    r.fail(
                        "zero-set-wrong",
                        e,
                        format!(
                            "x-intercepts \"{}\" leave out {p:?}, but f({p:?})=0",
                            a.k.x_intercept
                        ),
                    );
                }
            }
        } else {
            if let Some(&(p, q)) = runs.first() {
                r.fail(
                    "missing-zero-interval",
                    e,
                    format!(
                        "f is exactly 0 on [{p:?}, {q:?}] (and {} more runs); x-intercepts {}",
                        runs.len() - 1,
                        fmt_fams(&d.zeros)
                    ),
                );
            }
            roots.sort_by(f64::total_cmp);
            roots.dedup();
            let mut used: Vec<Option<f64>> = vec![None; d.zeros.len()];
            for &x in &roots {
                let mut hit = false;
                for (j, z) in d.zeros.iter().enumerate() {
                    let c = nearest(z, x);
                    // (A family's far copies carry its period's rounding.)
                    if !same_root(a, c, x) && !(z.period.is_some() && close_to(z, x, 16)) {
                        continue;
                    }
                    // One to one: a reported point zero stands for one root.
                    if z.period.is_none()
                        && let Some(prev) = used[j]
                        && !same_root(a, prev, x)
                    {
                        continue;
                    }
                    used[j] = Some(x);
                    hit = true;
                    break;
                }
                if !hit {
                    r.fail(
                        "missing-zero",
                        e,
                        format!(
                            "f({x:?})={:?} changes sign or is 0 there; x-intercepts {} (nearest {:?})",
                            a.eval(x),
                            fmt_fams(&d.zeros),
                            d.zeros.iter().map(|z| nearest(z, x)).collect::<Vec<_>>()
                        ),
                    );
                    break;
                }
            }
        }
    }
    if !a.unknown(flags::VERTICAL_ASYMPTOTES) {
        for &x in &poles {
            let claimed = d
                .vertical_asymptotes
                .iter()
                .chain(&d.excluded)
                .any(|v| floats_between(nearest(v, x), x) <= 1024);
            let edge = d.domain.iter().any(|iv| {
                [iv.lo.value, iv.hi.value]
                    .iter()
                    .any(|b| b.is_finite() && floats_between(*b, x) <= 1024)
            });
            if !claimed && !edge {
                r.fail(
                    "missing-pole",
                    e,
                    format!(
                        "|f| grows without bound at {x:?}; vertical asymptotes {}",
                        fmt_fams(&d.vertical_asymptotes)
                    ),
                );
                break;
            }
        }
    }
}

/// Places a reported extremum's neighbourhood mustn't reach past.
fn marks(a: &Analysed) -> Vec<f64> {
    let d = a.d();
    let mut m = Vec::new();
    for (f, _) in d.minima.iter().chain(&d.maxima) {
        m.extend(a.copies(f));
    }
    for f in d.vertical_asymptotes.iter().chain(&d.excluded) {
        m.extend(a.copies(f));
    }
    for iv in &d.domain {
        for v in [iv.lo.value, iv.hi.value] {
            if v.is_finite() {
                m.push(v);
            }
        }
    }
    m
}

/// A reported extremum is where it says, with the value it says, and
/// nothing near it (short of the next reported feature) is beyond it by
/// more than the noise at both places.
fn check_extremum_claims(a: &Analysed, r: &mut Report) {
    let d = a.d();
    let e = &a.expr;
    let marks = marks(a);
    for (list, sign, flag, name) in [
        (&d.minima, 1.0, flags::MINIMA, "minimum"),
        (&d.maxima, -1.0, flags::MAXIMA, "maximum"),
    ] {
        if a.unknown(flag) {
            continue;
        }
        'claims: for (fx, y) in list {
            for c in a.copies(fx) {
                let fv = a.eval(c);
                let n0 = a.noise(c);
                if !fv.is_finite() || ((fv - y).abs() > n0 + 4.0 * ulp(*y) && !same_shown(fv, *y)) {
                    r.fail(
                        "extremum-value-wrong",
                        e,
                        format!("{name} at {c:?} reported {y:?}, f={fv:?}"),
                    );
                    continue 'claims;
                }
                let gap = marks
                    .iter()
                    .filter(|&&m| floats_between(m, c) > 16)
                    .map(|m| (m - c).abs())
                    .fold(f64::INFINITY, f64::min);
                let s = c.abs().max(1.0);
                for h in [64.0 * ulp(c), 4096.0 * ulp(c), 1e-6 * s, 1e-3 * s] {
                    let h = h.min(0.5 * gap);
                    if h.is_nan() || h <= 0.0 {
                        continue;
                    }
                    let (xb, yb) = golden(a, c - h, c + h, sign);
                    if yb.is_finite() && sign * (yb - fv) < -(a.noise(xb) + n0) {
                        r.fail(
                            "extremum-not-extreme",
                            e,
                            format!("{name} ({c:?}, {y:?}) but f({xb:?})={yb:?}"),
                        );
                        continue 'claims;
                    }
                }
            }
        }
    }
}

/// Are p and q the same turn (minimum for sign 1): a few floats apart, or
/// the same value with no hill (valley) between, to within noise?
fn same_turn(a: &Analysed, p: f64, q: f64, sign: f64) -> bool {
    if floats_between(p, q) <= 16 {
        return true;
    }
    let (fp, fq) = (a.eval(p), a.eval(q));
    if !fp.is_finite() || !fq.is_finite() || (fp - fq).abs() > a.noise(p) + a.noise(q) {
        return false;
    }
    let top = (sign * fp).max(sign * fq);
    (1..8).all(|i| {
        let t = p + (q - p) * i as f64 / 8.0;
        let y = a.eval(t);
        y.is_finite() && sign * y <= top + a.noise(t)
    })
}

/// Turns the analysis didn't report: a sample beyond both neighbours by
/// more than the noise, refined, on a continuous stretch (not the edge of a
/// jump), must match a distinct reported extremum of its kind.
fn check_missing_extrema(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
    let d = a.d();
    let e = &a.expr;
    let n = xs.len();
    for (list, sign, flag, name) in [
        (&d.minima, 1.0, flags::MINIMA, "missing-minimum"),
        (&d.maxima, -1.0, flags::MAXIMA, "missing-maximum"),
    ] {
        if a.unknown(flag) {
            continue;
        }
        let mut cand: Vec<(usize, f64)> = (1..n.saturating_sub(1))
            .filter(|&i| ys[i - 1].is_finite() && ys[i].is_finite() && ys[i + 1].is_finite())
            .filter_map(|i| {
                let p = (sign * (ys[i - 1] - ys[i])).min(sign * (ys[i + 1] - ys[i]));
                (p > 0.0).then_some((i, p))
            })
            .collect();
        cand.sort_by(|x, y| y.1.total_cmp(&x.1));
        cand.truncate(48);
        let mut used: Vec<Option<f64>> = vec![None; list.len()];
        for (i, p) in cand {
            if p <= a.noise(xs[i]) + a.noise(xs[i - 1]).max(a.noise(xs[i + 1])) {
                continue;
            }
            let (m1, m2) = (0.5 * (xs[i - 1] + xs[i]), 0.5 * (xs[i] + xs[i + 1]));
            if a.defined(m1) != Some(true) || a.defined(m2) != Some(true) {
                continue;
            }
            // A turn of f, not a pole between the samples: f stays within
            // their size across the bracket.
            let span = ys[i - 1].abs().max(ys[i].abs()).max(ys[i + 1].abs());
            if (1..8).any(|j| {
                let t = xs[i - 1] + (xs[i + 1] - xs[i - 1]) * j as f64 / 8.0;
                let v = a.eval(t);
                !v.is_finite() || v.abs() > 4.0 * span
            }) {
                continue;
            }
            let (xm, ym) = golden(a, xs[i - 1], xs[i + 1], sign);
            // (Nor one whose own value is as uncertain as its height: the
            // floor of a cosine that rounds about 0.)
            if !ym.is_finite() || sign * ym > sign * ys[i] || a.rounding(xm) >= p {
                continue;
            }
            // Beyond both its near sides (golden section can end on an edge).
            let local = [64.0 * ulp(xm), 1e-6 * xm.abs().max(1.0)].iter().all(|&h| {
                [xm - h, xm + h].iter().all(|&t| {
                    let v = a.eval(t);
                    v.is_finite() && sign * (v - ym) >= -(a.noise(t) + a.noise(xm))
                })
            });
            if !local {
                continue;
            }
            // The edge of a jump or a pole, or a plateau, is no turn: there
            // f moves, a few steps of its resolution out, by as much as it
            // does across the whole bracket (the top of atan(tan(x)) − 10⁶
            // is a staircase; tan beside its pole; floor(cos x) between far
            // samples). At a turn that is a small part of it.
            let res = a.resolution(xm);
            let h = 4.0 * if res.is_finite() { res } else { 16.0 * ulp(xm) };
            let (d1, d2) = ((a.eval(xm - h) - ym).abs(), (a.eval(xm + h) - ym).abs());
            if !(d1.is_finite() && d2.is_finite()) || d1.max(d2) > 0.5 * p + a.rounding(xm) {
                continue;
            }
            // Nor where f moves, within a few dozen floats, by a good part
            // of its swing over the bracket (a sawtooth's top between far
            // samples).
            let swing = (1..8)
                .map(|j| a.eval(xs[i - 1] + (xs[i + 1] - xs[i - 1]) * j as f64 / 8.0) - ym)
                .filter(|v| v.is_finite())
                .fold(0.0f64, |m, v| m.max(v.abs()));
            let mut near: Vec<f64> = [2, 4, 8, 16, 32, 64]
                .iter()
                .flat_map(|&k| [nudge(xm, -k), nudge(xm, k)])
                .collect();
            let delta = (xs[i + 1] - xs[i - 1]) / 128.0;
            near.extend((-8..=8).map(|j| xm + j as f64 * delta));
            near.sort_by(f64::total_cmp);
            let vals: Vec<f64> = near.iter().map(|&t| a.eval(t)).collect();
            let step = vals
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(
                    0.0f64,
                    |m, v| if v.is_nan() { f64::INFINITY } else { m.max(v) },
                );
            if !(step <= 0.25 * swing + a.rounding(xm)) {
                continue;
            }
            // Nor is the top of the climb to a reported pole (a missing
            // pole is the missing-pole check's business).
            let (lo, hi) = (xs[i - 1].min(xm - h), xs[i + 1].max(xm + h));
            if d.vertical_asymptotes.iter().chain(&d.excluded).any(|v| {
                let c = nearest(v, xm);
                c >= lo && c <= hi || close_to(v, xm, 64)
            }) {
                continue;
            }
            let mut hit = false;
            for (j, (fx, _)) in list.iter().enumerate() {
                let c = nearest(fx, xm);
                if !same_turn(a, c, xm, sign) {
                    continue;
                }
                if fx.period.is_none()
                    && let Some(prev) = used[j]
                    && !same_turn(a, prev, xm, sign)
                {
                    r.fail(
                        "extrema-merged",
                        e,
                        format!(
                            "turns at {prev:?} and {xm:?} both matched to the reported one at {:?}",
                            fx.x
                        ),
                    );
                    hit = true;
                    break;
                }
                used[j] = Some(xm);
                hit = true;
                break;
            }
            if !hit {
                let reported: Vec<(f64, f64)> =
                    list.iter().take(6).map(|(f, y)| (f.x, *y)).collect();
                r.fail(
                    name,
                    e,
                    format!("turn at ({xm:?}, {ym:?}); reported {reported:?}"),
                );
                break;
            }
        }
    }
}

fn check_y_intercept(a: &Analysed, r: &mut Report) {
    if a.unknown(flags::Y_INTERCEPT) {
        return;
    }
    let e = &a.expr;
    let f0 = a.eval(0.0);
    match (a.d().y_intercept, a.defined(0.0)) {
        (Some(y), Some(false)) => r.fail(
            "y-intercept-where-undefined",
            e,
            format!("y-intercept {y:?} but f(0) is undefined"),
        ),
        (Some(y), Some(true))
            if (y - f0).abs() > a.noise(0.0) + 4.0 * ulp(y) && !same_shown(y, f0) =>
        {
            r.fail(
                "y-intercept-wrong",
                e,
                format!("y-intercept {y:?} but f(0)={f0:?}"),
            )
        }
        (None, Some(true)) => r.fail(
            // (Beyond the doubles, "unable to calculate" would be honest.)
            if f0.abs() == f64::MAX {
                "missing-y-intercept-beyond-doubles"
            } else {
                "missing-y-intercept"
            },
            e,
            format!("no y-intercept but f(0)={f0:?}"),
        ),
        _ => {}
    }
}

/// Is x a member of the family, to the rounding of its copies?
fn in_family(f: &Family, x: f64) -> bool {
    let c = nearest(f, x);
    let k = match f.period {
        Some(p) if p > 0.0 => ((x - f.x) / p).round().abs() * ulp(p),
        _ => 0.0,
    };
    (x - c).abs() <= 0.5 * ulp(x) + 2.0 * k
}

/// Is x within a few floats (or the rounding of a family's copies) of v?
fn close_to(f: &Family, x: f64, floats: i64) -> bool {
    let c = nearest(f, x);
    let k = match f.period {
        Some(p) if p > 0.0 => ((x - f.x) / p).round().abs() * ulp(p),
        _ => 0.0,
    };
    floats_between(c, x) <= floats || (x - c).abs() <= 4.0 * k
}

fn check_domain(a: &Analysed, xs: &[f64], r: &mut Report) {
    let d = a.d();
    if a.unknown(flags::DOMAIN) || d.domain.is_empty() {
        return;
    }
    let e = &a.expr;
    let ends: Vec<Family> = d
        .domain
        .iter()
        .flat_map(|iv| [iv.lo.value, iv.hi.value])
        .filter(|v| v.is_finite())
        .map(|v| Family {
            x: v,
            period: a.period(),
        })
        .chain(d.excluded.iter().copied())
        .collect();
    // Within a few floats of an end or an excluded point, which side is
    // which is the precision of the end, not a false claim.
    let near = |x: f64| ends.iter().any(|f| close_to(f, x, 4));
    let excluded = |x: f64| d.excluded.iter().any(|f| in_family(f, x));
    let detail = |x: f64| {
        format!(
            "x={x:?}: domain {} excl {}",
            fmt_set(&d.domain),
            fmt_fams(&d.excluded)
        )
    };
    let mut bad_in = None;
    let mut bad_out = None;
    let mut test = |x: f64, claimed: bool| {
        if near(x) {
            return;
        }
        // Which side of a boundary x is on can't be told closer than f's
        // own resolution there.
        let edge = |x: f64| {
            let d = a.defined(x);
            let res = a.resolution(x);
            !res.is_finite() || a.defined(x - 4.0 * res) != d || a.defined(x + 4.0 * res) != d
        };
        match a.defined(x) {
            Some(true) if !claimed && edge(x) => {}
            Some(false) if claimed && edge(x) => {}
            Some(true) if !claimed => {
                bad_out.get_or_insert(x);
            }
            Some(false) if claimed => {
                bad_in.get_or_insert(x);
            }
            _ => {}
        }
    };
    match a.period() {
        None => {
            for &x in xs {
                test(x, set_has(&d.domain, x) && !excluded(x));
            }
        }
        Some(p) => {
            // Given within one period [w, w + p]: a grid of that window,
            // and its copies many periods away.
            let w = d
                .domain
                .iter()
                .map(|iv| iv.lo.value)
                .filter(|v| v.is_finite())
                .fold(f64::INFINITY, f64::min);
            let whole = d
                .domain
                .iter()
                .any(|iv| iv.lo.value == f64::NEG_INFINITY && iv.hi.value == f64::INFINITY);
            let w = if w.is_finite() { w } else { 0.0 };
            for i in 0..2048 {
                let t = w + p * (i as f64 + 0.5) / 2048.0;
                let claimed = (whole || set_has(&d.domain, t)) && !excluded(t);
                for k in [0.0, 1.0, -1.0, 5.0, -17.0, 1000.0] {
                    test(t + k * p, claimed);
                }
            }
            for &x in xs.iter().step_by(7) {
                if whole {
                    test(x, !excluded(x));
                }
            }
        }
    }
    if let Some(x) = bad_in {
        r.fail("domain-includes-undefined", e, detail(x));
    }
    if let Some(x) = bad_out {
        r.fail("domain-excludes-defined", e, detail(x));
    }
}

fn check_monotonicity(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
    let d = a.d();
    if a.unknown(flags::MONOTONE_INTERVALS) || d.monotonicity.is_empty() {
        return;
    }
    let e = &a.expr;
    let ends: Vec<Family> = d
        .monotonicity
        .iter()
        .flat_map(|(iv, _)| [iv.lo.value, iv.hi.value])
        .filter(|v| v.is_finite())
        .map(|v| Family {
            x: v,
            period: a.period(),
        })
        .collect();
    let near = |x: f64| ends.iter().any(|f| close_to(f, x, 4));
    // Each interval: points inside move the claimed way, by more than the
    // noise at both. Against the running extreme, not the last point: many
    // steps each within the noise add up to a resolvable change.
    let contradict = |iv: &Interval, dir: Monotonicity, pts: &[(f64, f64)], r: &mut Report| {
        let found = match dir {
            Monotonicity::Constant => {
                let lo = pts.iter().min_by(|p, q| p.1.total_cmp(&q.1)).copied();
                let hi = pts.iter().max_by(|p, q| p.1.total_cmp(&q.1)).copied();
                match (lo, hi) {
                    (Some(l), Some(h)) if h.1 - l.1 > a.noise(l.0) + a.noise(h.0) => Some((l, h)),
                    _ => None,
                }
            }
            Monotonicity::Increasing | Monotonicity::Decreasing => {
                let sign = if dir == Monotonicity::Increasing {
                    1.0
                } else {
                    -1.0
                };
                let mut best: Option<(f64, f64)> = None;
                let mut out = None;
                for &(x, y) in pts {
                    if let Some((bx, by)) = best
                        && sign * y < sign * by
                        && (by - y).abs() > a.noise(bx) + a.noise(x)
                    {
                        out = Some(((bx, by), (x, y)));
                        break;
                    }
                    if best.is_none_or(|(_, by)| sign * y > sign * by) {
                        best = Some((x, y));
                    }
                }
                out
            }
            Monotonicity::Unknown => None,
        };
        let Some(((x1, y1), (x2, y2))) = found else {
            return false;
        };
        r.fail(
            "monotonicity-contradicted",
            e,
            format!(
                "{dir:?} on ({:?},{:?}) but f({x1:?})={y1:?}, f({x2:?})={y2:?}",
                iv.lo.value, iv.hi.value
            ),
        );
        true
    };
    match a.period() {
        None => {
            // (Over the claimed domain: whether that is right is the
            // domain check's business.)
            let mut gap = None;
            if !a.unknown(flags::DOMAIN) {
                for (&x, &y) in xs.iter().zip(ys) {
                    if y.is_finite()
                        && !near(x)
                        && set_has(&d.domain, x)
                        && !d.excluded.iter().any(|f| in_family(f, x))
                        && !d.monotonicity.iter().any(|(iv, _)| iv_has(iv, x))
                    {
                        gap.get_or_insert(x);
                    }
                }
            }
            if let Some(x) = gap {
                r.fail(
                    "monotonicity-misses-domain",
                    e,
                    format!("x={x:?} in domain but in no monotone interval"),
                );
            }
            for (iv, dir) in &d.monotonicity {
                let pts: Vec<(f64, f64)> = xs
                    .iter()
                    .zip(ys)
                    .filter(|(x, y)| y.is_finite() && iv_has(iv, **x) && !near(**x))
                    .map(|(x, y)| (*x, *y))
                    .collect();
                if contradict(iv, *dir, &pts, r) {
                    break;
                }
            }
        }
        Some(p) => {
            'iv: for (iv, dir) in &d.monotonicity {
                let (lo, hi) = (iv.lo.value, iv.hi.value);
                if !lo.is_finite() || !hi.is_finite() || hi <= lo {
                    continue;
                }
                for k in [0.0, 1.0, -3.0, 1000.0] {
                    let pts: Vec<(f64, f64)> = (0..128)
                        .map(|i| lo + (hi - lo) * (i as f64 + 0.5) / 128.0 + k * p)
                        .filter(|&x| !near(x))
                        .map(|x| (x, a.eval(x)))
                        .filter(|(_, y)| y.is_finite())
                        .collect();
                    if contradict(iv, *dir, &pts, r) {
                        break 'iv;
                    }
                }
            }
        }
    }
}

/// Vertical asymptotes are where f blows up (or is undefined nearby).
fn check_vertical(a: &Analysed, r: &mut Report) {
    if a.unknown(flags::VERTICAL_ASYMPTOTES) {
        return;
    }
    for v in &a.d().vertical_asymptotes {
        for c in a.copies(v) {
            // A few floats out, then ten and a hundred times as far: either
            // side's |f| grows towards x, or f keeps changing by as much
            // per decade nearer (a log pole: ln|x| + 10⁶ shrinks in size
            // towards 0 until e^(−10⁶), but steadily).
            let q = 16.0 * ulp(c);
            let grows = [-1.0, 1.0].iter().any(|&s| {
                // From the first step at which f changes at all: in a
                // shifted frame (1/sin(x − 1000)) f is flat across many
                // floats of x.
                let mut q = q;
                for _ in 0..64 {
                    if a.eval(c + s * q) != a.eval(c + s * 2.0 * q) {
                        break;
                    }
                    q *= 2.0;
                }
                let near = a.eval(c + s * q);
                let mid = a.eval(c + s * q * 10.0);
                let far = a.eval(c + s * q * 100.0);
                let (d1, d2) = (near - mid, mid - far);
                !near.is_finite()
                    || near.abs() == f64::MAX
                    || near.abs() > mid.abs() * 1.2
                    || (d1.signum() == d2.signum() && d1.abs() >= 0.5 * d2.abs() && d2 != 0.0)
            });
            if !grows {
                r.fail(
                    "asymptote-without-blowup",
                    &a.expr,
                    format!(
                        "x={c:?} but f there {:?} and {:?}",
                        a.eval(c - q),
                        a.eval(c + q)
                    ),
                );
                return;
            }
        }
    }
}

/// A horizontal asymptote c on a side: f − c must die away there. One that
/// keeps oscillating at the same size decade after decade, or grows,
/// contradicts it. (Deviations within the noise say nothing.)
fn check_horizontal(a: &Analysed, centres: &[f64], r: &mut Report) {
    use graphing::analysis::AsymptoteSide;
    if a.unknown(flags::HORIZONTAL_ASYMPTOTES) {
        return;
    }
    let e = &a.expr;
    // Far beyond every centre: a function shifted by 10⁹ is only in its
    // tail beyond that.
    let reach = centres.iter().fold(1.0f64, |m, c| m.max(c.abs()));
    let k0 = (10.0 * reach).log10().ceil().max(1.0) as i32;
    for &(c, side) in &a.d().horizontal_asymptotes {
        let sides: &[f64] = match side {
            AsymptoteSide::PositiveInfinity => &[1.0],
            AsymptoteSide::NegativeInfinity => &[-1.0],
            AsymptoteSide::AnyInfinity => &[1.0, -1.0],
            AsymptoteSide::Unknown => &[],
        };
        for &s in sides {
            let mut decades: Vec<(f64, usize)> = Vec::new();
            for k in k0..=(k0 + 8).min(15) {
                let (mut m, mut flips, mut last) = (0.0f64, 0usize, 0.0f64);
                for j in 0..40 {
                    let x = s
                        * 10f64.powf(k as f64 + j as f64 / 40.0)
                        * (1.0 + 0.0137 * (j % 3) as f64);
                    let y = a.eval(x);
                    if !y.is_finite() {
                        continue;
                    }
                    let dev = y - c;
                    if dev.abs() <= a.noise(x) {
                        continue;
                    }
                    m = m.max(dev.abs());
                    if last != 0.0 && dev.signum() != last {
                        flips += 1;
                    }
                    last = dev.signum();
                }
                decades.push((m, flips));
            }
            let live: Vec<(f64, usize)> = decades
                .iter()
                .copied()
                .skip_while(|d| d.0 == 0.0)
                .take_while(|d| d.0 > 0.0)
                .collect();
            if live.len() >= 3
                && live.iter().all(|d| d.1 >= 3)
                && live.last().unwrap().0 >= 0.5 * live[0].0
            {
                r.fail(
                    "hasymptote-oscillation-persists",
                    e,
                    format!(
                        "y={c:?} as x→{}∞ but f−c keeps swinging by {:e} over {} decades",
                        if s > 0.0 { "" } else { "−" },
                        live.last().unwrap().0,
                        live.len()
                    ),
                );
                break;
            }
            let (xm, xf) = (s * (100.0 * reach).max(1e8), s * (1e7 * reach).max(1e15));
            let (mid, far) = (a.eval(xm), a.eval(xf));
            if xf.abs() <= 1e300
                && mid.is_finite()
                && far.is_finite()
                && (far - c).abs() > a.noise(xf) + 2.0 * (mid - c).abs() + a.noise(xm)
            {
                r.fail(
                    "hasymptote-not-approached",
                    e,
                    format!("y={c:?} but f({xm:?})={mid:?}, f({xf:?})={far:?}"),
                );
                break;
            }
        }
    }
}

/// Even or odd as claimed, at every sampled pair ±x.
fn check_parity(a: &Analysed, xs: &[f64], r: &mut Report) {
    if a.unknown(flags::PARITY) {
        return;
    }
    let odd = match a.k.parity {
        Parity::Even => false,
        Parity::Odd => true,
        _ => return,
    };
    let e = &a.expr;
    for &x in xs.iter().filter(|x| **x > 0.0).step_by(3) {
        let (dp, dm) = (a.defined(x), a.defined(-x));
        if let (Some(p), Some(m)) = (dp, dm)
            && p != m
            && (-2..=2).all(|k| a.defined(nudge(x, k)) == dp && a.defined(nudge(-x, k)) == dm)
        {
            r.fail(
                "parity-domain-asymmetric",
                e,
                format!("{:?} but f is defined at only one of ±{x:?}", a.k.parity),
            );
            return;
        }
        if dp == Some(true) && dm == Some(true) {
            let (y, z) = (a.eval(x), a.eval(-x));
            let dev = if odd { (y + z).abs() } else { (y - z).abs() };
            if dev > 0.0 && dev > a.noise(x) + a.noise(-x) {
                r.fail(
                    "parity-contradicted",
                    e,
                    format!("{:?} but f({x:?})={y:?}, f({:?})={z:?}", a.k.parity, -x),
                );
                return;
            }
        }
    }
}

/// A claimed period P: f(x + P) = f(x) wherever either is defined, to
/// within the noise at both, at exact 0, the window's start, every reported
/// point and a few floats round it, a grid over [−100, 100], and a grid
/// over ±64 periods around each centre. One resolvable disagreement
/// disproves P. And P is the smallest: if P/2 or P/3 passed the same test
/// too (f not being constant), P is not the fundamental period.
fn check_period(a: &Analysed, centres: &[f64], r: &mut Report) {
    if a.unknown(flags::PERIODICITY) {
        return;
    }
    let Some(p) = a.period() else {
        return;
    };
    let d = a.d();
    let e = &a.expr;
    let mut pts = vec![0.0];
    let mut fams: Vec<Family> = Vec::new();
    fams.extend(d.zeros.iter().copied());
    fams.extend(
        d.minima
            .iter()
            .chain(&d.maxima)
            .chain(&d.inflection_points)
            .map(|m| m.0),
    );
    fams.extend(d.vertical_asymptotes.iter().chain(&d.excluded).copied());
    for iv in d.domain.iter().chain(d.monotonicity.iter().map(|m| &m.0)) {
        for v in [iv.lo.value, iv.hi.value] {
            if v.is_finite() {
                fams.push(Family::single(v));
            }
        }
    }
    for f in &fams {
        for k in [-64, -2, -1, 0, 1, 2, 64] {
            pts.push(nudge(f.x, k));
        }
    }
    for i in 0..4000 {
        pts.push(-100.0 + 200.0 * (i as f64 + 0.371) / 4000.0);
    }
    let mut cs = vec![0.0];
    cs.extend(centres);
    for c in cs {
        for i in 0..1024 {
            pts.push(c + p * (-64.0 + 128.0 * (i as f64 + 0.5) / 1024.0));
        }
    }
    pts.retain(|x| x.is_finite());
    let test = |q: f64| -> Option<String> {
        for &x in &pts {
            let x2 = x + q;
            match (a.defined(x), a.defined(x2)) {
                (Some(true), Some(true)) => {
                    let (y1, y2) = (a.eval(x), a.eval(x2));
                    // (Beyond the doubles, beside a pole: nothing to compare.)
                    if y1.abs() == f64::MAX || y2.abs() == f64::MAX {
                        continue;
                    }
                    if y1 != y2 && (y1 - y2).abs() > a.noise(x) + a.noise(x2) {
                        // x + P is rounded, and P itself stands for the true
                        // period to half an ulp, which |x/P| copies add up:
                        // within that reach of each point (beside a pole, a
                        // lot of f), do f's values overlap?
                        let reach = 2.0 * (ulp(x2) + ulp(q) * (x2 / q).abs().max(1.0));
                        let around = |t: f64| {
                            (-4..=4)
                                .map(|k| a.eval(t + k as f64 * reach / 4.0))
                                .filter(|v| v.is_finite())
                                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                                    (lo.min(v), hi.max(v))
                                })
                        };
                        let ((l1, h1), (l2, h2)) = (around(x), around(x2));
                        if h1 < l2 || h2 < l1 {
                            return Some(format!("f({x:?})={y1:?} but f({x2:?})={y2:?}"));
                        }
                    }
                }
                (Some(u), Some(v))
                    if u != v
                        && (-2..=2).all(|k| {
                            a.defined(nudge(x2, k)) == Some(v) && a.defined(nudge(x, k)) == Some(u)
                        }) =>
                {
                    return Some(format!(
                        "f is {} at {x:?} but {} at {x2:?}",
                        if u { "defined" } else { "undefined" },
                        if v { "defined" } else { "undefined" }
                    ));
                }
                _ => {}
            }
        }
        None
    };
    if let Some(why) = test(p) {
        r.fail("period-wrong", e, format!("period {p:?}: {why}"));
        return;
    }
    let ys: Vec<f64> = pts
        .iter()
        .map(|&x| a.eval(x))
        .filter(|y| y.is_finite())
        .collect();
    let constant = ys.windows(2).all(|w| w[0] == w[1]);
    if !constant {
        for n in [2.0, 3.0] {
            if test(p / n).is_none() {
                r.fail(
                    "period-not-fundamental",
                    e,
                    format!("period {p:?}, but {:?} is one too", p / n),
                );
                return;
            }
        }
    }
}

/// Known undefined points must be outside the domain and never intercepts.
/// `policy`: the calculator defines these points (0⁰ = 1), so a
/// disagreement is policy, not a false claim.
fn check_undefined(a: &Analysed, points: &[f64], policy: bool, r: &mut Report) {
    let name = |n: &'static str| -> &'static str {
        if !policy {
            return n;
        }
        match n {
            "undefined-point-in-domain" => "policy-0^0-point-in-domain",
            "undefined-y-intercept" => "policy-0^0-y-intercept",
            "undefined-zero" => "policy-0^0-zero",
            _ => "policy-0^0-extremum",
        }
    };
    if !a.ok() {
        // Refusing a function defined nowhere is the safe answer.
        let nowhere = (-200..=200).all(|i| a.defined(i as f64 * 0.37 + 0.01) != Some(true));
        r.fail(
            if nowhere {
                "policy-refused-nowhere-defined"
            } else {
                "undefined-not-analysed"
            },
            &a.expr,
            format!("{:?}", a.k.analysis_error),
        );
        return;
    }
    let d = a.d();
    for &p in points {
        let in_dom = match a.period() {
            Some(per) => {
                let w = d
                    .domain
                    .iter()
                    .map(|iv| iv.lo.value)
                    .filter(|v| v.is_finite())
                    .fold(f64::INFINITY, f64::min);
                let q = if w.is_finite() {
                    w + (p - w).rem_euclid(per)
                } else {
                    p
                };
                set_has(&d.domain, q) && !d.excluded.iter().any(|e| in_family(e, p))
            }
            None => set_has(&d.domain, p) && !d.excluded.iter().any(|e| in_family(e, p)),
        };
        if in_dom && !a.unknown(flags::DOMAIN) {
            r.fail(
                name("undefined-point-in-domain"),
                &a.expr,
                format!(
                    "x={p:?} is undefined but domain {} excl {}",
                    fmt_set(&d.domain),
                    fmt_fams(&d.excluded)
                ),
            );
        }
        if p == 0.0 && d.y_intercept.is_some() {
            r.fail(
                name("undefined-y-intercept"),
                &a.expr,
                format!("y-intercept {:?} at undefined x=0", d.y_intercept),
            );
        }
        if d.zeros.iter().any(|z| close_to(z, p, 4)) {
            r.fail(
                name("undefined-zero"),
                &a.expr,
                format!("x-intercept at undefined x={p:?}"),
            );
        }
        for (fx, y) in d.minima.iter().chain(&d.maxima) {
            if close_to(fx, p, 4) {
                r.fail(
                    name("undefined-extremum"),
                    &a.expr,
                    format!("extremum ({p:?}, {y:?}) at an undefined point"),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Transformed and equivalent functions.

/// Substitute `repl` for the variable x (not the x inside `exp`, `max`, ...).
fn subst_x(expr: &str, repl: &str) -> String {
    let chars: Vec<char> = expr.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let next = chars.get(i + 1).copied();
        if c == 'x'
            && !prev.is_some_and(|p| p.is_ascii_alphabetic())
            && !next.is_some_and(|n| n.is_ascii_alphabetic())
        {
            out.push('(');
            out.push_str(repl);
            out.push(')');
        } else {
            out.push(c);
        }
    }
    out
}

fn num(v: f64) -> String {
    let s = format!("{v}");
    if v < 0.0 { format!("({s})") } else { s }
}

/// y = k·f((x − a)/s) + c: x-features move by x ↦ s·x + a, values by
/// y ↦ k·y + c.
#[derive(Clone, Copy)]
struct Aff {
    a: f64,
    s: f64,
    k: f64,
    c: f64,
}

const ID: Aff = Aff {
    a: 0.0,
    s: 1.0,
    k: 1.0,
    c: 0.0,
};

impl Aff {
    fn apply(self, base: &str) -> String {
        let mut e = base.to_string();
        if self.a != 0.0 || self.s != 1.0 {
            let inner = match (self.a != 0.0, self.s != 1.0) {
                (true, false) => format!("x-{}", num(self.a)),
                (false, true) => format!("x/{}", num(self.s)),
                _ => format!("(x-{})/{}", num(self.a), num(self.s)),
            };
            e = subst_x(&e, &inner);
        }
        if self.k != 1.0 {
            e = format!("{}*({e})", num(self.k));
        }
        if self.c != 0.0 {
            e = format!("({e})+{}", num(self.c));
        }
        e
    }
    fn label(self) -> String {
        let mut v = Vec::new();
        if self.a != 0.0 {
            v.push(format!("shift {}", self.a));
        }
        if self.s != 1.0 {
            v.push(format!("stretch {}", self.s));
        }
        if self.k != 1.0 {
            v.push(format!("scale {}", self.k));
        }
        if self.c != 0.0 {
            v.push(format!("offset {}", self.c));
        }
        v.join(" + ")
    }
    fn mx(self, x: f64) -> f64 {
        self.s * x + self.a
    }
    fn my(self, y: f64) -> f64 {
        self.k * y + self.c
    }
    fn mfam(self, f: &Family) -> Family {
        Family {
            x: self.mx(f.x),
            period: f.period.map(|p| p * self.s.abs()),
        }
    }
}

/// Match two lists of families one to one with `same`, comparing each of
/// the first with the nearest member of each of the second.
fn match_families(a: &[Family], b: &[Family], same: &dyn Fn(f64, f64) -> bool) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut used = vec![false; b.len()];
    for fa in a {
        let hit = (0..b.len()).find(|&j| !used[j] && same(fa.x, nearest(&b[j], fa.x)));
        match hit {
            Some(j) => used[j] = true,
            None => return false,
        }
    }
    true
}

/// The noise at a point where `a` attains the value v, if a reported point
/// does; poles and asymptotic bounds have none.
fn bound_noise(a: &Analysed, v: f64) -> f64 {
    let d = a.d();
    d.minima
        .iter()
        .chain(&d.maxima)
        .chain(&d.inflection_points)
        .find(|(_, y)| *y == v)
        .map(|(f, _)| a.noise(f.x))
        .or_else(|| (d.y_intercept == Some(v)).then(|| a.noise(0.0)))
        .unwrap_or(0.0)
}

/// Two value sets agree: same pieces, same closedness, bounds within the
/// noise where they are attained (each side's own, and `ka` times the
/// first's for a scaled copy) and a few units in the last place.
fn same_values(want: &[Interval], wa: &Analysed, ka: f64, got: &[Interval], ga: &Analysed) -> bool {
    want.len() == got.len()
        && want.iter().zip(got).all(|(w, g)| {
            [(w.lo, g.lo), (w.hi, g.hi)].iter().all(|(bw, bg)| {
                if bw.value.is_infinite() || bg.value.is_infinite() {
                    return bw.value == bg.value;
                }
                bw.closed == bg.closed
                    && (same_shown(bw.value, bg.value)
                        || (bw.value - bg.value).abs()
                            <= ka.abs() * bound_noise(wa, bw.value)
                                + bound_noise(ga, bg.value)
                                + 8.0 * ulp(bw.value.abs().max(bg.value.abs())))
            })
        })
}

fn check_pair(b: &Analysed, o: &Analysed, t: Aff, r: &mut Report) {
    if !b.ok() || !o.ok() {
        return;
    }
    let (bd, od) = (b.d(), o.d());
    let e = &o.expr;
    let both = |flag: u32| !b.unknown(flag) && !o.unknown(flag);
    let flip = t.k < 0.0;
    if both(flags::PERIODICITY) {
        match (b.period(), o.period()) {
            (Some(pb), Some(po)) if (pb * t.s.abs() - po).abs() > 64.0 * ulp(po) => r.fail(
                "transform-period-differs",
                e,
                format!("{}: base period {pb:?}, got {po:?}", t.label()),
            ),
            (Some(_), None) | (None, Some(_)) => r.fail(
                "transform-periodicity-differs",
                e,
                format!(
                    "{}: base period {:?}, got {:?}",
                    t.label(),
                    b.period(),
                    o.period()
                ),
            ),
            _ => {}
        }
    }
    let mapped = |v: &[Family]| v.iter().map(|f| t.mfam(f)).collect::<Vec<_>>();
    if both(flags::ZEROS) && t.c == 0.0 {
        if b.zero_set() != o.zero_set() || b.zero_everywhere() != o.zero_everywhere() {
            r.fail(
                "transform-zeros-differ",
                e,
                format!(
                    "{}: base \"{}\", got \"{}\"",
                    t.label(),
                    b.k.x_intercept,
                    o.k.x_intercept
                ),
            );
        } else if !match_families(&mapped(&bd.zeros), &od.zeros, &|m, z| same_root(o, m, z)) {
            r.fail(
                "transform-zeros-differ",
                e,
                format!(
                    "{}: base zeros {} map to {}, got {}",
                    t.label(),
                    fmt_fams(&bd.zeros),
                    fmt_fams(&mapped(&bd.zeros)),
                    fmt_fams(&od.zeros)
                ),
            );
        }
    }
    if both(flags::VERTICAL_ASYMPTOTES)
        && !match_families(
            &mapped(&bd.vertical_asymptotes),
            &od.vertical_asymptotes,
            &|m, z| floats_between(m, z) <= 64 || (m - z).abs() <= 4.0 * ulp(t.a),
        )
    {
        r.fail(
            "transform-poles-differ",
            e,
            format!(
                "{}: base poles {} map to {}, got {}",
                t.label(),
                fmt_fams(&bd.vertical_asymptotes),
                fmt_fams(&mapped(&bd.vertical_asymptotes)),
                fmt_fams(&od.vertical_asymptotes)
            ),
        );
    }
    // Extrema: x mapped, y mapped (minima ↔ maxima for a negative scale).
    for (bl, ol, bf, of, sign, name) in [
        (
            &bd.minima,
            if flip { &od.maxima } else { &od.minima },
            flags::MINIMA,
            if flip { flags::MAXIMA } else { flags::MINIMA },
            if flip { -1.0 } else { 1.0 },
            "minima",
        ),
        (
            &bd.maxima,
            if flip { &od.minima } else { &od.maxima },
            flags::MAXIMA,
            if flip { flags::MINIMA } else { flags::MAXIMA },
            if flip { 1.0 } else { -1.0 },
            "maxima",
        ),
    ] {
        if b.unknown(bf) || o.unknown(of) {
            continue;
        }
        let bfam: Vec<Family> = bl.iter().map(|m| t.mfam(&m.0)).collect();
        let ofam: Vec<Family> = ol.iter().map(|m| m.0).collect();
        if !match_families(&bfam, &ofam, &|m, z| same_turn(o, m, z, sign)) {
            r.fail(
                "transform-extrema-differ",
                e,
                format!(
                    "{}: base {name} {} map to {}, got {}",
                    t.label(),
                    fmt_fams(&bl.iter().map(|m| m.0).collect::<Vec<_>>()),
                    fmt_fams(&bfam),
                    fmt_fams(&ofam)
                ),
            );
            continue;
        }
        // Values: the mapped base value, to within the noise at both.
        for (bm, mf) in bl.iter().zip(&bfam) {
            let want = t.my(bm.1);
            let Some(om) = ol
                .iter()
                .find(|om| same_turn(o, mf.x, nearest(&om.0, mf.x), sign))
            else {
                continue;
            };
            let tol = t.k.abs() * b.noise(bm.0.x) + o.noise(om.0.x) + 8.0 * ulp(want);
            if (want - om.1).abs() > tol && !same_shown(want, om.1) {
                r.fail(
                    "transform-extremum-y-differs",
                    e,
                    format!("{}: {name} y want {want}, got {}", t.label(), om.1),
                );
                break;
            }
        }
    }
    if both(flags::RANGE) && !bd.range.is_empty() && !od.range.is_empty() {
        let mut want: Vec<Interval> = bd
            .range
            .iter()
            .map(|iv| {
                let (mut lo, mut hi) = (iv.lo, iv.hi);
                lo.value = t.my(lo.value);
                hi.value = t.my(hi.value);
                if flip {
                    std::mem::swap(&mut lo, &mut hi);
                }
                Interval { lo, hi }
            })
            .collect();
        if flip {
            want.reverse();
        }
        // The base's bound noise, scaled: compare against the base's own
        // reported points.
        let ok = want.len() == od.range.len()
            && want
                .iter()
                .zip(&bd.range.iter().rev().collect::<Vec<_>>())
                .zip(&od.range)
                .all(|((w, _), g)| {
                    [(w.lo, g.lo), (w.hi, g.hi)].iter().all(|(bw, bg)| {
                        if bw.value.is_infinite() || bg.value.is_infinite() {
                            return bw.value == bg.value;
                        }
                        let base_v = (bw.value - t.c) / t.k;
                        bw.closed == bg.closed
                            && (same_shown(bw.value, bg.value)
                                || (bw.value - bg.value).abs()
                                    <= t.k.abs() * bound_noise(b, base_v)
                                        + bound_noise(o, bg.value)
                                        + 8.0 * ulp(bw.value.abs().max(bg.value.abs())))
                    })
                });
        if !ok {
            r.fail(
                "transform-range-differs",
                e,
                format!(
                    "{}: want {}, got {}",
                    t.label(),
                    fmt_set(&want),
                    fmt_set(&od.range)
                ),
            );
        }
    }
    if both(flags::HORIZONTAL_ASYMPTOTES) {
        let want: Vec<f64> = bd.horizontal_asymptotes.iter().map(|h| t.my(h.0)).collect();
        let got: Vec<f64> = od.horizontal_asymptotes.iter().map(|h| h.0).collect();
        let ok = want.len() == got.len()
            && want.iter().all(|w| {
                got.iter()
                    .any(|g| same_shown(*w, *g) || (w - g).abs() <= 8.0 * ulp(w.abs().max(g.abs())))
            });
        if !ok {
            r.fail(
                "transform-hasymptotes-differ",
                e,
                format!("{}: want {want:?}, got {got:?}", t.label()),
            );
        }
    }
    if both(flags::DOMAIN) && b.period().is_none() && o.period().is_none() {
        let want: Vec<(f64, bool, f64, bool)> = bd
            .domain
            .iter()
            .map(|iv| {
                (
                    t.mx(iv.lo.value),
                    iv.lo.closed,
                    t.mx(iv.hi.value),
                    iv.hi.closed,
                )
            })
            .collect();
        let ok = want.len() == od.domain.len()
            && want.iter().zip(&od.domain).all(|(w, g)| {
                let same = |p: f64, q: f64| {
                    p == q || (p.is_finite() && q.is_finite() && floats_between(p, q) <= 64)
                };
                same(w.0, g.lo.value)
                    && same(w.2, g.hi.value)
                    && (w.0.is_infinite() || w.1 == g.lo.closed)
                    && (w.2.is_infinite() || w.3 == g.hi.closed)
            })
            && match_families(&mapped(&bd.excluded), &od.excluded, &|m, z| {
                floats_between(m, z) <= 64
            });
        if !ok {
            r.fail(
                "transform-domain-differs",
                e,
                format!(
                    "{}: base {} excl {}, got {} excl {}",
                    t.label(),
                    fmt_set(&bd.domain),
                    fmt_fams(&bd.excluded),
                    fmt_set(&od.domain),
                    fmt_fams(&od.excluded)
                ),
            );
        }
    }
}

/// Two expressions for one function: every definite result agrees.
fn check_equiv(p: &Analysed, q: &Analysed, r: &mut Report) {
    let e = format!("{}  ≡  y={}", p.expr, q.expr);
    if p.ok() != q.ok() {
        r.fail(
            "equivalent-refused-differ",
            &e,
            format!("{:?} vs {:?}", p.k.analysis_error, q.k.analysis_error),
        );
        return;
    }
    if !p.ok() {
        return;
    }
    let (pd, qd) = (p.d(), q.d());
    let both = |flag: u32| !p.unknown(flag) && !q.unknown(flag);
    let mut differ = |what: &'static str, a: String, b: String| {
        r.fail(what, &e, format!("{a} vs {b}"));
    };
    // (A periodic domain is given within one window: compare sets only
    // when neither is; the samples check each against f.)
    if both(flags::DOMAIN)
        && p.period().is_none()
        && q.period().is_none()
        && (fmt_set(&pd.domain) != fmt_set(&qd.domain)
            || !match_families(&pd.excluded, &qd.excluded, &|m, z| {
                floats_between(m, z) <= 64
            }))
    {
        differ(
            "equivalent-domain-differ",
            format!("{} excl {}", fmt_set(&pd.domain), fmt_fams(&pd.excluded)),
            format!("{} excl {}", fmt_set(&qd.domain), fmt_fams(&qd.excluded)),
        );
    }
    if both(flags::RANGE) && !same_values(&pd.range, p, 1.0, &qd.range, q) {
        differ(
            "equivalent-range-differ",
            fmt_set(&pd.range),
            fmt_set(&qd.range),
        );
    }
    if both(flags::ZEROS)
        && (p.zero_everywhere() != q.zero_everywhere()
            || p.zero_set() != q.zero_set()
            || !match_families(&pd.zeros, &qd.zeros, &|m, z| same_root(p, m, z)))
    {
        differ(
            "equivalent-zeros-differ",
            format!("\"{}\" {}", p.k.x_intercept, fmt_fams(&pd.zeros)),
            format!("\"{}\" {}", q.k.x_intercept, fmt_fams(&qd.zeros)),
        );
    }
    for (pl, ql, flag, sign, what) in [
        (
            &pd.minima,
            &qd.minima,
            flags::MINIMA,
            1.0,
            "equivalent-minima-differ",
        ),
        (
            &pd.maxima,
            &qd.maxima,
            flags::MAXIMA,
            -1.0,
            "equivalent-maxima-differ",
        ),
    ] {
        let pf: Vec<Family> = pl.iter().map(|m| m.0).collect();
        let qf: Vec<Family> = ql.iter().map(|m| m.0).collect();
        if both(flag) && !match_families(&pf, &qf, &|m, z| same_turn(p, m, z, sign)) {
            differ(what, fmt_fams(&pf), fmt_fams(&qf));
        }
    }
    if both(flags::VERTICAL_ASYMPTOTES)
        && !match_families(&pd.vertical_asymptotes, &qd.vertical_asymptotes, &|m, z| {
            floats_between(m, z) <= 64
        })
    {
        differ(
            "equivalent-poles-differ",
            fmt_fams(&pd.vertical_asymptotes),
            fmt_fams(&qd.vertical_asymptotes),
        );
    }
    if both(flags::HORIZONTAL_ASYMPTOTES) {
        let (ph, qh): (Vec<f64>, Vec<f64>) = (
            pd.horizontal_asymptotes.iter().map(|h| h.0).collect(),
            qd.horizontal_asymptotes.iter().map(|h| h.0).collect(),
        );
        if ph.len() != qh.len()
            || !ph.iter().all(|a| {
                qh.iter()
                    .any(|b| same_shown(*a, *b) || (a - b).abs() <= 8.0 * ulp(a.abs().max(b.abs())))
            })
        {
            differ(
                "equivalent-hasymptotes-differ",
                format!("{ph:?}"),
                format!("{qh:?}"),
            );
        }
    }
    if both(flags::PARITY) && p.k.parity != q.k.parity {
        differ(
            "equivalent-parity-differ",
            format!("{:?}", p.k.parity),
            format!("{:?}", q.k.parity),
        );
    }
    // (Every number is a period of a constant.)
    let constant = |a: &Analysed| {
        !a.unknown(flags::RANGE) && a.d().range.iter().all(|iv| iv.lo.value == iv.hi.value)
    };
    if both(flags::PERIODICITY)
        && !(constant(p) || constant(q))
        && match (p.period(), q.period()) {
            (Some(a), Some(b)) => floats_between(a, b) > 64,
            (None, None) => false,
            _ => true,
        }
    {
        differ(
            "equivalent-period-differ",
            format!("{:?}", p.period()),
            format!("{:?}", q.period()),
        );
    }
    if both(flags::Y_INTERCEPT)
        && match (pd.y_intercept, qd.y_intercept) {
            (Some(a), Some(b)) => {
                (a - b).abs() > p.noise(0.0) + q.noise(0.0) + 4.0 * ulp(a) && !same_shown(a, b)
            }
            (None, None) => false,
            _ => true,
        }
    {
        differ(
            "equivalent-y-intercept-differ",
            format!("{:?}", pd.y_intercept),
            format!("{:?}", qd.y_intercept),
        );
    }
}

/// Definite claims against stated facts.
fn check_known(a: &Analysed, k: &Known, r: &mut Report) {
    let e = &a.expr;
    if !a.ok() {
        r.fail("known-refused", e, format!("{:?}", a.k.analysis_error));
        return;
    }
    let d = a.d();
    if let Some(except) = &k.domain_except
        && !a.unknown(flags::DOMAIN)
    {
        let claimed = |x: f64| set_has(&d.domain, x) && !d.excluded.iter().any(|f| in_family(f, x));
        let mut probes: Vec<f64> = (-2000..=2000).map(|i| i as f64 * 0.37 + 0.011).collect();
        for p in 0..=15 {
            probes.push(10f64.powi(p));
            probes.push(-(10f64.powi(p)));
        }
        for &p in except {
            for h in [1e-9, 1e-3, 1.0] {
                probes.push(p + h * p.abs().max(1.0));
                probes.push(p - h * p.abs().max(1.0));
            }
        }
        let wrong_in = except.iter().find(|&&p| claimed(p));
        let wrong_out = probes
            .iter()
            .find(|&&x| !except.contains(&x) && !claimed(x));
        if let Some(p) = wrong_in {
            r.fail(
                "known-domain-wrong",
                e,
                format!(
                    "{p:?} is excluded, but domain {} excl {}",
                    fmt_set(&d.domain),
                    fmt_fams(&d.excluded)
                ),
            );
        } else if let Some(x) = wrong_out {
            r.fail(
                "known-domain-wrong",
                e,
                format!(
                    "{x:?} is in the domain, but domain {} excl {}",
                    fmt_set(&d.domain),
                    fmt_fams(&d.excluded)
                ),
            );
        }
    }
    if let Some(range) = &k.range
        && !a.unknown(flags::RANGE)
    {
        let same = |w: f64, g: f64| {
            if w.is_infinite() || g.is_infinite() {
                w == g
            } else {
                same_shown(w, g) || (w - g).abs() <= 1e-9 * w.abs().max(g.abs())
            }
        };
        let ok = range.len() == d.range.len()
            && range.iter().zip(&d.range).all(|(w, g)| {
                same(w.0, g.lo.value)
                    && same(w.2, g.hi.value)
                    && (w.0.is_infinite() || w.1 == g.lo.closed)
                    && (w.2.is_infinite() || w.3 == g.hi.closed)
            });
        if !ok {
            let want: Vec<Interval> = range
                .iter()
                .map(|w| Interval {
                    lo: graphing::analysis::Bound {
                        value: w.0,
                        closed: w.1,
                    },
                    hi: graphing::analysis::Bound {
                        value: w.2,
                        closed: w.3,
                    },
                })
                .collect();
            r.fail(
                "known-range-wrong",
                e,
                format!("range is {}, claimed {}", fmt_set(&want), fmt_set(&d.range)),
            );
        }
    }
    if let Some(per) = k.period
        && !a.unknown(flags::PERIODICITY)
    {
        let ok = match (per, a.period()) {
            (None, None) => a.k.periodicity_direction != Periodicity::Periodic,
            (Some(p), Some(q)) => (p - q).abs() <= 1e-9 * p,
            _ => false,
        };
        if !ok {
            r.fail(
                "known-period-wrong",
                e,
                format!("period is {per:?}, claimed {:?}", a.period()),
            );
        }
    }
    if let Some(zs) = &k.zeros
        && !a.unknown(flags::ZEROS)
    {
        let ok = match zs {
            Zs::No => d.zeros.is_empty() && !a.zero_set(),
            Zs::All => a.zero_everywhere() || (a.zero_set() && d.zeros.is_empty()),
            Zs::At(v) => {
                !a.zero_set()
                    && d.zeros.len() == v.len()
                    && v.iter().all(|x| {
                        d.zeros
                            .iter()
                            .any(|z| floats_between(nearest(z, *x), *x) <= 64)
                    })
            }
        };
        if !ok {
            let want = match zs {
                Zs::No => "none".to_string(),
                Zs::All => "every x of the domain".to_string(),
                Zs::At(v) => format!("{v:?}"),
            };
            r.fail(
                "known-zeros-wrong",
                e,
                format!(
                    "x-intercepts are {want}, claimed \"{}\" {}",
                    a.k.x_intercept,
                    fmt_fams(&d.zeros)
                ),
            );
        }
    }
    if k.no_poles && !a.unknown(flags::VERTICAL_ASYMPTOTES) && !d.vertical_asymptotes.is_empty() {
        r.fail(
            "known-poles-wrong",
            e,
            format!(
                "no vertical asymptotes, claimed {}",
                fmt_fams(&d.vertical_asymptotes)
            ),
        );
    }
    if let Some(h) = &k.hasymptotes
        && !a.unknown(flags::HORIZONTAL_ASYMPTOTES)
    {
        let got: Vec<f64> = d.horizontal_asymptotes.iter().map(|h| h.0).collect();
        let ok = h.iter().all(|w| got.iter().any(|g| same_shown(*w, *g)))
            && got.iter().all(|g| h.iter().any(|w| same_shown(*w, *g)));
        if !ok {
            r.fail(
                "known-hasymptotes-wrong",
                e,
                format!("horizontal asymptotes are {h:?}, claimed {got:?}"),
            );
        }
    }
}

/// Where the compiled program gives a finite nonzero value, the reference
/// must agree to the last bit; a disagreement is a bug in the reference.
fn check_reference(a: &Analysed, centres: &[f64], r: &mut Report) {
    for x in samples(a, centres, false) {
        let y = a.raw(x);
        // (Gradual underflow of an intermediate rounds the compiled value;
        // the reference keeps it exact.)
        if !y.is_finite() || y == 0.0 || y.abs() < 1e-290 || x.abs() < 1e-290 {
            continue;
        }
        match a.reference(x) {
            R::V(v) if v.f() == y => {}
            R::V(v) if (v.f() - y).abs() <= 4.0 * ulp(y) => {}
            other => {
                r.fail(
                    "reference-disagrees",
                    &a.expr,
                    format!("f({x:?}) compiled {y:?}, reference {other:?}"),
                );
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `--at EXPR X`: one function at one point, for looking into a report.
    if args.first().is_some_and(|a| a == "--at") && args.len() >= 3 {
        let unit = match args.get(3).map(String::as_str) {
            Some("deg") => TrigUnit::Degrees,
            Some("grad") => TrigUnit::Grads,
            _ => TrigUnit::Radians,
        };
        let a = Analysed::new(&args[1], unit).expect("parses");
        let x: f64 = args[2].parse().expect("a number");
        println!("ast {:?}", a.ast);
        println!(
            "raw {:?} eval {:?} reference {:?} bound {:?} noise {:?}",
            a.raw(x),
            a.eval(x),
            a.reference(x),
            eb(&a.ast, x, a.unit),
            a.noise(x)
        );
        return;
    }
    let filter = args.iter().find(|a| !a.starts_with("--")).cloned();
    let check_ref = args.iter().any(|a| a == "--check-ref");
    let wanted = |e: &str| filter.as_ref().is_none_or(|f| e.contains(f.as_str()));
    let mut r = Report {
        failures: BTreeMap::new(),
    };
    let started = std::time::Instant::now();
    let mut count = 0usize;
    let mut slowest = (0.0f64, String::new());
    let mut coverage: Vec<(String, Coverage)> = Vec::new();
    let mut special_unknown: Vec<String> = Vec::new();
    let mut analyse = |expr: &str,
                       unit: TrigUnit,
                       r: &mut Report,
                       count: &mut usize,
                       cov: &mut Coverage|
     -> Option<Analysed> {
        let t0 = std::time::Instant::now();
        let Some(a) = Analysed::new(expr, unit) else {
            r.fail("does-not-parse", expr, String::new());
            return None;
        };
        let dt = t0.elapsed().as_secs_f64() * 1e3;
        if dt > slowest.0 {
            slowest = (dt, expr.to_string());
        }
        *count += 1;
        cov.add(&a);
        Some(a)
    };
    let mut transforms: Vec<Aff> = Vec::new();
    for a in [1.0, 1e3, 1e6, 1e9, 1e12] {
        transforms.push(Aff { a, ..ID });
    }
    for c in [
        1e-12, -1e-12, 1e-9, -1e-9, 1e-7, -1e-7, 1e-3, -1e-3, 1.0, -1.0, 1e6, -1e6,
    ] {
        transforms.push(Aff { c, ..ID });
    }
    for k in [1e-12, -1e-12, 1e-6, -1e-6, -1.0, 1e6, 1e12] {
        transforms.push(Aff { k, ..ID });
    }
    for s in [1e-3, 1e3, 1e6] {
        transforms.push(Aff { s, ..ID });
    }
    // Combined: shift, stretch, scale and offset at once.
    transforms.push(Aff {
        a: 1000.0,
        s: 1.0,
        k: -2.0,
        c: 1.0,
    });
    transforms.push(Aff {
        a: 1e9,
        s: 1000.0,
        k: 1.0,
        c: -1.0,
    });
    transforms.push(Aff {
        a: -1e6,
        s: 1.0,
        k: 1e-6,
        c: 1e6,
    });
    for base in BASES {
        if !wanted(base) {
            continue;
        }
        let mut cov = Coverage::default();
        let Some(b) = analyse(base, TrigUnit::Radians, &mut r, &mut count, &mut cov) else {
            continue;
        };
        if check_ref {
            check_reference(&b, &[0.0], &mut r);
        }
        check_one(&b, &[0.0], false, &mut r);
        for &t in &transforms {
            let expr = t.apply(base);
            let Some(o) = analyse(&expr, TrigUnit::Radians, &mut r, &mut count, &mut cov) else {
                continue;
            };
            if check_ref {
                check_reference(&o, &[t.a], &mut r);
            }
            check_one(&o, &[t.a], false, &mut r);
            check_pair(&b, &o, t, &mut r);
        }
        coverage.push((base.to_string(), cov));
    }
    let mut cov = Coverage::default();
    for (expr, unit, points, policy) in UNDEFINED {
        if !wanted(expr) {
            continue;
        }
        if let Some(a) = analyse(expr, *unit, &mut r, &mut count, &mut cov) {
            let tagged = Analysed {
                expr: format!("{expr}  [{unit:?}]"),
                ..a
            };
            check_one(&tagged, &[0.0], false, &mut r);
            check_undefined(&tagged, points, *policy, &mut r);
        }
    }
    coverage.push(("(undefined points)".to_string(), cov));
    let mut cov = Coverage::default();
    let mut cache: BTreeMap<String, Analysed> = BTreeMap::new();
    for s in specials() {
        if !wanted(&s.expr) {
            continue;
        }
        if let Some(a) = analyse(&s.expr, TrigUnit::Radians, &mut r, &mut count, &mut cov) {
            if check_ref {
                check_reference(&a, &s.centres, &mut r);
            }
            check_one(&a, &s.centres, s.wide, &mut r);
            let unknown: Vec<&str> = FEATURES
                .iter()
                .filter(|(f, _)| a.unknown(*f))
                .map(|(_, n)| *n)
                .collect();
            if !a.ok() {
                special_unknown.push(format!("y={}: refused ({:?})", s.expr, a.k.analysis_error));
            } else if !unknown.is_empty() {
                special_unknown.push(format!("y={}: unknown {}", s.expr, unknown.join(", ")));
            }
            cache.insert(s.expr.clone(), a);
        }
    }
    coverage.push(("(hand-written families)".to_string(), cov));
    let mut cov = Coverage::default();
    for (p, q) in equivalent() {
        if !wanted(&p) && !wanted(&q) {
            continue;
        }
        for e in [&p, &q] {
            if !cache.contains_key(e)
                && let Some(a) = analyse(e, TrigUnit::Radians, &mut r, &mut count, &mut cov)
            {
                check_one(&a, &[0.0], false, &mut r);
                cache.insert(e.clone(), a);
            }
        }
        if let (Some(a), Some(b)) = (cache.get(&p), cache.get(&q)) {
            check_equiv(a, b, &mut r);
        }
    }
    coverage.push(("(equivalence partners)".to_string(), cov));
    for k in known_cases() {
        if !wanted(k.expr) {
            continue;
        }
        if !cache.contains_key(k.expr) {
            let mut cov = Coverage::default();
            if let Some(a) = analyse(k.expr, TrigUnit::Radians, &mut r, &mut count, &mut cov) {
                check_one(&a, &[0.0], false, &mut r);
                cache.insert(k.expr.to_string(), a);
            }
        }
        if let Some(a) = cache.get(k.expr) {
            check_known(a, &k, &mut r);
        }
    }

    let total: usize = r.failures.values().map(Vec::len).sum();
    let real: usize = r
        .failures
        .iter()
        .filter(|(c, _)| !c.starts_with("policy-") && !c.starts_with("unverifiable-"))
        .map(|(_, v)| v.len())
        .sum();
    println!(
        "{count} functions analysed in {:.1} s (slowest {:.0} ms: y={}); {total} failures, {} of them policy or unverifiable",
        started.elapsed().as_secs_f64(),
        slowest.0,
        slowest.1,
        total - real
    );
    // One line per (check, expression), with a count, unless --all.
    let all = args.iter().any(|a| a == "--all");
    let mut checks: Vec<(&&str, &Vec<String>)> = r.failures.iter().collect();
    checks.sort_by_key(|(c, _)| (c.starts_with("unverifiable-"), c.starts_with("policy-")));
    for (check, list) in checks {
        let mut by_expr: Vec<(&str, &str, usize)> = Vec::new();
        for line in list {
            let (expr, detail) = line.split_once("    ").unwrap_or((line, ""));
            match by_expr.iter_mut().find(|(e, ..)| *e == expr) {
                Some(entry) => entry.2 += 1,
                None => by_expr.push((expr, detail, 1)),
            }
        }
        println!(
            "\n## {check}: {} failures in {} functions",
            list.len(),
            by_expr.len()
        );
        if all {
            for line in list {
                println!("  {line}");
            }
        } else {
            for (expr, detail, n) in &by_expr {
                println!("  {expr}  [{n}]  {detail}");
            }
        }
    }
    // Coverage: how many answers were definite.
    let mut sum = Coverage::default();
    for (_, c) in &coverage {
        sum.merge(c);
    }
    let analysed = sum.fns - sum.refused;
    println!(
        "\n## coverage: {} functions, {} refused; per feature, definite / unknown of {analysed} analysed",
        sum.fns, sum.refused
    );
    for (i, (_, name)) in FEATURES.iter().enumerate() {
        println!(
            "  {name:<9} {:>6} / {:<6} ({:.1}% definite)",
            analysed - sum.unknown[i],
            sum.unknown[i],
            100.0 * (analysed - sum.unknown[i]) as f64 / analysed.max(1) as f64
        );
    }
    println!("\n## coverage by base (functions, refused, unknown answers by feature)");
    for (name, c) in &coverage {
        let parts: Vec<String> = FEATURES
            .iter()
            .enumerate()
            .filter(|(i, _)| c.unknown[*i] > 0)
            .map(|(i, (_, n))| format!("{n} {}", c.unknown[i]))
            .collect();
        println!(
            "  {name:<36} {:>3} fns, {} refused, {} unknown{}{}",
            c.fns,
            c.refused,
            c.unknown_total(),
            if parts.is_empty() { "" } else { ": " },
            parts.join(", ")
        );
    }
    if !special_unknown.is_empty() {
        println!("\n## hand-written functions with unknown answers");
        for l in &special_unknown {
            println!("  {l}");
        }
    }
    if real > 0 {
        std::process::exit(1);
    }
}
