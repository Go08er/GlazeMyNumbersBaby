//! Graphing calculator (upstream GraphingCalculator + EquationInputArea +
//! KeyGraphFeaturesPanel + GraphingSettings + GraphingNumPad).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use appcore::KeyPress;
use appcore::graph::{self as session, SavedEquation};
use appcore::input::{self, GraphAction};
use appcore::keys::GRAPH_PAD;
use graphing::analysis::KeyGraphFeatures;
use graphing::equation::LineStyle;
use graphing::graph::EquationPlot;
use graphing::grid::Grid;
use graphing::trace::TracePoint;
use graphing::{EquationId, Graph, TrigUnit, Viewport};
use tiny_skia::PathBuilder;
use winit::event_loop::EventLoopProxy;

use crate::app::{Cx, Msg as AppMsg, UserEvent};
use crate::edit::TextEdit;
use crate::gfx::{Color, Rect};
use crate::ui::{self, Align, BODY, CAPTION, Frame, Hit, SMALL, STRONG, Sense, Style, id};

pub const WIDE: f32 = 760.0;
const SIDE_W: f32 = 340.0;
const PAD_H: f32 = 214.0;
const INLINE_PLOT_MS: f64 = 12.0;
/// Re-analysis waits this long after the last edit (typing `sin(x)` doesn't
/// start six analyses).
const REANALYZE_AFTER: Duration = Duration::from_millis(150);

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Add,
    Remove(EquationId),
    Toggle(EquationId),
    Analyze(EquationId),
    Back,
    StylePopup(Option<EquationId>),
    Color(EquationId, usize),
    Line(EquationId, LineStyle),
    SettingsPopup(bool),
    ApplyRanges,
    Units(TrigUnit),
    Thickness(usize),
    ZoomIn,
    ZoomOut,
    Reset,
    Trace(bool),
    CopyImage,
    ShowGraph(bool),
    Pad(&'static str),
}

struct Row {
    id: EquationId,
    edit: TextEdit,
    color: usize,
}

enum Side {
    Equations,
    Analysis {
        id: EquationId,
        seq: u64,
        title: String,
        result: Option<Box<KeyGraphFeatures>>,
    },
}

#[derive(Clone, Copy, PartialEq)]
enum Popup {
    Style(EquationId),
    Settings,
}

/// A worker plot job a test holds instead of running it.
#[cfg(test)]
struct HeldJob {
    seq: u64,
    graph: Graph,
    vp: Viewport,
    cancel: Arc<AtomicBool>,
}

pub struct GraphPage {
    graph: Graph,
    rows: Vec<Row>,
    next_color: usize,
    vp: Option<Viewport>,
    plots: Vec<EquationPlot>,
    /// How long the plot shown took, its `Graph::plot_weight`, and that of
    /// the plot running on the worker.
    plot_ms: f64,
    plot_weight: f64,
    pending_weight: f64,
    dirty: bool,
    /// The latest plot request (inline or not), and the one the running
    /// worker job answers: a job's plots are shown only if no request came
    /// after its own.
    plot_seq: u64,
    plot_job: Option<u64>,
    /// A request is waiting for the running job to report back.
    again: bool,
    /// Cancels the running plot job when a newer request supersedes it.
    plot_cancel: Option<Arc<AtomicBool>>,
    /// Tests: worker jobs are held here, to be reported back by hand.
    #[cfg(test)]
    held_jobs: Option<Vec<HeldJob>>,
    analysis_seq: u64,
    analysis_cancel: Option<Arc<AtomicBool>>,
    /// The analysed equation changed: re-run analysis at this time.
    reanalyze_at: Option<Instant>,
    trace_on: bool,
    pointer: Option<(f32, f32)>,
    trace: Option<(EquationId, TracePoint)>,
    show_graph: bool,
    side: Side,
    popup: Option<Popup>,
    ranges: [TextEdit; 4],
    range_error: bool,
    line_width: f64,
    canvas: Rect,
    vars: BTreeMap<String, TextEdit>,
    /// The equation field the keypad types into.
    last_field: Option<ui::Id>,
    /// Delivers worker results; always set in the app (tests plot inline).
    proxy: Option<EventLoopProxy<UserEvent>>,
    drag_vp: Option<Viewport>,
}

fn msg(m: Msg) -> AppMsg {
    AppMsg::Graph(m)
}

fn eq_field(eq: EquationId) -> ui::Id {
    id(("eq", eq))
}

fn range_field(i: usize) -> ui::Id {
    id(("range", i))
}

fn var_field(name: &str) -> ui::Id {
    id(("var-field", name))
}

fn var_slider(name: &str) -> ui::Id {
    id(("var-slider", name))
}

/// How a variable slider moves.
#[derive(Clone, Copy, Debug)]
pub enum Adjust {
    /// To this fraction of its range (pointer).
    Fraction(f64),
    /// By this many steps (keyboard, AT increment/decrement).
    Steps(f64),
    Min,
    Max,
    /// To this value (AT set value).
    To(f64),
}

/// A variable's keyboard step: its own step, or a hundredth of its range.
fn var_step(v: &graphing::variable::Variable) -> f64 {
    if v.step() > 0.0 {
        v.step()
    } else {
        ((v.max() - v.min()) / 100.0).max(f64::MIN_POSITIVE)
    }
}

fn canvas_id() -> ui::Id {
    id("graph-canvas")
}

impl GraphPage {
    /// Rounds each number typed in an equation to `p` (Settings' "Number
    /// precision"), then shows the equations as read afresh.
    pub fn set_number_precision(&mut self, p: appcore::graph::NumberPrecision) {
        self.graph.set_literal_digits(p.digits());
        self.equations_reread();
    }

    /// The graph read every equation afresh (Number precision, the trig
    /// unit): the variables, the plot and the analysis follow, as after an
    /// edit (R14-M-01; errors are drawn from the graph each frame). An
    /// equation can stop or start being drawn with no edit (14 digits make
    /// `1.0000000000000001-1` zero), and with it the variables the graph
    /// lists (`Graph::refresh` keeps a slider for an equation not drawn for
    /// now and lists it again as it was).
    fn equations_reread(&mut self) {
        self.dirty = true;
        self.sync_vars();
        self.analysis_inputs_changed();
    }

    pub fn new(saved: Vec<SavedEquation>, proxy: EventLoopProxy<UserEvent>) -> GraphPage {
        Self::build(saved, Some(proxy))
    }

    #[cfg(test)]
    pub fn for_test(saved: Vec<SavedEquation>) -> GraphPage {
        Self::build(saved, None)
    }

    fn build(saved: Vec<SavedEquation>, proxy: Option<EventLoopProxy<UserEvent>>) -> GraphPage {
        let mut page = GraphPage {
            graph: Graph::new(),
            rows: Vec::new(),
            next_color: session::next_color(&saved),
            vp: None,
            plots: Vec::new(),
            plot_ms: 0.0,
            plot_weight: 0.0,
            pending_weight: 0.0,
            dirty: true,
            plot_seq: 0,
            plot_job: None,
            again: false,
            plot_cancel: None,
            #[cfg(test)]
            held_jobs: None,
            analysis_seq: 0,
            analysis_cancel: None,
            reanalyze_at: None,
            trace_on: false,
            pointer: None,
            trace: None,
            show_graph: false,
            side: Side::Equations,
            popup: None,
            ranges: std::array::from_fn(|_| TextEdit::new("", 24)),
            range_error: false,
            line_width: graphing::graph::DEFAULT_LINE_WIDTH,
            canvas: Rect::default(),
            vars: BTreeMap::new(),
            last_field: None,
            proxy,
            drag_vp: None,
        };
        for eq in &saved {
            let id = page.graph.add_equation(&eq.text);
            if let Some(style) = session::style_from_key(&eq.style) {
                page.graph.set_line_style(id, style);
            }
            if eq.hidden {
                page.graph.set_line_enabled(id, false);
            }
            page.rows.push(Row {
                id,
                edit: TextEdit::new(&eq.text, session::MAX_EQUATION_CHARS),
                color: eq.color,
            });
        }
        if page.rows.is_empty() {
            page.add("");
        }
        page.sync_vars();
        page
    }

    pub fn save(&self) -> serde_json::Value {
        let list: Vec<SavedEquation> = self
            .rows
            .iter()
            .filter(|r| !r.edit.text.trim().is_empty())
            .map(|r| SavedEquation {
                text: r.edit.text.clone(),
                color: r.color,
                style: session::style_key(self.graph.line_style(r.id)).into(),
                hidden: !self.graph.is_line_enabled(r.id),
            })
            .collect();
        serde_json::to_value(list).unwrap_or_default()
    }

    fn add(&mut self, text: &str) -> Option<EquationId> {
        if self.graph.len() >= session::MAX_EQUATIONS {
            return None;
        }
        let text = session::clamp_text(text);
        let id = self.graph.add_equation(text);
        self.rows.push(Row {
            id,
            edit: TextEdit::new(text, session::MAX_EQUATION_CHARS),
            color: self.next_color,
        });
        self.next_color += 1;
        self.dirty = true;
        Some(id)
    }

    fn sync_vars(&mut self) {
        let vars = self.graph.variables().clone();
        self.vars.retain(|k, _| vars.contains_key(k));
        for (name, v) in vars {
            let fmt = format_value(v.value());
            self.vars
                .entry(name)
                .and_modify(|e| {
                    if e.text.parse::<f64>().ok() != Some(v.value()) {
                        e.set_text(&fmt);
                    }
                })
                .or_insert_with(|| TextEdit::new(&fmt, 24));
        }
    }

    pub fn narrow_toggle(&self, width: f32) -> Option<bool> {
        (width < WIDE).then_some(self.show_graph)
    }

    pub fn close_popup(&mut self) -> bool {
        if self.popup.take().is_some() {
            return true;
        }
        if matches!(self.side, Side::Analysis { .. }) {
            self.side = Side::Equations;
            return true;
        }
        false
    }

    pub fn copy_text(&self) -> Option<String> {
        let f = self.last_field?;
        self.rows
            .iter()
            .find(|r| eq_field(r.id) == f)
            .map(|r| r.edit.text.clone())
    }

