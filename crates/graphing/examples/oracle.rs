//! An independent consistency check of what function analysis shows (not
//! run in CI).
//!
//! `cargo run --release -p graphing --example oracle [-- FILTER] [--all]`
//! checks the pool (or the functions whose text contains FILTER) and exits
//! with status 1 if it finds a contradiction. `-- --dump EXPR` prints one
//! function's analysis and findings, with the same exit status. `--
//! --selftest` plants known false answers into real analyses and exits
//! with status 1 unless every check catches its own.
//!
//! The sweep (`examples/sweep.rs`) runs the runtime gate's own checks, so a
//! clean sweep can't vouch for those checks. This one shares none of them:
//! it reads only the public analysis result (what the panel shows, and the
//! numbers behind it), the panel's formatter (`format_nonzero`), the
//! compiled program, and the reference evaluator
//! `analysis::truth::reval_typed` for *values* (a double with an unbounded
//! exponent where an intermediate leaves the doubles), both reading the
//! literals as the decimals typed, as the app does. Nothing from
//! `analysis::verify` is used: no noise estimate, rounding bound, sample
//! set, rule or tolerance of the gate's. What it does share with the app is
//! arithmetic: CORE-MATH's functions (`crates/crmath`), `dd.rs` under both
//! evaluators, and the exact folding of literal arithmetic (`big.rs`,
//! exact rationals rounded once). A wrong primitive
//! both use agrees with itself, so the pool also compares equivalent
//! spellings (below).
//!
//! # What is checked
//!
//! Every *definite* answer, against f sampled densely near 0 (steps of
//! 1/64 to ±10, 1/4 to ±100, integers to ±2000), log-spaced (24 a decade)
//! from 10⁻⁸ out to 10¹⁵, around the value of every x-free subtree of the
//! expression and its negative (at most 64, each with ±1 … 2²⁰ floats,
//! 10⁻¹² … 0.5 relative and 0.01 … 100 absolute steps, and a 1/16 grid
//! over ±8), and around every reported feature (at most 256). A
//! contradiction is reported by class:
//!
//! * the y-intercept, range (a sample outside it), domain (an undefined
//!   sample inside it or a defined one outside), each claimed isolated
//!   exclusion (f defined there and around it, changing by less than 10⁻³
//!   of itself out to four resolution steps, and nothing it could be
//!   undefined through — a divisor, a function's argument, the cos/sin of a
//!   tan/sec/cot/csc argument, a log base less 1, a power's base unless the
//!   exponent is a positive whole number — at 0, undefined or changing sign
//!   there), zeros (each reported one, and sign changes or exact zeros
//!   between samples that none matches), extremum values and their
//!   locality, turns between samples that no reported extremum matches,
//!   parity (a witness against Odd or Even), the period (f(x + P) against
//!   f(x)), monotonicity (a reversal inside a claimed interval), vertical
//!   asymptotes (a pole between samples, or an undefined sample with
//!   growth beside it, that none matches);
//! * one to one: two crossings, or two turns of a kind, both matched to one
//!   reported point and told apart by something resolvably different
//!   between them (f away from 0; f below or above both);
//! * the shapes the panel writes, comparing no digits with anything: a
//!   range written as one value ({c}) needs f the same at every sample,
//!   within their rounding; two elements of a set (excluded points or
//!   families), or the two bounds of an interval (a monotone piece of
//!   each period too), that read alike are a contradiction whatever the
//!   numbers behind them; and across a row's pieces, two adjacent ones
//!   sharing a closed end that reads alike, or ends that read alike about
//!   a gap the panel's own numbers leave where f is undefined at a sample
//!   inside it (a domain's, or the monotone pieces' about it);
//! * an open range bound reached at a turn, to within f's tolerance;
//! * horizontal and oblique asymptotes: |f − line| no smaller at all three
//!   decades further out (the largest over nine points a decade: 10¹² and
//!   10¹⁵ for a horizontal line, 10⁶ and 10⁹ for an oblique one, or from
//!   100× the expression's largest number if further), beyond tolerance and
//!   moving the line as the panel shows it;
//! * inflection points: f's value there, and the second difference at
//!   x ∓ 2h keeping its sign across x at every resolvable scale (at least
//!   three; h = 10⁻¹⁵ … 10³ and 10⁻⁶ … 10⁻² of |x|, at least 1000 floats,
//!   at most a quarter of the way to another reported inflection;
//!   resolvable means beyond 64× the rounding bounds of the three values);
//! * the period's being the least: f(x + P/2) or f(x + P/3) equal to f(x)
//!   within tolerance at every one of at least 200 samples (|x| ≤ 100) over
//!   which f varies by 10⁶ times that tolerance;
//! * the evaluators: the compiled value and the reference's within 8 ulp
//!   of the reference rounded to a double (subnormals by their own
//!   spacing); 0 against a nonzero double, opposite signs, undefined
//!   against defined and a finite double against a value beyond the
//!   largest are never within it. Such a point is used for nothing else;
//! * equivalent spellings (`check_pair`): one spelling's compiled value (what
//!   the app plots and analyses) against the other's reference value,
//!   beyond the second's tolerance, 8 ulp and twice the first's rounding
//!   bound, where the panel writes them differently (skipped where the
//!   first's bound can't be computed); and, for spellings equal
//!   everywhere, every pair of definite answers (one function can't have
//!   two; values compared as the panel writes them or to 10⁻⁹, asymptotes
//!   not compared for a constant, whose own line is a convention).
//!
//! Notes, listed but not failures (sampling can neither confirm nor refute
//! them): a closed range bound no sample, turn or reported value comes to;
//! "neither" parity with no sample against even or odd (beyond the doubles
//! compared exactly; a one-sided undefined point counts); "not periodic"
//! for an f that keeps a circular period at every sample; and how many
//! samples the reference can't evaluate (Unknown: nothing there is checked,
//! and the compiled value is not taken for the truth).
//!
//! # Tolerances, all of them
//!
//! * `tol(x)`: 4 ulp of f(x), plus twice a first-order bound on the
//!   rounding of each of f's operations at x (`F::error`, written here),
//!   plus f's largest change over ±1…4, ±16…64 and ±256…1024 floats and
//!   over two of its resolution steps (`F::res`: how far x must move, up to
//!   2⁴² floats, for f to change; a shifted frame is a staircase). Values
//!   closer than that are the same.
//! * A point within 8 ulp of a reported end, excluded point or pole (plus
//!   10⁻¹² of x and four resolution steps for interval ends) is the
//!   precision of that point, not a claim about it.
//! * Values (range bounds, extremum values, the y-intercept) are only
//!   different if the panel's formatter also writes them differently.
//!   Shapes (domain, zeros, parity, period, monotonicity, asymptotes) get
//!   no such allowance.
//! * A reported zero or zero value holds if f is exactly 0 there, changes
//!   sign within eight resolution steps, or is no bigger than four times
//!   its change over those (a zero between floats), or a crossing within
//!   10⁻⁶ relative is written the same; beside a reported pole, closer
//!   than f resolves, it can't be told. Beyond the doubles this is judged
//!   on the reference's exact sign and magnitude.
//! * A found crossing, turn or pole matches a reported one within 10⁻⁶
//!   relative (the panel's 6 digits), or anywhere between the samples that
//!   bracket it; for one-to-one, the nearest reported point within 10⁻⁶ of
//!   the found position. Crossings in a reported zero's rounding noise (f
//!   within tolerance all the way to it) are not reported missing.
//! * Beyond 10⁷ from 0 and 10⁴ from the expression's numbers, sign
//!   changes and turns are not classified (floats there are too sparse
//!   against sawtooth and plateau features).
//! * Caps: 400 bisected sign changes, 500 undefined samples tested for a
//!   pole, 300 refined turns per kind (nearest the origin first), 4000
//!   samples per period comparison.
//!
//! # Known false alarms
//!
//! * `evaluator-disagrees` at `(x/0.001)^(1/3)` and `^(2/3)`, x = 5e-324:
//!   the reference's `pow_rat` on that near-subnormal quotient is about 60
//!   ulp off (the compiled value is right to the last bit, by 80-digit
//!   decimal). Reported so it is seen until the reference is fixed.
//! * In principle: a part of f below the doubles' resolution with a longer
//!   period (`period-not-fundamental`), a residual approaching a horizontal
//!   line more slowly than any power of log log x (`asymptote-not-approached`),
//!   and an inflection whose sign change only shows below 1000 floats of x.
//!   None occurs in the pool.

