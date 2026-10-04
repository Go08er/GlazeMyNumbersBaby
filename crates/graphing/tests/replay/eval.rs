//! Evaluating the graphing language over a box with the replay's own
//! series and intervals, from the language's specification:
//!
//! * literals are the decimals typed (read from the source text);
//! * powers: an exponent written as an integer or a ratio of integers
//!   (`3`, `-2`, `1/3`) is an integer power or a real root (a negative
//!   base only for an odd denominator, 0 only to a positive power); an
//!   exponent that varies with x takes the TI rule (a positive base, or 0
//!   to a positive power); any other constant exponent allows a negative
//!   base only for an integer value, and 0 only to a positive power;
//! * trigonometric functions take and give angles in the analysis' unit;
//!   arccot has range (0, π); arcsec = arccos(1/x), arccsc = arcsin(1/x);
//!   arsech = arcosh(1/x), arcsch = arsinh(1/x), arcoth = artanh(1/x);
//! * `log` is base 10, `log(b, x)` base b (b > 0, b ≠ 1), `root(x, n)` the
//!   real n-th root (odd n accepts negative x), `mod` floored (the sign of
//!   the divisor), `round` half away from zero, `sign(0)` = 0.
//!
//! Factorials, nCr, nPr and sliders are not modelled: anything built on
//! them is left unconfirmed.

use super::iv::{self, Iv, Unit};
use super::series::{self as se, S};
use graphing::ast::{BinOp, Constant, Expr, Func};
use std::collections::HashMap;

/// The typed literals of a source text, by the double each parses to.
#[derive(Clone, Debug, Default)]
pub struct Lits {
    by_bits: HashMap<u64, Vec<String>>,
    untyped: std::cell::Cell<u32>,
}

fn superscript_digit(c: char) -> Option<char> {
    Some(match c {
        '⁰' => '0',
        '¹' => '1',
        '²' => '2',
        '³' => '3',
        '⁴' => '4',
        '⁵' => '5',
        '⁶' => '6',
        '⁷' => '7',
        '⁸' => '8',
        '⁹' => '9',
        _ => return None,
    })
}

impl Lits {
    /// Every run of digits and `.` (and of superscript digits) in `text`.
    pub fn of(text: &str) -> Lits {
        Lits::of_with(text, false)
    }

    /// The same with `,` as the decimal separator (`comma`).
    pub fn of_with(text: &str, comma: bool) -> Lits {
        let sep = if comma { ',' } else { '.' };
        let mut by_bits: HashMap<u64, Vec<String>> = HashMap::new();
        let mut add = |s: &str| {
            let s = &s.replace(sep, ".");
            if s.chars().any(|c| c.is_ascii_digit())
                && let Ok(v) = s.parse::<f64>()
            {
                let e = by_bits.entry(v.to_bits()).or_default();
                if !e.iter().any(|t| t == s) {
                    e.push(s.to_string());
                }
            }
        };
        let mut cur = String::new();
        let mut sup = String::new();
        for c in text.chars() {
            if c.is_ascii_digit() || c == sep {
                cur.push(c);
            } else if !cur.is_empty() {
                add(&cur);
                cur.clear();
            }
            if let Some(d) = superscript_digit(c) {
                sup.push(d);
            } else if !sup.is_empty() {
                add(&sup);
                sup.clear();
            }
        }
        add(&cur);
        add(&sup);
        Lits {
            by_bits,
            untyped: Default::default(),
        }
    }

