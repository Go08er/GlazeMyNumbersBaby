//! Checking one claim: each kind's statement (`certify::cert`'s doc
//! comments) proved with the replay's own enclosures, by bisection of the
//! box and, failing that, at a higher precision.

use super::eval::{Lits, at_path, contains_x, written_rational};
use super::iv::{self, Iv, Unit};
use super::series::{self as se};
use super::{B, Claim, Class, Fx, Outcome, Side, Subject, Via, exact, growth};
use graphing::ast::{BinOp, Constant, Expr, Func};

/// The precisions tried in turn (bits).
const PRECS: [u32; 2] = [160, 640];
/// How many times one check may split its box.
const BUDGET: u32 = 3000;

/// Which tree a check ran on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tree {
    /// The canonical tree of the source.
    Orig,
    /// The certifier's: the simplifier's form of f, the derivative trees.
    Eval,
}

/// A check's verdict on one box.
#[derive(Clone, Debug)]
pub enum V {
    Yes,
    /// Proven false (only on the canonical tree).
    No(String),
    Unknown(String),
}

impl V {
    fn yes(&self) -> bool {
        matches!(self, V::Yes)
    }
}

fn unknown(s: impl Into<String>) -> V {
    V::Unknown(s.into())
}

/// Whether the tree uses something the replay doesn't model.
pub fn unmodelled(e: &Expr) -> bool {
    match e {
        Expr::Y => true,
        Expr::Num(..) | Expr::Const(_) | Expr::X | Expr::Var(_) => false,
        Expr::Neg(a) | Expr::Degrees(a) => unmodelled(a),
        Expr::Bin(_, a, b) => unmodelled(a) || unmodelled(b),
        Expr::Call(f, args) => {
            matches!(
                f,
                Func::Factorial | Func::DoubleFactorial | Func::NCr | Func::NPr
            ) || args.iter().any(unmodelled)
        }
    }
}

// ------------------------------------------------------------ boxes

/// A double strictly inside `(lo, hi)` to split at (geometric across wide
/// spans, out to the tails by factors of 16).
pub fn split(lo: f64, hi: f64) -> Option<f64> {
    if lo >= hi || lo.is_nan() || hi.is_nan() {
        return None;
    }
    let m = if lo == f64::NEG_INFINITY && hi == f64::INFINITY {
        0.0
    } else if hi == f64::INFINITY {
        if lo < 0.0 {
            0.0
        } else if lo < 1.0 {
            1.0
        } else {
            lo * 16.0
        }
    } else if lo == f64::NEG_INFINITY {
        if hi > 0.0 {
            0.0
        } else if hi > -1.0 {
            -1.0
        } else {
            hi * 16.0
        }
    } else if lo > 0.0 && hi / lo > 64.0 {
        lo.sqrt() * hi.sqrt()
    } else if hi < 0.0 && lo / hi > 64.0 {
        -((-lo).sqrt() * (-hi).sqrt())
    } else {
        lo / 2.0 + hi / 2.0
    };
    if lo < m && m < hi {
        Some(m)
    } else {
        let n = lo.next_up();
        (n < hi).then_some(n)
    }
}

/// `check` on `[lo, hi]`, splitting where it can't decide.
pub fn prove(lo: f64, hi: f64, budget: &mut u32, check: &mut dyn FnMut(f64, f64) -> V) -> V {
    match check(lo, hi) {
        V::Unknown(why) => {
            if *budget == 0 {
                return V::Unknown(format!("{why} (out of budget)"));
            }
            let Some(m) = split(lo, hi) else {
                // No double inside: the ends, as points, may still show the
                // claim false.
                if lo < hi {
                    // (±∞ is no point of the box: x is real.)
                    for p in [lo, hi].into_iter().filter(|p| p.is_finite()) {
                        if let V::No(w) = check(p, p) {
                            return V::No(w);
                        }
                    }
                }
                return V::Unknown(format!("{why} on [{lo:e}, {hi:e}]"));
            };
            *budget -= 1;
            match prove(lo, m, budget, check) {
                V::Yes => prove(m, hi, budget, check),
                other => other,
            }
        }
        v => v,
    }
}

/// `f` at each precision in turn, until it decides.
pub fn ladder(mut f: impl FnMut() -> V) -> V {
    let mut last = unknown("");
    for p in PRECS {
        iv::set_prec(p);
        last = f();
        if !matches!(last, V::Unknown(_)) {
            break;
        }
    }
    iv::set_prec(PRECS[0]);
    last
}

fn tail_box(side: Side, from: f64) -> B {
    match side {
        Side::Right => (from, f64::INFINITY),
        Side::Left => (f64::NEG_INFINITY, from),
    }
}

fn show(v: &Iv) -> String {
    if v.empty {
        "undefined".into()
    } else {
        format!("[{:.6e}, {:.6e}]", v.lo.to_f64(), v.hi.to_f64())
    }
}

/// (k + j)! / j!.
fn falling(k: usize, j: usize) -> f64 {
    ((j + 1)..=(j + k)).map(|i| i as f64).product()
}

// ------------------------------------------------------------ subjects

/// A claim's subject as a tree to evaluate: s = (the tree)⁽ᵏ⁾.
pub struct Subj<'a> {
    pub fx: &'a Fx,
    pub of: Subject,
    pub e: Expr,
    pub k: usize,
    pub tree: Tree,
}

impl<'a> Subj<'a> {
    /// The certifier's tree this subject is read from was shown identical
    /// to the canonical one exactly ([`super::algebra`]).
    pub fn verified(&self) -> bool {
        match (self.tree, &self.of) {
            (Tree::Orig, _) => true,
            (Tree::Eval, Subject::F(k)) if *k > 0 && self.k == 0 => self.fx.verified.d[k - 1],
            (Tree::Eval, Subject::F(_)) => self.fx.verified.f,
            (Tree::Eval, Subject::G(..)) => false,
        }
    }

