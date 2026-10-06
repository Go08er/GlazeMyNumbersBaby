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
use graphing::ast::{BinOp, Constant, Expr, Func, Lit};

/// What the numbers of a tree stand for: each occurrence its own value,
/// as the parser's tree records it (`graphing::ast::Lit`), never looked up
/// by its double, which two decimals typed can share
/// (`1.0000000000000001` and `1`: review 13, R13-M-01). The replay reads
/// the source's literals itself too (each run of digits, rounded as the
/// binding says) and checks the tree's against them ([`Lits::check`]).
#[derive(Clone, Debug, Default)]
pub struct Lits {
    /// The decimals typed in the source, as the replay reads them.
    typed: Vec<String>,
    /// The decimal each slider stands for, where a digit limit rounded its
    /// value to one ([`Lits::with_sliders`]); any other slider is its
    /// double.
    sliders: Vec<(String, String)>,
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

/// The decimal `text` (digits and at most one `.`) rounded to `n`
/// significant digits, half away from zero, as the binding's digit limit
/// says each typed number was: worked in GMP rationals (q·10^(n−1−e)
/// rounded to an integer, 10^e ≤ q < 10^(e+1)), apart from the lexer's
/// digit-string rounding.
pub fn round_sig(text: &str, n: u8) -> String {
    use rug::{Integer, Rational};
    let t = match (text.starts_with('.'), text.ends_with('.')) {
        (true, _) => format!("0{text}"),
        (_, true) => format!("{text}0"),
        _ => text.to_string(),
    };
    let Some(q) = super::exact::decimal(&t) else {
        return text.to_string();
    };
    if q == 0 {
        return text.to_string();
    }
    let ten = |k: i64| -> Rational {
        let p = Rational::from(Integer::u_pow_u(10, k.unsigned_abs() as u32));
        if k < 0 { Rational::from(1) / p } else { p }
    };
    // The decimal exponent, from the text, then made exact.
    let (int, frac) = t.split_once('.').unwrap_or((&t, ""));
    let int = int.trim_start_matches('0');
    let mut e: i64 = if int.is_empty() {
        -((frac.len() - frac.trim_start_matches('0').len()) as i64) - 1
    } else {
        int.len() as i64 - 1
    };
    while q < ten(e) {
        e -= 1;
    }
    while q >= ten(e + 1) {
        e += 1;
    }
    let k = i64::from(n) - 1 - e;
    // `round` takes a tie away from zero.
    let m: Integer = (q * ten(k)).round().numer().clone();
    // m·10^(−k) as a decimal.
    let digits = m.to_string();
    if k <= 0 {
        return format!("{digits}{}", "0".repeat(k.unsigned_abs() as usize));
    }
    let k = k as usize;
    let padded = format!("{digits:0>width$}", width = k + 1);
    let (i, f) = padded.split_at(padded.len() - k);
    let f = f.trim_end_matches('0');
    if f.is_empty() {
        i.to_string()
    } else {
        format!("{i}.{f}")
    }
}

impl Lits {
    /// Every run of digits and `.` (and of superscript digits) in `text`.
    pub fn of(text: &str) -> Lits {
        Lits::of_with(text, false, None)
    }

