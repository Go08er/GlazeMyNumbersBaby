//! What f does next to a point it excludes: blows up (a vertical
//! asymptote) or stays bounded.
//!
//! *Bounded* is read off one enclosure: f over a neighbourhood N of the
//! point (the point included) encloses f's values on the part of N where
//! it is defined, so a bounded enclosure bounds f there.
//!
//! *Unbounded* follows the tree: a quotient whose divisor has a simple zero
//! in N (it crosses 0 once, strictly monotone) while its numerator stays
//! away from 0, tan and sec at a simple zero of their cosine, a logarithm
//! whose argument falls to 0, and sums, products, powers of such a term
//! with factors that stay bounded (and away from 0) on N.

use super::cover::side;
use super::fun::{Fun, Stop, usable};
use crate::ast::{BinOp, Expr, Func};
use crate::compile::syntactic_rational;
use crate::interval::{Dec, Interval};

/// How f behaves next to an excluded point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Near {
    /// |f| → ∞ from the side(s) looked at.
    Pole,
    /// f stays bounded.
    Bounded,
    /// Not decided.
    Unknown,
}

/// e is continuous on N and its enclosure there is bounded.
fn bounded_cont(f: &Fun<'_>, e: &Expr, n: Interval) -> Result<bool, Stop> {
    let s = f.ser_of(e, n, 0)?;
    Ok(s[0].dec >= Dec::Dac && s[0].iv.is_bounded() && !s[0].is_empty())
}

/// e is continuous on N and never 0 there.
fn nonzero_cont(f: &Fun<'_>, e: &Expr, n: Interval) -> Result<bool, Stop> {
    let s = f.ser_of(e, n, 0)?;
    Ok(s[0].dec >= Dec::Dac && s[0].ne0())
}

/// g has exactly one zero in N: continuous and strictly monotone there,
/// strictly opposite signs at N's ends.
fn simple_zero(f: &Fun<'_>, g: &Expr, n: Interval) -> Result<bool, Stop> {
    if !(n.lo().is_finite() && n.hi().is_finite()) {
        return Ok(false);
    }
    let s = f.ser_of(g, n, 1)?;
    if !(s[0].dec >= Dec::Dac && usable(&s, 1) && s[1].ne0()) {
        return Ok(false);
    }
    let (a, b) = (
        f.ser_of(g, Interval::point(n.lo()), 0)?[0],
        f.ser_of(g, Interval::point(n.hi()), 0)?[0],
    );
    Ok(matches!((side(&a, 0.0), side(&b, 0.0)), (Some(x), Some(y)) if x != y))
}

/// g is 0 at exactly one point of N, continuous there: g has a simple
/// zero, or g = cⁿ (n a positive integer) with c having one.
fn vanishes(f: &Fun<'_>, g: &Expr, n: Interval) -> Result<bool, Stop> {
    if simple_zero(f, g, n)? {
        return Ok(true);
    }
    Ok(match g {
        Expr::Bin(BinOp::Pow, c, p) => {
            matches!(syntactic_rational(p), Some((p, 1)) if p > 0) && simple_zero(f, c, n)?
        }
        Expr::Neg(c) => vanishes(f, c, n)?,
        _ => false,
    })
}

/// |e| → ∞ at the one excluded point in N (approached from both sides).
pub fn pole(f: &Fun<'_>, e: &Expr, n: Interval) -> Result<bool, Stop> {
    Ok(match e {
        Expr::Neg(a) => pole(f, a, n)?,
        Expr::Bin(BinOp::Add | BinOp::Sub, a, b) => {
            (pole(f, a, n)? && bounded_cont(f, b, n)?) || (pole(f, b, n)? && bounded_cont(f, a, n)?)
        }
        Expr::Bin(BinOp::Mul, a, b) => {
            (pole(f, a, n)? && nonzero_cont(f, b, n)?) || (pole(f, b, n)? && nonzero_cont(f, a, n)?)
        }
        Expr::Bin(BinOp::Div, a, b) => {
            (vanishes(f, b, n)? && nonzero_cont(f, a, n)?) || (pole(f, a, n)? && nonzero_cont(f, b, n)?)
        }
        Expr::Bin(BinOp::Pow, a, b) => match syntactic_rational(b) {
            Some((p, 1)) if p > 0 => pole(f, a, n)?,
            Some((p, 1)) if p < 0 => simple_zero(f, a, n)?,
            _ => false,
        },
        Expr::Call(func, args) => {
            let u = &args[0];
            match func {
                Func::Tan | Func::Sec => {
                    simple_zero(f, &Expr::call1(Func::Cos, u.clone()), n)?
                        && (*func == Func::Sec
                            || nonzero_cont(f, &Expr::call1(Func::Sin, u.clone()), n)?)
                }
                Func::Cot | Func::Csc => {
                    simple_zero(f, &Expr::call1(Func::Sin, u.clone()), n)?
                        && (*func == Func::Csc
                            || nonzero_cont(f, &Expr::call1(Func::Cos, u.clone()), n)?)
                }
                Func::Csch | Func::Coth => simple_zero(f, u, n)?,
                Func::Abs => pole(f, u, n)?,
                _ => false,
            }
        }
        _ => false,
    })
}

