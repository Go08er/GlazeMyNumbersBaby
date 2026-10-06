//! Tokenizer for equation text.
//!
//! Accepts both the ASCII forms the graphing numpad inserts (`arcsin()`,
//! `ceiling()`, `root(x, n)`, `log(b, x)`, `>=`) and the Unicode forms a user
//! may type or paste (`π`, `×`, `÷`, `−`, `≤`, `≥`, `√`, `∛`, `²`, `⁻¹`,
//! `𝑥`, `𝑦`).

use crate::ast::{Constant, FUNCTION_NAMES, Func};
use crate::error::{EquationError, SyntaxErrorCode};
use std::ops::Range;

/// Relation operator between the two sides of an equation/inequality.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RelOp {
    /// `=`
    Eq,
    /// `<`
    Lt,
    /// `≤` / `<=`
    Le,
    /// `>`
    Gt,
    /// `≥` / `>=`
    Ge,
}

impl RelOp {
    /// True for `<`, `≤`, `>`, `≥`.
    pub fn is_inequality(self) -> bool {
        !matches!(self, RelOp::Eq)
    }

    /// True for the strict operators `<` and `>`.
    pub fn is_strict(self) -> bool {
        matches!(self, RelOp::Lt | RelOp::Gt)
    }

    /// The operator with sides swapped (`a < b` ⇔ `b > a`).
    pub fn flipped(self) -> RelOp {
        match self {
            RelOp::Eq => RelOp::Eq,
            RelOp::Lt => RelOp::Gt,
            RelOp::Le => RelOp::Ge,
            RelOp::Gt => RelOp::Lt,
            RelOp::Ge => RelOp::Le,
        }
    }

