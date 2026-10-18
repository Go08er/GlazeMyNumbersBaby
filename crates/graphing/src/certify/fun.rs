//! The function under analysis: its canonical tree, its literals, the
//! angle unit, and a budget of interval evaluations.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::ast::{BinOp, Expr, Func, Lit};
use crate::compile::CompileOptions;
use crate::interval::{Ctx, DecInterval, Interval, Series, taylor};
use crate::simplify::q::Q;

/// Why certification stopped early.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The evaluation budget ran out.
    Budget,
    /// The caller cancelled.
    Cancelled,
}

/// The work one unit of the evaluation budget stands for: a tree's nodes
/// times the Taylor order plus one (x³ − 2x + 1/(x − 1) to order 2 is one
/// unit; a sum of forty sines, about eight).
pub const UNIT_WORK: usize = 64;

/// The tree every claim refers to: the parser's, with `a·a` written `a²`
/// (an interval product of a box with itself loses the correlation:
/// `(x−c)·(x−c)` over a box around c straddles 0, `(x−c)²` doesn't). Its
/// numbers are the parser's, each with its own exact value: the written 2
/// is exactly 2, whatever a typed `2.0000000000000001` parsed to.
pub fn canonical(e: &Expr) -> Expr {
    let go = canonical;
    match e {
        Expr::Num(..) | Expr::Const(_) | Expr::X | Expr::Y | Expr::Var(_) => e.clone(),
        Expr::Neg(a) => Expr::Neg(Box::new(go(a))),
        Expr::Degrees(a) => Expr::Degrees(Box::new(go(a))),
        Expr::Bin(op, a, b) => {
            let (a, b) = (go(a), go(b));
            if *op == BinOp::Mul && a == b {
                Expr::bin(BinOp::Pow, a, Expr::exact(2.0))
            } else {
                Expr::bin(*op, a, b)
            }
        }
        Expr::Call(f, args) => Expr::Call(*f, args.iter().map(go).collect()),
    }
}

/// `e` as a·x + b with a, b exact rationals, if it is one (sliders and π
/// aside).
pub(crate) fn affine(e: &Expr) -> Option<(Q, Q)> {
    Some(match e {
        Expr::X => (Q::ONE, Q::ZERO),
        Expr::Num(v, lit) => (Q::ZERO, lit.q(*v)?),
        Expr::Neg(a) => {
            let (a, b) = affine(a)?;
            (a.neg()?, b.neg()?)
        }
        Expr::Bin(op @ (BinOp::Add | BinOp::Sub), l, r) => {
            let ((a, b), (c, d)) = (affine(l)?, affine(r)?);
            if *op == BinOp::Add {
                (a.add(c)?, b.add(d)?)
            } else {
                (a.sub(c)?, b.sub(d)?)
            }
        }
        Expr::Bin(BinOp::Mul, l, r) => {
            let ((a, b), (c, d)) = (affine(l)?, affine(r)?);
            match (a.is_zero(), c.is_zero()) {
                (true, _) => (c.mul(b)?, d.mul(b)?),
                (_, true) => (a.mul(d)?, b.mul(d)?),
                _ => return None,
            }
        }
        Expr::Bin(BinOp::Div, l, r) => {
            let ((a, b), (c, d)) = (affine(l)?, affine(r)?);
            if !c.is_zero() || d.is_zero() {
                return None;
            }
            (a.div(d)?, b.div(d)?)
        }
        _ => return None,
    })
}

/// A whole number ≥ 0 that `Num` reads back exactly.
fn int_num(q: Q) -> Option<Expr> {
    (q.is_int() && q.signum() >= 0 && q.numer() <= 1 << 53).then(|| Expr::exact(q.numer() as f64))
}

/// `e` as c·xᵏ (c exact, k whole ≤ 64) if it is a monomial.
fn monomial(e: &Expr) -> Option<(Q, u32)> {
    Some(match e {
        Expr::X => (Q::ONE, 1),
        Expr::Num(v, lit) => (lit.q(*v)?, 0),
        Expr::Neg(a) => {
            let (c, k) = monomial(a)?;
            (c.neg()?, k)
        }
        Expr::Bin(BinOp::Mul, a, b) => {
            let ((c, k), (d, j)) = (monomial(a)?, monomial(b)?);
            (c.mul(d)?, k + j)
        }
        Expr::Bin(BinOp::Div, a, b) => {
            let ((c, k), (d, 0)) = (monomial(a)?, monomial(b)?) else {
                return None;
            };
            (c.div(d)?, k)
        }
        Expr::Bin(BinOp::Pow, a, b) => {
            let (c, k) = monomial(a)?;
            let n = crate::compile::syntactic_rational(b, crate::compile::Reading::Typed)
                .filter(|&(_, q)| q == 1)
                .map(|(p, _)| p)
                .filter(|&p| (0..=64).contains(&p))?;
            (c.powi(n.into())?, k * u32::try_from(n).ok()?)
        }
        _ => return None,
    })
    .filter(|&(_, k)| k <= 64)
}

/// The terms of a sum of monomials (± each), as coefficients by power.
fn monomial_sum(e: &Expr, sign: bool, out: &mut Vec<Q>) -> Option<()> {
    match e {
        Expr::Bin(op @ (BinOp::Add | BinOp::Sub), a, b) => {
            monomial_sum(a, sign, out)?;
            monomial_sum(b, sign == (*op == BinOp::Add), out)
        }
        _ => {
            let (c, k) = monomial(e)?;
            let c = if sign { c } else { c.neg()? };
            let k = k as usize;
            if out.len() <= k {
                out.resize(k + 1, Q::ZERO);
            }
            out[k] = out[k].add(c)?;
            Some(())
        }
    }
}

/// An exact rational as an expression `Num` reads back exactly.
fn q_num(q: Q) -> Option<Expr> {
    let m = q.abs()?;
    let e = if m.is_int() {
        int_num(m)?
    } else {
        Expr::bin(
            BinOp::Div,
            int_num(Q::int(m.numer()))?,
            int_num(Q::int(m.denom()))?,
        )
    };
    Some(if q.signum() < 0 {
        Expr::Neg(Box::new(e))
    } else {
        e
    })
}

/// A sum of monomials of degree ≥ 2 (x² + x + 1, as a rational function's
/// parts come expanded) in Horner's form, ((aₙx + aₙ₋₁)x + …)x + a₀: on a
/// tail every product there has its factors of one sign, where the sum
/// as written is ∞ − ∞ (x² + x on (−∞, −M]).
fn horner(e: &Expr) -> Option<Expr> {
    if !matches!(e, Expr::Bin(BinOp::Add | BinOp::Sub, ..)) {
        return None;
    }
    let mut cs: Vec<Q> = Vec::new();
    monomial_sum(e, true, &mut cs)?;
    while cs.last().is_some_and(|c| c.is_zero()) {
        cs.pop();
    }
    if cs.len() < 3 {
        return None;
    }
    let mut acc: Option<Expr> = None;
    for c in cs.iter().rev() {
        let next = match acc {
            None => q_num(*c)?,
            Some(a) => {
                let ax = match a {
                    Expr::Num(v, Lit::Exact) if v == 1.0 => Expr::X,
                    a => Expr::bin(BinOp::Mul, a, Expr::X),
                };
                if c.is_zero() {
                    ax
                } else if c.signum() < 0 {
                    Expr::bin(BinOp::Sub, ax, q_num(c.neg()?)?)
                } else {
                    Expr::bin(BinOp::Add, ax, q_num(*c)?)
                }
            }
        };
        acc = Some(next);
    }
    acc
}

/// `e` with each part a·x + b in x (exact rationals a ≠ 0 and b, with
/// c = −b/a an integer) written a·(x − c): x − c is exact near c, where
/// a·x + b cancels. (x/1000 − 1 at x = 1000 + 2⁻⁴³, evaluated as written,
/// is [0, 2⁻⁵²]; as (x − 1000)/1000, 2⁻⁴³/1000 to an ulp.) And each sum of
/// monomials of degree ≥ 2 in Horner's form ([`horner`]). The same real
/// function, so f's evaluated tree may be written so.
pub fn recentre(e: &Expr) -> Expr {
    if !matches!(e, Expr::X)
        && let Some((a, b)) = affine(e)
        && !a.is_zero()
        && let Some(form) = (|| {
            let c = b.neg()?.div(a)?;
            let shifted = match c.signum() {
                0 => Expr::X,
                1 => Expr::bin(BinOp::Sub, Expr::X, int_num(c)?),
                _ => Expr::bin(BinOp::Add, Expr::X, int_num(c.neg()?)?),
            };
            let m = a.abs()?;
            let scaled = if m == Q::ONE {
                shifted
            } else if m.is_int() {
                Expr::bin(BinOp::Mul, int_num(m)?, shifted)
            } else if m.numer() == 1 {
                Expr::bin(BinOp::Div, shifted, int_num(Q::int(m.denom()))?)
            } else {
                let num = int_num(Q::int(m.numer()))?;
                let den = int_num(Q::int(m.denom()))?;
                Expr::bin(BinOp::Mul, Expr::bin(BinOp::Div, num, den), shifted)
            };
            Some(if a.signum() < 0 {
                Expr::Neg(Box::new(scaled))
            } else {
                scaled
            })
        })()
    {
        return form;
    }
    if let Some(h) = horner(e) {
        return h;
    }
    match e {
        Expr::Neg(a) => Expr::Neg(Box::new(recentre(a))),
        Expr::Degrees(a) => Expr::Degrees(Box::new(recentre(a))),
        Expr::Bin(op, a, b) => Expr::bin(*op, recentre(a), recentre(b)),
        Expr::Call(f, args) => Expr::Call(*f, args.iter().map(recentre).collect()),
        _ => e.clone(),
    }
}

