//! Equations as typed in the equation box, and their classification into
//! the forms the plotter and analyzer understand.
//!
//! Classification follows `GraphControl::Equation::GetRequest`:
//! any of `<`, `>`, `≤`, `≥` makes an inequality (the original sends it to
//! `plotIneq2D` and switches the line style to dashed), an `=` makes an
//! equation (`plotEq2d`), a bare expression containing both `x` and `y` is
//! refused, and any other bare expression is plotted as `y = expression`
//! (`plot2d`).

use crate::ast::{Expr, Func};
use crate::compile::{CompileOptions, Program};
use crate::error::{EquationError, ErrorCode, SyntaxErrorCode};
use crate::lexer::{ParseOptions, RelOp};
use crate::parser::{ParsedInput, parse_input};
use crate::simplify::{self, as_num, linear_in};

/// Line style of a curve (`GraphControl::EquationLineStyle`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum LineStyle {
    #[default]
    Solid,
    Dot,
    Dash,
    DashDot,
    DashDotDot,
}

/// Which way an explicit curve is parameterised.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    /// `y = f(x)`: x is the independent variable.
    X,
    /// `x = g(y)`: y is the independent variable.
    Y,
}

/// The broad kind of an equation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EquationKind {
    /// `y = f(x)`, `f(x) = …`, or a bare expression in x. Function analysis
    /// is available.
    Function,
    /// `x = g(y)`. Analysis reports "only supported for f(x)".
    InverseFunction,
    /// An implicit relation `F(x, y) = G(x, y)`, e.g. a circle.
    Implicit,
    /// An inequality (possibly chained, e.g. `1 < y < 3`) describing a
    /// shaded region.
    Inequality,
}

/// One condition of an inequality, normalised to `g < 0` (strict) or
/// `g ≤ 0`.
#[derive(Clone, Debug, PartialEq)]
pub struct Condition {
    /// The expression that must be negative (or non-positive).
    pub g: Expr,
    /// True for `<` / `>`.
    pub strict: bool,
}

/// An inequality with one side isolated: `y ⋚ f(x)` or `x ⋚ g(y)`.
#[derive(Clone, Debug, PartialEq)]
pub struct ExplicitBound {
    /// The isolated variable's axis (`Axis::X` means `y ⋚ f(x)`).
    pub axis: Axis,
    /// The bound, as a function of the independent variable.
    pub f: Expr,
    /// True if the region is above (for `Axis::X`) / right of (for
    /// `Axis::Y`) the bound.
    pub greater: bool,
    /// True for a strict inequality.
    pub strict: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Form {
    /// The curve `dependent = f(independent)`; for `Axis::Y` the expression
    /// is in terms of `y`.
    Explicit { axis: Axis, f: Expr },
    /// `f(x, y) = 0`.
    Implicit { f: Expr },
    /// Region where all conditions hold.
    Inequality {
        conditions: Vec<Condition>,
        explicit: Option<ExplicitBound>,
    },
}

/// A parsed, classified equation.
#[derive(Clone, Debug, PartialEq)]
pub struct Equation {
    text: String,
    /// How `text` was tokenized (its literals are read back the same way).
    parse: ParseOptions,
    form: Form,
    variables: Vec<String>,
    function_name: Option<String>,
    relations: Vec<RelOp>,
}

impl Equation {
    /// Parses and classifies equation text with default options.
    pub fn parse(text: &str) -> Result<Equation, EquationError> {
        Equation::parse_with(text, ParseOptions::default())
    }

    /// Parses and classifies equation text.
    pub fn parse_with(text: &str, opts: ParseOptions) -> Result<Equation, EquationError> {
        let parsed = parse_input(text, opts)?;
        let whole = 0..text.chars().count();
        let form = classify(&parsed, whole)?;
        let mut variables: Vec<String> = Vec::new();
        for side in &parsed.sides {
            for v in side.variables() {
                if !variables.contains(&v) {
                    variables.push(v);
                }
            }
        }
        Ok(Equation {
            text: text.to_string(),
            parse: opts,
            form,
            variables,
            function_name: parsed.function_name.clone(),
            relations: parsed.rels.iter().map(|r| r.0).collect(),
        })
    }

    /// The original text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The options `text` was parsed with.
    pub fn parse_options(&self) -> ParseOptions {
        self.parse
    }

    /// The kind of equation.
    pub fn kind(&self) -> EquationKind {
        match &self.form {
            Form::Explicit { axis: Axis::X, .. } => EquationKind::Function,
            Form::Explicit { axis: Axis::Y, .. } => EquationKind::InverseFunction,
            Form::Implicit { .. } => EquationKind::Implicit,
            Form::Inequality { .. } => EquationKind::Inequality,
        }
    }

