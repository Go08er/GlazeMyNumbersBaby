//! Checks of an analysis against its function, and the runtime gate built
//! on them: a claim the function's own values contradict is not shown; its
//! row says "Unable to calculate …" instead.
//!
//! The truth is the function itself ([`super::truth`]): the compiled
//! program's double where no intermediate left the doubles on the way
//! (`Redo::No` and a normal result: then it is the same operations the
//! reference does), and everywhere else the reference evaluator: undefined
//! (NaN), exactly 0, a double, or a nonzero value beyond the doubles (whose
//! double stand-in, ±5·10⁻³²⁴ or ±the largest double, keeps only its sign
//! and side; claims about such values are checked on the reference's own
//! numbers, in logarithms of their sizes where needed). Every tolerance is
//! local, and these are all of them:
//!
//! * `noise(x)`: f's change to its nearest other value at least two floats
//!   either side of x (a feature at the neighbouring float is as good as the
//!   floats allow; in a shifted frame f only changes every so many floats),
//!   plus a first-order bound on the rounding of each of f's operations at
//!   x, propagated through the tree. Where that bound can't be had (the
//!   plain tree leaves the doubles: e^718 in 1/(1 + e^718)), the
//!   reference's change to its neighbours and 64 ulps of the value. Beyond
//!   the doubles, the same in the reference's arithmetic, relative, plus
//!   2⁻⁴⁰ of the value for its own rounding (exp of a large argument): no
//!   double other than the stand-in is within it, unless f's neighbours
//!   differ from it by as much as it is. Values closer than the noise can't
//!   be told apart.
//! * A discrepancy let pass only because the noise there is unbounded
//!   (`Analysed::forgives`) leaves what it was checked for unverified, and
//!   the gate drops it; unbounded noise is never evidence for a claim
//!   (`Analysed::within`).
//! * `resolution(x)`: how far f's argument must move for f to change. Which
//!   side of a domain boundary x is on, and whether a sign change is a
//!   crossing, a pole or a jump, are judged at that scale.
//! * Points within four floats (or the rounding of a periodic family's
//!   copies) of a reported end or excluded point: the precision of that
//!   end, not a claim about the point.
//! * `same_shown`: a value the panel writes the same way is the same claim
//!   (its own formatters: whole numbers in full to 10¹⁵, others to 6
//!   significant digits; 545843449.45 reported as 545843449 reads
//!   "545843449" either way, 9999999999999 and 10¹³ don't). Only for values
//!   (range bounds, extremum values, y-intercepts, asymptote values);
//!   shapes (pieces, open or closed, constant or not, a point or an
//!   interval) and positions get no such allowance, and a claimed 0 none at
//!   all.
//! * x + P is rounded and P stands for the true period to half an ulp: the
//!   period check compares f across that reach.
//!
//! Nothing is relative to the function's size elsewhere, to a shift or an
//! offset, or to anything the analysis engine estimates.
//!
//! The checks sample where they are told to ([`gate`] picks points for the
//! app; the sweep, `examples/sweep.rs`, many more) and can only refute: a
//! claim that passes is not thereby proved. What sampling can't refute,
//! [`gate`] makes unknown by rule. The gate's samples ([`gate_samples`]):
//! 0.005 apart to ±10, the integers to ±400, ten a decade out to ±10¹⁵, a
//! 0.05 grid to ±200 for f with trig, floor, ceil, round or mod, and around
//! each centre ([`centres_in`]: ± every part of the expression without x,
//! evaluated, and their pairwise sums, differences, products and
//! quotients; the [`MAX_NUMBERS`] largest numbers, at most [`MAX_CENTRES`]
//! centres, none beyond 10¹⁵) the same grids again plus its neighbouring
//! floats, and the floats around each reported feature (at most 256
//! features). Where a check tests a few hundred points of many, it tests
//! the largest deviations first. It runs the checks up to three times
//! (dropping a claim changes what the others are checked against), within
//! [`GATE_BUDGET`] units of work, polling cancellation as it goes; what it
//! couldn't finish is dropped, never kept.
//!
//! Evidence a check finds but can't settle (a sampled turn it can't place,
//! a candidate past its caps nothing accounts for: 48 candidate turns get
//! the full tests for a periodic f, 176 otherwise, and 512 more the cheaper
//! ones) leaves the feature unverified ([`Report::doubt`]), never
//! discarded. A value only the compiled program has, the reference
//! abstaining (or a constant part folding to another double), is no
//! evidence for anything: what is checked with it is unverified. And where
//! the domain is unknown, so are the "none" answers found by scanning it.

#![allow(missing_docs)]

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};

