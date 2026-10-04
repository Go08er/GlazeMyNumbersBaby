//! The side conditions of an expression: where each of its operations is
//! defined, and where a step function jumps.
//!
//! An expression is defined at x exactly when every condition of
//! [`side_conditions`] holds there, so its domain is their intersection.
//! They are read off the expression *as written*: simplifying `x/x` to 1
//! keeps `x ≠ 0`, `tan(x)·cos(x)` to sin x keeps `cos x ≠ 0`. The
//! certifier computes a domain as the simplified form's domain intersected
//! with these (`domain(simplified) ∩ conditions(original)`); holes come
//! from here and only from here.
//!
//! The meanings follow the graphing evaluator (`functions.rs`,
//! `compile.rs`), including the TI-84 Plus CE's power rule.

use crate::ast::{BinOp, Expr, Func};
use crate::compile::syntactic_rational;

/// Which trig function's zeros a condition excludes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Trig {
    /// sin(arg) ≠ 0 (under cot and csc).
    Sin,
    /// cos(arg) ≠ 0 (under tan and sec).
    Cos,
}

/// One end of a range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct End {
    /// The bound (an exact small number: ±1, 0).
    pub value: f64,
    /// True if the bound itself is allowed.
    pub closed: bool,
}

/// One condition.
#[derive(Clone, Debug, PartialEq)]
pub enum Cond {
    /// `e ≠ 0` (a divisor, a base under a power ≤ 0, csch/coth's argument,
    /// mod's divisor, a log's argument).
    NonZero(Expr),
    /// `e > 0` (ln, log, a log base).
    Positive(Expr),
    /// `e ≥ 0` (an even root).
    NonNegative(Expr),
    /// `lo ≤ e ≤ hi`, ends open or closed (asin and acos: [−1, 1]; atanh:
    /// (−1, 1); asech: (0, 1]; acosh: [1, ∞)).
    Within {
        /// The expression.
        e: Expr,
        /// Lower end (`None`: unbounded).
        lo: Option<End>,
        /// Upper end (`None`: unbounded).
        hi: Option<End>,
    },
    /// `|e| ≥ 1` (closed) or `|e| > 1` (open): asec, acsc; acoth.
    Outside {
        /// The expression.
        e: Expr,
        /// True if |e| = 1 is allowed.
        closed: bool,
    },
    /// `sin(arg) ≠ 0` or `cos(arg) ≠ 0`, in the expression's angle unit.
    TrigNonZero {
        /// Which function.
        f: Trig,
        /// Its argument.
        arg: Expr,
    },
    /// The TI power rule for an exponent that varies with x: `base > 0`,
    /// or `base = 0` with `exponent > 0`.
    PowVar {
        /// The base.
        base: Expr,
        /// The exponent.
        exponent: Expr,
    },
    /// `functions::pow` for a constant exponent that isn't a syntactic
    /// rational: `base ≥ 0` unless the exponent is an integer value,
    /// `base ≠ 0` if the exponent is ≤ 0.
    PowConst {
        /// The base.
        base: Expr,
        /// The (constant) exponent.
        exponent: Expr,
    },
    /// Where a function is defined only on a set the conditions above
    /// can't state (factorials, nCr, a general root degree): the
    /// certifier decides by intervals.
    Opaque(Expr),
}

/// A point where an expression may jump: a step function's argument
/// crossing a step (not a domain condition).
#[derive(Clone, Debug, PartialEq)]
pub enum Jump {
    /// floor, ceil, round: the argument crossing an integer (round: a
    /// half-integer).
    Step {
        /// The function.
        f: Func,
        /// Its argument.
        arg: Expr,
    },
    /// sign: the argument crossing 0.
    Sign(Expr),
    /// mod(a, b): a/b crossing an integer.
    Mod(Expr, Expr),
}

/// The domain conditions of `e`, outermost first, without duplicates.
pub fn side_conditions(e: &Expr) -> Vec<Cond> {
    let mut out = Vec::new();
    collect(e, &mut out);
    let mut uniq: Vec<Cond> = Vec::with_capacity(out.len());
    for c in out {
        if !uniq.contains(&c) {
            uniq.push(c);
        }
    }
    uniq
}

/// The step functions' jump conditions of `e`.
pub fn jumps(e: &Expr) -> Vec<Jump> {
    let mut out = Vec::new();
    e.visit(&mut |n| {
        if let Expr::Call(f, args) = n {
            match f {
                Func::Floor | Func::Ceil | Func::Round => out.push(Jump::Step {
                    f: *f,
                    arg: args[0].clone(),
                }),
                Func::Sign => out.push(Jump::Sign(args[0].clone())),
                Func::Mod => out.push(Jump::Mod(args[0].clone(), args[1].clone())),
                _ => {}
            }
        }
    });
    out
}

fn depends_on_x(e: &Expr) -> bool {
    e.contains_x()
}

