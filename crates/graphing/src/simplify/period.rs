//! Periods, proven from the expression's structure.
//!
//! If every occurrence of x lies inside a sin, cos, tan, sec, csc or cot
//! whose argument is a·x + b, with a known exactly, then shifting x by a
//! common multiple P of those functions' periods (a full turn / |a|; a
//! half turn / |a| for tan and cot) leaves every such application, and so
//! the whole expression *and every one of its domain conditions*, as it
//! was: f(x + P) and f(x) are the same expression of the same values.
//! That is a proof that P is *a* period; whether it is the least one is
//! the certifier's to show (interval witnesses against P/k).
//!
//! Exact coefficients are rationals, or rationals times a power of π
//! (`sin(π·x)` has period 2, `sin(2x)` has π). Periods of different powers
//! of π, or an inexact coefficient (`sin(√2·x)`), give no period.

use super::lang::ExactLiterals;
use super::q::Q;
use crate::ast::{BinOp, Constant, Expr, Func};
use crate::compile::syntactic_rational;
use crate::functions::TrigUnit;

/// q·πᵏ, exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PiQ {
    /// The rational factor.
    pub q: Q,
    /// The power of π.
    pub k: i32,
}

impl PiQ {
    fn mul(self, o: PiQ) -> Option<PiQ> {
        Some(PiQ {
            q: self.q.mul(o.q)?,
            k: self.k.checked_add(o.k)?,
        })
    }

    fn div(self, o: PiQ) -> Option<PiQ> {
        Some(PiQ {
            q: self.q.div(o.q)?,
            k: self.k.checked_sub(o.k)?,
        })
    }

    fn add(self, o: PiQ) -> Option<PiQ> {
        if self.q.is_zero() {
            return Some(o);
        }
        if o.q.is_zero() {
            return Some(self);
        }
        (self.k == o.k).then_some(())?;
        Some(PiQ {
            q: self.q.add(o.q)?,
            k: self.k,
        })
    }

    /// The value as a double.
    pub fn to_f64(self) -> f64 {
        self.q.to_f64() * std::f64::consts::PI.powi(self.k)
    }
}

/// A proven period of a function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Period {
    /// The period, exactly, in x units (for degree and grad modes, x is
    /// the angle in that unit: `sin x` has period 360 there).
    pub value: PiQ,
}

impl Period {
    /// The period as a double.
    pub fn to_f64(self) -> f64 {
        self.value.to_f64()
    }
}

/// The exact value of an x-free expression, if it is q·πᵏ.
pub fn exact_constant(e: &Expr, lits: &ExactLiterals) -> Option<PiQ> {
    Some(match e {
        Expr::Num(v) => PiQ {
            q: lits.exact(*v)?,
            k: 0,
        },
        Expr::Const(Constant::Pi) => PiQ { q: Q::ONE, k: 1 },
        Expr::Neg(a) => {
            let a = exact_constant(a, lits)?;
            PiQ {
                q: a.q.neg()?,
                k: a.k,
            }
        }
        Expr::Degrees(a) => exact_constant(a, lits)?,
        Expr::Bin(op, a, b) => {
            let (x, y) = (exact_constant(a, lits)?, exact_constant(b, lits)?);
            match op {
                BinOp::Add => x.add(y)?,
                BinOp::Sub => x.add(PiQ {
                    q: y.q.neg()?,
                    k: y.k,
                })?,
                BinOp::Mul => x.mul(y)?,
                BinOp::Div => x.div(y)?,
                BinOp::Pow => {
                    let (p, q) = syntactic_rational(b)?;
                    if q != 1 || p.unsigned_abs() > 64 {
                        return None;
                    }
                    PiQ {
                        q: x.q.powi(p as i64)?,
                        k: x.k.checked_mul(p)?,
                    }
                }
            }
        }
        _ => return None,
    })
}

/// `e` as a·x + b with a exact (`None` if it isn't, or a = 0).
pub fn affine(e: &Expr, lits: &ExactLiterals) -> Option<PiQ> {
    fn coef(e: &Expr, lits: &ExactLiterals) -> Option<PiQ> {
        // The coefficient of x, for an expression known to be affine.
        if !e.contains_x() {
            return Some(PiQ { q: Q::ZERO, k: 0 });
        }
        match e {
            Expr::X => Some(PiQ { q: Q::ONE, k: 0 }),
            Expr::Neg(a) => {
                let c = coef(a, lits)?;
                Some(PiQ {
                    q: c.q.neg()?,
                    k: c.k,
                })
            }
            Expr::Degrees(a) => coef(a, lits),
            Expr::Bin(BinOp::Add, a, b) => coef(a, lits)?.add(coef(b, lits)?),
            Expr::Bin(BinOp::Sub, a, b) => {
                let c = coef(b, lits)?;
                coef(a, lits)?.add(PiQ {
                    q: c.q.neg()?,
                    k: c.k,
                })
            }
            Expr::Bin(BinOp::Mul, a, b) => match (a.contains_x(), b.contains_x()) {
                (true, false) => coef(a, lits)?.mul(exact_constant(b, lits)?),
                (false, true) => exact_constant(a, lits)?.mul(coef(b, lits)?),
                _ => None,
            },
            Expr::Bin(BinOp::Div, a, b) if !b.contains_x() => {
                coef(a, lits)?.div(exact_constant(b, lits)?)
            }
            _ => None,
        }
    }
    let c = coef(e, lits)?;
    (!c.q.is_zero()).then_some(c)
}

