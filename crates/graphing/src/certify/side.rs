//! Where f is defined: the side conditions of its partial operations, and
//! the domain they make.
//!
//! f is defined at x exactly when every partial operation in its tree is
//! (a divisor ≠ 0, a logarithm's argument > 0, an even root's ≥ 0, a
//! power with a varying exponent's base > 0 or 0 to a positive power, the
//! cosine under tan and sec ≠ 0, …), the same rules as the evaluator
//! (`functions.rs`) and the interval core. Each condition asks a *side
//! expression* g to take values in an allowed set; the domain is where all
//! of them hold. Conditions are taken children first, so each one's cover
//! runs only where the conditions inside it already hold (g is defined
//! there).
//!
//! The zeros of sin, cos, tan, cot and 1 ± sin, 1 ± cos of an affine
//! argument are infinite families no cover can list: they come from a
//! table, and the domain excludes them as families.

use super::cert::{
    Bound, Certificate, Claim, DomainValue, Enc, Family, Piece, Region, Row, Subject, Via, XBox,
};
use super::cover::{Cover, Leaf, Target, claims};
use super::fun::{Fun, Stop, at_path};
use crate::ast::{BinOp, Expr, Func};
use crate::compile::syntactic_rational;
use crate::functions::TrigUnit;
use crate::interval::{DecInterval, Interval, elem};

/// An interval of allowed values of g.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seg {
    pub lo: f64,
    pub lo_in: bool,
    pub hi: f64,
    pub hi_in: bool,
}

const fn seg(lo: f64, lo_in: bool, hi: f64, hi_in: bool) -> Seg {
    Seg {
        lo,
        lo_in,
        hi,
        hi_in,
    }
}

const INF: f64 = f64::INFINITY;
const NONZERO: &[Seg] = &[seg(-INF, false, 0.0, false), seg(0.0, false, INF, false)];
const POSITIVE: &[Seg] = &[seg(0.0, false, INF, false)];
const NONNEG: &[Seg] = &[seg(0.0, true, INF, false)];
const UNIT: &[Seg] = &[seg(-1.0, true, 1.0, true)];
const OPEN_UNIT: &[Seg] = &[seg(-1.0, false, 1.0, false)];
const OUTSIDE_UNIT: &[Seg] = &[seg(-INF, false, -1.0, true), seg(1.0, true, INF, false)];
const OPEN_OUTSIDE_UNIT: &[Seg] = &[seg(-INF, false, -1.0, false), seg(1.0, false, INF, false)];
const AT_LEAST_ONE: &[Seg] = &[seg(1.0, true, INF, false)];
const ZERO_TO_ONE: &[Seg] = &[seg(0.0, false, 1.0, true)];
const LOG_BASE: &[Seg] = &[seg(0.0, false, 1.0, false), seg(1.0, false, INF, false)];

/// Every value in [lo, hi] is allowed (one segment holds them all).
fn allows_all(segs: &[Seg], lo: f64, hi: f64) -> bool {
    segs.iter()
        .any(|s| (lo > s.lo || (lo == s.lo && s.lo_in)) && (hi < s.hi || (hi == s.hi && s.hi_in)))
}

fn allows(segs: &[Seg], v: f64) -> bool {
    segs.iter()
        .any(|s| (v > s.lo || (v == s.lo && s.lo_in)) && (v < s.hi || (v == s.hi && s.hi_in)))
}

/// One condition: the side expression must take values in `allowed`.
#[derive(Clone, Debug)]
pub struct SideCond {
    pub path: Vec<u8>,
    pub via: Via,
    /// The expression to evaluate: the sub-tree, or cos/sin of a trig
    /// call's argument.
    pub g: Expr,
    pub allowed: Vec<Seg>,
    /// For a power with a varying (or non-integer) exponent: g = 0 is
    /// allowed exactly where the exponent at this path is > 0.
    pub zero_if_exponent: Option<Vec<u8>>,
}

impl SideCond {
    pub fn subject(&self) -> Subject {
        Subject::G {
            path: self.path.clone(),
            via: self.via,
        }
    }
}

/// The side conditions of f, children first; or why they can't be stated.
impl SideCond {
    /// The condition is g ≠ 0 (no more, no less).
    pub fn allowed_nonzero(&self) -> bool {
        self.allowed.as_slice() == NONZERO && self.zero_if_exponent.is_none()
    }
}