    pub fn paste(&mut self, text: &str, cx: &mut Cx) {
        self.insert(text.trim(), cx);
    }

    // ------------------------------------------------------------ fields

    pub fn field(&mut self, fid: ui::Id) -> Option<&mut TextEdit> {
        if let Some(r) = self.rows.iter_mut().find(|r| eq_field(r.id) == fid) {
            self.last_field = Some(fid);
            return Some(&mut r.edit);
        }
        if self.popup == Some(Popup::Settings)
            && let Some(i) = (0..4).find(|&i| range_field(i) == fid)
        {
            return Some(&mut self.ranges[i]);
        }
        self.vars
            .iter_mut()
            .find(|(k, _)| var_field(k) == fid)
            .map(|(_, e)| e)
    }

    pub fn field_changed(&mut self, fid: ui::Id, _cx: &mut Cx) {
        if let Some(r) = self.rows.iter().find(|r| eq_field(r.id) == fid) {
            let (id, text) = (r.id, r.edit.text.clone());
            self.graph.set_equation_text(id, &text);
            self.dirty = true;
            self.sync_vars();
            self.analysis_inputs_changed();
            return;
        }
        let var = self
            .vars
            .iter()
            .find(|(k, _)| var_field(k) == fid)
            .map(|(k, e)| (k.clone(), e.text.clone()));
        if let Some((name, text)) = var
            && let Ok(x) = text.replace('−', "-").trim().parse::<f64>()
            && x.is_finite()
        {
            // Widen the slider if the typed value is outside it.
            self.graph.update_variable(&name, |v| {
                if x < v.min() {
                    v.set_min(x);
                }
                if x > v.max() {
                    v.set_max(x);
                }
            });
            self.graph.set_variable(&name, x);
            self.dirty = true;
            self.analysis_inputs_changed();
        }
    }

    /// Enter in a field.
    pub fn field_activate(&mut self, fid: ui::Id, cx: &mut Cx) {
        if (0..4).any(|i| range_field(i) == fid) {
            self.apply_ranges();
            return;
        }
        // Enter plots and moves on to a fresh expression, like upstream.
        let last = self.rows.last().map(|r| eq_field(r.id)) == Some(fid);
        let filled = self
            .rows
            .last()
            .is_some_and(|r| !r.edit.text.trim().is_empty());
        if last
            && filled
            && let Some(id) = self.add("")
        {
            *cx.focus = Some(eq_field(id));
        }
    }

    fn insert(&mut self, text: &str, cx: &mut Cx) {
        let target = cx
            .focus
            .filter(|f| self.rows.iter().any(|r| eq_field(r.id) == *f))
            .or(self.last_field)
            .filter(|f| self.rows.iter().any(|r| eq_field(r.id) == *f))
            .or_else(|| self.rows.last().map(|r| eq_field(r.id)))
            .or_else(|| self.add("").map(eq_field));
        let Some(fid) = target else { return };
        if let Some(r) = self.rows.iter_mut().find(|r| eq_field(r.id) == fid) {
            if text == "\u{8}" {
                r.edit.backspace(false);
            } else {
                r.edit.insert(text);
            }
        }
        *cx.focus = Some(fid);
        self.last_field = Some(fid);
        self.field_changed(fid, cx);
    }

    fn apply_ranges(&mut self) {
        let p = |e: &TextEdit| e.text.replace('−', "-").trim().parse::<f64>().ok();
        let ok = match (
            p(&self.ranges[0]),
            p(&self.ranges[1]),
            p(&self.ranges[2]),
            p(&self.ranges[3]),
            self.vp,
        ) {
            (Some(x0), Some(x1), Some(y0), Some(y1), Some(mut vp)) => {
                let ok = vp.set_display_ranges(x0, x1, y0, y1).is_ok();
                if ok {
                    self.vp = Some(vp);
                    self.dirty = true;
                }
                ok
            }
            _ => false,
        };
        // Rejected ranges (unparsable, min ≥ max, or a span the graph can't
        // map) are flagged instead of silently ignored.
        self.range_error = !ok;
    }

    fn fill_ranges(&mut self) {
        if let Some(vp) = self.vp {
            // What was typed reads back exactly (1.0001, not 1).
            let (xs, ys) = (vp.x_span(), vp.y_span());
            for (e, (v, span)) in self.ranges.iter_mut().zip([
                (vp.x_min, xs),
                (vp.x_max, xs),
                (vp.y_min, ys),
                (vp.y_max, ys),
            ]) {
                e.set_text(&graphing::viewport::range_text(v, span));
            }
        }
        self.range_error = false;
    }

    // ------------------------------------------------------------ messages

    pub fn update(&mut self, m: Msg, cx: &mut Cx) {
        match m {
            Msg::Add => match self.add("") {
                Some(id) => *cx.focus = Some(eq_field(id)),
                None => cx.toast("You can graph up to 14 equations"),
            },
            Msg::Remove(id) => {
                self.graph.remove_equation(id);
                self.rows.retain(|r| r.id != id);
                self.dirty = true;
                self.sync_vars();
                if self.rows.is_empty() {
                    self.add("");
                }
                self.analysis_inputs_changed();
            }
            Msg::Toggle(id) => {
                let on = !self.graph.is_line_enabled(id);
                self.graph.set_line_enabled(id, on);
                self.dirty = true;
                // A hidden equation's variables aren't listed (kept, as
                // they were, for when it is shown).
                self.sync_vars();
            }
            Msg::Analyze(id) => {
                self.show_graph = false;
                self.start_analysis(id);
            }
            Msg::Back => {
                self.cancel_analysis();
                self.side = Side::Equations;
            }
            Msg::StylePopup(p) => self.popup = p.map(Popup::Style),
            Msg::Color(id, c) => {
                if let Some(r) = self.rows.iter_mut().find(|r| r.id == id) {
                    r.color = c;
                }
            }
            Msg::Line(id, style) => {
                self.graph.set_line_style(id, style);
                self.dirty = true;
            }
            Msg::SettingsPopup(open) => {
                self.popup = open.then_some(Popup::Settings);
                if open {
                    self.fill_ranges();
                }
            }
            Msg::ApplyRanges => self.apply_ranges(),
            Msg::Units(u) => {
                self.graph.set_trig_unit(u);
                self.equations_reread();
            }
            Msg::Thickness(i) => {
                self.line_width = graphing::graph::LINE_WIDTHS[i.min(3)];
            }
            Msg::ZoomIn => self.zoom(|vp| vp.zoom_in()),
            Msg::ZoomOut => self.zoom(|vp| vp.zoom_out()),
            Msg::Reset => {
                self.vp = None;
                self.dirty = true;
                if self.popup == Some(Popup::Settings) {
                    self.popup = None;
                }
            }
            Msg::Trace(on) => {
                self.trace_on = on;
                if !on {
                    self.trace = None;
                }
            }
            Msg::CopyImage => {} // handled by the app (it owns the fonts)
            Msg::ShowGraph(on) => self.show_graph = on,
            Msg::Pad(text) => self.insert(text, cx),
        }
    }

    fn zoom(&mut self, f: impl FnOnce(&mut Viewport)) {
        if let Some(vp) = self.vp.as_mut() {
            f(vp);
            self.dirty = true;
            if self.popup == Some(Popup::Settings) {
                self.fill_ranges();
            }
        }
    }

    pub fn key(&mut self, kp: &KeyPress, cx: &mut Cx) -> bool {
        use appcore::{Key, Named};
        if let (Key::Named(n), false, false) = (kp.key, kp.ctrl, kp.alt) {
            let focus = *cx.focus;
            if focus == Some(canvas_id()) {
                let d = if kp.shift { 1.0 } else { 5.0 };
                let moved = match n {
                    Named::Left => self.trace_key(-d, 0.0),
                    Named::Right => self.trace_key(d, 0.0),
                    Named::Up => self.trace_key(0.0, -d),
                    Named::Down => self.trace_key(0.0, d),
                    _ => false,
                };
                if moved {
                    return true;
                }
            }
            if let Some(f) = focus {
                let change = match n {
                    Named::Left | Named::Down => Some(Adjust::Steps(-1.0)),
                    Named::Right | Named::Up => Some(Adjust::Steps(1.0)),
                    Named::PageDown => Some(Adjust::Steps(-10.0)),
                    Named::PageUp => Some(Adjust::Steps(10.0)),
                    Named::Home => Some(Adjust::Min),
                    Named::End => Some(Adjust::Max),
                    _ => None,
                };
                if let Some(change) = change
                    && self.adjust_slider(f, change)
                {
                    return true;
                }
            }
        }
        match input::graph_shortcut(kp) {
            Some(GraphAction::ZoomIn) => self.zoom(|vp| vp.zoom_in()),
            Some(GraphAction::ZoomOut) => self.zoom(|vp| vp.zoom_out()),
            Some(GraphAction::ResetView) => {
                self.vp = None;
                self.dirty = true;
            }
            Some(GraphAction::ShowGraph) => self.show_graph = true,
            None => return false,
        }
        true
    }

    // ------------------------------------------------------------ pointer

    #[allow(clippy::too_many_arguments)]
    pub fn drag(
        &mut self,
        hid: ui::Id,
        rect: Rect,
        x: f32,
        _y: f32,
        dx: f32,
        dy: f32,
        active: bool,
        _cx: &mut Cx,
    ) {
        if hid == canvas_id() {
            if !active {
                self.drag_vp = None;
                return;
            }
            if let Some(vp) = self.vp.as_mut()
                && (dx != 0.0 || dy != 0.0)
            {
                vp.pan_pixels(dx as f64, dy as f64);
                self.dirty = true;
            }
            return;
        }
        if !active {
            return;
        }
        let track = rect.inset_xy(8.0, 0.0);
        let frac = ((x - track.x) / track.w.max(1.0)).clamp(0.0, 1.0) as f64;
        self.adjust_slider(hid, Adjust::Fraction(frac));
    }

