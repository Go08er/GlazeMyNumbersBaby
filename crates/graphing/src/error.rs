//! Error types for equation parsing / graphing.
//!
//! The numeric codes and their user-facing messages mirror
//! `GraphControl/Models/Equation.h` (`ErrorType`, `EvaluationErrorCode`,
//! `SyntaxErrorCode`) and `EquationViewModel.EquationErrorText` from the
//! original application.

use crate::strings as s;
use std::fmt;
use std::ops::Range;

/// Category of an equation error (`GraphControl::ErrorType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorType {
    Evaluation = 0,
    Syntax = 1,
    Abort = 2,
}

/// `GraphControl::EvaluationErrorCode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum EvaluationErrorCode {
    Overflow = 2,
    RequireRadiansMode = 3,
    TooComplexToSolve = 4,
    RequireDegreesMode = 5,
    FactorialInvalidArgument = -1,
    Factorial2InvalidArgument = -2,
    FactorialCannotPerformOnLargeNumber = -3,
    ModuloCannotPerformOnFloat = -5,
    EquationTooComplexToSolveSymbolic = -7,
    EquationHasNoSolution = -8,
    EquationTooComplexToSolve = -9,
    EquationTooComplexToPlot = -10,
    DivideByZero = -15,
    InequalityTooComplexToSolve = -41,
    InequalityHasNoSolution = -42,
    MutuallyExclusiveConditions = -43,
    OutOfDomain = -101,
    GeNotSupported = -503,
    GeGeneralError = -504,
    GeTooComplexToSolve = -506,
}

/// `GraphControl::SyntaxErrorCode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum SyntaxErrorCode {
    /// Found `)` without matching `(`.
    ParenthesisMismatch = 1,
    /// Found `(` without matching `)`.
    UnmatchedParenthesis = 2,
    /// More than one decimal point in a number, e.g. `7.3.2`.
    TooManyDecimalPoints = 3,
    /// A decimal point without digits, e.g. `3+.+4`.
    DecimalPointWithoutDigits = 4,
    /// e.g. `3-4*`.
    UnexpectedEndOfExpression = 5,
    /// e.g. `3-*4`.
    UnexpectedToken = 6,
    /// e.g. `[`, `#`, or a stray comma.
    InvalidToken = 7,
    /// e.g. `y = x = 2`.
    TooManyEquals = 8,
    /// e.g. `4 + 83 = 9`.
    EqualWithoutGraphVariable = 10,
    InvalidEquationSyntax = 11,
    /// Nothing in the expression.
    EmptyExpression = 12,
    EqualWithoutEquation = 14,
    InvalidEquationFormat = 15,
    ExpectParenthesisAfterFunctionName = 25,
    /// e.g. `root(a)`.
    IncorrectNumParameter = 26,
    /// e.g. `x_`.
    InvalidVariableNameFormat = 32,
    /// Found `}` without matching `{`.
    BracketMismatch = 34,
    /// Found `{` without matching `}`.
    UnmatchedBracket = 35,
    /// `i` and `I` cannot be used as variable names in the real number field.
    CannotUseIInReal = 48,
    GeneralError = 52,
}

/// The error code of an [`EquationError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// A syntax error found while parsing.
    Syntax(SyntaxErrorCode),
    /// A (constant) evaluation error found while compiling.
    Evaluation(EvaluationErrorCode),
    /// "The equation could not be graphed" (`GeneralError`), used e.g. for a
    /// bare expression that contains both `x` and `y` (which the original
    /// `Equation::GetRequest` refuses to graph).
    General,
}

/// An error attached to an equation, with the character span (indices into
/// the input's `char`s, not bytes) that caused it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquationError {
    pub code: ErrorCode,
    /// Character range in the input text (char indices). May be empty
    /// (`start == end`) for errors at a position, e.g. end of input.
    pub span: Range<usize>,
}

impl EquationError {
    pub(crate) fn syntax(code: SyntaxErrorCode, span: Range<usize>) -> Self {
        EquationError {
            code: ErrorCode::Syntax(code),
            span,
        }
    }

