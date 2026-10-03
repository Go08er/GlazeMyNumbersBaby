//! Compilation of expression trees to a compact stack bytecode, with
//! constant folding, and fast scalar / batched evaluation.
//!
//! Parameter variables are substituted by their current values at compile
//! time (recompiling is a few microseconds, so moving a slider simply
//! recompiles). The trig unit is baked in as well.

use crate::ast::{BinOp, Constant, Expr, Func};
use crate::error::{EquationError, EvaluationErrorCode};
use crate::functions::{self as fns, TrigUnit};
use crate::wide::{self, Wide};

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
    pub(crate) fn apply(self, x: f64) -> f64 {
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
            Atanh => fns::atanh(x),
            Asech => fns::asech(x),
            Acsch => fns::acsch(x),
            Acoth => fns::acoth(x),
            Sqrt => x.sqrt(),
            Cbrt => x.cbrt(),
            Log10 => fns::log10(x),
            Ln => fns::ln(x),
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
    pub(crate) fn apply(self, a: f64, b: f64) -> f64 {
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
    /// A constant beyond the range of a double (e^−1000 inside x/e^−1000):
    /// its rounding, and its value for the extended-range evaluation.
    Big(f64, Wide),
    X,
    Y,
    Add,
    Sub,
    Mul,
    Div,
    /// Division by something never exactly 0 where defined (e^g, 2^g, …):
    /// a 0 there is underflow, so the quotient overflows to ±∞ instead of
    /// being undefined (1/e^(1/x) just left of 0).
    DivNz,
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
    /// Whether the result of normal operands can overflow, or underflow to
    /// 0 or a subnormal, rather than be exact there. (A sum overflows only
    /// beyond 10³⁰⁸; a difference that is 0 or subnormal is exact.)
    fn may_leave_range(&self) -> bool {
        use Fn1::*;
        match self {
            Op::Mul | Op::Div | Op::DivNz | Op::Pow | Op::PowRat(..) => true,
            Op::PowI(n) => !matches!(n, 0 | 1),
            Op::F1(f) => matches!(
                f,
                Sec(_)
                    | Csc(_)
                    | Cot(_)
                    | Sinh
                    | Cosh
                    | Sech
                    | Csch
                    | Coth
                    | Asech
                    | Acsch
                    | Exp
                    | Factorial
                    | DoubleFactorial
            ),
            Op::F2(f) => matches!(f, Fn2::Root | Fn2::NCr | Fn2::NPr),
            _ => false,
        }
    }

    /// Rough relative evaluation cost (an addition is 1).
    fn cost(&self) -> usize {
        match self {
            Op::Const(_) | Op::Big(..) | Op::X | Op::Y | Op::Add | Op::Sub | Op::Mul | Op::Neg => 1,
            Op::Div | Op::DivNz | Op::PowI(_) => 2,
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
    fns::pow_int(b, n)
}

/// Whether `v` may be the rounding of a value beyond a double's range: ±∞,
/// 0 or subnormal. NaN is not. (Branch-free, for batches.)
#[inline(always)]
fn left_range(v: f64) -> bool {
    let b = v.to_bits() & !(1u64 << 63);
    (b == f64::INFINITY.to_bits()) | (b < 1u64 << 52)
}

/// A compiled expression of `x` and `y`.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    ops: Vec<Op>,
    max_stack: usize,
    uses_x: bool,
    uses_y: bool,
    cost: usize,
    /// Whether an operation before the last can overflow or underflow, or a
    /// constant is beyond a double, so that the result (finite or not) may
    /// come from rounding out of a double's range (see [`Wide`]).
    wide: bool,
    /// Per instruction, whether its value can be the rounding of one beyond
    /// a double's range when its operands are normal: ±∞, 0 or subnormal
    /// after a product, quotient, power, e^x, … other than the last (see
    /// [`Program::eval_tagged`]). A sum overflows only to ±∞, which every
    /// later operation keeps (as ±∞ or NaN) or turns into a 0 that is
    /// itself checked: so only the result is checked for that.
    scan: Box<[bool]>,
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
        let ops = piece.into_code();
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
                Op::Const(_) | Op::Big(..) => depth += 1,
                Op::X => {
                    uses_x = true;
                    depth += 1
                }
                Op::Y => {
                    uses_y = true;
                    depth += 1
                }
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::DivNz | Op::Pow | Op::F2(_) => {
                    depth -= 1
                }
                Op::PowI(_) | Op::PowRat(..) | Op::Neg | Op::F1(_) => {}
            }
            max_stack = max_stack.max(depth);
        }
        debug_assert_eq!(depth, 1);
        let cost = ops.iter().map(Op::cost).sum();
        // Only a rounded intermediate can mislead a later operation; the
        // last one's own overflow or underflow is what extended range gives.
        let last = ops.len() - 1;
        let scan: Box<[bool]> = ops
            .iter()
            .enumerate()
            .map(|(i, op)| matches!(op, Op::Big(..)) || (i != last && op.may_leave_range()))
            .collect();
        let wide =
            scan.iter().any(|&s| s) || ops[..last].iter().any(|op| matches!(op, Op::Add | Op::Sub));
        Program {
            ops,
            max_stack,
            uses_x,
            uses_y,
            cost,
            wide,
            scan,
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
            [Op::Const(v) | Op::Big(v, _)] => Some(*v),
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

    /// Evaluates at one point. Undefined points give NaN; a defined value
    /// beyond the range of a double gives ±∞ or 0, never NaN, and ±∞ only
    /// for such a value (e^x/e^x is 1 for x > 709.8, not ∞/∞, and ln(e^x)
    /// is x, not ∞).
    #[inline]
    pub fn eval(&self, x: f64, y: f64) -> f64 {
        self.eval_tagged(x, y).0
    }

    /// [`Program::eval`], and whether the value needed re-evaluating in
    /// extended range: where an intermediate overflowed to ±∞ or underflowed
    /// to 0 or a subnormal, whether or not the double result is finite
    /// (1 + √(−e^(−800)) is undefined, not 1; √(e^(−800))·e^400 is 1, not 0).
    #[inline]
    pub(crate) fn eval_tagged(&self, x: f64, y: f64) -> (f64, Redo) {
        if !self.wide {
            return (self.eval_plain(x, y), Redo::No);
        }
        let (v, rounded) = if self.max_stack <= SCALAR_STACK {
            let mut stack = [0.0f64; SCALAR_STACK];
            self.eval_with::<false, true>(&mut stack, x, y)
        } else {
            let mut stack = vec![0.0f64; self.max_stack];
            self.eval_with::<false, true>(&mut stack, x, y)
        };
        if rounded || !v.is_normal() {
            self.redo(x, y)
        } else {
            (v, Redo::No)
        }
    }

    /// Whether a value from [`Program::eval_plain`] may be the rounding of
    /// one beyond a double's range, to get with [`Program::redo`].
    pub(crate) fn may_redo(&self) -> bool {
        self.wide
    }

    /// The value at a point where an intermediate double may have left the
    /// range, in extended range if one did before any undefined operation.
    #[cold]
    pub(crate) fn redo(&self, x: f64, y: f64) -> (f64, Redo) {
        let (v, off) = if self.max_stack <= 8 {
            let mut stack = [0.0f64; 8];
            self.eval_with::<true, false>(&mut stack, x, y)
        } else if self.max_stack <= SCALAR_STACK {
            let mut stack = [0.0f64; SCALAR_STACK];
            self.eval_with::<true, false>(&mut stack, x, y)
        } else {
            let mut stack = vec![0.0f64; self.max_stack];
            self.eval_with::<true, false>(&mut stack, x, y)
        };
        if !off {
            return (v, Redo::No);
        }
        let w = self.eval_wide(x, y);
        let tag = if w == Wide::Unknown {
            Redo::Unknown
        } else {
            Redo::Wide
        };
        (w.to_f64(), tag)
    }

    /// Whether the value at a point is exactly 0, rather than a nonzero value
    /// below a double's range (e^x for x < −745.1).
    pub(crate) fn is_exact_zero(&self, x: f64, y: f64) -> bool {
        self.eval_wide(x, y).is_zero()
    }

    /// Evaluates in double arithmetic only: NaN or ±∞ may then also be a
    /// value beyond a double's range.
    #[inline]
    pub(crate) fn eval_plain(&self, x: f64, y: f64) -> f64 {
        if self.max_stack <= SCALAR_STACK {
            let mut stack = [0.0f64; SCALAR_STACK];
            self.eval_with::<false, false>(&mut stack, x, y).0
        } else {
            let mut stack = vec![0.0f64; self.max_stack];
            self.eval_with::<false, false>(&mut stack, x, y).0
        }
    }

    /// Evaluates in doubles; with `CHECK`, also whether the result may be
    /// the rounding of a value beyond their range: whether, before any
    /// undefined operation, an intermediate result overflowed or underflowed
    /// (to 0 or a subnormal), so that later operations may have made
    /// something else of it. Where operands are normal or exact, extended
    /// range gives what a double does (both NaN for √−1, both overflow for
    /// e^800). With `FLAG` (cheaper, and not stopping early), whether any
    /// intermediate may have left the range at all (see `scan`).
    #[inline(always)]
    fn eval_with<const CHECK: bool, const FLAG: bool>(
        &self,
        stack: &mut [f64],
        x: f64,
        y: f64,
    ) -> (f64, bool) {
        let mut sp = 0usize;
        let last = self.ops.len() - 1;
        let mut rounded = false;
        for (i, (op, &scan)) in self.ops.iter().zip(self.scan.iter()).enumerate() {
            match *op {
                Op::Const(v) | Op::Big(v, _) => {
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
                    stack[sp - 1] = fns::div(stack[sp - 1], stack[sp]);
                }
                Op::DivNz => {
                    sp -= 1;
                    stack[sp - 1] /= stack[sp];
                }
                Op::Pow => {
                    sp -= 1;
                    stack[sp - 1] = fns::pow(stack[sp - 1], stack[sp]);
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
            if FLAG {
                rounded |= scan & left_range(stack[sp - 1]);
            }
            if CHECK {
                let v = stack[sp - 1];
                match op {
                    // Exact.
                    Op::Const(_) | Op::X | Op::Y => {}
                    Op::Big(..) => return (v, true),
                    // Undefined, with nothing rounded before it: so is the
                    // result, as every operation keeps NaN.
                    _ if v.is_nan() => return (v, false),
                    _ if !v.is_normal()
                        && i != last
                        && (v.is_infinite() || op.may_leave_range()) =>
                    {
                        return (v, true);
                    }
                    _ => {}
                }
            }
        }
        (stack[0], rounded)
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
        self.eval_batch_tagged(xs, ys, out, |_, _| {});
    }

    /// [`Program::eval_batch`], calling `redone(i, how)` for each point `i`
    /// that needed re-evaluating in extended range (see
    /// [`Program::eval_tagged`]).
    pub(crate) fn eval_batch_tagged(
        &self,
        xs: Input<'_>,
        ys: Input<'_>,
        out: &mut [f64],
        mut redone: impl FnMut(usize, Redo),
    ) {
        if !self.wide {
            self.eval_batch_plain(xs, ys, out);
            return;
        }
        let n = out.len();
        if let Input::Slice(s) = xs {
            assert!(s.len() >= n, "x slice too short");
        }
        if let Input::Slice(s) = ys {
            assert!(s.len() >= n, "y slice too short");
        }
        let at = |i: usize, s: Input<'_>| match s {
            Input::Scalar(v) => v,
            Input::Slice(s) => s[i],
        };
        let mut stack = vec![[0.0f64; CHUNK]; self.max_stack.max(1)];
        let mut rounded = [false; CHUNK];
        let mut start = 0;
        while start < n {
            let len = CHUNK.min(n - start);
            rounded[..len].fill(false);
            self.eval_chunk::<true>(&mut stack, &mut rounded, xs, ys, start, len);
            out[start..start + len].copy_from_slice(&stack[0][..len]);
            for (r, v) in rounded[..len].iter_mut().zip(&out[start..start + len]) {
                *r |= !v.is_normal();
            }
            for k in (0..len).filter(|&k| rounded[k]) {
                let i = start + k;
                let (v, how) = self.redo(at(i, xs), at(i, ys));
                out[i] = v;
                if how != Redo::No {
                    redone(i, how);
                }
            }
            start += len;
        }
    }

    /// [`Program::eval_plain`] on many points.
    pub(crate) fn eval_batch_plain(&self, xs: Input<'_>, ys: Input<'_>, out: &mut [f64]) {
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
        let mut unused = [false; CHUNK];
        let mut start = 0;
        while start < n {
            let len = CHUNK.min(n - start);
            self.eval_chunk::<false>(&mut stack, &mut unused, xs, ys, start, len);
            out[start..start + len].copy_from_slice(&stack[0][..len]);
            start += len;
        }
    }

    /// One chunk of points; with `FLAG`, marks in `rounded` the points where
    /// an intermediate may have left a double's range (see `scan`).
    fn eval_chunk<const FLAG: bool>(
        &self,
        stack: &mut [[f64; CHUNK]],
        rounded: &mut [bool; CHUNK],
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
        for (i, op) in self.ops.iter().enumerate() {
            match *op {
                Op::Const(v) | Op::Big(v, _) => {
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
                Op::Add | Op::Sub | Op::Mul | Op::Div | Op::DivNz | Op::Pow | Op::F2(_) => {
                    sp -= 1;
                    let (lo, hi) = stack.split_at_mut(sp);
                    let a = &mut lo[sp - 1][..len];
                    let b = &hi[0][..len];
                    match *op {
                        Op::Add => a.iter_mut().zip(b).for_each(|(a, b)| *a += b),
                        Op::Sub => a.iter_mut().zip(b).for_each(|(a, b)| *a -= b),
                        Op::Mul => a.iter_mut().zip(b).for_each(|(a, b)| *a *= b),
                        Op::Div => a.iter_mut().zip(b).for_each(|(a, b)| *a = fns::div(*a, *b)),
                        Op::DivNz => a.iter_mut().zip(b).for_each(|(a, b)| *a /= b),
                        Op::Pow => a.iter_mut().zip(b).for_each(|(a, b)| *a = fns::pow(*a, *b)),
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
                        Fn1::Ln => s.iter_mut().for_each(|a| *a = fns::ln(*a)),
                        _ => s.iter_mut().for_each(|a| *a = f.apply(*a)),
                    }
                }
            }
            if FLAG && self.scan[i] {
                for (r, &v) in rounded[..len].iter_mut().zip(&stack[sp - 1][..len]) {
                    *r |= left_range(v);
                }
            }
        }
    }
}

/// How [`Program::eval_tagged`] came by a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Redo {
    /// In double arithmetic.
    No,
    /// Re-evaluated in extended range: the double result was NaN or ±∞.
    Wide,
    /// Re-evaluated, and even that couldn't tell whether the program is
    /// defined there (the sine of e^1000): NaN, but not known undefined.
    Unknown,
}

impl Program {
    /// Evaluates in extended range (see [`Wide`]).
    pub(crate) fn eval_wide(&self, x: f64, y: f64) -> Wide {
        if self.max_stack <= 8 {
            let mut stack = [Wide::Undef; 8];
            self.eval_wide_with(&mut stack, x, y)
        } else {
            let mut stack = vec![Wide::Undef; self.max_stack];
            self.eval_wide_with(&mut stack, x, y)
        }
    }

    fn eval_wide_with(&self, stack: &mut [Wide], x: f64, y: f64) -> Wide {
        let mut sp = 0usize;
        for op in &self.ops {
            let v = match *op {
                Op::Const(v) => Wide::new(v),
                Op::Big(_, w) => w,
                Op::X => Wide::new(x),
                Op::Y => Wide::new(y),
                Op::PowI(n) => wide::powi(stack[sp - 1], n),
                Op::PowRat(p, q) => wide::pow_rational(stack[sp - 1], p, q),
                Op::Neg => stack[sp - 1].neg(),
                Op::F1(f) => wide::apply1(f, stack[sp - 1]),
                _ => {
                    sp -= 1;
                    wide_bin(op, stack[sp - 1], stack[sp])
                }
            };
            // Undefined operands make the whole undefined.
            if v == Wide::Undef {
                return v;
            }
            if matches!(op, Op::Const(_) | Op::Big(..) | Op::X | Op::Y) {
                sp += 1;
            }
            stack[sp - 1] = v;
        }
        stack[0]
    }
}

/// A binary instruction on extended-range values.
fn wide_bin(op: &Op, a: Wide, b: Wide) -> Wide {
    match *op {
        Op::Add => a.add(b),
        Op::Sub => a.sub(b),
        Op::Mul => a.mul(b),
        // A never-zero divisor is never an exact 0 here, where nothing
        // underflows: plain division.
        Op::Div | Op::DivNz => a.div(b),
        Op::Pow => wide::pow(a, b),
        Op::F2(f) => wide::apply2(f, a, b),
        _ => unreachable!("not a binary instruction"),
    }
}

enum Piece {
    /// A folded constant, in extended range so that a value beyond a
    /// double's (e^−1000) is not taken for 0 or ∞. The flag records whether
    /// a parameter variable was involved (so that e.g. `x/a` with `a = 0` is
    /// not a syntax-level "divide by zero" error).
    Const(Wide, bool),
    Code(Vec<Op>),
}

/// The instruction pushing a folded constant.
fn const_op(w: Wide) -> Op {
    let v = w.to_f64();
    if Wide::new(v) == w {
        Op::Const(v)
    } else {
        Op::Big(v, w)
    }
}

impl Piece {
    fn into_code(self) -> Vec<Op> {
        match self {
            Piece::Const(w, _) => vec![const_op(w)],
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

/// Never exactly 0 where defined, so a 0 it evaluates to is underflow
/// (e^(1/x) just left of 0, (2e^x)² far left), not a zero.
fn never_zero(e: &Expr) -> bool {
    match e {
        Expr::Num(v) => *v != 0.0,
        Expr::Const(_) => true,
        Expr::Neg(a) | Expr::Bin(BinOp::Div | BinOp::Pow, a, _) => never_zero(a),
        Expr::Bin(BinOp::Mul, a, b) => never_zero(a) && never_zero(b),
        Expr::Call(Func::Exp, _) => true,
        _ => false,
    }
}

/// The value `e` takes wherever it is defined, when its form shows that it
/// is the same everywhere: 0·x, 0/x and x − x are 0, x/x and e^x/e^x are 1,
/// x·a is 0 for a = 0. Where an operand is undefined so is the whole (0·ln x
/// for x ≤ 0, x/x at 0): this says nothing about the domain.
pub(crate) fn constant_where_defined(e: &Expr, opts: &CompileOptions<'_>) -> Option<f64> {
    let w = constant_wide(e, opts)?;
    let v = w.to_f64();
    // Not a value beyond a double's range, which would round to 0 or ∞.
    (Wide::new(v) == w).then_some(v)
}

fn constant_wide(e: &Expr, opts: &CompileOptions<'_>) -> Option<Wide> {
    let c = |a: &Expr| constant_wide(a, opts);
    let value = |w: Wide| matches!(w, Wide::Val(..)).then_some(w);
    let zero = Wide::Val(0.0, 0.0);
    if !e.any(&|n| matches!(n, Expr::X | Expr::Y)) {
        return match lower(e, opts) {
            Ok(Piece::Const(w, _)) => value(w),
            _ => None,
        };
    }
    match e {
        Expr::Neg(a) => c(a).map(Wide::neg),
        Expr::Degrees(a) => c(a),
        // 0 times, or 0 divided by, anything defined is 0.
        Expr::Bin(BinOp::Mul, a, b) => {
            let (ca, cb) = (c(a), c(b));
            if ca.is_some_and(Wide::is_zero) || cb.is_some_and(Wide::is_zero) {
                return Some(zero);
            }
            value(ca?.mul(cb?))
        }
        Expr::Bin(BinOp::Div, a, b) => {
            let ca = c(a);
            if ca.is_some_and(Wide::is_zero) {
                return Some(zero);
            }
            if a == b {
                return Some(Wide::ONE);
            }
            // Two scalings of one expression, in proportion: ((x − 5)/3)/(x − 5)
            // is 1/3, (2x + 2)/(x + 1) is 2.
            if let (Some(sa), Some(sb)) = (Scaled::of(a, opts), Scaled::of(b, opts))
                && let Some(r) = sa.ratio(&sb)
            {
                return value(r);
            }
            value(ca?.div(c(b)?))
        }
        Expr::Bin(BinOp::Sub, a, b) if a == b => Some(zero),
        // Shifts of one expression that cancel: (x + 1) − x is 1.
        Expr::Bin(BinOp::Add | BinOp::Sub, ..)
            if let Some(s) = Scaled::of(e, opts)
                && s.k == 0.0 =>
        {
            value(Wide::new(s.c).div(Wide::new(s.d)))
        }
        Expr::Bin(op, a, b) => value(apply_bin(*op, c(a)?, c(b)?)),
        Expr::Call(f, args) => {
            let mut it = args.iter().map(c);
            let first = it.next()??;
            if let Some(f1) = fn1_for(*f, opts.trig_unit) {
                return value(wide::apply1(f1, first));
            }
            let f2 = fn2_for(*f)?;
            it.try_fold(first, |acc, w| value(wide::apply2(f2, acc, w?)))
        }
        _ => None,
    }
}

/// An expression as (k·b + c)/d: an expression b with x, scaled and shifted
/// by constants. Every constant is exact: an operation whose result a double
/// can't hold exactly isn't followed (division only multiplies d), so that
/// equal coefficients mean equal real numbers.
#[derive(Clone, Copy, Debug)]
struct Scaled<'e> {
    k: f64,
    b: &'e Expr,
    c: f64,
    d: f64,
}

/// a·b, if a double holds it exactly.
fn mul_exact(a: f64, b: f64) -> Option<f64> {
    let p = a * b;
    (p.is_finite() && a.mul_add(b, -p) == 0.0).then_some(p)
}

/// a + b, if a double holds it exactly.
fn add_exact(a: f64, b: f64) -> Option<f64> {
    let s = a + b;
    let bb = s - a;
    (s.is_finite() && (a - (s - bb)) + (b - bb) == 0.0).then_some(s)
}

impl<'e> Scaled<'e> {
    fn of(e: &'e Expr, opts: &CompileOptions<'_>) -> Option<Scaled<'e>> {
        // A constant that is exactly a double.
        let konst = |a: &Expr| -> Option<f64> {
            if a.any(&|n| matches!(n, Expr::X | Expr::Y)) {
                return None;
            }
            match lower(a, opts) {
                Ok(Piece::Const(w, _)) => {
                    let v = w.to_f64();
                    (v.is_finite() && Wide::new(v) == w).then_some(v)
                }
                _ => None,
            }
        };
        let whole = Scaled {
            k: 1.0,
            b: e,
            c: 0.0,
            d: 1.0,
        };
        if konst(e).is_some() {
            return None;
        }
        let s = match e {
            Expr::Neg(a) => {
                let s = Scaled::of(a, opts)?;
                Scaled {
                    k: -s.k,
                    c: -s.c,
                    ..s
                }
            }
            Expr::Bin(BinOp::Mul, a, b) => match (konst(a), konst(b)) {
                (Some(m), None) | (None, Some(m)) => {
                    let s = Scaled::of(if konst(a).is_some() { b } else { a }, opts)?;
                    Scaled {
                        k: mul_exact(s.k, m)?,
                        c: mul_exact(s.c, m)?,
                        ..s
                    }
                }
                _ => whole,
            },
            Expr::Bin(BinOp::Div, a, b) => match konst(b) {
                Some(m) if m != 0.0 => {
                    let s = Scaled::of(a, opts)?;
                    Scaled {
                        d: mul_exact(s.d, m)?,
                        ..s
                    }
                }
                _ => whole,
            },
            Expr::Bin(op @ (BinOp::Add | BinOp::Sub), a, b) => {
                let sign = if *op == BinOp::Sub { -1.0 } else { 1.0 };
                // A constant m is 0·b + m over 1.
                let lone = |m: f64, s: Scaled<'e>| Scaled {
                    k: 0.0,
                    c: m,
                    d: 1.0,
                    ..s
                };
                let (sa, sb) = match (konst(a), konst(b)) {
                    (Some(m), None) => {
                        let sb = Scaled::of(b, opts)?;
                        (lone(m, sb), sb)
                    }
                    (None, Some(m)) => {
                        let sa = Scaled::of(a, opts)?;
                        (sa, lone(m, sa))
                    }
                    _ => (Scaled::of(a, opts)?, Scaled::of(b, opts)?),
                };
                // (ka·b + ca)/da ± (kb·b + cb)/db over the common da·db.
                if sa.b != sb.b {
                    return Some(whole);
                }
                let (sb_k, sb_c) = (sign * sb.k, sign * sb.c);
                if sa.d == sb.d {
                    Scaled {
                        k: add_exact(sa.k, sb_k)?,
                        c: add_exact(sa.c, sb_c)?,
                        ..sa
                    }
                } else {
                    Scaled {
                        k: add_exact(mul_exact(sa.k, sb.d)?, mul_exact(sb_k, sa.d)?)?,
                        c: add_exact(mul_exact(sa.c, sb.d)?, mul_exact(sb_c, sa.d)?)?,
                        d: mul_exact(sa.d, sb.d)?,
                        b: sa.b,
                    }
                }
            }
            _ => whole,
        };
        Some(s)
    }

    /// self/other where both scale the same expression in proportion
    /// (k₁c₂ = k₂c₁, exactly): (k₁d₂)/(k₂d₁) wherever the divisor isn't 0.
    fn ratio(&self, o: &Scaled<'_>) -> Option<Wide> {
        if self.b != o.b || o.k == 0.0 {
            return None;
        }
        // Exact products as a double and its rounding error.
        let two = |a: f64, b: f64| {
            let p = a * b;
            (p, a.mul_add(b, -p))
        };
        if two(self.k, o.c) != two(o.k, self.c) {
            return None;
        }
        let w = |v: f64| Wide::new(v);
        Some(w(self.k).mul(w(o.d)).div(w(o.k).mul(w(self.d))))
    }
}

fn lower(e: &Expr, opts: &CompileOptions<'_>) -> Result<Piece, EquationError> {
    Ok(match e {
        Expr::Num(v) => Piece::Const(Wide::new(*v), false),
        Expr::Const(Constant::Pi) => Piece::Const(Wide::new(std::f64::consts::PI), false),
        Expr::Const(Constant::E) => Piece::Const(Wide::new(std::f64::consts::E), false),
        Expr::X => Piece::Code(vec![Op::X]),
        Expr::Y => Piece::Code(vec![Op::Y]),
        Expr::Var(n) => Piece::Const(
            Wide::new(opts.variables.value(n).unwrap_or(DEFAULT_VARIABLE_VALUE)),
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
            Piece::Const(v, var) => Piece::Const(v.neg(), var),
            Piece::Code(mut c) => {
                c.push(Op::Neg);
                Piece::Code(c)
            }
        },
        Expr::Bin(BinOp::Div, a, b)
            if let Expr::Bin(BinOp::Pow, base, k) = &**b
                && syntactic_rational(k).is_some_and(|(p, _)| p > 0)
                && !never_zero(base) =>
        {
            // a/b^k as a·b^(−k): the same where b = 0 (undefined), but where
            // b^k underflows (1/x^400 near 0) the power overflows to ∞
            // instead of a division by an underflowed 0.
            let neg = Expr::Neg(k.clone());
            let r = Expr::Bin(BinOp::Pow, base.clone(), Box::new(neg));
            lower(&Expr::Bin(BinOp::Mul, a.clone(), Box::new(r)), opts)?
        }
        Expr::Bin(op, a, b) => {
            let la = lower(a, opts)?;
            if *op == BinOp::Pow
                && let Some((p, q)) = syntactic_rational(b)
            {
                // A literal 0 to a negative power is a division by zero.
                if p < 0 && matches!(la, Piece::Const(v, false) if v.is_zero()) {
                    return Err(EquationError::eval(EvaluationErrorCode::DivideByZero, 0..0));
                }
                return Ok(match la {
                    Piece::Const(v, var) => Piece::Const(
                        if q == 1 {
                            wide::powi(v, p)
                        } else {
                            wide::pow_rational(v, p, q)
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
                    // Exact zeros only: e^−1000 is not 0, so 1/e^−1000 is
                    // e^1000, not a division by zero.
                    if *op == BinOp::Div && y.is_zero() && !vb {
                        return Err(EquationError::eval(EvaluationErrorCode::DivideByZero, 0..0));
                    }
                    if *op == BinOp::Pow && x.is_zero() && y.to_f64() < 0.0 && !va && !vb {
                        return Err(EquationError::eval(EvaluationErrorCode::DivideByZero, 0..0));
                    }
                    Piece::Const(apply_bin(*op, x, y), va || vb)
                }
                (la, lb) => {
                    if let (BinOp::Div, Piece::Const(y, false)) = (op, &lb)
                        && y.is_zero()
                    {
                        return Err(EquationError::eval(EvaluationErrorCode::DivideByZero, 0..0));
                    }
                    let mut c = la.into_code();
                    c.extend(lb.into_code());
                    c.push(match op {
                        BinOp::Add => Op::Add,
                        BinOp::Sub => Op::Sub,
                        BinOp::Mul => Op::Mul,
                        BinOp::Div if never_zero(b) => Op::DivNz,
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
                    Piece::Const(v, var) => Piece::Const(wide::apply1(f1, v), var),
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
                            Piece::Const(wide::apply2(f2, a, b), va || vb)
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

fn apply_bin(op: BinOp, a: Wide, b: Wide) -> Wide {
    match op {
        BinOp::Add => a.add(b),
        BinOp::Sub => a.sub(b),
        BinOp::Mul => a.mul(b),
        BinOp::Div => a.div(b),
        BinOp::Pow => wide::pow(a, b),
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
