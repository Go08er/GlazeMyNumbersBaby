//! Limits from the tree's structure, the replay's own reading: how f (or
//! f/x) ends as x goes out along a tail or to a point from one side.
//!
//! Let u → +∞ be the distance out: x = u on the right tail, x = −u on the
//! left, x = p ± 1/u beside p. Every node of the formula, read from the
//! leaves up, is put in one of the kinds of [`Way`] by textbook facts:
//!
//! * limits of sums, products and quotients of convergent factors, and of
//!   continuous functions of a convergent argument (from the side it is
//!   approached, where the function is continuous only on that side);
//! * dominant terms: if f ~ a·u^q·(ln u)^r and g ~ b·u^s·(ln u)^t, then
//!   f·g ~ ab·u^(q+s)·(ln u)^(r+t), and f + g is the larger, or (a + b)·…
//!   for equal (q, r) with a + b ≠ 0; ln f ~ q·ln u for q ≠ 0;
//!   (f)^k ~ a^k·u^(kq)·(ln u)^(kr);
//! * e^g for g ~ c·u^q·(ln u)^r with q > 0 (or q = 0, r > 1) beats every
//!   power of u (c > 0) or is beaten by every inverse power (c < 0); a
//!   quantity beyond every power times any term of known sign stays
//!   beyond every power; one below every inverse power times any bounded
//!   factor stays below;
//! * the elementary functions at the ends of their domains (atan → ±π/2,
//!   tanh → ±1, ln → −∞ at 0⁺, …), and sin, cos of an unbounded argument
//!   stay in [−1, 1].
//!
//! An exact a·x + b (typed decimals) is one term, so x − p beside p is
//! exactly ±1/u. Nothing else is assumed: ∞ − ∞ at one rate, 0·∞ with a
//! rate unknown, and oscillation under a growing factor stay undecided.
//! Written for the replay from those facts; it shares no code with the
//! certifier.

use super::eval::{contains_x, written_rational};
use super::iv::{self, Iv};
use super::{Fx, series};
use graphing::ast::{BinOp, Expr, Func};
use rug::Rational;
use rug::float::Round;
use std::cmp::Ordering;

/// Where x goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum At {
    PosInf,
    NegInf,
    /// x → p⁺.
    Right(f64),
    /// x → p⁻.
    Left(f64),
}

/// Where the limit goes.
#[derive(Clone, Debug)]
pub enum Goes {
    PosInf,
    NegInf,
    /// A finite value, enclosed.
    To(Iv),
}

/// How a node behaves as u → +∞.
#[derive(Clone, Debug)]
enum Way {
    /// ≡ 0.
    Nil,
    /// a·u^q·(ln u)^r·(1 + o(1)), a ≠ 0 (its enclosure excludes 0),
    /// (q, r) ≠ (0, 0).
    Term {
        a: Iv,
        q: Rational,
        r: Rational,
    },
    /// → a value in `v`; strictly above it eventually (`Some(true)`),
    /// strictly below, or unknown.
    Tends {
        v: Iv,
        side: Option<bool>,
    },
    /// → 0 below every inverse power of u; sign eventually, if known.
    Vanishes(Option<bool>),
    /// → +∞ (`true`) or −∞ beyond every power of u.
    Explodes(bool),
    /// → ±∞, rate unknown.
    Diverges(bool),
    /// No limit known; lim sup and lim inf within the enclosure.
    Within(Iv),
    Lost,
}

use Way::*;

fn rq(n: i64, d: i64) -> Rational {
    Rational::from((n, d))
}

fn zero_iv() -> Iv {
    Iv::of(0.0)
}

fn one_iv() -> Iv {
    Iv::of(1.0)
}

/// An enclosure of a rational.
fn rat(q: &Rational) -> Iv {
    let p = iv::prec();
    let lo = rug::Float::with_val_round(p, q, Round::Down).0;
    let hi = rug::Float::with_val_round(p, q, Round::Up).0;
    Iv::new(lo, hi)
}

