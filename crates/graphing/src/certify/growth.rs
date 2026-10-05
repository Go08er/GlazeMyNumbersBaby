//! Limits by f's structure: how f (or f/x) ends as x goes out along a tail
//! or to a point from one side, read bottom-up from the formula's tree by
//! the rules of limits and dominant terms. A proof, not an estimate: each
//! rule is a theorem about the kinds of behaviour it names, and anything
//! outside them is left undecided (∞ − ∞ at one rate, 0·∞ at unknown
//! rates, oscillation under a growing factor). What slow limits need —
//! 1/ln x → 0, x^−0.0001 → 0, atan(ln|x|) → π/2 — where f's enclosures far
//! out settle too slowly to show them. The replay reads the same rules its
//! own way (`tests/replay/growth.rs`): a [`Claim::Limit`] it re-derives.
//!
//! With u → +∞ the distance out (x = u on the right tail, −u on the left,
//! p ± 1/u beside p), each node is one of [`Asy`]'s kinds. An exact
//! a·x + b is read as one term, so x − p beside p is ±1/u exactly (no
//! cancellation of p against itself).
//!
//! [`Claim::Limit`]: super::cert::Claim::Limit

use std::cmp::Ordering;

use super::cert::{R, To, Toward};
use super::fun::{Fun, affine};
use crate::ast::{BinOp, Expr, Func};
use crate::functions::TrigUnit;
use crate::interval::{Ctx, Dec, DecInterval, Interval, elem, taylor};
use crate::simplify::q::Q;

/// How a node behaves as u → +∞.
#[derive(Clone, Copy, Debug)]
enum Asy {
    /// Identically 0.
    Zero,
    /// c·u^q·(ln u)^r·(1 + o(1)) for a c* in `c` (which excludes 0), with
    /// (q, r) ≠ (0, 0).
    Dom {
        c: Interval,
        q: Q,
        r: Q,
    },
    /// Tends to a value in `l`: strictly above it from some u on
    /// (`Some(true)`), strictly below, or not known.
    Fin {
        l: Interval,
        from: Option<bool>,
    },
    /// Tends to 0 faster than every power of u (|f| ≤ u⁻ᴺ from some u on,
    /// for every N): positive from some u on (`Some(true)`), negative, or
    /// not known.
    Tiny(Option<bool>),
    /// Tends to +∞ (`true`) or −∞ faster than every power of u.
    Huge(bool),
    /// Tends to +∞ or −∞, at no rate known.
    Big(bool),
    /// No limit known, but lim sup ≤ hi and lim inf ≥ lo.
    Bounded(Interval),
    Unknown,
}

use Asy::*;

/// (q, r) against (0, 0): growing, settling or vanishing, as u^q·(ln u)^r.
fn class(q: Q, r: Q) -> Ordering {
    q.cmp(&Q::ZERO).then(r.cmp(&Q::ZERO))
}

fn zero() -> Interval {
    Interval::point(0.0)
}

/// c·u^q·(ln u)^r with c an enclosure: a dominant term when c excludes 0,
/// a limit for (q, r) = (0, 0); for c holding 0 only what is sure.
fn dom(c: Interval, q: Q, r: Q) -> Asy {
    if c.is_empty() || !c.is_bounded() {
        return Unknown;
    }
    match class(q, r) {
        Ordering::Equal => Fin { l: c, from: None },
        Ordering::Less if c.contains_zero() => Fin {
            l: zero(),
            from: None,
        },
        Ordering::Greater if c.contains_zero() => Unknown,
        _ => Dom { c, q, r },
    }
}

/// A dominant term: a `Dom`, or a limit away from 0 (class (0, 0)).
fn as_dom(a: Asy) -> Option<(Interval, Q, Q)> {
    match a {
        Dom { c, q, r } => Some((c, q, r)),
        Fin { l, .. } if !l.contains_zero() && l.is_bounded() => Some((l, Q::ZERO, Q::ZERO)),
        _ => None,
    }
}

