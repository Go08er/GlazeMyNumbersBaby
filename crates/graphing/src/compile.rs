//! Compilation of expression trees to a compact stack bytecode, with
//! constant folding, and fast scalar / batched evaluation.
//!
//! Parameter variables are substituted by their current values at compile
//! time (recompiling is a few microseconds, so moving a slider simply
//! recompiles). The trig unit is baked in as well.

use crate::ast::{BinOp, Constant, Expr, Func};
use crate::error::{EquationError, EvaluationErrorCode};
use crate::functions::{self as fns, TrigUnit};

/// Values for parameter variables used while compiling.
pub trait VariableValues {
    /// Value of the named variable, or `None` if unknown.
    fn value(&self, name: &str) -> Option<f64>;
}

impl VariableValues for () {
    fn value(&self, _: &str) -> Option<f64> {
        None
    }
}

impl VariableValues for std::collections::HashMap<String, f64> {
    fn value(&self, name: &str) -> Option<f64> {
        self.get(name).copied()
    }
}

impl VariableValues for std::collections::BTreeMap<String, f64> {
    fn value(&self, name: &str) -> Option<f64> {
        self.get(name).copied()
    }
}

impl VariableValues for [(&str, f64)] {
    fn value(&self, name: &str) -> Option<f64> {
        self.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
    }
}

impl<const N: usize> VariableValues for [(&str, f64); N] {
    fn value(&self, name: &str) -> Option<f64> {
        self.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
    }
}

/// Default value of a variable that has no value yet (`Variable(1.0)` in
/// the original `Grapher::UpdateVariables`).
pub const DEFAULT_VARIABLE_VALUE: f64 = 1.0;

/// Compilation settings.
#[derive(Clone, Copy)]
pub struct CompileOptions<'a> {
    /// Angle unit for trigonometric functions.
    pub trig_unit: TrigUnit,
    /// Values of parameter variables. Unknown variables get
    /// [`DEFAULT_VARIABLE_VALUE`].
    pub variables: &'a dyn VariableValues,
}

impl std::fmt::Debug for CompileOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompileOptions")
            .field("trig_unit", &self.trig_unit)
            .finish_non_exhaustive()
    }
}