/// The sub-expression at `path` (see [`super::cert`]).
pub fn at_path<'e>(e: &'e Expr, path: &[u8]) -> Option<&'e Expr> {
    let Some((&i, rest)) = path.split_first() else {
        return Some(e);
    };
    let child = match e {
        Expr::Neg(a) | Expr::Degrees(a) if i == 0 => a,
        Expr::Bin(_, a, _) if i == 0 => a,
        Expr::Bin(_, _, b) if i == 1 => b,
        Expr::Call(_, args) => args.get(i as usize)?,
        _ => return None,
    };
    at_path(child, rest)
}

/// The function, its options and its budget.
pub struct Fun<'a> {
    /// The canonical tree: where f is defined comes from its side
    /// conditions, and side expressions are paths into it.
    pub expr: Expr,
    /// The canonical tree f's values are enclosed with: `expr`, or the
    /// simplifier's form of it, equal to f wherever f is defined (it may be
    /// defined in more places: `(x²−1)/(x−1)` is evaluated as x + 1, so
    /// nothing cancels near the hole).
    pub eval: Expr,
    /// f′ and f″ as trees of their own: when f is a rational function,
    /// from its exact form N/D, f′ = (N′D − ND′)/D² and f″ = (P′D −
    /// 2PD′)/D³ with P = N′D − ND′, each numerator expanded exactly (what
    /// cancels in them cancels exactly, not in interval arithmetic);
    /// otherwise by symbolic differentiation (`crate::diff`) of the
    /// evaluated tree. Equal to f′, f″ wherever f is differentiable; used
    /// only where f is proven continuous.
    derivs: std::cell::OnceCell<Option<[Expr; 2]>>,
    /// The same trees for f with a kink ([`Fun::kinked`]): equal to f′, f″
    /// only away from the kinks, so used only by covers that leave the
    /// kinks out ([`Fun::derivs_off_kinks`]).
    derivs_kinked: std::cell::OnceCell<Option<[Expr; 2]>>,
    /// Whether f may have a symbolic derivative tree everywhere (no jumps,
    /// no kinks).
    pub smooth_tree: bool,
    /// f has jumps (floor, round, sign, mod of x): no derivative tree at
    /// all.
    pub steps: bool,
    /// f has a kink (abs, min or max of x): its derivative tree,
    /// sign(u)·u′, is defined at the kink, where f′ is not.
    pub kinked: bool,
    /// Expressions f's side conditions require to be ≠ 0 (divisors, the
    /// sine under csc, …): never 0 where f is defined.
    pub nonzero: Vec<Expr>,
    /// For a rational f, the numerators of f, f′, f″ in lowest terms
    /// ([`rational_numerators`]).
    pub numerators: Option<[Expr; 3]>,
    /// The simplifier may be used for proofs (`certify_equation`'s
    /// analyses; not the panel's own checks).
    pub simplifier: bool,
    pub opts: CompileOptions<'a>,
    /// Trees as evaluated: each with its arithmetic on literals and
    /// sliders alone folded exactly (`compile::fold_literals`, as the plot's
    /// interval form is), where that changes it. Off, the divisor
    /// 1.0000000000000001 − 1 is 10⁻¹⁶, where each literal enclosed alone
    /// gives [0, 2⁻⁵²]. (A few trees: f's, its simplified form, f′, f″, the
    /// side conditions'.)
    folds: std::cell::RefCell<Vec<(Expr, Option<std::sync::Arc<Expr>>)>>,
    /// How far out a tail must start to lie inside the domain, on the left
    /// and on the right (`rows::tail_past`; −∞ until the domain is
    /// known): the tails' starts are taken past it (`rows::far_points`).
    pub tails_past: Cell<[f64; 2]>,
    evals: Cell<u64>,
    /// The current phase's limit.
    budget: Cell<u64>,
    /// The overall limit.
    total: Cell<u64>,
    cancel: Option<&'a AtomicBool>,
}