/// A finite limit and the side it is approached from.
fn as_fin(a: Asy) -> Option<(Interval, Option<bool>)> {
    match a {
        Zero => Some((zero(), None)),
        Dom { c, q, r } if class(q, r) == Ordering::Less => Some((zero(), Some(c.gt0()))),
        Fin { l, from } => Some((l, from)),
        Tiny(s) => Some((zero(), s)),
        _ => None,
    }
}

/// An infinite limit: +∞ (`true`) or −∞.
fn as_inf(a: Asy) -> Option<bool> {
    match a {
        Dom { c, q, r } if class(q, r) == Ordering::Greater => Some(c.gt0()),
        Huge(s) | Big(s) => Some(s),
        _ => None,
    }
}

fn is_zero_point(l: Interval) -> bool {
    l.lo() == 0.0 && l.hi() == 0.0
}

fn neg(a: Asy) -> Asy {
    match a {
        Dom { c, q, r } => Dom { c: -c, q, r },
        Fin { l, from } => Fin {
            l: -l,
            from: from.map(|s| !s),
        },
        Tiny(s) => Tiny(s.map(|s| !s)),
        Huge(s) => Huge(!s),
        Big(s) => Big(!s),
        Bounded(b) => Bounded(-b),
        Zero | Unknown => a,
    }
}

fn add(a: Asy, b: Asy) -> Asy {
    match (a, b) {
        (Unknown, _) | (_, Unknown) => Unknown,
        (Zero, x) | (x, Zero) => x,
        (Huge(s), Huge(t)) | (Huge(s), Big(t)) | (Big(t), Huge(s)) => {
            if s == t {
                Huge(s)
            } else {
                Unknown
            }
        }
        // Anything else is below every power of u past some u.
        (Huge(s), _) | (_, Huge(s)) => Huge(s),
        (Big(s), Big(t)) => {
            if s == t {
                Big(s)
            } else {
                Unknown
            }
        }
        (Big(s), Dom { c, q, r }) | (Dom { c, q, r }, Big(s)) => {
            // Both unbounded: only the same way is sure.
            if class(q, r) == Ordering::Greater && c.gt0() != s {
                Unknown
            } else {
                Big(s)
            }
        }
        (Big(s), _) | (_, Big(s)) => Big(s),
        (
            Dom {
                c: c1,
                q: q1,
                r: r1,
            },
            Dom {
                c: c2,
                q: q2,
                r: r2,
            },
        ) => match (q1, r1).cmp(&(q2, r2)) {
            Ordering::Greater => a,
            Ordering::Less => b,
            Ordering::Equal => {
                let c = c1 + c2;
                if !c.contains_zero() {
                    Dom { c, q: q1, r: r1 }
                } else if class(q1, r1) == Ordering::Less {
                    Fin {
                        l: zero(),
                        from: None,
                    }
                } else {
                    Unknown
                }
            }
        },
        (Dom { c, q, r }, x) | (x, Dom { c, q, r }) => {
            // x tends to a value, to 0 fast, or stays bounded.
            if class(q, r) == Ordering::Greater {
                return Dom { c, q, r };
            }
            match x {
                Fin { l, from } => Fin {
                    l,
                    from: if from == Some(c.gt0()) { from } else { None },
                },
                Tiny(_) => Dom { c, q, r },
                Bounded(b) => Bounded(b),
                _ => Unknown,
            }
        }
        (Fin { l: l1, from: f1 }, Fin { l: l2, from: f2 }) => Fin {
            l: l1 + l2,
            from: if f1 == f2 { f1 } else { None },
        },
        (Fin { l, from }, Tiny(t)) | (Tiny(t), Fin { l, from }) => Fin {
            l,
            from: if from == t { from } else { None },
        },
        (Fin { l, .. }, Bounded(b)) | (Bounded(b), Fin { l, .. }) => Bounded(l + b),
        (Tiny(s), Tiny(t)) => Tiny(if s == t { s } else { None }),
        (Tiny(_), Bounded(b)) | (Bounded(b), Tiny(_)) => Bounded(b),
        (Bounded(x), Bounded(y)) => Bounded(x + y),
    }
}

