//! The numeric analysis algorithm.
//!
//! Outline:
//! 1. Compile f, f′ and f″ (symbolic derivatives; numeric fallback).
//! 2. Detect periodicity from the expression (trig functions of linear
//!    arguments, `mod`) and verify/reduce it numerically (|sin x| → π).
//! 3. Scan a window — one period, or [−10⁶, 10⁶] on a grid that is dense
//!    near 0 — for domain boundaries, excluded points (zeros of
//!    denominators, `cos` of `tan`'s argument, …), poles and jumps, which
//!    split the window into continuity pieces.
//! 4. On each piece: zeros (sign changes + Brent), extrema (sign changes of
//!    f′), inflection points (sign changes of f″), monotone intervals and
//!    the range (extrema, endpoint values and limits).
//! 5. Limits at ±∞ give horizontal/oblique asymptotes.
//!
//! Every evaluation is charged to a work budget (bytecode instructions):
//! a function too expensive to analyze reports `TooComplex` instead of
//! blocking its caller for seconds, and a caller can cancel from another
//! thread.

use super::format::{
    Bound, Interval, MINUS, Nice, format_family, format_number, format_number_tol,
    format_periodic_set, format_point, format_set,
};
use super::numeric::{
    SeqLimit, bisect_finite, brent, diverges_near, limit_at_infinity, one_sided_limit,
    sequence_limit, sinh_grid, uniform_grid,
};
use super::{
    AnalysisData, AnalysisError, AsymptoteSide, Family, KeyGraphFeatures, Monotonicity, Parity,
    Periodicity, flags,
};
use crate::ast::{BinOp, Expr, Func};
use crate::compile::{CompileOptions, Input, Program, syntactic_rational};
use crate::diff::derivative_bounded;
use crate::simplify::linear_in;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

/// Half-width of the analysis window for non-periodic functions.
const WINDOW: f64 = 1e6;
const N_HALF: usize = 20_000;
const PERIOD_STEPS: usize = 6_000;
/// More features than this (without periodicity) are reported as too complex.
const MAX_LISTED: usize = 24;
const MAX_EVENTS: usize = 400;
/// Most work one analysis may do, in [`Program::cost`] units over all
/// evaluations of f, f′, f″ and the candidate generators: a few hundred
/// milliseconds. Ordinary functions use up to about a tenth of it.
const WORK_BUDGET: u64 = 150_000_000;
/// Largest symbolic derivative used (nodes); a bigger one would cost more
/// per evaluation than finite differences of f, which are used instead.
const MAX_DERIVATIVE_NODES: usize = 4096;

/// Why an analysis stopped without a result.
pub(crate) enum Stop {
    /// The caller's cancel flag was set.
    Cancelled,
    /// A reportable failure.
    Error(AnalysisError),
}

impl From<AnalysisError> for Stop {
    fn from(e: AnalysisError) -> Stop {
        Stop::Error(e)
    }
}

struct Fun<'a> {
    f: Program,
    df: Option<Program>,
    d2f: Option<Program>,
    /// Instructions executed so far.
    work: Cell<u64>,
    budget: u64,
    cancel: Option<&'a AtomicBool>,
    cancelled: Cell<bool>,
}

impl Fun<'_> {
    /// Charges `n` work units. False once the budget is spent or the
    /// caller cancelled; evaluations then return NaN, which every stage
    /// already treats as "undefined", so the analysis winds down quickly.
    #[inline]
    fn spend(&self, n: usize) -> bool {
        let w = self.work.get().saturating_add(n.max(1) as u64);
        self.work.set(w);
        if w > self.budget {
            return false;
        }
        if self.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            self.cancelled.set(true);
            self.work.set(u64::MAX);
            return false;
        }
        true
    }

    /// Whether the analysis must be abandoned, and how to report it.
    fn stopped(&self) -> Option<Stop> {
        if self.cancelled.get() {
            Some(Stop::Cancelled)
        } else if self.work.get() > self.budget {
            Some(Stop::Error(AnalysisError::TooComplex))
        } else {
            None
        }
    }

    /// Evaluates `p` at `x`, charged to the budget.
    #[inline]
    fn eval(&self, p: &Program, x: f64) -> f64 {
        if self.spend(p.cost()) {
            p.eval_x(x)
        } else {
            f64::NAN
        }
    }

    /// Evaluates `p` on a grid, charged to the budget.
    fn eval_batch(&self, p: &Program, xs: &[f64], out: &mut [f64]) {
        if self.spend(p.cost().saturating_mul(xs.len())) {
            p.eval_batch(Input::Slice(xs), Input::Scalar(0.0), out);
        } else {
            out.fill(f64::NAN);
        }
    }

    #[inline]
    fn f(&self, x: f64) -> f64 {
        self.eval(&self.f, x)
    }

    fn df(&self, x: f64) -> f64 {
        match &self.df {
            Some(p) => self.eval(p, x),
            None => {
                let h = 1e-6 * x.abs().max(1.0);
                (self.f(x + h) - self.f(x - h)) / (2.0 * h)
            }
        }
    }

    fn d2f(&self, x: f64) -> f64 {
        match &self.d2f {
            Some(p) => self.eval(p, x),
            None => {
                let h = 1e-4 * x.abs().max(1.0);
                (self.df(x + h) - self.df(x - h)) / (2.0 * h)
            }
        }
    }

    fn batch(&self, which: u8, xs: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; xs.len()];
        let prog = match which {
            0 => Some(&self.f),
            1 => self.df.as_ref(),
            _ => self.d2f.as_ref(),
        };
        match prog {
            Some(p) => self.eval_batch(p, xs, &mut out),
            None => {
                for (o, &x) in out.iter_mut().zip(xs) {
                    *o = if which == 1 { self.df(x) } else { self.d2f(x) };
                }
            }
        }
        out
    }
}

pub(crate) fn analyze_expr(
    expr: &Expr,
    opts: &CompileOptions<'_>,
    cancel: Option<&AtomicBool>,
) -> Result<KeyGraphFeatures, Stop> {
    analyze_with_budget(expr, opts, cancel, WORK_BUDGET)
}

fn analyze_with_budget(
    expr: &Expr,
    opts: &CompileOptions<'_>,
    cancel: Option<&AtomicBool>,
    budget: u64,
) -> Result<KeyGraphFeatures, Stop> {
    let f = Program::compile(expr, opts).map_err(|_| AnalysisError::AnalysisCouldNotBePerformed)?;
    let unit = opts.trig_unit;
    let d1 = derivative_bounded(expr, unit, MAX_DERIVATIVE_NODES);
    let d2 = d1
        .as_ref()
        .and_then(|d| derivative_bounded(d, unit, MAX_DERIVATIVE_NODES));
    let df = d1.and_then(|e| Program::compile(&e, opts).ok());
    let d2f = d2.and_then(|e| Program::compile(&e, opts).ok());
    let fun = Fun {
        f,
        df,
        d2f,
        work: Cell::new(0),
        budget,
        cancel,
        cancelled: Cell::new(false),
    };

    if let Some(c) = fun.f.as_constant() {
        return if c.is_finite() {
            Ok(constant_features(c))
        } else {
            Err(AnalysisError::AnalysisCouldNotBePerformed.into())
        };
    }

    let gens = generator_programs(expr, opts);
    let scale = typical_scale(&fun);
    let period = detect_period(expr, &fun, opts, scale);
    if let Some(stop) = fun.stopped() {
        return Err(stop);
    }

    let k = match period {
        Some(p) => analyze_periodic(&fun, &gens, p, scale),
        None => analyze_aperiodic(&fun, &gens, scale),
    };
    // Running out of budget can surface as any failure downstream.
    if let Some(stop) = fun.stopped() {
        return Err(stop);
    }
    let mut k = k?;
    // Γ-based functions have poles at every negative integer: a numeric
    // scan cannot list them.
    let gamma_like = expr.any(&|e| {
        matches!(e, Expr::Call(Func::Factorial | Func::DoubleFactorial | Func::NCr | Func::NPr, args) if args.iter().any(|a| a.contains_x()))
    });
    if gamma_like {
        k.too_complex_features |= flags::DOMAIN
            | flags::RANGE
            | flags::VERTICAL_ASYMPTOTES
            | flags::MONOTONE_INTERVALS
            | flags::MINIMA
            | flags::MAXIMA
            | flags::INFLECTION_POINTS;
        k.domain.clear();
        k.range.clear();
        k.vertical_asymptotes.clear();
        k.monotonicity.clear();
        k.minima.clear();
        k.maxima.clear();
        k.inflection_points.clear();
    }
    k.parity = parity(&fun, scale);
    if let Some(stop) = fun.stopped() {
        return Err(stop);
    }
    match period {
        Some(p) => {
            k.periodicity_direction = Periodicity::Periodic;
            k.periodicity_expression = format_number(p);
        }
        None => k.periodicity_direction = Periodicity::NotPeriodic,
    }
    Ok(k)
}

fn constant_features(c: f64) -> KeyGraphFeatures {
    let all = Interval::all();
    let mut k = KeyGraphFeatures {
        domain: format_set("x", &[all]),
        range: format_set("y", &[Interval::closed(c, c)]),
        parity: Parity::Even,
        periodicity_direction: Periodicity::NotPeriodic,
        y_intercept: format_number(c),
        monotonicity: vec![(all.format(), Monotonicity::Constant)],
        ..Default::default()
    };
    if Nice::of(c).value() == 0.0 {
        k.x_intercept = format_set("x", &[all]);
    }
    k.data = AnalysisData {
        domain: vec![all],
        range: vec![Interval::closed(c, c)],
        y_intercept: Some(c),
        monotonicity: vec![(all, Monotonicity::Constant)],
        ..Default::default()
    };
    k
}