impl<'a> Fun<'a> {
    pub fn new(
        expr: &Expr,
        opts: CompileOptions<'a>,
        budget: u64,
        cancel: Option<&'a AtomicBool>,
    ) -> Fun<'a> {
        let expr = canonical(expr);
        Fun {
            eval: expr.clone(),
            expr,
            derivs: std::cell::OnceCell::new(),
            derivs_kinked: std::cell::OnceCell::new(),
            smooth_tree: false,
            steps: false,
            kinked: false,
            nonzero: Vec::new(),
            numerators: None,
            simplifier: false,
            opts,
            folds: std::cell::RefCell::new(Vec::new()),
            tails_past: Cell::new([f64::NEG_INFINITY; 2]),
            evals: Cell::new(0),
            budget: Cell::new(budget),
            total: Cell::new(budget),
            cancel,
        }
    }

    /// The simplifier's settings for f, when the simplifier may be used.
    pub fn settings(&self) -> Option<crate::simplify::Settings<'a>> {
        if !self.simplifier {
            return None;
        }
        let mut s = crate::simplify::Settings::new(&self.opts);
        s.cancel = self.cancel;
        // Several runs per analysis: each kept small.
        s.limits.nodes = CERTIFY_NODES;
        Some(s)
    }

    /// f′ and f″ as trees of their own ([`Fun::derivs`]'s field doc),
    /// built on first use (simplifying them costs tens of milliseconds).
    pub fn derivs(&self) -> Option<&[Expr; 2]> {
        self.derivs
            .get_or_init(|| self.build_derivs(self.smooth_tree))
            .as_ref()
    }

    /// The derivative trees for use away from f's kinks only: for an f
    /// without kinks the same as [`Fun::derivs`]; for one with kinks, the
    /// trees of `sign(u)·u′`, which equal f′ wherever u ≠ 0. A caller must
    /// keep the kinks (`certify::kinks`) out of every box it evaluates
    /// them on.
    pub fn derivs_off_kinks(&self) -> Option<&[Expr; 2]> {
        if !self.kinked {
            return self.derivs();
        }
        self.derivs_kinked
            .get_or_init(|| self.build_derivs(!self.steps))
            .as_ref()
    }

    fn build_derivs(&self, symbolic: bool) -> Option<[Expr; 2]> {
        if !self.simplifier {
            return None;
        }
        // (A rational f's, expanded exactly, in Horner's form.)
        let rational = rational_derivs(&self.expr).map(|d| d.map(|t| recentre(&t)));
        rational.or_else(|| {
            if !symbolic {
                return None;
            }
            let s = self.settings()?;
            // A typed decimal exponent as the exact fraction it is (x^0.9
            // is x^(9/10) where x ≥ 0, the only x it is evaluated at), so
            // the derivative's exponents stay exact; and every constant in
            // the trees typed or an integer (none computed in floating
            // point).
            let e = exact_exponents(&self.eval);
            let d = symbolic_derivs(&e, self.opts.trig_unit, Some(&s))?;
            d.iter().all(sound_constants).then_some(d)
        })
    }

    /// The derivative trees, if already built (either kind).
    pub fn derivs_built(&self) -> Option<&[Expr; 2]> {
        self.derivs
            .get()
            .and_then(|d| d.as_ref())
            .or_else(|| self.derivs_kinked.get().and_then(|d| d.as_ref()))
    }

    /// Where f′ or f″ is singular though f is continuous: the bases u
    /// (varying with x) of f's fractional powers u^r, √u, ∛u, with r: at
    /// u = 0, f′ is unbounded for r < 1 (x^(1/3), x^(2/3)), f″ for r < 2
    /// (x^(4/3)). Each base once, with its smallest r.
    pub fn singular_args(&self) -> Vec<(Expr, f64)> {
        let mut args: Vec<(Expr, f64)> = Vec::new();
        let exponent = |b: &Expr| -> Option<f64> {
            if b.contains_x() {
                return None;
            }
            // Written as a ratio of integers, any size (review 15,
            // R15-M-01), its order against 1 and 2 kept exactly though
            // p/q rounds (to 1, say); too long to carry, as small as any.
            match crate::compile::written(b, crate::compile::Reading::Typed) {
                Some(crate::compile::Written::Ratio(r)) => {
                    if r.is_integer() {
                        return None;
                    }
                    let v = r.value();
                    let below = |k: u64| {
                        v.sub(&crate::big::Rat::int(k))
                            .is_some_and(|d| d.is_negative())
                    };
                    let f = v.to_f64(crate::big::Round::Nearest);
                    return Some(if below(1) {
                        f.min(1f64.next_down())
                    } else if below(2) {
                        f.min(2f64.next_down())
                    } else {
                        f
                    });
                }
                Some(crate::compile::Written::Long) => return Some(0.0),
                None => {}
            }
            if !self.simplifier {
                return None;
            }
            let v = crate::simplify::period::exact_constant(b)?;
            (v.k == 0 && !v.q.is_int()).then(|| v.q.to_f64())
        };
        for e in [&self.expr, &self.eval] {
            e.visit(&mut |n| {
                let (u, r) = match n {
                    Expr::Bin(BinOp::Pow, u, b) if u.contains_x() => match exponent(b) {
                        Some(r) => (&**u, r),
                        None => return,
                    },
                    Expr::Call(Func::Sqrt, a) if a[0].contains_x() => (&a[0], 0.5),
                    Expr::Call(Func::Cbrt, a) if a[0].contains_x() => (&a[0], 1.0 / 3.0),
                    _ => return,
                };
                if r.is_nan() || r >= 2.0 {
                    return;
                }
                let u = canonical(u);
                match args.iter_mut().find(|(v, _)| *v == u) {
                    Some(a) => a.1 = a.1.min(r),
                    None => args.push((u, r)),
                }
            });
        }
        args
    }

    /// Where f's kinks are: the arguments u of its |u| (and a − b for
    /// each two arguments a, b of its min, max) that vary with x,
    /// canonical, each once. A kink is where one is 0. (Every two of three
    /// or more arguments: with none listed for them, min(x, 1, 2 − x) had
    /// no kink the rows knew of, and its f″, 0 off the kink at 1, was
    /// claimed 0 on the whole line; found by the replay.)
    pub fn kink_args(&self) -> Vec<Expr> {
        let mut args: Vec<Expr> = Vec::new();
        for e in [&self.expr, &self.eval] {
            e.visit(&mut |n| {
                let us: Vec<Expr> = match n {
                    Expr::Call(Func::Abs, a) if a[0].contains_x() => vec![canonical(&a[0])],
                    Expr::Call(Func::Min | Func::Max, a) => min_max_pairs(a)
                        .into_iter()
                        .map(|(i, j)| difference(&a[i], &a[j]))
                        .collect(),
                    _ => return,
                };
                for u in us {
                    if !args.contains(&u) {
                        args.push(u);
                    }
                }
            });
        }
        args
    }

    /// The exact point of the kink in the box `(l, r)`, one double either
    /// side of a double p at which a kink argument is exactly 0: with every
    /// kink argument strictly signed on each side of p (a nonzero value
    /// over the box, or an exact zero at p with a strictly signed
    /// derivative), f equals on `[l, p]` and on `[p, r]` the smooth form
    /// with each |u| (min, max) resolved by those signs, and f′'s strict
    /// sign on each side is that form's. `None` if any of it isn't
    /// decided. (f is continuous across the box: a `Kink` claim.)
    pub fn kink_at(&self, l: f64, r: f64) -> Result<Option<KinkPoint>, Stop> {
        use crate::interval::Dec;
        let p = l.next_up();
        if p.next_up() != r || !p.is_finite() {
            return Ok(None);
        }
        // (−0 is 0: the kink of |x| is at 0.)
        let p = if p == 0.0 { 0.0 } else { p };
        let around = Interval::new(l, r);
        let mut sides: Vec<(Expr, bool, bool)> = Vec::new();
        let mut at_zero = false;
        // The kink arguments of a min or max of three or more.
        let mut several: Vec<Expr> = Vec::new();
        for e in [&self.expr, &self.eval] {
            e.visit(&mut |n| {
                if let Expr::Call(Func::Min | Func::Max, a) = n
                    && a.len() > 2
                {
                    several.extend(
                        min_max_pairs(a)
                            .into_iter()
                            .map(|(i, j)| difference(&a[i], &a[j])),
                    );
                }
            });
        }
        for u in self.kink_args() {
            let s = self.ser_of(&u, around, 1)?;
            if s[0].dec >= Dec::Def && s[0].ne0() {
                let pos = s[0].gt0();
                sides.push((u, pos, pos));
                continue;
            }
            let v = self.ser_of(&u, Interval::point(p), 0)?[0];
            let zero = !v.is_empty() && v.lo() == 0.0 && v.hi() == 0.0 && v.dec >= Dec::Def;
            let d = s[1];
            if !(zero && s[0].dec >= Dec::Dac && d.dec >= Dec::Def && d.iv.is_bounded() && d.ne0())
            {
                return Ok(None);
            }
            // (A kink of a min or max of three or more arguments is a
            // kink, and what f does on its two sides is left open: the
            // independent checker has no rule for it, and the claim that
            // says it would stand unchecked.)
            if several.contains(&u) {
                return Ok(None);
            }
            at_zero = true;
            let rising = d.gt0();
            sides.push((u, !rising, rising));
        }
        if !at_zero {
            return self.singular_at(l, p, r);
        }
        for tree in [&self.expr, &self.eval] {
            let slope = |left: bool, x: Interval| -> Result<Option<bool>, Stop> {
                let e = one_sided(tree, &sides, left);
                let s = self.ser_of(&e, x, 2)?;
                let d = s[1];
                if s[0].dec < Dec::Dac || d.dec < Dec::Def || !d.iv.is_bounded() {
                    return Ok(None);
                }
                if d.ne0() {
                    return Ok(Some(d.gt0()));
                }
                // Strictly monotone up to the kink even if f′ is 0 at
                // that endpoint: x·|x| has f′=2|x|, which vanishes at 0
                // alone. A strict sign of f″ on this entire half makes
                // f′ strictly monotone; its exact endpoint zero and its
                // strict sign at the other end prove that sign on the
                // punctured half. No sampled values replace that proof.
                let dd = s[2];
                if dd.dec < Dec::Def || !dd.iv.is_bounded() || !dd.ne0() {
                    return Ok(None);
                }
                let at = self.ser_of(&e, Interval::point(p), 1)?;
                if at[0].dec < Dec::Dac
                    || at[1].dec < Dec::Def
                    || at[1].lo() != 0.0
                    || at[1].hi() != 0.0
                {
                    return Ok(None);
                }
                let far = if left { x.lo() } else { x.hi() };
                let end = self.ser_of(&e, Interval::point(far), 1)?;
                let want = if left { dd.lt0() } else { dd.gt0() };
                Ok((end[0].dec >= Dec::Dac
                    && end[1].dec >= Dec::Def
                    && end[1].ne0()
                    && end[1].gt0() == want)
                    .then_some(want))
            };
            if let (Some(left), Some(right)) = (
                slope(true, Interval::new(l, p))?,
                slope(false, Interval::new(p, r))?,
            ) {
                return Ok(Some(KinkPoint { p, left, right }));
            }
        }
        Ok(None)
    }

    /// What f′ and f″ do on each side of the one kink in the box
    /// `[l, r]` (`Claim::KinkSides`, whose statement this proves), on f's
    /// formula as typed or as evaluated (the same function across a box
    /// f is continuous on): every kink argument of that formula strictly
    /// signed over the box, or vanishing at one point of it (all of them
    /// exactly at one double, or one of them between two); f's two
    /// branches there, each smooth; and the sign of each branch's first
    /// and second derivative on its side. Or, with no corner in the box,
    /// f″ on each side of a point where it is singular. Nothing here is a
    /// value of f at a sample: `None` where any of it isn't proven.
    pub fn kink_sides(&self, l: f64, r: f64) -> Result<KinkSides, Stop> {
        let mut out = KinkSides { d1: None, d2: None };
        if !(l < r && l.is_finite() && r.is_finite()) {
            return Ok(out);
        }
        for tree in [&self.expr, &self.eval] {
            let s = self.kink_sides_of(tree, l, r)?;
            out.d1 = out.d1.or(s.d1);
            out.d2 = out.d2.or(s.d2);
            if self.expr == self.eval || (out.d1.is_some() && out.d2.is_some()) {
                break;
            }
        }
        if out.d1.is_none() || out.d2.is_none() {
            let s = self.singular_sides(l, r)?;
            out.d1 = out.d1.or(s.d1);
            out.d2 = out.d2.or(s.d2);
        }
        Ok(out)
    }

    /// [`Fun::kink_sides`] on one formula of f.
    fn kink_sides_of(&self, tree: &Expr, l: f64, r: f64) -> Result<KinkSides, Stop> {
        use super::cert::KinkSign;
        use crate::interval::Dec;
        const NONE: KinkSides = KinkSides { d1: None, d2: None };
        let around = Interval::new(l, r);
        // Each |u|'s u, and each two arguments of each min and max that
        // varies with x (the two that don't, too: which of three it picks
        // needs every pair).
        let mut args: Vec<Expr> = Vec::new();
        tree.visit(&mut |n| {
            let us: Vec<Expr> = match n {
                Expr::Call(Func::Abs, a) if a[0].contains_x() => vec![canonical(&a[0])],
                Expr::Call(Func::Min | Func::Max, a) if a.iter().any(|v| v.contains_x()) => (0..a
                    .len())
                    .flat_map(|i| (i + 1..a.len()).map(move |j| (i, j)))
                    .map(|(i, j)| difference(&a[i], &a[j]))
                    .collect(),
                _ => return,
            };
            for u in us {
                if !args.contains(&u) {
                    args.push(u);
                }
            }
        });
        // The middle double of a box of three, where a kink can be placed.
        let middle = {
            let p = l.next_up();
            (p.next_up() == r).then_some(if p == 0.0 { 0.0 } else { p })
        };
        let mut sides: Vec<(Expr, bool, bool)> = Vec::new();
        let (mut at, mut vanishing, mut between) = (None, 0usize, 0usize);
        for u in args {
            let s = self.ser_of(&u, around, 1)?;
            if s[0].dec >= Dec::Def && s[0].ne0() {
                let pos = s[0].gt0();
                sides.push((u, pos, pos));
                continue;
            }
            if s[0].is_empty() || s[0].dec < Dec::Dac {
                return Ok(NONE);
            }
            let point = |p: f64| Ok::<_, Stop>(self.ser_of(&u, Interval::point(p), 0)?[0]);
            // Exactly 0 at the middle double: its sign on each side of it
            // (a zero of any order: |x³| at 0).
            if let Some(p) = middle {
                let v = point(p)?;
                if !v.is_empty() && v.dec >= Dec::Def && v.lo() == 0.0 && v.hi() == 0.0 {
                    let strict = |k: Option<KinkSign>| match k {
                        Some(KinkSign::Pos) => Some(true),
                        Some(KinkSign::Neg) => Some(false),
                        _ => None,
                    };
                    let before = strict(self.signs_on(&u, Interval::new(l, p), Some(p))?[0]);
                    let after = strict(self.signs_on(&u, Interval::new(p, r), Some(p))?[0]);
                    let (Some(before), Some(after)) = (before, after) else {
                        return Ok(NONE);
                    };
                    at = middle;
                    vanishing += 1;
                    sides.push((u, before, after));
                    continue;
                }
            }
            // Strictly monotone, and of opposite signs at the two ends: 0
            // at one point strictly between them.
            let d = s[1];
            if !(d.dec >= Dec::Def && d.iv.is_bounded() && d.ne0()) {
                return Ok(NONE);
            }
            let rising = d.gt0();
            let (a, b) = (point(l)?, point(r)?);
            let apart = !a.is_empty()
                && !b.is_empty()
                && a.dec >= Dec::Def
                && b.dec >= Dec::Def
                && a.ne0()
                && b.ne0()
                && a.gt0() != rising
                && b.gt0() == rising;
            if !apart {
                return Ok(NONE);
            }
            between += 1;
            vanishing += 1;
            sides.push((u, !rising, rising));
        }
        // One kink: all at the one double, or one argument between two.
        if vanishing == 0 || (between > 0 && (vanishing != 1 || at.is_some())) {
            return Ok(NONE);
        }
        let (gl, gr) = (
            one_sided(tree, &sides, true),
            one_sided(tree, &sides, false),
        );
        // (Every corner resolved: a branch is a formula without one.)
        let mut corner = false;
        for g in [&gl, &gr] {
            g.visit(&mut |n| {
                corner |= match n {
                    Expr::Call(Func::Abs, a) => a[0].contains_x(),
                    Expr::Call(Func::Min | Func::Max, a) => a.iter().any(|v| v.contains_x()),
                    _ => false,
                }
            });
        }
        if corner {
            return Ok(NONE);
        }
        // With the kink at a double p: each branch up to p. Between two:
        // each over the whole box, the kink being anywhere in it.
        let (xl, xr) = match at {
            Some(p) => (Interval::new(l, p), Interval::new(p, r)),
            None => (around, around),
        };
        let (sl, sr) = (self.signs_on(&gl, xl, at)?, self.signs_on(&gr, xr, at)?);
        let mut joined = false;
        if let Some(p) = at {
            let (a, b) = (
                self.ser_of(&gl, Interval::point(p), 1)?,
                self.ser_of(&gr, Interval::point(p), 1)?,
            );
            let exact = |v: crate::interval::DecInterval| {
                !v.is_empty() && v.dec >= Dec::Def && v.lo() == v.hi() && v.lo().is_finite()
            };
            joined = a[0].dec >= Dec::Dac
                && b[0].dec >= Dec::Dac
                && exact(a[1])
                && exact(b[1])
                && a[1].lo() == b[1].lo();
        }
        let side = |k: usize| {
            sl[k].zip(sr[k]).map(|(left, right)| KinkSide {
                at,
                left,
                right,
                joined: k == 2 && joined,
            })
        };
        Ok(KinkSides {
            d1: side(1),
            d2: side(2),
        })
    }

    /// The signs of g, g′ and g″ on the box `x`, on which g is smooth; or,
    /// with `end` (one of its ends), on the box without that end. There,
    /// where a derivative isn't strictly signed over the box: g⁽ᵏ⁾, …,
    /// g⁽ᵐ⁻¹⁾ all exactly 0 at the end p and g⁽ᵐ⁾ strictly signed over the
    /// box make g⁽ᵏ⁾(t) = g⁽ᵐ⁾(ξ)·(t − p)ᵐ⁻ᵏ/(m − k)! for a ξ between
    /// (Taylor), of g⁽ᵐ⁾'s sign after p and of that sign times (−1)ᵐ⁻ᵏ
    /// before it. (x·|x| at 0: f′ = 2|x| is 0 at the kink alone.)
    fn signs_on(
        &self,
        g: &Expr,
        x: Interval,
        end: Option<f64>,
    ) -> Result<[Option<super::cert::KinkSign>; 3], Stop> {
        use super::cert::KinkSign;
        use crate::interval::Dec;
        // (How far past the derivative asked for the rule looks.)
        const PAST: usize = 3;
        let n = 2 + if end.is_some() { PAST } else { 0 };
        let s = self.ser_of(g, x, n)?;
        if s[0].is_empty() || s[0].dec < Dec::Dac {
            return Ok([None; 3]);
        }
        let valid = |c: crate::interval::DecInterval| {
            !c.is_empty() && c.dec >= Dec::Def && c.iv.is_bounded()
        };
        let at = match end {
            Some(p) => {
                let v = self.ser_of(g, Interval::point(p), n)?;
                (!v[0].is_empty() && v[0].dec >= Dec::Dac).then_some((p, v))
            }
            None => None,
        };
        let sign = |k: usize| -> Option<KinkSign> {
            // (A Taylor coefficient: g⁽ᵏ⁾/k!, of g⁽ᵏ⁾'s sign.)
            let c = s[k];
            if !valid(c) {
                return None;
            }
            if c.lo() == 0.0 && c.hi() == 0.0 {
                return Some(KinkSign::Zero);
            }
            if c.ne0() {
                return Some(if c.gt0() {
                    KinkSign::Pos
                } else {
                    KinkSign::Neg
                });
            }
            let (p, v) = at.as_ref()?;
            for m in k + 1..=n {
                let z = v[m - 1];
                if !(valid(z) && z.lo() == 0.0 && z.hi() == 0.0) || !valid(s[m]) {
                    return None;
                }
                if s[m].ne0() {
                    let before = *p == x.hi();
                    let flip = before && (m - k) % 2 == 1;
                    return Some(if s[m].gt0() != flip {
                        KinkSign::Pos
                    } else {
                        KinkSign::Neg
                    });
                }
            }
            None
        };
        Ok([sign(0), sign(1), sign(2)])
    }

    /// f′ and f″ beside a point where one is singular and f has no corner
    /// (every kink argument away from 0 over the box).
    ///
    /// On each side of the middle double p (f″; f′ so placed is
    /// [`Fun::kink_at`]'s): the base of a fractional power u^r (r < 2)
    /// exactly 0 at p, every such base away from 0 over the box or 0 at p
    /// with a strictly signed derivative; and f″'s enclosure over [l, p]
    /// and over [p, r] — where it exists — strictly signed (x^(2/3),
    /// x^(4/3) at 0).
    ///
    /// Or, placed nowhere: the derivative's enclosure over the whole box
    /// — where it exists — strictly signed, so of that sign on both sides
    /// of wherever it is singular (∛(x − 0.1): f′ > 0 either side of a
    /// vertical tangent no double holds).
    fn singular_sides(&self, l: f64, r: f64) -> Result<KinkSides, Stop> {
        use super::cert::KinkSign;
        use crate::interval::Dec;
        let mut out = KinkSides { d1: None, d2: None };
        let around = Interval::new(l, r);
        for u in self.kink_args() {
            let v = self.ser_of(&u, around, 0)?[0];
            if v.is_empty() || v.dec < Dec::Def || !v.ne0() {
                return Ok(out);
            }
        }
        // (No kink of f is in the box: the trees off the kinks are f′, f″
        // across it.)
        let Some(d) = self.derivs_off_kinks().cloned() else {
            return Ok(out);
        };
        let sign = |k: usize, x: Interval| -> Result<Option<KinkSign>, Stop> {
            let v = self.ser_of(&d[k - 1], x, 0)?[0];
            Ok(if v.is_empty() {
                None
            } else if v.gt0() {
                Some(KinkSign::Pos)
            } else if v.lt0() {
                Some(KinkSign::Neg)
            } else {
                None
            })
        };
        // On each side of the middle double.
        let p = l.next_up();
        if p.next_up() == r {
            let p = if p == 0.0 { 0.0 } else { p };
            let mut at_zero = false;
            let mut placed = true;
            for (u, _) in self.singular_args() {
                let s = self.ser_of(&u, around, 1)?;
                if s[0].dec >= Dec::Def && s[0].ne0() {
                    continue;
                }
                let v = self.ser_of(&u, Interval::point(p), 0)?[0];
                let zero = !v.is_empty() && v.lo() == 0.0 && v.hi() == 0.0 && v.dec >= Dec::Def;
                let du = s[1];
                if !(zero && du.dec >= Dec::Def && du.iv.is_bounded() && du.ne0()) {
                    placed = false;
                    break;
                }
                at_zero = true;
            }
            if placed
                && at_zero
                && let Some((left, right)) =
                    sign(2, Interval::new(l, p))?.zip(sign(2, Interval::new(p, r))?)
            {
                out.d2 = Some(KinkSide {
                    at: Some(p),
                    left,
                    right,
                    joined: false,
                });
            }
        }
        // Over the whole box.
        let whole = |s: KinkSign| KinkSide {
            at: None,
            left: s,
            right: s,
            joined: false,
        };
        out.d1 = sign(1, around)?.map(whole);
        if out.d2.is_none() {
            out.d2 = sign(2, around)?.map(whole);
        }
        Ok(out)
    }

    /// A singular point of f′ placed exactly (`kink_at`'s, for x^(2/3)):
    /// the base of a fractional power u^r (r < 1) exactly 0 at the double
    /// p, every such base strictly signed on each side of p within the box
    /// (away from 0 over it, or 0 at p with a strictly signed derivative),
    /// so f′ exists on the box but at p; and f′'s enclosure over [l, p] and
    /// over [p, r] — where it exists — strictly signed. f, continuous
    /// across the box (its `Kink` claim), is then strictly monotone on each
    /// side of p.
    fn singular_at(&self, l: f64, p: f64, r: f64) -> Result<Option<KinkPoint>, Stop> {
        use crate::interval::Dec;
        let around = Interval::new(l, r);
        let mut at_zero = false;
        for (u, rr) in self.singular_args() {
            let s = self.ser_of(&u, around, 1)?;
            if s[0].dec >= Dec::Def && s[0].ne0() {
                continue;
            }
            let v = self.ser_of(&u, Interval::point(p), 0)?[0];
            let zero = !v.is_empty() && v.lo() == 0.0 && v.hi() == 0.0 && v.dec >= Dec::Def;
            let d = s[1];
            if !(zero && d.dec >= Dec::Def && d.iv.is_bounded() && d.ne0()) {
                return Ok(None);
            }
            // (A power with r ≥ 1 is smooth enough for f′ at p.)
            at_zero |= rr < 1.0;
        }
        if !at_zero {
            return Ok(None);
        }
        let Some(d) = self.derivs().map(|d| d[0].clone()) else {
            return Ok(None);
        };
        let sign = |x: Interval| -> Result<Option<bool>, Stop> {
            let v = self.ser_of(&d, x, 0)?[0];
            Ok(if v.is_empty() {
                None
            } else if v.gt0() {
                Some(true)
            } else if v.lt0() {
                Some(false)
            } else {
                None
            })
        };
        Ok(
            match (sign(Interval::new(l, p))?, sign(Interval::new(p, r))?) {
                (Some(left), Some(right)) => Some(KinkPoint { p, left, right }),
                _ => None,
            },
        )
    }

    /// No kink of f in the box at which f is defined: each kink argument
    /// is away from 0 over it, or f with that kink's |u| set to 0 (its
    /// min, max to either side) is defined nowhere on it. The derivative
    /// trees off the kinks then stand for f′, f″ wherever these exist on
    /// the box, with no corner between.
    pub fn no_kink_in(&self, x: Interval) -> Result<bool, Stop> {
        for u in self.kink_args() {
            let v = self.ser_of(&u, x, 0)?[0];
            if !v.is_empty() && v.ne0() {
                continue;
            }
            let at = |e: &Expr| {
                e.map(&|n| match n {
                    Expr::Call(Func::Abs, a) if canonical(&a[0]) == u => Some(Expr::num(0.0)),
                    Expr::Call(Func::Min | Func::Max, a)
                        if a.len() == 2
                            && canonical(&Expr::Bin(
                                BinOp::Sub,
                                Box::new(a[0].clone()),
                                Box::new(a[1].clone()),
                            )) == u =>
                    {
                        Some(a[0].clone())
                    }
                    _ => None,
                })
            };
            // (On the formula, or on the simplified form it equals.)
            let mut undefined = false;
            for e in [&self.expr, &self.eval] {
                undefined |= self.ser_of(&at(e), x, 0)?[0].is_empty();
            }
            if !undefined {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The zero factors of `e` ([`zero_factors`]) that can vanish where f is
    /// defined: those f's side conditions keep away from 0 are left out.
    pub fn factors(&self, e: &Expr) -> Vec<Expr> {
        zero_factors(e)
            .into_iter()
            .filter(|h| !self.nonzero.contains(h))
            .collect()
    }

    /// The caller's cancel flag.
    pub fn cancelled(&self) -> bool {
        self.cancel.is_some_and(|c| c.load(Ordering::Relaxed))
    }

    /// Evaluations so far (in units of [`UNIT_WORK`]).
    pub fn evals(&self) -> u64 {
        self.evals.get()
    }

    /// Evaluations left.
    pub fn left(&self) -> u64 {
        self.budget.get().saturating_sub(self.evals.get())
    }

    /// Allows `n` more evaluations from now (one phase's share), never
    /// beyond `total` overall.
    pub fn allow(&self, n: u64) {
        let cap = self.total.get();
        self.budget.set((self.evals.get() + n).min(cap));
    }

    /// Charges one evaluation of `e` to order `n`: one unit for a tree
    /// of up to [`UNIT_WORK`] nodes·orders, more for a bigger one, as it
    /// takes longer (a sum of forty sines is thirty times x²'s work).
    fn charge(&self, e: &Expr, n: usize) -> Result<(), Stop> {
        if self.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(Stop::Cancelled);
        }
        let work = e.depth_and_size().1 * (n + 1);
        let cost = work.div_ceil(UNIT_WORK).max(1) as u64;
        let n = self.evals.get() + cost;
        if n > self.budget.get() {
            return Err(Stop::Budget);
        }
        self.evals.set(n);
        Ok(())
    }

    fn ctx(&self) -> Ctx<'_> {
        Ctx::new(self.opts)
    }

    /// `e` with its arithmetic on literals alone folded ([`Fun::folds`]),
    /// or `None` where nothing folds.
    fn folded(&self, e: &Expr) -> Option<std::sync::Arc<Expr>> {
        if let Some((_, f)) = self.folds.borrow().iter().find(|(k, _)| k == e) {
            return f.clone();
        }
        let f = crate::compile::fold_literals(e, &self.opts);
        let f = (f != *e).then(|| std::sync::Arc::new(f));
        let mut folds = self.folds.borrow_mut();
        // (Bounded: past that, folded afresh each time.)
        if folds.len() < 64 {
            folds.push((e.clone(), f.clone()));
        }
        f
    }

    /// Taylor coefficients of `e` over the box; where they leave e
    /// possibly undefined there, merged with those of `e` with its literal
    /// arithmetic folded exactly, if that changes it (off, f = x +
    /// 10⁻¹⁶/(1.0000000000000001 − 1) is x + 1, though its divisor
    /// enclosed literal by literal holds 0). Each encloses f's. (Not
    /// otherwise: an enclosure as tight as the exact value can't be shown
    /// any tighter at an open end, and the replay would leave a bound
    /// there unconfirmed.)
    fn taylor_of(&self, e: &Expr, x: Interval, n: usize) -> Series {
        let s = taylor(e, x, n, &self.ctx());
        if s[0].is_empty() || s[0].dec >= crate::interval::Dec::Def {
            return s;
        }
        match self.folded(e) {
            Some(f) => merge(s, &taylor(&f, x, n, &self.ctx())),
            None => s,
        }
    }

    /// Taylor coefficients of `e` (a sub-tree of `expr`) over the box, up
    /// to order `n`.
    pub fn ser_of(&self, e: &Expr, x: Interval, n: usize) -> Result<Series, Stop> {
        self.charge(e, n)?;
        let s = self.taylor_of(e, x, n);
        // f's simplified form, and the formula as written: each encloses
        // f's coefficients where the formula is defined throughout the box,
        // and each is tighter in places (the simplified form cancels, the
        // formula keeps a shift (x − a)/s exact where the simplifier
        // distributed it).
        // (Only where the simplified form leaves something to decide: a
        // coefficient not valid, unbounded, or holding 0.)
        if std::ptr::eq(e, &self.eval) && self.eval != self.expr && !s.iter().all(decided) {
            // (A second evaluation, charged as one.)
            self.charge(&self.expr, n)?;
            let t = self.taylor_of(&self.expr, x, n);
            return Ok(merge(s, &t));
        }
        Ok(s)
    }

    /// Taylor coefficients of f over the box (from the evaluated tree).
    pub fn ser(&self, x: Interval, n: usize) -> Result<Series, Stop> {
        self.ser_of(&self.eval, x, n)
    }

    /// f over the box.
    pub fn val(&self, x: Interval) -> Result<DecInterval, Stop> {
        Ok(self.ser(x, 0)?[0])
    }

    /// f over a narrow box, tightened by the mean value form: f(m) +
    /// f′(box)·(box − m) for its middle m, where f′ is valid there (at a
    /// turn f′ is near 0, so f's value is known far better than its
    /// enclosure over the box shows).
    pub fn val_tight(&self, x: Interval) -> Result<DecInterval, Stop> {
        let s = self.ser(x, 1)?;
        let v = s[0];
        if !(x.lo() < x.hi() && x.is_bounded() && usable(&s, 1) && s[1].iv.is_bounded()) {
            return Ok(v);
        }
        let m = x.lo() / 2.0 + x.hi() / 2.0;
        let vm = self.val(Interval::point(m))?;
        if vm.is_empty() || !vm.iv.is_bounded() {
            return Ok(v);
        }
        let dx = x - Interval::point(m);
        let mv = vm.iv + s[1].iv * dx;
        let iv = v.iv.intersect(mv);
        if iv.is_empty() {
            return Ok(v);
        }
        Ok(DecInterval { iv, ..v })
    }
}

