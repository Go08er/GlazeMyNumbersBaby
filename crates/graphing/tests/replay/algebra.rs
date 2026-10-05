//! Exact identities between trees: the simplifier's form of f, and the
//! certifier's trees of f′ and f″, against the canonical tree.
//!
//! Each tree is read as a fraction of polynomials with rational
//! coefficients in x and in *atoms*: the transcendental parts (sin u, eˣ,
//! √u, ln u, π, …), each an independent indeterminate named by its function
//! and its argument's normal form. tan, sec, csc, cot (and their hyperbolic
//! kin) become quotients of sines and cosines; log_b u is ln u / ln b;
//! arcsec u is arccos(1/u) and so on; a root or a power p/q is the atom
//! ᵠ√u to the p. Derivatives are taken in the same field (d sin u = cos u ·
//! u′, in the angle unit; d ln u = u′/u; d ᵠ√u = ᵠ√u·u′/(q·u); …).
//!
//! Two trees with N₁·D₂ = N₂·D₁ as polynomials are equal wherever both are
//! defined: every divisor is nonzero where its tree is defined, and the
//! identity holds for any values of the atoms, in particular the true
//! ones. Relations between atoms (sin² + cos² = 1, (√u)² = u) are not
//! used, so a true identity can go unproven, never the reverse.

use super::eval::{Lits, contains_x, written_rational};
use super::exact;
use super::iv::Unit;
use graphing::ast::{BinOp, Constant, Expr, Func};
use rug::Rational;
use std::collections::BTreeMap;

/// A monomial: (variable, exponent) pairs by variable; variable 0 is x.
type Mono = Vec<(u16, u32)>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Poly(BTreeMap<Mono, Rational>);

/// The most terms a polynomial may grow to before the check gives up.
const MAX_TERMS: usize = 1500;

fn mono_mul(a: &Mono, b: &Mono) -> Mono {
    let mut out: Mono = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        match (a.get(i), b.get(j)) {
            (Some(x), Some(y)) if x.0 == y.0 => {
                out.push((x.0, x.1 + y.1));
                i += 1;
                j += 1;
            }
            (Some(x), Some(y)) if x.0 < y.0 => {
                out.push(*x);
                i += 1;
            }
            (Some(_), Some(y)) => {
                out.push(*y);
                j += 1;
            }
            (Some(x), None) => {
                out.push(*x);
                i += 1;
            }
            (None, Some(y)) => {
                out.push(*y);
                j += 1;
            }
            (None, None) => unreachable!(),
        }
    }
    out
}

impl Poly {
    fn constant(q: Rational) -> Poly {
        let mut m = BTreeMap::new();
        if q != 0 {
            m.insert(Vec::new(), q);
        }
        Poly(m)
    }

    fn var(i: u16) -> Poly {
        let mut m = BTreeMap::new();
        m.insert(vec![(i, 1)], Rational::from(1));
        Poly(m)
    }

    pub fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    fn add(&self, o: &Poly) -> Option<Poly> {
        let mut m = self.0.clone();
        for (k, v) in &o.0 {
            let e = m.entry(k.clone()).or_insert_with(|| Rational::from(0));
            *e += v;
            if *e == 0 {
                m.remove(k);
            }
        }
        (m.len() <= MAX_TERMS).then_some(Poly(m))
    }

    fn neg(&self) -> Poly {
        Poly(
            self.0
                .iter()
                .map(|(k, v)| (k.clone(), -v.clone()))
                .collect(),
        )
    }

    fn sub(&self, o: &Poly) -> Option<Poly> {
        self.add(&o.neg())
    }

    fn mul(&self, o: &Poly) -> Option<Poly> {
        if self.0.len().saturating_mul(o.0.len()) > 16 * MAX_TERMS {
            return None;
        }
        let mut m: BTreeMap<Mono, Rational> = BTreeMap::new();
        for (a, x) in &self.0 {
            for (b, y) in &o.0 {
                let k = mono_mul(a, b);
                let e = m.entry(k).or_insert_with(|| Rational::from(0));
                *e += Rational::from(x * y);
            }
        }
        m.retain(|_, v| *v != 0);
        (m.len() <= MAX_TERMS).then_some(Poly(m))
    }

