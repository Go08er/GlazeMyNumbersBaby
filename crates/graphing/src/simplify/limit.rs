//! Limits at ±∞ by dominant terms ("Gruntz-lite"), kept to what can be
//! proven.
//!
//! As x → +∞ each subexpression is reduced to its **leading term**
//! c·e^{kx}·x^p·(ln x)^m (exact k, p, m; c exact, or an enclosure that
//! excludes 0), or to "tends to 0", "grows faster than any such term",
//! "stays bounded" or "unknown". A sum keeps its largest term; equal
//! terms add exactly, and a sum whose leading terms cancel is unknown
//! (the next term would be needed). Rational functions go through the
//! exact normaliser first. The limit at −∞ is that of f(−x) at +∞.
//!
//! "Unknown" is always allowed; a wrong limit never is.

use super::analysis::{Facts, rec_interval};
use super::lang::{ExactLiterals, to_rec};
use super::period::{PiQ, affine, exact_constant};
use super::q::Q;
use super::rational::{Dir, RationalLimit, rational_form};
use crate::ast::{BinOp, Expr, Func};
use crate::compile::syntactic_rational;
use crate::functions::TrigUnit;
use crate::interval::{DecInterval, Interval, elem};

/// A limit at ±∞.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Limit {
    /// +∞.
    PosInf,
    /// −∞.
    NegInf,
    /// A finite value, exactly q·πᵏ.
    Exact(PiQ),
    /// A finite value, enclosed.
    Approx(Interval),
    /// Not determined.
    Unknown,
}

#[derive(Clone, Copy, Debug)]
enum Coef {
    Exact(PiQ),
    Approx(Interval),
}

impl Coef {
    fn interval(self) -> Interval {
        match self {
            Coef::Exact(v) => {
                let q = v.q.interval();
                if v.k == 0 {
                    q
                } else {
                    let p = DecInterval::new(elem::pi());
                    elem::mul(&DecInterval::new(q), &elem::powi(&p, v.k)).iv
                }
            }
            Coef::Approx(i) => i,
        }
    }

    fn sign(self) -> Option<i32> {
        match self {
            Coef::Exact(v) => Some(v.q.signum()),
            Coef::Approx(i) => {
                if i.lo() > 0.0 {
                    Some(1)
                } else if i.hi() < 0.0 {
                    Some(-1)
                } else {
                    None
                }
            }
        }
    }

    fn approx(i: Interval) -> Option<Coef> {
        (!i.is_empty() && (i.lo() > 0.0 || i.hi() < 0.0)).then_some(Coef::Approx(i))
    }

    fn neg(self) -> Coef {
        match self {
            Coef::Exact(v) => {
                v.q.neg()
                    .map(|q| Coef::Exact(PiQ { q, k: v.k }))
                    .unwrap_or(Coef::Approx(-v_interval(v)))
            }
            Coef::Approx(i) => Coef::Approx(-i),
        }
    }

    fn mul(self, o: Coef) -> Option<Coef> {
        if let (Coef::Exact(a), Coef::Exact(b)) = (self, o)
            && let (Some(q), Some(k)) = (a.q.mul(b.q), a.k.checked_add(b.k))
        {
            return Some(Coef::Exact(PiQ { q, k }));
        }
        let r = elem::mul(
            &DecInterval::new(self.interval()),
            &DecInterval::new(o.interval()),
        );
        Coef::approx(r.iv)
    }

    fn recip(self) -> Option<Coef> {
        if let Coef::Exact(a) = self
            && let Some(q) = a.q.recip()
        {
            return Some(Coef::Exact(PiQ { q, k: -a.k }));
        }
        Coef::approx(elem::recip(&DecInterval::new(self.interval())).iv)
    }

    /// The sum of two coefficients of the same scale, if it doesn't vanish
    /// (or might).
    fn add(self, o: Coef) -> Option<Coef> {
        if let (Coef::Exact(a), Coef::Exact(b)) = (self, o)
            && a.k == b.k
        {
            let q = a.q.add(b.q)?;
            return (!q.is_zero()).then_some(Coef::Exact(PiQ { q, k: a.k }));
        }
        let r = elem::add(
            &DecInterval::new(self.interval()),
            &DecInterval::new(o.interval()),
        );
        Coef::approx(r.iv)
    }

