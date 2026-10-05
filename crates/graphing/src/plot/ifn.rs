//! An explicit curve's function as interval arithmetic sees it: the tree
//! as typed (in the independent variable), its literals' exact decimals,
//! the angle unit and the slider values it was compiled with. The plotter
//! proves continuity and chord errors with it; tracing proves a value's
//! digits (or that the point is undefined).

use std::collections::BTreeMap;

use crate::ast::{BinOp, Expr, Func, Lit};
use crate::compile::{CompileOptions, DEFAULT_VARIABLE_VALUE, VariableValues};
use crate::functions::TrigUnit;
use crate::interval::{Ctx, DecInterval, Interval, Literals, Series, enclose, taylor};
use crate::simplify::Q;

/// Slider values captured when the curve was compiled.
#[derive(Clone, Debug, Default)]
struct Vars(BTreeMap<String, f64>);

impl VariableValues for Vars {
    fn value(&self, name: &str) -> Option<f64> {
        self.0.get(name).copied()
    }
}

/// `f` of an explicit curve, for interval evaluation.
#[derive(Clone, Debug)]
pub struct IntervalFn {
    expr: Expr,
    literals: Literals,
    unit: TrigUnit,
    vars: Vars,
}

impl IntervalFn {
    /// `expr` in x (an `x = g(y)` curve already swapped), `literals` read
    /// from the text it was parsed from, and the options it was compiled
    /// with.
    pub(crate) fn new(expr: Expr, literals: Literals, opts: &CompileOptions<'_>) -> IntervalFn {
        let vars = expr
            .variables()
            .into_iter()
            .map(|n| {
                let v = opts.variables.value(&n).unwrap_or(DEFAULT_VARIABLE_VALUE);
                (n, v)
            })
            .collect();
        IntervalFn {
            expr,
            literals,
            unit: opts.trig_unit,
            vars: Vars(vars),
        }
    }

    fn with_ctx<R>(&self, f: impl FnOnce(&Ctx<'_>) -> R) -> R {
        let opts = CompileOptions {
            trig_unit: self.unit,
            variables: &self.vars,
        };
        f(&Ctx::new(opts, &self.literals))
    }

    /// An enclosure of f over [lo, hi], decorated.
    pub fn enclose(&self, lo: f64, hi: f64) -> DecInterval {
        self.with_ctx(|ctx| enclose(&self.expr, Interval::new(lo, hi), ctx))
    }

    /// Whether f is unbounded at `x` because a factorial there is at a
    /// pole of Γ (see [`Ctx::pole_probe`]): a pole, not a hole, however
    /// small the values beside it.
    pub(crate) fn pole_at(&self, x: f64) -> bool {
        let opts = CompileOptions {
            trig_unit: self.unit,
            variables: &self.vars,
        };
        let mut ctx = Ctx::new(opts, &self.literals);
        ctx.pole_probe = true;
        let e = enclose(&self.expr, Interval::point(x), &ctx);
        !e.is_empty() && !e.iv.is_bounded()
    }

    /// Taylor coefficients `c[0..=n]` of f over [lo, hi].
    pub fn series(&self, lo: f64, hi: f64, n: usize) -> Series {
        self.with_ctx(|ctx| taylor(&self.expr, Interval::new(lo, hi), n, ctx))
    }

    /// Relative cost of one interval evaluation, in the units of
    /// [`crate::compile::Program::cost`] (an addition is 1, a sine 8):
    /// the same weights, but Γ and nCr/nPr no dearer than a sine, as
    /// their interval forms are (their point forms are not).
    pub(crate) fn cost(&self) -> usize {
        fn c(e: &Expr) -> usize {
            match e {
                Expr::Num(..) | Expr::Const(_) | Expr::X | Expr::Y | Expr::Var(_) => 1,
                Expr::Neg(a) | Expr::Degrees(a) => 1 + c(a),
                Expr::Bin(op, a, b) => {
                    let own = match op {
                        BinOp::Add | BinOp::Sub | BinOp::Mul => 1,
                        BinOp::Div => 2,
                        BinOp::Pow => 8,
                    };
                    own + c(a) + c(b)
                }
                Expr::Call(f, args) => {
                    let own = match f {
                        Func::Abs | Func::Floor | Func::Ceil | Func::Round | Func::Sign => 1,
                        Func::Min | Func::Max | Func::Mod => 2,
                        Func::NCr | Func::NPr => 16,
                        _ => 8,
                    };
                    own + args.iter().map(c).sum::<usize>()
                }
            }
        }
        c(&self.expr)
    }

    /// f at the rational `x` exactly where f is a rational function: +,
    /// −, ×, ÷ and whole powers of x, sliders and literals that are their
    /// doubles exactly. None for anything else, where f is undefined, or
    /// past the range of [`Q`].
    pub(crate) fn exact_at(&self, x: Q) -> Option<Q> {
        fn ev(e: &Expr, x: Q, f: &IntervalFn) -> Option<Q> {
            match e {
                Expr::Num(v, Lit::Exact) => Q::from_f64(*v),
                Expr::X => Some(x),
                Expr::Var(n) => Q::from_f64(*f.vars.0.get(n)?),
                Expr::Neg(a) => ev(a, x, f)?.neg(),
                Expr::Bin(op, a, b) => {
                    let (a, b) = (ev(a, x, f)?, ev(b, x, f)?);
                    match op {
                        BinOp::Add => a.add(b),
                        BinOp::Sub => a.sub(b),
                        BinOp::Mul => a.mul(b),
                        BinOp::Div if !b.is_zero() => a.div(b),
                        // The TI rule: 0 to a power ≤ 0 is undefined.
                        BinOp::Pow if b.is_int() && !(a.is_zero() && b.signum() <= 0) => {
                            a.powi(b.as_int().filter(|k| k.abs() <= 64)?)
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        ev(&self.expr, x, self)
    }
}
