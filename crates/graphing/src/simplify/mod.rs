//! Simplification.
//!
//! Two layers live here:
//!
//! * At the top, smart constructors that keep generated expression trees
//!   small (constant folding and identity elimination), used by the
//!   equation solver and the symbolic differentiator.
//! * The submodules: the certified engine's simplifier. [`simplify`]
//!   rewrites a function of x by equality saturation ([`egg`]) with a
//!   vetted rule set ([`rules`]: each rule states where it holds and is
//!   checked against MPFR in `tests/simplify_rules.rs`), keeping the
//!   input's domain conditions ([`side`]) apart so that no hole is lost;
//!   [`prove_parity`] and [`prove_period`] give proofs (never disproofs);
//!   [`rational`] puts rational functions in lowest terms with exact
//!   asymptotes; [`limit`] finds limits at ±∞ by dominant terms.
//!
//! Domain of a simplified function = where the simplified form is defined
//! ∩ where all of the original's [`side_conditions`] hold.

pub mod analysis;
pub mod egraph;
pub mod lang;
pub mod limit;
pub mod period;
pub mod q;
pub mod rational;
pub mod rules;
pub mod side;

pub use egraph::{Limits, Parity, Settings, Simplified, Stop, prove_parity, simplify};
pub use lang::{ExactLiterals, Unsupported};
pub use limit::Limit;
pub use period::{Period, PiQ};
pub use q::Q;
pub use rational::{Dir, RationalForm, rational_form};
pub use side::{Cond, Jump, side_conditions};

/// Proves a period of `e` in `settings`' angle unit (see [`period`]).
pub fn prove_period(e: &Expr, settings: &Settings<'_>) -> Option<Period> {
    period::prove_period(e, settings.unit, settings.literals)
}

/// The limit of `e` at ±∞ (see [`limit`]).
pub fn limit_at(e: &Expr, dir: Dir, settings: &Settings<'_>) -> Limit {
    limit::limit(e, dir, settings.unit, settings.literals, settings.variables)
}

use crate::ast::{BinOp, Expr, Func, Lit};

// The smart constructors fold numbers only where that is exact: each
// number they read is exactly its double (`Lit::Exact`), and each sum,
// product or quotient they write is the exact one. A typed decimal (0.1,
// 1.0000000000000001) or a number not known exactly is kept as written, so
// that the derivative of 1.0000000000000001·x − 1·x is not 1 − 1 = 0
// (review 13, R13-M-01).

/// The double `v` as a number, exactly (−v under a sign when negative).
/// Only for a value known to be exactly `v`: an integer written, or a
/// result these constructors computed exactly.
pub(crate) fn num(v: f64) -> Expr {
    if v < 0.0 {
        Expr::Neg(Box::new(Expr::exact(-v)))
    } else {
        Expr::exact(v)
    }
}

/// The value of a number that is exactly its double (or of its negation).
pub(crate) fn as_num(e: &Expr) -> Option<f64> {
    match e {
        Expr::Num(v, Lit::Exact) => Some(*v),
        Expr::Neg(a) => as_num(a).map(|v| -v),
        _ => None,
    }
}

pub(crate) fn is_num(e: &Expr, v: f64) -> bool {
    as_num(e) == Some(v)
}

/// x + y when the double sum is exact (its rounding error, by Knuth's
/// two-sum, is 0).
fn exact_add(x: f64, y: f64) -> Option<f64> {
    let s = x + y;
    let b = s - x;
    let err = (x - (s - b)) + (y - b);
    (s.is_finite() && err == 0.0).then_some(s)
}

/// x·y when the double product is exact (its fma residual is 0, and it is
/// no subnormal, whose residual could itself be lost).
fn exact_mul(x: f64, y: f64) -> Option<f64> {
    let p = x * y;
    let normal = p == 0.0 && (x == 0.0 || y == 0.0) || p.is_normal();
    (normal && x.mul_add(y, -p) == 0.0).then_some(p)
}

/// x/y when the double quotient is exact.
fn exact_div(x: f64, y: f64) -> Option<f64> {
    if y == 0.0 {
        return None;
    }
    let q = x / y;
    let normal = q == 0.0 && x == 0.0 || q.is_normal();
    (normal && q.mul_add(y, -x) == 0.0).then_some(q)
}

