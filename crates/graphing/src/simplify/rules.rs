//! The rewrite rules, each with its stated side condition.
//!
//! A rule `lhs → rhs` with condition C must satisfy, for every real value
//! of its pattern variables that satisfies C: **if lhs is defined, rhs is
//! defined and has the same value**. So a rule may enlarge the domain
//! (`a/a → 1`) but never shrink it, and the input's holes, which come only
//! from its own side conditions ([`super::side`]), are never lost by
//! rewriting. `tests/simplify_rules.rs` (feature `mpfr-oracle`) checks
//! every rule this way against MPFR at 256 bits, in every angle unit.
//!
//! Patterns name the angle unit's turns with placeholders that
//! [`RuleSpec::lhs_for`] fills in: `QUARTER` (π/2, 90, 100), `HALF` (π,
//! 180, 200) and `TURN` (2π, 360, 400).

use egg::{ConditionalApplier, Id, Pattern, Rewrite, Subst, Var};

use super::analysis::{Facts, MathGraph};
use super::lang::Math;
use super::q::Q;
use crate::functions::TrigUnit;

/// A condition on one pattern variable, checked on its class (at rewrite
/// time) or used to choose sample values (in the MPFR test).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarCond {
    /// Strictly positive throughout.
    Pos,
    /// Strictly negative throughout.
    Neg,
    /// An exact integer constant (|n| < 10⁶, a syntactic exponent).
    Int,
    /// An exact even integer constant.
    EvenInt,
    /// An exact odd integer constant.
    OddInt,
    /// A rational constant p/q with q odd, |p|, |q| < 10⁶ (a real-root
    /// exponent that is odd or even in the base by p's parity).
    OddDenom,
    /// An exact multiple of the full turn (2kπ, 360k°, 400k grads).
    Turn,
    /// An exact odd multiple of the half turn ((2k+1)π, …).
    OddHalfTurn,
    /// An exact multiple of the half turn (kπ, …).
    HalfTurn,
}

/// One rule.
#[derive(Clone, Copy, Debug)]
pub struct RuleSpec {
    /// Name (unique).
    pub name: &'static str,
    /// Left-hand pattern (s-expression, may contain unit placeholders).
    pub lhs: &'static str,
    /// Right-hand pattern.
    pub rhs: &'static str,
    /// Conditions on pattern variables.
    pub conds: &'static [(&'static str, VarCond)],
    /// The identity's mathematical side condition, in words.
    pub why: &'static str,
}

impl RuleSpec {
    /// The left pattern for `unit`.
    pub fn lhs_for(&self, unit: TrigUnit) -> String {
        fill(self.lhs, unit)
    }

    /// The right pattern for `unit`.
    pub fn rhs_for(&self, unit: TrigUnit) -> String {
        fill(self.rhs, unit)
    }
}

/// Replaces the unit placeholders.
pub fn fill(p: &str, unit: TrigUnit) -> String {
    let (quarter, half, turn) = match unit {
        TrigUnit::Radians => ("(/ pi 2)", "pi", "(* 2 pi)"),
        TrigUnit::Degrees => ("90", "180", "360"),
        TrigUnit::Grads => ("100", "200", "400"),
    };
    p.replace("QUARTER", quarter)
        .replace("HALF", half)
        .replace("TURN", turn)
}

macro_rules! r {
    ($name:literal, $lhs:literal => $rhs:literal; $why:literal) => {
        RuleSpec { name: $name, lhs: $lhs, rhs: $rhs, conds: &[], why: $why }
    };
    ($name:literal, $lhs:literal => $rhs:literal if [$(($v:literal, $c:ident)),+]; $why:literal) => {
        RuleSpec { name: $name, lhs: $lhs, rhs: $rhs, conds: &[$(($v, VarCond::$c)),+], why: $why }
    };
}