    /// cʳ for a rational r (real-root meaning).
    fn pow(self, r: Q) -> Option<Coef> {
        if let Coef::Exact(a) = self
            && let Some(n) = r.as_int()
            && n.unsigned_abs() <= 64
        {
            let q = a.q.powi(n)?;
            return Some(Coef::Exact(PiQ {
                q,
                k: a.k.checked_mul(i32::try_from(n).ok()?)?,
            }));
        }
        let (p, d) = (
            i32::try_from(r.numer()).ok()?,
            i32::try_from(r.denom()).ok()?,
        );
        let base = DecInterval::new(self.interval());
        let v = if d == 1 {
            elem::powi(&base, p)
        } else {
            elem::pow_rational(&base, p, d)
        };
        Coef::approx(v.iv)
    }
}

fn v_interval(v: PiQ) -> Interval {
    Coef::Exact(v).interval()
}

#[derive(Clone, Copy, Debug)]
struct Term {
    c: Coef,
    k: Q,
    p: Q,
    m: Q,
}

impl Term {
    fn constant(c: Coef) -> Term {
        Term {
            c,
            k: Q::ZERO,
            p: Q::ZERO,
            m: Q::ZERO,
        }
    }

    /// The sign of the scale e^{kx}x^p(ln x)^m against 1: 1 grows, 0 is
    /// constant, −1 decays.
    fn growth(&self) -> i32 {
        for s in [self.k, self.p, self.m] {
            if s.signum() != 0 {
                return s.signum();
            }
        }
        0
    }

    fn scale_cmp(&self, o: &Term) -> std::cmp::Ordering {
        (self.k, self.p, self.m).cmp(&(o.k, o.p, o.m))
    }
}

#[derive(Clone, Copy, Debug)]
enum Asy {
    /// Exactly 0.
    Zero,
    /// The leading term.
    Term(Term),
    /// → 0, no leading term known.
    Small,
    /// → ±∞ faster than every term.
    Huge(i32),
    /// → ±∞ faster than every power x^p·(ln x)^m, but no faster than some
    /// term e^{kx}: e^√x, e^(x + sin x), x^(ln x), times powers. It
    /// outgrows a term with k ≤ 0 and loses to a [`Asy::Huge`]; against a
    /// term with k > 0, the leading term of its exponent g (it is e^g
    /// times a power), when known, decides.
    Mid(i32, Option<Term>),
    /// Bounded within the interval, no limit known.
    Bounded(Interval),
    Unknown,
}

struct Cx<'a> {
    unit: TrigUnit,
    lits: &'a ExactLiterals,
    facts: Facts,
}

impl Cx<'_> {
    fn constant(&self, e: &Expr) -> Asy {
        if let Some(v) = exact_constant(e, self.lits) {
            return if v.q.is_zero() {
                Asy::Zero
            } else {
                Asy::Term(Term::constant(Coef::Exact(v)))
            };
        }
        let Ok(rec) = to_rec(e, self.lits) else {
            return Asy::Unknown;
        };
        let i = rec_interval(&rec, &self.facts);
        if i.lo() == 0.0 && i.hi() == 0.0 {
            return Asy::Zero;
        }
        match Coef::approx(i) {
            Some(c) => Asy::Term(Term::constant(c)),
            None => Asy::Unknown,
        }
    }

    fn quarter(&self) -> PiQ {
        match self.unit {
            TrigUnit::Radians => PiQ {
                q: Q::new(1, 2).expect("½"),
                k: 1,
            },
            TrigUnit::Degrees => PiQ {
                q: Q::int(90),
                k: 0,
            },
            TrigUnit::Grads => PiQ {
                q: Q::int(100),
                k: 0,
            },
        }
    }
}

fn neg(a: Asy) -> Asy {
    match a {
        Asy::Term(t) => Asy::Term(Term { c: t.c.neg(), ..t }),
        Asy::Huge(s) => Asy::Huge(-s),
        Asy::Mid(s, g) => Asy::Mid(-s, g),
        Asy::Bounded(i) => Asy::Bounded(-i),
        other => other,
    }
}

