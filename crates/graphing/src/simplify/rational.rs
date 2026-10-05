//! Rational functions of x with exact rational coefficients: put in lowest
//! terms (`(x² − 1)/(x − 1)` is x + 1, with the cancelled factor x − 1
//! reported so its zero x = 1 can be shown as a hole), with exact
//! horizontal and oblique asymptotes and limits at ±∞.
//!
//! Every operation is exact over ℚ and checked; anything that would
//! overflow, or grow past [`MAX_DEGREE`], gives `None` rather than an
//! approximation.

use super::lang::ExactLiterals;
use super::q::Q;
use crate::ast::{BinOp, Expr};
use crate::compile::syntactic_rational;

/// The highest degree handled.
pub const MAX_DEGREE: usize = 64;

/// A polynomial Σ cᵢ xⁱ (no trailing zero coefficients; the zero
/// polynomial has none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Poly(Vec<Q>);

impl Poly {
    /// 0.
    pub fn zero() -> Poly {
        Poly(Vec::new())
    }

    /// The constant c.
    pub fn constant(c: Q) -> Poly {
        Poly::from(vec![c])
    }

    /// x.
    pub fn x() -> Poly {
        Poly::from(vec![Q::ZERO, Q::ONE])
    }

    fn from(mut c: Vec<Q>) -> Poly {
        while c.last().is_some_and(|q| q.is_zero()) {
            c.pop();
        }
        Poly(c)
    }

    /// The polynomial with these coefficients, constant term first.
    pub fn from_coefficients(c: Vec<Q>) -> Poly {
        Poly::from(c)
    }

    /// The coefficients, constant term first.
    pub fn coefficients(&self) -> &[Q] {
        &self.0
    }

    /// True for the zero polynomial.
    pub fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    /// The degree (`None` for 0).
    pub fn degree(&self) -> Option<usize> {
        self.0.len().checked_sub(1)
    }

    /// The leading coefficient (0 for the zero polynomial).
    pub fn lead(&self) -> Q {
        self.0.last().copied().unwrap_or(Q::ZERO)
    }

    /// p + q.
    pub fn add(&self, o: &Poly) -> Option<Poly> {
        let n = self.0.len().max(o.0.len());
        let mut c = Vec::with_capacity(n);
        for i in 0..n {
            let a = self.0.get(i).copied().unwrap_or(Q::ZERO);
            let b = o.0.get(i).copied().unwrap_or(Q::ZERO);
            c.push(a.add(b)?);
        }
        Some(Poly::from(c))
    }

    /// −p.
    pub fn neg(&self) -> Option<Poly> {
        Some(Poly(self.0.iter().map(|c| c.neg()).collect::<Option<_>>()?))
    }

    /// p − q.
    pub fn sub(&self, o: &Poly) -> Option<Poly> {
        self.add(&o.neg()?)
    }

    /// c·p.
    pub fn scale(&self, k: Q) -> Option<Poly> {
        Some(Poly::from(
            self.0.iter().map(|c| c.mul(k)).collect::<Option<_>>()?,
        ))
    }

    /// p·q.
    pub fn mul(&self, o: &Poly) -> Option<Poly> {
        if self.is_zero() || o.is_zero() {
            return Some(Poly::zero());
        }
        let n = self.0.len() + o.0.len() - 1;
        if n - 1 > MAX_DEGREE {
            return None;
        }
        let mut c = vec![Q::ZERO; n];
        for (i, a) in self.0.iter().enumerate() {
            for (j, b) in o.0.iter().enumerate() {
                c[i + j] = c[i + j].add(a.mul(*b)?)?;
            }
        }
        Some(Poly::from(c))
    }

    /// pⁿ.
    pub fn pow(&self, n: u32) -> Option<Poly> {
        let mut acc = Poly::constant(Q::ONE);
        for _ in 0..n {
            acc = acc.mul(self)?;
        }
        Some(acc)
    }

    /// (quotient, remainder) of p / d (d ≠ 0).
    pub fn divmod(&self, d: &Poly) -> Option<(Poly, Poly)> {
        let dd = d.degree()?;
        let lead = d.lead();
        let mut r = self.0.clone();
        let mut q = vec![Q::ZERO; self.0.len().saturating_sub(dd).max(1)];
        while r.len() > dd && !r.is_empty() {
            let k = r.len() - 1 - dd;
            let c = r[r.len() - 1].div(lead)?;
            q[k] = c;
            for (j, dj) in d.0.iter().enumerate() {
                r[k + j] = r[k + j].sub(c.mul(*dj)?)?;
            }
            // The leading term is now exactly 0.
            r.pop();
            while r.last().is_some_and(|x| x.is_zero()) {
                r.pop();
            }
        }
        Some((Poly::from(q), Poly::from(r)))
    }

    /// p scaled to leading coefficient 1 (0 stays 0).
    pub fn monic(&self) -> Option<Poly> {
        if self.is_zero() {
            return Some(Poly::zero());
        }
        self.scale(self.lead().recip()?)
    }

