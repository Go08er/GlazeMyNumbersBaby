//! The generator: random expressions biased toward the families the
//! review rounds 12–18 found bugs in, built from ordinary blocks
//! (polynomials, trig, exp/log, abs, floor, min/max, mod, sign) around the
//! risky atoms of each family, under a random number-precision setting,
//! with and without sliders. Deterministic: case `i` of seed `s` is a
//! function of `(s, i)` alone, whatever thread makes it.

use graphing::TrigUnit;

// ------------------------------------------------------------------ random

/// SplitMix64 (no dependency; one stream per case).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64, index: u64) -> Rng {
        let mut r = Rng(seed ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03);
        r.next();
        r.next();
        r
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (n > 0).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// True `num` times in `den`.
    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        self.next() % u64::from(den) < u64::from(num)
    }

    pub fn pick<T: Clone>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len())].clone()
    }

    /// The index of a weight chosen in proportion to it.
    pub fn weighted(&mut self, weights: &[u32]) -> usize {
        let total: u32 = weights.iter().sum();
        let mut r = (self.next() % u64::from(total)) as u32;
        for (i, w) in weights.iter().enumerate() {
            if r < *w {
                return i;
            }
            r -= w;
        }
        weights.len() - 1
    }
}

// -------------------------------------------------------------------- trees