/// The sign of a product of two signs, when both are known.
fn times(s: Option<bool>, t: Option<bool>) -> Option<bool> {
    Some(s? == t?)
}

fn mul(a: Asy, b: Asy) -> Asy {
    match (a, b) {
        (Unknown, _) | (_, Unknown) => return Unknown,
        (Zero, _) | (_, Zero) => return Zero,
        _ => {}
    }
    if let (Some((c1, q1, r1)), Some((c2, q2, r2))) = (as_dom(a), as_dom(b)) {
        return match (q1.add(q2), r1.add(r2)) {
            (Some(q), Some(r)) => dom(c1 * c2, q, r),
            _ => Unknown,
        };
    }
    let half = |a: Asy, b: Asy| -> Option<Asy> {
        Some(match (a, b) {
            (Huge(s), Huge(t)) | (Huge(s), Big(t)) => Huge(s == t),
            // Past every power, times a term or a limit away from 0.
            (Huge(s), x) if as_dom(x).is_some() => Huge(s == as_dom(x)?.0.gt0()),
            (Huge(s), Bounded(b)) if !b.contains_zero() => Huge(s == b.gt0()),
            (Huge(_), _) => Unknown,
            (Big(_), Huge(_)) => return None,
            (Big(s), Big(t)) => Big(s == t),
            (Big(s), x) if as_dom(x).is_some() => {
                let (c, q, r) = as_dom(x)?;
                if class(q, r) == Ordering::Less {
                    Unknown
                } else {
                    Big(s == c.gt0())
                }
            }
            (Big(s), Bounded(b)) if !b.contains_zero() => Big(s == b.gt0()),
            (Big(_), _) => Unknown,
            (Tiny(s), Tiny(t)) => Tiny(times(s, t)),
            (Tiny(s), x) if as_dom(x).is_some() => Tiny(times(s, Some(as_dom(x)?.0.gt0()))),
            (Tiny(_), Fin { .. } | Bounded(_)) => Tiny(None),
            (Tiny(_), _) => Unknown,
            // A vanishing term times a bounded factor.
            (Dom { c, q, r }, Fin { .. } | Bounded(_)) => {
                if class(q, r) == Ordering::Less {
                    Fin {
                        l: zero(),
                        from: None,
                    }
                } else if let Bounded(b) = b
                    && !b.contains_zero()
                {
                    Big(c.gt0() == b.gt0())
                } else {
                    Unknown
                }
            }
            (Fin { l: l1, from: f1 }, Fin { l: l2, from: f2 }) => Fin {
                l: l1 * l2,
                from: if is_zero_point(l1) && is_zero_point(l2) {
                    times(f1, f2)
                } else {
                    None
                },
            },
            (Fin { l, .. }, Bounded(b)) => {
                if is_zero_point(l) {
                    Fin {
                        l: zero(),
                        from: None,
                    }
                } else {
                    Bounded(l * b)
                }
            }
            (Bounded(x), Bounded(y)) => Bounded(x * y),
            _ => return None,
        })
    };
    half(a, b).or_else(|| half(b, a)).unwrap_or(Unknown)
}

fn recip(a: Asy) -> Asy {
    match a {
        Dom { c, q, r } => match (q.neg(), r.neg()) {
            (Some(q), Some(r)) => dom(c.recip(), q, r),
            _ => Unknown,
        },
        Fin { l, from } if !l.contains_zero() => Fin {
            l: l.recip(),
            from: from.map(|s| !s),
        },
        Fin { l, from: Some(s) } if is_zero_point(l) => Big(s),
        Huge(s) => Tiny(Some(s)),
        Big(s) => Fin {
            l: zero(),
            from: Some(s),
        },
        Tiny(Some(s)) => Huge(s),
        Bounded(b) if !b.contains_zero() => Bounded(b.recip()),
        _ => Unknown,
    }
}

fn dec(iv: Interval) -> DecInterval {
    DecInterval::new(iv)
}