use super::format::{format_nonzero, format_number};
use super::truth::*;
use super::{
    AnalysisData, AnalysisError, AsymptoteSide, Family, Interval, KeyGraphFeatures, Monotonicity,
    Parity, flags,
};
use crate::ast::Expr;
use crate::compile::{CompileOptions, Program, Redo};
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
    /// Whether every constant part of f folds, in the compiled program, to
    /// the double the reference has for it: only then can a compiled value
    /// reached without leaving the doubles stand for f's
    /// ([`Analysed::faithful`]). ln(e^(10¹⁵)⁴) − 4·10¹⁵ folds to 1/2, an
    /// exponent's fraction lost; the reference abstains.
    folds_faithfully: bool,
    cost: u64,
    /// The cost of a walk of the expression's tree (the reference, the
    /// rounding bound): its nodes, weighted as the compiler weighs them.
    tree_cost: u64,
    spent: Cell<u64>,
    limit: u64,
    cancel: Option<&'c AtomicBool>,
    cancelled: Cell<bool>,
    noises: RefCell<Memo<f64>>,
    rels: RefCell<Memo<f64>>,
    refs: RefCell<Memo<R>>,
    vals: RefCell<Memo<f64>>,
    /// Points where the reference abstains and only the compiled program
    /// has a value: no evidence for any claim ([`Analysed::eval`]).
    abstained: RefCell<Memo<()>>,
    /// Some discrepancy was let pass only because the noise where it was
    /// seen is unbounded ([`Analysed::forgives`]): what it was checked for
    /// is unverified.
    unbounded: Cell<bool>,
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
        let tree_cost = tree_cost(&ast).max(1);
        let folds_faithfully = constants_fold_faithfully(&ast, &plain);
        Analysed {
            expr: expr.into(),
            k,
            ast,
            unit: opts.trig_unit,
            strict: false,
            folds_faithfully,
            cost: f.as_ref().map_or(1, |p| p.cost().max(1) as u64),
            tree_cost,
            f,
            spent: Cell::new(0),
            limit: u64::MAX,
            cancel: None,
            cancelled: Cell::new(false),
            noises: Default::default(),
            rels: Default::default(),
            refs: Default::default(),
            vals: Default::default(),
            abstained: Default::default(),
            unbounded: Cell::new(false),
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
    /// Whether a discrepancy `d` at x is within f's noise there. Where that
    /// noise is unbounded (an intermediate past the doubles that the plain
    /// tree can't bound), nothing is: the discrepancy is let pass, but what
    /// it was checked for is marked unverified ([`Analysed::take_unbounded`]),
    /// and the gate drops it.
    pub fn forgives(&self, x: f64, d: f64) -> bool {
        self.forgives_n(self.noise(x), d)
    }
    /// [`Analysed::forgives`] against a noise already worked out (the sum
    /// of several points').
    pub fn forgives_n(&self, n: f64, d: f64) -> bool {
        if d.is_nan() {
            return false;
        }
        if n.is_nan() || (n.is_infinite() && d > 0.0) {
            self.unbounded.set(true);
            return true;
        }
        d <= n
    }
    /// Whether `d` at x is within f's noise there, as evidence for a claim
    /// (a value attained, a zero, the same turn): unbounded noise is no
    /// evidence of anything.
    pub fn within(&self, x: f64, d: f64) -> bool {
        self.within_n(self.noise(x), d)
    }
    /// [`Analysed::within`] against a noise already worked out.
    pub fn within_n(&self, n: f64, d: f64) -> bool {
        n.is_finite() && d <= n
    }
    /// Whether some discrepancy was forgiven only by unbounded noise since
    /// the last call.
    pub fn take_unbounded(&self) -> bool {
        self.unbounded.replace(false)
    }
    /// The compiled program's value.
    pub fn raw(&self, x: f64) -> f64 {
        self.charge(self.cost);
        match &self.f {
            Some(f) => f.eval(x, 0.0),
            None => f64::NAN,
        }
    }
    /// The compiled program's value where it is certainly f's: a normal
    /// double reached without any intermediate leaving the doubles (no
    /// overflow, no underflow, nothing re-evaluated in extended range), by
    /// the same operations the reference does. Elsewhere the reference
    /// decides ([`Analysed::eval`]).
    fn faithful(&self, x: f64) -> Option<f64> {
        if !self.folds_faithfully {
            return None;
        }
        self.charge(self.cost);
        let (v, how) = self.f.as_ref()?.eval_tagged(x, 0.0);
        (how == Redo::No && v.is_normal()).then_some(v)
    }
    /// The reference's value at x (remembered: it is the slowest step).
    pub fn reference(&self, x: f64) -> R {
        if let Some(&r) = self.refs.borrow().get(&x.to_bits()) {
            return r;
        }
        self.charge(4 * self.tree_cost);
        let r = reval(&self.ast, x, self.unit);
        self.refs.borrow_mut().insert(x.to_bits(), r);
        r
    }
    /// The value at x by the plain tree, and the bound on its rounding
    /// ([`eb`]).
    pub fn bound(&self, x: f64) -> (f64, f64) {
        self.charge(8 * self.tree_cost);
        eb(&self.ast, x, self.unit)
    }
    /// [`Analysed::bound`] for a part of the function.
    pub fn bound_of(&self, e: &Expr, x: f64) -> (f64, f64) {
        self.charge(8 * tree_cost(e).max(1));
        eb(e, x, self.unit)
    }
    /// f(x), as the reference has it: undefined (NaN), exactly 0, a double
    /// (the compiled one, which the compiler's rewrites move by a few
    /// roundings at most; if it differs from the reference by more, an
    /// intermediate left the doubles on the way and it is no value of f:
    /// 1 + √(−e^(−x−800)) is undefined, not 1), or a nonzero value beyond
    /// the doubles (then the smallest or largest double of its sign,
    /// [`Xf::proxy`]; [`Analysed::beyond`] has the value).
    pub fn eval(&self, x: f64) -> f64 {
        if let Some(v) = self.faithful(x) {
            return v;
        }
        if let Some(&v) = self.vals.borrow().get(&x.to_bits()) {
            self.vouch(x);
            return v;
        }
        let v = self.eval_at(x);
        self.vals.borrow_mut().insert(x.to_bits(), v);
        self.vouch(x);
        v
    }
    /// f(x) where the reference vouches for it; None where only the compiled
    /// program has a value (no evidence either way, and nothing marked).
    pub fn known(&self, x: f64) -> Option<f64> {
        let was = self.unbounded.get();
        let v = self.eval(x);
        if self.abstained(x) {
            self.unbounded.set(was);
            return None;
        }
        Some(v)
    }
    /// Whether f(x) was taken from the compiled program alone, the
    /// reference abstaining.
    pub fn abstained(&self, x: f64) -> bool {
        self.abstained.borrow().contains_key(&x.to_bits())
    }
    /// A value only the compiled program has (the reference abstains: an
    /// exponent's fraction lost far beyond the doubles) is no truth to
    /// check a claim against: whatever is checked with it is unverified.
    fn vouch(&self, x: f64) {
        if self.abstained.borrow().contains_key(&x.to_bits()) {
            self.unbounded.set(true);
        }
    }
    fn eval_at(&self, x: f64) -> f64 {
        let y = self.raw(x);
        match self.reference(x) {
            R::Undef => f64::NAN,
            R::V(v) if v.is_zero() => 0.0,
            R::V(v) if v.normal() => {
                let f = v.f();
                if y.is_finite() && (y - f).abs() <= REWRITE * y.abs().max(f.abs()) {
                    y
                } else {
                    f
                }
            }
            R::V(v) => v.proxy(),
            // (A 0 the reference can't vouch for may be underflow.)
            R::Unknown if y.is_finite() && y != 0.0 => {
                self.abstained.borrow_mut().insert(x.to_bits(), ());
                y
            }
            R::Unknown => f64::NAN,
        }
    }
    /// f(x) when it is a nonzero value beyond the doubles (whose double
    /// stand-in, [`Analysed::eval`], keeps only its sign and side).
    pub fn beyond(&self, x: f64) -> Option<Xf> {
        if !is_stand_in(self.eval(x)) {
            return None;
        }
        match self.reference(x) {
            R::V(v) if !v.is_zero() && !v.normal() => Some(v),
            _ => None,
        }
    }
    /// Whether f is defined at x (None: the reference can't tell).
    pub fn defined(&self, x: f64) -> Option<bool> {
        if self.eval(x).is_normal() {
            return Some(true);
        }
        match self.reference(x) {
            R::V(_) => Some(true),
            R::Undef => Some(false),
            R::Unknown if self.raw(x).is_finite() => Some(true),
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
        if let Some(v) = self.beyond(x) {
            return self.noise_beyond(x, v);
        }
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
        let (r, err) = self.bound(x);
        if !(y.is_finite() && r.is_finite() && err.is_finite()) {
            // The plain tree leaves the doubles on the way (e^718 in
            // 1/(1 + e^718)), so its rounding bound says nothing: where the
            // reference has the value, its own rounding and its change to
            // its neighbours bound it, relative to it. Elsewhere nothing
            // does.
            // (Its own rounding: 64 ulps of it, a few an operation; a value
            // in the subnormal range is rounded to them too.)
            return match self.reference(x) {
                R::V(v) if !v.is_zero() && y.is_finite() && y != 0.0 => {
                    let change = (self.rel_beyond(x, v) - REFERENCE_ROUNDING).clamp(0.0, 1.0);
                    spread + y.abs() * change + 64.0 * ulp(y)
                }
                _ => f64::INFINITY,
            };
        }
        // The compiler folds and rewrites (a/bᵏ as a·b⁻ᵏ): its double can
        // differ from the plain tree's by their roundings.
        spread + err + (r - y).abs()
    }
    /// The noise of a value beyond the doubles, in units of its double
    /// stand-in: the stand-in itself where f is indistinguishable from 0
    /// there ([`Analysed::rel_beyond`] at least 1: its neighbours differ
    /// from it by as much as it is, a zero of e^(−1000)·(x − 1/3)² between
    /// floats, or e^(−x²) at 10⁹, which changes by a factor e⁴⁷⁷ from one
    /// float to the next), and otherwise that fraction of it, which is no
    /// double at all: any double other than the stand-in is resolvably not
    /// f's value.
    fn noise_beyond(&self, x: f64, v: Xf) -> f64 {
        v.proxy().abs() * self.rel_beyond(x, v).min(1.0)
    }
    /// f's change from v = f(x) to its nearest other value at least two
    /// floats either side, relative to v, plus the reference's rounding
    /// ([`REFERENCE_ROUNDING`]): e^([`Analysed::log_beyond`]) − 1.
    pub fn rel_beyond(&self, x: f64, v: Xf) -> f64 {
        self.log_beyond(x, v).exp_m1()
    }
    /// ln(1 + |w − v|/|v|) for f's nearest other value w at least two floats
    /// either side of x (the larger side), in the reference's own
    /// arithmetic, plus the reference's rounding: f's noise at x as a
    /// factor, finite even where f changes by e⁴⁷⁷ from one float to the
    /// next (e^(−x²) at 10⁹).
    pub fn log_beyond(&self, x: f64, v: Xf) -> f64 {
        let key = x.to_bits();
        if let Some(&r) = self.rels.borrow().get(&key) {
            return r;
        }
        let mut l = 0.0f64;
        for dir in [-1i64, 1] {
            let mut k = 1i64;
            while k <= 1 << 40 {
                let R::V(w) = self.reference(nudge(x, dir * k)) else {
                    break;
                };
                let diff = w.add(v.neg());
                if !diff.is_zero() && k >= 2 {
                    l = l.max(v.abs().add(diff.abs()).ln() - v.abs().ln());
                    break;
                }
                k *= 2;
            }
        }
        let r = l + REFERENCE_ROUNDING;
        self.rels.borrow_mut().insert(key, r);
        r
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

/// Whether a value of [`Analysed::eval`] may stand in for one beyond the
/// doubles ([`Xf::proxy`]).
/// Whether each largest part of `e` without x compiles to the double the
/// reference has for it (or both say it is undefined).
fn constants_fold_faithfully(e: &Expr, opts: &CompileOptions<'_>) -> bool {
    if !e.contains_x() {
        let folded = Program::compile(e, opts).ok().and_then(|p| p.as_constant());
        return match (reval(e, 0.0, opts.trig_unit), folded) {
            (R::V(v), Some(c)) if v.is_zero() => c == 0.0,
            (R::V(v), Some(c)) if v.normal() => {
                let f = v.f();
                (c - f).abs() <= REWRITE * c.abs().max(f.abs())
            }
            (R::Undef, None) => true,
            (R::Undef, Some(c)) => c.is_nan(),
            _ => false,
        };
    }
    match e {
        Expr::Bin(_, l, r) => {
            constants_fold_faithfully(l, opts) && constants_fold_faithfully(r, opts)
        }
        Expr::Call(_, args) => args.iter().all(|a| constants_fold_faithfully(a, opts)),
        Expr::Neg(x) | Expr::Degrees(x) => constants_fold_faithfully(x, opts),
        _ => true,
    }
}

fn is_stand_in(y: f64) -> bool {
    y.abs() == f64::from_bits(1) || y.abs() == f64::MAX
}

/// What was worked out at each x (by the bits of x).
type Memo<T> = HashMap<u64, T, std::hash::BuildHasherDefault<BitsHasher>>;

/// A hasher for the bits of a double: one multiplication (the keys are
/// already spread, a cryptographic hash would cost more than the values).
#[derive(Default)]
struct BitsHasher(u64);

impl std::hash::Hasher for BitsHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(8) ^ b as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = (v ^ (v >> 29)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

/// How far, relative to it, a compiled double may be from the reference's
/// value of the same expression through the compiler's rewrites (folding,
/// a/bᵏ as a·b⁻ᵏ): a few roundings, far below this.
const REWRITE: f64 = 1.0 / (1u64 << 40) as f64;

/// The reference's own rounding of a value beyond the doubles, relative to
/// it: a few roundings an operation, and e^t's reduction of a large t.
const REFERENCE_ROUNDING: f64 = 1.0 / (1u64 << 40) as f64;

/// The work of one walk of e's tree, in the units of a program's cost.
fn tree_cost(e: &Expr) -> u64 {
    use crate::ast::BinOp;
    match e {
        Expr::Neg(a) | Expr::Degrees(a) => 1 + tree_cost(a),
        Expr::Bin(op, a, b) => {
            let own = match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul => 1,
                BinOp::Div => 2,
                _ => 8,
            };
            own + tree_cost(a) + tree_cost(b)
        }
        Expr::Call(_, args) => 8 + args.iter().map(tree_cost).sum::<u64>(),
        _ => 1,
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

/// A value that reads the same on the panel is the same claim: the
/// panel's own formatters ([`format_number`], and [`format_nonzero`] where
/// a tiny value must not read as 0) both write them alike. Whole numbers
/// are written in full up to 10¹⁵ (9999999999999 is not 10000000000000),
/// other values to 6 significant digits (545843449.45 reads as 545843449).
/// Positions and shapes (how many pieces, open or closed, constant or not,
/// a point or an interval) get no such allowance.
pub fn same_shown(a: f64, b: f64) -> bool {
    a == b
        || (a.is_finite()
            && b.is_finite()
            && format_number(a) == format_number(b)
            && format_nonzero(a) == format_nonzero(b))
}

/// The allowance for a claimed value's own last digit: a few units of it,
/// none for a claimed 0 (a value is 0 or it isn't).
pub fn slack(v: f64) -> f64 {
    if v == 0.0 { 0.0 } else { 4.0 * ulp(v) }
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
    /// [`flags`] of features a check found evidence against that it
    /// couldn't settle (a sampled turn it can't classify): no failure, but
    /// the gate doesn't keep them.
    pub unverified: u32,
    /// What that evidence was, per check (not counted as failures).
    pub doubts: BTreeMap<&'static str, Vec<String>>,
}

impl Report {
    /// Evidence against `flags` that a check can't settle either way.
    pub fn doubt(&mut self, check: &'static str, flags: u32, expr: &str, detail: String) {
        self.unverified |= flags;
        if !self.quiet {
            self.doubts
                .entry(check)
                .or_default()
                .push(format!("y={expr}    {detail}"));
        }
    }

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
    // (The farthest out of the range first: a few hundred are plenty, and
    // the first ones would otherwise be the rounding nearest the origin.)
    outside.sort_by(|&i, &j| set_dist(range, ys[j]).total_cmp(&set_dist(range, ys[i])));
    // (A few hundred are plenty: past that they are all within the noise.)
    outside.truncate(256);
    for (x, y) in outside.into_iter().map(|i| (xs[i], ys[i])) {
        let dist = set_dist(range, y);
        if dist > 0.0 && !shown_as_bound(a, range, y) && !a.forgives(x, dist) {
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
        if !set_has(range, y) && !shown_as_bound(a, range, y) && !a.forgives(x, set_dist(range, y))
        {
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
        y.is_finite() && (y == v || same_shown(y, v) || a.within(x, (y - v).abs()))
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
                && !a.forgives(fx.x, set_dist(range, *y))
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
        && !a.forgives(0.0, set_dist(range, y))
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
            if h.1 > l.1 && !a.forgives_n(l.2 + h.2, h.1 - l.1) {
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
        v.is_finite() && a.within(t, v.abs())
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
        y.is_finite() && (y == 0.0 || a.within(t, y.abs()))
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
        } else if ys[i] == 0.0 && matches!(a.reference(xs[i]), R::V(v) if v.is_zero()) {
            // 0 by the compiled program and the reference both, though
            // the rounding bound can't vouch for it ((x − 2⁴⁰)/(x − 2⁴⁰ −
            // 1/16) at 2⁴⁰, the bound charging 2⁴⁰ an ulp): a root to
            // account for. (If it isn't one, "none" only goes unknown.)
            roots.push(xs[i]);
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
                            && !a.forgives(xs[i], ys[i].abs())
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
                if !fv.is_finite()
                    || (!a.forgives_n(n0 + slack(*y), (fv - y).abs()) && !same_shown(fv, *y))
                {
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
                    if yb.is_finite()
                        && sign * (yb - fv) < 0.0
                        && !a.forgives_n(a.noise(xb) + n0, sign * (fv - yb))
                    {
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
    if !fp.is_finite() || !fq.is_finite() || !a.within_n(a.noise(p) + a.noise(q), (fp - fq).abs()) {
        return false;
    }
    let top = (sign * fp).max(sign * fq);
    (1..8).all(|i| {
        let t = p + (q - p) * i as f64 / 8.0;
        let y = a.eval(t);
        y.is_finite() && (sign * y <= top || a.within(t, sign * y - top))
    })
}

/// Whether the sampled candidate turn at `i` (between `l` and `h`, `p`
/// deep) looks like a turn of f rather than a pole between the samples,
/// the edge of a jump or a sawtooth's top: the cheaper tests of
/// [`check_missing_extrema`], at the sample itself.
fn plain_turn(
    a: &Analysed,
    xs: &[f64],
    ys: &[f64],
    (l, i, h): (usize, usize, usize),
    p: f64,
) -> bool {
    let (xi, yi) = (xs[i], ys[i]);
    if a.defined(0.5 * (xs[l] + xi)) != Some(true) || a.defined(0.5 * (xi + xs[h])) != Some(true) {
        return false;
    }
    let span = ys[l].abs().max(yi.abs()).max(ys[h].abs());
    let inner: Vec<f64> = (1..8)
        .map(|j| a.eval(xs[l] + (xs[h] - xs[l]) * j as f64 / 8.0))
        .collect();
    if inner.iter().any(|v| !v.is_finite() || v.abs() > 4.0 * span) {
        return false;
    }
    let res = a.resolution(xi);
    let step4 = 4.0 * if res.is_finite() { res } else { 16.0 * ulp(xi) };
    let (d1, d2) = (
        (a.eval(xi - step4) - yi).abs(),
        (a.eval(xi + step4) - yi).abs(),
    );
    if !(d1.is_finite() && d2.is_finite()) || d1.max(d2) > 0.5 * p + a.rounding(xi) {
        return false;
    }
    let swing = inner
        .iter()
        .map(|v| v - yi)
        .fold(0.0f64, |m, v| m.max(v.abs()));
    let mut near: Vec<f64> = [2, 4, 8, 16, 32, 64]
        .iter()
        .flat_map(|&k| [nudge(xi, -k), nudge(xi, k)])
        .collect();
    let delta = (xs[h] - xs[l]) / 128.0;
    near.extend((-8..=8).map(|j| xi + j as f64 * delta));
    near.sort_by(f64::total_cmp);
    let vals: Vec<f64> = near.iter().map(|&t| a.eval(t)).collect();
    let step = vals
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(
            0.0f64,
            |m, v| if v.is_nan() { f64::INFINITY } else { m.max(v) },
        );
    !(step.is_nan() || step > 0.25 * swing + a.rounding(xi))
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
        // A sample (or a run of equal ones: neighbouring floats at a peak
        // round to the same height) beyond the samples either side.
        let mut cand: Vec<(usize, usize, usize, f64)> = Vec::new();
        let mut s0 = 1;
        while s0 + 1 < n {
            let mut t = s0;
            while t + 1 < n && ys[t + 1] == ys[s0] {
                t += 1;
            }
            let (l, h) = (s0 - 1, t + 1);
            if h < n && ys[l].is_finite() && ys[s0].is_finite() && ys[h].is_finite() {
                let p = (sign * (ys[l] - ys[s0])).min(sign * (ys[h] - ys[s0]));
                if p > 0.0 {
                    cand.push((l, (s0 + t) / 2, h, p));
                }
            }
            s0 = t + 1;
        }
        // A run whose sides are within the noise of it is a flat top so
        // far: its sides are the nearest samples beyond that (the top of
        // 2·e^(−(x − 1522756)²), sampled a few floats either side, rounds
        // to 2 within 64 floats of it).
        let mut widened = 0;
        for c in cand.iter_mut() {
            let (l, i, h, p) = *c;
            let n0 = a.noise(xs[i]);
            if p > n0 + a.noise(xs[l]).max(a.noise(xs[h])) || widened >= 256 {
                continue;
            }
            widened += 1;
            let flat = |j: usize| {
                ys[j].is_finite()
                    && sign * (ys[j] - ys[i]) >= 0.0
                    && (ys[j] - ys[i]).abs() <= n0 + a.noise(xs[j])
            };
            let (mut l2, mut h2) = (l, h);
            for _ in 0..64 {
                if l2 == 0 || !flat(l2) {
                    break;
                }
                l2 -= 1;
            }
            for _ in 0..64 {
                if h2 + 1 >= n || !flat(h2) {
                    break;
                }
                h2 += 1;
            }
            if ys[l2].is_finite() && ys[h2].is_finite() {
                let p2 = (sign * (ys[l2] - ys[i])).min(sign * (ys[h2] - ys[i]));
                if p2 > p {
                    *c = (l2, i, h2, p2);
                }
            }
        }
        cand.sort_by(|x, y| y.3.total_cmp(&x.3));
        // The most prominent candidates get the full tests below. Past
        // them, a periodic f's are copies of what its claims (stated per
        // period) and those tests covered; a non-periodic f's get the
        // cheaper tests, at the sample itself: a reported turn of its kind
        // or a pole in its bracket accounts for one, and so does f jumping
        // or blowing up there. One nothing accounts for is a sampled turn
        // nobody reported, and the list can't be vouched for.
        let periodic = a.period().is_some();
        let rest = cand.split_off(cand.len().min(if periodic { 48 } else { 176 }));
        let inside = |f: &Family, l: usize, i: usize, h: usize| {
            let c = nearest(f, xs[i]);
            (c >= xs[l] && c <= xs[h]) || close_to(f, xs[l], 64) || close_to(f, xs[h], 64)
        };
        let mut looked = 0;
        for &(l, i, h, p) in rest.iter().filter(|_| !periodic) {
            if a.within_n(a.noise(xs[i]) + a.noise(xs[l]).max(a.noise(xs[h])), p)
                || list.iter().any(|(fx, _)| inside(fx, l, i, h))
                || d.vertical_asymptotes
                    .iter()
                    .chain(&d.excluded)
                    .any(|f| inside(f, l, i, h))
            {
                continue;
            }
            looked += 1;
            if looked > 512 || plain_turn(a, xs, ys, (l, i, h), p) {
                r.doubt(
                    "extrema-unchecked",
                    flag,
                    e,
                    format!(
                        "a {name} between {:?} and {:?} past the first 176 nothing accounts for",
                        xs[l], xs[h]
                    ),
                );
                break;
            }
        }
        let mut used: Vec<Option<f64>> = vec![None; list.len()];
        for (l, i, h, p) in cand {
            if a.forgives_n(a.noise(xs[i]) + a.noise(xs[l]).max(a.noise(xs[h])), p) {
                continue;
            }
            let (xi, yi) = (xs[i], ys[i]);
            let (mut xl, mut yl, mut xh, mut yh, mut p) = (xs[l], ys[l], xs[h], ys[h], p);
            // A bracket a few of f's resolution steps across can't tell a
            // narrow smooth peak from the edge of a jump (the top of
            // 2·e^(−(x − 1522756)²/10⁻⁶), sampled a few floats either side,
            // falls by as much four steps out as across the bracket): widen
            // it while f keeps falling away on both sides.
            let res = a.resolution(xi);
            let unit = if res.is_finite() { res } else { 16.0 * ulp(xi) };
            for _ in 0..48 {
                if (xi - xl).min(xh - xi) >= 1024.0 * unit {
                    break;
                }
                let (tl, th) = (xi - 2.0 * (xi - xl), xi + 2.0 * (xh - xi));
                let (vl, vh) = (a.eval(tl), a.eval(th));
                let q = (sign * (vl - yi)).min(sign * (vh - yi));
                if !(vl.is_finite() && vh.is_finite()) || q.is_nan() || q <= p {
                    break;
                }
                (xl, yl, xh, yh, p) = (tl, vl, th, vh, q);
            }
            let (m1, m2) = (0.5 * (xl + xi), 0.5 * (xi + xh));
            if a.defined(m1) != Some(true) || a.defined(m2) != Some(true) {
                continue;
            }
            // A turn of f, not a pole between the samples: f stays within
            // their size across the bracket.
            let span = yl.abs().max(yi.abs()).max(yh.abs());
            if (1..8).any(|j| {
                let t = xl + (xh - xl) * j as f64 / 8.0;
                let v = a.eval(t);
                !v.is_finite() || v.abs() > 4.0 * span
            }) {
                continue;
            }
            // (The sample itself if refining ends short of it, a float off
            // the flat top.)
            let (xm, ym) = match golden(a, xl, xh, sign) {
                (xm, ym) if ym.is_finite() && sign * ym <= sign * yi => (xm, ym),
                _ => (xi, yi),
            };
            // The edge of a jump or a pole, or a plateau, is no turn: there
            // f moves, a few steps of its resolution out, by as much as it
            // does across the whole bracket (the top of atan(tan(x)) − 10⁶
            // is a staircase; tan beside its pole; floor(cos x) between far
            // samples). At a turn that is a small part of it.
            let res = a.resolution(xm);
            let step4 = 4.0 * if res.is_finite() { res } else { 16.0 * ulp(xm) };
            let (d1, d2) = (
                (a.eval(xm - step4) - ym).abs(),
                (a.eval(xm + step4) - ym).abs(),
            );
            if !(d1.is_finite() && d2.is_finite()) || d1.max(d2) > 0.5 * p + a.rounding(xm) {
                continue;
            }
            // Nor where f moves, within a few dozen floats, by a good part
            // of its swing over the bracket (a sawtooth's top between far
            // samples).
            let swing = (1..8)
                .map(|j| a.eval(xl + (xh - xl) * j as f64 / 8.0) - ym)
                .filter(|v| v.is_finite())
                .fold(0.0f64, |m, v| m.max(v.abs()));
            let mut near: Vec<f64> = [2, 4, 8, 16, 32, 64]
                .iter()
                .flat_map(|&k| [nudge(xm, -k), nudge(xm, k)])
                .collect();
            let delta = (xh - xl) / 128.0;
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
            let (lo, hi) = (xl.min(xm - step4), xh.max(xm + step4));
            if d.vertical_asymptotes.iter().chain(&d.excluded).any(|v| {
                let c = nearest(v, xm);
                c >= lo && c <= hi || close_to(v, xm, 64)
            }) {
                continue;
            }
            // A turn whose own value is as uncertain as its height (the
            // floor of a cosine that rounds about 0), or that refining
            // can't place (golden section ending on an edge), can't be told
            // from noise: the list stands unverified, not vouched for.
            // (One a reported turn of its kind accounts for needs no
            // placing: the claim checks see to it.)
            let explained = list.iter().any(|(fx, _)| {
                let c = nearest(fx, xm);
                (c >= xl && c <= xh) || close_to(fx, xm, 64)
            });
            if a.rounding(xm) >= p {
                if explained {
                    continue;
                }
                r.doubt(
                    "extremum-unclassified",
                    flag,
                    e,
                    format!("{name}: turn at ({xm:?}, {ym:?}) no taller than its rounding"),
                );
                continue;
            }
            // (Not reaching past a reported pole: beyond it f is another
            // branch.)
            let pole_gap = d
                .vertical_asymptotes
                .iter()
                .chain(&d.excluded)
                .map(|f| (nearest(f, xm) - xm).abs())
                .fold(f64::INFINITY, f64::min);
            let local = [64.0 * ulp(xm), 1e-6 * xm.abs().max(1.0)].iter().all(|&h| {
                let h = h.min(0.5 * pole_gap);
                [xm - h, xm + h].iter().all(|&t| {
                    let v = a.eval(t);
                    v.is_finite() && sign * (v - ym) >= -(a.noise(t) + a.noise(xm))
                })
            });
            if !local {
                if !explained {
                    r.doubt(
                        "extremum-unclassified",
                        flag,
                        e,
                        format!("{name}: sampled turn near ({xi:?}, {yi:?}) not placed"),
                    );
                }
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

/// Between consecutive reported turns of a kind, f has none of that kind;
/// so no sample between them (or between one and a stretch's end: a closed
/// domain end, or a tail with its claimed asymptote) rises above both ends
/// (falls below both, for minima) by more than the noise. One that does is
/// a turn the list misses, or, where f jumps without the analysis saying
/// so, a supremum it never attains: either way the list can't be vouched
/// for. (Poles and excluded points end a stretch without bounding it.)
pub fn check_extrema_between(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
    let d = a.d();
    if a.period().is_some()
        || d.minima
            .iter()
            .chain(&d.maxima)
            .any(|(f, _)| f.period.is_some())
        || d.vertical_asymptotes
            .iter()
            .chain(&d.excluded)
            .any(|f| f.period.is_some())
    {
        return;
    }
    let e = &a.expr;
    let tail = |side: AsymptoteSide| -> Option<f64> {
        if a.unknown(flags::HORIZONTAL_ASYMPTOTES) {
            return None;
        }
        d.horizontal_asymptotes
            .iter()
            .find(|(_, s)| *s == side || *s == AsymptoteSide::AnyInfinity)
            .map(|(y, _)| *y)
    };
    // Where a stretch ends, and what f is there (None: it doesn't bound
    // the stretch).
    let mut stops: Vec<(f64, Option<f64>)> = vec![
        (f64::NEG_INFINITY, tail(AsymptoteSide::NegativeInfinity)),
        (f64::INFINITY, tail(AsymptoteSide::PositiveInfinity)),
    ];
    for f in d.vertical_asymptotes.iter().chain(&d.excluded) {
        stops.push((f.x, None));
    }
    if !a.unknown(flags::DOMAIN) {
        for iv in &d.domain {
            for b in [iv.lo, iv.hi] {
                if b.value.is_finite() {
                    let y = a.eval(b.value);
                    stops.push((b.value, (b.closed && y.is_finite()).then_some(y)));
                }
            }
        }
    } else {
        return;
    }
    for (list, sign, flag, name) in [
        (&d.minima, 1.0, flags::MINIMA, "minimum"),
        (&d.maxima, -1.0, flags::MAXIMA, "maximum"),
    ] {
        if a.unknown(flag) {
            continue;
        }
        let mut marks = stops.clone();
        for (f, y) in list {
            // The turn's value as the function has it there, or as claimed:
            // whichever is further out.
            let v = a.eval(f.x);
            let v = if v.is_finite() && sign * v < sign * y {
                v
            } else {
                *y
            };
            marks.push((f.x, Some(v)));
        }
        marks.sort_by(|p, q| p.0.total_cmp(&q.0));
        for w in marks.windows(2) {
            let ((x0, v0), (x1, v1)) = (w[0], w[1]);
            let (Some(v0), Some(v1)) = (v0, v1) else {
                continue;
            };
            // The end f stays on the inner side of: the lower of the two
            // for minima, the higher for maxima.
            let bound = if sign * v0 < sign * v1 { v0 } else { v1 };
            let from = xs.partition_point(|&x| x <= x0);
            let to = xs.partition_point(|&x| x < x1);
            let mut worst: Option<(f64, f64, f64)> = None;
            for k in from..to {
                let (x, y) = (xs[k], ys[k]);
                if !y.is_finite()
                    || floats_between(x, x0) <= 4
                    || floats_between(x, x1) <= 4
                    || sign * (bound - y) <= 0.0
                {
                    continue;
                }
                let past = sign * (bound - y);
                if worst.is_none_or(|(_, _, p)| past > p) {
                    worst = Some((x, y, past));
                }
            }
            let Some((x, y, past)) = worst else {
                continue;
            };
            let ends = [x0, x1]
                .iter()
                .filter(|v| v.is_finite())
                .map(|&v| a.noise(v))
                .fold(0.0f64, f64::max);
            if a.within_n(a.noise(x) + ends + slack(bound), past) || same_shown(y, bound) {
                continue;
            }
            r.doubt(
                "extrema-incomplete",
                flag,
                e,
                format!(
                    "f({x:?})={y:?} beyond both ends of ({x0:?}, {x1:?}) (bound {bound:?}) with no {name} reported between"
                ),
            );
            break;
        }
    }
}

/// Between consecutive reported turns of a kind, f has none of that kind:
/// on such a stretch, unbroken by a pole, an excluded point or an undefined
/// sample, no sample has a higher one on each side (for minima; a lower
/// one, for maxima) by more than the noise. A sampled valley (peak) there
/// is a turn the list misses (between the peaks of
/// 2·e^(−(x − 1522756)²/10⁻⁹) + e^(−x²) lies a trough far below the
/// doubles), or an infimum never attained at a jump: either way the list
/// can't be vouched for.
pub fn check_valleys(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
    let d = a.d();
    if a.period().is_some()
        || a.unknown(flags::VERTICAL_ASYMPTOTES)
        || a.unknown(flags::DOMAIN)
        || d.minima
            .iter()
            .chain(&d.maxima)
            .any(|(f, _)| f.period.is_some())
        || d.vertical_asymptotes
            .iter()
            .chain(&d.excluded)
            .any(|f| f.period.is_some())
    {
        return;
    }
    let e = &a.expr;
    let n = xs.len();
    let poles: Vec<f64> = d
        .vertical_asymptotes
        .iter()
        .chain(&d.excluded)
        .map(|f| f.x)
        .collect();
    for (list, sign, flag, name) in [
        (&d.minima, 1.0, flags::MINIMA, "minimum"),
        (&d.maxima, -1.0, flags::MAXIMA, "maximum"),
    ] {
        if a.unknown(flag) {
            continue;
        }
        // Where stretches end: poles, excluded points, and this list's own
        // turns (with the neighbourhood their claim check covers).
        let mut cuts: Vec<(f64, f64)> = poles.iter().map(|&c| (c, c)).collect();
        for (f, _) in list {
            let h = 1e-3 * f.x.abs().max(1.0);
            cuts.push((f.x - h, f.x + h));
        }
        let cut_between = |x0: f64, x1: f64| cuts.iter().any(|&(lo, hi)| hi >= x0 && lo <= x1);
        let in_cut = |x: f64| cuts.iter().any(|&(lo, hi)| x >= lo && x <= hi);
        // v = f for minima, −f for maxima: a valley of v is the turn.
        let mut k = 0;
        'runs: while k < n {
            // A run of defined samples with no cut inside.
            let s0 = k;
            while k < n
                && ys[k].is_finite()
                && !in_cut(xs[k])
                && (k == s0 || !cut_between(xs[k - 1], xs[k]))
            {
                k += 1;
            }
            let run = s0..k;
            if k == s0 {
                k += 1;
            }
            if run.len() < 3 {
                continue;
            }
            let v: Vec<f64> = run.clone().map(|j| sign * ys[j]).collect();
            let m = v.len();
            let mut pre = vec![(f64::NEG_INFINITY, 0usize); m];
            for j in 1..m {
                pre[j] = if v[j - 1] > pre[j - 1].0 {
                    (v[j - 1], j - 1)
                } else {
                    pre[j - 1]
                };
            }
            let mut suf = vec![(f64::NEG_INFINITY, 0usize); m];
            for j in (0..m - 1).rev() {
                suf[j] = if v[j + 1] > suf[j + 1].0 {
                    (v[j + 1], j + 1)
                } else {
                    suf[j + 1]
                };
            }
            // The deepest few valleys, checked against the noise.
            let mut deep: Vec<(f64, usize)> = (1..m - 1)
                .filter_map(|j| {
                    let depth = pre[j].0.min(suf[j].0) - v[j];
                    (depth > 0.0).then_some((depth, j))
                })
                .collect();
            deep.sort_by(|p, q| q.0.total_cmp(&p.0));
            for &(depth, j) in deep.iter().take(8) {
                let (xl, xb, xr) = (xs[s0 + pre[j].1], xs[s0 + j], xs[s0 + suf[j].1]);
                let tol = a.noise(xb) + a.noise(xl).max(a.noise(xr));
                if a.forgives_n(tol, depth) {
                    continue;
                }
                r.doubt(
                    "extrema-incomplete",
                    flag,
                    e,
                    format!(
                        "f({xb:?})={:?} with {} values either side ({xl:?}, {xr:?}) and no {name} reported between",
                        ys[s0 + j],
                        if sign > 0.0 { "higher" } else { "lower" }
                    ),
                );
                break 'runs;
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
    // A 0 where f(0) is certainly not 0 (beyond the doubles, e^(−10⁸), or
    // larger than its own rounding): the intercept is at exactly x = 0, so
    // no neighbouring value excuses it.
    if a.d().y_intercept == Some(0.0)
        && let R::V(v) = a.reference(0.0)
        && !v.is_zero()
        && (!v.normal() || v.f().abs() > a.rounding(0.0))
    {
        r.fail(
            "y-intercept-wrong",
            flags::Y_INTERCEPT,
            e,
            format!("y-intercept 0 but f(0)={v:?}, not 0"),
        );
        return;
    }
    match (a.d().y_intercept, a.defined(0.0)) {
        (Some(y), Some(false)) => r.fail(
            "y-intercept-where-undefined",
            flags::Y_INTERCEPT,
            e,
            format!("y-intercept {y:?} but f(0) is undefined"),
        ),
        (Some(y), Some(true))
            if !same_shown(y, f0) && !a.forgives_n(a.noise(0.0) + slack(y), (y - f0).abs()) =>
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

/// Where an operand that makes f undefined at its zeros (a divisor, the
/// cosine under a tangent, …) crosses 0 continuously between two samples,
/// f is undefined at a point between them, even one no double is
/// (tan x · cos x at π/2, sin x / sin x at π): a domain that claims that
/// point is wrong. (Within ±10⁶, where the floats still resolve a trig
/// argument's zeros; the first 256 crossings.)
pub fn check_singular_zeros(a: &Analysed, xs: &[f64], r: &mut Report) {
    let d = a.d();
    if a.unknown(flags::DOMAIN) || d.domain.is_empty() {
        return;
    }
    let e = &a.expr;
    let mut ops = Vec::new();
    // (A varying exponent is NaN here, so its base counts.)
    singular_operands(a, f64::NAN, &a.ast, &mut ops);
    ops.retain(|o| o.contains_x());
    ops.dedup();
    if ops.is_empty() {
        return;
    }
    let plain = CompileOptions {
        trig_unit: a.unit,
        ..CompileOptions::default()
    };
    // What the claim already says about a point: excluded, a pole, or an
    // end of the domain (to a few floats).
    let period = a.period();
    let marks: Vec<Family> = d
        .excluded
        .iter()
        .chain(&d.vertical_asymptotes)
        .copied()
        .chain(
            d.domain
                .iter()
                .flat_map(|iv| [iv.lo.value, iv.hi.value])
                .filter(|v| v.is_finite())
                .map(|v| Family { x: v, period }),
        )
        .collect();
    // (To where the operand's own rounding puts its zero, too: in a
    // shifted frame, tan(x − 1000), cos(x − 1000) is only known to ulp(1000)
    // or so, and a claimed position carries the shift's rounding.)
    let accounted = |c: f64, o: &Expr| {
        marks.iter().any(|f| close_to(f, c, 64)) || {
            let (_, err) = a.bound_of(o, c);
            let h = 1e-6 * c.abs().max(1.0);
            let slope = ((a.bound_of(o, c + h).0 - a.bound_of(o, c - h).0) / (2.0 * h)).abs();
            let tol = if err.is_finite() && slope > 0.0 {
                4.0 * err / slope
            } else {
                0.0
            };
            marks
                .iter()
                .any(|f| (nearest(f, c) - c).abs() <= tol + 64.0 * ulp(c))
        }
    };
    let whole = d
        .domain
        .iter()
        .any(|iv| iv.lo.value == f64::NEG_INFINITY && iv.hi.value == f64::INFINITY);
    let w = d
        .domain
        .iter()
        .map(|iv| iv.lo.value)
        .filter(|v| v.is_finite())
        .fold(f64::INFINITY, f64::min);
    let w = if w.is_finite() { w } else { 0.0 };
    let claimed = |x: f64| {
        let t = match period {
            Some(p) => w + (x - w).rem_euclid(p),
            None => x,
        };
        (whole || set_has(&d.domain, t)) && !d.excluded.iter().any(|f| in_family(f, x))
    };
    let mut tried = 0;
    for o in &ops {
        let Ok(prog) = Program::compile(o, &plain) else {
            continue;
        };
        a.charge((prog.cost().max(1) * xs.len()) as u64);
        let vs: Vec<f64> = xs.iter().map(|&x| prog.eval(x, 0.0)).collect();
        for i in 0..xs.len().saturating_sub(1) {
            if a.over() || tried >= 256 {
                return;
            }
            let (x0, x1, v0, v1) = (xs[i], xs[i + 1], vs[i], vs[i + 1]);
            if x0.abs() > 1e6
                || x1.abs() > 1e6
                || !(v0.is_finite() && v1.is_finite())
                || v0 == 0.0
                || v1 == 0.0
                || v0.signum() == v1.signum()
            {
                continue;
            }
            tried += 1;
            // To adjacent floats, on o's own values.
            let (mut lo, mut hi, mut vl) = (x0, x1, v0);
            let scale = v0.abs().max(v1.abs());
            let mut smooth = true;
            for _ in 0..200 {
                let m = 0.5 * (lo + hi);
                if m <= lo || m >= hi {
                    break;
                }
                let vm = prog.eval(m, 0.0);
                a.charge(prog.cost().max(1) as u64);
                if !vm.is_finite() || vm.abs() > scale {
                    smooth = false;
                    break;
                }
                if vm == 0.0 {
                    (lo, hi) = (m, m);
                    break;
                }
                if vm.signum() == vl.signum() {
                    (lo, vl) = (m, vm);
                } else {
                    hi = m;
                }
            }
            if !smooth {
                continue;
            }
            // A crossing, not a jump: o is small at the floats beside it.
            let (vlo, vhi) = (prog.eval(lo, 0.0), prog.eval(hi, 0.0));
            let (_, err) = a.bound_of(o, lo);
            let small = vlo.abs().min(vhi.abs());
            if !(small <= 1e-6 * scale || small <= 64.0 * err) {
                continue;
            }
            let c = lo;
            if accounted(c, o) || accounted(hi, o) || !claimed(c) || !claimed(hi) {
                continue;
            }
            r.fail(
                "domain-includes-singular-point",
                flags::DOMAIN,
                e,
                format!(
                    "{o} crosses 0 between {lo:?} and {hi:?}, inside domain {} excl {}",
                    fmt_set(&d.domain),
                    fmt_fams(&d.excluded)
                ),
            );
            return;
        }
    }
}

/// Where a·x + b = j·q/2 + k·s for the affine argument `u` (q the half
/// turn: π, 180 or 200; j quarter turns; s = q, or the whole turn 2q with
/// `whole`), as a family of x placed to the last bit, with how far a
/// placing by sampling may be off (the shift's rounding). `None` if u
/// varies with x but isn't affine, `Some(None)` if it doesn't vary.
pub(super) fn turn_family(
    u: &Expr,
    quarters: u8,
    whole: bool,
    opts: &CompileOptions<'_>,
) -> Option<Option<(Family, f64)>> {
    if !u.contains_x() {
        return Some(None);
    }
    let num = |e: &Expr| Program::compile(e, opts).ok()?.as_constant();
    let (a, b) = crate::simplify::linear_in(u, &|n| matches!(n, Expr::X))
        .and_then(|(c, r)| Some((num(&c)?, num(&r)?)))?;
    if !(a != 0.0 && a.is_finite() && b.is_finite()) {
        return None;
    }
    // The half turn, as a double and what it leaves out.
    let (q, q_lo) = match opts.trig_unit {
        TrigUnit::Radians => (std::f64::consts::PI, 1.2246467991473532e-16),
        TrigUnit::Degrees => (180.0, 0.0),
        TrigUnit::Grads => (200.0, 0.0),
    };
    // (Multiples of q/2 and 2q are exact, as is their low part.)
    let j = f64::from(quarters);
    let (off, off_lo) = (j * q / 2.0, j * q_lo / 2.0);
    let (s, s_lo) = if whole {
        (2.0 * q, 2.0 * q_lo)
    } else {
        (q, q_lo)
    };
    // a·x0 = off − b − m·s for the nearest whole m, reduced with one
    // rounding (−b − m·s is small, so its fma is exact to its own last bit)
    // and π's low part: tan(x − 1000) has its pole at the double nearest
    // π/2 + 1000 − 319π, not 10⁻¹³ off it.
    let m = ((off - b) / s).round();
    let rem = (-m).mul_add(s, -b) + off + (off_lo - m * s_lo);
    let x0 = rem / a;
    let tol = 1e-9 * x0.abs().max(1.0) + 16.0 * (b.abs() * f64::EPSILON) / a.abs();
    Some(Some((
        Family {
            x: x0,
            period: Some(s / a.abs()),
        },
        tol,
    )))
}

/// Points of f's domain that no sampling can show it lacks: the zeros of a
/// sine, cosine, tangent or cotangent factor of something f is undefined
/// at the zeros of (a divisor, a logarithm's argument, a power's base
/// under an exponent that isn't positive, the cosine under tan and sec, the
/// sine under cot and csc), which in radians are no double. `fams`: those
/// of an affine argument, as families (with how far a placing may be off).
/// `hidden`: such a factor of an argument that isn't affine, touched
/// rather than crossed (an even power, under |·| or a root), so that no
/// sign change shows its zeros either.
#[derive(Debug, Default)]
pub(super) struct TrigSingular {
    pub fams: Vec<(Family, f64)>,
    pub hidden: bool,
}

pub(super) fn trig_singular(e: &Expr, opts: &CompileOptions<'_>) -> TrigSingular {
    use crate::ast::{BinOp, Func};
    type Const<'a> = &'a dyn Fn(&Expr) -> Option<f64>;
    // The operands f is undefined at the zeros of.
    fn operands(e: &Expr, c: Const, out: &mut Vec<Expr>) {
        match e {
            Expr::Bin(op, l, r) => {
                match op {
                    BinOp::Div => out.push((**r).clone()),
                    BinOp::Pow if c(r).is_some_and(|p| p <= 0.0) => out.push((**l).clone()),
                    _ => {}
                }
                operands(l, c, out);
                operands(r, c, out);
            }
            Expr::Call(f, args) => {
                match f {
                    Func::Ln | Func::Log | Func::LogBase | Func::Coth | Func::Csch => {
                        out.extend(args.iter().cloned())
                    }
                    Func::Mod => out.extend(args.get(1).cloned()),
                    Func::Tan | Func::Sec => out.push(Expr::Call(Func::Cos, args.clone())),
                    Func::Cot | Func::Csc => out.push(Expr::Call(Func::Sin, args.clone())),
                    _ => {}
                }
                args.iter().for_each(|a| operands(a, c, out));
            }
            Expr::Neg(x) | Expr::Degrees(x) => operands(x, c, out),
            _ => {}
        }
    }
    // coef · g(u)ⁿ for g sin or cos (true), n 1 or 2.
    fn sin_cos_power(e: &Expr, c: Const) -> Option<(f64, bool, Expr, u8)> {
        match e {
            Expr::Call(f @ (Func::Sin | Func::Cos), args)
                if args.len() == 1 && args[0].contains_x() =>
            {
                Some((1.0, *f == Func::Cos, args[0].clone(), 1))
            }
            Expr::Bin(BinOp::Pow, l, r) if c(r) == Some(2.0) => {
                let (k, cos, u, n) = sin_cos_power(l, c)?;
                (n == 1).then_some((k * k, cos, u, 2))
            }
            Expr::Neg(x) => sin_cos_power(x, c).map(|(k, cos, u, n)| (-k, cos, u, n)),
            Expr::Bin(BinOp::Mul, l, r) => match (c(l), c(r)) {
                (Some(k), None) => sin_cos_power(r, c).map(|(j, cos, u, n)| (k * j, cos, u, n)),
                (None, Some(k)) => sin_cos_power(l, c).map(|(j, cos, u, n)| (k * j, cos, u, n)),
                _ => None,
            },
            _ => None,
        }
    }
    // An operand's trig zeros: (quarter turns, whole-turn step, argument,
    // touched): sin u at kπ, cos u at π/2 + kπ, …; 1 + cos u at π + 2kπ,
    // 1 − sin² u at π/2 + kπ, …, where sin or cos reaches ±1 and the
    // operand only touches 0.
    fn factors(e: &Expr, c: Const, touched: bool, out: &mut Vec<(u8, bool, Expr, bool)>) {
        match e {
            Expr::Bin(BinOp::Mul, l, r) => {
                factors(l, c, touched, out);
                factors(r, c, touched, out);
            }
            // A quotient is 0 where its numerator is.
            Expr::Bin(BinOp::Div, l, _) => factors(l, c, touched, out),
            Expr::Bin(BinOp::Pow, l, r) => {
                if let Some(p) = c(r).filter(|p| *p > 0.0 && p.is_finite()) {
                    let odd = p.fract() == 0.0 && p % 2.0 != 0.0;
                    factors(l, c, touched || !odd, out);
                }
            }
            Expr::Bin(op @ (BinOp::Add | BinOp::Sub), l, r) => {
                // k + coef·gⁿ = 0 where gⁿ = −k/coef.
                let sign = if *op == BinOp::Sub { -1.0 } else { 1.0 };
                let (k, term) = match (c(l), c(r)) {
                    (Some(k), None) => (
                        k,
                        sin_cos_power(r, c).map(|(j, g, u, n)| (sign * j, g, u, n)),
                    ),
                    (None, Some(k)) => (sign * k, sin_cos_power(l, c)),
                    _ => return,
                };
                let Some((coef, cos, u, n)) = term else {
                    return;
                };
                if coef == 0.0 {
                    return;
                }
                let at = -k / coef;
                let zero = match (n, at, cos) {
                    (1, 1.0, true) => Some((0, true)),
                    (1, -1.0, true) => Some((2, true)),
                    (1, 1.0, false) => Some((1, true)),
                    (1, -1.0, false) => Some((3, true)),
                    (2, 1.0, true) => Some((0, false)),
                    (2, 1.0, false) => Some((1, false)),
                    // (Elsewhere g crosses its level: a sign change shows it.)
                    _ => None,
                };
                if let Some((j, whole)) = zero {
                    out.push((j, whole, u, true));
                }
            }
            Expr::Neg(x) => factors(x, c, touched, out),
            Expr::Call(Func::Abs | Func::Sqrt, args) if args.len() == 1 => {
                factors(&args[0], c, true, out)
            }
            Expr::Call(Func::Cbrt, args) if args.len() == 1 => factors(&args[0], c, touched, out),
            Expr::Call(f @ (Func::Sin | Func::Cos | Func::Tan | Func::Cot), args)
                if args.len() == 1 && args[0].contains_x() =>
            {
                let j = if matches!(f, Func::Cos | Func::Cot) {
                    1
                } else {
                    0
                };
                out.push((j, false, args[0].clone(), touched));
            }
            _ => {}
        }
    }
    let constant = |e: &Expr| -> Option<f64> {
        if e.contains_x() {
            return None;
        }
        Program::compile(e, opts).ok()?.as_constant()
    };
    let mut ops = Vec::new();
    operands(e, &constant, &mut ops);
    let mut out = TrigSingular::default();
    for o in ops.iter().filter(|o| o.contains_x()) {
        let mut fs = Vec::new();
        factors(o, &constant, false, &mut fs);
        for (quarters, whole, u, touched) in fs {
            match turn_family(&u, quarters, whole, opts) {
                Some(Some(f)) => {
                    if !out
                        .fams
                        .iter()
                        .any(|(g, _)| g.x == f.0.x && g.period == f.0.period)
                    {
                        out.fams.push(f);
                    }
                }
                Some(None) => {}
                None => out.hidden |= touched,
            }
        }
    }
    out
}

/// Where f is undefined at points no double is and no sign change shows
/// (the zeros of cos x under cos² x / cos² x, at π/2 + kπ): the claimed
/// domain must leave them out, as an excluded point, a pole or a gap.
/// Those of an argument that isn't affine can't be placed: a definite
/// domain is then unverified.
pub fn check_trig_singular(a: &Analysed, r: &mut Report) {
    let d = a.d();
    if a.unknown(flags::DOMAIN) || d.domain.is_empty() {
        return;
    }
    let opts = CompileOptions {
        trig_unit: a.unit,
        ..CompileOptions::default()
    };
    let ts = trig_singular(&a.ast, &opts);
    if ts.hidden {
        r.fail(
            "domain-hides-singular-points",
            flags::DOMAIN,
            &a.expr,
            "an operand touches 0, at points no double is, through a trig factor whose argument isn't affine".into(),
        );
        return;
    }
    if ts.fams.is_empty() {
        return;
    }
    let period = a.period();
    let marks: Vec<Family> = d
        .excluded
        .iter()
        .chain(&d.vertical_asymptotes)
        .copied()
        .chain(
            d.domain
                .iter()
                .flat_map(|iv| [iv.lo.value, iv.hi.value])
                .filter(|v| v.is_finite())
                .map(|v| Family { x: v, period }),
        )
        .collect();
    let whole = d
        .domain
        .iter()
        .any(|iv| iv.lo.value == f64::NEG_INFINITY && iv.hi.value == f64::INFINITY);
    let w = d
        .domain
        .iter()
        .map(|iv| iv.lo.value)
        .filter(|v| v.is_finite())
        .fold(f64::INFINITY, f64::min);
    let w = if w.is_finite() { w } else { 0.0 };
    let claimed = |x: f64| {
        let t = match period {
            Some(p) => w + (x - w).rem_euclid(p),
            None => x,
        };
        (whole || set_has(&d.domain, t)) && !d.excluded.iter().any(|f| in_family(f, x))
    };
    // How far either side of c f is undefined at every double (1 + cos x
    // rounds to exactly 0 within 1.5·10⁻⁸ of π): a claim may place the
    // point anywhere in that band.
    let band = |c: f64| {
        // (Only where the reference vouches: beside 0 the doubles are
        // subnormal, where it may abstain, and that is no evidence.)
        let undefined = |x: f64| a.known(x).is_some_and(|v| !v.is_finite());
        let limit = 1e-6 * c.abs().max(1.0);
        let mut h = 2.0 * ulp(c);
        if !(undefined(c + h) && undefined(c - h)) {
            return 0.0;
        }
        while h < limit && undefined(c + 2.0 * h) && undefined(c - 2.0 * h) {
            h *= 2.0;
        }
        h
    };
    for (fam, tol) in &ts.fams {
        let q = fam.period.unwrap_or(0.0);
        for k in -3..=3 {
            let c = fam.x + k as f64 * q;
            if !c.is_finite() || c.abs() > 1e15 {
                continue;
            }
            let w = tol + 64.0 * ulp(c) + 2.0 * band(c);
            let shown = marks
                .iter()
                .any(|m| close_to(m, c, 64) || (nearest(m, c) - c).abs() <= w);
            if shown || !claimed(c) {
                continue;
            }
            r.fail(
                "domain-includes-singular-family",
                flags::DOMAIN,
                &a.expr,
                format!(
                    "undefined at {c:?} (family {:?} + k·{q:?}), inside domain {} excl {}",
                    fam.x,
                    fmt_set(&d.domain),
                    fmt_fams(&d.excluded)
                ),
            );
            return;
        }
    }
}

/// The operands of `e` that make it undefined where they are 0 (at `at`,
/// for a power's exponent): divisors, logarithms' arguments, a power's base
/// under an exponent that isn't positive, the cosine under tan and sec and
/// the sine under cot and csc, mod's divisor.
fn singular_operands(a: &Analysed, at: f64, e: &Expr, out: &mut Vec<Expr>) {
    use crate::ast::{BinOp, Func};
    match e {
        Expr::Bin(op, l, rr) => {
            match op {
                BinOp::Div => out.push((**rr).clone()),
                BinOp::Pow => {
                    // (A NaN exponent counts: it can't be shown positive.)
                    let (p, _) = a.bound_of(rr, at);
                    if p.is_nan() || p <= 0.0 {
                        out.push((**l).clone());
                    }
                }
                _ => {}
            }
            singular_operands(a, at, l, out);
            singular_operands(a, at, rr, out);
        }
        Expr::Call(f, args) => {
            match f {
                Func::Ln | Func::Log | Func::LogBase | Func::Coth | Func::Csch => {
                    out.extend(args.iter().cloned())
                }
                Func::Mod => out.extend(args.get(1).cloned()),
                Func::Tan | Func::Sec => {
                    out.push(Expr::Call(Func::Cos, args.clone()));
                }
                Func::Cot | Func::Csc => {
                    out.push(Expr::Call(Func::Sin, args.clone()));
                }
                _ => {}
            }
            args.iter().for_each(|x| singular_operands(a, at, x, out));
        }
        Expr::Neg(x) | Expr::Degrees(x) => singular_operands(a, at, x, out),
        _ => {}
    }
}

/// Whether a point excluded from the domain has anything to show for it:
/// f undefined within two floats, blowing up beside it, or an operand that
/// makes f undefined there within its own rounding of doing so: a divisor
/// at 0, a logarithm's argument at 0, a zero base under an exponent that
/// isn't positive, the sine or cosine under a cotangent or tangent at 0.
/// Only operations that can be undefined count: (x − c)² is 0 at c, and
/// defined, so it shows nothing (1/(1 + (x − c)²/10⁻¹²) is defined at c).
pub fn exclusion_supported(a: &Analysed, c: f64) -> bool {
    if (-2..=2).any(|k| a.defined(nudge(c, k)) != Some(true)) || blows_up(a, c) {
        return true;
    }
    // Analytically: a zero of a trig factor of such an operand (cos² x
    // under cos² x / cos² x touches 0 at π/2, a point no double is).
    let opts = CompileOptions {
        trig_unit: a.unit,
        ..CompileOptions::default()
    };
    if trig_singular(&a.ast, &opts)
        .fams
        .iter()
        .any(|(f, tol)| close_to(f, c, 64) || (nearest(f, c) - c).abs() <= tol + 64.0 * ulp(c))
    {
        return true;
    }
    let mut ops = Vec::new();
    singular_operands(a, c, &a.ast, &mut ops);
    ops.iter().filter(|o| o.contains_x()).any(|o| {
        [-1, 0, 1].iter().any(|&k| {
            let t = nudge(c, k);
            let (v, err) = a.bound_of(o, t);
            v.is_finite() && v.abs() <= err + 4.0 * ulp(v)
        }) || crosses_near(a, o, c)
    })
}

/// Whether operand `o` crosses 0 within a few dozen floats of c,
/// continuously (smaller there than a little way off, so not through a
/// pole): its zero is then a point no double hits (cos x at π/2), and
/// a point excluded there is where f is undefined.
fn crosses_near(a: &Analysed, o: &Expr, c: f64) -> bool {
    // (Or as far as the expression's own numbers round: in a shifted
    // frame, tan(x − 1000), the argument is only known to ulp(1000).)
    let scale = centres_in(o, a.unit)
        .iter()
        .map(|v| v.abs())
        .filter(|v| v.is_finite())
        .fold(0.0f64, f64::max);
    let w = (64.0 * ulp(c)).max(4.0 * ulp(scale));
    let (lo, _) = a.bound_of(o, c - w);
    let (hi, _) = a.bound_of(o, c + w);
    if !(lo.is_finite() && hi.is_finite()) || lo * hi >= 0.0 {
        return false;
    }
    let h = 1e-3 * c.abs().max(1.0);
    let (l2, _) = a.bound_of(o, c - h);
    let (h2, _) = a.bound_of(o, c + h);
    l2.is_finite() && h2.is_finite() && lo.abs() < l2.abs() && hi.abs() < h2.abs()
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
                            !a.forgives_n(a.noise(bx) + a.noise(x), (by - y).abs())
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

/// Vertical asymptotes are where f blows up (or is undefined nearby), and
/// every undefined sample beside defined ones where f blows up is one,
/// whatever the analysis says of the domain (the pole of
/// (x − 10¹²)/(x − 10¹² − 1/16), sampled at its centre).
pub fn check_vertical(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
    if a.unknown(flags::VERTICAL_ASYMPTOTES) {
        return;
    }
    let d = a.d();
    let n = xs.len();
    let mut tried = 0;
    for i in 0..n {
        if tried >= 64 || a.over() {
            break;
        }
        let edge = (i > 0 && ys[i - 1].is_finite()) || (i + 1 < n && ys[i + 1].is_finite());
        if !ys[i].is_nan() || !edge || a.defined(xs[i]) != Some(false) {
            continue;
        }
        let c = xs[i];
        if d.vertical_asymptotes.iter().any(|v| close_to(v, c, 1024)) {
            continue;
        }
        tried += 1;
        if certainly_blows_up(a, c) {
            r.fail(
                "missing-pole",
                flags::VERTICAL_ASYMPTOTES,
                &a.expr,
                format!(
                    "f is undefined at {c:?} and blows up beside it; vertical asymptotes {}",
                    fmt_fams(&d.vertical_asymptotes)
                ),
            );
            return;
        }
    }
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
                    if a.forgives(x, dev.abs()) {
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
                && !a.forgives_n(
                    a.noise(xf) + 2.0 * (mid - c).abs() + a.noise(xm),
                    (far - c).abs(),
                )
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
                && !a.forgives_n(n + slack(c), (v - c).abs())
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
        // (A stand-in for a value beyond the doubles, ±5·10⁻³²⁴ or the
        // largest double, is no value to settle on; nor is one only the
        // compiled program has, the reference abstaining: e^(10¹⁰⁰) is
        // beyond what it can size.)
        let y = a.known(s * x)?;
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
    // Every pair ±x of samples: one defined and the other not, or their
    // values apart from what the claim says, the largest first (an
    // asymmetry far out, 10⁶ − 1/(1 + e^(x + 10⁶)) beyond 10⁶, mustn't wait
    // behind rounding near 0).
    let mut devs: Vec<(f64, f64)> = Vec::new();
    for &x in xs.iter().filter(|x| **x > 0.0) {
        if a.over() {
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
            if dev > 0.0 {
                devs.push((x, dev));
            }
        }
    }
    devs.sort_by(|p, q| q.1.total_cmp(&p.1));
    for &(x, dev) in devs.iter().take(256) {
        if !a.forgives_n(a.noise(x) + a.noise(-x), dev) {
            r.fail(
                "parity-contradicted",
                flags::PARITY,
                e,
                format!(
                    "{:?} but f({x:?})={:?}, f({:?})={:?}",
                    a.k.parity,
                    a.eval(x),
                    -x,
                    a.eval(-x)
                ),
            );
            return;
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
                        !a.forgives_n(a.noise(x) + a.noise(x2), (y1 - y2).abs())
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
                || (!a.forgives_n(a.noise(c) + slack(*y), (fv - y).abs()) && !same_shown(fv, *y))
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
                if a.forgives_n(nl, vl.abs())
                    || a.forgives_n(nr, vr.abs())
                    || vl.signum() != vr.signum()
                {
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
            if okm && okf && !a.forgives_n(sf + 2.0 * dm + sm, df) {
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

/// Which of f(x1) and f(x2) is larger, where that can be told: by more than
/// their noise, in the reference's arithmetic and in logarithms of their
/// sizes (where either is beyond the doubles their double stand-ins are all
/// alike, and their noise can be a factor of e⁴⁷⁷). None when they can't be
/// told apart.
fn compare_beyond(a: &Analysed, x1: f64, x2: f64) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering::*;
    // The value, and its noise as a factor (ln of it).
    let val = |x: f64| match a.reference(x) {
        R::V(v) if v.is_zero() => Some((v, 0.0)),
        R::V(v) if v.normal() => {
            let n = a.noise(x);
            Some((
                v,
                if n.is_finite() {
                    (n / v.f().abs()).ln_1p() + REFERENCE_ROUNDING
                } else {
                    f64::INFINITY
                },
            ))
        }
        R::V(v) => Some((v, a.log_beyond(x, v))),
        _ => None,
    };
    let ((v1, l1), (v2, l2)) = (val(x1)?, val(x2)?);
    // (Exponents this far out are where the reference saturates, e^t for
    // t below −4·10¹⁸: its size there is not known.)
    if [v1, v2].iter().any(|v| v.e.unsigned_abs() >= 1 << 60) {
        return None;
    }
    let undecided = || {
        if !(l1 + l2).is_finite() {
            a.unbounded.set(true);
        }
        None
    };
    let (s1, s2) = (v1.sign(), v2.sign());
    if s1 != s2 {
        // Either side of 0 (or 0 and not): apart, unless one of them is
        // indistinguishable from 0 (its noise as large as it is).
        if (s1 != 0.0 && l1 >= std::f64::consts::LN_2)
            || (s2 != 0.0 && l2 >= std::f64::consts::LN_2)
        {
            return undecided();
        }
        return s1.partial_cmp(&s2);
    }
    if s1 == 0.0 {
        return None;
    }
    let d = v1.abs().ln() - v2.abs().ln();
    // (NaN noise decides nothing either.)
    if d.abs().partial_cmp(&(l1 + l2)) != Some(std::cmp::Ordering::Greater) {
        return undecided();
    }
    Some(if (s1 > 0.0) == (d > 0.0) {
        Greater
    } else {
        Less
    })
}

/// Which of f(x1) and f(x2) is larger in the reference's arithmetic, noise
/// aside (None where either is undefined or unknown): the cheap test before
/// [`compare_beyond`].
fn order_beyond(a: &Analysed, x1: f64, x2: f64) -> Option<std::cmp::Ordering> {
    let (R::V(v1), R::V(v2)) = (a.reference(x1), a.reference(x2)) else {
        return None;
    };
    let d = v1.add(v2.neg());
    Some(if d.is_zero() {
        std::cmp::Ordering::Equal
    } else if d.sign() > 0.0 {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Less
    })
}

/// Where f is beyond the doubles (the far tails of e^(−x²), the dip
/// between two far-apart Gaussians), its double stand-ins are all alike,
/// so its shape there is checked on the reference's values: a monotone
/// interval doesn't turn back, and a turn among the samples has a reported
/// extremum of its kind between its neighbours.
pub fn check_beyond(a: &Analysed, xs: &[f64], ys: &[f64], r: &mut Report) {
    use std::cmp::Ordering::*;
    let d = a.d();
    let n = xs.len();
    let beyond: Vec<bool> = (0..n)
        .map(|i| is_stand_in(ys[i]) && a.beyond(xs[i]).is_some())
        .collect();
    if !beyond.iter().any(|b| *b) {
        return;
    }
    let e = &a.expr;
    // Monotone intervals (given outright, not per period).
    if !a.unknown(flags::MONOTONE_INTERVALS) && a.period().is_none() {
        'iv: for (iv, dir) in &d.monotonicity {
            let want = match dir {
                Monotonicity::Increasing => Less,
                Monotonicity::Decreasing => Greater,
                _ => continue,
            };
            let inside: Vec<usize> = (0..n)
                .filter(|&i| ys[i].is_finite() && iv_has(iv, xs[i]))
                .collect();
            for w in inside.windows(2) {
                let (i, j) = (w[0], w[1]);
                if !(beyond[i] || beyond[j]) {
                    continue;
                }
                if a.over() {
                    return;
                }
                // (Only a step the wrong way needs its noise.)
                if matches!(order_beyond(a, xs[i], xs[j]), None | Some(Equal))
                    || order_beyond(a, xs[i], xs[j]) == Some(want)
                {
                    continue;
                }
                if let Some(o) = compare_beyond(a, xs[i], xs[j])
                    && o != want
                {
                    r.fail(
                        "monotonicity-contradicted",
                        flags::MONOTONE_INTERVALS,
                        e,
                        format!(
                            "{dir:?} on ({:?},{:?}) but f({:?}) and f({:?}), beyond the doubles, go the other way",
                            iv.lo.value, iv.hi.value, xs[i], xs[j]
                        ),
                    );
                    break 'iv;
                }
            }
        }
    }
    // Turns among neighbouring defined samples, at least one of the three
    // beyond the doubles.
    for (list, want, flag, name) in [
        (&d.minima, Less, flags::MINIMA, "missing-minimum"),
        (&d.maxima, Greater, flags::MAXIMA, "missing-maximum"),
    ] {
        if a.unknown(flag) {
            continue;
        }
        let mut tried = 0;
        for i in 1..n.saturating_sub(1) {
            if !(beyond[i - 1] || beyond[i] || beyond[i + 1])
                || !(ys[i - 1].is_finite() && ys[i].is_finite() && ys[i + 1].is_finite())
            {
                continue;
            }
            if a.over() {
                break;
            }
            // (Only a turn by the reference's values needs its noise.)
            if order_beyond(a, xs[i], xs[i - 1]) != Some(want)
                || order_beyond(a, xs[i], xs[i + 1]) != Some(want)
            {
                continue;
            }
            if tried >= 256 {
                break;
            }
            tried += 1;
            if compare_beyond(a, xs[i], xs[i - 1]) != Some(want)
                || compare_beyond(a, xs[i], xs[i + 1]) != Some(want)
            {
                continue;
            }
            // Defined all the way across, with no pole between.
            let (lo, hi) = (xs[i - 1], xs[i + 1]);
            if (1..8).any(|j| a.defined(lo + (hi - lo) * j as f64 / 8.0) != Some(true))
                || d.vertical_asymptotes
                    .iter()
                    .chain(&d.excluded)
                    .any(|v| (lo..=hi).contains(&nearest(v, xs[i])))
            {
                continue;
            }
            let reported = list
                .iter()
                .any(|(f, _)| (lo..=hi).contains(&nearest(f, xs[i])));
            if !reported {
                r.fail(
                    name,
                    flag,
                    e,
                    format!(
                        "f turns between {lo:?} and {hi:?} (beyond the doubles); reported {}",
                        fmt_fams(&list.iter().map(|m| m.0).collect::<Vec<_>>())
                    ),
                );
                break;
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
    // (Outside the gate, a discrepancy forgiven by unbounded noise is no
    // failure, only unverified.)
    a.take_unbounded();
}

type Step = fn(&Analysed, &[f64], &[f64], &[f64], &mut Report);

/// The checks, each with the features it vouches for once it has run to
/// the end.
fn steps() -> [(u32, Step); 13] {
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
            check_extrema_between(a, xs, ys, r);
            check_valleys(a, xs, ys, r);
        }),
        (flags::INFLECTION_POINTS, |a, _, _, _, r| {
            check_inflections(a, r)
        }),
        (flags::DOMAIN, |a, xs, ys, _, r| {
            check_domain(a, xs, ys, r);
            check_singular_zeros(a, xs, r);
            check_trig_singular(a, r);
        }),
        (flags::MONOTONE_INTERVALS, |a, xs, ys, _, r| {
            check_monotonicity(a, xs, ys, r)
        }),
        (flags::VERTICAL_ASYMPTOTES, |a, xs, ys, _, r| {
            check_vertical(a, xs, ys, r)
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
        (
            flags::MONOTONE_INTERVALS | flags::MINIMA | flags::MAXIMA,
            |a, xs, ys, _, r| check_beyond(a, xs, ys, r),
        ),
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

/// The numbers in the expression: every part of it without x, evaluated
/// (π, e, a literal, and 1234·1234 = 1522756 or 2¹⁰ or √2 as a whole), the
/// largest such parts first.
fn literals(e: &Expr, unit: TrigUnit, out: &mut Vec<f64>) {
    // (Its parts too, below: the 1234 in 1234·1234.)
    if !e.contains_x()
        && !matches!(e, Expr::Y)
        && let R::V(v) = reval(e, 0.0, unit)
        && v.normal()
    {
        out.push(v.f());
    }
    match e {
        Expr::Neg(a) | Expr::Degrees(a) => literals(a, unit, out),
        Expr::Bin(_, a, b) => {
            literals(a, unit, out);
            literals(b, unit, out);
        }
        Expr::Call(_, args) => args.iter().for_each(|a| literals(a, unit, out)),
        _ => {}
    }
}

/// At most this many distinct numbers of the expression are combined into
/// centres ([`centres_of`]), and at most [`MAX_CENTRES`] centres kept.
pub const MAX_NUMBERS: usize = 24;
/// See [`MAX_NUMBERS`].
pub const MAX_CENTRES: usize = 64;

/// Where f's features can gather, from its text alone: ± every number in
/// it (every part without x, evaluated), and their pairwise sums,
/// differences, products and quotients (the centres of (x − 10⁹)(x − 10⁹ −
/// 1/16), of x/1000 − 5, of a shift inside a shift, of x − 1234·1234).
/// Numbers beyond [`MAX_NUMBERS`] (in order of size, largest first) and
/// centres beyond [`MAX_CENTRES`] or 10¹⁵ are left out.
pub fn centres_of(e: &Expr) -> Vec<f64> {
    centres_in(e, TrigUnit::Radians)
}

/// [`centres_of`] with the angle unit the parts are evaluated in.
pub fn centres_in(e: &Expr, unit: TrigUnit) -> Vec<f64> {
    let mut lits = Vec::new();
    literals(e, unit, &mut lits);
    lits.retain(|v| v.is_finite() && *v != 0.0);
    lits.sort_by(|p, q| q.abs().total_cmp(&p.abs()).then(p.total_cmp(q)));
    lits.dedup();
    lits.truncate(MAX_NUMBERS);
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
    seen.truncate(MAX_CENTRES);
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
    a.take_unbounded();
    if !a.unknown(flags::RANGE) {
        let at = |x: f64, v: f64| {
            let y = a.eval(x);
            y.is_finite() && (y == v || same_shown(y, v) || a.within(x, (y - v).abs()))
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
        // A piece whose different ends the panel writes alike reads as
        // another shape: (10⁶, 10⁶ + 10⁻⁶) shown as "(1000000, 1000000)".
        let collapsed = d.range.iter().any(|iv| {
            iv.lo.value != iv.hi.value
                && iv.lo.value.is_finite()
                && iv.hi.value.is_finite()
                && same_shown(iv.lo.value, iv.hi.value)
        });
        if bad
            || collapsed
            || !range_supported(a, xs, ys, centres)
            || !hole_values_attained(a, xs, ys)
            || a.take_unbounded()
        {
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
        let n = a.noise(x) + slack(v);
        a.within_n(n, (y - v).abs()) && a.within_n(n, (bound - v).abs())
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
    if !(yt == v || same_shown(yt, v) || a.within(xt, (yt - v).abs())) {
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
                .any(|&i| a.within(xs[i], (ys[i] - v).abs()))
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
    // The samples, with the far tails and the most extreme turns refined
    // (a sample rarely lands on the top of a peak).
    let mut pts: Vec<(f64, f64)> = xs.iter().copied().zip(ys.iter().copied()).collect();
    for s in [1.0, -1.0] {
        for x in [(1e6 * reach).max(1e30), 1e100, 1e300] {
            pts.push((s * x, a.eval(s * x)));
        }
        pts.extend(refine_extremes(a, xs, ys, s, 8));
    }
    pts.sort_by(|p, q| p.0.total_cmp(&q.0));
    pts.dedup_by(|p, q| p.0 == q.0);
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
        // The end of a stretch: f goes on the way it was going, to the end
        // of its claimed piece, if it is seen to: towards a value beyond
        // the doubles, a pole, or (past the last sample) growing at least
        // as fast as a logarithm. (Towards a limit, e^x/(e^x + 1) at 10³⁰⁰
        // or 10¹²·atan(1/x) beside 0, it doesn't.)
        let goes_on = |k: Option<usize>, end: f64| match k {
            None if end.is_infinite() => unbounded_tail(a, x0.signum(), reach),
            None => tail_approaches(a, x0.signum(), reach, end),
            Some(k) if pts[k].1.is_nan() && end.is_infinite() => {
                certainly_blows_up(a, pts[k].0) || unbounded_at_edge(a, x0, pts[k].0)
            }
            Some(k) if pts[k].1.is_nan() => edge_approaches(a, x0, pts[k].0, end),
            Some(_) => true,
        };
        let mut inners = Vec::new();
        if i > 0 && finite(i - 1) && (i + 1 >= n || !finite(i + 1)) {
            inners.push((i - 1, (i + 1 < n).then_some(i + 1)));
        }
        if i + 1 < n && finite(i + 1) && (i == 0 || !finite(i - 1)) {
            inners.push((i + 1, i.checked_sub(1)));
        }
        for (j, beyond) in inners {
            // (Which way f goes: from the nearest sample inwards with another
            // value; beside a pole the floats make f a staircase.)
            let mut j = j;
            for _ in 0..64 {
                if pts[j].1 != y0 {
                    break;
                }
                let next = if j < i { j.checked_sub(1) } else { Some(j + 1) };
                match next {
                    Some(t) if t < n && finite(t) => j = t,
                    _ => break,
                }
            }
            let dir = (y0 - pts[j].1).signum();
            if let Some(iv) = piece(y0) {
                if dir > 0.0 && goes_on(beyond, iv.hi.value) {
                    cover.push((y0, iv.hi.value, x0, x0));
                } else if dir < 0.0 && goes_on(beyond, iv.lo.value) {
                    cover.push((iv.lo.value, y0, x0, x0));
                }
            }
        }
    }
    // Also both ends of every stretch the samples end on, where there is a
    // pole between two samples: each side keeps going. (Not across a jump.)
    for i in 0..n.saturating_sub(1) {
        if !(finite(i) && finite(i + 1)) {
            continue;
        }
        if joined[i] || a.over() || !pole_between(a, pts[i].0, pts[i + 1].0) {
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
            .reduce(f64::min)
            // (A value no sample takes, a claimed end, has no noise.)
            .unwrap_or(0.0)
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
                        && !a.forgives_n(least_noise(at) + least_noise(lo), lo - at)))
            {
                return false;
            }
            at = at.max(hi);
            if at >= iv.hi.value {
                break;
            }
        }
        if at < iv.hi.value
            && (iv.hi.value == f64::INFINITY
                || (iv.hi.value - at > 4.0 * (ulp(at) + ulp(iv.hi.value))
                    && !a.forgives_n(least_noise(at), iv.hi.value - at)))
        {
            return false;
        }
    }
    true
}

/// f towards the edge of its domain between `inside` (defined) and
/// `outside` (not): its values from 1/100 of the way (or of the edge's
/// size) out, each a hundredfold nearer, down to a few floats from it (the
/// nearest stretch where it is defined all the way).
fn edge_profile(a: &Analysed, inside: f64, outside: f64) -> Option<(f64, Vec<f64>)> {
    if a.defined(inside) != Some(true) {
        return None;
    }
    let (mut lo, mut hi) = (inside, outside);
    for _ in 0..100 {
        if floats_between(lo, hi) <= 1 {
            break;
        }
        let mid = from_ord(to_ord(lo) + (to_ord(hi) - to_ord(lo)) / 2);
        if a.defined(mid) == Some(true) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (edge, s) = (lo, (inside - outside).signum());
    let mut d = 0.01 * edge.abs().max(1.0);
    let mut vals = Vec::new();
    while d > 16.0 * ulp(edge) && vals.len() < 200 {
        let y = a.eval(edge + s * d);
        if y.is_finite() {
            vals.push(y);
        } else {
            vals.clear();
        }
        d /= 100.0;
    }
    (vals.len() >= 6).then_some((edge + s * d * 100.0, vals))
}

/// Whether f grows without bound towards the edge of its domain between
/// `inside` and `outside`: nearer and nearer it, its steps keep one
/// direction and don't shrink (ln x, x^(−0.01) at 0, whose growth the
/// doubles cut short at 744 or 1744; not atan(1/x), whose steps shrink to
/// its limit).
fn unbounded_at_edge(a: &Analysed, inside: f64, outside: f64) -> bool {
    let Some((near, vals)) = edge_profile(a, inside, outside) else {
        return false;
    };
    let steps: Vec<f64> = vals.windows(2).map(|w| w[1] - w[0]).collect();
    let last = &steps[steps.len() - 4..];
    let one_way = last.iter().all(|&d| d > 0.0) || last.iter().all(|&d| d < 0.0);
    one_way
        && last.windows(2).all(|w| w[1].abs() >= 0.8 * w[0].abs())
        && !a.within(near, (vals[vals.len() - 1] - vals[vals.len() - 5]).abs())
}

/// Whether f approaches the value `end` towards the edge of its domain
/// between `inside` and `outside` ([`approaches`]: e^(−atanh x) to 0 at 1).
fn edge_approaches(a: &Analysed, inside: f64, outside: f64, end: f64) -> bool {
    let Some((near, vals)) = edge_profile(a, inside, outside) else {
        return false;
    };
    let tail = &vals[vals.len() - 5..];
    let dists: Vec<f64> = tail.iter().map(|y| (y - end).abs()).collect();
    a.within(near, dists[dists.len() - 1]) || approaches(&dists, &[2.0; 4])
}

/// Whether distances from a value, at points `decades[k]` decades apart,
/// show f tending to it: they shrink by the same factor a decade (to 2%: f
/// is that value plus a power of the distance, x^(−0.001) to 0 at ∞, √(1 −
/// x) to 0 at 1), or by at least half over the last step. (1 − 1/ln x does
/// tend to 1, but nothing in its values says so; 2 + x^(−1) doesn't tend to
/// 1, and its distances from 1 shrink ever more slowly.)
fn approaches(dists: &[f64], decades: &[f64]) -> bool {
    if dists.len() < 3 || dists.iter().any(|d| !(d.is_finite() && *d > 0.0)) {
        return false;
    }
    let per: Vec<f64> = dists
        .windows(2)
        .zip(decades)
        .map(|(w, n)| (w[1] / w[0]).ln() / n)
        .collect();
    let (lo, hi) = per
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &v| {
            (l.min(v), h.max(v))
        });
    let n = dists.len();
    (hi < 0.0 && lo / hi <= 1.02)
        || (per.iter().all(|&v| v < 0.0) && dists[n - 1] <= 0.5 * dists[n - 2])
}

/// Whether f has a pole between lo and hi: zooming in on where |f| is
/// largest (or f undefined), f blows up there.
fn pole_between(a: &Analysed, mut lo: f64, mut hi: f64) -> bool {
    for _ in 0..40 {
        let mut best = (f64::NEG_INFINITY, lo);
        for j in 1..16 {
            let t = lo + (hi - lo) * j as f64 / 16.0;
            let v = a.eval(t);
            let m = if v.is_nan() { f64::INFINITY } else { v.abs() };
            if m > best.0 {
                best = (m, t);
            }
        }
        let w = (hi - lo) / 16.0;
        (lo, hi) = (best.1 - w, best.1 + w);
        if best.0 == f64::INFINITY || floats_between(lo, hi) < 64 || a.over() {
            break;
        }
    }
    let c = 0.5 * (lo + hi);
    (-64..=64)
        .step_by(16)
        .any(|k| certainly_blows_up(a, nudge(c, k)))
        || certainly_blows_up(a, lo)
        || certainly_blows_up(a, hi)
}

/// Whether f, far out on side s, approaches the finite value `end`: its
/// distance from it, from beyond every centre out to 10³⁰⁰, shrinks, by at
/// least half over the last two hundred decades, or ends within the noise
/// (1/x to 0, e^x/(e^x + 1) to 1; not 1 − 1/ln x to 1, nor sin x to
/// anything).
fn tail_approaches(a: &Analysed, s: f64, reach: f64, end: f64) -> bool {
    let t0 = (1e6 * reach).max(1e30);
    let ts: Vec<f64> = [t0, 1e100, 1e200, 1e300]
        .iter()
        .copied()
        .filter(|&t| t >= t0)
        .collect();
    let d: Vec<f64> = ts.iter().map(|&t| (a.eval(s * t) - end).abs()).collect();
    if d.len() < 2 || d.iter().any(|v| !v.is_finite()) {
        return false;
    }
    let decades: Vec<f64> = ts.windows(2).map(|w| (w[1] / w[0]).log10()).collect();
    a.within(s * 1e300, d[d.len() - 1]) || approaches(&d, &decades)
}

/// Whether f keeps growing far out on side s, at least as fast as a
/// logarithm: its step per decade, from beyond every centre out to 10³⁰⁰,
/// doesn't shrink, and is beyond its noise.
fn unbounded_tail(a: &Analysed, s: f64, reach: f64) -> bool {
    let t0 = (1e6 * reach).max(1e30);
    let pts: Vec<(f64, f64)> = [t0, 1e100, 1e200, 1e300]
        .iter()
        .filter(|&&t| t >= t0)
        .map(|&t| (t.log10(), a.eval(s * t)))
        .collect();
    if pts.len() < 3 || pts.iter().any(|p| !p.1.is_finite()) {
        // (Beyond the doubles out there, growing: it has.)
        return pts
            .last()
            .is_some_and(|p| p.1.is_infinite() || p.1.abs() == f64::MAX);
    }
    let steps: Vec<f64> = pts
        .windows(2)
        .map(|w| (w[1].1 - w[0].1) / (w[1].0 - w[0].0))
        .collect();
    let up = steps.iter().all(|&d| d > 0.0) || steps.iter().all(|&d| d < 0.0);
    up && steps.windows(2).all(|w| w[1].abs() >= 0.8 * w[0].abs())
        && (pts[pts.len() - 1].1 - pts[0].1).abs() > a.noise(s * 1e300) + a.noise(s * t0)
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
    let stop = || cancel.is_some_and(|c| c.load(Ordering::Relaxed));
    let centres = centres_in(&a.ast, a.unit);
    let xs = gate_samples(&a, &centres);
    if stop() {
        return Err(super::engine::Stop::Cancelled);
    }
    let mut r = Report {
        quiet,
        ..Report::default()
    };
    let mut ys: Vec<f64> = Vec::with_capacity(xs.len());
    for (i, &x) in xs.iter().enumerate() {
        if i % 256 == 0 && (a.over() || stop()) {
            break;
        }
        ys.push(a.eval(x));
    }
    if ys.len() < xs.len() || a.over() {
        if a.cancelled() || stop() {
            return Err(super::engine::Stop::Cancelled);
        }
        // Out of budget before anything was checked: nothing stays.
        r.dropped.1 |= ALL;
        apply(&mut a.k, ALL);
        spent.set(a.spent());
        return Ok((a.k, r));
    }
    if !quiet {
        r.work.push((0, a.spent(), xs.len() as u32));
    }
    // A sample only the compiled program has a value for (the reference
    // abstains) is no truth for the checks that read the samples: what
    // they check is unverified.
    let shaky = xs.iter().any(|&x| a.abstained(x));
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
            a.take_unbounded();
            step(&a, &xs, &ys, &centres, &mut r);
            if !quiet {
                r.work.push((vouches, a.spent() - before, r.refuted));
            }
            // (Let pass only where the noise is unbounded: unverified.)
            if a.over() || a.take_unbounded() {
                unverified |= vouches;
            }
        }
        unverified |= r.unverified & !r.refuted;
        if shaky {
            unverified |= ALL & !r.refuted;
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
        // Where f is defined isn't known: nor is where it has poles (an
        // undefined point the domain was wrong about may be one), nor any
        // "none" found by scanning where it was said to be defined (the
        // zero at 2⁴⁰ of (x − 2⁴⁰)/(x − 2⁴⁰ − 1/16), claimed excluded).
        if drop & flags::DOMAIN != 0 || a.unknown(flags::DOMAIN) {
            let k = &a.k;
            let mut deps = flags::VERTICAL_ASYMPTOTES;
            // A function constant where defined: its period and its
            // constant pieces are its holes' (0·sin x / sin x's claimed
            // kπ/12 with the domain it came with).
            if drop & flags::RANGE == 0
                && matches!(k.data.range.as_slice(), [iv] if iv.lo.value == iv.hi.value)
            {
                deps |= flags::PERIODICITY | flags::MONOTONE_INTERVALS;
            }
            for (flag, none) in [
                (
                    flags::ZEROS,
                    // "None", or every x of the domain ("x ∈ …").
                    k.data.zeros.is_empty()
                        && (k.x_intercept.is_empty() || k.x_intercept.starts_with("x ∈")),
                ),
                (flags::MINIMA, k.data.minima.is_empty()),
                (flags::MAXIMA, k.data.maxima.is_empty()),
                (
                    flags::INFLECTION_POINTS,
                    k.data.inflection_points.is_empty(),
                ),
                (
                    flags::HORIZONTAL_ASYMPTOTES,
                    k.data.horizontal_asymptotes.is_empty(),
                ),
                (
                    flags::OBLIQUE_ASYMPTOTES,
                    k.data.oblique_asymptotes.is_empty(),
                ),
            ] {
                if none {
                    deps |= flag;
                }
            }
            let deps = (0..13)
                .map(|b| 1u32 << b)
                .filter(|&f| deps & f != 0 && !a.unknown(f))
                .fold(0, |m, f| m | f);
            drop |= deps;
            r.dropped.2 |= deps & !dropped & !r.refuted & !unverified;
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

/// Features stated within one period, which go if the period goes (the
/// range too: it was found over one period).
const PER_PERIOD: u32 = flags::DOMAIN
    | flags::RANGE
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