/// Taylor coefficients of f over a box from its simplified form (`s`) and
/// from the formula (`t`), equal wherever the formula is defined: up to the
/// order the formula's are valid to (its value defined throughout the box,
/// its derivatives' too, with f continuous there), each coefficient is the
/// intersection of the two, or the formula's alone where the simplified
/// form's isn't valid. The formula defined throughout the box means the two
/// trees are the same function there, so whatever either proves holds.
/// A coefficient that leaves nothing to decide: valid, bounded and not 0.
fn decided(c: &DecInterval) -> bool {
    !c.is_empty() && c.dec >= crate::interval::Dec::Def && c.iv.is_bounded() && c.ne0()
}

fn merge(mut s: Series, t: &Series) -> Series {
    use crate::interval::Dec;
    let valid = |c: &DecInterval| !c.is_empty() && c.dec >= Dec::Def;
    let Some(t0) = t.first().filter(|c| valid(c)) else {
        return s;
    };
    // The orders the formula's coefficients are good to.
    let upto = if t0.dec >= Dec::Dac {
        t.iter().skip(1).take_while(|c| valid(c)).count()
    } else {
        0
    };
    let s_cont = s.first().is_some_and(|c| valid(c) && c.dec >= Dec::Dac);
    for (j, c) in s.iter_mut().enumerate().take(upto + 1) {
        let tj = t[j];
        // The simplified form's coefficient j stands only with its value
        // valid (and, past order 0, continuous).
        let ok = valid(c) && (j == 0 || s_cont);
        if !ok {
            *c = tj;
            continue;
        }
        let iv = c.iv.intersect(tj.iv);
        if iv.is_empty() {
            continue;
        }
        *c = DecInterval {
            iv,
            dec: c.dec.max(tj.dec),
            pos: c.pos || tj.pos,
            neg: c.neg || tj.neg,
        };
    }
    s
}