/// c^k for an enclosure c of one sign and an exact k (a negative c only for
/// a whole k or an odd root, `odd`): the enclosure, or none.
fn power(c: Interval, k: Q, odd: bool) -> Option<Interval> {
    if c.gt0() {
        let v = elem::pow(&dec(c), &dec(k.interval()));
        return (!v.is_empty() && v.iv.is_bounded()).then_some(v.iv);
    }
    if c.lt0() && odd {
        let m = power(-c, k, odd)?;
        return Some(if k.numer() % 2 == 0 { m } else { -m });
    }
    None
}

/// a^k for an exact k; `odd`: k is whole, or written p/q with q odd, so a
/// negative a has a real power.
fn pow_q(a: Asy, k: Q, odd: bool) -> Asy {
    if k.is_zero() {
        return Fin {
            l: Interval::point(1.0),
            from: None,
        };
    }
    let up = k.signum() > 0;
    // The sign of a^k for a of sign s (none if a negative a has no power).
    let sign = |s: bool| -> Option<bool> {
        if s {
            Some(true)
        } else if odd {
            Some(k.numer() % 2 == 0)
        } else {
            None
        }
    };
    match a {
        Zero if up => Zero,
        Dom { c, q, r } => match (power(c, k, odd), q.mul(k), r.mul(k)) {
            (Some(c), Some(q), Some(r)) => dom(c, q, r),
            _ => Unknown,
        },
        Fin { l, .. } if !l.contains_zero() => match power(l, k, odd) {
            Some(v) => Fin { l: v, from: None },
            None => Unknown,
        },
        Fin { l, from: Some(s) } if is_zero_point(l) => match sign(s) {
            Some(t) if up => Fin {
                l: zero(),
                from: Some(t),
            },
            Some(t) => Big(t),
            None => Unknown,
        },
        Tiny(Some(s)) => match sign(s) {
            Some(t) if up => Tiny(Some(t)),
            Some(t) => Huge(t),
            None => Unknown,
        },
        Huge(s) => match sign(s) {
            Some(t) if up => Huge(t),
            Some(t) => Tiny(Some(t)),
            None => Unknown,
        },
        Big(s) => match sign(s) {
            Some(t) if up => Big(t),
            Some(t) => Fin {
                l: zero(),
                from: Some(t),
            },
            None => Unknown,
        },
        Bounded(b) if b.gt0() => match power(b, k, odd) {
            Some(v) => Bounded(v),
            None => Unknown,
        },
        _ => Unknown,
    }
}

/// a^k for a constant k known only to an enclosure (π, a slider): a
/// positive a only, its rate dropped.
fn pow_iv(a: Asy, k: Interval) -> Asy {
    if !k.is_bounded() || k.contains_zero() {
        return Unknown;
    }
    let up = k.gt0();
    let pw = |v: Interval| {
        let p = elem::pow(&dec(v), &dec(k));
        (!p.is_empty() && p.iv.is_bounded()).then_some(p.iv)
    };
    match a {
        Dom { c, q, r } if c.gt0() => {
            if (class(q, r) == Ordering::Greater) == up {
                Big(true)
            } else {
                Fin {
                    l: zero(),
                    from: Some(true),
                }
            }
        }
        Fin { l, .. } if l.gt0() => pw(l).map_or(Unknown, |v| Fin { l: v, from: None }),
        Fin {
            l,
            from: Some(true),
        } if is_zero_point(l) => {
            if up {
                Fin {
                    l: zero(),
                    from: Some(true),
                }
            } else {
                Big(true)
            }
        }
        Tiny(Some(true)) => {
            if up {
                Tiny(Some(true))
            } else {
                Huge(true)
            }
        }
        Huge(true) => {
            if up {
                Huge(true)
            } else {
                Tiny(Some(true))
            }
        }
        Big(true) => {
            if up {
                Big(true)
            } else {
                Fin {
                    l: zero(),
                    from: Some(true),
                }
            }
        }
        Bounded(b) if b.gt0() => pw(b).map_or(Unknown, Bounded),
        _ => Unknown,
    }
}