pub fn sides(f: &Fun<'_>) -> Result<Vec<SideCond>, String> {
    let mut out = Vec::new();
    walk(f, &f.expr, &mut Vec::new(), &mut out)?;
    // |v| > 0 is v ≠ 0, |v| ≥ 0 always holds: on v itself, which is
    // smooth where |v| has its corner.
    let mut done = Vec::new();
    for mut c in out {
        if c.via == Via::Itself
            && c.zero_if_exponent.is_none()
            && let Expr::Call(Func::Abs, args) = &c.g
        {
            if c.allowed.as_slice() == NONNEG {
                continue;
            }
            if c.allowed.as_slice() == POSITIVE || c.allowed.as_slice() == NONZERO {
                c.g = args[0].clone();
                c.path.push(0);
                c.allowed = NONZERO.to_vec();
            }
        }
        done.push(c);
    }
    Ok(done)
}

fn cond(path: &[u8], g: &Expr, allowed: &[Seg]) -> SideCond {
    SideCond {
        path: path.to_vec(),
        via: Via::Itself,
        g: g.clone(),
        allowed: allowed.to_vec(),
        zero_if_exponent: None,
    }
}

/// The value of an x-free sub-tree.
fn constant(f: &Fun<'_>, e: &Expr) -> Option<DecInterval> {
    f.ser_of(e, Interval::point(0.0), 0).ok().map(|s| s[0])
}

/// The factors of g whose zeros are g's zeros (a product's factors, a
/// positive power's base, a quotient's numerator; its divisor has a
/// condition of its own). A constant factor that is exactly 0 makes g ≡ 0.
fn nonzero_factors(
    f: &Fun<'_>,
    e: &Expr,
    path: &mut Vec<u8>,
    out: &mut Vec<SideCond>,
) -> Result<(), String> {
    let mut child = |i: u8, c: &Expr, out: &mut Vec<SideCond>| -> Result<(), String> {
        path.push(i);
        let r = nonzero_factors(f, c, path, out);
        path.pop();
        r
    };
    match e {
        Expr::Neg(a) => child(0, a, out),
        Expr::Bin(BinOp::Mul, a, b) => {
            child(0, a, out)?;
            child(1, b, out)
        }
        Expr::Bin(BinOp::Div, a, _) => child(0, a, out),
        Expr::Bin(BinOp::Pow, a, b) if matches!(syntactic_rational(b), Some((p, _)) if p > 0) => {
            child(0, a, out)
        }
        e if e.contains_x() => {
            out.push(cond(path, e, NONZERO));
            Ok(())
        }
        e if e.contains_y() => Err("involves y".into()),
        e => match constant(f, e) {
            Some(v) if v.ne0() => Ok(()),
            Some(v) if v.lo() == 0.0 && v.hi() == 0.0 => {
                // A divisor that is exactly 0 everywhere.
                out.push(cond(path, e, NONZERO));
                Ok(())
            }
            _ => Err("a constant divisor that may be 0".into()),
        },
    }
}

