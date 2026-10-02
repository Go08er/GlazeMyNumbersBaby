//! Symbolic differentiation with respect to `x`.
//!
//! Used by the analyzer so that critical points and inflection points are
//! roots of exact derivative programs instead of noisy finite differences.
//! Piecewise-constant functions (floor, ceiling, round, sign) differentiate
//! to 0 almost everywhere; `|u|` to `sign(u)·u'`. Returns `None` for the few
//! functions without an elementary derivative (factorials, nCr, nPr).
//!
//! Derivatives can be much larger than their input (nested compositions
//! repeat inner subtrees; `max(a, b, …)` doubles per argument), so building
//! one is bounded by a node budget; over budget is `None` as well.

use crate::ast::{BinOp, Constant, Expr, Func};
use crate::compile::syntactic_rational;
use crate::functions::TrigUnit;
use crate::simplify::{add, call, div, is_num, mul, neg, num, pow, pow_rat, sub};

/// Node budget of [`derivative`]: generous for anything typed by hand,
/// small enough that a pathological input fails fast instead of exhausting
/// memory.
pub const DEFAULT_MAX_NODES: usize = 1 << 16;

/// d/dx of `e`. Trig derivatives include the chain factor for the angle
/// unit (e.g. d/dx sin x = (π/180)·cos x in degrees mode).
///
/// `None` if a function has no elementary derivative or the result would
/// exceed [`DEFAULT_MAX_NODES`] (see [`derivative_bounded`]).
pub fn derivative(e: &Expr, unit: TrigUnit) -> Option<Expr> {
    derivative_bounded(e, unit, DEFAULT_MAX_NODES)
}

/// Like [`derivative`], giving up (`None`) once building the result has
/// cost about `max_nodes` nodes. The count is an over-estimate, so the
/// returned tree never has more than `max_nodes` nodes.
pub fn derivative_bounded(e: &Expr, unit: TrigUnit, max_nodes: usize) -> Option<Expr> {
    let mut cx = Cx {
        k: unit.to_radians_factor(),
        left: max_nodes,
    };
    d(e, &mut cx)
}

/// Differentiation context: the angle-unit factor and the remaining budget.
struct Cx {
    k: f64,
    left: usize,
}

impl Cx {
    /// Pays for `n` new nodes; `None` once the budget is spent.
    fn charge(&mut self, n: usize) -> Option<()> {
        self.left = self.left.checked_sub(n)?;
        Some(())
    }

    /// Pays for copying `e` (and building around it) `times` times.
    fn charge_copies(&mut self, e: &Expr, times: usize) -> Option<()> {
        self.charge(times.saturating_mul(e.depth_and_size().1))
    }
}

fn free(e: &Expr) -> bool {
    !e.contains_x()
}

fn unit_mul(k: f64, e: Expr) -> Expr {
    if k == 1.0 { e } else { mul(Expr::Num(k), e) }
}

fn sq(e: Expr) -> Expr {
    pow_rat(e, 2, 1)
}

fn d(e: &Expr, cx: &mut Cx) -> Option<Expr> {
    cx.charge(1)?;
    if free(e) {
        return Some(Expr::Num(0.0));
    }
    Some(match e {
        Expr::X => Expr::Num(1.0),
        Expr::Num(_) | Expr::Const(_) | Expr::Y | Expr::Var(_) => Expr::Num(0.0),
        Expr::Neg(a) => neg(d(a, cx)?),
        Expr::Degrees(a) => d(a, cx)?,
        Expr::Bin(op, a, b) => {
            let (a, b) = (&**a, &**b);
            // Each rule copies a, b and e at most twice in total.
            cx.charge_copies(e, 2)?;
            match op {
                BinOp::Add => add(d(a, cx)?, d(b, cx)?),
                BinOp::Sub => sub(d(a, cx)?, d(b, cx)?),
                BinOp::Mul => {
                    if free(a) {
                        mul(a.clone(), d(b, cx)?)
                    } else if free(b) {
                        mul(d(a, cx)?, b.clone())
                    } else {
                        add(mul(d(a, cx)?, b.clone()), mul(a.clone(), d(b, cx)?))
                    }
                }
                BinOp::Div => {
                    if free(b) {
                        div(d(a, cx)?, b.clone())
                    } else if free(a) {
                        // (c/b)' = -c·b'/b²
                        neg(div(mul(a.clone(), d(b, cx)?), sq(b.clone())))
                    } else {
                        div(
                            sub(mul(d(a, cx)?, b.clone()), mul(a.clone(), d(b, cx)?)),
                            sq(b.clone()),
                        )
                    }
                }
                BinOp::Pow => {
                    if free(b) {
                        let da = d(a, cx)?;
                        if let Some((p, q)) = syntactic_rational(b) {
                            // (p/q)·a^((p-q)/q)·a'
                            let coef = if q == 1 {
                                num(p as f64)
                            } else {
                                div(num(p as f64), Expr::Num(q as f64))
                            };
                            let (np, nq) = reduce(p as i64 - q as i64, q as i64);
                            mul(mul(coef, pow_rat(a.clone(), np, nq)), da)
                        } else {
                            // b·a^(b-1)·a'
                            mul(
                                mul(b.clone(), pow(a.clone(), sub(b.clone(), Expr::Num(1.0)))),
                                da,
                            )
                        }
                    } else if free(a) {
                        // a^b·ln(a)·b'
                        let lna = if matches!(a, Expr::Const(Constant::E)) {
                            Expr::Num(1.0)
                        } else {
                            call(Func::Ln, a.clone())
                        };
                        mul(mul(e.clone(), lna), d(b, cx)?)
                    } else {
                        // a^b·(b'·ln a + b·a'/a)
                        let t1 = mul(d(b, cx)?, call(Func::Ln, a.clone()));
                        let t2 = div(mul(b.clone(), d(a, cx)?), a.clone());
                        mul(e.clone(), add(t1, t2))
                    }
                }
            }
        }
        Expr::Call(f, args) => return d_call(*f, args, cx),
    })
}