fn add(a: Asy, b: Asy) -> Asy {
    use Asy::*;
    match (a, b) {
        (Unknown, _) | (_, Unknown) => Unknown,
        (Zero, x) | (x, Zero) => x,
        (Huge(s), Huge(t)) => {
            if s == t {
                Huge(s)
            } else {
                Unknown
            }
        }
        // (A Mid is no faster than some term, which a Huge outgrows.)
        (Huge(s), _) | (_, Huge(s)) => Huge(s),
        (Mid(s, g), Mid(t, h)) => {
            if s == t {
                return Mid(s, None);
            }
            // e^g − e^h: the one whose exponent's leading term is larger.
            match g.zip(h).and_then(|(g, h)| term_cmp(&g, &h)) {
                Some(std::cmp::Ordering::Greater) => Mid(s, g),
                Some(std::cmp::Ordering::Less) => Mid(t, h),
                _ => Unknown,
            }
        }
        (Mid(s, g), Term(t)) | (Term(t), Mid(s, g)) => {
            // It outgrows a power (k = 0) or a decaying exponential; against
            // a growing e^{kx}, its exponent's rate decides.
            if t.k.signum() <= 0 {
                return Mid(s, g);
            }
            match g.and_then(|g| below_exp(&g, t.k)) {
                Some(true) => Term(t),
                Some(false) => Mid(s, g),
                None if t.c.sign() == Some(s) => Mid(s, None),
                None => Unknown,
            }
        }
        (Mid(s, g), Small | Bounded(_)) | (Small | Bounded(_), Mid(s, g)) => Mid(s, g),
        (Term(x), Term(y)) => match x.scale_cmp(&y) {
            std::cmp::Ordering::Greater => Term(x),
            std::cmp::Ordering::Less => Term(y),
            std::cmp::Ordering::Equal => match x.c.add(y.c) {
                Some(c) => Term(self::Term { c, ..x }),
                None => Unknown,
            },
        },
        (Term(t), Small) | (Small, Term(t)) => {
            if t.growth() >= 0 {
                Term(t)
            } else {
                Small
            }
        }
        (Term(t), Bounded(_)) | (Bounded(_), Term(t)) => {
            if t.growth() > 0 {
                Term(t)
            } else {
                Unknown
            }
        }
        (Small, Small) => Small,
        _ => Unknown,
    }
}

fn mul(a: Asy, b: Asy) -> Asy {
    use Asy::*;
    match (a, b) {
        (Unknown, _) | (_, Unknown) => Unknown,
        (Zero, _) | (_, Zero) => Zero,
        (Huge(s), Huge(t)) => Huge(s * t),
        (Huge(s), Term(t)) | (Term(t), Huge(s)) => match (t.growth() >= 0, t.c.sign()) {
            (true, Some(c)) => Huge(s * c),
            _ => Unknown,
        },
        (Huge(s), Mid(t, _)) | (Mid(t, _), Huge(s)) => Huge(s * t),
        (Mid(s, _), Mid(t, _)) => Mid(s * t, None),
        // Times a power (even a decaying one: the exponent's rate is the
        // same) or a growing exponential.
        (Mid(s, g), Term(t)) | (Term(t), Mid(s, g)) => match (t.k.signum(), t.c.sign()) {
            (0, Some(c)) => Mid(s * c, g),
            (1, Some(c)) => Mid(s * c, None),
            _ => Unknown,
        },
        (Term(x), Term(y)) => match (x.c.mul(y.c), x.k.add(y.k), x.p.add(y.p), x.m.add(y.m)) {
            (Some(c), Some(k), Some(p), Some(m)) => Term(self::Term { c, k, p, m }),
            _ => Unknown,
        },
        (Term(t), Small) | (Small, Term(t)) => {
            if t.growth() <= 0 {
                Small
            } else {
                Unknown
            }
        }
        (Term(t), Bounded(_)) | (Bounded(_), Term(t)) => {
            if t.growth() < 0 {
                Small
            } else {
                Unknown
            }
        }
        (Small, Small) | (Small, Bounded(_)) | (Bounded(_), Small) => Small,
        _ => Unknown,
    }
}

fn recip(a: Asy) -> Asy {
    match a {
        Asy::Term(t) => match (t.c.recip(), t.k.neg(), t.p.neg(), t.m.neg()) {
            (Some(c), Some(k), Some(p), Some(m)) => Asy::Term(Term { c, k, p, m }),
            _ => Asy::Unknown,
        },
        Asy::Huge(_) | Asy::Mid(..) => Asy::Small,
        _ => Asy::Unknown,
    }
}

