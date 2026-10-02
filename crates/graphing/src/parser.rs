//! Recursive-descent parser for equation text.
//!
//! Grammar (lowest to highest precedence):
//!
//! ```text
//! input     := sum (relop sum)*                    relop: = < > ≤ ≥ <= >=
//! sum       := product (('+' | '-') product)*
//! product   := unary (('*' | '/' | '×' | '÷' | 'mod') unary)*
//! unary     := ('-' | '+') unary | juxt
//! juxt      := power power*                        implicit multiplication
//! power     := postfix ('^' exponent)?             right associative
//! exponent  := ('-' | '+') exponent | power
//! postfix   := primary ('!' | '!!' | '°')*
//! primary   := number | variable | x | y | π | e | '(' sum ')' | '{' sum '}'
//!            | '|' sum '|' | '√' power | '∛' power | function
//! function  := name ('^' exponent)? ( '(' sum (',' sum)* ')' | ['-'] juxt-without-functions )
//! ```
//!
//! Implicit multiplication binds tighter than explicit `*` and `/`, matching
//! the linear-format math input the original used (`1/2x` is `1/(2x)`).
//! A function without parentheses takes the following implicit-product
//! chain as its argument but stops at the next function name, so
//! `sin 2x` is `sin(2x)` and `sin x cos x` is `sin(x)·cos(x)`.
//! `sin^2 x` / `sin²x` is `(sin x)²` and `sin^-1 x` / `sin⁻¹x` is `arcsin x`.

use crate::ast::{BinOp, Expr, Func};
use crate::error::{EquationError, EvaluationErrorCode, SyntaxErrorCode};
use crate::lexer::{ParseOptions, RelOp, Tok, Token, tokenize};
use std::ops::Range;

/// Result of parsing a whole input line: one or more sides separated by
/// relation operators.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedInput {
    /// The sides, left to right (`sides.len() == rels.len() + 1`).
    pub sides: Vec<Expr>,
    /// Character spans of the sides.
    pub side_spans: Vec<Range<usize>>,
    /// Relation operators between consecutive sides, with their spans.
    pub rels: Vec<(RelOp, Range<usize>)>,
    /// `Some(name)` when the input started with a function definition head
    /// such as `f(x) =`; that head is then the first side as `y`.
    pub function_name: Option<String>,
}

/// Parses a single expression (no relation operators allowed).
pub fn parse_expression(input: &str) -> Result<Expr, EquationError> {
    let parsed = parse_input(input, ParseOptions::default())?;
    if let Some((_, span)) = parsed.rels.first() {
        return Err(EquationError::syntax(
            SyntaxErrorCode::InvalidEquationFormat,
            span.clone(),
        ));
    }
    Ok(parsed.sides.into_iter().next().expect("at least one side"))
}

/// Longest equation text accepted, in characters (the equation box's
/// limit). Longer input is an equation error rather than unbounded work.
pub const MAX_EXPRESSION_LEN: usize = 1000;

/// Deepest expression tree accepted. Everything downstream (compilation,
/// differentiation, analysis, `Drop`) walks trees recursively; this keeps
/// them far inside a 2 MiB thread stack.
pub const MAX_TREE_DEPTH: usize = 256;

fn too_complex(span: Range<usize>) -> EquationError {
    EquationError::eval(EvaluationErrorCode::EquationTooComplexToPlot, span)
}