    pub fn new(fx: &'a Fx, of: &Subject, tree: Tree) -> Option<Subj<'a>> {
        let (e, k) = match of {
            Subject::F(k) => match tree {
                Tree::Orig => (fx.f.clone(), *k),
                Tree::Eval => {
                    if *k > 0
                        && let Some(d) = &fx.d[k - 1]
                    {
                        (d.clone(), 0)
                    } else {
                        (fx.f_eval.clone()?, *k)
                    }
                }
            },
            Subject::G(path, via) => {
                if tree == Tree::Eval {
                    return None;
                }
                (side_tree(&fx.f, path, *via)?, 0)
            }
        };
        Some(Subj {
            fx,
            of: of.clone(),
            e,
            k,
            tree,
        })
    }

    /// c[j] ∋ s⁽ʲ⁾/j! over `[lo, hi]` for j ≤ m, and whether s is valid
    /// there (f defined, and for k ≥ 1 continuous with its derivatives up
    /// to k; g defined), judged on the canonical tree.
    pub fn coeffs(&self, lo: f64, hi: f64, m: usize) -> (Vec<Iv>, bool) {
        let mut s = self.fx.series(&self.e, lo, hi, self.k + m);
        // f's own tree ∞ − ∞ on a tail: its Horner form, the same function.
        if self.tree == Tree::Orig
            && matches!(self.of, Subject::F(_))
            && !valid_f(&s, self.k)
            && let Some(h) = &self.fx.f_alt
        {
            let t = self.fx.series(h, lo, hi, self.k + m);
            if valid_f(&t, self.k) {
                s = t;
            }
        }
        let c: Vec<Iv> = (0..=m)
            .map(|j| {
                if self.k == 0 {
                    s[j].clone()
                } else {
                    iv::mul(&s[self.k + j], &Iv::of(falling(self.k, j)))
                }
            })
            .collect();
        let valid = match &self.of {
            Subject::G(..) => !s[0].empty && s[0].def,
            Subject::F(k) => {
                if self.tree == Tree::Orig {
                    valid_f(&s, *k)
                } else {
                    // f's validity, as for its own tree: from its Horner
                    // form where f's tree is ∞ − ∞ on a tail (the same
                    // function on the same domain: 1/(x² + x + 1) at −∞).
                    let ok = |e: &Expr| valid_f(&self.fx.series(e, lo, hi, *k), *k);
                    (ok(&self.fx.f) || self.fx.f_alt.as_ref().is_some_and(ok))
                        && !c[0].empty
                        && c[0].def
                }
            }
        };
        (c, valid)
    }

    /// f (the canonical tree) is defined nowhere on the box: the subject
    /// can't be valid there.
    pub fn nowhere(&self, lo: f64, hi: f64) -> bool {
        let e = match &self.of {
            Subject::F(_) => &self.fx.f,
            Subject::G(..) => &self.e,
        };
        self.fx.series(e, lo, hi, 0)[0].empty
    }

    /// The enclosure of c[j] over the box, tightened by the mean value
    /// form where c[j+1] exists; with validity, and whether c[j] is
    /// continuous there (c[j+1] exists, or j = 0 and the value is).
    pub fn enc(&self, j: usize, lo: f64, hi: f64) -> (Iv, bool, bool) {
        let (c, valid) = self.coeffs(lo, hi, j + 1);
        let mut v = c[j].clone();
        let next = &c[j + 1];
        let smooth = !next.empty && next.def;
        let cont = smooth || (j == 0 && !v.empty && v.cont);
        if lo < hi && lo.is_finite() && hi.is_finite() && smooth && next.bounded() {
            let m = lo / 2.0 + hi / 2.0;
            let (cm, _) = self.coeffs(m, m, j);
            if !cm[j].empty && cm[j].bounded() {
                let dx = iv::sub(&Iv::of2(lo, hi), &Iv::of(m));
                let mv = iv::add(&cm[j], &iv::mul(&iv::scale(next, (j + 1) as f64), &dx));
                v = tighten(&v, &mv);
            }
        }
        (v, valid, cont)
    }

    /// The subject at a double.
    pub fn at(&self, x: f64) -> (Iv, bool) {
        let (c, valid) = self.coeffs(x, x, 0);
        (c[0].clone(), valid)
    }

    /// The subject at a double equals `c` exactly: by the enclosure, or by
    /// exact rational arithmetic (f only).
    pub fn exactly(&self, x: f64, c: f64) -> Option<bool> {
        let (v, valid) = self.at(x);
        if !valid {
            return None;
        }
        if v.is_exactly(c) {
            return Some(true);
        }
        if !(v.lo <= c && v.hi >= c) {
            return Some(false);
        }
        if self.k == 0 {
            let q = exact::eval(
                &self.e,
                exact::of_f64(x).as_ref(),
                &self.fx.lits,
                &self.fx.vars,
                self.fx.unit,
            )?;
            return Some(exact::of_f64(c).is_some_and(|c| c == q));
        }
        None
    }
}

/// f valid for order k: defined, and for k ≥ 1 continuous with
/// derivatives up to k.
fn valid_f(s: &[Iv], k: usize) -> bool {
    !s[0].empty && s[0].def && (k == 0 || (s[0].cont && (1..=k).all(|j| !s[j].empty && s[j].def)))
}

/// `v`'s bounds narrowed to `w`'s, keeping `v`'s decoration.
fn tighten(v: &Iv, w: &Iv) -> Iv {
    if v.empty || w.empty {
        return v.clone();
    }
    let m = v.meet(w);
    let mut out = v.clone();
    if m.lo <= m.hi {
        out.lo = m.lo;
        out.hi = m.hi;
    }
    out
}

/// The side expression g at `path`: the node itself, or the cosine (sine)
/// of the argument of the tan or sec (cot or csc) there.
pub fn side_tree(f: &Expr, path: &[u8], via: Via) -> Option<Expr> {
    let node = at_path(f, path)?;
    Some(match via {
        Via::Itself => node.clone(),
        Via::CosOfArg => match node {
            Expr::Call(Func::Tan | Func::Sec, args) => Expr::Call(Func::Cos, vec![args[0].clone()]),
            _ => return None,
        },
        Via::SinOfArg => match node {
            Expr::Call(Func::Cot | Func::Csc, args) => Expr::Call(Func::Sin, vec![args[0].clone()]),
            _ => return None,
        },
    })
}

// ------------------------------------------------------------ checks

/// f's own tree defined on all of `x` (shown on the canonical tree).
pub fn orig_defined(fx: &Fx, x: B) -> bool {
    defined(fx, x, Some(false)).class == Class::Strong
}

/// Runs `f` on the canonical tree, then (if it couldn't decide) on the
/// certifier's tree, but only where f's own tree is shown defined on each
/// box of `on` (the claim's): the certifier's tree equals f only where f
/// is defined. (For f′ and f″ the subject's validity also asks f's own
/// derivatives there, [`Subj::coeffs`].) Strong, weak, refuted or
/// unconfirmed.
fn by_trees(fx: &Fx, of: &Subject, on: &[B], mut f: impl FnMut(&Subj) -> V) -> Outcome {
    let Some(s) = Subj::new(fx, of, Tree::Orig) else {
        return Outcome::new(Class::Unsupported, "no such side expression");
    };
    if unmodelled(&s.e) {
        return Outcome::new(
            Class::Unsupported,
            "the tree uses a function the replay doesn't model",
        );
    }
    let why = match ladder(|| f(&s)) {
        V::Yes => return Outcome::new(Class::Strong, ""),
        V::No(why) => return Outcome::new(Class::Refuted, why),
        V::Unknown(why) => why,
    };
    if let Some(t) = Subj::new(fx, of, Tree::Eval)
        && t.e != s.e
        && on.iter().all(|x| orig_defined(fx, *x))
        && let V::Yes = ladder(|| match f(&t) {
            V::No(w) => V::Unknown(w),
            v => v,
        })
    {
        if t.verified() {
            return Outcome::new(
                Class::Strong,
                "on the certifier's tree, shown identical to f's exactly",
            );
        }
        return Outcome::new(Class::Weak, "on the certifier's tree");
    }
    Outcome::new(Class::Unconfirmed, why)
}

fn on(x: B, check: &mut dyn FnMut(f64, f64) -> V) -> V {
    let mut budget = BUDGET;
    prove(x.0, x.1, &mut budget, check)
}

/// The subject valid on the box and strictly on side `above` of c.
/// The factors of a product (or quotient's numerator and denominator),
/// flattened.
fn mul_factors(e: &Expr, out: &mut Vec<Expr>) {
    match e {
        Expr::Bin(BinOp::Mul, a, b) => {
            mul_factors(a, out);
            mul_factors(b, out);
        }
        _ => out.push(e.clone()),
    }
}

fn product_of(fs: &[Expr]) -> Expr {
    fs.iter()
        .cloned()
        .reduce(|a, b| Expr::Bin(BinOp::Mul, Box::new(a), Box::new(b)))
        .unwrap_or(Expr::exact(1.0))
}

/// e's strict sign over [lo, hi] (`true`: positive) from its parts: its
/// enclosure, or a product or quotient of parts each strictly signed, or a
/// sum or difference whose terms share a factor c (a·c ± b·c = c·(a ± b))
/// signed as c and the rest. (x·e⁻ˣ's f′, e⁻ˣ − x·e⁻ˣ, is e⁻ˣ·(1 − x):
/// negative on [16, ∞) though its enclosure there holds 0.) Identities
/// and the sign rules of products only: the replay's own.
fn factor_sign(fx: &Fx, e: &Expr, lo: f64, hi: f64, depth: usize) -> Option<bool> {
    let v = fx.series(e, lo, hi, 0)[0].clone();
    if v.empty || !v.def {
        return None;
    }
    if v.gt(0.0) {
        return Some(true);
    }
    if v.lt(0.0) {
        return Some(false);
    }
    if depth == 0 {
        return None;
    }
    let d = depth - 1;
    match e {
        Expr::Neg(a) => factor_sign(fx, a, lo, hi, d).map(|s| !s),
        Expr::Bin(BinOp::Mul | BinOp::Div, a, b) => {
            Some(factor_sign(fx, a, lo, hi, d)? == factor_sign(fx, b, lo, hi, d)?)
        }
        Expr::Bin(op @ (BinOp::Add | BinOp::Sub), a, b) => {
            let (mut fa, mut fb) = (Vec::new(), Vec::new());
            mul_factors(a, &mut fa);
            mul_factors(b, &mut fb);
            let c = fa.iter().find(|f| contains_x(f) && fb.contains(f))?.clone();
            let ia = fa.iter().position(|f| *f == c)?;
            let ib = fb.iter().position(|f| *f == c)?;
            fa.remove(ia);
            fb.remove(ib);
            let rest = Expr::Bin(*op, Box::new(product_of(&fa)), Box::new(product_of(&fb)));
            Some(factor_sign(fx, &c, lo, hi, d)? == factor_sign(fx, &rest, lo, hi, d)?)
        }
        _ => None,
    }
}

fn beyond(s: &Subj, x: B, c: f64, above: bool, cont: bool) -> V {
    on(x, &mut |lo, hi| {
        let (v, valid, is_cont) = s.enc(0, lo, hi);
        if !valid {
            if s.tree == Tree::Orig && s.nowhere(lo, hi) {
                return V::No(format!("undefined on [{lo:e}, {hi:e}]"));
            }
            return unknown("not shown valid");
        }
        if cont && !is_cont {
            return unknown("not shown continuous");
        }
        if v.beyond(c, above) {
            return V::Yes;
        }
        // Against 0, the tree's own factors may tell.
        if c == 0.0 && s.k == 0 && factor_sign(s.fx, &s.e, lo, hi, 4) == Some(above) {
            return V::Yes;
        }
        let wrong = if above { v.le(c) } else { v.ge(c) };
        if s.tree == Tree::Orig && wrong {
            return V::No(format!("{} on [{lo:e}, {hi:e}]", show(&v)));
        }
        unknown(format!("{} on [{lo:e}, {hi:e}]", show(&v)))
    })
}

/// The subject at a double strictly on side `above` of c.
fn end_beyond(s: &Subj, x: f64, c: f64, above: bool) -> V {
    let (v, valid) = s.at(x);
    if !valid {
        if s.tree == Tree::Orig && s.nowhere(x, x) {
            return V::No(format!("undefined at {x:e}"));
        }
        return unknown(format!("not valid at {x:e}"));
    }
    if v.beyond(c, above) {
        return V::Yes;
    }
    let wrong = if above { v.le(c) } else { v.ge(c) };
    if s.tree == Tree::Orig && wrong {
        return V::No(format!("{} at {x:e}", show(&v)));
    }
    unknown(format!("{} at {x:e}", show(&v)))
}

/// The subject continuous and strictly monotone on the box: c[1] valid
/// and of one sign throughout.
fn monotone(s: &Subj, x: B) -> V {
    let m = if x.0 < x.1 {
        x.0 / 2.0 + x.1 / 2.0
    } else {
        x.0
    };
    let (d, _, _) = s.enc(1, m, m);
    let signs: Vec<bool> = if d.gt(0.0) {
        vec![true]
    } else if d.lt(0.0) {
        vec![false]
    } else {
        vec![true, false]
    };
    let mut last = unknown("not shown monotone");
    for up in signs {
        last = on(x, &mut |lo, hi| {
            let (d, valid, _) = s.enc(1, lo, hi);
            if !valid || d.empty || !d.def {
                return unknown(format!("derivative not shown on [{lo:e}, {hi:e}]"));
            }
            if d.beyond(0.0, up) {
                V::Yes
            } else {
                unknown(format!("derivative {} on [{lo:e}, {hi:e}]", show(&d)))
            }
        });
        if last.yes() {
            return V::Yes;
        }
    }
    last
}

fn all(vs: impl IntoIterator<Item = V>) -> V {
    let mut out = V::Yes;
    for v in vs {
        match v {
            V::Yes => {}
            V::No(w) => return V::No(w),
            V::Unknown(w) => {
                if out.yes() {
                    out = V::Unknown(w);
                }
            }
        }
    }
    out
}

pub fn check(fx: &Fx, c: &Claim, cert: &[Claim]) -> Outcome {
    if let Some(why) = &fx.refused {
        return Outcome::new(
            Class::Unconfirmed,
            format!("{why}: the language refuses f, so no claim about it stands"),
        );
    }
    match c {
        Claim::Defined(x) => defined(fx, *x, Some(false)),
        Claim::Continuous(x) => defined(fx, *x, Some(true)),
        Claim::Undefined(x) => defined(fx, *x, None),
        Claim::Beyond { x, of, c, above } => {
            by_trees(fx, of, &[*x], |s| beyond(s, *x, *c, *above, false))
        }
        Claim::NoCross { x, of, c, above } => by_trees(fx, of, &[*x], |s| {
            all([
                end_beyond(s, x.0, *c, *above),
                end_beyond(s, x.1, *c, *above),
                monotone(s, *x),
            ])
        }),
        Claim::OneCross { x, of, c } => by_trees(fx, of, &[*x], |s| {
            let a = end_beyond(s, x.0, *c, true);
            let below_first = !a.yes();
            all([
                end_beyond(s, x.0, *c, !below_first),
                end_beyond(s, x.1, *c, below_first),
                monotone(s, *x),
            ])
        }),
        Claim::ExactAt { x, of, c, at } => by_trees(fx, of, &[*x], |s| {
            if !(x.0 <= *at && *at <= x.1) {
                return V::No(format!("{at:e} is outside the box"));
            }
            let exact = match s.exactly(*at, *c) {
                Some(true) => V::Yes,
                Some(false) if s.tree == Tree::Orig => {
                    V::No(format!("{} at {at:e}", show(&s.at(*at).0)))
                }
                _ => unknown(format!("{} at {at:e}", show(&s.at(*at).0))),
            };
            all([exact, monotone(s, *x)])
        }),
        Claim::Factors { x, of, c, above } => by_trees(fx, of, &[*x], |s| {
            let direct = beyond(s, *x, *c, *above, true);
            if !matches!(direct, V::Unknown(_)) {
                return direct;
            }
            factors(s, *x, *c, *above, None)
        }),
        Claim::Touch {
            x,
            of,
            c,
            at,
            order,
            above,
        } => by_trees(fx, of, &[*x], |s| touch(s, *x, *c, *at, *order, *above)),
        Claim::Value { x, of, lo, hi } => by_trees(fx, of, &[*x], |s| value(s, *x, *lo, *hi)),
        Claim::Family { of, x0, period } => family(fx, of, *x0, *period),
        Claim::TailBeyond {
            side,
            from,
            of,
            c,
            above,
        } => by_trees(fx, of, &[tail_box(*side, *from)], |s| {
            beyond(s, tail_box(*side, *from), *c, *above, false)
        }),
        Claim::TailChain {
            side,
            from,
            of,
            c,
            above,
            order,
        } => by_trees(fx, of, &[tail_box(*side, *from)], |s| {
            tail_chain(s, *side, *from, *c, *above, *order)
        }),
        Claim::TailValue { side, from, lo, hi } => {
            by_trees(fx, &Subject::F(0), &[tail_box(*side, *from)], |s| {
                value(s, tail_box(*side, *from), *lo, *hi)
            })
        }
        Claim::Unbounded { near, at } => unbounded(fx, *near, *at),
        Claim::Kink(x) => defined(fx, *x, Some(true)),
        Claim::KinkAt { x, at, left, right } => kink_at(fx, *x, *at, *left, *right),
        Claim::Bounded { near, at, lo, hi } => {
            if !(near.0 <= at.0 && at.1 <= near.1) {
                return Outcome::new(
                    Class::Refuted,
                    "the excluded point's box is not inside near",
                );
            }
            // Off the excluded point (the certifier's tree may be defined
            // there, f isn't).
            let off = [(near.0, at.0.next_down()), (at.1.next_up(), near.1)];
            let mut off: Vec<B> = off.into_iter().filter(|b| b.0 <= b.1).collect();
            if off.is_empty() {
                off.push(*near);
            }
            by_trees(fx, &Subject::F(0), &off, |s| {
                where_defined(s, *near, *lo, *hi)
            })
        }
        Claim::Removable { near, at, lo, hi } => removable(fx, *near, *at, *lo, *hi),
        Claim::Simplifier(fact) => simplifier(fx, fact),
        Claim::Limit { at, over_x, to } => limit(fx, *at, *over_x, *to),
        Claim::TailLine {
            side,
            from,
            m,
            b,
            lo,
            hi,
        } => tail_line(fx, *side, *from, *m, *b, *lo, *hi),
        Claim::Gap(x) => gap(*x, cert),
        Claim::GapClear { x, of } => match of {
            Subject::F(k) if *k <= 2 => {
                if unmodelled(&fx.f) {
                    return Outcome::new(
                        Class::Unsupported,
                        "the tree uses a function the replay doesn't model",
                    );
                }
                match gap_clear(fx, *k, *x) {
                    Some(true) => Outcome::new(Class::Strong, ""),
                    Some(false) => Outcome::new(Class::Weak, "away from 0 on the certifier's tree"),
                    None => Outcome::new(Class::Unconfirmed, "not shown away from 0 in the gap"),
                }
            }
            _ => Outcome::new(Class::Unsupported, "a gap claim on a side expression"),
        },
    }
}

/// The kink arguments of `e`, from the spec: the u of each |u| and the
/// a − b of each min(a, b), max(a, b) that varies with x.
fn kink_args(e: &Expr, out: &mut Vec<Expr>) {
    match e {
        Expr::Call(Func::Abs, a) if contains_x(&a[0]) => {
            if !out.contains(&a[0]) {
                out.push(a[0].clone());
            }
            kink_args(&a[0], out);
        }
        Expr::Call(Func::Min | Func::Max, a) if a.len() == 2 && a.iter().any(contains_x) => {
            let d = Expr::Bin(BinOp::Sub, Box::new(a[0].clone()), Box::new(a[1].clone()));
            if !out.contains(&d) {
                out.push(d);
            }
            a.iter().for_each(|v| kink_args(v, out));
        }
        Expr::Neg(a) | Expr::Degrees(a) => kink_args(a, out),
        Expr::Bin(_, a, b) => {
            kink_args(a, out);
            kink_args(b, out);
        }
        Expr::Call(_, args) => args.iter().for_each(|v| kink_args(v, out)),
        _ => {}
    }
}

/// `e` on one side of a kink: each |u| as u or −u, each min, max as the
/// argument it picks, by the sign each kink argument has on that side.
fn one_sided(e: &Expr, sides: &[(Expr, bool, bool)], left: bool) -> Option<Expr> {
    let sign = |u: &Expr| {
        sides
            .iter()
            .find(|(v, ..)| v == u)
            .map(|(_, l, r)| if left { *l } else { *r })
    };
    Some(match e {
        Expr::Call(Func::Abs, a) if contains_x(&a[0]) => {
            let inner = one_sided(&a[0], sides, left)?;
            if sign(&a[0])? {
                inner
            } else {
                Expr::Neg(Box::new(inner))
            }
        }
        Expr::Call(f @ (Func::Min | Func::Max), a) if a.len() == 2 && a.iter().any(contains_x) => {
            let d = Expr::Bin(BinOp::Sub, Box::new(a[0].clone()), Box::new(a[1].clone()));
            let a_bigger = sign(&d)?;
            let pick = if a_bigger == (*f == Func::Max) {
                &a[0]
            } else {
                &a[1]
            };
            one_sided(pick, sides, left)?
        }
        Expr::Neg(a) => Expr::Neg(Box::new(one_sided(a, sides, left)?)),
        Expr::Degrees(a) => Expr::Degrees(Box::new(one_sided(a, sides, left)?)),
        Expr::Bin(op, a, b) => Expr::Bin(
            *op,
            Box::new(one_sided(a, sides, left)?),
            Box::new(one_sided(b, sides, left)?),
        ),
        Expr::Call(f, args) => Expr::Call(
            *f,
            args.iter()
                .map(|v| one_sided(v, sides, left))
                .collect::<Option<Vec<_>>>()?,
        ),
        _ => e.clone(),
    })
}

/// A kink placed exactly (`KinkAt`): some kink argument is exactly 0 at
/// `at`; every kink argument is strictly signed on each side of `at`
/// inside the box (away from 0 over it, or 0 at `at` with a strictly
/// signed derivative over it), so f is the one-sided form there; and that
/// form's derivative has the claimed strict sign on [x.0, at], [at, x.1].
fn kink_at(fx: &Fx, x: B, at: f64, left: bool, right: bool) -> Outcome {
    if !(x.0 < at && at < x.1) {
        return Outcome::new(Class::Refuted, "the kink's point is not inside its box");
    }
    if unmodelled(&fx.f) {
        return Outcome::new(
            Class::Unsupported,
            "the tree uses a function the replay doesn't model",
        );
    }
    let mut args = Vec::new();
    kink_args(&fx.f, &mut args);
    // (Set when a singular point's f′ was signed on the certifier's tree
    // of f′, compared with f′ at points only.)
    let weak = std::cell::Cell::new(false);
    let v = ladder(|| {
        let mut sides: Vec<(Expr, bool, bool)> = Vec::new();
        let mut zero = false;
        for u in &args {
            let s = fx.series(u, x.0, x.1, 1);
            if s[0].def && (s[0].gt(0.0) || s[0].lt(0.0)) {
                let pos = s[0].gt(0.0);
                sides.push((u.clone(), pos, pos));
                continue;
            }
            let at_u = fx.series(u, at, at, 0);
            let d = &s[1];
            if !(at_u[0].def && at_u[0].is_exactly(0.0)) {
                return unknown("a kink argument is not decided over the box");
            }
            if !(s[0].cont && d.def && d.bounded() && (d.gt(0.0) || d.lt(0.0))) {
                return unknown("a kink argument's slope is not decided over the box");
            }
            zero = true;
            let rising = d.gt(0.0);
            sides.push((u.clone(), !rising, rising));
        }
        if !zero {
            // No corner: a point where f′ is singular (u^r, r < 1, u = 0)?
            return singular_at(fx, x, at, left, right, &sides, &weak);
        }
        for (on_left, lo, hi, want) in [(true, x.0, at, left), (false, at, x.1, right)] {
            let Some(e) = one_sided(&fx.f, &sides, on_left) else {
                return unknown("no one-sided form");
            };
            let s = fx.series(&e, lo, hi, 1);
            let d = &s[1];
            if !(s[0].cont && d.def && d.bounded()) {
                return unknown("the one-sided form's slope is not decided");
            }
            if (want && d.lt(0.0)) || (!want && d.gt(0.0)) {
                return V::No("the one-sided slope has the other sign".into());
            }
            if !((want && d.gt(0.0)) || (!want && d.lt(0.0))) {
                return unknown("the one-sided slope is not strictly signed");
            }
        }
        V::Yes
    });
    match v {
        V::Yes if weak.get() => Outcome::new(Class::Weak, "f′ signed on the certifier's tree"),
        V::Yes => Outcome::new(Class::Strong, ""),
        V::No(w) => Outcome::new(Class::Refuted, w),
        V::Unknown(w) => Outcome::new(Class::Unconfirmed, w),
    }
}

/// The bases u (varying with x) of f's fractional powers u^r with r < 1
/// (√u, ∛u, a typed or written non-integer exponent): f′ is unbounded
/// where u = 0.
fn singular_args(fx: &Fx, e: &Expr, out: &mut Vec<Expr>) {
    let r = |b: &Expr| -> Option<f64> {
        if contains_x(b) {
            return None;
        }
        if let Some((p, q)) = super::eval::written_rational(b, &fx.lits) {
            if q == 1 {
                return None;
            }
            // (Below 1 exactly, though p/q may round to 1.)
            let r = rug::Rational::from((p, q));
            let v = r.to_f64();
            return Some(if r < 1 && v >= 1.0 {
                1.0f64.next_down()
            } else {
                v
            });
        }
        let v = fx.series(b, 0.0, 0.0, 0)[0].clone();
        (v.is_point() && !v.lo.is_integer()).then(|| v.lo.to_f64())
    };
    match e {
        Expr::Bin(BinOp::Pow, u, b) if contains_x(u) && r(b).is_some_and(|r| r < 1.0) => {
            if !out.contains(u) {
                out.push((**u).clone());
            }
            singular_args(fx, u, out);
        }
        Expr::Call(Func::Sqrt | Func::Cbrt, a) if contains_x(&a[0]) => {
            if !out.contains(&a[0]) {
                out.push(a[0].clone());
            }
            singular_args(fx, &a[0], out);
        }
        Expr::Neg(a) | Expr::Degrees(a) => singular_args(fx, a, out),
        Expr::Bin(_, a, b) => {
            singular_args(fx, a, out);
            singular_args(fx, b, out);
        }
        Expr::Call(_, args) => args.iter().for_each(|v| singular_args(fx, v, out)),
        _ => {}
    }
}

/// A `KinkAt` with no corner: f′ singular at `at` (a base of a power
/// u^r, r < 1, exactly 0 there), every such base strictly signed on each
/// side within the box, so f′ exists on the box but at `at`; f′'s
/// enclosure (f's series, or the certifier's tree of f′) over [x.0, at]
/// and [at, x.1], where it exists, has the claimed strict signs.
fn singular_at(
    fx: &Fx,
    x: B,
    at: f64,
    left: bool,
    right: bool,
    corners: &[(Expr, bool, bool)],
    weak: &std::cell::Cell<bool>,
) -> V {
    // (Every corner argument away from 0 over the box: f is smooth there
    // but where f′ is singular.)
    if corners.iter().any(|(_, l, r)| l != r) {
        return V::No("a corner argument changes sign in the box".into());
    }
    let mut args = Vec::new();
    singular_args(fx, &fx.f, &mut args);
    let mut zero = false;
    for u in &args {
        let s = fx.series(u, x.0, x.1, 1);
        if s[0].def && (s[0].gt(0.0) || s[0].lt(0.0)) {
            continue;
        }
        let at_u = fx.series(u, at, at, 0);
        let d = &s[1];
        if !(at_u[0].def && at_u[0].is_exactly(0.0)) {
            return unknown("a power's base is not decided over the box");
        }
        if !(s[0].cont && d.def && d.bounded() && (d.gt(0.0) || d.lt(0.0))) {
            return unknown("a power's base's slope is not decided over the box");
        }
        zero = true;
    }
    if !zero {
        return V::No("no corner or power's base is 0 at the point".into());
    }
    for (lo, hi, want) in [(x.0, at, left), (at, x.1, right)] {
        let far = if lo == at { hi } else { lo };
        let d = fx.series(&fx.f, lo, hi, 1)[1].clone();
        let sign = if d.gt(0.0) {
            Some(true)
        } else if d.lt(0.0) {
            Some(false)
        } else if let Some(t) = &fx.d[0] {
            // The certifier's tree of f′ (weak unless shown identical to
            // f′ exactly), signed factor by factor on the side less `at`.
            if !fx.verified.d[0] {
                weak.set(true);
            }
            open_sign(fx, t, lo, hi, at, far, 6)
        } else {
            None
        };
        match sign {
            Some(s) if s == want => {}
            Some(_) => return V::No("f′ has the other sign beside the point".into()),
            None => return unknown("f′ is not strictly signed beside the point"),
        }
    }
    V::Yes
}

/// e's strict sign on [lo, hi] less the point `at` (an end of it), `far`
/// the other end: its enclosure, or by its factors — a power u^(p/q) of a
/// base u exactly 0 at `at` and strictly monotone over the box has, off
/// `at`, the sign of u at `far` to the p (q odd), or is positive (q even).
fn open_sign(fx: &Fx, e: &Expr, lo: f64, hi: f64, at: f64, far: f64, depth: usize) -> Option<bool> {
    let v = fx.series(e, lo, hi, 0)[0].clone();
    if !v.empty && v.gt(0.0) {
        return Some(true);
    }
    if !v.empty && v.lt(0.0) {
        return Some(false);
    }
    let d = depth.checked_sub(1)?;
    match e {
        Expr::Neg(a) => open_sign(fx, a, lo, hi, at, far, d).map(|s| !s),
        Expr::Bin(BinOp::Mul | BinOp::Div, a, b) => {
            Some(open_sign(fx, a, lo, hi, at, far, d)? == open_sign(fx, b, lo, hi, at, far, d)?)
        }
        Expr::Bin(BinOp::Pow, u, b) => {
            let (p, q) = super::eval::written_rational(b, &fx.lits)?;
            let (p_even, q_even) = (p.is_even(), q.is_even());
            let s = fx.series(u, lo, hi, 1);
            let at_u = fx.series(u, at, at, 0);
            let monotone =
                s[0].cont && s[1].def && s[1].bounded() && (s[1].gt(0.0) || s[1].lt(0.0));
            if !(at_u[0].def && at_u[0].is_exactly(0.0) && monotone) {
                return None;
            }
            let uf = fx.series(u, far, far, 0)[0].clone();
            let pos = if uf.gt(0.0) {
                true
            } else if uf.lt(0.0) {
                false
            } else {
                return None;
            };
            if q_even {
                pos.then_some(true)
            } else {
                Some(pos || p_even)
            }
        }
        _ => None,
    }
}

/// f defined on the box (`Some(false)`), defined and continuous there
/// (`Some(true)`), or defined nowhere on it (`None`).
fn defined(fx: &Fx, x: B, want: Option<bool>) -> Outcome {
    if unmodelled(&fx.f) {
        return Outcome::new(
            Class::Unsupported,
            "the tree uses a function the replay doesn't model",
        );
    }
    let v = ladder(|| {
        on(x, &mut |lo, hi| {
            let s = fx.series(&fx.f, lo, hi, 0);
            let v = &s[0];
            let at = format!("on [{lo:e}, {hi:e}]");
            // f's Horner form, the same function on the same domain, where
            // f's tree is ∞ − ∞ on a tail (x² + x + 1 at −∞).
            let alt = |cont: bool| {
                fx.f_alt.as_ref().is_some_and(|h| {
                    let w = &fx.series(h, lo, hi, 0)[0];
                    !w.empty && w.def && (!cont || w.cont)
                })
            };
            match want {
                Some(cont) => {
                    if !v.empty && v.def && (!cont || v.cont) || !v.empty && alt(cont) {
                        V::Yes
                    } else if v.empty {
                        V::No(format!("undefined {at}"))
                    } else {
                        unknown(format!("{} {at}", show(v)))
                    }
                }
                None => {
                    if v.empty {
                        V::Yes
                    } else if v.def {
                        V::No(format!("defined {at}"))
                    } else {
                        unknown(format!("{} {at}", show(v)))
                    }
                }
            }
        })
    });
    match v {
        V::Yes => Outcome::new(Class::Strong, ""),
        V::No(w) => Outcome::new(Class::Refuted, w),
        V::Unknown(w) => Outcome::new(Class::Unconfirmed, w),
    }
}

fn value(s: &Subj, x: B, lo: f64, hi: f64) -> V {
    let v = value_valid(s, x, lo, hi);
    let V::No(why) = &v else { return v };
    // (At a point, f is defined or it isn't: the claim says it is.)
    if !why.starts_with("undefined") || x.0 == x.1 {
        return v;
    }
    // Not valid on the whole box (it holds a point where f is undefined):
    // do the values lie in [lo, hi] wherever f is defined? (On the
    // certifier's tree too, which equals f there.)
    let mut trees = vec![(Subj::new(s.fx, &s.of, Tree::Orig), "")];
    if let Some(t) = Subj::new(s.fx, &s.of, Tree::Eval) {
        trees.push((Some(t), " (on the certifier's tree)"));
    }
    for (t, on_what) in trees {
        let Some(t) = t else { continue };
        let where_defined = on(x, &mut |a, b| {
            let (c, _) = t.coeffs(a, b, 0);
            if c[0].empty || (c[0].ge(lo) && c[0].le(hi)) {
                V::Yes
            } else {
                unknown(format!("{} on [{a:e}, {b:e}]", show(&c[0])))
            }
        });
        if where_defined.yes() {
            return V::No(format!("{why}; {WHERE_DEFINED}{on_what}"));
        }
    }
    v
}

/// Wherever the subject is defined on the box, its values lie in
/// [lo, hi] (it need not be defined anywhere there): refuted only where it
/// is defined on a whole piece with values outside.
fn where_defined(s: &Subj, x: B, lo: f64, hi: f64) -> V {
    on(x, &mut |a, b| {
        let (c, _) = s.coeffs(a, b, 0);
        let v = &c[0];
        if v.empty || (v.ge(lo) && v.le(hi)) {
            V::Yes
        } else if s.tree == Tree::Orig && v.def && (v.lt(lo) || v.gt(hi)) {
            V::No(format!("{} on [{a:e}, {b:e}]", show(v)))
        } else {
            unknown(format!("{} on [{a:e}, {b:e}]", show(v)))
        }
    })
}

/// The note on a Value claim false only as written: it holds wherever f
/// is defined.
pub const WHERE_DEFINED: &str = "where f is defined its values do lie in [lo, hi]";

fn value_valid(s: &Subj, x: B, lo: f64, hi: f64) -> V {
    let point_claim = x.0 == x.1 && lo == hi;
    on(x, &mut |a, b| {
        let (v, valid, _) = s.enc(0, a, b);
        if !valid {
            if s.tree == Tree::Orig && s.nowhere(a, b) {
                return V::No(format!("undefined on [{a:e}, {b:e}]"));
            }
            return unknown(format!("not shown valid on [{a:e}, {b:e}]"));
        }
        if v.ge(lo) && v.le(hi) {
            return V::Yes;
        }
        if point_claim && let Some(e) = s.exactly(a, lo) {
            return if e {
                V::Yes
            } else if s.tree == Tree::Orig {
                V::No(format!("{} at {a:e}", show(&v)))
            } else {
                unknown("not exact")
            };
        }
        if s.tree == Tree::Orig && (v.lt(lo) || v.gt(hi)) {
            return V::No(format!("{} on [{a:e}, {b:e}]", show(&v)));
        }
        unknown(format!("{} on [{a:e}, {b:e}]", show(&v)))
    })
}

// ------------------------------------------------------------ factors

/// Factors whose zeros include e's (where e is defined): the operands of
/// products, a quotient's numerator, a power's base, the argument of a
/// function that vanishes only where it does; none for a function that
/// never vanishes; e itself otherwise.
pub fn zero_factors(e: &Expr) -> Vec<Expr> {
    let mut out = Vec::new();
    fn go(e: &Expr, out: &mut Vec<Expr>) {
        match e {
            Expr::Num(v, _) if *v != 0.0 => {}
            Expr::Const(_) => {}
            Expr::Neg(a) | Expr::Degrees(a) => go(a, out),
            Expr::Bin(BinOp::Mul, a, b) => {
                go(a, out);
                go(b, out);
            }
            Expr::Bin(BinOp::Div | BinOp::Pow, a, _) => go(a, out),
            Expr::Call(f, args) => {
                let a = &args[0];
                let minus_one =
                    || Expr::Bin(BinOp::Sub, Box::new(a.clone()), Box::new(Expr::exact(1.0)));
                match f {
                    Func::Sqrt
                    | Func::Cbrt
                    | Func::Root
                    | Func::Abs
                    | Func::Sign
                    | Func::Atan
                    | Func::Asin
                    | Func::Sinh
                    | Func::Tanh
                    | Func::Asinh
                    | Func::Atanh => go(a, out),
                    Func::Exp
                    | Func::Cosh
                    | Func::Sech
                    | Func::Sec
                    | Func::Csc
                    | Func::Csch
                    | Func::Acot
                    | Func::Acsc
                    | Func::Acsch
                    | Func::Acoth => {}
                    Func::Tan => out.push(Expr::Call(Func::Sin, vec![a.clone()])),
                    Func::Cot => out.push(Expr::Call(Func::Cos, vec![a.clone()])),
                    Func::Ln | Func::Log | Func::Acos | Func::Acosh | Func::Asec | Func::Asech => {
                        out.push(minus_one())
                    }
                    Func::LogBase => out.push(Expr::Bin(
                        BinOp::Sub,
                        Box::new(args[1].clone()),
                        Box::new(Expr::exact(1.0)),
                    )),
                    _ => out.push(e.clone()),
                }
            }
            // a + b with a, b ≥ 0: 0 only where both are, so where
            // either's factors are (the fewer).
            Expr::Bin(BinOp::Add, a, b) if nonneg(a) && nonneg(b) => {
                let (mut fa, mut fb) = (Vec::new(), Vec::new());
                go(a, &mut fa);
                go(b, &mut fb);
                out.extend(if fb.len() < fa.len() { fb } else { fa });
            }
            // a·c ± b·c = c·(a ± b): the shared factors (as multisets),
            // then what is left.
            Expr::Bin(op @ (BinOp::Add | BinOp::Sub), a, b) => {
                let (fa, fb) = (product(a), product(b));
                let mut pool = fb.clone();
                let mut common = Vec::new();
                let mut rest_a = Vec::new();
                for f in fa {
                    match pool.iter().position(|g| *g == f) {
                        Some(i) if contains_x(&f) => common.push(pool.remove(i)),
                        _ => rest_a.push(f),
                    }
                }
                if common.is_empty() {
                    out.push(e.clone());
                    return;
                }
                for c in &common {
                    go(c, out);
                }
                let join = |fs: Vec<Expr>| {
                    fs.into_iter()
                        .reduce(|x, y| Expr::Bin(BinOp::Mul, Box::new(x), Box::new(y)))
                        .unwrap_or(Expr::exact(1.0))
                };
                out.push(Expr::Bin(*op, Box::new(join(rest_a)), Box::new(join(pool))));
            }
            _ => out.push(e.clone()),
        }
    }
    /// e ≥ 0 wherever it is defined: an even power, |u|, √u, eᵘ, cosh u,
    /// a positive number, or a sum of these.
    fn nonneg(e: &Expr) -> bool {
        match e {
            Expr::Num(v, _) => *v >= 0.0,
            Expr::Const(_) => true,
            Expr::Bin(BinOp::Pow, _, p) => {
                written_rational(p, &Lits::default()).is_some_and(|(n, d)| d == 1 && n.is_even())
            }
            Expr::Bin(BinOp::Add, a, b) => nonneg(a) && nonneg(b),
            Expr::Call(Func::Abs | Func::Sqrt | Func::Exp | Func::Cosh, _) => true,
            _ => false,
        }
    }
    /// The factors of a product (a negation's operand counts as itself:
    /// −u's zeros are u's).
    fn product(e: &Expr) -> Vec<Expr> {
        match e {
            Expr::Bin(BinOp::Mul, a, b) => {
                let mut v = product(a);
                v.extend(product(b));
                v
            }
            Expr::Neg(a) => {
                let mut v = product(a);
                v.push(Expr::exact(-1.0));
                v
            }
            _ => vec![e.clone()],
        }
    }
    go(e, &mut out);
    out
}

/// Expressions that are nonzero wherever `e` is defined: divisors, bases
/// of negative powers, logarithm arguments, the cosine (sine) under tan
/// and sec (cot and csc), the argument of coth and csch.
pub fn nonzero_where_defined(e: &Expr, out: &mut Vec<Expr>) {
    match e {
        Expr::Num(..) | Expr::Const(_) | Expr::X | Expr::Y | Expr::Var(_) => {}
        Expr::Neg(a) | Expr::Degrees(a) => nonzero_where_defined(a, out),
        Expr::Bin(op, a, b) => {
            if *op == BinOp::Div {
                out.push((**b).clone());
            }
            if *op == BinOp::Pow
                && written_rational(b, &Lits::default()).is_some_and(|(p, _)| p < 0)
            {
                out.push((**a).clone());
            }
            nonzero_where_defined(a, out);
            nonzero_where_defined(b, out);
        }
        Expr::Call(f, args) => {
            let a = &args[0];
            match f {
                Func::Ln | Func::Log | Func::Coth | Func::Csch => out.push(a.clone()),
                Func::LogBase => out.push(args[1].clone()),
                Func::Tan | Func::Sec => out.push(Expr::Call(Func::Cos, vec![a.clone()])),
                Func::Cot | Func::Csc => out.push(Expr::Call(Func::Sin, vec![a.clone()])),
                _ => {}
            }
            for a in args {
                nonzero_where_defined(a, out);
            }
        }
    }
}

/// The subject continuous on the box, nonzero there factor by factor, of
/// the sign `above` at a point; with `touch` = Some(at): a factor may
/// instead be exactly 0 at `at` and strictly monotone on the box.
fn factors(s: &Subj, x: B, c: f64, above: bool, touch: Option<f64>) -> V {
    if c != 0.0 {
        return unknown("factors only against 0");
    }
    // The tree whose factors are taken: the subject's own (a derivative
    // only as the certifier's tree).
    if s.k > 0 {
        return unknown("factors of a derivative need its tree");
    }
    let mut safe = Vec::new();
    nonzero_where_defined(&s.fx.f, &mut safe);
    nonzero_where_defined(&s.e, &mut safe);
    let fs = zero_factors(&s.e);
    // Continuous (and valid) on the box.
    let cont = on(x, &mut |lo, hi| {
        let (c, valid) = s.coeffs(lo, hi, 0);
        if valid && c[0].cont {
            V::Yes
        } else {
            unknown("not shown continuous")
        }
    });
    if !cont.yes() {
        return cont;
    }
    for h in fs {
        if safe.contains(&h) {
            continue;
        }
        let away = on(x, &mut |lo, hi| {
            let v = s.fx.series(&h, lo, hi, 0);
            if v[0].ne0() {
                V::Yes
            } else {
                unknown(format!("a factor {} on [{lo:e}, {hi:e}]", show(&v[0])))
            }
        });
        if away.yes() {
            continue;
        }
        let Some(at) = touch else { return away };
        let g = Subj {
            fx: s.fx,
            of: Subject::G(Vec::new(), Via::Itself),
            e: h.clone(),
            k: 0,
            tree: s.tree,
        };
        if g.exactly(at, 0.0) != Some(true) || !monotone(&g, x).yes() {
            return away;
        }
    }
    // The sign, at a point of the box other than `at`.
    let p = match touch {
        Some(at) if at == x.0 => {
            if x.1 > x.0 {
                split(x.0, x.1).unwrap_or(x.1)
            } else {
                return unknown("a point box");
            }
        }
        Some(_) => split(x.0, x.1).unwrap_or(x.0),
        None => split(x.0, x.1).unwrap_or(x.0),
    };
    let p = if p.is_finite() {
        p
    } else {
        split(x.0.max(-1e300), x.1.min(1e300)).unwrap_or(0.0)
    };
    end_beyond(s, p, c, above)
}

// ------------------------------------------------------------ touch

/// s − c is exactly 0 at `at` (an end of the box) and strictly on side
/// `above` elsewhere on the box: Taylor's theorem at `at` with Lagrange's
/// remainder (orders tried from the claim's), or the factors.
fn touch(s: &Subj, x: B, c: f64, at: f64, order: usize, above: bool) -> V {
    if at != x.0 && at != x.1 {
        return V::No(format!("{at:e} is not an end of the box"));
    }
    match s.exactly(at, c) {
        Some(true) => {}
        Some(false) if s.tree == Tree::Orig => {
            return V::No(format!("{} at {at:e}", show(&s.at(at).0)));
        }
        _ => return unknown(format!("not shown exact at {at:e}")),
    }
    let mut orders: Vec<usize> = Vec::new();
    if order >= 1 {
        orders.push(order);
    }
    for m in 1..=6 {
        if !orders.contains(&m) {
            orders.push(m);
        }
    }
    let mut last = unknown("no order decides");
    for m in orders {
        last = taylor_touch(s, x, at, m, above);
        if last.yes() {
            return V::Yes;
        }
    }
    let z = factors(s, x, c, above, Some(at));
    if z.yes() { V::Yes } else { last }
}

fn taylor_touch(s: &Subj, x: B, at: f64, m: usize, above: bool) -> V {
    let (pc, valid) = s.coeffs(at, at, m);
    if !valid {
        return unknown("not valid at the point");
    }
    for (j, cj) in pc.iter().enumerate().take(m).skip(1) {
        if !cj.is_exactly(0.0) {
            return unknown(format!("coefficient {j} {} at {at:e}", show(cj)));
        }
    }
    // The order-m coefficient exists and is exactly 0 at `at`: however the
    // box below is split, one piece holds `at`, and an enclosure over it
    // holds that 0, so isn't of one sign. Splitting can't decide; it only
    // halves toward `at` to the last double, a thousand boxes and more
    // for each order (x^2000000 at 0: every order to 6 is 0 there, and
    // each box's series takes twenty-odd squarings).
    let pm = &pc[m];
    if pm.is_exactly(0.0) && pm.def {
        return unknown(format!("coefficient {m} 0 at {at:e}"));
    }
    // The order-m coefficient over the box, of one sign τ.
    let left_end = at == x.0;
    let want = |tau: bool| -> bool {
        if left_end || m.is_multiple_of(2) {
            tau == above
        } else {
            tau != above
        }
    };
    for tau in [true, false] {
        if !want(tau) {
            continue;
        }
        let v = on(x, &mut |lo, hi| {
            let (c, valid) = s.coeffs(lo, hi, m);
            if !valid || c[m].empty || !c[m].def {
                return unknown(format!("order {m} not valid on [{lo:e}, {hi:e}]"));
            }
            if c[m].beyond(0.0, tau) {
                V::Yes
            } else {
                unknown(format!(
                    "order {m} coefficient {} on [{lo:e}, {hi:e}]",
                    show(&c[m])
                ))
            }
        });
        return v;
    }
    unknown("no sign")
}

// ------------------------------------------------------------ tails

/// On the tail s⁽ᵒʳᵈᵉʳ⁾ has one sign σ, and at `from` s − c and its
/// derivatives below `order` have the signs σ forces; so s stays on side
/// `above` of c.
fn tail_chain(s: &Subj, side: Side, from: f64, c: f64, above: bool, order: usize) -> V {
    if order == 0 {
        return V::No("a chain of order 0".into());
    }
    let x = tail_box(side, from);
    let sigma_ok = |sigma: bool| -> bool {
        // The sign s − c ends up with.
        let s0 = match side {
            Side::Right => sigma,
            Side::Left => sigma == order.is_multiple_of(2),
        };
        s0 == above
    };
    for sigma in [true, false] {
        if !sigma_ok(sigma) {
            continue;
        }
        let top = on(x, &mut |lo, hi| {
            let (cs, valid) = s.coeffs(lo, hi, order);
            let d = &cs[order];
            if !valid || d.empty || !d.def {
                return unknown(format!("order {order} not valid on [{lo:e}, {hi:e}]"));
            }
            if d.beyond(0.0, sigma) {
                V::Yes
            } else {
                unknown(format!("order {order} {} on [{lo:e}, {hi:e}]", show(d)))
            }
        });
        if !top.yes() {
            return top;
        }
        let (pc, valid) = s.coeffs(from, from, order);
        if !valid {
            return unknown(format!("not valid at {from:e}"));
        }
        for (i, ci) in pc.iter().enumerate().take(order) {
            let v = if i == 0 {
                iv::sub(ci, &Iv::of(c))
            } else {
                ci.clone()
            };
            let want = match side {
                Side::Right => sigma,
                Side::Left => sigma == ((order - i).is_multiple_of(2)),
            };
            if !v.beyond(0.0, want) {
                return unknown(format!("order {i} {} at {from:e}", show(&v)));
            }
        }
        return V::Yes;
    }
    unknown("no sign")
}

// ------------------------------------------------------------ families

/// u = a·x + b with a and b constant (a ≠ 0): (a, b) exactly where
/// rational arithmetic reaches, else enclosed.
fn affine(e: &Expr, fx: &Fx) -> Option<(Iv, Iv)> {
    let ctx = fx.ctx();
    let constant = |e: &Expr| -> Option<Iv> {
        if contains_x(e) {
            return None;
        }
        let v = super::eval::eval(e, &se::constant(Iv::of(0.0), 0), 0, &ctx)[0].clone();
        (!v.empty && v.def && v.bounded()).then_some(v)
    };
    match e {
        Expr::X => Some((Iv::of(1.0), Iv::of(0.0))),
        Expr::Degrees(a) => affine(a, fx),
        Expr::Neg(a) => {
            let (p, q) = affine(a, fx)?;
            Some((iv::neg(&p), iv::neg(&q)))
        }
        Expr::Bin(op @ (BinOp::Add | BinOp::Sub), a, b) => {
            let la = affine(a, fx).or_else(|| Some((Iv::of(0.0), constant(a)?)))?;
            let lb = affine(b, fx).or_else(|| Some((Iv::of(0.0), constant(b)?)))?;
            Some(if *op == BinOp::Add {
                (iv::add(&la.0, &lb.0), iv::add(&la.1, &lb.1))
            } else {
                (iv::sub(&la.0, &lb.0), iv::sub(&la.1, &lb.1))
            })
        }
        Expr::Bin(BinOp::Mul, a, b) => {
            if let Some(k) = constant(a) {
                let (p, q) = affine(b, fx)?;
                Some((iv::mul(&k, &p), iv::mul(&k, &q)))
            } else {
                let k = constant(b)?;
                let (p, q) = affine(a, fx)?;
                Some((iv::mul(&p, &k), iv::mul(&q, &k)))
            }
        }
        Expr::Bin(BinOp::Div, a, b) => {
            let k = constant(b)?;
            let (p, q) = affine(a, fx)?;
            Some((iv::div(&p, &k), iv::div(&q, &k)))
        }
        _ => None,
    }
}

/// The same, exactly (degrees and grads; no π).
fn affine_exact(e: &Expr, fx: &Fx) -> Option<(rug::Rational, rug::Rational)> {
    use rug::Rational;
    let constant = |e: &Expr| -> Option<Rational> {
        if contains_x(e) {
            return None;
        }
        exact::eval(e, None, &fx.lits, &fx.vars, fx.unit)
    };
    match e {
        Expr::X => Some((Rational::from(1), Rational::from(0))),
        Expr::Degrees(a) => affine_exact(a, fx),
        Expr::Neg(a) => {
            let (p, q) = affine_exact(a, fx)?;
            Some((-p, -q))
        }
        Expr::Bin(op @ (BinOp::Add | BinOp::Sub), a, b) => {
            let la = affine_exact(a, fx).or_else(|| Some((Rational::from(0), constant(a)?)))?;
            let lb = affine_exact(b, fx).or_else(|| Some((Rational::from(0), constant(b)?)))?;
            Some(if *op == BinOp::Add {
                (la.0 + lb.0, la.1 + lb.1)
            } else {
                (la.0 - lb.0, la.1 - lb.1)
            })
        }
        Expr::Bin(BinOp::Mul, a, b) => {
            if let Some(k) = constant(a) {
                let (p, q) = affine_exact(b, fx)?;
                Some((k.clone() * p, k * q))
            } else {
                let k = constant(b)?;
                let (p, q) = affine_exact(a, fx)?;
                Some((p * k.clone(), q * k))
            }
        }
        Expr::Bin(BinOp::Div, a, b) => {
            let k = constant(b)?;
            if k == 0 {
                return None;
            }
            let (p, q) = affine_exact(a, fx)?;
            Some((p / k.clone(), q / k))
        }
        _ => None,
    }
}

/// The zeros of g = s(u) for the table's s, in quarter turns: (first
/// zero, spacing), as u = θ₀ + k·T.
fn zero_table<'e>(g: &'e Expr, lits: &Lits) -> Option<(&'e Expr, i64, i64)> {
    // Exactly 1 as typed (a typed 1.0000000000000001 is held as the double
    // 1 but is not 1).
    let one = |e: &Expr| matches!(e, Expr::Num(v, lit) if *v == 1.0 && lits.exact(*v, lit) == Some(rug::Rational::from(1)));
    fn trig(e: &Expr) -> Option<(Func, &Expr)> {
        match e {
            Expr::Call(f @ (Func::Sin | Func::Cos | Func::Tan | Func::Cot), args) => {
                Some((*f, &args[0]))
            }
            _ => None,
        }
    }
    // (θ₀, T) in quarter turns.
    if let Some((f, u)) = trig(g) {
        return Some(match f {
            Func::Sin | Func::Tan => (u, 0, 2),
            _ => (u, 1, 2),
        });
    }
    let (op, a, b) = match g {
        Expr::Bin(op @ (BinOp::Add | BinOp::Sub), a, b) => (*op, &**a, &**b),
        _ => return None,
    };
    // 1 + s, s + 1, 1 − s, s − 1.
    let (s, plus) = if one(a) {
        (b, op == BinOp::Add)
    } else if one(b) {
        (a, op == BinOp::Add)
    } else {
        return None;
    };
    let (f, u) = trig(s)?;
    Some(match (f, plus) {
        // sin u = −1: u = −1 quarter + 4k; sin u = 1: 1 + 4k.
        (Func::Sin, true) => (u, -1, 4),
        (Func::Sin, false) => (u, 1, 4),
        // cos u = −1: 2 + 4k; cos u = 1: 0 + 4k.
        (Func::Cos, true) => (u, 2, 4),
        (Func::Cos, false) => (u, 0, 4),
        _ => return None,
    })
}