    /// The same with `,` as the decimal separator (`comma`), each number
    /// rounded to `digits` significant digits when the binding says so.
    pub fn of_with(text: &str, comma: bool, digits: Option<u8>) -> Lits {
        let sep = if comma { ',' } else { '.' };
        let mut typed: Vec<String> = Vec::new();
        let mut add = |s: &str| {
            let s = &s.replace(sep, ".");
            let s = &match digits {
                Some(n) if s.chars().any(|c| c.is_ascii_digit()) => round_sig(s, n),
                _ => s.clone(),
            };
            if s.chars().any(|c| c.is_ascii_digit()) && !typed.contains(s) {
                typed.push(s.to_string());
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
            typed,
            sliders: Vec::new(),
            untyped: Default::default(),
        }
    }

    /// The sliders `vars` (as the binding gives their doubles), and the
    /// decimals some stand for (`decimals`), read under the digit limit
    /// `digits`, checked: under a limit each slider is its value rounded to
    /// so many significant digits, half away from zero (the replay rounds
    /// for itself: a decimal given must have no more digits and its double
    /// must be the slider's; a slider without one must be a double of so
    /// few digits); off, each is exactly its double.
    pub fn with_sliders(
        mut self,
        vars: &[(String, f64)],
        decimals: &[(String, String)],
        digits: Option<u8>,
    ) -> Result<Lits, String> {
        let Some(n) = digits else {
            if let Some((name, _)) = decimals.first() {
                return Err(format!(
                    "binding: slider {name} given a decimal without a digit limit"
                ));
            }
            return Ok(self);
        };
        for (name, v) in vars {
            match decimals.iter().find(|(m, _)| m == name) {
                Some((_, d)) => {
                    let q =
                        literal_value(d).ok_or_else(|| format!("binding: slider {name} = {d}"))?;
                    let mag = d.trim_start_matches('-');
                    if literal_value(&round_sig(mag, n)) != literal_value(mag)
                        || d.parse::<f64>().ok() != Some(*v)
                        || rug::Rational::from_f64(*v) == Some(q)
                    {
                        return Err(format!(
                            "binding: slider {name} = {d} is no {n}-digit decimal held as {v:e}"
                        ));
                    }
                }
                None => {
                    let exact = format!("{:.1074}", v.abs());
                    if v.is_finite()
                        && *v != 0.0
                        && literal_value(&round_sig(&exact, n)) != literal_value(&exact)
                    {
                        return Err(format!(
                            "binding: slider {name} = {v:e} is not rounded to {n} digits"
                        ));
                    }
                }
            }
        }
        self.sliders = decimals.to_vec();
        Ok(self)
    }

    /// The decimal the slider `name` stands for, if a digit limit made it
    /// one.
    pub fn slider(&self, name: &str) -> Option<&str> {
        self.sliders
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, d)| d.as_str())
    }

    /// The slider `name` exactly, from its double `v` in the binding.
    pub fn slider_exact(&self, name: &str, v: f64) -> Option<rug::Rational> {
        match self.slider(name) {
            Some(d) => literal_value(d),
            None => rug::Rational::from_f64(v),
        }
    }

    /// Checks the numbers of `e`, the parser's tree of the source, against
    /// the replay's own reading of its text: each decimal the tree records
    /// as typed was typed, and the double held is the one nearest it; and
    /// each decimal typed that its double doesn't hold is in the tree as
    /// typed (not taken for its double). Only whole numbers the parser
    /// writes itself (∜'s 4) are in it untyped.
    pub fn check(&self, e: &Expr) -> Result<(), String> {
        let same = |a: &str, b: &str| {
            let (a, b) = (literal_value(a), literal_value(b));
            a.is_some() && a == b
        };
        let mut bad: Option<String> = None;
        let mut seen: Vec<String> = Vec::new();
        e.visit(&mut |n| {
            if let Expr::Num(v, lit) = n {
                match lit {
                    Lit::Exact => {}
                    Lit::Decimal(d) => {
                        if !self.typed.iter().any(|t| same(t, d))
                            || d.parse::<f64>().ok() != Some(*v)
                        {
                            bad = Some(format!("the tree's {d} (held as {v:e}) isn't typed in it"));
                        }
                        seen.push(d.to_string());
                    }
                    other => {
                        bad = Some(format!(
                            "the tree has a number not typed: {v:e} ({other:?})"
                        ))
                    }
                }
            }
        });
        if let Some(b) = bad {
            return Err(format!("binding: {b}"));
        }
        for t in &self.typed {
            let Some(q) = literal_value(t) else { continue };
            let held = t.parse::<f64>().ok().and_then(rug::Rational::from_f64);
            if held.as_ref() != Some(&q) && !seen.iter().any(|s| same(s, t)) {
                return Err(format!(
                    "binding: {t}, typed, is in the parser's tree only as its double"
                ));
            }
        }
        Ok(())
    }

    /// The number `Num(v, lit)` stands for: the decimal typed, the double
    /// itself, or for a number not known exactly (only in a certifier's
    /// tree) anything rounding to `v`: a few ulps either way, and noted.
    pub fn num(&self, v: f64, lit: &Lit) -> Iv {
        match lit {
            Lit::Exact if v.is_finite() => Iv::of(v),
            Lit::Decimal(d) => match d.strip_prefix('-') {
                Some(p) => iv::neg(&iv::decimal(p)),
                None => iv::decimal(d),
            },
            _ => {
                self.untyped.set(self.untyped.get() + 1);
                let w = v.abs() * 2f64.powi(-48);
                Iv::of2(v - w, v + w)
            }
        }
    }

    /// The exact value of `Num(v, lit)`, when it has one.
    pub fn exact(&self, v: f64, lit: &Lit) -> Option<rug::Rational> {
        match lit {
            Lit::Exact => rug::Rational::from_f64(v),
            Lit::Decimal(d) => literal_value(d),
            _ => None,
        }
    }