    /// True for inequalities (`GraphControl::Equation::IsInequality`).
    pub fn is_inequality(&self) -> bool {
        matches!(self.form, Form::Inequality { .. })
    }

    /// The relation operators as typed (empty for a bare expression).
    pub fn relations(&self) -> &[RelOp] {
        &self.relations
    }

    /// For inequalities: true if every relation is strict (`<`, `>`), in
    /// which case the boundary is not part of the region and is
    /// conventionally drawn dashed.
    pub fn is_strict(&self) -> bool {
        match &self.form {
            Form::Inequality { conditions, .. } => conditions.iter().all(|c| c.strict),
            _ => false,
        }
    }

    /// The line style the original app assigns: `Dash` for inequalities,
    /// `Solid` otherwise.
    pub fn default_line_style(&self) -> LineStyle {
        if self.is_inequality() {
            LineStyle::Dash
        } else {
            LineStyle::Solid
        }
    }

    /// Parameter variables (everything except x and y), in order of first
    /// appearance.
    pub fn variables(&self) -> &[String] {
        &self.variables
    }

    /// `Some("f")` for input like `f(x) = x^2`.
    pub fn function_name(&self) -> Option<&str> {
        self.function_name.as_deref()
    }

    /// For explicit curves: the axis of the independent variable and the
    /// expression (`y = f(x)` → `(Axis::X, f)`; `x = g(y)` → `(Axis::Y, g)`
    /// with `g` written in terms of `y`).
    pub fn explicit(&self) -> Option<(Axis, &Expr)> {
        match &self.form {
            Form::Explicit { axis, f } => Some((*axis, f)),
            _ => None,
        }
    }

    /// For implicit relations: `F` such that the curve is `F(x, y) = 0`.
    pub fn implicit(&self) -> Option<&Expr> {
        match &self.form {
            Form::Implicit { f } => Some(f),
            _ => None,
        }
    }

    /// For inequalities: the normalised conditions.
    pub fn conditions(&self) -> Option<&[Condition]> {
        match &self.form {
            Form::Inequality { conditions, .. } => Some(conditions),
            _ => None,
        }
    }

    /// For inequalities with an isolated variable: the explicit bound.
    pub fn explicit_bound(&self) -> Option<&ExplicitBound> {
        match &self.form {
            Form::Inequality { explicit, .. } => explicit.as_ref(),
            _ => None,
        }
    }

    /// Compiles the equation for plotting/tracing with the given variable
    /// values and trig unit.
    pub fn compile(&self, opts: &CompileOptions<'_>) -> Result<CompiledEquation, EquationError> {
        let whole = 0..self.text.chars().count();
        let fix = |mut e: EquationError| {
            if e.span.is_empty() {
                e.span = whole.clone();
            }
            e
        };
        let form = match &self.form {
            Form::Explicit { axis, f } => {
                // For x = g(y) the program takes y through its "x" input.
                let expr = if *axis == Axis::Y {
                    f.swap_xy()
                } else {
                    f.clone()
                };
                CompiledForm::Explicit {
                    axis: *axis,
                    f: Program::compile(&expr, opts).map_err(fix)?,
                }
            }
            Form::Implicit { f } => CompiledForm::Implicit {
                f: Program::compile(f, opts).map_err(fix)?,
            },
            Form::Inequality {
                conditions,
                explicit,
            } => {
                let field_expr = if conditions.len() == 1 {
                    conditions[0].g.clone()
                } else {
                    Expr::Call(Func::Max, conditions.iter().map(|c| c.g.clone()).collect())
                };
                let field = Program::compile(&field_expr, opts).map_err(fix)?;
                let bound = match explicit {
                    Some(b) => {
                        let expr = if b.axis == Axis::Y {
                            b.f.swap_xy()
                        } else {
                            b.f.clone()
                        };
                        Some(CompiledBound {
                            axis: b.axis,
                            f: Program::compile(&expr, opts).map_err(fix)?,
                            greater: b.greater,
                        })
                    }
                    None => None,
                };
                let conds = if conditions.len() == 1 {
                    vec![(field.clone(), conditions[0].strict)]
                } else {
                    conditions
                        .iter()
                        .map(|c| Ok((Program::compile(&c.g, opts).map_err(fix)?, c.strict)))
                        .collect::<Result<Vec<_>, EquationError>>()?
                };
                CompiledForm::Inequality {
                    field,
                    conditions: conds,
                    bound,
                    strict: self.is_strict(),
                }
            }
        };
        Ok(CompiledEquation { form })
    }
}

/// An explicit inequality bound, compiled. The program takes the
/// independent variable through its `x` input.
#[derive(Clone, Debug)]
pub(crate) struct CompiledBound {
    pub axis: Axis,
    pub f: Program,
    pub greater: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum CompiledForm {
    /// The program takes the independent variable through its `x` input.
    Explicit {
        axis: Axis,
        f: Program,
    },
    Implicit {
        f: Program,
    },
    /// `field(x, y) < 0` (or `≤ 0`) inside the region; `field` is the max
    /// of the individual conditions `(g, strict)`.
    Inequality {
        field: Program,
        conditions: Vec<(Program, bool)>,
        bound: Option<CompiledBound>,
        strict: bool,
    },
}

/// An equation compiled for evaluation (variables and trig unit baked in).
#[derive(Clone, Debug)]
pub struct CompiledEquation {
    pub(crate) form: CompiledForm,
}

impl CompiledEquation {
    /// For explicit curves, evaluates the dependent coordinate for a value
    /// of the independent one (`f(x)` for `y = f(x)`, `g(y)` for
    /// `x = g(y)`).
    pub fn eval_explicit(&self, t: f64) -> Option<f64> {
        match &self.form {
            CompiledForm::Explicit { f, .. } => Some(f.eval(t, 0.0)),
            _ => None,
        }
    }

