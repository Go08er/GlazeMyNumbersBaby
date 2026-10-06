//! The simplifier's e-graph language, and conversion to and from the
//! parser's [`Expr`].
//!
//! Powers come in the evaluator's three meanings (`compile.rs`), kept
//! apart because a decimal and a fraction can be the same rational yet
//! mean different things (`(−32)^(1/5)` is −2, `(−32)^0.2` is undefined):
//!
//! * `^` — an exponent written as an integer or a ratio of integers
//!   (`syntactic_rational`): integer powers, real odd roots; `0^p` for
//!   `p ≤ 0` undefined.
//! * `^c` — any other constant exponent (`functions::pow`): a negative
//!   base only to an integer value; `0^e` for `e ≤ 0` undefined.
//! * `^v` — an exponent that varies with x (the TI rule,
//!   `functions::pow_var`): base > 0, or base 0 to a positive power.
//!
//! Numbers are exact rationals ([`Q`]): each occurrence's own exact value
//! (`ast::Lit`: `0.000001` is 1/1000000, and `1.0000000000000001` is not
//! the 1 beside it, though both are held as the double 1). A number not
//! known exactly (one an analysis wrote, `Lit::Near`) is a [`Real`], an
//! opaque constant enclosed by its double's neighbours; a number known
//! exactly that no `Q` holds (a decimal of 40 digits) is refused rather
//! than made opaque, where it could be taken for another number with the
//! same double. Angles are in the expression's unit (the e-graph analysis
//! knows which); `value°` is just the value, as degrees mode is the only
//! mode that accepts it.

use std::fmt;
use std::str::FromStr;

use egg::{Id, Language, RecExpr, Symbol, define_language};

use super::q::Q;
use crate::ast::{BinOp, Constant, Expr, Func, Lit};
use crate::compile::{Reading, Written, written};

/// A number not known exactly (`ast::Lit::Near`): the double an analysis
/// wrote, standing for some real within an ulp of it; and which occurrence
/// it is in the term. Two such numbers are never taken for one: their
/// doubles may agree while the reals differ (10¹⁶ + 1 and 10¹⁶, both held
/// as 10¹⁶), so x/x-style cancellation never applies between them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Real(pub u64, pub u32);

impl Real {
    /// The double.
    pub fn value(self) -> f64 {
        f64::from_bits(self.0)
    }
}

impl fmt::Display for Real {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "r{:x}.{}", self.0, self.1)
    }
}

impl FromStr for Real {
    type Err = ();
    fn from_str(s: &str) -> Result<Real, ()> {
        let h = s.strip_prefix('r').ok_or(())?;
        let (b, k) = h.split_once('.').ok_or(())?;
        Ok(Real(
            u64::from_str_radix(b, 16).map_err(|_| ())?,
            k.parse().map_err(|_| ())?,
        ))
    }
}

define_language! {
    /// Expressions in the e-graph.
    pub enum Math {
        "+" = Add([Id; 2]),
        "-" = Sub([Id; 2]),
        "*" = Mul([Id; 2]),
        "/" = Div([Id; 2]),
        "^" = PowQ([Id; 2]),
        "^c" = PowC([Id; 2]),
        "^v" = PowV([Id; 2]),
        "neg" = Neg(Id),
        "sin" = Sin(Id),
        "cos" = Cos(Id),
        "tan" = Tan(Id),
        "sec" = Sec(Id),
        "csc" = Csc(Id),
        "cot" = Cot(Id),
        "asin" = Asin(Id),
        "acos" = Acos(Id),
        "atan" = Atan(Id),
        "asec" = Asec(Id),
        "acsc" = Acsc(Id),
        "acot" = Acot(Id),
        "sinh" = Sinh(Id),
        "cosh" = Cosh(Id),
        "tanh" = Tanh(Id),
        "asinh" = Asinh(Id),
        "atanh" = Atanh(Id),
        "sqrt" = Sqrt(Id),
        "cbrt" = Cbrt(Id),
        "ln" = Ln(Id),
        "exp" = Exp(Id),
        "abs" = Abs(Id),
        Call(Tag, Vec<Id>),
        Num(Q),
        Real(Real),
        Symbol(Symbol),
    }
}

/// A function no rule mentions (floor, mod, nCr, min, sech, …): one node
/// kind for all of them keeps the e-graph code small.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Tag(pub Func);