fn exp(a: Asy) -> Asy {
    let ev = |v: Interval| {
        let e = elem::exp(&dec(v));
        (!e.is_empty() && e.iv.is_bounded()).then_some(e.iv)
    };
    match a {
        Zero => Fin {
            l: Interval::point(1.0),
            from: None,
        },
        Dom { c, q, r } => match class(q, r) {
            // e^(c·u^q·(ln u)^r) with q > 0, or q = 0 and r > 1, is past
            // every power of u (c > 0) or below every inverse power.
            Ordering::Greater => {
                let fast = q.signum() > 0 || r > Q::ONE;
                match (c.gt0(), fast) {
                    (true, true) => Huge(true),
                    (true, false) => Big(true),
                    (false, true) => Tiny(Some(true)),
                    (false, false) => Fin {
                        l: zero(),
                        from: Some(true),
                    },
                }
            }
            _ => Fin {
                l: Interval::point(1.0),
                from: Some(c.gt0()),
            },
        },
        Fin { l, from } => ev(l).map_or(Unknown, |v| Fin { l: v, from }),
        Tiny(s) => Fin {
            l: Interval::point(1.0),
            from: s,
        },
        Huge(true) => Huge(true),
        Huge(false) => Tiny(Some(true)),
        Big(true) => Big(true),
        Big(false) => Fin {
            l: zero(),
            from: Some(true),
        },
        Bounded(b) => ev(b).map_or(Unknown, Bounded),
        Unknown => Unknown,
    }
}

fn ln(a: Asy) -> Asy {
    match a {
        // ln(c·u^q·(ln u)^r) = q·ln u + r·ln ln u + ln c + o(1).
        Dom { c, q, r } if c.gt0() => {
            if !q.is_zero() {
                dom(q.interval(), Q::ZERO, Q::ONE)
            } else {
                Big(r.signum() > 0)
            }
        }
        Fin { l, from } if l.gt0() => {
            let v = elem::ln(&dec(l));
            if v.is_empty() || !v.iv.is_bounded() {
                Unknown
            } else {
                Fin { l: v.iv, from }
            }
        }
        Fin {
            l,
            from: Some(true),
        } if is_zero_point(l) => Big(false),
        Tiny(Some(true)) => Big(false),
        Huge(true) | Big(true) => Big(true),
        Bounded(b) if b.gt0() => {
            let v = elem::ln(&dec(b));
            if v.is_empty() || !v.iv.is_bounded() {
                Unknown
            } else {
                Bounded(v.iv)
            }
        }
        _ => Unknown,
    }
}

/// φ continuous at the limit of a (from the side it is approached from):
/// φ's enclosure over a box just wider than the limit's, that side only
/// when the side is known, defined and continuous there.
fn cont(a: Asy, phi: &dyn Fn(&DecInterval) -> DecInterval) -> Asy {
    let Some((l, from)) = as_fin(a) else {
        return Unknown;
    };
    let b = match from {
        Some(true) => Interval::new(l.lo(), l.hi().next_up()),
        Some(false) => Interval::new(l.lo().next_down(), l.hi()),
        None => Interval::new(l.lo().next_down(), l.hi().next_up()),
    };
    let v = phi(&dec(b));
    if v.is_empty() || v.dec < Dec::Dac || !v.iv.is_bounded() {
        return Unknown;
    }
    Fin {
        l: v.iv,
        from: None,
    }
}

/// φ's limit at +∞ (`up`) or −∞, for a φ monotone out there with a finite
/// limit: its enclosure over the doubles' far end and beyond.
fn at_inf(up: bool, phi: &dyn Fn(&DecInterval) -> DecInterval) -> Asy {
    let b = if up {
        Interval::new(f64::MAX, f64::INFINITY)
    } else {
        Interval::new(f64::NEG_INFINITY, -f64::MAX)
    };
    let v = phi(&dec(b));
    if v.is_empty() || !v.iv.is_bounded() {
        return Unknown;
    }
    Fin {
        l: v.iv,
        from: None,
    }
}

/// Reads a tree as x → `at`.
struct Cx<'a, 'f> {
    f: &'a Fun<'f>,
    at: Toward,
    ctx: Ctx<'a>,
}

impl Cx<'_, '_> {
    fn unit(&self) -> TrigUnit {
        self.ctx.opts.trig_unit
    }

