//! Adversarial invariant sweep over function analysis (not run in CI).
//!
//! `cargo run --release -p graphing --example sweep [-- FILTER] [--all] [--check-ref]`
//! (`--check-ref` also checks the reference evaluator against the compiled
//! program; `-- --at EXPR X [deg|grad]` shows one function at one point.)
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
use std::f64::consts::PI;

use graphing::analysis::truth::*;
use graphing::analysis::verify::*;
use graphing::analysis::{Family, Interval, Periodicity, analyze, flags};
use graphing::compile::CompileOptions;
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
        // (Its range holds 0: zeros at −1 + kπ.)
        Known {
            range: Some(vec![(1.0 - PI / 2.0, false, 1.0 + PI / 2.0, false)]),
            period: Some(Some(PI)),
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

// ---------------------------------------------------------------------------
// Report and coverage.

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

/// Every check on one analysed function on its own, at the sweep's
/// samples.
fn check_one(a: &Analysed, centres: &[f64], wide: bool, r: &mut Report) {
    if !a.ok() {
        return;
    }
    let xs = samples(a, centres, wide);
    let ys: Vec<f64> = xs.iter().map(|&x| a.eval(x)).collect();
    check_all(a, &xs, &ys, centres, r);
}

/// A failure of a sweep-only check (no feature to forget).
fn fail(r: &mut Report, check: &'static str, expr: &str, detail: String) {
    r.fail(check, 0, expr, detail);
}

/// Parses and analyses `y = expr`.
fn analysed(expr: &str, unit: TrigUnit) -> Option<Analysed<'static>> {
    let opts = CompileOptions {
        trig_unit: unit,
        ..CompileOptions::default()
    };
    let eq = Equation::parse(&format!("y={expr}")).ok()?;
    let ast = eq.explicit()?.1.clone();
    let k = analyze(&eq, &opts);
    Some(Analysed::new(expr, k, &ast, &opts))
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
        fail(
            r,
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
            fail(
                r,
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
            fail(
                r,
                name("undefined-y-intercept"),
                &a.expr,
                format!("y-intercept {:?} at undefined x=0", d.y_intercept),
            );
        }
        if d.zeros.iter().any(|z| close_to(z, p, 4)) {
            fail(
                r,
                name("undefined-zero"),
                &a.expr,
                format!("x-intercept at undefined x={p:?}"),
            );
        }
        for (fx, y) in d.minima.iter().chain(&d.maxima) {
            if close_to(fx, p, 4) {
                fail(
                    r,
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
            (Some(pb), Some(po)) if (pb * t.s.abs() - po).abs() > 64.0 * ulp(po) => fail(
                r,
                "transform-period-differs",
                e,
                format!("{}: base period {pb:?}, got {po:?}", t.label()),
            ),
            (Some(_), None) | (None, Some(_)) => fail(
                r,
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
            fail(
                r,
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
            fail(
                r,
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
        fail(
            r,
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
            fail(
                r,
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
                fail(
                    r,
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
            fail(
                r,
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
            fail(
                r,
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
            fail(
                r,
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
        fail(
            r,
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
        fail(r, what, &e, format!("{a} vs {b}"));
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
        fail(r, "known-refused", e, format!("{:?}", a.k.analysis_error));
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
            fail(
                r,
                "known-domain-wrong",
                e,
                format!(
                    "{p:?} is excluded, but domain {} excl {}",
                    fmt_set(&d.domain),
                    fmt_fams(&d.excluded)
                ),
            );
        } else if let Some(x) = wrong_out {
            fail(
                r,
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
            fail(
                r,
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
            fail(
                r,
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
            fail(
                r,
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
        fail(
            r,
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
            fail(
                r,
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
            // (Where the plain doubles overflow, both evaluate through an
            // extended exponent: e^t at t ≈ 700 is good to 10⁻¹³ there.)
            R::V(v)
                if !eb(&a.ast, x, a.unit).0.is_finite() && (v.f() - y).abs() <= 1e-12 * y.abs() => {
            }
            // (The reference abstaining is no disagreement: f falls back to
            // the compiled value.)
            R::Unknown => {}
            other => {
                fail(
                    r,
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
        let a = analysed(&args[1], unit).expect("parses");
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
    let mut r = Report::default();
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
        let Some(a) = analysed(expr, unit) else {
            fail(r, "does-not-parse", expr, String::new());
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
            let mut tagged = a;
            tagged.expr = format!("{expr}  [{unit:?}]");
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