    /// Display symbol.
    pub fn symbol(self) -> &'static str {
        match self {
            RelOp::Eq => "=",
            RelOp::Lt => "<",
            RelOp::Le => "≤",
            RelOp::Gt => ">",
            RelOp::Ge => "≥",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Tok {
    Num(f64),
    /// A parameter variable name (already split from letter runs).
    Var(String),
    X,
    Y,
    Const(Constant),
    Func(Func),
    /// `log_b`: logarithm with a subscripted base; the base follows.
    LogSub,
    /// Infix `mod`.
    ModOp,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Bang,
    Degree,
    Pipe,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Sqrt,
    Cbrt,
    FourthRoot,
    Rel(RelOp),
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub tok: Tok,
    pub span: Range<usize>,
}

/// Options affecting tokenization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParseOptions {
    /// Use `,` as the decimal separator and `;` as the argument separator
    /// (the original's `DecimalCommaAndListSemicolon` localization).
    pub decimal_comma: bool,
    /// Each number typed is rounded on entry to this many significant
    /// decimal digits, half away from zero (`round_decimal`), and the
    /// rounded decimal is the number from then on: for the curve, its
    /// trace, its analysis and its certificate alike. `None` ("off"): the
    /// decimal exactly as typed. A calculator's precision: 14 is the TI-84
    /// Plus CE's ([`TI84_DIGITS`]); with 15 or fewer, two numbers that read
    /// differently never share a double while both are in the doubles'
    /// normal range (beyond it they can: 10⁻⁴⁰⁰ and 2·10⁻⁴⁰⁰ both round
    /// to 0, 10⁴⁰⁰ and 2·10⁴⁰⁰ to ∞, and a subnormal has fewer digits).
    /// Each occurrence is its own number all the same (`ast::Lit`). See
    /// [`LITERAL_DIGITS`].
    pub literal_digits: Option<u8>,
}

/// The digit limits [`ParseOptions::literal_digits`] takes (besides off).
pub const LITERAL_DIGITS: std::ops::RangeInclusive<u8> = 5..=20;

/// The TI-84 Plus CE's precision: 14 significant digits (TI documents the
/// TI-83 Plus and TI-84 Plus family as calculating with 14 digits:
/// `docs/ti-conventions.md` has the source). The apps' default for
/// [`ParseOptions::literal_digits`].
pub const TI84_DIGITS: u8 = 14;

/// The decimal `digits` (ASCII digits and at most one `.`, no sign)
/// rounded to `n` (≥ 1) significant digits, half away from zero: `0.1234567`
/// to 3 is `0.123`, `99.95` is `100`, `1.0000000000000001` to 14 is `1`.
/// A decimal with no more significant digits than that, or 0, comes back
/// as given.
pub fn round_decimal(digits: &str, n: u8) -> String {
    let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
    let mut all: Vec<u8> = int.bytes().chain(frac.bytes()).map(|b| b - b'0').collect();
    let mut point = int.len();
    let Some(first) = all.iter().position(|&d| d != 0) else {
        return digits.to_string();
    };
    let keep = first + usize::from(n.max(1));
    if keep >= all.len() || all[keep..].iter().all(|&d| d == 0) {
        return digits.to_string();
    }
    let up = all[keep] >= 5;
    all.truncate(keep);
    if up {
        let mut i = keep;
        loop {
            if i == 0 {
                all.insert(0, 1);
                point += 1;
                break;
            }
            i -= 1;
            if all[i] == 9 {
                all[i] = 0;
            } else {
                all[i] += 1;
                break;
            }
        }
    }
    // The places between the digits kept and the point are zeros.
    while all.len() < point {
        all.push(0);
    }
    let s: String = all.iter().map(|&d| char::from(d + b'0')).collect();
    let (i, f) = s.split_at(point);
    let i = match i.trim_start_matches('0') {
        "" => "0",
        i => i,
    };
    match f.trim_end_matches('0') {
        "" => i.to_string(),
        f => format!("{i}.{f}"),
    }
}

/// The double nearest `v` rounded to `n` significant digits half away from
/// zero, as a typed number is (a slider's value under a digit limit:
/// [`ParseOptions::literal_digits`]). Exact on `v`'s decimal expansion.
pub fn round_to_digits(v: f64, n: u8) -> f64 {
    if !v.is_finite() || v == 0.0 {
        return v;
    }
    let r: f64 = round_decimal(&format!("{:.1074}", v.abs()), n)
        .parse()
        .unwrap_or(v.abs());
    r.copysign(v)
}

fn superscript_value(c: char) -> Option<char> {
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
        '⁻' => '-',
        '⁺' => '+',
        '⁽' => '(',
        '⁾' => ')',
        _ => return None,
    })
}

/// Maps mathematical italic/bold letters (as used on the numpad labels,
/// e.g. `𝑥`) and a few look-alikes to plain letters.
fn normalize_letter(c: char) -> char {
    let u = c as u32;
    // Mathematical italic capital A..Z / small a..z.
    if (0x1D434..=0x1D44D).contains(&u) {
        return char::from_u32(u - 0x1D434 + 'A' as u32).unwrap_or(c);
    }
    if (0x1D44E..=0x1D467).contains(&u) {
        return char::from_u32(u - 0x1D44E + 'a' as u32).unwrap_or(c);
    }
    // Mathematical bold capital/small.
    if (0x1D400..=0x1D419).contains(&u) {
        return char::from_u32(u - 0x1D400 + 'A' as u32).unwrap_or(c);
    }
    if (0x1D41A..=0x1D433).contains(&u) {
        return char::from_u32(u - 0x1D41A + 'a' as u32).unwrap_or(c);
    }
    if c == 'ℎ' {
        return 'h';
    }
    c
}

fn is_letter(c: char) -> bool {
    c != 'π' && c != 'Π' && normalize_letter(c).is_alphabetic()
}

pub(crate) fn tokenize(input: &str, opts: ParseOptions) -> Result<Vec<Token>, EquationError> {
    tokenize_with(input, opts, &mut None)
}