#[path = "oracle/checks.rs"]
mod checks;

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering as AtOrd};

use checks::{Finding, check, check_pair, check_with};
use graphing::analysis::{Family, KeyGraphFeatures, Parity, analyze};
use graphing::compile::CompileOptions;
use graphing::{Equation, TrigUnit};

// ---------------------------------------------------------------- the pool

/// Review 10's cases and more of their shapes.
fn adversarial() -> Vec<String> {
    let mut v: Vec<String> = [
        "1+sqrt(-exp(-(x+800)))",
        "sqrt(-exp(-(x+800)))^0",
        "sqrt(exp(-(x+800)))*exp((x+800)/2)",
        "exp(-1000)*(x+1)*exp(500)*exp(500)",
        "((x+2^1023)+(x+2^1023))/(x+2^1023)",
        "exp(-(x-100)^2)+exp(-x^2)",
        "exp(-(x-1000)^2)+exp(-x^2)",
        "exp(-(x-1234)^2)+exp(-x^2)",
        "(x-1000000000000)/(x-1000000000000.0625)",
        "(x-1000000000)/(x-1000000000.0625)",
        "2*exp(-(x-1522756)^2)+exp(-x^2)",
        "exp(-(x-1234*1234)^2)+exp(-x^2)",
        "sin(x)+10000000000000",
        "cos(x)+10000000000000",
        "2*sin(x)+10000000000000",
        // Separated bumps, centres as products and powers.
        "exp(-(x-50)^2)+exp(-(x+70)^2)",
        "3*exp(-(x-2^20)^2)+exp(-x^2)",
        "exp(-(x-10^7)^2)-exp(-x^2)",
        "exp(-(x-1000*1000)^2)+exp(-(x-999*1001)^2)",
        "1/(1+(x-3^13)^2)+1/(1+x^2)",
        "exp(-(x-7*11*13)^2/0.01)+x^2/10^9",
        // Close quotients.
        "(x-1000000)/(x-1000000.0625)",
        "(x-2^40)/(x-2^40-1/16)",
        "1/(x-1000000000000.0625)+1/(x-1000000000000)",
        "(x-1000*1000)/(x-1000*1000-1/8)",
        // Huge offsets of bounded functions.
        "sin(x)+1000000000000",
        "atan(x)+100000000000000",
        "tanh(x)+10^13",
        "cos(2*x)*3+10^12",
        "floor(sin(x))+10^12",
        // Hidden underflow inside sqrt, ln, pow.
        "ln(exp(-(x+800)))",
        "sqrt(exp(-(x+800)))",
        "(exp(-(x+800)))^0.5*exp(400)",
        "ln(1+exp(-(x+800)))",
        "exp(-(x+800))^0",
        "1/sqrt(exp(-(x+800)))",
        "sqrt(-exp(-x^2-800))+1",
        "ln(-exp(-(x+800)))+1",
        "(-exp(-(x+800)))^(1/2)+2",
        "x^2*exp(-1000)*exp(1000)",
        // Review 13: a gap between pieces 10⁻⁷ wide (R13-M-06); an
        // exponent and a degree nowhere defined, enclosed by whole numbers
        // (R13-M-02); whole counts past 60, r > n (R13-M-03).
        "sqrt((x-sin(1))*(x-sin(1)-0.0000001))",
        "ln((x-sin(1))*(x-sin(1)-0.0000001))",
        "x^floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-30)))",
        "root(x,1+floor(sqrt(sin(4)^2+cos(4)^2-1-10^(-30))))",
        "nCr(61,62)",
        "nPr(61,62)",
        "abs(nCr(61,62))+1",
        // A root degree typed as an odd integer past 2⁵³, whose doubles are
        // even (R13-M-04), and ones even or no integer as typed.
        "root(-8,9007199254740993)",
        "root(x,9007199254740993)",
        "root(x,-9007199254740993)",
        "root(x,18014398509481986)",
        "root(x,3.0000000000000001)",
        // Decimals that share a double, each its own number (R13-M-01).
        "10^16*(1.0000000000000001-1)+x",
        "10^16*(1.0000000000000001-0.99999999999999999)+x",
        "1.0000000000000001*x-1*x",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for c in [100.0, 1000.0, 1234.0, 123456.0] {
        v.push(format!("exp(-(x-{c})^2)+exp(-x^2)"));
        v.push(format!("exp(-(x-{c})^2)+exp(-(x+{c})^2)"));
    }
    // Review 11: narrow peaks far out (widths √w), Gaussian and rational,
    // beside an ordinary one at 0; a rational peak's centre is defined.
    for c in ["1234", "1522756", "1234*1234"] {
        for w in ["0.001", "0.000001", "0.000000001", "0.000000000001"] {
            v.push(format!("2*exp(-(x-{c})^2/{w})+exp(-x^2)"));
            v.push(format!("1/(1+(x-{c})^2/{w})+exp(-x^2)"));
            v.push(format!("2/(1+(x-{c})^2/{w})+1/(1+x^2)"));
        }
    }
    // Review 11's arithmetic cases (and see `spellings`).
    for e in [
        "acot(1000000000000000)*1000000000000000",
        "acot(10^17)*10^17",
        "acot(x)*1000000000000000",
        "ln(exp(1000000000000000)^4)-4000000000000000",
        "ln(exp(1000000000000000)*exp(1000000000000000)*exp(1000000000000000)*exp(1000000000000000))-4000000000000000",
    ] {
        v.push(e.to_string());
    }
    v
}

/// Planted false answers: each must be caught by the named check (the
/// checks' own discrimination test). Returns whether all were.
fn selftest() -> bool {
    use graphing::analysis::{AsymptoteSide, Bound};
    type Plant = Box<dyn Fn(&mut KeyGraphFeatures)>;
    let one = |x: f64| Family::single(x);
    let cases: Vec<(&str, &str, Plant)> = vec![
        (
            "x^3",
            "inflection-not-inflection",
            Box::new(move |k| k.data.inflection_points.push((one(1.0), 1.0))),
        ),
        (
            "x^3",
            "inflection-value-wrong",
            Box::new(|k| {
                for p in &mut k.data.inflection_points {
                    p.1 += 0.5;
                }
            }),
        ),
        (
            "atan(x)",
            "asymptote-not-approached",
            Box::new(|k| {
                for a in &mut k.data.horizontal_asymptotes {
                    a.0 *= 0.99;
                }
            }),
        ),
        (
            "x+1/x",
            "asymptote-not-approached",
            Box::new(|k| {
                for a in &mut k.data.oblique_asymptotes {
                    a.1 += 1.0;
                }
            }),
        ),
        (
            "sin(x)",
            "period-not-fundamental",
            Box::new(|k| {
                k.data.period = k.data.period.map(|p| 2.0 * p);
            }),
        ),
        (
            "sin(x)",
            "range-open-bound-reached",
            Box::new(|k| {
                for i in &mut k.data.range {
                    i.hi = Bound {
                        value: i.hi.value,
                        closed: false,
                    };
                }
            }),
        ),
        (
            "(x-1.1)*(x-1.1000003)",
            "zeros-merged",
            Box::new(move |k| k.data.zeros = vec![one(1.1)]),
        ),
        (
            "(x-1)^2*(x-1.000001)^2",
            "extrema-merged",
            Box::new(move |k| k.data.minima = vec![(one(1.0), 0.0)]),
        ),
        (
            "x^2+1",
            "excluded-point-defined",
            Box::new(move |k| k.data.excluded.push(one(2.0))),
        ),
        (
            "x^2",
            "zero-missing",
            Box::new(|k| {
                k.data.zeros.clear();
                k.x_intercept.clear();
            }),
        ),
        (
            "x^2",
            "neither-unwitnessed",
            Box::new(|k| k.parity = Parity::Neither),
        ),
        (
            "1/x",
            "vertical-asymptote-missing",
            Box::new(|k| {
                k.data.vertical_asymptotes.clear();
                k.data.excluded.clear();
            }),
        ),
        (
            "exp(-x^2)",
            "asymptote-not-approached",
            Box::new(|k| {
                k.data.horizontal_asymptotes = vec![(0.5, AsymptoteSide::AnyInfinity)];
            }),
        ),
        // Review 13, R13-M-01: what the panel said before, when two
        // decimals sharing the double 1 were taken for one number:
        // 10^16·(1.0000000000000001 − 1) + x is x + 1, not x (its
        // y-intercept 1, its zero −1), and 10^16·(1.0000000000000001 − 1)·x
        // is x, not 0. The reference reads each literal as typed.
        (
            "10^16*(1.0000000000000001-1)+x",
            "y-intercept-wrong",
            Box::new(|k| {
                k.data.y_intercept = Some(0.0);
                k.y_intercept = "0".into();
            }),
        ),
        (
            "10^16*(1.0000000000000001-1)+x",
            "zero-not-zero",
            Box::new(move |k| {
                k.data.zeros = vec![one(0.0)];
                k.x_intercept = "0".into();
            }),
        ),
        (
            "10^16*(1.0000000000000001-1)*x",
            "range-singleton-varies",
            Box::new(|k| k.range = "y ∈ {0}".into()),
        ),
        // Review 12, R12-M-04: a range written as one value it isn't
        // (the numbers behind it left as they were: only the text lies).
        (
            "sin(1)+(sin(x)+2)/10000000",
            "range-singleton-varies",
            Box::new(|k| k.range = "y ∈ {≈0.841471}".into()),
        ),
        // Two excluded points, and an interval's two bounds, that read
        // alike (the review's were 10⁻⁷ apart; here only the text lies).
        (
            "1/((x-1)*(x-2))",
            "set-elements-alike",
            Box::new(|k| k.domain = "x ∈ ℝ \\ {≈1.5, ≈1.5}".into()),
        ),
        (
            "1/((x-1)*(x-2))",
            "interval-bounds-alike",
            Box::new(|k| {
                if let Some(m) = k.monotonicity.get_mut(1) {
                    m.0 = "(≈1.5, ≈1.5)".into();
                }
            }),
        ),
        // Review 13, R13-M-06: the two ends of a gap between pieces
        // written alike, as they were (only the text lies): closed, they
        // say two pieces share a point; open, f is undefined between the
        // numbers behind them.
        (
            "sqrt((x-sin(1))*(x-sin(1)-0.0000001))",
            "row-ends-alike",
            Box::new(|k| k.domain = "x ∈ (−∞, ≈0.841471] ∪ [≈0.841471, ∞)".into()),
        ),
        (
            "ln((x-sin(1))*(x-sin(1)-0.0000001))",
            "row-ends-alike",
            Box::new(|k| k.domain = "x ∈ (−∞, ≈0.841471) ∪ (≈0.841471, ∞)".into()),
        ),
        (
            "sqrt((x-sin(1))*(x-sin(1)-0.0000001))",
            "row-ends-alike",
            Box::new(|k| {
                if let [a, b] = k.monotonicity.as_mut_slice() {
                    a.0 = "(−∞, ≈0.841471)".into();
                    b.0 = "(≈0.841471, ∞)".into();
                }
            }),
        ),
    ];
    let mut ok = true;
    for (expr, want, plant) in &cases {
        let found = check_with(expr, TrigUnit::Radians, &[], false, plant.as_ref());
        let hit = found.iter().any(|f| f.class == *want);
        ok &= hit;
        println!(
            "{} y={expr}: planted, expecting {want}: {}",
            if hit { "ok  " } else { "MISS" },
            found.iter().map(|f| f.class).collect::<Vec<_>>().join(", ")
        );
    }
    ok
}

/// Review 11's case in degrees.
const DEGREES: &[&str] = &["sin(x+2^-1074)/(x+2^-1074)", "sin(x)/x", "tan(x)", "cot(x)"];

/// Expressions equal to each other where `x > 0` (or everywhere, `false`):
/// the first's compiled value, which the app plots and analyses, against
/// the second's reference value (the truth by another route, where the
/// first's own reference shares the same arithmetic).
fn spellings() -> Vec<(&'static str, &'static str, bool)> {
    vec![
        ("acot(x)", "atan(1/x)", true),
        ("acot(x)*x", "atan(1/x)*x", true),
        (
            "acot(1000000000000000)*1000000000000000",
            "atan(1/1000000000000000)*1000000000000000",
            false,
        ),
        ("acot(10^17)*10^17", "atan(1/10^17)*10^17", false),
        ("ln(exp(1000000000000000)^4)-4000000000000000", "0", false),
        ("ln(exp(x)^4)", "4*x", false),
        ("ln(exp(x)^3)-3*x", "0", false),
        ("ln(exp(x)^2)", "2*x", false),
        ("sqrt(exp(x)^2)", "exp(x)", false),
        ("exp(x)^2/exp(x)", "exp(x)", false),
        ("1/(1+exp(-x))", "exp(x)/(exp(x)+1)", false),
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--selftest") {
        std::process::exit(if selftest() { 0 } else { 1 });
    }
    if args.first().is_some_and(|a| a == "--dump") && args.len() >= 2 {
        // One function: its analysis and what the checks find; the exit
        // status is 1 if they find anything, as for a whole run.
        let opts = CompileOptions::default();
        let eq = Equation::parse(&format!("y={}", args[1])).expect("parses");
        let k = analyze(&eq, &opts);
        println!("{k:#?}");
        let found = check(&args[1], TrigUnit::Radians, &[], false);
        for f in &found {
            println!(
                "{}{}: {}",
                if f.note { "note: " } else { "" },
                f.class,
                f.detail
            );
        }
        if found.iter().any(|f| !f.note) {
            std::process::exit(1);
        }
        return;
    }
    let filter = args.iter().find(|a| !a.starts_with("--")).cloned();
    // (expression, unit, centres, wide grid, the other spelling and whether
    // only x > 0 is compared).
    type Job = (String, TrigUnit, Vec<f64>, bool, Option<(String, bool)>);
    let mut pool: Vec<Job> = Vec::new();
    let mut push = |e: String, u: TrigUnit, c: Vec<f64>, w: bool| {
        if filter.as_ref().is_none_or(|f| e.contains(f.as_str())) {
            pool.push((e, u, c, w, None));
        }
    };
    let transforms = transforms();
    for base in BASES {
        push(base.to_string(), TrigUnit::Radians, vec![], false);
        for t in &transforms {
            push(t.apply(base), TrigUnit::Radians, vec![t.a], false);
        }
    }
    for (e, u, _, _) in UNDEFINED {
        push(e.to_string(), *u, vec![], false);
    }
    for s in specials() {
        push(s.expr, TrigUnit::Radians, s.centres, s.wide);
    }
    for (p, q) in equivalent() {
        push(p, TrigUnit::Radians, vec![], false);
        push(q, TrigUnit::Radians, vec![], false);
    }
    for e in adversarial() {
        push(e, TrigUnit::Radians, vec![], false);
    }
    for e in DEGREES {
        push(e.to_string(), TrigUnit::Degrees, vec![], false);
    }
    pool.sort_by(|a, b| a.0.cmp(&b.0).then((a.1 as u8).cmp(&(b.1 as u8))));
    pool.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    let mut pairs: Vec<(String, String, bool)> = equivalent()
        .into_iter()
        .map(|(p, q)| (p, q, false))
        .collect();
    pairs.extend(
        spellings()
            .into_iter()
            .map(|(p, q, pos)| (p.to_string(), q.to_string(), pos)),
    );
    for (p, q, pos) in pairs {
        if filter
            .as_ref()
            .is_none_or(|f| p.contains(f.as_str()) || q.contains(f.as_str()))
        {
            pool.push((p, TrigUnit::Radians, vec![], false, Some((q, pos))));
        }
    }

    let started = std::time::Instant::now();
    let next = AtomicUsize::new(0);
    let found: Mutex<Vec<Finding>> = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, AtOrd::Relaxed);
                    let Some((e, u, c, w, other)) = pool.get(i) else {
                        break;
                    };
                    let mut r = match other {
                        Some((q, pos)) => check_pair(e, q, *pos),
                        None => check(e, *u, c, *w),
                    };
                    if *u != TrigUnit::Radians {
                        for f in &mut r {
                            f.expr = format!("{} [{u:?}]", f.expr);
                        }
                    }
                    found.lock().unwrap().extend(r);
                }
            });
        }
    });
    let found = found.into_inner().unwrap();
    let (notes, fails): (Vec<&Finding>, Vec<&Finding>) = found.iter().partition(|f| f.note);
    let functions = |fs: &[&Finding]| {
        let mut e: Vec<&str> = fs.iter().map(|f| f.expr.as_str()).collect();
        e.sort();
        e.dedup();
        e.len()
    };
    println!(
        "{} functions in {:.1} s; {} contradictions in {} functions ({} notes in {} functions: not contradictions)",
        pool.len(),
        started.elapsed().as_secs_f64(),
        fails.len(),
        functions(&fails),
        notes.len(),
        functions(&notes),
    );
    let all = args.iter().any(|a| a == "--all");
    for (title, list) in [("", &fails), ("note: ", &notes)] {
        let mut by: std::collections::BTreeMap<&str, Vec<&Finding>> = Default::default();
        for f in list.iter() {
            by.entry(f.class).or_default().push(f);
        }
        for (class, fs) in &by {
            println!(
                "\n## {title}{class}: {} in {} functions",
                fs.len(),
                functions(fs)
            );
            let mut shown = 0;
            let mut last = "";
            for f in fs.iter() {
                if f.expr == last {
                    continue;
                }
                last = &f.expr;
                println!("  y={}: {}", f.expr, f.detail);
                shown += 1;
                if shown >= 60 && !all {
                    println!("  … (--all for every one)");
                    break;
                }
            }
        }
    }
    if !fails.is_empty() {
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------- copied from sweep.rs

fn transforms() -> Vec<Aff> {
    let mut t: Vec<Aff> = Vec::new();
    for a in [1.0, 1e3, 1e6, 1e9, 1e12] {
        t.push(Aff { a, ..ID });
    }
    for c in [
        1e-12, -1e-12, 1e-9, -1e-9, 1e-7, -1e-7, 1e-3, -1e-3, 1.0, -1.0, 1e6, -1e6,
    ] {
        t.push(Aff { c, ..ID });
    }
    for k in [1e-12, -1e-12, 1e-6, -1e-6, -1.0, 1e6, 1e12] {
        t.push(Aff { k, ..ID });
    }
    for s in [1e-3, 1e3, 1e6] {
        t.push(Aff { s, ..ID });
    }
    t.push(Aff {
        a: 1000.0,
        s: 1.0,
        k: -2.0,
        c: 1.0,
    });
    t.push(Aff {
        a: 1e9,
        s: 1000.0,
        k: 1.0,
        c: -1.0,
    });
    t.push(Aff {
        a: -1e6,
        s: 1.0,
        k: 1e-6,
        c: 1e6,
    });
    t
}

fn num(v: f64) -> String {
    let s = format!("{v}");
    if v < 0.0 { format!("({s})") } else { s }
}

/// y = k·f((x − a)/s) + c.
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
}

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