    /// Move a variable's slider (`hid`), by pointer, keyboard or assistive
    /// technology. False if `hid` isn't a variable slider.
    pub fn adjust_slider(&mut self, hid: ui::Id, change: Adjust) -> bool {
        let Some(name) = self.vars.keys().find(|k| var_slider(k) == hid).cloned() else {
            return false;
        };
        let Some(v) = self.graph.variable(&name).copied() else {
            return false;
        };
        let step = var_step(&v);
        // On the step grid (to 15 digits, so 71 × 0.1 is 7.1), then within
        // the range: the ends themselves needn't be on the grid.
        let quantized = |x: f64| {
            if v.step() > 0.0 {
                let q = (x / v.step()).round() * v.step();
                format!("{q:.14e}").parse().unwrap_or(q)
            } else {
                x
            }
        };
        let value = match change {
            Adjust::Fraction(f) => quantized(v.min() + f * (v.max() - v.min())),
            Adjust::Steps(n) => quantized(v.value() + n * step),
            Adjust::Min => v.min(),
            Adjust::Max => v.max(),
            Adjust::To(x) => quantized(x),
        };
        if !value.is_finite() {
            return true;
        }
        let value = value.clamp(v.min(), v.max());
        self.graph.set_variable(&name, value);
        if let Some(e) = self.vars.get_mut(&name) {
            e.set_text(&format_value(value));
        }
        self.dirty = true;
        self.analysis_inputs_changed();
        true
    }

    /// Keyboard tracing, as the original's Grapher does: with the graph
    /// focused, the arrows move the trace cursor (5 px, or 1 px with Shift)
    /// and the traced point follows. The cursor keeps every step along the
    /// arrow's axis, even one too small to move the traced point, and
    /// moves to the curve across it.
    fn trace_key(&mut self, dx: f32, dy: f32) -> bool {
        let c = self.canvas;
        if c.w <= 1.0 || c.h <= 1.0 {
            return false;
        }
        self.trace_on = true;
        let (x, y) = self
            .pointer
            .filter(|&(x, y)| c.contains(x, y))
            .unwrap_or((c.cx(), c.cy()));
        let (x, y) = (
            (x + dx).clamp(c.x, c.right() - 1.0),
            (y + dy).clamp(c.y, c.bottom() - 1.0),
        );
        self.pointer = Some((x, y));
        self.update_trace();
        if let Some((_, t)) = &self.trace {
            let (tx, ty) = (c.x + t.screen_x as f32, c.y + t.screen_y as f32);
            self.pointer = Some(if dx != 0.0 { (x, ty) } else { (tx, y) });
        }
        true
    }

    pub fn wheel(&mut self, x: f32, y: f32, dy: f32, hits: &[Hit]) -> bool {
        let top = hits
            .iter()
            .rev()
            .find(|h| h.visible && h.rect.contains(x, y) && h.sense != Sense::Scroll);
        if !top.is_some_and(|h| h.id == canvas_id()) {
            return false;
        }
        let c = self.canvas;
        if let Some(vp) = self.vp.as_mut() {
            vp.wheel_zoom(
                (x - c.x) as f64,
                (y - c.y) as f64,
                dy as f64 / 48.0 * graphing::viewport::WHEEL_DELTA,
            );
            self.dirty = true;
        }
        true
    }

    /// Whether (x, y) is on the plot (where a pinch has to start).
    pub fn on_canvas(&self, x: f32, y: f32) -> bool {
        self.canvas.contains(x, y)
    }

    /// A pinch that began on the canvas: zoom by `factor` (below 1 zooms
    /// in) about (x, y), held inside the canvas. True if it applied.
    pub fn pinch(&mut self, x: f32, y: f32, factor: f64) -> bool {
        let c = self.canvas;
        if !factor.is_finite() || factor <= 0.0 || c.w <= 0.0 || c.h <= 0.0 {
            return false;
        }
        let (px, py) = ((x - c.x).clamp(0.0, c.w), (y - c.y).clamp(0.0, c.h));
        if let Some(vp) = self.vp.as_mut() {
            vp.zoom_about_pixel(px as f64, py as f64, factor);
            self.dirty = true;
        }
        true
    }

    /// Pointer moved; returns true if the trace changed.
    pub fn pointer(&mut self, x: f32, y: f32) -> bool {
        self.pointer = Some((x, y));
        if !self.trace_on {
            return false;
        }
        let before = self
            .trace
            .as_ref()
            .map(|t| (t.0, t.1.screen_x, t.1.screen_y));
        self.update_trace();
        before
            != self
                .trace
                .as_ref()
                .map(|t| (t.0, t.1.screen_x, t.1.screen_y))
    }

    pub fn pointer_left(&mut self) {
        self.pointer = None;
        self.trace = None;
    }

    fn update_trace(&mut self) {
        let (Some(vp), Some((x, y))) = (self.vp, self.pointer) else {
            self.trace = None;
            return;
        };
        let c = self.canvas;
        if !c.contains(x, y) {
            self.trace = None;
            return;
        }
        self.trace = self.graph.trace(
            &vp,
            &self.plots,
            (x - c.x) as f64,
            (y - c.y) as f64,
            graphing::trace::DEFAULT_TRACE_RADIUS_PX,
        );
    }

    // ------------------------------------------------------------ async results

