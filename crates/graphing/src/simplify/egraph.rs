//! Running the rules: simplification and parity proofs.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use egg::{BackoffScheduler, CostFunction, Extractor, Id, Language, RecExpr, Runner, StopReason};

use super::analysis::{Facts, MathGraph};
use super::lang::{ExactLiterals, Math, PI, Unsupported, from_rec, to_rec};
use super::q::Q;
use super::rules::rewrites;
use super::side::{Cond, Jump, jumps, side_conditions};
use crate::ast::Expr;
use crate::compile::{CompileOptions, DEFAULT_VARIABLE_VALUE};
use crate::functions::TrigUnit;
use crate::interval::Interval;

/// Bounds on one saturation run.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// E-graph nodes.
    pub nodes: usize,
    /// Rule-application rounds.
    pub iterations: usize,
    /// Wall time.
    pub time: Duration,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            nodes: 20_000,
            iterations: 12,
            time: Duration::from_millis(50),
        }
    }
}

/// What the simplifier works with.
#[derive(Clone, Copy)]
pub struct Settings<'a> {
    /// Angle unit.
    pub unit: TrigUnit,
    /// The x values considered (ℝ for a function's global analysis).
    pub x: Interval,
    /// Slider values.
    pub variables: &'a dyn crate::compile::VariableValues,
    /// The exact decimals typed for the literals.
    pub literals: &'a ExactLiterals,
    /// Checked between rounds; a set flag stops the run.
    pub cancel: Option<&'a AtomicBool>,
    /// Saturation bounds.
    pub limits: Limits,
}

impl<'a> Settings<'a> {
    /// Settings for `opts` with the given literals, over all of ℝ.
    pub fn new(opts: &CompileOptions<'a>, literals: &'a ExactLiterals) -> Settings<'a> {
        Settings {
            unit: opts.trig_unit,
            x: Interval::ENTIRE,
            variables: opts.variables,
            literals,
            cancel: None,
            limits: Limits::default(),
        }
    }

    fn facts(&self, e: &Expr) -> Facts {
        Facts {
            unit: self.unit,
            x: self.x,
            vars: e
                .variables()
                .into_iter()
                .map(|n| {
                    let v = self.variables.value(&n).unwrap_or(DEFAULT_VARIABLE_VALUE);
                    (n, v)
                })
                .collect(),
        }
    }
}

/// Why a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// No rule had anything left to add.
    Saturated,
    /// A node, round or time limit.
    Limit,
    /// The cancel flag.
    Cancelled,
}

/// A simplified expression and what it must still respect.
#[derive(Clone, Debug)]
pub struct Simplified {
    /// The simplified expression. Equal to the input wherever the input
    /// is defined; it may be defined in more places (`x/x` gives 1).
    pub expr: Expr,
    /// The same as an e-graph term.
    pub term: RecExpr<Math>,
    /// The input's domain conditions ([`side_conditions`]): the domain is
    /// where all hold (and the simplified form is defined).
    pub conditions: Vec<Cond>,
    /// The input's step-function jumps.
    pub jumps: Vec<Jump>,
    /// True if the expression changed.
    pub changed: bool,
    /// How the run ended.
    pub stop: Stop,
}

/// AST size, with numerically poor forms made dear so the extraction
/// prefers their stable equivalents when the rules found one:
/// `acot t` (cancels for large t; `atan(1/t)` doesn't), `quarter − atan`,
/// `1 − cos a`, `√(a+1) − √a`.
struct StableCost<'g> {
    egraph: &'g MathGraph,
}

/// A test on one e-node.
type NodeTest<'a> = &'a dyn Fn(&Math) -> bool;

impl StableCost<'_> {
    fn has(&self, id: Id, f: &dyn Fn(&Math) -> bool) -> bool {
        self.egraph[id].nodes.iter().any(f)
    }

    /// The class holds −c for a class c with a node matching `f`.
    fn has_neg(&self, id: Id, f: &dyn Fn(&Math) -> bool) -> bool {
        self.egraph[id]
            .nodes
            .iter()
            .any(|n| matches!(n, Math::Neg(c) if self.has(*c, f)))
    }

    fn is_one(&self, id: Id) -> bool {
        self.egraph[id].data.q == Some(Q::ONE)
    }

    fn is_quarter(&self, id: Id) -> bool {
        let d = &self.egraph[id].data;
        d.pim == Q::new(1, 2) || d.q == Some(Q::int(90)) || d.q == Some(Q::int(100))
    }

    /// a − b (written `a − b` or `a + (−b)`) that cancels for some x.
    fn cancels(&self, a: Id, b_has: &dyn Fn(NodeTest<'_>) -> bool) -> bool {
        let cos = |n: &Math| matches!(n, Math::Cos(_));
        let atan = |n: &Math| matches!(n, Math::Atan(_));
        let sqrt = |n: &Math| matches!(n, Math::Sqrt(_));
        (self.is_one(a) && b_has(&cos))
            || (self.is_quarter(a) && b_has(&atan))
            || (self.has(a, &sqrt) && b_has(&sqrt))
    }
}

impl CostFunction<Math> for StableCost<'_> {
    type Cost = usize;
    fn cost<C>(&mut self, enode: &Math, mut costs: C) -> usize
    where
        C: FnMut(Id) -> usize,
    {
        let penalty = match enode {
            Math::Acot(_) => 6,
            Math::Sub([a, b]) if self.cancels(*a, &|f| self.has(*b, f)) => 10,
            Math::Add([a, b])
                if self.cancels(*a, &|f| self.has_neg(*b, f))
                    || self.cancels(*b, &|f| self.has_neg(*a, f)) =>
            {
                10
            }
            _ => 0,
        };
        enode.fold(1 + penalty, |s, i| s.saturating_add(costs(i)))
    }
}