/// f′ and f″ of a rational function from its exact form (see
/// [`Fun::derivs`]); `None` if f isn't one or the arithmetic would overflow.
pub fn rational_derivs(e: &Expr) -> Option<[Expr; 2]> {
    use crate::simplify::q::Q;
    use crate::simplify::rational::Poly;
    let rf = crate::simplify::rational_form(e)?;
    let (n, d) = (rf.reduced.num, rf.reduced.den);
    let deriv = |p: &Poly| -> Option<Poly> {
        let mut out = Poly::zero();
        for (i, c) in p.coefficients().iter().enumerate().skip(1) {
            let term = Poly::x()
                .pow(i as u32 - 1)?
                .scale(c.mul(Q::int(i as i128))?)?;
            out = out.add(&term)?;
        }
        Some(out)
    };
    let (n1, d1) = (deriv(&n)?, deriv(&d)?);
    let p = n1.mul(&d)?.sub(&n.mul(&d1)?)?;
    let p2 = deriv(&p)?.mul(&d)?.sub(&p.mul(&d1)?.scale(Q::int(2))?)?;
    let over = |num: &Poly, k: f64| -> Expr {
        if d.degree() == Some(0) {
            // D is monic: 1.
            num.to_expr()
        } else {
            Expr::bin(
                BinOp::Div,
                num.to_expr(),
                Expr::bin(BinOp::Pow, d.to_expr(), Expr::num(k)),
            )
        }
    };
    // Every coefficient written as doubles that hold it exactly (q_expr
    // rounds a numerator or denominator past 2⁵³: 1 + 10⁻³⁰ would read 1).
    let exact = |q: &Poly| {
        q.coefficients()
            .iter()
            .all(|c| c.numer().unsigned_abs() <= 1 << 53 && c.denom() <= 1 << 53)
    };
    if !(exact(&p) && exact(&p2) && exact(&d)) {
        return None;
    }
    Some([canonical(&over(&p, 2.0)), canonical(&over(&p2, 3.0))])
}