    /// ∂/∂(variable i).
    fn partial(&self, i: u16) -> Poly {
        let mut m: BTreeMap<Mono, Rational> = BTreeMap::new();
        for (k, v) in &self.0 {
            let Some(pos) = k.iter().position(|(var, _)| *var == i) else {
                continue;
            };
            let e = k[pos].1;
            let mut k2 = k.clone();
            if e == 1 {
                k2.remove(pos);
            } else {
                k2[pos].1 = e - 1;
            }
            let c: Rational = v * Rational::from(e);
            let slot = m.entry(k2).or_insert_with(|| Rational::from(0));
            *slot += c;
        }
        m.retain(|_, v| *v != 0);
        Poly(m)
    }

    fn vars(&self) -> Vec<u16> {
        let mut v: Vec<u16> = self
            .0
            .keys()
            .flat_map(|k| k.iter().map(|(i, _)| *i))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// A normal form, for naming atoms.
    fn key(&self) -> String {
        let mut s = String::new();
        for (k, v) in &self.0 {
            s += &format!("{v}");
            for (i, e) in k {
                s += &format!("·v{i}^{e}");
            }
            s.push(';');
        }
        s
    }
}

/// N/D.
#[derive(Clone, Debug)]
pub struct Frac {
    pub n: Poly,
    pub d: Poly,
}

impl Frac {
    fn of(p: Poly) -> Frac {
        Frac {
            n: p,
            d: Poly::constant(Rational::from(1)),
        }
    }

    fn constant(q: Rational) -> Frac {
        Frac::of(Poly::constant(q))
    }

    fn add(&self, o: &Frac) -> Option<Frac> {
        if self.d == o.d {
            return Some(Frac {
                n: self.n.add(&o.n)?,
                d: self.d.clone(),
            });
        }
        Some(Frac {
            n: self.n.mul(&o.d)?.add(&o.n.mul(&self.d)?)?,
            d: self.d.mul(&o.d)?,
        })
    }

    fn neg(&self) -> Frac {
        Frac {
            n: self.n.neg(),
            d: self.d.clone(),
        }
    }

    fn sub(&self, o: &Frac) -> Option<Frac> {
        self.add(&o.neg())
    }

    fn mul(&self, o: &Frac) -> Option<Frac> {
        Some(Frac {
            n: self.n.mul(&o.n)?,
            d: self.d.mul(&o.d)?,
        })
    }

    fn div(&self, o: &Frac) -> Option<Frac> {
        if o.n.is_zero() {
            return None;
        }
        Some(Frac {
            n: self.n.mul(&o.d)?,
            d: self.d.mul(&o.n)?,
        })
    }

    fn powi(&self, p: i64) -> Option<Frac> {
        if p.unsigned_abs() > 64 {
            return None;
        }
        let mut r = Frac::constant(Rational::from(1));
        for _ in 0..p.unsigned_abs() {
            r = r.mul(self)?;
        }
        if p < 0 {
            Frac::constant(Rational::from(1)).div(&r)
        } else {
            Some(r)
        }
    }

    fn key(&self) -> String {
        let f = self.norm();
        format!("({})/({})", f.n.key(), f.d.key())
    }

    /// With a constant denominator divided through (to 1).
    fn norm(&self) -> Frac {
        match (self.d.0.len(), self.d.0.get(&Vec::new())) {
            (1, Some(c)) if *c != 1 => Frac {
                n: Poly(
                    self.n
                        .0
                        .iter()
                        .map(|(k, v)| (k.clone(), Rational::from(v / c)))
                        .collect(),
                ),
                d: Poly::constant(Rational::from(1)),
            },
            // (−a)/(−b) as a/b: the denominator's highest coefficient > 0.
            _ if self.d.0.iter().next_back().is_some_and(|(_, v)| *v < 0) => Frac {
                n: self.n.neg(),
                d: self.d.neg(),
            },
            _ => self.clone(),
        }
    }

    /// The numerator's highest monomial has a negative coefficient (with a
    /// constant positive denominator): −u's form is the canonical one.
    fn leading_negative(&self) -> bool {
        let f = self.norm();
        f.d.vars().is_empty()
            && f.d.0.get(&Vec::new()).is_some_and(|c| *c > 0)
            && f.n.0.iter().next_back().is_some_and(|(_, v)| *v < 0)
    }