/// aʳ for a constant rational r (real-root meaning).
fn pow(a: Asy, r: Q) -> Asy {
    match a {
        Asy::Zero => {
            if r.signum() > 0 {
                Asy::Zero
            } else {
                Asy::Unknown
            }
        }
        Asy::Term(t) => match (t.c.pow(r), t.k.mul(r), t.p.mul(r), t.m.mul(r)) {
            (Some(c), Some(k), Some(p), Some(m)) => Asy::Term(Term { c, k, p, m }),
            _ => Asy::Unknown,
        },
        Asy::Small if r.signum() > 0 => Asy::Small,
        Asy::Huge(s) if r.signum() > 0 => {
            if s > 0 {
                Asy::Huge(1)
            } else if r.denom() % 2 == 1 {
                Asy::Huge(if r.numer() % 2 == 0 { 1 } else { -1 })
            } else {
                Asy::Unknown
            }
        }
        Asy::Huge(_) if r.signum() < 0 => Asy::Small,
        // (A positive power of something beyond every power still is.)
        Asy::Mid(s, g) if r.signum() > 0 => {
            // (e^g)ʳ = e^(r·g).
            let g = g.and_then(|g| {
                let c = g.c.mul(Coef::Exact(PiQ { q: r, k: 0 }))?;
                Some(Term { c, ..g })
            });
            if s > 0 {
                Asy::Mid(1, g)
            } else if r.denom() % 2 == 1 {
                Asy::Mid(if r.numer() % 2 == 0 { 1 } else { -1 }, g)
            } else {
                Asy::Unknown
            }
        }
        Asy::Mid(..) if r.signum() < 0 => Asy::Small,
        _ => Asy::Unknown,
    }
}

fn exp_of(a: Asy) -> Asy {
    match a {
        Asy::Zero | Asy::Small => Asy::Term(Term::constant(Coef::Exact(PiQ { q: Q::ONE, k: 0 }))),
        Asy::Term(t) => match (t.growth(), t.c.sign()) {
            (0, _) => {
                let i = elem::exp(&DecInterval::new(t.c.interval())).iv;
                Coef::approx(i).map_or(Asy::Unknown, |c| Asy::Term(Term::constant(c)))
            }
            (g, _) if g < 0 => Asy::Term(Term::constant(Coef::Exact(PiQ { q: Q::ONE, k: 0 }))),
            (_, Some(s)) if s < 0 => Asy::Small,
            (_, Some(_)) => exp_of_growing(t),
            _ => Asy::Unknown,
        },
        Asy::Huge(s) if s > 0 => Asy::Huge(1),
        Asy::Huge(_) => Asy::Small,
        // e^A for an A beyond every power is beyond e^(x²), so every term.
        Asy::Mid(s, _) if s > 0 => Asy::Huge(1),
        Asy::Mid(..) => Asy::Small,
        Asy::Bounded(i) => Asy::Bounded(elem::exp(&DecInterval::new(i)).iv),
        Asy::Unknown => Asy::Unknown,
    }
}

/// e^(c·e^{kx}·x^p·(ln x)^m) for a term that grows with c > 0. Beyond every
/// term only when the exponent outgrows x (k > 0, p > 1, or x times a
/// growing power of ln x); e^√x, e^(x/ln x) and x^(ln x) are only beyond
/// every power, and e^(c·ln x) is the power x^c itself.
/// Two growing terms' order (the larger outgrows the other), when known.
fn term_cmp(a: &Term, b: &Term) -> Option<std::cmp::Ordering> {
    match a.scale_cmp(b) {
        std::cmp::Ordering::Equal => {
            let (x, y) = (a.c.interval(), b.c.interval());
            if x.lo() > y.hi() {
                Some(std::cmp::Ordering::Greater)
            } else if x.hi() < y.lo() {
                Some(std::cmp::Ordering::Less)
            } else {
                None
            }
        }
        o => Some(o),
    }
}

/// Whether e^g (times a power) is below e^{kx} for k > 0, by g's leading
/// term (growing, with no exponential part): `Some(true)` when g grows
/// slower than k·x, `Some(false)` faster, `None` when that isn't known.
fn below_exp(g: &Term, k: Q) -> Option<bool> {
    let one = Q::ONE;
    if !g.k.is_zero() || g.p > one {
        return None;
    }
    if g.p < one || g.m.signum() < 0 {
        return Some(true);
    }
    if g.m.signum() > 0 {
        return Some(false);
    }
    // c·x against k·x.
    let (c, k) = (g.c.interval(), k.interval());
    if c.hi() < k.lo() {
        Some(true)
    } else if c.lo() > k.hi() {
        Some(false)
    } else {
        None
    }
}

fn exp_of_growing(t: Term) -> Asy {
    let one = Q::ONE;
    if t.k.signum() > 0 || t.p > one || (t.p == one && t.m.signum() > 0) {
        return Asy::Huge(1);
    }
    if t.k.is_zero() && t.p.is_zero() && t.m == one {
        return match t.c {
            Coef::Exact(v) if v.k == 0 => Asy::Term(Term {
                c: Coef::Exact(PiQ { q: one, k: 0 }),
                k: Q::ZERO,
                p: v.q,
                m: Q::ZERO,
            }),
            _ => Asy::Unknown,
        };
    }
    // x^p·(ln x)^m with 0 < p ≤ 1 (m ≤ 0 at p = 1), or p = 0 and m > 1.
    if t.p.signum() > 0 || (t.p.is_zero() && t.m > one) {
        return Asy::Mid(1, Some(t));
    }
    // e^((ln x)^m), 0 < m < 1: slower than every power.
    Asy::Unknown
}

