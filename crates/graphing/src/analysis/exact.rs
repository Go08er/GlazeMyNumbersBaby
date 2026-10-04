//! Exact values for the analysis panel. The panel shows a closed form
//! (`√2`, `π/2`, `(1 + √5)/2`) only when it is proven, and the values here
//! are exact by construction: rational arithmetic, square roots of exact
//! rationals, rational multiples of π, and the trigonometric functions'
//! values at multiples of π/6 and π/4. A candidate is never *recognised*
//! from a double: it is built exactly (a root of an exact polynomial, a
//! rational multiple of an exact period) and then checked exactly (the
//! function, or its derivative, evaluates to exactly 0 there).

use crate::ast::{BinOp, Constant, Expr, Func};
use crate::functions::TrigUnit;
use crate::simplify::Q;
use crate::simplify::rational::Poly;
use crate::simplify::{ExactLiterals, PiQ};

use super::format::{MINUS, Nice, superscript};

/// An exact real number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ex {
    /// a + b·√c, with c > 1 square-free, or b = 0 (and c = 1): a rational.
    Alg { a: Q, b: Q, c: i64 },
    /// q·π.
    Pi(Q),
}

fn sq(q: Q) -> Option<Q> {
    q.mul(q)
}

/// n = s²·c with c square-free (n ≥ 0, small enough to factor).
fn square_free(n: i128) -> Option<(i128, i64)> {
    if n < 0 {
        return None;
    }
    if n == 0 {
        return Some((0, 1));
    }
    if n > 1_000_000_000_000 {
        return None;
    }
    let (mut r, mut s) = (n, 1i128);
    let mut f = 2i128;
    while f * f <= r {
        while r % (f * f) == 0 {
            r /= f * f;
            s *= f;
        }
        f += 1;
    }
    Some((s, i64::try_from(r).ok()?))
}

/// ⌊q⌋.
fn floor_q(q: Q) -> i128 {
    q.numer().div_euclid(q.denom())
}

/// The sign of self − v for a double v that Q can't hold (beyond about
/// 2⁻¹²⁰ or 2¹²⁰): by the signs, or when they differ by far more than
/// rounding.
fn by_margin(e: Ex, v: f64) -> Option<i32> {
    let s = e.sign()?;
    let vs = if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    };
    if s != vs {
        return Some((s - vs).signum());
    }
    let f = e.to_f64();
    if (f - v).abs() > 1e-6 * f.abs().max(v.abs()) {
        return Some(if f > v { 1 } else { -1 });
    }
    None
}

/// The double π is just below π; the next double is above it.
fn pi_bounds() -> Option<(Q, Q)> {
    let lo = std::f64::consts::PI;
    Some((Q::from_f64(lo)?, Q::from_f64(lo.next_up())?))
}

// add, sub, mul, div and neg return `None` where the result has no form
// here (or overflows), which the operator traits can't express.
#[allow(clippy::should_implement_trait)]
impl Ex {
    /// The rational `q`.
    pub fn q(q: Q) -> Ex {
        Ex::Alg {
            a: q,
            b: Q::ZERO,
            c: 1,
        }
    }

    /// The integer `n`.
    pub fn int(n: i128) -> Ex {
        Ex::q(Q::int(n))
    }

    fn norm(self) -> Ex {
        match self {
            Ex::Alg { a, b, .. } if b.is_zero() => Ex::q(a),
            Ex::Pi(q) if q.is_zero() => Ex::q(Q::ZERO),
            e => e,
        }
    }

    /// The value as a rational, if it is one.
    pub fn rational(self) -> Option<Q> {
        match self.norm() {
            Ex::Alg { a, b, .. } if b.is_zero() => Some(a),
            _ => None,
        }
    }

    pub fn is_zero(self) -> bool {
        self.rational().is_some_and(|q| q.is_zero())
    }

    pub fn neg(self) -> Option<Ex> {
        Some(match self {
            Ex::Alg { a, b, c } => Ex::Alg {
                a: a.neg()?,
                b: b.neg()?,
                c,
            },
            Ex::Pi(q) => Ex::Pi(q.neg()?),
        })
    }

    pub fn add(self, o: Ex) -> Option<Ex> {
        let (s, o) = (self.norm(), o.norm());
        Some(
            match (s, o) {
                (
                    Ex::Alg { a, b, c },
                    Ex::Alg {
                        a: a2,
                        b: b2,
                        c: c2,
                    },
                ) => {
                    let c = if b.is_zero() {
                        c2
                    } else if b2.is_zero() || c == c2 {
                        c
                    } else {
                        return None;
                    };
                    Ex::Alg {
                        a: a.add(a2)?,
                        b: b.add(b2)?,
                        c,
                    }
                }
                (Ex::Pi(p), Ex::Pi(q)) => Ex::Pi(p.add(q)?),
                (Ex::Pi(p), z) | (z, Ex::Pi(p)) if z.is_zero() => Ex::Pi(p),
                _ => return None,
            }
            .norm(),
        )
    }

