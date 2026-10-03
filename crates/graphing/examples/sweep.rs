//! Adversarial invariant sweep over function analysis (not run in CI).
//!
//! `cargo run --release -p graphing --example sweep [-- FILTER] [--all]`
//!
//! For a set of base functions it analyses shifted, offset and scaled
//! variants and checks invariants that must hold whatever the function:
//! internal consistency (a reported minimum lies in the range, 0 is in the
//! range exactly when there are x-intercepts, ...), agreement with dense
//! sampling (domain, range, monotonicity), and that transformed functions get
//! correspondingly transformed results. A feature the analysis marks as
//! unknown is never counted as a failure; a definite claim that contradicts
//! the function is. Prints failures grouped by check, one line each.

use std::collections::BTreeMap;

use graphing::analysis::{
    AnalysisData, AnalysisError, Family, Interval, KeyGraphFeatures, Monotonicity, Parity, analyze,
    flags,
};
use graphing::compile::CompileOptions;
use graphing::equation::CompiledEquation;
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
/// (which must be outside the claimed domain and never be intercepts).
const UNDEFINED: &[(&str, TrigUnit, &[f64])] = &[
    ("x^-1", TrigUnit::Radians, &[0.0]),
    ("1/(x^-1)", TrigUnit::Radians, &[0.0]),
    ("1/(1/x)", TrigUnit::Radians, &[0.0]),
    ("x*(1/x)", TrigUnit::Radians, &[0.0]),
    ("1^(1/x)", TrigUnit::Radians, &[0.0]),
    ("(1/x)^0", TrigUnit::Radians, &[0.0]),
    ("1^(ln(x))", TrigUnit::Radians, &[0.0, -1.0, -5.0]),
    ("exp(-x^-2)", TrigUnit::Radians, &[0.0]),
    ("1/asech(x)", TrigUnit::Radians, &[0.0]),
    ("1/acoth(x)", TrigUnit::Radians, &[1.0, -1.0]),
    ("exp(-atanh(x))", TrigUnit::Radians, &[1.0, -1.0]),
    ("atan(tan(x))", TrigUnit::Degrees, &[90.0, -90.0, 270.0]),
    ("atan(tan(x))", TrigUnit::Grads, &[100.0, -100.0, 300.0]),
    ("atan(sec(x))", TrigUnit::Degrees, &[90.0, 270.0]),
    ("atan(csc(x))", TrigUnit::Degrees, &[0.0, 180.0]),
    ("atan(cot(x))", TrigUnit::Degrees, &[0.0, 180.0]),
    ("atan(1/(x-x))", TrigUnit::Radians, &[0.0, 1.0, -3.0]),
    ("ln(x-x)", TrigUnit::Radians, &[0.0, 1.0]),
    ("e^(-1/abs(x))", TrigUnit::Radians, &[0.0]),
    ("atan(1/sin(x))", TrigUnit::Radians, &[0.0]),
    ("1/e^(1/x)", TrigUnit::Radians, &[0.0]),
    ("(x-x)^0", TrigUnit::Radians, &[0.0, 1.0]),
    ("atan(x^-2)", TrigUnit::Radians, &[0.0]),
    ("atan(log(x))", TrigUnit::Radians, &[0.0]),
    ("atan(ln(abs(x)))", TrigUnit::Radians, &[0.0]),
    ("0^(-x)", TrigUnit::Radians, &[0.0, 1.0, 2.5]),
    ("root(x,-2)", TrigUnit::Radians, &[0.0]),
    ("1/root(x,-2)", TrigUnit::Radians, &[0.0]),
    ("x^(-1/3)", TrigUnit::Radians, &[0.0]),
    ("1/x^(-1/3)", TrigUnit::Radians, &[0.0]),
    ("e^(1/(x-1))", TrigUnit::Radians, &[1.0]),
    ("atan(1/(x^2-1))", TrigUnit::Radians, &[1.0, -1.0]),
];

struct Analysed {
    expr: String,
    k: KeyGraphFeatures,
    f: CompiledEquation,
}