fn family(fx: &Fx, of: &Subject, x0: B, period: B) -> Outcome {
    let Subject::G(path, via) = of else {
        return Outcome::new(Class::Refuted, "a family of f itself");
    };
    let Some(g) = side_tree(&fx.f, path, *via) else {
        return Outcome::new(Class::Unsupported, "no such side expression");
    };
    let Some((u, theta, step)) = zero_table(&g, &fx.lits) else {
        // No table form: a member where g is not 0 refutes the family
        // (1.0000000000000001 − cos x is 10⁻¹⁶ at 0).
        let v = fx.series(&g, x0.0, x0.1, 0)[0].clone();
        if !v.empty && v.def && (v.gt(0.0) || v.lt(0.0)) {
            return Outcome::new(
                Class::Refuted,
                format!("g is {} at the family's member, not 0", show(&v)),
            );
        }
        return Outcome::new(
            Class::Unconfirmed,
            format!("not a table form: {}", g.formula()),
        );
    };
    // A quarter turn in the unit.
    let quarter_exact: Option<i64> = match fx.unit {
        Unit::Radians => None,
        Unit::Degrees => Some(90),
        Unit::Grads => Some(100),
    };
    let inside = |v: &Iv, b: B| v.ge(b.0) && v.le(b.1);
    // Exactly, where there's no π.
    if let (Some(q), Some((a, b))) = (quarter_exact, affine_exact(u, fx))
        && a != 0
    {
        use rug::Rational;
        let t = Rational::from(step * q);
        let per = (t.clone() / a.clone()).abs();
        let ok_per = per >= Rational::from_f64(period.0).unwrap_or_default()
            && per <= Rational::from_f64(period.1).unwrap_or_default();
        // x_k = (θ₀ q + k T − b)/a: the k nearest x0's middle.
        let th = Rational::from(theta * q);
        let mid = Rational::from_f64(x0.0 / 2.0 + x0.1 / 2.0).unwrap_or_default();
        let kf = ((mid * a.clone() + b.clone() - th.clone()) / t.clone())
            .to_f64()
            .round();
        let ok_x0 = (-2..=2).any(|dk| {
            let k = Rational::from_f64(kf + dk as f64).unwrap_or_default();
            let xk = (th.clone() + k * t.clone() - b.clone()) / a.clone();
            xk >= Rational::from_f64(x0.0).unwrap_or_default()
                && xk <= Rational::from_f64(x0.1).unwrap_or_default()
        });
        return if ok_per && ok_x0 {
            Outcome::new(Class::Strong, "")
        } else {
            Outcome::new(
                Class::Refuted,
                format!("exact period {per} or first zero not in the boxes"),
            )
        };
    }
    let v = ladder(|| {
        let Some((a, b)) = affine(u, fx) else {
            return unknown(format!("argument not affine: {}", u.formula()));
        };
        if !a.ne0() {
            return unknown("slope not shown nonzero");
        }
        let quarter = match quarter_exact {
            Some(q) => Iv::of(q as f64),
            None => iv::div(&iv::pi(), &Iv::of(2.0)),
        };
        let t = iv::scale(&quarter, step as f64);
        let per = iv::abs(&iv::div(&t, &a));
        let th = iv::scale(&quarter, theta as f64);
        let mid = Iv::of(x0.0 / 2.0 + x0.1 / 2.0);
        let kq = iv::div(&iv::sub(&iv::add(&iv::mul(&mid, &a), &b), &th), &t);
        let kf = kq.mid().to_f64().round();
        let found = (-2..=2).any(|dk| {
            let k = Iv::of(kf + dk as f64);
            let xk = iv::div(&iv::sub(&iv::add(&th, &iv::mul(&k, &t)), &b), &a);
            inside(&xk, x0)
        });
        if inside(&per, period) && found {
            V::Yes
        } else if (per.lt(period.0) || per.gt(period.1)) && per.def {
            V::No(format!("period {} outside the box", show(&per)))
        } else {
            unknown(format!(
                "period {} or first zero not inside the boxes",
                show(&per)
            ))
        }
    });
    match v {
        V::Yes => Outcome::new(Class::Strong, ""),
        V::No(w) => Outcome::new(Class::Refuted, w),
        V::Unknown(w) => Outcome::new(Class::Unconfirmed, w),
    }
}

