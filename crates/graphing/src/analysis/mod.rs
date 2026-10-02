//! Key graph features ("Function analysis" panel).
//!
//! Produces the same set of fields as the original engine's
//! `IGraphFunctionAnalysisData` / `GraphControl::KeyGraphFeaturesInfo`, and
//! [`KeyGraphFeatures::items`] lays them out exactly like
//! `EquationViewModel.PopulateKeyGraphFeatures` (titles, "none" texts,
//! period shown only when known, the "too complex" footer).
//!
//! The original engine is symbolic; this one is numeric. Results are
//! computed by sampling, root finding on exact symbolic derivatives, limit
//! probing and periodicity detection, then formatted with recognition of
//! simple closed forms (integers, fractions, multiples of π, surds, e).
//! When a feature cannot be determined reliably (e.g. infinitely many
//! non-periodic zeros, as in `sin(x²)`), it is reported through the
//! too-complex flags like the original does.

mod engine;
pub mod format;
pub(crate) mod numeric;

use crate::compile::CompileOptions;
use crate::equation::{Axis, Equation, EquationKind};
use crate::strings as s;

pub use format::{Bound, Interval};

/// Why analysis produced no features (`CalculatorApp::AnalysisErrorType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AnalysisError {
    /// Analysis succeeded.
    #[default]
    NoError = 0,
    /// "Analysis could not be performed for the function."
    AnalysisCouldNotBePerformed = 1,
    /// "Analysis is not supported for this function." (implicit relations,
    /// inequalities)
    AnalysisNotSupported = 2,
    /// "Analysis is only supported for functions in the f(x) format." (x = g(y))
    VariableIsNotX = 3,
    /// The function needs more work to analyze than one analysis may spend
    /// (not in the original, whose engine has its own limits).
    TooComplex = 4,
}

impl AnalysisError {
    /// The message shown instead of the feature list.
    pub fn message(self) -> Option<&'static str> {
        match self {
            AnalysisError::NoError => None,
            AnalysisError::AnalysisCouldNotBePerformed => {
                Some(s::KGF_ANALYSIS_COULD_NOT_BE_PERFORMED)
            }
            AnalysisError::AnalysisNotSupported => Some(s::KGF_ANALYSIS_NOT_SUPPORTED),
            AnalysisError::VariableIsNotX => Some(s::KGF_VARIABLE_IS_NOT_X),
            AnalysisError::TooComplex => Some(s::KGF_ANALYSIS_TOO_COMPLEX),
        }
    }
}

/// `FunctionParityType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Parity {
    #[default]
    Unknown = 0,
    Odd = 1,
    Even = 2,
    Neither = 3,
}

/// `FunctionPeriodicityType` (the panel hides the period row for `Unknown`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Periodicity {
    #[default]
    Unknown = 0,
    Periodic = 1,
    NotPeriodic = 2,
}

/// `FunctionMonotonicityType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Monotonicity {
    #[default]
    Unknown = 0,
    Increasing = 1,
    Decreasing = 2,
    Constant = 3,
}

/// `AsymptoteType`: where a horizontal/oblique asymptote applies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AsymptoteSide {
    #[default]
    Unknown = 0,
    PositiveInfinity = 1,
    NegativeInfinity = 2,
    AnyInfinity = 3,
}

/// Bit flags for [`KeyGraphFeatures::too_complex_features`]
/// (`CalculatorApp::KeyGraphFeaturesFlag`).
pub mod flags {
    pub const DOMAIN: u32 = 1;
    pub const RANGE: u32 = 2;
    pub const PARITY: u32 = 4;
    pub const PERIODICITY: u32 = 8;
    pub const ZEROS: u32 = 16;
    pub const Y_INTERCEPT: u32 = 32;
    pub const MINIMA: u32 = 64;
    pub const MAXIMA: u32 = 128;
    pub const INFLECTION_POINTS: u32 = 256;
    pub const VERTICAL_ASYMPTOTES: u32 = 512;
    pub const HORIZONTAL_ASYMPTOTES: u32 = 1024;
    pub const OBLIQUE_ASYMPTOTES: u32 = 2048;
    pub const MONOTONE_INTERVALS: u32 = 4096;
}

/// A set of x values `x + k·period` (k ∈ ℤ), or a single value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Family {
    /// Representative value.
    pub x: f64,
    /// Period of the family, if it repeats.
    pub period: Option<f64>,
}