    /// The number a tree's `Num(v)` stands for: a typed decimal (the hull
    /// if several decimals parsed to `v`), its negation, an integer the
    /// parser wrote (exact below 2⁵³), or else anything rounding to `v`.
    pub fn num(&self, v: f64) -> Iv {
        let typed = |bits: u64| -> Option<Iv> {
            let texts = self.by_bits.get(&bits)?;
            let mut out: Option<Iv> = None;
            for t in texts {
                let d = iv::decimal(t);
                out = Some(match out {
                    None => d,
                    Some(o) => o.hull(&d),
                });
            }
            out
        };
        if let Some(i) = typed(v.to_bits()) {
            return i;
        }
        if let Some(i) = typed((-v).to_bits()) {
            return iv::neg(&i);
        }
        if v == v.trunc() && v.abs() <= 9007199254740992.0 {
            return Iv::of(v);
        }
        // A number nobody typed (only in a simplifier's tree): a few ulps
        // either way, and noted.
        self.untyped.set(self.untyped.get() + 1);
        let w = v.abs() * 2f64.powi(-48);
        Iv::of2(v - w, v + w)
    }

    /// The exact value of a tree's `Num(v)`, when one decimal (or an
    /// integer) stands for it.
    pub fn exact(&self, v: f64) -> Option<rug::Rational> {
        let one = |bits: u64| -> Option<Option<rug::Rational>> {
            let texts = self.by_bits.get(&bits)?;
            let qs: Vec<rug::Rational> = texts
                .iter()
                .filter_map(|t| super::exact::decimal(t))
                .collect();
            Some(
                if qs.len() == texts.len() && qs.windows(2).all(|w| w[0] == w[1]) {
                    qs.into_iter().next()
                } else {
                    None
                },
            )
        };
        if let Some(q) = one(v.to_bits()) {
            return q;
        }
        if let Some(q) = one((-v).to_bits()) {
            return q.map(|q| -q);
        }
        if v == v.trunc() && v.abs() <= 9007199254740992.0 {
            return rug::Rational::from_f64(v);
        }
        None
    }

    /// How many numbers were enclosed without a typed decimal.
    pub fn untyped(&self) -> u32 {
        self.untyped.get()
    }
}

/// What an evaluation needs besides the tree.
pub struct Ctx<'a> {
    pub unit: Unit,
    pub lits: &'a Lits,
    /// Slider values (the certificate's binding); others are unknown.
    pub vars: &'a [(String, f64)],
}

pub fn contains_x(e: &Expr) -> bool {
    match e {
        Expr::X | Expr::Y => true,
        Expr::Num(_) | Expr::Const(_) | Expr::Var(_) => false,
        Expr::Neg(a) | Expr::Degrees(a) => contains_x(a),
        Expr::Bin(_, a, b) => contains_x(a) || contains_x(b),
        Expr::Call(_, args) => args.iter().any(contains_x),
    }
}

/// An exponent written as an integer or a ratio of integers, in lowest
/// terms with a positive denominator.
pub fn written_rational(e: &Expr) -> Option<(i64, i64)> {
    fn int(e: &Expr) -> Option<i64> {
        match e {
            Expr::Num(v) if *v == v.trunc() && v.abs() < 1e6 => Some(*v as i64),
            Expr::Neg(a) => int(a).map(|v| -v),
            _ => None,
        }
    }
    let (p, q) = match e {
        Expr::Neg(a) => {
            let (p, q) = written_rational(a)?;
            return Some((-p, q));
        }
        Expr::Bin(BinOp::Div, a, b) => (int(a)?, int(b)?),
        _ => (int(e)?, 1),
    };
    if q == 0 {
        return None;
    }
    fn gcd(a: i64, b: i64) -> i64 {
        if b == 0 { a.abs() } else { gcd(b, a % b) }
    }
    let g = gcd(p, q).max(1);
    let (mut p, mut q) = (p / g, q / g);
    if q < 0 {
        p = -p;
        q = -q;
    }
    Some((p, q))
}