fn collect(e: &Expr, out: &mut Vec<Cond>) {
    // Conditions on x-free subexpressions are constants the evaluator
    // folds (a literal 1/0 is an error before analysis); keep them only
    // when they involve x or a slider.
    fn varies(e: &Expr) -> bool {
        depends_on_x(e) || e.any(&|n| matches!(n, Expr::Var(_)))
    }
    fn push(out: &mut Vec<Cond>, c: Cond, on: &Expr) {
        if varies(on) {
            out.push(c);
        }
    }
    match e {
        Expr::Num(_) | Expr::Const(_) | Expr::X | Expr::Y | Expr::Var(_) => {}
        Expr::Neg(a) | Expr::Degrees(a) => collect(a, out),
        Expr::Bin(op, a, b) => {
            collect(a, out);
            collect(b, out);
            match op {
                BinOp::Div => push(out, Cond::NonZero((**b).clone()), b),
                BinOp::Pow => {
                    if let Some((p, q)) = syntactic_rational(b) {
                        if p <= 0 {
                            push(out, Cond::NonZero((**a).clone()), a);
                        }
                        if q % 2 == 0 {
                            push(out, Cond::NonNegative((**a).clone()), a);
                        }
                    } else if b.contains_x() {
                        out.push(Cond::PowVar {
                            base: (**a).clone(),
                            exponent: (**b).clone(),
                        });
                    } else if varies(a) || varies(b) {
                        out.push(Cond::PowConst {
                            base: (**a).clone(),
                            exponent: (**b).clone(),
                        });
                    }
                }
                _ => {}
            }
        }
        Expr::Call(f, args) => {
            for a in args {
                collect(a, out);
            }
            let a = &args[0];
            let r = |v: f64, closed: bool| Some(End { value: v, closed });
            use Func::*;
            match f {
                Tan | Sec => push(
                    out,
                    Cond::TrigNonZero {
                        f: Trig::Cos,
                        arg: a.clone(),
                    },
                    a,
                ),
                Cot | Csc => push(
                    out,
                    Cond::TrigNonZero {
                        f: Trig::Sin,
                        arg: a.clone(),
                    },
                    a,
                ),
                Asin | Acos => push(
                    out,
                    Cond::Within {
                        e: a.clone(),
                        lo: r(-1.0, true),
                        hi: r(1.0, true),
                    },
                    a,
                ),
                Asec | Acsc => push(
                    out,
                    Cond::Outside {
                        e: a.clone(),
                        closed: true,
                    },
                    a,
                ),
                Acosh => push(
                    out,
                    Cond::Within {
                        e: a.clone(),
                        lo: r(1.0, true),
                        hi: None,
                    },
                    a,
                ),
                Atanh => push(
                    out,
                    Cond::Within {
                        e: a.clone(),
                        lo: r(-1.0, false),
                        hi: r(1.0, false),
                    },
                    a,
                ),
                Asech => push(
                    out,
                    Cond::Within {
                        e: a.clone(),
                        lo: r(0.0, false),
                        hi: r(1.0, true),
                    },
                    a,
                ),
                Acsch | Csch | Coth => push(out, Cond::NonZero(a.clone()), a),
                Acoth => push(
                    out,
                    Cond::Outside {
                        e: a.clone(),
                        closed: false,
                    },
                    a,
                ),
                Sqrt => push(out, Cond::NonNegative(a.clone()), a),
                Ln | Log => push(out, Cond::Positive(a.clone()), a),
                LogBase => {
                    // log(b, x): b > 0, b ≠ 1, x > 0.
                    let (b, x) = (&args[0], &args[1]);
                    push(out, Cond::Positive(b.clone()), b);
                    push(
                        out,
                        Cond::NonZero(Expr::bin(BinOp::Sub, b.clone(), Expr::Num(1.0))),
                        b,
                    );
                    push(out, Cond::Positive(x.clone()), x);
                }
                Mod => push(out, Cond::NonZero(args[1].clone()), &args[1]),
                Root => {
                    let n = &args[1];
                    match syntactic_rational(n) {
                        Some((k, 1)) if k % 2 != 0 && k > 0 => {}
                        Some((k, 1)) if k % 2 != 0 => push(out, Cond::NonZero(a.clone()), a),
                        Some((k, 1)) if k > 0 => push(out, Cond::NonNegative(a.clone()), a),
                        Some((k, 1)) if k < 0 => push(out, Cond::Positive(a.clone()), a),
                        _ => {
                            if varies(a) || varies(n) {
                                out.push(Cond::Opaque(e.clone()));
                            }
                        }
                    }
                }
                Factorial | DoubleFactorial | NCr | NPr => {
                    if args.iter().any(|a| varies(a)) {
                        out.push(Cond::Opaque(e.clone()));
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equation::Equation;

    fn f(s: &str) -> Expr {
        let eq = Equation::parse(&format!("y={s}")).unwrap();
        eq.explicit().unwrap().1.clone()
    }

    #[test]
    fn holes_come_from_the_expression_as_written() {
        let c = side_conditions(&f("tan(x)*cos(x)"));
        assert_eq!(
            c,
            vec![Cond::TrigNonZero {
                f: Trig::Cos,
                arg: Expr::X
            }]
        );
        assert_eq!(side_conditions(&f("x/x")), vec![Cond::NonZero(Expr::X)]);
        assert_eq!(side_conditions(&f("ln(x)")), vec![Cond::Positive(Expr::X)]);
        assert!(matches!(side_conditions(&f("x^x"))[0], Cond::PowVar { .. }));
        assert_eq!(side_conditions(&f("x^0")), vec![Cond::NonZero(Expr::X)]);
        assert!(side_conditions(&f("x^(1/3)")).is_empty());
        assert_eq!(
            side_conditions(&f("x^(1/2)")),
            vec![Cond::NonNegative(Expr::X)]
        );
        assert!(side_conditions(&f("sin(x)+1/2")).is_empty());
    }

    #[test]
    fn jumps_are_listed() {
        assert_eq!(jumps(&f("floor(x)+sign(x)")).len(), 2);
    }
}