impl Tag {
    /// The functions carried by [`Math::Call`] (every other one has its
    /// own node kind).
    pub const FUNCS: [Func; 21] = [
        Func::Sech,
        Func::Csch,
        Func::Coth,
        Func::Acosh,
        Func::Asech,
        Func::Acsch,
        Func::Acoth,
        Func::Root,
        Func::Log,
        Func::LogBase,
        Func::Floor,
        Func::Ceil,
        Func::Round,
        Func::Sign,
        Func::Mod,
        Func::Factorial,
        Func::DoubleFactorial,
        Func::NCr,
        Func::NPr,
        Func::Min,
        Func::Max,
    ];

    fn index(self) -> usize {
        Tag::FUNCS
            .iter()
            .position(|f| *f == self.0)
            .unwrap_or(usize::MAX)
    }
}

impl PartialOrd for Tag {
    fn partial_cmp(&self, o: &Tag) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Tag {
    fn cmp(&self, o: &Tag) -> std::cmp::Ordering {
        self.index().cmp(&o.index())
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{:?}", self.0)
    }
}

impl FromStr for Tag {
    type Err = ();
    fn from_str(s: &str) -> Result<Tag, ()> {
        let name = s.strip_prefix('@').ok_or(())?;
        Tag::FUNCS
            .iter()
            .find(|f| format!("{f:?}") == name)
            .map(|f| Tag(*f))
            .ok_or(())
    }
}

/// The symbol for the graph variable x.
pub const X: &str = "x";
/// The symbol for π.
pub const PI: &str = "pi";
/// The symbol for e.
pub const E: &str = "e";

impl Lit {
    /// The exact rational `Num(v, self)` stands for, if it is known and
    /// fits a [`Q`].
    pub fn q(&self, v: f64) -> Option<Q> {
        match self {
            Lit::Exact => Q::from_f64(v),
            Lit::Decimal(d) => match d.strip_prefix('-') {
                Some(p) => Q::from_decimal(p).and_then(Q::neg),
                None => Q::from_decimal(d),
            },
            Lit::Folded(_) | Lit::Near => None,
        }
    }
}

/// Why an expression can't be put in the e-graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// It uses y (a relation, not a function of x).
    UsesY,
    /// A number known exactly that no [`Q`] holds (a decimal of 40
    /// digits): as an opaque constant it could be taken for another number
    /// with the same double.
    LongNumber,
}

/// Converts `e` (a function of x) to an e-graph term.
pub fn to_rec(e: &Expr) -> Result<RecExpr<Math>, Unsupported> {
    let mut rec = RecExpr::default();
    add(e, &mut rec)?;
    Ok(rec)
}

