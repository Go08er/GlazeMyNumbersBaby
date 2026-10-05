//! The e-graph analysis: what is known of each class's value.
//!
//! * `q`: the exact rational the class is, if it's a constant (folded
//!   exactly: 1/0, 0⁰ and overflows give nothing).
//! * `pim`: the rational k with the class equal to k·π exactly (for the
//!   periodicity rules in radians).
//! * `iv`: an enclosure of the class's values for x in the analysis box,
//!   at the points where the class is defined (empty where it is defined
//!   nowhere). It gates the conditional rules (`ln(ab) → ln a + ln b` only
//!   when a > 0 and b > 0 throughout).
//!
//! Classes are merged by **intersecting** their enclosures. Every rule
//! keeps or enlarges the domain of the term it matched (checked against
//! MPFR, `tests/simplify_rules.rs`), so the narrowest domain in a class is
//! that of the term the input contained, and each member's enclosure holds
//! the class's values there: their intersection does too. A hull merge
//! would also be sound but never settles on cycles such as
//! `ln(exp(a)) = a` (each pass widens by an ulp).

use egg::{Analysis, DidMerge, EGraph, Id, Language};

use super::lang::{E, Math, PI, X};
use super::q::Q;
use crate::ast::Func;
use crate::functions::TrigUnit;
use crate::interval::{DecInterval, Interval, elem};

/// What the analysis knows of a class.
#[derive(Clone, Debug)]
pub struct Data {
    /// The exact rational value, for a constant class.
    pub q: Option<Q>,
    /// k with value = k·π exactly.
    pub pim: Option<Q>,
    /// Enclosure of the values on the analysis box, where defined.
    pub iv: Interval,
}

/// The analysis: angle unit, the box of x, and slider values.
#[derive(Clone, Debug)]
pub struct Facts {
    /// The angle unit of the trig functions.
    pub unit: TrigUnit,
    /// The x values considered (ℝ to simplify a function globally).
    pub x: Interval,
    /// Slider values (symbols `$name`), enclosed (a point, or under a
    /// digit limit the doubles either side of the decimal).
    pub vars: Vec<(String, Interval)>,
}

impl Default for Facts {
    fn default() -> Facts {
        Facts {
            unit: TrigUnit::Radians,
            x: Interval::new(f64::NEG_INFINITY, f64::INFINITY),
            vars: Vec::new(),
        }
    }
}

/// The e-graph type the simplifier uses.
pub type MathGraph = EGraph<Math, Facts>;

fn di(i: Interval) -> DecInterval {
    DecInterval::new(i)
}

