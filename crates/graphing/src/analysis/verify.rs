//! Checks of an analysis against its function, and the runtime gate built
//! on them: a claim the function's own values contradict is not shown; its
//! row says "Unable to calculate …" instead.
//!
//! The truth is the function itself ([`super::truth`]): the compiled program,
//! with the reference evaluator deciding where that gives 0, ±∞ or NaN
//! (undefined, exactly 0, or a value beyond the doubles). Every tolerance is
//! local, and these are all of them:
//!
//! * `noise(x)`: f's change to its nearest other value at least two floats
//!   either side of x (a feature at the neighbouring float is as good as the
//!   floats allow; in a shifted frame f only changes every so many floats),
//!   plus a first-order bound on the rounding of each of f's operations at
//!   x, propagated through the tree. Values closer than that can't be told
//!   apart.
//! * `resolution(x)`: how far f's argument must move for f to change. Which
//!   side of a domain boundary x is on, and whether a sign change is a
//!   crossing, a pole or a jump, are judged at that scale.
//! * Points within four floats (or the rounding of a periodic family's
//!   copies) of a reported end or excluded point: the precision of that
//!   end, not a claim about the point.
//! * `same_shown`: a value that reads the same in the panel's 6 significant
//!   digits is the same claim, because the panel shows exactly that text
//!   (545843449.45 reported as 545843449 reads "545843449" either way).
//!   Only for values (range bounds, extremum values, y-intercepts,
//!   asymptote values); shapes (pieces, open or closed, constant or not, a
//!   point or an interval) and positions get no such allowance.
//! * x + P is rounded and P stands for the true period to half an ulp: the
//!   period check compares f across that reach.
//!
//! Nothing is relative to the function's size elsewhere, to a shift or an
//! offset, or to anything the analysis engine estimates.
//!
//! The checks sample where they are told to ([`gate`] picks points for the
//! app; the sweep, `examples/sweep.rs`, many more) and can only refute: a
//! claim that passes is not thereby proved. What sampling can't refute,
//! [`gate`] makes unknown by rule.

#![allow(missing_docs)]

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use super::format::format_decimal;
use super::truth::*;
use super::{
    AnalysisData, AnalysisError, AsymptoteSide, Family, Interval, KeyGraphFeatures, Monotonicity,
    Parity, flags,
};
use crate::ast::Expr;
use crate::compile::{CompileOptions, Program};
use crate::functions::TrigUnit;

/// A function and what its analysis claims, with the means to check one
/// against the other. Every evaluation is charged to a budget; once it is
/// spent (or the caller cancels), [`Analysed::over`] says so and the checks
/// stop early.
pub struct Analysed<'c> {
    /// The function as text (for reports).
    pub expr: String,
    /// What the analysis claims.
    pub k: KeyGraphFeatures,
    /// The function, slider variables bound to their values.
    pub ast: Expr,
    pub unit: TrigUnit,
    /// An open range bound gets no `same_shown` allowance: it is a value
    /// f never takes (a limit), not a value rounded for display, and a
    /// sample past it by more than the noise contradicts it (the gate).
    pub strict: bool,
    f: Option<Program>,
    cost: u64,
    spent: Cell<u64>,
    limit: u64,
    cancel: Option<&'c AtomicBool>,
    cancelled: Cell<bool>,
    noises: std::cell::RefCell<std::collections::HashMap<u64, f64>>,
}