    /// An x-free tree's value.
    fn constant(&self, e: &Expr) -> Asy {
        // (With the simplifier's exact reading of numbers only.)
        if self.f.exact.is_some()
            && let Some((a, b)) = affine(e)
            && a.is_zero()
            && b.is_zero()
        {
            return Zero;
        }
        let v = taylor(e, Interval::point(0.0), 0, &self.ctx)[0];
        if v.is_empty() || v.dec < Dec::Def || !v.iv.is_bounded() {
            return Unknown;
        }
        Fin {
            l: v.iv,
            from: None,
        }
    }

    /// a·x + b (exact) as x → `at`.
    fn affine(&self, a: Q, b: Q) -> Asy {
        match self.at {
            Toward::PosInf | Toward::NegInf => {
                if a.is_zero() {
                    return if b.is_zero() {
                        Zero
                    } else {
                        Fin {
                            l: b.interval(),
                            from: None,
                        }
                    };
                }
                let c = if self.at == Toward::PosInf {
                    a.interval()
                } else {
                    -a.interval()
                };
                dom(c, Q::ONE, Q::ZERO)
            }
            Toward::Right(p) | Toward::Left(p) => {
                let right = matches!(self.at, Toward::Right(_));
                let Some(v) = Q::from_f64(p.0)
                    .and_then(|p| a.mul(p))
                    .and_then(|ap| ap.add(b))
                else {
                    return Unknown;
                };
                // a·x + b = v + a·(x − p) = v ± a/u.
                let s = if right { a.interval() } else { -a.interval() };
                if a.is_zero() {
                    return if v.is_zero() {
                        Zero
                    } else {
                        Fin {
                            l: v.interval(),
                            from: None,
                        }
                    };
                }
                if v.is_zero() {
                    dom(s, Q::ONE.neg().unwrap_or(Q::ZERO), Q::ZERO)
                } else {
                    Fin {
                        l: v.interval(),
                        from: Some(s.gt0()),
                    }
                }
            }
        }
    }

    fn asy(&self, e: &Expr) -> Asy {
        if !e.contains_x() {
            return self.constant(e);
        }
        if self.f.exact.is_some()
            && let Some((a, b)) = affine(e)
        {
            return self.affine(a, b);
        }
        match e {
            Expr::X => match self.at {
                Toward::PosInf => dom(Interval::point(1.0), Q::ONE, Q::ZERO),
                Toward::NegInf => dom(Interval::point(-1.0), Q::ONE, Q::ZERO),
                Toward::Right(p) | Toward::Left(p) => {
                    let right = matches!(self.at, Toward::Right(_));
                    if p.0 == 0.0 {
                        let c = if right { 1.0 } else { -1.0 };
                        dom(Interval::point(c), Q::ONE.neg().unwrap_or(Q::ZERO), Q::ZERO)
                    } else {
                        Fin {
                            l: Interval::point(p.0),
                            from: Some(right),
                        }
                    }
                }
            },
            Expr::Neg(a) => neg(self.asy(a)),
            Expr::Bin(op, a, b) => {
                let (x, y) = (a.as_ref(), b.as_ref());
                match op {
                    BinOp::Add => add(self.asy(x), self.asy(y)),
                    BinOp::Sub => add(self.asy(x), neg(self.asy(y))),
                    BinOp::Mul => mul(self.asy(x), self.asy(y)),
                    BinOp::Div => mul(self.asy(x), recip(self.asy(y))),
                    BinOp::Pow => self.pow(x, y),
                }
            }
            Expr::Call(func, args) => self.call(*func, args),
            _ => Unknown,
        }
    }