    /// The same value: N₁·D₂ = N₂·D₁.
    pub fn same(&self, o: &Frac) -> Option<bool> {
        Some(self.n.mul(&o.d)?.sub(&o.n.mul(&self.d)?)?.is_zero())
    }
}

/// What an atom is, for its derivative.
#[derive(Clone, Debug)]
enum Atom {
    /// π, e, ln of a constant: constants.
    Const,
    /// f(u) for a named function of one argument.
    Of(&'static str, Frac),
    /// ᵠ√u.
    Root(u32, Frac),
    /// a^b for an exponent varying with x.
    PowVar(Frac, Frac),
    /// mod(a, b)'s floor(a/b).
    Steps,
}

/// The atoms met so far (variable i + 1 is atoms[i]), by name.
pub struct Field<'a> {
    names: Vec<String>,
    atoms: Vec<Atom>,
    lits: &'a Lits,
    vars: &'a [(String, f64)],
    unit: Unit,
}

impl<'a> Field<'a> {
    pub fn new(lits: &'a Lits, vars: &'a [(String, f64)], unit: Unit) -> Field<'a> {
        Field {
            names: Vec::new(),
            atoms: Vec::new(),
            lits,
            vars,
            unit,
        }
    }

    fn atom(&mut self, name: String, a: Atom) -> Frac {
        let i = match self.names.iter().position(|n| *n == name) {
            Some(i) => i,
            None => {
                self.names.push(name);
                self.atoms.push(a);
                self.names.len() - 1
            }
        };
        Frac::of(Poly::var(i as u16 + 1))
    }

    /// f(u) as an atom, its argument in a normal form: a fraction with a
    /// constant denominator is divided through; sin and cos shed whole
    /// quarter turns of a constant term (sin(u + π/2) = cos u, sin(u + π)
    /// = −sin u) and take the argument's sign out (sin(−u) = −sin u,
    /// cos(−u) = cos u), as do the odd functions (sinh, atan, asin,
    /// asinh, atanh) and the even ones (cosh, abs).
    fn of(&mut self, f: &'static str, u: Frac) -> Frac {
        let u = u.norm();
        if matches!(f, "sin" | "cos")
            && let Some(r) = self.trig(f == "cos", &u)
        {
            return r;
        }
        let odd = matches!(f, "sinh" | "atan" | "asin" | "asinh" | "atanh");
        let even = matches!(f, "cosh" | "abs");
        if (odd || even) && u.leading_negative() {
            let a = self.plain(f, u.neg().norm());
            return if odd { a.neg() } else { a };
        }
        self.plain(f, u)
    }

    fn plain(&mut self, f: &'static str, u: Frac) -> Frac {
        let name = format!("{f}{}", u.key());
        self.atom(name, Atom::Of(f, u))
    }

    /// sin u (`cos` false) or cos u, reduced: u = v + c·H with H the half
    /// turn and c rational; c's whole quarter turns become a rotation, the
    /// rest stays in the argument; then v's sign is taken out.
    fn trig(&mut self, cos: bool, u: &Frac) -> Option<Frac> {
        if !u.d.vars().is_empty() {
            return None;
        }
        // The constant part in half turns.
        let pi_var = self
            .names
            .iter()
            .position(|n| n == "π")
            .map(|i| i as u16 + 1);
        let half = match self.unit {
            Unit::Radians => None,
            Unit::Degrees => Some(Rational::from(180)),
            Unit::Grads => Some(Rational::from(200)),
        };
        let d0 = u.d.0.get(&Vec::new())?.clone();
        let (c, rest) = match (&half, pi_var) {
            (Some(h), _) => {
                let c0 = u.n.0.get(&Vec::new()).cloned().unwrap_or_default();
                let mut rest = u.n.clone();
                rest.0.remove(&Vec::new());
                (c0 / d0.clone() / h.clone(), rest)
            }
            (None, Some(p)) => {
                let key = vec![(p, 1)];
                let c0 = u.n.0.get(&key).cloned().unwrap_or_default();
                let mut rest = u.n.clone();
                rest.0.remove(&key);
                (c0 / d0.clone(), rest)
            }
            (None, None) => (Rational::from(0), u.n.clone()),
        };
        // Quarter turns: c = q/2 + r with r in [0, 1/2).
        let quarters = (c.clone() * 2u32).floor();
        let r = c - quarters.clone() / Rational::from(2);
        let q = quarters.numer().mod_u(4);
        let shifted = if r == 0 {
            Frac {
                n: rest,
                d: u.d.clone(),
            }
        } else {
            let k = match (&half, pi_var) {
                (Some(h), _) => Frac::constant(r * h.clone()),
                (None, Some(p)) => Frac::of(Poly::var(p)).mul(&Frac::constant(r))?,
                (None, None) => unreachable!(),
            };
            Frac {
                n: rest,
                d: u.d.clone(),
            }
            .add(&k)?
        }
        .norm();
        // sin(v + q quarters): q = 1 → cos v, 2 → −sin v, 3 → −cos v; cos
        // is sin a quarter on.
        let q = (q + if cos { 1 } else { 0 }) % 4;
        let (base_cos, negate) = match q {
            0 => (false, false),
            1 => (true, false),
            2 => (false, true),
            _ => (true, true),
        };
        // The argument's sign out: sin is odd, cos even.
        let (v, flip) = if shifted.leading_negative() {
            (shifted.neg().norm(), !base_cos)
        } else {
            (shifted, false)
        };
        let a = self.plain(if base_cos { "cos" } else { "sin" }, v);
        Some(if negate != flip { a.neg() } else { a })
    }