fn walk(f: &Fun<'_>, e: &Expr, path: &mut Vec<u8>, out: &mut Vec<SideCond>) -> Result<(), String> {
    // Children first.
    match e {
        Expr::Num(_) | Expr::Const(_) | Expr::X | Expr::Var(_) => {}
        Expr::Y => return Err("involves y".into()),
        Expr::Neg(a) => {
            path.push(0);
            walk(f, a, path, out)?;
            path.pop();
        }
        Expr::Degrees(a) => {
            if f.opts.trig_unit != TrigUnit::Degrees {
                return Err("° outside degrees mode".into());
            }
            path.push(0);
            walk(f, a, path, out)?;
            path.pop();
        }
        Expr::Bin(_, a, b) => {
            path.push(0);
            walk(f, a, path, out)?;
            path.pop();
            path.push(1);
            walk(f, b, path, out)?;
            path.pop();
        }
        Expr::Call(_, args) => {
            for (i, a) in args.iter().enumerate() {
                path.push(i as u8);
                walk(f, a, path, out)?;
                path.pop();
            }
        }
    }
    // Then the node's own condition.
    let child_path = |i: u8| {
        let mut p = path.clone();
        p.push(i);
        p
    };
    match e {
        Expr::Bin(BinOp::Div, _, b) => {
            let mut p = child_path(1);
            nonzero_factors(f, b, &mut p, out)?;
        }
        Expr::Bin(BinOp::Pow, a, b) => {
            let base = child_path(0);
            // As `interval::taylor`: a literal integer or fraction, then a
            // constant integer, then the general power.
            let rational = syntactic_rational(b).or_else(|| {
                if b.contains_x() {
                    return None;
                }
                let v = constant(f, b)?;
                (v.iv.is_point() && v.lo() == v.lo().trunc() && v.lo().abs() < 1e6)
                    .then_some((v.lo() as i32, 1))
            });
            match rational {
                Some((p, 1)) if p > 0 => {}
                Some((_, 1)) => {
                    let mut bp = base;
                    nonzero_factors(f, a, &mut bp, out)?;
                }
                Some((p, q)) if q % 2 == 0 => {
                    out.push(cond(&base, a, if p > 0 { NONNEG } else { POSITIVE }))
                }
                Some((p, _)) => {
                    if p < 0 {
                        let mut bp = base;
                        nonzero_factors(f, a, &mut bp, out)?;
                    }
                }
                None if !a.contains_x()
                    && constant(f, a).is_some_and(|v| v.lo() == 0.0 && v.hi() == 0.0) =>
                {
                    // 0^b: defined exactly where b > 0 (0⁰ is not).
                    out.push(cond(&child_path(1), b, POSITIVE));
                }
                None => {
                    // b > 0 base, or a zero base to a positive power.
                    let mut c = cond(&base, a, POSITIVE);
                    c.zero_if_exponent = Some(child_path(1));
                    out.push(c);
                }
            }
        }
        Expr::Call(func, args) => {
            use Func::*;
            let arg = |i: usize| (child_path(i as u8), &args[i]);
            match func {
                Tan | Sec | Cot | Csc => {
                    let (s, via) = if matches!(func, Tan | Sec) {
                        (Cos, Via::CosOfArg)
                    } else {
                        (Sin, Via::SinOfArg)
                    };
                    out.push(SideCond {
                        path: path.clone(),
                        via,
                        g: Expr::call1(s, args[0].clone()),
                        allowed: NONZERO.to_vec(),
                        zero_if_exponent: None,
                    });
                }
                Csch | Coth | Acsch => {
                    let (mut p, a) = arg(0);
                    nonzero_factors(f, a, &mut p, out)?;
                }
                Ln | Log => {
                    let (p, a) = arg(0);
                    out.push(cond(&p, a, POSITIVE));
                }
                LogBase => {
                    let (p0, b) = arg(0);
                    out.push(cond(&p0, b, LOG_BASE));
                    let (p1, a) = arg(1);
                    out.push(cond(&p1, a, POSITIVE));
                }
                Sqrt => {
                    let (p, a) = arg(0);
                    out.push(cond(&p, a, NONNEG));
                }
                Root => {
                    let (p, a) = arg(0);
                    let n = &args[1];
                    if n.contains_x() {
                        return Err("a root of varying degree".into());
                    }
                    let Some(v) = constant(f, n) else {
                        return Err("a root of unknown degree".into());
                    };
                    if !(v.iv.is_point() && v.lo() == v.lo().trunc() && v.lo().abs() < 1e6) {
                        return Err("a root of non-integer degree".into());
                    }
                    let d = v.lo() as i64;
                    match (d % 2 == 0, d > 0) {
                        _ if d == 0 => return Err("a root of degree 0".into()),
                        (true, true) => out.push(cond(&p, a, NONNEG)),
                        (true, false) => out.push(cond(&p, a, POSITIVE)),
                        (false, true) => {}
                        (false, false) => {
                            let mut pp = p;
                            nonzero_factors(f, a, &mut pp, out)?;
                        }
                    }
                }
                Asin | Acos => {
                    let (p, a) = arg(0);
                    out.push(cond(&p, a, UNIT));
                }
                Asec | Acsc => {
                    let (p, a) = arg(0);
                    out.push(cond(&p, a, OUTSIDE_UNIT));
                }
                Acosh => {
                    let (p, a) = arg(0);
                    out.push(cond(&p, a, AT_LEAST_ONE));
                }
                Atanh => {
                    let (p, a) = arg(0);
                    out.push(cond(&p, a, OPEN_UNIT));
                }
                Asech => {
                    let (p, a) = arg(0);
                    out.push(cond(&p, a, ZERO_TO_ONE));
                }
                Acoth => {
                    let (p, a) = arg(0);
                    out.push(cond(&p, a, OPEN_OUTSIDE_UNIT));
                }
                Mod => {
                    let (mut p, b) = arg(1);
                    nonzero_factors(f, b, &mut p, out)?;
                }
                Factorial | DoubleFactorial | NCr | NPr => {
                    return Err(format!("{} has no side condition table", func.name()));
                }
                _ => {}
            }
        }
        _ => {}
    }
    Ok(())
}

// ------------------------------------------------------------- families

/// Half a turn in the unit: π, 180 or 200.
fn half_turn(unit: TrigUnit) -> Interval {
    match unit {
        TrigUnit::Radians => elem::pi(),
        u => Interval::point(u.full_turn() / 2.0),
    }
}