fn asy(e: &Expr, cx: &Cx<'_>) -> Asy {
    if !e.contains_x() {
        return cx.constant(e);
    }
    match e {
        Expr::X => Asy::Term(Term {
            c: Coef::Exact(PiQ { q: Q::ONE, k: 0 }),
            k: Q::ZERO,
            p: Q::ONE,
            m: Q::ZERO,
        }),
        Expr::Neg(a) => neg(asy(a, cx)),
        Expr::Degrees(a) => asy(a, cx),
        Expr::Bin(op, a, b) => match op {
            BinOp::Add => add(asy(a, cx), asy(b, cx)),
            BinOp::Sub => add(asy(a, cx), neg(asy(b, cx))),
            BinOp::Mul => mul(asy(a, cx), asy(b, cx)),
            BinOp::Div => {
                let d = asy(b, cx);
                match d {
                    Asy::Zero | Asy::Small | Asy::Bounded(_) | Asy::Unknown => Asy::Unknown,
                    d => mul(asy(a, cx), recip(d)),
                }
            }
            BinOp::Pow => {
                if let Some((p, q)) = syntactic_rational(b, cx.lits) {
                    let r = Q::new(p as i128, q as i128).expect("q ≠ 0");
                    return pow(asy(a, cx), r);
                }
                if !b.contains_x() {
                    // functions::pow: a non-integer exponent needs a base ≥ 0.
                    let Some(r) = exact_constant(b, cx.lits).filter(|v| v.k == 0) else {
                        return Asy::Unknown;
                    };
                    let base = asy(a, cx);
                    let positive = match base {
                        Asy::Term(t) => t.c.sign() == Some(1),
                        Asy::Huge(s) | Asy::Mid(s, _) => s > 0,
                        _ => false,
                    };
                    return if r.q.is_int() || positive {
                        pow(base, r.q)
                    } else {
                        Asy::Unknown
                    };
                }
                // e^b is exp(b); a^b = e^(b·ln a) where defined (the TI
                // rule: a > 0).
                if matches!(**a, Expr::Const(crate::ast::Constant::E)) {
                    return asy(&Expr::Call(Func::Exp, vec![(**b).clone()]), cx);
                }
                let ln = Expr::Call(Func::Ln, vec![(**a).clone()]);
                let arg = Expr::bin(BinOp::Mul, (**b).clone(), ln);
                asy(&Expr::Call(Func::Exp, vec![arg]), cx)
            }
        },
        Expr::Call(f, args) => {
            let a = &args[0];
            use Func::*;
            match f {
                Exp => {
                    // An exactly affine exponent gives the term e^d·e^{sx}.
                    if let Some(s) = affine(a, cx.lits)
                        && s.k == 0
                    {
                        let at0 = a.map(&|n| matches!(n, Expr::X).then_some(Expr::Num(0.0)));
                        let d = cx.constant(&at0);
                        let c = match d {
                            Asy::Zero => Some(Coef::Exact(PiQ { q: Q::ONE, k: 0 })),
                            Asy::Term(t) => {
                                Coef::approx(elem::exp(&DecInterval::new(t.c.interval())).iv)
                            }
                            _ => None,
                        };
                        return match c {
                            Some(c) => Asy::Term(Term {
                                c,
                                k: s.q,
                                p: Q::ZERO,
                                m: Q::ZERO,
                            }),
                            None => Asy::Unknown,
                        };
                    }
                    exp_of(asy(a, cx))
                }
                Ln | Log => {
                    let r = match asy(a, cx) {
                        Asy::Term(t) if t.c.sign() == Some(1) => {
                            if t.k.signum() != 0 {
                                Asy::Term(Term {
                                    c: Coef::Exact(PiQ { q: t.k, k: 0 }),
                                    k: Q::ZERO,
                                    p: Q::ONE,
                                    m: Q::ZERO,
                                })
                            } else if t.p.signum() != 0 {
                                Asy::Term(Term {
                                    c: Coef::Exact(PiQ { q: t.p, k: 0 }),
                                    k: Q::ZERO,
                                    p: Q::ZERO,
                                    m: Q::ONE,
                                })
                            } else if t.m.signum() != 0 {
                                Asy::Unknown
                            } else {
                                let i = elem::ln(&DecInterval::new(t.c.interval())).iv;
                                if i.lo() == 0.0 && i.hi() == 0.0 {
                                    // ln(1 + o(1)) → 0, but is not 0: x·ln(1 + 1/x)
                                    // → 1, not 0.
                                    Asy::Small
                                } else {
                                    Coef::approx(i)
                                        .map_or(Asy::Unknown, |c| Asy::Term(Term::constant(c)))
                                }
                            }
                        }
                        _ => Asy::Unknown,
                    };
                    if *f == Log {
                        let ln10 = elem::ln(&DecInterval::new(Interval::point(10.0))).iv;
                        let k = Coef::approx(ln10).expect("ln 10 > 0");
                        mul(r, recip(Asy::Term(Term::constant(k))))
                    } else {
                        r
                    }
                }
                Sqrt => pow(asy(a, cx), Q::new(1, 2).expect("½")),
                Cbrt => pow(asy(a, cx), Q::new(1, 3).expect("⅓")),
                Atan | Tanh | Acot => {
                    let inner = asy(a, cx);
                    let to = |s: i32| -> Asy {
                        let v = if *f == Tanh {
                            PiQ {
                                q: Q::int(s as i128),
                                k: 0,
                            }
                        } else if *f == Acot {
                            if s > 0 {
                                return Asy::Small;
                            }
                            let q = cx.quarter();
                            PiQ {
                                q: q.q.mul(Q::int(2)).expect("2·quarter"),
                                k: q.k,
                            }
                        } else {
                            let q = cx.quarter();
                            PiQ {
                                q: q.q.mul(Q::int(s as i128)).expect("±quarter"),
                                k: q.k,
                            }
                        };
                        Asy::Term(Term::constant(Coef::Exact(v)))
                    };
                    match inner {
                        Asy::Term(t) if t.growth() > 0 => match t.c.sign() {
                            Some(s) => to(s),
                            None => Asy::Unknown,
                        },
                        Asy::Huge(s) | Asy::Mid(s, _) => to(s),
                        Asy::Zero | Asy::Small if *f != Acot => Asy::Small,
                        _ => Asy::Unknown,
                    }
                }
                Sinh | Cosh => {
                    let e1 = Expr::Call(Exp, vec![a.clone()]);
                    let e2 = Expr::Call(Exp, vec![Expr::Neg(Box::new(a.clone()))]);
                    let op = if *f == Sinh { BinOp::Sub } else { BinOp::Add };
                    let sum = Expr::bin(op, e1, e2);
                    asy(&Expr::bin(BinOp::Div, sum, Expr::Num(2.0)), cx)
                }
                Sin | Cos => match asy(a, cx) {
                    Asy::Term(t) if t.growth() > 0 => Asy::Bounded(Interval::new(-1.0, 1.0)),
                    Asy::Huge(_) | Asy::Mid(..) => Asy::Bounded(Interval::new(-1.0, 1.0)),
                    Asy::Zero | Asy::Small if *f == Sin => Asy::Small,
                    Asy::Zero | Asy::Small => {
                        Asy::Term(Term::constant(Coef::Exact(PiQ { q: Q::ONE, k: 0 })))
                    }
                    _ => Asy::Unknown,
                },
                Abs => match asy(a, cx) {
                    Asy::Term(t) => match t.c.sign() {
                        Some(-1) => neg(Asy::Term(t)),
                        Some(_) => Asy::Term(t),
                        None => Asy::Unknown,
                    },
                    Asy::Huge(_) => Asy::Huge(1),
                    Asy::Mid(_, g) => Asy::Mid(1, g),
                    other @ (Asy::Zero | Asy::Small) => other,
                    _ => Asy::Unknown,
                },
                Floor | Ceil | Round => match asy(a, cx) {
                    // ⌊A⌋ = A + O(1): the leading term of a growing A.
                    Asy::Term(t) if t.growth() > 0 => Asy::Term(t),
                    h @ (Asy::Huge(_) | Asy::Mid(..)) => h,
                    _ => Asy::Unknown,
                },
                _ => Asy::Unknown,
            }
        }
        _ => Asy::Unknown,
    }
}