    fn pow(&self, base: &Expr, k: &Expr) -> Asy {
        if !k.contains_x() {
            let a = self.asy(base);
            if let Some((p, q)) = crate::compile::syntactic_rational(k, self.f.lits)
                && let Some(kq) = Q::new(i128::from(p), i128::from(q))
            {
                return pow_q(a, kq, q % 2 != 0);
            }
            if let Some(exact) = self.f.exact
                && let Some(v) = crate::simplify::period::exact_constant(k, exact)
                && v.k == 0
            {
                return pow_q(a, v.q, v.q.is_int());
            }
            let kv = taylor(k, Interval::point(0.0), 0, &self.ctx)[0];
            if kv.is_empty() || kv.dec < Dec::Def {
                return Unknown;
            }
            return pow_iv(a, kv.iv);
        }
        // a^b with b varying: e^(b·ln a), a > 0.
        exp(mul(self.asy(k), ln(self.asy(base))))
    }

    fn call(&self, func: Func, args: &[Expr]) -> Asy {
        let u = self.unit();
        if args.len() != 1 {
            return match func {
                Func::Root if args.len() == 2 && !args[1].contains_x() => {
                    match crate::compile::syntactic_rational(&args[1], self.f.lits) {
                        Some((n, 1)) if n != 0 => match Q::new(1, i128::from(n)) {
                            Some(k) => pow_q(self.asy(&args[0]), k, n % 2 != 0),
                            None => Unknown,
                        },
                        _ => Unknown,
                    }
                }
                Func::LogBase if args.len() == 2 && !args[0].contains_x() => {
                    let b = self.constant(&args[0]);
                    match b {
                        Fin { l, .. } if l.gt0() => {
                            let lb = elem::ln(&dec(l));
                            if lb.is_empty() || lb.iv.contains_zero() || !lb.iv.is_bounded() {
                                return Unknown;
                            }
                            mul(
                                ln(self.asy(&args[1])),
                                Fin {
                                    l: lb.iv.recip(),
                                    from: None,
                                },
                            )
                        }
                        _ => Unknown,
                    }
                }
                Func::Min | Func::Max if args.len() == 2 => {
                    let (a, b) = (self.asy(&args[0]), self.asy(&args[1]));
                    let max = func == Func::Max;
                    match (as_fin(a), as_fin(b), as_inf(a), as_inf(b)) {
                        (Some((l1, _)), Some((l2, _)), ..) => {
                            let m = if max {
                                elem::max(&dec(l1), &dec(l2))
                            } else {
                                elem::min(&dec(l1), &dec(l2))
                            };
                            Fin {
                                l: m.iv,
                                from: None,
                            }
                        }
                        // One unbounded, the other settling: max picks the
                        // one going to +∞, min the one going to −∞.
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
                        _ => Unknown,
                    }
                }
                _ => Unknown,
            };
        }
        let a = self.asy(&args[0]);
        match func {
            Func::Sqrt => pow_q(a, Q::new(1, 2).unwrap_or(Q::ONE), false),
            Func::Cbrt => pow_q(a, Q::new(1, 3).unwrap_or(Q::ONE), true),
            Func::Exp => exp(a),
            Func::Ln => ln(a),
            Func::Log => mul(
                ln(a),
                Fin {
                    l: elem::ln(&dec(Interval::point(10.0))).iv.recip(),
                    from: None,
                },
            ),
            Func::Abs => match a {
                Dom { c, q, r } => Dom { c: c.abs(), q, r },
                Huge(_) => Huge(true),
                Big(_) => Big(true),
                Tiny(s) => Tiny(s.map(|_| true)),
                Fin { l, from } if !l.contains_zero() => Fin {
                    l: l.abs(),
                    from: from.map(|f| f == l.gt0()),
                },
                Fin { l, from } if is_zero_point(l) => Fin {
                    l,
                    from: from.map(|_| true),
                },
                Fin { l, .. } => Fin {
                    l: l.abs(),
                    from: None,
                },
                Bounded(b) => Bounded(b.abs()),
                Zero | Unknown => a,
            },
            Func::Floor | Func::Ceil | Func::Round => {
                if as_inf(a).is_some() {
                    // f + a value in [−1, 1]: as f.
                    return add(a, Bounded(Interval::new(-1.0, 1.0)));
                }
                let phi = |v: &DecInterval| match func {
                    Func::Floor => elem::floor(v),
                    Func::Ceil => elem::ceil(v),
                    _ => elem::round(v),
                };
                match a {
                    Bounded(b) => {
                        let v = phi(&dec(b));
                        if v.is_empty() { Unknown } else { Bounded(v.iv) }
                    }
                    _ => cont(a, &phi),
                }
            }
            Func::Sign => match as_inf(a) {
                Some(s) => Fin {
                    l: Interval::point(if s { 1.0 } else { -1.0 }),
                    from: None,
                },
                None => cont(a, &elem::sign),
            },
            Func::Sin | Func::Cos => {
                if as_inf(a).is_some() || matches!(a, Bounded(_)) {
                    return Bounded(Interval::new(-1.0, 1.0));
                }
                if func == Func::Sin {
                    cont(a, &|v| elem::sin(v, u))
                } else {
                    cont(a, &|v| elem::cos(v, u))
                }
            }
            Func::Sinh => {
                let half = Fin {
                    l: Interval::point(0.5),
                    from: None,
                };
                mul(add(exp(a), neg(exp(neg(a)))), half)
            }
            Func::Cosh => {
                let half = Fin {
                    l: Interval::point(0.5),
                    from: None,
                };
                mul(add(exp(a), exp(neg(a))), half)
            }
            Func::Asinh => match as_inf(a) {
                Some(s) => Big(s),
                None => cont(a, &elem::asinh),
            },
            Func::Acosh => match as_inf(a) {
                Some(true) => Big(true),
                Some(false) => Unknown,
                None => cont(a, &elem::acosh),
            },
            _ => {
                // Monotone out there with a finite limit at ±∞, and
                // continuous inside: by the enclosure.
                let phi: Box<dyn Fn(&DecInterval) -> DecInterval> = match func {
                    Func::Atan => Box::new(move |v| elem::atan(v, u)),
                    Func::Acot => Box::new(move |v| elem::acot(v, u)),
                    Func::Asec => Box::new(move |v| elem::asec(v, u)),
                    Func::Acsc => Box::new(move |v| elem::acsc(v, u)),
                    Func::Tanh => Box::new(elem::tanh),
                    Func::Coth => Box::new(elem::coth),
                    Func::Sech => Box::new(elem::sech),
                    Func::Csch => Box::new(elem::csch),
                    Func::Acoth => Box::new(elem::acoth),
                    Func::Acsch => Box::new(elem::acsch),
                    // Continuous where defined, no limit at ±∞ needed.
                    Func::Tan => return cont(a, &|v| elem::tan(v, u)),
                    Func::Sec => return cont(a, &|v| elem::sec(v, u)),
                    Func::Csc => return cont(a, &|v| elem::csc(v, u)),
                    Func::Cot => return cont(a, &|v| elem::cot(v, u)),
                    Func::Asin => return cont(a, &|v| elem::asin(v, u)),
                    Func::Acos => return cont(a, &|v| elem::acos(v, u)),
                    Func::Atanh => return cont(a, &elem::atanh),
                    Func::Asech => return cont(a, &elem::asech),
                    _ => return Unknown,
                };
                match as_inf(a) {
                    Some(s) => at_inf(s, &*phi),
                    None => cont(a, &*phi),
                }
            }
        }
    }
}

/// f's limit (or f/x's, `over_x`) as x → `at`, by the tree's structure:
/// `None` when the rules don't decide it.
pub fn limit(f: &Fun<'_>, at: Toward, over_x: bool) -> Option<To> {
    let e = if over_x {
        Expr::bin(BinOp::Div, f.expr.clone(), Expr::X)
    } else {
        f.expr.clone()
    };
    let cx = Cx {
        f,
        at,
        ctx: Ctx::new(f.opts, f.lits),
    };
    let a = cx.asy(&e);
    match a {
        Zero => Some(To::In {
            lo: R(0.0),
            hi: R(0.0),
        }),
        _ => {
            if let Some(s) = as_inf(a) {
                return Some(if s { To::PosInf } else { To::NegInf });
            }
            let (l, _) = as_fin(a)?;
            (l.is_bounded() && !l.is_empty()).then_some(To::In {
                lo: R(l.lo()),
                hi: R(l.hi()),
            })
        }
    }
}