/// The tree claims refer to: the parser's, with `a·a` written `a²`.
pub fn canonical(e: &Expr) -> Expr {
    match e {
        Expr::Num(_) | Expr::Const(_) | Expr::X | Expr::Y | Expr::Var(_) => e.clone(),
        Expr::Neg(a) => Expr::Neg(Box::new(canonical(a))),
        Expr::Degrees(a) => Expr::Degrees(Box::new(canonical(a))),
        Expr::Bin(op, a, b) => {
            let (a, b) = (canonical(a), canonical(b));
            if *op == BinOp::Mul && a == b {
                Expr::Bin(BinOp::Pow, Box::new(a), Box::new(Expr::Num(2.0)))
            } else {
                Expr::Bin(*op, Box::new(a), Box::new(b))
            }
        }
        Expr::Call(f, args) => Expr::Call(*f, args.iter().map(canonical).collect()),
    }
}

/// The sub-tree at `path` (child indices from the root).
pub fn at_path<'e>(e: &'e Expr, path: &[u8]) -> Option<&'e Expr> {
    let Some((&i, rest)) = path.split_first() else {
        return Some(e);
    };
    let child = match e {
        Expr::Neg(a) | Expr::Degrees(a) if i == 0 => a,
        Expr::Bin(_, a, _) if i == 0 => a,
        Expr::Bin(_, _, b) if i == 1 => b,
        Expr::Call(_, args) => args.get(i as usize)?,
        _ => return None,
    };
    at_path(child, rest)
}

// ------------------------------------------------------------ formulas

fn func_named(name: &str, arity: usize) -> Option<Func> {
    use Func::*;
    Some(match name {
        "sin" => Sin,
        "cos" => Cos,
        "tan" => Tan,
        "sec" => Sec,
        "csc" => Csc,
        "cot" => Cot,
        "arcsin" => Asin,
        "arccos" => Acos,
        "arctan" => Atan,
        "arcsec" => Asec,
        "arccsc" => Acsc,
        "arccot" => Acot,
        "sinh" => Sinh,
        "cosh" => Cosh,
        "tanh" => Tanh,
        "sech" => Sech,
        "csch" => Csch,
        "coth" => Coth,
        "arcsinh" => Asinh,
        "arccosh" => Acosh,
        "arctanh" => Atanh,
        "arcsech" => Asech,
        "arccsch" => Acsch,
        "arccoth" => Acoth,
        "sqrt" => Sqrt,
        "cbrt" => Cbrt,
        "root" => Root,
        "log" if arity == 2 => LogBase,
        "log" => Log,
        "ln" => Ln,
        "exp" => Exp,
        "abs" => Abs,
        "floor" => Floor,
        "ceiling" => Ceil,
        "round" => Round,
        "sign" => Sign,
        "mod" => Mod,
        "factorial" => Factorial,
        "factorial2" => DoubleFactorial,
        "nCr" => NCr,
        "nPr" => NPr,
        "min" => Min,
        "max" => Max,
        _ => return None,
    })
}

/// Parses the prefix notation of `Expr::formula` (`Add(x,Mul(2,x))`).
pub fn parse_formula(s: &str) -> Option<Expr> {
    let cs: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut i = 0;
    let e = term(&cs, &mut i)?;
    (i == cs.len()).then_some(e)
}