    fn root(&mut self, q: u32, u: Frac) -> Frac {
        let name = format!("root{q}{}", u.key());
        self.atom(name, Atom::Root(q, u))
    }

    fn pi(&mut self) -> Frac {
        self.atom("π".into(), Atom::Const)
    }

    /// One unit in radians (π/180 in degrees), as a fraction.
    fn unit_rad(&mut self) -> Frac {
        match self.unit {
            Unit::Radians => Frac::constant(Rational::from(1)),
            Unit::Degrees => self
                .pi()
                .div(&Frac::constant(Rational::from(180)))
                .expect("180"),
            Unit::Grads => self
                .pi()
                .div(&Frac::constant(Rational::from(200)))
                .expect("200"),
        }
    }

    fn one() -> Frac {
        Frac::constant(Rational::from(1))
    }

    /// The tree as a fraction, or None (a number with no exact value, too
    /// many terms, a function read no way).
    pub fn read(&mut self, e: &Expr) -> Option<Frac> {
        Some(match e {
            Expr::Num(v, lit) => Frac::constant(self.lits.exact(*v, lit)?),
            Expr::Const(Constant::Pi) => self.pi(),
            Expr::Const(Constant::E) => self.atom("e".into(), Atom::Const),
            Expr::X => Frac::of(Poly::var(0)),
            Expr::Y => return None,
            Expr::Var(n) => Frac::constant(
                self.lits
                    .slider_exact(n, self.vars.iter().find(|(m, _)| m == n)?.1)?,
            ),
            Expr::Neg(a) => self.read(a)?.neg(),
            Expr::Degrees(a) => self.read(a)?,
            Expr::Bin(BinOp::Pow, a, b) => {
                let base = self.read(a)?;
                if let Some((p, q)) = written_rational(b, self.lits) {
                    if q == 1 {
                        return base.powi(p);
                    }
                    return self.root(q as u32, base).powi(p);
                }
                if contains_x(b) {
                    let ex = self.read(b)?;
                    let name = format!("pow{}{}", base.key(), ex.key());
                    return Some(self.atom(name, Atom::PowVar(base, ex)));
                }
                // A constant exponent with an exact rational value.
                let q = exact::eval(b, None, self.lits, self.vars)?;
                let (p, d) = (q.numer().to_i64()?, q.denom().to_u32()?);
                if d == 1 {
                    return base.powi(p);
                }
                return self.root(d, base).powi(p);
            }
            Expr::Bin(op, a, b) => {
                let (a, b) = (self.read(a)?, self.read(b)?);
                match op {
                    BinOp::Add => a.add(&b)?,
                    BinOp::Sub => a.sub(&b)?,
                    BinOp::Mul => a.mul(&b)?,
                    BinOp::Div => a.div(&b)?,
                    BinOp::Pow => unreachable!(),
                }
            }
            Expr::Call(f, args) => self.call(*f, args)?,
        })
    }