/// The numerators of f, f′ and f″ in lowest terms, for a rational f: on
/// f's domain each vanishes exactly where its function does (what is left
/// of the denominator vanishes only at f's poles).
pub fn rational_numerators(e: &Expr) -> Option<[Expr; 3]> {
    use crate::simplify::q::Q;
    use crate::simplify::rational::Poly;
    let rf = crate::simplify::rational_form(e)?;
    let (n, d) = (rf.reduced.num, rf.reduced.den);
    let deriv = |p: &Poly| -> Option<Poly> {
        let mut out = Poly::zero();
        for (i, c) in p.coefficients().iter().enumerate().skip(1) {
            let term = Poly::x()
                .pow(i as u32 - 1)?
                .scale(c.mul(Q::int(i as i128))?)?;
            out = out.add(&term)?;
        }
        Some(out)
    };
    // num / gcd(num, den).
    let lowest = |num: &Poly, den: &Poly| -> Option<Poly> {
        if num.is_zero() {
            return Some(Poly::zero());
        }
        let g = num.gcd(den)?;
        Some(num.divmod(&g)?.0)
    };
    let (n1, d1) = (deriv(&n)?, deriv(&d)?);
    let p = n1.mul(&d)?.sub(&n.mul(&d1)?)?;
    let p2 = deriv(&p)?.mul(&d)?.sub(&p.mul(&d1)?.scale(Q::int(2))?)?;
    let (d2, d3) = (d.pow(2)?, d.pow(3)?);
    let ps = [n, lowest(&p, &d2)?, lowest(&p2, &d3)?];
    // (Only with every coefficient a double's exactly: see rational_derivs.)
    let exact = ps.iter().all(|q| {
        q.coefficients()
            .iter()
            .all(|c| c.numer().unsigned_abs() <= 1 << 53 && c.denom() <= 1 << 53)
    });
    exact.then(|| ps.map(|q| canonical(&q.to_expr())))
}