/// Parses an input line into sides and relation operators.
///
/// Input longer than [`MAX_EXPRESSION_LEN`], nested deeper than the parser
/// allows, or producing a tree deeper than [`MAX_TREE_DEPTH`] is rejected
/// with `EquationTooComplexToPlot`.
pub fn parse_input(input: &str, opts: ParseOptions) -> Result<ParsedInput, EquationError> {
    let len = input.chars().count();
    if len > MAX_EXPRESSION_LEN {
        return Err(too_complex(0..len));
    }
    let mut toks = tokenize(input, opts)?;
    if toks.is_empty() {
        return Err(EquationError::syntax(
            SyntaxErrorCode::EmptyExpression,
            0..len,
        ));
    }
    // Function definition head: `f(x) = …` (also `y(x) = …`).
    let mut function_name = None;
    if toks.len() >= 5 {
        let head_name = match &toks[0].tok {
            Tok::Var(n) => Some(n.clone()),
            Tok::Y => Some("y".to_string()),
            _ => None,
        };
        if let Some(name) = head_name
            && toks[1].tok == Tok::LParen
            && toks[2].tok == Tok::X
            && toks[3].tok == Tok::RParen
            && toks[4].tok == Tok::Rel(RelOp::Eq)
        {
            let span = toks[0].span.start..toks[3].span.end;
            toks.drain(0..4);
            toks.insert(0, Token { tok: Tok::Y, span });
            function_name = Some(name);
        }
    }
    // Whitespace before a token ends a denominator (`1/2 x` is x/2, as in
    // the linear math format, while `1/2x` is 1/(2x)).
    let chars: Vec<char> = input.chars().collect();
    let space_before: Vec<bool> = toks
        .iter()
        .map(|t| {
            t.span.start > 0
                && t.span.start <= chars.len()
                && chars[t.span.start - 1].is_whitespace()
        })
        .collect();
    let mut p = Parser {
        toks: &toks,
        space_before: &space_before,
        pos: 0,
        abs_depth: 0,
        depth: 0,
        len,
    };
    let mut parsed = p.parse_input()?;
    parsed.function_name = function_name;
    // Sums, products, implicit products and postfix operators are parsed
    // iteratively, so the recursion limit alone does not bound tree depth.
    for (side, span) in parsed.sides.iter().zip(&parsed.side_spans) {
        if side.depth_and_size().0 > MAX_TREE_DEPTH {
            return Err(too_complex(span.clone()));
        }
    }
    Ok(parsed)
}

/// Deepest parser recursion accepted. Every recursive grammar edge passes
/// through a counted rule: each primary (so each group, `|…|`, radical and
/// function), each prefix sign and each exponent. Far beyond anything typed
/// by hand, far below stack limits.
const MAX_DEPTH: usize = 200;

struct Parser<'a> {
    toks: &'a [Token],
    space_before: &'a [bool],
    pos: usize,
    abs_depth: usize,
    /// Current recursion depth (groups, unary signs, exponent chains).
    depth: usize,
    len: usize,
}

type PResult<T> = Result<T, EquationError>;

