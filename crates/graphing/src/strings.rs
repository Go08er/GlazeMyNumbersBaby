//! User-facing en-US strings used by the graphing feature.
//!
//! These are copied verbatim (typos included, e.g. "aysmptotes") from the
//! original application's `Calculator/Resources/en-US/Resources.resw` so that
//! the port shows exactly the same text. Each constant is named after its
//! resource key; [`resource`] looks a key up at runtime.

/// Key graph feature panel titles.
pub const DOMAIN: &str = "Domain";
pub const RANGE: &str = "Range";
pub const X_INTERCEPT: &str = "X-Intercept";
pub const Y_INTERCEPT: &str = "Y-Intercept";
pub const MINIMA: &str = "Minima";
pub const MAXIMA: &str = "Maxima";
pub const INFLECTION_POINTS: &str = "Inflection points";
pub const VERTICAL_ASYMPTOTES: &str = "Vertical asymptotes";
pub const HORIZONTAL_ASYMPTOTES: &str = "Horizontal asymptotes";
pub const OBLIQUE_ASYMPTOTES: &str = "Oblique asymptotes";
pub const PARITY: &str = "Parity";
pub const PERIODICITY: &str = "Period";
pub const MONOTONICITY: &str = "Monotonicity";

/// Key graph feature "none"/status strings.
pub const KGF_ANALYSIS_COULD_NOT_BE_PERFORMED: &str =
    "Analysis could not be performed for the function.";
pub const KGF_ANALYSIS_NOT_SUPPORTED: &str = "Analysis is not supported for this function.";
/// Not an upstream string: GMNB bounds the work one analysis may do.
pub const KGF_ANALYSIS_TOO_COMPLEX: &str =
    "This function is too complex for Calculator to analyze.";
pub const KGF_VARIABLE_IS_NOT_X: &str =
    "Analysis is only supported for functions in the f(x) format. Example: y=x";
pub const KGF_DOMAIN_NONE: &str = "Unable to calculate the domain for this function.";
pub const KGF_RANGE_NONE: &str = "Unable to calculate the range for this function.";
pub const KGF_X_INTERCEPT_NONE: &str = "The function does not have any x-intercepts.";
pub const KGF_Y_INTERCEPT_NONE: &str = "The function does not have any y-intercepts.";
pub const KGF_MINIMA_NONE: &str = "The function does not have any minima points.";
pub const KGF_MAXIMA_NONE: &str = "The function does not have any maxima points.";
pub const KGF_INFLECTION_POINTS_NONE: &str = "The function does not have any inflection points.";
pub const KGF_VERTICAL_ASYMPTOTES_NONE: &str =
    "The function does not have any vertical asymptotes.";
pub const KGF_HORIZONTAL_ASYMPTOTES_NONE: &str =
    "The function does not have any horizontal asymptotes.";
pub const KGF_OBLIQUE_ASYMPTOTES_NONE: &str = "The function does not have any oblique aysmptotes.";
pub const KGF_PARITY_ODD: &str = "The function is odd.";
pub const KGF_PARITY_EVEN: &str = "The function is even.";
pub const KGF_PARITY_NEITHER: &str = "The function is neither even nor odd.";
pub const KGF_PARITY_UNKNOWN: &str = "The function parity is unknown.";
pub const KGF_PERIODICITY_UNKNOWN: &str = "The function periodicity is unknown.";
pub const KGF_PERIODICITY_NOT_PERIODIC: &str = "The function is not periodic.";
pub const KGF_PERIODICITY_ERROR: &str = "Periodicity is not supported for this function.";
pub const KGF_MONOTONICITY_INCREASING: &str = "Increasing";
pub const KGF_MONOTONICITY_DECREASING: &str = "Decreasing";
pub const KGF_MONOTONICITY_CONSTANT: &str = "Constant";
pub const KGF_MONOTONICITY_UNKNOWN: &str = "The monotonicity of the function is unknown.";
pub const KGF_MONOTONICITY_ERROR: &str = "Unable to determine the monotonicity of the function.";
pub const KGF_TOO_COMPLEX_FEATURES_ERROR: &str =
    "These features are too complex for Calculator to calculate:";

/// Equation error strings (evaluation errors).
pub const OVERFLOW: &str = "Overflow (the number is too large)";
pub const REQUIRE_RADIANS_MODE: &str = "Radians mode is required to graph this equation.";
pub const TOO_COMPLEX_TO_SOLVE: &str = "This function is too complex to graph";
pub const REQUIRE_DEGREES_MODE: &str = "Degrees mode is required to graph this function";
pub const FACTORIAL_INVALID_ARGUMENT: &str = "The factorial function has an invalid argument";
pub const FACTORIAL_CANNOT_PERFORM_ON_LARGE_NUMBER: &str =
    "The factorial function has an argument that is too large to graph";
pub const MODULO_CANNOT_PERFORM_ON_FLOAT: &str = "Modulo can only be used with whole numbers";
pub const EQUATION_HAS_NO_SOLUTION: &str = "The equation has no solution";
pub const DIVIDE_BY_ZERO: &str = "Cannot divide by zero";
pub const MUTUALLY_EXCLUSIVE_CONDITIONS: &str =
    "The equation contains logical conditions that are mutually exclusive";