fn is_nil(v: &Iv) -> bool {
    !v.empty && v.lo == 0 && v.hi == 0
}

fn order(q: &Rational, r: &Rational) -> Ordering {
    q.cmp0().then(r.cmp0())
}

fn bounded(v: &Iv) -> bool {
    !v.empty && v.bounded()
}

/// a·u^q·(ln u)^r, with what an enclosure a holding 0 still says.
fn term(a: Iv, q: Rational, r: Rational) -> Way {
    if !bounded(&a) {
        return Lost;
    }
    match order(&q, &r) {
        Ordering::Equal => Tends { v: a, side: None },
        Ordering::Less if a.has0() => Tends {
            v: zero_iv(),
            side: None,
        },
        Ordering::Greater if a.has0() => Lost,
        _ => Term { a, q, r },
    }
}

/// As a term: a `Term`, or a limit away from 0 (as u⁰).
fn as_term(w: &Way) -> Option<(Iv, Rational, Rational)> {
    match w {
        Term { a, q, r } => Some((a.clone(), q.clone(), r.clone())),
        Tends { v, .. } if bounded(v) && v.ne0() => Some((v.clone(), rq(0, 1), rq(0, 1))),
        _ => None,
    }
}

/// A finite limit and its side.
fn finite(w: &Way) -> Option<(Iv, Option<bool>)> {
    match w {
        Nil => Some((zero_iv(), None)),
        Term { a, q, r } if order(q, r) == Ordering::Less => Some((zero_iv(), Some(a.gt(0.0)))),
        Tends { v, side } => Some((v.clone(), *side)),
        Vanishes(s) => Some((zero_iv(), *s)),
        _ => None,
    }
}

/// An infinite limit.
fn infinite(w: &Way) -> Option<bool> {
    match w {
        Term { a, q, r } if order(q, r) == Ordering::Greater => Some(a.gt(0.0)),
        Explodes(s) | Diverges(s) => Some(*s),
        _ => None,
    }
}

fn negate(w: Way) -> Way {
    match w {
        Term { a, q, r } => Term {
            a: iv::neg(&a),
            q,
            r,
        },
        Tends { v, side } => Tends {
            v: iv::neg(&v),
            side: side.map(|s| !s),
        },
        Vanishes(s) => Vanishes(s.map(|s| !s)),
        Explodes(s) => Explodes(!s),
        Diverges(s) => Diverges(!s),
        Within(v) => Within(iv::neg(&v)),
        Nil | Lost => w,
    }
}

fn plus(x: Way, y: Way) -> Way {
    match (x, y) {
        (Lost, _) | (_, Lost) => Lost,
        (Nil, w) | (w, Nil) => w,
        (Explodes(s), Explodes(t)) | (Explodes(s), Diverges(t)) | (Diverges(t), Explodes(s)) => {
            if s == t { Explodes(s) } else { Lost }
        }
        (Explodes(s), _) | (_, Explodes(s)) => Explodes(s),
        (Diverges(s), Diverges(t)) => {
            if s == t {
                Diverges(s)
            } else {
                Lost
            }
        }
        (Diverges(s), Term { a, q, r }) | (Term { a, q, r }, Diverges(s)) => {
            if order(&q, &r) == Ordering::Greater && a.gt(0.0) != s {
                Lost
            } else {
                Diverges(s)
            }
        }
        (Diverges(s), _) | (_, Diverges(s)) => Diverges(s),
        (
            Term {
                a: a1,
                q: q1,
                r: r1,
            },
            Term {
                a: a2,
                q: q2,
                r: r2,
            },
        ) => match q1.cmp(&q2).then(r1.cmp(&r2)) {
            Ordering::Greater => Term {
                a: a1,
                q: q1,
                r: r1,
            },
            Ordering::Less => Term {
                a: a2,
                q: q2,
                r: r2,
            },
            Ordering::Equal => {
                let a = iv::add(&a1, &a2);
                if a.ne0() {
                    Term { a, q: q1, r: r1 }
                } else if order(&q1, &r1) == Ordering::Less {
                    Tends {
                        v: zero_iv(),
                        side: None,
                    }
                } else {
                    Lost
                }
            }
        },
        (Term { a, q, r }, w) | (w, Term { a, q, r }) => {
            if order(&q, &r) == Ordering::Greater {
                return Term { a, q, r };
            }
            // A term going to 0, and w: a limit, a fast 0, or bounded.
            let s = a.gt(0.0);
            match w {
                Tends { v, side } => Tends {
                    v,
                    side: if side == Some(s) { side } else { None },
                },
                Vanishes(_) => Term { a, q, r },
                Within(v) => Within(v),
                _ => Lost,
            }
        }
        (Tends { v: v1, side: s1 }, Tends { v: v2, side: s2 }) => Tends {
            v: iv::add(&v1, &v2),
            side: if s1 == s2 { s1 } else { None },
        },
        (Tends { v, side }, Vanishes(t)) | (Vanishes(t), Tends { v, side }) => Tends {
            v,
            side: if side == t { side } else { None },
        },
        (Tends { v, .. }, Within(b)) | (Within(b), Tends { v, .. }) => Within(iv::add(&v, &b)),
        (Vanishes(s), Vanishes(t)) => Vanishes(if s == t { s } else { None }),
        (Vanishes(_), Within(b)) | (Within(b), Vanishes(_)) => Within(b),
        (Within(a), Within(b)) => Within(iv::add(&a, &b)),
    }
}