/// Each numeric literal's value and its text (digits and `.`), in order:
/// the exact decimal the parsed double stands for.
pub(crate) fn literal_texts(
    input: &str,
    opts: ParseOptions,
) -> Result<Vec<(f64, String)>, EquationError> {
    let mut lits = Some(Vec::new());
    tokenize_with(input, opts, &mut lits)?;
    Ok(lits.unwrap_or_default())
}

fn tokenize_with(
    input: &str,
    opts: ParseOptions,
    lits: &mut Option<Vec<(f64, String)>>,
) -> Result<Vec<Token>, EquationError> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let (decimal_sep, list_sep) = if opts.decimal_comma {
        (',', ';')
    } else {
        ('.', ',')
    };
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // Numbers.
        if c.is_ascii_digit() || c == decimal_sep {
            let mut seen_dot = false;
            let mut digits = String::new();
            let mut any_digit = false;
            while i < chars.len() {
                let d = chars[i];
                if d.is_ascii_digit() {
                    digits.push(d);
                    any_digit = true;
                    i += 1;
                } else if d == decimal_sep {
                    if seen_dot {
                        // `7.3.2`
                        let mut end = i + 1;
                        while end < chars.len()
                            && (chars[end].is_ascii_digit() || chars[end] == decimal_sep)
                        {
                            end += 1;
                        }
                        return Err(EquationError::syntax(
                            SyntaxErrorCode::TooManyDecimalPoints,
                            start..end,
                        ));
                    }
                    seen_dot = true;
                    digits.push('.');
                    i += 1;
                } else {
                    break;
                }
            }
            if !any_digit {
                return Err(EquationError::syntax(
                    SyntaxErrorCode::DecimalPointWithoutDigits,
                    start..i,
                ));
            }
            // Rounded on entry, under a digit limit: from here on the
            // number is the rounded decimal.
            if let Some(n) = opts.literal_digits {
                digits = round_decimal(&digits, n);
            }
            let v: f64 = digits
                .parse()
                .map_err(|_| EquationError::syntax(SyntaxErrorCode::InvalidToken, start..i))?;
            if let Some(l) = lits.as_mut() {
                l.push((v, digits.clone()));
            }
            out.push(Token {
                tok: Tok::Num(v),
                span: start..i,
            });
            continue;
        }
        // Superscript runs: x², x⁻¹, x⁽²⁺¹⁾ → ^( ... ).
        if superscript_value(c).is_some() {
            let mut inner = String::new();
            while let Some(v) = chars.get(i).and_then(|&ch| superscript_value(ch)) {
                inner.push(v);
                i += 1;
            }
            let inner_opts = ParseOptions {
                decimal_comma: false,
                ..opts
            };
            let sub = tokenize_with(&inner, inner_opts, lits).map_err(|e| EquationError {
                code: e.code,
                span: start..i,
            })?;
            out.push(Token {
                tok: Tok::Caret,
                span: start..start + 1,
            });
            out.push(Token {
                tok: Tok::LParen,
                span: start..start,
            });
            for t in sub {
                out.push(Token {
                    tok: t.tok,
                    span: start..i,
                });
            }
            out.push(Token {
                tok: Tok::RParen,
                span: i..i,
            });
            continue;
        }
        if c == 'π' || c == 'Π' {
            out.push(Token {
                tok: Tok::Const(Constant::Pi),
                span: start..start + 1,
            });
            i += 1;
            continue;
        }
        if is_letter(c) {
            let mut run = String::new();
            while i < chars.len() && is_letter(chars[i]) {
                run.push(normalize_letter(chars[i]));
                i += 1;
            }
            split_letter_run(&run, start, &chars, &mut i, &mut out)?;
            continue;
        }
        let single = |t: Tok| Token {
            tok: t,
            span: start..start + 1,
        };
        let next = chars.get(i + 1).copied();
        let tok = match c {
            '+' => single(Tok::Plus),
            '-' | '−' | '–' => single(Tok::Minus),
            '*' | '×' | '·' | '⋅' | '∙' | '∗' => single(Tok::Star),
            '/' | '÷' | '∕' => single(Tok::Slash),
            '^' => {
                if next == Some('^') {
                    return Err(EquationError::syntax(
                        SyntaxErrorCode::UnexpectedToken,
                        start + 1..start + 2,
                    ));
                }
                single(Tok::Caret)
            }
            '!' => single(Tok::Bang),
            '°' => single(Tok::Degree),
            '|' => single(Tok::Pipe),
            '(' => single(Tok::LParen),
            ')' => single(Tok::RParen),
            '{' => single(Tok::LBrace),
            '}' => single(Tok::RBrace),
            '√' => single(Tok::Sqrt),
            '∛' => single(Tok::Cbrt),
            '∜' => single(Tok::FourthRoot),
            '≤' | '⩽' => single(Tok::Rel(RelOp::Le)),
            '≥' | '⩾' => single(Tok::Rel(RelOp::Ge)),
            '<' => {
                if next == Some('=') {
                    i += 1;
                    Token {
                        tok: Tok::Rel(RelOp::Le),
                        span: start..start + 2,
                    }
                } else {
                    single(Tok::Rel(RelOp::Lt))
                }
            }
            '>' => {
                if next == Some('=') {
                    i += 1;
                    Token {
                        tok: Tok::Rel(RelOp::Ge),
                        span: start..start + 2,
                    }
                } else {
                    single(Tok::Rel(RelOp::Gt))
                }
            }
            '=' => match next {
                Some('<') => {
                    i += 1;
                    Token {
                        tok: Tok::Rel(RelOp::Le),
                        span: start..start + 2,
                    }
                }
                Some('>') => {
                    i += 1;
                    Token {
                        tok: Tok::Rel(RelOp::Ge),
                        span: start..start + 2,
                    }
                }
                Some('=') => {
                    i += 1;
                    Token {
                        tok: Tok::Rel(RelOp::Eq),
                        span: start..start + 2,
                    }
                }
                _ => single(Tok::Rel(RelOp::Eq)),
            },
            c if c == list_sep => single(Tok::Comma),
            _ => {
                return Err(EquationError::syntax(
                    SyntaxErrorCode::InvalidToken,
                    start..start + 1,
                ));
            }
        };
        out.push(tok);
        i += 1;
    }
    Ok(out)
}