fn term(cs: &[char], i: &mut usize) -> Option<Expr> {
    let start = *i;
    let c = *cs.get(*i)?;
    if c.is_ascii_digit() || c == '-' || c == '.' {
        while *i < cs.len() && (cs[*i].is_ascii_digit() || "-.eE+".contains(cs[*i])) {
            // A '-' or '+' only at the start or after an exponent mark.
            if (cs[*i] == '-' || cs[*i] == '+') && *i > start && !matches!(cs[*i - 1], 'e' | 'E') {
                break;
            }
            *i += 1;
        }
        let t: String = cs[start..*i].iter().collect();
        return t.parse::<f64>().ok().map(Expr::Num);
    }
    while *i < cs.len() && (cs[*i].is_alphanumeric() || cs[*i] == '_') {
        *i += 1;
    }
    let name: String = cs[start..*i].iter().collect();
    if name.is_empty() {
        return None;
    }
    if cs.get(*i) != Some(&'(') {
        return Some(match name.as_str() {
            "pi" => Expr::Const(Constant::Pi),
            "e" => Expr::Const(Constant::E),
            "x" => Expr::X,
            "y" => Expr::Y,
            "inf" => Expr::Num(f64::INFINITY),
            _ => Expr::Var(name),
        });
    }
    *i += 1;
    let mut args = Vec::new();
    loop {
        args.push(term(cs, i)?);
        match cs.get(*i)? {
            ',' => *i += 1,
            ')' => {
                *i += 1;
                break;
            }
            _ => return None,
        }
    }
    let bin = |op: BinOp, mut a: Vec<Expr>| -> Option<Expr> {
        if a.len() != 2 {
            return None;
        }
        let b = a.pop()?;
        let x = a.pop()?;
        Some(Expr::Bin(op, Box::new(x), Box::new(b)))
    };
    match name.as_str() {
        "Add" => bin(BinOp::Add, args),
        "Sub" => bin(BinOp::Sub, args),
        "Mul" => bin(BinOp::Mul, args),
        "Div" => bin(BinOp::Div, args),
        "Pow" => bin(BinOp::Pow, args),
        "Neg" if args.len() == 1 => Some(Expr::Neg(Box::new(args.pop()?))),
        "Deg" if args.len() == 1 => Some(Expr::Degrees(Box::new(args.pop()?))),
        _ => Some(Expr::Call(func_named(&name, args.len())?, args)),
    }
}

// ------------------------------------------------------------ evaluation

/// f's series over `[lo, hi]` (endpoints doubles, ±∞ allowed) to order n.
pub fn series(e: &Expr, lo: f64, hi: f64, n: usize, ctx: &Ctx<'_>) -> S {
    let x = se::var(Iv::of2(lo, hi), n);
    eval(e, &x, n, ctx)
}

fn sq(a: &S) -> S {
    se::powi(a, 2)
}

fn one(n: usize) -> S {
    se::constant(Iv::of(1.0), n)
}

/// The unit's series scaling: an angle given in the unit, in radians.
fn to_rad(a: &S, unit: Unit) -> S {
    if unit == Unit::Radians {
        a.clone()
    } else {
        se::scale(a, &unit.to_rad())
    }
}

fn from_rad(a: &S, unit: Unit) -> S {
    if unit == Unit::Radians {
        a.clone()
    } else {
        se::scale(a, &unit.per_rad())
    }
}

fn sin_cos(a: &S, unit: Unit) -> (S, S) {
    let s0 = iv::sin_cos(&a[0], unit, false);
    let c0 = iv::sin_cos(&a[0], unit, true);
    se::sin_cos(&to_rad(a, unit), s0, c0)
}

/// a^(p/q) (q > 1) with real-root semantics.
fn pow_rat(a: &S, p: i64, q: i64) -> S {
    let n = se::order(a);
    let value = iv::pow_rat(&a[0], p, q);
    if value.empty {
        return vec![Iv::empty(); n + 1];
    }
    let e = iv::div(&Iv::of(p as f64), &Iv::of(q as f64));
    if a[0].gt(0.0) {
        se::pow_real(a, &e, value)
    } else if a[0].lt(0.0) && q % 2 == 1 {
        // (−a)^(p/q), negated for odd p.
        let na = se::neg(a);
        let s = se::pow_real(&na, &e, iv::pow_rat(&na[0], p, q));
        let s = if p % 2 != 0 { se::neg(&s) } else { s };
        se::with_value(s, value)
    } else {
        se::only_value_unless(se::constant(value, n), false)
    }
}