fn sign_of(s: Option<bool>, t: Option<bool>) -> Option<bool> {
    Some(s? == t?)
}

fn times(x: Way, y: Way) -> Way {
    if matches!(x, Lost) || matches!(y, Lost) {
        return Lost;
    }
    if matches!(x, Nil) || matches!(y, Nil) {
        return Nil;
    }
    if let (Some((a, q, r)), Some((b, s, t))) = (as_term(&x), as_term(&y)) {
        return term(iv::mul(&a, &b), q + s, r + t);
    }
    // One way round: `None` to try the other.
    let one = |x: &Way, y: &Way| -> Option<Way> {
        Some(match (x, y) {
            (Explodes(s), Explodes(t)) | (Explodes(s), Diverges(t)) => Explodes(s == t),
            (Explodes(s), w) if as_term(w).is_some() => Explodes(*s == as_term(w)?.0.gt(0.0)),
            (Explodes(s), Within(b)) if b.ne0() => Explodes(*s == b.gt(0.0)),
            (Explodes(_), _) => Lost,
            (Diverges(_), Explodes(_)) => return None,
            (Diverges(s), Diverges(t)) => Diverges(s == t),
            (Diverges(s), w) if as_term(w).is_some() => {
                let (a, q, r) = as_term(w)?;
                if order(&q, &r) == Ordering::Less {
                    Lost
                } else {
                    Diverges(*s == a.gt(0.0))
                }
            }
            (Diverges(s), Within(b)) if b.ne0() => Diverges(*s == b.gt(0.0)),
            (Diverges(_), _) => Lost,
            (Vanishes(s), Vanishes(t)) => Vanishes(sign_of(*s, *t)),
            (Vanishes(s), w) if as_term(w).is_some() => {
                Vanishes(sign_of(*s, Some(as_term(w)?.0.gt(0.0))))
            }
            (Vanishes(_), Tends { .. } | Within(_)) => Vanishes(None),
            (Vanishes(_), _) => Lost,
            (Term { a, q, r }, Tends { .. } | Within(_)) => {
                if order(q, r) == Ordering::Less {
                    Tends {
                        v: zero_iv(),
                        side: None,
                    }
                } else if let Within(b) = y
                    && b.ne0()
                {
                    Diverges(a.gt(0.0) == b.gt(0.0))
                } else {
                    Lost
                }
            }
            (Tends { v: v1, side: s1 }, Tends { v: v2, side: s2 }) => Tends {
                v: iv::mul(v1, v2),
                side: if is_nil(v1) && is_nil(v2) {
                    sign_of(*s1, *s2)
                } else {
                    None
                },
            },
            (Tends { v, .. }, Within(b)) => {
                if is_nil(v) {
                    Tends {
                        v: zero_iv(),
                        side: None,
                    }
                } else {
                    Within(iv::mul(v, b))
                }
            }
            (Within(a), Within(b)) => Within(iv::mul(a, b)),
            _ => return None,
        })
    };
    one(&x, &y).or_else(|| one(&y, &x)).unwrap_or(Lost)
}

