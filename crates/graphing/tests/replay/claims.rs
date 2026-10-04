//! Checking one claim: each kind's statement (`certify::cert`'s doc
//! comments) proved with the replay's own enclosures, by bisection of the
//! box and, failing that, at a higher precision.

use super::eval::{at_path, contains_x, written_rational};
use super::iv::{self, Iv, Unit};
use super::series::{self as se};
use super::{B, Claim, Class, Fx, Outcome, Side, Subject, Via, exact};
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
        Expr::Num(_) | Expr::Const(_) | Expr::X | Expr::Var(_) => false,
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
    if !(lo < hi) {
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
        let s = self.fx.series(&self.e, lo, hi, self.k + m);
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
                    let o = self.fx.series(&self.fx.f, lo, hi, *k);
                    valid_f(&o, *k) && !c[0].empty && c[0].def
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

/// Runs `f` on the canonical tree, then (if it couldn't decide) on the
/// certifier's tree: strong, weak, refuted or unconfirmed.
fn by_trees(fx: &Fx, of: &Subject, mut f: impl FnMut(&Subj) -> V) -> Outcome {
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
        && let V::Yes = ladder(|| match f(&t) {
            V::No(w) => V::Unknown(w),
            v => v,
        })
    {
        return Outcome::new(Class::Weak, "on the certifier's tree");
    }
    Outcome::new(Class::Unconfirmed, why)
}

fn on(x: B, check: &mut dyn FnMut(f64, f64) -> V) -> V {
    let mut budget = BUDGET;
    prove(x.0, x.1, &mut budget, check)
}

/// The subject valid on the box and strictly on side `above` of c.
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

pub fn check(fx: &Fx, c: &Claim) -> Outcome {
    match c {
        Claim::Defined(x) => defined(fx, *x, true),
        Claim::Undefined(x) => defined(fx, *x, false),
        Claim::Beyond { x, of, c, above } => by_trees(fx, of, |s| beyond(s, *x, *c, *above, false)),
        Claim::NoCross { x, of, c, above } => by_trees(fx, of, |s| {
            all([
                end_beyond(s, x.0, *c, *above),
                end_beyond(s, x.1, *c, *above),
                monotone(s, *x),
            ])
        }),
        Claim::OneCross { x, of, c } => by_trees(fx, of, |s| {
            let a = end_beyond(s, x.0, *c, true);
            let below_first = !a.yes();
            all([
                end_beyond(s, x.0, *c, !below_first),
                end_beyond(s, x.1, *c, below_first),
                monotone(s, *x),
            ])
        }),
        Claim::ExactAt { x, of, c, at } => by_trees(fx, of, |s| {
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
        Claim::Factors { x, of, c, above } => by_trees(fx, of, |s| {
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
        } => by_trees(fx, of, |s| touch(s, *x, *c, *at, *order, *above)),
        Claim::Value { x, of, lo, hi } => by_trees(fx, of, |s| value(s, *x, *lo, *hi)),
        Claim::Family { of, x0, period } => family(fx, of, *x0, *period),
        Claim::TailBeyond {
            side,
            from,
            of,
            c,
            above,
        } => by_trees(fx, of, |s| {
            beyond(s, tail_box(*side, *from), *c, *above, false)
        }),
        Claim::TailChain {
            side,
            from,
            of,
            c,
            above,
            order,
        } => by_trees(fx, of, |s| tail_chain(s, *side, *from, *c, *above, *order)),
        Claim::TailValue { side, from, lo, hi } => by_trees(fx, &Subject::F(0), |s| {
            value(s, tail_box(*side, *from), *lo, *hi)
        }),
        Claim::Unbounded { near, at } => unbounded(fx, *near, *at),
        Claim::Removable { near, at, lo, hi } => removable(fx, *near, *at, *lo, *hi),
        Claim::Simplifier(fact) => simplifier(fx, fact),
        Claim::Gap(x) => gap(*x),
    }
}

fn defined(fx: &Fx, x: B, want: bool) -> Outcome {
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
            match (want, v.empty, v.def) {
                (true, false, true) | (false, true, _) => V::Yes,
                (true, true, _) => V::No(format!("undefined on [{lo:e}, {hi:e}]")),
                (false, false, true) => V::No(format!("defined on [{lo:e}, {hi:e}]")),
                _ => unknown(format!("{} on [{lo:e}, {hi:e}]", show(v))),
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
    if !why.starts_with("undefined") {
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
            Expr::Num(v) if *v != 0.0 => {}
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
                    || Expr::Bin(BinOp::Sub, Box::new(a.clone()), Box::new(Expr::Num(1.0)));
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
                        Box::new(Expr::Num(1.0)),
                    )),
                    _ => out.push(e.clone()),
                }
            }
            _ => out.push(e.clone()),
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
        Expr::Num(_) | Expr::Const(_) | Expr::X | Expr::Y | Expr::Var(_) => {}
        Expr::Neg(a) | Expr::Degrees(a) => nonzero_where_defined(a, out),
        Expr::Bin(op, a, b) => {
            if *op == BinOp::Div {
                out.push((**b).clone());
            }
            if *op == BinOp::Pow && written_rational(b).is_some_and(|(p, _)| p < 0) {
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
    // The order-m coefficient over the box, of one sign τ.
    let left_end = at == x.0;
    let want = |tau: bool| -> bool {
        if left_end || m % 2 == 0 {
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
            Side::Left => sigma == (order % 2 == 0),
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
                Side::Left => sigma == ((order - i) % 2 == 0),
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
        exact::eval(e, None, &fx.lits, &fx.vars)
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
fn zero_table(g: &Expr) -> Option<(&Expr, i64, i64)> {
    let one = |e: &Expr| matches!(e, Expr::Num(v) if *v == 1.0);
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
    let Some((u, theta, step)) = zero_table(&g) else {
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
                written_rational(k).is_some_and(|(p, _)| p > 0) && self.vanishes(a)
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
        let one = Expr::Num(1.0);
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
            Expr::Bin(BinOp::Pow, a, k) => match written_rational(k) {
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
fn pi_q(text: &str) -> Option<Iv> {
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
pub fn tree_parity(e: &Expr) -> Option<bool> {
    if !contains_x(e) {
        return Some(true);
    }
    Some(match e {
        Expr::X => false,
        Expr::Neg(a) | Expr::Degrees(a) => tree_parity(a)?,
        Expr::Bin(BinOp::Add | BinOp::Sub, a, b) => {
            let (pa, pb) = (tree_parity(a)?, tree_parity(b)?);
            (pa == pb).then_some(pa)?
        }
        Expr::Bin(BinOp::Mul | BinOp::Div, a, b) => tree_parity(a)? == tree_parity(b)?,
        Expr::Bin(BinOp::Pow, a, b) => {
            let pa = tree_parity(a)?;
            if contains_x(b) {
                (pa && tree_parity(b)?).then_some(true)?
            } else if pa {
                true
            } else {
                match written_rational(b)? {
                    (p, q) if q % 2 == 1 => p % 2 == 0,
                    _ => return None,
                }
            }
        }
        Expr::Call(Func::Root, args) if args.len() == 2 && !contains_x(&args[1]) => {
            let pa = tree_parity(&args[0])?;
            match written_rational(&args[1])? {
                (n, 1) if pa || n % 2 != 0 => pa,
                _ => return None,
            }
        }
        Expr::Call(f, args) if args.len() == 1 => {
            if tree_parity(&args[0])? {
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
            .all(|a| tree_parity(a) == Some(true))
            .then_some(true)?,
        _ => return None,
    })
}

const SAMPLES: [f64; 12] = [
    0.1, 0.37, 0.5, 1.0, 1.3, 2.9, 3.7, 7.7, 11.3, 25.1, 113.0, 1e4,
];

fn simplifier(fx: &Fx, fact: &str) -> Outcome {
    if unmodelled(&fx.f) {
        return Outcome::new(
            Class::Unsupported,
            "the tree uses a function the replay doesn't model",
        );
    }
    if let Some(rest) = fact.strip_prefix("f is ") {
        let even = rest.starts_with("even");
        if !even && !rest.starts_with("odd") {
            return Outcome::new(Class::Unconfirmed, format!("unread fact: {fact}"));
        }
        // The structural rules are the replay's own: a proof.
        if tree_parity(&fx.f) == Some(even) {
            return Outcome::new(Class::Strong, "by the tree's structure");
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
    if let Some(rest) = fact.strip_prefix("f → ") {
        let Some((lim, at)) = rest.split_once(" as x → ") else {
            return Outcome::new(Class::Unconfirmed, format!("unread fact: {fact}"));
        };
        // Points approaching `at`, nearer each time.
        let xs: Vec<f64> = match at {
            "+∞" => [1e3, 1e6, 1e12, 1e24, 1e48, 1e96, 1e192, 1e300].to_vec(),
            "−∞" => [-1e3, -1e6, -1e12, -1e24, -1e48, -1e96, -1e192, -1e300].to_vec(),
            other => {
                let (p, right) = if let Some(p) = other.strip_suffix('⁺') {
                    (p, true)
                } else if let Some(p) = other.strip_suffix('⁻') {
                    (p, false)
                } else {
                    return Outcome::new(Class::Unconfirmed, format!("unread fact: {fact}"));
                };
                let Ok(p) = p.parse::<f64>() else {
                    return Outcome::new(Class::Unconfirmed, format!("unread point: {fact}"));
                };
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
        };
        // The values decided there (f defined, bounded).
        let vals: Vec<Iv> = xs
            .iter()
            .map(|&x| f_at(fx, &Iv::of(x)))
            .filter(|v| !v.empty && v.def && v.bounded())
            .collect();
        if vals.len() < 3 {
            return Outcome::new(Class::Unconfirmed, "f not decided near the limit");
        }
        // Weak evidence: f moves monotonically toward the limit along the
        // points (rising without bound, falling, or ever closer to L).
        let ok = match lim {
            "+∞" => vals.windows(2).all(|w| w[1].lo > w[0].hi),
            "−∞" => vals.windows(2).all(|w| w[1].hi < w[0].lo),
            l => {
                let target = if let Some(b) = l.strip_prefix("in ") {
                    bracket(b)
                } else {
                    pi_q(l)
                };
                let Some(t) = target else {
                    return Outcome::new(Class::Unconfirmed, format!("unread limit: {fact}"));
                };
                let d = |v: &Iv| iv::abs(&iv::sub(v, &t)).hi.to_f64();
                let ds: Vec<f64> = vals.iter().map(d).collect();
                let scale = t.hi.to_f64().abs().max(1.0);
                ds.windows(2).all(|w| w[1] <= w[0])
                    && (ds[ds.len() - 1] < ds[0] || ds[0] <= 1e-12 * scale)
            }
        };
        return if ok {
            Outcome::new(Class::Weak, "agrees at points approaching the limit")
        } else {
            Outcome::new(
                Class::Unconfirmed,
                "points approaching the limit don't show it",
            )
        };
    }
    Outcome::new(Class::Unconfirmed, format!("unread fact: {fact}"))
}

/// A gap claims nothing; it must be a few doubles wide at most.
fn gap(x: B) -> Outcome {
    let mut n = 0;
    let mut p = x.0;
    while p < x.1 && n <= 64 {
        p = p.next_up();
        n += 1;
    }
    if n <= 64 {
        Outcome::new(Class::Strong, format!("{n} doubles wide"))
    } else {
        Outcome::new(
            Class::Unconfirmed,
            format!("a gap over 64 doubles wide: [{:e}, {:e}]", x.0, x.1),
        )
    }
}

/// Constants by name (for the example's printing).
pub fn constant_name(c: Constant) -> &'static str {
    match c {
        Constant::Pi => "π",
        Constant::E => "e",
    }
}