/// Splits a run of letters into function names, constants and single-letter
/// variables (implicit multiplication: `xy` = x·y, `xsinx` = x·sin x).
/// Handles `_` subscripts on the last letter (`a_1`, `k_max`) and `log_b`.
fn split_letter_run(
    run: &str,
    run_start: usize,
    chars: &[char],
    i: &mut usize,
    out: &mut Vec<Token>,
) -> Result<(), EquationError> {
    let letters: Vec<char> = run.chars().collect();
    let mut p = 0;
    while p < letters.len() {
        let rest: String = letters[p..].iter().collect();
        // Longest matching known name.
        let mut best: Option<(usize, Tok)> = None;
        for (name, f) in FUNCTION_NAMES {
            if rest.starts_with(name) {
                let len = name.chars().count();
                if best.as_ref().is_none_or(|(l, _)| len > *l) {
                    let tok = if *f == Func::Mod {
                        Tok::ModOp
                    } else {
                        Tok::Func(*f)
                    };
                    best = Some((len, tok));
                }
            }
        }
        if rest.starts_with("pi") && best.as_ref().is_none_or(|(l, _)| *l < 2) {
            best = Some((2, Tok::Const(Constant::Pi)));
        }
        let span_start = run_start + p;
        if let Some((len, tok)) = best {
            let at_end = p + len == letters.len();
            // `log_b`: subscripted base.
            if tok == Tok::Func(Func::Log) && at_end && chars.get(*i) == Some(&'_') {
                *i += 1;
                out.push(Token {
                    tok: Tok::LogSub,
                    span: span_start..*i,
                });
                return Ok(());
            }
            // `mod(a, b)` is a function call; `a mod b` is the infix operator.
            let tok = if tok == Tok::ModOp
                && at_end
                && next_non_space(chars, *i) == Some('(')
                && out_is_operand_start(out)
            {
                Tok::Func(Func::Mod)
            } else {
                tok
            };
            out.push(Token {
                tok,
                span: span_start..span_start + len,
            });
            p += len;
            continue;
        }
        let c = letters[p];
        let span = span_start..span_start + 1;
        let is_last = p + 1 == letters.len();
        if is_last && chars.get(*i) == Some(&'_') {
            // Subscripted variable name: letter '_' alphanumerics.
            let us = *i;
            *i += 1;
            let sub_start = *i;
            let braced = chars.get(*i) == Some(&'{');
            if braced {
                *i += 1;
            }
            let mut sub = String::new();
            while *i < chars.len() && (chars[*i].is_alphanumeric() && chars[*i] != 'π') {
                sub.push(normalize_letter(chars[*i]));
                *i += 1;
            }
            if braced {
                if chars.get(*i) == Some(&'}') {
                    *i += 1;
                } else {
                    return Err(EquationError::syntax(
                        SyntaxErrorCode::InvalidVariableNameFormat,
                        span_start..*i,
                    ));
                }
            }
            if sub.is_empty() || (*i < chars.len() && chars[*i] == '_') {
                let end = (*i + 1).min(chars.len()).max(us + 1);
                return Err(EquationError::syntax(
                    SyntaxErrorCode::InvalidVariableNameFormat,
                    span_start..end,
                ));
            }
            let _ = sub_start;
            let name = format!("{c}_{sub}");
            out.push(Token {
                tok: Tok::Var(name),
                span: span_start..*i,
            });
            return Ok(());
        }
        let tok = match c {
            'x' => Tok::X,
            'y' => Tok::Y,
            'e' => Tok::Const(Constant::E),
            'i' | 'I' => {
                return Err(EquationError::syntax(
                    SyntaxErrorCode::CannotUseIInReal,
                    span,
                ));
            }
            _ => Tok::Var(c.to_string()),
        };
        out.push(Token { tok, span });
        p += 1;
    }
    Ok(())
}