    /// How many numbers were enclosed without a typed decimal.
    pub fn untyped(&self) -> u32 {
        self.untyped.get()
    }
}

/// A decimal (digits, at most one `.`, a `-` in front) exactly.
pub fn literal_value(text: &str) -> Option<rug::Rational> {
    let (neg, p) = match text.strip_prefix('-') {
        Some(p) => (true, p),
        None => (false, text),
    };
    let p = match (p.starts_with('.'), p.ends_with('.')) {
        (true, _) => format!("0{p}"),
        (_, true) => format!("{p}0"),
        _ => p.to_string(),
    };
    let q = super::exact::decimal(&p)?;
    Some(if neg { -q } else { q })
}

/// What a number written in a certificate's formula stands for: the
/// decimal written (`Expr::formula` writes each number so that it reads
/// back as itself), the double when that is it; a number written another
/// way (∞, an exponent) is only near its double.
fn written(t: &str, v: f64) -> Lit {
    if !v.is_finite() || t.contains(['e', 'E']) {
        return Lit::Near;
    }
    match literal_value(t) {
        Some(q) if rug::Rational::from_f64(v) == Some(q.clone()) => Lit::Exact,
        Some(_) => Lit::Decimal(t.into()),
        None => Lit::Near,
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
        Expr::Num(..) | Expr::Const(_) | Expr::Var(_) => false,
        Expr::Neg(a) | Expr::Degrees(a) => contains_x(a),
        Expr::Bin(_, a, b) => contains_x(a) || contains_x(b),
        Expr::Call(_, args) => args.iter().any(contains_x),
    }
}