impl Default for CompileOptions<'_> {
    fn default() -> Self {
        CompileOptions {
            trig_unit: TrigUnit::Radians,
            variables: &(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Fn1 {
    Sin(TrigUnit),
    Cos(TrigUnit),
    Tan(TrigUnit),
    Sec(TrigUnit),
    Csc(TrigUnit),
    Cot(TrigUnit),
    Asin(TrigUnit),
    Acos(TrigUnit),
    Atan(TrigUnit),
    Asec(TrigUnit),
    Acsc(TrigUnit),
    Acot(TrigUnit),
    Sinh,
    Cosh,
    Tanh,
    Sech,
    Csch,
    Coth,
    Asinh,
    Acosh,
    Atanh,
    Asech,
    Acsch,
    Acoth,
    Sqrt,
    Cbrt,
    Log10,
    Ln,
    Exp,
    Abs,
    Floor,
    Ceil,
    Round,
    Sign,
    Factorial,
    DoubleFactorial,
}

impl Fn1 {
    #[inline(always)]
    fn apply(self, x: f64) -> f64 {
        use Fn1::*;
        match self {
            Sin(u) => fns::sin_u(x, u),
            Cos(u) => fns::cos_u(x, u),
            Tan(u) => fns::tan_u(x, u),
            Sec(u) => fns::sec_u(x, u),
            Csc(u) => fns::csc_u(x, u),
            Cot(u) => fns::cot_u(x, u),
            Asin(u) => fns::asin_u(x, u),
            Acos(u) => fns::acos_u(x, u),
            Atan(u) => fns::atan_u(x, u),
            Asec(u) => fns::asec_u(x, u),
            Acsc(u) => fns::acsc_u(x, u),
            Acot(u) => fns::acot_u(x, u),
            Sinh => x.sinh(),
            Cosh => x.cosh(),
            Tanh => x.tanh(),
            Sech => fns::sech(x),
            Csch => fns::csch(x),
            Coth => fns::coth(x),
            Asinh => x.asinh(),
            Acosh => x.acosh(),
            Atanh => x.atanh(),
            Asech => fns::asech(x),
            Acsch => fns::acsch(x),
            Acoth => fns::acoth(x),
            Sqrt => x.sqrt(),
            Cbrt => x.cbrt(),
            Log10 => x.log10(),
            Ln => x.ln(),
            Exp => x.exp(),
            Abs => x.abs(),
            Floor => x.floor(),
            Ceil => x.ceil(),
            Round => fns::round(x),
            Sign => fns::sign(x),
            Factorial => fns::factorial(x),
            DoubleFactorial => fns::double_factorial(x),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Fn2 {
    Root,
    LogBase,
    Mod,
    NCr,
    NPr,
    Min,
    Max,
}

impl Fn2 {
    #[inline(always)]
    fn apply(self, a: f64, b: f64) -> f64 {
        match self {
            Fn2::Root => fns::root(a, b),
            Fn2::LogBase => fns::log_base(a, b),
            Fn2::Mod => fns::modulo(a, b),
            Fn2::NCr => fns::ncr(a, b),
            Fn2::NPr => fns::npr(a, b),
            Fn2::Min => {
                if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.min(b)
                }
            }
            Fn2::Max => {
                if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.max(b)
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Op {
    Const(f64),
    X,
    Y,
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    /// Integer power (exact for negative bases).
    PowI(i32),
    /// Rational power p/q with real-root semantics for odd q.
    PowRat(i32, i32),
    Neg,
    F1(Fn1),
    F2(Fn2),
}

impl Op {
    /// Rough relative evaluation cost (an addition is 1).
    fn cost(&self) -> usize {
        match self {
            Op::Const(_) | Op::X | Op::Y | Op::Add | Op::Sub | Op::Mul | Op::Neg => 1,
            Op::Div | Op::PowI(_) => 2,
            Op::Pow | Op::PowRat(..) => 8,
            Op::F1(Fn1::Factorial | Fn1::DoubleFactorial) => 60,
            Op::F1(Fn1::Abs | Fn1::Floor | Fn1::Ceil | Fn1::Round | Fn1::Sign) => 1,
            Op::F1(_) => 8,
            Op::F2(Fn2::NCr | Fn2::NPr) => 300,
            Op::F2(Fn2::Min | Fn2::Max | Fn2::Mod) => 2,
            Op::F2(_) => 8,
        }
    }
}

#[inline(always)]
fn powi(b: f64, n: i32) -> f64 {
    match n {
        2 => b * b,
        3 => b * b * b,
        _ => b.powi(n),
    }
}

/// A compiled expression of `x` and `y`.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    ops: Vec<Op>,
    max_stack: usize,
    uses_x: bool,
    uses_y: bool,
    cost: usize,
}

/// Evaluation input for one coordinate of a batch: either one value for all
/// points or one value per point.
#[derive(Clone, Copy, Debug)]
pub enum Input<'a> {
    /// The same value for every point.
    Scalar(f64),
    /// One value per point (must be as long as the output).
    Slice(&'a [f64]),
}

const CHUNK: usize = 256;
const SCALAR_STACK: usize = 32;

impl Program {
    /// Compiles an expression.
    pub fn compile(expr: &Expr, opts: &CompileOptions<'_>) -> Result<Program, EquationError> {
        let piece = lower(expr, opts)?;
        let ops = match piece {
            Piece::Const(v, _) => vec![Op::Const(v)],
            Piece::Code(c) => c,
        };
        Ok(Program::from_ops(ops))
    }

    /// A program returning a constant.
    pub fn constant(v: f64) -> Program {
        Program::from_ops(vec![Op::Const(v)])
    }

    fn from_ops(ops: Vec<Op>) -> Program {
        let mut depth: usize = 0;
        let mut max_stack = 0;
        let mut uses_x = false;
        let mut uses_y = false;
        for op in &ops {
            match op {
                Op::Const(_) => depth += 1,
                Op::X => {
                    uses_x = true;
                    depth += 1
                }
                Op::Y => {
                    uses_y = true;
                    depth += 1
                }
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Pow | Op::F2(_) => depth -= 1,
                Op::PowI(_) | Op::PowRat(..) | Op::Neg | Op::F1(_) => {}
            }
            max_stack = max_stack.max(depth);
        }
        debug_assert_eq!(depth, 1);
        let cost = ops.iter().map(Op::cost).sum();
        Program {
            ops,
            max_stack,
            uses_x,
            uses_y,
            cost,
        }
    }

    /// Estimated cost of one evaluation, in units of about one arithmetic
    /// instruction (a sine is ~8, Γ ~60, nCr up to a few hundred). Used to
    /// bound analysis and plotting work.
    pub fn cost(&self) -> usize {
        self.cost
    }

    /// True if the program reads `x`.
    pub fn uses_x(&self) -> bool {
        self.uses_x
    }

    /// True if the program reads `y`.
    pub fn uses_y(&self) -> bool {
        self.uses_y
    }

    /// `Some(value)` if the program is a constant.
    pub fn as_constant(&self) -> Option<f64> {
        match self.ops.as_slice() {
            [Op::Const(v)] => Some(*v),
            _ => None,
        }
    }

    /// Number of bytecode instructions (for diagnostics).
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Always false: a program has at least one instruction.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Evaluates at one point.
    #[inline]
    pub fn eval(&self, x: f64, y: f64) -> f64 {
        if self.max_stack <= SCALAR_STACK {
            let mut stack = [0.0f64; SCALAR_STACK];
            self.eval_with(&mut stack, x, y)
        } else {
            let mut stack = vec![0.0f64; self.max_stack];
            self.eval_with(&mut stack, x, y)
        }
    }

    #[inline(always)]
    fn eval_with(&self, stack: &mut [f64], x: f64, y: f64) -> f64 {
        let mut sp = 0usize;
        for op in &self.ops {
            match *op {
                Op::Const(v) => {
                    stack[sp] = v;
                    sp += 1;
                }
                Op::X => {
                    stack[sp] = x;
                    sp += 1;
                }
                Op::Y => {
                    stack[sp] = y;
                    sp += 1;
                }
                Op::Add => {
                    sp -= 1;
                    stack[sp - 1] += stack[sp];
                }
                Op::Sub => {
                    sp -= 1;
                    stack[sp - 1] -= stack[sp];
                }
                Op::Mul => {
                    sp -= 1;
                    stack[sp - 1] *= stack[sp];
                }
                Op::Div => {
                    sp -= 1;
                    stack[sp - 1] /= stack[sp];
                }
                Op::Pow => {
                    sp -= 1;
                    stack[sp - 1] = stack[sp - 1].powf(stack[sp]);
                }
                Op::PowI(n) => stack[sp - 1] = powi(stack[sp - 1], n),
                Op::PowRat(p, q) => stack[sp - 1] = fns::pow_rational(stack[sp - 1], p, q),
                Op::Neg => stack[sp - 1] = -stack[sp - 1],
                Op::F1(f) => stack[sp - 1] = f.apply(stack[sp - 1]),
                Op::F2(f) => {
                    sp -= 1;
                    stack[sp - 1] = f.apply(stack[sp - 1], stack[sp]);
                }
            }
        }
        stack[0]
    }

    /// Evaluates `f(x)` with `y = 0`.
    #[inline]
    pub fn eval_x(&self, x: f64) -> f64 {
        self.eval(x, 0.0)
    }

    /// Evaluates many points at once. `out.len()` points are computed;
    /// slice inputs must be at least that long. Instructions are applied to
    /// chunks of points at a time, amortising dispatch.
    pub fn eval_batch(&self, xs: Input<'_>, ys: Input<'_>, out: &mut [f64]) {
        let n = out.len();
        if let Input::Slice(s) = xs {
            assert!(s.len() >= n, "x slice too short");
        }
        if let Input::Slice(s) = ys {
            assert!(s.len() >= n, "y slice too short");
        }
        if let Some(v) = self.as_constant() {
            out.fill(v);
            return;
        }
        let mut stack = vec![[0.0f64; CHUNK]; self.max_stack.max(1)];
        let mut start = 0;
        while start < n {
            let len = CHUNK.min(n - start);
            self.eval_chunk(&mut stack, xs, ys, start, len);
            out[start..start + len].copy_from_slice(&stack[0][..len]);
            start += len;
        }
    }

    fn eval_chunk(
        &self,
        stack: &mut [[f64; CHUNK]],
        xs: Input<'_>,
        ys: Input<'_>,
        start: usize,
        len: usize,
    ) {
        let mut sp = 0usize;
        let load = |dst: &mut [f64; CHUNK], src: Input<'_>| match src {
            Input::Scalar(v) => dst[..len].fill(v),
            Input::Slice(s) => dst[..len].copy_from_slice(&s[start..start + len]),
        };
        for op in &self.ops {
            match *op {
                Op::Const(v) => {
                    stack[sp][..len].fill(v);
                    sp += 1;
                }
                Op::X => {
                    load(&mut stack[sp], xs);
                    sp += 1;
                }
                Op::Y => {
                    load(&mut stack[sp], ys);
                    sp += 1;
                }
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Pow | Op::F2(_) => {
                    sp -= 1;
                    let (lo, hi) = stack.split_at_mut(sp);
                    let a = &mut lo[sp - 1][..len];
                    let b = &hi[0][..len];
                    match *op {
                        Op::Add => a.iter_mut().zip(b).for_each(|(a, b)| *a += b),
                        Op::Sub => a.iter_mut().zip(b).for_each(|(a, b)| *a -= b),
                        Op::Mul => a.iter_mut().zip(b).for_each(|(a, b)| *a *= b),
                        Op::Div => a.iter_mut().zip(b).for_each(|(a, b)| *a /= b),
                        Op::Pow => a.iter_mut().zip(b).for_each(|(a, b)| *a = a.powf(*b)),
                        Op::F2(f) => a.iter_mut().zip(b).for_each(|(a, b)| *a = f.apply(*a, *b)),
                        _ => unreachable!(),
                    }
                }
                Op::PowI(k) => stack[sp - 1][..len]
                    .iter_mut()
                    .for_each(|a| *a = powi(*a, k)),
                Op::PowRat(p, q) => stack[sp - 1][..len]
                    .iter_mut()
                    .for_each(|a| *a = fns::pow_rational(*a, p, q)),
                Op::Neg => stack[sp - 1][..len].iter_mut().for_each(|a| *a = -*a),
                Op::F1(f) => {
                    let s = &mut stack[sp - 1][..len];
                    match f {
                        Fn1::Sin(TrigUnit::Radians) => s.iter_mut().for_each(|a| *a = a.sin()),
                        Fn1::Cos(TrigUnit::Radians) => s.iter_mut().for_each(|a| *a = a.cos()),
                        Fn1::Exp => s.iter_mut().for_each(|a| *a = a.exp()),
                        Fn1::Sqrt => s.iter_mut().for_each(|a| *a = a.sqrt()),
                        Fn1::Abs => s.iter_mut().for_each(|a| *a = a.abs()),
                        Fn1::Ln => s.iter_mut().for_each(|a| *a = a.ln()),
                        _ => s.iter_mut().for_each(|a| *a = f.apply(*a)),
                    }
                }
            }
        }
    }
}

enum Piece {
    /// A folded constant; the flag records whether a parameter variable was
    /// involved (so that e.g. `x/a` with `a = 0` is not a syntax-level
    /// "divide by zero" error).
    Const(f64, bool),
    Code(Vec<Op>),
}

impl Piece {
    fn into_code(self) -> Vec<Op> {
        match self {
            Piece::Const(v, _) => vec![Op::Const(v)],
            Piece::Code(c) => c,
        }
    }
}

/// Recognises an exponent written as an integer or a ratio of integers
/// (`3`, `-2`, `1/3`, `(2/3)`, `-1/3`), returning `(p, q)` in lowest terms.
pub(crate) fn syntactic_rational(e: &Expr) -> Option<(i32, i32)> {
    fn int(e: &Expr) -> Option<i64> {
        match e {
            Expr::Num(v) if *v == v.trunc() && v.abs() < 1e6 => Some(*v as i64),
            Expr::Neg(a) => int(a).map(|v| -v),
            _ => None,
        }
    }
    let (p, q) = match e {
        Expr::Neg(a) => {
            let (p, q) = syntactic_rational(a)?;
            return Some((-p, q));
        }
        Expr::Bin(BinOp::Div, a, b) => (int(a)?, int(b)?),
        _ => (int(e)?, 1),
    };
    if q == 0 {
        return None;
    }
    let g = gcd(p.unsigned_abs(), q.unsigned_abs()) as i64;
    let (mut p, mut q) = (p / g.max(1), q / g.max(1));
    if q < 0 {
        p = -p;
        q = -q;
    }
    Some((i32::try_from(p).ok()?, i32::try_from(q).ok()?))
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

fn fn1_for(f: Func, unit: TrigUnit) -> Option<Fn1> {
    use Func::*;
    Some(match f {
        Sin => Fn1::Sin(unit),
        Cos => Fn1::Cos(unit),
        Tan => Fn1::Tan(unit),
        Sec => Fn1::Sec(unit),
        Csc => Fn1::Csc(unit),
        Cot => Fn1::Cot(unit),
        Asin => Fn1::Asin(unit),
        Acos => Fn1::Acos(unit),
        Atan => Fn1::Atan(unit),
        Asec => Fn1::Asec(unit),
        Acsc => Fn1::Acsc(unit),
        Acot => Fn1::Acot(unit),
        Sinh => Fn1::Sinh,
        Cosh => Fn1::Cosh,
        Tanh => Fn1::Tanh,
        Sech => Fn1::Sech,
        Csch => Fn1::Csch,
        Coth => Fn1::Coth,
        Asinh => Fn1::Asinh,
        Acosh => Fn1::Acosh,
        Atanh => Fn1::Atanh,
        Asech => Fn1::Asech,
        Acsch => Fn1::Acsch,
        Acoth => Fn1::Acoth,
        Sqrt => Fn1::Sqrt,
        Cbrt => Fn1::Cbrt,
        Log => Fn1::Log10,
        Ln => Fn1::Ln,
        Exp => Fn1::Exp,
        Abs => Fn1::Abs,
        Floor => Fn1::Floor,
        Ceil => Fn1::Ceil,
        Round => Fn1::Round,
        Sign => Fn1::Sign,
        Factorial => Fn1::Factorial,
        DoubleFactorial => Fn1::DoubleFactorial,
        _ => return None,
    })
}

fn fn2_for(f: Func) -> Option<Fn2> {
    Some(match f {
        Func::Root => Fn2::Root,
        Func::LogBase => Fn2::LogBase,
        Func::Mod => Fn2::Mod,
        Func::NCr => Fn2::NCr,
        Func::NPr => Fn2::NPr,
        Func::Min => Fn2::Min,
        Func::Max => Fn2::Max,
        _ => return None,
    })
}

fn lower(e: &Expr, opts: &CompileOptions<'_>) -> Result<Piece, EquationError> {
    Ok(match e {
        Expr::Num(v) => Piece::Const(*v, false),
        Expr::Const(Constant::Pi) => Piece::Const(std::f64::consts::PI, false),
        Expr::Const(Constant::E) => Piece::Const(std::f64::consts::E, false),
        Expr::X => Piece::Code(vec![Op::X]),
        Expr::Y => Piece::Code(vec![Op::Y]),
        Expr::Var(n) => Piece::Const(
            opts.variables.value(n).unwrap_or(DEFAULT_VARIABLE_VALUE),
            true,
        ),
        Expr::Degrees(a) => {
            if opts.trig_unit != TrigUnit::Degrees {
                return Err(EquationError::eval(
                    EvaluationErrorCode::RequireDegreesMode,
                    0..0,
                ));
            }
            lower(a, opts)?
        }
        Expr::Neg(a) => match lower(a, opts)? {
            Piece::Const(v, var) => Piece::Const(-v, var),
            Piece::Code(mut c) => {
                c.push(Op::Neg);
                Piece::Code(c)
            }
        },
        Expr::Bin(op, a, b) => {
            let la = lower(a, opts)?;
            if *op == BinOp::Pow
                && let Some((p, q)) = syntactic_rational(b)
            {
                return Ok(match la {
                    Piece::Const(v, var) => Piece::Const(
                        if q == 1 {
                            v.powi(p)
                        } else {
                            fns::pow_rational(v, p, q)
                        },
                        var,
                    ),
                    Piece::Code(mut c) => {
                        if q == 1 {
                            if p == 1 {
                                return Ok(Piece::Code(c));
                            }
                            c.push(Op::PowI(p));
                        } else {
                            c.push(Op::PowRat(p, q));
                        }
                        Piece::Code(c)
                    }
                });
            }
            let lb = lower(b, opts)?;
            match (la, lb) {
                (Piece::Const(x, va), Piece::Const(y, vb)) => {
                    if *op == BinOp::Div && y == 0.0 && !vb {
                        return Err(EquationError::eval(EvaluationErrorCode::DivideByZero, 0..0));
                    }
                    Piece::Const(apply_bin(*op, x, y), va || vb)
                }
                (la, lb) => {
                    if let (BinOp::Div, Piece::Const(y, false)) = (op, &lb)
                        && *y == 0.0
                    {
                        return Err(EquationError::eval(EvaluationErrorCode::DivideByZero, 0..0));
                    }
                    let mut c = la.into_code();
                    c.extend(lb.into_code());
                    c.push(match op {
                        BinOp::Add => Op::Add,
                        BinOp::Sub => Op::Sub,
                        BinOp::Mul => Op::Mul,
                        BinOp::Div => Op::Div,
                        BinOp::Pow => Op::Pow,
                    });
                    Piece::Code(c)
                }
            }
        }
        Expr::Call(f, args) => {
            let lowered: Vec<Piece> = args
                .iter()
                .map(|a| lower(a, opts))
                .collect::<Result<_, _>>()?;
            if let Some(f1) = fn1_for(*f, opts.trig_unit) {
                let arg = lowered.into_iter().next().expect("arity checked by parser");
                match arg {
                    Piece::Const(v, var) => Piece::Const(f1.apply(v), var),
                    Piece::Code(mut c) => {
                        c.push(Op::F1(f1));
                        Piece::Code(c)
                    }
                }
            } else {
                let f2 = fn2_for(*f).expect("every function is unary or binary");
                // Fold left for variadic min/max.
                let mut it = lowered.into_iter();
                let mut acc = it.next().expect("at least one argument");
                for next in it {
                    acc = match (acc, next) {
                        (Piece::Const(a, va), Piece::Const(b, vb)) => {
                            Piece::Const(f2.apply(a, b), va || vb)
                        }
                        (a, b) => {
                            let mut c = a.into_code();
                            c.extend(b.into_code());
                            c.push(Op::F2(f2));
                            Piece::Code(c)
                        }
                    };
                }
                acc
            }
        }
    })
}

fn apply_bin(op: BinOp, a: f64, b: f64) -> f64 {
    match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => a / b,
        BinOp::Pow => a.powf(b),
    }
}

/// Parses and compiles an expression in one go (convenience for tests and
/// simple uses). Variables take their default value.
pub fn compile_str(src: &str, unit: TrigUnit) -> Result<Program, EquationError> {
    let e = crate::parser::parse_expression(src)?;
    Program::compile(
        &e,
        &CompileOptions {
            trig_unit: unit,
            variables: &(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(src: &str, x: f64) -> f64 {
        compile_str(src, TrigUnit::Radians).unwrap().eval(x, 0.0)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-12 * (1.0 + b.abs())
    }

    #[test]
    fn basic_eval() {
        assert_eq!(ev("1+2*3", 0.0), 7.0);
        assert_eq!(ev("2x^2", 3.0), 18.0);
        assert_eq!(ev("-x^2", 3.0), -9.0);
        assert_eq!(ev("1/2x", 2.0), 0.25);
        assert_eq!(ev("|x - 5|", 2.0), 3.0);
        assert_eq!(ev("x!", 4.0), 24.0);
        assert!(close(ev("2π", 0.0), 2.0 * std::f64::consts::PI));
        assert!(close(ev("e^x", 1.0), std::f64::consts::E));
        assert!(close(ev("log(2, x)", 8.0), 3.0));
        assert!(close(ev("root(x, 3)", -27.0), -3.0));
        assert!(close(ev("x^(1/3)", -8.0), -2.0));
        assert!(close(ev("x^(2/3)", -8.0), 4.0));
        assert!(ev("x^0.5", -4.0).is_nan());
        assert!(ev("sqrt(x)", -1.0).is_nan());
        assert!(close(ev("x^-2", 2.0), 0.25));
        assert!(close(ev("(-2)^3", 0.0), -8.0));
        assert_eq!(ev("floor(x) + ceiling(x)", 1.5), 3.0);
        assert_eq!(ev("x mod 3", -1.0), 2.0);
        assert!(close(ev("sin(x)^2 + cos(x)^2", 0.7), 1.0));
        assert!(close(ev("arcsin(1)", 0.0), std::f64::consts::FRAC_PI_2));
        assert!(close(ev("sec(0) + csc(π/2) + cot(π/4)", 0.0), 3.0));
        assert!(close(ev("sinh(x) - (e^x - e^-x)/2", 1.3), 0.0));
        assert!(close(ev("arccosh(cosh(2))", 0.0), 2.0));
        assert!(close(ev("ln(e^3)", 0.0), 3.0));
        assert!(close(ev("log(1000)", 0.0), 3.0));
        assert!(close(ev("10^x", 2.0), 100.0));
        assert!(close(ev("cbrt(-8)", 0.0), -2.0));
        assert!(close(ev("nCr(5, 2) + nPr(5, 2)", 0.0), 30.0));
    }

    #[test]
    fn trig_units_are_baked_in() {
        let p = compile_str("sin(x)", TrigUnit::Degrees).unwrap();
        assert!(close(p.eval(30.0, 0.0), 0.5));
        let p = compile_str("arctan(1)", TrigUnit::Degrees).unwrap();
        assert!(close(p.eval(0.0, 0.0), 45.0));
        let p = compile_str("cos(x)", TrigUnit::Grads).unwrap();
        assert_eq!(p.eval(200.0, 0.0), -1.0);
        // Hyperbolic functions ignore the unit.
        let p = compile_str("sinh(1)", TrigUnit::Degrees).unwrap();
        assert!(close(p.eval(0.0, 0.0), 1f64.sinh()));
        // Degree sign requires degrees mode.
        assert!(compile_str("sin(30°)", TrigUnit::Radians).is_err());
        let p = compile_str("sin(30°)", TrigUnit::Degrees).unwrap();
        assert!(close(p.eval(0.0, 0.0), 0.5));
    }

    #[test]
    fn constant_folding_and_variables() {
        let p = compile_str("2*3+sin(0)", TrigUnit::Radians).unwrap();
        assert_eq!(p.as_constant(), Some(6.0));
        let e = crate::parser::parse_expression("a x + b").unwrap();
        let vars = [("a", 2.0), ("b", 3.0)];
        let p = Program::compile(
            &e,
            &CompileOptions {
                trig_unit: TrigUnit::Radians,
                variables: &vars,
            },
        )
        .unwrap();
        assert_eq!(p.eval(4.0, 0.0), 11.0);
        // Unknown variables default to 1.
        let p = Program::compile(&e, &CompileOptions::default()).unwrap();
        assert_eq!(p.eval(4.0, 0.0), 5.0);
        // Literal division by zero is an evaluation error; through a variable it is not.
        let err = compile_str("x/0", TrigUnit::Radians).unwrap_err();
        assert_eq!(err.message(), "Cannot divide by zero");
        let e = crate::parser::parse_expression("x/a").unwrap();
        let vars = [("a", 0.0)];
        assert!(
            Program::compile(
                &e,
                &CompileOptions {
                    trig_unit: TrigUnit::Radians,
                    variables: &vars
                }
            )
            .is_ok()
        );
    }

    #[test]
    fn batch_matches_scalar() {
        let p = compile_str(
            "sin(3x) * e^(-x/4) + sqrt(|x|) - x^(1/3) + floor(x) + tan(x) + x mod 2",
            TrigUnit::Radians,
        )
        .unwrap();
        let xs: Vec<f64> = (0..1000).map(|i| -20.0 + i as f64 * 0.0371).collect();
        let mut out = vec![0.0; xs.len()];
        p.eval_batch(Input::Slice(&xs), Input::Scalar(0.0), &mut out);
        for (x, o) in xs.iter().zip(&out) {
            let s = p.eval(*x, 0.0);
            assert!((s.is_nan() && o.is_nan()) || s == *o, "{x}: {s} vs {o}");
        }
        let q = compile_str("x^2 + y^2", TrigUnit::Radians).unwrap();
        let ys: Vec<f64> = xs.iter().map(|x| x * 0.5).collect();
        q.eval_batch(Input::Slice(&xs), Input::Slice(&ys), &mut out);
        for i in 0..xs.len() {
            assert_eq!(out[i], q.eval(xs[i], ys[i]));
        }
    }
}
