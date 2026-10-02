//! The graph: a list of equations, the shared slider variables and the
//! evaluation settings — what `GraphControl::Grapher` drives through
//! `IGraph` in the original app.

use crate::analysis::{AnalysisError, KeyGraphFeatures, analyze_cancellable};
use crate::compile::{CompileOptions, VariableValues};
use crate::equation::{Axis, CompiledEquation, CompiledForm, Equation, EquationKind, LineStyle};
use crate::error::EquationError;
use crate::functions::TrigUnit;
use crate::lexer::ParseOptions;
use crate::plot::{Cancel, Plot, PlotOptions, contour_cost, plot, plot_with};
use crate::trace::{TracePoint, nearest_point};
use crate::variable::Variable;
use crate::viewport::Viewport;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

/// Maximum number of equations in the equation list
/// (`EquationInputArea.maxEquationSize`).
pub const MAX_EQUATIONS: usize = 14;

/// Default line width (`Grapher.LineWidth`); the settings offer 1–4.
pub const DEFAULT_LINE_WIDTH: f64 = 2.0;
/// Line widths offered by the graph settings.
pub const LINE_WIDTHS: [f64; 4] = [1.0, 2.0, 3.0, 4.0];

/// Refinement budget shared by the implicit relations and inequality
/// regions plotted together, in fine cells × field evaluation cost
/// ([`crate::compile::Program::cost`]). A relation that changes sign almost
/// everywhere is drawn on a coarser lattice instead of costing the whole
/// fine one; 14 of them together then cost about what a few did alone.
pub const MAX_IMPLICIT_WORK: f64 = 1.2e7;

/// Line width of the selected equation (`Grapher::UpdateGraphOptions`).
pub fn selected_line_width(width: f64) -> f64 {
    width + if width <= 2.0 { 1.0 } else { 2.0 }
}

/// Radius of the trace point (`RenderMain::SetPointRadius(LineWidth + 1)`).
pub fn trace_point_radius(width: f64) -> f64 {
    width + 1.0
}

/// Stable identifier of an equation in a [`Graph`]
/// (`IEquation::GetGraphEquationID`).
pub type EquationId = u32;

#[derive(Clone, Debug)]
struct Entry {
    id: EquationId,
    text: String,
    parsed: Result<Equation, EquationError>,
    compiled: Option<Result<CompiledEquation, EquationError>>,
    line_enabled: bool,
    line_style: LineStyle,
}

impl Entry {
    fn graphable(&self) -> bool {
        self.line_enabled && matches!(self.compiled, Some(Ok(_)))
    }
}

struct Vars<'a>(&'a BTreeMap<String, Variable>);

impl VariableValues for Vars<'_> {
    fn value(&self, name: &str) -> Option<f64> {
        self.0.get(name).map(|v| v.value())
    }
}

/// Geometry of one equation for a viewport.
#[derive(Clone, Debug)]
pub struct EquationPlot {
    /// Which equation.
    pub id: EquationId,
    /// Its geometry.
    pub plot: Plot,
}

/// A set of equations plotted together.
#[derive(Clone, Debug)]
pub struct Graph {
    entries: Vec<Entry>,
    variables: BTreeMap<String, Variable>,
    trig_unit: TrigUnit,
    parse_options: ParseOptions,
    plot_options: PlotOptions,
    next_id: EquationId,
}

impl Default for Graph {
    fn default() -> Self {
        Graph::new()
    }
}

impl Graph {
    /// An empty graph in radians.
    pub fn new() -> Graph {
        Graph {
            entries: Vec::new(),
            variables: BTreeMap::new(),
            trig_unit: TrigUnit::Radians,
            parse_options: ParseOptions::default(),
            plot_options: PlotOptions::default(),
            next_id: 1,
        }
    }