    /// The monic greatest common divisor.
    pub fn gcd(&self, o: &Poly) -> Option<Poly> {
        let (mut a, mut b) = (self.monic()?, o.monic()?);
        while !b.is_zero() {
            let (_, r) = a.divmod(&b)?;
            a = b;
            b = r.monic()?;
        }
        a.monic()
    }

    /// p(v), exactly.
    pub fn eval(&self, v: Q) -> Option<Q> {
        let mut acc = Q::ZERO;
        for c in self.0.iter().rev() {
            acc = acc.mul(v)?.add(*c)?;
        }
        Some(acc)
    }

    /// The polynomial as an expression.
    pub fn to_expr(&self) -> Expr {
        let mut terms: Vec<Expr> = Vec::new();
        for (i, c) in self.0.iter().enumerate().rev() {
            if c.is_zero() {
                continue;
            }
            let mono = match i {
                0 => None,
                1 => Some(Expr::X),
                n => Some(Expr::bin(BinOp::Pow, Expr::X, Expr::num(n as f64))),
            };
            let mag = c.abs().unwrap_or(*c);
            let coef = q_expr(mag);
            let term = match (mono, mag == Q::ONE) {
                (None, _) => coef,
                (Some(m), true) => m,
                (Some(m), false) => Expr::bin(BinOp::Mul, coef, m),
            };
            terms.push(if c.signum() < 0 {
                Expr::Neg(Box::new(term))
            } else {
                term
            });
        }
        let mut it = terms.into_iter();
        let Some(first) = it.next() else {
            return Expr::num(0.0);
        };
        it.fold(first, |acc, t| match t {
            Expr::Neg(t) => Expr::bin(BinOp::Sub, acc, *t),
            t => Expr::bin(BinOp::Add, acc, t),
        })
    }
}

/// An exact rational as an expression (`p/q` or an integer), each part
/// exactly (`Expr::int`).
pub fn q_expr(q: Q) -> Expr {
    let n = Expr::int(q.numer().unsigned_abs());
    let v = if q.is_int() {
        n
    } else {
        Expr::bin(BinOp::Div, n, Expr::int(q.denom().unsigned_abs()))
    };
    if q.signum() < 0 {
        Expr::Neg(Box::new(v))
    } else {
        v
    }
}

/// A rational function N/D in lowest terms, D monic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rational {
    /// Numerator.
    pub num: Poly,
    /// Denominator (monic, ≠ 0).
    pub den: Poly,
}

/// An expression's rational form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RationalForm {
    /// In lowest terms.
    pub reduced: Rational,
    /// The monic common factor that was cancelled: where it vanishes (and
    /// the reduced denominator doesn't), the expression has a removable
    /// hole.
    pub cancelled: Poly,
}

/// A limit's direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// x → +∞.
    PosInf,
    /// x → −∞.
    NegInf,
}

/// The limit of a rational function at ±∞.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RationalLimit {
    /// A finite value.
    Finite(Q),
    /// +∞.
    PosInf,
    /// −∞.
    NegInf,
}

impl Rational {
    fn new(num: Poly, den: Poly) -> Option<(Rational, Poly)> {
        if den.is_zero() {
            return None;
        }
        let g = num.gcd(&den)?;
        let g = if num.is_zero() { den.monic()? } else { g };
        let (n, _) = num.divmod(&g)?;
        let (d, _) = den.divmod(&g)?;
        let lead = d.lead();
        let (n, d) = (n.scale(lead.recip()?)?, d.scale(lead.recip()?)?);
        Some((Rational { num: n, den: d }, g))
    }

    /// The reduced function as an expression.
    pub fn to_expr(&self) -> Expr {
        if self.den.degree() == Some(0) {
            return self.num.to_expr();
        }
        Expr::bin(BinOp::Div, self.num.to_expr(), self.den.to_expr())
    }

    /// The horizontal asymptote y = c (deg N ≤ deg D), both sides.
    pub fn horizontal(&self) -> Option<Q> {
        let dn = self.num.degree();
        let dd = self.den.degree()?;
        match dn {
            None => Some(Q::ZERO),
            Some(n) if n < dd => Some(Q::ZERO),
            Some(n) if n == dd => self.num.lead().div(self.den.lead()),
            _ => None,
        }
    }

    /// The oblique asymptote y = m·x + b (deg N = deg D + 1), both sides.
    pub fn oblique(&self) -> Option<(Q, Q)> {
        let n = self.num.degree()?;
        let d = self.den.degree()?;
        if n != d + 1 {
            return None;
        }
        let (q, _) = self.num.divmod(&self.den)?;
        let c = q.coefficients();
        Some((c.get(1).copied()?, c.first().copied().unwrap_or(Q::ZERO)))
    }

