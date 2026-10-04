//! The function under analysis: its canonical tree, its literals, the
//! angle unit, and a budget of interval evaluations.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::ast::{BinOp, Expr};
use crate::compile::CompileOptions;
use crate::interval::{Ctx, DecInterval, Interval, Literals, Series, taylor};

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
    /// f′ and f″ as trees of their own, when f is a rational function:
    /// from its exact form N/D, f′ = (N′D − ND′)/D² and f″ = (P′D −
    /// 2PD′)/D³ with P = N′D − ND′, each numerator expanded exactly (what
    /// cancels in them cancels exactly, not in interval arithmetic).
    pub derivs: Option<[Expr; 2]>,
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
            derivs: None,
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
        Some(s)
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
        Ok(taylor(e, x, n, &self.ctx()))
    }

    /// Taylor coefficients of f over the box (from the evaluated tree).
    pub fn ser(&self, x: Interval, n: usize) -> Result<Series, Stop> {
        self.ser_of(&self.eval, x, n)
    }

    /// f over the box.
    pub fn val(&self, x: Interval) -> Result<DecInterval, Stop> {
        Ok(self.ser(x, 0)?[0])
    }
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
            let term = Poly::x().pow(i as u32 - 1)?.scale(c.mul(Q::int(i as i128))?)?;
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
            Expr::bin(BinOp::Div, num.to_expr(), Expr::bin(BinOp::Pow, d.to_expr(), Expr::Num(k)))
        }
    };
    Some([canonical(&over(&p, 2.0)), canonical(&over(&p2, 3.0))])
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
    let m = if m.is_finite() { m } else { lo / 2.0 + hi / 2.0 };
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