/// Median |f| over a sample of moderate x values: a magnitude for
/// tolerances.
fn typical_scale(fun: &Fun) -> f64 {
    let mut v: Vec<f64> = (0..200)
        .map(|i| -10.0 + 20.0 * (i as f64 + 0.37) / 200.0)
        .map(|x| fun.f(x).abs())
        .filter(|v| v.is_finite())
        .collect();
    if v.is_empty() {
        return 1.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2].max(1e-12)
}

// ---------------------------------------------------------------------------
// Candidate generators: sub-expressions whose zeros may be singular points
// or domain boundaries.

fn generators(e: &Expr, out: &mut Vec<Expr>) {
    let push = |g: Expr, out: &mut Vec<Expr>| {
        if g.contains_x() && !out.contains(&g) {
            out.push(g);
        }
    };
    let shift = |u: &Expr, c: f64| Expr::bin(BinOp::Sub, u.clone(), Expr::Num(c));
    match e {
        Expr::Bin(op, a, b) => {
            match op {
                BinOp::Div => push((**b).clone(), out),
                BinOp::Pow => {
                    let integral_nonneg = matches!(syntactic_rational(b), Some((p, 1)) if p >= 0);
                    if !integral_nonneg {
                        push((**a).clone(), out);
                    }
                }
                _ => {}
            }
            generators(a, out);
            generators(b, out);
        }
        Expr::Neg(a) | Expr::Degrees(a) => generators(a, out),
        Expr::Call(f, args) => {
            use Func::*;
            let u = &args[0];
            match f {
                Tan | Sec => push(Expr::call1(Cos, u.clone()), out),
                Cot | Csc => push(Expr::call1(Sin, u.clone()), out),
                Coth | Csch | Ln | Log | Sqrt | Acsch | Sign => push(u.clone(), out),
                Asin | Acos | Atanh | Acoth => {
                    push(shift(u, 1.0), out);
                    push(shift(u, -1.0), out);
                }
                Acosh => push(shift(u, 1.0), out),
                Asec | Acsc => {
                    push(shift(u, 1.0), out);
                    push(shift(u, -1.0), out);
                    push(u.clone(), out);
                }
                Asech => {
                    push(u.clone(), out);
                    push(shift(u, 1.0), out);
                }
                Root => push(u.clone(), out),
                LogBase => {
                    push(args[1].clone(), out);
                    push(shift(&args[0], 1.0), out);
                    push(args[0].clone(), out);
                }
                Mod => push(args[1].clone(), out),
                _ => {}
            }
            for a in args {
                generators(a, out);
            }
        }
        _ => {}
    }
}

fn generator_programs(expr: &Expr, opts: &CompileOptions<'_>) -> Vec<Program> {
    let mut gens = Vec::new();
    generators(expr, &mut gens);
    gens.iter()
        .filter_map(|g| Program::compile(g, opts).ok())
        .filter(|p| p.as_constant().is_none())
        .collect()
}

/// Zeros of `g` on a grid: sign changes (Brent), exact zeros, and
/// even-order zeros found as tiny local minima of |g|.
fn program_zeros(fun: &Fun, g: &Program, xs: &[f64], out: &mut Vec<f64>) {
    let mut gs = vec![0.0; xs.len()];
    fun.eval_batch(g, xs, &mut gs);
    let mut ge = |x: f64| fun.eval(g, x);
    let gscale = {
        let mut v: Vec<f64> = gs
            .iter()
            .map(|v| v.abs())
            .filter(|v| v.is_finite())
            .collect();
        v.sort_by(|a, b| a.total_cmp(b));
        if v.is_empty() {
            1.0
        } else {
            v[v.len() / 2].max(1e-300)
        }
    };
    let start_len = out.len();
    for i in 0..xs.len() {
        if out.len() - start_len > 4 * MAX_EVENTS {
            break;
        }
        let a = gs[i];
        if a == 0.0 {
            out.push(xs[i]);
            continue;
        }
        if i + 1 < xs.len() {
            let b = gs[i + 1];
            if a.is_finite() && b.is_finite() && b != 0.0 && (a < 0.0) != (b < 0.0) {
                let r = brent(&mut ge, xs[i], a, xs[i + 1], b);
                // Only genuine zeros (not poles of g).
                if ge(r).abs() <= 1e-6 * a.abs().max(b.abs()) || ge(r) == 0.0 {
                    out.push(r);
                }
            }
        }
        if i > 0 && i + 1 < xs.len() {
            let (p, n) = (gs[i - 1].abs(), gs[i + 1].abs());
            let c = a.abs();
            if c < p
                && c <= n
                && p.is_finite()
                && n.is_finite()
                && c < 0.5 * p.max(n)
                && (gs[i - 1] < 0.0) == (gs[i + 1] < 0.0)
            {
                let m = golden_min_abs(&mut ge, xs[i - 1], xs[i + 1]);
                let gm = ge(m).abs();
                if gm <= 1e-10 * gscale.max(1.0) {
                    // Prefer a closed form that is at least as good a root.
                    // Golden-section search locates an even-order zero to
                    // ~1e-8; trust a closed form that close.
                    let nice = Nice::with_tol(m, 1e-7);
                    out.push(if matches!(nice, Nice::Decimal(_)) {
                        m
                    } else {
                        nice.value()
                    });
                }
            }
        }
    }
}

fn golden_min_abs(f: &mut dyn FnMut(f64) -> f64, mut a: f64, mut b: f64) -> f64 {
    let gr = (5f64.sqrt() - 1.0) / 2.0;
    let mut c = b - gr * (b - a);
    let mut d = a + gr * (b - a);
    let mut fc = f(c).abs();
    let mut fd = f(d).abs();
    for _ in 0..120 {
        if (b - a).abs() <= 1e-15 * a.abs().max(b.abs()).max(1e-300) {
            break;
        }
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - gr * (b - a);
            fc = f(c).abs();
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + gr * (b - a);
            fd = f(d).abs();
        }
    }
    0.5 * (a + b)
}

/// Snaps a computed point to a recognised closed form when that is at
/// least as good a root of `g` (or when `g` is not given).
fn snap(x: f64, tol: f64) -> f64 {
    let n = Nice::with_tol(x, tol);
    match n {
        Nice::Decimal(_) => x,
        _ => n.value(),
    }
}

// ---------------------------------------------------------------------------
// Periodicity

#[derive(Clone, Copy, Debug, PartialEq)]
enum Per {
    Const,
    Periodic(f64),
    Not,
}

fn lcm_period(a: f64, b: f64) -> Option<f64> {
    let r = a / b;
    for q in 1..=24i64 {
        let p = (r * q as f64).round();
        if (1.0..=24.0).contains(&p) && (r - p / q as f64).abs() <= 1e-9 * r {
            // a/b = p/q → lcm = a·q = b·p
            return Some(a * q as f64);
        }
    }
    None
}

fn combine(a: Per, b: Per) -> Per {
    match (a, b) {
        (Per::Not, _) | (_, Per::Not) => Per::Not,
        (Per::Const, x) | (x, Per::Const) => x,
        (Per::Periodic(p), Per::Periodic(q)) => lcm_period(p, q).map_or(Per::Not, Per::Periodic),
    }
}

fn linear_coefficient(u: &Expr, opts: &CompileOptions<'_>) -> Option<f64> {
    let (c, _) = linear_in(u, &|e| matches!(e, Expr::X))?;
    if c.contains_x() || c.contains_y() {
        return None;
    }
    let v = Program::compile(&c, opts).ok()?.as_constant()?;
    if v != 0.0 && v.is_finite() {
        Some(v)
    } else {
        None
    }
}

fn per(e: &Expr, opts: &CompileOptions<'_>) -> Per {
    if !e.contains_x() {
        return Per::Const;
    }
    match e {
        Expr::X => Per::Not,
        Expr::Neg(a) | Expr::Degrees(a) => per(a, opts),
        Expr::Bin(_, a, b) => combine(per(a, opts), per(b, opts)),
        Expr::Call(f, args) => {
            use Func::*;
            let turn = opts.trig_unit.full_turn();
            match f {
                Sin | Cos | Sec | Csc | Tan | Cot => {
                    if let Some(a) = linear_coefficient(&args[0], opts) {
                        let base = if matches!(f, Tan | Cot) {
                            turn / 2.0
                        } else {
                            turn
                        };
                        return Per::Periodic(base / a.abs());
                    }
                }
                Mod => {
                    if !args[1].contains_x()
                        && let (Some(a), Ok(m)) = (
                            linear_coefficient(&args[0], opts),
                            Program::compile(&args[1], opts),
                        )
                        && let Some(m) = m.as_constant()
                        && m != 0.0
                        && m.is_finite()
                    {
                        return Per::Periodic((m / a).abs());
                    }
                }
                _ => {}
            }
            args.iter()
                .fold(Per::Const, |acc, a| combine(acc, per(a, opts)))
        }
        _ => Per::Not,
    }
}

fn verify_period(fun: &Fun, p: f64, scale: f64) -> bool {
    let mut ok = 0;
    let mut bad = 0;
    for i in 0..64 {
        let t = -3.1 * p + i as f64 * 0.1037 * p + 0.012_345 * p;
        let a = fun.f(t);
        let b = fun.f(t + p);
        match (a.is_finite(), b.is_finite()) {
            (true, true) => {
                if (a - b).abs() <= 1e-7 * (a.abs() + b.abs()) + 1e-9 * scale {
                    ok += 1;
                } else {
                    bad += 1;
                }
            }
            (false, false) => {}
            _ => bad += 1,
        }
    }
    ok >= 16 && bad <= 1
}

