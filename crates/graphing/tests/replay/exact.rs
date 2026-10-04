//! Exact rational values of the parts of a tree that have one: the typed
//! decimals, integer powers, the four operations, |·|, the steps and a
//! square root of a perfect square. Used where an interval can't show an
//! equality (0.1·x − 1 at 10) or a point exactly (90/0.1).

use super::eval::{Lits, written_rational};
use graphing::ast::{BinOp, Expr, Func};
use rug::{Integer, Rational};

/// A typed decimal (digits and at most one '.') exactly.
pub fn decimal(text: &str) -> Option<Rational> {
    let (int, frac) = match text.split_once('.') {
        Some((i, f)) => (i, f),
        None => (text, ""),
    };
    let digits = format!("{int}{frac}");
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n = Integer::from_str_radix(&digits, 10).ok()?;
    let d = Integer::from(Integer::u_pow_u(10, frac.len() as u32));
    Some(Rational::from((n, d)))
}

pub fn of_f64(v: f64) -> Option<Rational> {
    Rational::from_f64(v)
}

/// A rational's exact square root, if it has one.
fn sqrt_exact(q: &Rational) -> Option<Rational> {
    if *q < 0 {
        return None;
    }
    let (n, d) = (q.numer(), q.denom());
    if !n.is_perfect_square() || !d.is_perfect_square() {
        return None;
    }
    Some(Rational::from((n.clone().sqrt(), d.clone().sqrt())))
}

/// e at x (exactly), where every step is exact; `None` otherwise (or
/// where e is undefined).
pub fn eval(
    e: &Expr,
    x: Option<&Rational>,
    lits: &Lits,
    vars: &[(String, f64)],
) -> Option<Rational> {
    let ev = |e: &Expr| eval(e, x, lits, vars);
    Some(match e {
        Expr::Num(v) => lits.exact(*v)?,
        Expr::X => x?.clone(),
        Expr::Var(n) => of_f64(vars.iter().find(|(m, _)| m == n)?.1)?,
        Expr::Y | Expr::Const(_) => return None,
        Expr::Neg(a) => -ev(a)?,
        Expr::Degrees(a) => ev(a)?,
        Expr::Bin(BinOp::Pow, a, b) => {
            let (p, q) = written_rational(b)?;
            let base = ev(a)?;
            if q == 1 {
                pow_int(&base, p)?
            } else if q == 2 && p == 1 {
                sqrt_exact(&base)?
            } else {
                return None;
            }
        }
        Expr::Bin(op, a, b) => {
            let (a, b) = (ev(a)?, ev(b)?);
            match op {
                BinOp::Add => a + b,
                BinOp::Sub => a - b,
                BinOp::Mul => a * b,
                BinOp::Div => {
                    if b == 0 {
                        return None;
                    }
                    a / b
                }
                BinOp::Pow => unreachable!(),
            }
        }
        Expr::Call(f, args) => {
            let a = ev(&args[0])?;
            match f {
                Func::Abs => a.abs(),
                Func::Sqrt => sqrt_exact(&a)?,
                Func::Floor => Rational::from(a.floor()),
                Func::Ceil => Rational::from(a.ceil()),
                Func::Round => {
                    // Half away from zero.
                    let t = Rational::from((a.clone().abs() + Rational::from((1, 2))).floor());
                    if a < 0 { -t } else { t }
                }
                Func::Sign => Rational::from(a.cmp0() as i32),
                Func::Min | Func::Max => {
                    let mut acc = a;
                    for b in &args[1..] {
                        let b = ev(b)?;
                        let take = if *f == Func::Min { b < acc } else { b > acc };
                        if take {
                            acc = b;
                        }
                    }
                    acc
                }
                Func::Mod => {
                    let b = ev(&args[1])?;
                    if b == 0 {
                        return None;
                    }
                    let k = Rational::from((a.clone() / b.clone()).floor());
                    a - b * k
                }
                _ => return None,
            }
        }
    })
}

pub fn pow_int(base: &Rational, p: i64) -> Option<Rational> {
    if p.unsigned_abs() > 4096 {
        return None;
    }
    if p <= 0 && *base == 0 {
        return None;
    }
    let mut r = Rational::from(1);
    for _ in 0..p.unsigned_abs() {
        r *= base;
    }
    Some(if p < 0 { r.recip() } else { r })
}