    /// The limit at ±∞.
    pub fn limit(&self, dir: Dir) -> Option<RationalLimit> {
        if let Some(h) = self.horizontal() {
            return Some(RationalLimit::Finite(h));
        }
        let excess = self.num.degree()? - self.den.degree()?;
        let mut sign = self.num.lead().signum() * self.den.lead().signum();
        if dir == Dir::NegInf && excess % 2 == 1 {
            sign = -sign;
        }
        Some(if sign > 0 {
            RationalLimit::PosInf
        } else {
            RationalLimit::NegInf
        })
    }
}

/// `e` as a rational function of x with exact coefficients, if it is one
/// (x, exact constants, + − × ÷ and integer powers).
pub fn rational_form(e: &Expr, lits: &ExactLiterals) -> Option<RationalForm> {
    let (n, d) = parts(e, lits)?;
    let (reduced, g) = Rational::new(n, d)?;
    Some(RationalForm {
        reduced,
        cancelled: g,
    })
}

/// (numerator, denominator), not yet reduced.
fn parts(e: &Expr, lits: &ExactLiterals) -> Option<(Poly, Poly)> {
    let one = || Poly::constant(Q::ONE);
    Some(match e {
        Expr::X => (Poly::x(), one()),
        Expr::Num(v, lit) => (Poly::constant(lit.q(*v)?), one()),
        Expr::Neg(a) => {
            let (n, d) = parts(a, lits)?;
            (n.neg()?, d)
        }
        Expr::Degrees(a) => parts(a, lits)?,
        Expr::Bin(op, a, b) => match op {
            BinOp::Pow => {
                let (p, q) = syntactic_rational(b, lits)?;
                if q != 1 || p.unsigned_abs() as usize > MAX_DEGREE {
                    return None;
                }
                let (n, d) = parts(a, lits)?;
                if p >= 0 {
                    if p == 0 && n.is_zero() {
                        return None;
                    }
                    (n.pow(p as u32)?, d.pow(p as u32)?)
                } else {
                    if n.is_zero() {
                        return None;
                    }
                    (d.pow(p.unsigned_abs())?, n.pow(p.unsigned_abs())?)
                }
            }
            _ => {
                let (an, ad) = parts(a, lits)?;
                let (bn, bd) = parts(b, lits)?;
                match op {
                    BinOp::Add => (an.mul(&bd)?.add(&bn.mul(&ad)?)?, ad.mul(&bd)?),
                    BinOp::Sub => (an.mul(&bd)?.sub(&bn.mul(&ad)?)?, ad.mul(&bd)?),
                    BinOp::Mul => (an.mul(&bn)?, ad.mul(&bd)?),
                    BinOp::Div => {
                        if bn.is_zero() {
                            return None;
                        }
                        (an.mul(&bd)?, ad.mul(&bn)?)
                    }
                    BinOp::Pow => unreachable!(),
                }
            }
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equation::Equation;

    fn form(s: &str) -> Option<RationalForm> {
        let text = format!("y={s}");
        let eq = Equation::parse(&text).unwrap();
        let lits = ExactLiterals::of(&text, Default::default()).unwrap();
        rational_form(eq.explicit().unwrap().1, &lits)
    }

    fn q(n: i128, d: i128) -> Q {
        Q::new(n, d).unwrap()
    }

    #[test]
    fn cancels_and_reports_the_hole() {
        let f = form("(x^2-1)/(x-1)").unwrap();
        assert_eq!(f.reduced.to_expr().to_string(), "x+1");
        assert_eq!(f.cancelled, Poly::from(vec![q(-1, 1), Q::ONE]));
        assert_eq!(f.cancelled.eval(Q::ONE), Some(Q::ZERO), "hole at x = 1");
    }

    #[test]
    fn asymptotes_are_exact() {
        let f = form("(3x^2+1)/(2x^2-x)").unwrap();
        assert_eq!(f.reduced.horizontal(), Some(q(3, 2)));
        let f = form("(x^2+1)/(x-3)").unwrap();
        assert_eq!(f.reduced.oblique(), Some((Q::ONE, Q::int(3))));
        assert_eq!(f.reduced.limit(Dir::NegInf), Some(RationalLimit::NegInf));
        let f = form("x^3-2x+1/(x-1)").unwrap();
        assert_eq!(f.reduced.horizontal(), None);
        assert_eq!(f.reduced.oblique(), None);
        assert_eq!(f.reduced.limit(Dir::PosInf), Some(RationalLimit::PosInf));
        assert_eq!(f.reduced.limit(Dir::NegInf), Some(RationalLimit::NegInf));
        let f = form("1/(x^2-4)").unwrap();
        assert_eq!(f.reduced.horizontal(), Some(Q::ZERO));
        // Exact decimals: the pole at 10¹² + 1/16 stays exact.
        let f = form("(x-1000000000000)/(x-1000000000000.0625)").unwrap();
        assert_eq!(f.reduced.horizontal(), Some(Q::ONE));
        assert_eq!(f.reduced.den.coefficients()[0], q(-16_000_000_000_001, 16));
        assert!(form("sin(x)/x").is_none());
        assert!(form("x/0").is_none());
    }
}