    pub fn sub(self, o: Ex) -> Option<Ex> {
        self.add(o.neg()?)
    }

    pub fn mul(self, o: Ex) -> Option<Ex> {
        let (s, o) = (self.norm(), o.norm());
        Some(
            match (s, o) {
                (
                    Ex::Alg { a, b, c },
                    Ex::Alg {
                        a: a2,
                        b: b2,
                        c: c2,
                    },
                ) => {
                    if b.is_zero() {
                        Ex::Alg {
                            a: a.mul(a2)?,
                            b: a.mul(b2)?,
                            c: c2,
                        }
                    } else if b2.is_zero() {
                        Ex::Alg {
                            a: a.mul(a2)?,
                            b: b.mul(a2)?,
                            c,
                        }
                    } else if c == c2 {
                        // (a + b√c)(a₂ + b₂√c) = a·a₂ + b·b₂·c + (a·b₂ + a₂·b)√c.
                        Ex::Alg {
                            a: a.mul(a2)?.add(b.mul(b2)?.mul(Q::int(c.into()))?)?,
                            b: a.mul(b2)?.add(a2.mul(b)?)?,
                            c,
                        }
                    } else {
                        return None;
                    }
                }
                (Ex::Pi(p), r) | (r, Ex::Pi(p)) => Ex::Pi(p.mul(r.rational()?)?),
            }
            .norm(),
        )
    }

    pub fn recip(self) -> Option<Ex> {
        match self.norm() {
            Ex::Alg { a, b, c } => {
                if b.is_zero() {
                    return Some(Ex::q(a.recip()?));
                }
                // 1/(a + b√c) = (a − b√c)/(a² − b²c), never 0 below.
                let d = sq(a)?.sub(sq(b)?.mul(Q::int(c.into()))?)?;
                Some(Ex::Alg {
                    a: a.div(d)?,
                    b: b.neg()?.div(d)?,
                    c,
                })
            }
            Ex::Pi(_) => None,
        }
    }

    pub fn div(self, o: Ex) -> Option<Ex> {
        if self.is_zero() && !o.is_zero() {
            return Some(Ex::int(0));
        }
        if let (Ex::Pi(p), Ex::Pi(q)) = (self.norm(), o.norm()) {
            return Some(Ex::q(p.div(q)?));
        }
        if o.is_zero() {
            return None;
        }
        self.mul(o.recip()?)
    }

    /// selfⁿ for an integer n (0⁰ and 0 to a negative power: undefined).
    pub fn powi(self, n: i64) -> Option<Ex> {
        if self.is_zero() {
            return (n > 0).then_some(self);
        }
        if n.unsigned_abs() > 64 {
            return None;
        }
        let mut acc = Ex::int(1);
        for _ in 0..n.unsigned_abs() {
            acc = acc.mul(self)?;
        }
        if n < 0 { acc.recip() } else { Some(acc) }
    }

    /// √self for a rational self ≥ 0.
    pub fn sqrt(self) -> Option<Ex> {
        let r = self.rational()?;
        if r.signum() < 0 {
            return None;
        }
        // √(n/d) = √(n·d)/d.
        let nd = r.numer().checked_mul(r.denom())?;
        let (s, c) = square_free(nd)?;
        let k = Q::new(s, r.denom())?;
        Some(if c == 1 {
            Ex::q(k)
        } else {
            Ex::Alg {
                a: Q::ZERO,
                b: k,
                c,
            }
        })
    }

    /// The real n-th root of a rational that is an exact n-th power.
    fn nth_root(self, n: u32) -> Option<Ex> {
        let r = self.rational()?;
        if n == 2 {
            return self.sqrt();
        }
        let neg = r.signum() < 0;
        if neg && n.is_multiple_of(2) {
            return None;
        }
        let root = |v: i128| -> Option<i128> {
            let g = (v as f64).powf(1.0 / n as f64).round() as i128;
            (g.checked_pow(n)? == v).then_some(g)
        };
        let (p, q) = (root(r.numer().abs())?, root(r.denom())?);
        let v = Q::new(p, q)?;
        Some(Ex::q(if neg { v.neg()? } else { v }))
    }

    /// The sign: −1, 0 or 1 (exactly).
    pub fn sign(self) -> Option<i32> {
        match self.norm() {
            Ex::Pi(q) => Some(q.signum()),
            Ex::Alg { a, b, c } => {
                if b.is_zero() {
                    return Some(a.signum());
                }
                let (sa, sb) = (a.signum(), b.signum());
                if sa == 0 || sa == sb {
                    return Some(sb);
                }
                // Opposite signs: the larger of a² and b²·c wins.
                let (a2, b2c) = (sq(a)?, sq(b)?.mul(Q::int(c.into()))?);
                Some(match a2.cmp(&b2c) {
                    std::cmp::Ordering::Greater => sa,
                    std::cmp::Ordering::Less => sb,
                    std::cmp::Ordering::Equal => 0,
                })
            }
        }
    }