    pub(crate) fn eval(code: EvaluationErrorCode, span: Range<usize>) -> Self {
        EquationError {
            code: ErrorCode::Evaluation(code),
            span,
        }
    }

    /// The error category, as in `GraphControl::ErrorType`.
    pub fn error_type(&self) -> ErrorType {
        match self.code {
            ErrorCode::Syntax(_) => ErrorType::Syntax,
            ErrorCode::Evaluation(_) | ErrorCode::General => ErrorType::Evaluation,
        }
    }

    /// The raw numeric error code, as in `GraphControl` (0 for `General`).
    pub fn raw_code(&self) -> i32 {
        match self.code {
            ErrorCode::Syntax(c) => c as i32,
            ErrorCode::Evaluation(c) => c as i32,
            ErrorCode::General => 0,
        }
    }

    /// The en-US message shown under the equation box.
    pub fn message(&self) -> &'static str {
        equation_error_text(self.error_type(), self.raw_code())
    }
}

impl fmt::Display for EquationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (at {}..{})",
            self.message(),
            self.span.start,
            self.span.end
        )
    }
}

impl std::error::Error for EquationError {}

/// Port of `EquationViewModel.EquationErrorText(errorType, errorCode)`.
pub fn equation_error_text(error_type: ErrorType, code: i32) -> &'static str {
    match error_type {
        ErrorType::Evaluation => match code {
            2 => s::OVERFLOW,
            3 => s::REQUIRE_RADIANS_MODE,
            4 | -9 | -7 | -10 | -41 | -506 => s::TOO_COMPLEX_TO_SOLVE,
            5 => s::REQUIRE_DEGREES_MODE,
            -1 | -2 => s::FACTORIAL_INVALID_ARGUMENT,
            -3 => s::FACTORIAL_CANNOT_PERFORM_ON_LARGE_NUMBER,
            -5 => s::MODULO_CANNOT_PERFORM_ON_FLOAT,
            -8 | -42 => s::EQUATION_HAS_NO_SOLUTION,
            -15 => s::DIVIDE_BY_ZERO,
            -43 => s::MUTUALLY_EXCLUSIVE_CONDITIONS,
            -101 => s::OUT_OF_DOMAIN,
            -503 => s::GE_NOT_SUPPORTED,
            _ => s::GENERAL_ERROR,
        },
        ErrorType::Syntax => match code {
            1 => s::PARENTHESIS_MISMATCH,
            2 => s::UNMATCHED_PARENTHESIS,
            3 => s::TOO_MANY_DECIMAL_POINTS,
            4 => s::DECIMAL_POINT_WITHOUT_DIGITS,
            5 => s::UNEXPECTED_END_OF_EXPRESSION,
            6 => s::UNEXPECTED_TOKEN,
            7 => s::INVALID_TOKEN,
            8 => s::TOO_MANY_EQUALS,
            10 => s::EQUAL_WITHOUT_GRAPH_VARIABLE,
            11 | 15 => s::INVALID_EQUATION_SYNTAX,
            12 => s::EMPTY_EXPRESSION,
            14 => s::EQUAL_WITHOUT_EQUATION,
            25 => s::EXPECT_PARENTHESIS_AFTER_FUNCTION_NAME,
            26 => s::INCORRECT_NUM_PARAMETER,
            32 => s::INVALID_VARIABLE_NAME_FORMAT,
            34 => s::BRACKET_MISMATCH,
            35 => s::UNMATCHED_BRACKET,
            48 => s::CANNOT_USE_I_IN_REAL,
            55 => s::INVALID_NUMBER_DIGIT,
            56 => s::INVALID_NUMBER_BASE,
            57 => s::INVALID_VARIABLE_SPECIFICATION,
            58 | 59 => s::EXPECTING_LOGICAL_OPERANDS,
            61 => s::CANNOT_USE_INDEX_VAR_IN_OP_LIMITS,
            62 => s::OVERFLOW,
            72 => s::CANNOT_USE_COMPLEX_INFINITY_IN_REAL,
            123 => s::CANNOT_USE_I_IN_INEQUALITY_SOLVING,
            _ => s::GENERAL_ERROR,
        },
        ErrorType::Abort => s::GENERAL_ERROR,
    }
}
