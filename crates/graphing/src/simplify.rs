//! Smart constructors that keep generated expression trees small (constant
//! folding and identity elimination). Used by the equation solver and the
//! symbolic differentiator.

use crate::ast::{BinOp, Expr, Func};

pub(crate) fn num(v: f64) -> Expr {
    if v < 0.0 {
        Expr::Neg(Box::new(Expr::Num(-v)))
    } else {
        Expr::Num(v)
    }
}

pub(crate) fn as_num(e: &Expr) -> Option<f64> {
    match e {
        Expr::Num(v) => Some(*v),
        Expr::Neg(a) => as_num(a).map(|v| -v),
        _ => None,
    }
}

pub(crate) fn is_num(e: &Expr, v: f64) -> bool {
    as_num(e) == Some(v)
}

pub(crate) fn neg(a: Expr) -> Expr {
    match a {
        Expr::Num(v) if v == 0.0 => Expr::Num(0.0),
        Expr::Neg(inner) => *inner,
        a => Expr::Neg(Box::new(a)),
    }
}

pub(crate) fn add(a: Expr, b: Expr) -> Expr {
    match (as_num(&a), as_num(&b)) {
        (Some(x), Some(y)) => num(x + y),
        (Some(x), _) if x == 0.0 => b,
        (_, Some(y)) if y == 0.0 => a,
        (_, Some(y)) if y < 0.0 => Expr::bin(BinOp::Sub, a, Expr::Num(-y)),
        _ => match b {
            Expr::Neg(nb) => Expr::bin(BinOp::Sub, a, *nb),
            b => Expr::bin(BinOp::Add, a, b),
        },
    }
}

pub(crate) fn sub(a: Expr, b: Expr) -> Expr {
    match (as_num(&a), as_num(&b)) {
        (Some(x), Some(y)) => num(x - y),
        (Some(x), _) if x == 0.0 => neg(b),
        (_, Some(y)) if y == 0.0 => a,
        _ => match b {
            Expr::Neg(nb) => Expr::bin(BinOp::Add, a, *nb),
            b => Expr::bin(BinOp::Sub, a, b),
        },
    }
}

pub(crate) fn mul(a: Expr, b: Expr) -> Expr {
    match (as_num(&a), as_num(&b)) {
        (Some(x), Some(y)) => num(x * y),
        (Some(x), _) if x == 0.0 => Expr::Num(0.0),
        (_, Some(y)) if y == 0.0 => Expr::Num(0.0),
        (Some(x), _) if x == 1.0 => b,
        (_, Some(y)) if y == 1.0 => a,
        (Some(x), _) if x == -1.0 => neg(b),
        (_, Some(y)) if y == -1.0 => neg(a),
        // Keep numbers in front.
        (None, Some(_)) => mul(b, a),
        (Some(x), None) => match b {
            // c·(d·e) → (c·d)·e
            Expr::Bin(BinOp::Mul, l, r) if as_num(&l).is_some() => {
                mul(num(x * as_num(&l).unwrap_or(1.0)), *r)
            }
            b => Expr::bin(BinOp::Mul, a, b),
        },
        _ => Expr::bin(BinOp::Mul, a, b),
    }
}

pub(crate) fn div(a: Expr, b: Expr) -> Expr {
    match (as_num(&a), as_num(&b)) {
        (Some(x), _) if x == 0.0 => Expr::Num(0.0),
        (_, Some(y)) if y == 1.0 => a,
        (_, Some(y)) if y == -1.0 => neg(a),
        (Some(x), Some(y)) if y != 0.0 && (x / y) == (x / y).trunc() => num(x / y),
        _ => Expr::bin(BinOp::Div, a, b),
    }
}

/// `a^k` for an integer or rational exponent written as `p/q`, keeping the
/// syntactic-rational form the compiler recognises.
pub(crate) fn pow_rat(a: Expr, p: i64, q: i64) -> Expr {
    if q == 1 {
        if p == 0 {
            return Expr::Num(1.0);
        }
        if p == 1 {
            return a;
        }
        return Expr::bin(BinOp::Pow, a, num(p as f64));
    }
    let e = Expr::bin(
        BinOp::Div,
        Expr::Num(p.unsigned_abs() as f64),
        Expr::Num(q as f64),
    );
    let e = if p < 0 { Expr::Neg(Box::new(e)) } else { e };
    Expr::bin(BinOp::Pow, a, e)
}

pub(crate) fn pow(a: Expr, b: Expr) -> Expr {
    if is_num(&b, 1.0) {
        return a;
    }
    if is_num(&b, 0.0) {
        return Expr::Num(1.0);
    }
    Expr::bin(BinOp::Pow, a, b)
}

pub(crate) fn call(f: Func, a: Expr) -> Expr {
    Expr::Call(f, vec![a])
}

/// If `e` is linear in `y` (`e = c·y + r` with `c`, `r` free of `y`),
/// returns `(c, r)`. `is_var` selects the variable (x or y).
pub(crate) fn linear_in(e: &Expr, is_var: &dyn Fn(&Expr) -> bool) -> Option<(Expr, Expr)> {
    if !e.any(is_var) {
        return Some((Expr::Num(0.0), e.clone()));
    }
    if is_var(e) {
        return Some((Expr::Num(1.0), Expr::Num(0.0)));
    }
    match e {
        Expr::Neg(a) => {
            let (c, r) = linear_in(a, is_var)?;
            Some((neg(c), neg(r)))
        }
        Expr::Bin(BinOp::Add, a, b) => {
            let (ca, ra) = linear_in(a, is_var)?;
            let (cb, rb) = linear_in(b, is_var)?;
            Some((add(ca, cb), add(ra, rb)))
        }
        Expr::Bin(BinOp::Sub, a, b) => {
            let (ca, ra) = linear_in(a, is_var)?;
            let (cb, rb) = linear_in(b, is_var)?;
            Some((sub(ca, cb), sub(ra, rb)))
        }
        Expr::Bin(BinOp::Mul, a, b) => {
            if !a.any(is_var) {
                let (cb, rb) = linear_in(b, is_var)?;
                Some((mul((**a).clone(), cb), mul((**a).clone(), rb)))
            } else if !b.any(is_var) {
                let (ca, ra) = linear_in(a, is_var)?;
                Some((mul(ca, (**b).clone()), mul(ra, (**b).clone())))
            } else {
                None
            }
        }
        Expr::Bin(BinOp::Div, a, b) if !b.any(is_var) => {
            let (ca, ra) = linear_in(a, is_var)?;
            Some((div(ca, (**b).clone()), div(ra, (**b).clone())))
        }
        _ => None,
    }
}