    /// Analyse `id` on a worker, cancelling any analysis still running.
    fn start_analysis(&mut self, id: EquationId) {
        self.cancel_analysis();
        self.analysis_seq += 1;
        let seq = self.analysis_seq;
        let title = self.graph.text(id).unwrap_or_default().to_string();
        self.side = Side::Analysis {
            id,
            seq,
            title,
            result: None,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.analysis_cancel = Some(cancel.clone());
        let graph = self.graph.clone();
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        let spawned = std::thread::Builder::new()
            .name("analysis".into())
            .spawn(move || {
                if let Some(f) = graph.analyze_cancellable(id, &cancel) {
                    let _ = proxy.send_event(UserEvent::Analysis(seq, Box::new(f)));
                }
            });
        if spawned.is_err() {
            // No thread to be had: analyse here rather than spin forever.
            let features = self.graph.analyze(id);
            self.analysis_done(seq, features);
        }
    }

    fn cancel_analysis(&mut self) {
        if let Some(c) = self.analysis_cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        self.reanalyze_at = None;
    }

    /// Something the open analysis depends on changed (its equation, a
    /// variable, the trig unit): drop the shown result now and re-analyse
    /// shortly, or go back to the list if the equation is gone.
    fn analysis_inputs_changed(&mut self) {
        let Side::Analysis {
            id, result, title, ..
        } = &mut self.side
        else {
            return;
        };
        let id = *id;
        if !self.rows.iter().any(|r| r.id == id) {
            self.cancel_analysis();
            self.side = Side::Equations;
            return;
        }
        *result = None;
        *title = self.graph.text(id).unwrap_or_default().to_string();
        if let Some(c) = self.analysis_cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        self.analysis_seq += 1; // a result already in flight is now stale
        self.reanalyze_at = Some(Instant::now() + REANALYZE_AFTER);
    }

    /// When the page next needs waking (pending re-analysis).
    pub fn deadline(&self) -> Option<Instant> {
        self.reanalyze_at
    }

    /// Timer tick; true if something changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        match (self.reanalyze_at, &self.side) {
            (Some(t), Side::Analysis { id, .. }) if now >= t => {
                let id = *id;
                self.start_analysis(id);
                true
            }
            _ => false,
        }
    }

    pub fn analysis_done(&mut self, seq: u64, features: KeyGraphFeatures) {
        if let Side::Analysis { seq: s, result, .. } = &mut self.side
            && *s == seq
            && self.analysis_seq == seq
        {
            *result = Some(Box::new(features));
        }
    }

    /// A worker plot finished (`None`: it was cancelled for a newer one).
    /// Its plots are shown only if no request came after its own, inline
    /// or not (R12-M-06: an old job finishing after a quick inline plot
    /// put its old graph's curves back).
    pub fn plot_done(&mut self, seq: u64, plots: Option<Vec<EquationPlot>>, ms: f64, _cx: &mut Cx) {
        if self.plot_job != Some(seq) {
            return;
        }
        self.plot_job = None;
        self.plot_cancel = None;
        let heavy = plots.is_some() && ms >= INLINE_PLOT_MS;
        if seq == self.plot_seq
            && let Some(plots) = plots
        {
            self.plot_ms = ms;
            self.plot_weight = self.pending_weight;
            self.plots = plots;
        }
        if self.again {
            self.again = false;
            self.dirty = true;
        } else if heavy {
            // A heavy plot's scratch (and the plots it replaced, just
            // freed) stays in the allocator's per-thread arenas: hand it
            // back, off this thread (it takes ~15 ms), once no newer plot
            // is wanted (a trim during a pan would contend with its
            // allocations).
            release_free_memory();
        }
        self.update_trace();
    }

    /// Whether plots can go to a worker (the app's; tests holding jobs).
    fn can_spawn(&self) -> bool {
        #[cfg(test)]
        if self.held_jobs.is_some() {
            return true;
        }
        self.proxy.is_some()
    }

    fn replot(&mut self) {
        let Some(vp) = self.vp else { return };
        if !self.dirty {
            return;
        }
        self.dirty = false;
        // A new request: whatever runs now answers an older one.
        self.plot_seq += 1;
        if let Some(c) = &self.plot_cancel {
            c.store(true, Ordering::Relaxed);
        }
        // Inline only if this plot, scaled from the last by the graph's
        // weight, is quick: an edit to a heavy row goes to the worker.
        let weight = self.graph.plot_weight();
        let predicted = graphing::graph::predicted_plot_ms(self.plot_ms, self.plot_weight, weight);
        if predicted < INLINE_PLOT_MS || !self.can_spawn() {
            self.again = false;
            let t = Instant::now();
            self.plots = self.graph.plot_parallel(&vp);
            self.plot_ms = t.elapsed().as_secs_f64() * 1e3;
            self.plot_weight = weight;
            self.update_trace();
            return;
        }
        // Heavy graph: plot on a worker and keep drawing the last result
        // (curves are in graph coordinates, so they still line up while
        // panning). One job at a time: a request made while one runs (and
        // was cancelled above) is made again once it reports back.
        if self.plot_job.is_some() {
            self.again = true;
            return;
        }
        let seq = self.plot_seq;
        self.plot_job = Some(seq);
        self.pending_weight = weight;
        let cancel = Arc::new(AtomicBool::new(false));
        self.plot_cancel = Some(cancel.clone());
        let graph = self.graph.clone();
        #[cfg(test)]
        if let Some(held) = &mut self.held_jobs {
            held.push(HeldJob {
                seq,
                graph,
                vp,
                cancel,
            });
            return;
        }
        let Some(proxy) = self.proxy.clone() else {
            return;
        };
        let spawned = std::thread::Builder::new()
            .name("plot".into())
            .spawn(move || {
                let t = Instant::now();
                let plots = graph.plot_parallel_cancellable(&vp, &cancel);
                let ms = t.elapsed().as_secs_f64() * 1e3;
                let _ = proxy.send_event(UserEvent::Plot(seq, plots, ms));
            });
        if spawned.is_err() {
            // No thread to be had: plot here, one equation after another
            // (plot_parallel would need threads too), rather than wait.
            self.plot_job = None;
            self.plot_cancel = None;
            self.plots = self.graph.plot(&vp);
            self.update_trace();
        }
    }

    // ------------------------------------------------------------ view

    pub fn view(&mut self, f: &mut Frame, area: Rect) {
        let wide = area.w >= WIDE;
        let area = area.inset_xy(8.0, 4.0);
        if wide {
            let (side, canvas) = area.take_left(SIDE_W);
            self.side_panel(f, side);
            self.draw_canvas(f, canvas.take_right(canvas.w - 8.0).0, true);
        } else if self.show_graph {
            self.draw_canvas(f, area, true);
        } else {
            self.side_panel(f, area);
        }
    }

    fn side_panel(&mut self, f: &mut Frame, r: Rect) {
        let t = f.t;
        let (pad, top) = r.inset_xy(0.0, 4.0).take_bottom(PAD_H);
        f.cv.fill_rect(Rect::new(pad.x, pad.y - 1.0, pad.w, 1.0), t.border);
        match &self.side {
            Side::Equations => self.equations(f, top),
            Side::Analysis { .. } => self.analysis(f, top),
        }
        self.keypad(f, pad.take_bottom(PAD_H - 8.0).0);
    }

    fn equations(&mut self, f: &mut Frame, r: Rect) {
        let t = f.t;
        let sid = id("eq-scroll");
        let off = f.scroll_begin(sid, r, "Equations");
        let mut y = r.y - off;
        let n_series = t.series.len();
        for i in 0..self.rows.len() {
            let (eid, color) = (self.rows[i].id, self.rows[i].color);
            let line = Rect::new(r.x, y, r.w, 40.0);
            let c = t.series[color % n_series];
            let enabled = self.graph.is_line_enabled(eid);
            // Swatch: show/hide.
            let sw = Rect::new(line.x + 2.0, line.y + 6.0, 28.0, 28.0);
            let swid = id(("swatch", eid));
            f.cv.circle(
                sw.cx(),
                sw.cy(),
                9.0,
                if enabled { c } else { c.alpha(0.3) },
            );
            if f.hovered(swid) {
                f.cv.rounded(sw, 14.0, t.hover);
            }
            f.hit(swid, sw, Sense::Click, Some(msg(Msg::Toggle(eid))), true);
            if let Some(n) = f.node(swid, accesskit::Role::Button, "Show or hide", sw) {
                n.toggled = Some(enabled);
                n.clickable = true;
                n.focusable = true;
            }
            let (buttons, field) =
                Rect::new(line.x + 34.0, line.y + 2.0, line.w - 34.0, 36.0).take_right(96.0);
            let err = self.graph.error(eid).cloned();
            let has_err = err.is_some() && !self.rows[i].edit.text.trim().is_empty();
            f.text_field(
                eq_field(eid),
                field.take_left(field.w - 4.0).0,
                &self.rows[i].edit,
                "Enter an expression",
                has_err,
                "Equation",
            );
            let b = |k: usize| Rect::new(buttons.x + k as f32 * 32.0, buttons.y + 2.0, 32.0, 32.0);
            f.icon_button(
                id(("eq-style", eid)),
                b(0),
                appcore::icons::CHEVRON_DOWN,
                "Line color and style",
                msg(Msg::StylePopup(Some(eid))),
                true,
                Some(self.popup == Some(Popup::Style(eid))),
            );
            f.icon_button(
                id(("eq-analyze", eid)),
                b(1),
                appcore::icons::FUNCTION,
                "Analyze function",
                msg(Msg::Analyze(eid)),
                err.is_none() && !self.rows[i].edit.text.trim().is_empty(),
                None,
            );
            f.icon_button(
                id(("eq-remove", eid)),
                b(2),
                appcore::icons::CLOSE,
                "Remove equation",
                msg(Msg::Remove(eid)),
                true,
                None,
            );
            y += 42.0;
            if has_err && let Some(e) = err {
                let er = Rect::new(r.x + 36.0, y - 2.0, r.w - 40.0, 18.0);
                f.label_fit(er, e.message(), CAPTION, 9.0, t.danger, Align::Start);
                // Read out as GMNB's error label is.
                f.node(
                    id(("eq-error", eid)),
                    accesskit::Role::Label,
                    e.message(),
                    er,
                );
                y += 18.0;
            }
        }
        let add = Rect::new(r.x, y + 2.0, r.w, 36.0);
        f.button(
            id("eq-add"),
            add,
            "+  Enter an expression",
            BODY,
            msg(Msg::Add),
            self.graph.len() < session::MAX_EQUATIONS,
            None,
            false,
        );
        // Read without its "+", as GMNB's.
        if let Some(n) = f.nodes.as_mut().and_then(|v| v.last_mut()) {
            n.label = "Enter an expression".into();
        }
        y += 44.0;
        if !self.vars.is_empty() {
            f.label(
                Rect::new(r.x + 4.0, y, r.w, 26.0),
                "Variables",
                STRONG,
                t.fg,
                Align::Start,
            );
            y += 28.0;
            let names: Vec<String> = self.vars.keys().cloned().collect();
            for name in names {
                let Some(v) = self.graph.variable(&name).copied() else {
                    continue;
                };
                let row = Rect::new(r.x, y, r.w, 36.0);
                let (label, rest) = row.take_left(34.0);
                f.label(
                    label.inset_xy(6.0, 0.0),
                    &name,
                    Style::new(15.0, 600.0),
                    t.fg,
                    Align::Start,
                );
                let (value, slider) = rest.take_right(84.0);
                let span = (v.max() - v.min()).max(1e-12);
                let frac = ((v.value() - v.min()) / span) as f32;
                f.slider(
                    var_slider(&name),
                    slider,
                    frac,
                    &format!("Variable {name}"),
                    &format_value(v.value()),
                    [v.value(), v.min(), v.max(), var_step(&v)],
                );
                if let Some(e) = self.vars.get(&name) {
                    f.text_field(
                        var_field(&name),
                        value.inset_xy(2.0, 2.0),
                        e,
                        "0",
                        false,
                        &format!("Value of {name}"),
                    );
                }
                y += 40.0;
            }
        }
        f.scroll_end(sid, r, y + off - r.y);
    }

    fn analysis(&mut self, f: &mut Frame, r: Rect) {
        let t = f.t;
        let Side::Analysis { title, result, .. } = &self.side else {
            return;
        };
        let (head, body) = r.take_top(40.0);
        f.button(
            id("an-back"),
            head.take_left(190.0).0,
            "‹  Function analysis",
            STRONG,
            msg(Msg::Back),
            true,
            None,
            false,
        );
        let sid = id("an-scroll");
        let off = f.scroll_begin(sid, body, "Function analysis");
        let mut y = body.y - off;
        f.label_fit(
            Rect::new(body.x + 6.0, y, body.w - 12.0, 30.0),
            title,
            Style::new(18.0, 600.0),
            10.0,
            t.fg,
            Align::Start,
        );
        y += 36.0;
        match result {
            None => {
                f.label(
                    Rect::new(body.x + 6.0, y, body.w, 24.0),
                    "Analyzing…",
                    SMALL,
                    t.fg_dim,
                    Align::Start,
                );
                y += 28.0;
            }
            Some(features) => {
                if let Some(m) = features.analysis_error_string() {
                    for l in f.wrap(m, body.w - 12.0, SMALL) {
                        f.label(
                            Rect::new(body.x + 6.0, y, body.w, 20.0),
                            &l,
                            SMALL,
                            t.fg_dim,
                            Align::Start,
                        );
                        y += 20.0;
                    }
                } else {
                    for (k, item) in features.items().iter().enumerate() {
                        let top = y;
                        if !item.title.is_empty() {
                            f.label(
                                Rect::new(body.x + 12.0, y + 6.0, body.w - 24.0, 20.0),
                                &item.title,
                                CAPTION,
                                t.fg_dim,
                                Align::Start,
                            );
                            y += 26.0;
                        }
                        for v in &item.display_items {
                            for l in f.wrap(v, body.w - 24.0, BODY) {
                                f.label(
                                    Rect::new(body.x + 12.0, y, body.w - 24.0, 22.0),
                                    &l,
                                    if item.is_text { SMALL } else { BODY },
                                    t.fg,
                                    Align::Start,
                                );
                                y += 22.0;
                            }
                        }
                        for g in &item.grid_items {
                            let row = Rect::new(body.x + 12.0, y, body.w - 24.0, 22.0);
                            let (a, b) = row.take_left(row.w * 0.6);
                            f.label_fit(a, &g.expression, BODY, 9.0, t.fg, Align::Start);
                            f.label_fit(b, &g.direction, SMALL, 9.0, t.fg_dim, Align::Start);
                            y += 22.0;
                        }
                        // A list proven correct but maybe not complete
                        // says so.
                        if !item.note.is_empty() {
                            for l in f.wrap(&item.note, body.w - 24.0, CAPTION) {
                                f.label(
                                    Rect::new(body.x + 12.0, y, body.w - 24.0, 18.0),
                                    &l,
                                    CAPTION,
                                    t.fg_dim,
                                    Align::Start,
                                );
                                y += 18.0;
                            }
                        }
                        y += 8.0;
                        let card = Rect::new(body.x + 4.0, top, body.w - 8.0, y - top);
                        f.cv.rounded_border(card, 8.0, t.border, 1.0);
                        // Everything the card shows, monotonicity rows
                        // included; the untitled footer card is a note.
                        let title = if item.title.is_empty() {
                            "Note"
                        } else {
                            &item.title
                        };
                        if let Some(n) = f.node(id(("kgf", k)), accesskit::Role::Group, title, card)
                        {
                            let rows = item
                                .grid_items
                                .iter()
                                .map(|g| format!("{} {}", g.expression, g.direction));
                            let text: Vec<String> =
                                item.display_items.iter().cloned().chain(rows).collect();
                            // "≈" (known to its digits only) read as
                            // "approximately".
                            let mut value = graphing::trace::spoken(&text.join(", "));
                            if !item.note.is_empty() {
                                value.push_str(". ");
                                value.push_str(&item.note);
                            }
                            n.value = Some(value);
                        }
                        y += 6.0;
                    }
                }
            }
        }
        f.scroll_end(sid, body, y + off - body.y);
    }

    fn keypad(&mut self, f: &mut Frame, r: Rect) {
        let rows = GRAPH_PAD.len();
        let cols = GRAPH_PAD[0].len();
        f.group(id("graph-pad"), accesskit::Role::Group, "Keypad", r);
        for (ri, row) in GRAPH_PAD.iter().enumerate() {
            for (ci, (label, insert)) in row.iter().enumerate() {
                let cell = r.cell(rows, cols, ri, ci, 3.0);
                let look = if label.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    ui::KeyLook::Number
                } else {
                    ui::KeyLook::Function
                };
                let icon = (*label == "⌫").then_some(appcore::icons::BACKSPACE);
                let kid = id(("gpad", ri, ci));
                f.key(
                    kid,
                    cell,
                    label,
                    icon,
                    look,
                    appcore::keys::graph_pad_name(label),
                    msg(Msg::Pad(insert)),
                    true,
                );
                // Upstream's GraphingNumPad keys are CalculatorButtons,
                // which ignore Enter (graphing has none of its own here)
                // and can't take the focus: a click leaves it in the
                // equation they type into.
                f.calculator_key(kid);
                f.no_focus_on_click(kid);
            }
        }
        f.end_group();
    }

    pub fn canvas_rect(&self) -> Rect {
        self.canvas
    }

    /// Undo the canvas resize an off-screen render (copy image) caused.
    pub fn restore_canvas_rect(&mut self, r: Rect) {
        self.canvas = r;
        if let Some(vp) = self.vp {
            self.vp = Some(vp.with_size(r.w as f64, r.h as f64));
        }
    }

    /// Draw the graph canvas into `r` (also used to render "copy image").
    pub fn draw_canvas(&mut self, f: &mut Frame, r: Rect, controls: bool) {
        let t = f.t;
        self.canvas = r;
        let (w, h) = (r.w as f64, r.h as f64);
        match self.vp {
            Some(vp) if (vp.width - w).abs() > 0.5 || (vp.height - h).abs() > 0.5 => {
                self.vp = Some(vp.with_size(w, h));
                self.dirty = true;
            }
            None if w > 2.0 && h > 2.0 => {
                self.vp = Some(self.graph.fit_viewport(w, h));
                self.dirty = true;
            }
            _ => {}
        }
        self.replot();
        let Some(vp) = self.vp else { return };
        f.cv.rounded(r, 12.0, t.surface);
        f.push_clip(r);
        let to_sx = |x: f64| r.x + vp.to_screen(x, 0.0).0 as f32;
        let to_sy = |y: f64| r.y + vp.to_screen(0.0, y).1 as f32;
        let grid = Grid::for_viewport(&vp);
        let minor = t.fg.alpha(if t.dark { 0.05 } else { 0.06 });
        let major = t.fg.alpha(if t.dark { 0.11 } else { 0.13 });
        for x in &grid.x.minor {
            let sx = to_sx(*x);
            f.cv.line(sx, r.y, sx, r.bottom(), minor, 1.0);
        }
        for y in &grid.y.minor {
            let sy = to_sy(*y);
            f.cv.line(r.x, sy, r.right(), sy, minor, 1.0);
        }
        for tk in &grid.x.major {
            let sx = to_sx(tk.value);
            f.cv.line(sx, r.y, sx, r.bottom(), major, 1.0);
        }
        for tk in &grid.y.major {
            let sy = to_sy(tk.value);
            f.cv.line(r.x, sy, r.right(), sy, major, 1.0);
        }
        let axis = t.fg.alpha(0.55);
        let (ax, ay) = (to_sx(0.0), to_sy(0.0));
        if (r.x..=r.right()).contains(&ax) {
            f.cv.line(ax, r.y, ax, r.bottom(), axis, 1.3);
        }
        if (r.y..=r.bottom()).contains(&ay) {
            f.cv.line(r.x, ay, r.right(), ay, axis, 1.3);
        }
        let label_c = t.fg_dim;
        let st = Style::new(11.0, 400.0);
        let ly = ay.clamp(r.y + 4.0, r.bottom() - 18.0);
        for tk in &grid.x.major {
            if tk.value.abs() < 1e-12 {
                continue;
            }
            let line = f.layout(&tk.label, st);
            let sx = to_sx(tk.value) - line.width / 2.0;
            if sx < r.x + 4.0 || sx + line.width > r.right() - 4.0 {
                continue;
            }
            f.text
                .draw(&mut f.cv, &line, sx, ly + 4.0 + line.cap, label_c);
        }
        let lx = ax.clamp(r.x + 4.0, r.right() - 40.0);
        for tk in &grid.y.major {
            if tk.value.abs() < 1e-12 {
                continue;
            }
            let line = f.layout(&tk.label, st);
            let x = if lx + 6.0 + line.width > r.right() {
                lx - 6.0 - line.width
            } else {
                lx + 6.0
            };
            let y = to_sy(tk.value);
            if y < r.y + 10.0 || y > r.bottom() - 10.0 {
                continue;
            }
            f.text
                .draw(&mut f.cv, &line, x, y + line.cap / 2.0, label_c);
        }
        // Curves.
        let colors: Vec<(EquationId, Color)> = self
            .rows
            .iter()
            .map(|row| (row.id, t.series[row.color % t.series.len()]))
            .collect();
        for (i, ep) in self.plots.iter().enumerate() {
            let color = colors
                .iter()
                .find(|c| c.0 == ep.id)
                .map(|c| c.1)
                .unwrap_or(t.series[i % t.series.len()]);
            let pts = |pb: &mut PathBuilder, poly: &[graphing::Point], close: bool| {
                for (k, p) in poly.iter().enumerate() {
                    let (sx, sy) = vp.to_screen(p.x, p.y);
                    let (sx, sy) = (r.x + sx as f32, r.y + sy as f32);
                    if k == 0 {
                        pb.move_to(sx, sy);
                    } else {
                        pb.line_to(sx, sy);
                    }
                }
                if close {
                    pb.close();
                }
            };
            if !ep.plot.fill.is_empty() {
                let mut pb = PathBuilder::new();
                for poly in &ep.plot.fill {
                    pts(&mut pb, poly, true);
                }
                if let Some(p) = pb.finish() {
                    f.cv.fill_path(&p, color.alpha(0.18));
                }
            }
            let mut pb = PathBuilder::new();
            for poly in &ep.plot.curves {
                pts(&mut pb, poly, false);
            }
            if let Some(p) = pb.finish() {
                let style = self.graph.line_style(ep.id);
                let dash: Option<&[f32]> = match (ep.plot.boundary_dashed, style) {
                    (true, _) | (_, LineStyle::Dash) => Some(&[8.0, 6.0]),
                    (_, LineStyle::Dot) => Some(&[0.1, 5.0]),
                    (_, LineStyle::DashDot) => Some(&[8.0, 5.0, 0.1, 5.0]),
                    (_, LineStyle::DashDotDot) => Some(&[8.0, 5.0, 0.1, 5.0, 0.1, 5.0]),
                    _ => None,
                };
                f.cv.stroke_path(&p, color, self.line_width as f32, dash);
            }
            // Proven holes ((x²−1)/(x−1) at 1): open circles over the gap.
            let hole_r = graphing::graph::trace_point_radius(self.line_width) as f32 + 1.0;
            for h in &ep.plot.holes {
                let (sx, sy) = vp.to_screen(h.x, h.y);
                let (sx, sy) = (r.x + sx as f32, r.y + sy as f32);
                f.cv.circle(sx, sy, hole_r, t.surface);
                if let Some(p) = PathBuilder::from_circle(sx, sy, hole_r) {
                    let w = (self.line_width as f32 * 0.75).max(1.25);
                    f.cv.stroke_path(&p, color, w, None);
                }
            }
        }
        // Trace.
        if let Some((eid, tp)) = &self.trace {
            let color = colors
                .iter()
                .find(|c| c.0 == *eid)
                .map(|c| c.1)
                .unwrap_or(t.accent);
            let (sx, sy) = (r.x + tp.screen_x as f32, r.y + tp.screen_y as f32);
            f.cv.line(sx, r.y, sx, r.bottom(), color.alpha(0.45), 1.0);
            f.cv.line(r.x, sy, r.right(), sy, color.alpha(0.45), 1.0);
            let rad = graphing::graph::trace_point_radius(self.line_width) as f32 + 2.0;
            f.cv.circle(sx, sy, rad + 1.5, t.surface);
            f.cv.circle(sx, sy, rad, color);
            let text = tp.text();
            let line = f.layout(&text, SMALL);
            let (bw, bh) = (line.width + 18.0, 28.0);
            let mut bx = sx + 14.0;
            let mut by = sy - bh - 14.0;
            if bx + bw > r.right() - 6.0 {
                bx = sx - bw - 14.0;
            }
            if by < r.y + 6.0 {
                by = sy + 14.0;
            }
            let bubble = Rect::new(bx, by, bw, bh);
            f.surface(bubble, 8.0);
            f.draw_line(&line, bubble, Align::Center, t.fg);
            // Read as a screen reader should: "≈" as "approximately".
            let spoken = graphing::trace::spoken(&text);
            if let Some(n) = f.node(id("trace"), accesskit::Role::Label, &spoken, bubble) {
                n.live = true;
            }
        }
        f.pop_clip();
        f.cv.rounded_border(r, 12.0, t.border, 1.0);
        if !controls {
            return;
        }
        f.hit(canvas_id(), r, Sense::Drag, None, true);
        if let Some(n) = f.node(canvas_id(), accesskit::Role::Image, "Graph", r) {
            n.focusable = true;
        }
        // Toolbar.
        let tb = Rect::new(r.right() - 46.0, r.y + 8.0, 38.0, 6.0 * 36.0 + 8.0);
        f.cv.rounded(tb, 10.0, t.surface2.alpha(0.94));
        let items: [(&'static str, &str, Msg, Option<bool>); 6] = [
            (
                appcore::icons::ZOOM_IN,
                "Zoom in (Ctrl+Plus)",
                Msg::ZoomIn,
                None,
            ),
            (
                appcore::icons::ZOOM_OUT,
                "Zoom out (Ctrl+Minus)",
                Msg::ZoomOut,
                None,
            ),
            (
                appcore::icons::ZOOM_FIT,
                "Reset view (Ctrl+0)",
                Msg::Reset,
                None,
            ),
            (
                appcore::icons::TRACE,
                "Trace",
                Msg::Trace(!self.trace_on),
                Some(self.trace_on),
            ),
            (
                appcore::icons::COPY,
                "Copy graph image",
                Msg::CopyImage,
                None,
            ),
            (
                appcore::icons::SLIDERS,
                "Graph options",
                Msg::SettingsPopup(self.popup != Some(Popup::Settings)),
                Some(self.popup == Some(Popup::Settings)),
            ),
        ];
        for (k, (icon, name, m, on)) in items.into_iter().enumerate() {
            let b = Rect::new(tb.x + 2.0, tb.y + 4.0 + k as f32 * 36.0, 34.0, 34.0);
            f.icon_button(id(("gtool", k)), b, icon, name, msg(m), true, on);
        }
    }

    pub fn overlay(&mut self, f: &mut Frame, area: Rect) {
        let Some(popup) = self.popup else { return };
        let t = f.t;
        match popup {
            Popup::Style(eid) => {
                f.scrim(msg(Msg::StylePopup(None)), false);
                let anchor = f
                    .hits
                    .iter()
                    .find(|h| h.id == id(("eq-style", eid)))
                    .map(|h| h.rect)
                    .unwrap_or(area);
                let (w, h) = (264.0, 150.0);
                let x = (anchor.right() - w).clamp(area.x + 8.0, area.right() - w - 8.0);
                let y = (anchor.bottom() + 4.0).min(area.bottom() - h - 8.0);
                let card = Rect::new(x, y, w, h);
                f.card(card, 12.0);
                let inner = card.inset(12.0);
                f.label(
                    inner.take_top(20.0).0,
                    "Color",
                    CAPTION,
                    t.fg_dim,
                    Align::Start,
                );
                let cur = self
                    .rows
                    .iter()
                    .find(|r| r.id == eid)
                    .map_or(0, |r| r.color % t.series.len());
                for (i, c) in t.series.iter().enumerate() {
                    let b = Rect::new(inner.x + i as f32 * 40.0, inner.y + 24.0, 34.0, 34.0);
                    let cid = id(("color", i));
                    if i == cur {
                        f.cv.rounded_border(b, 17.0, t.fg, 2.0);
                    }
                    f.cv.circle(b.cx(), b.cy(), 11.0, *c);
                    f.hit(cid, b, Sense::Click, Some(msg(Msg::Color(eid, i))), true);
                    if let Some(n) = f.node(
                        cid,
                        accesskit::Role::RadioButton,
                        &format!("Color {}", i + 1),
                        b,
                    ) {
                        // Radio buttons report "checked" from toggled.
                        n.toggled = Some(i == cur);
                        n.selected = Some(i == cur);
                        n.clickable = true;
                        n.focusable = true;
                    }
                }
                f.label(
                    Rect::new(inner.x, inner.y + 64.0, inner.w, 20.0),
                    "Line style",
                    CAPTION,
                    t.fg_dim,
                    Align::Start,
                );
                let seg = Rect::new(inner.x, inner.y + 88.0, inner.w, 32.0);
                let style = self.graph.line_style(eid);
                for (i, (s, _, label)) in session::STYLES.into_iter().enumerate() {
                    f.button(
                        id(("lstyle", i)),
                        seg.cell(1, 3, 0, i, 4.0),
                        label,
                        SMALL,
                        msg(Msg::Line(eid, s)),
                        true,
                        Some(style == s),
                        true,
                    );
                }
            }
            Popup::Settings => {
                f.scrim(msg(Msg::SettingsPopup(false)), false);
                let anchor = f
                    .hits
                    .iter()
                    .find(|h| h.id == id(("gtool", 5usize)))
                    .map(|h| h.rect)
                    .unwrap_or(area);
                let (w, h) = (300.0, 300.0);
                let x = (anchor.x - w - 6.0).max(area.x + 8.0);
                let y = anchor.y.min(area.bottom() - h - 8.0).max(area.y + 8.0);
                let card = Rect::new(x, y, w, h);
                f.card(card, 12.0);
                let inner = card.inset(12.0);
                f.label(
                    inner.take_top(20.0).0,
                    "Window",
                    CAPTION,
                    t.fg_dim,
                    Align::Start,
                );
                for (i, label) in ["X-Min", "X-Max", "Y-Min", "Y-Max"].into_iter().enumerate() {
                    let c = Rect::new(inner.x, inner.y + 24.0, inner.w, 72.0).cell(
                        2,
                        2,
                        i / 2,
                        i % 2,
                        8.0,
                    );
                    let (l, e) = c.take_left(48.0);
                    f.label(l, label, SMALL, t.fg, Align::Start);
                    f.text_field(
                        range_field(i),
                        e,
                        &self.ranges[i],
                        "",
                        self.range_error,
                        label,
                    );
                }
                f.button(
                    id("ranges-apply"),
                    Rect::new(inner.right() - 80.0, inner.y + 100.0, 80.0, 30.0),
                    "Apply",
                    SMALL,
                    msg(Msg::ApplyRanges),
                    true,
                    None,
                    true,
                );
                f.label(
                    Rect::new(inner.x, inner.y + 136.0, inner.w, 20.0),
                    "Units",
                    CAPTION,
                    t.fg_dim,
                    Align::Start,
                );
                let seg = Rect::new(inner.x, inner.y + 158.0, inner.w, 30.0);
                let unit = self.graph.trig_unit();
                for (i, (u, label)) in [
                    (TrigUnit::Radians, "Radians"),
                    (TrigUnit::Degrees, "Degrees"),
                    (TrigUnit::Grads, "Gradians"),
                ]
                .into_iter()
                .enumerate()
                {
                    f.button(
                        id(("gunit", i)),
                        seg.cell(1, 3, 0, i, 4.0),
                        label,
                        SMALL,
                        msg(Msg::Units(u)),
                        true,
                        Some(unit == u),
                        true,
                    );
                }
                f.label(
                    Rect::new(inner.x, inner.y + 196.0, inner.w, 20.0),
                    "Line thickness",
                    CAPTION,
                    t.fg_dim,
                    Align::Start,
                );
                let seg = Rect::new(inner.x, inner.y + 218.0, inner.w - 96.0, 30.0);
                for (i, lw) in graphing::graph::LINE_WIDTHS.iter().enumerate() {
                    f.button(
                        id(("gthick", i)),
                        seg.cell(1, 4, 0, i, 4.0),
                        &format!("{lw}"),
                        SMALL,
                        msg(Msg::Thickness(i)),
                        true,
                        Some((self.line_width - lw).abs() < 1e-9),
                        true,
                    );
                }
                f.button(
                    id("greset"),
                    Rect::new(inner.right() - 88.0, inner.y + 218.0, 88.0, 30.0),
                    "Reset view",
                    SMALL,
                    msg(Msg::Reset),
                    true,
                    None,
                    false,
                );
            }
        }
    }
}