/// The interval semantics of one node, given its children's enclosures and
/// constants (shared with the extraction-time checks).
pub fn node_interval(
    node: &Math,
    unit: TrigUnit,
    iv: &dyn Fn(Id) -> Interval,
    q: &dyn Fn(Id) -> Option<Q>,
) -> Interval {
    let a = |i: &Id| di(iv(*i));
    let one = |i: &Id, f: &dyn Fn(&DecInterval) -> DecInterval| f(&a(i)).iv;
    let ang = |i: &Id, f: &dyn Fn(&DecInterval, TrigUnit) -> DecInterval| f(&a(i), unit).iv;
    match node {
        Math::Add([x, y]) => elem::add(&a(x), &a(y)).iv,
        Math::Sub([x, y]) => elem::sub(&a(x), &a(y)).iv,
        Math::Mul([x, y]) => elem::mul(&a(x), &a(y)).iv,
        Math::Div([x, y]) => elem::div(&a(x), &a(y)).iv,
        Math::PowQ([b, k]) => match q(*k) {
            Some(k) if k.is_int() => match k.as_int().and_then(|n| i32::try_from(n).ok()) {
                Some(n) => elem::powi(&a(b), n).iv,
                None => Interval::new(f64::NEG_INFINITY, f64::INFINITY),
            },
            Some(k) => match (i32::try_from(k.numer()), i32::try_from(k.denom())) {
                (Ok(p), Ok(d)) => elem::pow_rational(&a(b), p, d).iv,
                _ => Interval::new(f64::NEG_INFINITY, f64::INFINITY),
            },
            None => Interval::new(f64::NEG_INFINITY, f64::INFINITY),
        },
        Math::PowC([b, e]) => {
            // functions::pow: a negative base only to an integer value.
            match q(*e)
                .and_then(|k| k.as_int())
                .and_then(|n| i32::try_from(n).ok())
            {
                Some(n) => elem::powi(&a(b), n).iv,
                None => elem::pow(&a(b), &a(e)).iv,
            }
        }
        Math::PowV([b, e]) => elem::pow(&a(b), &a(e)).iv,
        Math::Neg(x) => elem::neg(&a(x)).iv,
        Math::Sin(x) => ang(x, &elem::sin),
        Math::Cos(x) => ang(x, &elem::cos),
        Math::Tan(x) => ang(x, &elem::tan),
        Math::Sec(x) => ang(x, &elem::sec),
        Math::Csc(x) => ang(x, &elem::csc),
        Math::Cot(x) => ang(x, &elem::cot),
        Math::Asin(x) => ang(x, &elem::asin),
        Math::Acos(x) => ang(x, &elem::acos),
        Math::Atan(x) => ang(x, &elem::atan),
        Math::Asec(x) => ang(x, &elem::asec),
        Math::Acsc(x) => ang(x, &elem::acsc),
        Math::Acot(x) => ang(x, &elem::acot),
        Math::Sinh(x) => one(x, &elem::sinh),
        Math::Cosh(x) => one(x, &elem::cosh),
        Math::Tanh(x) => one(x, &elem::tanh),
        Math::Asinh(x) => one(x, &elem::asinh),
        Math::Atanh(x) => one(x, &elem::atanh),
        Math::Sqrt(x) => one(x, &elem::sqrt),
        Math::Cbrt(x) => one(x, &elem::cbrt),
        Math::Ln(x) => one(x, &elem::ln),
        Math::Exp(x) => one(x, &elem::exp),
        Math::Abs(x) => one(x, &elem::abs),
        Math::Call(t, args) => {
            let arg = |k: usize| a(&args[k]);
            match t.0 {
                Func::Sech => elem::sech(&arg(0)).iv,
                Func::Csch => elem::csch(&arg(0)).iv,
                Func::Coth => elem::coth(&arg(0)).iv,
                Func::Acosh => elem::acosh(&arg(0)).iv,
                Func::Asech => elem::asech(&arg(0)).iv,
                Func::Acsch => elem::acsch(&arg(0)).iv,
                Func::Acoth => elem::acoth(&arg(0)).iv,
                Func::Root => elem::root(&arg(0), &arg(1)).iv,
                Func::Log => elem::log10(&arg(0)).iv,
                Func::LogBase => elem::log_base(&arg(0), &arg(1)).iv,
                Func::Floor => elem::floor(&arg(0)).iv,
                Func::Ceil => elem::ceil(&arg(0)).iv,
                Func::Round => elem::round(&arg(0)).iv,
                Func::Sign => elem::sign(&arg(0)).iv,
                Func::Mod => elem::modulo(&arg(0), &arg(1)).iv,
                Func::Factorial => elem::factorial(&arg(0)).iv,
                Func::DoubleFactorial => elem::double_factorial(&arg(0)).iv,
                Func::NCr => elem::ncr_npr(&arg(0), &arg(1), false).iv,
                Func::NPr => elem::ncr_npr(&arg(0), &arg(1), true).iv,
                Func::Min | Func::Max => args
                    .iter()
                    .map(a)
                    .reduce(|p, c| {
                        if t.0 == Func::Min {
                            elem::min(&p, &c)
                        } else {
                            elem::max(&p, &c)
                        }
                    })
                    .map_or(Interval::EMPTY, |d| d.iv),
                _ => Interval::ENTIRE,
            }
        }
        Math::Num(c) => c.interval(),
        Math::Real(r) => Interval::around(r.value()),
        Math::Symbol(_) => unreachable!("symbols are handled by the caller"),
    }
}

/// Exact constant folding of one node.
pub fn fold(node: &Math, q: &dyn Fn(Id) -> Option<Q>) -> Option<Q> {
    match node {
        Math::Num(c) => Some(*c),
        Math::Add([a, b]) => q(*a)?.add(q(*b)?),
        Math::Sub([a, b]) => q(*a)?.sub(q(*b)?),
        Math::Mul([a, b]) => q(*a)?.mul(q(*b)?),
        Math::Div([a, b]) => q(*a)?.div(q(*b)?),
        Math::Neg(a) => q(*a)?.neg(),
        Math::PowQ([b, k]) | Math::PowC([b, k]) | Math::PowV([b, k]) => {
            let k = q(*k)?.as_int()?;
            // Keep folded numbers small enough to stay useful.
            if k.unsigned_abs() > 64 {
                return None;
            }
            q(*b)?.powi(k)
        }
        Math::Abs(a) => q(*a)?.abs(),
        _ => None,
    }
}

/// Folding of rational multiples of π.
fn fold_pim(node: &Math, q: &dyn Fn(Id) -> Option<Q>, p: &dyn Fn(Id) -> Option<Q>) -> Option<Q> {
    let pz = |i: Id| p(i).or_else(|| q(i).filter(|c| c.is_zero()));
    match node {
        Math::Symbol(s) if s.as_str() == PI => Some(Q::ONE),
        Math::Num(c) if c.is_zero() => Some(Q::ZERO),
        Math::Add([a, b]) => pz(*a)?.add(pz(*b)?),
        Math::Sub([a, b]) => pz(*a)?.sub(pz(*b)?),
        Math::Neg(a) => p(*a)?.neg(),
        Math::Mul([a, b]) => match (q(*a), p(*a), q(*b), p(*b)) {
            (Some(c), _, _, Some(k)) | (_, Some(k), Some(c), _) => c.mul(k),
            _ => None,
        },
        Math::Div([a, b]) => p(*a)?.div(q(*b)?),
        _ => None,
    }
}