/// `u = α·x + β` with α ≠ 0, both enclosed.
fn affine(f: &Fun<'_>, u: &Expr) -> Option<(Interval, Interval)> {
    match u {
        Expr::X => Some((Interval::point(1.0), Interval::point(0.0))),
        e if !e.contains_x() && !e.contains_y() => {
            let v = constant(f, e)?;
            (!v.is_empty() && v.iv.is_bounded()).then_some((Interval::point(0.0), v.iv))
        }
        Expr::Neg(a) => {
            let (al, be) = affine(f, a)?;
            Some((-al, -be))
        }
        Expr::Degrees(a) => affine(f, a),
        Expr::Bin(BinOp::Add, a, b) => {
            let ((a1, b1), (a2, b2)) = (affine(f, a)?, affine(f, b)?);
            Some((a1 + a2, b1 + b2))
        }
        Expr::Bin(BinOp::Sub, a, b) => {
            let ((a1, b1), (a2, b2)) = (affine(f, a)?, affine(f, b)?);
            Some((a1 - a2, b1 - b2))
        }
        Expr::Bin(BinOp::Mul, a, b) => {
            let ((a1, b1), (a2, b2)) = (affine(f, a)?, affine(f, b)?);
            if a1.lo() == 0.0 && a1.hi() == 0.0 {
                Some((b1 * a2, b1 * b2))
            } else if a2.lo() == 0.0 && a2.hi() == 0.0 {
                Some((a1 * b2, b1 * b2))
            } else {
                None
            }
        }
        Expr::Bin(BinOp::Div, a, b) => {
            let ((a1, b1), (a2, b2)) = (affine(f, a)?, affine(f, b)?);
            (a2.lo() == 0.0 && a2.hi() == 0.0 && !b2.contains_zero()).then_some((a1 / b2, b1 / b2))
        }
        _ => None,
    }
}

/// x-zeros of s(u) for an affine u, given the zeros of s as
/// `u₀ + k·period_u`.
fn family_of(
    f: &Fun<'_>,
    u: &Expr,
    u0: Interval,
    period_u: Interval,
) -> Option<(Interval, Interval)> {
    let (al, be) = affine(f, u)?;
    if al.contains_zero() || !al.is_bounded() || !be.is_bounded() {
        return None;
    }
    let t = (u0 - be) / al;
    let period = period_u / al.abs();
    if !t.is_bounded() || !period.is_bounded() || period.lo() <= 0.0 {
        return None;
    }
    // The member nearest 0.
    let n = (t.mid() / period.mid()).round();
    if !n.is_finite() || n.abs() > 1e15 {
        return None;
    }
    let x0 = t - Interval::point(n) * period;
    Some((x0, period))
}

/// If g's zero set is a table family (in x), its representative and
/// period.
pub fn table_family(f: &Fun<'_>, g: &Expr) -> Option<(Interval, Interval)> {
    let unit = f.opts.trig_unit;
    let h = half_turn(unit);
    let two_h = h * Interval::point(2.0);
    let half_h = h / Interval::point(2.0);
    let trig = |e: &Expr| -> Option<(Func, Expr)> {
        match e {
            Expr::Call(func @ (Func::Sin | Func::Cos | Func::Tan | Func::Cot), args) => {
                Some((*func, args[0].clone()))
            }
            _ => None,
        }
    };
    let is_one = |e: &Expr| matches!(e, Expr::Num(v) if *v == 1.0);
    if let Some((func, u)) = trig(g) {
        let u0 = match func {
            Func::Sin | Func::Tan => Interval::point(0.0),
            _ => half_h,
        };
        return family_of(f, &u, u0, h);
    }
    // 1 ± s(u), s(u) − 1.
    let (sign, inner) = match g {
        Expr::Bin(BinOp::Add, a, b) if is_one(a) => (1.0, &**b),
        Expr::Bin(BinOp::Add, a, b) if is_one(b) => (1.0, &**a),
        Expr::Bin(BinOp::Sub, a, b) if is_one(a) => (-1.0, &**b),
        Expr::Bin(BinOp::Sub, a, b) if is_one(b) => (-1.0, &**a),
        _ => return None,
    };
    let (func, u) = trig(inner)?;
    // 1 + s = 0 where s = −1; 1 − s = 0 and s − 1 = 0 where s = 1.
    let target = -sign;
    let u0 = match (func, target > 0.0) {
        (Func::Cos, true) => Interval::point(0.0),
        (Func::Cos, false) => h,
        (Func::Sin, true) => half_h,
        (Func::Sin, false) => -half_h,
        _ => return None,
    };
    family_of(f, &u, u0, two_h)
}

// ------------------------------------------------------------- domain

/// An excluded family, with the side it comes from.
#[derive(Clone, Debug)]
pub struct XFamily {
    pub x0: Interval,
    pub period: Interval,
    pub subject: Subject,
}

/// The most members [`XFamily::members`] lists.
pub const MAX_MEMBERS: f64 = 1e5;