pub fn eval(e: &Expr, x: &S, n: usize, ctx: &Ctx<'_>) -> S {
    match e {
        Expr::Num(v) => se::constant(ctx.lits.num(*v), n),
        Expr::Const(Constant::Pi) => se::constant(iv::pi(), n),
        Expr::Const(Constant::E) => se::constant(iv::e(), n),
        Expr::X => x.clone(),
        // Stand-ins for a node whose own condition is set aside (the
        // domain check): any real, any positive, any negative value.
        Expr::Var(name) if name == "__any" => se::constant(Iv::entire(), n),
        Expr::Var(name) if name == "__pos" => {
            se::constant(Iv::of2(0.0, f64::INFINITY).strict(true, false), n)
        }
        Expr::Var(name) if name == "__neg" => {
            se::constant(Iv::of2(f64::NEG_INFINITY, 0.0).strict(false, true), n)
        }
        Expr::Var(name) => match ctx.vars.iter().find(|(v, _)| v == name) {
            Some((_, v)) => se::constant(Iv::of(*v), n),
            None => se::constant(Iv::unknown(), n),
        },
        Expr::Y => se::constant(Iv::unknown(), n),
        Expr::Neg(a) => se::neg(&eval(a, x, n, ctx)),
        Expr::Degrees(a) => eval(a, x, n, ctx),
        Expr::Bin(op, a, b) => {
            if *op == BinOp::Pow {
                return pow(a, b, x, n, ctx);
            }
            let (ea, eb) = (eval(a, x, n, ctx), eval(b, x, n, ctx));
            match op {
                BinOp::Add => se::add(&ea, &eb),
                BinOp::Sub => se::sub(&ea, &eb),
                BinOp::Mul => se::mul(&ea, &eb),
                BinOp::Div => se::div(&ea, &eb),
                BinOp::Pow => unreachable!(),
            }
        }
        Expr::Call(f, args) => call(*f, args, x, n, ctx),
    }
}

fn pow(a: &Expr, b: &Expr, x: &S, n: usize, ctx: &Ctx<'_>) -> S {
    let ea = eval(a, x, n, ctx);
    if let Some((p, q)) = written_rational(b) {
        return if q == 1 {
            se::powi(&ea, p)
        } else {
            pow_rat(&ea, p, q)
        };
    }
    let eb = eval(b, x, n, ctx);
    if contains_x(b) {
        // The TI rule: a positive base, or 0 to a positive power.
        let value = iv::pow_var(&ea[0], &eb[0]);
        if ea[0].gt(0.0) {
            let s = se::exp(&se::mul(&eb, &se::ln(&ea)));
            let v = s[0].meet(&value);
            return se::with_value(s, v);
        }
        return se::only_value_unless(se::constant(value, n), false);
    }
    let e0 = &eb[0];
    if e0.is_point() && e0.lo.is_integer() && e0.lo.to_f64().abs() < 1e9 {
        return se::powi(&ea, e0.lo.to_f64() as i64);
    }
    let value = iv::pow_const(&ea[0], e0);
    if ea[0].gt(0.0) && e0.def {
        return se::pow_real(&ea, e0, value);
    }
    se::only_value_unless(se::constant(value, n), false)
}

