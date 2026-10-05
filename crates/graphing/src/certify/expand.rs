//! Expansions at ±∞: f on a tail [M, ∞) (or (−∞, −M]) as a Laurent
//! polynomial in t = 1/x with a bound on the rest — a Taylor model in t
//! over T = (0, 1/M] (or [−1/M, 0)):
//!
//! f(1/t) = Σ c_k·tᵏ (lo ≤ k < p) + tᵖ·ρ(t), ρ(t) ∈ r for every t in T,
//!
//! each c_k an enclosure of one fixed real (not depending on t): only the
//! remainder varies with t. So f = c₋₁·x + c₀ + O(1/x) on the tail is an
//! oblique asymptote y = c₋₁·x + c₀ with f − (c₋₁·x + c₀) enclosed over the
//! whole tail by the rest, which tends to 0: √(x² + 1), x·e^(1/x),
//! x·atan x, x + 1/x + sin(x)/x, where f − (m·x + b) cancels only
//! symbolically and f's own enclosures far out can't show it.
//!
//! A smooth function g of a node u tending to u₀ is g's Taylor series at
//! u₀ (its coefficients enclosed over u₀'s enclosure, fixed reals) with
//! Lagrange's remainder over the values u takes on T; 1/u, √u, uᵏ of a
//! u ~ c·tᵐ factor out c·tᵐ first; atan of an unbounded u is ±¼ turn −
//! atan(1/u); e^u for u → −∞ is below every power of t; sin, cos of an
//! unbounded u are a bounded rest of order 0. The replay expands the same
//! way with its own arithmetic (`tests/replay/expand.rs`) and checks the
//! [`Claim::TailLine`] claims.
//!
//! [`Claim::TailLine`]: super::cert::Claim::TailLine

use super::fun::Fun;
use crate::ast::{BinOp, Constant, Expr, Func};
use crate::functions::TrigUnit;
use crate::interval::{Ctx, Dec, DecInterval, Interval, elem, taylor};

/// Coefficients are kept below t^P.
const P: i32 = 4;
/// Terms of a composed function's series.
const J: usize = 5;

fn dec(iv: Interval) -> DecInterval {
    DecInterval::new(iv)
}

fn zero() -> Interval {
    Interval::point(0.0)
}

fn exact_zero(v: Interval) -> bool {
    v.lo() == 0.0 && v.hi() == 0.0
}

/// A Laurent polynomial in t with a remainder: coefficients of t^lo …
/// t^(p−1), and tᵖ·ρ with ρ in `r`.
#[derive(Clone, Debug)]
pub struct Tm {
    pub lo: i32,
    pub c: Vec<Interval>,
    pub p: i32,
    pub r: Interval,
}

impl Tm {
    /// Leading coefficients exactly 0 dropped: `lo` the lowest order
    /// present.
    fn trim(mut self) -> Tm {
        let n = self.c.iter().take_while(|c| exact_zero(**c)).count();
        self.c.drain(..n);
        self.lo += n as i32;
        self
    }

    fn coef(&self, k: i32) -> Interval {
        if k < self.lo || k >= self.p {
            zero()
        } else {
            self.c[(k - self.lo) as usize]
        }
    }

    /// The constant c (a fixed real, enclosed).
    fn constant(c: Interval) -> Tm {
        let mut v = vec![zero(); P as usize];
        v[0] = c;
        Tm {
            lo: 0,
            c: v,
            p: P,
            r: zero(),
        }
    }

    /// tᵖ·ρ with ρ in `r` and nothing else.
    fn rest(p: i32, r: Interval) -> Tm {
        Tm {
            lo: p,
            c: Vec::new(),
            p,
            r,
        }
    }

    /// The lowest order with a coefficient not exactly 0, its coefficient
    /// away from 0 (none if one below holds 0 without being 0).
    pub fn lead(&self) -> Option<(i32, Interval)> {
        for (i, c) in self.c.iter().enumerate() {
            if exact_zero(*c) {
                continue;
            }
            return (!c.contains_zero()).then_some((self.lo + i as i32, *c));
        }
        None
    }
}

