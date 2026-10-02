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
            let v: f64 = digits
                .parse()
                .map_err(|_| EquationError::syntax(SyntaxErrorCode::InvalidToken, start..i))?;
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
            let sub = tokenize(&inner, ParseOptions::default()).map_err(|e| EquationError {
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
}