fn detect_period(expr: &Expr, fun: &Fun, opts: &CompileOptions<'_>, scale: f64) -> Option<f64> {
    let candidate = match per(expr, opts) {
        Per::Periodic(p) => Some(p),
        _ => {
            // Non-smooth periodic constructions such as x − floor(x).
            let nonsmooth = expr.any(&|e| {
                matches!(
                    e,
                    Expr::Call(Func::Floor | Func::Ceil | Func::Round | Func::Mod, _)
                )
            });
            if nonsmooth {
                let turn = opts.trig_unit.full_turn();
                [1.0, 2.0, turn / 2.0, turn]
                    .into_iter()
                    .find(|&p| verify_period(fun, p, scale))
            } else {
                None
            }
        }
    }?;
    if !verify_period(fun, candidate, scale) {
        return None;
    }
    let p = (2..=24)
        .rev()
        .map(|k| candidate / k as f64)
        .find(|&q| verify_period(fun, q, scale))
        .unwrap_or(candidate);
    Some(p)
}

// ---------------------------------------------------------------------------
// Parity

fn parity(fun: &Fun, scale: f64) -> Parity {
    let mut even = true;
    let mut odd = true;
    let mut any = false;
    for i in 0..60 {
        let x = 0.0371 * 1.31f64.powi(i) + 1e-3 * i as f64;
        let (a, b) = (fun.f(x), fun.f(-x));
        match (a.is_finite(), b.is_finite()) {
            (false, false) => continue,
            (true, true) => {}
            _ => return Parity::Neither,
        }
        any = true;
        let tol = 1e-9 * (a.abs() + b.abs()) + 1e-13 * scale;
        if (a - b).abs() > tol {
            even = false;
        }
        if (a + b).abs() > tol {
            odd = false;
        }
        if !even && !odd {
            return Parity::Neither;
        }
    }
    if !any {
        Parity::Unknown
    } else if even {
        Parity::Even
    } else if odd {
        Parity::Odd
    } else {
        Parity::Neither
    }
}

// ---------------------------------------------------------------------------
// Scanning a window into continuity pieces.

#[derive(Clone, Copy, Debug, PartialEq)]
enum EventKind {
    /// Domain boundary; `entering` = the domain starts here (going right).
    Boundary { entering: bool, closed: bool },
    /// Point excluded from an otherwise defined neighbourhood (hole or pole).
    Excluded { left_in: bool, right_in: bool },
    /// Jump discontinuity; `closed_left` = the point belongs to the left piece.
    Jump { closed_left: bool },
    /// Jump where the value at the point matches neither side.
    Isolated,
}

#[derive(Clone, Copy, Debug)]
struct Event {
    x: f64,
    kind: EventKind,
}

#[derive(Clone, Debug)]
struct Piece {
    lo: Bound,
    hi: Bound,
    /// Sample points inside the piece, including closed finite ends.
    samples: Vec<(f64, f64)>,
    /// The piece continues to the next one across a jump (same domain
    /// interval).
    jump_after: bool,
}

struct Scan {
    pieces: Vec<Piece>,
    poles: Vec<f64>,
    excluded: Vec<f64>,
    too_complex: bool,
}