fn next_non_space(chars: &[char], mut i: usize) -> Option<char> {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    chars.get(i).copied()
}

/// True when the previous token cannot end an operand, i.e. a following
/// `mod(` must be the function form rather than the infix operator.
fn out_is_operand_start(out: &[Token]) -> bool {
    match out.last() {
        None => true,
        Some(t) => matches!(
            t.tok,
            Tok::Plus
                | Tok::Minus
                | Tok::Star
                | Tok::Slash
                | Tok::Caret
                | Tok::LParen
                | Tok::LBrace
                | Tok::Comma
                | Tok::Rel(_)
                | Tok::ModOp
                | Tok::Sqrt
                | Tok::Cbrt
                | Tok::FourthRoot
                | Tok::Pipe
                | Tok::Func(_)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        tokenize(s, ParseOptions::default())
            .unwrap()
            .into_iter()
            .map(|t| t.tok)
            .collect()
    }

    #[test]
    fn letter_runs_split() {
        assert_eq!(toks("xy"), vec![Tok::X, Tok::Y]);
        assert_eq!(toks("sinx"), vec![Tok::Func(Func::Sin), Tok::X]);
        assert_eq!(toks("sinhx"), vec![Tok::Func(Func::Sinh), Tok::X]);
        assert_eq!(toks("sign"), vec![Tok::Func(Func::Sign)]);
        assert_eq!(toks("exp"), vec![Tok::Func(Func::Exp)]);
        assert_eq!(toks("ex"), vec![Tok::Const(Constant::E), Tok::X]);
        assert_eq!(toks("pi"), vec![Tok::Const(Constant::Pi)]);
        assert_eq!(toks("2π"), vec![Tok::Num(2.0), Tok::Const(Constant::Pi)]);
        assert_eq!(toks("a_1"), vec![Tok::Var("a_1".into())]);
        assert_eq!(toks("𝑥"), vec![Tok::X]);
    }

    #[test]
    fn superscripts() {
        assert_eq!(
            toks("x²"),
            vec![Tok::X, Tok::Caret, Tok::LParen, Tok::Num(2.0), Tok::RParen]
        );
        assert_eq!(
            toks("x⁻¹"),
            vec![
                Tok::X,
                Tok::Caret,
                Tok::LParen,
                Tok::Minus,
                Tok::Num(1.0),
                Tok::RParen
            ]
        );
    }

    #[test]
    fn number_errors() {
        let e = tokenize("7.3.2", ParseOptions::default()).unwrap_err();
        assert_eq!(
            e.code,
            crate::error::ErrorCode::Syntax(SyntaxErrorCode::TooManyDecimalPoints)
        );
        let e = tokenize("3+.+4", ParseOptions::default()).unwrap_err();
        assert_eq!(
            e.code,
            crate::error::ErrorCode::Syntax(SyntaxErrorCode::DecimalPointWithoutDigits)
        );
        assert_eq!(e.span, 2..3);
    }

    #[test]
    fn decimal_comma() {
        let t: Vec<Tok> = tokenize(
            "root(2,5; 3)",
            ParseOptions {
                decimal_comma: true,
                ..Default::default()
            },
        )
        .unwrap()
        .into_iter()
        .map(|t| t.tok)
        .collect();
        assert_eq!(
            t,
            vec![
                Tok::Func(Func::Root),
                Tok::LParen,
                Tok::Num(2.5),
                Tok::Comma,
                Tok::Num(3.0),
                Tok::RParen
            ]
        );
    }

    #[test]
    fn rounding_to_significant_digits() {
        let r = round_decimal;
        assert_eq!(r("0.1234567", 3), "0.123");
        assert_eq!(r("0.1235", 3), "0.124");
        // Half away from zero, not to even.
        assert_eq!(r("0.1225", 3), "0.123");
        assert_eq!(r("2.5", 1), "3");
        assert_eq!(r("99.95", 3), "100");
        assert_eq!(r("0.0999", 2), "0.1");
        assert_eq!(r("123456789", 3), "123000000");
        assert_eq!(r("0.000123456", 3), "0.000123");
        assert_eq!(r("1.0000000000000001", 14), "1");
        assert_eq!(r("0.99999999999999999", 14), "1");
        assert_eq!(r("1.00000000000005", 14), "1.0000000000001");
        // Nothing beyond the digits kept: as typed.
        assert_eq!(r("0.10", 14), "0.10");
        assert_eq!(r("007", 1), "007");
        assert_eq!(r("0", 5), "0");
        assert_eq!(r(".5", 5), ".5");
        assert_eq!(round_to_digits(0.1 + 0.2, 15), 0.3);
        assert_eq!(round_to_digits(-(0.1 + 0.2), 14), -0.3);
        assert_eq!(round_to_digits(1.0, 5), 1.0);
    }

    #[test]
    fn numbers_are_rounded_on_entry_under_a_digit_limit() {
        let ti = ParseOptions {
            literal_digits: Some(TI84_DIGITS),
            ..Default::default()
        };
        let nums =
            |s: &str, o: ParseOptions| -> Vec<(f64, String)> { literal_texts(s, o).unwrap() };
        assert_eq!(
            nums("1.0000000000000001*x-1*x", ti),
            vec![(1.0, "1".to_string()), (1.0, "1".to_string())]
        );
        // Superscripts too.
        assert_eq!(nums("x^1.0000000000000001+x²", ti)[0].1, "1");
        // Off: the decimal typed.
        assert_eq!(
            nums("1.0000000000000001", ParseOptions::default())[0].1,
            "1.0000000000000001"
        );
    }
}