fn call(f: Func, args: &[Expr], x: &S, n: usize, ctx: &Ctx<'_>) -> S {
    use Func::*;
    let unit = ctx.unit;
    let a = || eval(&args[0], x, n, ctx);
    match f {
        Sin => sin_cos(&a(), unit).0,
        Cos => sin_cos(&a(), unit).1,
        Tan => {
            let ea = a();
            let (s, c) = sin_cos(&ea, unit);
            let t = se::div(&s, &c);
            let direct = iv::tan(&ea[0], unit);
            if direct.empty {
                return vec![Iv::empty(); n + 1];
            }
            let v = t[0].meet(&direct);
            // Again from tan′ = 1 + tan², which keeps f′ ≥ 1 by a pole.
            let ode = se::tan_ode(&to_rad(&ea, unit), v.clone());
            let t = t.iter().zip(&ode).map(|(a, b)| a.meet(b)).collect();
            se::with_value(t, v)
        }
        Sec => se::recip(&sin_cos(&a(), unit).1),
        Csc => se::recip(&sin_cos(&a(), unit).0),
        Cot => {
            let (s, c) = sin_cos(&a(), unit);
            se::div(&c, &s)
        }
        Asin | Acos | Atan | Acot => {
            let ea = a();
            let (y0, d) = match f {
                Asin => (
                    iv::asin(&ea[0]),
                    se::recip(&se::sqrt(&se::sub(&one(n), &sq(&ea)))),
                ),
                Acos => (
                    iv::acos(&ea[0]),
                    se::neg(&se::recip(&se::sqrt(&se::sub(&one(n), &sq(&ea))))),
                ),
                Atan => (iv::atan(&ea[0]), se::recip(&se::add(&one(n), &sq(&ea)))),
                _ => (
                    iv::acot(&ea[0]),
                    se::neg(&se::recip(&se::add(&one(n), &sq(&ea)))),
                ),
            };
            let s = from_rad(&se::integrate(&ea, y0, &d), unit);
            // The value rounded in the unit itself (arctan(−∞) = −90°
            // exactly).
            let which = match f {
                Asin => 's',
                Acos => 'c',
                Atan => 't',
                _ => return s,
            };
            if unit == Unit::Radians {
                return s;
            }
            let v = s[0].meet(&iv::ainv_unit(&ea[0], unit, which));
            se::with_value(s, v)
        }
        Asec => {
            let inner = Expr::Call(Acos, vec![recip_expr(&args[0])]);
            eval(&inner, x, n, ctx)
        }
        Acsc => {
            let inner = Expr::Call(Asin, vec![recip_expr(&args[0])]);
            eval(&inner, x, n, ctx)
        }
        Sinh => se::sinh_cosh(&a()).0,
        Cosh => se::sinh_cosh(&a()).1,
        Tanh => {
            let ea = a();
            let (s, c) = se::sinh_cosh(&ea);
            let t = se::div(&s, &c);
            let v = t[0].meet(&iv::tanh(&ea[0]));
            se::with_value(t, v)
        }
        Sech => se::recip(&se::sinh_cosh(&a()).1),
        Csch => se::recip(&se::sinh_cosh(&a()).0),
        Coth => {
            // The value as 1/tanh: monotone, where cosh/sinh is ∞/∞ far out.
            let ea = a();
            let (s, c) = se::sinh_cosh(&ea);
            let t = se::div(&c, &s);
            let v = t[0].meet(&iv::recip(&iv::tanh(&ea[0])));
            se::with_value(t, v)
        }
        Asinh | Acosh | Atanh => {
            let ea = a();
            let (y0, d) = match f {
                Asinh => (
                    iv::asinh(&ea[0]),
                    se::recip(&se::sqrt(&se::add(&one(n), &sq(&ea)))),
                ),
                Acosh => (
                    iv::acosh(&ea[0]),
                    se::recip(&se::sqrt(&se::sub(&sq(&ea), &one(n)))),
                ),
                _ => (iv::atanh(&ea[0]), se::recip(&se::sub(&one(n), &sq(&ea)))),
            };
            se::integrate(&ea, y0, &d)
        }
        Asech => eval(&Expr::Call(Acosh, vec![recip_expr(&args[0])]), x, n, ctx),
        Acsch => eval(&Expr::Call(Asinh, vec![recip_expr(&args[0])]), x, n, ctx),
        Acoth => eval(&Expr::Call(Atanh, vec![recip_expr(&args[0])]), x, n, ctx),
        Sqrt => se::sqrt(&a()),
        Cbrt => pow_rat(&a(), 1, 3),
        Root => {
            let ea = a();
            let en = eval(&args[1], x, n, ctx);
            let k = &en[0];
            if contains_x(&args[1]) || !k.is_point() {
                return se::only_value_unless(se::constant(Iv::unknown(), n), false);
            }
            if k.lo.is_integer() {
                let k = k.lo.to_f64() as i64;
                if k == 0 {
                    return vec![Iv::empty(); n + 1];
                }
                let (p, q) = if k > 0 { (1, k) } else { (-1, -k) };
                return if q == 1 {
                    se::powi(&ea, p)
                } else {
                    pow_rat(&ea, p, q)
                };
            }
            // A non-integer degree: x ≥ 0, x^(1/n).
            let e = iv::recip(k);
            let value = iv::pow_const(&ea[0], &e);
            if ea[0].gt(0.0) {
                se::pow_real(&ea, &e, value)
            } else {
                se::only_value_unless(se::constant(value, n), false)
            }
        }
        Ln => se::ln(&a()),
        Log => {
            let ea = a();
            let ln10 = iv::ln(&Iv::of(10.0));
            let s = se::scale(&se::ln(&ea), &iv::recip(&ln10));
            let v = s[0].meet(&iv::log10(&ea[0]));
            se::with_value(s, v)
        }
        LogBase => {
            let eb = a();
            let ex = eval(&args[1], x, n, ctx);
            se::div(&se::ln(&ex), &se::ln(&eb))
        }
        Exp => se::exp(&a()),
        Abs => {
            let ea = a();
            if ea[0].gt(0.0) {
                ea
            } else if ea[0].lt(0.0) {
                se::neg(&ea)
            } else {
                let v = iv::abs(&ea[0]);
                se::only_value_unless(se::constant(v, n), false)
            }
        }
        Floor | Ceil => {
            let a0 = a()[0].clone();
            let v = if f == Floor {
                iv::floor(&a0)
            } else {
                iv::ceil(&a0)
            };
            se::step(v, n, iv::no_jump(&a0, 0.0))
        }
        Round => {
            let a0 = a()[0].clone();
            se::step(iv::round(&a0), n, iv::no_jump(&a0, 0.5))
        }
        Sign => {
            let a0 = a()[0].clone();
            let smooth = a0.ne0();
            se::step(iv::sign(&a0), n, smooth)
        }
        Mod => {
            let ea = a();
            let eb = eval(&args[1], x, n, ctx);
            let v = iv::modulo(&ea[0], &eb[0]);
            let ratio = iv::div(&ea[0], &eb[0]);
            let q = iv::floor(&ratio);
            if v.cont && q.cont && q.is_point() && iv::no_jump(&ratio, 0.0) {
                let k = se::constant(q, n);
                let s = se::sub(&ea, &se::mul(&eb, &k));
                let v2 = s[0].meet(&v);
                return se::with_value(s, v2);
            }
            se::only_value_unless(se::constant(v, n), false)
        }
        Min | Max => {
            let mut acc = eval(&args[0], x, n, ctx);
            for b in &args[1..] {
                let eb = eval(b, x, n, ctx);
                let (lo_a, lo_b) = (&acc[0], &eb[0]);
                let pick_a = if f == Min {
                    lo_a.hi < lo_b.lo
                } else {
                    lo_a.lo > lo_b.hi
                };
                let pick_b = if f == Min {
                    lo_b.hi < lo_a.lo
                } else {
                    lo_b.lo > lo_a.hi
                };
                acc = if pick_a && !lo_a.empty && !lo_b.empty {
                    acc
                } else if pick_b && !lo_a.empty && !lo_b.empty {
                    eb
                } else {
                    let v = if f == Min {
                        iv::min2(lo_a, lo_b)
                    } else {
                        iv::max2(lo_a, lo_b)
                    };
                    se::only_value_unless(se::constant(v, n), false)
                };
            }
            acc
        }
        Factorial | DoubleFactorial | NCr | NPr => {
            se::only_value_unless(se::constant(Iv::unknown(), n), false)
        }
    }
}

fn recip_expr(a: &Expr) -> Expr {
    Expr::Bin(BinOp::Div, Box::new(Expr::Num(1.0)), Box::new(a.clone()))
}