/// Every rule. The `why` strings state where the identity holds; the
/// conditions make sure it is only used there.
#[rustfmt::skip]
pub const RULES: &[RuleSpec] = &[
    // ----------------------------------------------------------- algebra
    r!("add-comm", "(+ ?a ?b)" => "(+ ?b ?a)"; "all reals"),
    r!("mul-comm", "(* ?a ?b)" => "(* ?b ?a)"; "all reals"),
    r!("add-assoc", "(+ ?a (+ ?b ?c))" => "(+ (+ ?a ?b) ?c)"; "all reals"),
    r!("add-assoc-r", "(+ (+ ?a ?b) ?c)" => "(+ ?a (+ ?b ?c))"; "all reals"),
    r!("mul-assoc", "(* ?a (* ?b ?c))" => "(* (* ?a ?b) ?c)"; "all reals"),
    r!("mul-assoc-r", "(* (* ?a ?b) ?c)" => "(* ?a (* ?b ?c))"; "all reals"),
    r!("sub-to-add", "(- ?a ?b)" => "(+ ?a (neg ?b))"; "all reals"),
    r!("add-neg-to-sub", "(+ ?a (neg ?b))" => "(- ?a ?b)"; "all reals"),
    r!("neg-neg", "(neg (neg ?a))" => "?a"; "all reals"),
    r!("neg-to-mul", "(neg ?a)" => "(* -1 ?a)"; "all reals"),
    r!("mul-m1-to-neg", "(* -1 ?a)" => "(neg ?a)"; "all reals"),
    r!("neg-add", "(neg (+ ?a ?b))" => "(+ (neg ?a) (neg ?b))"; "all reals"),
    r!("neg-mul", "(* (neg ?a) ?b)" => "(neg (* ?a ?b))"; "all reals"),
    r!("neg-mul-out", "(neg (* ?a ?b))" => "(* (neg ?a) ?b)"; "all reals"),
    r!("neg-div", "(/ (neg ?a) ?b)" => "(neg (/ ?a ?b))"; "b ≠ 0 on both sides"),
    r!("div-neg", "(/ ?a (neg ?b))" => "(neg (/ ?a ?b))"; "b ≠ 0 on both sides"),
    r!("add-0", "(+ ?a 0)" => "?a"; "all reals"),
    r!("mul-1", "(* ?a 1)" => "?a"; "all reals"),
    r!("mul-0", "(* ?a 0)" => "0"; "wherever a is defined (rhs everywhere)"),
    r!("sub-self", "(- ?a ?a)" => "0"; "wherever a is defined"),
    r!("div-self", "(/ ?a ?a)" => "1"; "a ≠ 0 where the lhs is defined"),
    r!("div-1", "(/ ?a 1)" => "?a"; "all reals"),
    r!("div-to-recip", "(/ ?a ?b)" => "(* ?a (/ 1 ?b))"; "b ≠ 0 on both sides"),
    r!("recip-to-div", "(* ?a (/ 1 ?b))" => "(/ ?a ?b)"; "b ≠ 0 on both sides"),
    r!("mul-div", "(* (/ ?a ?b) ?c)" => "(/ (* ?a ?c) ?b)"; "b ≠ 0 on both sides"),
    r!("div-mul-cancel", "(/ (* ?a ?b) ?a)" => "?b"; "a ≠ 0 where the lhs is defined"),
    r!("div-div", "(/ (/ ?a ?b) ?c)" => "(/ ?a (* ?b ?c))"; "b, c ≠ 0 ⇔ bc ≠ 0"),
    r!("distribute", "(* ?a (+ ?b ?c))" => "(+ (* ?a ?b) (* ?a ?c))"; "all reals"),
    r!("factor", "(+ (* ?a ?b) (* ?a ?c))" => "(* ?a (+ ?b ?c))"; "all reals"),
    // ---------------------------------------------------- integer powers
    r!("sq-to-mul", "(^ ?a 2)" => "(* ?a ?a)"; "all reals"),
    r!("mul-to-sq", "(* ?a ?a)" => "(^ ?a 2)"; "all reals"),
    r!("pow-1", "(^ ?a 1)" => "?a"; "all reals"),
    r!("pow-0", "(^ ?a 0)" => "1"; "a ≠ 0 where the lhs is defined (0⁰ undefined)"),
    r!("pow-m1", "(^ ?a -1)" => "(/ 1 ?a)"; "a ≠ 0 on both sides"),
    r!("pow-add", "(* (^ ?a ?p) (^ ?a ?q))" => "(^ ?a (+ ?p ?q))" if [("?p", Int), ("?q", Int)];
        "integers p, q: a ≠ 0 whenever p+q ≤ 0, as the lhs then needs"),
    r!("pow-pow", "(^ (^ ?a ?p) ?q)" => "(^ ?a (* ?p ?q))" if [("?p", Int), ("?q", Int)];
        "integers p, q: pq ≤ 0 only if p ≤ 0 or q ≤ 0, which need a ≠ 0"),
    r!("pow-mul", "(* (^ ?a ?n) (^ ?b ?n))" => "(^ (* ?a ?b) ?n)" if [("?n", Int)];
        "integer n: a, b ≠ 0 ⇔ ab ≠ 0 when n ≤ 0"),
    r!("diff-sq", "(- (^ ?a 2) (^ ?b 2))" => "(* (- ?a ?b) (+ ?a ?b))"; "all reals"),
    r!("diff-sq-1", "(- (^ ?a 2) 1)" => "(* (- ?a 1) (+ ?a 1))"; "all reals"),
    r!("neg-pow-even", "(^ (neg ?a) ?n)" => "(^ ?a ?n)" if [("?n", EvenInt)]; "even integer n"),
    r!("neg-pow-odd", "(^ (neg ?a) ?n)" => "(neg (^ ?a ?n))" if [("?n", OddInt)]; "odd integer n"),
    r!("abs-sq", "(^ (abs ?a) 2)" => "(^ ?a 2)"; "all reals"),
    r!("sqrt-sq", "(sqrt (^ ?a 2))" => "(abs ?a)"; "all reals"),
    r!("sq-sqrt", "(^ (sqrt ?a) 2)" => "?a"; "a ≥ 0 where the lhs is defined"),
    r!("abs-neg", "(abs (neg ?a))" => "(abs ?a)"; "all reals"),
    r!("cbrt-neg", "(cbrt (neg ?a))" => "(neg (cbrt ?a))"; "all reals (real cube root)"),
    // -------------------------------------------------------- exp and ln
    r!("ln-exp", "(ln (exp ?a))" => "?a"; "all reals"),
    r!("exp-ln", "(exp (ln ?a))" => "?a"; "a > 0 where the lhs is defined"),
    r!("exp-0", "(exp 0)" => "1"; "constant"),
    r!("exp-1", "(exp 1)" => "e"; "constant"),
    r!("ln-1", "(ln 1)" => "0"; "constant"),
    r!("ln-e", "(ln e)" => "1"; "constant"),
    r!("exp-pow", "(^ (exp ?a) ?n)" => "(exp (* ?n ?a))"; "e^a > 0: every power meaning agrees"),
    r!("exp-powc", "(^c (exp ?a) ?n)" => "(exp (* ?n ?a))"; "e^a > 0"),
    r!("exp-powv", "(^v (exp ?a) ?n)" => "(exp (* ?n ?a))"; "e^a > 0"),
    r!("exp-add", "(exp (+ ?a ?b))" => "(* (exp ?a) (exp ?b))"; "all reals"),
    r!("exp-mul", "(* (exp ?a) (exp ?b))" => "(exp (+ ?a ?b))"; "all reals"),
    r!("ln-pow", "(ln (^ ?a ?n))" => "(* ?n (ln ?a))" if [("?a", Pos)]; "a > 0"),
    r!("ln-powc", "(ln (^c ?a ?n))" => "(* ?n (ln ?a))" if [("?a", Pos)]; "a > 0"),
    r!("ln-powv", "(ln (^v ?a ?n))" => "(* ?n (ln ?a))" if [("?a", Pos)]; "a > 0"),
    r!("ln-mul", "(ln (* ?a ?b))" => "(+ (ln ?a) (ln ?b))" if [("?a", Pos), ("?b", Pos)]; "a, b > 0"),
    r!("ln-div", "(ln (/ ?a ?b))" => "(- (ln ?a) (ln ?b))" if [("?a", Pos), ("?b", Pos)]; "a, b > 0"),
    // -------------------------------------------------------------- trig
    r!("tan-def", "(tan ?a)" => "(/ (sin ?a) (cos ?a))"; "same poles"),
    r!("sec-def", "(sec ?a)" => "(/ 1 (cos ?a))"; "same poles"),
    r!("csc-def", "(csc ?a)" => "(/ 1 (sin ?a))"; "same poles"),
    r!("cot-def", "(cot ?a)" => "(/ (cos ?a) (sin ?a))"; "same poles"),
    r!("pythag", "(+ (^ (sin ?a) 2) (^ (cos ?a) 2))" => "1"; "all reals"),
    r!("pythag-sin", "(- 1 (^ (sin ?a) 2))" => "(^ (cos ?a) 2)"; "all reals"),
    r!("pythag-cos", "(- 1 (^ (cos ?a) 2))" => "(^ (sin ?a) 2)"; "all reals"),
    r!("sin-neg", "(sin (neg ?a))" => "(neg (sin ?a))"; "odd"),
    r!("cos-neg", "(cos (neg ?a))" => "(cos ?a)"; "even"),
    r!("tan-neg", "(tan (neg ?a))" => "(neg (tan ?a))"; "odd, same poles"),
    r!("sin-turn", "(sin (+ ?a ?w))" => "(sin ?a)" if [("?w", Turn)]; "period: a full turn"),
    r!("cos-turn", "(cos (+ ?a ?w))" => "(cos ?a)" if [("?w", Turn)]; "period: a full turn"),
    r!("sin-half", "(sin (+ ?a ?w))" => "(neg (sin ?a))" if [("?w", OddHalfTurn)]; "odd half turns"),
    r!("cos-half", "(cos (+ ?a ?w))" => "(neg (cos ?a))" if [("?w", OddHalfTurn)]; "odd half turns"),
    r!("tan-half", "(tan (+ ?a ?w))" => "(tan ?a)" if [("?w", HalfTurn)]; "period: a half turn, same poles"),
    r!("one-minus-cos", "(- 1 (cos ?a))" => "(* 2 (^ (sin (/ ?a 2)) 2))"; "half-angle, any unit"),
    // -------------------------------------------------- inverse trig
    r!("acot-def", "(acot ?a)" => "(- QUARTER (atan ?a))"; "range (0, half turn), acot 0 = quarter"),
    r!("acot-pos", "(acot ?a)" => "(atan (/ 1 ?a))" if [("?a", Pos)]; "a > 0 (no cancellation)"),
    r!("acot-neg", "(acot ?a)" => "(+ HALF (atan (/ 1 ?a)))" if [("?a", Neg)]; "a < 0"),
    r!("asec-def", "(asec ?a)" => "(acos (/ 1 ?a))"; "|a| ≥ 1 ⇔ |1/a| ≤ 1"),
    r!("acsc-def", "(acsc ?a)" => "(asin (/ 1 ?a))"; "|a| ≥ 1 ⇔ |1/a| ≤ 1"),
    r!("asin-neg", "(asin (neg ?a))" => "(neg (asin ?a))"; "odd"),
    r!("atan-neg", "(atan (neg ?a))" => "(neg (atan ?a))"; "odd"),
    // -------------------------------------------------------- hyperbolic
    r!("sinh-neg", "(sinh (neg ?a))" => "(neg (sinh ?a))"; "odd"),
    r!("cosh-neg", "(cosh (neg ?a))" => "(cosh ?a)"; "even"),
    r!("tanh-neg", "(tanh (neg ?a))" => "(neg (tanh ?a))"; "odd"),
    r!("asinh-neg", "(asinh (neg ?a))" => "(neg (asinh ?a))"; "odd"),
    r!("atanh-neg", "(atanh (neg ?a))" => "(neg (atanh ?a))"; "odd, |a| < 1 both sides"),
    r!("cosh-sinh", "(- (^ (cosh ?a) 2) (^ (sinh ?a) 2))" => "1"; "all reals"),
    // ------------------------------------------------------- stability
    r!("sqrt-diff", "(- (sqrt (+ ?a 1)) (sqrt ?a))" => "(/ 1 (+ (sqrt (+ ?a 1)) (sqrt ?a)))";
        "a ≥ 0 where the lhs is defined; the denominator is ≥ 1"),
];