pub(crate) fn neg(a: Expr) -> Expr {
    match a {
        Expr::Num(v, Lit::Exact) if v == 0.0 => num(0.0),
        Expr::Neg(inner) => *inner,
        a => Expr::Neg(Box::new(a)),
    }
}

pub(crate) fn add(a: Expr, b: Expr) -> Expr {
    let (x, y) = (as_num(&a), as_num(&b));
    if let (Some(x), Some(y)) = (x, y)
        && let Some(s) = exact_add(x, y)
    {
        return num(s);
    }
    match (x, y) {
        (Some(x), _) if x == 0.0 => b,
        (_, Some(y)) if y == 0.0 => a,
        (_, Some(y)) if y < 0.0 => Expr::bin(BinOp::Sub, a, num(-y)),
        _ => match b {
            Expr::Neg(nb) => Expr::bin(BinOp::Sub, a, *nb),
            b => Expr::bin(BinOp::Add, a, b),
        },
    }
}

pub(crate) fn sub(a: Expr, b: Expr) -> Expr {
    let (x, y) = (as_num(&a), as_num(&b));
    if let (Some(x), Some(y)) = (x, y)
        && let Some(s) = exact_add(x, -y)
    {
        return num(s);
    }
    match (x, y) {
        (Some(x), _) if x == 0.0 => neg(b),
        (_, Some(y)) if y == 0.0 => a,
        _ => match b {
            Expr::Neg(nb) => Expr::bin(BinOp::Add, a, *nb),
            b => Expr::bin(BinOp::Sub, a, b),
        },
    }
}

pub(crate) fn mul(a: Expr, b: Expr) -> Expr {
    let (x, y) = (as_num(&a), as_num(&b));
    if let (Some(x), Some(y)) = (x, y)
        && let Some(p) = exact_mul(x, y)
    {
        return num(p);
    }
    match (x, y) {
        (Some(x), _) if x == 0.0 => num(0.0),
        (_, Some(y)) if y == 0.0 => num(0.0),
        (Some(x), _) if x == 1.0 => b,
        (_, Some(y)) if y == 1.0 => a,
        (Some(x), _) if x == -1.0 => neg(b),
        (_, Some(y)) if y == -1.0 => neg(a),
        // Keep numbers in front.
        (None, Some(_)) => mul(b, a),
        (Some(x), None) => match b {
            // c·(d·e) → (c·d)·e, when c·d is exact.
            Expr::Bin(BinOp::Mul, l, r) if as_num(&l).and_then(|d| exact_mul(x, d)).is_some() => {
                let d = as_num(&l).unwrap_or(1.0);
                mul(num(x * d), *r)
            }
            b => Expr::bin(BinOp::Mul, a, b),
        },
        _ => Expr::bin(BinOp::Mul, a, b),
    }
}

pub(crate) fn div(a: Expr, b: Expr) -> Expr {
    let (x, y) = (as_num(&a), as_num(&b));
    if let (Some(x), Some(y)) = (x, y)
        && let Some(q) = exact_div(x, y)
    {
        return num(q);
    }
    match (x, y) {
        (Some(x), _) if x == 0.0 => num(0.0),
        (_, Some(y)) if y == 1.0 => a,
        (_, Some(y)) if y == -1.0 => neg(a),
        _ => Expr::bin(BinOp::Div, a, b),
    }
}

/// `a^k` for an integer or rational exponent written as `p/q`, keeping the
/// syntactic-rational form the compiler recognises.
pub(crate) fn pow_rat(a: Expr, p: i64, q: i64) -> Expr {
    if q == 1 {
        if p == 0 {
            return num(1.0);
        }
        if p == 1 {
            return a;
        }
        return Expr::bin(BinOp::Pow, a, num(p as f64));
    }
    let e = Expr::bin(
        BinOp::Div,
        Expr::num(p.unsigned_abs() as f64),
        Expr::num(q as f64),
    );
    let e = if p < 0 { Expr::Neg(Box::new(e)) } else { e };
    Expr::bin(BinOp::Pow, a, e)
}

pub(crate) fn pow(a: Expr, b: Expr) -> Expr {
    if is_num(&b, 1.0) {
        return a;
    }
    if is_num(&b, 0.0) {
        return num(1.0);
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
        return Some((num(0.0), e.clone()));
    }
    if is_var(e) {
        return Some((num(1.0), num(0.0)));
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