    fn call(&mut self, f: Func, args: &[Expr]) -> Option<Frac> {
        use Func::*;
        if args.len() == 2 {
            let (a, b) = (self.read(&args[0])?, self.read(&args[1])?);
            return match f {
                // log_b u = ln u / ln b.
                LogBase => self.of("ln", b).div(&self.of("ln", a)),
                Root => {
                    let n = exact::eval(&args[1], None, self.lits, self.vars)?;
                    if contains_x(&args[1]) || !n.is_integer() || n == 0 {
                        return None;
                    }
                    let k = n.numer().to_i64()?;
                    if k == 1 || k == -1 {
                        return a.powi(k);
                    }
                    self.root(k.unsigned_abs() as u32, a).powi(k.signum())
                }
                Mod => {
                    // a − b·⌊a/b⌋, the floor an atom (derivative 0).
                    let fl = self.atom(format!("floor{}", a.div(&b)?.key()), Atom::Steps);
                    a.sub(&b.mul(&fl)?)
                }
                _ => None,
            };
        }
        if args.len() != 1 {
            return None;
        }
        let u = self.read(&args[0])?;
        let one = Field::one();
        Some(match f {
            Sin => self.of("sin", u),
            Cos => self.of("cos", u),
            Tan => self.of("sin", u.clone()).div(&self.of("cos", u))?,
            Cot => self.of("cos", u.clone()).div(&self.of("sin", u))?,
            Sec => one.div(&self.of("cos", u))?,
            Csc => one.div(&self.of("sin", u))?,
            Sinh => self.of("sinh", u),
            Cosh => self.of("cosh", u),
            Tanh => self.of("sinh", u.clone()).div(&self.of("cosh", u))?,
            Coth => self.of("cosh", u.clone()).div(&self.of("sinh", u))?,
            Sech => one.div(&self.of("cosh", u))?,
            Csch => one.div(&self.of("sinh", u))?,
            Asin => self.of("asin", u),
            Acos => self.of("acos", u),
            Atan => self.of("atan", u),
            Acot => self.of("acot", u),
            Asec => self.of("acos", one.div(&u)?),
            Acsc => self.of("asin", one.div(&u)?),
            Asinh => self.of("asinh", u),
            Acosh => self.of("acosh", u),
            Atanh => self.of("atanh", u),
            Asech => self.of("acosh", one.div(&u)?),
            Acsch => self.of("asinh", one.div(&u)?),
            Acoth => self.of("atanh", one.div(&u)?),
            Sqrt => self.root(2, u),
            Cbrt => self.root(3, u),
            Exp => self.of("exp", u),
            Ln => self.of("ln", u),
            Log => {
                let ten = Frac::constant(Rational::from(10));
                self.of("ln", u).div(&self.of("ln", ten))?
            }
            Abs => self.of("abs", u),
            Sign => self.atom(format!("sign{}", u.key()), Atom::Steps),
            Floor => self.atom(format!("floor{}", u.key()), Atom::Steps),
            Ceil => self.atom(format!("ceil{}", u.key()), Atom::Steps),
            Round => self.atom(format!("round{}", u.key()), Atom::Steps),
            _ => return None,
        })
    }