fn inverse(w: Way) -> Way {
    match w {
        Term { a, q, r } => term(iv::recip(&a), -q, -r),
        Tends { v, side } if bounded(&v) && v.ne0() => Tends {
            v: iv::recip(&v),
            side: side.map(|s| !s),
        },
        Tends { v, side: Some(s) } if is_nil(&v) => Diverges(s),
        Explodes(s) => Vanishes(Some(s)),
        Diverges(s) => Tends {
            v: zero_iv(),
            side: Some(s),
        },
        Vanishes(Some(s)) => Explodes(s),
        Within(b) if b.ne0() => Within(iv::recip(&b)),
        _ => Lost,
    }
}

/// a^k for a of one sign, k exact; a negative a only when `odd` (k whole,
/// or written p/q with q odd).
fn power_of(a: &Iv, k: &Rational, odd: bool) -> Option<Iv> {
    if a.gt(0.0) {
        let v = iv::pow_const(a, &rat(k));
        return bounded(&v).then_some(v);
    }
    if a.lt(0.0) && odd {
        let m = power_of(&iv::neg(a), k, odd)?;
        let even = k.numer().is_even();
        return Some(if even { m } else { iv::neg(&m) });
    }
    None
}

fn power(w: Way, k: &Rational, odd: bool) -> Way {
    if k.cmp0() == Ordering::Equal {
        return Tends {
            v: one_iv(),
            side: None,
        };
    }
    let up = k.cmp0() == Ordering::Greater;
    let sign = |s: bool| -> Option<bool> {
        if s {
            Some(true)
        } else if odd {
            Some(k.numer().is_even())
        } else {
            None
        }
    };
    match w {
        Nil if up => Nil,
        Term { a, q, r } => match power_of(&a, k, odd) {
            Some(a) => term(a, q * k.clone(), r * k.clone()),
            None => Lost,
        },
        Tends { v, .. } if bounded(&v) && v.ne0() => match power_of(&v, k, odd) {
            Some(v) => Tends { v, side: None },
            None => Lost,
        },
        Tends { v, side: Some(s) } if is_nil(&v) => match sign(s) {
            Some(t) if up => Tends {
                v: zero_iv(),
                side: Some(t),
            },
            Some(t) => Diverges(t),
            None => Lost,
        },
        Vanishes(Some(s)) => match sign(s) {
            Some(t) if up => Vanishes(Some(t)),
            Some(t) => Explodes(t),
            None => Lost,
        },
        Explodes(s) => match sign(s) {
            Some(t) if up => Explodes(t),
            Some(t) => Vanishes(Some(t)),
            None => Lost,
        },
        Diverges(s) => match sign(s) {
            Some(t) if up => Diverges(t),
            Some(t) => Tends {
                v: zero_iv(),
                side: Some(t),
            },
            None => Lost,
        },
        Within(b) if b.gt(0.0) => match power_of(&b, k, odd) {
            Some(v) => Within(v),
            None => Lost,
        },
        _ => Lost,
    }
}