impl Analysed {
    fn new(expr: &str, unit: TrigUnit) -> Option<Analysed> {
        let opts = CompileOptions {
            trig_unit: unit,
            ..CompileOptions::default()
        };
        let eq = Equation::parse(&format!("y={expr}")).ok()?;
        let f = eq.compile(&opts).ok()?;
        let k = analyze(&eq, &opts);
        Some(Analysed {
            expr: expr.to_string(),
            k,
            f,
        })
    }
    fn eval(&self, x: f64) -> f64 {
        self.f.eval_explicit(x).unwrap_or(f64::NAN)
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
    fn periodic(&self) -> bool {
        self.d().period.is_some()
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

fn num(v: f64) -> String {
    let s = format!("{v}");
    if v < 0.0 { format!("({s})") } else { s }
}

fn close(a: f64, b: f64, rel: f64, abs: f64) -> bool {
    if a.is_infinite() || b.is_infinite() {
        return a == b;
    }
    (a - b).abs() <= rel * a.abs().max(b.abs()) + abs
}

fn interval_contains(iv: &Interval, v: f64, tol: f64) -> bool {
    let lo = iv.lo.value;
    let hi = iv.hi.value;
    (lo == f64::NEG_INFINITY || v >= lo - tol) && (hi == f64::INFINITY || v <= hi + tol)
}

/// Strictly inside (by more than tol), so closedness doesn't matter.
fn interval_interior(iv: &Interval, v: f64, tol: f64) -> bool {
    let lo = iv.lo.value;
    let hi = iv.hi.value;
    (lo == f64::NEG_INFINITY || v > lo + tol) && (hi == f64::INFINITY || v < hi - tol)
}

fn set_contains(set: &[Interval], v: f64, tol: f64) -> bool {
    set.iter().any(|iv| interval_contains(iv, v, tol))
}

/// Is v in the set, as an attained value: interior, or at a closed end?
fn set_attains(set: &[Interval], v: f64, tol: f64) -> bool {
    set.iter().any(|iv| {
        interval_interior(iv, v, tol)
            || (iv.lo.closed && close(iv.lo.value, v, 0.0, tol))
            || (iv.hi.closed && close(iv.hi.value, v, 0.0, tol))
    })
}

fn fmt_set(set: &[Interval]) -> String {
    set.iter()
        .map(|iv| {
            format!(
                "{}{},{}{}",
                if iv.lo.closed { "[" } else { "(" },
                iv.lo.value,
                iv.hi.value,
                if iv.hi.closed { "]" } else { ")" }
            )
        })
        .collect::<Vec<_>>()
        .join("∪")
}

/// Sample points: dense near the origin and near `centre`, log-spaced far
/// out, plus exact small integers and the points the analysis reports.
fn samples(a: &Analysed, centre: f64) -> Vec<f64> {
    let mut xs = Vec::new();
    for i in -2000..=2000 {
        xs.push(i as f64 * 0.005);
        xs.push(centre + i as f64 * 0.005);
    }
    for i in -400..=400 {
        xs.push(i as f64);
        xs.push(centre + i as f64);
    }
    for e in -60..=150 {
        let m = 10f64.powf(e as f64 / 10.0);
        for s in [1.0, -1.0] {
            xs.push(s * m);
            xs.push(centre + s * m);
        }
    }
    let d = a.d();
    let mut add = |x: f64| {
        if x.is_finite() {
            for h in [0.0, 1e-6, -1e-6, 1e-3, -1e-3, 0.1, -0.1] {
                xs.push(x + h * x.abs().max(1.0));
            }
        }
    };
    for z in &d.zeros {
        add(z.x);
    }
    for (f, _) in d.minima.iter().chain(&d.maxima).chain(&d.inflection_points) {
        add(f.x);
    }
    for v in &d.vertical_asymptotes {
        add(v.x);
    }
    xs.retain(|x| x.is_finite());
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.dedup();
    xs
}

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

/// Checks on one analysed function on its own.
fn check_one(a: &Analysed, centre: f64, r: &mut Report) {
    if !a.ok() {
        return;
    }
    let d = a.d();
    let e = &a.expr;
    // Rows flagged unknown keep no values.
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
                e,
                format!("{name}: {n} values kept"),
            );
        }
    }
    let range_known = !a.unknown(flags::RANGE) && !d.range.is_empty();
    let domain_known = !a.unknown(flags::DOMAIN) && !d.domain.is_empty() && !a.periodic();
    let xs = samples(a, centre);
    let ys: Vec<f64> = xs.iter().map(|&x| a.eval(x)).collect();
    let fmax = ys
        .iter()
        .filter(|y| y.is_finite())
        .fold(0f64, |m, y| m.max(y.abs()));
    // Resolution of x near a point: the shift's rounding dominates far out.
    let quantum = |x: f64| 1e-14 * (x.abs() + centre.abs()).max(1.0);
    // How far x is from the base function's origin (or from 0, for a
    // periodic family's representative): a shift by 10⁹ moves the
    // features, not their size, so probes scale with x − centre (a step of
    // 10⁻⁷·10⁹ = 100 jumps over every turn of a cubic).
    let size = |x: f64| (x - centre).abs().min(x.abs());
    // f's rounding spread at x: what neighbouring floats give. A value
    // below a minimum by less than this is noise of the expanded form
    // (x⁴ − 4x³ + 6x² − 4x + 1 is ±4·10⁻¹⁶ beside its exact minimum at 1).
    let spread = |x: f64| {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for k in -8..=8 {
            let v = a.eval(x + k as f64 * f64::EPSILON * x.abs().max(f64::MIN_POSITIVE));
            if v.is_finite() {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        if hi >= lo { hi - lo } else { 0.0 }
    };
    // Rounding of f's terms, which can be far larger than f: 10⁻¹⁴ of f a
    // unit away. Neighbouring floats can all give the same rounded value
    // (−4·10⁻¹⁶ beside the minimum of the expanded (x − 2)⁴), so the spread
    // alone misses it.
    let terms = |x: f64| {
        let u = size(x).max(1.0);
        1e-14
            * [a.eval(x - u), a.eval(x + u)]
                .iter()
                .filter(|v| v.is_finite())
                .fold(0.0f64, |m, v| m.max(v.abs()))
    };
    // Range vs samples.
    if range_known {
        let mut worst: Option<(f64, f64)> = None;
        for (&x, &y) in xs.iter().zip(&ys) {
            if y.is_finite() && !set_contains(&d.range, y, 1e-7 * y.abs() + 1e-12 * fmax.min(1.0)) {
                worst.get_or_insert((x, y));
            }
        }
        if let Some((x, y)) = worst {
            r.fail(
                "sample-outside-range",
                e,
                format!("f({x})={y} not in claimed range {}", fmt_set(&d.range)),
            );
        }
        // A closed finite bound must be attained. Sparse samples of a
        // stretched or fast function (sin((x/1000)²), sinc(x/1000)) can
        // miss the turn: refine the most extreme sample by golden-section
        // search between its neighbours, and accept the bound to within
        // f's change across the floats there (cos²(x − 10¹²) is 0 only
        // between floats 1.2·10⁻⁴ apart).
        let attained_near_extreme = |v: f64, is_lo: bool| {
            let sign = if is_lo { 1.0 } else { -1.0 };
            let Some(i) = (0..ys.len())
                .filter(|&i| ys[i].is_finite())
                .min_by(|&i, &j| (sign * ys[i]).total_cmp(&(sign * ys[j])))
            else {
                return false;
            };
            let (mut lo, mut hi) = (xs[i.saturating_sub(1)], xs[(i + 1).min(xs.len() - 1)]);
            let g = |x: f64| {
                let y = a.eval(x);
                if y.is_finite() {
                    sign * y
                } else {
                    f64::INFINITY
                }
            };
            let phi = 0.5 * (5f64.sqrt() - 1.0);
            for _ in 0..100 {
                let (m1, m2) = (hi - phi * (hi - lo), lo + phi * (hi - lo));
                if g(m1) <= g(m2) {
                    hi = m2;
                } else {
                    lo = m1;
                }
            }
            let x = 0.5 * (lo + hi);
            let y = a.eval(x);
            let q = quantum(x);
            let step = (a.eval(x - q) - y).abs().max((a.eval(x + q) - y).abs());
            y.is_finite()
                && (close(y, v, 1e-6, 1e-6 * v.abs().max(1e-300)) || (y - v).abs() <= step)
        };
        let smin = ys
            .iter()
            .filter(|y| y.is_finite())
            .fold(f64::INFINITY, |m, &y| m.min(y));
        let smax = ys
            .iter()
            .filter(|y| y.is_finite())
            .fold(f64::NEG_INFINITY, |m, &y| m.max(y));
        for iv in &d.range {
            for (b, is_lo) in [(iv.lo, true), (iv.hi, false)] {
                if !b.closed || !b.value.is_finite() {
                    continue;
                }
                let v = b.value;
                let tol = 1e-6 * v.abs().max(1e-300);
                let hit = d
                    .minima
                    .iter()
                    .chain(&d.maxima)
                    .chain(&d.inflection_points)
                    .any(|(_, y)| close(*y, v, 1e-6, 0.0))
                    || d.y_intercept.is_some_and(|y| close(y, v, 1e-6, 0.0))
                    || (v == 0.0 && !d.zeros.is_empty())
                    || ys.iter().any(|y| close(*y, v, 1e-6, tol))
                    || (is_lo && close(smin, v, 1e-6, tol))
                    || (!is_lo && close(smax, v, 1e-6, tol))
                    || attained_near_extreme(v, is_lo);
                if !hit {
                    r.fail(
                        "closed-range-bound-not-attained",
                        e,
                        format!(
                            "range {} has closed bound {v}; samples span [{smin}, {smax}], extrema {:?}",
                            fmt_set(&d.range),
                            d.minima.iter().chain(&d.maxima).map(|m| m.1).collect::<Vec<_>>()
                        ),
                    );
                }
            }
        }
        // 0 attained ⟺ x-intercepts.
        if !a.unknown(flags::ZEROS) {
            let attains0 = set_attains(&d.range, 0.0, 0.0);
            if attains0 && d.zeros.is_empty() {
                r.fail(
                    "range-has-0-but-no-zeros",
                    e,
                    format!("range {} attains 0, no x-intercepts", fmt_set(&d.range)),
                );
            }
            if !d.zeros.is_empty() && !set_contains(&d.range, 0.0, 0.0) {
                r.fail(
                    "zeros-but-range-lacks-0",
                    e,
                    format!(
                        "x-intercepts {:?}, range {}",
                        d.zeros.iter().map(|z| z.x).collect::<Vec<_>>(),
                        fmt_set(&d.range)
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
                if !set_contains(&d.range, *y, 1e-7 * y.abs() + 1e-300) {
                    r.fail(
                        "extremum-y-outside-range",
                        e,
                        format!("{what} ({}, {y}) but range {}", fx.x, fmt_set(&d.range)),
                    );
                }
            }
        }
        if let Some(y) = d.y_intercept
            && !set_contains(&d.range, y, 1e-7 * y.abs() + 1e-300)
        {
            r.fail(
                "y-intercept-outside-range",
                e,
                format!("y-intercept {y}, range {}", fmt_set(&d.range)),
            );
        }
    }
    // Reported points are where they say.
    for z in &d.zeros {
        let fz = a.eval(z.x);
        let h = 1e-3 * z.x.abs().max(1.0);
        let around = a.eval(z.x - h).abs().max(a.eval(z.x + h).abs());
        let q = quantum(z.x);
        let (l, rr) = (a.eval(z.x - q), a.eval(z.x + q));
        let nearby = l.signum() != rr.signum() || l.abs().min(rr.abs()) <= 1e-6 * around;
        // Within the rounding of f's terms (the expanded (x − 1)⁴ − 10⁻¹²).
        if !fz.is_finite()
            || (fz.abs() > 1e-6 * around.max(1e-300) && !nearby && fz.abs() > terms(z.x))
        {
            r.fail(
                "zero-not-a-zero",
                e,
                format!("x-intercept {} but f={fz} (around {around})", z.x),
            );
        }
    }
    for (what, list, sign) in [("minimum", &d.minima, 1.0), ("maximum", &d.maxima, -1.0)] {
        for (fx, y) in list {
            let fv = a.eval(fx.x);
            if !fv.is_finite() || !close(fv, *y, 1e-6, 1e-12 * fmax.min(1.0)) {
                r.fail(
                    "extremum-value-wrong",
                    e,
                    format!("{what} at {} reported {y}, f={fv}", fx.x),
                );
                continue;
            }
            {
                let step = (1e-7 * size(fx.x).max(1.0)).max(4.0 * quantum(fx.x));
                for s in [step, -step] {
                    let g = a.eval(fx.x + s);
                    let noise = spread(fx.x) + spread(fx.x + s) + terms(fx.x);
                    if g.is_finite()
                        && sign * (g - fv) < -1e-7 * fv.abs().max(g.abs()).max(1e-300) - noise
                    {
                        r.fail(
                            "extremum-not-local",
                            e,
                            format!("{what} at ({}, {fv}) but f({})={g}", fx.x, fx.x + s),
                        );
                    }
                }
            }
        }
    }
    // Positions are accurate to the panel's precision (6 significant
    // digits of x − centre, and no finer than the floats there allow): the
    // vertex from f′ ≈ (f(x−2h) − 8f(x−h) + 8f(x+h) − f(x+2h))/12h and
    // f″ ≈ (f(x−h) − 2f(x) + f(x+h))/h² is at x. Five points, as three
    // put a cubic's vertex h²·f‴/(6f″) off: more than 6 digits at the h
    // that rounding at 10⁹ needs.
    for (what, list) in [("minimum", &d.minima), ("maximum", &d.maxima)] {
        for (fx, _) in list {
            let x = fx.x;
            let h = (1e-5 * size(x).max(1e-3)).max(4.0 * quantum(x));
            let w = [-2.0, -1.0, 0.0, 1.0, 2.0].map(|k| a.eval(x + k * h));
            let curv = w[1] + w[3] - 2.0 * w[2];
            // A bend within rounding (a flat quartic minimum) places no vertex.
            let noise = 4.0 * (spread(x - h) + spread(x) + spread(x + h)) + terms(x);
            if w.iter().any(|v| !v.is_finite())
                || curv.abs() <= (1e-12 * w[2].abs()).max(noise).max(1e-300)
            {
                continue;
            }
            let slope_h = (w[0] - 8.0 * w[1] + 8.0 * w[3] - w[4]) / 12.0;
            let delta = -h * slope_h / curv;
            if delta.abs() > (5e-6 * size(x).max(1e-3)).max(quantum(x)) && delta.abs() < 1e3 * h {
                r.fail(
                    "extremum-imprecise",
                    e,
                    format!("{what} reported at x={x}, vertex at about {}", x + delta),
                );
            }
        }
    }
    for z in &d.zeros {
        let x = z.x;
        let h = 1e-6 * x.abs().max(1e-3).max(1e3 * quantum(x));
        let (l, m, rr) = (a.eval(x - h), a.eval(x), a.eval(x + h));
        let slope = (rr - l) / (2.0 * h);
        if l.is_finite()
            && m.is_finite()
            && rr.is_finite()
            && slope != 0.0
            && l.signum() != rr.signum()
        {
            let delta = m / slope;
            if delta.abs() > 5e-7 * x.abs().max(1e-3) {
                r.fail(
                    "zero-imprecise",
                    e,
                    format!("x-intercept {x}, root at about {}", x - delta),
                );
            }
        }
    }
    // y-intercept.
    if !a.unknown(flags::Y_INTERCEPT) {
        let f0 = a.eval(0.0);
        match d.y_intercept {
            Some(y) if !f0.is_finite() => r.fail(
                "y-intercept-where-undefined",
                e,
                format!("y-intercept {y} but f(0)={f0}"),
            ),
            Some(y) if !close(y, f0, 1e-6, 1e-12) => r.fail(
                "y-intercept-wrong",
                e,
                format!("y-intercept {y} but f(0)={f0}"),
            ),
            None if f0.is_finite() => r.fail(
                "missing-y-intercept",
                e,
                format!("no y-intercept but f(0)={f0}"),
            ),
            _ => {}
        }
    }
    // Domain vs samples.
    if domain_known {
        // An excluded point is that float (999999999.999999 is not 10⁹, nor
        // 3·10⁻¹⁹ the 0 of coth); a family's copies, a few ulps.
        let in_dom = |x: f64| {
            d.domain.iter().any(|iv| interval_contains(iv, x, 0.0))
                && !d.excluded.iter().any(|p| match p.period {
                    None => p.x == x,
                    Some(_) => p.contains(x, 4.0 * f64::EPSILON),
                })
        };
        let interior = |x: f64| {
            let tol = 1e-9 * x.abs().max(1.0);
            d.domain.iter().any(|iv| interval_interior(iv, x, tol))
                && !d.excluded.iter().any(|p| p.contains(x, 1e-6))
        };
        let mut undefined_in = None;
        let mut defined_out = None;
        for (&x, &y) in xs.iter().zip(&ys) {
            if y.is_nan() && interior(x) && x.abs() < 1e300 {
                undefined_in.get_or_insert((x, y));
            }
            if y.is_finite() && !in_dom(x) {
                defined_out.get_or_insert((x, y));
            }
        }
        if let Some((x, y)) = undefined_in {
            r.fail(
                "domain-includes-undefined",
                e,
                format!(
                    "f({x})={y} but domain {} excl {:?}",
                    fmt_set(&d.domain),
                    d.excluded.iter().map(|p| p.x).collect::<Vec<_>>()
                ),
            );
        }
        if let Some((x, y)) = defined_out {
            r.fail(
                "domain-excludes-defined",
                e,
                format!(
                    "f({x})={y} but domain {} excl {:?}",
                    fmt_set(&d.domain),
                    d.excluded.iter().map(|p| p.x).collect::<Vec<_>>()
                ),
            );
        }
        // Monotonicity covers the domain and agrees with samples.
        if !a.unknown(flags::MONOTONE_INTERVALS) && !d.monotonicity.is_empty() {
            let mut gap = None;
            for (&x, &y) in xs.iter().zip(&ys) {
                if y.is_finite()
                    && in_dom(x)
                    && !d
                        .monotonicity
                        .iter()
                        .any(|(iv, _)| interval_contains(iv, x, 1e-9 * x.abs().max(1.0)))
                {
                    gap.get_or_insert(x);
                }
            }
            if let Some(x) = gap {
                r.fail(
                    "monotonicity-misses-domain",
                    e,
                    format!("x={x} in domain but in no monotone interval"),
                );
            }
            for (iv, dir) in &d.monotonicity {
                let pts: Vec<(f64, f64)> = xs
                    .iter()
                    .zip(&ys)
                    .filter(|(x, y)| {
                        y.is_finite() && interval_interior(iv, **x, 1e-9 * x.abs().max(1.0))
                    })
                    .map(|(x, y)| (*x, *y))
                    .collect();
                for w in pts.windows(2) {
                    let ((x1, y1), (x2, y2)) = (w[0], w[1]);
                    let tol = 1e-9 * y1.abs().max(y2.abs()) + 1e-300;
                    let bad = match dir {
                        Monotonicity::Increasing => y2 < y1 - tol,
                        Monotonicity::Decreasing => y2 > y1 + tol,
                        Monotonicity::Constant => (y2 - y1).abs() > tol,
                        Monotonicity::Unknown => false,
                    };
                    // Not by less than f's rounding (the expanded (x − 1)⁴
                    // wobbles by 10⁻¹⁵ beside 2).
                    let noise = || spread(x1) + spread(x2) + terms(x1).max(terms(x2));
                    if bad && (y2 - y1).abs() > tol + noise() {
                        r.fail(
                            "monotonicity-contradicted",
                            e,
                            format!(
                                "{dir:?} on ({},{}) but f({x1})={y1}, f({x2})={y2}",
                                iv.lo.value, iv.hi.value
                            ),
                        );
                        break;
                    }
                }
            }
        }
    }
    // Vertical asymptotes are where f blows up (or is undefined nearby).
    for v in &d.vertical_asymptotes {
        if v.period.is_some() {
            continue;
        }
        // |f| grows without bound approaching x (from at least one side).
        // Probes a few float spacings out (20 units out at 10¹² was beside
        // the pole of a cubic whose features are 1 apart).
        let q = (4.0 * quantum(v.x)).max(1e-9);
        // Either side's |f| grows as x approaches, or f keeps changing by
        // as much per decade nearer (a log pole: ln|x| + 10⁶ shrinks in
        // size towards 0 until e^(−10⁶), but steadily).
        let grows = [-1.0, 1.0].iter().any(|s| {
            let near = a.eval(v.x + s * q);
            let mid = a.eval(v.x + s * q * 10.0);
            let far = a.eval(v.x + s * q * 100.0);
            let (d1, d2) = (near - mid, mid - far);
            !near.is_finite()
                || near.abs() > mid.abs() * 1.2
                || (d1.signum() == d2.signum() && d1.abs() >= 0.5 * d2.abs() && d2 != 0.0)
        });
        let near = a.eval(v.x - q).abs().max(a.eval(v.x + q).abs());
        let far = a.eval(v.x - q * 1e3).abs().max(a.eval(v.x + q * 1e3).abs());
        if !grows {
            r.fail(
                "asymptote-without-blowup",
                e,
                format!("x={} but |f| near {near}, further {far}", v.x),
            );
        }
    }
}

/// Known undefined points must be outside the domain and never intercepts.
fn check_undefined(a: &Analysed, points: &[f64], r: &mut Report) {
    if !a.ok() {
        r.fail(
            "undefined-not-analysed",
            &a.expr,
            format!("{:?}", a.k.analysis_error),
        );
        return;
    }
    let d = a.d();
    for &p in points {
        let in_dom = if a.periodic() {
            // Within one period: reduce p into the claimed window.
            let per = d.period.unwrap_or(1.0);
            let w = d.domain.first().map_or(0.0, |iv| iv.lo.value);
            let q = if w.is_finite() {
                w + (p - w).rem_euclid(per)
            } else {
                p
            };
            d.domain.iter().any(|iv| interval_interior(iv, q, 1e-9))
                && !d.excluded.iter().any(|e| e.contains(p, 1e-9))
        } else {
            d.domain.iter().any(|iv| interval_interior(iv, p, 1e-9))
                && !d.excluded.iter().any(|e| e.contains(p, 1e-9))
        };
        if in_dom && !a.unknown(flags::DOMAIN) {
            r.fail(
                "undefined-point-in-domain",
                &a.expr,
                format!(
                    "x={p} is undefined but domain {} excl {:?}",
                    fmt_set(&d.domain),
                    d.excluded.iter().map(|e| e.x).collect::<Vec<_>>()
                ),
            );
        }
        if p == 0.0 && d.y_intercept.is_some() {
            r.fail(
                "undefined-y-intercept",
                &a.expr,
                format!("y-intercept {:?} at undefined x=0", d.y_intercept),
            );
        }
        if d.zeros.iter().any(|z| z.contains(p, 1e-9)) {
            r.fail(
                "undefined-zero",
                &a.expr,
                format!("x-intercept at undefined x={p}"),
            );
        }
        for (fx, y) in d.minima.iter().chain(&d.maxima) {
            if fx.contains(p, 1e-9) {
                r.fail(
                    "undefined-extremum",
                    &a.expr,
                    format!("extremum ({p}, {y}) at an undefined point"),
                );
            }
        }
    }
}

fn nonperiodic_xs(fams: &[Family]) -> Vec<f64> {
    fams.iter()
        .filter(|f| f.period.is_none())
        .map(|f| f.x)
        .collect()
}

/// Compare two x-feature lists after mapping the base's through `map`.
fn same_points(
    base: &[f64],
    other: &[f64],
    map: &dyn Fn(f64) -> f64,
    xtol: &dyn Fn(f64) -> f64,
) -> bool {
    base.len() == other.len()
        && base
            .iter()
            .all(|&b| other.iter().any(|&o| close(map(b), o, 0.0, xtol(map(b)))))
}

fn same_set(a: &[Interval], b: &[Interval], rel: f64, abs: f64) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            close(x.lo.value, y.lo.value, rel, abs) && close(x.hi.value, y.hi.value, rel, abs)
        })
}

/// Transformation of a base function and what it does to the results.
#[derive(Clone, Copy)]
enum T {
    /// f(x − a)
    Shift(f64),
    /// f(x) + c
    Offset(f64),
    /// k · f(x)
    Scale(f64),
    /// f(x / s)
    Stretch(f64),
}

impl T {
    fn apply(self, base: &str) -> String {
        match self {
            T::Shift(a) => subst_x(base, &format!("x-{}", num(a))),
            T::Offset(c) => format!("({base})+{}", num(c)),
            T::Scale(k) => format!("{}*({base})", num(k)),
            T::Stretch(s) => subst_x(base, &format!("x/{}", num(s))),
        }
    }
    fn label(self) -> String {
        match self {
            T::Shift(a) => format!("shift {a}"),
            T::Offset(c) => format!("offset {c}"),
            T::Scale(k) => format!("scale {k}"),
            T::Stretch(s) => format!("stretch {s}"),
        }
    }
    fn mx(self, x: f64) -> f64 {
        match self {
            T::Shift(a) => x + a,
            T::Stretch(s) => x * s,
            _ => x,
        }
    }
    fn my(self, y: f64) -> f64 {
        match self {
            T::Offset(c) => y + c,
            T::Scale(k) => y * k,
            _ => y,
        }
    }
}

fn check_pair(b: &Analysed, o: &Analysed, t: T, r: &mut Report) {
    if !b.ok() || !o.ok() || b.periodic() || o.periodic() {
        return;
    }
    let (bd, od) = (b.d(), o.d());
    let e = &o.expr;
    let shift = match t {
        T::Shift(a) => a.abs(),
        _ => 0.0,
    };
    let xtol = |x: f64| 1e-10 * shift + 1e-6 * x.abs().max(1.0);
    let both = |flag: u32| !b.unknown(flag) && !o.unknown(flag);
    if both(flags::ZEROS) && !matches!(t, T::Offset(_)) {
        let (bz, oz) = (nonperiodic_xs(&bd.zeros), nonperiodic_xs(&od.zeros));
        if !same_points(&bz, &oz, &|x| t.mx(x), &xtol) {
            r.fail(
                "transform-zeros-differ",
                e,
                format!("{}: base zeros {bz:?}, got {oz:?}", t.label()),
            );
        }
    }
    if both(flags::VERTICAL_ASYMPTOTES) && !matches!(t, T::Offset(_) | T::Scale(_)) {
        let (bv, ov) = (
            nonperiodic_xs(&bd.vertical_asymptotes),
            nonperiodic_xs(&od.vertical_asymptotes),
        );
        if !same_points(&bv, &ov, &|x| t.mx(x), &xtol) {
            r.fail(
                "transform-poles-differ",
                e,
                format!("{}: base poles {bv:?}, got {ov:?}", t.label()),
            );
        }
    }
    // Extrema: x mapped; y mapped (minima ↔ maxima for a negative scale).
    let flip = matches!(t, T::Scale(k) if k < 0.0);
    for (bl, ol, bf, of, name) in [
        (
            &bd.minima,
            if flip { &od.maxima } else { &od.minima },
            flags::MINIMA,
            if flip { flags::MAXIMA } else { flags::MINIMA },
            "minima",
        ),
        (
            &bd.maxima,
            if flip { &od.minima } else { &od.maxima },
            flags::MAXIMA,
            if flip { flags::MINIMA } else { flags::MAXIMA },
            "maxima",
        ),
    ] {
        if b.unknown(bf) || o.unknown(of) {
            continue;
        }
        let bx: Vec<f64> = bl
            .iter()
            .filter(|m| m.0.period.is_none())
            .map(|m| m.0.x)
            .collect();
        let ox: Vec<f64> = ol
            .iter()
            .filter(|m| m.0.period.is_none())
            .map(|m| m.0.x)
            .collect();
        if !same_points(&bx, &ox, &|x| t.mx(x), &xtol) {
            r.fail(
                "transform-extrema-differ",
                e,
                format!("{}: base {name} x {bx:?}, got {ox:?}", t.label()),
            );
            continue;
        }
        for (bm, om) in bl.iter().zip(ol.iter()) {
            let want = t.my(bm.1);
            if !close(want, om.1, 1e-6, 1e-15 * want.abs().max(1.0)) && bl.len() == 1 {
                r.fail(
                    "transform-extremum-y-differs",
                    e,
                    format!("{}: {name} y want {want}, got {}", t.label(), om.1),
                );
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
        let c = match t {
            T::Offset(c) => c.abs(),
            _ => 0.0,
        };
        if !same_set(&want, &od.range, 1e-6, 1e-3 * c) {
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
    if both(flags::HORIZONTAL_ASYMPTOTES) && matches!(t, T::Shift(_) | T::Stretch(_)) {
        let bh: Vec<f64> = bd.horizontal_asymptotes.iter().map(|h| h.0).collect();
        let oh: Vec<f64> = od.horizontal_asymptotes.iter().map(|h| h.0).collect();
        if bh.len() != oh.len()
            || !bh
                .iter()
                .all(|y| oh.iter().any(|z| close(*y, *z, 1e-6, 1e-12)))
        {
            r.fail(
                "transform-hasymptotes-differ",
                e,
                format!("{}: base {bh:?}, got {oh:?}", t.label()),
            );
        }
    }
    if let T::Shift(a) = t
        && a != 0.0
        && matches!(o.k.parity, Parity::Even | Parity::Odd)
        && !matches!(b.k.parity, Parity::Unknown)
        && bd.range.len() == 1
        && bd.range[0].lo.value != bd.range[0].hi.value
    {
        r.fail(
            "shifted-parity",
            e,
            format!(
                "{}: parity {:?} (base {:?})",
                t.label(),
                o.k.parity,
                b.k.parity
            ),
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let filter = args.iter().find(|a| !a.starts_with("--")).cloned();
    let mut r = Report {
        failures: BTreeMap::new(),
    };
    let started = std::time::Instant::now();
    let mut count = 0usize;
    let mut slowest = (0.0f64, String::new());
    let transforms: Vec<T> = [0.0, 1.0, 1e3, 1e6, 1e9, 1e12]
        .into_iter()
        .map(T::Shift)
        .chain(
            [
                1e-12, -1e-12, 1e-9, -1e-9, 1e-7, -1e-7, 1e-3, -1e-3, 1.0, -1.0, 1e6, -1e6,
            ]
            .into_iter()
            .map(T::Offset),
        )
        .chain(
            [1e-12, -1e-12, 1e-6, -1e-6, -1.0, 1e6, 1e12]
                .into_iter()
                .map(T::Scale),
        )
        .chain([1e-3, 1e3, 1e6].into_iter().map(T::Stretch))
        .collect();
    for base in BASES {
        if filter.as_ref().is_some_and(|f| !base.contains(f.as_str())) {
            continue;
        }
        let Some(b) = Analysed::new(base, TrigUnit::Radians) else {
            r.fail("does-not-parse", base, String::new());
            continue;
        };
        check_one(&b, 0.0, &mut r);
        for &t in &transforms {
            if matches!(t, T::Shift(a) if a == 0.0) {
                continue;
            }
            let expr = t.apply(base);
            let t0 = std::time::Instant::now();
            let Some(o) = Analysed::new(&expr, TrigUnit::Radians) else {
                r.fail("does-not-parse", &expr, String::new());
                continue;
            };
            let dt = t0.elapsed().as_secs_f64() * 1e3;
            if dt > slowest.0 {
                slowest = (dt, expr.clone());
            }
            count += 1;
            let centre = match t {
                T::Shift(a) => a,
                _ => 0.0,
            };
            check_one(&o, centre, &mut r);
            check_pair(&b, &o, t, &mut r);
        }
    }
    for (expr, unit, points) in UNDEFINED {
        if filter.as_ref().is_some_and(|f| !expr.contains(f.as_str())) {
            continue;
        }
        match Analysed::new(expr, *unit) {
            Some(a) => {
                count += 1;
                let tagged = Analysed {
                    expr: format!("{expr}  [{unit:?}]"),
                    ..a
                };
                check_one(&tagged, 0.0, &mut r);
                check_undefined(&tagged, points, &mut r);
            }
            None => r.fail("does-not-parse", expr, String::new()),
        }
    }
    let total: usize = r.failures.values().map(Vec::len).sum();
    println!(
        "{count} functions analysed in {:.1} s (slowest {:.0} ms: y={}); {total} failures",
        started.elapsed().as_secs_f64(),
        slowest.0,
        slowest.1
    );
    // One line per (check, expression), with a count, unless --all.
    let all = args.iter().any(|a| a == "--all");
    for (check, list) in &r.failures {
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
}
