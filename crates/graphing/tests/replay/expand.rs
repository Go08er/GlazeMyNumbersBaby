//! Expansions at ±∞, the replay's own: f on a tail [M, ∞) (or (−∞, −M])
//! written in t = 1/x, over T = (0, 1/M] (or [−1/M, 0)), as
//!
//! f = Σ a_k·tᵏ (lo ≤ k < p) + tᵖ·ρ(t), ρ(t) in an enclosure for t in T,
//!
//! where each a_k encloses one fixed real and only ρ varies with t. The
//! rules, from calculus:
//!
//! * x = 1/t; constants are themselves; sums and products term by term,
//!   what passes order p going into the remainder with T's powers;
//! * g(u) for g smooth on u's values, u tending to u₀: Taylor's theorem at
//!   u₀, g⁽ʲ⁾(u₀)/j! enclosed over u₀'s enclosure (fixed reals), the
//!   remainder g⁽ᴶ⁺¹⁾(ξ)/(J+1)!·(u − u₀)^(J+1) with ξ over u's values;
//! * 1/u, √u, uᵏ for u = c·tᵐ·v (v → 1): c's power, |t|'s power (whole),
//!   and v's by the rule above;
//! * atan u for u of one sign, unbounded: ±¼ turn − atan(1/u);
//! * e^u for u ≤ −κ/|t| on T: below e^(−κM)·M^N·|t|^N for M ≥ N/κ;
//! * sin, cos of an unbounded u: a remainder of order 0 in [−1, 1];
//!   floor, ceil, round of one: u and such a remainder;
//! * e^(ln u) is u, where defined.
//!
//! With m = a₋₁ (≠ 0) and b = a₀, f − (m·x + b) is the rest, of order ≥ 1:
//! it tends to 0, and its range over T bounds it on the whole tail. Written
//! for the replay from those facts, with MPFR intervals and the replay's
//! own series; it shares no code with the certifier.

use super::Fx;
use super::eval::{contains_x, written_rational};
use super::iv::{self, Iv, Unit};
use graphing::ast::{BinOp, Constant, Expr, Func};

/// Orders kept: below t^TOP.
const TOP: i32 = 4;
/// Terms of a composed function's series.
const TERMS: usize = 5;

fn nil() -> Iv {
    Iv::of(0.0)
}

fn is_nil(v: &Iv) -> bool {
    !v.empty && v.lo == 0 && v.hi == 0
}

fn ok(v: &Iv) -> bool {
    !v.empty && v.bounded()
}

/// A Laurent polynomial in t with a remainder.
#[derive(Clone, Debug)]
pub struct Lp {
    pub lo: i32,
    pub a: Vec<Iv>,
    pub p: i32,
    pub rho: Iv,
}

impl Lp {
    fn at(&self, k: i32) -> Iv {
        if k < self.lo || k >= self.p {
            nil()
        } else {
            self.a[(k - self.lo) as usize].clone()
        }
    }

    fn number(v: Iv) -> Lp {
        let mut a = vec![nil(); TOP as usize];
        a[0] = v;
        Lp {
            lo: 0,
            a,
            p: TOP,
            rho: nil(),
        }
        .tidy()
    }

    fn only_rest(p: i32, rho: Iv) -> Lp {
        Lp {
            lo: p,
            a: Vec::new(),
            p,
            rho,
        }
    }

    /// Leading zeros (exact) dropped.
    fn tidy(mut self) -> Lp {
        let n = self.a.iter().take_while(|v| is_nil(v)).count();
        self.a.drain(..n);
        self.lo += n as i32;
        self
    }

    /// The lowest order present, its coefficient away from 0.
    fn leading(&self) -> Option<(i32, Iv)> {
        let v = self.a.first()?;
        (ok(v) && v.ne0()).then(|| (self.lo, v.clone()))
    }
}

/// Expansions over one tail.
pub struct Tail<'a> {
    fx: &'a Fx,
    t: Iv,
    right: bool,
    m: f64,
}