impl Family {
    /// A single value.
    pub fn single(x: f64) -> Family {
        Family { x, period: None }
    }

    /// True if `v` belongs to the family (relative tolerance `tol`).
    pub fn contains(&self, v: f64, tol: f64) -> bool {
        match self.period {
            None => (v - self.x).abs() <= tol * v.abs().max(1.0),
            Some(p) => {
                let k = ((v - self.x) / p).round();
                (v - self.x - k * p).abs() <= tol * v.abs().max(1.0).max(p)
            }
        }
    }
}

/// Numeric analysis results (useful for drawing markers and for tests).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnalysisData {
    /// Fundamental period, if periodic.
    pub period: Option<f64>,
    /// Domain intervals (within one period `[w, w + period]` if periodic).
    pub domain: Vec<Interval>,
    /// Isolated excluded points (families if periodic).
    pub excluded: Vec<Family>,
    /// Range intervals.
    pub range: Vec<Interval>,
    /// x-intercepts.
    pub zeros: Vec<Family>,
    /// y-intercept value.
    pub y_intercept: Option<f64>,
    /// Local minima `(x, f(x))`.
    pub minima: Vec<(Family, f64)>,
    /// Local maxima `(x, f(x))`.
    pub maxima: Vec<(Family, f64)>,
    /// Inflection points `(x, f(x))`.
    pub inflection_points: Vec<(Family, f64)>,
    /// Vertical asymptotes `x = c`.
    pub vertical_asymptotes: Vec<Family>,
    /// Horizontal asymptotes `y = c`.
    pub horizontal_asymptotes: Vec<(f64, AsymptoteSide)>,
    /// Oblique asymptotes `y = m·x + b` as `(m, b, side)`.
    pub oblique_asymptotes: Vec<(f64, f64, AsymptoteSide)>,
    /// Monotone intervals (within one period if periodic).
    pub monotonicity: Vec<(Interval, Monotonicity)>,
}

/// Key graph features of a function, mirroring `KeyGraphFeaturesInfo`.
/// Empty strings / lists mean "none" (the panel then shows the
/// corresponding "does not have any …" text).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyGraphFeatures {
    pub domain: String,
    pub range: String,
    pub parity: Parity,
    pub periodicity_direction: Periodicity,
    pub periodicity_expression: String,
    /// `Zeros` in the original.
    pub x_intercept: String,
    pub y_intercept: String,
    pub minima: Vec<String>,
    pub maxima: Vec<String>,
    pub inflection_points: Vec<String>,
    pub vertical_asymptotes: Vec<String>,
    pub horizontal_asymptotes: Vec<String>,
    pub oblique_asymptotes: Vec<String>,
    /// Monotone intervals in x order (the original used a `std::map`,
    /// i.e. lexicographic order of the interval text).
    pub monotonicity: Vec<(String, Monotonicity)>,
    /// [`flags`] of features too complex to calculate.
    pub too_complex_features: u32,
    pub analysis_error: AnalysisError,
    /// The underlying numbers.
    pub data: AnalysisData,
}

/// One row of the function analysis panel (`KeyGraphFeaturesItem`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyGraphFeaturesItem {
    /// Row title, e.g. "Domain" (empty for the too-complex footer).
    pub title: String,
    /// Math items (or one text item when `is_text`).
    pub display_items: Vec<String>,
    /// Monotonicity grid rows.
    pub grid_items: Vec<GridDisplayItem>,
    /// True if `display_items` is plain text rather than math.
    pub is_text: bool,
}

/// One monotonicity row (`GridDisplayItems`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridDisplayItem {
    /// Interval, e.g. `(−∞, 0)`.
    pub expression: String,
    /// "Increasing", "Decreasing", "Constant" or an unknown text.
    pub direction: String,
}

impl KeyGraphFeatures {
    /// A result carrying only an error (`KeyGraphFeaturesInfo::Create(type)`).
    pub fn error(e: AnalysisError) -> KeyGraphFeatures {
        KeyGraphFeatures {
            analysis_error: e,
            ..Default::default()
        }
    }