fn add(e: &Expr, rec: &mut RecExpr<Math>) -> Result<Id, Unsupported> {
    let go = |e: &Expr, rec: &mut RecExpr<Math>| add(e, rec);
    Ok(match e {
        Expr::Num(v, lit) => match lit.q(*v) {
            Some(q) => rec.add(Math::Num(q)),
            // Only a number not known exactly is an opaque constant.
            // (Its place in the term tells occurrences apart.)
            None if *lit == Lit::Near => {
                let k = u32::try_from(rec.as_ref().len()).unwrap_or(u32::MAX);
                rec.add(Math::Real(Real(v.to_bits(), k)))
            }
            None => return Err(Unsupported::LongNumber),
        },
        Expr::Const(Constant::Pi) => rec.add(Math::Symbol(PI.into())),
        Expr::Const(Constant::E) => rec.add(Math::Symbol(E.into())),
        Expr::X => rec.add(Math::Symbol(X.into())),
        Expr::Y => return Err(Unsupported::UsesY),
        Expr::Var(n) => rec.add(Math::Symbol(format!("${n}").into())),
        Expr::Degrees(a) => go(a, rec)?,
        Expr::Neg(a) => {
            let a = go(a, rec)?;
            rec.add(Math::Neg(a))
        }
        Expr::Bin(op, a, b) => {
            let ia = go(a, rec)?;
            // Written as a ratio of integers, any size: a real root
            // (`^`), never `^c`'s positive-base rule (review 15, R15-M-01:
            // x^(1000001/3) was `^c`); past what a `Q` holds, not put in
            // the e-graph (its parities can't be read off a double).
            if *op == BinOp::Pow {
                match written(b, Reading::Typed) {
                    // (A whole number past 10⁶ too: `^c`'s interval of a
                    // power past i32 was the positive-base rule's.)
                    Some(Written::Ratio(r)) => {
                        let (p, q) = r.parts().ok_or(Unsupported::LongNumber)?;
                        let q = Q::new(p, q).ok_or(Unsupported::LongNumber)?;
                        let k = rec.add(Math::Num(q));
                        return Ok(rec.add(Math::PowQ([ia, k])));
                    }
                    Some(Written::Long) => return Err(Unsupported::LongNumber),
                    None => {}
                }
            }
            let ib = go(b, rec)?;
            rec.add(match op {
                BinOp::Add => Math::Add([ia, ib]),
                BinOp::Sub => Math::Sub([ia, ib]),
                BinOp::Mul => Math::Mul([ia, ib]),
                BinOp::Div => Math::Div([ia, ib]),
                BinOp::Pow if b.contains_x() => Math::PowV([ia, ib]),
                BinOp::Pow => Math::PowC([ia, ib]),
            })
        }
        Expr::Call(f, args) => {
            let mut ids = Vec::with_capacity(args.len());
            for a in args {
                ids.push(go(a, rec)?);
            }
            let one = |ids: &[Id]| ids[0];
            use Func::*;
            if Tag::FUNCS.contains(f) {
                return Ok(rec.add(Math::Call(Tag(*f), ids)));
            }
            rec.add(match f {
                Sin => Math::Sin(one(&ids)),
                Cos => Math::Cos(one(&ids)),
                Tan => Math::Tan(one(&ids)),
                Sec => Math::Sec(one(&ids)),
                Csc => Math::Csc(one(&ids)),
                Cot => Math::Cot(one(&ids)),
                Asin => Math::Asin(one(&ids)),
                Acos => Math::Acos(one(&ids)),
                Atan => Math::Atan(one(&ids)),
                Asec => Math::Asec(one(&ids)),
                Acsc => Math::Acsc(one(&ids)),
                Acot => Math::Acot(one(&ids)),
                Sinh => Math::Sinh(one(&ids)),
                Cosh => Math::Cosh(one(&ids)),
                Tanh => Math::Tanh(one(&ids)),
                Asinh => Math::Asinh(one(&ids)),
                Atanh => Math::Atanh(one(&ids)),
                Sqrt => Math::Sqrt(one(&ids)),
                Cbrt => Math::Cbrt(one(&ids)),
                Ln => Math::Ln(one(&ids)),
                Exp => Math::Exp(one(&ids)),
                Abs => Math::Abs(one(&ids)),
                other => Math::Call(Tag(*other), ids),
            })
        }
    })
}

/// Converts an e-graph term back to an [`Expr`] (the root is the last
/// node). Exact rationals become `p/q` (or an integer); powers keep their
/// meaning: `^` exponents come back as a syntactic rational.
pub fn from_rec(rec: &RecExpr<Math>) -> Expr {
    let nodes = rec.as_ref();
    build(nodes, Id::from(nodes.len() - 1))
}

fn num(q: Q) -> Expr {
    // (Each part exactly, past 2⁵³ too: `Expr::int`.)
    let int = |v: i128| -> Expr {
        let f = Expr::int(v.unsigned_abs());
        if v < 0 { Expr::Neg(Box::new(f)) } else { f }
    };
    if q.is_int() {
        return int(q.numer());
    }
    let d = Expr::int(q.denom().unsigned_abs());
    let n = Expr::int(q.numer().unsigned_abs());
    let r = Expr::bin(BinOp::Div, n, d);
    if q.numer() < 0 {
        Expr::Neg(Box::new(r))
    } else {
        r
    }
}