    /// Whether lo ≤ self ≤ hi, exactly (`None` if it can't be told).
    #[allow(clippy::float_cmp)]
    pub fn within(self, lo: f64, hi: f64) -> Option<bool> {
        let at_least = |v: f64| -> Option<bool> {
            if v == f64::NEG_INFINITY {
                return Some(true);
            }
            if v == f64::INFINITY {
                return Some(false);
            }
            match Q::from_f64(v) {
                Some(q) => Some(self.sub(Ex::q(q))?.sign()? >= 0),
                // A double too small or large for Q: told apart by sign,
                // or by a wide margin.
                None => by_margin(self, v).map(|o| o >= 0),
            }
        };
        let at_most = |v: f64| -> Option<bool> {
            if v == f64::INFINITY {
                return Some(true);
            }
            if v == f64::NEG_INFINITY {
                return Some(false);
            }
            match Q::from_f64(v) {
                Some(q) => Some(Ex::q(q).sub(self)?.sign()? >= 0),
                None => by_margin(self, v).map(|o| o <= 0),
            }
        };
        if let Ex::Pi(q) = self.norm() {
            // q·π lies strictly between q·π₋ and q·π₊: π to 20 decimals
            // (or, if that overflows, the doubles beside π).
            let fine = Q::new(314_159_265_358_979_323_846, 100_000_000_000_000_000_000).zip(
                Q::new(314_159_265_358_979_323_847, 100_000_000_000_000_000_000),
            );
            let bounds = |(plo, phi): (Q, Q)| Some((q.mul(plo)?, q.mul(phi)?));
            let (a, b) = fine
                .and_then(bounds)
                .or_else(|| pi_bounds().and_then(bounds))?;
            let (a, b) = if q.signum() < 0 { (b, a) } else { (a, b) };
            let lo_q = (lo.is_finite()).then(|| Q::from_f64(lo)).flatten();
            let hi_q = (hi.is_finite()).then(|| Q::from_f64(hi)).flatten();
            let above_lo = lo == f64::NEG_INFINITY || lo_q.is_some_and(|l| l <= a);
            let below_hi = hi == f64::INFINITY || hi_q.is_some_and(|h| b <= h);
            if above_lo && below_hi {
                return Some(true);
            }
            let out_lo = lo_q.is_some_and(|l| b < l) || lo == f64::INFINITY;
            let out_hi = hi_q.is_some_and(|h| a > h) || hi == f64::NEG_INFINITY;
            return if out_lo || out_hi { Some(false) } else { None };
        }
        Some(at_least(lo)? && at_most(hi)?)
    }

    /// The value as a double (nearest, or near).
    pub fn to_f64(self) -> f64 {
        match self.norm() {
            Ex::Alg { a, b, c } => a.to_f64() + b.to_f64() * (c as f64).sqrt(),
            Ex::Pi(q) => q.to_f64() * std::f64::consts::PI,
        }
    }

    /// The angle in turns of π (`self` = t·π in radians), in `unit`.
    fn half_turns(self, unit: TrigUnit) -> Option<Q> {
        match (unit, self.norm()) {
            (TrigUnit::Radians, Ex::Pi(q)) => Some(q),
            (TrigUnit::Radians, e) if e.is_zero() => Some(Q::ZERO),
            (TrigUnit::Degrees, e) => e.rational()?.div(Q::int(180)),
            (TrigUnit::Grads, e) => e.rational()?.div(Q::int(200)),
            _ => None,
        }
    }

    /// sin(t·π) for t a multiple of 1/6 or 1/4.
    fn sin_pi(t: Q) -> Option<Ex> {
        let two = Q::int(2);
        let t = t.sub(two.mul(Q::int(floor_q(t.div(two)?)))?)?;
        let m = t.mul(Q::int(12))?;
        if !m.is_int() {
            return None;
        }
        let m = m.numer();
        let half = Q::new(1, 2)?;
        let surd = |c: i64, neg: bool| {
            let b = if neg { half.neg() } else { Some(half) };
            b.map(|b| Ex::Alg { a: Q::ZERO, b, c })
        };
        let rat = |n: i128, d: i128| Q::new(n, d).map(Ex::q);
        match m {
            0 | 12 => rat(0, 1),
            2 | 10 => rat(1, 2),
            14 | 22 => rat(-1, 2),
            6 => rat(1, 1),
            18 => rat(-1, 1),
            3 | 9 => surd(2, false),
            15 | 21 => surd(2, true),
            4 | 8 => surd(3, false),
            16 | 20 => surd(3, true),
            _ => None,
        }
    }

