//! Expression tree produced by the parser.

use std::fmt;

/// A built-in function. Covers everything on the graphing calculator's
/// numpad (trig / inverse / hyperbolic / inverse hyperbolic flyout, the
/// function flyout, roots, logarithms) plus a few extras.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Func {
    Sin,
    Cos,
    Tan,
    Sec,
    Csc,
    Cot,
    Asin,
    Acos,
    Atan,
    Asec,
    Acsc,
    Acot,
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
    /// Square root.
    Sqrt,
    /// Real cube root.
    Cbrt,
    /// `root(x, n)`: the n-th root of x (real root for odd n).
    Root,
    /// `log(x)`: base-10 logarithm.
    Log,
    /// `log(b, x)`: logarithm of x in base b (the numpad inserts `log(b, x)`).
    LogBase,
    /// Natural logarithm.
    Ln,
    Exp,
    Abs,
    Floor,
    /// `ceiling(x)` / `ceil(x)`.
    Ceil,
    Round,
    /// `sign(x)` / `sgn(x)`.
    Sign,
    /// `mod(a, b)` / `a mod b` (floored modulo, result has the sign of b).
    Mod,
    /// `n!` (extended to the reals through Γ(n+1)).
    Factorial,
    /// `n!!` (defined for integers n ≥ -1).
    DoubleFactorial,
    /// `nCr(n, r)`.
    NCr,
    /// `nPr(n, r)`.
    NPr,
    /// `min(a, b, ...)`.
    Min,
    /// `max(a, b, ...)`.
    Max,
}

impl Func {
    /// Canonical name, used when printing.
    pub fn name(self) -> &'static str {
        use Func::*;
        match self {
            Sin => "sin",
            Cos => "cos",
            Tan => "tan",
            Sec => "sec",
            Csc => "csc",
            Cot => "cot",
            Asin => "arcsin",
            Acos => "arccos",
            Atan => "arctan",
            Asec => "arcsec",
            Acsc => "arccsc",
            Acot => "arccot",
            Sinh => "sinh",
            Cosh => "cosh",
            Tanh => "tanh",
            Sech => "sech",
            Csch => "csch",
            Coth => "coth",
            Asinh => "arcsinh",
            Acosh => "arccosh",
            Atanh => "arctanh",
            Asech => "arcsech",
            Acsch => "arccsch",
            Acoth => "arccoth",
            Sqrt => "sqrt",
            Cbrt => "cbrt",
            Root => "root",
            Log => "log",
            LogBase => "log",
            Ln => "ln",
            Exp => "exp",
            Abs => "abs",
            Floor => "floor",
            Ceil => "ceiling",
            Round => "round",
            Sign => "sign",
            Mod => "mod",
            Factorial => "factorial",
            DoubleFactorial => "factorial2",
            NCr => "nCr",
            NPr => "nPr",
            Min => "min",
            Max => "max",
        }
    }

    /// Accepted number of arguments `(min, max)`.
    pub fn arity(self) -> (usize, usize) {
        use Func::*;
        match self {
            Root | LogBase | Mod | NCr | NPr => (2, 2),
            Min | Max => (1, usize::MAX),
            _ => (1, 1),
        }
    }

    /// True for sin/cos/tan/sec/csc/cot, whose *argument* is an angle in the
    /// current trig unit.
    pub fn takes_angle(self) -> bool {
        use Func::*;
        matches!(self, Sin | Cos | Tan | Sec | Csc | Cot)
    }

    /// True for the inverse trig functions, whose *result* is an angle in
    /// the current trig unit. Hyperbolic functions are unaffected by the
    /// trig unit, as in the original engine.
    pub fn returns_angle(self) -> bool {
        use Func::*;
        matches!(self, Asin | Acos | Atan | Asec | Acsc | Acot)
    }

    /// The inverse function used for the `f^-1(x)` / `f⁻¹(x)` notation.
    pub fn inverse(self) -> Option<Func> {
        use Func::*;
        Some(match self {
            Sin => Asin,
            Cos => Acos,
            Tan => Atan,
            Sec => Asec,
            Csc => Acsc,
            Cot => Acot,
            Sinh => Asinh,
            Cosh => Acosh,
            Tanh => Atanh,
            Sech => Asech,
            Csch => Acsch,
            Coth => Acoth,
            _ => return None,
        })
    }
}