    /// Adds an equation (even an invalid or empty one; check
    /// [`Graph::error`]). Returns its id.
    pub fn add_equation(&mut self, text: &str) -> EquationId {
        let id = self.next_id;
        self.next_id += 1;
        let parsed = Equation::parse_with(text, self.parse_options);
        let line_style = parsed
            .as_ref()
            .map(|e| e.default_line_style())
            .unwrap_or_default();
        self.entries.push(Entry {
            id,
            text: text.to_string(),
            parsed,
            compiled: None,
            line_enabled: true,
            line_style,
        });
        self.refresh();
        id
    }

    /// Replaces an equation's text. Returns false for an unknown id.
    pub fn set_equation_text(&mut self, id: EquationId, text: &str) -> bool {
        let opts = self.parse_options;
        let Some(e) = self.entries.iter_mut().find(|e| e.id == id) else {
            return false;
        };
        e.text = text.to_string();
        e.parsed = Equation::parse_with(text, opts);
        // Like `Equation::GetRequest`, an inequality switches to dashed.
        if let Ok(p) = &e.parsed
            && p.is_inequality()
        {
            e.line_style = LineStyle::Dash;
        }
        // Re-enabled when edited (`OnEquationChanged` resets the state).
        e.line_enabled = true;
        self.refresh();
        true
    }