fn check(c: VarCond, unit: TrigUnit, d: &super::analysis::Data) -> bool {
    let small = |k: Q| k.numer().unsigned_abs() < 1_000_000 && k.denom() < 1_000_000;
    let turn_q = |q: Q| -> Option<Q> {
        // The class as a multiple of the half turn.
        match unit {
            TrigUnit::Radians => d.pim,
            TrigUnit::Degrees => q.div(Q::int(180)),
            TrigUnit::Grads => q.div(Q::int(200)),
        }
    };
    let halves = || -> Option<Q> {
        match unit {
            TrigUnit::Radians => d.pim,
            _ => turn_q(d.q?),
        }
    };
    match c {
        VarCond::Pos => !d.iv.is_empty() && d.iv.lo() > 0.0,
        VarCond::Neg => !d.iv.is_empty() && d.iv.hi() < 0.0,
        VarCond::Int => d.q.is_some_and(|k| k.is_int() && small(k)),
        VarCond::EvenInt => {
            d.q.is_some_and(|k| k.is_int() && small(k) && k.numer() % 2 == 0)
        }
        VarCond::OddInt => {
            d.q.is_some_and(|k| k.is_int() && small(k) && k.numer() % 2 != 0)
        }
        VarCond::OddDenom => d.q.is_some_and(|k| small(k) && k.denom() % 2 == 1),
        VarCond::Turn => halves().is_some_and(|h| h.is_int() && h.numer() % 2 == 0),
        VarCond::OddHalfTurn => halves().is_some_and(|h| h.is_int() && h.numer() % 2 != 0),
        VarCond::HalfTurn => halves().is_some_and(|h| h.is_int()),
    }
}