/// Expansions over one tail.
pub struct Ex<'a> {
    /// T: [0, 1/M] on the right, [−1/M, 0] on the left (0 is never a value
    /// of t; bounds over the closed box hold the more so).
    t: Interval,
    right: bool,
    m: f64,
    ctx: Ctx<'a>,
}

impl<'a> Ex<'a> {
    pub fn new(f: &'a Fun<'_>, right: bool, m: f64) -> Ex<'a> {
        let tau = (1.0 / m).next_up();
        let t = if right {
            Interval::new(0.0, tau)
        } else {
            Interval::new(-tau, 0.0)
        };
        Ex {
            t,
            right,
            m,
            ctx: Ctx::new(f.opts, f.lits),
        }
    }

    /// Tᵏ (k ≥ 0).
    fn tp(&self, k: i32) -> Interval {
        if k <= 0 {
            return Interval::point(1.0);
        }
        elem::powi(&dec(self.t), k).iv
    }

    /// The range over T of Σ c_k·tᵏ + tᵖ·ρ (orders ≥ 0 only: `None` if a
    /// negative order has a coefficient other than 0, or p < 0).
    fn range(&self, a: &Tm) -> Option<Interval> {
        if a.p < 0 {
            return None;
        }
        let mut s = self.tp(a.p) * a.r;
        for (i, c) in a.c.iter().enumerate() {
            let k = a.lo + i as i32;
            if k < 0 {
                if !exact_zero(*c) {
                    return None;
                }
                continue;
            }
            s = s + *c * self.tp(k);
        }
        (!s.is_empty() && s.is_bounded()).then_some(s)
    }

    /// `a` with its remainder at order q ≤ a.p: terms from q up folded in.
    fn lower(&self, a: &Tm, q: i32) -> Tm {
        if q >= a.p {
            return a.clone();
        }
        let mut r = a.r * self.tp(a.p - q);
        let mut c = Vec::new();
        for (i, v) in a.c.iter().enumerate() {
            let k = a.lo + i as i32;
            if k < q {
                c.push(*v);
            } else {
                r = r + *v * self.tp(k - q);
            }
        }
        let lo = a.lo.min(q);
        Tm { lo, c, p: q, r }
    }

    fn shift(a: &Tm, s: i32) -> Tm {
        Tm {
            lo: a.lo + s,
            c: a.c.clone(),
            p: a.p + s,
            r: a.r,
        }
    }

    fn scale(a: &Tm, k: Interval) -> Tm {
        Tm {
            lo: a.lo,
            c: a.c.iter().map(|c| *c * k).collect(),
            p: a.p,
            r: a.r * k,
        }
    }

    fn add(&self, a: &Tm, b: &Tm) -> Tm {
        let p = a.p.min(b.p);
        let (a, b) = (self.lower(a, p), self.lower(b, p));
        let lo = a.lo.min(b.lo);
        let c = (lo..p).map(|k| a.coef(k) + b.coef(k)).collect();
        Tm {
            lo,
            c,
            p,
            r: a.r + b.r,
        }
        .trim()
    }

    fn neg(a: &Tm) -> Tm {
        Ex::scale(a, Interval::point(-1.0))
    }

    /// The polynomial part over tᵏ⁰ (k₀ its lowest order): its range over
    /// T, the orders all ≥ 0.
    fn poly_over(&self, a: &Tm) -> Interval {
        let mut s = zero();
        for (i, c) in a.c.iter().enumerate() {
            s = s + *c * self.tp(i as i32);
        }
        s
    }

    fn mul(&self, a: &Tm, b: &Tm) -> Tm {
        // (A + t^pa·ρa)(B + t^pb·ρb) = AB + t^pa·ρa·B + t^pb·ρb·A + t^(pa+pb)·ρa·ρb.
        let (la, lb) = (
            if a.c.is_empty() { a.p } else { a.lo },
            if b.c.is_empty() { b.p } else { b.lo },
        );
        let p = (a.p + lb).min(b.p + la).min(a.p + b.p).min(P);
        let lo = (la + lb).min(p);
        let mut c = vec![zero(); (p - lo).max(0) as usize];
        let mut r = zero();
        for (i, x) in a.c.iter().enumerate() {
            for (j, y) in b.c.iter().enumerate() {
                let k = a.lo + i as i32 + b.lo + j as i32;
                let v = *x * *y;
                if k < p {
                    c[(k - lo) as usize] = c[(k - lo) as usize] + v;
                } else {
                    r = r + v * self.tp(k - p);
                }
            }
        }
        if !b.c.is_empty() {
            r = r + a.r * self.poly_over(b) * self.tp(a.p + b.lo - p);
        }
        if !a.c.is_empty() {
            r = r + b.r * self.poly_over(a) * self.tp(b.p + a.lo - p);
        }
        r = r + a.r * b.r * self.tp(a.p + b.p - p);
        Tm { lo, c, p, r }.trim()
    }

    fn powi(&self, a: &Tm, n: usize) -> Tm {
        let mut out = Tm::constant(Interval::point(1.0));
        for _ in 0..n {
            out = self.mul(&out, a);
        }
        out
    }

    /// g(u) for g's tree in x (smooth where u goes), u bounded on T:
    /// g's series at u₀ with Lagrange's remainder over u's values.
    fn compose(&self, g: &Expr, u: &Tm) -> Option<Tm> {
        if u.lo < 0 && u.c.iter().take((-u.lo) as usize).any(|c| !exact_zero(*c)) {
            return None;
        }
        let u0 = u.coef(0);
        // σ = u − u₀: orders ≥ 1, or the remainder's.
        let mut sigma = u.clone();
        if (sigma.lo..sigma.p).contains(&0) {
            sigma.c[(-sigma.lo) as usize] = zero();
        }
        let sigma = sigma.trim();
        let s_range = self.range(&sigma)?;
        let gs = taylor(g, u0, J, &self.ctx);
        if !super::fun::usable(&gs, J) || gs.iter().any(|c| !c.iv.is_bounded()) {
            return None;
        }
        let over = u0.hull(u0 + s_range);
        let gr = taylor(g, over, J + 1, &self.ctx);
        if !super::fun::usable(&gr, J + 1) || !gr[J + 1].iv.is_bounded() {
            return None;
        }
        // Horner: Σ g_j σʲ.
        let mut out = Tm::constant(gs[J].iv);
        for j in (0..J).rev() {
            out = self.add(&self.mul(&out, &sigma), &Tm::constant(gs[j].iv));
        }
        // σ^(J+1)·g^(J+1)(ξ)/(J+1)!, all in the remainder.
        let w = self.powi(&sigma, J + 1);
        let q = if w.c.is_empty() { w.p } else { w.lo };
        let wr = self.range(&Ex::shift(&w, -q))?;
        Some(self.add(&out, &Tm::rest(q, wr * gr[J + 1].iv)))
    }

    /// u = c·tᵐ·(1 + o(1)) as (m, c, u/(c·tᵐ)), the last tending to 1.
    fn factor(&self, u: &Tm) -> Option<(i32, Interval, Tm)> {
        let (m, c) = u.lead()?;
        let v = Ex::scale(&Ex::shift(u, -m), c.recip());
        Some((m, c, v))
    }

    fn tree(f: Func) -> Expr {
        Expr::Call(f, vec![Expr::X])
    }

    fn recip(&self, u: &Tm) -> Option<Tm> {
        let (m, c, v) = self.factor(u)?;
        let inv = Expr::bin(BinOp::Div, Expr::Num(1.0), Expr::X);
        let w = self.compose(&inv, &v)?;
        Some(Ex::shift(&Ex::scale(&w, c.recip()), -m))
    }

    /// The sign of tᵐ on T.
    fn t_sign(&self, m: i32) -> f64 {
        if self.right || m % 2 == 0 { 1.0 } else { -1.0 }
    }

    /// uᵏ for k = n/d (`odd`: d odd, so a negative u has a real power):
    /// |c|ᵏ·|t|^(mk)·(u/(c·tᵐ))ᵏ, with u's sign.
    fn power(&self, u: &Tm, n: i64, d: i64, k: &Expr) -> Option<Tm> {
        if n == 0 {
            return Some(Tm::constant(Interval::point(1.0)));
        }
        let (m, c, v) = self.factor(u)?;
        let mk = i64::from(m) * n;
        if mk % d != 0 {
            return None;
        }
        let e = (mk / d) as i32;
        // u's sign on T, and |t|^e = sign(t)^e·tᵉ.
        let neg = (c.lt0()) != (self.t_sign(m) < 0.0);
        if neg && d % 2 == 0 {
            return None;
        }
        let ck = elem::pow(
            &dec(c.abs()),
            &dec(Interval::point(n as f64) / Interval::point(d as f64)),
        );
        if ck.is_empty() || !ck.iv.is_bounded() {
            return None;
        }
        // (A whole k is exact as written; a ratio as its value.)
        let g = Expr::bin(BinOp::Pow, Expr::X, k.clone());
        let w = self.compose(&g, &v)?;
        let mut s = self.t_sign(e);
        if neg && n % 2 != 0 {
            s = -s;
        }
        Some(Ex::shift(&Ex::scale(&w, ck.iv * Interval::point(s)), e))
    }

    /// A quarter turn in the analysis' unit.
    fn quarter(&self) -> Interval {
        match self.ctx.opts.trig_unit {
            TrigUnit::Radians => elem::pi() / Interval::point(2.0),
            TrigUnit::Degrees => Interval::point(90.0),
            TrigUnit::Grads => Interval::point(100.0),
        }
    }

    fn exp(&self, u: &Tm) -> Option<Tm> {
        if let Some(t) = self.compose(&Ex::tree(Func::Exp), u) {
            return Some(t);
        }
        // u → −∞: |u| ≥ κ/|t| on T (|t| ≤ 1), so e^u/|t|^P ≤ e^(−κM)·M^P
        // for M ≥ P/κ (e^(−κw)·w^P falls for w ≥ P/κ).
        let (m, c, v) = self.factor(u)?;
        if m >= 0 {
            return None;
        }
        let vr = self.range(&v)?;
        if !vr.gt0() || self.m > 1e300 {
            return None;
        }
        let sign = if c.gt0() { 1.0 } else { -1.0 } * self.t_sign(m);
        if sign > 0.0 {
            return None;
        }
        let kappa = (c.abs() * vr).lo();
        if kappa.is_nan() || kappa <= 0.0 || self.m < f64::from(P) / kappa || self.m.abs() < 1.0 {
            return None;
        }
        let w = Interval::point(self.m);
        let b = elem::exp(&dec(Interval::point(-kappa) * w)).iv * elem::powi(&dec(w), P).iv;
        if !b.is_bounded() {
            return None;
        }
        let r = Interval::new(0.0, b.hi()) * Interval::point(self.t_sign(P));
        Some(Tm::rest(P, r))
    }

    /// The expansion of `e` over the tail.
    pub fn of(&self, e: &Expr) -> Option<Tm> {
        if !e.contains_x() {
            let v = taylor(e, Interval::point(0.0), 0, &self.ctx)[0];
            if v.is_empty() || v.dec < Dec::Def || !v.iv.is_bounded() {
                return None;
            }
            return Some(Tm::constant(v.iv));
        }
        Some(match e {
            Expr::X => {
                let mut c = vec![zero(); (P + 1) as usize];
                c[0] = Interval::point(1.0);
                Tm {
                    lo: -1,
                    c,
                    p: P,
                    r: zero(),
                }
            }
            Expr::Neg(a) => Ex::neg(&self.of(a)?),
            Expr::Bin(op, a, b) => match op {
                BinOp::Add => self.add(&self.of(a)?, &self.of(b)?),
                BinOp::Sub => self.add(&self.of(a)?, &Ex::neg(&self.of(b)?)),
                BinOp::Mul => self.mul(&self.of(a)?, &self.of(b)?),
                BinOp::Div => self.mul(&self.of(a)?, &self.recip(&self.of(b)?)?),
                BinOp::Pow => return self.pow(a, b),
            },
            Expr::Call(func, args) if args.len() == 1 => return self.call(*func, &args[0]),
            _ => return None,
        })
    }

    fn pow(&self, a: &Expr, b: &Expr) -> Option<Tm> {
        if b.contains_x() {
            // e^(b·ln a): e^(ln a) is a itself.
            if matches!(a, Expr::Const(Constant::E))
                && let Expr::Call(Func::Ln, args) = b
            {
                return self.of(&args[0]);
            }
            let la = if matches!(a, Expr::Const(Constant::E)) {
                Tm::constant(Interval::point(1.0))
            } else {
                self.compose(&Ex::tree(Func::Ln), &self.of(a)?)?
            };
            return self.exp(&self.mul(&self.of(b)?, &la));
        }
        let u = self.of(a)?;
        if let Some((n, d)) = crate::compile::syntactic_rational(b, self.ctx.literals) {
            return self.power(&u, i64::from(n), i64::from(d), b);
        }
        // Another constant exponent: a base tending to a positive value.
        self.compose(&Expr::bin(BinOp::Pow, Expr::X, b.clone()), &u)
    }

    fn call(&self, func: Func, a: &Expr) -> Option<Tm> {
        if func == Func::Exp
            && let Expr::Call(Func::Ln, args) = a
        {
            return self.of(&args[0]);
        }
        let u = self.of(a)?;
        let bounded = self.range(&u).is_some();
        match func {
            Func::Exp => self.exp(&u),
            Func::Sqrt => {
                let half = Expr::bin(BinOp::Div, Expr::Num(1.0), Expr::Num(2.0));
                self.power(&u, 1, 2, &half)
            }
            Func::Cbrt => {
                let third = Expr::bin(BinOp::Div, Expr::Num(1.0), Expr::Num(3.0));
                self.power(&u, 1, 3, &third)
            }
            Func::Abs => {
                let (m, c, v) = self.factor(&u)?;
                // u's sign on T: c·tᵐ's, v staying positive.
                if !self.range(&v)?.gt0() {
                    return None;
                }
                let pos = c.gt0() == (self.t_sign(m) > 0.0);
                Some(if pos { u } else { Ex::neg(&u) })
            }
            Func::Sin | Func::Cos if !bounded => Some(Tm::rest(0, Interval::new(-1.0, 1.0))),
            Func::Floor | Func::Ceil | Func::Round if !bounded => {
                Some(self.add(&u, &Tm::rest(0, Interval::new(-1.0, 1.0))))
            }
            Func::Atan | Func::Acot if !bounded => {
                // atan u = ±¼ turn − atan(1/u), u of one sign on T.
                let (m, c, v) = self.factor(&u)?;
                if !self.range(&v)?.gt0() {
                    return None;
                }
                let pos = c.gt0() == (self.t_sign(m) > 0.0);
                let q = self.quarter();
                let inv = self.recip(&u)?;
                let at = self.compose(&Ex::tree(Func::Atan), &inv)?;
                let atan = self.add(&Tm::constant(if pos { q } else { -q }), &Ex::neg(&at));
                Some(if func == Func::Atan {
                    atan
                } else {
                    // acot u = ¼ turn − atan u.
                    self.add(&Tm::constant(q), &Ex::neg(&atan))
                })
            }
            Func::Sin
            | Func::Cos
            | Func::Tan
            | Func::Atan
            | Func::Acot
            | Func::Ln
            | Func::Log
            | Func::Asin
            | Func::Acos
            | Func::Sinh
            | Func::Cosh
            | Func::Tanh
            | Func::Asinh
            | Func::Sec
            | Func::Csc
            | Func::Cot => self.compose(&Ex::tree(func), &u),
            _ => None,
        }
    }
}

/// An oblique line on one tail from the expansion over [M, ∞) (or
/// (−∞, −M]): (m, b, f − (m·x + b)'s range there), m away from 0.
pub fn line(f: &Fun<'_>, right: bool, m: f64) -> Option<(Interval, Interval, Interval)> {
    let ex = Ex::new(f, right, m);
    let tm = ex.of(&f.expr)?;
    if tm.p < 1 {
        return None;
    }
    for k in tm.lo..-1 {
        if !exact_zero(tm.coef(k)) {
            return None;
        }
    }
    let (slope, b) = (tm.coef(-1), tm.coef(0));
    if slope.contains_zero() || !slope.is_bounded() || !b.is_bounded() {
        return None;
    }
    let mut rest = tm.clone();
    for k in rest.lo..1.min(rest.p) {
        rest.c[(k - rest.lo) as usize] = zero();
    }
    let band = ex.range(&rest)?;
    Some((slope, b, band))
}
