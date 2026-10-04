//! An explicit curve's function as interval arithmetic sees it: the tree
//! as typed (in the independent variable), its literals' exact decimals,
//! the angle unit and the slider values it was compiled with. The plotter
//! proves continuity and chord errors with it; tracing proves a value's
//! digits (or that the point is undefined).

use std::collections::BTreeMap;

use crate::ast::Expr;
use crate::compile::{CompileOptions, DEFAULT_VARIABLE_VALUE, VariableValues};
use crate::functions::TrigUnit;
use crate::interval::{Ctx, DecInterval, Interval, Literals, Series, enclose, taylor};

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

    /// Taylor coefficients `c[0..=n]` of f over [lo, hi].
    pub fn series(&self, lo: f64, hi: f64, n: usize) -> Series {
        self.with_ctx(|ctx| taylor(&self.expr, Interval::new(lo, hi), n, ctx))
    }
}