/// a^k for a constant k known only as an enclosure: positive a only.
fn power_iv(w: Way, k: &Iv) -> Way {
    if !bounded(k) || k.has0() {
        return Lost;
    }
    let up = k.gt(0.0);
    let pw = |v: &Iv| {
        let p = iv::pow_const(v, k);
        bounded(&p).then_some(p)
    };
    let zero_from_above = || Tends {
        v: zero_iv(),
        side: Some(true),
    };
    match w {
        Term { a, q, r } if a.gt(0.0) => {
            if (order(&q, &r) == Ordering::Greater) == up {
                Diverges(true)
            } else {
                zero_from_above()
            }
        }
        Tends { v, .. } if v.gt(0.0) => pw(&v).map_or(Lost, |v| Tends { v, side: None }),
        Tends {
            v,
            side: Some(true),
        } if is_nil(&v) => {
            if up {
                zero_from_above()
            } else {
                Diverges(true)
            }
        }
        Vanishes(Some(true)) => {
            if up {
                Vanishes(Some(true))
            } else {
                Explodes(true)
            }
        }
        Explodes(true) => {
            if up {
                Explodes(true)
            } else {
                Vanishes(Some(true))
            }
        }
        Diverges(true) => {
            if up {
                Diverges(true)
            } else {
                zero_from_above()
            }
        }
        Within(b) if b.gt(0.0) => pw(&b).map_or(Lost, Within),
        _ => Lost,
    }
}

fn exp_of(w: Way) -> Way {
    let ev = |v: &Iv| {
        let e = iv::exp(v);
        bounded(&e).then_some(e)
    };
    match w {
        Nil => Tends {
            v: one_iv(),
            side: None,
        },
        Term { a, q, r } => {
            if order(&q, &r) == Ordering::Greater {
                let fast = q.cmp0() == Ordering::Greater || r > 1;
                match (a.gt(0.0), fast) {
                    (true, true) => Explodes(true),
                    (true, false) => Diverges(true),
                    (false, true) => Vanishes(Some(true)),
                    (false, false) => Tends {
                        v: zero_iv(),
                        side: Some(true),
                    },
                }
            } else {
                Tends {
                    v: one_iv(),
                    side: Some(a.gt(0.0)),
                }
            }
        }
        Tends { v, side } => ev(&v).map_or(Lost, |v| Tends { v, side }),
        Vanishes(s) => Tends {
            v: one_iv(),
            side: s,
        },
        Explodes(true) => Explodes(true),
        Explodes(false) => Vanishes(Some(true)),
        Diverges(true) => Diverges(true),
        Diverges(false) => Tends {
            v: zero_iv(),
            side: Some(true),
        },
        Within(b) => ev(&b).map_or(Lost, Within),
        Lost => Lost,
    }
}

fn ln_of(w: Way) -> Way {
    match w {
        // ln(a·u^q·(ln u)^r) = q·ln u + r·ln ln u + ln a + o(1).
        Term { a, q, r } if a.gt(0.0) => {
            if q.cmp0() != Ordering::Equal {
                term(rat(&q), rq(0, 1), rq(1, 1))
            } else {
                Diverges(r.cmp0() == Ordering::Greater)
            }
        }
        Tends { v, side } if v.gt(0.0) => {
            let l = iv::ln(&v);
            if bounded(&l) {
                Tends { v: l, side }
            } else {
                Lost
            }
        }
        Tends {
            v,
            side: Some(true),
        } if is_nil(&v) => Diverges(false),
        Vanishes(Some(true)) => Diverges(false),
        Explodes(true) | Diverges(true) => Diverges(true),
        Within(b) if b.gt(0.0) => {
            let l = iv::ln(&b);
            if bounded(&l) { Within(l) } else { Lost }
        }
        _ => Lost,
    }
}

/// One float either way of an enclosure (MPFR's spacing at the working
/// precision).
fn widen(v: &Iv, side: Option<bool>) -> Iv {
    let mut lo = v.lo.clone();
    let mut hi = v.hi.clone();
    if side != Some(true) {
        lo.next_down();
    }
    if side != Some(false) {
        hi.next_up();
    }
    Iv::new(lo, hi)
}

pub struct Reader<'a> {
    fx: &'a Fx,
    at: At,
}