/// The limit of `e` as x → ±∞ (`Unknown` when not proven).
pub fn limit(
    e: &Expr,
    dir: Dir,
    unit: TrigUnit,
    lits: &ExactLiterals,
    variables: &dyn crate::compile::VariableValues,
) -> Limit {
    if let Some(f) = rational_form(e, lits) {
        return match f.reduced.limit(dir) {
            Some(RationalLimit::Finite(q)) => Limit::Exact(PiQ { q, k: 0 }),
            Some(RationalLimit::PosInf) => Limit::PosInf,
            Some(RationalLimit::NegInf) => Limit::NegInf,
            None => Limit::Unknown,
        };
    }
    let e = match dir {
        Dir::PosInf => e.clone(),
        Dir::NegInf => e.map(&|n| matches!(n, Expr::X).then(|| Expr::Neg(Box::new(Expr::X)))),
    };
    let facts = Facts {
        unit,
        x: Interval::ENTIRE,
        vars: e
            .variables()
            .into_iter()
            .map(|n| {
                let v = variables
                    .value(&n)
                    .unwrap_or(crate::compile::DEFAULT_VARIABLE_VALUE);
                (n, v)
            })
            .collect(),
    };
    let cx = Cx { unit, lits, facts };
    let first = read_limit(asy(&e, &cx));
    if first != Limit::Unknown {
        return first;
    }
    // The limit of f(x) at ±∞ is f(x + c)'s for any c: an exponential
    // centred far out (e^(x − 1000), whose e⁻¹⁰⁰⁰ is no double) is taken
    // about its centre, where its constant part folds away exactly.
    for c in centres(&e, lits) {
        let shifted =
            e.map(&|n| matches!(n, Expr::X).then(|| Expr::bin(BinOp::Add, Expr::X, Expr::Num(c))));
        let l = read_limit(asy(&shifted, &cx));
        if l != Limit::Unknown {
            return l;
        }
    }
    Limit::Unknown
}