fn err<T>(code: SyntaxErrorCode, span: Range<usize>) -> PResult<T> {
    Err(EquationError::syntax(code, span))
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    fn peek_token(&self) -> Option<&'a Token> {
        self.toks.get(self.pos)
    }

    fn bump(&mut self) -> Option<&'a Token> {
        let t = self.toks.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn eof_span(&self) -> Range<usize> {
        self.len..self.len
    }

    fn prev_end(&self) -> usize {
        if self.pos == 0 {
            0
        } else {
            self.toks[self.pos - 1].span.end
        }
    }

    fn unexpected(&self, t: &Token) -> EquationError {
        let code = match t.tok {
            Tok::RParen => SyntaxErrorCode::ParenthesisMismatch,
            Tok::RBrace => SyntaxErrorCode::BracketMismatch,
            Tok::Comma => SyntaxErrorCode::InvalidToken,
            Tok::Rel(_) => SyntaxErrorCode::InvalidEquationFormat,
            _ => SyntaxErrorCode::UnexpectedToken,
        };
        EquationError::syntax(code, t.span.clone())
    }

    fn parse_input(&mut self) -> PResult<ParsedInput> {
        if let Some(t) = self.peek_token()
            && let Tok::Rel(op) = t.tok
        {
            let code = if op == RelOp::Eq {
                SyntaxErrorCode::EqualWithoutEquation
            } else {
                SyntaxErrorCode::UnexpectedToken
            };
            return err(code, t.span.clone());
        }
        let mut sides = Vec::new();
        let mut side_spans = Vec::new();
        let mut rels = Vec::new();
        let start = self.peek_token().map(|t| t.span.start).unwrap_or(0);
        sides.push(self.parse_sum()?);
        side_spans.push(start..self.prev_end());
        while let Some(t) = self.peek_token() {
            match t.tok {
                Tok::Rel(op) => {
                    self.pos += 1;
                    rels.push((op, t.span.clone()));
                    match self.peek_token() {
                        None => {
                            return err(
                                SyntaxErrorCode::UnexpectedEndOfExpression,
                                self.eof_span(),
                            );
                        }
                        Some(n) if matches!(n.tok, Tok::Rel(_)) => {
                            return err(SyntaxErrorCode::UnexpectedToken, n.span.clone());
                        }
                        Some(_) => {}
                    }
                    let s = self.peek_token().map(|t| t.span.start).unwrap_or(self.len);
                    sides.push(self.parse_sum()?);
                    side_spans.push(s..self.prev_end());
                }
                _ => return Err(self.unexpected(t)),
            }
        }
        Ok(ParsedInput {
            sides,
            side_spans,
            rels,
            function_name: None,
        })
    }

    /// Run `f` one recursion level deeper, refusing absurd nesting so that
    /// pathological input (e.g. thousands of nested parentheses) becomes an
    /// equation error instead of a stack overflow.
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        if self.depth >= MAX_DEPTH {
            let span = self
                .peek_token()
                .map(|t| t.span.clone())
                .unwrap_or(0..self.len);
            return Err(too_complex(span));
        }
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        r
    }

    /// Sums are reached recursively only through primaries (groups, `|…|`,
    /// function arguments), which are counted.
    fn parse_sum(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_product()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_product()?;
            lhs = Expr::bin(op, lhs, rhs);
        }
        Ok(lhs)
    }

    fn parse_product(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(Tok::Star) => {
                    self.pos += 1;
                    let rhs = self.parse_unary()?;
                    lhs = Expr::bin(BinOp::Mul, lhs, rhs);
                }
                Some(Tok::Slash) => {
                    self.pos += 1;
                    let rhs = self.parse_denominator()?;
                    lhs = Expr::bin(BinOp::Div, lhs, rhs);
                }
                Some(Tok::ModOp) => {
                    self.pos += 1;
                    let rhs = self.parse_unary()?;
                    lhs = Expr::Call(Func::Mod, vec![lhs, rhs]);
                }
                // Factors left over after a denominator that ended at a
                // space: `1/2 x` = (1/2)·x.
                _ if self.can_start_factor(false) => {
                    let rhs = self.parse_juxt(false, false)?;
                    lhs = Expr::bin(BinOp::Mul, lhs, rhs);
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_denominator(&mut self) -> PResult<Expr> {
        self.nested(|p| p.parse_denominator_inner())
    }

    fn parse_denominator_inner(&mut self) -> PResult<Expr> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.pos += 1;
                Ok(Expr::Neg(Box::new(self.parse_denominator()?)))
            }
            Some(Tok::Plus) => {
                self.pos += 1;
                self.parse_denominator()
            }
            _ => self.parse_juxt(false, true),
        }
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        self.nested(|p| p.parse_unary_inner())
    }

    fn parse_unary_inner(&mut self) -> PResult<Expr> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.pos += 1;
                Ok(Expr::Neg(Box::new(self.parse_unary()?)))
            }
            Some(Tok::Plus) => {
                self.pos += 1;
                self.parse_unary()
            }
            _ => self.parse_juxt(false, false),
        }
    }

    fn can_start_factor(&self, stop_at_func: bool) -> bool {
        match self.peek() {
            Some(Tok::Num(_) | Tok::Var(_) | Tok::X | Tok::Y | Tok::Const(_)) => true,
            Some(Tok::LParen | Tok::LBrace | Tok::Sqrt | Tok::Cbrt | Tok::FourthRoot) => true,
            Some(Tok::Func(_) | Tok::LogSub) => !stop_at_func,
            Some(Tok::Pipe) => self.abs_depth == 0,
            _ => false,
        }
    }

    fn parse_juxt(&mut self, stop_at_func: bool, stop_at_space: bool) -> PResult<Expr> {
        let mut lhs = self.parse_power()?;
        while self.can_start_factor(stop_at_func)
            && !(stop_at_space && self.space_before.get(self.pos).copied().unwrap_or(false))
        {
            let rhs = self.parse_power()?;
            lhs = Expr::bin(BinOp::Mul, lhs, rhs);
        }
        Ok(lhs)
    }

    fn parse_power(&mut self) -> PResult<Expr> {
        let base = self.parse_postfix()?;
        if self.peek() == Some(&Tok::Caret) {
            self.pos += 1;
            let exp = self.parse_exponent()?;
            return Ok(Expr::bin(BinOp::Pow, base, exp));
        }
        Ok(base)
    }

    fn parse_exponent(&mut self) -> PResult<Expr> {
        self.nested(|p| p.parse_exponent_inner())
    }

    fn parse_exponent_inner(&mut self) -> PResult<Expr> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.pos += 1;
                Ok(Expr::Neg(Box::new(self.parse_exponent()?)))
            }
            Some(Tok::Plus) => {
                self.pos += 1;
                self.parse_exponent()
            }
            _ => self.parse_power(),
        }
    }

    fn parse_postfix(&mut self) -> PResult<Expr> {
        let mut e = self.parse_primary()?;
        loop {
            match self.peek() {
                Some(Tok::Bang) => {
                    let first = self.bump().expect("peeked");
                    let double = matches!(self.peek_token(), Some(t) if t.tok == Tok::Bang && t.span.start == first.span.end);
                    if double {
                        self.pos += 1;
                        e = Expr::Call(Func::DoubleFactorial, vec![e]);
                    } else {
                        e = Expr::Call(Func::Factorial, vec![e]);
                    }
                }
                Some(Tok::Degree) => {
                    self.pos += 1;
                    e = Expr::Degrees(Box::new(e));
                }
                _ => break,
            }
        }
        Ok(e)
    }

    /// Parses `( … )` or `{ … }` after the opening token has been consumed.
    fn parse_group(&mut self, open: &Token) -> PResult<Expr> {
        let (close, unmatched) = if open.tok == Tok::LBrace {
            (Tok::RBrace, SyntaxErrorCode::UnmatchedBracket)
        } else {
            (Tok::RParen, SyntaxErrorCode::UnmatchedParenthesis)
        };
        if self.peek() == Some(&close) {
            let t = self.bump().expect("peeked");
            return err(SyntaxErrorCode::UnexpectedToken, t.span.clone());
        }
        if self.peek().is_none() {
            return err(unmatched, open.span.clone());
        }
        let saved = self.abs_depth;
        self.abs_depth = 0;
        let inner = self.parse_sum()?;
        self.abs_depth = saved;
        match self.peek_token() {
            Some(t) if t.tok == close => {
                self.pos += 1;
                Ok(inner)
            }
            None => err(unmatched, open.span.clone()),
            Some(t) => Err(self.unexpected(t)),
        }
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        self.nested(|p| p.parse_primary_inner())
    }

    fn parse_primary_inner(&mut self) -> PResult<Expr> {
        let Some(t) = self.bump() else {
            return err(SyntaxErrorCode::UnexpectedEndOfExpression, self.eof_span());
        };
        match &t.tok {
            Tok::Num(v) => Ok(Expr::Num(*v)),
            Tok::Var(n) => Ok(Expr::Var(n.clone())),
            Tok::X => Ok(Expr::X),
            Tok::Y => Ok(Expr::Y),
            Tok::Const(c) => Ok(Expr::Const(*c)),
            Tok::LParen | Tok::LBrace => self.parse_group(t),
            Tok::Pipe => {
                if self.peek().is_none() {
                    return err(SyntaxErrorCode::UnexpectedEndOfExpression, self.eof_span());
                }
                self.abs_depth += 1;
                let inner = self.parse_sum()?;
                self.abs_depth -= 1;
                match self.peek_token() {
                    Some(c) if c.tok == Tok::Pipe => {
                        self.pos += 1;
                        Ok(Expr::Call(Func::Abs, vec![inner]))
                    }
                    None => err(SyntaxErrorCode::InvalidEquationSyntax, t.span.clone()),
                    Some(c) => Err(self.unexpected(c)),
                }
            }
            Tok::Sqrt => Ok(Expr::Call(Func::Sqrt, vec![self.parse_power()?])),
            Tok::Cbrt => Ok(Expr::Call(Func::Cbrt, vec![self.parse_power()?])),
            Tok::FourthRoot => Ok(Expr::Call(
                Func::Root,
                vec![self.parse_power()?, Expr::Num(4.0)],
            )),
            Tok::Func(f) => self.parse_function(*f, t.span.clone()),
            Tok::LogSub => {
                let base = match self.peek_token() {
                    None => {
                        return err(SyntaxErrorCode::UnexpectedEndOfExpression, self.eof_span());
                    }
                    Some(b) => match &b.tok {
                        Tok::Num(v) => {
                            self.pos += 1;
                            Expr::Num(*v)
                        }
                        Tok::Var(n) => {
                            self.pos += 1;
                            Expr::Var(n.clone())
                        }
                        Tok::X => {
                            self.pos += 1;
                            Expr::X
                        }
                        Tok::Y => {
                            self.pos += 1;
                            Expr::Y
                        }
                        Tok::Const(c) => {
                            self.pos += 1;
                            Expr::Const(*c)
                        }
                        Tok::LParen | Tok::LBrace => {
                            self.pos += 1;
                            self.parse_group(b)?
                        }
                        _ => return Err(self.unexpected(b)),
                    },
                };
                let arg = self.parse_function_args(Func::Log, t.span.clone())?;
                if arg.len() != 1 {
                    return err(
                        SyntaxErrorCode::IncorrectNumParameter,
                        t.span.start..self.prev_end(),
                    );
                }
                Ok(Expr::Call(
                    Func::LogBase,
                    vec![base, arg.into_iter().next().expect("one arg")],
                ))
            }
            _ => Err(self.unexpected(t)),
        }
    }

    /// Parses the argument list of a function: either parenthesised and
    /// comma separated, or an unparenthesised implicit-product chain.
    fn parse_function_args(&mut self, _f: Func, name_span: Range<usize>) -> PResult<Vec<Expr>> {
        match self.peek_token() {
            None => err(SyntaxErrorCode::UnexpectedEndOfExpression, self.eof_span()),
            Some(open) if open.tok == Tok::LParen || open.tok == Tok::LBrace => {
                self.pos += 1;
                let close = if open.tok == Tok::LBrace {
                    Tok::RBrace
                } else {
                    Tok::RParen
                };
                let unmatched = if open.tok == Tok::LBrace {
                    SyntaxErrorCode::UnmatchedBracket
                } else {
                    SyntaxErrorCode::UnmatchedParenthesis
                };
                let saved = self.abs_depth;
                self.abs_depth = 0;
                let mut args = Vec::new();
                loop {
                    match self.peek_token() {
                        None => return err(unmatched, open.span.clone()),
                        Some(t) if t.tok == close => {
                            return err(SyntaxErrorCode::UnexpectedToken, t.span.clone());
                        }
                        Some(t) if t.tok == Tok::Comma => {
                            return err(SyntaxErrorCode::UnexpectedToken, t.span.clone());
                        }
                        _ => {}
                    }
                    args.push(self.parse_sum()?);
                    match self.peek_token() {
                        Some(t) if t.tok == Tok::Comma => {
                            self.pos += 1;
                        }
                        Some(t) if t.tok == close => {
                            self.pos += 1;
                            break;
                        }
                        None => return err(unmatched, open.span.clone()),
                        Some(t) => return Err(self.unexpected(t)),
                    }
                }
                self.abs_depth = saved;
                Ok(args)
            }
            Some(t) => {
                if !matches!(
                    t.tok,
                    Tok::Minus
                        | Tok::Plus
                        | Tok::Num(_)
                        | Tok::Var(_)
                        | Tok::X
                        | Tok::Y
                        | Tok::Const(_)
                        | Tok::Func(_)
                        | Tok::LogSub
                        | Tok::Sqrt
                        | Tok::Cbrt
                        | Tok::FourthRoot
                        | Tok::Pipe
                ) {
                    let _ = name_span;
                    return Err(self.unexpected(t));
                }
                let arg = match t.tok {
                    Tok::Minus => {
                        self.pos += 1;
                        Expr::Neg(Box::new(self.parse_juxt(true, false)?))
                    }
                    Tok::Plus => {
                        self.pos += 1;
                        self.parse_juxt(true, false)?
                    }
                    _ => self.parse_juxt(true, false)?,
                };
                Ok(vec![arg])
            }
        }
    }

    fn parse_function(&mut self, f: Func, name_span: Range<usize>) -> PResult<Expr> {
        let mut power = None;
        if self.peek() == Some(&Tok::Caret) {
            self.pos += 1;
            power = Some(self.parse_exponent()?);
        }
        let args = self.parse_function_args(f, name_span.clone())?;
        let full_span = name_span.start..self.prev_end();
        let f = match (f, args.len()) {
            (Func::Log, 2) => Func::LogBase,
            (f, _) => f,
        };
        let (min, max) = if f == Func::Log { (1, 2) } else { f.arity() };
        if args.len() < min || args.len() > max {
            return err(SyntaxErrorCode::IncorrectNumParameter, full_span);
        }
        let call = Expr::Call(f, args);
        Ok(match power {
            None => call,
            Some(p) => {
                if is_minus_one(&p)
                    && let Some(inv) = f.inverse()
                {
                    let Expr::Call(_, args) = call else {
                        unreachable!()
                    };
                    return Ok(Expr::Call(inv, args));
                }
                Expr::bin(BinOp::Pow, call, p)
            }
        })
    }
}