impl XFamily {
    /// The members within `[lo, hi]`, enclosed; `None` when there are too
    /// many to list (more than [`MAX_MEMBERS`]).
    pub fn members(&self, lo: f64, hi: f64) -> Option<Vec<Interval>> {
        let p = self.period;
        let k0 = ((lo - self.x0.hi()) / p.lo()).floor() - 1.0;
        let k1 = ((hi - self.x0.lo()) / p.lo()).ceil() + 1.0;
        let mut out = Vec::new();
        if !(k0.is_finite() && k1.is_finite()) || k1 - k0 > MAX_MEMBERS {
            return None;
        }
        let mut k = k0;
        while k <= k1 {
            let m = self.x0 + Interval::point(k) * p;
            if m.hi() >= lo && m.lo() <= hi {
                out.push(m);
            }
            k += 1.0;
        }
        Some(out)
    }
}

/// The domain and what the other rows need from it.
pub struct Domain {
    pub row: Row<DomainValue>,
    /// The domain's pieces (when known).
    pub pieces: Vec<Piece>,
    /// Excluded families.
    pub families: Vec<XFamily>,
    /// Claims (side covers, families) the domain rests on.
    pub claims: Vec<Claim>,
}

/// The closed x-box a piece's interior is covered by: an open end at a
/// double starts one double inside it, an end known only to an enclosure
/// leaves the enclosure out (a gap).
pub fn piece_box(p: &Piece) -> (f64, f64) {
    let lo = match p.lo {
        Bound::NegInf => -INF,
        Bound::PosInf => INF,
        Bound::At { x, closed } => {
            if x.is_point() {
                if closed { x.lo.0 } else { x.lo.0.next_up() }
            } else {
                x.hi.0.next_up()
            }
        }
    };
    let hi = match p.hi {
        Bound::NegInf => -INF,
        Bound::PosInf => INF,
        Bound::At { x, closed } => {
            if x.is_point() {
                if closed { x.lo.0 } else { x.lo.0.next_down() }
            } else {
                x.lo.0.next_down()
            }
        }
    };
    (lo, hi)
}

/// One stretch of x in the scan of a side's cover.
#[derive(Clone, Copy)]
enum Item {
    /// A box, in or out of the allowed set.
    Run(bool),
    /// A single point (enclosed), in or out.
    Point(Enc, bool),
}

/// Where the side holds inside one piece, from its cover. `None` if the
/// cover leaves something undecided.
fn pieces_of(
    f: &Fun<'_>,
    s: &SideCond,
    cs: &[f64],
    piece: &Piece,
    cover: &Cover,
) -> Result<Option<Vec<Piece>>, Stop> {
    if !cover.complete() {
        return Ok(None);
    }
    let band_in = |i: usize| -> bool {
        let probe = if cs.is_empty() {
            0.0
        } else if i == 0 {
            cs[0] - 1.0
        } else if i >= cs.len() {
            cs[cs.len() - 1] + 1.0
        } else {
            cs[i - 1] / 2.0 + cs[i] / 2.0
        };
        allows(&s.allowed, probe)
    };
    // g = cⱼ at x: allowed by the set, or (a zero base) when the exponent
    // is > 0 there.
    let value_in = |j: usize, x: Interval| -> Result<Option<bool>, Stop> {
        let c = cs[j];
        if c == 0.0
            && let Some(ep) = &s.zero_if_exponent
        {
            let Some(e) = at_path(&f.expr, ep) else {
                return Ok(None);
            };
            let v = f.ser_of(e, x, 0)?[0];
            return Ok(if v.gt0() {
                Some(true)
            } else if v.hi() <= 0.0 || v.lt0() {
                Some(false)
            } else {
                None
            });
        }
        Ok(Some(allows(&s.allowed, c)))
    };
    let mut items = Vec::new();
    for l in &cover.leaves {
        match *l {
            Leaf::Band { band, .. } => items.push(Item::Run(band_in(band))),
            Leaf::Equal { a, b, j, .. } => {
                let Some(v) = value_in(j, Interval::new(a, b))? else {
                    return Ok(None);
                };
                items.push(Item::Run(v));
            }
            Leaf::Undefined { .. } => items.push(Item::Run(false)),
            Leaf::At { p, j } => {
                let Some(v) = value_in(j, Interval::point(p))? else {
                    return Ok(None);
                };
                items.push(Item::Point(Enc::point(p), v));
            }
            Leaf::Cross {
                a,
                b,
                l,
                r,
                j,
                exact,
                rising,
            } => {
                let (left, right) = if rising { (j, j + 1) } else { (j + 1, j) };
                let at = match exact {
                    Some(p) => Interval::point(p),
                    None => Interval::new(l, r),
                };
                let Some(v) = value_in(j, at)? else {
                    return Ok(None);
                };
                let (x, before, after) = match exact {
                    Some(p) => (Enc::point(p), a < p, p < b),
                    None => (Enc::new(l, r), true, true),
                };
                if before {
                    items.push(Item::Run(band_in(left)));
                }
                items.push(Item::Point(x, v));
                if after {
                    items.push(Item::Run(band_in(right)));
                }
            }
            Leaf::Touch { a, p, j, above, .. } => {
                let Some(v) = value_in(j, Interval::point(p))? else {
                    return Ok(None);
                };
                let run = Item::Run(band_in(j + usize::from(above)));
                let at = Item::Point(Enc::point(p), v);
                if p == a {
                    items.extend([at, run]);
                } else {
                    items.extend([run, at]);
                }
            }
            Leaf::Flag { .. } => return Ok(None),
        }
    }
    // A point seen from both sides (its own leaf and a box ending there).
    items.dedup_by(
        |y, x| matches!((*x, *y), (Item::Point(p, u), Item::Point(q, v)) if p == q && u == v),
    );
    Ok(scan(&items, piece))
}

