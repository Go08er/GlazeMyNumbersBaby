//! The numeric analysis algorithm.
//!
//! Outline:
//! 1. Compile f, f′ and f″ (symbolic derivatives; numeric fallback).
//! 2. Detect periodicity from the expression (trig functions of linear
//!    arguments, `mod`) and verify/reduce it numerically (|sin x| → π).
//! 3. Scan a window — one period, or [−10⁶, 10⁶] on a grid that is dense
//!    near 0 — for domain boundaries, excluded points (zeros of
//!    denominators, `cos` of `tan`'s argument, …), poles and jumps, which
//!    split the window into continuity pieces. A non-periodic function is
//!    first probed out to ±10¹⁵ for zeros of f, f′, f″ and of those
//!    singularity generators; the window widens to take in what is found
//!    (with a dense grid around it too). What may lie beyond 10¹⁵ (by a
//!    polynomial's root bound, or a generator still heading for zero)
//!    is reported as unknown rather than extrapolated.
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
    Bound, Interval, MINUS, Nice, format_decimal, format_family, format_family_apart,
    format_nonzero, format_number, format_number_apart, format_periodic_set, format_set,
    format_set_with,
};
use super::numeric::{
    SeqLimit, bisect_defined, brent, diverges_near, limit_at_infinity_beyond_err, one_sided_limit,
    one_sided_limit_err, sample, sequence_limit_noisy, sinh_grid, uniform_grid,
};
use super::{
    AnalysisData, AnalysisError, AsymptoteSide, Family, KeyGraphFeatures, Monotonicity, Parity,
    Periodicity, flags,
};
use crate::ast::{BinOp, Expr, Func};
use crate::compile::{
    CompileOptions, Input, Program, Redo, constant_where_defined, syntactic_rational,
};
use crate::diff::derivative_bounded;
use crate::functions::TrigUnit;
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
    /// Whether f is built with abs, max, min, floor, … of x: only then can
    /// it equal a constant or a line on a stretch without being that
    /// everywhere (a tail that underflows onto a line is not that line).
    piecewise: bool,
    /// Expressions whose zeros are kinks of f, where f′ may jump.
    kinks: Vec<Program>,
    /// f's limits at −∞ and +∞ when its form shows them exactly: a
    /// constant plus terms that vanish there (x^−0.0001 + 10⁻⁹ tends to
    /// 10⁻⁹, a value no sampling of so slow a tail can pin down).
    tails: [Option<f64>; 2],
    /// Bases of powers whose exponent varies with x (x^x, (−2)^x): where
    /// one is negative, the power is undefined except where the exponent
    /// is an integer, isolated points no sampling finds.
    power_bases: Vec<Program>,
    /// Whether f was beyond even extended range somewhere within the search
    /// (the sine of e^1000): NaN there, but not known to be undefined.
    unresolved: Cell<bool>,
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

    /// Evaluates `p` at `x`, charged to the budget. Only f's own values are
    /// re-evaluated in extended range where a double can't hold them (see
    /// `Program::eval`); its derivatives' and helpers' are kept as doubles
    /// give them, as the rest of the analysis expects.
    #[inline]
    fn eval(&self, p: &Program, x: f64) -> f64 {
        if !self.spend(p.cost()) {
            return f64::NAN;
        }
        if !std::ptr::eq(p, &self.f) {
            return p.eval_plain(x, 0.0);
        }
        let (v, redo) = p.eval_tagged(x, 0.0);
        if redo != Redo::No {
            self.redone(p, x, redo);
        }
        v
    }

    /// Evaluates `p` on a grid, charged to the budget.
    fn eval_batch(&self, p: &Program, xs: &[f64], out: &mut [f64]) {
        if !self.spend(p.cost().saturating_mul(xs.len())) {
            out.fill(f64::NAN);
            return;
        }
        if !std::ptr::eq(p, &self.f) || !p.may_redo() {
            p.eval_batch_plain(Input::Slice(xs), Input::Scalar(0.0), out);
            return;
        }
        p.eval_batch_tagged(Input::Slice(xs), Input::Scalar(0.0), out, |i, redo| {
            self.redone(p, xs[i], redo)
        });
    }

    /// Charges a re-evaluation of f at `x` in extended range (several times
    /// the cost of the double one), and notes a value even that couldn't
    /// resolve within the search. Beyond it f is only probed for its trend,
    /// where such a NaN reads as before.
    #[cold]
    fn redone(&self, p: &Program, x: f64, redo: Redo) {
        self.spend(p.cost().saturating_mul(8));
        if redo == Redo::Unknown && x.abs() <= REACH {
            self.unresolved.set(true);
        }
    }

    #[inline]
    fn f(&self, x: f64) -> f64 {
        self.eval(&self.f, x)
    }

    /// Whether f is exactly 0 at `x`, not a value too small for a double.
    fn exact_zero(&self, x: f64) -> bool {
        self.spend(self.f.cost().saturating_mul(8)) && self.f.is_exact_zero(x, 0.0)
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
    let k = analyze_expr_ungated(expr, opts, cancel)?;
    // Nothing the function's own values contradict is shown.
    super::verify::gate(k, expr, opts, cancel)
}

/// [`analyze_expr`] without the gate.
pub(crate) fn analyze_expr_ungated(
    expr: &Expr,
    opts: &CompileOptions<'_>,
    cancel: Option<&AtomicBool>,
) -> Result<KeyGraphFeatures, Stop> {
    let k = analyze_framed(expr, opts, cancel)?;
    let k = match constant_where_defined(expr, opts) {
        Some(c) => constant_where(k, c),
        None => k,
    };
    Ok(trig_holes(k, expr, opts))
}

/// The poles of tan, sec, cot and csc of an affine argument a·x + b, as
/// families of x: a·x + b = q/2 + kq for tan and sec, kq for cot and csc,
/// q the half turn (π, 180 or 200). In radians no double is one of them,
/// so a product that cancels one (tan x · cos x) is finite at every double
/// beside it, and sampling sees nothing there. None if such a function's
/// argument varies with x but isn't affine.
fn trig_pole_families(e: &Expr, opts: &CompileOptions<'_>) -> Option<Vec<Family>> {
    fn walk(e: &Expr, opts: &CompileOptions<'_>, out: &mut Vec<Family>) -> bool {
        match e {
            Expr::Call(f, args) => {
                if !args.iter().all(|a| walk(a, opts, out)) {
                    return false;
                }
                let (Func::Tan | Func::Sec | Func::Cot | Func::Csc, [u]) = (f, args.as_slice())
                else {
                    return true;
                };
                if !u.contains_x() {
                    return true;
                }
                let num = |e: &Expr| Program::compile(e, opts).ok()?.as_constant();
                let Some((a, b)) = linear_in(u, &|n| matches!(n, Expr::X))
                    .and_then(|(c, r)| Some((num(&c)?, num(&r)?)))
                else {
                    return false;
                };
                if !(a != 0.0 && a.is_finite() && b.is_finite()) {
                    return false;
                }
                let q = match opts.trig_unit {
                    TrigUnit::Radians => std::f64::consts::PI,
                    TrigUnit::Degrees => 180.0,
                    TrigUnit::Grads => 200.0,
                };
                let off = if matches!(f, Func::Tan | Func::Sec) {
                    q / 2.0
                } else {
                    0.0
                };
                let period = q / a.abs();
                out.push(Family {
                    x: normalize((off - b) / a, period),
                    period: Some(period),
                });
                true
            }
            Expr::Bin(_, l, r) => walk(l, opts, out) && walk(r, opts, out),
            Expr::Neg(x) | Expr::Degrees(x) => walk(x, opts, out),
            _ => true,
        }
    }
    let mut out = Vec::new();
    walk(e, opts, &mut out).then_some(out)
}

/// Whether families `f` and `g` share a point (within rounding).
fn families_meet(f: &Family, g: &Family) -> bool {
    let near = |fam: &Family, t: f64| {
        let c = match fam.period {
            Some(p) if p > 0.0 => fam.x + ((t - fam.x) / p).round() * p,
            _ => fam.x,
        };
        (c - t).abs() <= 1e-9 * t.abs().max(1.0)
    };
    match g.period {
        Some(q) if q > 0.0 => (-64..=64).any(|j| near(f, g.x + j as f64 * q)),
        _ => near(f, g.x),
    }
}

/// Holes the samples can't show: where tan, sec, cot or csc of an affine
/// argument has a pole (no double in radians) and the function stays
/// bounded beside it (tan x · cos x at π/2 + kπ), excluded from the domain
/// as the family they are. What was found at those points (a turn of
/// sin x where tan x · cos x is undefined) is made unknown, and with it the
/// range those turns would attain. A pole family that can't be added this
/// way (no period to state it in, or f unbounded beside it, a pole the
/// scan missed) makes the domain unknown.
fn trig_holes(mut k: KeyGraphFeatures, expr: &Expr, opts: &CompileOptions<'_>) -> KeyGraphFeatures {
    if k.analysis_error != AnalysisError::NoError || k.too_complex_features & flags::DOMAIN != 0 {
        return k;
    }
    let Some(fams) = trig_pole_families(expr, opts) else {
        // (A non-affine argument's poles are the gate's to look for.)
        return k;
    };
    let Ok(f) = Program::compile(expr, opts) else {
        return k;
    };
    let mut bail = false;
    let mut added: Vec<Family> = Vec::new();
    for fam in &fams {
        let d = &k.data;
        let covered = d.excluded.iter().chain(&d.vertical_asymptotes).any(|g| {
            (-4..=4).all(|m| {
                let t = fam.x + m as f64 * fam.period.unwrap_or(0.0);
                families_meet(g, &Family::single(t))
            })
        });
        if covered {
            continue;
        }
        // Outside the claimed domain altogether (beside sqrt(sin x)'s
        // gaps): nothing claimed there.
        let in_domain = |t: f64| match d.period {
            None => d.domain.iter().any(|iv| iv.contains(t)),
            Some(p) => {
                let lo = d
                    .domain
                    .iter()
                    .map(|iv| iv.lo.value)
                    .fold(f64::INFINITY, f64::min);
                let lo = if lo.is_finite() { lo } else { 0.0 };
                let r = lo + (t - lo).rem_euclid(p);
                d.domain
                    .iter()
                    .any(|iv| iv.contains(r) || iv.contains(r - p) || iv.contains(r + p))
            }
        };
        if !(-4..=4).any(|m| in_domain(fam.x + m as f64 * fam.period.unwrap_or(0.0))) {
            continue;
        }
        // Bounded beside it: a hole. Growing: a pole the scan missed.
        let side = |h: f64| {
            f.eval(fam.x + h, 0.0)
                .abs()
                .max(f.eval(fam.x - h, 0.0).abs())
        };
        let (far, near) = (
            side(1e-3 * fam.x.abs().max(1.0)),
            side(1e-9 * fam.x.abs().max(1.0)),
        );
        let bounded = far.is_finite() && near.is_finite() && near <= 2.0 * far + 1.0;
        let fits = match (d.period, fam.period) {
            (Some(p), Some(q)) => {
                let n = (p / q).round();
                (1.0..=64.0).contains(&n) && (p - n * q).abs() <= 1e-9 * p
            }
            _ => false,
        };
        let full = match d.period {
            Some(p) => {
                let covered: f64 = d.domain.iter().map(|i| i.hi.value - i.lo.value).sum();
                (covered - p).abs() <= 1e-9 * p
            }
            None => false,
        };
        if bounded && fits && full {
            added.push(*fam);
        } else {
            bail = true;
        }
    }
    if bail {
        forget(&mut k, flags::DOMAIN | flags::VERTICAL_ASYMPTOTES);
        return k;
    }
    if added.is_empty() {
        return k;
    }
    let p = k.data.period.expect("added only with a period");
    let mut reps: Vec<f64> = Vec::new();
    for g in k.data.excluded.iter().chain(&added) {
        let q = g.period.unwrap_or(p);
        let n = (p / q).round().max(1.0) as usize;
        reps.extend((0..n).map(|j| g.x + j as f64 * q));
    }
    k.data.excluded = families(reps, p);
    let reps: Vec<f64> = k.data.excluded.iter().map(|f| f.x).collect();
    let per = k.data.excluded.first().and_then(|f| f.period).unwrap_or(p);
    k.domain = format_periodic_set("x", &[], &reps, per);
    // What sits on the new holes isn't there.
    let mut drop = 0;
    let d = &k.data;
    for (flag, list) in [
        (
            flags::MINIMA,
            d.minima.iter().map(|m| m.0).collect::<Vec<_>>(),
        ),
        (flags::MAXIMA, d.maxima.iter().map(|m| m.0).collect()),
        (
            flags::INFLECTION_POINTS,
            d.inflection_points.iter().map(|m| m.0).collect(),
        ),
        (flags::ZEROS, d.zeros.clone()),
    ] {
        if list
            .iter()
            .any(|g| added.iter().any(|h| families_meet(h, g)))
        {
            drop |= flag;
        }
    }
    // A bound attained at a turn that isn't there may not be attained.
    if drop & (flags::MINIMA | flags::MAXIMA) != 0 {
        drop |= flags::RANGE;
    }
    if matches!(k.parity, Parity::Odd | Parity::Even)
        && !added
            .iter()
            .all(|h| families_meet(h, &Family::single(-h.x)))
    {
        k.parity = Parity::Unknown;
        k.too_complex_features |= flags::PARITY;
    }
    if added.iter().any(|h| families_meet(h, &Family::single(0.0))) {
        k.data.y_intercept = None;
        k.y_intercept.clear();
    }
    if drop != 0 {
        forget(&mut k, drop);
    }
    k
}

fn analyze_framed(
    expr: &Expr,
    opts: &CompileOptions<'_>,
    cancel: Option<&AtomicBool>,
) -> Result<KeyGraphFeatures, Stop> {
    // f(x) = g(s·x + t), every x inside the same affine form up to scale:
    // analyse g around its own origin and map what it finds back, so that
    // (x − 10⁶)² + 1, sin(x − 1000)/(x − 1000) or (x/0.001)³ − 3(x/0.001)
    // get the same search, grid and tolerances as x² + 1, sin(x)/x, x³ − 3x.
    if let Some((g, a, b, lost)) = affine_argument(expr, opts) {
        let kg = analyze_with_budget(&g, opts, cancel, WORK_BUDGET)?;
        let fun = make_fun(expr, opts, cancel, WORK_BUDGET)?;
        let everywhere = kg.too_complex_features & flags::DOMAIN == 0
            && kg.domain == format_set("x", &[Interval::all()])
            && kg.data.vertical_asymptotes.is_empty();
        let mut k = map_affine(kg, &fun, a, b);
        if let Some(stop) = fun.stopped() {
            return Err(stop);
        }
        if lost {
            // sin(x + 10¹⁸): its period and values are sin's, but where its
            // zeros, turns and poles fall is b mod 2π, which rounding lost.
            // f(0) is still sin(10¹⁸), exactly reduced.
            let mut which = flags::ZEROS
                | flags::MINIMA
                | flags::MAXIMA
                | flags::INFLECTION_POINTS
                | flags::MONOTONE_INTERVALS
                | flags::PARITY;
            if everywhere {
                k.domain = format_set("x", &[Interval::all()]);
                k.data.domain = vec![Interval::all()];
            } else {
                which |= flags::DOMAIN | flags::VERTICAL_ASYMPTOTES;
            }
            k.parity = Parity::Unknown;
            let y0 = fun.f(0.0);
            k.data.y_intercept = y0.is_finite().then_some(y0);
            k.y_intercept = if y0.is_finite() {
                fmt_y(y0)
            } else {
                String::new()
            };
            if y0.is_infinite() {
                which |= flags::Y_INTERCEPT;
            }
            forget(&mut k, which);
        }
        return Ok(k);
    }
    analyze_with_budget(expr, opts, cancel, WORK_BUDGET)
}