/// Returns the heap's free memory to the system (PREREVIEW_D: 14 ×
/// `sin(x*y)<0` left idle RSS at 135 MB, 123 of it free arena memory).
fn release_free_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        let _ = std::thread::Builder::new()
            .name("trim".into())
            // SAFETY: malloc_trim only returns free memory to the system.
            .spawn(|| unsafe {
                libc::malloc_trim(0);
            });
    }
}

fn format_value(v: f64) -> String {
    // To 3 decimals, as the field shows; past 10¹⁵ (where v·1000 can
    // overflow and every double is whole anyway) in e-notation.
    if !v.is_finite() || v.abs() >= 1e15 {
        return format!("{v:e}");
    }
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { "0".into() } else { format!("{r}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A variable's value as its field shows it, however big.
    #[test]
    fn variable_values_format_at_any_size() {
        assert_eq!(format_value(1.23456), "1.235");
        assert_eq!(format_value(-0.0001), "0");
        assert_eq!(format_value(1e306), "1e306");
        assert_eq!(format_value(-2.5e20), "-2.5e20");
    }

    /// Pre-review: a variable's slider works from the keyboard (and says
    /// its number), and arrows on the focused graph trace it.
    #[test]
    fn sliders_and_tracing_work_from_the_keyboard() {
        use appcore::Named;
        let mut g = GraphPage::for_test(session::from_list("y=a*x"));
        let mut pm = tiny_skia::Pixmap::new(760, 700).unwrap();
        let (mut text, mut icons, input) = (
            crate::text::Text::new(),
            ui::Icons::default(),
            ui::Input::default(),
        );
        let mut scrolls = std::collections::HashMap::new();
        let mut f = Frame::new(
            crate::gfx::Canvas::new(pm.as_mut(), 1.0, false),
            &mut text,
            &mut icons,
            crate::theme::Theme::new(false, None),
            &input,
            &mut scrolls,
            true,
        );
        g.view(&mut f, Rect::new(0.0, 46.0, 760.0, 654.0));
        let node = f
            .nodes
            .as_ref()
            .unwrap()
            .iter()
            .find(|n| n.id == var_slider("a"))
            .expect("slider drawn")
            .clone();
        drop(f);
        let v = *g.graph.variable("a").unwrap();
        assert_eq!(
            node.numeric,
            Some([v.value(), v.min(), v.max(), var_step(&v)])
        );

        let (mut toasts, mut focus) = (Vec::new(), Some(var_slider("a")));
        let mut cx = Cx {
            toasts: &mut toasts,
            clipboard: None,
            wide: true,
            focus: &mut focus,
        };
        let value = |g: &GraphPage| g.graph.variable("a").unwrap().value();
        assert!(g.key(&KeyPress::named(Named::Right), &mut cx));
        assert!(value(&g) > v.value());
        assert!(g.key(&KeyPress::named(Named::End), &mut cx));
        assert_eq!(value(&g), v.max());
        assert!(g.key(&KeyPress::named(Named::Home), &mut cx));
        assert_eq!(value(&g), v.min());
        // R9-L-01: an end off the step grid is reached exactly, and the
        // grid never carries the value past it.
        g.graph.set_variable("a", 7.06);
        for change in [Adjust::To(7.06), Adjust::Max, Adjust::Steps(1.0)] {
            assert!(g.adjust_slider(var_slider("a"), change));
            let v = g.graph.variable("a").unwrap();
            assert_eq!((v.value(), v.max()), (7.06, 7.06));
        }
        assert!(g.adjust_slider(var_slider("a"), Adjust::Steps(-1.0)));
        assert_eq!(value(&g), 7.0);
        assert!(g.adjust_slider(var_slider("a"), Adjust::Steps(1.0)));
        assert_eq!(value(&g), 7.06);
        assert!(g.adjust_slider(var_slider("a"), Adjust::Fraction(1.0)));
        assert_eq!(value(&g), 7.06);

        *cx.focus = Some(canvas_id());
        assert!(!g.trace_on);
        assert!(g.key(&KeyPress::named(Named::Right), &mut cx));
        assert!(g.trace_on && g.pointer.is_some());
    }

    /// R9-M-05: repeated arrows keep moving the traced point along a steep
    /// line as along a shallow one, with and without Shift.
    #[test]
    fn keyboard_tracing_follows_steep_curves() {
        use appcore::Named;
        for (src, shift) in [
            ("y=1000*x", false),
            ("y=1000*x", true),
            ("y=1000000000*x", false),
            ("y=1000000000*x", true),
            ("y=1000000000000000*x", false),
            ("y=1000000000000000*x", true),
            ("y=100000000000000000000*x", false),
            ("y=100000000000000000000*x", true),
            ("y=x", false),
        ] {
            let mut g = GraphPage::for_test(session::from_list(src));
            let mut pm = tiny_skia::Pixmap::new(760, 700).unwrap();
            let (mut text, mut icons, input) = (
                crate::text::Text::new(),
                ui::Icons::default(),
                ui::Input::default(),
            );
            let mut scrolls = std::collections::HashMap::new();
            let mut f = Frame::new(
                crate::gfx::Canvas::new(pm.as_mut(), 1.0, false),
                &mut text,
                &mut icons,
                crate::theme::Theme::new(false, None),
                &input,
                &mut scrolls,
                true,
            );
            g.view(&mut f, Rect::new(0.0, 46.0, 760.0, 654.0));
            drop(f);
            let (mut toasts, mut focus) = (Vec::new(), Some(canvas_id()));
            let mut cx = Cx {
                toasts: &mut toasts,
                clipboard: None,
                wide: true,
                focus: &mut focus,
            };
            let up = KeyPress {
                shift,
                ..KeyPress::named(Named::Up)
            };
            let mut ys = Vec::new();
            for _ in 0..21 {
                assert!(g.key(&up, &mut cx));
                let (_, t) = g.trace.expect("still on the curve");
                ys.push(t.y);
            }
            let vp = g.vp.unwrap();
            let px = if shift { 1.0 } else { 5.0 };
            assert!(ys.windows(2).all(|w| w[1] > w[0]), "{src} {shift}: {ys:?}");
            // Up moves the cursor by `px` each time; the traced point keeps
            // within a few pixels of it.
            let travelled = (ys[20] - ys[0]) / vp.y_per_px();
            assert!(
                (travelled - 20.0 * px).abs() < 3.0,
                "{src} {shift}: {travelled}"
            );
        }
    }

    /// PREREVIEW_C Low: ranges typed into the settings read back as typed
    /// (they were rounded to 3 decimals, so [1.0001, 1.0002] collapsed).
    #[test]
    fn typed_ranges_round_trip() {
        let mut g = GraphPage::for_test(session::from_list("y=x"));
        g.vp = Some(graphing::Viewport::default_for_size(760.0, 654.0));
        for (e, t) in g.ranges.iter_mut().zip(["1.0001", "1.0002", "-1", "1"]) {
            e.set_text(t);
        }
        g.apply_ranges();
        assert!(!g.range_error);
        g.fill_ranges();
        let texts: Vec<String> = g.ranges.iter().map(|e| e.text.clone()).collect();
        assert_eq!(texts, ["1.0001", "1.0002", "-1", "1"]);
        g.apply_ranges();
        assert!(!g.range_error);
        let vp = g.vp.unwrap();
        assert_eq!((vp.x_min, vp.x_max), (1.0001, 1.0002));
    }

    /// PREREVIEW_B B-M4: the traced value reaches screen readers with "≈"
    /// read as "approximately" (the bubble itself shows "≈").
    #[test]
    fn traced_values_are_spoken_as_approximately() {
        use appcore::Named;
        let mut g = GraphPage::for_test(session::from_list("y=sin(x)"));
        let mut pm = tiny_skia::Pixmap::new(760, 700).unwrap();
        let (mut text, mut icons, input) = (
            crate::text::Text::new(),
            ui::Icons::default(),
            ui::Input::default(),
        );
        let mut scrolls = std::collections::HashMap::new();
        let frame = |g: &mut GraphPage,
                     pm: &mut tiny_skia::Pixmap,
                     text: &mut crate::text::Text,
                     icons: &mut ui::Icons,
                     scrolls: &mut std::collections::HashMap<_, _>| {
            let mut f = Frame::new(
                crate::gfx::Canvas::new(pm.as_mut(), 1.0, false),
                text,
                icons,
                crate::theme::Theme::new(false, None),
                &input,
                scrolls,
                true,
            );
            g.view(&mut f, Rect::new(0.0, 46.0, 760.0, 654.0));
            f.nodes.take().unwrap()
        };
        frame(&mut g, &mut pm, &mut text, &mut icons, &mut scrolls);
        let (mut toasts, mut focus) = (Vec::new(), Some(canvas_id()));
        let mut cx = Cx {
            toasts: &mut toasts,
            clipboard: None,
            wide: true,
            focus: &mut focus,
        };
        assert!(g.key(&KeyPress::named(Named::Right), &mut cx));
        let (_, t) = g.trace.expect("tracing");
        assert!(t.text().contains('≈'), "{}", t.text());
        let nodes = frame(&mut g, &mut pm, &mut text, &mut icons, &mut scrolls);
        let label = &nodes
            .iter()
            .find(|n| n.id == id("trace"))
            .expect("trace label")
            .label;
        assert!(
            label.contains("approximately ") && !label.contains('≈'),
            "{label}"
        );
    }

    /// Number precision rounds each number typed: 1.0000000000000001 is
    /// 1 + 10⁻¹⁶ Off and 1 at 14 digits.
    #[test]
    fn number_precision_rounds_typed_numbers() {
        // 1.0000000000000001 is 1 + 10⁻¹⁶ as typed, and 1 at 14 digits.
        let mut g = GraphPage::for_test(session::from_list("y=10^16*(1.0000000000000001-1)+x"));
        let id = g.rows[0].id;
        g.set_number_precision(session::NumberPrecision::OFF);
        assert_eq!(g.graph.evaluate(id, 0.0), Some(1.0));
        g.set_number_precision(session::NumberPrecision::DEFAULT);
        assert_eq!(g.graph.evaluate(id, 0.0), Some(0.0));
    }

    /// The accessibility nodes of one frame of the page.
    fn frame_nodes(g: &mut GraphPage) -> Vec<ui::Node> {
        let mut pm = tiny_skia::Pixmap::new(760, 700).unwrap();
        let (mut text, mut icons, input) = (
            crate::text::Text::new(),
            ui::Icons::default(),
            ui::Input::default(),
        );
        let mut scrolls = std::collections::HashMap::new();
        let mut f = Frame::new(
            crate::gfx::Canvas::new(pm.as_mut(), 1.0, false),
            &mut text,
            &mut icons,
            crate::theme::Theme::new(false, None),
            &input,
            &mut scrolls,
            true,
        );
        g.view(&mut f, Rect::new(0.0, 46.0, 760.0, 654.0));
        f.nodes.take().unwrap()
    }

    /// R14-M-01: Number precision Off → 14 → Off, with an equation that
    /// 14 digits make invalid for a while (its constant divides by zero),
    /// keeps a = 0.3 in the graph, in its field and in its slider's
    /// accessible value; at 14 the error is shown and a isn't.
    #[test]
    fn a_precision_change_keeps_variables_and_their_controls() {
        let mut g = GraphPage::for_test(session::from_list("a*x+10^(-16)/(1.0000000000000001-1)"));
        let id = g.rows[0].id;
        g.set_number_precision(session::NumberPrecision::OFF);
        let (mut toasts, mut focus) = (Vec::new(), None);
        let mut cx = Cx {
            toasts: &mut toasts,
            clipboard: None,
            wide: true,
            focus: &mut focus,
        };
        g.field(var_field("a")).expect("a's field").set_text("0.3");
        g.field_changed(var_field("a"), &mut cx);
        let want = *g.graph.variable("a").unwrap();
        assert_eq!(want.value(), 0.3);
        let slider = |nodes: &[ui::Node]| {
            nodes
                .iter()
                .find(|n| n.id == var_slider("a"))
                .and_then(|n| n.numeric)
        };
        assert_eq!(slider(&frame_nodes(&mut g)).unwrap()[0], 0.3);

        g.set_number_precision(session::NumberPrecision::DEFAULT);
        assert_eq!(
            g.graph.error(id).map(|e| e.message()),
            Some("Cannot divide by zero")
        );
        assert!(!g.vars.contains_key("a"), "a isn't listed while not drawn");
        let nodes = frame_nodes(&mut g);
        assert_eq!(slider(&nodes), None);
        assert!(
            nodes.iter().any(|n| n.label == "Cannot divide by zero"),
            "the error is shown"
        );

        g.set_number_precision(session::NumberPrecision::OFF);
        assert!(g.graph.error(id).is_none());
        assert_eq!(g.graph.variable("a"), Some(&want));
        assert_eq!(g.vars["a"].text, "0.3");
        assert_eq!(slider(&frame_nodes(&mut g)).unwrap()[0], 0.3);
        assert!((g.graph.evaluate(id, 2.0).unwrap() - 1.6).abs() < 1e-12);
    }

    /// Hiding an equation unlists its variables; shown again, they are as
    /// they were.
    #[test]
    fn hiding_an_equation_keeps_its_variables() {
        let mut g = GraphPage::for_test(session::from_list("y=a*x"));
        let id = g.rows[0].id;
        g.graph.set_variable("a", 0.3);
        g.sync_vars();
        let (mut toasts, mut focus) = (Vec::new(), None);
        let mut cx = Cx {
            toasts: &mut toasts,
            clipboard: None,
            wide: true,
            focus: &mut focus,
        };
        g.update(Msg::Toggle(id), &mut cx);
        assert!(g.vars.is_empty());
        g.update(Msg::Toggle(id), &mut cx);
        assert_eq!(g.graph.variable("a").unwrap().value(), 0.3);
        assert_eq!(g.vars["a"].text, "0.3");
    }

    #[test]
    fn holes_are_open_circles() {
        let mut g = GraphPage::for_test(session::from_list("y=(x^2-1)/(x-1)"));
        let mut pm = tiny_skia::Pixmap::new(760, 700).unwrap();
        let (mut text, mut icons, input) = (
            crate::text::Text::new(),
            ui::Icons::default(),
            ui::Input::default(),
        );
        let mut scrolls = std::collections::HashMap::new();
        let mut f = Frame::new(
            crate::gfx::Canvas::new(pm.as_mut(), 1.0, false),
            &mut text,
            &mut icons,
            crate::theme::Theme::new(false, None),
            &input,
            &mut scrolls,
            true,
        );
        g.view(&mut f, Rect::new(0.0, 46.0, 760.0, 654.0));
        drop(f);
        let holes = &g.plots[0].plot.holes;
        assert_eq!(holes.len(), 1, "{holes:?}");
        let (vp, c) = (g.vp.unwrap(), g.canvas);
        let at = |x: f64, y: f64| {
            let (sx, sy) = vp.to_screen(x, y);
            let p = pm
                .pixel((c.x as f64 + sx) as u32, (c.y as f64 + sy) as u32)
                .unwrap();
            [p.red(), p.green(), p.blue()]
        };
        let centre = at(holes[0].x, holes[0].y);
        let surface = crate::theme::Theme::new(false, None).surface;
        let close = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(p, q)| p.abs_diff(q) <= 8);
        let s = surface.rgb3().map(|v| (v * 255.0).round() as u8);
        assert!(close(centre, s), "{centre:?} vs {s:?}");
        // The curve itself, away from the hole, is stroked.
        let away = at(holes[0].x + 1.0, holes[0].y + 1.0);
        assert!(!close(away, s), "{away:?}");
    }

    /// R4-M-04: function analysis results (labels only, nothing focusable)
    /// are a keyboard scroll target once they overflow their panel.
    #[test]
    fn analysis_results_are_a_keyboard_scroll_target() {
        let mut g = GraphPage::for_test(session::from_list("y=x^3-2x+1/(x-1)"));
        let eq = g.rows[0].id;
        let (mut toasts, mut focus) = (Vec::new(), None);
        let mut cx = Cx {
            toasts: &mut toasts,
            clipboard: None,
            wide: true,
            focus: &mut focus,
        };
        g.update(Msg::Analyze(eq), &mut cx);
        let features = g.graph.analyze(eq);
        g.analysis_done(g.analysis_seq, features);

        let mut pm = tiny_skia::Pixmap::new(760, 500).unwrap();
        let (mut text, mut icons, input) = (
            crate::text::Text::new(),
            ui::Icons::default(),
            ui::Input::default(),
        );
        let mut scrolls = std::collections::HashMap::new();
        let mut f = Frame::new(
            crate::gfx::Canvas::new(pm.as_mut(), 1.0, false),
            &mut text,
            &mut icons,
            crate::theme::Theme::new(false, None),
            &input,
            &mut scrolls,
            true,
        );
        g.view(&mut f, Rect::new(0.0, 46.0, 760.0, 454.0));
        let sid = id("an-scroll");
        let hit = f.hits.iter().find(|h| h.id == sid).expect("panel drawn");
        assert!(hit.focusable, "overflowing results can't take focus");
        let node = f
            .nodes
            .as_ref()
            .unwrap()
            .iter()
            .find(|n| n.id == sid)
            .unwrap();
        assert!(node.focusable && node.scrollable);

        // R8-M-05: the results reach Linux assistive technology, as labels
        // inside each card, monotonicity rows included.
        let out = crate::a11y::exported(f.nodes.as_ref().unwrap());
        drop(f);
        assert!(scrolls[&sid].max() > 0.0);
        let labels: Vec<&str> = out
            .iter()
            .filter(|(r, ..)| *r == accesskit::Role::Label)
            .map(|(_, n, _)| n.as_str())
            .collect();
        assert!(labels.iter().any(|l| l.contains("x = 1")), "{labels:?}");
        assert!(
            labels.iter().any(|l| l.contains("Increasing")),
            "{labels:?}"
        );
    }

    /// A row too long to plot inline with no plot before it to go by,
    /// on a page that holds its worker jobs to report them back by hand.
    fn heavy_page() -> (GraphPage, Viewport) {
        let terms: Vec<String> = (1..=30).map(|k| format!("sin({k}x)")).collect();
        let mut g = GraphPage::for_test(session::from_list(&format!("y={}", terms.join("+"))));
        g.held_jobs = Some(Vec::new());
        let vp = Viewport::default_for_size(760.0, 700.0);
        g.vp = Some(vp);
        (g, vp)
    }

    fn geometry(plots: &[EquationPlot]) -> Vec<(EquationId, &graphing::Plot)> {
        plots.iter().map(|p| (p.id, &p.plot)).collect()
    }

    /// R12-M-06: a heavy plot goes to a worker; a quick edit while it runs
    /// is plotted inline and cancels it; the worker's report, arriving
    /// after (it finished before it saw the flag), is dropped: it plotted
    /// the old row.
    #[test]
    fn a_stale_worker_never_replaces_a_newer_inline_plot() {
        let (mut g, vp) = heavy_page();
        g.replot();
        let job = g.held_jobs.as_mut().unwrap().pop();
        let job = job.expect("a heavy first plot is a worker's");
        assert!(g.plots.is_empty());

        let (mut toasts, mut focus) = (Vec::new(), None);
        let mut cx = Cx {
            toasts: &mut toasts,
            clipboard: None,
            wide: true,
            focus: &mut focus,
        };
        let field = eq_field(g.rows[0].id);
        g.field(field).unwrap().set_text("y=x");
        g.field_changed(field, &mut cx);
        g.replot();
        assert!(g.held_jobs.as_ref().unwrap().is_empty(), "plotted inline");
        assert!(
            job.cancel.load(Ordering::Relaxed),
            "the old job is cancelled"
        );
        let new = g.graph.plot_parallel(&vp);
        assert_eq!(geometry(&g.plots), geometry(&new));

        let stale = job
            .graph
            .plot_parallel_cancellable(&job.vp, &AtomicBool::new(false))
            .expect("not cancelled");
        assert_ne!(geometry(&stale), geometry(&new));
        g.plot_done(job.seq, Some(stale), 900.0, &mut cx);
        assert_eq!(geometry(&g.plots), geometry(&new));
        assert!(!g.dirty, "nothing waits");
        // The prediction is still the inline plot's.
        g.dirty = true;
        g.replot();
        assert!(g.held_jobs.as_ref().unwrap().is_empty());
    }

    /// A request made while a job runs cancels it and waits; the job's
    /// report (stale) is dropped and asks for the request again, which
    /// then runs; a report from no running job changes nothing.
    #[test]
    fn a_request_during_a_job_is_made_again_after_it() {
        let (mut g, vp) = heavy_page();
        let (mut toasts, mut focus) = (Vec::new(), None);
        let mut cx = Cx {
            toasts: &mut toasts,
            clipboard: None,
            wide: true,
            focus: &mut focus,
        };
        g.replot();
        let first = g.held_jobs.as_mut().unwrap().pop().unwrap();
        g.dirty = true;
        g.replot();
        assert!(g.held_jobs.as_ref().unwrap().is_empty(), "waits");
        assert!(first.cancel.load(Ordering::Relaxed));
        let plots = g.graph.plot_parallel(&vp);
        g.plot_done(first.seq, Some(plots.clone()), 900.0, &mut cx);
        assert!(g.plots.is_empty(), "a newer request came after it");
        assert!(g.dirty, "the waiting request is made again");

        g.replot();
        let second = g.held_jobs.as_mut().unwrap().pop().unwrap();
        assert!(second.seq > first.seq && !second.cancel.load(Ordering::Relaxed));
        g.plot_done(second.seq, Some(plots.clone()), 900.0, &mut cx);
        assert_eq!(geometry(&g.plots), geometry(&plots));
        g.plot_done(second.seq, Some(Vec::new()), 1.0, &mut cx);
        g.plot_done(first.seq, Some(Vec::new()), 1.0, &mut cx);
        assert_eq!(geometry(&g.plots), geometry(&plots));
        assert!(!g.dirty);
    }
}