fn condition(var: Var, c: VarCond) -> impl Fn(&mut MathGraph, Id, &Subst) -> bool {
    move |eg: &mut MathGraph, _id, s| {
        let unit = eg.analysis.unit;
        check(c, unit, &eg[s[var]].data)
    }
}

struct Checked {
    conds: Vec<(Var, VarCond)>,
}

impl egg::Condition<Math, Facts> for Checked {
    fn check(&self, eg: &mut MathGraph, id: Id, s: &Subst) -> bool {
        self.conds.iter().all(|(v, c)| condition(*v, *c)(eg, id, s))
    }
}

/// The rules as egg rewrites for `unit`.
pub fn rewrites(unit: TrigUnit) -> Vec<Rewrite<Math, Facts>> {
    RULES
        .iter()
        .map(|r| {
            let lhs: Pattern<Math> = r.lhs_for(unit).parse().expect("rule lhs parses");
            let rhs: Pattern<Math> = r.rhs_for(unit).parse().expect("rule rhs parses");
            if r.conds.is_empty() {
                Rewrite::new(r.name, lhs, rhs).expect("rule is well formed")
            } else {
                let conds = r
                    .conds
                    .iter()
                    .map(|(v, c)| (v.parse().expect("variable"), *c))
                    .collect();
                let applier = ConditionalApplier {
                    condition: Checked { conds },
                    applier: rhs,
                };
                Rewrite::new(r.name, lhs, applier).expect("rule is well formed")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_parses_in_every_unit() {
        for unit in [TrigUnit::Radians, TrigUnit::Degrees, TrigUnit::Grads] {
            assert_eq!(rewrites(unit).len(), RULES.len());
        }
        let mut names: Vec<_> = RULES.iter().map(|r| r.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), RULES.len(), "rule names are unique");
    }
}