    /// The error text shown instead of the panel, if any
    /// (`EquationViewModel.AnalysisErrorString`).
    pub fn analysis_error_string(&self) -> Option<&'static str> {
        self.analysis_error.message()
    }

    /// Panel rows, exactly as `EquationViewModel.PopulateKeyGraphFeatures`
    /// builds them. Empty when there is an analysis error.
    pub fn items(&self) -> Vec<KeyGraphFeaturesItem> {
        if self.analysis_error != AnalysisError::NoError {
            return Vec::new();
        }
        let mut items = Vec::new();
        let text = |title: &str, value: &str, none: &str| {
            if value.is_empty() {
                KeyGraphFeaturesItem {
                    title: title.into(),
                    display_items: vec![none.into()],
                    is_text: true,
                    ..Default::default()
                }
            } else {
                KeyGraphFeaturesItem {
                    title: title.into(),
                    display_items: vec![value.into()],
                    is_text: false,
                    ..Default::default()
                }
            }
        };
        let list = |title: &str, values: &[String], none: &str| {
            if values.is_empty() {
                KeyGraphFeaturesItem {
                    title: title.into(),
                    display_items: vec![none.into()],
                    is_text: true,
                    ..Default::default()
                }
            } else {
                KeyGraphFeaturesItem {
                    title: title.into(),
                    display_items: values.to_vec(),
                    is_text: false,
                    ..Default::default()
                }
            }
        };
        items.push(text(s::DOMAIN, &self.domain, s::KGF_DOMAIN_NONE));
        items.push(text(s::RANGE, &self.range, s::KGF_RANGE_NONE));
        items.push(text(
            s::X_INTERCEPT,
            &self.x_intercept,
            s::KGF_X_INTERCEPT_NONE,
        ));
        items.push(text(
            s::Y_INTERCEPT,
            &self.y_intercept,
            s::KGF_Y_INTERCEPT_NONE,
        ));
        items.push(list(s::MINIMA, &self.minima, s::KGF_MINIMA_NONE));
        items.push(list(s::MAXIMA, &self.maxima, s::KGF_MAXIMA_NONE));
        items.push(list(
            s::INFLECTION_POINTS,
            &self.inflection_points,
            s::KGF_INFLECTION_POINTS_NONE,
        ));
        items.push(list(
            s::VERTICAL_ASYMPTOTES,
            &self.vertical_asymptotes,
            s::KGF_VERTICAL_ASYMPTOTES_NONE,
        ));
        items.push(list(
            s::HORIZONTAL_ASYMPTOTES,
            &self.horizontal_asymptotes,
            s::KGF_HORIZONTAL_ASYMPTOTES_NONE,
        ));
        items.push(list(
            s::OBLIQUE_ASYMPTOTES,
            &self.oblique_asymptotes,
            s::KGF_OBLIQUE_ASYMPTOTES_NONE,
        ));
        let parity = match self.parity {
            Parity::Odd => s::KGF_PARITY_ODD,
            Parity::Even => s::KGF_PARITY_EVEN,
            Parity::Neither => s::KGF_PARITY_NEITHER,
            Parity::Unknown => s::KGF_PARITY_UNKNOWN,
        };
        items.push(KeyGraphFeaturesItem {
            title: s::PARITY.into(),
            display_items: vec![parity.into()],
            is_text: true,
            ..Default::default()
        });
        match self.periodicity_direction {
            Periodicity::Unknown => {}
            Periodicity::Periodic => {
                if self.periodicity_expression.is_empty() {
                    items.push(KeyGraphFeaturesItem {
                        title: s::PERIODICITY.into(),
                        display_items: vec![s::KGF_PERIODICITY_UNKNOWN.into()],
                        is_text: true,
                        ..Default::default()
                    });
                } else {
                    items.push(KeyGraphFeaturesItem {
                        title: s::PERIODICITY.into(),
                        display_items: vec![self.periodicity_expression.clone()],
                        is_text: false,
                        ..Default::default()
                    });
                }
            }
            Periodicity::NotPeriodic => items.push(KeyGraphFeaturesItem {
                title: s::PERIODICITY.into(),
                display_items: vec![s::KGF_PERIODICITY_NOT_PERIODIC.into()],
                // The original leaves IsText false here (rendered as math).
                is_text: false,
                ..Default::default()
            }),
        }
        let mut mono = KeyGraphFeaturesItem {
            title: s::MONOTONICITY.into(),
            ..Default::default()
        };
        for (expr, dir) in &self.monotonicity {
            let direction = match dir {
                Monotonicity::Increasing => s::KGF_MONOTONICITY_INCREASING,
                Monotonicity::Decreasing => s::KGF_MONOTONICITY_DECREASING,
                Monotonicity::Constant => s::KGF_MONOTONICITY_CONSTANT,
                Monotonicity::Unknown => s::KGF_MONOTONICITY_UNKNOWN,
            };
            mono.grid_items.push(GridDisplayItem {
                expression: expr.clone(),
                direction: direction.into(),
            });
        }
        if mono.grid_items.is_empty() {
            mono.display_items.push(s::KGF_MONOTONICITY_ERROR.into());
            mono.is_text = true;
        }
        items.push(mono);
        if self.too_complex_features != 0 {
            let order: [(u32, &str); 13] = [
                (flags::DOMAIN, s::DOMAIN),
                (flags::RANGE, s::RANGE),
                (flags::ZEROS, s::X_INTERCEPT),
                (flags::Y_INTERCEPT, s::Y_INTERCEPT),
                (flags::PARITY, s::PARITY),
                (flags::PERIODICITY, s::PERIODICITY),
                (flags::MINIMA, s::MINIMA),
                (flags::MAXIMA, s::MAXIMA),
                (flags::INFLECTION_POINTS, s::INFLECTION_POINTS),
                (flags::VERTICAL_ASYMPTOTES, s::VERTICAL_ASYMPTOTES),
                (flags::HORIZONTAL_ASYMPTOTES, s::HORIZONTAL_ASYMPTOTES),
                (flags::OBLIQUE_ASYMPTOTES, s::OBLIQUE_ASYMPTOTES),
                (flags::MONOTONE_INTERVALS, s::MONOTONICITY),
            ];
            let names: Vec<&str> = order
                .iter()
                .filter(|(f, _)| self.too_complex_features & f != 0)
                .map(|(_, n)| *n)
                .collect();
            items.push(KeyGraphFeaturesItem {
                title: String::new(),
                display_items: vec![s::KGF_TOO_COMPLEX_FEATURES_ERROR.into(), names.join(", ")],
                is_text: true,
                ..Default::default()
            });
        }
        items
    }
}