/// `e` with each constant exponent that is a typed decimal (`x^0.9`)
/// written as the exact fraction it is (`x^(9/10)`). The two agree wherever
/// the decimal power is defined (base ≥ 0, or an integer exponent).
fn exact_exponents(e: &Expr) -> Expr {
    e.map(&|n| match n {
        Expr::Bin(BinOp::Pow, a, b)
            if !b.contains_x()
                && crate::compile::written(b, crate::compile::Reading::Typed).is_none() =>
        {
            let q = match &**b {
                Expr::Num(v, lit) => lit.q(*v)?,
                Expr::Neg(inner) => match &**inner {
                    Expr::Num(v, lit) => lit.q(*v)?.neg()?,
                    _ => return None,
                },
                _ => return None,
            };
            Some(Expr::bin(
                BinOp::Pow,
                exact_exponents(a),
                crate::simplify::rational::q_expr(q),
            ))
        }
        _ => None,
    })
}

/// Every constant of `e` is an integer or a typed literal (enclosed as the
/// decimal typed), never a value computed in floating point.
fn sound_constants(e: &Expr) -> bool {
    let mut ok = true;
    e.visit(&mut |n| {
        if let Expr::Num(v, lit) = n
            && lit.q(*v).is_none()
        {
            ok = false;
        }
    });
    ok
}

/// The node budget of a symbolic derivative.
const DIFF_NODES: usize = 4096;

/// What f′ (`d1`) and f″ (`d2`) do on each side of the one kink in a box
/// (see [`Fun::kink_sides`]), each where proven.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KinkSides {
    pub d1: Option<KinkSide>,
    pub d2: Option<KinkSide>,
}

/// What one derivative does on each side of a kink (a
/// `Claim::KinkSides`): where the kink is, when at a double; the
/// derivative's sign before it and after it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KinkSide {
    pub at: Option<f64>,
    pub left: super::cert::KinkSign,
    pub right: super::cert::KinkSign,
    /// (f″ only.) f′ is continuous across the kink: opposite strict signs
    /// are an inflection there.
    pub joined: bool,
}

/// A kink placed exactly (see [`Fun::kink_at`]): at the double `p`, with
/// f′ strictly positive (`true`) or negative on `[p⁻, p]` (`left`) and on
/// `[p, p⁺]` (`right`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KinkPoint {
    pub p: f64,
    pub left: bool,
    pub right: bool,
}

/// `e` on one side of a kink: each |u| as u or −u and each min(a, b),
/// max(a, b) as the argument it picks, by the signs `sides` gives each
/// kink argument (canonical u, or a − b) on that side; the rest as is.
pub fn one_sided(e: &Expr, sides: &[(Expr, bool, bool)], left: bool) -> Expr {
    let sign_of = |u: &Expr| {
        sides
            .iter()
            .find(|(v, ..)| v == u)
            .map(|(_, l, r)| if left { *l } else { *r })
    };
    e.map(&|n| match n {
        Expr::Call(Func::Abs, a) if a[0].contains_x() => {
            let inner = one_sided(&a[0], sides, left);
            let pos = sign_of(&canonical(&a[0]))?;
            Some(if pos {
                inner
            } else {
                Expr::Neg(Box::new(inner))
            })
        }
        Expr::Call(f @ (Func::Min | Func::Max), a)
            if a.len() == 2 && a.iter().any(|v| v.contains_x()) =>
        {
            let d = difference(&a[0], &a[1]);
            // a − b > 0: max picks a, min picks b.
            let a_bigger = sign_of(&d)?;
            let pick = if a_bigger == (*f == Func::Max) {
                &a[0]
            } else {
                &a[1]
            };
            Some(one_sided(pick, sides, left))
        }
        // Of three or more: the argument that every difference it is in
        // puts first (each of them known on this side, or it stays).
        Expr::Call(f @ (Func::Min | Func::Max), a) if a.len() > 2 => {
            let first = |k: usize| {
                (0..a.len()).filter(|&j| j != k).all(|j| {
                    let (i, j, k_first) = if k < j { (k, j, true) } else { (j, k, false) };
                    // a_i − a_j > 0: max puts a_i first, min a_j.
                    sign_of(&difference(&a[i], &a[j]))
                        .is_some_and(|i_bigger| (i_bigger == (*f == Func::Max)) == k_first)
                })
            };
            let k = (0..a.len()).find(|&k| first(k))?;
            Some(one_sided(&a[k], sides, left))
        }
        _ => None,
    })
}

/// a − b, canonical: the kink argument of min(a, b), max(a, b).
fn difference(a: &Expr, b: &Expr) -> Expr {
    canonical(&Expr::Bin(
        BinOp::Sub,
        Box::new(a.clone()),
        Box::new(b.clone()),
    ))
}

/// Each two of a min's or max's arguments of which one varies with x.
fn min_max_pairs(a: &[Expr]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for i in 0..a.len() {
        for j in i + 1..a.len() {
            if a[i].contains_x() || a[j].contains_x() {
                out.push((i, j));
            }
        }
    }
    out
}

/// E-graph nodes for each of an analysis's simplifier runs (several per
/// analysis, so fewer than the simplifier's default): a count, not a time,
/// so the same function always gets the same answer.
pub const CERTIFY_NODES: usize = 20_000;

/// f′ and f″ of `e` by symbolic differentiation, each simplified (with
/// `settings`, when given) so that what cancels in them cancels exactly.
pub fn symbolic_derivs(
    e: &Expr,
    unit: crate::functions::TrigUnit,
    settings: Option<&crate::simplify::Settings<'_>>,
) -> Option<[Expr; 2]> {
    let simp = |d: Expr| -> Expr {
        match settings.and_then(|s| crate::simplify::simplify(&d, s).ok()) {
            Some(s) if s.changed => s.expr,
            _ => d,
        }
    };
    let d = |e: &Expr| -> Option<Expr> {
        Some(exact_unit_factor(
            crate::diff::derivative_bounded(e, unit, DIFF_NODES)?,
            e,
            unit,
        ))
    };
    let d1 = simp(d(e)?);
    let d2 = simp(d(&d1)?);
    Some([canonical(&d1), canonical(&d2)])
}

/// `d`, the derivative of `e` in degrees or grads, with the angle unit's
/// factor written exactly: π/180 (π/200), where the differentiator writes
/// the double nearest it, a number only near its own (`Lit::Near`) that
/// [`sound_constants`] refuses. Each `Near` number of `d` with that double
/// is the factor (the differentiator writes no other number so) unless
/// `e` holds one already, when `d` is left as it is. (Every derivative
/// tree in degrees was refused so: atan(ln|x|) in degrees had no f′ to
/// clear the gaps beside its excluded 0, where radians had. Round 20.)
fn exact_unit_factor(d: Expr, e: &Expr, unit: crate::functions::TrigUnit) -> Expr {
    use crate::ast::{Constant, Lit};
    use crate::functions::TrigUnit;
    let half_turn = match unit {
        TrigUnit::Radians => return d,
        TrigUnit::Degrees => 180.0,
        TrigUnit::Grads => 200.0,
    };
    let k = unit.to_radians_factor();
    let is_factor = |n: &Expr| matches!(n, Expr::Num(v, Lit::Near) if *v == k);
    let mut held = false;
    e.visit(&mut |n| held |= is_factor(n));
    if held {
        return d;
    }
    d.map(&|n| {
        is_factor(n).then(|| {
            Expr::bin(
                BinOp::Div,
                Expr::Const(Constant::Pi),
                Expr::Num(half_turn, Lit::Exact),
            )
        })
    })
}