/// `expr` as g(a·x + b) when every x in it sits in an affine form with one
/// common root −b/a, other than x itself: g (in x) and (a, b). The largest
/// affine forms are tried first, then the smallest: ((x − 10⁹) − 1)·
/// ((x − 10⁹) − 2) has forms with roots 10⁹ + 1 and 10⁹ + 2, but both are
/// built on x − 10⁹. Not when the root is beyond the search (its features
/// couldn't be told apart in floating point there), nor for a periodic g so
/// far out that b modulo the period is lost.
///
/// The last element says where g's features fall in x is lost: a periodic g
/// whose b is so far out that b modulo the period is rounding (or beyond the
/// search) keeps its period and values only.
fn affine_argument(expr: &Expr, opts: &CompileOptions<'_>) -> Option<(Expr, f64, f64, bool)> {
    let num = |e: &Expr| Program::compile(e, opts).ok()?.as_constant();
    let affine = |e: &Expr| -> Option<(f64, f64)> {
        let (c, r) = linear_in(e, &|n| matches!(n, Expr::X))?;
        let (a, b) = (num(&c)?, num(&r)?);
        (a != 0.0 && a.is_finite() && b.is_finite()).then_some((a, b))
    };
    if !expr.contains_x() || affine(expr).is_some() {
        return None;
    }
    // An affine form built on a smaller one that isn't x alone.
    fn has_inner(e: &Expr, affine: &dyn Fn(&Expr) -> Option<(f64, f64)>) -> bool {
        let inner = |c: &Expr| {
            c.contains_x() && (!matches!(c, Expr::X) && affine(c).is_some() || has_inner(c, affine))
        };
        match e {
            Expr::Neg(x) | Expr::Degrees(x) => inner(x),
            Expr::Bin(_, x, y) => inner(x) || inner(y),
            Expr::Call(_, args) => args.iter().any(inner),
            _ => false,
        }
    }
    fn rewrite(
        e: &Expr,
        affine: &dyn Fn(&Expr) -> Option<(f64, f64)>,
        base: &mut Option<(f64, f64)>,
        smallest: bool,
    ) -> Option<Expr> {
        if !e.contains_x() {
            return Some(e.clone());
        }
        if let Some((ai, bi)) = affine(e).filter(|_| !(smallest && has_inner(e, affine))) {
            let (a, b) = *base.get_or_insert((ai, bi));
            let (r, ri) = (-b / a, -bi / ai);
            if (r - ri).abs() > 1e-13 * r.abs().max(ri.abs()) {
                return None;
            }
            let k = ai / a;
            return Some(if k == 1.0 {
                Expr::X
            } else {
                Expr::bin(BinOp::Mul, Expr::Num(k), Expr::X)
            });
        }
        let mut go = |x: &Expr| rewrite(x, affine, base, smallest);
        Some(match e {
            Expr::Neg(x) => Expr::Neg(Box::new(go(x)?)),
            Expr::Degrees(x) => Expr::Degrees(Box::new(go(x)?)),
            Expr::Bin(op, x, y) => Expr::bin(*op, go(x)?, go(y)?),
            Expr::Call(f, args) => Expr::Call(*f, args.iter().map(go).collect::<Option<Vec<_>>>()?),
            _ => e.clone(),
        })
    }
    let (g, (a, b)) = [false, true].into_iter().find_map(|smallest| {
        let mut base = None;
        let g = rewrite(expr, &affine, &mut base, smallest)?;
        Some((g, base?))
    })?;
    let r = -b / a;
    if r == 0.0 && a.abs() == 1.0 {
        return None;
    }
    // A periodic g with b far beyond its period: b mod P is rounding.
    if let Per::Periodic(pg) = per(&g, opts)
        && (b.abs() > 1e9 * pg || r.abs() > REACH)
    {
        return Some((g, a, b, true));
    }
    if r.abs() > REACH {
        return None;
    }
    Some((g, a, b, false))
}

/// The analysis of g mapped to f(x) = g(a·x + b): positions x = (u − b)/a,
/// directions and sides flipped for a < 0, oblique asymptotes rewritten in
/// x; values, the range and horizontal asymptotes unchanged. The
/// y-intercept and parity are f's own.
fn map_affine(kg: KeyGraphFeatures, fun: &Fun, a: f64, b: f64) -> KeyGraphFeatures {
    let flip = a < 0.0;
    let mx = |u: f64| (u - b) / a;
    let d = &kg.data;
    let period = d.period.map(|p| p / a.abs());
    let fam = |f: &Family| match f.period {
        Some(pu) => {
            let pf = pu / a.abs();
            Family {
                x: normalize(mx(f.x), pf),
                period: Some(pf),
            }
        }
        None => Family::single(mx(f.x)),
    };
    let bound = |bd: Bound| Bound {
        value: mx(bd.value),
        closed: bd.closed,
    };
    let iv = |i: &Interval| {
        let (lo, hi) = (bound(i.lo), bound(i.hi));
        if flip {
            Interval { lo: hi, hi: lo }
        } else {
            Interval { lo, hi }
        }
    };
    // Intervals within one period, moved together next to 0.
    let ivs = |v: &[Interval]| -> Vec<Interval> {
        let mut out: Vec<Interval> = v.iter().map(iv).collect();
        if flip {
            out.reverse();
        }
        if let (Some(p), Some(first)) = (period, out.first())
            && first.lo.value.is_finite()
        {
            let shift = (first.lo.value / p).floor() * p;
            for i in &mut out {
                i.lo.value -= shift;
                i.hi.value -= shift;
            }
        }
        out
    };
    let points = |v: &[(Family, f64)]| -> Vec<(Family, f64)> {
        let mut out: Vec<(Family, f64)> = v.iter().map(|(f, y)| (fam(f), *y)).collect();
        out.sort_by(|p, q| p.0.x.total_cmp(&q.0.x));
        out
    };
    let fams = |v: &[Family]| -> Vec<Family> {
        let mut out: Vec<Family> = v.iter().map(fam).collect();
        out.sort_by(|p, q| p.x.total_cmp(&q.x));
        out
    };
    let side = |s: AsymptoteSide| match (flip, s) {
        (true, AsymptoteSide::PositiveInfinity) => AsymptoteSide::NegativeInfinity,
        (true, AsymptoteSide::NegativeInfinity) => AsymptoteSide::PositiveInfinity,
        (_, s) => s,
    };
    let mut mono: Vec<(Interval, Monotonicity)> = d
        .monotonicity
        .iter()
        .map(|(i, m)| {
            let m = match (flip, *m) {
                (true, Monotonicity::Increasing) => Monotonicity::Decreasing,
                (true, Monotonicity::Decreasing) => Monotonicity::Increasing,
                (_, m) => m,
            };
            (iv(i), m)
        })
        .collect();
    if flip {
        mono.reverse();
    }
    if let (Some(p), Some(first)) = (period, mono.first())
        && first.0.lo.value.is_finite()
    {
        let shift = (first.0.lo.value / p).floor() * p;
        for (i, _) in &mut mono {
            i.lo.value -= shift;
            i.hi.value -= shift;
        }
    }
    let data = AnalysisData {
        period,
        domain: ivs(&d.domain),
        excluded: fams(&d.excluded),
        range: d.range.clone(),
        zeros: fams(&d.zeros),
        y_intercept: None,
        minima: points(&d.minima),
        maxima: points(&d.maxima),
        inflection_points: points(&d.inflection_points),
        vertical_asymptotes: fams(&d.vertical_asymptotes),
        horizontal_asymptotes: d
            .horizontal_asymptotes
            .iter()
            .map(|&(v, s)| (v, side(s)))
            .collect(),
        oblique_asymptotes: d
            .oblique_asymptotes
            .iter()
            .map(|&(m, c, s)| (clean(m * a, 1e-15 * (m * a).abs()), m * b + c, side(s)))
            .collect(),
        monotonicity: mono,
    };
    let mut k = KeyGraphFeatures {
        range: kg.range.clone(),
        too_complex_features: kg.too_complex_features,
        data,
        ..Default::default()
    };
    // y-intercept: f(0), where f is defined.
    let in_domain = |x: f64| {
        let dd = &k.data;
        let inside = match period {
            Some(p) => {
                let w = dd.domain.first().map_or(0.0, |i| i.lo.value);
                let q = if w.is_finite() {
                    w + (x - w).rem_euclid(p)
                } else {
                    x
                };
                dd.domain.iter().any(|i| i.contains(q))
            }
            None => dd.domain.iter().any(|i| i.contains(x)),
        };
        inside
            && !dd
                .excluded
                .iter()
                .chain(&dd.vertical_asymptotes)
                .any(|f| f.contains(x, 1e-12))
    };
    // With the domain unknown (isolated points of (x − 1)^(x − 1)), a value
    // at 0 is still a point of the graph.
    let y0 = fun.f(0.0);
    let domain_known = k.too_complex_features & flags::DOMAIN == 0;
    if y0.is_finite()
        && (in_domain(0.0) || !domain_known)
        && k.too_complex_features & flags::Y_INTERCEPT == 0
    {
        k.data.y_intercept = Some(clean(y0, value_noise(fun, 0.0, y0)));
    }
    // f(0) exists but is beyond a double's range ((x − 1000)e^(1000 − x)).
    if y0.is_infinite() {
        k.too_complex_features |= flags::Y_INTERCEPT;
    }
    // Parity: f's own, tested also around the root of the form and at
    // what was found, where a shifted f lives (e^(x − 10⁶) is 0 near 0).
    let r = -b / a;
    let mut extra: Vec<f64> = [0.25, 0.5, 1.0, 2.0, 5.0]
        .iter()
        .flat_map(|&d| [r + d / a.abs(), r - d / a.abs()])
        .collect();
    extra.push(r);
    extra.extend(k.data.zeros.iter().map(|z| z.x));
    extra.extend(
        k.data
            .minima
            .iter()
            .chain(&k.data.maxima)
            .chain(&k.data.inflection_points)
            .map(|m| m.0.x),
    );
    k.parity = parity(fun, typical_scale(fun), 0.0, &extra);
    if kg.parity == Parity::Unknown && matches!(k.parity, Parity::Even | Parity::Odd) {
        k.parity = Parity::Unknown;
    }
    k.periodicity_direction = kg.periodicity_direction;
    if let Some(p) = period {
        k.periodicity_expression = format_number(p);
    }
    render(&mut k);
    k
}

/// The display strings of `k` from its data (as the analyses write them).
fn render(k: &mut KeyGraphFeatures) {
    let d = &k.data;
    let too = k.too_complex_features;
    let known = |f: u32| too & f == 0;
    match d.period {
        None => {
            if known(flags::DOMAIN) {
                k.domain = format_set("x", &d.domain);
            }
            if known(flags::ZEROS) {
                let near: Vec<f64> = (d.zeros.iter())
                    .chain(&d.excluded)
                    .chain(&d.vertical_asymptotes)
                    .map(|f| f.x)
                    .collect();
                k.x_intercept = d
                    .zeros
                    .iter()
                    .map(|z| format_number_apart(z.x, &near))
                    .collect::<Vec<_>>()
                    .join(", ");
            }
            let pts = |v: &[(Family, f64)]| -> Vec<String> {
                v.iter().map(|(f, y)| fmt_point(f.x, *y)).collect()
            };
            k.minima = pts(&d.minima);
            k.maxima = pts(&d.maxima);
            k.inflection_points = pts(&d.inflection_points);
            k.vertical_asymptotes = d
                .vertical_asymptotes
                .iter()
                .map(|f| format!("x = {}", format_number(f.x)))
                .collect();
            k.monotonicity = d
                .monotonicity
                .iter()
                .map(|(iv, m)| (snap_interval(*iv, 1e-9).format(), *m))
                .collect();
        }
        Some(p) => {
            if known(flags::DOMAIN) {
                let covered: f64 = d.domain.iter().map(|i| i.hi.value - i.lo.value).sum();
                let full_cover = (covered - p).abs() <= 1e-9 * p;
                k.domain = if full_cover && d.excluded.is_empty() {
                    format_set("x", &[Interval::all()])
                } else if full_cover {
                    let reps: Vec<f64> = d.excluded.iter().map(|f| f.x).collect();
                    let per = d.excluded.first().and_then(|f| f.period).unwrap_or(p);
                    format_periodic_set("x", &[], &reps, per)
                } else {
                    format_periodic_set("x", &d.domain, &[], p)
                };
            }
            if known(flags::ZEROS) && !d.zeros.is_empty() {
                let others: Vec<Family> = d
                    .excluded
                    .iter()
                    .chain(&d.vertical_asymptotes)
                    .copied()
                    .collect();
                k.x_intercept = format!("{}, k ∈ ℤ", fmt_zero_families(&d.zeros, &others));
            }
            let pts = |v: &[(Family, f64)]| -> Vec<String> {
                v.iter()
                    .map(|(f, y)| format!("({}, {}), k ∈ ℤ", fmt_family(f), fmt_y(*y)))
                    .collect()
            };
            k.minima = pts(&d.minima);
            k.maxima = pts(&d.maxima);
            k.inflection_points = pts(&d.inflection_points);
            k.vertical_asymptotes = d
                .vertical_asymptotes
                .iter()
                .map(|f| format!("x = {}, k ∈ ℤ", fmt_family(f)))
                .collect();
            k.monotonicity = d
                .monotonicity
                .iter()
                .map(|(iv, m)| {
                    (
                        format!("{}, k ∈ ℤ", snap_interval(*iv, 1e-9).format_periodic(p)),
                        *m,
                    )
                })
                .collect();
        }
    }
    if let Some(y) = d.y_intercept {
        k.y_intercept = fmt_y(y);
    }
    k.horizontal_asymptotes = d
        .horizontal_asymptotes
        .iter()
        .map(|(v, _)| format!("y = {}", fmt_y(*v)))
        .collect();
    k.oblique_asymptotes = d
        .oblique_asymptotes
        .iter()
        .map(|&(m, c, _)| format_line(m, c))
        .collect();
}

/// The function `expr` evaluates, with its derivatives, under a budget.
fn make_fun<'a>(
    expr: &Expr,
    opts: &CompileOptions<'_>,
    cancel: Option<&'a AtomicBool>,
    budget: u64,
) -> Result<Fun<'a>, Stop> {
    let f = Program::compile(expr, opts).map_err(|_| AnalysisError::AnalysisCouldNotBePerformed)?;
    let unit = opts.trig_unit;
    let d1 = derivative_bounded(expr, unit, MAX_DERIVATIVE_NODES);
    let d2 = d1
        .as_ref()
        .and_then(|d| derivative_bounded(d, unit, MAX_DERIVATIVE_NODES));
    let df = d1.and_then(|e| Program::compile(&e, opts).ok());
    let d2f = d2.and_then(|e| Program::compile(&e, opts).ok());
    let piecewise = expr.any(&|e| {
        matches!(
            e,
            Expr::Call(
                Func::Abs
                    | Func::Floor
                    | Func::Ceil
                    | Func::Round
                    | Func::Sign
                    | Func::Mod
                    | Func::Min
                    | Func::Max,
                args
            ) if args.iter().any(|a| a.contains_x())
        )
    });
    let tails = [-1.0, 1.0].map(|side| exact_tail(expr, opts, side));
    let mut power_bases = Vec::new();
    expr.visit(&mut |e| {
        if let Expr::Bin(BinOp::Pow, b, g) = e
            && g.contains_x()
            && let Ok(p) = Program::compile(b, opts)
            && p.as_constant().is_none_or(|c| c < 0.0)
        {
            power_bases.push(p);
        }
    });
    Ok(Fun {
        f,
        df,
        d2f,
        piecewise,
        kinks: kink_programs(expr, opts),
        tails,
        power_bases,
        unresolved: Cell::new(false),
        work: Cell::new(0),
        budget,
        cancel,
        cancelled: Cell::new(false),
    })
}