    /// The scalar field whose zero set is the curve (`y − f(x)` for
    /// explicit curves, `F(x, y)` for implicit ones, the region boundary
    /// field for inequalities).
    pub fn field(&self, x: f64, y: f64) -> f64 {
        match &self.form {
            CompiledForm::Explicit { axis: Axis::X, f } => y - f.eval(x, 0.0),
            CompiledForm::Explicit { axis: Axis::Y, f } => x - f.eval(y, 0.0),
            CompiledForm::Implicit { f } => f.eval(x, y),
            CompiledForm::Inequality { field, .. } => field.eval(x, y),
        }
    }

    /// For inequalities: whether `(x, y)` is inside the shaded region.
    pub fn contains(&self, x: f64, y: f64) -> Option<bool> {
        match &self.form {
            CompiledForm::Inequality { conditions, .. } => {
                Some(conditions.iter().all(|(g, strict)| {
                    let v = g.eval(x, y);
                    if *strict { v < 0.0 } else { v <= 0.0 }
                }))
            }
            _ => None,
        }
    }
}

fn is_y(e: &Expr) -> bool {
    matches!(e, Expr::Y)
}

fn is_x(e: &Expr) -> bool {
    matches!(e, Expr::X)
}

fn has_xy(e: &Expr) -> bool {
    e.contains_x() || e.contains_y()
}

/// Tries to write `lhs - rhs` (linear in `var` with a non-zero numeric
/// coefficient) as `var = solution`. Returns `(solution, coefficient)`.
fn solve_linear(lhs: &Expr, rhs: &Expr, var_is: &dyn Fn(&Expr) -> bool) -> Option<(Expr, f64)> {
    let diff = Expr::bin(crate::ast::BinOp::Sub, lhs.clone(), rhs.clone());
    let (c, r) = linear_in(&diff, var_is)?;
    if c.contains_x() || c.contains_y() || !c.variables().is_empty() {
        return None;
    }
    let cv = crate::compile::Program::compile(&c, &CompileOptions::default())
        .ok()?
        .as_constant()?;
    if cv == 0.0 || !cv.is_finite() {
        return None;
    }
    // c·v + r = 0  →  v = −r / c
    let sol = match as_num(&c) {
        Some(v) if v == 1.0 => simplify::neg(r),
        Some(v) if v == -1.0 => r,
        _ => simplify::div(simplify::neg(r), c),
    };
    Some((sol, cv))
}

fn condition(a: &Expr, op: RelOp, b: &Expr) -> Condition {
    use crate::ast::BinOp::Sub;
    match op {
        RelOp::Lt | RelOp::Le => Condition {
            g: Expr::bin(Sub, a.clone(), b.clone()),
            strict: op.is_strict(),
        },
        _ => Condition {
            g: Expr::bin(Sub, b.clone(), a.clone()),
            strict: op.is_strict(),
        },
    }
}

fn classify(p: &ParsedInput, whole: std::ops::Range<usize>) -> Result<Form, EquationError> {
    if p.rels.is_empty() {
        let e = &p.sides[0];
        if e.contains_y() {
            // `Equation::GetRequest` refuses a bare expression with both x
            // and y; one with only y cannot be y = f(x) either.
            return Err(EquationError {
                code: ErrorCode::General,
                span: whole,
            });
        }
        return Ok(Form::Explicit {
            axis: Axis::X,
            f: e.clone(),
        });
    }
    if !p.sides.iter().any(has_xy) {
        return Err(EquationError::syntax(
            SyntaxErrorCode::EqualWithoutGraphVariable,
            whole,
        ));
    }
    let any_eq = p.rels.iter().any(|(op, _)| *op == RelOp::Eq);
    if p.rels.len() > 1 {
        if any_eq {
            let span = p.rels[1].1.clone();
            return Err(EquationError::syntax(SyntaxErrorCode::TooManyEquals, span));
        }
        let conditions = (0..p.rels.len())
            .map(|i| condition(&p.sides[i], p.rels[i].0, &p.sides[i + 1]))
            .collect();
        return Ok(Form::Inequality {
            conditions,
            explicit: None,
        });
    }
    let (op, _) = p.rels[0];
    let (lhs, rhs) = (&p.sides[0], &p.sides[1]);
    if op == RelOp::Eq {
        if is_y(lhs) && !rhs.contains_y() {
            return Ok(Form::Explicit {
                axis: Axis::X,
                f: rhs.clone(),
            });
        }
        if is_y(rhs) && !lhs.contains_y() {
            return Ok(Form::Explicit {
                axis: Axis::X,
                f: lhs.clone(),
            });
        }
        // Prefer a function of x (analysis works on those): solve for y when
        // the equation is linear in y, e.g. `x = 2y` or `x + y = 1`.
        if let Some((sol, _)) = solve_linear(lhs, rhs, &is_y)
            && !sol.contains_y()
        {
            return Ok(Form::Explicit {
                axis: Axis::X,
                f: sol,
            });
        }
        if is_x(lhs) && !rhs.contains_x() {
            return Ok(Form::Explicit {
                axis: Axis::Y,
                f: rhs.clone(),
            });
        }
        if is_x(rhs) && !lhs.contains_x() {
            return Ok(Form::Explicit {
                axis: Axis::Y,
                f: lhs.clone(),
            });
        }
        if let Some((sol, _)) = solve_linear(lhs, rhs, &is_x)
            && !sol.contains_x()
        {
            return Ok(Form::Explicit {
                axis: Axis::Y,
                f: sol,
            });
        }
        return Ok(Form::Implicit {
            f: simplify::sub(lhs.clone(), rhs.clone()),
        });
    }
    // A single inequality.
    let cond = condition(lhs, op, rhs);
    let strict = op.is_strict();
    let greater_than = matches!(op, RelOp::Gt | RelOp::Ge);
    let explicit = if is_y(lhs) && !rhs.contains_y() {
        Some(ExplicitBound {
            axis: Axis::X,
            f: rhs.clone(),
            greater: greater_than,
            strict,
        })
    } else if is_y(rhs) && !lhs.contains_y() {
        Some(ExplicitBound {
            axis: Axis::X,
            f: lhs.clone(),
            greater: !greater_than,
            strict,
        })
    } else if let Some((sol, c)) = solve_linear(lhs, rhs, &is_y).filter(|(s, _)| !s.contains_y()) {
        // c·y + r ⋚ 0: dividing by a negative c flips the direction.
        Some(ExplicitBound {
            axis: Axis::X,
            f: sol,
            greater: greater_than == (c > 0.0),
            strict,
        })
    } else if is_x(lhs) && !rhs.contains_x() {
        Some(ExplicitBound {
            axis: Axis::Y,
            f: rhs.clone(),
            greater: greater_than,
            strict,
        })
    } else if is_x(rhs) && !lhs.contains_x() {
        Some(ExplicitBound {
            axis: Axis::Y,
            f: lhs.clone(),
            greater: !greater_than,
            strict,
        })
    } else if let Some((sol, c)) = solve_linear(lhs, rhs, &is_x).filter(|(s, _)| !s.contains_x()) {
        Some(ExplicitBound {
            axis: Axis::Y,
            f: sol,
            greater: greater_than == (c > 0.0),
            strict,
        })
    } else {
        None
    };
    Ok(Form::Inequality {
        conditions: vec![cond],
        explicit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(s: &str) -> EquationKind {
        Equation::parse(s)
            .unwrap_or_else(|e| panic!("{s}: {e}"))
            .kind()
    }

    #[test]
    fn classification() {
        assert_eq!(kind("y = x^2"), EquationKind::Function);
        assert_eq!(kind("x^2"), EquationKind::Function);
        assert_eq!(kind("sin(x)"), EquationKind::Function);
        assert_eq!(kind("x^2 = y"), EquationKind::Function);
        assert_eq!(kind("f(x) = 2x + 1"), EquationKind::Function);
        assert_eq!(kind("y = 3"), EquationKind::Function);
        assert_eq!(kind("x = y^2"), EquationKind::InverseFunction);
        assert_eq!(kind("x = 3"), EquationKind::InverseFunction);
        assert_eq!(kind("x^2 + y^2 = 25"), EquationKind::Implicit);
        // Solved for x: x = y - y².
        assert_eq!(kind("y = x + y^2"), EquationKind::InverseFunction);
        assert_eq!(kind("sin(y) = x^2 + y"), EquationKind::Implicit);
        // Linear in y wins over an isolated x.
        assert_eq!(kind("x = 2y"), EquationKind::Function);
        assert_eq!(kind("y < x"), EquationKind::Inequality);
        assert_eq!(kind("x^2 + y^2 <= 4"), EquationKind::Inequality);
        assert_eq!(kind("1 < y < 3"), EquationKind::Inequality);
        // Linear in y with a constant coefficient: solved.
        assert_eq!(kind("2y = x"), EquationKind::Function);
        assert_eq!(kind("x + y = 1"), EquationKind::Function);
        assert_eq!(kind("x + y^2 = 4"), EquationKind::InverseFunction);
        assert_eq!(kind("xy = 1"), EquationKind::Implicit);
    }

    #[test]
    fn solved_forms_evaluate_correctly() {
        let e = Equation::parse("2y - 4 = x").unwrap();
        let c = e.compile(&CompileOptions::default()).unwrap();
        assert_eq!(c.eval_explicit(2.0), Some(3.0));
        let e = Equation::parse("-y + x > 1").unwrap();
        let b = e.explicit_bound().unwrap();
        assert!(!b.greater, "x - y > 1  ⇔  y < x - 1");
        let c = e.compile(&CompileOptions::default()).unwrap();
        assert_eq!(c.contains(5.0, 0.0), Some(true));
        assert_eq!(c.contains(0.0, 5.0), Some(false));
    }

    #[test]
    fn inequality_directions() {
        let e = Equation::parse("y > x^2").unwrap();
        let b = e.explicit_bound().unwrap();
        assert!(b.greater && b.strict);
        let e = Equation::parse("x^2 >= y").unwrap();
        let b = e.explicit_bound().unwrap();
        assert!(!b.greater && !b.strict);
        assert!(!e.is_strict());
        let c = Equation::parse("x^2 + y^2 < 25")
            .unwrap()
            .compile(&CompileOptions::default())
            .unwrap();
        assert_eq!(c.contains(0.0, 0.0), Some(true));
        assert_eq!(c.contains(5.0, 0.0), Some(false));
        let c = Equation::parse("1 < y <= 3")
            .unwrap()
            .compile(&CompileOptions::default())
            .unwrap();
        assert_eq!(c.contains(0.0, 2.0), Some(true));
        assert_eq!(c.contains(0.0, 3.0), Some(true));
        assert_eq!(c.contains(0.0, 1.0), Some(false));
        assert_eq!(c.contains(0.0, 4.0), Some(false));
        assert_eq!(
            Equation::parse("y < x").unwrap().default_line_style(),
            LineStyle::Dash
        );
    }

    #[test]
    fn errors() {
        let msg = |s: &str| Equation::parse(s).unwrap_err().message();
        assert_eq!(msg("x + y"), "The equation could not be graphed");
        assert_eq!(
            msg("3 = 4"),
            "The function must contain at least one x or y variable"
        );
        assert_eq!(
            msg("a = 4"),
            "The function must contain at least one x or y variable"
        );
        assert_eq!(msg("y = x = 2"), "There are too many equal signs");
        assert_eq!(msg(""), "The expression is empty");
        assert_eq!(
            msg("y = (x"),
            "The equation is missing a closing parenthesis"
        );
    }

    #[test]
    fn variables_are_collected() {
        let e = Equation::parse("y = a x^2 + b x + c").unwrap();
        assert_eq!(e.variables(), ["a", "b", "c"]);
        let e = Equation::parse("y = k_1 sin(x)").unwrap();
        assert_eq!(e.variables(), ["k_1"]);
    }
}