/// An expression as typed: the generator's own tree, so that a failing
/// one can be shrunk node by node.
#[derive(Clone, Debug, PartialEq)]
pub enum T {
    X,
    /// A slider.
    Var(&'static str),
    /// A decimal as typed (digits and at most one point, no sign).
    Num(String),
    /// `pi` or `e`.
    Const(&'static str),
    /// Text kept as written: a risky atom whose spelling matters
    /// (`1000001/3`, `10^17*(0.1+0.2-0.3)`). In parentheses wherever it is
    /// an operand.
    Raw(String),
    Neg(Box<T>),
    /// `+ - * / ^`.
    Bin(char, Box<T>, Box<T>),
    Call(&'static str, Vec<T>),
    /// `u!`, or `u!!` (`true`).
    Bang(Box<T>, bool),
}

pub fn num(s: &str) -> T {
    T::Num(s.to_string())
}

pub fn raw(s: &str) -> T {
    T::Raw(s.to_string())
}

pub fn bin(op: char, a: T, b: T) -> T {
    T::Bin(op, Box::new(a), Box::new(b))
}

pub fn call(name: &'static str, args: Vec<T>) -> T {
    T::Call(name, args)
}

pub fn call1(name: &'static str, a: T) -> T {
    T::Call(name, vec![a])
}

pub fn neg(a: T) -> T {
    T::Neg(Box::new(a))
}

impl T {
    fn atom(&self) -> bool {
        matches!(
            self,
            T::X | T::Var(_) | T::Num(_) | T::Const(_) | T::Call(..)
        )
    }

    /// As an operand that must read as one unit.
    fn unit(&self) -> String {
        if self.atom() {
            self.text()
        } else {
            format!("({})", self.text())
        }
    }

    /// The text typed: every product explicit, every nested operation in
    /// parentheses unless its precedence already says so.
    pub fn text(&self) -> String {
        match self {
            T::X => "x".into(),
            T::Var(n) => (*n).into(),
            T::Num(s) => s.clone(),
            T::Const(c) => (*c).into(),
            T::Raw(s) => s.clone(),
            T::Neg(a) => format!("-{}", a.unit()),
            T::Bin(op, a, b) => {
                let sum = |t: &T| matches!(t, T::Bin('+' | '-', ..));
                let product = |t: &T| matches!(t, T::Bin('*' | '/', ..));
                let power = |t: &T| matches!(t, T::Bin('^', ..) | T::Bang(..));
                let bare = |t: &T, yes: bool| if yes { t.text() } else { t.unit() };
                let (l, r) = match op {
                    '+' | '-' => (
                        bare(a, sum(a) || product(a) || power(a)),
                        bare(b, product(b) || power(b)),
                    ),
                    '*' | '/' => (bare(a, product(a) || power(a)), bare(b, power(b))),
                    _ => (a.unit(), b.unit()),
                };
                format!("{l}{op}{r}")
            }
            T::Call(name, args) => {
                let args: Vec<String> = args.iter().map(T::text).collect();
                format!("{name}({})", args.join(","))
            }
            T::Bang(a, double) => {
                let a = match **a {
                    T::X | T::Var(_) | T::Num(_) => a.text(),
                    _ => format!("({})", a.text()),
                };
                format!("{a}{}", if *double { "!!" } else { "!" })
            }
        }
    }

    pub fn size(&self) -> usize {
        match self {
            T::Neg(a) | T::Bang(a, _) => 1 + a.size(),
            T::Bin(_, a, b) => 1 + a.size() + b.size(),
            T::Call(_, args) => 1 + args.iter().map(T::size).sum::<usize>(),
            T::Raw(s) => 1 + s.len() / 8,
            _ => 1,
        }
    }

    pub fn children(&self) -> Vec<&T> {
        match self {
            T::Neg(a) | T::Bang(a, _) => vec![a],
            T::Bin(_, a, b) => vec![a, b],
            T::Call(_, args) => args.iter().collect(),
            _ => Vec::new(),
        }
    }

    /// The tree with the node at `path` (child indices from the root)
    /// replaced by `with`.
    pub fn replaced(&self, path: &[usize], with: &T) -> T {
        let Some((&i, rest)) = path.split_first() else {
            return with.clone();
        };
        match self {
            T::Neg(a) => T::Neg(Box::new(a.replaced(rest, with))),
            T::Bang(a, d) => T::Bang(Box::new(a.replaced(rest, with)), *d),
            T::Bin(op, a, b) => {
                if i == 0 {
                    T::Bin(*op, Box::new(a.replaced(rest, with)), b.clone())
                } else {
                    T::Bin(*op, a.clone(), Box::new(b.replaced(rest, with)))
                }
            }
            T::Call(n, args) => {
                let mut args = args.clone();
                args[i] = args[i].replaced(rest, with);
                T::Call(n, args)
            }
            _ => self.clone(),
        }
    }

    /// Every node's path, the root first, parents before children.
    pub fn paths(&self) -> Vec<Vec<usize>> {
        fn go(t: &T, at: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
            out.push(at.clone());
            for (i, c) in t.children().into_iter().enumerate() {
                at.push(i);
                go(c, at, out);
                at.pop();
            }
        }
        let mut out = Vec::new();
        go(self, &mut Vec::new(), &mut out);
        out
    }

    pub fn at(&self, path: &[usize]) -> &T {
        match path.split_first() {
            None => self,
            Some((&i, rest)) => self.children()[i].at(rest),
        }
    }

    pub fn has_var(&self) -> bool {
        match self {
            T::Var(_) => true,
            T::Raw(s) => raw_vars(s).next().is_some(),
            _ => self.children().into_iter().any(T::has_var),
        }
    }
}

/// The sliders a raw text names (`a`, `b` standing alone).
fn raw_vars(s: &str) -> impl Iterator<Item = &'static str> + '_ {
    let cs = s.as_bytes();
    (0..cs.len()).filter_map(move |i| {
        let letter = |j: Option<&u8>| j.is_some_and(|c| c.is_ascii_alphabetic());
        let alone = !letter(i.checked_sub(1).and_then(|j| cs.get(j))) && !letter(cs.get(i + 1));
        match cs[i] {
            b'a' if alone => Some("a"),
            b'b' if alone => Some("b"),
            _ => None,
        }
    })
}

/// The numbers typed in `text`, as their digits (`12.5`, `0.1`, `3`).
pub fn literals(text: &str) -> Vec<String> {
    let cs: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        if cs[i].is_ascii_digit() {
            let s = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                i += 1;
            }
            out.push(cs[s..i].iter().collect());
        } else {
            i += 1;
        }
    }
    out
}

/// `text` with every number typed rounded to `digits` significant digits,
/// as the lexer rounds it on entry: the same function typed with nothing
/// left to round.
pub fn rounded_text(text: &str, digits: u8) -> String {
    let cs: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        if cs[i].is_ascii_digit() {
            let s = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                i += 1;
            }
            let lit: String = cs[s..i].iter().collect();
            out.push_str(&graphing::lexer::round_decimal(&lit, digits));
        } else {
            out.push(cs[i]);
            i += 1;
        }
    }
    out
}

/// Whether rounding to `digits` leaves every number of `text` as typed.
pub fn unrounded(text: &str, digits: Option<u8>) -> bool {
    digits.is_none_or(|d| {
        literals(text)
            .iter()
            .all(|l| graphing::lexer::round_decimal(l, d) == *l)
    })
}

/// Whether a slider set to `v` reads the same under a limit of `digits`
/// as with none: its exact decimal has no more significant digits.
pub fn slider_unrounded(v: f64, digits: Option<u8>) -> bool {
    digits.is_none_or(|d| {
        let exact = format!("{:.1074}", v.abs());
        let significant = exact
            .trim_end_matches('0')
            .chars()
            .filter(char::is_ascii_digit)
            .skip_while(|c| *c == '0')
            .count();
        significant <= usize::from(d)
    })
}

// -------------------------------------------------------------------- cases