/// Runs the rules on `egraph` until saturation or a limit. The cancel
/// flag is checked before the run; the time limit bounds how long a set
/// flag can go unseen.
fn saturate(egraph: MathGraph, s: &Settings<'_>) -> (MathGraph, Stop) {
    if s.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return (egraph, Stop::Cancelled);
    }
    let facts = egraph.analysis.clone();
    let r: Runner<Math, Facts, ()> = Runner::new(facts)
        .with_egraph(egraph)
        .with_scheduler(BackoffScheduler::default())
        .with_iter_limit(s.limits.iterations)
        .with_node_limit(s.limits.nodes)
        .with_time_limit(s.limits.time)
        .run(&rewrites(s.unit));
    let stop = match r.stop_reason {
        Some(StopReason::Saturated) => Stop::Saturated,
        _ if s.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) => Stop::Cancelled,
        _ => Stop::Limit,
    };
    (r.egraph, stop)
}

/// Simplifies `e`, a function of x.
pub fn simplify(e: &Expr, s: &Settings<'_>) -> Result<Simplified, Unsupported> {
    let start = to_rec(e, s.literals)?;
    let conditions = side_conditions(e);
    let jumps = jumps(e);
    let mut egraph = MathGraph::new(s.facts(e));
    let root = egraph.add_expr(&start);
    let (egraph, stop) = saturate(egraph, s);
    let root = egraph.find(root);
    let extractor = Extractor::new(&egraph, StableCost { egraph: &egraph });
    let (_, best) = extractor.find_best(root);
    // The input is in the e-graph, so the best term costs no more.
    let changed = from_rec(&best) != from_rec(&start);
    let term = if changed { best } else { start };
    Ok(Simplified {
        expr: from_rec(&term),
        term,
        conditions,
        jumps,
        changed,
        stop,
    })
}

/// A function's proven symmetry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parity {
    /// f(−x) = f(x) on a domain symmetric about 0.
    Even,
    /// f(−x) = −f(x) on a domain symmetric about 0.
    Odd,
}

fn mirror(e: &Expr) -> Expr {
    e.map(&|n| matches!(n, Expr::X).then(|| Expr::Neg(Box::new(Expr::X))))
}

fn add_term(egraph: &mut MathGraph, rec: &RecExpr<Math>) -> Id {
    egraph.add_expr(rec)
}

/// Proves that `e` is even or odd. `None` means not proven (it may still
/// be either; interval witnesses can disprove).
pub fn prove_parity(e: &Expr, s: &Settings<'_>) -> Option<Parity> {
    if !e.contains_x() {
        // A constant: even, on ℝ (or nowhere).
        return Some(Parity::Even);
    }
    // Parity is about all of ℝ: symmetric box.
    let mut facts = s.facts(e);
    facts.x = Interval::ENTIRE;
    let mut egraph = MathGraph::new(facts);
    let f = add_term(&mut egraph, &to_rec(e, s.literals).ok()?);
    let fm = add_term(&mut egraph, &to_rec(&mirror(e), s.literals).ok()?);
    let neg_f = egraph.add(Math::Neg(f));
    // Each condition's expression and its mirror, to show the domain is
    // symmetric.
    let conds = side_conditions(e);
    let mut pairs = Vec::new();
    for c in &conds {
        let exprs: Vec<&Expr> = match c {
            Cond::NonZero(g)
            | Cond::Positive(g)
            | Cond::NonNegative(g)
            | Cond::Within { e: g, .. }
            | Cond::Outside { e: g, .. }
            | Cond::TrigNonZero { arg: g, .. }
            | Cond::Opaque(g) => vec![g],
            Cond::PowVar { base, exponent } | Cond::PowConst { base, exponent } => {
                vec![base, exponent]
            }
        };
        let mut ids = Vec::new();
        for g in exprs {
            let a = add_term(&mut egraph, &to_rec(g, s.literals).ok()?);
            let b = add_term(&mut egraph, &to_rec(&mirror(g), s.literals).ok()?);
            let na = egraph.add(Math::Neg(a));
            ids.push((a, b, na));
        }
        pairs.push((c, ids));
    }
    let (eg, stop) = saturate(egraph, s);
    if stop == Stop::Cancelled {
        return None;
    }
    let eg = &eg;
    let same = |a: Id, b: Id| eg.find(a) == eg.find(b);
    // Each condition: its expressions even, or (where the condition's set
    // is symmetric) odd.
    for (c, ids) in &pairs {
        let ok = ids.iter().all(|(a, b, na)| {
            let even = same(*a, *b);
            let odd = same(*b, *na);
            match c {
                Cond::NonZero(_) | Cond::Outside { .. } | Cond::TrigNonZero { .. } => even || odd,
                Cond::Within { lo, hi, .. } => {
                    let symmetric = match (lo, hi) {
                        (Some(l), Some(h)) => l.value == -h.value && l.closed == h.closed,
                        (None, None) => true,
                        _ => false,
                    };
                    even || (odd && symmetric)
                }
                _ => even,
            }
        });
        if !ok {
            return None;
        }
    }
    if same(f, fm) {
        Some(Parity::Even)
    } else if same(fm, neg_f) {
        Some(Parity::Odd)
    } else {
        None
    }
}

/// True if `id`'s class is π times a rational (for callers that build
/// their own e-graphs).
pub fn pi_multiple(egraph: &MathGraph, id: Id) -> Option<Q> {
    egraph[id].data.pim
}

/// Re-exported symbol name for π, for callers building patterns.
pub const PI_SYMBOL: &str = PI;