pub const OUT_OF_DOMAIN: &str = "Equation is out of domain";
pub const GE_NOT_SUPPORTED: &str = "Graphing this equation is not supported";
pub const GENERAL_ERROR: &str = "The equation could not be graphed";

/// Equation error strings (syntax errors).
pub const PARENTHESIS_MISMATCH: &str = "The equation is missing an opening parenthesis";
pub const UNMATCHED_PARENTHESIS: &str = "The equation is missing a closing parenthesis";
pub const TOO_MANY_DECIMAL_POINTS: &str = "There are too many decimal points in a number";
pub const DECIMAL_POINT_WITHOUT_DIGITS: &str = "A decimal point is missing digits";
pub const UNEXPECTED_END_OF_EXPRESSION: &str = "Unexpected end of expression";
pub const UNEXPECTED_TOKEN: &str = "Unexpected characters in the expression";
pub const INVALID_TOKEN: &str = "Invalid characters in the expression";
pub const TOO_MANY_EQUALS: &str = "There are too many equal signs";
pub const EQUAL_WITHOUT_GRAPH_VARIABLE: &str =
    "The function must contain at least one x or y variable";
pub const INVALID_EQUATION_SYNTAX: &str = "Invalid expression";
pub const EMPTY_EXPRESSION: &str = "The expression is empty";
pub const EQUAL_WITHOUT_EQUATION: &str = "Equal was used without an equation";
pub const EXPECT_PARENTHESIS_AFTER_FUNCTION_NAME: &str = "Parenthesis missing after function name";
pub const INCORRECT_NUM_PARAMETER: &str =
    "A mathematical operation has the incorrect number of parameters";
pub const INVALID_VARIABLE_NAME_FORMAT: &str = "A variable name is invalid";
pub const BRACKET_MISMATCH: &str = "The equation is missing an opening bracket";
pub const UNMATCHED_BRACKET: &str = "The equation is missing a closing bracket";
pub const CANNOT_USE_I_IN_REAL: &str = "\"i\" and \"I\" cannot be used as variable names";
pub const INVALID_NUMBER_DIGIT: &str = "The digit could not be resolved for the given base";
pub const INVALID_NUMBER_BASE: &str = "The base must be greater than 2 and less than 36";
pub const INVALID_VARIABLE_SPECIFICATION: &str =
    "A mathematical operation requires one of its parameters to be a variable";
pub const EXPECTING_LOGICAL_OPERANDS: &str = "Equation is mixing logical and scalar operands";
pub const CANNOT_USE_INDEX_VAR_IN_OP_LIMITS: &str =
    "x or y cannot be used in the upper or lower limits";
pub const CANNOT_USE_COMPLEX_INFINITY_IN_REAL: &str = "Cannot use complex infinity";
pub const CANNOT_USE_I_IN_INEQUALITY_SOLVING: &str = "Cannot use complex numbers in inequalities";

/// Miscellaneous graphing strings.
pub const VARIABLE_LIST_VIEW_ITEM: &str = "Variable %1 list item";
pub const KEY_GRAPH_FEATURES_LABEL: &str = "Function analysis";