/// Pieces of the allowed set from a scan of runs and points in x order.
/// Membership changes only at a point; a change between two runs with no
/// point between them would contradict the enclosures (`None`).
fn scan(items: &[Item], piece: &Piece) -> Option<Vec<Piece>> {
    let mut out = Vec::new();
    // The open piece's lower bound, and the point just before (if the
    // last item was one).
    let mut cur: Option<Bound> = None;
    let mut point: Option<(Enc, bool)> = None;
    for (i, it) in items.iter().enumerate() {
        match *it {
            Item::Run(inn) => {
                match (inn, cur) {
                    (true, None) => {
                        cur = Some(match point {
                            Some((x, pin)) => Bound::At { x, closed: pin },
                            None if i == 0 => piece.lo,
                            None => return None,
                        });
                    }
                    (false, Some(lo)) => {
                        let (x, pin) = point?;
                        out.push(Piece {
                            lo,
                            hi: Bound::At { x, closed: pin },
                        });
                        cur = None;
                    }
                    (false, None) => {
                        // A lone allowed point between two runs that aren't.
                        if let Some((x, true)) = point {
                            let b = Bound::At { x, closed: true };
                            out.push(Piece { lo: b, hi: b });
                        }
                    }
                    (true, Some(_)) => {}
                }
                point = None;
            }
            Item::Point(x, inn) => {
                if point.is_some() {
                    return None;
                }
                if let (Some(lo), false) = (cur, inn) {
                    out.push(Piece {
                        lo,
                        hi: Bound::At { x, closed: false },
                    });
                    cur = None;
                }
                point = Some((x, inn));
            }
        }
    }
    match (cur, point) {
        (Some(lo), _) => out.push(Piece { lo, hi: piece.hi }),
        (None, Some((x, true))) => {
            let b = Bound::At { x, closed: true };
            out.push(Piece { lo: b, hi: b });
        }
        _ => {}
    }
    Some(out)
}

/// What a piece's box (`[lo, hi]`, from [`piece_box`]) leaves out at the
/// piece's ends: an open end at a double is undefined, and the reals
/// between a bound and the double the box starts from (a bound known only
/// to an enclosure, or the reals beside an open end) are a gap.
fn end_gaps(p: &Piece, lo: f64, hi: f64, out: &mut Vec<Claim>) {
    for (b, is_lo) in [(p.lo, true), (p.hi, false)] {
        let Bound::At { x, closed } = b else { continue };
        let undefined = Claim::Undefined {
            x: XBox::point(x.lo.0),
        };
        if !closed && x.is_point() && !out.contains(&undefined) {
            out.push(undefined);
        }
        let (from, to) = if is_lo { (x.lo.0, lo) } else { (hi, x.hi.0) };
        let gap = Claim::Gap {
            x: XBox::new(from, to),
        };
        if from < to && !out.contains(&gap) {
            out.push(gap);
        }
    }
}

/// The whole line, as one piece.
pub fn line() -> Piece {
    Piece {
        lo: Bound::NegInf,
        hi: Bound::PosInf,
    }
}