impl<'a> Tail<'a> {
    pub fn new(fx: &'a Fx, right: bool, m: f64) -> Tail<'a> {
        let tau = iv::div(&Iv::of(1.0), &Iv::of(m));
        let t = if right {
            Iv::new(iv::fl(0.0), tau.hi.clone())
        } else {
            Iv::new(iv::neg(&tau).lo, iv::fl(0.0))
        };
        Tail { fx, t, right, m }
    }

    fn tk(&self, k: i32) -> Iv {
        if k <= 0 {
            Iv::of(1.0)
        } else {
            iv::powi(&self.t, i64::from(k))
        }
    }

    /// Σ a_k·T^k + T^p·ρ, nonnegative orders only.
    fn over(&self, x: &Lp) -> Option<Iv> {
        if x.p < 0 {
            return None;
        }
        let mut s = iv::mul(&self.tk(x.p), &x.rho);
        for (i, v) in x.a.iter().enumerate() {
            let k = x.lo + i as i32;
            if k < 0 {
                if !is_nil(v) {
                    return None;
                }
                continue;
            }
            s = iv::add(&s, &iv::mul(v, &self.tk(k)));
        }
        ok(&s).then_some(s)
    }

    /// The same with orders from q up in the remainder.
    fn down_to(&self, x: &Lp, q: i32) -> Lp {
        if q >= x.p {
            return x.clone();
        }
        let mut rho = iv::mul(&x.rho, &self.tk(x.p - q));
        let mut a = Vec::new();
        for (i, v) in x.a.iter().enumerate() {
            let k = x.lo + i as i32;
            if k < q {
                a.push(v.clone());
            } else {
                rho = iv::add(&rho, &iv::mul(v, &self.tk(k - q)));
            }
        }
        Lp {
            lo: x.lo.min(q),
            a,
            p: q,
            rho,
        }
    }

    fn times_t(x: &Lp, s: i32) -> Lp {
        Lp {
            lo: x.lo + s,
            a: x.a.clone(),
            p: x.p + s,
            rho: x.rho.clone(),
        }
    }

    fn times(x: &Lp, k: &Iv) -> Lp {
        Lp {
            lo: x.lo,
            a: x.a.iter().map(|v| iv::mul(v, k)).collect(),
            p: x.p,
            rho: iv::mul(&x.rho, k),
        }
    }

    fn sum(&self, x: &Lp, y: &Lp) -> Lp {
        let p = x.p.min(y.p);
        let (x, y) = (self.down_to(x, p), self.down_to(y, p));
        let lo = x.lo.min(y.lo);
        Lp {
            lo,
            a: (lo..p).map(|k| iv::add(&x.at(k), &y.at(k))).collect(),
            p,
            rho: iv::add(&x.rho, &y.rho),
        }
        .tidy()
    }

    fn minus(x: &Lp) -> Lp {
        Tail::times(x, &Iv::of(-1.0))
    }

    /// The polynomial part divided by t^lo, over T.
    fn reduced(&self, x: &Lp) -> Iv {
        let mut s = nil();
        for (i, v) in x.a.iter().enumerate() {
            s = iv::add(&s, &iv::mul(v, &self.tk(i as i32)));
        }
        s
    }

    fn product(&self, x: &Lp, y: &Lp) -> Lp {
        let lx = if x.a.is_empty() { x.p } else { x.lo };
        let ly = if y.a.is_empty() { y.p } else { y.lo };
        let p = (x.p + ly).min(y.p + lx).min(x.p + y.p).min(TOP);
        let lo = (lx + ly).min(p);
        let mut a = vec![nil(); (p - lo).max(0) as usize];
        let mut rho = nil();
        for (i, u) in x.a.iter().enumerate() {
            for (j, v) in y.a.iter().enumerate() {
                let k = x.lo + i as i32 + y.lo + j as i32;
                let w = iv::mul(u, v);
                if k < p {
                    let s = (k - lo) as usize;
                    a[s] = iv::add(&a[s], &w);
                } else {
                    rho = iv::add(&rho, &iv::mul(&w, &self.tk(k - p)));
                }
            }
        }
        if !y.a.is_empty() {
            let t = iv::mul(&iv::mul(&x.rho, &self.reduced(y)), &self.tk(x.p + y.lo - p));
            rho = iv::add(&rho, &t);
        }
        if !x.a.is_empty() {
            let t = iv::mul(&iv::mul(&y.rho, &self.reduced(x)), &self.tk(y.p + x.lo - p));
            rho = iv::add(&rho, &t);
        }
        rho = iv::add(
            &rho,
            &iv::mul(&iv::mul(&x.rho, &y.rho), &self.tk(x.p + y.p - p)),
        );
        Lp { lo, a, p, rho }.tidy()
    }

    /// g(u), g's tree in x, by Taylor's theorem at u's limit.
    fn through(&self, g: &Expr, u: &Lp) -> Option<Lp> {
        if (u.lo..0).any(|k| !is_nil(&u.at(k))) {
            return None;
        }
        let u0 = u.at(0);
        let mut d = u.clone();
        if (d.lo..d.p).contains(&0) {
            d.a[(-d.lo) as usize] = nil();
        }
        let d = d.tidy();
        let spread = self.over(&d)?;
        let coeffs = self.fx.series_iv(g, u0.clone(), TERMS);
        if !coeffs[0].cont || coeffs.iter().any(|c| !ok(c) || !c.def) {
            return None;
        }
        let around = u0.hull(&iv::add(&u0, &spread));
        let far = self.fx.series_iv(g, around, TERMS + 1);
        if !far[0].cont || !far[TERMS + 1].def || !ok(&far[TERMS + 1]) {
            return None;
        }
        let mut out = Lp::number(coeffs[TERMS].clone());
        for j in (0..TERMS).rev() {
            out = self.sum(&self.product(&out, &d), &Lp::number(coeffs[j].clone()));
        }
        let mut w = Lp::number(Iv::of(1.0));
        for _ in 0..=TERMS {
            w = self.product(&w, &d);
        }
        let q = if w.a.is_empty() { w.p } else { w.lo };
        let wr = self.over(&Tail::times_t(&w, -q))?;
        Some(self.sum(&out, &Lp::only_rest(q, iv::mul(&wr, &far[TERMS + 1]))))
    }

    /// u = c·tᵐ·v with v → 1.
    fn split(&self, u: &Lp) -> Option<(i32, Iv, Lp)> {
        let (m, c) = u.leading()?;
        let v = Tail::times(&Tail::times_t(u, -m), &iv::recip(&c));
        Some((m, c, v))
    }

    /// tᵐ's sign on T.
    fn sgn(&self, m: i32) -> f64 {
        if self.right || m % 2 == 0 { 1.0 } else { -1.0 }
    }

    fn inverse(&self, u: &Lp) -> Option<Lp> {
        let (m, c, v) = self.split(u)?;
        let g = Expr::Bin(BinOp::Div, Box::new(Expr::exact(1.0)), Box::new(Expr::X));
        let w = self.through(&g, &v)?;
        Some(Tail::times_t(&Tail::times(&w, &iv::recip(&c)), -m))
    }

    /// u^(n/d), `k` the exponent's tree.
    fn root_power(&self, u: &Lp, n: i64, d: i64, k: &Expr) -> Option<Lp> {
        if n == 0 {
            return Some(Lp::number(Iv::of(1.0)));
        }
        let (m, c, v) = self.split(u)?;
        let mk = i64::from(m) * n;
        if mk % d != 0 {
            return None;
        }
        let e = (mk / d) as i32;
        let negative = c.lt(0.0) != (self.sgn(m) < 0.0);
        if negative && d % 2 == 0 {
            return None;
        }
        let ck = iv::pow_const(&iv::abs(&c), &iv::div(&Iv::of(n as f64), &Iv::of(d as f64)));
        if !ok(&ck) {
            return None;
        }
        let g = Expr::Bin(BinOp::Pow, Box::new(Expr::X), Box::new(k.clone()));
        let w = self.through(&g, &v)?;
        let mut s = self.sgn(e);
        if negative && n % 2 != 0 {
            s = -s;
        }
        Some(Tail::times_t(
            &Tail::times(&w, &iv::mul(&ck, &Iv::of(s))),
            e,
        ))
    }

    fn quarter(&self) -> Iv {
        match self.fx.unit {
            Unit::Radians => iv::div(&iv::pi(), &Iv::of(2.0)),
            Unit::Degrees => Iv::of(90.0),
            _ => Iv::of(100.0),
        }
    }

    fn exponential(&self, u: &Lp) -> Option<Lp> {
        let g = Expr::Call(Func::Exp, vec![Expr::X]);
        if let Some(t) = self.through(&g, u) {
            return Some(t);
        }
        let (m, c, v) = self.split(u)?;
        if m >= 0 {
            return None;
        }
        let vr = self.over(&v)?;
        if !vr.gt(0.0) || self.m > 1e300 {
            return None;
        }
        let down = c.gt(0.0) != (self.sgn(m) > 0.0);
        if !down {
            return None;
        }
        let kappa = iv::mul(&iv::abs(&c), &vr).lo.to_f64();
        if kappa.is_nan() || kappa <= 0.0 || self.m < f64::from(TOP) / kappa {
            return None;
        }
        let w = Iv::of(self.m);
        let b = iv::mul(
            &iv::exp(&iv::mul(&Iv::of(-kappa), &w)),
            &iv::powi(&w, i64::from(TOP)),
        );
        if !ok(&b) {
            return None;
        }
        let r = iv::mul(&Iv::new(iv::fl(0.0), b.hi.clone()), &Iv::of(self.sgn(TOP)));
        Some(Lp::only_rest(TOP, r))
    }

    pub fn of(&self, e: &Expr) -> Option<Lp> {
        if !contains_x(e) {
            let v = self.fx.series_iv(e, Iv::of(0.0), 0)[0].clone();
            if !ok(&v) || !v.def {
                return None;
            }
            return Some(Lp::number(v));
        }
        Some(match e {
            Expr::X => {
                let mut a = vec![nil(); (TOP + 1) as usize];
                a[0] = Iv::of(1.0);
                Lp {
                    lo: -1,
                    a,
                    p: TOP,
                    rho: nil(),
                }
            }
            Expr::Neg(x) => Tail::minus(&self.of(x)?),
            Expr::Bin(op, x, y) => match op {
                BinOp::Add => self.sum(&self.of(x)?, &self.of(y)?),
                BinOp::Sub => self.sum(&self.of(x)?, &Tail::minus(&self.of(y)?)),
                BinOp::Mul => self.product(&self.of(x)?, &self.of(y)?),
                BinOp::Div => self.product(&self.of(x)?, &self.inverse(&self.of(y)?)?),
                BinOp::Pow => return self.power(x, y),
            },
            Expr::Call(func, args) if args.len() == 1 => return self.call(*func, &args[0]),
            _ => return None,
        })
    }

    fn power(&self, base: &Expr, k: &Expr) -> Option<Lp> {
        let is_e = matches!(base, Expr::Const(Constant::E));
        if contains_x(k) {
            if is_e && let Expr::Call(Func::Ln, args) = k {
                return self.of(&args[0]);
            }
            let ln = if is_e {
                Lp::number(Iv::of(1.0))
            } else {
                self.through(&Expr::Call(Func::Ln, vec![Expr::X]), &self.of(base)?)?
            };
            return self.exponential(&self.product(&self.of(k)?, &ln));
        }
        let u = self.of(base)?;
        if let Some((n, d)) = written_rational(k, &self.fx.lits) {
            return self.root_power(&u, n, d, k);
        }
        let g = Expr::Bin(BinOp::Pow, Box::new(Expr::X), Box::new(k.clone()));
        self.through(&g, &u)
    }

    fn call(&self, func: Func, arg: &Expr) -> Option<Lp> {
        if func == Func::Exp
            && let Expr::Call(Func::Ln, args) = arg
        {
            return self.of(&args[0]);
        }
        let u = self.of(arg)?;
        let bounded = self.over(&u).is_some();
        let tree = Expr::Call(func, vec![Expr::X]);
        // u's sign on T, from its leading term (the rest staying positive).
        let sign = || -> Option<bool> {
            let (m, c, v) = self.split(&u)?;
            self.over(&v)?
                .gt(0.0)
                .then(|| c.gt(0.0) == (self.sgn(m) > 0.0))
        };
        match func {
            Func::Exp => self.exponential(&u),
            Func::Sqrt => {
                let half = Expr::Bin(
                    BinOp::Div,
                    Box::new(Expr::exact(1.0)),
                    Box::new(Expr::exact(2.0)),
                );
                self.root_power(&u, 1, 2, &half)
            }
            Func::Cbrt => {
                let third = Expr::Bin(
                    BinOp::Div,
                    Box::new(Expr::exact(1.0)),
                    Box::new(Expr::exact(3.0)),
                );
                self.root_power(&u, 1, 3, &third)
            }
            Func::Abs => Some(if sign()? { u } else { Tail::minus(&u) }),
            Func::Sin | Func::Cos if !bounded => Some(Lp::only_rest(0, Iv::of2(-1.0, 1.0))),
            Func::Floor | Func::Ceil | Func::Round if !bounded => {
                Some(self.sum(&u, &Lp::only_rest(0, Iv::of2(-1.0, 1.0))))
            }
            Func::Atan | Func::Acot if !bounded => {
                let pos = sign()?;
                let q = self.quarter();
                let inv = self.inverse(&u)?;
                let small = self.through(&Expr::Call(Func::Atan, vec![Expr::X]), &inv)?;
                let atan = self.sum(
                    &Lp::number(if pos { q.clone() } else { iv::neg(&q) }),
                    &Tail::minus(&small),
                );
                Some(if func == Func::Atan {
                    atan
                } else {
                    self.sum(&Lp::number(q), &Tail::minus(&atan))
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
            | Func::Cot => self.through(&tree, &u),
            _ => None,
        }
    }
}

/// f's line on the tail beyond `from` (its sign the side): (m, b, the
/// range of f − (m·x + b) there), m away from 0.
pub fn line(fx: &Fx, from: f64) -> Option<(Iv, Iv, Iv)> {
    let tail = Tail::new(fx, from > 0.0, from.abs());
    let lp = tail.of(&fx.f)?;
    if lp.p < 1 || (lp.lo..-1).any(|k| !is_nil(&lp.at(k))) {
        return None;
    }
    let (m, b) = (lp.at(-1), lp.at(0));
    if !ok(&m) || m.has0() || !ok(&b) {
        return None;
    }
    let mut rest = lp.clone();
    for k in rest.lo..1.min(rest.p) {
        rest.a[(k - rest.lo) as usize] = nil();
    }
    let band = tail.over(&rest.tidy())?;
    Some((m, b, band))
}