fn is_minus_one(e: &Expr) -> bool {
    match e {
        Expr::Neg(a) => matches!(**a, Expr::Num(v) if v == 1.0),
        Expr::Num(v) => *v == -1.0,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    fn f(s: &str) -> String {
        parse_expression(s)
            .unwrap_or_else(|e| panic!("{s}: {e}"))
            .formula()
    }

    fn code(s: &str) -> SyntaxErrorCode {
        match parse_input(s, ParseOptions::default()).unwrap_err().code {
            ErrorCode::Syntax(c) => c,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn precedence() {
        assert_eq!(f("1+2*3"), "Add(1,Mul(2,3))");
        assert_eq!(f("2x^2"), "Mul(2,Pow(x,2))");
        assert_eq!(f("-x^2"), "Neg(Pow(x,2))");
        assert_eq!(f("2^-x"), "Pow(2,Neg(x))");
        assert_eq!(f("2^3^2"), "Pow(2,Pow(3,2))");
        assert_eq!(f("1/2x"), "Div(1,Mul(2,x))");
        assert_eq!(f("1/2 x"), "Mul(Div(1,2),x)");
        assert_eq!(f("x/2 sin(x)"), "Mul(Div(x,2),sin(x))");
        assert_eq!(f("1/(2 x)"), "Div(1,Mul(2,x))");
        assert_eq!(f("2 x"), "Mul(2,x)");
        assert_eq!(f("a/b/c"), "Div(Div(a,b),c)");
        assert_eq!(f("x-1-2"), "Sub(Sub(x,1),2)");
        assert_eq!(f("2*-3"), "Mul(2,Neg(3))");
        assert_eq!(f("e^2x"), "Mul(Pow(e,2),x)");
        assert_eq!(f("e^(2x)"), "Pow(e,Mul(2,x))");
        assert_eq!(f("x mod 3"), "mod(x,3)");
        assert_eq!(f("mod(x,3)"), "mod(x,3)");
    }

    #[test]
    fn implicit_multiplication() {
        assert_eq!(f("2x"), "Mul(2,x)");
        assert_eq!(f("xy"), "Mul(x,y)");
        assert_eq!(f("2π"), "Mul(2,pi)");
        assert_eq!(f("3sin(x)"), "Mul(3,sin(x))");
        assert_eq!(f("(x+1)(x-1)"), "Mul(Add(x,1),Sub(x,1))");
        assert_eq!(f("x(x+1)"), "Mul(x,Add(x,1))");
        assert_eq!(f("2(3)"), "Mul(2,3)");
        assert_eq!(f("ab"), "Mul(a,b)");
        assert_eq!(f("2xsin(x)"), "Mul(Mul(2,x),sin(x))");
    }

    #[test]
    fn functions_without_parentheses() {
        assert_eq!(f("sin x"), "sin(x)");
        assert_eq!(f("sin 2x"), "sin(Mul(2,x))");
        assert_eq!(f("sin x cos x"), "Mul(sin(x),cos(x))");
        assert_eq!(f("sin x^2"), "sin(Pow(x,2))");
        assert_eq!(f("sin x/2"), "Div(sin(x),2)");
        assert_eq!(f("ln x + 1"), "Add(ln(x),1)");
        assert_eq!(f("sinx"), "sin(x)");
        assert_eq!(f("sin^2 x"), "Pow(sin(x),2)");
        assert_eq!(f("sin²(x)"), "Pow(sin(x),2)");
        assert_eq!(f("sin^-1(x)"), "arcsin(x)");
        assert_eq!(f("sin⁻¹(x)"), "arcsin(x)");
        assert_eq!(f("cosh⁻¹ x"), "arccosh(x)");
        assert_eq!(f("sin -x"), "sin(Neg(x))");
    }

    #[test]
    fn numpad_function_forms() {
        assert_eq!(f("arcsin(x)"), "arcsin(x)");
        assert_eq!(f("arccoth(x)"), "arccoth(x)");
        assert_eq!(f("ceiling(x)"), "ceiling(x)");
        assert_eq!(f("root(x, 3)"), "root(x,3)");
        assert_eq!(f("log(2, x)"), "log(2,x)");
        assert_eq!(f("log(x)"), "log(x)");
        assert_eq!(f("log_2(x)"), "log(2,x)");
        assert_eq!(f("ln(x)"), "ln(x)");
        assert_eq!(f("√x"), "sqrt(x)");
        assert_eq!(f("√(x+1)"), "sqrt(Add(x,1))");
        assert_eq!(f("∛x"), "cbrt(x)");
        assert_eq!(f("10^x"), "Pow(10,x)");
        assert_eq!(f("1/x"), "Div(1,x)");
        assert_eq!(f("x²"), "Pow(x,2)");
        assert_eq!(f("x³+1"), "Add(Pow(x,3),1)");
        assert_eq!(f("x!"), "factorial(x)");
        assert_eq!(f("x!!"), "factorial2(x)");
        assert_eq!(f("3x!"), "Mul(3,factorial(x))");
        assert_eq!(f("max(x, 1, 2)"), "max(x,1,2)");
        assert_eq!(f("2 × 3 ÷ 4 − 1"), "Sub(Div(Mul(2,3),4),1)");
    }

    #[test]
    fn absolute_value_bars() {
        assert_eq!(f("|x|"), "abs(x)");
        assert_eq!(f("2|x|"), "Mul(2,abs(x))");
        assert_eq!(f("|x||y|"), "Mul(abs(x),abs(y))");
        assert_eq!(f("||x|-|y||"), "abs(Sub(abs(x),abs(y)))");
        assert_eq!(f("|x-1|+|x+1|"), "Add(abs(Sub(x,1)),abs(Add(x,1)))");
        assert_eq!(f("|x(|y|)|"), "abs(Mul(x,abs(y)))");
        assert_eq!(f("abs(x)"), "abs(x)");
    }

    #[test]
    fn relations() {
        let p = parse_input("y = x^2", ParseOptions::default()).unwrap();
        assert_eq!(p.sides.len(), 2);
        assert_eq!(p.rels[0].0, RelOp::Eq);
        let p = parse_input("1 < y <= 3", ParseOptions::default()).unwrap();
        assert_eq!(
            p.rels.iter().map(|r| r.0).collect::<Vec<_>>(),
            vec![RelOp::Lt, RelOp::Le]
        );
        let p = parse_input("y ≥ x", ParseOptions::default()).unwrap();
        assert_eq!(p.rels[0].0, RelOp::Ge);
        let p = parse_input("f(x) = x^2", ParseOptions::default()).unwrap();
        assert_eq!(p.function_name.as_deref(), Some("f"));
        assert_eq!(p.sides[0], Expr::Y);
        // Not a function definition: multiplication.
        let p = parse_input("a(x+1) = y", ParseOptions::default()).unwrap();
        assert_eq!(p.function_name, None);
        assert_eq!(p.sides[0].formula(), "Mul(a,Add(x,1))");
    }

    #[test]
    fn errors() {
        assert_eq!(code(""), SyntaxErrorCode::EmptyExpression);
        assert_eq!(code("   "), SyntaxErrorCode::EmptyExpression);
        assert_eq!(code("(x+1"), SyntaxErrorCode::UnmatchedParenthesis);
        assert_eq!(code("x+1)"), SyntaxErrorCode::ParenthesisMismatch);
        assert_eq!(code("x+)"), SyntaxErrorCode::ParenthesisMismatch);
        assert_eq!(code("{x+1"), SyntaxErrorCode::UnmatchedBracket);
        assert_eq!(code("x}"), SyntaxErrorCode::BracketMismatch);
        assert_eq!(code("3-*4"), SyntaxErrorCode::UnexpectedToken);
        assert_eq!(code("3-4*"), SyntaxErrorCode::UnexpectedEndOfExpression);
        assert_eq!(code("y="), SyntaxErrorCode::UnexpectedEndOfExpression);
        assert_eq!(code("=x"), SyntaxErrorCode::EqualWithoutEquation);
        assert_eq!(code("7.3.2"), SyntaxErrorCode::TooManyDecimalPoints);
        assert_eq!(code("x+[1]"), SyntaxErrorCode::InvalidToken);
        assert_eq!(code("3,5"), SyntaxErrorCode::InvalidToken);
        assert_eq!(code("x#"), SyntaxErrorCode::InvalidToken);
        assert_eq!(code("root(x)"), SyntaxErrorCode::IncorrectNumParameter);
        assert_eq!(code("sin(x, 2)"), SyntaxErrorCode::IncorrectNumParameter);
        assert_eq!(code("y = i x"), SyntaxErrorCode::CannotUseIInReal);
        assert_eq!(code("x_"), SyntaxErrorCode::InvalidVariableNameFormat);
        assert_eq!(code("sin()"), SyntaxErrorCode::UnexpectedToken);
        assert_eq!(code("(y=x)"), SyntaxErrorCode::InvalidEquationFormat);
    }

    #[test]
    fn error_positions() {
        let e = parse_input("y = (x + 1", ParseOptions::default()).unwrap_err();
        assert_eq!(e.span, 4..5);
        let e = parse_input("y = x + 1)", ParseOptions::default()).unwrap_err();
        assert_eq!(e.span, 9..10);
        let e = parse_input("y = 2 * * x", ParseOptions::default()).unwrap_err();
        assert_eq!(e.span, 8..9);
        let e = parse_input("y = sin(x", ParseOptions::default()).unwrap_err();
        assert_eq!(e.span, 7..8);
        assert_eq!(e.message(), "The equation is missing a closing parenthesis");
    }

    #[test]
    fn display_round_trips() {
        for s in [
            "2x^2+sin(x)",
            "-x^2",
            "1/(2x)",
            "(x+1)(x-1)",
            "2^(-x)",
            "|x|-3",
            "x!",
            "e^(x/2)",
            "a-(-b)",
            "(-2)^x",
        ] {
            let e = parse_expression(s).unwrap();
            let again = parse_expression(&e.to_string()).unwrap();
            assert_eq!(e, again, "{s} -> {e}");
        }
    }
}