/// Every resource key known to this crate together with its en-US value.
pub const RESOURCES: &[(&str, &str)] = &[
    ("Domain", DOMAIN),
    ("Range", RANGE),
    ("XIntercept", X_INTERCEPT),
    ("YIntercept", Y_INTERCEPT),
    ("Minima", MINIMA),
    ("Maxima", MAXIMA),
    ("InflectionPoints", INFLECTION_POINTS),
    ("VerticalAsymptotes", VERTICAL_ASYMPTOTES),
    ("HorizontalAsymptotes", HORIZONTAL_ASYMPTOTES),
    ("ObliqueAsymptotes", OBLIQUE_ASYMPTOTES),
    ("Parity", PARITY),
    ("Periodicity", PERIODICITY),
    ("Monotonicity", MONOTONICITY),
    (
        "KGFAnalysisCouldNotBePerformed",
        KGF_ANALYSIS_COULD_NOT_BE_PERFORMED,
    ),
    ("KGFAnalysisNotSupported", KGF_ANALYSIS_NOT_SUPPORTED),
    ("KGFVariableIsNotX", KGF_VARIABLE_IS_NOT_X),
    ("KGFDomainNone", KGF_DOMAIN_NONE),
    ("KGFRangeNone", KGF_RANGE_NONE),
    ("KGFXInterceptNone", KGF_X_INTERCEPT_NONE),
    ("KGFYInterceptNone", KGF_Y_INTERCEPT_NONE),
    ("KGFMinimaNone", KGF_MINIMA_NONE),
    ("KGFMaximaNone", KGF_MAXIMA_NONE),
    ("KGFInflectionPointsNone", KGF_INFLECTION_POINTS_NONE),
    ("KGFVerticalAsymptotesNone", KGF_VERTICAL_ASYMPTOTES_NONE),
    (
        "KGFHorizontalAsymptotesNone",
        KGF_HORIZONTAL_ASYMPTOTES_NONE,
    ),
    ("KGFObliqueAsymptotesNone", KGF_OBLIQUE_ASYMPTOTES_NONE),
    ("KGFParityOdd", KGF_PARITY_ODD),
    ("KGFParityEven", KGF_PARITY_EVEN),
    ("KGFParityNeither", KGF_PARITY_NEITHER),
    ("KGFParityUnknown", KGF_PARITY_UNKNOWN),
    ("KGFPeriodicityUnknown", KGF_PERIODICITY_UNKNOWN),
    ("KGFPeriodicityNotPeriodic", KGF_PERIODICITY_NOT_PERIODIC),
    ("KGFPeriodicityError", KGF_PERIODICITY_ERROR),
    ("KGFMonotonicityIncreasing", KGF_MONOTONICITY_INCREASING),
    ("KGFMonotonicityDecreasing", KGF_MONOTONICITY_DECREASING),
    ("KGFMonotonicityConstant", KGF_MONOTONICITY_CONSTANT),
    ("KGFMonotonicityUnknown", KGF_MONOTONICITY_UNKNOWN),
    ("KGFMonotonicityError", KGF_MONOTONICITY_ERROR),
    ("KGFTooComplexFeaturesError", KGF_TOO_COMPLEX_FEATURES_ERROR),
    ("Overflow", OVERFLOW),
    ("RequireRadiansMode", REQUIRE_RADIANS_MODE),
    ("TooComplexToSolve", TOO_COMPLEX_TO_SOLVE),
    ("RequireDegreesMode", REQUIRE_DEGREES_MODE),
    ("FactorialInvalidArgument", FACTORIAL_INVALID_ARGUMENT),
    (
        "FactorialCannotPerformOnLargeNumber",
        FACTORIAL_CANNOT_PERFORM_ON_LARGE_NUMBER,
    ),
    ("ModuloCannotPerformOnFloat", MODULO_CANNOT_PERFORM_ON_FLOAT),
    ("EquationHasNoSolution", EQUATION_HAS_NO_SOLUTION),
    ("DivideByZero", DIVIDE_BY_ZERO),
    ("MutuallyExclusiveConditions", MUTUALLY_EXCLUSIVE_CONDITIONS),
    ("OutOfDomain", OUT_OF_DOMAIN),
    ("GE_NotSupported", GE_NOT_SUPPORTED),
    ("GeneralError", GENERAL_ERROR),
    ("ParenthesisMismatch", PARENTHESIS_MISMATCH),
    ("UnmatchedParenthesis", UNMATCHED_PARENTHESIS),
    ("TooManyDecimalPoints", TOO_MANY_DECIMAL_POINTS),
    ("DecimalPointWithoutDigits", DECIMAL_POINT_WITHOUT_DIGITS),
    ("UnexpectedEndOfExpression", UNEXPECTED_END_OF_EXPRESSION),
    ("UnexpectedToken", UNEXPECTED_TOKEN),
    ("InvalidToken", INVALID_TOKEN),
    ("TooManyEquals", TOO_MANY_EQUALS),
    ("EqualWithoutGraphVariable", EQUAL_WITHOUT_GRAPH_VARIABLE),
    ("InvalidEquationSyntax", INVALID_EQUATION_SYNTAX),
    ("EmptyExpression", EMPTY_EXPRESSION),
    ("EqualWithoutEquation", EQUAL_WITHOUT_EQUATION),
    (
        "ExpectParenthesisAfterFunctionName",
        EXPECT_PARENTHESIS_AFTER_FUNCTION_NAME,
    ),
    ("IncorrectNumParameter", INCORRECT_NUM_PARAMETER),
    ("InvalidVariableNameFormat", INVALID_VARIABLE_NAME_FORMAT),
    ("BracketMismatch", BRACKET_MISMATCH),
    ("UnmatchedBracket", UNMATCHED_BRACKET),
    ("CannotUseIInReal", CANNOT_USE_I_IN_REAL),
    ("InvalidNumberDigit", INVALID_NUMBER_DIGIT),
    ("InvalidNumberBase", INVALID_NUMBER_BASE),
    (
        "InvalidVariableSpecification",
        INVALID_VARIABLE_SPECIFICATION,
    ),
    ("ExpectingLogicalOperands", EXPECTING_LOGICAL_OPERANDS),
    (
        "CannotUseIndexVarInOpLimits",
        CANNOT_USE_INDEX_VAR_IN_OP_LIMITS,
    ),
    (
        "CannotUseComplexInfinityInReal",
        CANNOT_USE_COMPLEX_INFINITY_IN_REAL,
    ),
    (
        "CannotUseIInInequalitySolving",
        CANNOT_USE_I_IN_INEQUALITY_SOLVING,
    ),
    ("VariableListViewItem", VARIABLE_LIST_VIEW_ITEM),
    ("KeyGraphFeaturesLabel", KEY_GRAPH_FEATURES_LABEL),
];

/// Looks up an en-US resource string by its `.resw` key.
pub fn resource(key: &str) -> Option<&'static str> {
    RESOURCES.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}
