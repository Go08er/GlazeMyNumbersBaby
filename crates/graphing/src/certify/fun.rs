//! The function under analysis: its canonical tree, its literals, the
//! angle unit, and a budget of interval evaluations.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::ast::{BinOp, Expr, Func};
use crate::compile::CompileOptions;
use crate::interval::{Ctx, DecInterval, Interval, Literals, Series, taylor};
use crate::simplify::q::Q;

/// Why certification stopped early.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The evaluation budget ran out.
    Budget,
    /// The caller cancelled.
    Cancelled,
}

/// The tree every claim refers to: the parser's, with `a·a` written `a²`
/// (an interval product of a box with itself loses the correlation:
/// `(x−c)·(x−c)` over a box around c straddles 0, `(x−c)²` doesn't).
pub fn canonical(e: &Expr) -> Expr {
    match e {
        Expr::Num(_) | Expr::Const(_) | Expr::X | Expr::Y | Expr::Var(_) => e.clone(),
        Expr::Neg(a) => Expr::Neg(Box::new(canonical(a))),
        Expr::Degrees(a) => Expr::Degrees(Box::new(canonical(a))),
        Expr::Bin(op, a, b) => {
            let (a, b) = (canonical(a), canonical(b));
            if *op == BinOp::Mul && a == b {
                Expr::bin(BinOp::Pow, a, Expr::Num(2.0))
            } else {
                Expr::bin(*op, a, b)
            }
        }
        Expr::Call(f, args) => Expr::Call(*f, args.iter().map(canonical).collect()),
    }
}