/// Names recognised as functions (and their aliases). Longest match wins when
/// splitting a run of letters, so `sinh` beats `sin` and `exp` beats `e`.
pub(crate) const FUNCTION_NAMES: &[(&str, Func)] = &[
    ("sin", Func::Sin),
    ("cos", Func::Cos),
    ("tan", Func::Tan),
    ("tg", Func::Tan),
    ("sec", Func::Sec),
    ("csc", Func::Csc),
    ("cosec", Func::Csc),
    ("cot", Func::Cot),
    ("ctg", Func::Cot),
    ("arcsin", Func::Asin),
    ("asin", Func::Asin),
    ("arccos", Func::Acos),
    ("acos", Func::Acos),
    ("arctan", Func::Atan),
    ("atan", Func::Atan),
    ("arcsec", Func::Asec),
    ("asec", Func::Asec),
    ("arccsc", Func::Acsc),
    ("acsc", Func::Acsc),
    ("arccot", Func::Acot),
    ("acot", Func::Acot),
    ("sinh", Func::Sinh),
    ("cosh", Func::Cosh),
    ("tanh", Func::Tanh),
    ("sech", Func::Sech),
    ("csch", Func::Csch),
    ("coth", Func::Coth),
    ("arcsinh", Func::Asinh),
    ("asinh", Func::Asinh),
    ("arsinh", Func::Asinh),
    ("arccosh", Func::Acosh),
    ("acosh", Func::Acosh),
    ("arcosh", Func::Acosh),
    ("arctanh", Func::Atanh),
    ("atanh", Func::Atanh),
    ("artanh", Func::Atanh),
    ("arcsech", Func::Asech),
    ("asech", Func::Asech),
    ("arsech", Func::Asech),
    ("arccsch", Func::Acsch),
    ("acsch", Func::Acsch),
    ("arcsch", Func::Acsch),
    ("arccoth", Func::Acoth),
    ("acoth", Func::Acoth),
    ("arcoth", Func::Acoth),
    ("sqrt", Func::Sqrt),
    ("cbrt", Func::Cbrt),
    ("root", Func::Root),
    ("log", Func::Log),
    ("ln", Func::Ln),
    ("exp", Func::Exp),
    ("abs", Func::Abs),
    ("floor", Func::Floor),
    ("ceiling", Func::Ceil),
    ("ceil", Func::Ceil),
    ("round", Func::Round),
    ("sign", Func::Sign),
    ("sgn", Func::Sign),
    ("mod", Func::Mod),
    ("nCr", Func::NCr),
    ("nPr", Func::NPr),
    ("min", Func::Min),
    ("max", Func::Max),
];

/// A named mathematical constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Constant {
    /// π
    Pi,
    /// Euler's number e
    E,
}

impl Constant {
    /// Numeric value.
    pub fn value(self) -> f64 {
        match self {
            Constant::Pi => std::f64::consts::PI,
            Constant::E => std::f64::consts::E,
        }
    }
}

/// Binary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

/// An expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// A numeric literal.
    Num(f64),
    /// π or e.
    Const(Constant),
    /// The horizontal graph variable.
    X,
    /// The vertical graph variable.
    Y,
    /// Any other variable (a slider parameter), e.g. `a` or `k_1`.
    Var(String),
    /// Unary minus.
    Neg(Box<Expr>),
    /// Binary operation.
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// Function call.
    Call(Func, Vec<Expr>),
    /// `value°`: an angle given in degrees. Only valid in degrees mode, like
    /// the original (`RequireDegreesMode`).
    Degrees(Box<Expr>),
}

