//! Graphing calculator (upstream GraphingCalculator + EquationInputArea +
//! KeyGraphFeaturesPanel + GraphingSettings + GraphingNumPad).

use std::collections::BTreeMap;
use std::time::Instant;

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

pub struct GraphPage {
    graph: Graph,
    rows: Vec<Row>,
    next_color: usize,
    vp: Option<Viewport>,
    plots: Vec<EquationPlot>,
    plot_ms: f64,
    dirty: bool,
    busy: bool,
    again: bool,
    seq: u64,
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

fn canvas_id() -> ui::Id {
    id("graph-canvas")
}

impl GraphPage {
    pub fn new(saved: Vec<SavedEquation>) -> GraphPage {
        let mut page = GraphPage {
            graph: Graph::new(),
            rows: Vec::new(),
            next_color: session::next_color(&saved),
            vp: None,
            plots: Vec::new(),
            plot_ms: 0.0,
            dirty: true,
            busy: false,
            again: false,
            seq: 0,
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
            proxy: None,
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
            for (e, v) in self
                .ranges
                .iter_mut()
                .zip([vp.x_min, vp.x_max, vp.y_min, vp.y_max])
            {
                e.set_text(&format_value(v));
            }
        }
        self.range_error = false;
    }

    // ------------------------------------------------------------ messages

    pub fn update(&mut self, m: Msg, cx: &mut Cx) {
        self.proxy = Some(cx.proxy.clone());
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
            }
            Msg::Toggle(id) => {
                let on = !self.graph.is_line_enabled(id);
                self.graph.set_line_enabled(id, on);
                self.dirty = true;
            }
            Msg::Analyze(id) => {
                self.seq += 1;
                let seq = self.seq;
                let title = self.graph.text(id).unwrap_or_default().to_string();
                self.side = Side::Analysis {
                    seq,
                    title,
                    result: None,
                };
                self.show_graph = false;
                let graph = self.graph.clone();
                let proxy = cx.proxy.clone();
                // Analysis can take a moment for complicated expressions.
                let _ = std::thread::Builder::new()
                    .name("analysis".into())
                    .spawn(move || {
                        let _ =
                            proxy.send_event(UserEvent::Analysis(seq, Box::new(graph.analyze(id))));
                    });
            }
            Msg::Back => self.side = Side::Equations,
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
                self.dirty = true;
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

    pub fn key(&mut self, kp: &KeyPress, _cx: &mut Cx) -> bool {
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
        let name = self.vars.keys().find(|k| var_slider(k) == hid).cloned();
        if let Some(name) = name
            && let Some(v) = self.graph.variable(&name).copied()
        {
            let track = rect.inset_xy(8.0, 0.0);
            let frac = ((x - track.x) / track.w.max(1.0)).clamp(0.0, 1.0) as f64;
            let mut value = v.min() + frac * (v.max() - v.min());
            let step = v.step();
            if step > 0.0 {
                value = (value / step).round() * step;
            }
            self.graph.set_variable(&name, value);
            if let Some(e) = self.vars.get_mut(&name) {
                e.set_text(&format_value(value));
            }
            self.dirty = true;
        }
    }

    pub fn wheel(&mut self, x: f32, y: f32, dy: f32, hits: &[Hit]) -> bool {
        let top = hits
            .iter()
            .rev()
            .find(|h| h.rect.contains(x, y) && h.sense != Sense::Scroll);
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

    pub fn analysis_done(&mut self, seq: u64, features: KeyGraphFeatures) {
        if let Side::Analysis { seq: s, result, .. } = &mut self.side
            && *s == seq
        {
            *result = Some(Box::new(features));
        }
    }

    pub fn plot_done(&mut self, _seq: u64, plots: Vec<EquationPlot>, ms: f64, _cx: &mut Cx) {
        self.busy = false;
        self.plot_ms = ms;
        self.plots = plots;
        if self.again {
            self.again = false;
            self.dirty = true;
        }
        self.update_trace();
    }

    fn replot(&mut self) {
        let Some(vp) = self.vp else { return };
        if !self.dirty {
            return;
        }
        if self.plot_ms < INLINE_PLOT_MS {
            self.dirty = false;
            let t = Instant::now();
            self.plots = self.graph.plot_parallel(&vp);
            self.plot_ms = t.elapsed().as_secs_f64() * 1e3;
            self.update_trace();
            return;
        }
        // Heavy graph: plot on a worker and keep drawing the last result
        // (curves are in graph coordinates, so they still line up while
        // panning). Requests made meanwhile coalesce into one re-plot.
        let Some(proxy) = self.proxy.clone() else {
            self.plot_ms = 0.0;
            return;
        };
        self.dirty = false;
        if self.busy {
            self.again = true;
            return;
        }
        self.busy = true;
        self.seq += 1;
        let (seq, graph) = (self.seq, self.graph.clone());
        let _ = std::thread::Builder::new()
            .name("plot".into())
            .spawn(move || {
                let t = Instant::now();
                let plots = graph.plot_parallel(&vp);
                let _ =
                    proxy.send_event(UserEvent::Plot(seq, plots, t.elapsed().as_secs_f64() * 1e3));
            });
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
        let off = f.scroll_begin(sid, r);
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
                f.label_fit(
                    Rect::new(r.x + 36.0, y - 2.0, r.w - 40.0, 18.0),
                    e.message(),
                    CAPTION,
                    9.0,
                    t.danger,
                    Align::Start,
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
        let off = f.scroll_begin(sid, body);
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
                        y += 8.0;
                        let card = Rect::new(body.x + 4.0, top, body.w - 8.0, y - top);
                        f.cv.rounded_border(card, 8.0, t.border, 1.0);
                        if let Some(n) =
                            f.node(id(("kgf", k)), accesskit::Role::Group, &item.title, card)
                        {
                            n.value = Some(item.display_items.join(", "));
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
                f.key(
                    id(("gpad", ri, ci)),
                    cell,
                    label,
                    icon,
                    look,
                    appcore::keys::graph_pad_name(label),
                    msg(Msg::Pad(insert)),
                    true,
                );
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
            let text = graphing::trace::format_trace_value(tp.x, tp.y, vp.precision());
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
            if let Some(n) = f.node(id("trace"), accesskit::Role::Label, &text, bubble) {
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

fn format_value(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { "0".into() } else { format!("{r}") }
}