// ------------------------------------------------------------ poles

struct Near<'a> {
    fx: &'a Fx,
    near: B,
    at: B,
}

impl Near<'_> {
    fn over(&self, e: &Expr) -> Iv {
        self.fx.series(e, self.near.0, self.near.1, 0)[0].clone()
    }

    fn bounded(&self, e: &Expr) -> bool {
        let v = self.over(e);
        !v.empty && v.bounded()
    }

    fn away0(&self, e: &Expr) -> bool {
        let v = self.over(e);
        v.bounded() && v.ne0()
    }

    /// e → 0 at a point p of `at` (the only one there), from within near.
    fn vanishes(&self, e: &Expr) -> bool {
        let s = Subj {
            fx: self.fx,
            of: Subject::G(Vec::new(), Via::Itself),
            e: e.clone(),
            k: 0,
            tree: Tree::Orig,
        };
        let (a, b) = self.at;
        if a == b {
            // e(p) = 0 exactly, e continuous on near.
            let cont = on(self.near, &mut |lo, hi| {
                let v = self.fx.series(e, lo, hi, 0);
                if !v[0].empty && v[0].cont {
                    V::Yes
                } else {
                    unknown("")
                }
            });
            if s.exactly(a, 0.0) == Some(true) && cont.yes() {
                return true;
            }
        } else {
            // Strictly monotone on near, changing sign across `at`.
            let (va, _) = s.at(a);
            let (vb, _) = s.at(b);
            let opposite = (va.gt(0.0) && vb.lt(0.0))
                || (va.lt(0.0) && vb.gt(0.0))
                || va.is_exactly(0.0)
                || vb.is_exactly(0.0);
            if opposite && monotone(&s, self.near).yes() {
                return true;
            }
        }
        match e {
            Expr::Neg(a) | Expr::Degrees(a) => self.vanishes(a),
            Expr::Bin(BinOp::Pow, a, k) => {
                written_rational(k, &self.fx.lits).is_some_and(|(p, _)| p > 0) && self.vanishes(a)
            }
            Expr::Bin(BinOp::Mul, a, b) => {
                (self.vanishes(a) && self.bounded(b)) || (self.vanishes(b) && self.bounded(a))
            }
            Expr::Bin(BinOp::Div, a, b) => self.vanishes(a) && self.away0(b),
            Expr::Call(Func::Abs | Func::Sqrt | Func::Cbrt | Func::Sinh | Func::Tanh, args) => {
                self.vanishes(&args[0])
            }
            _ => false,
        }
    }

    /// |e| → ∞ at a point of `at`, from within near.
    fn blows(&self, e: &Expr) -> bool {
        let call = |f: Func, a: &Expr| Expr::Call(f, vec![a.clone()]);
        let one = Expr::exact(1.0);
        match e {
            Expr::Neg(a) | Expr::Degrees(a) => self.blows(a),
            Expr::Bin(BinOp::Add | BinOp::Sub, a, b) => {
                (self.blows(a) && self.bounded(b)) || (self.blows(b) && self.bounded(a))
            }
            Expr::Bin(BinOp::Mul, a, b) => {
                (self.blows(a) && self.away0(b)) || (self.blows(b) && self.away0(a))
            }
            Expr::Bin(BinOp::Div, a, b) => {
                (self.vanishes(b) && self.away0(a)) || (self.blows(a) && self.bounded(b))
            }
            Expr::Bin(BinOp::Pow, a, k) => match written_rational(k, &self.fx.lits) {
                Some((p, _)) if p < 0 => self.vanishes(a),
                Some((p, _)) if p > 0 => self.blows(a),
                _ => false,
            },
            Expr::Call(f, args) => {
                let a = &args[0];
                match f {
                    Func::Tan | Func::Sec => self.vanishes(&call(Func::Cos, a)),
                    Func::Cot | Func::Csc => self.vanishes(&call(Func::Sin, a)),
                    Func::Coth | Func::Csch => self.vanishes(a),
                    Func::Ln | Func::Log => self.vanishes(a),
                    Func::LogBase => {
                        !contains_x(a) && self.away0(&call(Func::Ln, a)) && self.vanishes(&args[1])
                    }
                    Func::Sqrt | Func::Cbrt | Func::Abs | Func::Sinh | Func::Cosh => self.blows(a),
                    Func::Root => !contains_x(&args[1]) && self.blows(a),
                    Func::Atanh | Func::Acoth => {
                        self.vanishes(&Expr::Bin(
                            BinOp::Sub,
                            Box::new(one.clone()),
                            Box::new(a.clone()),
                        )) || self.vanishes(&Expr::Bin(
                            BinOp::Add,
                            Box::new(one.clone()),
                            Box::new(a.clone()),
                        ))
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

/// Neighbourhoods of `at` inside `near`, from `near` itself down.
fn nears(near: B, at: B) -> Vec<B> {
    let mut out = vec![near];
    let s = at.0.abs().max(at.1.abs()).max(1.0);
    for w in [1e-3, 1e-6, 1e-9, 1e-12] {
        let d = w * s;
        let lo = if near.0 == at.0 {
            at.0
        } else {
            (at.0 - d).max(near.0)
        };
        let hi = if near.1 == at.1 {
            at.1
        } else {
            (at.1 + d).min(near.1)
        };
        if (lo, hi) != *out.last().unwrap() && lo <= at.0 && at.1 <= hi {
            out.push((lo, hi));
        }
    }
    out
}

fn unbounded(fx: &Fx, near: B, at: B) -> Outcome {
    if !(near.0 <= at.0 && at.1 <= near.1) {
        return Outcome::new(
            Class::Refuted,
            "the excluded point's box is not inside near",
        );
    }
    if unmodelled(&fx.f) {
        return Outcome::new(
            Class::Unsupported,
            "the tree uses a function the replay doesn't model",
        );
    }
    let tries = |e: &Expr| -> bool {
        for p in PRECS {
            iv::set_prec(p);
            for n in nears(near, at) {
                let nb = Near { fx, near: n, at };
                if nb.blows(e) {
                    iv::set_prec(PRECS[0]);
                    return true;
                }
            }
        }
        iv::set_prec(PRECS[0]);
        false
    };
    if tries(&fx.f) {
        return Outcome::new(Class::Strong, "");
    }
    if let Some(g) = &fx.f_eval
        && tries(g)
    {
        return Outcome::new(Class::Weak, "on the certifier's tree");
    }
    Outcome::new(Class::Unconfirmed, "no blow-up shown")
}

/// f's limit at the double p: the value of f's continuous extension,
/// from series at p with 0/0 cancelled; f′ of the extension must exist
/// there (so it's continuous at p).
pub fn limit_at(fx: &Fx, p: f64) -> Option<Iv> {
    let s = se::with_shift(|| fx.series(&fx.f, p, p, 8));
    (!s[0].empty && s[0].def && s[0].bounded() && !s[1].empty && s[1].def).then(|| s[0].clone())
}

/// A limit by the tree's structure: derived again by the replay's own
/// reading of the rules (`growth`), a proof when it agrees.
fn limit(fx: &Fx, at: growth::At, over_x: bool, to: super::Toward) -> Outcome {
    use super::Toward;
    use growth::Goes;
    if unmodelled(&fx.f) {
        return Outcome::new(
            Class::Unsupported,
            "the tree uses a function the replay doesn't model",
        );
    }
    let Some(got) = growth::limit(fx, at, over_x) else {
        return Outcome::new(
            Class::Unconfirmed,
            "the tree's structure doesn't decide the limit",
        );
    };
    match (got, to) {
        (Goes::PosInf, Toward::PosInf) | (Goes::NegInf, Toward::NegInf) => {
            Outcome::new(Class::Strong, "by the tree's structure")
        }
        (Goes::To(v), Toward::In(lo, hi)) => {
            if v.ge(lo) && v.le(hi) {
                Outcome::new(Class::Strong, "by the tree's structure")
            } else if v.hi < lo || v.lo > hi {
                Outcome::new(
                    Class::Refuted,
                    format!("the limit is {}, outside [{lo:e}, {hi:e}]", show(&v)),
                )
            } else {
                Outcome::new(
                    Class::Unconfirmed,
                    format!("the limit {} is not inside [{lo:e}, {hi:e}]", show(&v)),
                )
            }
        }
        (got, to) => Outcome::new(
            Class::Refuted,
            format!("the limit is {got:?} by the tree's structure, not {to:?}"),
        ),
    }
}

/// A line on a tail, f − (m·x + b) in [lo, hi] there: f expanded again
/// in 1/x by the replay's own rules (`expand`), a proof when its slope,
/// intercept and rest lie in the claimed enclosures.
fn tail_line(fx: &Fx, side: Side, from: f64, m: B, b: B, lo: f64, hi: f64) -> Outcome {
    if unmodelled(&fx.f) {
        return Outcome::new(
            Class::Unsupported,
            "the tree uses a function the replay doesn't model",
        );
    }
    if (from > 0.0) != (side == Side::Right) || from == 0.0 {
        return Outcome::new(Class::Refuted, "the tail starts on the other side of 0");
    }
    let Some((ms, bs, band)) = super::expand::line(fx, from) else {
        return Outcome::new(
            Class::Unconfirmed,
            "f doesn't expand to a line in 1/x on the tail",
        );
    };
    let within = |v: &Iv, e: B| v.ge(e.0) && v.le(e.1);
    let apart = |v: &Iv, e: B| v.hi < e.0 || v.lo > e.1;
    let parts = [
        (&ms, m, "slope"),
        (&bs, b, "intercept"),
        (&band, (lo, hi), "rest"),
    ];
    for (v, e, what) in parts {
        if apart(v, e) {
            return Outcome::new(
                Class::Refuted,
                format!("the {what} is {}, outside [{:e}, {:e}]", show(v), e.0, e.1),
            );
        }
    }
    for (v, e, what) in parts {
        if !within(v, e) {
            return Outcome::new(
                Class::Unconfirmed,
                format!(
                    "the {what} {} is not inside [{:e}, {:e}]",
                    show(v),
                    e.0,
                    e.1
                ),
            );
        }
    }
    Outcome::new(Class::Strong, "by f's expansion in 1/x")
}

fn removable(fx: &Fx, near: B, at: B, lo: f64, hi: f64) -> Outcome {
    if !(near.0 <= at.0 && at.1 <= near.1) {
        return Outcome::new(
            Class::Refuted,
            "the excluded point's box is not inside near",
        );
    }
    if at.0 == at.1 && !unmodelled(&fx.f) {
        for p in PRECS {
            iv::set_prec(p);
            if let Some(v) = limit_at(fx, at.0) {
                if v.ge(lo) && v.le(hi) {
                    iv::set_prec(PRECS[0]);
                    return Outcome::new(Class::Strong, "the limit from series at the point");
                }
                if (v.lt(lo) || v.gt(hi)) && p == PRECS[PRECS.len() - 1] {
                    iv::set_prec(PRECS[0]);
                    return Outcome::new(Class::Refuted, format!("the limit is in {}", show(&v)));
                }
            }
        }
        iv::set_prec(PRECS[0]);
    }
    // The certifier's tree: defined and continuous on near, its values on
    // `at` in [lo, hi].
    let Some(g) = &fx.f_eval else {
        return Outcome::new(Class::Unconfirmed, "no evaluated tree");
    };
    let v = ladder(|| {
        let cont = on(near, &mut |a, b| {
            let v = fx.series(g, a, b, 0);
            if !v[0].empty && v[0].cont {
                V::Yes
            } else {
                unknown(format!(
                    "the evaluated tree {} on [{a:e}, {b:e}]",
                    show(&v[0])
                ))
            }
        });
        if !cont.yes() {
            return cont;
        }
        on(at, &mut |a, b| {
            let v = fx.series(g, a, b, 0);
            if v[0].ge(lo) && v[0].le(hi) {
                V::Yes
            } else {
                unknown(format!(
                    "the evaluated tree {} on [{a:e}, {b:e}]",
                    show(&v[0])
                ))
            }
        })
    });
    match v {
        V::Yes => Outcome::new(Class::Weak, "the simplifier's form fills the hole"),
        V::No(w) | V::Unknown(w) => Outcome::new(Class::Unconfirmed, w),
    }
}

// ------------------------------------------------------------ simplifier facts

/// A number written `q`, `p/q`, `q·π`, `q·π^k` (the certifier's `pi_q`),
/// or `+∞`/`−∞` (as None with the sign).
/// The parts (p, d, k) of a number written `p/d·π^k` (`pi_q`'s notation).
pub fn piq_parts(text: &str) -> Option<(i64, i64, i32)> {
    let (q, k) = match text.split_once('·') {
        Some((q, rest)) => {
            let k = if rest == "π" {
                1
            } else {
                rest.strip_prefix("π^")?.parse::<i32>().ok()?
            };
            (q, k)
        }
        None => (text, 0),
    };
    let (n, d) = match q.split_once('/') {
        Some((n, d)) => (n.parse().ok()?, d.parse().ok()?),
        None => (q.parse().ok()?, 1),
    };
    Some((n, d, k))
}

pub fn pi_q(text: &str) -> Option<Iv> {
    let (q, k) = match text.split_once('·') {
        Some((q, rest)) => {
            let k = if rest == "π" {
                1
            } else {
                rest.strip_prefix("π^")?.parse::<i32>().ok()?
            };
            (q, k)
        }
        None => (text, 0),
    };
    let q = match q.split_once('/') {
        Some((n, d)) => iv::div(
            &Iv::of(n.parse::<f64>().ok()?),
            &Iv::of(d.parse::<f64>().ok()?),
        ),
        None => Iv::of(q.parse::<f64>().ok()?),
    };
    let mut v = q;
    for _ in 0..k.unsigned_abs() {
        v = if k > 0 {
            iv::mul(&v, &iv::pi())
        } else {
            iv::div(&v, &iv::pi())
        };
    }
    Some(v)
}

fn bracket(text: &str) -> Option<Iv> {
    let t = text.trim().strip_prefix('[')?.strip_suffix(']')?;
    let (a, b) = t.split_once(", ")?;
    Some(Iv::of2(a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// f over the doubles enclosing `x` (rounded outward).
fn f_at(fx: &Fx, x: &Iv) -> Iv {
    let (lo, hi) = outward(x);
    fx.series(&fx.f, lo, hi, 0)[0].clone()
}

pub fn outward(x: &Iv) -> (f64, f64) {
    (
        x.lo.to_f64_round(rug::float::Round::Down),
        x.hi.to_f64_round(rug::float::Round::Up),
    )
}

/// Structural parity, from the language's rules: x odd, constants even,
/// sums keep a shared parity, products multiply, an integer (odd-root)
/// power of an odd base goes with its exponent, odd/even functions.
pub fn tree_parity(e: &Expr, lits: &Lits) -> Option<bool> {
    if !contains_x(e) {
        return Some(true);
    }
    Some(match e {
        Expr::X => false,
        Expr::Neg(a) | Expr::Degrees(a) => tree_parity(a, lits)?,
        Expr::Bin(BinOp::Add | BinOp::Sub, a, b) => {
            let (pa, pb) = (tree_parity(a, lits)?, tree_parity(b, lits)?);
            (pa == pb).then_some(pa)?
        }
        Expr::Bin(BinOp::Mul | BinOp::Div, a, b) => tree_parity(a, lits)? == tree_parity(b, lits)?,
        Expr::Bin(BinOp::Pow, a, b) => {
            let pa = tree_parity(a, lits)?;
            if contains_x(b) {
                (pa && tree_parity(b, lits)?).then_some(true)?
            } else if pa {
                true
            } else {
                match written_rational(b, lits)? {
                    (p, q) if q.is_odd() => p.is_even(),
                    _ => return None,
                }
            }
        }
        Expr::Call(Func::Root, args) if args.len() == 2 && !contains_x(&args[1]) => {
            let pa = tree_parity(&args[0], lits)?;
            match written_rational(&args[1], lits)? {
                (n, q) if q == 1 && (pa || n.is_odd()) => pa,
                _ => return None,
            }
        }
        Expr::Call(f, args) if args.len() == 1 => {
            if tree_parity(&args[0], lits)? {
                true
            } else {
                match f {
                    Func::Sin
                    | Func::Tan
                    | Func::Cot
                    | Func::Csc
                    | Func::Sinh
                    | Func::Tanh
                    | Func::Coth
                    | Func::Csch
                    | Func::Asin
                    | Func::Atan
                    | Func::Acsc
                    | Func::Asinh
                    | Func::Atanh
                    | Func::Acsch
                    | Func::Acoth
                    | Func::Cbrt => false,
                    Func::Cos | Func::Sec | Func::Cosh | Func::Sech | Func::Abs => true,
                    _ => return None,
                }
            }
        }
        Expr::Call(_, args) => args
            .iter()
            .all(|a| tree_parity(a, lits) == Some(true))
            .then_some(true)?,
        _ => return None,
    })
}

/// Points a symmetry or a period is tested at: near 0, ordinary, and far
/// out.
const SAMPLES: [f64; 20] = [
    1e-9, 1e-6, 1e-3, 0.1, 0.37, 0.5, 1.0, 1.3, 2.9, 3.7, 7.7, 11.3, 25.1, 113.0, 1e4, 1.37e6,
    2.9e9, 1.13e12, 3.7e15, 7.7e30,
];

fn simplifier(fx: &Fx, fact: &str) -> Outcome {
    if unmodelled(&fx.f) {
        return Outcome::new(
            Class::Unsupported,
            "the tree uses a function the replay doesn't model",
        );
    }
    if let Some(rest) = fact.strip_prefix("f is ")
        && (rest.starts_with("even") || rest.starts_with("odd"))
    {
        let even = rest.starts_with("even");
        // The structural rules are the replay's own: a proof.
        if tree_parity(&fx.f, &fx.lits) == Some(even) {
            return Outcome::new(Class::Strong, "by the tree's structure");
        }
        // f(−x) ≡ ±f(x) exactly (the domain's symmetry: its row).
        let mirror =
            fx.f.map(&|n| matches!(n, Expr::X).then(|| Expr::Neg(Box::new(Expr::X))));
        if super::algebra::same_trees(&fx.f, &mirror, !even, &fx.lits, &fx.vars, fx.unit) {
            return Outcome::new(Class::Strong, "f(−x) ≡ ±f(x) exactly");
        }
        for &x in &SAMPLES {
            let (a, b) = (f_at(fx, &Iv::of(x)), f_at(fx, &Iv::of(-x)));
            if a.empty != b.empty && (a.def || b.def) {
                return Outcome::new(Class::Refuted, format!("defined at only one of ±{x}"));
            }
            if a.empty || !a.def || !b.def {
                continue;
            }
            let other = if even { b } else { iv::neg(&b) };
            if other.hi < a.lo || a.hi < other.lo {
                return Outcome::new(
                    Class::Refuted,
                    format!("f(−{x}) ≠ {}f({x})", if even { "" } else { "−" }),
                );
            }
        }
        return Outcome::new(Class::Weak, "agrees at sample points");
    }
    if let Some(rest) = fact.strip_prefix("f(x + ") {
        let Some((p, _)) = rest.split_once(") = f(x)") else {
            return Outcome::new(Class::Unconfirmed, format!("unread fact: {fact}"));
        };
        // f(x + P) ≡ f(x) exactly, for P written q·πᵏ, or (P only
        // enclosed) a q·πᵏ with a small denominator inside the enclosure:
        // a period there. (The domain's invariance: its row.)
        let mut candidates: Vec<(i64, i64, i32)> = piq_parts(p).into_iter().collect();
        if p == "P"
            && let Some(b) = fact.split_once("P ∈ ").and_then(|(_, b)| bracket(b))
        {
            for k in [1, 0] {
                let v = if k == 1 {
                    iv::div(&b, &iv::pi())
                } else {
                    b.clone()
                };
                let m = v.mid().to_f64();
                for d in 1..=720i64 {
                    let n = (m * d as f64).round() as i64;
                    let q = iv::div(&Iv::of(n as f64), &Iv::of(d as f64));
                    let val = if k == 1 { iv::mul(&q, &iv::pi()) } else { q };
                    if n != 0 && val.ge(b.lo.to_f64()) && val.le(b.hi.to_f64()) {
                        candidates.push((n, d, k));
                        break;
                    }
                }
            }
        }
        for (n, d, k) in candidates {
            let Some(pe) = super::algebra::piq_expr(n, d, k) else {
                continue;
            };
            let shifted = fx.f.map(&|e| {
                matches!(e, Expr::X)
                    .then(|| Expr::Bin(BinOp::Add, Box::new(Expr::X), Box::new(pe.clone())))
            });
            if super::algebra::same_trees(&fx.f, &shifted, false, &fx.lits, &fx.vars, fx.unit) {
                return Outcome::new(
                    Class::Strong,
                    format!("f(x + P) ≡ f(x) exactly, P = {n}/{d}·π^{k}"),
                );
            }
        }
        let per = if p == "P" {
            fact.split_once("P ∈ ").and_then(|(_, b)| bracket(b))
        } else {
            pi_q(p)
        };
        let Some(per) = per else {
            return Outcome::new(Class::Unconfirmed, format!("unread period: {fact}"));
        };
        for &x in SAMPLES.iter().chain(&[-0.7, -2.2, -9.1]) {
            let a = f_at(fx, &Iv::of(x));
            let shifted = iv::add(&Iv::of(x), &per);
            let b = f_at(fx, &shifted);
            if a.empty || !a.def || b.empty {
                continue;
            }
            if b.def && (b.hi < a.lo || a.hi < b.lo) {
                return Outcome::new(Class::Refuted, format!("f({x}) ≠ f({x} + P)"));
            }
        }
        return Outcome::new(Class::Weak, "agrees at sample points");
    }
    // Weak only when f's values at every point tested, far out (or near
    // the point), bear the limit out; refuted when they settle elsewhere.
    let weak = |ok: Approach, what: &str| match ok {
        Approach::Consistent(how) => Outcome::new(
            Class::Weak,
            format!("agrees at points approaching {what}: {how}"),
        ),
        Approach::Inconsistent(why) => Outcome::new(Class::Refuted, format!("{what}: {why}")),
        Approach::Undecided(why) => Outcome::new(Class::Unconfirmed, format!("{what}: {why}")),
    };
    // A rational f: its behaviour at ±∞ exactly, from N/D's degrees and
    // leading coefficients.
    if let Some(o) = rational_fact(fx, fact) {
        return o;
    }
    if let Some(rest) = fact.strip_prefix("f → ") {
        let Some((lim, at)) = rest.split_once(" as x → ") else {
            return Outcome::new(Class::Unconfirmed, format!("unread fact: {fact}"));
        };
        return weak(approaches(fx, at, &|_, v| v, lim), "the limit");
    }
    if let Some(rest) = fact.strip_prefix("f = ")
        && let Some((mb, _)) = rest.split_once(" on its domain (the simplifier's form)")
        && let Some((m, b)) = mb.split_once("·x + ")
    {
        // f is m·x + b wherever defined: tested at points.
        let (Some(m), Some(b)) = (pi_q(m), pi_q(b)) else {
            return Outcome::new(Class::Unconfirmed, format!("unread line: {fact}"));
        };
        for &x in SAMPLES.iter().chain(&[-0.7, -2.2, -9.1, -1e3, -3.7e9]) {
            let v = f_at(fx, &Iv::of(x));
            if v.empty || !v.def {
                continue;
            }
            let line = iv::add(&iv::mul(&m, &Iv::of(x)), &b);
            if v.hi < line.lo || line.hi < v.lo {
                return Outcome::new(Class::Refuted, format!("f({x}) is off the line"));
            }
        }
        return Outcome::new(Class::Weak, "on the line at sample points");
    }
    if let Some(rest) = fact.strip_prefix("f − ")
        && let Some((m, rest)) = rest.split_once("·x = ")
        && let Some((g, rest)) = rest.split_once(" has period ")
        && let Some((p, _)) = rest.split_once(": no oblique asymptote")
    {
        return periodic_rest(fx, m, g, p, fact);
    }
    // Oblique asymptotes (and their absence).
    let over_x = |x: f64, v: Iv| iv::div(&v, &Iv::of(x));
    if fact.starts_with("f is periodic: f − (m·x + b) does not tend to 0") {
        // A periodic f is bounded where defined: f/x → 0 both ways, so no
        // line with m ≠ 0 is approached. Tested far out on both sides.
        for at in ["+∞", "−∞"] {
            let o = weak(approaches(fx, at, &over_x, "0"), "f/x → 0");
            if o.class != Class::Weak {
                return o;
            }
        }
        return Outcome::new(Class::Weak, "f/x → 0 at points far out both ways");
    }
    if let Some(rest) = fact.strip_prefix("f/x → ±∞ as x → ") {
        let at = rest.split_once(':').map_or(rest, |(a, _)| a);
        let abs_over_x = |x: f64, v: Iv| iv::abs(&over_x(x, v));
        return weak(approaches(fx, at, &abs_over_x, "+∞"), "f/x → ±∞");
    }
    if let Some(rest) = fact.strip_prefix("f/x → 0 as x → ") {
        let at = rest.split_once(' ').map_or(rest, |(a, _)| a);
        return weak(approaches(fx, at, &over_x, "0"), "f/x → 0");
    }
    if let Some(rest) = fact.strip_prefix("f/x → ") {
        // f/x → m and f − m·x → b as x → at.
        let parse = || -> Option<(String, String, String)> {
            let (m, rest) = rest.split_once(" and f − m·x → ")?;
            let (b, at) = rest.split_once(" as x → ")?;
            Some((m.to_string(), b.to_string(), at.to_string()))
        };
        let Some((m, b, at)) = parse() else {
            return Outcome::new(Class::Unconfirmed, format!("unread fact: {fact}"));
        };
        if pi_q(&m).is_none() {
            return Outcome::new(Class::Unconfirmed, format!("unread slope: {fact}"));
        }
        let slope = weak(approaches(fx, &at, &over_x, &m), "the slope");
        if slope.class != Class::Weak {
            return slope;
        }
        // (m enclosed at the precision the points are evaluated at.)
        let minus_mx = |x: f64, v: Iv| {
            let mv = pi_q(&m).unwrap_or_else(Iv::entire);
            iv::sub(&v, &iv::mul(&mv, &Iv::of(x)))
        };
        return weak(approaches(fx, &at, &minus_mx, &b), "the intercept");
    }
    if let Some(rest) = fact.strip_prefix("f = N/D exactly and N/D − (") {
        // m·x + b) → 0 as x → ±∞
        let Some((mb, _)) = rest.split_once(") → 0") else {
            return Outcome::new(Class::Unconfirmed, format!("unread fact: {fact}"));
        };
        let Some((m, b)) = mb.split_once("·x + ") else {
            return Outcome::new(Class::Unconfirmed, format!("unread fact: {fact}"));
        };
        if pi_q(m).is_none() || pi_q(b).is_none() {
            return Outcome::new(Class::Unconfirmed, format!("unread line: {fact}"));
        }
        for at in ["+∞", "−∞"] {
            let gap = |x: f64, v: Iv| {
                let (mv, bv) = (
                    pi_q(m).unwrap_or_else(Iv::entire),
                    pi_q(b).unwrap_or_else(Iv::entire),
                );
                iv::sub(&v, &iv::add(&iv::mul(&mv, &Iv::of(x)), &bv))
            };
            let o = weak(approaches(fx, at, &gap, "0"), "the line");
            if o.class != Class::Weak {
                return o;
            }
        }
        return Outcome::new(Class::Weak, "agrees at points approaching ±∞");
    }
    if fact.starts_with("f = N/D exactly with deg N ≠ deg D + 1") {
        // No line: f/x doesn't settle on a nonzero slope far out.
        let mut scale: f64 = 1.0;
        fx.f.visit(&mut |n| {
            if let graphing::ast::Expr::Num(v, _) = n
                && v.is_finite()
            {
                scale = scale.max(v.abs());
            }
        });
        iv::set_prec(2400);
        let r = |x: f64| over_x(x, f_at(fx, &Iv::of(x)));
        let close = |a: &Iv, b: &Iv| {
            a.bounded() && b.bounded() && b.ne0() && {
                let d = iv::abs(&iv::sub(a, b));
                d.hi.to_f64() <= 1e-6 * iv::abs(b).lo.to_f64()
            }
        };
        let mut settles = false;
        for sign in [1.0, -1.0] {
            let v: Vec<Iv> = [1.6e5, 1.6e13, 1.6e25]
                .iter()
                .map(|k| r(sign * k * scale))
                .collect();
            settles |= close(&v[0], &v[1]) && close(&v[1], &v[2]);
        }
        iv::set_prec(PRECS[0]);
        return if settles {
            Outcome::new(Class::Unconfirmed, "f/x settles on a nonzero slope far out")
        } else {
            Outcome::new(Class::Weak, "f/x settles on no nonzero slope far out")
        };
    }
    Outcome::new(Class::Unconfirmed, format!("unread fact: {fact}"))
}

/// f − m·x is G, of period P: each by the field of fractions in x and
/// atoms (strong), or else at sample points (weak).
fn periodic_rest(fx: &Fx, m: &str, g: &str, p: &str, fact: &str) -> Outcome {
    let unread = || Outcome::new(Class::Unconfirmed, format!("unread fact: {fact}"));
    let (Some((mn, md, mk)), Some((pn, pd, pk))) = (piq_parts(m), piq_parts(p)) else {
        return unread();
    };
    let (Some(me), Some(pe), Some(gt)) = (
        super::algebra::piq_expr(mn, md, mk),
        super::algebra::piq_expr(pn, pd, pk),
        super::eval::parse_formula(g),
    ) else {
        return unread();
    };
    let rest = Expr::Bin(
        BinOp::Sub,
        Box::new(fx.f.clone()),
        Box::new(Expr::Bin(BinOp::Mul, Box::new(me), Box::new(Expr::X))),
    );
    let shifted = gt.map(&|e| {
        matches!(e, Expr::X).then(|| Expr::Bin(BinOp::Add, Box::new(Expr::X), Box::new(pe.clone())))
    });
    let same =
        |a: &Expr, b: &Expr| super::algebra::same_trees(a, b, false, &fx.lits, &fx.vars, fx.unit);
    let (is_rest, is_periodic) = (same(&rest, &gt), same(&gt, &shifted));
    if is_rest && is_periodic {
        return Outcome::new(Class::Strong, "f − m·x is G exactly, and G(x + P) ≡ G(x)");
    }
    // At sample points: f − m·x against G, and G against G shifted.
    let ctx = fx.ctx();
    let at = |e: &Expr, x: &Iv| super::eval::eval(e, &se::var(x.clone(), 0), 0, &ctx)[0].clone();
    for &x in SAMPLES.iter().chain(&[-0.7, -2.2, -9.1]) {
        let xv = Iv::of(x);
        let (a, b) = (at(&rest, &xv), at(&gt, &xv));
        if !a.empty && a.def && !b.empty && b.def && (a.hi < b.lo || b.hi < a.lo) {
            return Outcome::new(Class::Refuted, format!("f − m·x is not G at {x}"));
        }
        let c = at(&shifted, &xv);
        if !b.empty && b.def && !c.empty && c.def && (b.hi < c.lo || c.hi < b.lo) {
            return Outcome::new(Class::Refuted, format!("G(x + P) ≠ G(x) at {x}"));
        }
    }
    Outcome::new(Class::Weak, "agrees at sample points")
}

/// How a limit fact of a rational f begins (the certifier's mark).
pub const RATIONAL: &str = "f = N/D exactly: ";

/// A fact about f at ±∞ decided exactly when f is a rational function of
/// x (N/D over the rationals): strong when it holds, refuted when not;
/// None when f is no rational function or the fact is about something
/// else.
fn rational_fact(fx: &Fx, fact: &str) -> Option<Outcome> {
    use rug::Rational;
    // "f = N/D exactly: …" claims f is rational: refuted if it isn't one.
    let (fact, claims_rational) = match fact.strip_prefix(RATIONAL) {
        Some(rest) => (rest, true),
        None => (fact, false),
    };
    let Some(rat) = super::algebra::Rat::of(&fx.f, &fx.lits, &fx.vars, fx.unit) else {
        return claims_rational
            .then(|| Outcome::new(Class::Refuted, "f is no rational function of x"));
    };
    let q = |t: &str| -> Option<Rational> {
        let (n, d, k) = piq_parts(t.trim())?;
        (k == 0).then(|| Rational::from((n, d)))
    };
    let verdict = |ok: bool, what: String| {
        if ok {
            Outcome::new(Class::Strong, format!("exactly: {what}"))
        } else {
            Outcome::new(Class::Refuted, format!("f is N/D and {what}"))
        }
    };
    let side = |at: &str| match at {
        "+∞" => Some(true),
        "−∞" => Some(false),
        _ => None,
    };
    if let Some(rest) = fact.strip_prefix("f → ") {
        let (lim, at) = rest.split_once(" as x → ")?;
        let right = side(at)?;
        let got = rat.limit(right);
        let shown = match &got {
            Ok(l) => format!("its limit at {at} is {l}"),
            Err(up) => format!("it goes to {} at {at}", if *up { "+∞" } else { "−∞" }),
        };
        let ok = match (lim, &got) {
            ("+∞", Err(true)) | ("−∞", Err(false)) => true,
            (l, Ok(v)) if l.starts_with("in ") => {
                let b = bracket(l.strip_prefix("in ")?)?;
                let v = v.to_f64();
                b.lo.to_f64() <= v && v <= b.hi.to_f64()
            }
            (l, Ok(v)) => q(l).is_some_and(|w| w == *v),
            _ => false,
        };
        return Some(verdict(ok, shown));
    }
    if let Some(rest) = fact.strip_prefix("f/x → ±∞ as x → ") {
        side(rest.split_once(':').map_or(rest, |(a, _)| a))?;
        return Some(verdict(
            rat.excess() > 1,
            format!("deg N − deg D = {}", rat.excess()),
        ));
    }
    if let Some(rest) = fact.strip_prefix("the line ") {
        // A line on its domain (holes aside): N/D is m·x + b exactly.
        let (mb, _) = rest.split_once(" on its domain")?;
        let (m, b) = mb.split_once("·x + ")?;
        let (m, b) = (q(m)?, q(b)?);
        let got = rat.affine();
        let shown = match &got {
            Some((gm, gb)) => format!("N/D = {gm}·x + {gb}"),
            None => "N/D is no line".to_string(),
        };
        return Some(verdict(got == Some((m, b)), shown));
    }
    if fact.starts_with("f = N/D exactly with deg N ≠ deg D + 1") {
        return Some(verdict(
            rat.excess() != 1,
            format!("deg N − deg D = {}", rat.excess()),
        ));
    }
    let line = |m: &str, b: &str| -> Option<Outcome> {
        let (m, b) = (q(m)?, q(b)?);
        let got = rat.line();
        let shown = match &got {
            Some((gm, gb)) => format!("N/D − ({gm}·x + {gb}) → 0"),
            None => format!("deg N − deg D = {}", rat.excess()),
        };
        Some(verdict(
            got.is_some_and(|(gm, gb)| gm == m && gb == b),
            shown,
        ))
    };
    if let Some(rest) = fact.strip_prefix("f = N/D exactly and N/D − (") {
        let (mb, _) = rest.split_once(") → 0")?;
        let (m, b) = mb.split_once("·x + ")?;
        return line(m, b);
    }
    if let Some(rest) = fact.strip_prefix("f/x → ") {
        let (m, rest) = rest.split_once(" and f − m·x → ")?;
        let (b, _) = rest.split_once(" as x → ")?;
        return line(m, b);
    }
    None
}

/// What f's values along points ever nearer `at` say of a claimed limit.
pub enum Approach {
    /// Every decided value bears it out (already there, or ever closer).
    Consistent(String),
    /// The values settle away from it (or don't grow).
    Inconsistent(String),
    Undecided(String),
}

/// The points a limit is tested at, ever nearer `at` ("+∞", "−∞", "p⁺",
/// "p⁻"): far out, a decade sequence beyond every constant in f to 10¹⁵
/// times them and on to 10³⁰⁰; near p, p ± 10⁻²…10⁻¹² (relative) and a few
/// doubles.
fn approach_points(fx: &Fx, at: &str) -> Result<Vec<f64>, String> {
    let mut scale: f64 = 1.0;
    fx.f.visit(&mut |n| {
        if let Expr::Num(v, _) = n
            && v.is_finite()
        {
            scale = scale.max(v.abs());
        }
    });
    let far: Vec<f64> = [
        16.0, 1.6e2, 1.6e3, 1.6e6, 1.6e9, 1.6e12, 1.6e15, 1.6e30, 1.6e60, 1.6e120, 1.6e240,
    ]
    .iter()
    .map(|k| k * scale)
    .filter(|x| *x <= 1e300)
    .collect();
    Ok(match at {
        "+∞" => far,
        "−∞" => far.iter().map(|x| -x).collect(),
        other => {
            let (p, right) = if let Some(p) = other.strip_suffix('⁺') {
                (p, true)
            } else if let Some(p) = other.strip_suffix('⁻') {
                (p, false)
            } else {
                return Err(format!("unread point {other}"));
            };
            let p: f64 = p.parse().map_err(|_| format!("unread point {other}"))?;
            let s = p.abs().max(1.0);
            let ulp = p.abs().next_up() - p.abs();
            let mut ds: Vec<f64> = [1e-2, 1e-4, 1e-6, 1e-8, 1e-10, 1e-12]
                .iter()
                .map(|d| d * s)
                .chain([4096.0 * ulp, 64.0 * ulp, 4.0 * ulp])
                .filter(|d| *d >= ulp)
                .collect();
            ds.sort_by(|a, b| b.total_cmp(a));
            ds.dedup();
            ds.iter()
                .map(|d| if right { p + d } else { p - d })
                .filter(|x| *x != p)
                .collect()
        }
    })
}

/// Whether `g(x, f(x))` tends to `lim` ("+∞", "−∞", a number in the
/// certifier's notation, or "in [lo, hi]") as x → `at`, judged at
/// [`approach_points`] in high precision. The nearest points decide:
/// consistent when there already (to 10⁻⁹) or ever closer; inconsistent
/// when the last three stay apart from a finite limit by more than 10⁻⁶ of
/// it without coming closer, or settle short of an infinite one.
pub fn approaches(fx: &Fx, at: &str, g: &dyn Fn(f64, Iv) -> Iv, lim: &str) -> Approach {
    let xs = match approach_points(fx, at) {
        Ok(xs) => xs,
        Err(e) => return Approach::Undecided(e),
    };
    iv::set_prec(2400);
    let vals: Vec<(f64, Iv)> = xs
        .iter()
        .map(|&x| (x, f_at(fx, &Iv::of(x))))
        .filter(|(_, v)| !v.empty && v.def && v.bounded())
        .map(|(x, v)| (x, g(x, v)))
        .filter(|(_, v)| !v.empty && v.bounded())
        .collect();
    let r = judge(&vals, lim);
    iv::set_prec(PRECS[0]);
    r
}

fn judge(vals: &[(f64, Iv)], lim: &str) -> Approach {
    if vals.len() < 3 {
        return Approach::Undecided("f not decided near the limit".into());
    }
    let n = vals.len();
    let last3 = &vals[n - 3..];
    let show = |(x, v): &(f64, Iv)| format!("{:.6e} at {x:e}", v.mid().to_f64());
    match lim {
        "+∞" | "−∞" => {
            let up = lim == "+∞";
            let grows = |a: &Iv, b: &Iv| if up { b.lo > a.hi } else { b.hi < a.lo };
            let rising = vals.windows(2).all(|w| grows(&w[0].1, &w[1].1));
            let (first, last) = (&vals[0].1, &vals[n - 1].1);
            // (In MPFR: the values may be past every double.)
            let gain = if up {
                iv::sub(last, first)
            } else {
                iv::sub(first, last)
            };
            if rising && gain.gt(1.0) {
                return Approach::Consistent(format!(
                    "{} then {}",
                    show(&vals[0]),
                    show(&vals[n - 1])
                ));
            }
            // Settled: the nearest three within 10⁻⁶ of each other.
            let lo = last3
                .iter()
                .map(|(_, v)| v.lo.to_f64())
                .fold(f64::INFINITY, f64::min);
            let hi = last3
                .iter()
                .map(|(_, v)| v.hi.to_f64())
                .fold(f64::NEG_INFINITY, f64::max);
            if hi - lo <= 1e-6 * lo.abs().max(hi.abs()).max(1.0) {
                return Approach::Inconsistent(format!(
                    "f settles ({}, {}, {}), not going to {lim}",
                    show(&last3[0]),
                    show(&last3[1]),
                    show(&last3[2])
                ));
            }
            Approach::Undecided(format!(
                "points approaching don't show {lim}: {}",
                vals.iter().map(show).collect::<Vec<_>>().join(", ")
            ))
        }
        l => {
            let target = if let Some(b) = l.strip_prefix("in ") {
                bracket(b)
            } else {
                pi_q(l)
            };
            let Some(t) = target else {
                return Approach::Undecided(format!("unread limit {l}"));
            };
            let scale = t.hi.to_f64().abs().max(t.lo.to_f64().abs()).max(1.0);
            // Distance from the limit: at most, and at least.
            let far = |v: &Iv| iv::abs(&iv::sub(v, &t)).hi.to_f64();
            let apart = |v: &Iv| {
                let below = t.lo.to_f64() - v.hi.to_f64();
                let above = v.lo.to_f64() - t.hi.to_f64();
                below.max(above).max(0.0)
            };
            let ds: Vec<f64> = vals.iter().map(|(_, v)| far(v)).collect();
            let last = ds[n - 1];
            if last <= 1e-9 * scale {
                return Approach::Consistent(format!("within {last:.1e} at {:e}", vals[n - 1].0));
            }
            if ds[n - 3..].windows(2).all(|w| w[1] <= w[0]) && last < 0.5 * ds[0] {
                return Approach::Consistent(format!("ever closer: {:.3e} … {last:.3e}", ds[0]));
            }
            let gaps: Vec<f64> = last3.iter().map(|(_, v)| apart(v)).collect();
            if gaps.iter().all(|g| *g > 1e-6 * scale)
                && gaps.windows(2).all(|w| w[1] >= 0.999 * w[0])
            {
                return Approach::Inconsistent(format!(
                    "f stays apart from {l}: {}, {}, {}",
                    show(&last3[0]),
                    show(&last3[1]),
                    show(&last3[2])
                ));
            }
            Approach::Undecided(format!("points approaching don't show {l}"))
        }
    }
}

/// A gap claims nothing. It is a few doubles wide, or it is the
/// enclosure x0 + k·period of a member of one of the certificate's
/// families (a far member's is k periods' rounding wide) and a few doubles
/// either side.
fn gap(x: B, cert: &[Claim]) -> Outcome {
    let mut n = 0;
    let mut p = x.0;
    while p < x.1 && n <= 64 {
        p = p.next_up();
        n += 1;
    }
    if n <= 64 {
        return Outcome::new(Class::Strong, format!("{n} doubles wide"));
    }
    for c in cert {
        if let Claim::Family { x0, period, .. } = c
            && let Some(k) = member_box(x, *x0, *period)
        {
            return Outcome::new(
                Class::Strong,
                format!("member {k} of the family at {:e}, enclosed", x0.0),
            );
        }
    }
    Outcome::new(
        Class::Unconfirmed,
        format!("a gap over 64 doubles wide: [{:e}, {:e}]", x.0, x.1),
    )
}

/// The k for which the box `x` holds the box x0 + k·period (computed
/// exactly) and reaches at most 64 doubles beyond it on either side.
fn member_box(x: B, x0: B, period: B) -> Option<i64> {
    use rug::Rational;
    let q = Rational::from_f64;
    if !(period.0 > 0.0 && period.1.is_finite()) {
        return None;
    }
    let mid = |b: B| b.0 / 2.0 + b.1 / 2.0;
    let kf = ((mid(x) - mid(x0)) / mid(period)).round();
    if !kf.is_finite() || kf.abs() > 1e15 {
        return None;
    }
    let step = |mut v: f64, up: bool| {
        for _ in 0..65 {
            v = if up { v.next_up() } else { v.next_down() };
        }
        v
    };
    (-1..=1).find_map(|dk| {
        let k = kf + dk as f64;
        let (pa, pb) = if k >= 0.0 {
            (period.0, period.1)
        } else {
            (period.1, period.0)
        };
        let lo = q(x0.0)? + q(k)? * q(pa)?;
        let hi = q(x0.1)? + q(k)? * q(pb)?;
        let holds = q(x.0)? <= lo && hi <= q(x.1)?;
        let near = step(lo.to_f64(), false) <= x.0 && x.1 <= step(hi.to_f64(), true);
        (holds && near).then_some(k as i64)
    })
}

/// Whether f⁽ᵏ⁾ has no zero in the gap box `g` (or is 0 throughout it),
/// wherever it exists there:
/// `Some(true)` shown on the canonical tree (or on a certifier's tree shown
/// identical to it exactly), `Some(false)` only on the certifier's tree,
/// `None` not shown.
pub fn gap_clear(fx: &Fx, k: usize, g: B) -> Option<bool> {
    if unmodelled(&fx.f) {
        return None;
    }
    // The reals strictly inside the gap: its ends are doubles other claims
    // decide (or the excluded point). Beside an excluded 0, x keeps its
    // strict sign.
    let x = |p: u32| {
        iv::set_prec(p);
        Iv::of2(g.0, g.1).strict(g.0 == 0.0, g.1 == 0.0)
    };
    // Away from 0, or exactly 0 throughout (f⁽ᵏ⁾ ≡ 0: no zero of its own).
    let away = |e: &Expr, k: usize| -> bool {
        [160, 640].into_iter().any(|p| {
            let xb = x(p);
            let s = se::where_defined(|| fx.series_iv(e, xb, k));
            let c = &s[k];
            let r = s[0].empty || c.empty || c.ne0() || (c.is_point() && c.lo == 0);
            iv::set_prec(160);
            r
        })
    };
    if away(&fx.f, k) {
        return Some(true);
    }
    // A factor exactly 0 at one of the gap's doubles and strictly monotone
    // over the closed gap is 0 nowhere strictly inside it.
    let end_root = |h: &Expr| -> bool {
        iv::set_prec(160);
        let root = [g.0, g.1].into_iter().any(|e| {
            let v = &fx.series_iv(h, Iv::of(e), 0)[0];
            v.def && v.is_point() && v.lo == 0
        });
        root && {
            let s = fx.series_iv(h, Iv::of2(g.0, g.1), 1);
            s[0].cont && s[1].def && !s[1].empty && s[1].ne0()
        }
    };
    // Factor by factor (f's zeros are among its factors').
    let by_factors = |e: &Expr| -> bool {
        let mut safe = Vec::new();
        nonzero_where_defined(&fx.f, &mut safe);
        nonzero_where_defined(e, &mut safe);
        zero_factors(e)
            .iter()
            .all(|h| safe.contains(h) || away(h, 0) || end_root(h))
    };
    if k == 0 && by_factors(&fx.f) {
        return Some(true);
    }
    if k > 0
        && let Some(d) = &fx.d[k - 1]
        && (away(d, 0) || by_factors(d))
    {
        return Some(fx.verified.d[k - 1]);
    }
    // The simplifier's form of f (equal to f where f is defined).
    if let Some(e) = &fx.f_eval
        && (away(e, k) || (k == 0 && by_factors(e)))
    {
        return Some(fx.verified.f);
    }
    rational_clear(fx, k, g).then_some(true)
}

/// For f a rational function of x alone: f⁽ᵏ⁾ = N/D exactly, read by the
/// replay's own algebra, and wherever f⁽ᵏ⁾ exists its zeros are N's
/// ([`super::algebra::numerator`]). N, a polynomial with exact
/// coefficients, has no zero strictly inside the gap `g` if its enclosure
/// there is strictly signed, or if it is strictly monotone over the closed
/// gap (N′'s enclosure strictly signed) and its exact values at the gap's
/// ends aren't of strictly opposite signs. ((x + 1)/(x² − 1) + 10⁻⁷ beside
/// its hole at −1: f's tree is 0/0 there, and N = x + 1 + 10⁻⁷·(x² − 1) is
/// 0 at −1 and increasing.)
fn rational_clear(fx: &Fx, k: usize, g: B) -> bool {
    use rug::Rational;
    use std::cmp::Ordering;
    if !(g.0.is_finite() && g.1.is_finite() && g.0 <= g.1) {
        return false;
    }
    let Some(n) = super::algebra::numerator(&fx.f, k, &fx.lits, &fx.vars, fx.unit) else {
        return false;
    };
    if n.is_empty() {
        return false;
    }
    let horner = |p: &[Rational], x: &Iv| -> Iv {
        p.iter().rev().fold(Iv::of(0.0), |acc, c| {
            iv::add(&iv::mul(&acc, x), &iv::ratio(c.numer(), c.denom()))
        })
    };
    iv::set_prec(160);
    let xb = Iv::of2(g.0, g.1);
    if horner(&n, &xb).ne0() {
        return true;
    }
    let dn: Vec<Rational> = n
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, c)| Rational::from(c * i as u32))
        .collect();
    if !horner(&dn, &xb).ne0() {
        return false;
    }
    let sign_at = |x: f64| -> Option<Ordering> {
        let q = Rational::from_f64(x)?;
        let v = n.iter().rev().fold(Rational::new(), |acc, c| acc * &q + c);
        Some(v.cmp0())
    };
    let (Some(a), Some(b)) = (sign_at(g.0), sign_at(g.1)) else {
        return false;
    };
    !matches!(
        (a, b),
        (Ordering::Less, Ordering::Greater) | (Ordering::Greater, Ordering::Less)
    )
}

/// Constants by name (for the example's printing).
pub fn constant_name(c: Constant) -> &'static str {
    match c {
        Constant::Pi => "π",
        Constant::E => "e",
    }
}