/// The precision settings the apps offer that matter: the lowest, the
/// default (the TI-84 Plus CE's 14), the doubles' 15, the first that lets
/// two decimals share a double, the highest, and off.
pub const DIGITS: [Option<u8>; 6] = [Some(5), Some(14), Some(15), Some(16), Some(20), None];

#[derive(Clone, Debug)]
pub struct Case {
    pub index: u64,
    /// The family its risky atom is from.
    pub family: &'static str,
    pub expr: T,
    /// A spelling equal to `expr` everywhere (the same domain too).
    pub pair: Option<T>,
    pub digits: Option<u8>,
    pub sliders: Vec<(&'static str, f64)>,
    pub unit: TrigUnit,
}

impl Case {
    pub fn setting(&self) -> String {
        let digits = match self.digits {
            Some(d) => d.to_string(),
            None => "off".into(),
        };
        let sliders: Vec<String> = self
            .sliders
            .iter()
            .map(|(n, v)| format!("{n}={v:?}"))
            .collect();
        let unit = match self.unit {
            TrigUnit::Radians => "rad",
            TrigUnit::Degrees => "deg",
            TrigUnit::Grads => "grad",
        };
        format!(
            "digits={digits} unit={unit}{}{}",
            if sliders.is_empty() { "" } else { " sliders=" },
            sliders.join(",")
        )
    }
}

/// A sub-expression undefined for every x, which only exact reasoning
/// shows: the radicand is −10⁻ⁿ by sin² + cos² = 1, and its enclosure in
/// doubles straddles 0 (reviews 13, 15 and 17).
fn nowhere(r: &mut Rng) -> T {
    let c = r.pick(&["4", "1", "2", "7", "0.5", "10"]);
    let n = r.pick(&["30", "30", "100", "20", "40", "300"]);
    let radicand = format!("sin({c})^2+cos({c})^2-1-10^(-{n})");
    match r.below(4) {
        0 | 1 => raw(&format!("floor(sqrt({radicand}))")),
        2 => raw(&format!("ceil(sqrt({radicand}))")),
        _ => raw(&format!("sqrt({radicand})")),
    }
}

/// Decimals whose double is another number's, long ones, and integers
/// around the doubles' and the machine integers' limits.
const RISKY_NUMBERS: &[&str] = &[
    "1.0000000000000001",
    "0.99999999999999999",
    "2.0000000000000001",
    "1.00000000000000000001",
    "3.0000000000000000001",
    "0.30000000000000004",
    "0.1",
    "0.2",
    "0.3",
    "0.7",
    "1.5",
    "2.5",
    "0.5",
    "1.00001",
    "100000.5",
    "123456.7",
    "0.000012345678",
    "9007199254740993",
    "9007199254740992",
    "9007199254740991",
    "18014398509481985",
    "4294967299",
    "4294967296",
    "2147483649",
    "1000001",
    "9223372036854775807",
    "9223372036854775809",
    "18446744073709551617",
    "100000000000000000001",
    "340282366920938463463374607431768211457",
];

/// Arithmetic on literals alone, exact by the convention: its value in
/// the comment is what a hand folds it to.
const LITERAL_ARITHMETIC: &[(&str, &str)] = &[
    ("10^17*(0.1+0.2-0.3)", "0"),
    ("10^16*(1.0000000000000001-1)", "1"),
    ("10^(-16)/(1.0000000000000001-1)", "1"),
    ("0.1*3", "0.3"),
    ("0.1+0.2", "0.3"),
    ("0.1+0.9", "1"),
    ("0.5*2", "1"),
    ("1.5+1.5", "3"),
    ("0.25+0.5", "0.75"),
    ("6/4", "1.5"),
    ("3/2", "1.5"),
    ("1/4", "0.25"),
    ("2^10", "1024"),
    ("10^3/8", "125"),
    ("nCr(9007199254740993,1)-9007199254740992", "1"),
    ("nCr(10,3)", "120"),
    ("nPr(10,3)", "720"),
    ("5!", "120"),
    ("7!!", "105"),
    ("171!/170!", "171"),
    ("nCr(61,62)+2", "2"),
    ("(2^60+1)-2^60", "1"),
    ("abs(0.1-0.3)*5", "1"),
    ("1.0000000000000001*10^16-10^16", "1"),
    ("0.3-0.1*3+2", "2"),
];

/// Exponents: integers of any size, ratios written and not, decimals
/// that are and aren't integers.
const EXPONENTS: &[&str] = &[
    "2",
    "3",
    "-1",
    "-2",
    "0",
    "1",
    "1/3",
    "2/3",
    "-2/5",
    "1/2",
    "3/2",
    "6/4",
    "4/6",
    "3/1",
    "0/3",
    "1000001/3",
    "1/1000001",
    "1000001/3000003",
    "2000002/3",
    "1/1000002",
    "1/9007199254740993",
    "9007199254740993/3",
    "2/9007199254740994",
    "9007199254740993",
    "9007199254740992",
    "-9007199254740993",
    "1000001",
    "2000000",
    "-1000002",
    "123457",
    "4294967299",
    "18446744073709551617",
    "9223372036854775807",
    "0.2",
    "0.5",
    "2.0",
    "1.0000000000000001",
    "2.0000000000000001",
    "0.1+0.9",
    "0.5*2",
    "1.5/0.5",
    "0.1*3",
    "pi",
    "3^40",
    "2^70+1",
];

/// Root degrees: odd, even, huge, negative, zero, no integer.
const DEGREES: &[&str] = &[
    "2",
    "3",
    "4",
    "5",
    "1",
    "0",
    "-1",
    "-2",
    "-3",
    "2.5",
    "0.5",
    "1.0000000000000001",
    "3.0000000000000000001",
    "9007199254740993",
    "9007199254740992",
    "-9007199254740993",
    "4294967299",
    "4294967298",
    "18446744073709551617",
    "1000000000000000000000000000001",
    "1000000000000000000000000000002",
    "-1000000000000000000000000000001",
    "0.1+0.9+2",
    "6/2",
    "7/2",
];

/// Counts of whole numbers at the extremes, as constants.
const COUNTS: &[&str] = &[
    "nCr(61,62)",
    "nPr(61,62)",
    "99!!",
    "170!",
    "171!",
    "171!/170!",
    "(-1)!!",
    "(-2)!!",
    "(-3)!",
    "0!",
    "0.5!",
    "(-0.5)!",
    "1.5!!",
    "nCr(2^1000,2)/(2^1000*(2^1000-1)/2)",
    "nCr(1000,500)",
    "nCr(200,100)/10^58",
    "nPr(20,-1)",
    "nCr(-5,2)",
    "nCr(5,-2)",
    "nCr(5.5,2)",
    "nCr(5,2.5)",
    "nCr(9007199254740993,1)",
    "nPr(9007199254740993,1)",
    "nCr(9007199254740993,9007199254740992)",
    "nCr(9007199254740993,2)/9007199254740993",
    "ceil((99!!-6625061298371663*2^208)/10^70)",
    "nCr(0,0)",
    "nPr(0,0)",
    "20!/18!",
    "25!!",
    "nCr(100,3)",
];

/// Constants no double holds and a hair apart: values and bounds that
/// read alike to six digits.
const NEAR: &[(&str, &str)] = &[
    ("sin(1)", "0.0000001"),
    ("cos(2)", "0.0000001"),
    ("sqrt(2)", "0.000001"),
    ("pi", "0.00000001"),
    ("e", "0.0000001"),
    ("ln(3)", "0.000001"),
    ("1/3", "0.0000001"),
];

/// Slider values: whole, short decimals that are and aren't doubles, a
/// double a digit limit rounds, and large.
const SLIDER_VALUES: &[f64] = &[
    1.0,
    2.0,
    3.0,
    -1.0,
    0.5,
    -2.5,
    0.3,
    0.1,
    0.30000000000000004,
    1.0000000000000002,
    0.0,
    1e16,
    0.3333333333333333,
    123456.7,
];

struct Gen<'r> {
    r: &'r mut Rng,
    sliders: bool,
}