    /// d/dx of variable i (x or an atom), where the atom is differentiable.
    fn dvar(&mut self, i: u16) -> Option<Frac> {
        if i == 0 {
            return Some(Field::one());
        }
        let a = self.atoms[i as usize - 1].clone();
        let me = Frac::of(Poly::var(i));
        let one = Field::one();
        Some(match a {
            Atom::Const | Atom::Steps => Frac::constant(Rational::from(0)),
            Atom::Root(q, u) => {
                // d u^(1/q) = u^(1/q)·u′/(q·u).
                let du = self.d(&u)?;
                me.mul(&du)?
                    .div(&u.mul(&Frac::constant(Rational::from(q)))?)?
            }
            Atom::PowVar(a, b) => {
                // d aᵇ = aᵇ·(b′·ln a + b·a′/a).
                let (da, db) = (self.d(&a)?, self.d(&b)?);
                let ln = self.of("ln", a.clone());
                me.mul(&db.mul(&ln)?.add(&b.mul(&da)?.div(&a)?)?)?
            }
            Atom::Of(f, u) => {
                let du = self.d(&u)?;
                let k = self.unit_rad();
                let sq = u.mul(&u)?;
                let inner = match f {
                    "sin" => self.of("cos", u.clone()).mul(&k)?,
                    "cos" => self.of("sin", u.clone()).mul(&k)?.neg(),
                    "sinh" => self.of("cosh", u.clone()),
                    "cosh" => self.of("sinh", u.clone()),
                    "exp" => me.clone(),
                    "ln" => one.div(&u)?,
                    "abs" => self.atom(format!("sign{}", u.key()), Atom::Steps),
                    // Inverse trig gives the unit: 1/k times the radian rate.
                    "atan" => one.div(&one.add(&sq)?)?.div(&k)?,
                    "acot" => one.div(&one.add(&sq)?)?.div(&k)?.neg(),
                    "asin" => one.div(&self.root(2, one.sub(&sq)?))?.div(&k)?,
                    "acos" => one.div(&self.root(2, one.sub(&sq)?))?.div(&k)?.neg(),
                    "asinh" => one.div(&self.root(2, sq.add(&one)?))?,
                    "acosh" => one.div(&self.root(2, sq.sub(&one)?))?,
                    "atanh" => one.div(&one.sub(&sq)?)?,
                    _ => return None,
                };
                inner.mul(&du)?
            }
        })
    }

    /// d/dx of a fraction: (N′D − ND′)/D².
    pub fn d(&mut self, f: &Frac) -> Option<Frac> {
        let dn = self.dpoly(&f.n)?;
        let dd = self.dpoly(&f.d)?;
        let num = dn
            .mul(&Frac::of(f.d.clone()))?
            .sub(&Frac::of(f.n.clone()).mul(&dd)?)?;
        num.div(&Frac::of(f.d.mul(&f.d)?))
    }

    fn dpoly(&mut self, p: &Poly) -> Option<Frac> {
        let mut acc = Frac::constant(Rational::from(0));
        for i in p.vars() {
            let part = p.partial(i);
            let dv = self.dvar(i)?;
            acc = acc.add(&Frac::of(part).mul(&dv)?)?;
        }
        Some(acc)
    }
}

/// Which of the certifier's trees are exactly the canonical tree's (f's
/// simplified form ≡ f, f′'s tree ≡ f′, f″'s ≡ f″).
#[derive(Clone, Copy, Debug, Default)]
pub struct Verified {
    pub f: bool,
    pub d: [bool; 2],
}

pub fn verify(
    f: &Expr,
    f_eval: Option<&Expr>,
    d: [Option<&Expr>; 2],
    lits: &Lits,
    vars: &[(String, f64)],
    unit: Unit,
) -> Verified {
    let mut out = Verified::default();
    let mut fld = Field::new(lits, vars, unit);
    let Some(base) = fld.read(f) else { return out };
    if let Some(e) = f_eval
        && let Some(g) = fld.read(e)
    {
        out.f = base.same(&g) == Some(true);
    }
    let mut cur = base;
    for (k, dk) in d.iter().enumerate() {
        let Some(next) = fld.d(&cur) else { break };
        if let Some(t) = dk
            && let Some(g) = fld.read(t)
        {
            out.d[k] = next.same(&g) == Some(true);
        }
        cur = next;
    }
    out
}

/// `a` and `b` (negated with `negate`) are the same function wherever both
/// are defined, shown exactly.
pub fn same_trees(
    a: &Expr,
    b: &Expr,
    negate: bool,
    lits: &Lits,
    vars: &[(String, f64)],
    unit: Unit,
) -> bool {
    let mut fld = Field::new(lits, vars, unit);
    let (Some(x), Some(y)) = (fld.read(a), fld.read(b)) else {
        return false;
    };
    let y = if negate { y.neg() } else { y };
    x.same(&y) == Some(true)
}