    /// An angle in `unit` equal to t·π (t rational).
    fn angle(t: Q, unit: TrigUnit) -> Option<Ex> {
        Some(match unit {
            TrigUnit::Radians => Ex::Pi(t).norm(),
            TrigUnit::Degrees => Ex::q(t.mul(Q::int(180))?),
            TrigUnit::Grads => Ex::q(t.mul(Q::int(200))?),
        })
    }

    /// Inverse trigonometric values that are multiples of π/12: the angle
    /// in [lo, hi]·π (in turns of π) whose sine (`cos`: cosine; `tan`:
    /// tangent) is `self`.
    fn arc(self, kind: Func, unit: TrigUnit) -> Option<Ex> {
        let candidates: &[(i128, i128)] = match kind {
            Func::Asin => &[
                (-1, 2),
                (-1, 3),
                (-1, 4),
                (-1, 6),
                (0, 1),
                (1, 6),
                (1, 4),
                (1, 3),
                (1, 2),
            ],
            Func::Acos => &[
                (0, 1),
                (1, 6),
                (1, 4),
                (1, 3),
                (1, 2),
                (2, 3),
                (3, 4),
                (5, 6),
                (1, 1),
            ],
            Func::Atan => &[(-1, 3), (-1, 4), (-1, 6), (0, 1), (1, 6), (1, 4), (1, 3)],
            _ => return None,
        };
        for &(n, d) in candidates {
            let t = Q::new(n, d)?;
            let s = Ex::sin_pi(t)?;
            let c = Ex::sin_pi(t.add(Q::new(1, 2)?)?)?;
            let v = match kind {
                Func::Asin => s,
                Func::Acos => c,
                _ => s.div(c)?,
            };
            if v.sub(self)?.is_zero() {
                return Ex::angle(t, unit);
            }
        }
        None
    }

    /// `pi_q` as an exact value (k = 0: rational; k = 1: q·π).
    pub fn from_piq(v: PiQ) -> Option<Ex> {
        match v.k {
            0 => Some(Ex::q(v.q)),
            1 => Some(Ex::Pi(v.q).norm()),
            _ => None,
        }
    }
}