/// How `e` behaves as x → side·∞ when its form makes that certain: Some(±1)
/// if it grows without bound to ±∞.
fn grows(e: &Expr, opts: &CompileOptions<'_>, side: f64) -> Option<f64> {
    let num = |e: &Expr| Program::compile(e, opts).ok()?.as_constant();
    if !e.contains_x() {
        return None;
    }
    if let Some((c, _)) = linear_in(e, &|n| matches!(n, Expr::X))
        && let Some(a) = num(&c)
        && a != 0.0
        && a.is_finite()
    {
        return Some(a.signum() * side);
    }
    match e {
        Expr::Neg(g) => grows(g, opts, side).map(|s| -s),
        Expr::Bin(BinOp::Add | BinOp::Sub, a, b) if !b.contains_x() => grows(a, opts, side),
        Expr::Bin(BinOp::Add, a, b) if !a.contains_x() => grows(b, opts, side),
        Expr::Bin(BinOp::Mul, a, b) => {
            let (c, g) = if a.contains_x() { (b, a) } else { (a, b) };
            let k = num(c)?;
            (k != 0.0 && k.is_finite()).then_some(())?;
            grows(g, opts, side).map(|s| s * k.signum())
        }
        Expr::Bin(BinOp::Pow, g, k) => {
            let k = num(k)?;
            let s = grows(g, opts, side)?;
            if k <= 0.0 || !k.is_finite() {
                None
            } else if s > 0.0 {
                Some(1.0)
            } else if k.fract() == 0.0 {
                Some(if (k as i64) % 2 == 0 { 1.0 } else { -1.0 })
            } else {
                None
            }
        }
        Expr::Call(Func::Exp | Func::Ln | Func::Log | Func::Sqrt, args) => {
            (grows(&args[0], opts, side)? > 0.0).then_some(1.0)
        }
        _ => None,
    }
}

/// Whether `e` certainly tends to 0 as x → side·∞.
fn vanishes(e: &Expr, opts: &CompileOptions<'_>, side: f64) -> bool {
    let num = |e: &Expr| Program::compile(e, opts).ok().and_then(|p| p.as_constant());
    match e {
        Expr::Neg(v) => vanishes(v, opts, side),
        Expr::Bin(BinOp::Pow, g, k) => {
            num(k).is_some_and(|k| k < 0.0 && k.is_finite()) && grows(g, opts, side).is_some()
        }
        Expr::Bin(BinOp::Div, n, d) => {
            (!n.contains_x()
                && num(n).is_some_and(f64::is_finite)
                && grows(d, opts, side).is_some())
                || (vanishes(n, opts, side)
                    && !d.contains_x()
                    && num(d).is_some_and(|k| k != 0.0 && k.is_finite()))
        }
        Expr::Bin(BinOp::Mul, a, b) => {
            let (c, v) = if a.contains_x() { (b, a) } else { (a, b) };
            !c.contains_x() && num(c).is_some_and(f64::is_finite) && vanishes(v, opts, side)
        }
        Expr::Call(Func::Exp, args) => grows(&args[0], opts, side).is_some_and(|s| s < 0.0),
        _ => false,
    }
}

/// f's limit as x → side·∞ when f is a constant plus terms that certainly
/// vanish there; that constant.
fn exact_tail(expr: &Expr, opts: &CompileOptions<'_>, side: f64) -> Option<f64> {
    fn terms<'e>(e: &'e Expr, sign: f64, out: &mut Vec<(f64, &'e Expr)>) {
        match e {
            Expr::Bin(BinOp::Add, a, b) => {
                terms(a, sign, out);
                terms(b, sign, out);
            }
            Expr::Bin(BinOp::Sub, a, b) => {
                terms(a, sign, out);
                terms(b, -sign, out);
            }
            Expr::Neg(a) => terms(a, -sign, out),
            _ => out.push((sign, e)),
        }
    }
    if !expr.contains_x() {
        return None;
    }
    let mut parts = Vec::new();
    terms(expr, 1.0, &mut parts);
    let mut c = 0.0;
    for (sign, t) in parts {
        if t.contains_x() {
            if !vanishes(t, opts, side) {
                return None;
            }
        } else {
            c += sign * Program::compile(t, opts).ok()?.as_constant()?;
        }
    }
    c.is_finite().then_some(c)
}

fn analyze_with_budget(
    expr: &Expr,
    opts: &CompileOptions<'_>,
    cancel: Option<&AtomicBool>,
    budget: u64,
) -> Result<KeyGraphFeatures, Stop> {
    let fun = make_fun(expr, opts, cancel, budget)?;
    if trig_unresolved(expr, opts) {
        return Ok(unresolved_features(&fun));
    }

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

    // A non-periodic function's features may lie beyond the base window.
    let beyond = match period {
        Some(_) => Beyond::default(),
        None => beyond_window(&fun, expr, opts),
    };
    if let Some(stop) = fun.stopped() {
        return Err(stop);
    }
    let k = match period {
        Some(p) => analyze_periodic(&fun, &gens, p, scale),
        None => analyze_aperiodic(&fun, &gens, &beyond, scale),
    };
    // Running out of budget can surface as any failure downstream.
    if let Some(stop) = fun.stopped() {
        return Err(stop);
    }
    let mut k = k?;
    // Features that may also lie beyond the farthest search are unknown,
    // not extrapolated; so are those too many to follow out there, unless
    // the analysis already marked them.
    forget(&mut k, beyond.unknown);
    // A singular point possibly beyond the search (the pole of 1/(x − 10²⁰))
    // hides how f behaves on towards ±∞.
    if beyond.unknown & SINGULAR != 0 {
        forget(
            &mut k,
            flags::HORIZONTAL_ASYMPTOTES | flags::OBLIQUE_ASYMPTOTES,
        );
    }
    let lost = beyond.lost & !k.too_complex_features;
    forget(&mut k, lost);
    // Turns of f that went unfollowed: the range stands only if it holds
    // every value seen out there.
    if k.too_complex_features & flags::RANGE == 0
        && !beyond.range_check.is_empty()
        && !beyond.range_check.iter().all(|&v| {
            k.data.range.iter().any(|i| {
                let t = 1e-9 * v.abs();
                (v > i.lo.value - t || (i.lo.closed && v >= i.lo.value - t))
                    && (v < i.hi.value + t || (i.hi.closed && v <= i.hi.value + t))
            })
        })
    {
        forget(&mut k, flags::RANGE);
    }
    // Γ-based functions have poles at every negative integer: a numeric
    // scan cannot list them.
    let gamma_like = expr.any(&|e| {
        matches!(e, Expr::Call(Func::Factorial | Func::DoubleFactorial | Func::NCr | Func::NPr, args) if args.iter().any(|a| a.contains_x()))
    });
    if gamma_like {
        forget(
            &mut k,
            flags::DOMAIN
                | flags::RANGE
                | flags::VERTICAL_ASYMPTOTES
                | flags::MONOTONE_INTERVALS
                | flags::MINIMA
                | flags::MAXIMA
                | flags::INFLECTION_POINTS,
        );
    }
    let found: Vec<f64> = k
        .data
        .zeros
        .iter()
        .map(|z| z.x)
        .chain(
            k.data
                .minima
                .iter()
                .chain(&k.data.maxima)
                .chain(&k.data.inflection_points)
                .map(|m| m.0.x),
        )
        .chain(k.data.vertical_asymptotes.iter().map(|v| v.x))
        .collect();
    k.parity = parity(&fun, scale, beyond.extent, &found);
    // Something unknown beyond the search could break a symmetry seen
    // inside it.
    if beyond.unknown != 0 && matches!(k.parity, Parity::Even | Parity::Odd) {
        k.parity = Parity::Unknown;
    }
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
    // The range holds 0 yet no zero was located: f does cross 0, just where
    // doubles can't place it apart from an edge (ln x + 10⁶ at e^(−10⁶)).
    // That's unknown, not "none".
    let holds_zero = |i: &Interval| {
        (i.lo.value < 0.0 || (i.lo.closed && i.lo.value == 0.0))
            && (i.hi.value > 0.0 || (i.hi.closed && i.hi.value == 0.0))
    };
    if k.too_complex_features & (flags::ZEROS | flags::RANGE) == 0
        && k.data.zeros.is_empty()
        && k.data.range.iter().any(holds_zero)
    {
        k.too_complex_features |= flags::ZEROS;
    }
    // Somewhere f is beyond even extended range (sin(e^x) for x > 709.8):
    // whether it is defined there, and what it does, are unknown, not a gap
    // in its domain.
    if fun.unresolved.get() {
        forget(&mut k, VALUES);
        k.parity = Parity::Unknown;
    }
    // A feature that couldn't be determined in full keeps none of its
    // values: the ones found (the asymptote on one side, the extrema near
    // the origin of sin(x)/x) would read as all there is.
    let unknown = k.too_complex_features;
    forget(&mut k, unknown);
    Ok(k)
}

/// Every feature that depends on f's values rather than only its form.
const VALUES: u32 = flags::DOMAIN
    | flags::RANGE
    | flags::ZEROS
    | flags::MINIMA
    | flags::MAXIMA
    | flags::INFLECTION_POINTS
    | flags::VERTICAL_ASYMPTOTES
    | flags::HORIZONTAL_ASYMPTOTES
    | flags::OBLIQUE_ASYMPTOTES
    | flags::MONOTONE_INTERVALS;

/// The features of f = c wherever it is defined (0·x, 0/x, 0/e^(1/x), x/x,
/// e^x/e^x), given those found for its domain: c on each piece of it, every
/// point of it an x-intercept if c = 0, and nothing else to find. Sampling
/// alone can't tell a stretch of zeros from underflow, nor 1 from rounding.
fn constant_where(mut k: KeyGraphFeatures, c: f64) -> KeyGraphFeatures {
    if k.too_complex_features & flags::DOMAIN != 0 || k.data.domain.is_empty() {
        return k;
    }
    if k.domain == format_set("x", &[Interval::all()]) {
        return constant_features(c);
    }
    let c = if c.abs() <= 1e-15 { 0.0 } else { c };
    let derived = VALUES & !(flags::DOMAIN | flags::MONOTONE_INTERVALS);
    forget(&mut k, derived);
    k.too_complex_features &= !derived;
    if c == 0.0 {
        k.x_intercept = k.domain.clone();
    }
    k.range = format_set_with("y", &[Interval::closed(c, c)], &fmt_y);
    k.data.range = vec![Interval::closed(c, c)];
    if k.data.period.is_none() {
        k.too_complex_features &= !flags::MONOTONE_INTERVALS;
        k.data.monotonicity = (k.data.domain.iter())
            .map(|&i| (i, Monotonicity::Constant))
            .collect();
        k.monotonicity = (k.data.monotonicity.iter())
            .map(|(i, m)| (i.format(), *m))
            .collect();
    }
    k
}

/// Mark `which` features as too complex to determine, dropping whatever
/// was found for them.
pub(super) fn forget(k: &mut KeyGraphFeatures, which: u32) {
    k.too_complex_features |= which;
    let d = &mut k.data;
    if which & flags::DOMAIN != 0 {
        k.domain.clear();
        d.domain.clear();
        d.excluded.clear();
    }
    if which & flags::RANGE != 0 {
        k.range.clear();
        d.range.clear();
    }
    if which & flags::ZEROS != 0 {
        k.x_intercept.clear();
        d.zeros.clear();
    }
    if which & flags::MINIMA != 0 {
        k.minima.clear();
        d.minima.clear();
    }
    if which & flags::MAXIMA != 0 {
        k.maxima.clear();
        d.maxima.clear();
    }
    if which & flags::INFLECTION_POINTS != 0 {
        k.inflection_points.clear();
        d.inflection_points.clear();
    }
    if which & flags::VERTICAL_ASYMPTOTES != 0 {
        k.vertical_asymptotes.clear();
        d.vertical_asymptotes.clear();
    }
    if which & flags::MONOTONE_INTERVALS != 0 {
        k.monotonicity.clear();
        d.monotonicity.clear();
    }
    if which & flags::Y_INTERCEPT != 0 {
        k.y_intercept.clear();
        d.y_intercept = None;
    }
    if which & flags::HORIZONTAL_ASYMPTOTES != 0 {
        k.horizontal_asymptotes.clear();
        d.horizontal_asymptotes.clear();
    }
    if which & flags::OBLIQUE_ASYMPTOTES != 0 {
        k.oblique_asymptotes.clear();
        d.oblique_asymptotes.clear();
    }
}

/// Whether a circular function's argument is beyond 2⁴⁸ periods at every
/// moderate x (sin(x + 10¹⁸) + x): consecutive floats there are more than
/// 1/16 of a period apart, so the computed values say nothing about the
/// function's shape near the origin.
fn trig_unresolved(expr: &Expr, opts: &CompileOptions<'_>) -> bool {
    let period = match opts.trig_unit {
        TrigUnit::Radians => std::f64::consts::TAU,
        TrigUnit::Degrees => 360.0,
        TrigUnit::Grads => 400.0,
    };
    let limit = period * 2f64.powi(48);
    let xs = [-10.0, -3.0, -1.0, -0.3, 0.0, 0.3, 1.0, 3.0, 10.0];
    let mut unresolved = false;
    expr.visit(&mut |e| {
        if let Expr::Call(
            Func::Sin | Func::Cos | Func::Tan | Func::Sec | Func::Csc | Func::Cot,
            args,
        ) = e
            && let Some(arg) = args.first().filter(|a| a.contains_x())
            && let Ok(p) = Program::compile(arg, opts)
            && xs.iter().all(|&x| {
                let v = p.eval(x, 0.0);
                v.is_nan() || v.abs() > limit
            })
        {
            unresolved = true;
        }
    });
    unresolved
}

/// Everything unknown but the value at 0 (see [`trig_unresolved`]).
fn unresolved_features(fun: &Fun) -> KeyGraphFeatures {
    let mut k = KeyGraphFeatures {
        parity: Parity::Unknown,
        periodicity_direction: Periodicity::Unknown,
        ..Default::default()
    };
    let y0 = fun.f(0.0);
    if y0.is_finite() {
        k.y_intercept = fmt_y(y0);
        k.data.y_intercept = Some(y0);
    }
    k.too_complex_features = flags::DOMAIN
        | flags::RANGE
        | flags::PARITY
        | flags::PERIODICITY
        | flags::ZEROS
        | flags::MINIMA
        | flags::MAXIMA
        | flags::INFLECTION_POINTS
        | flags::VERTICAL_ASYMPTOTES
        | flags::HORIZONTAL_ASYMPTOTES
        | flags::OBLIQUE_ASYMPTOTES
        | flags::MONOTONE_INTERVALS;
    k
}