/// An exponent written as an integer or a ratio of integers, in lowest
/// terms with a positive denominator. Each integer is one exactly as typed
/// (`lits`): `1.0000000000000001` is no integer, though its double is 1.
pub fn written_rational(e: &Expr, lits: &Lits) -> Option<(i64, i64)> {
    fn int(e: &Expr, lits: &Lits) -> Option<i64> {
        match e {
            Expr::Num(v, lit)
                if *v == v.trunc()
                    && v.abs() < 1e6
                    && lits.exact(*v, lit) == Some(rug::Rational::from(*v as i64)) =>
            {
                Some(*v as i64)
            }
            Expr::Neg(a) => int(a, lits).map(|v| -v),
            _ => None,
        }
    }
    let (p, q) = match e {
        Expr::Neg(a) => {
            let (p, q) = written_rational(a, lits)?;
            return Some((-p, q));
        }
        Expr::Bin(BinOp::Div, a, b) => (int(a, lits)?, int(b, lits)?),
        _ => (int(e, lits)?, 1),
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

/// The tree claims refer to: the parser's, with `a·a` written `a²` (each
/// number with its own value: two factors are the same only when their
/// numbers are, and the written 2 is exactly 2).
pub fn canonical(e: &Expr) -> Expr {
    match e {
        Expr::Num(..) | Expr::Const(_) | Expr::X | Expr::Y | Expr::Var(_) => e.clone(),
        Expr::Neg(a) => Expr::Neg(Box::new(canonical(a))),
        Expr::Degrees(a) => Expr::Degrees(Box::new(canonical(a))),
        Expr::Bin(op, a, b) => {
            let (a, b) = (canonical(a), canonical(b));
            if *op == BinOp::Mul && a == b {
                Expr::Bin(BinOp::Pow, Box::new(a), Box::new(Expr::exact(2.0)))
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
        let v = t.parse::<f64>().ok()?;
        return Some(Expr::Num(v, written(&t, v)));
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
            "inf" => Expr::Num(f64::INFINITY, Lit::Near),
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

/// A term of a sum as a monomial: its constant factor (an expression, its
/// numbers kept as typed) and the power of x.
fn monomial(e: &Expr, base: &Expr, lits: &Lits) -> Option<(Expr, u32)> {
    let one = || Expr::exact(1.0);
    Some(match e {
        _ if e == base => (one(), 1),
        _ if !contains_x(e) => (e.clone(), 0),
        Expr::Neg(a) => {
            let (c, k) = monomial(a, base, lits)?;
            (Expr::Neg(Box::new(c)), k)
        }
        Expr::Bin(BinOp::Mul, a, b) => {
            let ((c, k), (d, j)) = (monomial(a, base, lits)?, monomial(b, base, lits)?);
            (Expr::Bin(BinOp::Mul, Box::new(c), Box::new(d)), k + j)
        }
        Expr::Bin(BinOp::Div, a, b) if !contains_x(b) => {
            let (c, k) = monomial(a, base, lits)?;
            (Expr::Bin(BinOp::Div, Box::new(c), b.clone()), k)
        }
        Expr::Bin(BinOp::Pow, a, b) if **a == *base => {
            let (n, 1) = written_rational(b, lits)? else {
                return None;
            };
            (one(), u32::try_from(n).ok().filter(|n| *n <= 64)?)
        }
        _ => return None,
    })
}

/// The bases a sum's terms may be monomials in: x, and each base of a
/// whole power among them ((x − 1000)² + (x − 1000) + 1).
fn bases(e: &Expr, lits: &Lits) -> Vec<Expr> {
    let mut out = vec![Expr::X];
    e.visit(&mut |n| {
        if let Expr::Bin(BinOp::Pow, a, b) = n
            && contains_x(a)
            && written_rational(b, lits).is_some_and(|(_, q)| q == 1)
            && !out.contains(a)
        {
            out.push((**a).clone());
        }
    });
    out
}

/// The signed terms of a sum, a node equal to `base` kept whole.
fn terms(e: &Expr, base: &Expr, sign: bool, out: &mut Vec<(bool, Expr)>) {
    match e {
        Expr::Bin(op @ (BinOp::Add | BinOp::Sub), a, b) if e != base => {
            terms(a, base, sign, out);
            terms(b, base, sign == (*op == BinOp::Add), out);
        }
        _ => out.push((sign, e.clone())),
    }
}

/// `e` with each sum of monomials of degree ≥ 2 in Horner's form,
/// ((cₙ·x + cₙ₋₁)·x + …)·x + c₀, each cₖ the (signed) sum of the constant
/// factors of the terms in xᵏ. Only + and · rearranged: the same function
/// on the same domain. `None` if nothing changes.
pub fn horner(e: &Expr, lits: &Lits) -> Option<Expr> {
    fn go(e: &Expr, lits: &Lits) -> Expr {
        if matches!(e, Expr::Bin(BinOp::Add | BinOp::Sub, ..)) {
            let (ms, base) = bases(e, lits)
                .into_iter()
                .find_map(|b| {
                    let mut ts = Vec::new();
                    terms(e, &b, true, &mut ts);
                    let ms: Option<Vec<(bool, Expr, u32)>> = ts
                        .iter()
                        .map(|(s, t)| monomial(t, &b, lits).map(|(c, k)| (*s, c, k)))
                        .collect();
                    ms.filter(|m| m.iter().any(|t| t.2 >= 2)).map(|m| (m, b))
                })
                .map_or((None, Expr::X), |(m, b)| (Some(m), b));
            if let Some(ms) = ms
                && let Some(n) = ms.iter().map(|m| m.2).max()
                && n >= 2
                && ms.len() >= 2
            {
                let coef = |k: u32| -> Option<Expr> {
                    ms.iter().filter(|m| m.2 == k).fold(None, |acc, (s, c, _)| {
                        let c = c.clone();
                        Some(match acc {
                            None if *s => c,
                            None => Expr::Neg(Box::new(c)),
                            Some(a) => Expr::Bin(
                                if *s { BinOp::Add } else { BinOp::Sub },
                                Box::new(a),
                                Box::new(c),
                            ),
                        })
                    })
                };
                let mut acc = coef(n).unwrap_or(Expr::exact(0.0));
                for k in (0..n).rev() {
                    let ax = Expr::Bin(BinOp::Mul, Box::new(acc), Box::new(base.clone()));
                    acc = match coef(k) {
                        Some(c) => Expr::Bin(BinOp::Add, Box::new(ax), Box::new(c)),
                        None => ax,
                    };
                }
                return acc;
            }
        }
        match e {
            Expr::Neg(a) => Expr::Neg(Box::new(go(a, lits))),
            Expr::Degrees(a) => Expr::Degrees(Box::new(go(a, lits))),
            Expr::Bin(op, a, b) => Expr::Bin(*op, Box::new(go(a, lits)), Box::new(go(b, lits))),
            Expr::Call(f, args) => Expr::Call(*f, args.iter().map(|a| go(a, lits)).collect()),
            _ => e.clone(),
        }
    }
    let h = go(e, lits);
    (h != *e).then_some(h)
}

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
    pow_rat_int(a, p, &rug::Integer::from(q))
}

/// [`pow_rat`] for any q > 1, however long (a root's degree typed with
/// hundreds of digits: review 14, R14-M-04).
fn pow_rat_int(a: &S, p: i64, q: &rug::Integer) -> S {
    let n = se::order(a);
    let value = |a: &Iv| match q.to_i64() {
        Some(q) => iv::pow_rat(a, p, q),
        None => iv::pow_rat_big(a, p, q),
    };
    let v = value(&a[0]);
    if v.empty {
        return vec![Iv::empty(); n + 1];
    }
    // (p/q exactly, rounded outward: a degree past 2⁵³ is no double.)
    let e = iv::ratio(p, q);
    if a[0].gt(0.0) {
        se::pow_real(a, &e, v)
    } else if a[0].lt(0.0) && q.is_odd() {
        // (−a)^(p/q), negated for odd p.
        let na = se::neg(a);
        let s = se::pow_real(&na, &e, value(&na[0]));
        let s = if p % 2 != 0 { se::neg(&s) } else { s };
        se::with_value(s, v)
    } else {
        se::only_value_unless(se::constant(v, n), false)
    }
}

pub fn eval(e: &Expr, x: &S, n: usize, ctx: &Ctx<'_>) -> S {
    match e {
        Expr::Num(v, lit) => se::constant(ctx.lits.num(*v, lit), n),
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
        // ... any value in [−1, 1], any value ≥ 1 (supersets of the open
        // sets a node's argument keeps beside an edge of its domain).
        Expr::Var(name) if name == "__unit" => se::constant(Iv::of2(-1.0, 1.0), n),
        Expr::Var(name) if name == "__ge1" => se::constant(Iv::of2(1.0, f64::INFINITY), n),
        // A slider: its double, or the decimal a digit limit made it.
        Expr::Var(name) => match (
            ctx.lits.slider(name),
            ctx.vars.iter().find(|(v, _)| v == name),
        ) {
            (Some(d), _) => se::constant(
                match d.strip_prefix('-') {
                    Some(p) => iv::neg(&iv::decimal(p)),
                    None => iv::decimal(d),
                },
                n,
            ),
            (None, Some((_, v))) => se::constant(Iv::of(*v), n),
            (None, None) => se::constant(Iv::unknown(), n),
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
    if let Some((p, q)) = written_rational(b, ctx.lits) {
        return if q == 1 {
            se::powi(&ea, p)
        } else {
            pow_rat(&ea, p, q)
        };
    }
    // A constant exponent whose exact value is an integer (0.1 + 0.9) is
    // an integer power, however written; any other constant allows a
    // negative base only for an integer value, which the decimals typed
    // decide (1.0000000000000001 is none, though its double is 1).
    if !contains_x(b)
        && let Some(k) = super::exact::eval(b, None, ctx.lits, ctx.vars)
        && k.is_integer()
        && let Some(k) = k.numer().to_i64().filter(|k| k.unsigned_abs() < 1 << 20)
    {
        return se::powi(&ea, k);
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
            if contains_x(&args[1]) {
                return se::only_value_unless(se::constant(Iv::unknown(), n), false);
            }
            // The degree exactly, where it has an exact value (a typed
            // 9007199254740993 is odd, though its double is even; a typed
            // 3.0000000000000001 is no integer, though no binary point
            // holds it: review 13, R13-M-04); else its enclosure, when a
            // point.
            // Any size: a typed −(10³⁰¹ + 1) is odd (review 14, R14-M-04).
            let whole = match super::exact::eval(&args[1], None, ctx.lits, ctx.vars) {
                Some(q) if q.is_integer() => Some(Some(q.numer().clone())),
                Some(_) => None,
                None if k.is_point() && k.lo.is_integer() => Some(k.lo.to_integer()),
                None if k.is_point() => None,
                None => return se::only_value_unless(se::constant(Iv::unknown(), n), false),
            };
            if let Some(k) = whole {
                let Some(k) = k else {
                    return se::only_value_unless(se::constant(Iv::unknown(), n), false);
                };
                if k == 0 {
                    return vec![Iv::empty(); n + 1];
                }
                let (p, q) = if k > 0 { (1, k) } else { (-1, -k) };
                return if q == 1 {
                    se::powi(&ea, p)
                } else {
                    pow_rat_int(&ea, p, &q)
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
                    se::winner(acc, &eb)
                } else if pick_b && !lo_a.empty && !lo_b.empty {
                    se::winner(eb, &acc)
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
    Expr::Bin(BinOp::Div, Box::new(Expr::exact(1.0)), Box::new(a.clone()))
}