/// `e` at x = `x`, exactly (`None` if undefined there, or not exactly
/// computable here). `lits` gives the typed literals' exact values; a
/// slider is its double.
pub fn eval(
    e: &Expr,
    x: Ex,
    unit: TrigUnit,
    lits: &ExactLiterals,
    vars: &dyn crate::compile::VariableValues,
) -> Option<Ex> {
    let ev = |a: &Expr| eval(a, x, unit, lits, vars);
    match e {
        Expr::Num(v) => match lits.exact(*v) {
            Some(q) => Some(Ex::q(q)),
            // The angle-unit factor a derivative carries (π/180, π/200):
            // no typed number (callers check that none was typed with
            // these bits).
            None if unit != TrigUnit::Radians
                && v.to_bits() == unit.to_radians_factor().to_bits() =>
            {
                Some(Ex::Pi(Q::new(1, i128::from(unit.full_turn() as i64 / 2))?))
            }
            None => None,
        },
        Expr::Const(Constant::Pi) => Some(Ex::Pi(Q::ONE)),
        Expr::Const(Constant::E) => None,
        Expr::X => Some(x),
        Expr::Y => None,
        Expr::Var(name) => Some(Ex::q(Q::from_f64(
            vars.value(name)
                .unwrap_or(crate::compile::DEFAULT_VARIABLE_VALUE),
        )?)),
        Expr::Neg(a) => ev(a)?.neg(),
        Expr::Degrees(a) => (unit == TrigUnit::Degrees).then(|| ev(a)).flatten(),
        // eᵇ: exactly only e⁰ = 1.
        Expr::Bin(BinOp::Pow, a, b) if matches!(**a, Expr::Const(Constant::E)) => {
            ev(b)?.is_zero().then(|| Ex::int(1))
        }
        Expr::Bin(op, a, b) => {
            let u = ev(a)?;
            match op {
                BinOp::Add => u.add(ev(b)?),
                BinOp::Sub => u.sub(ev(b)?),
                BinOp::Mul => u.mul(ev(b)?),
                BinOp::Div => u.div(ev(b)?),
                BinOp::Pow => pow(u, b, x, unit, lits, vars),
            }
        }
        Expr::Call(f, args) => {
            let arg = |i: usize| args.get(i).and_then(&ev);
            let a0 = arg(0)?;
            let trig = |s: i32| -> Option<Ex> {
                // s: 0 sin, 1 cos, 2 tan.
                let t = a0.half_turns(unit)?;
                let sin = Ex::sin_pi(t)?;
                let cos = Ex::sin_pi(t.add(Q::new(1, 2)?)?)?;
                match s {
                    0 => Some(sin),
                    1 => Some(cos),
                    _ => sin.div(cos),
                }
            };
            match f {
                Func::Sin => trig(0),
                Func::Cos => trig(1),
                Func::Tan => trig(2),
                Func::Csc => Ex::int(1).div(trig(0)?),
                Func::Sec => Ex::int(1).div(trig(1)?),
                Func::Cot => trig(1)?.div(trig(0)?),
                Func::Asin | Func::Acos | Func::Atan => a0.arc(*f, unit),
                Func::Sqrt => a0.sqrt(),
                Func::Cbrt => a0.nth_root(3),
                Func::Root => {
                    let n = arg(1)?.rational()?;
                    let n = u32::try_from(n.as_int()?).ok()?;
                    (n >= 2).then(|| a0.nth_root(n)).flatten()
                }
                Func::Exp => a0.is_zero().then(|| Ex::int(1)),
                // The hyperbolic functions at 0 (and acosh at 1).
                Func::Sinh | Func::Tanh | Func::Asinh | Func::Atanh => {
                    a0.is_zero().then(|| Ex::int(0))
                }
                Func::Cosh | Func::Sech => a0.is_zero().then(|| Ex::int(1)),
                Func::Acosh => (a0.rational()? == Q::ONE).then(|| Ex::int(0)),
                // n! for a small whole n ≥ 0.
                Func::Factorial => {
                    let n = a0.rational()?;
                    if !n.is_int() || n.signum() < 0 || n.numer() > 30 {
                        return None;
                    }
                    Some(Ex::int((1..=n.numer()).product()))
                }
                // log_b(1) = 0, log_b(b) = 1.
                Func::LogBase => {
                    let x = arg(1)?;
                    let b = a0.rational()?;
                    if b.signum() <= 0 || b == Q::ONE {
                        return None;
                    }
                    match x.rational()? {
                        r if r == Q::ONE => Some(Ex::int(0)),
                        r if r == b => Some(Ex::int(1)),
                        _ => None,
                    }
                }
                Func::Ln => (a0.rational()? == Q::ONE).then(|| Ex::int(0)),
                Func::Log => {
                    // log₁₀ of an integer power of 10.
                    let r = a0.rational()?;
                    for k in -18i32..=18 {
                        let p = Q::int(10).powi(k.into())?;
                        if p == r {
                            return Some(Ex::int(k.into()));
                        }
                    }
                    None
                }
                Func::Abs => match a0.sign()? {
                    s if s < 0 => a0.neg(),
                    _ => Some(a0),
                },
                Func::Sign => Some(Ex::int(a0.sign()?.into())),
                Func::Floor | Func::Ceil | Func::Round => {
                    let r = a0.rational()?;
                    let fl = floor_q(r);
                    Some(Ex::int(match f {
                        Func::Floor => fl,
                        Func::Ceil => {
                            if r.is_int() {
                                fl
                            } else {
                                fl + 1
                            }
                        }
                        _ => floor_q(r.add(Q::new(1, 2)?)?),
                    }))
                }
                Func::Min | Func::Max => {
                    let mut best = a0;
                    for i in 1..args.len() {
                        let v = arg(i)?;
                        let d = v.sub(best)?.sign()?;
                        if (*f == Func::Min && d < 0) || (*f == Func::Max && d > 0) {
                            best = v;
                        }
                    }
                    Some(best)
                }
                _ => None,
            }
        }
    }
}

/// `u^b` (b the exponent's tree) as the graphing evaluator defines it: an
/// exponent varying with x needs a positive base (or 0 to a positive
/// power); a constant one is a real power (odd roots of negatives for a
/// typed fraction).
fn pow(
    u: Ex,
    b: &Expr,
    x: Ex,
    unit: TrigUnit,
    lits: &ExactLiterals,
    vars: &dyn crate::compile::VariableValues,
) -> Option<Ex> {
    let v = eval(b, x, unit, lits, vars)?;
    let r = v.rational()?;
    if b.contains_x() {
        let s = u.sign()?;
        if s < 0 || (s == 0 && r.signum() <= 0) {
            return None;
        }
    }
    if r.is_int() {
        return u.powi(i64::try_from(r.numer()).ok()?);
    }
    // A typed fraction p/q: real roots of negatives for odd q.
    let (p, q) = (r.numer(), r.denom());
    let typed = crate::compile::syntactic_rational(b).is_some();
    if u.sign()? < 0 && (!typed || q % 2 == 0) {
        return None;
    }
    if u.is_zero() {
        return (p > 0).then_some(u);
    }
    u.nth_root(u32::try_from(q).ok()?)?
        .powi(i64::try_from(p).ok()?)
}

// ------------------------------------------------------------- polynomials