fn constant_features(c: f64) -> KeyGraphFeatures {
    let all = Interval::all();
    // Rounding residue of a computed constant (sin(π) ≈ 1.2·10⁻¹⁶) is 0; a
    // genuine small constant (10⁻¹⁰) is kept, not taken for 0.
    let c = if c.abs() <= 1e-15 { 0.0 } else { c };
    let mut k = KeyGraphFeatures {
        domain: format_set("x", &[all]),
        range: format_set_with("y", &[Interval::closed(c, c)], &fmt_y),
        parity: Parity::Even,
        periodicity_direction: Periodicity::NotPeriodic,
        y_intercept: fmt_y(c),
        monotonicity: vec![(all.format(), Monotonicity::Constant)],
        ..Default::default()
    };
    if c == 0.0 {
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

/// How far f(x) = `v` may be from the function's exact value at the real
/// number x stands for: what the rounding of x itself moves f by (f′·δ +
/// f″·δ²/2 over a few units in the last place δ), and the rounding of the
/// evaluation, seen as roughness of f across the neighbouring floats. This
/// is local: a value is noise only against what f does at x, never against
/// f's size elsewhere (10⁻⁷ is a genuine minimum of x² + 10⁻⁷ though x² is
/// 25 on average near the origin).
fn value_noise(fun: &Fun, x: f64, v: f64) -> f64 {
    let u = 4.0 * f64::EPSILON * x.abs();
    let mut n = 0.0;
    if u > 0.0 {
        let (d1, d2) = (fun.df(x), fun.d2f(x));
        if d1.is_finite() {
            n += d1.abs() * u;
        }
        if d2.is_finite() {
            n += 0.5 * d2.abs() * u * u;
        }
        let (a, b) = (fun.f(x - u), fun.f(x + u));
        if a.is_finite() && b.is_finite() && v.is_finite() {
            n += (a - 2.0 * v + b).abs();
        }
    }
    8.0 * n + 16.0 * f64::EPSILON * v.abs()
}

/// A value of f cleared of noise: 0 if it is within `noise` of 0, a
/// recognised closed form if it is that close to one, otherwise itself. A
/// genuine tiny value is never turned into 0 by closed-form recognition.
fn clean(v: f64, noise: f64) -> f64 {
    clean_within(v, noise, false)
}

/// [`clean`] for a level f only tends to (a horizontal asymptote): also
/// moved within 10⁻⁹ of it where that changes no digit the panel shows, as
/// telling f's tail from that level is no finer.
fn clean_level(v: f64, noise: f64) -> f64 {
    clean_within(v, noise, true)
}

fn clean_within(v: f64, noise: f64, shown: bool) -> f64 {
    if !v.is_finite() {
        return v;
    }
    if v.abs() <= noise {
        return 0.0;
    }
    // Within the value's noise (a limit known to 10⁻⁸ is 3, not
    // 3.0000000051), or 10⁻⁹ of it but no more than 10⁻⁶ in all
    // (545843449.4 is not the integer 545843449, nor 10⁶ + 10⁻⁶ just 10⁶);
    // never more than 10⁻⁶ of it.
    let tol = (noise / v.abs()).clamp(1e-9, 1e-6);
    let s = snap(v, tol);
    let d = (s - v).abs();
    let near = d <= noise || d <= 1e-6 || (shown && format_decimal(s) == format_decimal(v));
    if s != 0.0 && d <= tol * v.abs() && near {
        s
    } else {
        v
    }
}

/// `c`, a root of `g`, moved to a recognised closed form (or 0) when that
/// lies within c's own uncertainty, the stretch around c where g can't be
/// told from 0 for rounding. x⁴ − 4x³ + 6x² − 4x + 1 turns at 1, but its
/// f′ is rounding noise for |x − 1| ≲ 2·10⁻⁵: its minimum is at 1, not at
/// 1.0000052. The zero of sin x − 10⁻⁹ at 10⁻⁹ is resolved and stays.
/// Never moved where `ok` is false (outside the domain).
fn snap_root(c: f64, g: &dyn Fn(f64) -> f64, ok: &dyn Fn(f64) -> bool) -> f64 {
    if !c.is_finite() || c == 0.0 {
        return c;
    }
    let unit = c.abs();
    let h = 64.0 * f64::EPSILON * unit;
    // g's rounding noise near c: the roughness (third differences) of its
    // values, over several spacings. Rounded values are quantised: right at
    // a flat root they can all come out exactly 0 over a few ulps (the f′ of
    // x⁴ − 4x³ + 6x² − 4x + 1 near 1), and only a wider look shows the noise.
    // Wider only while g is still noise across the look: beyond, the
    // roughness is g's own change between rounded arguments (far from 0
    // at 10¹², whole units apart are a wide look).
    let roughness = |step: f64| -> Option<(f64, f64)> {
        let w = [-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0].map(|k| g(c + k * step));
        if w.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let big = w.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let n = (0..4)
            .map(|i| (w[i + 3] - 3.0 * w[i + 2] + 3.0 * w[i + 1] - w[i]).abs())
            .fold(0.0f64, f64::max)
            + 64.0 * f64::EPSILON * big;
        Some((n, big))
    };
    let Some((mut noise, mut big)) = roughness(h) else {
        return c;
    };
    for scale in [16.0, 256.0, 4096.0, 65536.0] {
        if h * scale > 1e-6 * unit || big > 64.0 * noise {
            break;
        }
        match roughness(h * scale) {
            Some((n, b)) => {
                noise = noise.max(n);
                big = b;
            }
            None => break,
        }
    }
    let flat = |x: f64| {
        let v = g(x);
        v.is_finite() && v.abs() <= 8.0 * noise
    };
    // A root reported where g is plainly not 0 (picked from a stretch the
    // detection floored to 0, as for x⁴ − 4x³ + 6x² − 4x + 1 + 10⁶): move
    // to the nearest place g is noise or changes sign, and work from there.
    let gc = g(c);
    let mut c = c;
    if gc.is_finite() && !flat(c) {
        let mut d = h;
        'out: for _ in 0..48 {
            if d > 1e-3 * unit {
                break;
            }
            for dir in [-1.0, 1.0] {
                let x = c + dir * d;
                let v = g(x);
                if !v.is_finite() || !ok(x) {
                    continue;
                }
                if flat(x) {
                    c = x;
                    break 'out;
                }
                if v.signum() != gc.signum() {
                    let (mut a, mut b) = (c, x);
                    for _ in 0..80 {
                        let m = 0.5 * (a + b);
                        if m == a || m == b {
                            break;
                        }
                        let vm = g(m);
                        if vm.is_finite() && vm.signum() == gc.signum() && !flat(m) {
                            a = m;
                        } else {
                            b = m;
                        }
                    }
                    c = b;
                    break 'out;
                }
            }
            d *= 2.0;
        }
    }
    // How far g stays indistinguishable from 0 on each side of c. The two
    // sides are measured apart: bisection lands anywhere in the band, often
    // near one edge (1.0000052 in a band of ±1.3·10⁻⁵ around 1), so a
    // symmetric reach would stop at the near edge.
    let reach = |dir: f64| {
        let mut d = h;
        for _ in 0..48 {
            if 2.0 * d > 1e-3 * unit || !flat(c + dir * 2.0 * d) {
                break;
            }
            d *= 2.0;
        }
        d
    };
    let (lo, hi) = (c - reach(-1.0), c + reach(1.0));
    let delta = (hi - lo) / 2.0;
    if lo <= 0.0 && 0.0 <= hi && ok(0.0) && flat(0.0) {
        return 0.0;
    }
    let within = |s: f64| s >= lo && s <= hi && ok(s) && flat(s);
    let tol = (delta / unit).clamp(1e-13, 1e-4);
    let s = snap(c, tol);
    if s != c && within(s) {
        return s;
    }
    // No closed form: the middle of the band is the better estimate when
    // the band is wide (the root of a flat g is where it is symmetric).
    if delta > 16.0 * h {
        let m = 0.5 * (lo + hi);
        let sm = snap(m, tol);
        if sm != m && within(sm) {
            return sm;
        }
        if within(m) {
            return m;
        }
    }
    c
}

/// Median |f| over a sample of moderate x values: a magnitude for
/// tolerances. A function that is tiny everywhere (1/((x − 10³)…(x − 9·10³))
/// is about 10⁻³³ near 0) keeps its own size, so its extrema aren't taken
/// for zeros; 10⁻¹² stands in only when f is mostly exactly 0 there.
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
    match v[v.len() / 2] {
        0.0 => 1e-12,
        m => m.max(1e-280),
    }
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

/// Sub-expressions whose zeros are kinks of f, where f′ may jump: the
/// arguments of abs, and differences of the arguments of max and min.
fn kink_exprs(e: &Expr, out: &mut Vec<Expr>) {
    let push = |g: Expr, out: &mut Vec<Expr>| {
        if g.contains_x() && !out.contains(&g) {
            out.push(g);
        }
    };
    match e {
        Expr::Bin(_, a, b) => {
            kink_exprs(a, out);
            kink_exprs(b, out);
        }
        Expr::Neg(a) | Expr::Degrees(a) => kink_exprs(a, out),
        Expr::Call(f, args) => {
            match f {
                Func::Abs => push(args[0].clone(), out),
                Func::Max | Func::Min => {
                    let n = args.len().min(6);
                    for i in 0..n {
                        for j in i + 1..n {
                            push(Expr::bin(BinOp::Sub, args[i].clone(), args[j].clone()), out);
                        }
                    }
                }
                _ => {}
            }
            for a in args {
                kink_exprs(a, out);
            }
        }
        _ => {}
    }
}

fn kink_programs(expr: &Expr, opts: &CompileOptions<'_>) -> Vec<Program> {
    let mut kinks = Vec::new();
    kink_exprs(expr, &mut kinks);
    kinks
        .iter()
        .filter_map(|g| Program::compile(g, opts).ok())
        .filter(|p| p.as_constant().is_none())
        .collect()
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

// ---------------------------------------------------------------------------
// Beyond the base window. Its grid covers [−10⁶, 10⁶]; a pole at 2·10⁶ or
// the zero of x − 2·10⁶ lies outside it and must not be extrapolated away.

/// How far out features are looked for.
const REACH: f64 = 1e15;
/// Probe points per side between `WINDOW` and `REACH` (geometric).
const PROBE_STEPS: usize = 800;
/// More features than this on one side of the probe is oscillation (as in
/// sin(x)/x), not structure worth widening the window for: those features
/// are reported as unknown beyond the window.
const MAX_PROBED: usize = 8;
/// A polynomial's probe has no such cap short of its degree bound (the
/// roots of f, f′ and f″ of degree ≤ 64).
const MAX_POLY_PROBED: usize = 3 * 64;

/// Features a singular point beyond the search would change.
const SINGULAR: u32 =
    flags::DOMAIN | flags::VERTICAL_ASYMPTOTES | flags::RANGE | flags::MONOTONE_INTERVALS;
/// Features a zero of f, f′ or f″ beyond the search would change.
const OF_F: u32 = flags::ZEROS
    | flags::RANGE
    | flags::MINIMA
    | flags::MAXIMA
    | flags::MONOTONE_INTERVALS
    | flags::INFLECTION_POINTS;

/// Sample points per side of the dense grid laid around each feature
/// found beyond the window (features cluster there as they do near 0).
const LOCAL_HALF: usize = 4000;
/// The same for a polynomial, whose (many, double, well separated) far
/// features each get one: still dense enough at the centre (≈ 0.02 at
/// 10⁷) to split roots a unit apart.
const LOCAL_HALF_POLY: usize = 800;

/// What lies beyond the base window.
#[derive(Default)]
struct Beyond {
    /// Largest |x| of anything found between `WINDOW` and `REACH`, or 0.
    extent: f64,
    /// Where those things are (a few, clustered).
    features: Vec<f64>,
    /// Zeros of the generators found there: candidate singular points.
    candidates: Vec<f64>,
    /// Features that may also lie beyond `REACH`, where nothing is
    /// searched (flags): reported as unknown rather than extrapolated.
    unknown: u32,
    /// Features the probe found too many of to follow (oscillation, as in
    /// sin(x)/x): unknown beyond the window, unless the analysis already
    /// said so. Unlike `unknown`, they don't put a symmetry in doubt.
    lost: u32,
    /// Points per side of the local grid around each feature.
    local_half: usize,
    /// Values of f out where critical points went unfollowed (the probe's,
    /// and far ones beyond the search): a range claimed globally must hold
    /// them all, or it is unknown.
    range_check: Vec<f64>,
}

/// Look past the base window for zeros (and sign changes) of f, f′, f″ and
/// the singularity generators, and for signs of more beyond `REACH`.
fn beyond_window(fun: &Fun, expr: &Expr, opts: &CompileOptions<'_>) -> Beyond {
    let pos: Vec<f64> = (0..=PROBE_STEPS)
        .map(|i| WINDOW * (REACH / WINDOW).powf(i as f64 / PROBE_STEPS as f64))
        .collect();
    let neg: Vec<f64> = pos.iter().rev().map(|x| -x).collect();
    let mut gen_exprs = Vec::new();
    generators(expr, &mut gen_exprs);
    let gens: Vec<(Expr, Program)> = gen_exprs
        .into_iter()
        .filter_map(|g| {
            let p = Program::compile(&g, opts).ok()?;
            p.as_constant().is_none().then_some((g, p))
        })
        .collect();
    // A polynomial has a few roots, all of which matter: its probe isn't
    // capped (nine double roots between 2·10⁶ and 10⁷ are not oscillation).
    let f_poly = poly_coeffs(expr, opts).is_some();
    let mut out = Beyond {
        local_half: if f_poly { LOCAL_HALF_POLY } else { LOCAL_HALF },
        ..Beyond::default()
    };
    let note = |out: &mut Beyond, pts: &[f64]| {
        for &x in pts {
            out.extent = out.extent.max(x.abs());
            // One local grid serves features within 1% of each other.
            if !out
                .features
                .iter()
                .any(|&c| (c - x).abs() <= 1e-2 * c.abs())
            {
                out.features.push(x);
            }
        }
    };
    // Features a probe found too many of to follow are unknown beyond the
    // window, not absent.
    let lost = [
        flags::ZEROS,
        flags::MINIMA | flags::MAXIMA | flags::MONOTONE_INTERVALS,
        flags::INFLECTION_POINTS,
    ];
    for xs in [&neg, &pos] {
        let fs = fun.batch(0, xs);
        let dfs = fun.batch(1, xs);
        for which in 0..3u8 {
            let vs = match which {
                0 => fs.clone(),
                1 => dfs.clone(),
                _ => fun.batch(2, xs),
            };
            // Rounding noise in f′ and f″ (of a function linear in disguise,
            // like (x² − 1)/(x − 1)) is no sign change: values below this
            // floor, relative to f, don't count.
            let floor: Vec<f64> = (0..xs.len())
                .map(|i| {
                    let x1 = 1.0 + xs[i].abs();
                    let (f0, f1) = (fs[i].abs(), dfs[i].abs());
                    match which {
                        0 => 0.0,
                        1 => 1e-9 * f0 / x1,
                        _ => 1e-9 * (f1 / x1 + f0 / (x1 * x1)),
                    }
                })
                .map(|v| if v.is_finite() { v } else { 0.0 })
                .collect();
            let mut g = |x: f64| match which {
                0 => fun.f(x),
                1 => fun.df(x),
                _ => fun.d2f(x),
            };
            let cap = if f_poly { MAX_POLY_PROBED } else { MAX_PROBED };
            let pts = probe_points(xs, &vs, &floor, &mut g, cap, true);
            if pts.len() <= cap {
                note(&mut out, &pts);
            } else {
                out.lost |= lost[which as usize];
                // Unfollowed turns of f: the range must hold what f does
                // out there (it does for sin(x)/x, whose far swings are tiny;
                // not for squared factors with zeros at 4·10⁶ … 10⁸).
                if which == 1 {
                    out.range_check
                        .extend(fs.iter().copied().filter(|v| v.is_finite()));
                }
            }
        }
        for (e, p) in &gens {
            let mut vs = vec![0.0; xs.len()];
            fun.eval_batch(p, xs, &mut vs);
            let cap = if poly_coeffs(e, opts).is_some() {
                MAX_POLY_PROBED
            } else {
                MAX_PROBED
            };
            let none = vec![0.0; xs.len()];
            let pts = probe_points(xs, &vs, &none, &mut |x| fun.eval(p, x), cap, false);
            if pts.len() <= cap {
                note(&mut out, &pts);
                out.candidates.extend(pts);
            } else {
                out.lost |= SINGULAR;
            }
        }
    }

    // Beyond REACH nothing is searched. A polynomial's real roots (and
    // those of its derivatives) are bounded, so it is either known to have
    // none out there or flagged. A singularity generator of any other kind
    // is flagged if it is still heading for zero at the end of the search
    // (1/(ln x − 100)). For f itself that test can't tell a far zero from
    // a slow decay (x^0.9′ = 0.9·x^−0.1, 1/ln x), so a non-polynomial f's
    // zeros, extrema and inflections are only searched out to REACH.
    let far = [0.5 * REACH, 0.75 * REACH, REACH];
    let heading = |g: &dyn Fn(f64) -> f64| {
        [-1.0, 1.0]
            .iter()
            .any(|&s| heading_to_zero(far.map(|x| g(s * x))))
    };
    if poly_coeffs(expr, opts).is_some_and(|c| root_bound(&c) > REACH) {
        out.unknown |= OF_F;
    }
    // A non-polynomial f may still cross 0 beyond the search (ln x − 40 at
    // e⁴⁰ ≈ 2.4·10¹⁷), which would add a zero and change its range: f is
    // sampled once per decade out to the largest x there is.
    if !f_poly {
        for s in [-1.0, 1.0] {
            let far = beyond_reach(fun, s);
            match far.zeros {
                // Crossing once (ln x − 40) only adds a zero: the range
                // already follows the limit at infinity.
                FarZeros::One | FarZeros::Many => out.lost |= flags::ZEROS,
                // Out and back again: an extremum out there the range missed.
                FarZeros::Two => out.lost |= flags::ZEROS | flags::RANGE,
                FarZeros::None => {}
            }
            // A turn of f out there ((ln x − 40)(ln x − 50) at e⁴⁵, or
            // (ln x − 40)² touching 0 at e⁴⁰): its extrema, monotonicity and
            // any zero it touches are unknown, and the range must hold f's
            // values there.
            if far.turns {
                out.lost |=
                    flags::ZEROS | flags::MINIMA | flags::MAXIMA | flags::MONOTONE_INTERVALS;
                out.range_check.extend(far.values.iter().copied());
            }
            if far.bends {
                out.lost |= flags::INFLECTION_POINTS;
            }
        }
    }
    for (e, p) in &gens {
        let unknown = match poly_coeffs(e, opts).map(|c| root_bound(&c)) {
            Some(b) => b > REACH,
            None => heading(&|x| fun.eval(p, x)),
        };
        if unknown {
            out.unknown |= SINGULAR;
        }
    }
    out.candidates.sort_by(|a, b| a.total_cmp(b));
    out
}

/// Sign changes (refined: zeros, or poles of g) and even-order zeros of g
/// on an ascending grid; stops once there are more than `cap`. Values at
/// or below `floor` (rounding noise) don't count as either sign. An exact
/// zero sample counts when its neighbours are of normal size (x − 10¹⁵ is
/// exactly 0 at the last sample, with nothing beyond to bracket it); runs
/// of zeros and underflowed neighbours (e^x far left) are not roots.
fn probe_points(
    xs: &[f64],
    vs: &[f64],
    floor: &[f64],
    g: &mut dyn FnMut(f64) -> f64,
    cap: usize,
    zeros_only: bool,
) -> Vec<f64> {
    let mut out = Vec::new();
    let ok = |i: usize| vs[i].is_finite() && vs[i] != 0.0 && vs[i].abs() > floor[i];
    let normal = |i: usize| ok(i) && vs[i].abs() > 1e-250;
    for i in 0..xs.len() {
        if out.len() > cap {
            break;
        }
        if vs[i] == 0.0 {
            let before = i.checked_sub(1);
            let after = (i + 1 < xs.len()).then_some(i + 1);
            let isolated = before.is_none_or(normal) && after.is_none_or(normal);
            if isolated && (before.is_some() || after.is_some()) {
                out.push(xs[i]);
            }
            continue;
        }
        if i + 1 == xs.len() {
            break;
        }
        let (a, b) = (vs[i], vs[i + 1]);
        if ok(i) && ok(i + 1) && (a < 0.0) != (b < 0.0) {
            let r = brent(g, xs[i], a, xs[i + 1], b);
            // A sign change through a pole of g is no zero of it.
            if !zeros_only || g(r).abs() <= 1e-6 * a.abs().max(b.abs()) {
                out.push(r);
            }
        }
        if i > 0 && ok(i - 1) && ok(i) && ok(i + 1) && (vs[i - 1] < 0.0) == (b < 0.0) {
            let (p, c, n) = (vs[i - 1].abs(), a.abs(), b.abs());
            if c < p && c <= n {
                let m = golden_min_abs(g, xs[i - 1], xs[i + 1]);
                if g(m).abs() <= 1e-10 * c {
                    out.push(m);
                }
            }
        }
    }
    out
}

/// Whether g, sampled at three evenly spaced far points, is still heading
/// for zero about linearly, so a zero may lie further out. Decays that only
/// approach 0 (1/x, x^(−1/2)) slow down faster than this, and a value
/// settling on a constant barely moves.
fn heading_to_zero(v: [f64; 3]) -> bool {
    if !v.iter().all(|x| x.is_finite() && x.abs() > 1e-250) {
        return false;
    }
    if (v[0] < 0.0) != (v[1] < 0.0) || (v[1] < 0.0) != (v[2] < 0.0) {
        return false;
    }
    let [a, b, c] = v.map(f64::abs);
    a > b && b > c && a - c > 1e-9 * a && b - c >= 0.66 * (a - b)
}

enum FarZeros {
    None,
    One,
    Two,
    /// Oscillation, as in sin(x)/x.
    Many,
}

/// What f does beyond the search on one side.
struct FarOut {
    zeros: FarZeros,
    /// f′ changes sign: an extremum out there.
    turns: bool,
    /// f″ changes sign: an inflection out there.
    bends: bool,
    /// f at the far samples.
    values: Vec<f64>,
}

/// [`zeros_beyond_reach`], plus sign changes of f′ and f″ at the same
/// decade samples (genuine zeros only, not poles).
fn beyond_reach(fun: &Fun, s: f64) -> FarOut {
    let xs: Vec<f64> = (15..=307).map(|t| s * 10f64.powi(t)).collect();
    let fs: Vec<f64> = xs.iter().map(|&x| fun.f(x)).collect();
    let changes = |which: u8| -> bool {
        let mut g = |x: f64| if which == 1 { fun.df(x) } else { fun.d2f(x) };
        let pts: Vec<(f64, f64, f64)> = xs
            .iter()
            .zip(&fs)
            .map(|(&x, &fv)| (x, g(x), fv))
            .filter(|(_, v, fv)| v.is_finite() && *v != 0.0 && fv.is_finite())
            .collect();
        pts.windows(2).any(|w| {
            let ((xa, a, fa), (xb, b, fb)) = (w[0], w[1]);
            // Rounding noise in f′ and f″ relative to f is no turn: f′
            // scales like f/x and f″ like f/x² ((ln x − 40)(ln x − 50) bends
            // at e⁴⁶ with f″ ≈ 10⁻³⁸).
            let floor = |fv: f64, x: f64| 1e-9 * fv.abs() / x.abs().max(1.0).powi(i32::from(which));
            if (a < 0.0) == (b < 0.0) || a.abs() <= floor(fa, xa) || b.abs() <= floor(fb, xb) {
                return false;
            }
            let r = brent(&mut g, xa, a, xb, b);
            g(r).abs() <= 1e-6 * a.abs().max(b.abs())
        })
    };
    let turns = changes(1);
    let bends = changes(2);
    FarOut {
        zeros: zeros_beyond_reach(fun, s),
        turns,
        bends,
        values: fs.into_iter().filter(|v| v.is_finite()).collect(),
    }
}

/// Zeros of f beyond the search on the side `s`: f is sampled at ±10ᵗ for
/// t = 15 … 307 and each sign change refined, keeping only genuine zeros
/// (the pole of 1/(x − 10²⁰) flips the sign too). Also one when f is still
/// closing in on 0 at a steady pace where the samples end (ln x − 800,
/// whose zero is past the largest double). A decay that only approaches 0
/// (1/(x − 10¹⁵) on the left, 1/ln x, x^−0.01) has none.
fn zeros_beyond_reach(fun: &Fun, s: f64) -> FarZeros {
    let pts: Vec<(f64, f64)> = (15..=307)
        .map(|t| s * 10f64.powi(t))
        .map(|x| (x, fun.f(x)))
        .filter(|(_, v)| v.is_finite() && *v != 0.0)
        .collect();
    let mut f = |x: f64| fun.f(x);
    let mut zeros = 0;
    for w in pts.windows(2) {
        let ((xa, a), (xb, b)) = (w[0], w[1]);
        if (a < 0.0) != (b < 0.0) {
            let r = brent(&mut f, xa, a, xb, b);
            if f(r).abs() <= 1e-6 * a.abs().max(b.abs()) {
                zeros += 1;
            }
        }
    }
    let n = pts.len();
    let steady =
        n >= 4 && steady_toward_zero([pts[n - 4].1, pts[n - 3].1, pts[n - 2].1, pts[n - 1].1]);
    match zeros {
        0 if !steady => FarZeros::None,
        0 | 1 => FarZeros::One,
        2 => FarZeros::Two,
        _ => FarZeros::Many,
    }
}

/// Whether four successive decade samples of one sign close in on 0 by
/// steps that don't ease off at all, beyond rounding: ln x − c moves by
/// exactly ln 10 per decade. A power tail eases off by 10^p per decade,
/// however close to 1 (x^−0.0001: 0.99977), and approaches 0 without
/// reaching it; 1/ln x eases off by 0.9935.
fn steady_toward_zero(v: [f64; 4]) -> bool {
    if !v.iter().all(|x| x.signum() == v[0].signum()) {
        return false;
    }
    let a = v.map(f64::abs);
    let steps = [a[0] - a[1], a[1] - a[2], a[2] - a[3]];
    let rounding = 64.0 * f64::EPSILON * a[0];
    steps.iter().all(|&d| d > 1e-12 * a[0])
        && steps
            .windows(2)
            .all(|w| w[1] >= w[0] * (1.0 - 1e-9) - 4.0 * rounding)
}

/// Coefficients (constant term first) of `e` as a polynomial in x with
/// constant coefficients, if it is one of modest degree.
fn poly_coeffs(e: &Expr, opts: &CompileOptions<'_>) -> Option<Vec<f64>> {
    const MAX_DEGREE: usize = 64;
    if !e.contains_x() {
        let c = Program::compile(e, opts).ok()?.as_constant()?;
        return c.is_finite().then(|| vec![c]);
    }
    let mul = |a: &[f64], b: &[f64]| -> Option<Vec<f64>> {
        if a.len() + b.len() - 2 > MAX_DEGREE {
            return None;
        }
        let mut r = vec![0.0; a.len() + b.len() - 1];
        for (i, x) in a.iter().enumerate() {
            for (j, y) in b.iter().enumerate() {
                r[i + j] += x * y;
            }
        }
        Some(r)
    };
    match e {
        Expr::X => Some(vec![0.0, 1.0]),
        Expr::Neg(a) => Some(poly_coeffs(a, opts)?.iter().map(|c| -c).collect()),
        Expr::Bin(op @ (BinOp::Add | BinOp::Sub), a, b) => {
            let (a, b) = (poly_coeffs(a, opts)?, poly_coeffs(b, opts)?);
            let s = if *op == BinOp::Add { 1.0 } else { -1.0 };
            let mut r = vec![0.0; a.len().max(b.len())];
            for (i, x) in a.iter().enumerate() {
                r[i] += x;
            }
            for (i, y) in b.iter().enumerate() {
                r[i] += s * y;
            }
            Some(r)
        }
        Expr::Bin(BinOp::Mul, a, b) => mul(&poly_coeffs(a, opts)?, &poly_coeffs(b, opts)?),
        Expr::Bin(BinOp::Div, a, b) if !b.contains_x() => {
            let d = poly_coeffs(b, opts)?[0];
            (d != 0.0).then_some(())?;
            Some(poly_coeffs(a, opts)?.iter().map(|c| c / d).collect())
        }
        Expr::Bin(BinOp::Pow, a, b) => {
            let (n, 1) = syntactic_rational(b)? else {
                return None;
            };
            let n = usize::try_from(n).ok().filter(|&n| n <= MAX_DEGREE)?;
            let base = poly_coeffs(a, opts)?;
            let mut r = vec![1.0];
            for _ in 0..n {
                r = mul(&r, &base)?;
            }
            Some(r)
        }
        _ => None,
    }
}

/// An upper bound on the magnitude of every (real or complex) root of the
/// polynomial with coefficients `c` (constant term first); 0 for a
/// constant. This is Fujiwara's bound,
/// 2·max(|a₍ₙ₋₁₎/aₙ|, |a₍ₙ₋₂₎/aₙ|^(1/2), …, |a₁/aₙ|^(1/(n−1)), |a₀/(2aₙ)|^(1/n)),
/// which holds for every polynomial; it can overestimate the largest root,
/// by more than a factor 2 for some, so it is only used to rule roots out.
fn root_bound(c: &[f64]) -> f64 {
    let n = match c.iter().rposition(|&a| a != 0.0) {
        Some(n) if n > 0 => n,
        _ => return 0.0,
    };
    let an = c[n];
    let mut m: f64 = 0.0;
    for k in 1..=n {
        let a = if k == n { c[0] / 2.0 } else { c[n - k] };
        m = m.max((a / an).abs().powf(1.0 / k as f64));
    }
    2.0 * m
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
    let pairs: Vec<(f64, f64)> = (0..64)
        .map(|i| {
            let t = -3.1 * p + i as f64 * 0.1037 * p + 0.012_345 * p;
            (fun.f(t), fun.f(t + p))
        })
        .collect();
    // f must repeat to within rounding of how much it varies here, not of
    // its own size: tan x + 10⁶ agrees with itself shifted by π/24 to
    // 10⁻⁷ of 10⁶, yet that is no period. The median deviation keeps a
    // pole's huge values from widening the tolerance.
    let mut vals: Vec<f64> = pairs
        .iter()
        .flat_map(|&(a, b)| [a, b])
        .filter(|v| v.is_finite())
        .collect();
    if vals.is_empty() {
        return false;
    }
    vals.sort_by(f64::total_cmp);
    let med = vals[vals.len() / 2];
    let mut dev: Vec<f64> = vals.iter().map(|v| (v - med).abs()).collect();
    dev.sort_by(f64::total_cmp);
    let spread = match dev[dev.len() / 2] {
        0.0 => dev[dev.len() - 1],
        m => m,
    };
    let floor = if spread > 0.0 { 0.0 } else { 1e-9 * scale };
    let mut ok = 0;
    let mut bad = 0;
    for (a, b) in pairs {
        match (a.is_finite(), b.is_finite()) {
            (true, true) => {
                let tol = 1e-7 * spread + 64.0 * f64::EPSILON * (a.abs() + b.abs()) + floor;
                if (a - b).abs() <= tol {
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

/// Even, odd or neither, from samples out to the base window and, where
/// features were found beyond it, out to them too (1/(x³ − 10³⁰) looks
/// even until x nears 10¹⁰).
fn parity(fun: &Fun, scale: f64, extent: f64, extra: &[f64]) -> Parity {
    let mut even = true;
    let mut odd = true;
    let mut any = false;
    let near = (0..60).map(|i| (0.0371 * 1.31f64.powi(i) + 1e-3 * i as f64, true));
    // Out there values can be far below the typical scale (≈10⁻³⁰ for
    // 1/(x³ − 10³⁰)), so only a relative tolerance means anything.
    let far = (0..=12)
        .map(|i| WINDOW * (3.0 * extent / WINDOW).powf(i as f64 / 12.0))
        .chain([0.3, 0.5, 0.7, 0.9, 1.3, 2.0].map(|t| t * extent))
        .filter(|_| extent > WINDOW)
        .map(|x| (x, false));
    // And where something was found: e^(−(x − 1000)²) is 0 at every point
    // above but 1 at 1000 and 0 at −1000.
    let found = extra
        .iter()
        .filter(|x| x.is_finite() && **x != 0.0)
        .map(|&x| (x.abs(), false));
    for (x, near) in near.chain(far).chain(found) {
        let (a, b) = (fun.f(x), fun.f(-x));
        match (a.is_finite(), b.is_finite()) {
            (false, false) => continue,
            (true, true) => {}
            _ => return Parity::Neither,
        }
        any = true;
        let tol = 1e-9 * (a.abs() + b.abs()) + if near { 1e-13 * scale } else { 0.0 };
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
    /// Values beyond the floating-point range that no pole or infinite end
    /// explains (e^(1000 − x²) near 0): f's size, and so its extrema,
    /// bends and range, can't be measured there.
    overflow: bool,
}

/// Which samples are in the domain: everything but NaN. Undefined points
/// (division by exactly zero, logs of zero, poles of Γ…) evaluate to NaN,
/// so ±∞ only comes from floating-point overflow of a defined value: e^x
/// for x > 709.8, or 1/x^400 near 0 where x^400 is subnormal.
fn defined_mask(fs: &[f64]) -> Vec<bool> {
    fs.iter().map(|v| !v.is_nan()).collect()
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
        // Neither undefined nor a pole where f is defined but overflows
        // (1/x^400 where x^400 is subnormal).
        if (!dl && !dr) || fc.is_infinite() {
            continue;
        }
        if fc.is_nan() || pole_l || pole_r {
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
        let edge = bisect_defined(&mut f, good, bad);
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
    let mut overflow = false;
    let mut i = 0;
    while i < n {
        if !fs[i].is_infinite() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j + 1 < n && fs[j + 1].is_infinite() {
            j += 1;
        }
        let (a, b) = (xs[i.saturating_sub(1)], xs[(j + 1).min(n - 1)]);
        let at_end = (i == 0 && lo_inf) || (j == n - 1 && hi_inf);
        if !at_end && !poles.iter().any(|&p| p >= a && p <= b) {
            overflow = true;
        }
        i = j + 1;
    }
    Scan {
        pieces,
        poles,
        excluded,
        too_complex,
        overflow,
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
    /// How far each end of `range` may be off (see [`value_noise`]).
    range_noise: (f64, f64),
    /// An end of the range can't be determined (see `piece_features`).
    range_unsure: bool,
    /// Values at closed ends that f jumps to from inside (0^x is 0 for
    /// x > 0 but 1 at 0): points of the range apart from `range`.
    range_points: Vec<f64>,
}

/// Whether `v` never turns: all steps of one sign (or zero).
fn monotone(v: &[f64]) -> bool {
    v.len() > 2 && (v.windows(2).all(|w| w[1] >= w[0]) || v.windows(2).all(|w| w[1] <= w[0]))
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

/// Points closing in on `e` from the side `side` (±1) by decades, from
/// `gap` (the distance to the nearest sample there), while f stays well
/// conditioned: its change over a step a thousandth of the distance to `e`
/// must agree with f′ and f″ there. Cancellation near a removable
/// singularity ((1 − cos x)/x² at 0) fails that and ends the ladder, as
/// does a flat or underflowed stretch, where there is nothing to find.
fn ladder(fun: &Fun, e: f64, side: f64, gap: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    if gap.is_nan() || gap <= 0.0 || gap.is_infinite() {
        return out;
    }
    for k in 1..=12 {
        let d = gap * 10f64.powi(-k);
        let x = e + side * d;
        let x2 = x + side * 1e-3 * d;
        if (x - e) * side <= 0.0 || (x2 - x).abs() < 64.0 * f64::EPSILON * x.abs() {
            break;
        }
        let (v, v2) = (fun.f(x), fun.f(x2));
        if !v.is_finite() || !v2.is_finite() {
            break;
        }
        let (d1, d2) = (fun.df(x), fun.d2f(x));
        if v == v2 && d1 == 0.0 {
            break;
        }
        let dx = x2 - x;
        let lin = d1 * dx;
        let quad = if d2.is_finite() {
            0.5 * d2 * dx * dx
        } else {
            0.0
        };
        let tol = 1e-2 * (lin.abs() + quad.abs()) + 64.0 * f64::EPSILON * (v.abs() + v2.abs());
        if !lin.is_finite() || (v2 - v - lin - quad).abs() > tol {
            break;
        }
        out.push((x, v));
    }
    out
}

/// The piece's samples, with ladders added closing in on its finite ends
/// and on both sides of each kink inside it, so that what lies between
/// them and the nearest grid sample is seen: the maximum of √x − 1000x at
/// 2.5·10⁻⁷, the minima of x² − 0.001|x| at ±0.0005.
fn refined_samples(fun: &Fun, p: &Piece, kinks: &[f64]) -> Vec<(f64, f64)> {
    let s = &p.samples;
    let first_after = |c: f64| s.iter().find(|q| q.0 > c).map(|q| q.0);
    let last_before = |c: f64| s.iter().rev().find(|q| q.0 < c).map(|q| q.0);
    let mut extra: Vec<(f64, f64)> = Vec::new();
    if p.lo.value.is_finite()
        && let Some(a) = first_after(p.lo.value)
    {
        extra.extend(ladder(fun, p.lo.value, 1.0, a - p.lo.value));
    }
    if p.hi.value.is_finite()
        && let Some(b) = last_before(p.hi.value)
    {
        extra.extend(ladder(fun, p.hi.value, -1.0, p.hi.value - b));
    }
    for &c in kinks {
        let v = fun.f(c);
        if v.is_finite() {
            extra.push((c, v));
        }
        if let Some(b) = last_before(c) {
            extra.extend(ladder(fun, c, -1.0, c - b));
        }
        if let Some(a) = first_after(c) {
            extra.extend(ladder(fun, c, 1.0, a - c));
        }
    }
    if extra.is_empty() {
        return s.clone();
    }
    let mut all = s.clone();
    all.extend(
        extra
            .into_iter()
            .filter(|q| q.0 > p.lo.value && q.0 < p.hi.value),
    );
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    all.dedup_by(|a, b| a.0 == b.0);
    all
}

/// `xs`/`vs` with the points `at` (strictly inside, not already there)
/// merged in, valued by `g`.
fn merged(xs: &[f64], vs: &[f64], at: &[f64], g: &dyn Fn(f64) -> f64) -> (Vec<f64>, Vec<f64>) {
    let (Some(&lo), Some(&hi)) = (xs.first(), xs.last()) else {
        return (xs.to_vec(), vs.to_vec());
    };
    let mut pts: Vec<(f64, f64)> = xs.iter().copied().zip(vs.iter().copied()).collect();
    for &c in at {
        if c > lo && c < hi && xs.binary_search_by(|x| x.total_cmp(&c)).is_err() {
            pts.push((c, g(c)));
        }
    }
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    pts.dedup_by(|a, b| a.0 == b.0);
    pts.into_iter().unzip()
}

fn piece_features(fun: &Fun, p: &Piece, scale: f64, wrap_periodic: bool) -> PieceFeatures {
    let mut out = PieceFeatures::default();
    if p.samples.is_empty() {
        return out;
    }
    let grid: Vec<f64> = p.samples.iter().map(|s| s.0).collect();
    let mut kinks = Vec::new();
    for g in &fun.kinks {
        program_zeros(fun, g, &grid, &mut kinks);
    }
    kinks.retain(|&c| c > p.lo.value && c < p.hi.value);
    dedup_sorted(&mut kinks);
    kinks.truncate(4 * MAX_LISTED);
    let samples = refined_samples(fun, p, &kinks);
    let xs: Vec<f64> = samples.iter().map(|s| s.0).collect();
    let fs: Vec<f64> = samples.iter().map(|s| s.1).collect();
    let mut f = |x: f64| fun.f(x);

    // Zeros. A run of several exact-zero samples is a stretch where f
    // vanishes identically (or underflows): not a list of intercepts.
    // A run is genuine (not underflow) when a sample next to it is of
    // normal magnitude, e.g. max(x, 0) = 0 for x ≤ 0 next to x, or when f
    // is exactly 0 there in extended range, as floor(x) is on [0, 1), a
    // piece of its own between jumps.
    let normal = |v: f64| v.is_finite() && v.abs() > 1e-250;
    let mut inner_zero_run = false;
    let mut i = 0;
    while i < fs.len() {
        if fs[i] == 0.0 {
            let mut j = i;
            while j + 1 < fs.len() && fs[j + 1] == 0.0 {
                j += 1;
            }
            if j > i
                && ((i > 0 && normal(fs[i - 1]))
                    || (j + 1 < fs.len() && normal(fs[j + 1]))
                    || fun.exact_zero(xs[(i + j) / 2]))
            {
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

    // Critical points and inflections. Each order is searched through the
    // turning points of the next: f″'s sign changes are where f′ turns, so
    // f′ is sampled there too (x³/3 − 0.00015x² + 2·10⁻⁸x turns at 10⁻⁴
    // and 2·10⁻⁴, both between two grid samples, either side of its
    // inflection), and f is sampled at its extrema before its zeros are
    // searched ((x − 0.0001)(x − 0.0002) is positive at every grid sample).
    let interior: Vec<usize> = (0..xs.len())
        .filter(|&i| xs[i] > p.lo.value && xs[i] < p.hi.value)
        .collect();
    let mut ixs: Vec<f64> = interior.iter().map(|&i| xs[i]).collect();
    let mut ifs: Vec<f64> = interior.iter().map(|&i| fs[i]).collect();
    let mut crit: Vec<(f64, i8, i8)> = Vec::new();
    let mut crit_v: Vec<(f64, f64)> = Vec::new();
    // f′ at the interior samples, kept for the monotonicity pass below.
    let mut dfs_kept: Option<Vec<f64>> = None;
    if !ixs.is_empty() {
        let dfs0 = fun.batch(1, &ixs);
        let d2s = fun.batch(2, &ixs);
        // Rounding noise in f″ (e.g. for a function that is linear in
        // disguise) must not count as concavity changes: zero out values
        // below a per-sample noise floor relative to f′. Not to f: an
        // offset (x³ + 10⁶) changes nothing about how f bends.
        // And to f's size nearby, not over the whole grid: ((x − 2·10⁶)(x −
        // 2·10⁶ − 1))² is 10²⁵ at 0 but bends at 2·10⁶ + 0.21 by 0.04.
        let d2floor = |x: f64, d1: f64, fl: f64| {
            let x1 = 1.0 + x.abs();
            1e-9 * d1.abs() / x1 + 1e-14 * fl / (x1 * x1)
        };
        let on_grid0: Vec<bool> = ixs
            .iter()
            .map(|x| grid.binary_search_by(|g| g.total_cmp(x)).is_ok())
            .collect();
        let fnear: Vec<f64> = (0..ixs.len())
            .map(|i| {
                let (a, b) = (i.saturating_sub(32), (i + 33).min(ixs.len()));
                (a..b)
                    .filter(|&j| j == i || on_grid0[j])
                    .map(|j| ifs[j])
                    .filter(|v| v.is_finite())
                    .fold(0.0f64, |m, v| m.max(v.abs()))
            })
            .collect();
        let _ = scale;
        let d2c: Vec<f64> = d2s
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                if v.abs() <= d2floor(ixs[i], dfs0[i], fnear[i]) {
                    0.0
                } else {
                    v
                }
            })
            .collect();
        let mut d2 = |x: f64| fun.d2f(x);
        let (infl_c, erratic2) = sign_changes(&ixs, &d2c, &mut d2, 0.0);
        out.erratic |= erratic2;
        let mut infl_xs: Vec<f64> = infl_c.iter().map(|c| c.0).collect();

        // Neighbours are grid samples only: the ladders closing in on a pole
        // (x − 1000 + 1/(x − 1000)) have huge f′ that says nothing about
        // the noise of f′ a unit away.
        let (base_xs, base_fs) = (ixs.clone(), ifs.clone());
        let mut dfs = dfs0.clone();
        for round in 0..2 {
            let (mx, mf) = merged(&base_xs, &base_fs, &infl_xs, &|x| fun.f(x));
            dfs = if mx.len() == base_xs.len() {
                dfs0.clone()
            } else {
                fun.batch(1, &mx)
            };
            (ixs, ifs) = (mx, mf);
            // Rounding noise in f′ is judged locally, against f′ and f
            // nearby: a floor set by f′'s size over the whole grid (10³⁰ far
            // out) would erase genuine sign changes where f′ is small, as
            // between roots of ((x − 2·10⁶)(x − 2·10⁶ − 1))² a unit apart.
            const W: usize = 32;
            let on_grid: Vec<bool> = ixs
                .iter()
                .map(|x| grid.binary_search_by(|g| g.total_cmp(x)).is_ok())
                .collect();
            let biggest = |v: &[f64], a: usize, b: usize, i: usize| {
                (a..b)
                    .filter(|&j| j == i || on_grid[j])
                    .map(|j| v[j])
                    .filter(|v| v.is_finite())
                    .fold(0.0f64, |m, v| m.max(v.abs()))
            };
            let dfloor: Vec<f64> = (0..dfs.len())
                .map(|i| {
                    let (a, b) = (i.saturating_sub(W), (i + W + 1).min(dfs.len()));
                    1e-13 * biggest(&dfs, a, b, i) + 1e-18 * biggest(&ifs, a, b, i)
                })
                .collect();
            let dfs_c: Vec<f64> = dfs
                .iter()
                .zip(&dfloor)
                .map(|(&v, &fl)| if v.abs() <= fl { 0.0 } else { v })
                .collect();
            let mut df = |x: f64| fun.df(x);
            let (c, erratic1) = sign_changes(&ixs, &dfs_c, &mut df, 0.0);
            let inside = |x: f64| x > p.lo.value && x < p.hi.value && fun.f(x).is_finite();
            // A turn found next to a kink, closer than its ladders reach
            // (10⁻¹¹ of the grid gap there), is the kink: max(x, 0) turns
            // at 0, not at −7·10⁻¹⁵.
            let at_kink = |x: f64| {
                kinks.iter().copied().find(|&kx| {
                    let i = grid.partition_point(|&g| g < kx);
                    let after = grid[i.min(grid.len() - 1)..]
                        .iter()
                        .find(|&&g| g > kx)
                        .map_or(f64::INFINITY, |&g| g - kx);
                    let before = grid[..i].last().map_or(f64::INFINITY, |&g| kx - g);
                    (x - kx).abs() <= 1e-11 * after.min(before)
                })
            };
            crit = if erratic1 {
                c
            } else {
                c.into_iter()
                    .map(|(x, b, a)| {
                        let x = at_kink(x).unwrap_or_else(|| snap_root(x, &|t| fun.df(t), &inside));
                        (x, b, a)
                    })
                    .collect()
            };
            out.erratic |= erratic1;
            if round == 1 || crit.is_empty() {
                break;
            }
            // And f″ is sampled at the extrema: two inflections between grid
            // samples straddle the extremum between them
            // (((x − 1000)(x − 1001))² bends at 1000.21 and 1000.79, either
            // side of its maximum at 1000.5).
            let crit_xs: Vec<f64> = crit.iter().map(|c| c.0).collect();
            let (d2x, d2v) = merged(&base_xs, &d2c, &crit_xs, &|x| {
                let v = fun.d2f(x);
                if v.abs() <= d2floor(x, fun.df(x), fun.f(x).abs()) {
                    0.0
                } else {
                    v
                }
            });
            let (more, _) = sign_changes(&d2x, &d2v, &mut d2, 0.0);
            let more: Vec<f64> = more
                .into_iter()
                .map(|c| c.0)
                .filter(|c| {
                    !infl_xs
                        .iter()
                        .any(|o| (o - c).abs() <= 1e-9 * c.abs().max(1.0))
                })
                .collect();
            if more.is_empty() {
                break;
            }
            infl_xs.extend(more);
            infl_xs.sort_by(|a, b| a.total_cmp(b));
        }
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
            crit_v.push((c, v));
        }
        // Inflection points, each confirmed by how f itself bends within a
        // third of the way to the nearest other candidate, kink or end.
        for (i, &c0) in infl_xs.iter().enumerate() {
            let mut near = (c0 - p.lo.value).min(p.hi.value - c0);
            for (j, &o) in infl_xs.iter().enumerate() {
                if j != i {
                    near = near.min((o - c0).abs());
                }
            }
            for &o in &kinks {
                near = near.min((o - c0).abs());
            }
            let room = near / 3.0;
            let c = snap_root(c0, &|t| fun.d2f(t), &|t| {
                t > p.lo.value && t < p.hi.value && fun.f(t).is_finite()
            });
            let v = fun.f(c);
            if !v.is_finite() {
                continue;
            }
            // Not at a corner of f (f′ continuous or a vertical tangent).
            let h = (1e-4 * room)
                .min(1e-7 * c.abs().max(1.0))
                .max(16.0 * f64::EPSILON * c.abs());
            let (l, r) = (fun.df(c - h), fun.df(c + h));
            let corner = l.is_finite()
                && r.is_finite()
                && (l - r).abs() > 1e-3 * (1.0 + l.abs().max(r.abs()))
                && l.abs().max(r.abs()) < 1e6;
            if !corner && concavity_changes(fun, c, room) {
                out.inflections.push((c, v));
            }
        }
        dfs_kept = Some(dfs);
    }

    // Zeros: sign changes over the samples and the extrema.
    let crit_xs: Vec<f64> = crit_v.iter().map(|c| c.0).collect();
    let (zx, zf) = merged(&xs, &fs, &crit_xs, &|x| fun.f(x));
    let (zero_crossings, erratic0) = sign_changes(&zx, &zf, &mut f, 0.0);
    out.erratic |= erratic0;
    let in_piece = |x: f64| {
        (x > p.lo.value || (p.lo.closed && x == p.lo.value))
            && (x < p.hi.value || (p.hi.closed && x == p.hi.value))
            && fun.f(x).is_finite()
    };
    for (x, _, _) in zero_crossings {
        let x = if erratic0 {
            x
        } else {
            snap_root(x, &|t| fun.f(t), &in_piece)
        };
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
    // An extremum whose value is 0 to within noise touches the axis (a
    // double root).
    for &(c, v) in &crit_v {
        if v.abs() <= value_noise(fun, c, v)
            && !out
                .zeros
                .iter()
                .any(|z| (z - c).abs() <= 1e-7 * c.abs().max(1.0))
        {
            out.zeros.push(c);
        }
    }

    // Monotonicity: split at critical points; directions from the f′
    // samples inside each sub-interval (found by binary search).
    let dvals: Vec<f64> = dfs_kept.unwrap_or_default();
    let ivals: Vec<f64> = ifs;
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
            let ms: Vec<f64> = if a.is_finite() && b.is_finite() {
                (1..=5).map(|k| a + (b - a) * k as f64 / 6.0).collect()
            } else if a.is_finite() {
                vec![a + 1.0]
            } else if b.is_finite() {
                vec![b - 1.0]
            } else {
                vec![0.0]
            };
            (
                ms.iter().map(|&m| fun.df(m)).collect(),
                ms.iter().map(|&m| fun.f(m)).collect(),
            )
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
        // Flat where f′ can't move f by more than rounding across the
        // stretch (f′ exactly 0, as for floor(x), or the noise of a
        // constant in disguise), judged against f here: 1/(x³ − 10³⁰) is
        // tiny everywhere but not constant, and ((x − 1000)(x − 1001))²
        // isn't constant between its roots however small next to f(0).
        let fmag = fin.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let flat = dmax == 0.0 || dmax * (b - a) <= 1e-9 * fmag;
        let dir = if flat && fspan <= 1e-9 * fmag {
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
    let mut isolated_ends: Vec<f64> = Vec::new();
    for &(x, v) in out.minima.iter().chain(out.maxima.iter()) {
        acc.consider(v, Src::At(x));
    }
    for (end, side) in [(p.lo, 1.0), (p.hi, -1.0)] {
        if end.value.is_infinite() {
            // Sample past the piece's other end, inside the piece.
            let other = if side > 0.0 { p.hi.value } else { p.lo.value };
            let exact = fun.tails[usize::from(end.value > 0.0)];
            let limit = match exact {
                Some(c) => (SeqLimit::Converges(c), 0.0),
                None => limit_at_infinity_beyond_err(&mut f, end.value.signum(), other),
            };
            match limit {
                (SeqLimit::Converges(l), err) => acc.consider(l, Src::Limit(err)),
                (SeqLimit::PosInf, _) => acc.consider(f64::INFINITY, Src::Limit(0.0)),
                (SeqLimit::NegInf, _) => acc.consider(f64::NEG_INFINITY, Src::Limit(0.0)),
                (SeqLimit::Unknown, _) => {
                    // Oscillating tails: check whether the extremes keep growing.
                    let sgn = end.value.signum();
                    let far: Vec<f64> = (0..400)
                        .map(|i| f(sgn * (1e3 + i as f64 * 2.4937e3)))
                        .collect();
                    let near: Vec<f64> = (0..400)
                        .map(|i| f(sgn * (1e2 + i as f64 * 2.4937e2)))
                        .collect();
                    // A steady tail whose limit couldn't be pinned down (1/ln x
                    // creeps to 0): that end of the range is unknown, not the
                    // last sample's value.
                    let fin: Vec<f64> = far.iter().copied().filter(|v| v.is_finite()).collect();
                    if monotone(&fin) {
                        out.range_unsure = true;
                    }
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
                        acc.consider(f64::INFINITY, Src::Limit(0.0));
                    }
                    if mn(&far) == f64::NEG_INFINITY || mn(&far) < -2.0 * mn(&near).abs().max(scale)
                    {
                        acc.consider(f64::NEG_INFINITY, Src::Limit(0.0));
                    }
                }
            }
        } else {
            let v = if end.closed || wrap_periodic {
                fun.f(end.value)
            } else {
                f64::NAN
            };
            // f jumps at a closed end (0^x at 0): that value is a point of
            // the range of its own; the piece's values approach the limit.
            let width = p.hi.value - p.lo.value;
            let jump = (v.is_finite() && end.closed && !wrap_periodic && width > 0.0)
                .then(|| one_sided_limit_err(&mut f, end.value, side, width))
                .and_then(|(l, err, _)| match l {
                    SeqLimit::Converges(l)
                        if (l - v).abs() > 1e-6 * v.abs().max(l.abs()).max(scale) + 16.0 * err =>
                    {
                        Some((l, err))
                    }
                    _ => None,
                });
            if let Some((l, err)) = jump {
                out.range_points.push(v);
                isolated_ends.push(end.value);
                acc.consider(l, Src::Limit(err));
            } else if v.is_finite() {
                acc.consider(v, Src::At(end.value));
            } else {
                match one_sided_limit_err(&mut f, end.value, side, p.hi.value - p.lo.value) {
                    (SeqLimit::Converges(l), err, _) => acc.consider(l, Src::Limit(err)),
                    (SeqLimit::PosInf, ..) => acc.consider(f64::INFINITY, Src::Limit(0.0)),
                    (SeqLimit::NegInf, ..) => acc.consider(f64::NEG_INFINITY, Src::Limit(0.0)),
                    // Creeping towards a limit the samples can't pin down
                    // (1/asech(x) at 0, logarithmically): that end of the
                    // range is unknown, not the last sample's value. An
                    // oscillating approach adds nothing the samples and
                    // extrema don't already give.
                    (SeqLimit::Unknown, _, monotone) => {
                        if monotone {
                            out.range_unsure = true;
                        }
                    }
                }
            }
        }
    }
    // Samples catch anything the candidates missed (strictly beyond only).
    let tol = 1e-9 * scale;
    let samples: Vec<(f64, f64)> = samples
        .into_iter()
        .filter(|q| !isolated_ends.contains(&q.0))
        .collect();
    for &(x, v) in &samples {
        if v.is_finite() && (v < acc.lo.value - tol || v > acc.hi.value + tol) && x.abs() < 1e4 {
            acc.consider(v, Src::At(x));
        }
    }
    // A bound that is only a limit can still be attained on a constant
    // stretch (max(x, 0) = 0 for x ≤ 0), which only a piecewise f has, or
    // one constant throughout ((1/x)⁰, 1^(1/x)): an analytic one that
    // underflows onto its limit (e^(x − 1000) − 2) never attains it. Only moderate x count: far out, rounding makes
    // max(1 − e^(−x), 0) equal its limit 1.
    let all_same = samples.iter().all(|q| q.1 == samples[0].1);
    let flat_capable = fun.piecewise
        || fun
            .df
            .as_ref()
            .is_some_and(|p| p.as_constant() == Some(0.0))
        || all_same;
    for &(x, v) in samples.iter().filter(|_| flat_capable) {
        // Constant stretches have f′ exactly 0 (tanh(20) rounds to 1 but
        // its derivative does not vanish), or one undefined by its form
        // when f is the same throughout (0^(−x) = 0 for x < 0, but its
        // f′ involves ln 0).
        let d = fun.df(x);
        if x.abs() > 20.0 || !(d == 0.0 || (d.is_nan() && all_same)) {
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
    // A piece of the domain has values: nothing found, or a single value
    // that isn't attained, means they couldn't be measured (1/x^400 is
    // beyond 10³⁰⁸ or below 10⁻³⁰⁸ almost everywhere), not that there are
    // none.
    let attained_point = acc.lo.value == acc.hi.value && acc.lo.closed && acc.hi.closed;
    if p.lo.value < p.hi.value && !(acc.lo.value < acc.hi.value || attained_point) {
        out.range_unsure = true;
    }
    if acc.lo.value <= acc.hi.value {
        out.range = Some(Interval {
            lo: acc.lo,
            hi: acc.hi,
        });
        let noise = |src: Src, v: f64| match src {
            Src::At(x) => value_noise(fun, x, v),
            Src::Limit(e) => e,
        };
        out.range_noise = (
            noise(acc.lo_src, acc.lo.value),
            noise(acc.hi_src, acc.hi.value),
        );
    }
    out
}

/// Confirms an inflection candidate with second differences of f itself
/// (rejects sign changes of f″ that are only rounding noise): f bends
/// opposite ways either side of `c`, by more than f's own rounding noise
/// there, at some step no wider than `room`. Judged locally, so an offset
/// (x³ + 10⁶) or a shift ((x − 1000)·e^(−(x−1000)²)) changes nothing.
fn concavity_changes(fun: &Fun, c: f64, room: f64) -> bool {
    let mut h = room.min(1e-2 * c.abs().max(1.0));
    let f = |x: f64| fun.f(x);
    let f0 = f(c);
    // How rough f's evaluation is here: third differences at a step far
    // below h, where a smooth f moves by f‴·(h/64)³ only but rounding
    // (cancellation in (x² − 1)/(x − 1) next to 1) as much as at any
    // step. Measured at the first, widest step too: at a tiny one,
    // neighbouring roundings agree and hide it.
    let rough_at = |d: f64| {
        let w = [-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0].map(|k| f(c + k * d));
        (0..4)
            .map(|i| (w[i + 3] - 3.0 * w[i + 2] + 3.0 * w[i + 1] - w[i]).abs())
            .fold(0.0f64, f64::max)
    };
    let rough0 = rough_at(h / 64.0);
    for _ in 0..12 {
        if h.is_nan() || h <= 0.0 || c - h == c || c + h == c {
            return false;
        }
        let (l2, l1, r1, r2) = (f(c - 2.0 * h), f(c - h), f(c + h), f(c + 2.0 * h));
        let left = f0 - 2.0 * l1 + l2;
        let right = r2 - 2.0 * r1 + f0;
        let rough = rough0.max(rough_at(h / 64.0));
        let noise = 4.0 * value_noise(fun, c, f0)
            + 2.0 * rough
            + 16.0
                * f64::EPSILON
                * (l2.abs() + 2.0 * l1.abs() + 2.0 * f0.abs() + 2.0 * r1.abs() + r2.abs());
        if left.is_finite()
            && right.is_finite()
            && noise.is_finite()
            && (left < 0.0) != (right < 0.0)
            && left.abs() > noise
            && right.abs() > noise
        {
            return true;
        }
        h /= 4.0;
    }
    false
}

/// Where a range bound comes from: a value attained at x, or a limit known
/// to within an error.
#[derive(Clone, Copy)]
enum Src {
    At(f64),
    Limit(f64),
}

struct RangeAcc {
    lo: Bound,
    hi: Bound,
    lo_src: Src,
    hi_src: Src,
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
            lo_src: Src::Limit(0.0),
            hi_src: Src::Limit(0.0),
        }
    }

    fn consider(&mut self, v: f64, src: Src) {
        if v.is_nan() {
            return;
        }
        let attained = matches!(src, Src::At(_));
        if v < self.lo.value || (v == self.lo.value && attained) {
            self.lo = Bound {
                value: v,
                closed: attained && v.is_finite(),
            };
            self.lo_src = src;
        }
        if v > self.hi.value || (v == self.hi.value && attained) {
            self.hi = Bound {
                value: v,
                closed: attained && v.is_finite(),
            };
            self.hi_src = src;
        }
    }
}

/// Union of intervals (sorted, merging overlaps and touching ends). Empty
/// ones, such as (∞, ∞) from a piece whose ends both diverge upwards, are
/// dropped.
fn union(mut parts: Vec<Interval>) -> Vec<Interval> {
    parts.retain(|p| {
        p.lo.value < p.hi.value
            || (p.lo.value == p.hi.value && p.lo.closed && p.hi.closed && p.lo.value.is_finite())
    });
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

/// The range as a set: the pieces' ranges with each bound cleared of its
/// own noise ([`clean`]: an attained value's [`value_noise`] where it is
/// attained, a limit's error), then joined. A genuine tiny bound keeps its
/// value and sign: the minimum 10⁻⁷ of x² + 10⁻⁷, the maximum −2.5·10⁻⁹ of
/// 1/(x² − 4·10⁸) beside a positive branch.
fn range_set(parts: Vec<(Interval, (f64, f64))>) -> Vec<Interval> {
    let tidy = |b: Bound, noise: f64| Bound {
        value: clean(b.value, noise),
        closed: b.closed,
    };
    union(
        parts
            .into_iter()
            .map(|(i, (nl, nh))| Interval {
                lo: tidy(i.lo, nl),
                hi: tidy(i.hi, nh),
            })
            .collect(),
    )
}

/// A value of f for display: noise already gone (see [`clean`]), so a
/// tiny remaining value is genuine and isn't shown as 0.
fn fmt_y(v: f64) -> String {
    format_nonzero(v)
}

/// `(x, y)` for an extremum or inflection point; y as [`fmt_y`].
fn fmt_point(x: f64, y: f64) -> String {
    format!("({}, {})", format_number(x), fmt_y(y))
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

/// Horizontal and oblique asymptotes at the enabled ends, and whether an
/// end's behaviour couldn't be determined (then they are unknown).
fn asymptotes_at_infinity(
    fun: &Fun,
    domain: &[Interval],
    samples: &[(f64, f64)],
) -> (Horizontal, Oblique, bool) {
    let to_neg = domain
        .first()
        .is_some_and(|d| d.lo.value == f64::NEG_INFINITY);
    let to_pos = domain.last().is_some_and(|d| d.hi.value == f64::INFINITY);
    // Where the last stretch of the domain towards each end starts: the
    // tail is sampled beyond it (1/(√(x/10⁶) − 1) has a pole at 10⁶).
    let start = |sign: f64| {
        let b = if sign > 0.0 {
            domain.last().map(|d| d.lo.value)
        } else {
            domain.first().map(|d| d.hi.value)
        };
        b.filter(|v| v.is_finite()).unwrap_or(0.0)
    };
    let mut horiz: Vec<(f64, AsymptoteSide)> = Vec::new();
    let mut obl: Vec<(f64, f64, AsymptoteSide)> = Vec::new();
    let mut unsure = false;
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
        // Only a piecewise f can equal a line on its tail without being
        // that line everywhere, so an analytic one must be on it at every
        // sample: x + e^(−x²) and (x − 1000)·e^(−(x−1000)²) merely round or
        // underflow onto theirs, x/x and (x² − 1)/(x − 1) are theirs.
        let everywhere = |m: f64, b: f64| {
            fun.piecewise
                || samples.iter().all(|&(x, v)| {
                    let line = m * x + b;
                    (v - line).abs() <= 1e-9 * v.abs().max((m * x).abs()).max(b.abs())
                })
        };
        let coincides = |f: &mut dyn FnMut(f64) -> f64, m: f64, b: f64| {
            everywhere(m, b)
                && [10.0, 100.0, 1000.0].iter().all(|&t| {
                    let x = sign * t;
                    let v = f(x);
                    let line = m * x + b;
                    (v - line).abs()
                        <= 64.0 * f64::EPSILON * v.abs().max((m * x).abs()).max(b.abs())
                })
        };
        let from = start(sign);
        let exact = fun.tails[usize::from(sign > 0.0)];
        let limit = match exact {
            Some(c) => (SeqLimit::Converges(c), 0.0),
            None => limit_at_infinity_beyond_err(&mut f, sign, from),
        };
        match limit {
            (SeqLimit::Converges(l), err) => {
                let l = if exact.is_some() {
                    l
                } else {
                    clean_level(l, err)
                };
                if !coincides(&mut f, 0.0, l) {
                    horiz.push((l, side));
                }
            }
            (SeqLimit::PosInf | SeqLimit::NegInf, _) => {
                let first = ((2.0 * from.abs()).max(10.0).log10().ceil() as i32).max(1);
                let xs: Vec<f64> = (first..first + 15).map(|k| sign * 10f64.powi(k)).collect();
                // With each value's rounding: f(x) − m·x cancels.
                let (fv, fnoise) = sample(&mut f, &xs);
                let ms: Vec<f64> = xs.iter().zip(&fv).map(|(&x, &v)| v / x).collect();
                let msn: Vec<f64> = xs.iter().zip(&fnoise).map(|(&x, &n)| n / x.abs()).collect();
                if let (SeqLimit::Converges(m), em) = sequence_limit_noisy(&ms, &msn) {
                    let m = clean(m, em.max(1e-12 * m.abs()));
                    if m != 0.0 {
                        let bs: Vec<f64> = (0..10).map(|i| fv[i] - m * xs[i]).collect();
                        let bsn: Vec<f64> = (0..10)
                            .map(|i| fnoise[i] + 4.0 * f64::EPSILON * (m * xs[i]).abs())
                            .collect();
                        match sequence_limit_noisy(&bs, &bsn) {
                            (SeqLimit::Converges(b), eb) => {
                                let b = clean(b, eb);
                                if !coincides(&mut f, m, b) {
                                    obl.push((m, b, side));
                                }
                            }
                            // Growing like m·x with an offset that creeps
                            // (x + ln x would): unknown. One that swings
                            // (x + sin x) has no asymptote.
                            _ => {
                                let fin: Vec<f64> =
                                    bs.iter().copied().filter(|v| v.is_finite()).collect();
                                if monotone(&fin) {
                                    unsure = true;
                                }
                            }
                        }
                    }
                }
            }
            (SeqLimit::Unknown, _) => {
                // A steady tail whose limit couldn't be pinned down (1/ln x):
                // whether it levels off is unknown, not "no asymptote".
                let tail: Vec<f64> = (1..=17)
                    .map(|k| f(sign * 10f64.powi(k)))
                    .filter(|v| v.is_finite())
                    .collect();
                if monotone(&tail) {
                    unsure = true;
                }
            }
        }
    }
    // Merge identical asymptotes on both sides.
    if horiz.len() == 2
        && (horiz[0].0 - horiz[1].0).abs() <= 1e-9 * horiz[0].0.abs().max(horiz[1].0.abs())
    {
        horiz = vec![(horiz[0].0, AsymptoteSide::AnyInfinity)];
    }
    if obl.len() == 2
        && (obl[0].0 - obl[1].0).abs() <= 1e-9 * obl[0].0.abs().max(1.0)
        && (obl[0].1 - obl[1].1).abs() <= 1e-7 * obl[0].1.abs().max(1.0)
    {
        obl = vec![(obl[0].0, obl[0].1, AsymptoteSide::AnyInfinity)];
    }
    (horiz, obl, unsure)
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
    if b == 0.0 {
        format!("y = {mt}")
    } else if b < 0.0 {
        format!("y = {mt} {MINUS} {}", format_nonzero(-b))
    } else {
        format!("y = {mt} + {}", format_nonzero(b))
    }
}

// ---------------------------------------------------------------------------
// Non-periodic analysis.

fn analyze_aperiodic(
    fun: &Fun,
    gens: &[Program],
    beyond: &Beyond,
    scale: f64,
) -> Result<KeyGraphFeatures, AnalysisError> {
    // Widen the window to take in whatever lies beyond it, keeping the
    // grid as dense near 0, and as dense again around each feature out
    // there (x + 1/(x − 2·10⁶) turns at 2·10⁶ ± 1).
    // A feature exactly at the base window's edge ((x − 10⁶)² + 1 turns at
    // 10⁶, the last grid point) needs grid beyond it too.
    let xs = if beyond.extent >= WINDOW {
        let w = (4.0 * beyond.extent).min(4.0 * REACH);
        let n = (N_HALF as f64 * w.asinh() / WINDOW.asinh()).ceil() as usize;
        let mut xs = sinh_grid(w, n);
        for &c in &beyond.features {
            xs.extend(
                sinh_grid(0.5 * c.abs(), beyond.local_half)
                    .into_iter()
                    .map(|d| c + d),
            );
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        xs.dedup_by(|a, b| (*a - *b).abs() <= 1e-12 * a.abs().max(1.0));
        xs
    } else {
        sinh_grid(WINDOW, N_HALF)
    };
    let sc = scan(fun, &xs, &beyond.candidates, gens, true, true, scale);
    if sc.pieces.is_empty() {
        return Err(AnalysisError::AnalysisCouldNotBePerformed);
    }
    let mut k = KeyGraphFeatures::default();
    let mut too = 0u32;
    if sc.too_complex {
        too |= flags::RANGE | flags::MONOTONE_INTERVALS;
    }
    if isolated_powers(fun, &xs) {
        too |= flags::DOMAIN | flags::RANGE | flags::ZEROS;
    }
    if sc.overflow {
        too |= flags::ZEROS
            | flags::MINIMA
            | flags::MAXIMA
            | flags::INFLECTION_POINTS
            | flags::MONOTONE_INTERVALS
            | flags::RANGE;
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
        if pf.range_unsure {
            too |= flags::RANGE;
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
            ranges.push((r, pf.range_noise));
        }
        for &v in &pf.range_points {
            let at = Bound {
                value: v,
                closed: true,
            };
            ranges.push((Interval { lo: at, hi: at }, (0.0, 0.0)));
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
    for p in minima
        .iter_mut()
        .chain(maxima.iter_mut())
        .chain(infl.iter_mut())
    {
        p.1 = clean(p.1, value_noise(fun, p.0, p.1));
    }
    // Already moved to closed forms within their own uncertainty (see
    // `snap_root`): a zero at 10⁻⁹ (sin x − 10⁻⁹) is not 0.
    let zeros: Vec<f64> = zeros;

    if zeros.len() > MAX_LISTED || too & flags::ZEROS != 0 {
        too |= flags::ZEROS;
    } else {
        // A zero beside a pole (1/(x − 2) + 10⁶ at 1.999999) or another
        // zero keeps the digits that tell them apart.
        let near: Vec<f64> = (zeros.iter())
            .chain(&sc.poles)
            .chain(&sc.excluded)
            .copied()
            .collect();
        k.x_intercept = zeros
            .iter()
            .map(|&z| format_number_apart(z, &near))
            .collect::<Vec<_>>()
            .join(", ");
    }
    if minima.len() > MAX_LISTED {
        too |= flags::MINIMA;
    } else {
        k.minima = minima.iter().map(|&(x, y)| fmt_point(x, y)).collect();
    }
    if maxima.len() > MAX_LISTED {
        too |= flags::MAXIMA;
    } else {
        k.maxima = maxima.iter().map(|&(x, y)| fmt_point(x, y)).collect();
    }
    if infl.len() > MAX_LISTED {
        too |= flags::INFLECTION_POINTS;
    } else {
        k.inflection_points = infl.iter().map(|&(x, y)| fmt_point(x, y)).collect();
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

    let range = range_set(ranges);
    if range.len() > 8 || too & flags::RANGE != 0 {
        too |= flags::RANGE;
    } else {
        k.range = format_set_with("y", &range, &fmt_y);
    }

    // y-intercept.
    let y0 = fun.f(0.0);
    let zero_excluded = sc.excluded.contains(&0.0) || sc.poles.contains(&0.0);
    let y_intercept = if y0.is_finite() && !zero_excluded {
        Some(clean(y0, value_noise(fun, 0.0, y0)))
    } else {
        None
    };
    // f(0) exists but is beyond a double's range ((x − 1000)e^(1000 − x)).
    if y0.is_infinite() && !zero_excluded {
        too |= flags::Y_INTERCEPT;
    }
    if let Some(y) = y_intercept {
        k.y_intercept = fmt_y(y);
    }

    // Asymptotes at infinity.
    let samples: Vec<(f64, f64)> = sc
        .pieces
        .iter()
        .flat_map(|p| p.samples.iter().copied())
        .collect();
    let (horiz, obl, unsure) = asymptotes_at_infinity(fun, &domain, &samples);
    if unsure {
        too |= flags::HORIZONTAL_ASYMPTOTES | flags::OBLIQUE_ASYMPTOTES;
    }
    k.horizontal_asymptotes = horiz
        .iter()
        .map(|(v, _)| format!("y = {}", fmt_y(*v)))
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
    // Reducing x leaves rounding of order p·ε, nothing more: a zero at
    // 10⁻⁹ (sin x − 10⁻⁹) is not kπ.
    let s = snap(r, 1e-9);
    let r = if (s - r).abs() <= 1e-12 * r.abs().max(p) {
        s
    } else {
        r
    };
    if r <= -p / 2.0 + 1e-12 * p { r + p } else { r }
}

/// Groups representatives (mod p) into families, merging evenly spaced
/// ones into a finer family (sin x zeros 0 and π → kπ).
fn families(mut reps: Vec<f64>, p: f64) -> Vec<Family> {
    // Positions are found to rounding: zeros of sin(x) + 10⁻⁷ at −10⁻⁷ and
    // π + 10⁻⁷ are not one family kπ − 10⁻⁷.
    let tol = 1e-10 * p;
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

/// Zeros as a list, each with the digits that tell it apart from the
/// other zeros and from `others` (poles, excluded points).
/// Whether f is undefined somewhere a power with a varying exponent has a
/// negative base: x^x is defined at x = −1, −2, … (and wherever x is a
/// fraction with an odd denominator) between the samples, so its domain,
/// range and zeros aren't what the samples show.
fn isolated_powers(fun: &Fun, xs: &[f64]) -> bool {
    if fun.power_bases.is_empty() {
        return false;
    }
    let fs = fun.batch(0, xs);
    let mut bs = vec![0.0; xs.len()];
    fun.power_bases.iter().any(|b| {
        fun.eval_batch(b, xs, &mut bs);
        bs.iter().zip(&fs).any(|(b, f)| *b < 0.0 && f.is_nan())
    })
}

fn fmt_zero_families(zeros: &[Family], others: &[Family]) -> String {
    let near: Vec<f64> = zeros.iter().chain(others).map(|f| f.x).collect();
    zeros
        .iter()
        .map(|z| match z.period {
            Some(p) => format_family_apart(z.x, p, &near),
            None => format_number_apart(z.x, &near),
        })
        .collect::<Vec<_>>()
        .join(", ")
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
    let mut zero_interval = false;
    for pc in &sa.pieces {
        // Only interior features of pieces count (window cuts are artificial).
        let pf = piece_features(fun, pc, scale, false);
        zero_interval |= pf.zero_interval;
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
    // A stretch where f is 0 isn't a list of intercepts.
    if zero_interval {
        too |= flags::ZEROS;
    }
    if isolated_powers(fun, &xs) {
        too |= flags::DOMAIN | flags::RANGE | flags::ZEROS;
    }
    if sc.overflow {
        too |= flags::ZEROS
            | flags::MINIMA
            | flags::MAXIMA
            | flags::INFLECTION_POINTS
            | flags::MONOTONE_INTERVALS
            | flags::RANGE;
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
        if pf.range_unsure {
            too |= flags::RANGE;
        }
        if let Some(r) = pf.range {
            ranges.push((r, pf.range_noise));
        }
        for &v in &pf.range_points {
            let at = Bound {
                value: v,
                closed: true,
            };
            ranges.push((Interval { lo: at, hi: at }, (0.0, 0.0)));
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
            let y = clean(y, value_noise(fun, x, y));
            match groups
                .iter_mut()
                .find(|g| (g.0 - y).abs() <= 1e-9 * y.abs().max(g.0.abs()))
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
            .map(|(f, y)| format!("({}, {}), k ∈ ℤ", fmt_family(f), fmt_y(*y)))
            .collect()
    };
    if !zero_f.is_empty() {
        let others: Vec<Family> = pole_f.iter().chain(&excluded_fam).copied().collect();
        k.x_intercept = format!("{}, k ∈ ℤ", fmt_zero_families(&zero_f, &others));
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
    let range = range_set(ranges);
    if range.len() > 8 || too & flags::RANGE != 0 {
        too |= flags::RANGE;
    } else {
        k.range = format_set_with("y", &range, &fmt_y);
    }

    let y0 = fun.f(0.0);
    let zero_excluded = excluded_fam
        .iter()
        .chain(pole_f.iter())
        .any(|f| f.contains(0.0, 1e-9));
    let y_intercept = if y0.is_finite() && !zero_excluded {
        Some(clean(y0, value_noise(fun, 0.0, y0)))
    } else {
        None
    };
    if y0.is_infinite() && !zero_excluded {
        too |= flags::Y_INTERCEPT;
    }
    if let Some(y) = y_intercept {
        k.y_intercept = fmt_y(y);
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