/// `e` as a·x + b with a, b exact rationals, if it is one (sliders and π
/// aside).
fn affine(e: &Expr, lits: &crate::simplify::ExactLiterals) -> Option<(Q, Q)> {
    Some(match e {
        Expr::X => (Q::ONE, Q::ZERO),
        Expr::Num(v) => (Q::ZERO, lits.exact(*v)?),
        Expr::Neg(a) => {
            let (a, b) = affine(a, lits)?;
            (a.neg()?, b.neg()?)
        }
        Expr::Bin(op @ (BinOp::Add | BinOp::Sub), l, r) => {
            let ((a, b), (c, d)) = (affine(l, lits)?, affine(r, lits)?);
            if *op == BinOp::Add {
                (a.add(c)?, b.add(d)?)
            } else {
                (a.sub(c)?, b.sub(d)?)
            }
        }
        Expr::Bin(BinOp::Mul, l, r) => {
            let ((a, b), (c, d)) = (affine(l, lits)?, affine(r, lits)?);
            match (a.is_zero(), c.is_zero()) {
                (true, _) => (c.mul(b)?, d.mul(b)?),
                (_, true) => (a.mul(d)?, b.mul(d)?),
                _ => return None,
            }
        }
        Expr::Bin(BinOp::Div, l, r) => {
            let ((a, b), (c, d)) = (affine(l, lits)?, affine(r, lits)?);
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
    (q.is_int() && q.signum() >= 0 && q.numer() <= 1 << 53).then(|| Expr::Num(q.numer() as f64))
}

/// `e` with each part a·x + b in x (exact rationals a ≠ 0 and b, with
/// c = −b/a an integer) written a·(x − c): x − c is exact near c, where
/// a·x + b cancels. (x/1000 − 1 at x = 1000 + 2⁻⁴³, evaluated as written,
/// is [0, 2⁻⁵²]; as (x − 1000)/1000, 2⁻⁴³/1000 to an ulp.) The same real
/// function, so f's evaluated tree may be written so.
pub fn recentre(e: &Expr, lits: &crate::simplify::ExactLiterals) -> Expr {
    if !matches!(e, Expr::X)
        && let Some((a, b)) = affine(e, lits)
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
    match e {
        Expr::Neg(a) => Expr::Neg(Box::new(recentre(a, lits))),
        Expr::Degrees(a) => Expr::Degrees(Box::new(recentre(a, lits))),
        Expr::Bin(op, a, b) => Expr::bin(*op, recentre(a, lits), recentre(b, lits)),
        Expr::Call(f, args) => Expr::Call(*f, args.iter().map(|a| recentre(a, lits)).collect()),
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

/// The function, its literals and its budget.
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
    pub lits: &'a Literals,
    /// The exact literals, for the simplifier's proofs (none: no
    /// simplifier).
    pub exact: Option<&'a crate::simplify::ExactLiterals>,
    pub opts: CompileOptions<'a>,
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
        lits: &'a Literals,
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
            lits,
            exact: None,
            opts,
            evals: Cell::new(0),
            budget: Cell::new(budget),
            total: Cell::new(budget),
            cancel,
        }
    }

    /// The simplifier's settings for f, when it has its literals.
    pub fn settings(&self) -> Option<crate::simplify::Settings<'a>> {
        let mut s = crate::simplify::Settings::new(&self.opts, self.exact?);
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
        let exact = self.exact?;
        rational_derivs(&self.expr, exact).or_else(|| {
            if !symbolic {
                return None;
            }
            let s = self.settings()?;
            // A typed decimal exponent as the exact fraction it is (x^0.9
            // is x^(9/10) where x ≥ 0, the only x it is evaluated at), so
            // the derivative's exponents stay exact; and every constant in
            // the trees typed or an integer (none computed in floating
            // point).
            let e = exact_exponents(&self.eval, exact);
            let d = symbolic_derivs(&e, self.opts.trig_unit, Some(&s))?;
            d.iter().all(|t| sound_constants(t, exact)).then_some(d)
        })
    }

    /// The derivative trees, if already built (either kind).
    pub fn derivs_built(&self) -> Option<&[Expr; 2]> {
        self.derivs
            .get()
            .and_then(|d| d.as_ref())
            .or_else(|| self.derivs_kinked.get().and_then(|d| d.as_ref()))
    }

    /// Where f's kinks are: the arguments u of its |u| (and a − b of its
    /// min(a, b), max(a, b)) that vary with x, canonical, each once. A kink
    /// is where one is 0.
    pub fn kink_args(&self) -> Vec<Expr> {
        let mut args: Vec<Expr> = Vec::new();
        for e in [&self.expr, &self.eval] {
            e.visit(&mut |n| {
                let u = match n {
                    Expr::Call(Func::Abs, a) if a[0].contains_x() => canonical(&a[0]),
                    Expr::Call(Func::Min | Func::Max, a)
                        if a.len() == 2 && a.iter().any(|v| v.contains_x()) =>
                    {
                        canonical(&Expr::Bin(
                            BinOp::Sub,
                            Box::new(a[0].clone()),
                            Box::new(a[1].clone()),
                        ))
                    }
                    _ => return,
                };
                if !args.contains(&u) {
                    args.push(u);
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
            at_zero = true;
            let rising = d.gt0();
            sides.push((u, !rising, rising));
        }
        if !at_zero {
            return Ok(None);
        }
        for tree in [&self.expr, &self.eval] {
            let slope = |left: bool, x: Interval| -> Result<Option<bool>, Stop> {
                let e = one_sided(tree, &sides, left);
                let s = self.ser_of(&e, x, 1)?;
                let d = s[1];
                Ok(
                    (s[0].dec >= Dec::Dac && d.dec >= Dec::Def && d.iv.is_bounded() && d.ne0())
                        .then(|| d.gt0()),
                )
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
                    Expr::Call(Func::Abs, a) if canonical(&a[0]) == u => Some(Expr::Num(0.0)),
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

    /// Evaluations so far.
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

    fn charge(&self) -> Result<(), Stop> {
        if self.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(Stop::Cancelled);
        }
        let n = self.evals.get() + 1;
        if n > self.budget.get() {
            return Err(Stop::Budget);
        }
        self.evals.set(n);
        Ok(())
    }

    fn ctx(&self) -> Ctx<'_> {
        Ctx::new(self.opts, self.lits)
    }

    /// Taylor coefficients of `e` (a sub-tree of `expr`) over the box, up
    /// to order `n`.
    pub fn ser_of(&self, e: &Expr, x: Interval, n: usize) -> Result<Series, Stop> {
        self.charge()?;
        let s = taylor(e, x, n, &self.ctx());
        // f's simplified form, and the formula as written: each encloses
        // f's coefficients where the formula is defined throughout the box,
        // and each is tighter in places (the simplified form cancels, the
        // formula keeps a shift (x − a)/s exact where the simplifier
        // distributed it).
        if std::ptr::eq(e, &self.eval) && self.eval != self.expr {
            let t = taylor(&self.expr, x, n, &self.ctx());
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
pub fn rational_derivs(e: &Expr, lits: &crate::simplify::ExactLiterals) -> Option<[Expr; 2]> {
    use crate::simplify::q::Q;
    use crate::simplify::rational::Poly;
    let rf = crate::simplify::rational_form(e, lits)?;
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
                Expr::bin(BinOp::Pow, d.to_expr(), Expr::Num(k)),
            )
        }
    };
    Some([canonical(&over(&p, 2.0)), canonical(&over(&p2, 3.0))])
}

/// The numerators of f, f′ and f″ in lowest terms, for a rational f: on
/// f's domain each vanishes exactly where its function does (what is left
/// of the denominator vanishes only at f's poles).
pub fn rational_numerators(e: &Expr, lits: &crate::simplify::ExactLiterals) -> Option<[Expr; 3]> {
    use crate::simplify::q::Q;
    use crate::simplify::rational::Poly;
    let rf = crate::simplify::rational_form(e, lits)?;
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
    Some([
        canonical(&n.to_expr()),
        canonical(&lowest(&p, &d2)?.to_expr()),
        canonical(&lowest(&p2, &d3)?.to_expr()),
    ])
}

/// `e` with each constant exponent that is a typed decimal (`x^0.9`)
/// written as the exact fraction it is (`x^(9/10)`). The two agree wherever
/// the decimal power is defined (base ≥ 0, or an integer exponent).
fn exact_exponents(e: &Expr, exact: &crate::simplify::ExactLiterals) -> Expr {
    e.map(&|n| match n {
        Expr::Bin(BinOp::Pow, a, b)
            if !b.contains_x() && crate::compile::syntactic_rational(b).is_none() =>
        {
            let v = match &**b {
                Expr::Num(v) => *v,
                Expr::Neg(inner) => match &**inner {
                    Expr::Num(v) => -*v,
                    _ => return None,
                },
                _ => return None,
            };
            let q = exact.exact(v)?;
            Some(Expr::bin(
                BinOp::Pow,
                exact_exponents(a, exact),
                crate::simplify::rational::q_expr(q),
            ))
        }
        _ => None,
    })
}

/// Every constant of `e` is an integer or a typed literal (enclosed as the
/// decimal typed), never a value computed in floating point.
fn sound_constants(e: &Expr, exact: &crate::simplify::ExactLiterals) -> bool {
    let mut ok = true;
    e.visit(&mut |n| {
        if let Expr::Num(v) = n
            && exact.exact(*v).is_none()
        {
            ok = false;
        }
    });
    ok
}

/// The node budget of a symbolic derivative.
const DIFF_NODES: usize = 4096;

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
            let d = canonical(&Expr::Bin(
                BinOp::Sub,
                Box::new(a[0].clone()),
                Box::new(a[1].clone()),
            ));
            // a − b > 0: max picks a, min picks b.
            let a_bigger = sign_of(&d)?;
            let pick = if a_bigger == (*f == Func::Max) {
                &a[0]
            } else {
                &a[1]
            };
            Some(one_sided(pick, sides, left))
        }
        _ => None,
    })
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
    let d1 = simp(crate::diff::derivative_bounded(e, unit, DIFF_NODES)?);
    let d2 = simp(crate::diff::derivative_bounded(&d1, unit, DIFF_NODES)?);
    Some([canonical(&d1), canonical(&d2)])
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
            Expr::Num(v) if *v != 0.0 => {}
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
                    out.push(Expr::bin(BinOp::Sub, args[0].clone(), Expr::Num(1.0)))
                }
                // log_b u = 0 where u = 1 (b a valid base wherever defined).
                Func::LogBase if args.len() == 2 => {
                    out.push(Expr::bin(BinOp::Sub, args[1].clone(), Expr::Num(1.0)))
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
                        .unwrap_or(Expr::Num(1.0));
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
            Expr::Num(v) => (*v >= 0.0).then_some(*v > 0.0),
            Expr::Const(_) => Some(true),
            Expr::Bin(BinOp::Pow, a, p) => match crate::compile::syntactic_rational(p) {
                Some((n, 1)) if n % 2 == 0 => Some(never_zero(a)),
                _ => sign_of(a).map(|pos| pos && never_zero(e)),
            },
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