/// Factors whose zeros, on the domain of `e`, are the zeros of `e`: the
/// argument of an outer function that vanishes only where its argument
/// does (powers, roots, |·|, atan, sinh, …), each factor of a product, the
/// numerator of a quotient (its divisor is never 0 where `e` is defined),
/// u − 1 for ln u; nothing for a function that never vanishes (exp, cosh,
/// sec, …). `e` itself when nothing applies.
pub fn zero_factors(e: &Expr) -> Vec<Expr> {
    use crate::ast::Func;
    let mut out = Vec::new();
    fn go(e: &Expr, out: &mut Vec<Expr>) {
        match e {
            Expr::Num(v, _) if *v != 0.0 => {}
            Expr::Const(_) => {}
            Expr::Neg(a) => go(a, out),
            Expr::Bin(BinOp::Mul, a, b) => {
                go(a, out);
                go(b, out);
            }
            Expr::Bin(BinOp::Div, a, _) => go(a, out),
            Expr::Bin(BinOp::Pow, a, _) => go(a, out),
            Expr::Call(f, args) if args.len() == 1 => match f {
                Func::Sqrt
                | Func::Cbrt
                | Func::Abs
                | Func::Sign
                | Func::Atan
                | Func::Asin
                | Func::Sinh
                | Func::Tanh
                | Func::Asinh
                | Func::Atanh => go(&args[0], out),
                Func::Exp | Func::Cosh | Func::Sech | Func::Sec | Func::Csc | Func::Csch => {}
                // tan u = 0 where sin u = 0, cot u = 0 where cos u = 0
                // (both finite there): factors smooth across the poles.
                Func::Tan => out.push(Expr::call1(Func::Sin, args[0].clone())),
                Func::Cot => out.push(Expr::call1(Func::Cos, args[0].clone())),
                Func::Ln | Func::Log => {
                    out.push(Expr::bin(BinOp::Sub, args[0].clone(), Expr::num(1.0)))
                }
                // log_b u = 0 where u = 1 (b a valid base wherever defined).
                Func::LogBase if args.len() == 2 => {
                    out.push(Expr::bin(BinOp::Sub, args[1].clone(), Expr::num(1.0)))
                }
                _ => out.push(e.clone()),
            },
            Expr::Call(Func::Root, args) => go(&args[0], out),
            // A sum of a positive term and a nonnegative one is never 0.
            Expr::Bin(BinOp::Add, a, b)
                if matches!(
                    (sign_of(a), sign_of(b)),
                    (Some(true), Some(_)) | (Some(_), Some(true))
                ) => {}
            // a·c ± b·c = c·(a ± b): the shared factors, then the rest.
            Expr::Bin(op @ (BinOp::Add | BinOp::Sub), a, b) => {
                let (sa, fa) = product(a);
                let (sb, fb) = product(b);
                // As multisets: x·x·eˣ and x·eˣ share x·eˣ once.
                let mut pool = fb.clone();
                let mut common: Vec<Expr> = Vec::new();
                for f in &fa {
                    if f.contains_x()
                        && let Some(i) = pool.iter().position(|g| g == f)
                    {
                        common.push(pool.remove(i));
                    }
                }
                if common.is_empty() {
                    out.push(e.clone());
                    return;
                }
                let rest = |fs: &[Expr], neg: bool| -> Expr {
                    let mut left: Vec<Expr> = fs.to_vec();
                    for c in &common {
                        if let Some(i) = left.iter().position(|f| f == c) {
                            left.remove(i);
                        }
                    }
                    let p = left
                        .into_iter()
                        .reduce(|x, y| Expr::bin(BinOp::Mul, x, y))
                        .unwrap_or(Expr::num(1.0));
                    if neg { Expr::Neg(Box::new(p)) } else { p }
                };
                for c in &common {
                    go(c, out);
                }
                out.push(Expr::bin(*op, rest(&fa, sa), rest(&fb, sb)));
            }
            _ => out.push(e.clone()),
        }
    }
    /// From the tree alone, where e is defined: `Some(true)` e > 0,
    /// `Some(false)` e ≥ 0.
    fn sign_of(e: &Expr) -> Option<bool> {
        let never_zero = |e: &Expr| {
            let mut fs = Vec::new();
            go(e, &mut fs);
            fs.is_empty()
        };
        match e {
            Expr::Num(v, _) => (*v >= 0.0).then_some(*v > 0.0),
            Expr::Const(_) => Some(true),
            // (At face value: an even power is ≥ 0, and so is a power of a
            // base ≥ 0 if the exponent is in fact no integer.)
            Expr::Bin(BinOp::Pow, a, p) => {
                match crate::compile::syntactic_rational(p, crate::compile::Reading::FaceValue) {
                    Some((n, 1)) if n % 2 == 0 => Some(never_zero(a)),
                    _ => sign_of(a).map(|pos| pos && never_zero(e)),
                }
            }
            Expr::Bin(BinOp::Mul | BinOp::Div, a, b) => {
                let (sa, sb) = (sign_of(a)?, sign_of(b)?);
                Some(sa && sb)
            }
            Expr::Bin(BinOp::Add, a, b) => {
                let (sa, sb) = (sign_of(a)?, sign_of(b)?);
                Some(sa || sb)
            }
            Expr::Call(f, args) if args.len() == 1 => match f {
                Func::Exp | Func::Cosh | Func::Sech => Some(true),
                Func::Abs | Func::Sqrt => Some(never_zero(&args[0])),
                _ => None,
            },
            _ => None,
        }
    }

    /// e = ±Π factors (negations pulled out): (negative, factors).
    fn product(e: &Expr) -> (bool, Vec<Expr>) {
        match e {
            Expr::Neg(a) => {
                let (s, f) = product(a);
                (!s, f)
            }
            Expr::Bin(BinOp::Mul, a, b) => {
                let (sa, mut fa) = product(a);
                let (sb, fb) = product(b);
                fa.extend(fb);
                (sa != sb, fa)
            }
            _ => (false, vec![e.clone()]),
        }
    }
    go(e, &mut out);
    out.into_iter().map(|g| canonical(&g)).collect()
}

/// The values of the expression's x-free sub-trees: where its features
/// tend to sit (the centre of `(x − c)²`, the scale of `x/s`).
pub fn centres(f: &Fun<'_>) -> Vec<f64> {
    let mut out = Vec::new();
    fn walk(e: &Expr, f: &Fun<'_>, out: &mut Vec<f64>) {
        if !e.contains_x() && !e.contains_y() {
            if let Ok(v) = f.ser_of(e, Interval::point(0.0), 0) {
                let m = v[0].iv;
                if !m.is_empty() && m.is_bounded() {
                    let c = m.mid();
                    if c.is_finite() {
                        out.push(c);
                    }
                }
            }
            return;
        }
        match e {
            Expr::Neg(a) | Expr::Degrees(a) => walk(a, f, out),
            Expr::Bin(_, a, b) => {
                walk(a, f, out);
                walk(b, f, out);
            }
            Expr::Call(_, args) => {
                for a in args {
                    walk(a, f, out);
                }
            }
            _ => {}
        }
    }
    walk(&f.expr, f, &mut out);
    out
}

/// The Taylor coefficients up to `k` are usable on the box: f is defined
/// and continuous there and each coefficient comes from defined
/// operations. Unlike `interval::derivs_valid` an enclosure may be
/// unbounded (on a tail f′ = 2x is, and e^x overflows past 709.8): a
/// derivative that doesn't exist (a kink, a vertical tangent, a divisor
/// reaching 0) comes out undefined (*trv*) from the interval core, never
/// as a defined unbounded value.
pub fn usable(s: &crate::interval::Series, k: usize) -> bool {
    use crate::interval::Dec;
    s[0].dec >= Dec::Dac
        && s[1..=k.min(s.len() - 1)]
            .iter()
            .all(|c| !c.is_empty() && c.dec >= Dec::Def)
}

/// The spacing of doubles at `x` (∞ for ±∞).
pub fn ulp(x: f64) -> f64 {
    let a = x.abs();
    if !a.is_finite() {
        return f64::INFINITY;
    }
    a.next_up() - a
}

/// Where to split `[lo, hi]`: the middle, or geometrically across many
/// magnitudes and towards an infinite end; `None` when no double lies
/// strictly inside.
pub fn split(lo: f64, hi: f64) -> Option<f64> {
    let m = if lo == f64::NEG_INFINITY && hi == f64::INFINITY {
        0.0
    } else if hi == f64::INFINITY {
        if lo >= 1.0 {
            lo * 4.0
        } else {
            lo.max(0.0) + 1.0
        }
    } else if lo == f64::NEG_INFINITY {
        if hi <= -1.0 {
            hi * 4.0
        } else {
            hi.min(0.0) - 1.0
        }
    } else if lo > 0.0 && hi / lo > 16.0 {
        lo.max(f64::MIN_POSITIVE).sqrt() * hi.sqrt()
    } else if hi < 0.0 && lo / hi > 16.0 {
        -((-hi).max(f64::MIN_POSITIVE).sqrt() * (-lo).sqrt())
    } else if lo < 0.0 && hi > 0.0 {
        // Across 0: split at 0 first (features and exact zeros sit there).
        0.0
    } else {
        lo / 2.0 + hi / 2.0
    };
    let m = if m.is_finite() {
        m
    } else {
        lo / 2.0 + hi / 2.0
    };
    (m > lo && m < hi).then_some(m)
}

/// True when the box is too narrow to split usefully (a few doubles).
pub fn atomic(lo: f64, hi: f64) -> bool {
    if !(lo.is_finite() && hi.is_finite()) {
        return false;
    }
    hi - lo <= 4.0 * ulp(lo.abs().max(hi.abs())).max(f64::from_bits(1))
}

/// How far out the analysis looks before calling a tail undecidable: past
/// 10¹⁵ doubles are integers apart and a periodic function's features
/// can't be told apart.
pub const REACH: f64 = 1e15;