fn reduce(p: i64, q: i64) -> (i64, i64) {
    fn gcd(a: i64, b: i64) -> i64 {
        if b == 0 { a.abs() } else { gcd(b, a % b) }
    }
    let g = gcd(p, q).max(1);
    (p / g, q / g)
}

fn d_call(f: Func, args: &[Expr], cx: &mut Cx) -> Option<Expr> {
    use Func::*;
    let k = cx.k;
    if args.len() == 1 {
        let u = &args[0];
        let du = d(u, cx)?;
        if is_num(&du, 0.0) {
            return Some(Expr::Num(0.0));
        }
        // Every rule below copies u at most three times.
        cx.charge_copies(u, 3)?;
        let c = |f: Func| call(f, u.clone());
        let one = || Expr::Num(1.0);
        let inner = match f {
            Sin => unit_mul(k, c(Cos)),
            Cos => neg(unit_mul(k, c(Sin))),
            Tan => unit_mul(k, sq(c(Sec))),
            Sec => unit_mul(k, mul(c(Sec), c(Tan))),
            Csc => neg(unit_mul(k, mul(c(Csc), c(Cot)))),
            Cot => neg(unit_mul(k, sq(c(Csc)))),
            Asin => div(one(), unit_mul(k, call(Sqrt, sub(one(), sq(u.clone()))))),
            Acos => neg(div(
                one(),
                unit_mul(k, call(Sqrt, sub(one(), sq(u.clone())))),
            )),
            Atan => div(one(), unit_mul(k, add(one(), sq(u.clone())))),
            Acot => neg(div(one(), unit_mul(k, add(one(), sq(u.clone()))))),
            Asec => div(
                one(),
                unit_mul(k, mul(c(Abs), call(Sqrt, sub(sq(u.clone()), one())))),
            ),
            Acsc => neg(div(
                one(),
                unit_mul(k, mul(c(Abs), call(Sqrt, sub(sq(u.clone()), one())))),
            )),
            Sinh => c(Cosh),
            Cosh => c(Sinh),
            Tanh => sq(c(Sech)),
            Sech => neg(mul(c(Sech), c(Tanh))),
            Csch => neg(mul(c(Csch), c(Coth))),
            Coth => neg(sq(c(Csch))),
            Asinh => div(one(), call(Sqrt, add(sq(u.clone()), one()))),
            Acosh => div(one(), call(Sqrt, sub(sq(u.clone()), one()))),
            Atanh | Acoth => div(one(), sub(one(), sq(u.clone()))),
            Asech => neg(div(
                one(),
                mul(u.clone(), call(Sqrt, sub(one(), sq(u.clone())))),
            )),
            Acsch => neg(div(
                one(),
                mul(c(Abs), call(Sqrt, add(one(), sq(u.clone())))),
            )),
            Sqrt => div(one(), mul(Expr::Num(2.0), c(Sqrt))),
            Cbrt => div(one(), mul(Expr::Num(3.0), sq(c(Cbrt)))),
            Log => div(one(), mul(u.clone(), Expr::Num(std::f64::consts::LN_10))),
            Ln => div(one(), u.clone()),
            Exp => c(Exp),
            Abs => c(Sign),
            Floor | Ceil | Round | Sign => return Some(Expr::Num(0.0)),
            Min | Max => one(),
            Factorial | DoubleFactorial => return None,
            Root | LogBase | Mod | NCr | NPr => unreachable!("binary"),
        };
        return Some(mul(inner, du));
    }
    match f {
        Root if args.len() == 2 => {
            let (u, n) = (&args[0], &args[1]);
            if !free(n) {
                return None;
            }
            // root(u, n)' = root(u, n) / (n·u) · u'
            let du = d(u, cx)?;
            cx.charge_copies(u, 2)?;
            cx.charge_copies(n, 2)?;
            Some(mul(
                div(Expr::Call(Root, args.to_vec()), mul(n.clone(), u.clone())),
                du,
            ))
        }
        LogBase if args.len() == 2 => {
            let (b, u) = (&args[0], &args[1]);
            // ln(u)/ln(b)
            cx.charge_copies(u, 1)?;
            cx.charge_copies(b, 1)?;
            let q = div(call(Func::Ln, u.clone()), call(Func::Ln, b.clone()));
            d(&q, cx)
        }
        Mod if args.len() == 2 => {
            let (u, v) = (&args[0], &args[1]);
            // u − v·floor(u/v)
            let du = d(u, cx)?;
            if free(v) {
                return Some(du);
            }
            let dv = d(v, cx)?;
            cx.charge_copies(u, 1)?;
            cx.charge_copies(v, 1)?;
            Some(sub(
                du,
                mul(dv, call(Func::Floor, div(u.clone(), v.clone()))),
            ))
        }
        Min | Max => {
            // Fold pairwise: max(a,b)' = (a'+b')/2 + sign(a−b)·(a'−b')/2,
            // min(a,b)' = (a'+b')/2 − sign(a−b)·(a'−b')/2.
            cx.charge_copies(&args[0], 1)?;
            let mut acc = args[0].clone();
            let mut dacc = d(&args[0], cx)?;
            for b in &args[1..] {
                let db = d(b, cx)?;
                // The accumulated derivative is copied every step: without
                // this charge the result doubles per argument.
                cx.charge_copies(&dacc, 1)?;
                cx.charge_copies(&db, 1)?;
                cx.charge_copies(&acc, 1)?;
                cx.charge_copies(b, 2)?;
                let half_sum = div(add(dacc.clone(), db.clone()), Expr::Num(2.0));
                let half_diff = div(
                    mul(call(Func::Sign, sub(acc.clone(), b.clone())), sub(dacc, db)),
                    Expr::Num(2.0),
                );
                dacc = if f == Max {
                    add(half_sum, half_diff)
                } else {
                    sub(half_sum, half_diff)
                };
                acc = Expr::Call(f, vec![acc, b.clone()]);
            }
            Some(dacc)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{CompileOptions, Program};
    use crate::parser::parse_expression;

    fn check(src: &str, unit: TrigUnit, xs: &[f64]) {
        let e = parse_expression(src).unwrap();
        let de = derivative(&e, unit).unwrap_or_else(|| panic!("no derivative for {src}"));
        let opts = CompileOptions {
            trig_unit: unit,
            variables: &(),
        };
        let f = Program::compile(&e, &opts).unwrap();
        let df = Program::compile(&de, &opts).unwrap();
        for &x in xs {
            let h = 1e-6 * x.abs().max(1.0);
            let numeric = (f.eval_x(x + h) - f.eval_x(x - h)) / (2.0 * h);
            let exact = df.eval_x(x);
            if numeric.is_nan() {
                continue;
            }
            assert!(
                (numeric - exact).abs() <= 1e-5 * (1.0 + exact.abs()),
                "{src} at {x}: numeric {numeric} vs symbolic {exact} ({de})"
            );
        }
    }

    #[test]
    fn derivatives_match_finite_differences() {
        let xs = [0.3, 0.7, 1.3, 2.1, -0.4, -1.7];
        for src in [
            "x^3 - 2x + 1",
            "sin(x)cos(x)",
            "tan(x)",
            "sec(x) + csc(x) + cot(x)",
            "e^(2x) / (1 + x^2)",
            "ln(x^2 + 1)",
            "sqrt(x^2 + 2)",
            "x^(1/3)",
            "x^(2/3)",
            "|x - 0.5|",
            "arctan(x) + arcsin(x/3) + arccos(x/4)",
            "arcsec(x + 3) + arccsc(x + 3) + arccot(x)",
            "sinh(x) + cosh(x) + tanh(x)",
            "sech(x) + csch(x + 3) + coth(x + 3)",
            "arcsinh(x) + arccosh(x + 3) + arctanh(x/3)",
            "arcsech(x/3 + 0.5) + arccsch(x + 3) + arccoth(x + 3)",
            "x^x",
            "2^x",
            "log(x + 3) + log(2, x + 3)",
            "cbrt(x) + root(x + 3, 4)",
            "x mod 1.5",
            "max(x, x^2) + min(sin(x), 0.2)",
            "e^x",
            "1/x",
        ] {
            check(src, TrigUnit::Radians, &xs);
        }
        check(
            "sin(x) + cos(2x) + tan(x/3)",
            TrigUnit::Degrees,
            &[10.0, 33.0, -70.0],
        );
        check("arcsin(x/2) + arctan(x)", TrigUnit::Grads, &[0.3, -0.5]);
    }

    #[test]
    fn simple_forms() {
        let de = derivative(&parse_expression("3x^2").unwrap(), TrigUnit::Radians).unwrap();
        assert_eq!(de.formula(), "Mul(6,x)");
        let de = derivative(&parse_expression("x").unwrap(), TrigUnit::Radians).unwrap();
        assert_eq!(de.formula(), "1");
        assert!(derivative(&parse_expression("x!").unwrap(), TrigUnit::Radians).is_none());
    }
}