impl Analysis<Math> for Facts {
    type Data = Data;

    fn make(egraph: &mut MathGraph, enode: &Math, _id: Id) -> Data {
        let q = |i: Id| egraph[i].data.q;
        let p = |i: Id| egraph[i].data.pim;
        let iv = |i: Id| egraph[i].data.iv;
        let unit = egraph.analysis.unit;
        let value = match enode {
            Math::Symbol(s) => match s.as_str() {
                X => egraph.analysis.x,
                PI => elem::pi(),
                E => elem::e(),
                name => egraph
                    .analysis
                    .vars
                    .iter()
                    .find(|(n, _)| name == format!("${n}"))
                    .map_or(Interval::new(f64::NEG_INFINITY, f64::INFINITY), |(_, v)| *v),
            },
            node => node_interval(node, unit, &iv, &q),
        };
        let cq = fold(enode, &q);
        let iv = match cq {
            Some(c) => c.interval().intersect(value).or(c.interval()),
            None => value,
        };
        Data {
            q: cq,
            pim: fold_pim(enode, &q, &p),
            iv,
        }
    }

    fn merge(&mut self, to: &mut Data, from: Data) -> DidMerge {
        let before = to.iv;
        let meet = to.iv.intersect(from.iv);
        // Disjoint non-empty enclosures can't both hold the class's values
        // on its domain unless that domain is empty within the box; keep
        // the one already there rather than invent an empty class.
        let new = if meet.is_empty() && !to.iv.is_empty() && !from.iv.is_empty() {
            to.iv
        } else {
            meet
        };
        to.iv = new;
        let mut changed_to = before != new;
        let changed_from = from.iv != new;
        let mut from_more = false;
        if to.q.is_none() && from.q.is_some() {
            to.q = from.q;
            changed_to = true;
        } else if from.q.is_none() && to.q.is_some() {
            from_more = true;
        }
        if to.pim.is_none() && from.pim.is_some() {
            to.pim = from.pim;
            changed_to = true;
        } else if from.pim.is_none() && to.pim.is_some() {
            from_more = true;
        }
        DidMerge(changed_to, changed_from || from_more)
    }

    fn modify(egraph: &mut MathGraph, id: Id) {
        if let Some(c) = egraph[id].data.q {
            let added = egraph.add(Math::Num(c));
            egraph.union(id, added);
        }
    }
}

trait Or {
    fn or(self, o: Interval) -> Interval;
}

impl Or for Interval {
    fn or(self, o: Interval) -> Interval {
        if self.is_empty() { o } else { self }
    }
}

/// Re-exported for the rule conditions: the class data of `id`.
pub fn data(egraph: &MathGraph, id: Id) -> &Data {
    &egraph[id].data
}

/// True if any node of the class mentions x (directly or below).
pub fn mentions_x(egraph: &MathGraph, id: Id) -> bool {
    // A constant class is x-free whatever its members look like.
    if egraph[id].data.q.is_some() || egraph[id].data.pim.is_some() {
        return false;
    }
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![egraph.find(id)];
    while let Some(c) = stack.pop() {
        if !seen.insert(c) {
            continue;
        }
        for n in &egraph[c].nodes {
            if let Math::Symbol(s) = n
                && s.as_str() == X
            {
                return true;
            }
            for ch in n.children() {
                stack.push(egraph.find(*ch));
            }
        }
    }
    false
}

/// An enclosure of a term's value, x ranging over `facts.x` (no e-graph:
/// each node once, children first).
pub fn rec_interval(rec: &egg::RecExpr<Math>, facts: &Facts) -> Interval {
    let nodes = rec.as_ref();
    let mut ivs: Vec<Interval> = Vec::with_capacity(nodes.len());
    let mut qs: Vec<Option<Q>> = Vec::with_capacity(nodes.len());
    for node in nodes {
        let iv = |i: Id| ivs[usize::from(i)];
        let q = |i: Id| qs[usize::from(i)];
        let v = match node {
            Math::Symbol(s) => match s.as_str() {
                X => facts.x,
                PI => elem::pi(),
                E => elem::e(),
                name => facts
                    .vars
                    .iter()
                    .find(|(n, _)| name == format!("${n}"))
                    .map_or(Interval::ENTIRE, |(_, v)| *v),
            },
            n => node_interval(n, facts.unit, &iv, &q),
        };
        let fq = fold(node, &q);
        ivs.push(v);
        qs.push(fq);
    }
    ivs.last().copied().unwrap_or(Interval::EMPTY)
}