/// [`pole`], and e ≠ 0 on the rest of N: a quotient whose numerator stays
/// away from 0, tan/cot whose sine/cosine does, sec, csc, csch, coth, and
/// products and negations of such.
pub fn pole_free(f: &Fun<'_>, e: &Expr, n: Interval) -> Result<bool, Stop> {
    Ok(match e {
        Expr::Neg(a) => pole_free(f, a, n)?,
        Expr::Bin(BinOp::Mul, a, b) => {
            (pole_free(f, a, n)? && nonzero_cont(f, b, n)?) || (pole_free(f, b, n)? && nonzero_cont(f, a, n)?)
        }
        Expr::Bin(BinOp::Div, a, b) => {
            (vanishes(f, b, n)? && nonzero_cont(f, a, n)?)
                || (pole_free(f, a, n)? && nonzero_cont(f, b, n)?)
        }
        Expr::Bin(BinOp::Pow, a, b) => match syntactic_rational(b) {
            Some((p, 1)) if p < 0 => simple_zero(f, a, n)?,
            Some((p, 1)) if p > 0 => pole_free(f, a, n)?,
            _ => false,
        },
        Expr::Call(Func::Tan | Func::Cot | Func::Sec | Func::Csc | Func::Csch | Func::Coth, _) => {
            pole(f, e, n)?
        }
        _ => false,
    })
}

/// |e| → ∞ as x approaches the domain end p from inside, the side `n`
/// lies on (`n` has p as one end): the logarithm of an argument that falls
/// to 0, and the same compositions as [`pole`].
pub fn end_pole(f: &Fun<'_>, e: &Expr, n: Interval, p_end: f64) -> Result<bool, Stop> {
    let to_zero = |g: &Expr| -> Result<bool, Stop> {
        // g continuous on the closed side box, exactly 0 at the end, and
        // strictly positive inside.
        let s = f.ser_of(g, n, 0)?;
        if s[0].dec < Dec::Dac {
            return Ok(false);
        }
        let at = f.ser_of(g, Interval::point(p_end), 0)?[0];
        if !(at.lo() == 0.0 && at.hi() == 0.0) {
            return Ok(false);
        }
        let inside = if p_end == n.lo() {
            Interval::new(n.lo().next_up(), n.hi())
        } else {
            Interval::new(n.lo(), n.hi().next_down())
        };
        Ok(f.ser_of(g, inside, 0)?[0].gt0())
    };
    Ok(match e {
        Expr::Neg(a) => end_pole(f, a, n, p_end)?,
        Expr::Bin(BinOp::Add | BinOp::Sub, a, b) => {
            (end_pole(f, a, n, p_end)? && bounded_cont(f, b, n)?)
                || (end_pole(f, b, n, p_end)? && bounded_cont(f, a, n)?)
        }
        Expr::Bin(BinOp::Mul, a, b) => {
            (end_pole(f, a, n, p_end)? && nonzero_cont(f, b, n)?)
                || (end_pole(f, b, n, p_end)? && nonzero_cont(f, a, n)?)
        }
        Expr::Call(Func::Ln | Func::Log, args) => to_zero(&args[0])?,
        _ => false,
    })
}

/// f's enclosure over N is bounded: f is bounded near the point.
pub fn bounded_near(f: &Fun<'_>, n: Interval) -> Result<bool, Stop> {
    let v = f.val(n)?;
    Ok(!v.is_empty() && v.iv.is_bounded())
}