/// Proves a period of `e`: a common multiple of the periods of the trig
/// functions holding every x. `None`: not proven (`e` may still be
/// periodic).
pub fn prove_period(e: &Expr, unit: TrigUnit, lits: &ExactLiterals) -> Option<Period> {
    if !e.contains_x() {
        // A constant: no period (the panel's convention).
        return None;
    }
    // Full and half turns as q·πᵏ.
    let (full, half) = match unit {
        TrigUnit::Radians => (PiQ { q: Q::int(2), k: 1 }, PiQ { q: Q::ONE, k: 1 }),
        TrigUnit::Degrees => (
            PiQ {
                q: Q::int(360),
                k: 0,
            },
            PiQ {
                q: Q::int(180),
                k: 0,
            },
        ),
        TrigUnit::Grads => (
            PiQ {
                q: Q::int(400),
                k: 0,
            },
            PiQ {
                q: Q::int(200),
                k: 0,
            },
        ),
    };
    let mut periods: Vec<PiQ> = Vec::new();
    // Every x must be inside a periodic application with an affine
    // argument.
    fn walk(
        e: &Expr,
        lits: &ExactLiterals,
        full: PiQ,
        half: PiQ,
        out: &mut Vec<PiQ>,
    ) -> Option<()> {
        if !e.contains_x() {
            return Some(());
        }
        match e {
            Expr::X => None,
            Expr::Call(
                f @ (Func::Sin | Func::Cos | Func::Sec | Func::Csc | Func::Tan | Func::Cot),
                args,
            ) => {
                let a = affine(&args[0], lits)?;
                let turn = if matches!(f, Func::Tan | Func::Cot) {
                    half
                } else {
                    full
                };
                let abs = PiQ {
                    q: a.q.abs()?,
                    k: a.k,
                };
                out.push(turn.div(abs)?);
                Some(())
            }
            Expr::Neg(a) | Expr::Degrees(a) => walk(a, lits, full, half, out),
            Expr::Bin(_, a, b) => {
                walk(a, lits, full, half, out)?;
                walk(b, lits, full, half, out)
            }
            Expr::Call(_, args) => {
                for a in args {
                    walk(a, lits, full, half, out)?;
                }
                Some(())
            }
            _ => Some(()),
        }
    }
    walk(e, lits, full, half, &mut periods)?;
    let first = *periods.first()?;
    let mut p = first;
    for t in &periods[1..] {
        if t.k != p.k {
            return None;
        }
        p = PiQ {
            q: p.q.lcm(t.q)?,
            k: p.k,
        };
    }
    Some(Period { value: p })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equation::Equation;

    fn period(s: &str, unit: TrigUnit) -> Option<PiQ> {
        let text = format!("y={s}");
        let eq = Equation::parse(&text).unwrap();
        let lits = ExactLiterals::of(&text, Default::default()).unwrap();
        prove_period(eq.explicit().unwrap().1, unit, &lits).map(|p| p.value)
    }

    fn pi(n: i128, d: i128) -> Option<PiQ> {
        Some(PiQ {
            q: Q::new(n, d).unwrap(),
            k: 1,
        })
    }

    #[test]
    fn periods_from_structure() {
        use TrigUnit::*;
        assert_eq!(period("sin(x)", Radians), pi(2, 1));
        assert_eq!(period("tan(x)", Radians), pi(1, 1));
        assert_eq!(period("sin(2x)+cos(3x)", Radians), pi(2, 1));
        assert_eq!(period("sin(x/2)^2+tan(x)", Radians), pi(4, 1));
        assert_eq!(period("sin(x)+sin(sqrt(2)*x)", Radians), None);
        assert_eq!(period("sin(x)+x", Radians), None);
        assert_eq!(period("x*0+sin(x)", Radians), None, "an x outside");
        assert_eq!(
            period("sin(pi*x)", Radians),
            Some(PiQ { q: Q::int(2), k: 0 })
        );
        assert_eq!(
            period("sin(x)", Degrees),
            Some(PiQ {
                q: Q::int(360),
                k: 0
            })
        );
        assert_eq!(
            period("tan(3x+1)", Grads),
            Some(PiQ {
                q: Q::new(200, 3).unwrap(),
                k: 0
            })
        );
        assert_eq!(period("2", Radians), None, "constants: not periodic");
        assert_eq!(period("sin(x)+sin(pi*x)", Radians), None);
    }
}