/// The number (p/d)·πᵏ (k = 0 or 1) as a tree of integers.
pub fn piq_expr(p: i64, d: i64, k: i32) -> Option<Expr> {
    let int = |v: i64| {
        // (Beyond 2⁵³ a whole number is no exact double: `Expr::num`.)
        let n = Expr::num(v.unsigned_abs() as f64);
        if v < 0 { Expr::Neg(Box::new(n)) } else { n }
    };
    let q = if d == 1 {
        int(p)
    } else {
        Expr::Bin(BinOp::Div, Box::new(int(p)), Box::new(int(d)))
    };
    match k {
        0 => Some(q),
        1 => Some(Expr::Bin(
            BinOp::Mul,
            Box::new(q),
            Box::new(Expr::Const(Constant::Pi)),
        )),
        _ => None,
    }
}

/// A rational function of x alone, N/D, as coefficient lists (index =
/// power), for its behaviour at ±∞.
pub struct Rat {
    pub n: Vec<Rational>,
    pub d: Vec<Rational>,
}

fn coeffs(p: &Poly) -> Option<Vec<Rational>> {
    let mut out: Vec<Rational> = Vec::new();
    for (k, v) in &p.0 {
        let e = match k.as_slice() {
            [] => 0,
            [(0, e)] => *e as usize,
            _ => return None,
        };
        if e > 4096 {
            return None;
        }
        if out.len() <= e {
            out.resize(e + 1, Rational::new());
        }
        out[e] = v.clone();
    }
    while out.last().is_some_and(|c| *c == 0) {
        out.pop();
    }
    Some(out)
}

impl Rat {
    /// f as N/D over the rationals in x alone (no atoms), if it is one.
    pub fn of(f: &Expr, lits: &Lits, vars: &[(String, f64)], unit: Unit) -> Option<Rat> {
        let mut fld = Field::new(lits, vars, unit);
        let fr = fld.read(f)?;
        let (n, d) = (coeffs(&fr.n)?, coeffs(&fr.d)?);
        (!d.is_empty()).then_some(Rat { n, d })
    }

    /// The limit as x → +∞ (`right`) or −∞: Some(Some(L)), or Some(None)
    /// with the sign of an infinite one in `inf`.
    pub fn limit(&self, right: bool) -> Result<Rational, bool> {
        let (dn, dd) = (self.n.len(), self.d.len());
        if self.n.is_empty() || dn < dd {
            return Ok(Rational::new());
        }
        let lead = self.n[dn - 1].clone() / self.d[dd - 1].clone();
        if dn == dd {
            return Ok(lead);
        }
        // ±∞ with the sign of lead·x^(dn − dd).
        let odd = (dn - dd) % 2 == 1;
        let positive = (lead > 0) == (right || !odd);
        Err(positive)
    }

    /// N/D − (m·x + b) → 0 at ±∞: (m, b) when deg N = deg D + 1.
    pub fn line(&self) -> Option<(Rational, Rational)> {
        let (dn, dd) = (self.n.len(), self.d.len());
        if dn != dd + 1 {
            return None;
        }
        // Two steps of long division.
        let m = self.n[dn - 1].clone() / self.d[dd - 1].clone();
        // N − m·x·D: its coefficient at x^(dd − 1).
        let mut r = self.n.clone();
        for (i, c) in self.d.iter().enumerate() {
            r[i + 1] -= Rational::from(&m * c);
        }
        let b = r[dd - 1].clone() / self.d[dd - 1].clone();
        Some((m, b))
    }

    /// N/D as m·x + b exactly (D divides N, the quotient of degree ≤ 1).
    pub fn affine(&self) -> Option<(Rational, Rational)> {
        let d = &self.d;
        let lead = d.last()?.clone();
        let mut r = self.n.clone();
        let mut q = vec![Rational::new(); r.len().saturating_sub(d.len()) + 1];
        while r.len() >= d.len() && !r.is_empty() {
            let k = r.len() - d.len();
            let c = r[r.len() - 1].clone() / lead.clone();
            for (i, v) in d.iter().enumerate() {
                r[i + k] -= Rational::from(&c * v);
            }
            q[k] = c;
            r.pop();
            while r.last().is_some_and(|v| *v == 0) {
                r.pop();
            }
        }
        if r.iter().any(|v| *v != 0) || q.iter().skip(2).any(|v| *v != 0) {
            return None;
        }
        let at = |i: usize| q.get(i).cloned().unwrap_or_default();
        Some((at(1), at(0)))
    }

    /// deg N − deg D.
    pub fn excess(&self) -> i64 {
        self.n.len() as i64 - self.d.len() as i64
    }
}