impl Gen<'_> {
    fn small(&mut self) -> T {
        num(&(1 + self.r.below(9)).to_string())
    }

    fn decimal(&mut self) -> T {
        num(self.r.pick(&[
            "0.5", "1.5", "0.25", "0.1", "0.2", "0.3", "2.5", "10", "100", "0.01",
        ]))
    }

    fn leaf(&mut self) -> T {
        match self
            .r
            .weighted(&[50, 16, 8, 5, 8, if self.sliders { 30 } else { 0 }])
        {
            0 => T::X,
            1 => self.small(),
            2 => self.decimal(),
            3 => T::Const(self.r.pick(&["pi", "e"])),
            4 => num(self.r.pick(RISKY_NUMBERS)),
            _ => T::Var(self.r.pick(&["a", "b"])),
        }
    }

    /// A polynomial in x with small coefficients.
    fn poly(&mut self) -> T {
        let mut t = match self.r.below(3) {
            0 => T::X,
            1 => bin('^', T::X, num(&(2 + self.r.below(3)).to_string())),
            _ => bin('*', self.small(), T::X),
        };
        for _ in 0..self.r.below(3) {
            let term = match self.r.below(4) {
                0 => self.small(),
                1 => bin('*', self.small(), T::X),
                2 => bin('^', T::X, num(&(2 + self.r.below(4)).to_string())),
                _ => self.decimal(),
            };
            t = bin(self.r.pick(&['+', '-']), t, term);
        }
        t
    }

    /// An ordinary expression of x, `depth` operations deep at most.
    fn ordinary(&mut self, depth: usize) -> T {
        if depth == 0 {
            return self.leaf();
        }
        match self.r.weighted(&[22, 14, 26, 18, 8, 6, 6]) {
            0 => self.leaf(),
            1 => self.poly(),
            2 => {
                let f = self.r.pick(&[
                    "sin", "cos", "tan", "exp", "ln", "sqrt", "abs", "floor", "ceil", "round",
                    "sign", "atan", "sinh", "cosh", "tanh", "asin", "acos", "cbrt", "log", "sec",
                    "cot",
                ]);
                call1(f, self.ordinary(depth - 1))
            }
            3 => {
                let op = self.r.pick(&['+', '-', '*', '/', '+', '*']);
                bin(op, self.ordinary(depth - 1), self.ordinary(depth - 1))
            }
            4 => {
                let e = match self.r.below(3) {
                    0 => num(&(2 + self.r.below(4)).to_string()),
                    1 => raw(self.r.pick(&["1/2", "1/3", "2/3", "-1", "-2", "3/2"])),
                    _ => self.leaf(),
                };
                bin('^', self.ordinary(depth - 1), e)
            }
            5 => {
                let f = self.r.pick(&["min", "max", "mod"]);
                call(f, vec![self.ordinary(depth - 1), self.ordinary(depth - 1)])
            }
            _ => neg(self.ordinary(depth - 1)),
        }
    }

    /// A term undefined somewhere: at points, on a half-line, off the
    /// integers, or on a set no interval shows.
    fn partial(&mut self) -> T {
        let c = self.small();
        match self.r.below(16) {
            0 => call1("sqrt", bin('-', T::X, c)),
            1 => call1("ln", T::X),
            2 => bin('/', num("1"), T::X),
            3 => call1("sqrt", call1("sin", T::X)),
            4 => call1("asin", T::X),
            5 => bin('^', T::X, T::X),
            6 => bin('^', T::X, num("0.5")),
            7 => bin('^', bin('-', T::X, c), num("0")),
            8 => bin('^', call1("floor", T::X), num("0")),
            9 => call("root", vec![raw("-1"), raw("3+x^4")]),
            10 => bin('/', num("1"), call1("floor", T::X)),
            11 => call1("tan", T::X),
            12 => T::Bang(Box::new(T::X), false),
            13 => call1("ln", bin('-', c, bin('^', T::X, num("2")))),
            14 => call1("acosh", T::X),
            _ => call("log", vec![T::X, num("2")]),
        }
    }

    /// `u` (possibly undefined somewhere, or everywhere) inside a
    /// shortcut that could lose that: a power or root read off as a
    /// constant, a min or max that picks the other argument, a step, a
    /// product with 0.
    fn shortcut(&mut self, u: T, v: T) -> T {
        match self.r.below(22) {
            0 => bin('^', u, num("0")),
            1 => bin('*', num("0"), u),
            2 => bin('-', u.clone(), u),
            3 => bin('/', u.clone(), u),
            4 => call("min", vec![v, bin('+', u, self.small())]),
            5 => call("max", vec![v, bin('-', u, self.small())]),
            6 => call("nCr", vec![u, num("0")]),
            7 => call("nPr", vec![u, num("0")]),
            8 => call1("floor", u),
            9 => call1("sign", u),
            10 => call1("abs", u),
            11 => bin('^', u, num("2")),
            12 => bin('^', v, bin('*', num("0"), u)),
            13 => call("mod", vec![v, u]),
            14 => call("root", vec![v, bin('+', num("1"), bin('*', num("0"), u))]),
            15 => bin('+', v, bin('*', num("0"), u)),
            16 => bin('^', u, num("1")),
            17 => bin('^', v, u),
            18 => call("root", vec![v, bin('+', self.small(), u)]),
            19 => call("min", vec![u, v]),
            20 => call1("exp", bin('*', num("0"), call1("ln", u))),
            _ => bin('+', u, v),
        }
    }

    fn literal(&mut self) -> T {
        let u = self.ordinary(1);
        match self.r.below(14) {
            0 | 1 => bin('^', u, raw(self.r.pick(EXPONENTS))),
            2 => {
                // Two numbers of one double, or a number and its neighbour.
                let (p, q) = self.r.pick(&[
                    ("1.0000000000000001", "1"),
                    ("2.0000000000000001", "2"),
                    ("0.99999999999999999", "1"),
                    ("0.30000000000000004", "0.3"),
                    ("9007199254740993", "9007199254740992"),
                    ("0.1", "0.1"),
                    ("1.00000000000000000001", "1.0000000000000001"),
                ]);
                bin('-', bin('*', num(p), u.clone()), bin('*', num(q), u))
            }
            3 | 4 => {
                let (a, _) = self.r.pick(LITERAL_ARITHMETIC);
                let op = self.r.pick(&['+', '*', '-']);
                bin(op, raw(a), u)
            }
            5 => {
                let l = self
                    .r
                    .pick(&["1.0000000000000001", "0.99999999999999999", "1", "1.5"]);
                bin('/', self.small(), bin('-', num(l), call1("cos", u)))
            }
            6 => call("root", vec![u, raw(self.r.pick(DEGREES))]),
            7 => {
                // A written ratio p/q of any size, maybe not in lowest terms.
                let p = self
                    .r
                    .pick(&[1u64, 2, 3, 5, 7, 1000001, 9007199254740993, 4294967299]);
                let q = self
                    .r
                    .pick(&[2u64, 3, 4, 5, 6, 7, 1000001, 1000002, 9007199254740993]);
                let k = self.r.pick(&[1u64, 1, 2, 3, 1000001]);
                let sign = if self.r.chance(1, 4) { "-" } else { "" };
                let e = format!(
                    "{sign}{}/{}",
                    u128::from(p) * u128::from(k),
                    u128::from(q) * u128::from(k)
                );
                bin('^', u, raw(&e))
            }
            8 => bin('+', raw("nCr(9007199254740993,1)-9007199254740992"), u),
            9 => bin(
                '+',
                bin('*', raw("10^17"), bin('-', self.leaf(), num("0.3"))),
                u,
            ),
            10 => bin('^', num(self.r.pick(RISKY_NUMBERS)), u),
            11 => bin('*', num(self.r.pick(RISKY_NUMBERS)), u),
            12 => bin(
                '+',
                bin('-', bin('+', raw("10^5000"), num("1")), raw("10^5000")),
                u,
            ),
            _ => bin('-', u, num(self.r.pick(RISKY_NUMBERS))),
        }
    }

    fn definedness(&mut self) -> T {
        let v = self.ordinary(1);
        let u = if self.r.chance(1, 2) {
            let n = nowhere(self.r);
            if self.r.chance(1, 3) {
                bin('+', n, self.ordinary(1))
            } else {
                n
            }
        } else {
            self.partial()
        };
        self.shortcut(u, v)
    }

    fn counting(&mut self) -> T {
        let u = match self.r.below(8) {
            0 | 1 => T::X,
            2 => call1("floor", T::X),
            3 => bin('-', T::X, num("0.5")),
            4 => call1("abs", T::X),
            5 => bin('*', num("2"), T::X),
            6 => neg(T::X),
            _ => self.ordinary(1),
        };
        let k = raw(self.r.pick(&[
            "0",
            "1",
            "2",
            "3",
            "62",
            "2.5",
            "-1",
            "0.5",
            "9007199254740993",
            "x",
            "floor(x)",
        ]));
        match self.r.below(14) {
            0 => T::Bang(Box::new(u), false),
            1 => T::Bang(Box::new(u), true),
            2 | 3 => call("nCr", vec![u, k]),
            4 => call("nPr", vec![u, k]),
            5 => call("nCr", vec![self.small(), u]),
            6 | 7 => {
                let c = raw(self.r.pick(COUNTS));
                let op = self.r.pick(&['+', '*', '-', '/']);
                bin(op, c, u)
            }
            8 => bin('^', u, raw(self.r.pick(COUNTS))),
            9 | 10 => {
                let base = match self.r.below(5) {
                    0 => raw("-8"),
                    1 => bin('-', T::X, self.small()),
                    2 => neg(bin('^', T::X, num("2"))),
                    3 => raw("-1"),
                    _ => u.clone(),
                };
                let degree = if self.r.chance(1, 4) {
                    u
                } else {
                    raw(self.r.pick(DEGREES))
                };
                call("root", vec![base, degree])
            }
            11 => {
                let (p, q) = self.r.pick(&[
                    ("4294967299", "3"),
                    ("9007199254740993", "3"),
                    ("5", "3"),
                    ("4294967298", "2"),
                ]);
                let b = bin('+', T::X, num("1"));
                bin(
                    '-',
                    call("root", vec![b.clone(), num(p)]),
                    call("root", vec![b, num(q)]),
                )
            }
            12 => bin('/', num("1"), T::Bang(Box::new(u), false)),
            _ => call1("ceil", bin('/', T::Bang(Box::new(u), true), self.small())),
        }
    }

    fn display(&mut self) -> T {
        let (c, g) = self.r.pick(NEAR);
        let d = || bin('-', T::X, raw(c));
        let dg = || bin('-', bin('-', T::X, raw(c)), num(g));
        match self.r.below(9) {
            0 => bin(
                '+',
                raw(c),
                bin('/', bin('+', call1("sin", T::X), num("2")), raw("10^7")),
            ),
            1 => bin('/', num("1"), bin('*', d(), dg())),
            2 => call1("sqrt", bin('*', d(), dg())),
            3 => call1("ln", bin('*', d(), dg())),
            4 => bin('*', d(), dg()),
            5 => bin('+', call1("abs", d()), call1("abs", dg())),
            6 => bin('*', bin('^', d(), num("2")), bin('^', dg(), num("2"))),
            7 => bin('+', call1("atan", T::X), num(g)),
            _ => bin(
                '/',
                bin('*', call1("sign", T::X), bin('/', T::X, T::X)),
                raw("10^3"),
            ),
        }
    }

    /// Two spellings equal everywhere, with the same domain.
    fn pair(&mut self) -> (T, T) {
        let f = self.ordinary(1);
        match self.r.below(12) {
            0 | 1 => {
                // p/q against the ratio in lowest terms.
                let p = self
                    .r
                    .pick(&[1u64, 2, 3, 5, 7, 1000001, 4294967299, 9007199254740993]);
                let q = self.r.pick(&[2u64, 3, 5, 7, 9, 1000001, 1000003]);
                let g = gcd(p, q);
                let (p, q) = (p / g, q / g);
                let k = self.r.pick(&[2u64, 3, 4, 1000001, 3000003]);
                let sign = if self.r.chance(1, 4) { "-" } else { "" };
                let long = format!(
                    "{sign}{}/{}",
                    u128::from(p) * u128::from(k),
                    u128::from(q) * u128::from(k)
                );
                let short = if q == 1 {
                    format!("{sign}{p}")
                } else {
                    format!("{sign}{p}/{q}")
                };
                if self.r.chance(2, 3) {
                    (bin('^', f.clone(), raw(&long)), bin('^', f, raw(&short)))
                } else {
                    (bin('*', raw(&long), f.clone()), bin('*', raw(&short), f))
                }
            }
            2 | 3 => {
                // x^(a+b) against x^c, c = a + b exactly.
                let (ab, c) = self.r.pick(&[
                    ("0.1+0.9", "1"),
                    ("1.5+1.5", "3"),
                    ("0.25+0.5", "0.75"),
                    ("1+2", "3"),
                    ("0.5*2", "1"),
                    ("6/2", "3"),
                    ("2.0", "2"),
                    ("0.1+0.2", "0.3"),
                    ("2.5-0.5", "2"),
                    ("9007199254740992+1", "9007199254740993"),
                    ("1.5/0.5", "3"),
                    ("0.2*5+1", "2"),
                ]);
                (bin('^', f.clone(), raw(ab)), bin('^', f, raw(c)))
            }
            4 | 5 => {
                // u − u + f against f, u defined everywhere.
                let u = match self.r.below(6) {
                    0 => raw("sin(4)^2"),
                    1 => num(self.r.pick(RISKY_NUMBERS)),
                    2 => call1("sin", T::X),
                    3 => bin('^', T::X, num("2")),
                    4 => raw("10^17"),
                    _ => call1("atan", T::X),
                };
                (bin('+', bin('-', u.clone(), u), f.clone()), f)
            }
            6 | 7 => {
                // Literal arithmetic folded by hand.
                let (a, v) = self.r.pick(LITERAL_ARITHMETIC);
                let op = self.r.pick(&['+', '*', '-']);
                (bin(op, raw(a), f.clone()), bin(op, raw(v), f))
            }
            8 => {
                // A root against the power 1/n: the same odd and even rule.
                let n = self.r.pick(&[
                    "3",
                    "5",
                    "2",
                    "4",
                    "9007199254740993",
                    "4294967299",
                    "1000001",
                ]);
                (
                    call("root", vec![f.clone(), num(n)]),
                    bin('^', f, raw(&format!("1/{n}"))),
                )
            }
            9 => {
                let (l, r) = self.r.pick(&[("sqrt", "1/2"), ("cbrt", "1/3")]);
                (call1(l, f.clone()), bin('^', f, raw(r)))
            }
            10 => {
                // A sum of twins against the product.
                let n = num(self.r.pick(RISKY_NUMBERS));
                (
                    bin(
                        '+',
                        bin('*', n.clone(), f.clone()),
                        bin('*', n.clone(), f.clone()),
                    ),
                    bin('*', bin('*', num("2"), n), f),
                )
            }
            _ => {
                // An integer power written out.
                (
                    bin('*', bin('*', f.clone(), f.clone()), f.clone()),
                    bin('^', f, num("3")),
                )
            }
        }
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// The largest tree generated (nodes), and the longest text (characters).
pub const MAX_SIZE: usize = 60;
pub const MAX_TEXT: usize = 400;

/// Case `index` of `seed`.
pub fn case(seed: u64, index: u64) -> Case {
    let mut r = Rng::new(seed, index);
    // A few tries for a tree within the size limits; then a plain one.
    for _ in 0..8 {
        let sliders = r.chance(3, 10);
        let mut g = Gen { r: &mut r, sliders };
        let which = g.r.weighted(&[26, 24, 20, 8, 22]);
        let (family, atom, pair) = match which {
            0 => ("literal", g.literal(), None),
            1 => ("definedness", g.definedness(), None),
            2 => ("counting", g.counting(), None),
            3 => ("display", g.display(), None),
            _ => {
                let (l, r) = g.pair();
                ("pair", l, Some(r))
            }
        };
        // Around the atom: alone, in a sum or product with an ordinary
        // block, or under a function.
        let wrap = |g: &mut Gen<'_>, t: T| -> T {
            match g.r.weighted(&[40, 18, 12, 10, 8, 6, 6]) {
                0 => t,
                1 => bin('+', t, g.ordinary(1)),
                2 => bin('*', g.ordinary(1), t),
                3 => {
                    let f =
                        g.r.pick(&["abs", "floor", "sin", "exp", "sqrt", "atan", "sign", "ln"]);
                    call1(f, t)
                }
                4 => bin('-', g.ordinary(1), t),
                5 => bin('/', g.ordinary(1), t),
                _ => call("min", vec![t, g.ordinary(1)]),
            }
        };
        let (expr, pair) = match pair {
            // Both spellings in the same place.
            Some(p) => {
                let mut probe = Rng(g.r.0);
                let mut g2 = Gen {
                    r: &mut probe,
                    sliders,
                };
                let e = wrap(&mut g, atom);
                (e, Some(wrap(&mut g2, p)))
            }
            None => (wrap(&mut g, atom), None),
        };
        let fits = |t: &T| t.size() <= MAX_SIZE && t.text().len() <= MAX_TEXT;
        if !fits(&expr) || pair.as_ref().is_some_and(|p| !fits(p)) {
            continue;
        }
        let used = expr.has_var() || pair.as_ref().is_some_and(T::has_var);
        let wanted = sliders;
        let mut sliders: Vec<(&'static str, f64)> = if used {
            vec![("a", r.pick(SLIDER_VALUES)), ("b", r.pick(SLIDER_VALUES))]
        } else {
            Vec::new()
        };
        // A slider in a number's place, set to that number: the double
        // where the number was the decimal (and, under a digit limit, the
        // decimal again).
        let mut expr = expr;
        if wanted && !used && pair.is_none() {
            let numbers: Vec<Vec<usize>> = expr
                .paths()
                .into_iter()
                .filter(|p| matches!(expr.at(p), T::Num(_)))
                .collect();
            if !numbers.is_empty() {
                let at = r.pick(&numbers);
                if let T::Num(text) = expr.at(&at)
                    && let Ok(v) = text.parse::<f64>()
                    && v.is_finite()
                {
                    expr = expr.replaced(&at, &T::Var("a"));
                    sliders = vec![("a", v)];
                }
            }
        }
        let mut digits = DIGITS[r.weighted(&[2, 6, 2, 4, 4, 8])];
        // Two spellings are one function only where rounding leaves every
        // number of both as typed (to 5 digits 1000001/3000003 is no cube
        // root): a setting that does, or off.
        if let Some(p) = &pair {
            let ok = |d: Option<u8>| unrounded(&expr.text(), d) && unrounded(&p.text(), d);
            if !ok(digits) {
                digits = *DIGITS.iter().rev().find(|d| ok(**d)).unwrap_or(&None);
            }
        }
        let unit = [TrigUnit::Radians, TrigUnit::Degrees, TrigUnit::Grads][r.weighted(&[90, 7, 3])];
        return Case {
            index,
            family,
            expr,
            pair,
            digits,
            sliders,
            unit,
        };
    }
    Case {
        index,
        family: "plain",
        expr: bin('+', bin('^', T::X, num("2")), num("1")),
        pair: None,
        digits: None,
        sliders: Vec::new(),
        unit: TrigUnit::Radians,
    }
}
