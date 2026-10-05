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
use crate::plot::IntervalFn;
use crate::simplify;
use std::sync::Arc;

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
        // The literals as typed: a coefficient solved for is the decimals'.
        let lits = crate::interval::Literals::of(text, opts)
            .unwrap_or_else(|_| crate::interval::Literals::none());
        let form = classify(&parsed, whole, &lits)?;
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
        // The literals as typed, for the programs and interval evaluation
        // alike (a literal that can't be read back is enclosed by its
        // neighbours instead), so that the curve drawn, its trace and its
        // analysis are of one function.
        let lits = crate::interval::Literals::of(&self.text, self.parse)
            .unwrap_or_else(|_| crate::interval::Literals::none());
        // The interval form folds arithmetic on literals alone exactly, as
        // the program does, into one tight enclosure.
        let interval = |e: Expr| {
            let e = crate::compile::fold_literals(&e, opts, &lits);
            Some(Arc::new(IntervalFn::new(e, lits.clone(), opts)))
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
                    f: Program::compile_typed(&expr, opts, &lits).map_err(fix)?,
                    iv: interval(expr),
                }
            }
            Form::Implicit { f } => CompiledForm::Implicit {
                f: Program::compile_typed(f, opts, &lits).map_err(fix)?,
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
                let field = Program::compile_typed(&field_expr, opts, &lits).map_err(fix)?;
                let bound = match explicit {
                    Some(b) => {
                        let expr = if b.axis == Axis::Y {
                            b.f.swap_xy()
                        } else {
                            b.f.clone()
                        };
                        Some(CompiledBound {
                            axis: b.axis,
                            f: Program::compile_typed(&expr, opts, &lits).map_err(fix)?,
                            iv: interval(expr),
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
                        .map(|c| {
                            Ok((
                                Program::compile_typed(&c.g, opts, &lits).map_err(fix)?,
                                c.strict,
                            ))
                        })
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
    /// `f` for interval evaluation (proven joins and gaps).
    pub iv: Option<Arc<IntervalFn>>,
    pub greater: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum CompiledForm {
    /// The program takes the independent variable through its `x` input.
    Explicit {
        axis: Axis,
        f: Program,
        /// `f` for interval evaluation (proven joins and gaps, traced
        /// values' digits).
        iv: Option<Arc<IntervalFn>>,
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
    /// [`crate::plot::IntervalFn::cost`] of an explicit curve or an
    /// inequality's explicit bound (the program's cost without one).
    pub(crate) fn interval_cost(&self) -> usize {
        match &self.form {
            CompiledForm::Explicit { f, iv, .. } => iv.as_ref().map_or(f.cost(), |iv| iv.cost()),
            CompiledForm::Inequality { bound: Some(b), .. } => {
                b.iv.as_ref().map_or(b.f.cost(), |iv| iv.cost())
            }
            CompiledForm::Implicit { f } => f.cost(),
            CompiledForm::Inequality { field, .. } => field.cost(),
        }
    }

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
            CompiledForm::Explicit {
                axis: Axis::X, f, ..
            } => y - f.eval(x, 0.0),
            CompiledForm::Explicit {
                axis: Axis::Y, f, ..
            } => x - f.eval(y, 0.0),
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

/// A part of `c·v + r` (see [`linear`]): 0, 1, or a tree of the equation's
/// own subtrees. No number is computed on the way: every literal in it is
/// still the decimal typed, so that `0.1·y + 0.2·y − 0.3·y` has the
/// coefficient 0.1 + 0.2 − 0.3, exactly 0, not the doubles' 5.55·10⁻¹⁷.
#[derive(Clone, Debug)]
enum Part {
    Zero,
    One,
    Tree(Expr),
}

impl Part {
    fn expr(self) -> Expr {
        match self {
            Part::Zero => Expr::exact(0.0),
            Part::One => Expr::exact(1.0),
            Part::Tree(e) => e,
        }
    }

    fn neg(self) -> Part {
        match self {
            Part::Zero => Part::Zero,
            Part::Tree(Expr::Neg(a)) => Part::Tree(*a),
            p => Part::Tree(Expr::Neg(Box::new(p.expr()))),
        }
    }

    fn add(self, o: Part) -> Part {
        use crate::ast::BinOp::{Add, Sub};
        match (self, o) {
            (Part::Zero, q) => q,
            (p, Part::Zero) => p,
            (p, Part::Tree(Expr::Neg(b))) => Part::Tree(Expr::bin(Sub, p.expr(), *b)),
            (p, q) => Part::Tree(Expr::bin(Add, p.expr(), q.expr())),
        }
    }

    fn sub(self, o: Part) -> Part {
        match (self, o) {
            (p, Part::Zero) => p,
            (Part::Zero, q) => q.neg(),
            (p, q) => Part::Tree(Expr::bin(crate::ast::BinOp::Sub, p.expr(), q.expr())),
        }
    }

    /// The part times `k`, a factor free of the variable.
    fn times(self, k: &Expr, k_first: bool) -> Part {
        use crate::ast::BinOp::Mul;
        match self {
            Part::Zero => Part::Zero,
            Part::One => Part::Tree(k.clone()),
            Part::Tree(t) if k_first => Part::Tree(Expr::bin(Mul, k.clone(), t)),
            Part::Tree(t) => Part::Tree(Expr::bin(Mul, t, k.clone())),
        }
    }

    fn over(self, k: &Expr) -> Part {
        match self {
            Part::Zero => Part::Zero,
            p => Part::Tree(Expr::bin(crate::ast::BinOp::Div, p.expr(), k.clone())),
        }
    }
}

/// `e` as `c·v + r` with c and r free of the variable `v` (`is_var`), if
/// it is linear in it.
fn linear(e: &Expr, is_var: &dyn Fn(&Expr) -> bool) -> Option<(Part, Part)> {
    use crate::ast::BinOp::{Add, Div, Mul, Sub};
    if !e.any(is_var) {
        return Some((Part::Zero, Part::Tree(e.clone())));
    }
    if is_var(e) {
        return Some((Part::One, Part::Zero));
    }
    Some(match e {
        Expr::Neg(a) => {
            let (c, r) = linear(a, is_var)?;
            (c.neg(), r.neg())
        }
        Expr::Bin(op @ (Add | Sub), a, b) => {
            let ((ca, ra), (cb, rb)) = (linear(a, is_var)?, linear(b, is_var)?);
            if *op == Add {
                (ca.add(cb), ra.add(rb))
            } else {
                (ca.sub(cb), ra.sub(rb))
            }
        }
        Expr::Bin(Mul, a, b) if !a.any(is_var) => {
            let (c, r) = linear(b, is_var)?;
            (c.times(a, true), r.times(a, true))
        }
        Expr::Bin(Mul, a, b) if !b.any(is_var) => {
            let (c, r) = linear(a, is_var)?;
            (c.times(b, false), r.times(b, false))
        }
        Expr::Bin(Div, a, b) if !b.any(is_var) => {
            let (c, r) = linear(a, is_var)?;
            (c.over(b), r.over(b))
        }
        _ => return None,
    })
}

/// The sign of a coefficient c (free of x, y and sliders) when c is proven
/// nonzero, in every angle unit alike: exactly, from the literals as typed
/// (`0.1 + 0.2 − 0.3` is 0), else by an enclosure that excludes 0
/// (`sin(π)`, ±10⁻¹⁶ in doubles, is no proven nonzero).
fn coefficient_sign(c: &Expr, lits: &crate::interval::Literals) -> Option<f64> {
    use crate::functions::TrigUnit;
    use crate::interval::{Ctx, Dec, Interval, enclose};
    let mut sign = None;
    for unit in [TrigUnit::Radians, TrigUnit::Degrees, TrigUnit::Grads] {
        let opts = CompileOptions {
            trig_unit: unit,
            variables: &(),
        };
        let s = match crate::compile::typed_value(c, &opts) {
            Some(crate::compile::TypedValue::Value(v)) if v != 0.0 => v.signum(),
            // Exactly 0, a division by 0, or too long to tell.
            Some(_) => return None,
            None => {
                let e = enclose(c, Interval::point(0.0), &Ctx::new(opts, lits));
                match (e.dec >= Dec::Def, e.gt0(), e.lt0()) {
                    (true, true, _) => 1.0,
                    (true, _, true) => -1.0,
                    _ => return None,
                }
            }
        };
        if sign.is_some_and(|t| t != s) {
            return None;
        }
        sign = Some(s);
    }
    sign
}

/// Tries to write `lhs - rhs`, linear in `var` with a coefficient proven
/// nonzero, as `var = solution`. Returns `(solution, the coefficient's
/// sign)`. A coefficient exactly 0 (`y·(0.1 + 0.2 − 0.3) = x`) drops the
/// variable: there is nothing to solve for, and the relation stays
/// implicit (here x = 0).
fn solve_linear(
    lhs: &Expr,
    rhs: &Expr,
    var_is: &dyn Fn(&Expr) -> bool,
    lits: &crate::interval::Literals,
) -> Option<(Expr, f64)> {
    let diff = Expr::bin(crate::ast::BinOp::Sub, lhs.clone(), rhs.clone());
    let (c, r) = linear(&diff, var_is)?;
    let c = c.expr();
    if c.contains_x() || c.contains_y() || !c.variables().is_empty() {
        return None;
    }
    let sign = coefficient_sign(&c, lits)?;
    // c·v + r = 0  →  v = −r / c (a written ±1, exact here, not divided by).
    let one = |e: &Expr| e.exact_value() == Some(1.0);
    let sol = match &c {
        e if one(e) => r.neg().expr(),
        Expr::Neg(a) if one(a) => r.expr(),
        _ => Expr::bin(crate::ast::BinOp::Div, r.neg().expr(), c.clone()),
    };
    Some((sol, sign))
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

fn classify(
    p: &ParsedInput,
    whole: std::ops::Range<usize>,
    lits: &crate::interval::Literals,
) -> Result<Form, EquationError> {
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
        if let Some((sol, _)) = solve_linear(lhs, rhs, &is_y, lits)
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
        if let Some((sol, _)) = solve_linear(lhs, rhs, &is_x, lits)
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
    } else if let Some((sol, c)) =
        solve_linear(lhs, rhs, &is_y, lits).filter(|(s, _)| !s.contains_y())
    {
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
    } else if let Some((sol, c)) =
        solve_linear(lhs, rhs, &is_x, lits).filter(|(s, _)| !s.contains_x())
    {
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

    /// A coefficient is solved for only when proven nonzero, from the
    /// literals as typed: y·(0.1 + 0.2 − 0.3) = x has no y in it (the
    /// coefficient is exactly 0) and is the line x = 0, not y = x/5.55·10⁻¹⁷.
    #[test]
    fn coefficients_are_the_decimals_typed() {
        let opts = CompileOptions::default();
        for src in [
            "y*(0.1+0.2-0.3)=x",
            "0.1*y+0.2*y-0.3*y=x",
            "x=(0.1+0.2-0.3)*y",
            "sin(pi)*y=x",
        ] {
            let e = Equation::parse(src).unwrap();
            assert_eq!(e.kind(), EquationKind::InverseFunction, "{src}");
            let c = e.compile(&opts).unwrap();
            for t in [0.0, 3.0, -2.5] {
                let v = c.eval_explicit(t).unwrap();
                assert!(v.abs() < 1e-15, "{src}: x({t}) = {v}");
            }
            // (Where the coefficient folds as a whole, exactly 0; 0.1·y +
            // 0.2·y − 0.3·y is evaluated in doubles at each y.)
            if src.contains("(0.1+0.2-0.3)") {
                assert_eq!(c.eval_explicit(4.0), Some(0.0), "{src}");
            }
        }
        // Both coefficients exactly 0: no variable to solve for.
        let e = Equation::parse("y*(0.1+0.2-0.3)=x*(0.5-0.25-0.25)+1").unwrap();
        assert_eq!(e.kind(), EquationKind::Implicit);
        // A typed 1.0000000000000001 is no 1: it is not divided away, and
        // the solution divides by it as typed, never by the 1 it would
        // write (each number carries its own value: review 13).
        let e = Equation::parse("1.0000000000000001*y=x").unwrap();
        assert_eq!(e.kind(), EquationKind::Function);
        let (_, f) = e.explicit().unwrap();
        assert!(
            f.formula().contains("1.0000000000000001"),
            "{}",
            f.formula()
        );
        // Proven nonzero, exactly or by an enclosure: solved as before.
        for (src, x, y) in [
            ("(0.1+0.2)*y=x", 0.3, 1.0),
            ("2y=x", 4.0, 2.0),
            ("pi*y=x", std::f64::consts::PI, 1.0),
            ("-y+x=1", 3.0, 2.0),
        ] {
            let e = Equation::parse(src).unwrap();
            assert_eq!(e.kind(), EquationKind::Function, "{src}");
            let c = e.compile(&opts).unwrap();
            let v = c.eval_explicit(x).unwrap();
            assert!((v - y).abs() <= 4.0 * f64::EPSILON, "{src}: y({x}) = {v}");
        }
        // An inequality's direction from the coefficient's proven sign.
        let e = Equation::parse("(0.1-0.3)*y > x").unwrap();
        let b = e.explicit_bound().unwrap();
        assert!(!b.greater, "−0.2y > x  ⇔  y < −5x");
        let e = Equation::parse("y*(0.1+0.2-0.3) > x").unwrap();
        assert_eq!(e.explicit_bound().unwrap().axis, Axis::Y);
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