impl Expr {
    /// Convenience constructor for a binary node.
    pub fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
        Expr::Bin(op, Box::new(a), Box::new(b))
    }

    /// Convenience constructor for a single-argument call.
    pub fn call1(f: Func, a: Expr) -> Expr {
        Expr::Call(f, vec![a])
    }

    /// Depth (a leaf is 1) and number of nodes, computed without recursion
    /// so it is safe on trees of any shape.
    pub fn depth_and_size(&self) -> (usize, usize) {
        let mut stack = vec![(self, 1usize)];
        let (mut depth, mut size) = (0, 0);
        while let Some((e, d)) = stack.pop() {
            size += 1;
            depth = depth.max(d);
            match e {
                Expr::Num(_) | Expr::Const(_) | Expr::X | Expr::Y | Expr::Var(_) => {}
                Expr::Neg(a) | Expr::Degrees(a) => stack.push((a, d + 1)),
                Expr::Bin(_, a, b) => {
                    stack.push((a, d + 1));
                    stack.push((b, d + 1));
                }
                Expr::Call(_, args) => stack.extend(args.iter().map(|a| (a, d + 1))),
            }
        }
        (depth, size)
    }

    /// True if `x` occurs anywhere.
    pub fn contains_x(&self) -> bool {
        self.any(&|e| matches!(e, Expr::X))
    }

    /// True if `y` occurs anywhere.
    pub fn contains_y(&self) -> bool {
        self.any(&|e| matches!(e, Expr::Y))
    }

    /// True if the predicate holds for this node or any descendant.
    pub fn any(&self, pred: &dyn Fn(&Expr) -> bool) -> bool {
        if pred(self) {
            return true;
        }
        match self {
            Expr::Num(_) | Expr::Const(_) | Expr::X | Expr::Y | Expr::Var(_) => false,
            Expr::Neg(a) | Expr::Degrees(a) => a.any(pred),
            Expr::Bin(_, a, b) => a.any(pred) || b.any(pred),
            Expr::Call(_, args) => args.iter().any(|a| a.any(pred)),
        }
    }

    /// Visits every node (pre-order).
    pub fn visit(&self, f: &mut dyn FnMut(&Expr)) {
        f(self);
        match self {
            Expr::Num(_) | Expr::Const(_) | Expr::X | Expr::Y | Expr::Var(_) => {}
            Expr::Neg(a) | Expr::Degrees(a) => a.visit(f),
            Expr::Bin(_, a, b) => {
                a.visit(f);
                b.visit(f);
            }
            Expr::Call(_, args) => {
                for a in args {
                    a.visit(f);
                }
            }
        }
    }

    /// Names of the parameter variables used (excluding x and y), in order of
    /// first appearance, without duplicates.
    pub fn variables(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        self.visit(&mut |e| {
            if let Expr::Var(n) = e
                && !out.iter().any(|o| o == n)
            {
                out.push(n.clone());
            }
        });
        out
    }

    /// Returns a copy with `x` and `y` swapped (used to treat `x = g(y)`
    /// with the same machinery as `y = f(x)`).
    pub fn swap_xy(&self) -> Expr {
        self.map(&|e| match e {
            Expr::X => Some(Expr::Y),
            Expr::Y => Some(Expr::X),
            _ => None,
        })
    }

    /// Rebuilds the tree, replacing nodes for which `f` returns `Some`.
    pub fn map(&self, f: &dyn Fn(&Expr) -> Option<Expr>) -> Expr {
        if let Some(r) = f(self) {
            return r;
        }
        match self {
            Expr::Num(_) | Expr::Const(_) | Expr::X | Expr::Y | Expr::Var(_) => self.clone(),
            Expr::Neg(a) => Expr::Neg(Box::new(a.map(f))),
            Expr::Degrees(a) => Expr::Degrees(Box::new(a.map(f))),
            Expr::Bin(op, a, b) => Expr::Bin(*op, Box::new(a.map(f)), Box::new(b.map(f))),
            Expr::Call(func, args) => Expr::Call(*func, args.iter().map(|a| a.map(f)).collect()),
        }
    }

    /// Unambiguous prefix notation, e.g. `Mul(2,Pow(x,2))`. Handy for tests,
    /// in the spirit of the original engine's `Formula` format.
    pub fn formula(&self) -> String {
        match self {
            Expr::Num(v) => fmt_num(*v),
            Expr::Const(Constant::Pi) => "pi".into(),
            Expr::Const(Constant::E) => "e".into(),
            Expr::X => "x".into(),
            Expr::Y => "y".into(),
            Expr::Var(n) => n.clone(),
            Expr::Neg(a) => format!("Neg({})", a.formula()),
            Expr::Degrees(a) => format!("Deg({})", a.formula()),
            Expr::Bin(op, a, b) => {
                let name = match op {
                    BinOp::Add => "Add",
                    BinOp::Sub => "Sub",
                    BinOp::Mul => "Mul",
                    BinOp::Div => "Div",
                    BinOp::Pow => "Pow",
                };
                format!("{}({},{})", name, a.formula(), b.formula())
            }
            Expr::Call(func, args) => {
                let args: Vec<String> = args.iter().map(|a| a.formula()).collect();
                format!("{}({})", func.name(), args.join(","))
            }
        }
    }

    fn precedence(&self) -> u8 {
        match self {
            Expr::Bin(BinOp::Add | BinOp::Sub, ..) => 1,
            Expr::Bin(BinOp::Mul | BinOp::Div, ..) => 2,
            Expr::Neg(_) => 3,
            Expr::Bin(BinOp::Pow, ..) => 4,
            Expr::Num(v) if *v < 0.0 => 3,
            _ => 5,
        }
    }
}

fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Linear (ASCII) rendering with minimal parentheses and explicit `*`,
/// e.g. `2*x^2+sin(x)`. The output re-parses to the same tree.
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let wrap = |e: &Expr, min_prec: u8, f: &mut fmt::Formatter<'_>| -> fmt::Result {
            if e.precedence() < min_prec {
                write!(f, "({e})")
            } else {
                write!(f, "{e}")
            }
        };
        match self {
            Expr::Num(v) => write!(f, "{}", fmt_num(*v)),
            Expr::Const(Constant::Pi) => write!(f, "π"),
            Expr::Const(Constant::E) => write!(f, "e"),
            Expr::X => write!(f, "x"),
            Expr::Y => write!(f, "y"),
            Expr::Var(n) => write!(f, "{n}"),
            Expr::Neg(a) => {
                write!(f, "-")?;
                wrap(a, 4, f)
            }
            Expr::Degrees(a) => {
                wrap(a, 5, f)?;
                write!(f, "°")
            }
            Expr::Bin(op, a, b) => match op {
                BinOp::Add => {
                    wrap(a, 1, f)?;
                    write!(f, "+")?;
                    wrap(b, 2, f)
                }
                BinOp::Sub => {
                    wrap(a, 1, f)?;
                    write!(f, "-")?;
                    wrap(b, 2, f)
                }
                BinOp::Mul => {
                    wrap(a, 2, f)?;
                    write!(f, "*")?;
                    wrap(b, 3, f)
                }
                BinOp::Div => {
                    wrap(a, 2, f)?;
                    write!(f, "/")?;
                    wrap(b, 3, f)
                }
                BinOp::Pow => {
                    wrap(a, 5, f)?;
                    write!(f, "^")?;
                    wrap(b, 4, f)
                }
            },
            Expr::Call(func, args) => {
                let name = match func {
                    Func::Factorial => {
                        wrap(&args[0], 5, f)?;
                        return write!(f, "!");
                    }
                    Func::DoubleFactorial => {
                        wrap(&args[0], 5, f)?;
                        return write!(f, "!!");
                    }
                    other => other.name(),
                };
                write!(f, "{name}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{a}")?;
                }
                write!(f, ")")
            }
        }
    }
}