fn build(nodes: &[Math], id: Id) -> Expr {
    let b = |i: &Id| build(nodes, *i);
    let bin = |op: BinOp, [a, c]: &[Id; 2]| Expr::bin(op, build(nodes, *a), build(nodes, *c));
    let call =
        |f: Func, args: &[Id]| Expr::Call(f, args.iter().map(|i| build(nodes, *i)).collect());
    match &nodes[usize::from(id)] {
        // a + (−b) reads as a − b; −1·a as −a.
        Math::Add([a, c]) => match &nodes[usize::from(*c)] {
            Math::Neg(d) => Expr::bin(BinOp::Sub, b(a), b(d)),
            Math::Num(q) if q.signum() < 0 => match q.neg() {
                Some(m) => Expr::bin(BinOp::Sub, b(a), num(m)),
                None => Expr::bin(BinOp::Add, b(a), b(c)),
            },
            _ => Expr::bin(BinOp::Add, b(a), b(c)),
        },
        Math::Sub(ab) => bin(BinOp::Sub, ab),
        Math::Mul([a, c]) if matches!(&nodes[usize::from(*a)], Math::Num(q) if *q == Q::int(-1)) => {
            Expr::Neg(Box::new(b(c)))
        }
        Math::Mul(ab) => bin(BinOp::Mul, ab),
        Math::Div(ab) => bin(BinOp::Div, ab),
        Math::PowQ(ab) | Math::PowV(ab) => bin(BinOp::Pow, ab),
        Math::PowC([a, k]) => {
            // A `^c` exponent must not come back as a syntactic rational
            // (`compile.rs` would read (−32)^(1/5) as a real root, where
            // (−32)^0.2 is undefined): a non-integer rational is written as
            // the double it is, or kept apart from the fraction syntax.
            let exponent = match &nodes[usize::from(*k)] {
                Math::Num(q) if !q.is_int() => match q.exact_f64() {
                    Some(v) => Expr::exact(v),
                    None => Expr::bin(BinOp::Add, num(*q), Expr::exact(0.0)),
                },
                _ => b(k),
            };
            Expr::bin(BinOp::Pow, b(a), exponent)
        }
        Math::Neg(a) => Expr::Neg(Box::new(b(a))),
        Math::Sin(a) => call(Func::Sin, &[*a]),
        Math::Cos(a) => call(Func::Cos, &[*a]),
        Math::Tan(a) => call(Func::Tan, &[*a]),
        Math::Sec(a) => call(Func::Sec, &[*a]),
        Math::Csc(a) => call(Func::Csc, &[*a]),
        Math::Cot(a) => call(Func::Cot, &[*a]),
        Math::Asin(a) => call(Func::Asin, &[*a]),
        Math::Acos(a) => call(Func::Acos, &[*a]),
        Math::Atan(a) => call(Func::Atan, &[*a]),
        Math::Asec(a) => call(Func::Asec, &[*a]),
        Math::Acsc(a) => call(Func::Acsc, &[*a]),
        Math::Acot(a) => call(Func::Acot, &[*a]),
        Math::Sinh(a) => call(Func::Sinh, &[*a]),
        Math::Cosh(a) => call(Func::Cosh, &[*a]),
        Math::Tanh(a) => call(Func::Tanh, &[*a]),
        Math::Asinh(a) => call(Func::Asinh, &[*a]),
        Math::Atanh(a) => call(Func::Atanh, &[*a]),
        Math::Sqrt(a) => call(Func::Sqrt, &[*a]),
        Math::Cbrt(a) => call(Func::Cbrt, &[*a]),
        Math::Ln(a) => call(Func::Ln, &[*a]),
        Math::Exp(a) => call(Func::Exp, &[*a]),
        Math::Abs(a) => call(Func::Abs, &[*a]),
        Math::Call(t, args) => call(t.0, args),
        Math::Num(q) => num(*q),
        Math::Real(r) => {
            let v = r.value();
            if v < 0.0 {
                Expr::Neg(Box::new(Expr::Num(-v, Lit::Near)))
            } else {
                Expr::Num(v, Lit::Near)
            }
        }
        Math::Symbol(s) => match s.as_str() {
            X => Expr::X,
            PI => Expr::Const(Constant::Pi),
            E => Expr::Const(Constant::E),
            other => Expr::Var(other.trim_start_matches('$').to_string()),
        },
    }
}

/// The number of nodes of a term (a cheap size measure).
pub fn size(rec: &RecExpr<Math>) -> usize {
    fn go(nodes: &[Math], id: Id) -> usize {
        1 + nodes[usize::from(id)]
            .children()
            .iter()
            .map(|c| go(nodes, *c))
            .sum::<usize>()
    }
    let nodes = rec.as_ref();
    go(nodes, Id::from(nodes.len() - 1))
}