/// p′.
pub fn derivative(p: &Poly) -> Option<Poly> {
    let c = p.coefficients();
    let mut d = Vec::new();
    for (i, ci) in c.iter().enumerate().skip(1) {
        d.push(ci.mul(Q::int(i as i128))?);
    }
    Some(Poly::from_coefficients(d))
}

fn divisors(n: i128) -> Option<Vec<i128>> {
    let n = n.abs();
    if n == 0 || n > 1_000_000_000_000 {
        return None;
    }
    let mut out = Vec::new();
    let mut i = 1i128;
    while i * i <= n {
        if n % i == 0 {
            out.push(i);
            if i * i != n {
                out.push(n / i);
            }
        }
        i += 1;
        if out.len() > 2048 {
            return None;
        }
    }
    Some(out)
}

/// The distinct real roots of `p` that can be written exactly: its
/// rational roots, and the roots of what is left when that is a
/// quadratic. Roots of a remaining cubic or higher factor are left out.
pub fn exact_roots(p: &Poly) -> Vec<Ex> {
    let mut out: Vec<Ex> = Vec::new();
    let Some(deg) = p.degree() else {
        return out;
    };
    if deg == 0 {
        return out;
    }
    let mut rest = p.clone();
    let push = |e: Ex, out: &mut Vec<Ex>| {
        if !out.contains(&e) {
            out.push(e);
        }
    };
    // Rational roots: p/q with p | a₀ and q | aₙ of the integer form.
    let rational_roots = |rest: &Poly| -> Option<Vec<Q>> {
        let c = rest.coefficients();
        let mut l = 1i128;
        for q in c {
            let d = q.denom();
            let g = gcd_i(l, d);
            l = l.checked_mul(d / g)?;
        }
        let ints: Vec<i128> = c
            .iter()
            .map(|q| {
                q.mul(Q::int(l))
                    .and_then(|v| v.is_int().then_some(v.numer()))
            })
            .collect::<Option<_>>()?;
        // Zero roots first.
        let lowest = ints.iter().position(|v| *v != 0)?;
        let mut found = Vec::new();
        if lowest > 0 {
            found.push(Q::ZERO);
        }
        let (a0, an) = (ints[lowest], *ints.last()?);
        let (ps, qs) = (divisors(a0)?, divisors(an)?);
        if ps.len() * qs.len() > 20_000 {
            return None;
        }
        for &pp in &ps {
            for &qq in &qs {
                for s in [1i128, -1] {
                    let r = Q::new(s * pp, qq)?;
                    if !found.contains(&r) && rest.eval(r)? == Q::ZERO {
                        found.push(r);
                    }
                }
            }
        }
        Some(found)
    };
    if let Some(rs) = rational_roots(&rest) {
        for r in rs {
            push(Ex::q(r), &mut out);
            // Divide out every copy of (x − r).
            let lin = Poly::from_coefficients(vec![r.neg().unwrap_or(Q::ZERO), Q::ONE]);
            while let Some((qt, rm)) = rest.divmod(&lin) {
                if !rm.is_zero() {
                    break;
                }
                rest = qt;
            }
        }
    }
    match rest.degree() {
        Some(1) => {
            let c = rest.coefficients();
            if let Some(r) = c[0].neg().and_then(|n| n.div(c[1])) {
                push(Ex::q(r), &mut out);
            }
        }
        Some(2) => {
            let c = rest.coefficients();
            let (a, b, cc) = (c[2], c[1], c[0]);
            // (−b ± √(b² − 4ac)) / 2a.
            let disc = sq(b).and_then(|b2| {
                Q::int(4)
                    .mul(a)
                    .and_then(|v| v.mul(cc))
                    .and_then(|v| b2.sub(v))
            });
            if let Some(d) = disc
                && d.signum() >= 0
                && let Some(s) = Ex::q(d).sqrt()
                && let Some(den) = Q::int(2).mul(a)
            {
                for sg in [1i128, -1] {
                    let r = Ex::q(b.neg().unwrap_or(Q::ZERO))
                        .add(s.mul(Ex::int(sg)).unwrap_or(s))
                        .and_then(|v| v.div(Ex::q(den)));
                    if let Some(r) = r {
                        push(r, &mut out);
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn gcd_i(mut a: i128, mut b: i128) -> i128 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

// ------------------------------------------------------------- display

/// A rational in the panel's style: small fractions as `p/q`, terminating
/// decimals in full, otherwise a fraction of moderate size.
fn rational_text(q: Q) -> Option<String> {
    let (n, d) = (q.numer(), q.denom());
    if d == 1 {
        return Some(int_text(n));
    }
    // Small fractions as p/q (1/2, −9/4, 19/36); a large one whose decimal
    // ends reads better that way (2000000.5, not 4000001/2).
    let terminates = {
        let mut dd = d;
        for p in [2, 5] {
            while dd % p == 0 {
                dd /= p;
            }
        }
        dd == 1
    };
    if d <= 16 && n.unsigned_abs() <= 1_000_000_000 && (n.unsigned_abs() <= 1000 || !terminates) {
        return Some(Nice::Rational(i64::try_from(n).ok()?, i64::try_from(d).ok()?).to_string());
    }
    // A terminating decimal (d = 2ᵃ5ᵇ) of at most 17 significant digits.
    let mut dd = d;
    let mut k = 0u32;
    while dd % 10 == 0 || dd % 2 == 0 || dd % 5 == 0 {
        if dd % 10 == 0 {
            dd /= 10;
        } else if dd % 2 == 0 {
            dd /= 2;
        } else {
            dd /= 5;
        }
        k += 1;
        if k > 40 {
            break;
        }
    }
    if dd == 1 {
        // n/d = n·(10ᵏ/d)/10ᵏ with k large enough.
        for k in 1..=30u32 {
            let p = 10i128.checked_pow(k)?;
            if p % d == 0 {
                let m = n.checked_mul(p / d)?;
                let neg = m < 0;
                let digits = m.unsigned_abs().to_string();
                let k = k as usize;
                let (int, frac) = if digits.len() > k {
                    (
                        digits[..digits.len() - k].to_string(),
                        digits[digits.len() - k..].to_string(),
                    )
                } else {
                    ("0".into(), format!("{:0>k$}", digits))
                };
                let frac = frac.trim_end_matches('0');
                let all = format!("{int}{frac}");
                let sig = all.trim_start_matches('0').len();
                if sig > 17 {
                    return None;
                }
                let sign = if neg { MINUS } else { "" };
                // Below 10⁻⁵, like the decimals elsewhere: m×10ⁿ.
                if int == "0" && frac.len() > sig + 4 {
                    let digits = all.trim_start_matches('0');
                    let e = -((frac.len() - digits.len()) as i32) - 1;
                    let m = if digits.len() > 1 {
                        format!("{}.{}", &digits[..1], &digits[1..])
                    } else {
                        digits.to_string()
                    };
                    return Some(format!("{sign}{m}×10{}", superscript(e)));
                }
                return Some(if frac.is_empty() {
                    format!("{sign}{int}")
                } else {
                    format!("{sign}{int}.{frac}")
                });
            }
        }
    }
    if n.unsigned_abs() <= 1_000_000 && d <= 1_000_000 {
        return Some(Nice::Rational(i64::try_from(n).ok()?, i64::try_from(d).ok()?).to_string());
    }
    None
}

fn int_text(n: i128) -> String {
    let sign = if n < 0 { MINUS } else { "" };
    let digits = n.unsigned_abs().to_string();
    // From 10¹⁶ on, m×10ⁿ (exactly: every nonzero digit kept).
    if digits.len() > 16 {
        let kept = digits.trim_end_matches('0');
        let m = if kept.len() > 1 {
            format!("{}.{}", &kept[..1], &kept[1..])
        } else {
            kept.to_string()
        };
        return format!("{sign}{m}×10{}", superscript(digits.len() as i32 - 1));
    }
    format!("{sign}{digits}")
}

impl Ex {
    /// The closed form, for the panel (`None` if it would be unwieldy).
    pub fn text(self) -> Option<String> {
        match self.norm() {
            Ex::Pi(q) => {
                let (n, d) = (
                    i64::try_from(q.numer()).ok()?,
                    i64::try_from(q.denom()).ok()?,
                );
                (n.unsigned_abs() <= 1_000_000 && d <= 1_000_000)
                    .then(|| Nice::Pi(n, d).to_string())
            }
            Ex::Alg { a, b, c } => {
                if b.is_zero() {
                    return rational_text(a);
                }
                // Over a common denominator: (A ± B√c)/d.
                let d = {
                    let (x, y) = (a.denom(), b.denom());
                    x.checked_mul(y / gcd_i(x, y))?
                };
                let big_a = a.mul(Q::int(d))?.numer();
                let big_b = b.mul(Q::int(d))?.numer();
                if big_a.unsigned_abs() > 1_000_000
                    || big_b.unsigned_abs() > 1_000_000
                    || d > 1_000_000
                {
                    return None;
                }
                let coef = |v: i128| {
                    if v.unsigned_abs() == 1 {
                        String::new()
                    } else {
                        v.unsigned_abs().to_string()
                    }
                };
                let surd = format!("{}√{c}", coef(big_b));
                let num = if big_a == 0 {
                    format!("{}{surd}", if big_b < 0 { MINUS } else { "" })
                } else {
                    format!(
                        "{} {} {surd}",
                        int_text(big_a),
                        if big_b < 0 { MINUS } else { "+" }
                    )
                };
                Some(if d == 1 {
                    num
                } else if big_a == 0 {
                    format!("{num}/{d}")
                } else {
                    format!("({num})/{d}")
                })
            }
        }
    }

    /// `k`-multiple text for a period: `2kπ`, `kπ/2`, `360k`.
    pub fn times_k(self) -> Option<String> {
        match self.norm() {
            Ex::Pi(q) => Some(
                Nice::Pi(
                    i64::try_from(q.numer()).ok()?,
                    i64::try_from(q.denom()).ok()?,
                )
                .times_k(),
            ),
            Ex::Alg { a, b, .. } if b.is_zero() => Some(
                Nice::Rational(
                    i64::try_from(a.numer()).ok()?,
                    i64::try_from(a.denom()).ok()?,
                )
                .times_k(),
            ),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: i128, d: i128) -> Q {
        Q::new(n, d).unwrap()
    }

    #[test]
    fn arithmetic_and_text() {
        let r2 = Ex::int(2).sqrt().unwrap();
        assert_eq!(r2.text().unwrap(), "√2");
        assert_eq!(r2.mul(r2).unwrap(), Ex::int(2));
        let phi = Ex::int(1)
            .add(Ex::int(5).sqrt().unwrap())
            .unwrap()
            .div(Ex::int(2))
            .unwrap();
        assert_eq!(phi.text().unwrap(), "(1 + √5)/2");
        assert_eq!(Ex::q(q(1, 2)).text().unwrap(), "1/2");
        assert_eq!(Ex::Pi(q(-1, 2)).text().unwrap(), "−π/2");
        assert_eq!(
            Ex::q(q(16000000000001, 16)).text().unwrap(),
            "1000000000000.0625"
        );
        assert_eq!(
            Ex::int(8).sqrt().unwrap().neg().unwrap().text().unwrap(),
            "−2√2"
        );
        assert_eq!(phi.sign(), Some(1));
        assert_eq!(Ex::int(1).sub(r2).unwrap().sign(), Some(-1));
        assert_eq!(r2.within(1.414, 1.415), Some(true));
        assert_eq!(r2.within(1.415, 2.0), Some(false));
        assert_eq!(Ex::Pi(q(1, 2)).within(1.57, 1.571), Some(true));
        assert_eq!(Ex::Pi(q(1, 2)).within(1.5709, 2.0), Some(false));
    }

    #[test]
    fn trig_values_at_special_angles() {
        let lits = ExactLiterals::none();
        let vars = ();
        let f = |s: &str, x: Ex, u: TrigUnit| {
            let eq = crate::Equation::parse(&format!("y={s}")).unwrap();
            let (_, e) = eq.explicit().unwrap();
            eval(e, x, u, &lits, &vars)
        };
        let half_pi = Ex::Pi(q(1, 2));
        assert_eq!(f("cos(x)", half_pi, TrigUnit::Radians), Some(Ex::int(0)));
        assert_eq!(f("sin(x)", half_pi, TrigUnit::Radians), Some(Ex::int(1)));
        assert_eq!(
            f("sin(x)", Ex::Pi(q(1, 4)), TrigUnit::Radians).and_then(Ex::text),
            Some("√2/2".into())
        );
        assert_eq!(f("tan(x)", half_pi, TrigUnit::Radians), None);
        assert_eq!(
            f("cos(x)", Ex::int(90), TrigUnit::Degrees),
            Some(Ex::int(0))
        );
        assert_eq!(
            f("atan(x)", Ex::int(1), TrigUnit::Radians),
            Some(Ex::Pi(q(1, 4)))
        );
        assert_eq!(f("x^x", Ex::int(-1), TrigUnit::Radians), None);
        assert_eq!(f("x^0", Ex::int(0), TrigUnit::Radians), None);
        assert_eq!(
            f("(-8)^(1/3)", Ex::int(0), TrigUnit::Radians),
            Some(Ex::int(-2))
        );
    }

    #[test]
    fn polynomial_roots() {
        // x³ − 3x: 0 and ±√3.
        let p = Poly::from_coefficients(vec![Q::ZERO, Q::int(-3), Q::ZERO, Q::ONE]);
        let mut texts: Vec<String> = exact_roots(&p)
            .into_iter()
            .map(|e| e.text().unwrap())
            .collect();
        texts.sort();
        assert_eq!(texts, vec!["0", "−√3", "√3"]);
        // x² − x − 1: the golden ratio and its conjugate.
        let p = Poly::from_coefficients(vec![Q::int(-1), Q::int(-1), Q::ONE]);
        let mut texts: Vec<String> = exact_roots(&p)
            .into_iter()
            .map(|e| e.text().unwrap())
            .collect();
        texts.sort();
        assert_eq!(texts, vec!["(1 + √5)/2", "(1 − √5)/2"]);
        // x² + 1: none.
        let p = Poly::from_coefficients(vec![Q::ONE, Q::ZERO, Q::ONE]);
        assert!(exact_roots(&p).is_empty());
    }
}