/// Where f is defined.
pub fn domain(f: &Fun<'_>) -> Domain {
    let unknown = |reason: String, families: Vec<XFamily>| Domain {
        row: Row::unknown(reason),
        pieces: vec![line()],
        families,
        claims: Vec::new(),
    };
    let conds = match sides(f) {
        Ok(c) => c,
        Err(why) => return unknown(why, Vec::new()),
    };
    let mut pieces = vec![line()];
    let mut families: Vec<XFamily> = Vec::new();
    let mut cl = Vec::new();
    for s in &conds {
        let subject = s.subject();
        if s.allowed.as_slice() == NONZERO
            && let Some((x0, period)) = table_family(f, &s.g)
        {
            cl.push(Claim::Family {
                of: subject.clone(),
                x0: XBox::new(x0.lo(), x0.hi()),
                period: XBox::new(period.lo(), period.hi()),
            });
            families.push(XFamily {
                x0,
                period,
                subject,
            });
            continue;
        }
        let mut cs: Vec<f64> = s
            .allowed
            .iter()
            .flat_map(|g| [g.lo, g.hi])
            .filter(|v| v.is_finite())
            .collect();
        cs.sort_by(f64::total_cmp);
        cs.dedup();
        let target = Target {
            expr: &s.g,
            k: 0,
            in_domain: false,
        };
        let mut next = Vec::new();
        for p in &pieces {
            let (lo, hi) = piece_box(p);
            if lo > hi {
                continue;
            }
            // The cover says nothing about the reals the box leaves out.
            end_gaps(p, lo, hi, &mut cl);
            let cover = Cover::run(f, &target, &cs, &[(lo, hi)]);
            if let Some(stop) = cover.stopped {
                return unknown(
                    match stop {
                        Stop::Budget => "out of budget".into(),
                        Stop::Cancelled => "cancelled".into(),
                    },
                    families,
                );
            }
            cl.extend(claims(&cover, &subject, &cs));
            match pieces_of(f, s, &cs, p, &cover) {
                Ok(Some(ps)) => {
                    // A closed end known only to an enclosure (a gap the
                    // cover left out) keeps this piece's bound: the
                    // condition must hold over the whole enclosure, so at
                    // the end itself (1/√(1 − (x/0.001)²) is undefined at
                    // the ends √ allows).
                    for q in &ps {
                        for (b, own) in [(q.lo, p.lo), (q.hi, p.hi)] {
                            if let Bound::At { x, closed: true } = b
                                && !x.is_point()
                                && b == own
                            {
                                let v = f.ser_of(&s.g, Interval::new(x.lo.0, x.hi.0), 0);
                                let ok = match v {
                                    Ok(v) => {
                                        let v = v[0];
                                        !v.is_empty()
                                            && s.zero_if_exponent.is_none()
                                            && allows_all(&s.allowed, v.lo(), v.hi())
                                    }
                                    Err(_) => false,
                                };
                                if !ok {
                                    return unknown(
                                        "a closed end known to an enclosure is not decided".into(),
                                        families,
                                    );
                                }
                            }
                        }
                    }
                    next.extend(ps)
                }
                Ok(None) => {
                    return unknown(
                        format!("where {} is allowed is not decided", s.g.formula()),
                        families,
                    );
                }
                Err(_) => return unknown("out of budget".into(), families),
            }
        }
        pieces = next;
    }
    for p in &pieces {
        let (lo, hi) = piece_box(p);
        end_gaps(p, lo, hi, &mut cl);
    }
    let mut cert = Certificate::new(Region::Line);
    cert.extend(cl.iter().cloned());
    Domain {
        row: Row::Certified {
            value: DomainValue {
                pieces: pieces.clone(),
                excluded: families
                    .iter()
                    .map(|fam| Family {
                        x0: Enc::new(fam.x0.lo(), fam.x0.hi()),
                        period: Enc::new(fam.period.lo(), fam.period.hi()),
                    })
                    .collect(),
            },
            cert,
        },
        pieces,
        families,
        claims: cl,
    }
}

/// One closed box of the domain's interior, and the bounds it stands for
/// at each end: a domain bound, an excluded family member, or (`clipped`)
/// the edge of the window.
#[derive(Clone, Copy, Debug)]
pub struct IBox {
    pub a: f64,
    pub b: f64,
    pub lo: Bound,
    pub hi: Bound,
    pub lo_clipped: bool,
    pub hi_clipped: bool,
}