/// The integers c (|c| ≥ 2⁸, below 2⁵³) at which an exponential's affine
/// exponent a·x + b in `e` is 0 (c = −b/a): where to take e^(…) about.
pub fn centres(e: &Expr, lits: &ExactLiterals) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    e.visit(&mut |n| {
        let arg = match n {
            Expr::Call(Func::Exp, args) => args.first(),
            Expr::Bin(BinOp::Pow, a, b) if matches!(**a, Expr::Const(crate::ast::Constant::E)) => {
                Some(&**b)
            }
            _ => None,
        };
        let Some(arg) = arg else { return };
        let Some(s) = affine(arg, lits).filter(|s| s.k == 0 && !s.q.is_zero()) else {
            return;
        };
        let at0 = arg.map(&|m| matches!(m, Expr::X).then_some(Expr::Num(0.0)));
        let Some(d) = exact_constant(&at0, lits).filter(|d| d.k == 0) else {
            return;
        };
        let Some(c) = d.q.neg().and_then(|b| b.div(s.q)) else {
            return;
        };
        if let Some(v) = c.as_int()
            && (256..=1 << 53).contains(&v.unsigned_abs())
            && !out.contains(&(v as f64))
        {
            out.push(v as f64);
        }
    });
    out.truncate(4);
    out
}