/// Analyzes an equation (`Grapher::AnalyzeEquation`): explicit functions of
/// x are analyzed; `x = g(y)` gives [`AnalysisError::VariableIsNotX`];
/// implicit relations and inequalities give
/// [`AnalysisError::AnalysisNotSupported`]; failures give
/// [`AnalysisError::AnalysisCouldNotBePerformed`].
pub fn analyze(eq: &Equation, opts: &CompileOptions<'_>) -> KeyGraphFeatures {
    analyze_cancellable(eq, opts, None)
        .unwrap_or_else(|| unreachable!("analysis without a cancel flag is never cancelled"))
}

/// [`analyze`], polling `cancel` while it works: returns `None` soon after
/// the flag becomes true. Analysis is bounded either way (an over-budget
/// function reports [`AnalysisError::TooComplex`]); the flag lets a caller
/// running it on a worker thread abandon a result it no longer needs.
pub fn analyze_cancellable(
    eq: &Equation,
    opts: &CompileOptions<'_>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Option<KeyGraphFeatures> {
    let error = |e| Some(KeyGraphFeatures::error(e));
    match eq.kind() {
        EquationKind::Function => {}
        EquationKind::InverseFunction => return error(AnalysisError::VariableIsNotX),
        EquationKind::Implicit | EquationKind::Inequality => {
            return error(AnalysisError::AnalysisNotSupported);
        }
    }
    let Some((Axis::X, f)) = eq.explicit() else {
        return error(AnalysisError::AnalysisCouldNotBePerformed);
    };
    match engine::analyze_expr(f, opts, cancel) {
        Ok(k) => Some(k),
        Err(engine::Stop::Cancelled) => None,
        Err(engine::Stop::Error(e)) => error(e),
    }
}

/// Analyzes a function given as text (`y = …`, `f(x) = …` or a bare
/// expression), with default variable values and radians.
pub fn analyze_str(text: &str) -> KeyGraphFeatures {
    match Equation::parse(text) {
        Ok(eq) => analyze(&eq, &CompileOptions::default()),
        Err(_) => KeyGraphFeatures::error(AnalysisError::AnalysisCouldNotBePerformed),
    }
}