/// Closed boxes covering the domain's interior for the other rows: within
/// `window` when given (one period of a periodic f, or `[-w, w]` when
/// there are excluded families: an infinite family can't be covered), with
/// each family member cut out (a member at a double is
/// left out exactly, one known only to an enclosure leaves a gap).
/// Returns the boxes, the claims for what they leave out (gaps, excluded
/// doubles), and whether they reach the whole line.
pub fn interior(d: &Domain, window: Option<(f64, f64)>) -> (Vec<IBox>, Vec<Claim>, bool) {
    let mut boxes = Vec::new();
    let mut gaps = Vec::new();
    let whole = window.is_none();
    for p in &d.pieces {
        let (mut lo, mut hi) = piece_box(p);
        let (mut lo_b, mut hi_b) = (p.lo, p.hi);
        let (mut lo_c, mut hi_c) = (false, false);
        if let Some((a, b)) = window {
            if lo < a {
                lo = a;
                lo_b = Bound::At {
                    x: Enc::point(a),
                    closed: true,
                };
                lo_c = true;
            }
            if hi > b {
                hi = b;
                hi_b = Bound::At {
                    x: Enc::point(b),
                    closed: true,
                };
                hi_c = true;
            }
        }
        if lo > hi {
            continue;
        }
        let (blo, bhi) = piece_box(p);
        end_gaps(p, blo, bhi, &mut gaps);
        // Cut out the family members in [lo, hi]. (Too many to list: no
        // boxes there, the whole stretch a gap no row can call clear.)
        let Some(cuts) = d
            .families
            .iter()
            .map(|fam| fam.members(lo, hi))
            .collect::<Option<Vec<Vec<Interval>>>>()
        else {
            gaps.push(Claim::Gap {
                x: XBox::new(lo, hi),
            });
            continue;
        };
        let mut cuts: Vec<Interval> = cuts.into_iter().flatten().collect();
        cuts.sort_by(|a, b| a.lo().total_cmp(&b.lo()));
        let (mut start, mut start_b, mut start_c) = (lo, lo_b, lo_c);
        for c in cuts {
            if c.is_point() {
                gaps.push(Claim::Undefined {
                    x: XBox::point(c.lo()),
                });
            }
            // The member's enclosure and the reals out to the doubles the
            // boxes on either side stop at.
            gaps.push(Claim::Gap {
                x: XBox::new(c.lo().next_down(), c.hi().next_up()),
            });
            let at = Bound::At {
                x: Enc::new(c.lo(), c.hi()),
                closed: false,
            };
            let end = c.lo().next_down();
            if start <= end {
                boxes.push(IBox {
                    a: start,
                    b: end,
                    lo: start_b,
                    hi: at,
                    lo_clipped: start_c,
                    hi_clipped: false,
                });
            }
            let next = c.hi().next_up();
            if next > start {
                start = next;
                start_b = at;
                start_c = false;
            }
        }
        if start <= hi {
            boxes.push(IBox {
                a: start,
                b: hi,
                lo: start_b,
                hi: hi_b,
                lo_clipped: start_c,
                hi_clipped: hi_c,
            });
        }
    }
    (boxes, gaps, whole)
}

/// Halvings of `[-w, w]` [`defined_boxes`] may evaluate f's tree on.
const DEFINED_EVALS: usize = 4096;

/// When the domain isn't decided, the boxes the other rows run on: where
/// f's own tree is shown defined (decoration at least *def*) within
/// `[-w, w]`, found by halving until each piece is defined, undefined, or
/// too narrow to halve. f's simplified form may be defined where f isn't;
/// on these boxes it equals f. Each comes one double in from the box
/// claimed Defined, so f equals the form on a neighbourhood of it (and
/// f′, f″ the form's). What they leave out is undecided: rows on them list
/// what they find, never all of it. Returns the boxes and the Defined
/// claims.
pub fn defined_boxes(f: &Fun<'_>, w: f64) -> (Vec<IBox>, Vec<Claim>) {
    let mut found: Vec<(f64, f64)> = Vec::new();
    let mut level = vec![(-w, w)];
    let mut evals = 0;
    'halving: while !level.is_empty() {
        let mut next = Vec::new();
        for (a, b) in level {
            if evals == DEFINED_EVALS {
                break 'halving;
            }
            evals += 1;
            let Ok(s) = f.ser_of(&f.expr, Interval::new(a, b), 0) else {
                break 'halving;
            };
            let v = s[0];
            if v.is_empty() {
                continue;
            }
            if v.dec >= crate::interval::Dec::Def {
                found.push((a, b));
                continue;
            }
            let m = a / 2.0 + b / 2.0;
            if b - a > 1e-10 * a.abs().max(b.abs()).max(1.0) && a < m && m < b {
                next.push((a, m));
                next.push((m, b));
            }
        }
        level = next;
    }
    found.sort_by(|p, q| p.0.total_cmp(&q.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (a, b) in found {
        match merged.last_mut() {
            Some(m) if m.1 >= a => m.1 = m.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    let at = |x: f64| Bound::At {
        x: Enc::point(x),
        closed: true,
    };
    let mut boxes = Vec::new();
    let mut claims = Vec::new();
    for (a, b) in merged {
        let (ia, ib) = (a.next_up(), b.next_down());
        if ia > ib {
            continue;
        }
        boxes.push(IBox {
            a: ia,
            b: ib,
            lo: at(ia),
            hi: at(ib),
            lo_clipped: true,
            hi_clipped: true,
        });
        claims.push(Claim::Defined { x: XBox::new(a, b) });
    }
    (boxes, claims)
}

/// True if `x` (a double) is in the domain's pieces, as far as the bounds
/// tell (an enclosed bound counts as excluding its enclosure).
pub fn in_pieces(pieces: &[Piece], x: f64) -> bool {
    pieces.iter().any(|p| {
        let (lo, hi) = piece_box(p);
        lo <= x && x <= hi
    })
}

/// A family's x-enclosures as a value.
pub fn family_value(fam: &XFamily) -> Family {
    Family {
        x0: Enc::new(fam.x0.lo(), fam.x0.hi()),
        period: Enc::new(fam.period.lo(), fam.period.hi()),
    }
}