    /// Removes an equation.
    pub fn remove_equation(&mut self, id: EquationId) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        let removed = self.entries.len() != before;
        if removed {
            self.refresh();
        }
        removed
    }

    /// Ids of all equations, in list order.
    pub fn equation_ids(&self) -> Vec<EquationId> {
        self.entries.iter().map(|e| e.id).collect()
    }

    /// Number of equations.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if there are no equations.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn entry(&self, id: EquationId) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The equation's text.
    pub fn text(&self, id: EquationId) -> Option<&str> {
        self.entry(id).map(|e| e.text.as_str())
    }

    /// The parsed equation, if valid.
    pub fn equation(&self, id: EquationId) -> Option<&Equation> {
        self.entry(id).and_then(|e| e.parsed.as_ref().ok())
    }

    /// The error to show under the equation, if any (syntax error, or an
    /// evaluation error such as "Cannot divide by zero"). Empty input is
    /// not an error; it is simply not graphed.
    pub fn error(&self, id: EquationId) -> Option<&EquationError> {
        let e = self.entry(id)?;
        if e.text.trim().is_empty() {
            return None;
        }
        match (&e.parsed, &e.compiled) {
            (Err(err), _) => Some(err),
            (_, Some(Err(err))) => Some(err),
            _ => None,
        }
    }

    /// Whether the equation is drawn (the show/hide toggle).
    pub fn set_line_enabled(&mut self, id: EquationId, enabled: bool) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.line_enabled = enabled;
        }
        self.refresh();
    }

    /// Whether the equation is drawn.
    pub fn is_line_enabled(&self, id: EquationId) -> bool {
        self.entry(id).is_some_and(|e| e.line_enabled)
    }

    /// Line style of an equation.
    pub fn line_style(&self, id: EquationId) -> LineStyle {
        self.entry(id).map(|e| e.line_style).unwrap_or_default()
    }

    /// Sets the line style of an equation.
    pub fn set_line_style(&mut self, id: EquationId, style: LineStyle) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.line_style = style;
        }
    }

    /// The trig unit.
    pub fn trig_unit(&self) -> TrigUnit {
        self.trig_unit
    }

    /// Changes the trig unit (recompiles every equation).
    pub fn set_trig_unit(&mut self, unit: TrigUnit) {
        if self.trig_unit != unit {
            self.trig_unit = unit;
            self.refresh();
        }
    }

    /// Changes the decimal/list separators (re-parses every equation).
    pub fn set_parse_options(&mut self, opts: ParseOptions) {
        if self.parse_options != opts {
            self.parse_options = opts;
            for e in &mut self.entries {
                e.parsed = Equation::parse_with(&e.text, opts);
            }
            self.refresh();
        }
    }

    /// Plotting quality/performance settings.
    pub fn plot_options(&self) -> &PlotOptions {
        &self.plot_options
    }

    /// Changes plotting settings.
    pub fn set_plot_options(&mut self, opts: PlotOptions) {
        self.plot_options = opts;
    }

    /// The slider variables used by the graphed equations, sorted by name
    /// (like the original's `Grapher::Variables` map).
    pub fn variables(&self) -> &BTreeMap<String, Variable> {
        &self.variables
    }

    /// A variable by name.
    pub fn variable(&self, name: &str) -> Option<&Variable> {
        self.variables.get(name)
    }

    /// Sets a variable's value (`IGraph::SetArgValue`), extending its slider
    /// range if needed. Unknown names are added. Recompiles.
    pub fn set_variable(&mut self, name: &str, value: f64) {
        self.variables
            .entry(name.to_string())
            .or_insert_with(|| Variable::new(value))
            .set_value(value);
        self.recompile();
    }

    /// Edits a variable's slider settings (min/max/step) and/or value via a
    /// closure; recompiles afterwards.
    pub fn update_variable(&mut self, name: &str, f: impl FnOnce(&mut Variable)) -> bool {
        let Some(v) = self.variables.get_mut(name) else {
            return false;
        };
        f(v);
        self.recompile();
        true
    }

    fn compile_options(&self) -> (TrigUnit, &BTreeMap<String, Variable>) {
        (self.trig_unit, &self.variables)
    }

    fn recompile(&mut self) {
        let (unit, vars) = self.compile_options();
        let opts = CompileOptions {
            trig_unit: unit,
            variables: &Vars(vars),
        };
        let compiled: Vec<Option<Result<CompiledEquation, EquationError>>> = self
            .entries
            .iter()
            .map(|e| e.parsed.as_ref().ok().map(|p| p.compile(&opts)))
            .collect();
        for (e, c) in self.entries.iter_mut().zip(compiled) {
            e.compiled = c;
        }
    }

    /// Re-derives the variable set (`Grapher::UpdateVariables`: names used
    /// by graphable equations, keeping existing slider settings, new ones
    /// at value 1) and recompiles.
    fn refresh(&mut self) {
        // Compile first with the current variables to know which equations
        // are graphable, then rebuild the variable set from them.
        self.recompile();
        let mut names: Vec<String> = Vec::new();
        for e in &self.entries {
            if e.graphable()
                && let Ok(p) = &e.parsed
            {
                names.extend(p.variables().iter().cloned());
            }
        }
        let mut vars = BTreeMap::new();
        for n in names {
            let v = self.variables.get(&n).copied().unwrap_or_default();
            vars.insert(n, v);
        }
        if vars != self.variables {
            self.variables = vars;
            self.recompile();
        }
    }

    /// The compiled form of an equation (for custom evaluation).
    pub fn compiled(&self, id: EquationId) -> Option<&CompiledEquation> {
        self.entry(id)
            .and_then(|e| e.compiled.as_ref())
            .and_then(|c| c.as_ref().ok())
    }

    /// Ids of the equations that are drawn (valid, non-empty, enabled), in
    /// list order (`Grapher::GetGraphableEquations`).
    pub fn graphable_ids(&self) -> Vec<EquationId> {
        self.entries
            .iter()
            .filter(|e| e.graphable())
            .map(|e| e.id)
            .collect()
    }

    /// The drawn equations with their compiled forms, in list order.
    fn plot_work(&self) -> Vec<(EquationId, &CompiledEquation)> {
        self.entries
            .iter()
            .filter(|e| e.graphable())
            .filter_map(|e| Some((e.id, e.compiled.as_ref()?.as_ref().ok()?)))
            .collect()
    }

    /// Plot options for plotting `work` together: [`MAX_IMPLICIT_WORK`] is
    /// shared between the relations that are contoured.
    fn shared_options(&self, work: &[(EquationId, &CompiledEquation)]) -> PlotOptions {
        let mut opts = self.plot_options;
        let cost: usize = work.iter().filter_map(|(_, c)| contour_cost(c)).sum();
        if cost > 0 {
            opts.implicit_max_refined = opts
                .implicit_max_refined
                .min(MAX_IMPLICIT_WORK / cost as f64);
        }
        opts
    }

    /// Geometry of every drawn equation for a viewport.
    pub fn plot(&self, vp: &Viewport) -> Vec<EquationPlot> {
        let work = self.plot_work();
        let opts = self.shared_options(&work);
        work.into_iter()
            .map(|(id, c)| EquationPlot {
                id,
                plot: plot(c, vp, &opts),
            })
            .collect()
    }

    /// Geometry of every drawn equation, computed on several threads.
    pub fn plot_parallel(&self, vp: &Viewport) -> Vec<EquationPlot> {
        self.plot_parallel_with(vp, &Cancel(None))
    }

    /// [`Graph::plot_parallel`] for a worker thread: polls `cancel` and
    /// returns `None` soon after it is set (e.g. because the viewport moved
    /// on and a newer request superseded this one). The work itself is
    /// bounded: explicit curves by [`PlotOptions::max_evals`], implicit
    /// relations by [`MAX_IMPLICIT_WORK`].
    ///
    /// ```
    /// use std::sync::{Arc, atomic::AtomicBool};
    /// use graphing::{Graph, Viewport};
    ///
    /// let mut g = Graph::new();
    /// g.add_equation("sin(x*y) < 0");
    /// let g = Arc::new(g);
    /// let cancel = Arc::new(AtomicBool::new(false));
    /// let vp = Viewport::default_for_size(640.0, 480.0);
    /// let job = std::thread::spawn({
    ///     let (g, cancel) = (g.clone(), cancel.clone());
    ///     move || g.plot_parallel_cancellable(&vp, &cancel)
    /// });
    /// // Store `true` into `cancel` to abandon the job early.
    /// let plots = job.join().unwrap().expect("not cancelled");
    /// assert_eq!(plots.len(), 1);
    /// ```
    pub fn plot_parallel_cancellable(
        &self,
        vp: &Viewport,
        cancel: &AtomicBool,
    ) -> Option<Vec<EquationPlot>> {
        let c = Cancel(Some(cancel));
        let plots = self.plot_parallel_with(vp, &c);
        (!c.is_set()).then_some(plots)
    }

    fn plot_parallel_with(&self, vp: &Viewport, cancel: &Cancel<'_>) -> Vec<EquationPlot> {
        let work = self.plot_work();
        let opts = self.shared_options(&work);
        if work.len() <= 1 {
            return work
                .into_iter()
                .map(|(id, c)| EquationPlot {
                    id,
                    plot: plot_with(c, vp, &opts, cancel),
                })
                .collect();
        }
        let cancel = *cancel;
        std::thread::scope(|s| {
            let handles: Vec<_> = work
                .iter()
                .map(|&(id, c)| {
                    s.spawn(move || EquationPlot {
                        id,
                        plot: plot_with(c, vp, &opts, &cancel),
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("plot thread"))
                .collect()
        })
    }

    /// Geometry of one equation.
    pub fn plot_equation(&self, id: EquationId, vp: &Viewport) -> Option<Plot> {
        Some(plot(self.compiled(id)?, vp, &self.plot_options))
    }

    /// For `y = f(x)`: f at `x` (and for `x = g(y)`: g at `y`).
    pub fn evaluate(&self, id: EquationId, t: f64) -> Option<f64> {
        self.compiled(id)?.eval_explicit(t)
    }

    /// Function analysis for an equation (`Grapher::AnalyzeEquation`).
    ///
    /// Bounded: a function too expensive to analyze reports
    /// [`AnalysisError::TooComplex`] after a few hundred milliseconds at
    /// most. Use [`Graph::analyze_cancellable`] to run it off the UI thread.
    pub fn analyze(&self, id: EquationId) -> KeyGraphFeatures {
        self.analyze_with(id, None)
            .expect("analysis without a cancel flag is never cancelled")
    }

    /// [`Graph::analyze`] for a worker thread: polls `cancel` and returns
    /// `None` soon after it is set. `Graph` is `Send + Sync`, so share it
    /// (or a clone) with the worker through an `Arc`.
    pub fn analyze_cancellable(
        &self,
        id: EquationId,
        cancel: &AtomicBool,
    ) -> Option<KeyGraphFeatures> {
        self.analyze_with(id, Some(cancel))
    }

    fn analyze_with(
        &self,
        id: EquationId,
        cancel: Option<&AtomicBool>,
    ) -> Option<KeyGraphFeatures> {
        let could_not = || {
            Some(KeyGraphFeatures::error(
                AnalysisError::AnalysisCouldNotBePerformed,
            ))
        };
        let Some(e) = self.entry(id) else {
            return could_not();
        };
        let Ok(p) = &e.parsed else {
            return could_not();
        };
        if !matches!(e.compiled, Some(Ok(_))) {
            return could_not();
        }
        let vars = Vars(&self.variables);
        analyze_cancellable(
            p,
            &CompileOptions {
                trig_unit: self.trig_unit,
                variables: &vars,
            },
            cancel,
        )
    }

    /// Nearest traced point to the pointer among the drawn equations, using
    /// geometry previously returned by [`Graph::plot`] for the same viewport.
    pub fn trace(
        &self,
        vp: &Viewport,
        plots: &[EquationPlot],
        px: f64,
        py: f64,
        radius_px: f64,
    ) -> Option<(EquationId, TracePoint)> {
        let pairs: Vec<(EquationId, &CompiledEquation, &Plot)> = plots
            .iter()
            .filter_map(|p| Some((p.id, self.compiled(p.id)?, &p.plot)))
            .collect();
        let curves: Vec<(&CompiledEquation, &Plot)> =
            pairs.iter().map(|(_, c, p)| (*c, *p)).collect();
        let t = nearest_point(vp, &curves, px, py, radius_px)?;
        Some((pairs[t.index].0, t))
    }

    /// A view for the given pixel size: the default [−10, 10] view, with
    /// the y range fitted to the explicit functions when they would
    /// otherwise be mostly off-screen ("zoom to fit").
    pub fn fit_viewport(&self, width: f64, height: f64) -> Viewport {
        let base = Viewport::default_for_size(width, height);
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        let mut any_visible = false;
        let mut any_function = false;
        for e in self.entries.iter().filter(|e| e.graphable()) {
            let Some(Ok(c)) = &e.compiled else { continue };
            let CompiledForm::Explicit { axis: Axis::X, f } = &c.form else {
                any_visible = true;
                continue;
            };
            any_function = true;
            let mut vals: Vec<f64> = (0..=400)
                .map(|i| base.x_min + base.x_span() * i as f64 / 400.0)
                .map(|x| f.eval(x, 0.0))
                .filter(|v| v.is_finite())
                .collect();
            if vals.is_empty() {
                continue;
            }
            vals.sort_by(|a, b| a.total_cmp(b));
            if vals.iter().any(|v| *v >= base.y_min && *v <= base.y_max) {
                any_visible = true;
            }
            lo = lo.min(vals[vals.len() / 20]);
            hi = hi.max(vals[vals.len() - 1 - vals.len() / 20]);
        }
        if !any_function || any_visible || !(lo.is_finite() && hi.is_finite()) {
            return base;
        }
        let pad = ((hi - lo) * 0.1).max(1e-9 * hi.abs().max(1.0)).max(1e-12);
        let mut v = base;
        let _ = v.set_display_ranges(base.x_min, base.x_max, lo - pad, hi + pad);
        v
    }

    /// Kind of an equation, if valid.
    pub fn kind(&self, id: EquationId) -> Option<EquationKind> {
        self.equation(id).map(|e| e.kind())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_follow_equations() {
        let mut g = Graph::new();
        let a = g.add_equation("y = a x^2 + b");
        assert_eq!(
            g.variables().keys().cloned().collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(g.variable("a").unwrap().value(), 1.0);
        g.set_variable("a", 2.0);
        assert_eq!(g.evaluate(a, 3.0), Some(19.0));
        // Editing keeps existing slider values, drops unused variables.
        g.set_equation_text(a, "y = a x + c");
        assert_eq!(
            g.variables().keys().cloned().collect::<Vec<_>>(),
            ["a", "c"]
        );
        assert_eq!(g.variable("a").unwrap().value(), 2.0);
        // A hidden equation's variables are not listed.
        g.set_line_enabled(a, false);
        assert!(g.variables().is_empty());
    }

    #[test]
    fn errors_and_graphable() {
        let mut g = Graph::new();
        let ok = g.add_equation("y = sin(x)");
        let bad = g.add_equation("y = (x");
        let empty = g.add_equation("");
        let div = g.add_equation("y = x/0");
        assert!(g.error(ok).is_none());
        assert_eq!(
            g.error(bad).unwrap().message(),
            "The equation is missing a closing parenthesis"
        );
        assert!(g.error(empty).is_none());
        assert_eq!(g.error(div).unwrap().message(), "Cannot divide by zero");
        assert_eq!(g.graphable_ids(), vec![ok]);
        let deg = g.add_equation("y = sin(x°)");
        assert_eq!(
            g.error(deg).unwrap().message(),
            "Degrees mode is required to graph this function"
        );
        g.set_trig_unit(TrigUnit::Degrees);
        assert!(g.error(deg).is_none());
    }

    #[test]
    fn plot_and_trace_and_analyze() {
        let mut g = Graph::new();
        let a = g.add_equation("y = x^2 - 4");
        let b = g.add_equation("x^2 + y^2 < 9");
        let vp = Viewport::default_for_size(800.0, 600.0);
        let plots = g.plot(&vp);
        assert_eq!(plots.len(), 2);
        assert!(!plots[1].plot.fill.is_empty());
        assert!(plots[1].plot.boundary_dashed);
        let (px, py) = vp.to_screen(2.0, 0.0);
        let (id, t) = g.trace(&vp, &plots, px, py, 20.0).unwrap();
        assert!(id == a || id == b);
        assert!(t.distance_px < 2.0);
        let k = g.analyze(a);
        assert_eq!(k.x_intercept, "−2, 2");
        assert_eq!(
            g.analyze(b).analysis_error,
            AnalysisError::AnalysisNotSupported
        );
        let par = g.plot_parallel(&vp);
        assert_eq!(par.len(), 2);
        assert_eq!(par[0].plot, plots[0].plot);
    }

    #[test]
    fn fit_viewport_brings_functions_into_view() {
        let mut g = Graph::new();
        g.add_equation("y = x^2 + 1000");
        let v = g.fit_viewport(800.0, 800.0);
        assert!(v.y_min > 900.0 && v.y_max > 1000.0, "{v:?}");
        let mut g = Graph::new();
        g.add_equation("y = x");
        assert_eq!(
            g.fit_viewport(800.0, 800.0),
            Viewport::default_for_size(800.0, 800.0)
        );
    }

    #[test]
    fn line_widths() {
        assert_eq!(selected_line_width(2.0), 3.0);
        assert_eq!(selected_line_width(3.0), 5.0);
        assert_eq!(trace_point_radius(DEFAULT_LINE_WIDTH), 3.0);
    }

    #[test]
    fn graph_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Graph>();
        assert_send_sync::<Plot>();
        assert_send_sync::<CompiledEquation>();
    }
}