fn read_limit(a: Asy) -> Limit {
    match a {
        Asy::Zero | Asy::Small => Limit::Exact(PiQ { q: Q::ZERO, k: 0 }),
        Asy::Huge(s) | Asy::Mid(s, _) => {
            if s > 0 {
                Limit::PosInf
            } else {
                Limit::NegInf
            }
        }
        Asy::Term(t) => match t.growth() {
            g if g < 0 => Limit::Exact(PiQ { q: Q::ZERO, k: 0 }),
            0 => match t.c {
                Coef::Exact(v) => Limit::Exact(v),
                Coef::Approx(i) => Limit::Approx(i),
            },
            _ => match t.c.sign() {
                Some(1) => Limit::PosInf,
                Some(_) => Limit::NegInf,
                None => Limit::Unknown,
            },
        },
        Asy::Bounded(_) | Asy::Unknown => Limit::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equation::Equation;

    fn lim(s: &str, dir: Dir) -> Limit {
        let text = format!("y={s}");
        let eq = Equation::parse(&text).unwrap();
        let lits = ExactLiterals::of(&text, Default::default()).unwrap();
        limit(eq.explicit().unwrap().1, dir, TrigUnit::Radians, &lits, &())
    }

    fn exact(n: i128, d: i128, k: i32) -> Limit {
        Limit::Exact(PiQ {
            q: Q::new(n, d).unwrap(),
            k,
        })
    }

    #[test]
    fn dominant_terms() {
        use Dir::*;
        assert_eq!(lim("exp(-x^2)", PosInf), exact(0, 1, 0));
        assert_eq!(lim("exp(-x^2)", NegInf), exact(0, 1, 0));
        assert_eq!(lim("atan(x)", PosInf), exact(1, 2, 1));
        assert_eq!(lim("atan(x)", NegInf), exact(-1, 2, 1));
        assert_eq!(lim("1/(1+e^x)", PosInf), exact(0, 1, 0));
        assert_eq!(lim("1/(1+e^x)", NegInf), exact(1, 1, 0));
        assert_eq!(lim("sin(x)/x", PosInf), exact(0, 1, 0));
        assert_eq!(lim("x*e^(-x)", PosInf), exact(0, 1, 0));
        assert_eq!(lim("x*e^(-x)", NegInf), Limit::NegInf);
        assert_eq!(lim("ln(x)/x", PosInf), exact(0, 1, 0));
        assert_eq!(lim("x^(1/3)", NegInf), Limit::NegInf);
        assert_eq!(lim("e^x/x^100", PosInf), Limit::PosInf);
        assert_eq!(lim("(2x+1)/(x-3)", PosInf), exact(2, 1, 0));
        assert_eq!(lim("tanh(x)", NegInf), exact(-1, 1, 0));
        assert_eq!(lim("sin(x)", PosInf), Limit::Unknown);
        assert_eq!(lim("x-x+sin(x)", PosInf), Limit::Unknown);
        assert_eq!(lim("sqrt(x^2+x)-x", PosInf), Limit::Unknown, "cancels");
        assert_eq!(lim("e^(x^2)-x", PosInf), Limit::PosInf);
        assert_eq!(lim("1-1/x", PosInf), exact(1, 1, 0));
        assert_eq!(lim("acot(x)", NegInf), exact(1, 1, 1));
        assert_eq!(lim("acot(x)", PosInf), exact(0, 1, 0));
        // 1^∞: the base's leading term 1 says nothing of the power.
        assert_eq!(lim("ln(1+1/x)", PosInf), exact(0, 1, 0));
        assert_eq!(lim("x*ln(1+1/x)", PosInf), Limit::Unknown);
        assert_eq!(lim("(1+1/x)^x", PosInf), Limit::Unknown);
        assert_eq!(lim("(1+1/x)^x", NegInf), Limit::Unknown);
        assert_eq!(lim("(1+1/x^2)^x", PosInf), Limit::Unknown);
        // Centred far out: taken about the centre.
        assert_eq!(lim("e^(x-1000)", NegInf), exact(0, 1, 0));
        assert_eq!(lim("e^(x-1000)/x", PosInf), Limit::PosInf);
        assert_eq!(lim("(x-1000000)*e^(-(x-1000000))", PosInf), exact(0, 1, 0));
    }

    /// e^g for a g that grows no faster than x outgrows every power but
    /// not every exponential (review 12, R12-L-03: exp(√x) − exp(x) was
    /// +∞).
    #[test]
    fn slow_exponentials_are_ordered() {
        use Dir::*;
        // Decided, and right.
        for (f, want) in [
            ("exp(sqrt(x))-exp(x)", Limit::NegInf),
            ("exp(x)-exp(sqrt(x))", Limit::PosInf),
            ("exp(sqrt(x))-x^100", Limit::PosInf),
            ("x^100-exp(sqrt(x))", Limit::NegInf),
            ("exp(sqrt(x))/x^1000", Limit::PosInf),
            ("exp(x^2)-exp(sqrt(x))", Limit::PosInf),
            ("exp(sqrt(x))-exp(x^2)", Limit::NegInf),
            ("exp(sqrt(x))-exp(x^(1/3))", Limit::PosInf),
            ("exp(-sqrt(x))", exact(0, 1, 0)),
            ("atan(exp(sqrt(x)))", exact(1, 2, 1)),
            // e^(ln x) is x, a power: it loses to x².
            ("exp(ln(x))-x^2", Limit::NegInf),
            ("exp(2*ln(x))-x^3", Limit::NegInf),
            // e^(x + sin x) ≤ e^(x + 1), below e^(2x).
            ("exp(x+sin(x))-exp(2x)", Limit::NegInf),
            ("2^x-3^x", Limit::NegInf),
            ("e^(x^2)-x", Limit::PosInf),
        ] {
            assert_eq!(lim(f, PosInf), want, "{f}");
        }
        // Undecided is allowed; wrong never.
        for (f, truth) in [
            ("exp(sqrt(x))*exp(-x)", exact(0, 1, 0)),
            ("exp(exp(sqrt(x)))-exp(x^3)", Limit::PosInf),
            ("exp(x+sin(x))-exp(x)", Limit::Unknown),
        ] {
            let got = lim(f, PosInf);
            assert!(got == truth || got == Limit::Unknown, "{f}: {got:?}");
        }
    }
}