impl<'c> Analysed<'c> {
    /// `ast` is the right-hand side of `y = …`, `k` its analysis.
    pub fn new(
        expr: impl Into<String>,
        k: KeyGraphFeatures,
        ast: &Expr,
        opts: &CompileOptions<'_>,
    ) -> Analysed<'c> {
        let ast = bind_variables(ast, opts);
        let plain = CompileOptions {
            trig_unit: opts.trig_unit,
            ..CompileOptions::default()
        };
        let f = Program::compile(&ast, &plain).ok();
        Analysed {
            expr: expr.into(),
            k,
            ast,
            unit: opts.trig_unit,
            strict: false,
            cost: f.as_ref().map_or(1, |p| p.cost().max(1) as u64),
            f,
            spent: Cell::new(0),
            limit: u64::MAX,
            cancel: None,
            cancelled: Cell::new(false),
            noises: Default::default(),
        }
    }
    /// At most `limit` units of work (a program's cost per evaluation),
    /// polling `cancel`.
    pub fn with_budget(mut self, limit: u64, cancel: Option<&'c AtomicBool>) -> Analysed<'c> {
        self.limit = limit;
        self.cancel = cancel;
        self
    }
    /// The budget is spent, or the caller cancelled.
    pub fn over(&self) -> bool {
        self.spent.get() > self.limit || self.cancelled.get()
    }
    /// The caller cancelled.
    pub fn cancelled(&self) -> bool {
        self.cancelled.get()
    }
    /// Work done so far.
    pub fn spent(&self) -> u64 {
        self.spent.get()
    }
    fn charge(&self, units: u64) {
        let before = self.spent.get();
        let spent = before + units;
        self.spent.set(spent);
        if (before >> 14) != (spent >> 14)
            && let Some(c) = self.cancel
            && c.load(Ordering::Relaxed)
        {
            self.cancelled.set(true);
        }
    }
    /// The compiled program's value.
    pub fn raw(&self, x: f64) -> f64 {
        self.charge(self.cost);
        match &self.f {
            Some(f) => f.eval(x, 0.0),
            None => f64::NAN,
        }
    }
    pub fn reference(&self, x: f64) -> R {
        self.charge(4 * self.cost);
        reval(&self.ast, x, self.unit)
    }
    /// The value at x by the plain tree, and the bound on its rounding
    /// ([`eb`]).
    pub fn bound(&self, x: f64) -> (f64, f64) {
        self.charge(8 * self.cost);
        eb(&self.ast, x, self.unit)
    }
    /// [`Analysed::bound`] for a part of the function.
    pub fn bound_of(&self, e: &Expr, x: f64) -> (f64, f64) {
        self.charge(8 * self.cost);
        eb(e, x, self.unit)
    }
    /// f(x): the compiled value, unless that is 0 or not finite, where the
    /// reference decides: undefined (NaN), exactly 0, or a nonzero value
    /// beyond the doubles (then the smallest or largest double of its sign).
    pub fn eval(&self, x: f64) -> f64 {
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
    pub fn defined(&self, x: f64) -> Option<bool> {
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
    pub fn resolution(&self, x: f64) -> f64 {
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
    pub fn rounding(&self, x: f64) -> f64 {
        let y = self.raw(x);
        if !y.is_finite() {
            return f64::INFINITY;
        }
        let (r, err) = self.bound(x);
        err + if r.is_finite() { (r - y).abs() } else { 0.0 }
    }
    /// Certainly exactly 0 at x: no operation on the way rounded.
    pub fn exact_zero(&self, x: f64) -> bool {
        self.eval(x) == 0.0 && self.bound(x).1 == 0.0
    }
    /// How far f's double value at x can be from the function's value
    /// there, as far as a claim can tell: f's change to its nearest other
    /// values either side, at least two floats out (a feature placed at a
    /// neighbouring float is as good as the floats allow; in a shifted
    /// frame, sin(x − 1000), f only changes every thousand floats of x),
    /// plus the bound on the rounding of f's operations at x. Local by
    /// construction.
    pub fn noise(&self, x: f64) -> f64 {
        if let Some(&n) = self.noises.borrow().get(&x.to_bits()) {
            return n;
        }
        let n = self.noise_at(x);
        self.noises.borrow_mut().insert(x.to_bits(), n);
        n
    }
    fn noise_at(&self, x: f64) -> f64 {
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
        let (r, err) = self.bound(x);
        // The compiler folds and rewrites (a/bᵏ as a·b⁻ᵏ): its double can
        // differ from the plain tree's by their roundings.
        let lowering = if r.is_finite() { (r - y).abs() } else { 0.0 };
        spread + err + lowering
    }
    pub fn ok(&self) -> bool {
        self.k.analysis_error == AnalysisError::NoError
    }
    pub fn unknown(&self, flag: u32) -> bool {
        self.k.too_complex_features & flag != 0
    }
    pub fn d(&self) -> &AnalysisData {
        &self.k.data
    }
    pub fn period(&self) -> Option<f64> {
        self.d().period.filter(|p| p.is_finite() && *p > 0.0)
    }
    /// The x-intercepts are every x of the domain ("x ∈ ℝ").
    pub fn zero_everywhere(&self) -> bool {
        self.k.x_intercept == "x ∈ ℝ"
    }
    /// The x-intercepts are given as a set of intervals, not points.
    pub fn zero_set(&self) -> bool {
        self.k.x_intercept.starts_with("x ∈")
    }
    /// An x-intercept set this checker reads: ℝ, or ℝ without some points
    /// ("x ∈ ℝ \ {0}"); the points left out. None for any other set.
    pub fn zero_set_except(&self) -> Option<Vec<f64>> {
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
    pub fn copies(&self, f: &Family) -> Vec<f64> {
        match f.period {
            Some(p) if p.is_finite() && p > 0.0 => {
                vec![f.x, f.x + p, f.x - p, f.x + 7.0 * p]
            }
            _ => vec![f.x],
        }
    }
}

/// The member of a family nearest v.
pub fn nearest(f: &Family, v: f64) -> f64 {
    match f.period {
        Some(p) if p.is_finite() && p > 0.0 => f.x + ((v - f.x) / p).round() * p,
        _ => f.x,
    }
}

// ---------------------------------------------------------------------------
// Sets.

pub fn iv_has(iv: &Interval, v: f64) -> bool {
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

pub fn set_has(set: &[Interval], v: f64) -> bool {
    set.iter().any(|iv| iv_has(iv, v))
}

/// Distance from v to the set's closure.
pub fn set_dist(set: &[Interval], v: f64) -> f64 {
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

/// Whether v, outside the range, reads as its nearest bound (see
/// [`same_shown`], and [`Analysed::strict`] for open bounds).
pub fn shown_as_bound(a: &Analysed, set: &[Interval], v: f64) -> bool {
    let b = set
        .iter()
        .flat_map(|iv| [iv.lo, iv.hi])
        .filter(|b| b.value.is_finite())
        .min_by(|p, q| (p.value - v).abs().total_cmp(&(q.value - v).abs()));
    match b {
        Some(b) if a.strict && !b.closed => false,
        Some(b) => same_shown(v, b.value),
        None => false,
    }
}

/// The bound of the set nearest v.
pub fn nearest_bound(set: &[Interval], v: f64) -> f64 {
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
pub fn same_shown(a: f64, b: f64) -> bool {
    a == b || (a.is_finite() && b.is_finite() && format_decimal(a) == format_decimal(b))
}

pub fn fmt_set(set: &[Interval]) -> String {
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

pub fn fmt_fams(f: &[Family]) -> String {
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

/// Golden-section search for the smallest (sign 1) or largest (sign −1)
/// value on [lo, hi]; undefined points count as worst.
pub fn golden(a: &Analysed, mut lo: f64, mut hi: f64, sign: f64) -> (f64, f64) {
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
pub fn refine_extremes(
    a: &Analysed,
    xs: &[f64],
    ys: &[f64],
    sign: f64,
    k: usize,
) -> Vec<(f64, f64)> {
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

/// Failed checks: by check, one line each, and the features they refute.
#[derive(Default)]
pub struct Report {
    pub failures: BTreeMap<&'static str, Vec<String>>,
    /// [`flags`] of the features contradicted.
    pub refuted: u32,
    /// Keep no text (the gate only needs `refuted`).
    pub quiet: bool,
    /// Work per step of the gate: (features vouched for, work, refuted
    /// so far).
    pub work: Vec<(u32, u64, u32)>,
    /// What the gate dropped: refuted by a check, unchecked for want of
    /// budget, or by rule ([`flags`] each).
    pub dropped: (u32, u32, u32),
}

impl Report {
    pub fn fail(&mut self, check: &'static str, refutes: u32, expr: &str, detail: String) {
        self.refuted |= refutes;
        if !self.quiet {
            self.failures
                .entry(check)
                .or_default()
                .push(format!("y={expr}    {detail}"));
        }
    }
}

// ---------------------------------------------------------------------------
// Checks on one function.

/// Rows flagged unknown keep no values.
pub fn check_flagged(a: &Analysed, r: &mut Report) {
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
                flag,
                &a.expr,
                format!("{name}: {n} values kept"),
            );
        }
    }
}

pub fn check_range(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
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
    // (A few hundred are plenty: past that they are all within the noise.)
    outside.truncate(256);
    for (x, y) in outside.into_iter().map(|i| (xs[i], ys[i])) {
        let dist = set_dist(range, y);
        if dist > 0.0 && !shown_as_bound(a, range, y) && dist > a.noise(x) {
            r.fail(
                "sample-outside-range",
                flags::RANGE,
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
        // The (x, f(x)) reading as each claimed point.
        let mut seen: Vec<Vec<(f64, f64)>> = vec![Vec::new(); range.len()];
        for (&x, &y) in xs.iter().zip(ys) {
            if !y.is_finite() {
                continue;
            }
            let Some(j) = (0..range.len()).find(|&j| same_shown(y, range[j].lo.value)) else {
                continue;
            };
            seen[j].push((x, y));
        }
        if let Some(((x1, y1), (x2, y2))) = seen.iter().find_map(|pts| resolvable_spread(a, pts)) {
            r.fail(
                "range-points-but-varies",
                flags::RANGE,
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
        if !set_has(range, y) && !shown_as_bound(a, range, y) && set_dist(range, y) > a.noise(x) {
            r.fail(
                "refined-value-outside-range",
                flags::RANGE,
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
                        "closed-range-bound-not-attained", flags::RANGE,
                        e,
                        format!(
                            "range {} has closed bound {v:?}; most extreme value found f({:?})={:?}",
                            fmt_set(range),
                            best.0,
                            best.1
                        ),
                    );
                }
            } else if let Some(i) = (0..ys.len())
                .filter(|&i| ys[i] == v)
                .take(64)
                .find(|&i| a.noise(xs[i]) == 0.0)
            {
                // An open bound is not attained: f exactly at it, with no
                // rounding anywhere, attains it.
                r.fail(
                    "open-range-bound-attained",
                    flags::RANGE,
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
                flags::RANGE | flags::ZEROS,
                e,
                format!("range {} attains 0, no x-intercepts", fmt_set(range)),
            );
        }
        if zeros && !attains0 {
            r.fail(
                "zeros-but-range-lacks-0",
                flags::RANGE | flags::ZEROS,
                e,
                format!(
                    "x-intercepts {}, range {}",
                    fmt_fams(&d.zeros),
                    fmt_set(range)
                ),
            );
        }
    }
    for (what, list, flag) in [
        ("minimum", &d.minima, flags::MINIMA),
        ("maximum", &d.maxima, flags::MAXIMA),
        ("inflection", &d.inflection_points, flags::INFLECTION_POINTS),
    ] {
        for (fx, y) in list {
            if !set_has(range, *y)
                && !same_shown(*y, nearest_bound(range, *y))
                && set_dist(range, *y) > a.noise(fx.x)
            {
                r.fail(
                    "extremum-y-outside-range",
                    flags::RANGE | flag,
                    e,
                    format!("{what} ({:?}, {y:?}) but range {}", fx.x, fmt_set(range)),
                );
            }
        }
    }
    if let Some(y) = d.y_intercept
        && !set_has(range, y)
        && !shown_as_bound(a, range, y)
        && set_dist(range, y) > a.noise(0.0)
    {
        r.fail(
            "y-intercept-outside-range",
            flags::RANGE | flags::Y_INTERCEPT,
            e,
            format!("y-intercept {y:?}, range {}", fmt_set(range)),
        );
    }
}

/// Two of the points `(x, f(x))` whose values differ by more than the noise
/// at both: among the two dozen lowest and highest (the extreme ones can
/// be where f is past its float resolution, trig of 10¹⁵).
pub fn resolvable_spread(a: &Analysed, pts: &[(f64, f64)]) -> Option<((f64, f64), (f64, f64))> {
    let mut by: Vec<(f64, f64)> = pts.iter().copied().filter(|p| p.1.is_finite()).collect();
    if by.len() < 2 {
        return None;
    }
    by.sort_by(|p, q| p.1.total_cmp(&q.1));
    let k = by.len().min(24);
    let lows: Vec<(f64, f64, f64)> = by[..k].iter().map(|&(x, y)| (x, y, a.noise(x))).collect();
    let highs: Vec<(f64, f64, f64)> = by[by.len() - k..]
        .iter()
        .rev()
        .map(|&(x, y)| (x, y, a.noise(x)))
        .collect();
    for l in &lows {
        for h in &highs {
            if h.1 - l.1 > l.2 + h.2 {
                return Some(((l.0, l.1), (h.0, h.1)));
            }
        }
    }
    None
}

/// What evaluation says about a claimed zero.
#[derive(PartialEq)]
pub enum Verdict {
    Holds,
    Fails,
    /// At or beside a pole, closer than f's resolution: 10⁶ + 10⁻⁶/(x + 10⁶)
    /// is 0 at −10⁶ − 10⁻¹², which no float can show.
    Unverifiable,
}

/// Is f indistinguishable from 0 at z or within a few steps of its own
/// resolution (f in a shifted frame changes only every so many floats),
/// without blowing up there (a pole is no zero)?
pub fn zero_ok(a: &Analysed, z: f64) -> Verdict {
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
pub fn same_root(a: &Analysed, p: f64, q: f64) -> bool {
    if floats_between(p, q) <= 64 {
        return true;
    }
    (1..8).all(|i| {
        let t = p + (q - p) * i as f64 / 8.0;
        let y = a.eval(t);
        y.is_finite() && (y == 0.0 || y.abs() <= a.noise(t))
    })
}

pub fn check_zero_claims(a: &Analysed, r: &mut Report) {
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
                flags::ZEROS,
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
                flags::ZEROS,
                &a.expr,
                format!("x-intercepts {} and {} are one zero", w[0], w[1]),
            );
            break;
        }
    }
}

/// What a sign change between two samples turns out to be.
pub enum Found {
    Crossing(f64),
    Pole(f64),
    Jump,
    Gap,
}

/// Bisect a sign change down to neighbouring floats and say what is there:
/// a zero crossing (f there is as small as its change across a few floats),
/// a pole (|f| grows towards it, far beyond the samples), a jump, or an
/// undefined point (gap).
pub fn bisect(a: &Analysed, mut lo: f64, mut hi: f64) -> Found {
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
pub fn check_missing_zeros(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
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
    // Sign changes between samples whose signs f's noise doesn't blur
    // (past f's resolution, trig of 10¹⁴, the signs say nothing), looking
    // across samples at its noise or exactly 0 in between (|x| − 10⁻¹² is
    // that small on either side of its zeros). An undefined sample ends a
    // stretch.
    let clear = |i: usize| ys[i].abs() > a.noise(xs[i]);
    // With the zeros unknown, only poles are looked for: a sign change with
    // f far bigger there than at the samples beyond.
    let zeros_wanted = !a.unknown(flags::ZEROS);
    let poles_wanted = !a.unknown(flags::VERTICAL_ASYMPTOTES);
    if !zeros_wanted && !poles_wanted {
        return;
    }
    let pole_like = |p: usize, i: usize| {
        let beyond = |j: Option<usize>| {
            j.filter(|&j| j < n && ys[j].is_finite())
                .map_or(0.0, |j| ys[j].abs())
        };
        ys[p].abs() + ys[i].abs() > 10.0 * (beyond(p.checked_sub(1)) + beyond(Some(i + 1)))
    };
    let mut prev: Option<usize> = None;
    for i in 0..n {
        if a.over() {
            return;
        }
        let y = ys[i];
        if !y.is_finite() {
            prev = None;
            continue;
        }
        if y == 0.0 {
            continue;
        }
        if let Some(p) = prev
            && ys[p].signum() != y.signum()
            && (zeros_wanted || pole_like(p, i))
        {
            // The nearest clear samples either side of the change.
            let left = (0..=p)
                .rev()
                .take(9)
                .take_while(|&j| ys[j].is_finite())
                .find(|&j| ys[j] != 0.0 && ys[j].signum() == ys[p].signum() && clear(j));
            let right = (i..n)
                .take(9)
                .take_while(|&j| ys[j].is_finite())
                .find(|&j| ys[j] != 0.0 && ys[j].signum() == y.signum() && clear(j));
            if let (Some(l), Some(rr)) = (left, right) {
                match bisect(a, xs[l], xs[rr]) {
                    Found::Crossing(x) => roots.push(x),
                    Found::Pole(x) => poles.push(x),
                    Found::Jump | Found::Gap => {}
                }
                if roots.len() + poles.len() > 4000 {
                    break;
                }
            }
        }
        prev = Some(i);
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
                if let Some(i) = (0..n)
                    .filter(|&i| ys[i].is_finite() && ys[i] != 0.0)
                    .take(256)
                    .find(|&i| {
                        inside(xs[i])
                            && ys[i].is_finite()
                            && ys[i] != 0.0
                            && ys[i].abs() > a.noise(xs[i])
                    })
                {
                    r.fail(
                        "zero-set-wrong",
                        flags::ZEROS,
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
                        flags::ZEROS,
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
                    flags::ZEROS,
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
                        "missing-zero", flags::ZEROS,
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
                    flags::VERTICAL_ASYMPTOTES | flags::DOMAIN,
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
pub fn marks(a: &Analysed) -> Vec<f64> {
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
pub fn check_extremum_claims(a: &Analysed, r: &mut Report) {
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
                        flag,
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
                            flag,
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
pub fn same_turn(a: &Analysed, p: f64, q: f64, sign: f64) -> bool {
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
pub fn check_missing_extrema(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
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
            if step.is_nan() || step > 0.25 * swing + a.rounding(xm) {
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
                        flag,
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
                    flag,
                    e,
                    format!("turn at ({xm:?}, {ym:?}); reported {reported:?}"),
                );
                break;
            }
        }
    }
}

pub fn check_y_intercept(a: &Analysed, r: &mut Report) {
    if a.unknown(flags::Y_INTERCEPT) {
        return;
    }
    let e = &a.expr;
    let f0 = a.eval(0.0);
    match (a.d().y_intercept, a.defined(0.0)) {
        (Some(y), Some(false)) => r.fail(
            "y-intercept-where-undefined",
            flags::Y_INTERCEPT,
            e,
            format!("y-intercept {y:?} but f(0) is undefined"),
        ),
        (Some(y), Some(true))
            if !same_shown(y, f0) && (y - f0).abs() > a.noise(0.0) + 4.0 * ulp(y) =>
        {
            r.fail(
                "y-intercept-wrong",
                flags::Y_INTERCEPT,
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
            flags::Y_INTERCEPT,
            e,
            format!("no y-intercept but f(0)={f0:?}"),
        ),
        _ => {}
    }
}

/// Is x a member of the family, to the rounding of its copies?
pub fn in_family(f: &Family, x: f64) -> bool {
    let c = nearest(f, x);
    let k = match f.period {
        Some(p) if p > 0.0 => ((x - f.x) / p).round().abs() * ulp(p),
        _ => 0.0,
    };
    (x - c).abs() <= 0.5 * ulp(x) + 2.0 * k
}

/// Is x within a few floats (or the rounding of a family's copies) of v?
pub fn close_to(f: &Family, x: f64, floats: i64) -> bool {
    let c = nearest(f, x);
    let k = match f.period {
        Some(p) if p > 0.0 => ((x - f.x) / p).round().abs() * ulp(p),
        _ => 0.0,
    };
    floats_between(c, x) <= floats || (x - c).abs() <= 4.0 * k
}

pub fn check_domain(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
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
        // (An undefined float with defined ones all round it is a hole, not
        // the edge of an interval: no doubt which side it is on.)
        let hole = |x: f64| {
            [-2, -1, 1, 2]
                .iter()
                .all(|&k| a.defined(nudge(x, k)) == Some(true))
        };
        match a.defined(x) {
            Some(true) if !claimed && edge(x) => {}
            Some(false) if claimed && edge(x) && !hole(x) => {}
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
            for (&x, &y) in xs.iter().zip(ys) {
                let claimed = set_has(&d.domain, x) && !excluded(x);
                // (A sample with a value where the domain says there is one
                // needs no more looking at.)
                if claimed && y.is_finite() && y != 0.0 {
                    continue;
                }
                test(x, claimed);
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
        r.fail("domain-includes-undefined", flags::DOMAIN, e, detail(x));
    }
    // An excluded point is where f is undefined, blows up, or is within
    // rounding of where it is undefined (x²−2 over x − √2 at the double
    // nearest √2).
    'ex: for f in &d.excluded {
        for c in a.copies(f) {
            if a.over() {
                break 'ex;
            }
            if !c.is_finite() || exclusion_supported(a, c) {
                continue;
            }
            r.fail(
                "excluded-point-defined",
                flags::DOMAIN,
                e,
                format!(
                    "x={c:?} excluded, but f is defined and continuous there: f={:?}",
                    a.eval(c)
                ),
            );
            break 'ex;
        }
    }
    if let Some(x) = bad_out {
        r.fail("domain-excludes-defined", flags::DOMAIN, e, detail(x));
    }
}

/// Whether a point excluded from the domain has anything to show for it:
/// f undefined within two floats, blowing up beside it, or an operand that
/// makes f undefined (a divisor, a logarithm's argument) within its own
/// rounding of doing so there.
pub fn exclusion_supported(a: &Analysed, c: f64) -> bool {
    if (-2..=2).any(|k| a.defined(nudge(c, k)) != Some(true)) || blows_up(a, c) {
        return true;
    }
    fn singular(e: &Expr, out: &mut Vec<Expr>) {
        use crate::ast::{BinOp, Func};
        match e {
            Expr::Bin(op, l, rr) => {
                if matches!(op, BinOp::Div | BinOp::Pow) {
                    out.push(if *op == BinOp::Div {
                        (**rr).clone()
                    } else {
                        (**l).clone()
                    });
                }
                singular(l, out);
                singular(rr, out);
            }
            Expr::Call(f, args) => {
                if matches!(
                    f,
                    Func::Ln | Func::Log | Func::LogBase | Func::Coth | Func::Csch
                ) {
                    out.extend(args.iter().cloned());
                }
                args.iter().for_each(|x| singular(x, out));
            }
            Expr::Neg(x) | Expr::Degrees(x) => singular(x, out),
            _ => {}
        }
    }
    let mut ops = Vec::new();
    singular(&a.ast, &mut ops);
    ops.iter().filter(|o| o.contains_x()).any(|o| {
        [-1, 0, 1].iter().any(|&k| {
            let t = nudge(c, k);
            let (v, err) = a.bound_of(o, t);
            v.is_finite() && v.abs() <= err + 4.0 * ulp(v)
        })
    })
}

/// Whether |f| grows without bound (or f keeps changing by as much per
/// decade nearer, a log pole) on either side of c.
pub fn blows_up(a: &Analysed, c: f64) -> bool {
    // A few floats out, then ten and a hundred times as far: either side's
    // |f| grows towards x, or f keeps changing by as much per decade
    // nearer (a log pole: ln|x| + 10⁶ shrinks in size towards 0 until
    // e^(−10⁶), but steadily).
    let q = 16.0 * ulp(c);
    [-1.0, 1.0].iter().any(|&s| {
        // From the first step at which f changes at all: in a shifted frame
        // (1/sin(x − 1000)) f is flat across many floats of x.
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
    })
}

/// Whether f certainly blows up on a side of c where it is defined: from
/// the nearest point at which it is finite (e^(1/x) only is from x ≈ 1/709
/// on), |f| grows towards c, by a factor or (a log pole) by the same step
/// each decade nearer. Stricter than [`blows_up`], which accepts a claim:
/// this one makes one (1/ln x tends to 0 at 0, ever more slowly).
pub fn certainly_blows_up(a: &Analysed, c: f64) -> bool {
    let limit = 1e-2 * c.abs().max(1.0);
    [-1.0, 1.0].iter().any(|&s| {
        let mut q = 16.0 * ulp(c);
        let mut near = a.eval(c + s * q);
        let mut tries = 0;
        while near.is_nan() && q < limit && tries < 600 {
            q *= 4.0;
            near = a.eval(c + s * q);
            tries += 1;
        }
        if near.is_nan() || q >= limit {
            return false;
        }
        if near.is_infinite() || near.abs() == f64::MAX {
            return true;
        }
        let (mid, far) = (a.eval(c + s * q * 10.0), a.eval(c + s * q * 100.0));
        if !(mid.is_finite() && far.is_finite()) {
            return false;
        }
        let (d1, d2) = (near - mid, mid - far);
        // (Growth beyond f's noise: rounding beside a hole, (x/3 + 1)/(x + 3)
        // at −3, also swells nearer.)
        let (n0, n1) = (a.noise(c + s * q), a.noise(c + s * q * 10.0));
        if !(near.abs() - mid.abs() > n0 + n1 && mid.abs() > far.abs()) {
            return false;
        }
        if near.abs() > 1.2 * mid.abs() {
            return true;
        }
        // A log pole: the same step per decade, and still the same many
        // decades further out (atan(ln|x|) tends to −π/2 with steps that
        // only slowly shrink).
        let same = |p: f64, q: f64| {
            p.signum() == q.signum() && p.abs() >= 0.8 * q.abs() && p.abs() <= 1.25 * q.abs()
        };
        if !(d2 != 0.0 && same(d1, d2)) {
            return false;
        }
        let mut t = q * 1000.0;
        let mut checked = false;
        while t * 100.0 < limit {
            t *= 10.0;
            checked = true;
        }
        if !checked {
            return false;
        }
        let (u, w) = (a.eval(c + s * t), a.eval(c + s * t * 10.0));
        u.is_finite() && w.is_finite() && same(u - w, d1)
    })
}

pub fn check_monotonicity(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
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
            Monotonicity::Constant => resolvable_spread(a, pts),
            Monotonicity::Increasing | Monotonicity::Decreasing => {
                let sign = if dir == Monotonicity::Increasing {
                    1.0
                } else {
                    -1.0
                };
                let mut best: Option<(f64, f64)> = None;
                let mut out = None;
                let mut tried = 0;
                for &(x, y) in pts {
                    if let Some((bx, by)) = best
                        && sign * y < sign * by
                        && tried < 256
                        && {
                            tried += 1;
                            (by - y).abs() > a.noise(bx) + a.noise(x)
                        }
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
            flags::MONOTONE_INTERVALS,
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
                    flags::MONOTONE_INTERVALS,
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
pub fn check_vertical(a: &Analysed, r: &mut Report) {
    if a.unknown(flags::VERTICAL_ASYMPTOTES) {
        return;
    }
    let d = a.d();
    for v in &d.vertical_asymptotes {
        for c in a.copies(v) {
            if !blows_up(a, c) {
                let q = 16.0 * ulp(c);
                r.fail(
                    "asymptote-without-blowup",
                    flags::VERTICAL_ASYMPTOTES,
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
    // Where f stops being defined (an excluded point, an end of the
    // domain), a side on which it blows up is a vertical asymptote, even
    // with no sign change (e^(1/x) − 10⁶ at 0).
    let mut ends: Vec<f64> = Vec::new();
    for f in &d.excluded {
        ends.extend(a.copies(f));
    }
    for iv in &d.domain {
        for b in [iv.lo.value, iv.hi.value] {
            if b.is_finite() {
                ends.extend(a.copies(&Family {
                    x: b,
                    period: a.period(),
                }));
            }
        }
    }
    for c in ends {
        if a.over() {
            return;
        }
        let claimed = d.vertical_asymptotes.iter().any(|v| close_to(v, c, 1024));
        if !claimed && c.is_finite() && certainly_blows_up(a, c) {
            r.fail(
                "missing-pole",
                flags::VERTICAL_ASYMPTOTES,
                &a.expr,
                format!(
                    "f blows up beside x={c:?}; vertical asymptotes {}",
                    fmt_fams(&d.vertical_asymptotes)
                ),
            );
            return;
        }
    }
}

/// A horizontal asymptote c on a side: f − c must die away there. One that
/// keeps oscillating at the same size decade after decade, or grows,
/// contradicts it. (Deviations within the noise say nothing.)
pub fn check_horizontal(a: &Analysed, centres: &[f64], r: &mut Report) {
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
                    flags::HORIZONTAL_ASYMPTOTES,
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
                    flags::HORIZONTAL_ASYMPTOTES,
                    e,
                    format!("y={c:?} but f({xm:?})={mid:?}, f({xf:?})={far:?}"),
                );
                break;
            }
            // Where f has settled, far out, on a value other than c, its
            // limit is that value.
            if let Some((v, n)) = settled(a, s, reach)
                && (v - c).abs() > n + 4.0 * ulp(c)
            {
                r.fail(
                    "hasymptote-wrong-value",
                    flags::HORIZONTAL_ASYMPTOTES,
                    e,
                    format!(
                        "y={c:?} as x→{}∞ but f settles on {v:?}",
                        if s > 0.0 { "" } else { "−" }
                    ),
                );
                return;
            }
        }
    }
    // And a side on which f settles has an asymptote there (unless f is a
    // constant, or a line is claimed).
    let d = a.d();
    let constant = !d.range.is_empty() && d.range.iter().all(|iv| iv.lo.value == iv.hi.value);
    if constant || a.unknown(flags::OBLIQUE_ASYMPTOTES) {
        return;
    }
    for s in [1.0, -1.0] {
        let covers = |side: AsymptoteSide| match side {
            AsymptoteSide::AnyInfinity => true,
            AsymptoteSide::PositiveInfinity => s > 0.0,
            AsymptoteSide::NegativeInfinity => s < 0.0,
            AsymptoteSide::Unknown => false,
        };
        if d.horizontal_asymptotes.iter().any(|h| covers(h.1))
            || d.oblique_asymptotes.iter().any(|o| covers(o.2))
        {
            continue;
        }
        if let Some((v, _)) = settled(a, s, reach) {
            r.fail(
                "missing-hasymptote",
                flags::HORIZONTAL_ASYMPTOTES,
                e,
                format!(
                    "no horizontal asymptote as x→{}∞, but f settles on {v:?}",
                    if s > 0.0 { "" } else { "−" }
                ),
            );
            return;
        }
    }
}

/// The value f has settled on far out on side `s` (beyond every centre),
/// with its noise there: the same, to within the noise, at the three
/// farthest of 10³⁰⁰, 10²⁰⁰, …, 10²⁰ where its value is a double and its
/// rounding is bounded (an intermediate that overflows, (x + 10⁶)² at
/// 10³⁰⁰, bounds nothing).
pub fn settled(a: &Analysed, s: f64, reach: f64) -> Option<(f64, f64)> {
    let floor = (1e6 * reach).max(1e20);
    let mut pts: Vec<(f64, f64)> = Vec::new();
    for x in [1e300, 1e200, 1e150, 1e100, 1e60, 1e40, 1e30, 1e25, 1e20] {
        if x < floor || pts.len() == 3 {
            break;
        }
        let y = a.eval(s * x);
        // (A stand-in for a value beyond the doubles, ±5·10⁻³²⁴ or the
        // largest double, is no value to settle on.)
        if !(y.is_finite() && y.abs() < f64::MAX && y.abs() != f64::from_bits(1)) {
            return None;
        }
        let n = a.noise(s * x);
        if !n.is_finite() {
            continue;
        }
        // Nor do values the floats blur (trig of 10³⁰): settled means f's
        // noise there is a small part of it.
        if n > 1e-6 * y.abs() {
            return None;
        }
        pts.push((y, n));
    }
    if pts.len() < 3 {
        return None;
    }
    let agree = pts
        .windows(2)
        .all(|w| (w[0].0 - w[1].0).abs() <= w[0].1 + w[1].1);
    agree.then_some(pts[0])
}

/// Even or odd as claimed, at every sampled pair ±x.
pub fn check_parity(a: &Analysed, xs: &[f64], r: &mut Report) {
    if a.unknown(flags::PARITY) {
        return;
    }
    let odd = match a.k.parity {
        Parity::Even => false,
        Parity::Odd => true,
        _ => return,
    };
    let e = &a.expr;
    let mut tried = 0;
    for &x in xs.iter().filter(|x| **x > 0.0).step_by(3) {
        if tried > 256 {
            return;
        }
        let (dp, dm) = (a.defined(x), a.defined(-x));
        if let (Some(p), Some(m)) = (dp, dm)
            && p != m
            && (-2..=2).all(|k| a.defined(nudge(x, k)) == dp && a.defined(nudge(-x, k)) == dm)
        {
            r.fail(
                "parity-domain-asymmetric",
                flags::PARITY,
                e,
                format!("{:?} but f is defined at only one of ±{x:?}", a.k.parity),
            );
            return;
        }
        if dp == Some(true) && dm == Some(true) {
            let (y, z) = (a.eval(x), a.eval(-x));
            let dev = if odd { (y + z).abs() } else { (y - z).abs() };
            if dev > 0.0 && {
                tried += 1;
                dev > a.noise(x) + a.noise(-x)
            } {
                r.fail(
                    "parity-contradicted",
                    flags::PARITY,
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
pub fn check_period(a: &Analysed, centres: &[f64], r: &mut Report) {
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
        let mut tried = 0;
        for &x in &pts {
            if tried > 512 || a.over() {
                return None;
            }
            let x2 = x + q;
            match (a.defined(x), a.defined(x2)) {
                (Some(true), Some(true)) => {
                    let (y1, y2) = (a.eval(x), a.eval(x2));
                    // (Beyond the doubles, beside a pole: nothing to compare.)
                    if y1.abs() == f64::MAX || y2.abs() == f64::MAX {
                        continue;
                    }
                    if y1 != y2 && {
                        tried += 1;
                        (y1 - y2).abs() > a.noise(x) + a.noise(x2)
                    } {
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
        r.fail(
            "period-wrong",
            flags::PERIODICITY,
            e,
            format!("period {p:?}: {why}"),
        );
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
                    flags::PERIODICITY,
                    e,
                    format!("period {p:?}, but {:?} is one too", p / n),
                );
                return;
            }
        }
    }
}

/// An inflection point is where it says, with the value it says, and f's
/// curvature turns there: on each side, as far as the next reported feature
/// allows, f bends the other way, or not resolvably at all.
pub fn check_inflections(a: &Analysed, r: &mut Report) {
    if a.unknown(flags::INFLECTION_POINTS) {
        return;
    }
    let d = a.d();
    let mut marks = marks(a);
    for (f, _) in &d.inflection_points {
        marks.extend(a.copies(f));
    }
    for (fx, y) in &d.inflection_points {
        for c in a.copies(fx) {
            if a.over() {
                return;
            }
            let fv = a.eval(c);
            if !fv.is_finite()
                || ((fv - y).abs() > a.noise(c) + 4.0 * ulp(*y) && !same_shown(fv, *y))
            {
                r.fail(
                    "inflection-value-wrong",
                    flags::INFLECTION_POINTS,
                    &a.expr,
                    format!("inflection at {c:?} reported {y:?}, f={fv:?}"),
                );
                return;
            }
            let gap = marks
                .iter()
                .filter(|&&m| floats_between(m, c) > 16)
                .map(|m| (m - c).abs())
                .fold(f64::INFINITY, f64::min);
            let s = c.abs().max(1.0);
            // Second differences f(t − h) − 2f(t) + f(t + h) beside c, at
            // t = c ∓ h: of one sign beyond their noise on both sides, at
            // every scale tried, is no inflection.
            let curv = |t: f64, h: f64| {
                let (l, m, rr) = (a.eval(t - h), a.eval(t), a.eval(t + h));
                let v = l - 2.0 * m + rr;
                let n = a.noise(t - h) + 2.0 * a.noise(t) + a.noise(t + h);
                (v, n)
            };
            let mut same_sign_everywhere = true;
            let mut tried = 0;
            for h in [1e-3 * s, 1e-2 * s, 1e-1 * s] {
                let h = h.min(0.2 * gap);
                if h.is_nan() || h <= 0.0 || floats_between(c, c + h) < 64 {
                    continue;
                }
                let ((vl, nl), (vr, nr)) = (curv(c - h, h), curv(c + h, h));
                if !(vl.is_finite() && vr.is_finite()) {
                    same_sign_everywhere = false;
                    break;
                }
                tried += 1;
                if !(vl.abs() > nl && vr.abs() > nr && vl.signum() == vr.signum()) {
                    same_sign_everywhere = false;
                    break;
                }
            }
            if tried >= 2 && same_sign_everywhere {
                r.fail(
                    "inflection-no-turn",
                    flags::INFLECTION_POINTS,
                    &a.expr,
                    format!("inflection at {c:?} but f bends the same way either side"),
                );
                return;
            }
        }
    }
}

/// The panel's precision for a value: half a unit in its 6th significant
/// digit (`format_decimal`).
fn shown_precision(v: f64) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return 0.0;
    }
    0.5 * 10f64.powi(v.abs().log10().floor() as i32 - 5)
}

/// An oblique asymptote y = m·x + b on a side: f − (m·x + b) dies away
/// there (to the precision the panel gives m and b).
pub fn check_oblique(a: &Analysed, centres: &[f64], r: &mut Report) {
    if a.unknown(flags::OBLIQUE_ASYMPTOTES) {
        return;
    }
    let reach = centres.iter().fold(1.0f64, |m, c| m.max(c.abs()));
    for &(m, b, side) in &a.d().oblique_asymptotes {
        let sides: &[f64] = match side {
            AsymptoteSide::PositiveInfinity => &[1.0],
            AsymptoteSide::NegativeInfinity => &[-1.0],
            AsymptoteSide::AnyInfinity => &[1.0, -1.0],
            AsymptoteSide::Unknown => &[],
        };
        for &s in sides {
            let (xm, xf) = (s * (100.0 * reach).max(1e8), s * (1e7 * reach).max(1e15));
            if xf.abs() > 1e300 {
                continue;
            }
            let dev = |x: f64| {
                let y = a.eval(x);
                let line = m * x + b;
                let slack = a.noise(x)
                    + x.abs() * shown_precision(m)
                    + shown_precision(b)
                    + 2.0 * (ulp(line) + ulp(m * x) + ulp(b));
                ((y - line).abs(), slack, y.is_finite())
            };
            let ((dm, sm, okm), (df, sf, okf)) = (dev(xm), dev(xf));
            if okm && okf && df > sf + 2.0 * dm + sm {
                r.fail(
                    "oasymptote-not-approached",
                    flags::OBLIQUE_ASYMPTOTES,
                    &a.expr,
                    format!(
                        "y={m:?}x+{b:?} but f deviates by {dm:e} at {xm:?} and {df:e} at {xf:?}"
                    ),
                );
                return;
            }
        }
    }
}

/// Every check on one function, at the sorted samples `xs` (`ys` = f there),
/// `centres` being where its features gather (shifts).
pub fn check_all(a: &Analysed, xs: &[f64], ys: &[f64], centres: &[f64], r: &mut Report) {
    for (_, step) in steps() {
        step(a, xs, ys, centres, r);
    }
}

type Step = fn(&Analysed, &[f64], &[f64], &[f64], &mut Report);

/// The checks, each with the features it vouches for once it has run to
/// the end.
fn steps() -> [(u32, Step); 12] {
    [
        (0, |a, _, _, _, r| check_flagged(a, r)),
        (flags::RANGE, |a, xs, ys, _, r| {
            if !a.unknown(flags::RANGE) && !a.d().range.is_empty() {
                check_range(a, xs, ys, r);
            }
        }),
        (flags::Y_INTERCEPT, |a, _, _, _, r| check_y_intercept(a, r)),
        (flags::ZEROS, |a, xs, ys, _, r| {
            check_zero_claims(a, r);
            check_missing_zeros(a, xs, ys, r);
        }),
        (flags::MINIMA | flags::MAXIMA, |a, xs, ys, _, r| {
            check_extremum_claims(a, r);
            check_missing_extrema(a, xs, ys, r);
        }),
        (flags::INFLECTION_POINTS, |a, _, _, _, r| {
            check_inflections(a, r)
        }),
        (flags::DOMAIN, |a, xs, ys, _, r| check_domain(a, xs, ys, r)),
        (flags::MONOTONE_INTERVALS, |a, xs, ys, _, r| {
            check_monotonicity(a, xs, ys, r)
        }),
        (flags::VERTICAL_ASYMPTOTES, |a, _, _, _, r| {
            check_vertical(a, r)
        }),
        (
            flags::HORIZONTAL_ASYMPTOTES | flags::OBLIQUE_ASYMPTOTES,
            |a, _, _, c, r| {
                check_horizontal(a, c, r);
                check_oblique(a, c, r);
            },
        ),
        (flags::PARITY, |a, xs, _, _, r| check_parity(a, xs, r)),
        (flags::PERIODICITY, |a, _, _, c, r| check_period(a, c, r)),
    ]
}

// ---------------------------------------------------------------------------
// The gate.

/// Work the gate may do, in units of one evaluation of a program of cost 1
/// (an evaluation of f costs its program's cost).
pub const GATE_BUDGET: u64 = 60_000_000;

/// Functions of x that can make f periodic (sin, floor(x) − x, x mod 1).
fn periodic_capable(e: &Expr) -> bool {
    use crate::ast::Func::*;
    match e {
        Expr::Call(f, args) => {
            (matches!(
                f,
                Sin | Cos | Tan | Sec | Csc | Cot | Floor | Ceil | Round | Mod
            ) && args.iter().any(Expr::contains_x))
                || args.iter().any(periodic_capable)
        }
        Expr::Neg(a) | Expr::Degrees(a) => periodic_capable(a),
        Expr::Bin(_, a, b) => periodic_capable(a) || periodic_capable(b),
        _ => false,
    }
}

/// The numbers written in the expression (π and e too).
fn literals(e: &Expr, out: &mut Vec<f64>) {
    match e {
        Expr::Num(v) => out.push(*v),
        Expr::Const(c) => out.push(c.value()),
        Expr::Neg(a) | Expr::Degrees(a) => literals(a, out),
        Expr::Bin(_, a, b) => {
            literals(a, out);
            literals(b, out);
        }
        Expr::Call(_, args) => args.iter().for_each(|a| literals(a, out)),
        _ => {}
    }
}

/// Where f's features can gather, from its text alone: ± every number in
/// it, and their pairwise sums, differences, products and quotients (the
/// centres of (x − 10⁹)(x − 10⁹ − 1/16), of x/1000 − 5, of a shift inside
/// a shift).
pub fn centres_of(e: &Expr) -> Vec<f64> {
    let mut lits = Vec::new();
    literals(e, &mut lits);
    lits.retain(|v| v.is_finite() && *v != 0.0);
    lits.sort_by(f64::total_cmp);
    lits.dedup();
    lits.truncate(16);
    let mut cs: Vec<f64> = lits.iter().flat_map(|&v| [v, -v]).collect();
    for (i, &p) in lits.iter().enumerate() {
        for &q in &lits[i + 1..] {
            for v in [p + q, p - q, p * q, p / q, q / p] {
                cs.push(v);
                cs.push(-v);
            }
        }
    }
    cs.retain(|c| c.is_finite() && *c != 0.0 && c.abs() <= 1e15);
    let mut seen: Vec<f64> = Vec::new();
    for c in cs {
        if !seen.contains(&c) {
            seen.push(c);
        }
    }
    seen.truncate(32);
    seen
}

/// The gate's samples: dense near 0, the integers to ±400, log-spaced to
/// ±10¹⁵, a grid to ±200 for periodic-capable f, the same around every
/// far centre, and the floats at and around every point the analysis
/// reports (and their periodic copies).
pub fn gate_samples(a: &Analysed, centres: &[f64]) -> Vec<f64> {
    let mut xs = Vec::with_capacity(16000);
    for i in -2000..=2000 {
        xs.push(i as f64 * 0.005);
    }
    for i in -400..=400 {
        xs.push(i as f64);
    }
    for e in -60..=150 {
        let m = 10f64.powf(e as f64 / 10.0);
        xs.push(m);
        xs.push(-m);
    }
    if periodic_capable(&a.ast) {
        for i in -4000..=4000 {
            xs.push(i as f64 * 0.05 + 0.0123);
        }
    }
    for &c in centres {
        for k in [-64, -8, -1, 1, 8, 64] {
            xs.push(nudge(c, k));
        }
        xs.push(c);
        if c.abs() < 10.0 {
            for e in -60..=-20 {
                let m = 10f64.powf(e as f64 / 10.0);
                xs.push(c + m);
                xs.push(c - m);
            }
            continue;
        }
        for i in -200..=200 {
            xs.push(c + i as f64 * 0.005);
        }
        for i in -50..=50 {
            xs.push(c + i as f64);
        }
        for e in (-60..=150).step_by(2) {
            let m = 10f64.powf(e as f64 / 10.0);
            xs.push(c + m);
            xs.push(c - m);
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
    pts.truncate(256);
    let mut cs = vec![0.0];
    cs.extend(centres.iter().copied());
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

/// Unknown by rule: what sampling can't refute and the analysis can't
/// show. Returns the [`flags`] of definite claims that aren't justified.
fn unjustified(a: &Analysed, xs: &[f64], ys: &[f64], centres: &[f64]) -> u32 {
    let d = a.d();
    let k = &a.k;
    let mut out = 0;
    // A closed end of a range piece (not a single value) must be attained
    // where f certainly takes it: a reported extremum, the y-intercept, a
    // closed end of the domain, or a sample computed exactly. Samples near
    // it prove nothing (e^x/(e^x + 1) reaches 1.0 in floats, never in
    // fact; sin x + sin(√2·x) comes ever closer to ±2), so neither do
    // refined sample extremes.
    if !a.unknown(flags::RANGE) {
        let at = |x: f64, v: f64| {
            let y = a.eval(x);
            y.is_finite() && (y == v || same_shown(y, v) || (y - v).abs() <= a.noise(x))
        };
        let attained = |v: f64, lower: bool| {
            d.minima
                .iter()
                .chain(&d.maxima)
                .any(|(fx, y)| same_shown(*y, v) && at(fx.x, v))
                || d.y_intercept
                    .is_some_and(|y| same_shown(y, v) && at(0.0, v))
                || d.domain.iter().any(|iv| {
                    [iv.lo, iv.hi]
                        .iter()
                        .any(|b| b.closed && b.value.is_finite() && at(b.value, v))
                })
                || xs
                    .iter()
                    .zip(ys)
                    .filter(|(_, y)| **y == v)
                    .take(64)
                    .any(|(&x, _)| a.rounding(x) == 0.0)
                || damped_turn(a, xs, ys, v, lower)
                || bounded_turn(a, xs, ys, v, lower)
        };
        let bad = d.range.iter().any(|iv| {
            iv.lo.value != iv.hi.value
                && [(iv.lo, true), (iv.hi, false)]
                    .iter()
                    .any(|(b, lower)| b.closed && b.value.is_finite() && !attained(b.value, *lower))
        });
        if bad || !range_supported(a, xs, ys, centres) || !hole_values_attained(a, xs, ys) {
            out |= flags::RANGE;
        }
    }
    // Neither even nor odd: shown by a pair ±x, both defined and f(−x)
    // resolvably neither f(x) nor −f(x), or defined at only one of them.
    if !a.unknown(flags::PARITY) && k.parity == Parity::Neither {
        // (Every third sample, and every excluded point and end of the
        // domain, where an asymmetry most likely is.)
        let mut cands: Vec<f64> = xs.iter().copied().filter(|x| *x > 0.0).step_by(3).collect();
        cands.extend(d.excluded.iter().map(|f| f.x.abs()));
        cands.extend(
            d.domain
                .iter()
                .flat_map(|iv| [iv.lo.value.abs(), iv.hi.value.abs()])
                .filter(|v| v.is_finite()),
        );
        let witness = cands.into_iter().filter(|x| *x > 0.0).any(|x| {
            match (a.defined(x), a.defined(-x)) {
                (Some(true), Some(true)) => {
                    let (y, z) = (a.eval(x), a.eval(-x));
                    y.is_finite() && z.is_finite() && y != z && y != -z && {
                        let n = a.noise(x) + a.noise(-x);
                        (y - z).abs() > n && (y + z).abs() > n
                    }
                }
                // (Defined at only one of them, exactly: a hole at 10⁹ and
                // none at −10⁹ is a witness.)
                (Some(p), Some(m)) => p != m,
                _ => false,
            }
        });
        if !witness {
            out |= flags::PARITY;
        }
    }
    // Not periodic: plain when nothing in f can repeat; otherwise f must
    // be seen to grow, decade after decade (x + sin x), which no periodic
    // function does.
    // (A constant is "not periodic" by the original's convention.)
    let constant = d.range.len() == 1 && d.range[0].lo.value == d.range[0].hi.value;
    if !a.unknown(flags::PERIODICITY)
        && k.periodicity_direction == super::Periodicity::NotPeriodic
        && !constant
        && periodic_capable(&a.ast)
        && !grows(a)
        && !dies_away(a)
    {
        out |= flags::PERIODICITY;
    }
    out
}

/// Bounds f can't pass whatever x is, from its form alone (interval
/// arithmetic: sin of anything is in [−1, 1], a sum within the sum of its
/// terms' bounds, …), in exact arithmetic. (−∞, ∞) where the form says
/// nothing.
pub fn enclosure(e: &Expr, unit: TrigUnit) -> (f64, f64) {
    use crate::ast::{BinOp, Func};
    const ALL: (f64, f64) = (f64::NEG_INFINITY, f64::INFINITY);
    let ok = |(l, h): (f64, f64)| {
        if l.is_nan() || h.is_nan() || l > h {
            ALL
        } else {
            (l, h)
        }
    };
    let quarter = unit.full_turn() / 4.0;
    match e {
        Expr::Num(v) => (*v, *v),
        Expr::Const(c) => (c.value(), c.value()),
        Expr::Neg(x) => {
            let (l, h) = enclosure(x, unit);
            (-h, -l)
        }
        Expr::Bin(op, x, y) => {
            let ((a, b), (c, d)) = (enclosure(x, unit), enclosure(y, unit));
            match op {
                BinOp::Add => ok((a + c, b + d)),
                BinOp::Sub => ok((a - d, b - c)),
                BinOp::Mul => {
                    let p = [a * c, a * d, b * c, b * d];
                    if p.iter().any(|v| v.is_nan()) {
                        ALL
                    } else {
                        ok((
                            p.iter().copied().fold(f64::INFINITY, f64::min),
                            p.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                        ))
                    }
                }
                BinOp::Div if c > 0.0 || d < 0.0 => {
                    let p = [a / c, a / d, b / c, b / d];
                    if p.iter().any(|v| v.is_nan()) {
                        ALL
                    } else {
                        ok((
                            p.iter().copied().fold(f64::INFINITY, f64::min),
                            p.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                        ))
                    }
                }
                BinOp::Pow if c == d && c > 0.0 && c.fract() == 0.0 && c % 2.0 == 0.0 => {
                    let m = a.abs().max(b.abs()).powf(c);
                    if a <= 0.0 && b >= 0.0 {
                        (0.0, m)
                    } else {
                        ok((a.abs().min(b.abs()).powf(c), m))
                    }
                }
                _ => ALL,
            }
        }
        Expr::Call(f, args) if args.len() == 1 => {
            let (l, h) = enclosure(&args[0], unit);
            match f {
                Func::Sin | Func::Cos => (-1.0, 1.0),
                Func::Tanh | Func::Sign => (-1.0, 1.0),
                Func::Sech => (0.0, 1.0),
                Func::Atan => (-quarter, quarter),
                Func::Acot => (-quarter, quarter),
                Func::Asin => (-quarter, quarter),
                Func::Acos => (0.0, 2.0 * quarter),
                Func::Abs => (
                    if l <= 0.0 && h >= 0.0 {
                        0.0
                    } else {
                        l.abs().min(h.abs())
                    },
                    l.abs().max(h.abs()),
                ),
                Func::Sqrt => (l.max(0.0).sqrt(), h.sqrt()),
                Func::Exp => (l.exp(), h.exp()),
                Func::Floor => (l.floor(), h.floor()),
                Func::Ceil => (l.ceil(), h.ceil()),
                _ => ALL,
            }
        }
        _ => ALL,
    }
}

/// A closed range bound v (a lower one if `lower`) that is both a bound
/// f's form can't pass ([`enclosure`]) and the value of a turn of f, to
/// within its noise: sin(x²) reaches 1 at √(π/2), and nothing reaches
/// further. (A turn's value short of the form's bound, sin x + sin(√2·x)
/// at 1.99994, says nothing about turns further out.)
fn bounded_turn(a: &Analysed, xs: &[f64], ys: &[f64], v: f64, lower: bool) -> bool {
    let (lo, hi) = enclosure(&a.ast, a.unit);
    let bound = if lower { lo } else { hi };
    if !bound.is_finite() {
        return false;
    }
    let sign = if lower { 1.0 } else { -1.0 };
    refine_extremes(a, xs, ys, sign, 8).iter().any(|&(x, y)| {
        let n = a.noise(x) + 4.0 * ulp(v);
        (y - v).abs() <= n && (bound - v).abs() <= n
    })
}

/// A closed range bound v (a lower one if `lower`) taken at a turn of a
/// dying oscillation, sin(x)/x's −0.217234: the most extreme refined sample
/// takes it, and beyond it, decade after decade on each side, f's extreme
/// keeps strictly further from v, so no later turn reaches it. (The
/// extremes of sin x + sin(√2·x) don't keep away, nor do sin(x²)'s.)
fn damped_turn(a: &Analysed, xs: &[f64], ys: &[f64], v: f64, lower: bool) -> bool {
    let sign = if lower { 1.0 } else { -1.0 };
    let Some(&(xt, yt)) = refine_extremes(a, xs, ys, sign, 4).first() else {
        return false;
    };
    if !(yt == v || same_shown(yt, v) || (yt - v).abs() <= a.noise(xt)) {
        return false;
    }
    let k0 = xt.abs().max(1.0).log10().ceil() as i32 + 1;
    let mut decades = 0;
    for s in [1.0, -1.0] {
        let mut prev = 0.0f64;
        for k in k0..=15 {
            if a.over() {
                return false;
            }
            let m = (0..64)
                .map(|j| {
                    let x = s
                        * 10f64.powf(k as f64 + j as f64 / 64.0)
                        * (1.0 + 0.0137 * (j % 3) as f64);
                    sign * a.eval(x)
                })
                .filter(|y| y.is_finite())
                .fold(f64::INFINITY, f64::min);
            if !m.is_finite() {
                continue;
            }
            let gap = m - sign * v;
            // (Equal once the floats can't tell the extremes apart.)
            if gap <= 0.0 || gap < prev {
                return false;
            }
            prev = gap;
            decades += 1;
        }
    }
    decades >= 4
}

/// At a removable hole (an excluded point where f tends to a value L from
/// both sides), a range that keeps L claims f takes it somewhere else: a
/// sample there, or f crossing it between two samples, away from the hole.
/// Otherwise nothing stands behind it (10⁶ + 10⁻⁶·(x + 1)/(x² − 1) leaves
/// out 10⁶ − 5·10⁻⁷ as (x + 1)/(x² − 1) leaves out −1/2).
fn hole_values_attained(a: &Analysed, xs: &[f64], ys: &[f64]) -> bool {
    let d = a.d();
    for f in &d.excluded {
        let c = f.x;
        if !c.is_finite() {
            continue;
        }
        // The same value from both sides, at two scales (beside a pole of
        // even order the sides agree, but not the scales).
        let pts: Vec<f64> = [-4096, -64, 64, 4096]
            .iter()
            .map(|&k| nudge(c, k))
            .collect();
        let vals: Vec<f64> = pts.iter().map(|&x| a.eval(x)).collect();
        let n = pts.iter().map(|&x| a.noise(x)).fold(0.0, f64::max);
        let (lo, hi) = vals
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &v| {
                (l.min(v), h.max(v))
            });
        if !(lo.is_finite() && hi.is_finite()) || hi - lo > 2.0 * n {
            continue;
        }
        let v = 0.5 * (vals[1] + vals[2]);
        if !set_has(&d.range, v) {
            continue;
        }
        let away = |x: f64| {
            let near = |c: f64| (x - c).abs() <= 1e-6 * c.abs().max(1.0);
            !a.copies(f).iter().any(|&c| near(c))
        };
        let n = xs.len();
        let ok = |i: usize| ys[i].is_finite() && away(xs[i]);
        let crossed = (0..n).any(|i| {
            ok(i)
                && (ys[i] == v
                    || (i + 1 < n && ok(i + 1) && (ys[i] - v).signum() != (ys[i + 1] - v).signum()))
        });
        // Or within the noise of it, among the closest samples.
        let close = || {
            let mut idx: Vec<usize> = (0..n).filter(|&i| ok(i)).collect();
            idx.sort_by(|&i, &j| (ys[i] - v).abs().total_cmp(&(ys[j] - v).abs()));
            idx.iter()
                .take(32)
                .any(|&i| (ys[i] - v).abs() <= a.noise(xs[i]))
        };
        if !crossed && !close() {
            return false;
        }
    }
    true
}

/// Whether the values f is seen to take cover the claimed range. Between
/// neighbouring samples on a continuous stretch, f takes every value in
/// between; towards a pole (or past the last sample, out to 10³⁰⁰) it goes
/// on the way it was going, as far as the claimed piece goes. A stretch of
/// the claimed range none of that reaches, wider than the noise at its
/// edges, is a claim nothing stands behind: the gap of 10⁶ + 10⁻⁶·(x +
/// 1/x) that a range of ℝ leaves out, say.
#[allow(clippy::needless_range_loop)]
fn range_supported(a: &Analysed, xs: &[f64], ys: &[f64], centres: &[f64]) -> bool {
    let range = &a.d().range;
    if range.is_empty() || range.iter().all(|iv| iv.lo.value == iv.hi.value) {
        return true;
    }
    let reach = centres.iter().fold(1.0f64, |m, c| m.max(c.abs()));
    // The samples, with the far tails.
    let mut pts: Vec<(f64, f64)> = xs.iter().copied().zip(ys.iter().copied()).collect();
    for s in [1.0, -1.0] {
        for x in [(1e6 * reach).max(1e30), 1e100, 1e300] {
            pts.push((s * x, a.eval(s * x)));
        }
    }
    pts.sort_by(|p, q| p.0.total_cmp(&q.0));
    let piece = |v: f64| {
        range
            .iter()
            .find(|iv| iv_has(iv, v) || iv.lo.value == v || iv.hi.value == v)
    };
    // Covered values: [lo, hi] with the x where each end is taken.
    let mut cover: Vec<(f64, f64, f64, f64)> = Vec::new();
    let n = pts.len();
    let finite = |i: usize| pts[i].1.is_finite() && pts[i].1.abs() < f64::MAX;
    let step = |i: usize, j: usize| (pts[j].1 - pts[i].1).abs();
    // Whether f was taken to be continuous from each sample to the next.
    let mut joined = vec![false; n];
    for i in 0..n {
        if !finite(i) {
            continue;
        }
        let (x0, y0) = pts[i];
        cover.push((y0, y0, x0, x0));
        // To the next sample, if f is continuous between (no pole in
        // between: a step far beyond its neighbours' needs f at the
        // quarter points to stay within reach of its ends).
        if i + 1 < n && finite(i + 1) {
            let (x1, y1) = pts[i + 1];
            let jump = step(i, i + 1);
            let around = [
                i.checked_sub(1).map(|h| (h, i)),
                (i + 2 < n).then_some((i + 1, i + 2)),
            ]
            .into_iter()
            .flatten()
            .filter(|&(h, k)| finite(h) && finite(k))
            .map(|(h, k)| step(h, k))
            .fold(0.0f64, f64::max);
            let continuous = jump <= 4.0 * around || {
                let (lo, hi) = (y0.min(y1) - jump, y0.max(y1) + jump);
                [0.25, 0.5, 0.75].iter().all(|t| {
                    let v = a.eval(x0 + (x1 - x0) * t);
                    v.is_finite() && v >= lo && v <= hi
                })
            };
            if continuous {
                joined[i] = true;
                if y0 <= y1 {
                    cover.push((y0, y1, x0, x1));
                } else {
                    cover.push((y1, y0, x1, x0));
                }
            }
        }
        // The end of a stretch (an undefined or unbounded neighbour, or the
        // last sample): f goes on the way it was going, to the end of its
        // claimed piece.
        let mut inners = Vec::new();
        if i > 0 && finite(i - 1) && (i + 1 >= n || !finite(i + 1)) {
            inners.push(i - 1);
        }
        if i + 1 < n && finite(i + 1) && (i == 0 || !finite(i - 1)) {
            inners.push(i + 1);
        }
        for j in inners {
            let dir = (y0 - pts[j].1).signum();
            if let Some(iv) = piece(y0) {
                if dir > 0.0 {
                    cover.push((y0, iv.hi.value, x0, x0));
                } else if dir < 0.0 {
                    cover.push((iv.lo.value, y0, x0, x0));
                }
            }
        }
    }
    // Also both ends of every stretch the samples end on (a pole between
    // two samples): each side keeps going.
    for i in 0..n.saturating_sub(1) {
        if !(finite(i) && finite(i + 1)) {
            continue;
        }
        if joined[i] {
            continue;
        }
        for (k, j) in [(i, i.wrapping_sub(1)), (i + 1, i + 2)] {
            if j < n && finite(j) {
                let (xk, yk) = pts[k];
                let dir = (yk - pts[j].1).signum();
                if let Some(iv) = piece(yk) {
                    if dir > 0.0 {
                        cover.push((yk, iv.hi.value, xk, xk));
                    } else if dir < 0.0 {
                        cover.push((iv.lo.value, yk, xk, xk));
                    }
                }
            }
        }
    }
    // Far out, a tail that swings both ways (x·sin x) goes on doing so.
    for s in [1.0, -1.0] {
        let tail: Vec<usize> = (0..n)
            .filter(|&i| s * pts[i].0 >= 1e15 && finite(i))
            .collect();
        let ys: Vec<f64> = tail.iter().map(|&i| pts[i].1).collect();
        let up = ys.windows(2).any(|w| w[1] > w[0]);
        let down = ys.windows(2).any(|w| w[1] < w[0]);
        // (Growing as it swings: a bounded tail past the floats' resolution,
        // trig of 10³⁰, swings at random and says nothing.)
        let grows = match (ys.first(), ys.last()) {
            (Some(f), Some(l)) => l.abs() > 1e3 * f.abs().max(f64::MIN_POSITIVE),
            _ => false,
        };
        if up && down && grows {
            for &i in &tail {
                if let Some(iv) = piece(pts[i].1) {
                    cover.push((iv.lo.value, iv.hi.value, pts[i].0, pts[i].0));
                }
            }
        }
    }
    cover.sort_by(|p, q| p.0.total_cmp(&q.0));
    // The least noise where f takes v (an end may be taken where the noise
    // is unbounded, past an overflowing intermediate, and also where it
    // isn't).
    let least_noise = |v: f64| {
        cover
            .iter()
            .flat_map(|c| [(c.0, c.2), (c.1, c.3)])
            .filter(|&(w, x)| w == v && x.is_finite())
            .map(|(_, x)| a.noise(x))
            .fold(f64::INFINITY, f64::min)
            .min(if v.is_finite() { f64::INFINITY } else { 0.0 })
    };
    // Walk each claimed piece: every stretch of it must be reached.
    for iv in range {
        if iv.lo.value == iv.hi.value {
            continue;
        }
        let mut at = iv.lo.value;
        for &(lo, hi, _, _) in &cover {
            if hi < at {
                continue;
            }
            if lo > iv.hi.value {
                break;
            }
            // A gap (at, lo): one f is never seen in, wider than the noise
            // where its edges are taken.
            if lo > at
                && (at == f64::NEG_INFINITY
                    || (lo - at > 4.0 * (ulp(at) + ulp(lo))
                        && lo - at > least_noise(at) + least_noise(lo)))
            {
                return false;
            }
            at = at.max(hi);
            if at >= iv.hi.value {
                break;
            }
        }
        if at < iv.hi.value && (iv.hi.value == f64::INFINITY || iv.hi.value - at > least_noise(at))
        {
            return false;
        }
    }
    true
}

/// f's swings about a claimed horizontal asymptote shrink decade after
/// decade (sin(x)/x about 0) while f isn't that constant: f has a limit,
/// and a periodic function with one is constant, so f is not periodic.
fn dies_away(a: &Analysed) -> bool {
    use super::AsymptoteSide::*;
    a.d().horizontal_asymptotes.iter().any(|&(l, side)| {
        let sides: &[f64] = match side {
            PositiveInfinity => &[1.0],
            NegativeInfinity => &[-1.0],
            AnyInfinity => &[1.0, -1.0],
            Unknown => &[],
        };
        sides.iter().any(|&s| {
            let mut prev = f64::INFINITY;
            let mut run = 0;
            for k in 1..=14 {
                let m = (0..40)
                    .map(|j| {
                        let x = s
                            * 10f64.powf(k as f64 + j as f64 / 40.0)
                            * (1.0 + 0.0137 * (j % 3) as f64);
                        (a.eval(x) - l).abs()
                    })
                    .filter(|d| d.is_finite())
                    .fold(0.0f64, f64::max);
                if m > 0.0 && m < prev {
                    run += 1;
                    if run >= 4 {
                        return true;
                    }
                } else {
                    run = 0;
                }
                prev = m;
            }
            false
        })
    })
}

/// |f| over a decade (40 points), median, keeps growing at least twofold
/// for three decades running on one side.
fn grows(a: &Analysed) -> bool {
    [1.0, -1.0].iter().any(|&s| {
        let mut medians = Vec::new();
        for k in 0..=14 {
            let mut v: Vec<f64> = (0..40)
                .map(|j| {
                    a.eval(
                        s * 10f64.powf(k as f64 + j as f64 / 40.0)
                            * (1.0 + 0.0137 * (j % 3) as f64),
                    )
                })
                .filter(|y| y.is_finite())
                .map(f64::abs)
                .collect();
            if v.len() < 20 {
                medians.push(f64::NAN);
                continue;
            }
            v.sort_by(f64::total_cmp);
            medians.push(v[v.len() / 2]);
        }
        let mut run = 0;
        for w in medians.windows(2) {
            // (Growth that leaves the doubles, e^(x/10)·sin x, has grown.)
            if w[0] > 0.0 && w[0] < f64::MAX && w[1] >= 2.0 * w[0] {
                run += 1;
                if run >= 3 || (run >= 2 && w[1] == f64::MAX) {
                    return true;
                }
            } else {
                run = 0;
            }
        }
        false
    })
}

/// Forgets the claims of `k` that `expr` (at `opts`) contradicts, that the
/// checks couldn't finish within the gate's budget, or that sampling can't
/// stand behind ([`unjustified`]): their rows say "Unable to calculate …".
pub(crate) fn gate(
    k: KeyGraphFeatures,
    expr: &Expr,
    opts: &CompileOptions<'_>,
    cancel: Option<&AtomicBool>,
) -> Result<KeyGraphFeatures, super::engine::Stop> {
    gate_with(k, expr, opts, cancel, true).map(|(k, _)| k)
}

/// [`gate`] on an ungated analysis, with its report: what it dropped and
/// why, and how much work it took. For looking into a result.
pub fn gate_report(
    k: KeyGraphFeatures,
    expr: &Expr,
    opts: &CompileOptions<'_>,
) -> (KeyGraphFeatures, Report, u64) {
    let spent = Cell::new(0);
    let (k, r) = gate_with_spent(k, expr, opts, None, false, &spent)
        .unwrap_or_else(|_| unreachable!("not cancellable"));
    (k, r, spent.get())
}

fn gate_with(
    k: KeyGraphFeatures,
    expr: &Expr,
    opts: &CompileOptions<'_>,
    cancel: Option<&AtomicBool>,
    quiet: bool,
) -> Result<(KeyGraphFeatures, Report), super::engine::Stop> {
    gate_with_spent(k, expr, opts, cancel, quiet, &Cell::new(0))
}

fn gate_with_spent(
    k: KeyGraphFeatures,
    expr: &Expr,
    opts: &CompileOptions<'_>,
    cancel: Option<&AtomicBool>,
    quiet: bool,
    spent: &Cell<u64>,
) -> Result<(KeyGraphFeatures, Report), super::engine::Stop> {
    if k.analysis_error != AnalysisError::NoError {
        return Ok((k, Report::default()));
    }
    let mut a = Analysed::new(String::new(), k, expr, opts).with_budget(GATE_BUDGET, cancel);
    a.strict = true;
    let centres = centres_of(&a.ast);
    let xs = gate_samples(&a, &centres);
    let ys: Vec<f64> = xs.iter().map(|&x| a.eval(x)).collect();
    let mut r = Report {
        quiet,
        ..Report::default()
    };
    if !quiet {
        r.work.push((0, a.spent(), xs.len() as u32));
    }
    let mut dropped = 0;
    // Dropping a claim changes what the others are checked against (an
    // extremum's neighbourhood reaches as far as the next reported pole):
    // again until nothing more goes.
    for _ in 0..3 {
        let mut unverified = 0;
        for (vouches, step) in steps() {
            if a.over() {
                unverified |= vouches;
                continue;
            }
            let before = a.spent();
            step(&a, &xs, &ys, &centres, &mut r);
            if !quiet {
                r.work.push((vouches, a.spent() - before, r.refuted));
            }
            if a.over() {
                unverified |= vouches;
            }
        }
        let mut drop = r.refuted | unverified;
        let mut ruled = 0;
        if !a.over() {
            ruled = unjustified(&a, &xs, &ys, &centres);
            drop |= ruled;
        }
        if a.over() {
            // Out of time: nothing unchecked stays.
            unverified |= ALL;
            drop |= ALL;
        }
        r.dropped.0 |= r.refuted & !dropped;
        r.dropped.1 |= unverified & !dropped & !r.refuted;
        r.dropped.2 |= ruled & !dropped & !r.refuted & !unverified;
        if a.cancelled() {
            return Err(super::engine::Stop::Cancelled);
        }
        // What is said per period goes with the period.
        if drop & flags::PERIODICITY != 0 && a.k.data.period.is_some() {
            drop |= PER_PERIOD;
        }
        let new = drop & !dropped;
        if new == 0 {
            break;
        }
        dropped |= new;
        apply(&mut a.k, new);
    }
    spent.set(a.spent());
    Ok((a.k, r))
}

/// Every feature's flag.
const ALL: u32 = (1 << 13) - 1;

/// Features stated within one period, which go if the period goes.
const PER_PERIOD: u32 = flags::DOMAIN
    | flags::ZEROS
    | flags::MINIMA
    | flags::MAXIMA
    | flags::INFLECTION_POINTS
    | flags::VERTICAL_ASYMPTOTES
    | flags::MONOTONE_INTERVALS;

/// Makes the features in `drop` unknown.
fn apply(k: &mut KeyGraphFeatures, drop: u32) {
    super::engine::forget(k, drop);
    if drop & flags::PARITY != 0 {
        k.parity = Parity::Unknown;
    }
    if drop & flags::PERIODICITY != 0 {
        k.periodicity_direction = super::Periodicity::Unknown;
        k.periodicity_expression.clear();
        k.data.period = None;
    }
}