impl Reader<'_> {
    /// φ = func applied to its argument, over a box of the argument.
    fn apply(&self, func: Func, x: &Iv) -> Iv {
        let tree = Expr::Call(func, vec![Expr::X]);
        self.fx.series_iv(&tree, x.clone(), 0)[0].clone()
    }

    /// φ continuous at the argument's limit (from its side, if known).
    fn continuous(&self, func: Func, w: &Way) -> Way {
        let Some((v, side)) = finite(w) else {
            return Lost;
        };
        let out = self.apply(func, &widen(&v, side));
        if out.empty || !out.cont || !bounded(&out) {
            return Lost;
        }
        Tends { v: out, side: None }
    }

    /// φ's limit at +∞ (`up`) or −∞, for φ monotone with a finite limit
    /// there.
    fn at_infinity(&self, func: Func, up: bool) -> Way {
        let x = if up {
            Iv::new(iv::fl(f64::MAX), iv::inf(true))
        } else {
            Iv::new(iv::inf(false), iv::fl(-f64::MAX))
        };
        let out = self.apply(func, &x);
        if out.empty || !bounded(&out) {
            return Lost;
        }
        Tends { v: out, side: None }
    }

    fn constant(&self, e: &Expr) -> Way {
        if let Some((a, b)) = self.linear(e)
            && a.cmp0() == Ordering::Equal
            && b.cmp0() == Ordering::Equal
        {
            return Nil;
        }
        let v = self.fx.series_iv(e, Iv::of(0.0), 0)[0].clone();
        if v.empty || !v.def || !bounded(&v) {
            return Lost;
        }
        Tends { v, side: None }
    }

    /// `e` as a·x + b with a, b exact (typed decimals).
    fn linear(&self, e: &Expr) -> Option<(Rational, Rational)> {
        Some(match e {
            Expr::X => (rq(1, 1), rq(0, 1)),
            Expr::Num(v, lit) => (rq(0, 1), self.fx.lits.exact(*v, lit)?),
            Expr::Neg(a) => {
                let (a, b) = self.linear(a)?;
                (-a, -b)
            }
            Expr::Bin(BinOp::Add, l, r) => {
                let ((a, b), (c, d)) = (self.linear(l)?, self.linear(r)?);
                (a + c, b + d)
            }
            Expr::Bin(BinOp::Sub, l, r) => {
                let ((a, b), (c, d)) = (self.linear(l)?, self.linear(r)?);
                (a - c, b - d)
            }
            Expr::Bin(BinOp::Mul, l, r) => {
                let ((a, b), (c, d)) = (self.linear(l)?, self.linear(r)?);
                if a.cmp0() == Ordering::Equal {
                    (c * b.clone(), d * b)
                } else if c.cmp0() == Ordering::Equal {
                    (a * d.clone(), b * d)
                } else {
                    return None;
                }
            }
            Expr::Bin(BinOp::Div, l, r) => {
                let ((a, b), (c, d)) = (self.linear(l)?, self.linear(r)?);
                if c.cmp0() != Ordering::Equal || d.cmp0() == Ordering::Equal {
                    return None;
                }
                (a / d.clone(), b / d)
            }
            _ => return None,
        })
    }

    /// a·x + b as x → the point.
    fn line(&self, a: Rational, b: Rational) -> Way {
        let nil_or = |v: Rational| {
            if v.cmp0() == Ordering::Equal {
                Nil
            } else {
                Tends {
                    v: rat(&v),
                    side: None,
                }
            }
        };
        match self.at {
            At::PosInf | At::NegInf => {
                if a.cmp0() == Ordering::Equal {
                    return nil_or(b);
                }
                let a = if self.at == At::PosInf { a } else { -a };
                term(rat(&a), rq(1, 1), rq(0, 1))
            }
            At::Right(p) | At::Left(p) => {
                let Some(pq) = Rational::from_f64(p) else {
                    return Lost;
                };
                let v = a.clone() * pq + b;
                if a.cmp0() == Ordering::Equal {
                    return nil_or(v);
                }
                // a·x + b = v + a·(x − p) = v ± a/u.
                let s = if matches!(self.at, At::Right(_)) {
                    a
                } else {
                    -a
                };
                if v.cmp0() == Ordering::Equal {
                    term(rat(&s), rq(-1, 1), rq(0, 1))
                } else {
                    Tends {
                        v: rat(&v),
                        side: Some(s.cmp0() == Ordering::Greater),
                    }
                }
            }
        }
    }

    fn way(&self, e: &Expr) -> Way {
        if !contains_x(e) {
            return self.constant(e);
        }
        if let Some((a, b)) = self.linear(e) {
            return self.line(a, b);
        }
        match e {
            Expr::Neg(a) => negate(self.way(a)),
            Expr::Bin(op, a, b) => match op {
                BinOp::Add => plus(self.way(a), self.way(b)),
                BinOp::Sub => plus(self.way(a), negate(self.way(b))),
                BinOp::Mul => times(self.way(a), self.way(b)),
                BinOp::Div => times(self.way(a), inverse(self.way(b))),
                BinOp::Pow => self.pow(a, b),
            },
            Expr::Call(func, args) => self.call(*func, args),
            // (x itself is a line; y and the rest are not f's.)
            _ => Lost,
        }
    }

    fn pow(&self, base: &Expr, k: &Expr) -> Way {
        if contains_x(k) {
            // a^b = e^(b·ln a), a > 0.
            return exp_of(times(self.way(k), ln_of(self.way(base))));
        }
        let w = self.way(base);
        if let Some((p, q)) = written_rational(k, &self.fx.lits) {
            let odd = q.is_odd();
            return power(w, &Rational::from((p, q)), odd);
        }
        // A typed decimal exponent: exact, a positive base only.
        if let Expr::Num(v, lit) = k
            && let Some(q) = self.fx.lits.exact(*v, lit)
        {
            let whole = q.is_integer();
            return power(w, &q, whole);
        }
        if let Expr::Neg(inner) = k
            && let Expr::Num(v, lit) = inner.as_ref()
            && let Some(q) = self.fx.lits.exact(*v, lit)
        {
            let whole = q.is_integer();
            return power(w, &(-q), whole);
        }
        let kv = self.fx.series_iv(k, Iv::of(0.0), 0)[0].clone();
        if kv.empty || !kv.def {
            return Lost;
        }
        power_iv(w, &kv)
    }

    fn call(&self, func: Func, args: &[Expr]) -> Way {
        use Func::*;
        if args.len() == 2 {
            return match func {
                Root if !contains_x(&args[1]) => match written_rational(&args[1], &self.fx.lits) {
                    Some((n, q)) if q == 1 && n != 0 => {
                        let odd = n.is_odd();
                        power(
                            self.way(&args[0]),
                            &Rational::from((rug::Integer::from(1), n)),
                            odd,
                        )
                    }
                    _ => Lost,
                },
                LogBase if !contains_x(&args[0]) => {
                    let b = self.fx.series_iv(&args[0], Iv::of(0.0), 0)[0].clone();
                    if b.empty || !b.def || !b.gt(0.0) {
                        return Lost;
                    }
                    let lb = iv::ln(&b);
                    if !bounded(&lb) || lb.has0() {
                        return Lost;
                    }
                    times(
                        ln_of(self.way(&args[1])),
                        Tends {
                            v: iv::recip(&lb),
                            side: None,
                        },
                    )
                }
                Min | Max => {
                    let (a, b) = (self.way(&args[0]), self.way(&args[1]));
                    let max = func == Max;
                    match (finite(&a), finite(&b), infinite(&a), infinite(&b)) {
                        (Some((u, _)), Some((v, _)), ..) => Tends {
                            v: if max {
                                iv::max2(&u, &v)
                            } else {
                                iv::min2(&u, &v)
                            },
                            side: None,
                        },
                        (Some(_), None, _, Some(s)) => {
                            if s == max {
                                b
                            } else {
                                a
                            }
                        }
                        (None, Some(_), Some(s), _) => {
                            if s == max {
                                a
                            } else {
                                b
                            }
                        }
                        _ => Lost,
                    }
                }
                _ => Lost,
            };
        }
        if args.len() != 1 {
            return Lost;
        }
        let w = self.way(&args[0]);
        match func {
            Sqrt => power(w, &rq(1, 2), false),
            Cbrt => power(w, &rq(1, 3), true),
            Exp => exp_of(w),
            Ln => ln_of(w),
            Log => times(
                ln_of(w),
                Tends {
                    v: iv::recip(&iv::ln(&Iv::of(10.0))),
                    side: None,
                },
            ),
            Abs => match w {
                Term { a, q, r } => Term {
                    a: iv::abs(&a),
                    q,
                    r,
                },
                Explodes(_) => Explodes(true),
                Diverges(_) => Diverges(true),
                Vanishes(s) => Vanishes(s.map(|_| true)),
                Tends { v, side } if v.ne0() => {
                    let pos = v.gt(0.0);
                    Tends {
                        v: iv::abs(&v),
                        side: side.map(|s| s == pos),
                    }
                }
                Tends { v, side } if is_nil(&v) => Tends {
                    v,
                    side: side.map(|_| true),
                },
                Tends { v, .. } => Tends {
                    v: iv::abs(&v),
                    side: None,
                },
                Within(b) => Within(iv::abs(&b)),
                Nil | Lost => w,
            },
            Floor | Ceil | Round => {
                if infinite(&w).is_some() {
                    // f moved by less than 1: as f.
                    return plus(w, Within(Iv::of2(-1.0, 1.0)));
                }
                if let Within(b) = &w {
                    let v = match func {
                        Floor => iv::floor(b),
                        Ceil => iv::ceil(b),
                        _ => iv::round(b),
                    };
                    return if v.empty { Lost } else { Within(v) };
                }
                self.continuous(func, &w)
            }
            Sign => match infinite(&w) {
                Some(s) => Tends {
                    v: Iv::of(if s { 1.0 } else { -1.0 }),
                    side: None,
                },
                None => self.continuous(func, &w),
            },
            Sin | Cos => {
                if infinite(&w).is_some() || matches!(w, Within(_)) {
                    Within(Iv::of2(-1.0, 1.0))
                } else {
                    self.continuous(func, &w)
                }
            }
            Sinh | Cosh => {
                let half = Tends {
                    v: Iv::of(0.5),
                    side: None,
                };
                let e1 = exp_of(w.clone());
                let e2 = exp_of(negate(w));
                let s = if func == Sinh { negate(e2) } else { e2 };
                times(plus(e1, s), half)
            }
            Asinh => match infinite(&w) {
                Some(s) => Diverges(s),
                None => self.continuous(func, &w),
            },
            Acosh => match infinite(&w) {
                Some(true) => Diverges(true),
                Some(false) => Lost,
                None => self.continuous(func, &w),
            },
            // Monotone far out with a finite limit there.
            Atan | Acot | Asec | Acsc | Tanh | Coth | Sech | Csch | Acoth | Acsch => {
                match infinite(&w) {
                    Some(s) => self.at_infinity(func, s),
                    None => self.continuous(func, &w),
                }
            }
            Tan | Sec | Csc | Cot | Asin | Acos | Atanh | Asech => self.continuous(func, &w),
            _ => Lost,
        }
    }
}

/// f's limit (f/x's, `over_x`) as x → `at`, read from the tree; `None`
/// when the facts don't decide it.
pub fn limit(fx: &Fx, at: At, over_x: bool) -> Option<Goes> {
    let e = if over_x {
        Expr::Bin(BinOp::Div, Box::new(fx.f.clone()), Box::new(Expr::X))
    } else {
        fx.f.clone()
    };
    let w = Reader { fx, at }.way(&e);
    if let Some(s) = infinite(&w) {
        return Some(if s { Goes::PosInf } else { Goes::NegInf });
    }
    let (v, _) = finite(&w)?;
    bounded(&v).then_some(Goes::To(v))
}

#[allow(dead_code)]
fn _uses(_: series::S) {}