/// Which samples are in the domain: finite values, plus runs of ±∞ that
/// continue from huge finite values (floating-point overflow such as e^x
/// for x > 709.8, not poles).
fn defined_mask(fs: &[f64]) -> Vec<bool> {
    let n = fs.len();
    let mut def: Vec<bool> = fs.iter().map(|v| v.is_finite()).collect();
    let mut i = 0;
    while i < n {
        if fs[i].is_infinite() {
            let mut j = i;
            while j + 1 < n && fs[j + 1].is_infinite() {
                j += 1;
            }
            let huge =
                |k: Option<usize>| k.is_some_and(|k| fs[k].is_finite() && fs[k].abs() > 1e250);
            let left = if i > 0 { Some(i - 1) } else { None };
            let right = if j + 1 < n { Some(j + 1) } else { None };
            if huge(left) || huge(right) {
                def[i..=j].iter_mut().for_each(|d| *d = true);
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    def
}

/// Bisects a large step between two finite samples towards the jump; a
/// continuous function's step shrinks, a jump's or pole's does not.
fn find_jump(
    fun: &Fun,
    mut a: f64,
    mut fa: f64,
    mut b: f64,
    mut fb: f64,
    scale: f64,
) -> Option<f64> {
    for _ in 0..120 {
        let m = 0.5 * (a + b);
        if m <= a || m >= b {
            // Down to adjacent floats: a steep but continuous function still
            // changes by about |f′|·(b − a) here.
            let d = fun.df(m).abs();
            if d.is_finite() && (fb - fa).abs() <= 10.0 * d * (b - a) {
                return None;
            }
            break;
        }
        let fm = fun.f(m);
        if !fm.is_finite() {
            return Some(m);
        }
        if (fm - fa).abs() >= (fb - fm).abs() {
            b = m;
            fb = fm;
        } else {
            a = m;
            fa = fm;
        }
        if (fb - fa).abs() <= 1e-9 * scale.max(fa.abs().min(fb.abs())) {
            return None;
        }
    }
    Some(0.5 * (a + b))
}

fn scan(
    fun: &Fun,
    xs: &[f64],
    extra_candidates: &[f64],
    gens: &[Program],
    lo_inf: bool,
    hi_inf: bool,
    scale: f64,
) -> Scan {
    let n = xs.len();
    let (lo, hi) = (xs[0], xs[n - 1]);
    let fs = fun.batch(0, xs);
    let defined = defined_mask(&fs);
    let mut f = |x: f64| fun.f(x);
    let mut events: Vec<Event> = Vec::new();
    let mut too_complex = false;

    // Candidate points from the expression structure.
    let mut cands: Vec<f64> = extra_candidates.to_vec();
    for g in gens {
        program_zeros(fun, g, xs, &mut cands);
    }
    let mut cands: Vec<f64> = cands
        .into_iter()
        .map(|c| snap(c, 1e-9))
        .filter(|c| *c >= lo && *c <= hi)
        .collect();
    cands.sort_by(|a, b| a.total_cmp(b));
    cands.dedup_by(|a, b| (*a - *b).abs() <= 1e-12 * a.abs().max(1.0));
    if cands.len() > 4 * MAX_EVENTS {
        too_complex = true;
        cands.truncate(4 * MAX_EVENTS);
    }
    let mut poles = Vec::new();
    let mut excluded = Vec::new();
    for &c in &cands {
        let d = 1e-9 * c.abs().max(1.0);
        let fc = f(c);
        // ±∞ beside c is floating-point overflow of a defined value
        // (e^(1/x) just right of 0); NaN is undefined.
        let fl = f(c - d);
        let fr = f(c + d);
        let (dl, dr) = (!fl.is_nan(), !fr.is_nan());
        let pole_l = dl && diverges_near(&mut f, c, -1.0).is_some();
        let pole_r = dr && diverges_near(&mut f, c, 1.0).is_some();
        if !dl && !dr {
            continue;
        }
        if !fc.is_finite() || pole_l || pole_r {
            if pole_l || pole_r {
                poles.push(c);
            }
            if dl && dr {
                excluded.push(c);
                events.push(Event {
                    x: c,
                    kind: EventKind::Excluded {
                        left_in: true,
                        right_in: true,
                    },
                });
            } else {
                // Open domain boundary at c.
                events.push(Event {
                    x: c,
                    kind: EventKind::Boundary {
                        entering: dr,
                        closed: false,
                    },
                });
            }
        }
    }

    // Finiteness transitions between grid nodes not explained by an event.
    let has_event_between =
        |events: &[Event], a: f64, b: f64| events.iter().any(|e| e.x >= a && e.x <= b);
    for i in 0..n - 1 {
        let (a, b) = (defined[i], defined[i + 1]);
        if a == b || has_event_between(&events, xs[i], xs[i + 1]) {
            continue;
        }
        let (good, bad) = if a {
            (xs[i], xs[i + 1])
        } else {
            (xs[i + 1], xs[i])
        };
        let edge = bisect_finite(&mut f, good, bad);
        let s = snap(edge, 1e-9);
        let (x, closed) = if f(s).is_finite() {
            (s, true)
        } else if (s - edge).abs() <= 1e-9 * edge.abs().max(1.0) {
            (s, false)
        } else {
            (edge, true)
        };
        // A boundary approached with diverging values is an asymptote.
        if (!closed || diverges_near(&mut f, x, if a { -1.0 } else { 1.0 }).is_some())
            && diverges_near(&mut f, x, if a { -1.0 } else { 1.0 }).is_some()
        {
            poles.push(x);
        }
        events.push(Event {
            x,
            kind: EventKind::Boundary {
                entering: !a,
                closed,
            },
        });
        if events.len() > MAX_EVENTS {
            too_complex = true;
            break;
        }
    }

    // Jumps and undetected poles between finite neighbours: steps much
    // larger than a neighbouring step.
    if !too_complex {
        let steps: Vec<f64> = (0..n - 1).map(|i| (fs[i + 1] - fs[i]).abs()).collect();
        let mut suspects: Vec<usize> = Vec::new();
        for i in 0..n - 1 {
            if !steps[i].is_finite() || steps[i] == 0.0 {
                continue;
            }
            let prev = if i > 0 { steps[i - 1] } else { f64::INFINITY };
            let next = if i + 2 < n {
                steps[i + 1]
            } else {
                f64::INFINITY
            };
            let neigh = if prev.is_finite() {
                prev
            } else {
                f64::INFINITY
            }
            .min(if next.is_finite() {
                next
            } else {
                f64::INFINITY
            });
            let neigh = if neigh.is_finite() { neigh } else { 0.0 };
            if steps[i] > 4.0 * neigh + 1e-12 * scale {
                suspects.push(i);
            }
        }
        if suspects.len() > 2 * MAX_EVENTS {
            // Erratic samples (an oscillation the grid cannot resolve, e.g.
            // sin(x²) far from 0): only examine the well-resolved centre.
            suspects.retain(|&i| xs[i].abs() < 50.0);
            if suspects.len() > 2 * MAX_EVENTS {
                too_complex = true;
                suspects.clear();
            }
        }
        for i in suspects {
            if has_event_between(&events, xs[i], xs[i + 1]) {
                continue;
            }
            if let Some(c) = find_jump(fun, xs[i], fs[i], xs[i + 1], fs[i + 1], scale) {
                let pole_l = diverges_near(&mut f, c, -1.0).is_some();
                let pole_r = diverges_near(&mut f, c, 1.0).is_some();
                if pole_l || pole_r {
                    let c = snap(c, 1e-9);
                    poles.push(c);
                    excluded.push(c);
                    events.push(Event {
                        x: c,
                        kind: EventKind::Excluded {
                            left_in: true,
                            right_in: true,
                        },
                    });
                } else {
                    let s = snap(c, 1e-9);
                    let c = if (s - c).abs() <= 1e-9 * c.abs().max(1.0) {
                        s
                    } else {
                        c
                    };
                    let fc = f(c);
                    let l = one_sided_limit(&mut f, c, -1.0);
                    let r = one_sided_limit(&mut f, c, 1.0);
                    let near =
                        |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(scale);
                    if fc.is_finite() && !near(fc, l) && !near(fc, r) {
                        // An isolated value, e.g. sign(0) = 0 between −1 and 1.
                        events.push(Event {
                            x: c,
                            kind: EventKind::Isolated,
                        });
                    } else if fc.is_finite() {
                        let closed_left = (fc - l).abs() <= (fc - r).abs();
                        events.push(Event {
                            x: c,
                            kind: EventKind::Jump { closed_left },
                        });
                    } else {
                        excluded.push(c);
                        events.push(Event {
                            x: c,
                            kind: EventKind::Excluded {
                                left_in: true,
                                right_in: true,
                            },
                        });
                    }
                }
                if events.len() > MAX_EVENTS {
                    too_complex = true;
                    break;
                }
            }
        }
    }
    events.sort_by(|a, b| a.x.total_cmp(&b.x));

    // Walk grid nodes and events in order, building pieces.
    let mut pieces: Vec<Piece> = Vec::new();
    let mut cur: Option<Piece> = None;
    let new_piece = |lo: Bound| Piece {
        lo,
        hi: lo,
        samples: Vec::new(),
        jump_after: false,
    };
    if defined[0] && !events.first().is_some_and(|e| e.x <= lo) {
        let b = if lo_inf {
            Bound {
                value: f64::NEG_INFINITY,
                closed: false,
            }
        } else {
            Bound {
                value: lo,
                closed: true,
            }
        };
        let mut p = new_piece(b);
        if !lo_inf {
            p.samples.push((lo, fs[0]));
        }
        cur = Some(p);
    }
    let mut ei = 0;
    let close = |cur: &mut Option<Piece>,
                 pieces: &mut Vec<Piece>,
                 hi: Bound,
                 f: &mut dyn FnMut(f64) -> f64| {
        if let Some(mut p) = cur.take() {
            p.hi = hi;
            if hi.closed && hi.value.is_finite() {
                let v = f(hi.value);
                if v.is_finite() && p.samples.last().is_none_or(|s| s.0 < hi.value) {
                    p.samples.push((hi.value, v));
                }
            }
            pieces.push(p);
        }
    };
    let open_at = |x: f64, closed: bool, f: &mut dyn FnMut(f64) -> f64| {
        let mut p = Piece {
            lo: Bound { value: x, closed },
            hi: Bound { value: x, closed },
            samples: Vec::new(),
            jump_after: false,
        };
        if closed {
            let v = f(x);
            if v.is_finite() {
                p.samples.push((x, v));
            }
        }
        p
    };
    for i in 0..n {
        let x = xs[i];
        while ei < events.len() && events[ei].x <= x {
            let e = events[ei];
            ei += 1;
            match e.kind {
                EventKind::Boundary { entering, closed } => {
                    if entering {
                        if cur.is_none() {
                            cur = Some(open_at(e.x, closed, &mut f));
                        }
                    } else {
                        close(&mut cur, &mut pieces, Bound { value: e.x, closed }, &mut f);
                    }
                }
                EventKind::Excluded { right_in, .. } => {
                    close(
                        &mut cur,
                        &mut pieces,
                        Bound {
                            value: e.x,
                            closed: false,
                        },
                        &mut f,
                    );
                    if right_in {
                        cur = Some(open_at(e.x, false, &mut f));
                    }
                }
                EventKind::Jump { closed_left } => {
                    let had = cur.is_some();
                    close(
                        &mut cur,
                        &mut pieces,
                        Bound {
                            value: e.x,
                            closed: closed_left,
                        },
                        &mut f,
                    );
                    if had && let Some(p) = pieces.last_mut() {
                        p.jump_after = true;
                    }
                    cur = Some(open_at(e.x, !closed_left, &mut f));
                }
                EventKind::Isolated => {
                    let had = cur.is_some();
                    close(
                        &mut cur,
                        &mut pieces,
                        Bound {
                            value: e.x,
                            closed: false,
                        },
                        &mut f,
                    );
                    if had && let Some(p) = pieces.last_mut() {
                        p.jump_after = true;
                    }
                    let mut point = open_at(e.x, true, &mut f);
                    point.jump_after = true;
                    pieces.push(point);
                    cur = Some(open_at(e.x, false, &mut f));
                }
            }
        }
        // Skip a grid node that coincides with an event.
        if ei > 0 && events[ei - 1].x == x {
            continue;
        }
        let v = fs[i];
        if defined[i] {
            if cur.is_none() {
                cur = Some(open_at(x, true, &mut f));
            } else if let Some(p) = cur.as_mut()
                && v.is_finite()
                && p.samples.last().is_none_or(|s| s.0 < x)
            {
                p.samples.push((x, v));
            }
        } else {
            close(
                &mut cur,
                &mut pieces,
                Bound {
                    value: x,
                    closed: false,
                },
                &mut f,
            );
        }
    }
    if let Some(mut p) = cur.take() {
        p.hi = if hi_inf {
            Bound {
                value: f64::INFINITY,
                closed: false,
            }
        } else {
            Bound {
                value: hi,
                closed: true,
            }
        };
        pieces.push(p);
    }
    // Remove degenerate pieces.
    pieces.retain(|p| {
        p.hi.value > p.lo.value || (p.lo.closed && p.hi.closed && !p.samples.is_empty())
    });
    poles.sort_by(|a, b| a.total_cmp(b));
    poles.dedup_by(|a, b| (*a - *b).abs() <= 1e-12 * a.abs().max(1.0));
    excluded.sort_by(|a, b| a.total_cmp(b));
    excluded.dedup_by(|a, b| (*a - *b).abs() <= 1e-12 * a.abs().max(1.0));
    Scan {
        pieces,
        poles,
        excluded,
        too_complex,
    }
}

// ---------------------------------------------------------------------------
// Features on continuity pieces.

#[derive(Default)]
struct PieceFeatures {
    zeros: Vec<f64>,
    /// f vanishes on a whole stretch (e.g. floor(x) on [0, 1)).
    zero_interval: bool,
    /// Oscillation too fast for the samples: features are incomplete.
    erratic: bool,
    minima: Vec<(f64, f64)>,
    maxima: Vec<(f64, f64)>,
    inflections: Vec<(f64, f64)>,
    /// Critical points (x, kind) in order, for monotonicity.
    monotone: Vec<(Interval, Monotonicity)>,
    range: Option<Interval>,
}

fn sign_of(v: f64, eps: f64) -> i8 {
    if v > eps {
        1
    } else if v < -eps {
        -1
    } else {
        0
    }
}

/// Roots of `g` (sampled as `vals` at `xs`) where its sign changes,
/// including through exact zeros. Returns (x, sign_before, sign_after).
/// Above this many sign changes in one piece the samples are treated as
/// an unresolved oscillation: only the well-resolved centre is refined.
const MAX_SIGN_CHANGES: usize = 2_000;

/// Roots of `g` (sampled as `vals` at `xs`) where its sign changes,
/// including through exact zeros. Returns (x, sign_before, sign_after) and
/// whether the samples were too erratic to resolve everywhere.
fn sign_changes(
    xs: &[f64],
    vals: &[f64],
    g: &mut dyn FnMut(f64) -> f64,
    eps: f64,
) -> (Vec<(f64, i8, i8)>, bool) {
    // Brackets first (cheap), refinement second.
    let mut brackets: Vec<(usize, usize, i8, i8)> = Vec::new();
    let mut last: Option<(usize, i8)> = None;
    for (i, &v) in vals.iter().enumerate().take(xs.len()) {
        let s = if v.is_finite() { sign_of(v, eps) } else { 0 };
        if s == 0 {
            continue;
        }
        if let Some((j, ls)) = last
            && ls != s
        {
            brackets.push((j, i, ls, s));
        }
        last = Some((i, s));
    }
    let erratic = brackets.len() > MAX_SIGN_CHANGES;
    if erratic {
        brackets.retain(|&(j, _, _, _)| xs[j].abs() < 50.0);
    }
    let out = brackets
        .into_iter()
        .map(|(j, i, ls, s)| {
            let x = if i == j + 1 {
                brent(g, xs[j], vals[j], xs[i], vals[i])
            } else {
                // Exact zeros in between: the middle one of the run.
                xs[(j + 1 + i - 1) / 2]
            };
            (x, ls, s)
        })
        .collect();
    (out, erratic)
}

fn piece_features(fun: &Fun, p: &Piece, scale: f64, wrap_periodic: bool) -> PieceFeatures {
    let mut out = PieceFeatures::default();
    if p.samples.is_empty() {
        return out;
    }
    let xs: Vec<f64> = p.samples.iter().map(|s| s.0).collect();
    let fs: Vec<f64> = p.samples.iter().map(|s| s.1).collect();
    let mut f = |x: f64| fun.f(x);

    // Zeros. A run of several exact-zero samples is a stretch where f
    // vanishes identically (or underflows): not a list of intercepts.
    // A run is genuine (not underflow) when a sample next to it is of
    // normal magnitude, e.g. floor(x) = 0 on [0, 1) next to ±1.
    let normal = |v: f64| v.is_finite() && v.abs() > 1e-250;
    let mut inner_zero_run = false;
    let mut i = 0;
    while i < fs.len() {
        if fs[i] == 0.0 {
            let mut j = i;
            while j + 1 < fs.len() && fs[j + 1] == 0.0 {
                j += 1;
            }
            if j > i && ((i > 0 && normal(fs[i - 1])) || (j + 1 < fs.len() && normal(fs[j + 1]))) {
                inner_zero_run = true;
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    if inner_zero_run {
        out.zero_interval = true;
    }
    let (zero_crossings, erratic0) = sign_changes(&xs, &fs, &mut f, 0.0);
    out.erratic |= erratic0;
    for (x, _, _) in zero_crossings {
        // A crossing reported at a zero sample that is part of a zero run
        // is the middle of a vanishing stretch, not an intercept.
        let through_run = inner_zero_run && {
            let i = xs.partition_point(|&sx| sx < x);
            i < xs.len()
                && xs[i] == x
                && fs[i] == 0.0
                && ((i > 0 && fs[i - 1] == 0.0) || (i + 1 < fs.len() && fs[i + 1] == 0.0))
        };
        if !through_run {
            out.zeros.push(x);
        }
    }
    for (i, &v) in fs.iter().enumerate() {
        // Isolated exact zeros only: runs of zeros are underflow (e^x for
        // x < −745) or stretches where f vanishes identically.
        let prev_zero = i > 0 && fs[i - 1] == 0.0;
        let next_zero = i + 1 < fs.len() && fs[i + 1] == 0.0;
        // Underflowed values (e^(1/x) just left of 0) are not intercepts.
        let normal_neighbour = (i > 0 && fs[i - 1].abs() > 1e-250)
            || (i + 1 < fs.len() && fs[i + 1].abs() > 1e-250)
            || fs.len() == 1;
        if v == 0.0 && !prev_zero && !next_zero && normal_neighbour {
            out.zeros.push(xs[i]);
        }
    }

    // Critical points.
    let interior: Vec<usize> = (0..xs.len())
        .filter(|&i| xs[i] > p.lo.value && xs[i] < p.hi.value)
        .collect();
    let ixs: Vec<f64> = interior.iter().map(|&i| xs[i]).collect();
    let mut crit: Vec<(f64, i8, i8)> = Vec::new();
    if !ixs.is_empty() {
        let dfs = fun.batch(1, &ixs);
        let dscale = {
            let mut v: Vec<f64> = dfs
                .iter()
                .map(|v| v.abs())
                .filter(|v| v.is_finite())
                .collect();
            v.sort_by(|a, b| a.total_cmp(b));
            if v.is_empty() {
                1.0
            } else {
                v[v.len() / 2].max(1e-300)
            }
        };
        let eps = 1e-12 * dscale.max(scale * 1e-6);
        let mut df = |x: f64| fun.df(x);
        let (c, erratic1) = sign_changes(&ixs, &dfs, &mut df, eps);
        crit = c;
        out.erratic |= erratic1;
        for &(c, before, after) in &crit {
            let v = fun.f(c);
            if !v.is_finite() {
                continue;
            }
            if before > 0 && after < 0 {
                out.maxima.push((c, v));
            } else {
                out.minima.push((c, v));
            }
            if v.abs() <= 1e-9 * scale.max(1e-300)
                && !out
                    .zeros
                    .iter()
                    .any(|z| (z - c).abs() <= 1e-7 * c.abs().max(1.0))
            {
                out.zeros.push(c);
            }
        }
        // Inflection points.
        let d2s = fun.batch(2, &ixs);
        let d2scale = {
            let mut v: Vec<f64> = d2s
                .iter()
                .map(|v| v.abs())
                .filter(|v| v.is_finite())
                .collect();
            v.sort_by(|a, b| a.total_cmp(b));
            if v.is_empty() {
                1.0
            } else {
                v[v.len() / 2].max(1e-300)
            }
        };
        let _ = d2scale;
        // Rounding noise in f″ (e.g. for a function that is linear in
        // disguise) must not count as concavity changes: zero out values
        // below a per-sample noise floor relative to f′ and f.
        let ifs: Vec<f64> = interior.iter().map(|&i| fs[i]).collect();
        let d2c: Vec<f64> = d2s
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let x1 = 1.0 + ixs[i].abs();
                let floor = 1e-9 * (dfs[i].abs() / x1 + ifs[i].abs() / (x1 * x1))
                    + 1e-14 * scale / (x1 * x1);
                if v.abs() <= floor { 0.0 } else { v }
            })
            .collect();
        let mut d2 = |x: f64| fun.d2f(x);
        let (infl_c, erratic2) = sign_changes(&ixs, &d2c, &mut d2, 0.0);
        out.erratic |= erratic2;
        for (c, _, _) in infl_c {
            let v = fun.f(c);
            if !v.is_finite() {
                continue;
            }
            // Not at a corner of f (f′ continuous or a vertical tangent).
            let h = 1e-7 * c.abs().max(1.0);
            let (l, r) = (fun.df(c - h), fun.df(c + h));
            let corner = l.is_finite()
                && r.is_finite()
                && (l - r).abs() > 1e-3 * (1.0 + l.abs().max(r.abs()))
                && l.abs().max(r.abs()) < 1e6;
            if !corner && concavity_changes(fun, c, p, scale) {
                out.inflections.push((c, v));
            }
        }
    }

    // Monotonicity: split at critical points; directions from the f′
    // samples inside each sub-interval (found by binary search).
    let dvals: Vec<f64> = if ixs.is_empty() {
        Vec::new()
    } else {
        fun.batch(1, &ixs)
    };
    let ivals: Vec<f64> = interior.iter().map(|&i| fs[i]).collect();
    let mut cuts: Vec<f64> = vec![p.lo.value];
    cuts.extend(crit.iter().map(|c| c.0));
    cuts.push(p.hi.value);
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b <= a {
            continue;
        }
        let i0 = ixs.partition_point(|&x| x <= a);
        let i1 = ixs.partition_point(|&x| x < b);
        let (ds, vs): (Vec<f64>, Vec<f64>) = if i1 > i0 {
            (dvals[i0..i1].to_vec(), ivals[i0..i1].to_vec())
        } else {
            let m = if a.is_finite() && b.is_finite() {
                0.5 * (a + b)
            } else if a.is_finite() {
                a + 1.0
            } else if b.is_finite() {
                b - 1.0
            } else {
                0.0
            };
            (vec![fun.df(m)], vec![fun.f(m)])
        };
        let dmax = ds
            .iter()
            .filter(|v| v.is_finite())
            .fold(0.0f64, |m, v| m.max(v.abs()));
        let fin: Vec<f64> = vs.iter().copied().filter(|v| v.is_finite()).collect();
        let fspan = fin.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - fin.iter().copied().fold(f64::INFINITY, f64::min);
        let pos = ds.iter().filter(|&&v| v > 0.0).count();
        let neg = ds.iter().filter(|&&v| v < 0.0).count();
        let dir = if dmax <= 1e-12 * scale.max(1e-300) || (fspan <= 1e-12 * scale && ds.len() > 2) {
            Monotonicity::Constant
        } else if neg == 0 && pos > 0 {
            Monotonicity::Increasing
        } else if pos == 0 && neg > 0 {
            Monotonicity::Decreasing
        } else if pos > 20 * neg {
            Monotonicity::Increasing
        } else if neg > 20 * pos {
            Monotonicity::Decreasing
        } else {
            Monotonicity::Unknown
        };
        // Same direction on both sides of a "critical point" (rounding
        // noise, or a stationary inflection): one interval.
        if let Some(last) = out.monotone.last_mut()
            && last.1 == dir
        {
            last.0.hi = Bound {
                value: b,
                closed: false,
            };
            continue;
        }
        out.monotone.push((Interval::open(a, b), dir));
    }

    // Range of the piece.
    let mut acc = RangeAcc::new();
    for &(_, v) in out.minima.iter().chain(out.maxima.iter()) {
        acc.consider(v, true);
    }
    for (end, side) in [(p.lo, 1.0), (p.hi, -1.0)] {
        if end.value.is_infinite() {
            match limit_at_infinity(&mut f, end.value.signum()) {
                SeqLimit::Converges(l) => acc.consider(l, false),
                SeqLimit::PosInf => acc.consider(f64::INFINITY, false),
                SeqLimit::NegInf => acc.consider(f64::NEG_INFINITY, false),
                SeqLimit::Unknown => {
                    // Oscillating tails: check whether the extremes keep growing.
                    let sgn = end.value.signum();
                    let far: Vec<f64> = (0..400)
                        .map(|i| f(sgn * (1e3 + i as f64 * 2.4937e3)))
                        .collect();
                    let near: Vec<f64> = (0..400)
                        .map(|i| f(sgn * (1e2 + i as f64 * 2.4937e2)))
                        .collect();
                    let mx = |v: &[f64]| {
                        v.iter()
                            .copied()
                            .filter(|x| !x.is_nan())
                            .fold(f64::NEG_INFINITY, f64::max)
                    };
                    let mn = |v: &[f64]| {
                        v.iter()
                            .copied()
                            .filter(|x| !x.is_nan())
                            .fold(f64::INFINITY, f64::min)
                    };
                    if mx(&far) == f64::INFINITY || mx(&far) > 2.0 * mx(&near).abs().max(scale) {
                        acc.consider(f64::INFINITY, false);
                    }
                    if mn(&far) == f64::NEG_INFINITY || mn(&far) < -2.0 * mn(&near).abs().max(scale)
                    {
                        acc.consider(f64::NEG_INFINITY, false);
                    }
                }
            }
        } else if end.closed || wrap_periodic {
            let v = fun.f(end.value);
            if v.is_finite() {
                acc.consider(v, true);
            } else {
                acc.consider(one_sided_limit(&mut f, end.value, side), false);
            }
        } else {
            acc.consider(one_sided_limit(&mut f, end.value, side), false);
        }
    }
    // Samples catch anything the candidates missed (strictly beyond only).
    let tol = 1e-9 * scale;
    for &(x, v) in &p.samples {
        if v.is_finite() && (v < acc.lo.value - tol || v > acc.hi.value + tol) && x.abs() < 1e4 {
            acc.consider(v, true);
        }
    }
    // A bound that is only a limit can still be attained on a constant
    // stretch (max(x, 0) = 0 for x ≤ 0). Only moderate x count: far out,
    // rounding makes 1 − e^(−x) equal its limit 1.
    for &(x, v) in &p.samples {
        // Constant stretches have f′ exactly 0 (tanh(20) rounds to 1 but
        // its derivative does not vanish).
        if x.abs() > 20.0 || fun.df(x) != 0.0 {
            continue;
        }
        let genuine = v != 0.0 || out.zero_interval;
        if genuine && v == acc.lo.value {
            acc.lo.closed = true;
        }
        if genuine && v == acc.hi.value {
            acc.hi.closed = true;
        }
    }
    if acc.lo.value <= acc.hi.value {
        out.range = Some(Interval {
            lo: acc.lo,
            hi: acc.hi,
        });
    }
    out
}

/// Confirms an inflection candidate with second differences of f itself
/// (rejects sign changes of f″ that are only rounding noise).
fn concavity_changes(fun: &Fun, c: f64, p: &Piece, scale: f64) -> bool {
    let room = (c - p.lo.value).min(p.hi.value - c);
    let mut h = 1e-2 * c.abs().max(1.0);
    if room.is_finite() {
        h = h.min(room / 3.0);
    }
    if h.is_nan() || h <= 0.0 {
        return false;
    }
    let f = |x: f64| fun.f(x);
    let f0 = f(c);
    let left = f0 - 2.0 * f(c - h) + f(c - 2.0 * h);
    let right = f(c + 2.0 * h) - 2.0 * f(c + h) + f0;
    let noise = 1e-11 * (f0.abs() + scale) + 1e-13 * (f(c - 2.0 * h).abs() + f(c + 2.0 * h).abs());
    left.is_finite()
        && right.is_finite()
        && (left < 0.0) != (right < 0.0)
        && left.abs() > noise
        && right.abs() > noise
}

struct RangeAcc {
    lo: Bound,
    hi: Bound,
}

impl RangeAcc {
    fn new() -> RangeAcc {
        RangeAcc {
            lo: Bound {
                value: f64::INFINITY,
                closed: false,
            },
            hi: Bound {
                value: f64::NEG_INFINITY,
                closed: false,
            },
        }
    }

    fn consider(&mut self, v: f64, attained: bool) {
        if v.is_nan() {
            return;
        }
        if v < self.lo.value || (v == self.lo.value && attained) {
            self.lo = Bound {
                value: v,
                closed: attained && v.is_finite(),
            };
        }
        if v > self.hi.value || (v == self.hi.value && attained) {
            self.hi = Bound {
                value: v,
                closed: attained && v.is_finite(),
            };
        }
    }
}

/// Union of intervals (sorted, merging overlaps and touching ends).
fn union(mut parts: Vec<Interval>) -> Vec<Interval> {
    parts.retain(|p| p.lo.value <= p.hi.value);
    parts.sort_by(|a, b| {
        a.lo.value
            .total_cmp(&b.lo.value)
            .then((!a.lo.closed).cmp(&(!b.lo.closed)))
    });
    let mut out: Vec<Interval> = Vec::new();
    for p in parts {
        if let Some(last) = out.last_mut() {
            let touch = p.lo.value < last.hi.value
                || (p.lo.value == last.hi.value && (p.lo.closed || last.hi.closed));
            if touch {
                if p.hi.value > last.hi.value || (p.hi.value == last.hi.value && p.hi.closed) {
                    last.hi = p.hi;
                }
                if p.lo.value == last.lo.value && p.lo.closed {
                    last.lo.closed = true;
                }
                continue;
            }
        }
        out.push(p);
    }
    out
}

/// Snaps interval ends to recognised closed forms.
fn snap_interval(i: Interval, tol: f64) -> Interval {
    let s = |b: Bound| Bound {
        value: if b.value.is_finite() {
            snap(b.value, tol)
        } else {
            b.value
        },
        closed: b.closed,
    };
    Interval {
        lo: s(i.lo),
        hi: s(i.hi),
    }
}

fn dedup_sorted(v: &mut Vec<f64>) {
    v.sort_by(|a, b| a.total_cmp(b));
    v.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * a.abs().max(1.0));
}

fn dedup_points(v: &mut Vec<(f64, f64)>) {
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v.dedup_by(|a, b| (a.0 - b.0).abs() <= 1e-9 * a.0.abs().max(1.0));
}

// ---------------------------------------------------------------------------
// Asymptotes at ±∞.

type Horizontal = Vec<(f64, AsymptoteSide)>;
type Oblique = Vec<(f64, f64, AsymptoteSide)>;

fn asymptotes_at_infinity(fun: &Fun, to_neg: bool, to_pos: bool) -> (Horizontal, Oblique) {
    let mut horiz: Vec<(f64, AsymptoteSide)> = Vec::new();
    let mut obl: Vec<(f64, f64, AsymptoteSide)> = Vec::new();
    let mut f = |x: f64| fun.f(x);
    for (enabled, sign, side) in [
        (to_pos, 1.0, AsymptoteSide::PositiveInfinity),
        (to_neg, -1.0, AsymptoteSide::NegativeInfinity),
    ] {
        if !enabled {
            continue;
        }
        // A line the function equals on its tail (|x| for y = x) is not an
        // asymptote. Compare to rounding level only: e^(−x²) is tiny, not 0.
        let coincides = |f: &mut dyn FnMut(f64) -> f64, m: f64, b: f64| {
            [10.0, 100.0, 1000.0].iter().all(|&t| {
                let x = sign * t;
                let v = f(x);
                let line = m * x + b;
                (v - line).abs() <= 64.0 * f64::EPSILON * v.abs().max((m * x).abs()).max(b.abs())
            })
        };
        match limit_at_infinity(&mut f, sign) {
            SeqLimit::Converges(l) => {
                let l = snap(l, 1e-8);
                if !coincides(&mut f, 0.0, l) {
                    horiz.push((l, side));
                }
            }
            SeqLimit::PosInf | SeqLimit::NegInf => {
                let xs: Vec<f64> = (1..=15).map(|k| sign * 10f64.powi(k)).collect();
                let ms: Vec<f64> = xs.iter().map(|&x| f(x) / x).collect();
                if let SeqLimit::Converges(m) = sequence_limit(&ms) {
                    let m = snap(m, 1e-8);
                    if m.abs() > 1e-9 {
                        let bs: Vec<f64> = xs[..10].iter().map(|&x| f(x) - m * x).collect();
                        if let SeqLimit::Converges(b) = sequence_limit(&bs) {
                            let b = snap(b, 1e-7);
                            if !coincides(&mut f, m, b) {
                                obl.push((m, b, side));
                            }
                        }
                    }
                }
            }
            SeqLimit::Unknown => {}
        }
    }
    // Merge identical asymptotes on both sides.
    if horiz.len() == 2 && (horiz[0].0 - horiz[1].0).abs() <= 1e-9 * horiz[0].0.abs().max(1.0) {
        horiz = vec![(horiz[0].0, AsymptoteSide::AnyInfinity)];
    }
    if obl.len() == 2
        && (obl[0].0 - obl[1].0).abs() <= 1e-9 * obl[0].0.abs().max(1.0)
        && (obl[0].1 - obl[1].1).abs() <= 1e-7 * obl[0].1.abs().max(1.0)
    {
        obl = vec![(obl[0].0, obl[0].1, AsymptoteSide::AnyInfinity)];
    }
    (horiz, obl)
}

/// `m·x` text: `x`, `−x`, `2x`, `x/2`, `3x/4`, `πx`, `1.5x`.
fn coef_x(m: f64) -> String {
    match Nice::of(m) {
        Nice::Rational(p, q) => {
            let sign = if p < 0 { MINUS } else { "" };
            let num = if p.abs() == 1 {
                "x".to_string()
            } else {
                format!("{}x", p.abs())
            };
            if q == 1 {
                format!("{sign}{num}")
            } else {
                format!("{sign}{num}/{q}")
            }
        }
        n => format!("{n}x"),
    }
}

fn format_line(m: f64, b: f64) -> String {
    let mt = coef_x(m);
    if Nice::with_tol(b, 1e-7).value() == 0.0 {
        format!("y = {mt}")
    } else if b < 0.0 {
        format!("y = {mt} {MINUS} {}", format_number_tol(-b, 1e-7))
    } else {
        format!("y = {mt} + {}", format_number_tol(b, 1e-7))
    }
}

// ---------------------------------------------------------------------------
// Non-periodic analysis.

fn analyze_aperiodic(
    fun: &Fun,
    gens: &[Program],
    scale: f64,
) -> Result<KeyGraphFeatures, AnalysisError> {
    let xs = sinh_grid(WINDOW, N_HALF);
    let sc = scan(fun, &xs, &[], gens, true, true, scale);
    if sc.pieces.is_empty() {
        return Err(AnalysisError::AnalysisCouldNotBePerformed);
    }
    let mut k = KeyGraphFeatures::default();
    let mut too = 0u32;
    if sc.too_complex {
        too |= flags::RANGE | flags::MONOTONE_INTERVALS;
    }

    // Domain: pieces joined across jumps.
    let mut domain: Vec<Interval> = Vec::new();
    let mut i = 0;
    while i < sc.pieces.len() {
        let mut iv = Interval {
            lo: sc.pieces[i].lo,
            hi: sc.pieces[i].hi,
        };
        while sc.pieces[i].jump_after && i + 1 < sc.pieces.len() {
            i += 1;
            iv.hi = sc.pieces[i].hi;
        }
        domain.push(snap_interval(iv, 1e-9));
        i += 1;
    }
    let domain = union(domain);
    if domain.len() > MAX_LISTED {
        too |= flags::DOMAIN;
    } else {
        k.domain = format_set("x", &domain);
    }

    // Per-piece features.
    let mut zeros = Vec::new();
    let mut minima = Vec::new();
    let mut maxima = Vec::new();
    let mut infl = Vec::new();
    let mut mono: Vec<(Interval, Monotonicity)> = Vec::new();
    let mut ranges = Vec::new();
    for (pi, p) in sc.pieces.iter().enumerate() {
        let pf = piece_features(fun, p, scale, false);
        if pf.zero_interval {
            too |= flags::ZEROS;
        }
        if pf.erratic {
            too |= flags::ZEROS
                | flags::MINIMA
                | flags::MAXIMA
                | flags::INFLECTION_POINTS
                | flags::MONOTONE_INTERVALS;
        }
        zeros.extend(pf.zeros);
        minima.extend(pf.minima);
        maxima.extend(pf.maxima);
        infl.extend(pf.inflections);
        if let Some(r) = pf.range {
            ranges.push(r);
        }
        // Merge monotone runs across points where f stays continuous.
        for (j, (iv, dir)) in pf.monotone.into_iter().enumerate() {
            let continuous_join = j == 0 && pi > 0 && {
                let prev = &sc.pieces[pi - 1];
                !prev.jump_after && prev.hi.value == p.lo.value && (prev.hi.closed || p.lo.closed)
            };
            if continuous_join
                && let Some(last) = mono.last_mut()
                && last.1 == dir
            {
                last.0.hi = iv.hi;
                continue;
            }
            mono.push((iv, dir));
        }
    }
    dedup_sorted(&mut zeros);
    dedup_points(&mut minima);
    dedup_points(&mut maxima);
    dedup_points(&mut infl);
    let zeros: Vec<f64> = zeros.into_iter().map(|z| snap(z, 1e-9)).collect();

    if zeros.len() > MAX_LISTED || too & flags::ZEROS != 0 {
        too |= flags::ZEROS;
    } else {
        k.x_intercept = zeros
            .iter()
            .map(|&z| format_number(z))
            .collect::<Vec<_>>()
            .join(", ");
    }
    if minima.len() > MAX_LISTED {
        too |= flags::MINIMA;
    } else {
        k.minima = minima.iter().map(|&(x, y)| format_point(x, y)).collect();
    }
    if maxima.len() > MAX_LISTED {
        too |= flags::MAXIMA;
    } else {
        k.maxima = maxima.iter().map(|&(x, y)| format_point(x, y)).collect();
    }
    if infl.len() > MAX_LISTED {
        too |= flags::INFLECTION_POINTS;
    } else {
        k.inflection_points = infl.iter().map(|&(x, y)| format_point(x, y)).collect();
    }
    if sc.poles.len() > MAX_LISTED {
        too |= flags::VERTICAL_ASYMPTOTES;
    } else {
        k.vertical_asymptotes = sc
            .poles
            .iter()
            .map(|&p| format!("x = {}", format_number(p)))
            .collect();
    }
    if mono.len() > MAX_LISTED || mono.iter().any(|m| m.1 == Monotonicity::Unknown) {
        too |= flags::MONOTONE_INTERVALS;
    } else {
        k.monotonicity = mono
            .iter()
            .map(|(iv, d)| (snap_interval(*iv, 1e-9).format(), *d))
            .collect();
    }

    let range = union(ranges.into_iter().map(|r| snap_interval(r, 1e-8)).collect());
    if range.len() > 8 || too & flags::RANGE != 0 {
        too |= flags::RANGE;
    } else {
        k.range = format_set("y", &range);
    }

    // y-intercept.
    let y0 = fun.f(0.0);
    let zero_excluded = sc.excluded.contains(&0.0) || sc.poles.contains(&0.0);
    let y_intercept = if y0.is_finite() && !zero_excluded {
        Some(snap(y0, 1e-9))
    } else {
        None
    };
    if let Some(y) = y_intercept {
        k.y_intercept = format_number(y);
    }

    // Asymptotes at infinity.
    let to_neg = domain
        .first()
        .is_some_and(|d| d.lo.value == f64::NEG_INFINITY);
    let to_pos = domain.last().is_some_and(|d| d.hi.value == f64::INFINITY);
    let (horiz, obl) = asymptotes_at_infinity(fun, to_neg, to_pos);
    k.horizontal_asymptotes = horiz
        .iter()
        .map(|(v, _)| format!("y = {}", format_number_tol(*v, 1e-8)))
        .collect();
    k.oblique_asymptotes = obl.iter().map(|&(m, b, _)| format_line(m, b)).collect();

    k.too_complex_features = too;
    k.data = AnalysisData {
        period: None,
        domain,
        excluded: sc.excluded.iter().map(|&x| Family::single(x)).collect(),
        range,
        zeros: zeros.iter().map(|&z| Family::single(z)).collect(),
        y_intercept,
        minima: minima
            .iter()
            .map(|&(x, y)| (Family::single(x), y))
            .collect(),
        maxima: maxima
            .iter()
            .map(|&(x, y)| (Family::single(x), y))
            .collect(),
        inflection_points: infl.iter().map(|&(x, y)| (Family::single(x), y)).collect(),
        vertical_asymptotes: sc.poles.iter().map(|&x| Family::single(x)).collect(),
        horizontal_asymptotes: horiz,
        oblique_asymptotes: obl,
        monotonicity: mono,
    };
    Ok(k)
}

// ---------------------------------------------------------------------------
// Periodic analysis: one period, reported as families x + k·P.

/// Representative of `x` modulo `p` in (−p/2, p/2].
fn normalize(x: f64, p: f64) -> f64 {
    let r = x - (x / p - 0.5).ceil() * p;
    let r = snap(r, 1e-9);
    if r <= -p / 2.0 + 1e-12 * p { r + p } else { r }
}

/// Groups representatives (mod p) into families, merging evenly spaced
/// ones into a finer family (sin x zeros 0 and π → kπ).
fn families(mut reps: Vec<f64>, p: f64) -> Vec<Family> {
    let tol = 1e-7 * p;
    reps = reps.into_iter().map(|x| normalize(x, p)).collect();
    reps.sort_by(|a, b| a.total_cmp(b));
    reps.dedup_by(|a, b| (*a - *b).abs() <= tol);
    if reps.len() > 1 && (reps[0] + p - reps[reps.len() - 1]).abs() <= tol {
        reps.pop();
    }
    let n = reps.len();
    if n >= 2 {
        let step = p / n as f64;
        let even = (0..n).all(|i| {
            let gap = if i + 1 < n {
                reps[i + 1] - reps[i]
            } else {
                reps[0] + p - reps[n - 1]
            };
            (gap - step).abs() <= tol
        });
        if even {
            let rep = normalize(reps[0], step);
            return vec![Family {
                x: rep,
                period: Some(snap(step, 1e-9)),
            }];
        }
    }
    reps.into_iter()
        .map(|x| Family { x, period: Some(p) })
        .collect()
}

fn fmt_family(f: &Family) -> String {
    match f.period {
        Some(p) => format_family(f.x, p),
        None => format_number(f.x),
    }
}

fn analyze_periodic(
    fun: &Fun,
    gens: &[Program],
    p: f64,
    scale: f64,
) -> Result<KeyGraphFeatures, AnalysisError> {
    // Pass A: point features over a slightly padded period (so features at
    // the period edges are interior), reported modulo p.
    let pad = p / 8.0;
    let wa = uniform_grid(
        -p / 2.0 - pad,
        p / 2.0 + pad,
        PERIOD_STEPS + PERIOD_STEPS / 4,
    );
    let sa = scan(fun, &wa, &[], gens, false, false, scale);
    let in_core = |x: f64| x > -p / 2.0 - 1e-9 * p && x <= p / 2.0 + 1e-9 * p;
    let mut zeros = Vec::new();
    let mut minima = Vec::new();
    let mut maxima = Vec::new();
    let mut infl = Vec::new();
    let mut starts: Vec<f64> = Vec::new();
    for pc in &sa.pieces {
        // Only interior features of pieces count (window cuts are artificial).
        let pf = piece_features(fun, pc, scale, false);
        let inside = |x: f64| {
            x > pc.lo.value && x < pc.hi.value
                || (pc.lo.closed && x == pc.lo.value && pc.lo.value > wa[0])
                || (pc.hi.closed && x == pc.hi.value && pc.hi.value < wa[wa.len() - 1])
        };
        zeros.extend(pf.zeros.into_iter().filter(|&x| in_core(x) && inside(x)));
        minima.extend(pf.minima.into_iter().filter(|m| in_core(m.0)));
        maxima.extend(pf.maxima.into_iter().filter(|m| in_core(m.0)));
        infl.extend(pf.inflections.into_iter().filter(|m| in_core(m.0)));
        for b in [pc.lo, pc.hi] {
            if b.value > wa[0] && b.value < wa[wa.len() - 1] && in_core(b.value) {
                starts.push(b.value);
            }
        }
    }
    for &x in sa.excluded.iter().chain(sa.poles.iter()) {
        if in_core(x) {
            starts.push(x);
        }
    }
    let domain_events = !starts.is_empty();
    let w0 = if domain_events {
        starts.iter().copied().fold(f64::INFINITY, f64::min)
    } else {
        minima
            .iter()
            .chain(maxima.iter())
            .map(|m| m.0)
            .fold(f64::INFINITY, f64::min)
    };
    let w0 = if w0.is_finite() {
        snap(w0, 1e-9)
    } else {
        -p / 2.0
    };

    // Pass B: domain, range and monotonicity over [w0, w0 + p].
    let w1 = w0 + p;
    let xs = uniform_grid(w0, w1, PERIOD_STEPS);
    let extra = if domain_events { vec![w0, w1] } else { vec![] };
    let sc = scan(fun, &xs, &extra, gens, false, false, scale);
    if sc.pieces.is_empty() {
        return Err(AnalysisError::AnalysisCouldNotBePerformed);
    }
    let mut k = KeyGraphFeatures::default();
    let mut too = 0u32;
    if sc.too_complex || sa.too_complex {
        too |= flags::DOMAIN | flags::RANGE;
    }

    let mut domain: Vec<Interval> = Vec::new();
    let mut i = 0;
    while i < sc.pieces.len() {
        let mut iv = Interval {
            lo: sc.pieces[i].lo,
            hi: sc.pieces[i].hi,
        };
        while sc.pieces[i].jump_after && i + 1 < sc.pieces.len() {
            i += 1;
            iv.hi = sc.pieces[i].hi;
        }
        domain.push(snap_interval(iv, 1e-9));
        i += 1;
    }
    let domain = union(domain);
    let mut excl_pts: Vec<f64> = sa
        .excluded
        .iter()
        .copied()
        .filter(|&x| in_core(x))
        .collect();
    excl_pts.extend(sc.excluded.iter().copied());
    let excluded_fam = families(excl_pts, p);
    let covered: f64 = domain.iter().map(|d| d.hi.value - d.lo.value).sum();
    let full_cover = (covered - p).abs() <= 1e-9 * p;
    if full_cover && excluded_fam.is_empty() {
        k.domain = format_set("x", &[Interval::all()]);
    } else if full_cover {
        let reps: Vec<f64> = excluded_fam.iter().map(|f| f.x).collect();
        let per = excluded_fam.first().and_then(|f| f.period).unwrap_or(p);
        k.domain = format_periodic_set("x", &[], &reps, per);
    } else {
        k.domain = format_periodic_set("x", &domain, &[], p);
    }

    let mut mono: Vec<(Interval, Monotonicity)> = Vec::new();
    let mut ranges = Vec::new();
    for pc in &sc.pieces {
        let pf = piece_features(fun, pc, scale, !domain_events);
        if let Some(r) = pf.range {
            ranges.push(r);
        }
        for (iv, dir) in pf.monotone {
            if let Some(last) = mono.last_mut()
                && last.1 == dir
                && last.0.hi.value == iv.lo.value
                && !pc.lo.value.eq(&iv.lo.value)
            {
                last.0.hi = iv.hi;
                continue;
            }
            mono.push((iv, dir));
        }
    }

    let zero_f = families(zeros, p);
    let point_families = |pts: &[(f64, f64)]| -> Vec<(Family, f64)> {
        // Points with the same y share a family.
        let mut groups: Vec<(f64, Vec<f64>)> = Vec::new();
        for &(x, y) in pts {
            let y = snap(y, 1e-9);
            match groups
                .iter_mut()
                .find(|g| (g.0 - y).abs() <= 1e-9 * y.abs().max(scale))
            {
                Some(g) => g.1.push(x),
                None => groups.push((y, vec![x])),
            }
        }
        let mut out = Vec::new();
        for (y, xs) in groups {
            for fam in families(xs, p) {
                out.push((fam, y));
            }
        }
        out.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        out
    };
    let min_f = point_families(&minima);
    let max_f = point_families(&maxima);
    let infl_f = point_families(&infl);
    let mut pole_pts: Vec<f64> = sa.poles.iter().copied().filter(|&x| in_core(x)).collect();
    pole_pts.extend(sc.poles.iter().copied());
    let pole_f = families(pole_pts, p);

    let fmt_pts = |v: &[(Family, f64)]| -> Vec<String> {
        v.iter()
            .map(|(f, y)| format!("({}, {}), k ∈ ℤ", fmt_family(f), format_number(*y)))
            .collect()
    };
    if !zero_f.is_empty() {
        k.x_intercept = format!(
            "{}, k ∈ ℤ",
            zero_f.iter().map(fmt_family).collect::<Vec<_>>().join(", ")
        );
    }
    k.minima = fmt_pts(&min_f);
    k.maxima = fmt_pts(&max_f);
    k.inflection_points = fmt_pts(&infl_f);
    k.vertical_asymptotes = pole_f
        .iter()
        .map(|f| format!("x = {}, k ∈ ℤ", fmt_family(f)))
        .collect();
    if mono.iter().any(|m| m.1 == Monotonicity::Unknown) || mono.len() > MAX_LISTED {
        too |= flags::MONOTONE_INTERVALS;
    } else {
        k.monotonicity = mono
            .iter()
            .map(|(iv, d)| {
                (
                    format!("{}, k ∈ ℤ", snap_interval(*iv, 1e-9).format_periodic(p)),
                    *d,
                )
            })
            .collect();
    }
    let range = union(ranges.into_iter().map(|r| snap_interval(r, 1e-8)).collect());
    if range.len() > 8 || too & flags::RANGE != 0 {
        too |= flags::RANGE;
    } else {
        k.range = format_set("y", &range);
    }

    let y0 = fun.f(0.0);
    let zero_excluded = excluded_fam
        .iter()
        .chain(pole_f.iter())
        .any(|f| f.contains(0.0, 1e-9));
    let y_intercept = if y0.is_finite() && !zero_excluded {
        Some(snap(y0, 1e-9))
    } else {
        None
    };
    if let Some(y) = y_intercept {
        k.y_intercept = format_number(y);
    }
    k.too_complex_features = too;
    k.data = AnalysisData {
        period: Some(p),
        domain,
        excluded: excluded_fam,
        range,
        zeros: zero_f,
        y_intercept,
        minima: min_f,
        maxima: max_f,
        inflection_points: infl_f,
        vertical_asymptotes: pole_f,
        horizontal_asymptotes: Vec::new(),
        oblique_asymptotes: Vec::new(),
        monotonicity: mono,
    };
    Ok(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDINARY: [&str; 14] = [
        "x^3 - 2x + 1",
        "sin(x)",
        "tan(x)",
        "1/x",
        "e^x / (1 + x^2)",
        "ln(x^2 - 1)",
        "sqrt(4 - x^2)",
        "|x - 2| + floor(x)",
        "x sin(1/x)",
        "sec(x) + csc(2x)",
        "arctan(x) + arcsin(x/3)",
        "x^x",
        "(x^2 + 1)/(x - 3)",
        "sin(x) cos(3x) + sin(5x)/x",
    ];

    fn run(src: &str, budget: u64) -> Result<KeyGraphFeatures, Stop> {
        let e = crate::parser::parse_expression(src).unwrap();
        analyze_with_budget(&e, &CompileOptions::default(), None, budget)
    }

    #[test]
    fn ordinary_functions_use_a_small_part_of_the_budget() {
        for src in ORDINARY {
            assert!(
                !matches!(
                    run(src, WORK_BUDGET / 5),
                    Err(Stop::Error(AnalysisError::TooComplex))
                ),
                "{src}"
            );
        }
    }

    #[test]
    fn running_out_of_budget_is_reported_not_a_panic() {
        // Evaluations return NaN once the budget is spent; every stage must
        // cope wherever that happens.
        for src in ORDINARY {
            for budget in [1, 1_000, 100_000, 1_000_000] {
                match run(src, budget) {
                    Ok(_) | Err(Stop::Error(AnalysisError::TooComplex)) => {}
                    Err(Stop::Error(e)) => panic!("{src} at {budget}: {e:?}"),
                    Err(Stop::Cancelled) => panic!("{src}: cancelled"),
                }
            }
        }
    }

    #[test]
    fn periods() {
        let opts = CompileOptions::default();
        let e = crate::parser::parse_expression("sin(2x) + cos(3x)").unwrap();
        match per(&e, &opts) {
            Per::Periodic(p) => assert!((p - 2.0 * std::f64::consts::PI).abs() < 1e-12),
            other => panic!("{other:?}"),
        }
        let e = crate::parser::parse_expression("sin(x) + sin(πx)").unwrap();
        assert_eq!(per(&e, &opts), Per::Not);
        let e = crate::parser::parse_expression("x sin(x)").unwrap();
        assert_eq!(per(&e, &opts), Per::Not);
    }

    #[test]
    fn family_merging() {
        let pi = std::f64::consts::PI;
        let f = families(vec![0.0, pi], 2.0 * pi);
        assert_eq!(f.len(), 1);
        assert!((f[0].period.unwrap() - pi).abs() < 1e-12);
        let f = families(vec![pi / 2.0], 2.0 * pi);
        assert_eq!(f.len(), 1);
        assert!((f[0].x - pi / 2.0).abs() < 1e-12);
        let f = families(vec![3.0 * pi / 2.0], 2.0 * pi);
        assert!((f[0].x + pi / 2.0).abs() < 1e-12);
        // (−p/2, p/2]: −π/2 mod π is π/2.
        let f = families(vec![-pi / 2.0], pi);
        assert!((f[0].x - pi / 2.0).abs() < 1e-12);
    }
}
