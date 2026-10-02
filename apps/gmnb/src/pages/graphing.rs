//! Graphing calculator (upstream GraphingCalculator.xaml + EquationInputArea +
//! KeyGraphFeaturesPanel + GraphingSettings + GraphingNumPad).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use appcore::KeyPress;
use appcore::graph::{self as session, SavedEquation};
use appcore::input::{self, GraphAction};
use appcore::keys::GRAPH_PAD;
use appcore::modes::ViewMode;
use graphing::{EquationId, Graph, TrigUnit};
use gtk::{gdk, gio, glib};

use super::{Ctx, Page};
use crate::widgets::graph_view::GraphView;
use crate::widgets::icon::{PathIcon, icon_toggle, paths};
use crate::widgets::width_bin::WidthBin;

const WIDE_PX: i32 = 760;

struct Row {
    id: EquationId,
    root: gtk::Box,
    entry: gtk::Entry,
    swatch: gtk::Button,
    error: gtk::Label,
    color: Cell<usize>,
}

pub struct GraphingPage {
    ctx: Rc<Ctx>,
    graph: Rc<RefCell<Graph>>,
    root: WidthBin,
    split: gtk::Box,
    side: gtk::Box,
    view_stack: gtk::Stack,
    graph_view: GraphView,
    rows: RefCell<Vec<Rc<Row>>>,
    list: gtk::Box,
    vars_box: gtk::Box,
    vars_header: gtk::Label,
    analysis_title: gtk::Label,
    analysis_body: gtk::Box,
    side_stack: gtk::Stack,
    focused: RefCell<Option<glib::WeakRef<gtk::Entry>>>,
    mode_toggle: adw::ToggleGroup,
    next_color: Cell<usize>,
    wide: Cell<bool>,
    building: Cell<bool>,
    /// Bumped per analysis request; stale background results are dropped.
    analysis_seq: Cell<u64>,
}

fn small_button(icon: &str, tip: &str) -> gtk::Button {
    let b = gtk::Button::builder()
        .child(&PathIcon::new(icon, 16))
        .tooltip_text(tip)
        .css_classes(["flat", "wc-icon-button", "wc-small"])
        .build();
    b.update_property(&[gtk::accessible::Property::Label(tip)]);
    b
}

impl GraphingPage {
    pub fn new(ctx: Rc<Ctx>) -> Rc<Self> {
        let graph = Rc::new(RefCell::new(Graph::new()));
        let graph_view = GraphView::default();
        graph_view.set_graph(graph.clone());
        graph_view.set_hexpand(true);
        graph_view.set_vexpand(true);

        // Graph toolbar overlay.
        let zoom_in = small_button(paths::ZOOM_IN, "Zoom in (Ctrl+Plus)");
        let zoom_out = small_button(paths::ZOOM_OUT, "Zoom out (Ctrl+Minus)");
        let reset = small_button(paths::ZOOM_FIT, "Reset view (Ctrl+0)");
        let trace_btn = icon_toggle(paths::TRACE, "Trace");
        trace_btn.add_css_class("wc-small");
        let copy = small_button(paths::COPY, "Copy graph image");
        let settings = gtk::MenuButton::builder()
            .child(&PathIcon::new(paths::SLIDERS, 16))
            .tooltip_text("Graph options")
            .css_classes(["flat", "wc-icon-button", "wc-small"])
            .build();
        let tools = gtk::Box::new(gtk::Orientation::Vertical, 2);
        tools.add_css_class("wc-graph-tools");
        for w in [
            zoom_in.upcast_ref::<gtk::Widget>(),
            zoom_out.upcast_ref(),
            reset.upcast_ref(),
            trace_btn.upcast_ref(),
            copy.upcast_ref(),
            settings.upcast_ref(),
        ] {
            tools.append(w);
        }
        tools.set_halign(gtk::Align::End);
        tools.set_valign(gtk::Align::Start);
        tools.set_margin_top(10);
        tools.set_margin_end(10);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&graph_view));
        overlay.add_overlay(&tools);

        // Equations list.
        let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let add = gtk::Button::builder().css_classes(["wc-add-eq"]).build();
        let add_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        add_box.append(&PathIcon::new(paths::PLUS, 16));
        add_box.append(&gtk::Label::new(Some("Enter an expression")));
        add.set_child(Some(&add_box));
        let vars_header = gtk::Label::new(Some("Variables"));
        vars_header.add_css_class("wc-section");
        vars_header.set_xalign(0.0);
        vars_header.set_visible(false);
        let vars_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let eq_col = gtk::Box::new(gtk::Orientation::Vertical, 8);
        eq_col.set_margin_start(4);
        eq_col.set_margin_end(4);
        eq_col.append(&list);
        eq_col.append(&add);
        eq_col.append(&vars_header);
        eq_col.append(&vars_box);
        let eq_scroll = gtk::ScrolledWindow::builder()
            .child(&eq_col)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();

        // Function analysis view.
        let back = gtk::Button::builder()
            .css_classes(["flat", "wc-back"])
            .halign(gtk::Align::Start)
            .build();
        let bb = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        bb.append(&PathIcon::new(paths::CHEVRON_LEFT, 16));
        bb.append(&gtk::Label::new(Some("Function analysis")));
        back.set_child(Some(&bb));
        let analysis_title = gtk::Label::new(None);
        analysis_title.add_css_class("wc-analysis-title");
        analysis_title.set_xalign(0.0);
        analysis_title.set_wrap(true);
        let analysis_body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let an_col = gtk::Box::new(gtk::Orientation::Vertical, 8);
        an_col.append(&back);
        an_col.append(&analysis_title);
        an_col.append(&analysis_body);
        let an_scroll = gtk::ScrolledWindow::builder()
            .child(&an_col)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();

        let side_stack = gtk::Stack::new();
        side_stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
        side_stack.set_transition_duration(240);
        side_stack.add_named(&eq_scroll, Some("equations"));
        side_stack.add_named(&an_scroll, Some("analysis"));

        // Keypad.
        let pad = gtk::Grid::builder()
            .row_spacing(4)
            .column_spacing(4)
            .column_homogeneous(true)
            .row_homogeneous(true)
            .build();
        pad.add_css_class("wc-graph-pad");

        let side = gtk::Box::new(gtk::Orientation::Vertical, 8);
        side.add_css_class("wc-glass-panel");
        side.add_css_class("wc-graph-side");
        side.append(&side_stack);
        side.append(&pad);

        let view_stack = gtk::Stack::new();
        view_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        view_stack.set_transition_duration(200);
        view_stack.set_vexpand(true);

        let split = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        split.set_margin_start(10);
        split.set_margin_end(10);
        split.set_margin_bottom(10);
        split.append(&view_stack);

        let mode_toggle = adw::ToggleGroup::new();
        mode_toggle.add(
            adw::Toggle::builder()
                .name("equations")
                .child(&PathIcon::new(paths::FUNCTION, 16))
                .tooltip("Equations")
                .build(),
        );
        mode_toggle.add(
            adw::Toggle::builder()
                .name("graph")
                .child(&PathIcon::new(paths::GRAPHING, 16))
                .tooltip("Graph (Ctrl+Home)")
                .build(),
        );
        mode_toggle.set_active_name(Some("equations"));
        mode_toggle.add_css_class("wc-toggle-group");

        let root = WidthBin::new(&split);
        let page = Rc::new(GraphingPage {
            ctx: ctx.clone(),
            graph: graph.clone(),
            root: root.clone(),
            split: split.clone(),
            side: side.clone(),
            view_stack: view_stack.clone(),
            graph_view: graph_view.clone(),
            rows: RefCell::default(),
            list,
            vars_box,
            vars_header,
            analysis_title,
            analysis_body,
            side_stack: side_stack.clone(),
            focused: RefCell::new(None),
            mode_toggle: mode_toggle.clone(),
            next_color: Cell::new(0),
            wide: Cell::new(false),
            building: Cell::new(false),
            analysis_seq: Cell::new(0),
        });
        view_stack.add_named(&side, Some("equations"));
        view_stack.add_named(&overlay, Some("graph"));
        page.build_pad(&pad);
        settings.set_popover(Some(&page.settings_popover()));

        {
            let weak = Rc::downgrade(&page);
            ctx.hub.subscribe(move |s| {
                if let Some(p) = weak.upgrade() {
                    p.graph_view.set_scheme(*s);
                    for r in p.rows.borrow().iter() {
                        p.paint_swatch(r);
                        p.graph_view
                            .set_color(r.id, s.series[r.color.get() % s.series.len()]);
                    }
                }
            });
        }
        {
            let gv = graph_view.clone();
            zoom_in.connect_clicked(move |_| gv.zoom_in());
            let gv = graph_view.clone();
            zoom_out.connect_clicked(move |_| gv.zoom_out());
            let gv = graph_view.clone();
            reset.connect_clicked(move |_| gv.reset_view());
            let gv = graph_view.clone();
            trace_btn.connect_toggled(move |b| gv.set_trace(b.is_active()));
            let weak = Rc::downgrade(&page);
            copy.connect_clicked(move |_| {
                if let Some(p) = weak.upgrade()
                    && let (Some(tex), Some(display)) =
                        (p.graph_view.to_texture(), gdk::Display::default())
                {
                    display.clipboard().set_texture(&tex);
                    p.ctx.toast("Graph copied to clipboard");
                }
            });
        }
        {
            let weak = Rc::downgrade(&page);
            add.connect_clicked(move |_| {
                if let Some(p) = weak.upgrade()
                    && let Some(row) = p.add_equation("")
                {
                    row.entry.grab_focus();
                }
            });
            // Weak: the button lives inside the stack it switches.
            let ss = side_stack.downgrade();
            back.connect_clicked(move |_| {
                if let Some(ss) = ss.upgrade() {
                    ss.set_visible_child_name("equations");
                }
            });
        }
        {
            let vs = view_stack.clone();
            mode_toggle.connect_active_name_notify(move |t| {
                if let Some(name) = t.active_name() {
                    vs.set_visible_child_name(&name);
                }
            });
        }
        {
            let weak = Rc::downgrade(&page);
            root.connect_width(move |w| {
                if let Some(p) = weak.upgrade() {
                    p.set_wide(w >= WIDE_PX);
                }
            });
        }

        page.restore();
        page
    }

    fn set_wide(&self, wide: bool) {
        if self.wide.replace(wide) == wide && self.side.parent().is_some() {
            return;
        }
        // Wide: equations panel and graph side by side. Narrow: a toggle.
        if let Some(parent) = self.side.parent() {
            if parent == *self.view_stack.upcast_ref::<gtk::Widget>() {
                self.view_stack.remove(&self.side);
            } else if let Some(b) = parent.downcast_ref::<gtk::Box>() {
                b.remove(&self.side);
            }
        }
        if wide {
            self.side.set_width_request(340);
            self.side.set_hexpand(false);
            self.split.prepend(&self.side);
            self.view_stack.set_visible_child_name("graph");
            self.mode_toggle.set_visible(false);
        } else {
            self.side.set_width_request(-1);
            self.view_stack.add_named(&self.side, Some("equations"));
            let name = self
                .mode_toggle
                .active_name()
                .unwrap_or_else(|| "equations".into());
            self.view_stack.set_visible_child_name(&name);
            self.mode_toggle.set_visible(true);
        }
    }

    fn build_pad(self: &Rc<Self>, pad: &gtk::Grid) {
        for (r, row) in GRAPH_PAD.iter().enumerate() {
            for (c, (label, insert)) in row.iter().enumerate() {
                let b = gtk::Button::with_label(label);
                b.update_property(&[gtk::accessible::Property::Label(
                    appcore::keys::graph_pad_name(label),
                )]);
                b.add_css_class("wc-key");
                b.add_css_class(
                    if label.chars().all(|ch| ch.is_ascii_digit() || ch == '.') {
                        "wc-num"
                    } else {
                        "wc-fn"
                    },
                );
                b.add_css_class("wc-pad-key");
                b.set_focus_on_click(false);
                let weak = Rc::downgrade(self);
                let insert = insert.to_string();
                b.connect_clicked(move |b| {
                    if let Some(p) = weak.upgrade() {
                        p.insert_text(&insert);
                        let s = p.ctx.hub.scheme();
                        p.ctx.pulse_at(
                            b,
                            b.width() as f32 / 2.0,
                            b.height() as f32 / 2.0,
                            s.accent,
                            0.4,
                        );
                    }
                });
                pad.attach(&b, c as i32, r as i32, 1, 1);
            }
        }
    }

    /// Type into the focused equation (or a new one) at its cursor.
    fn insert_text(self: &Rc<Self>, text: &str) {
        let focused = self.focused.borrow().as_ref().and_then(|w| w.upgrade());
        let entry = match focused {
            Some(e) if e.is_mapped() => e,
            _ => match self.rows.borrow().last().map(|r| r.entry.clone()) {
                Some(e) => e,
                None => match self.add_equation("") {
                    Some(r) => r.entry.clone(),
                    None => return,
                },
            },
        };
        let mut pos = entry.position();
        if text == "\u{8}" {
            if pos > 0 {
                entry.delete_text(pos - 1, pos);
            }
        } else {
            entry.insert_text(text, &mut pos);
            entry.set_position(pos);
        }
        entry.grab_focus_without_selecting();
    }

    fn paint_swatch(&self, row: &Row) {
        let s = self.ctx.hub.scheme();
        let c = s.series[row.color.get() % s.series.len()];
        let css = format!(
            "rgb({},{},{})",
            (c[0] * 255.0) as u8,
            (c[1] * 255.0) as u8,
            (c[2] * 255.0) as u8
        );
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&format!(
            "button {{ background: {css}; box-shadow: 0 0 12px -2px {css}; }}"
        ));
        #[allow(deprecated)]
        row.swatch
            .style_context()
            .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_USER + 5);
    }

    fn add_equation(self: &Rc<Self>, text: &str) -> Option<Rc<Row>> {
        if self.graph.borrow().len() >= session::MAX_EQUATIONS {
            self.ctx.toast("You can graph up to 14 equations");
            return None;
        }
        let text = session::clamp_text(text);
        let id = self.graph.borrow_mut().add_equation(text);
        let color = self.next_color.get();
        self.next_color.set(color + 1);

        let swatch = gtk::Button::builder()
            .css_classes(["wc-swatch"])
            .tooltip_text("Show or hide")
            .valign(gtk::Align::Center)
            .build();
        let entry = gtk::Entry::builder()
            // Plenty for any real equation; also bounds parse/compile work.
            .max_length(session::MAX_EQUATION_CHARS as i32)
            .text(text)
            .placeholder_text("Enter an expression")
            .hexpand(true)
            .css_classes(["wc-eq-entry"])
            .build();
        entry.update_property(&[gtk::accessible::Property::Label("Equation")]);
        let analyze = small_button(paths::FUNCTION, "Analyze function");
        let remove = small_button(paths::CLOSE, "Remove equation");
        let style = gtk::MenuButton::builder()
            .child(&PathIcon::new(paths::CHEVRON_DOWN, 14))
            .tooltip_text("Line color and style")
            .css_classes(["flat", "wc-icon-button", "wc-small"])
            .valign(gtk::Align::Center)
            .build();
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        line.append(&swatch);
        line.append(&entry);
        line.append(&style);
        line.append(&analyze);
        line.append(&remove);
        let error = gtk::Label::new(None);
        error.add_css_class("wc-eq-error");
        error.set_xalign(0.0);
        error.set_wrap(true);
        error.set_visible(false);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 2);
        root.add_css_class("wc-eq-row");
        root.append(&line);
        root.append(&error);
        let row = Rc::new(Row {
            id,
            root: root.clone(),
            entry: entry.clone(),
            swatch: swatch.clone(),
            error,
            color: Cell::new(color),
        });
        style.set_popover(Some(&self.style_popover(&row)));
        self.paint_swatch(&row);
        let s = self.ctx.hub.scheme();
        self.graph_view
            .set_color(id, s.series[color % s.series.len()]);
        let revealer = gtk::Revealer::builder()
            .child(&root)
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(260)
            .build();
        self.list.append(&revealer);
        glib::idle_add_local_once({
            let r = revealer.clone();
            move || r.set_reveal_child(true)
        });
        self.rows.borrow_mut().push(row.clone());

        let weak = Rc::downgrade(self);
        entry.connect_changed(move |e| {
            if let Some(p) = weak.upgrade()
                && !p.building.get()
            {
                p.equation_changed(id, &e.text());
            }
        });
        let weak = Rc::downgrade(self);
        let focus = gtk::EventControllerFocus::new();
        // Use the controller's own widget: capturing `entry` here would make
        // the entry own a closure that owns the entry (a reference cycle).
        focus.connect_enter(move |c| {
            if let (Some(p), Some(e)) = (weak.upgrade(), c.widget().and_downcast::<gtk::Entry>()) {
                p.focused.replace(Some(e.downgrade()));
            }
        });
        entry.add_controller(focus);
        let weak = Rc::downgrade(self);
        entry.connect_activate(move |_| {
            if let Some(p) = weak.upgrade() {
                // Enter plots and moves on to a fresh expression, like upstream.
                p.graph_view.animate_draw(id);
                let last = p.rows.borrow().last().map(|r| r.id) == Some(id);
                if last && let Some(r) = p.add_equation("") {
                    r.entry.grab_focus();
                }
            }
        });
        let weak = Rc::downgrade(self);
        swatch.connect_clicked(move |b| {
            if let Some(p) = weak.upgrade() {
                let on = !p.graph.borrow().is_line_enabled(id);
                p.graph.borrow_mut().set_line_enabled(id, on);
                b.set_opacity(if on { 1.0 } else { 0.35 });
                p.graph_view.invalidate();
                if on {
                    p.graph_view.animate_draw(id);
                }
            }
        });
        let weak = Rc::downgrade(self);
        analyze.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                p.show_analysis(id);
            }
        });
        let weak = Rc::downgrade(self);
        // Weak: the revealer contains this button, so a strong capture cycles.
        let rev = revealer.downgrade();
        remove.connect_clicked(move |_| {
            let (Some(p), Some(rev)) = (weak.upgrade(), rev.upgrade()) else {
                return;
            };
            p.graph.borrow_mut().remove_equation(id);
            p.rows.borrow_mut().retain(|r| r.id != id);
            rev.set_reveal_child(false);
            let (list, rev2) = (p.list.clone(), rev.clone());
            glib::timeout_add_local_once(std::time::Duration::from_millis(280), move || {
                list.remove(&rev2)
            });
            p.graph_view.invalidate();
            p.sync_variables();
        });
        if !text.is_empty() {
            self.equation_changed(id, text);
        }
        Some(row)
    }

    /// Upstream EquationStylePanelControl: colour + line style.
    fn style_popover(self: &Rc<Self>, row: &Rc<Row>) -> gtk::Popover {
        let colors = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let n = self.ctx.hub.scheme().series.len();
        for i in 0..n {
            let b = gtk::Button::builder()
                .css_classes(["wc-swatch", "wc-swatch-pick"])
                .tooltip_text(format!("Color {}", i + 1))
                .build();
            let probe = Row {
                id: row.id,
                root: row.root.clone(),
                entry: row.entry.clone(),
                swatch: b.clone(),
                error: row.error.clone(),
                color: Cell::new(i),
            };
            self.paint_swatch(&probe);
            // Weak row: the row's widgets own this popover and its buttons.
            let (weak, r) = (Rc::downgrade(self), Rc::downgrade(row));
            b.connect_clicked(move |_| {
                if let (Some(p), Some(r)) = (weak.upgrade(), r.upgrade()) {
                    r.color.set(i);
                    p.paint_swatch(&r);
                    let s = p.ctx.hub.scheme();
                    p.graph_view.set_color(r.id, s.series[i % s.series.len()]);
                }
            });
            colors.append(&b);
        }
        let styles = adw::ToggleGroup::new();
        for (_, name, label) in session::STYLES {
            styles.add(adw::Toggle::builder().name(name).label(label).build());
        }
        styles.set_active_name(Some(session::style_key(
            self.graph.borrow().line_style(row.id),
        )));
        let (weak, id) = (Rc::downgrade(self), row.id);
        styles.connect_active_name_notify(move |t| {
            if let Some(p) = weak.upgrade() {
                let style = t
                    .active_name()
                    .and_then(|n| session::style_from_key(&n))
                    .unwrap_or_default();
                p.graph.borrow_mut().set_line_style(id, style);
                p.graph_view.invalidate();
            }
        });
        let col = gtk::Box::new(gtk::Orientation::Vertical, 10);
        col.set_margin_top(8);
        col.set_margin_bottom(8);
        col.set_margin_start(8);
        col.set_margin_end(8);
        let h = |t: &str| {
            let l = gtk::Label::new(Some(t));
            l.add_css_class("wc-section");
            l.set_xalign(0.0);
            l
        };
        col.append(&h("Color"));
        col.append(&colors);
        col.append(&h("Line style"));
        col.append(&styles);
        gtk::Popover::builder()
            .child(&col)
            .css_classes(["wc-flyout"])
            .build()
    }

    fn equation_changed(self: &Rc<Self>, id: EquationId, text: &str) {
        let was_empty = self
            .graph
            .borrow()
            .text(id)
            .is_none_or(|t| t.trim().is_empty());
        self.graph.borrow_mut().set_equation_text(id, text);
        let err = self.graph.borrow().error(id).cloned();
        if let Some(row) = self.rows.borrow().iter().find(|r| r.id == id) {
            let show = err.is_some() && !text.trim().is_empty();
            row.error.set_visible(show);
            if let Some(e) = &err {
                row.error.set_text(e.message());
            }
            if show {
                row.root.add_css_class("wc-has-error");
            } else {
                row.root.remove_css_class("wc-has-error");
            }
        }
        self.graph_view.invalidate();
        if was_empty && err.is_none() {
            self.graph_view.animate_draw(id);
        }
        self.sync_variables();
    }

    fn sync_variables(self: &Rc<Self>) {
        while let Some(c) = self.vars_box.first_child() {
            self.vars_box.remove(&c);
        }
        let vars: Vec<(String, graphing::variable::Variable)> = self
            .graph
            .borrow()
            .variables()
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        self.vars_header.set_visible(!vars.is_empty());
        for (name, var) in vars {
            let label = gtk::Label::new(Some(&name));
            label.add_css_class("wc-var-name");
            let scale = gtk::Scale::with_range(
                gtk::Orientation::Horizontal,
                var.min(),
                var.max(),
                var.step().max(1e-9),
            );
            scale.set_value(var.value());
            scale.set_hexpand(true);
            scale.set_draw_value(false);
            scale.update_property(&[gtk::accessible::Property::Label(&format!(
                "Variable {name}"
            ))]);
            let value = gtk::SpinButton::with_range(-1e9, 1e9, var.step().max(1e-9));
            value.set_digits(3);
            value.set_value(var.value());
            value.set_width_chars(6);
            value.add_css_class("wc-spin");
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.add_css_class("wc-var-row");
            row.append(&label);
            row.append(&scale);
            row.append(&value);
            self.vars_box.append(&row);

            let weak = Rc::downgrade(self);
            // The scale and spin button update each other; weak captures so
            // the pair can be freed when the variable list is rebuilt.
            let (n, v2) = (name.clone(), value.downgrade());
            scale.connect_value_changed(move |s| {
                if let (Some(p), Some(v2)) = (weak.upgrade(), v2.upgrade()) {
                    p.graph.borrow_mut().set_variable(&n, s.value());
                    if (v2.value() - s.value()).abs() > 1e-12 {
                        v2.set_value(s.value());
                    }
                    p.graph_view.invalidate();
                }
            });
            let (s2, weak) = (scale.downgrade(), Rc::downgrade(self));
            let n = name.clone();
            value.connect_value_changed(move |v| {
                if let (Some(p), Some(s2)) = (weak.upgrade(), s2.upgrade()) {
                    let x = v.value();
                    // Widen the slider if the typed value is outside it.
                    p.graph.borrow_mut().update_variable(&n, |var| {
                        if x < var.min() {
                            var.set_min(x);
                        }
                        if x > var.max() {
                            var.set_max(x);
                        }
                    });
                    let adj = s2.adjustment();
                    adj.set_lower(adj.lower().min(x));
                    adj.set_upper(adj.upper().max(x));
                    if (s2.value() - x).abs() > 1e-12 {
                        s2.set_value(x);
                    }
                }
            });
        }
    }

    /// Function analysis runs on a worker thread (it can take a while for
    /// complicated expressions); the panel shows a spinner meanwhile.
    fn show_analysis(self: &Rc<Self>, id: EquationId) {
        while let Some(c) = self.analysis_body.first_child() {
            self.analysis_body.remove(&c);
        }
        let text = self.graph.borrow().text(id).unwrap_or_default().to_string();
        self.analysis_title.set_text(&text);
        let busy = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        busy.append(&adw::Spinner::new());
        let label = gtk::Label::new(Some("Analyzing…"));
        label.add_css_class("wc-empty");
        busy.append(&label);
        self.analysis_body.append(&busy);
        self.side_stack.set_visible_child_name("analysis");
        if !self.wide.get() {
            self.mode_toggle.set_active_name(Some("equations"));
        }
        let seq = self.analysis_seq.get() + 1;
        self.analysis_seq.set(seq);
        let graph = self.graph.borrow().clone();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let features = gio::spawn_blocking(move || graph.analyze(id)).await;
            let Some(p) = weak.upgrade() else { return };
            if p.analysis_seq.get() != seq {
                return;
            }
            while let Some(c) = p.analysis_body.first_child() {
                p.analysis_body.remove(&c);
            }
            match features {
                Ok(f) => p.fill_analysis(&f),
                Err(_) => p.analysis_message("Analysis failed for this function."),
            }
        });
    }

    fn analysis_message(&self, msg: &str) {
        let l = gtk::Label::new(Some(msg));
        l.set_wrap(true);
        l.set_xalign(0.0);
        l.add_css_class("wc-empty");
        self.analysis_body.append(&l);
    }

    fn fill_analysis(&self, features: &graphing::analysis::KeyGraphFeatures) {
        if let Some(msg) = features.analysis_error_string() {
            self.analysis_message(msg);
            return;
        }
        for item in features.items() {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 2);
            card.add_css_class("wc-kgf");
            if !item.title.is_empty() {
                let t = gtk::Label::new(Some(&item.title));
                t.add_css_class("wc-kgf-title");
                t.set_xalign(0.0);
                card.append(&t);
            }
            for v in &item.display_items {
                let l = gtk::Label::new(Some(v));
                l.add_css_class(if item.is_text {
                    "wc-kgf-text"
                } else {
                    "wc-kgf-value"
                });
                l.set_xalign(0.0);
                l.set_wrap(true);
                l.set_selectable(true);
                card.append(&l);
            }
            for g in &item.grid_items {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                let a = gtk::Label::new(Some(&g.expression));
                a.add_css_class("wc-kgf-value");
                let b = gtk::Label::new(Some(&g.direction));
                b.add_css_class("wc-kgf-text");
                row.append(&a);
                row.append(&b);
                card.append(&row);
            }
            self.analysis_body.append(&card);
        }
    }

    fn settings_popover(self: &Rc<Self>) -> gtk::Popover {
        let grid = gtk::Grid::builder()
            .row_spacing(6)
            .column_spacing(8)
            .build();
        let mk = |label: &str| {
            let e = gtk::Entry::builder()
                .width_chars(7)
                .css_classes(["wc-range-entry"])
                .build();
            e.update_property(&[gtk::accessible::Property::Label(label)]);
            e
        };
        let (xmin, xmax, ymin, ymax) = (mk("X-Min"), mk("X-Max"), mk("Y-Min"), mk("Y-Max"));
        for (i, (l, e)) in [
            ("X-Min", &xmin),
            ("X-Max", &xmax),
            ("Y-Min", &ymin),
            ("Y-Max", &ymax),
        ]
        .iter()
        .enumerate()
        {
            let lab = gtk::Label::new(Some(l));
            lab.set_xalign(0.0);
            grid.attach(&lab, (i % 2 * 2) as i32, (i / 2) as i32, 1, 1);
            grid.attach(*e, (i % 2 * 2 + 1) as i32, (i / 2) as i32, 1, 1);
        }
        let units = adw::ToggleGroup::new();
        for (n, l) in [("rad", "Radians"), ("deg", "Degrees"), ("grad", "Gradians")] {
            units.add(adw::Toggle::builder().name(n).label(l).build());
        }
        units.set_active_name(Some("rad"));
        let thick = adw::ToggleGroup::new();
        for (i, w) in graphing::graph::LINE_WIDTHS.iter().enumerate() {
            thick.add(
                adw::Toggle::builder()
                    .name(i.to_string())
                    .label(format!("{w}"))
                    .build(),
            );
        }
        thick.set_active_name(Some("1"));
        let reset = gtk::Button::with_label("Reset view");
        reset.add_css_class("flat");

        let col = gtk::Box::new(gtk::Orientation::Vertical, 10);
        col.set_margin_top(8);
        col.set_margin_bottom(8);
        col.set_margin_start(8);
        col.set_margin_end(8);
        let h = |t: &str| {
            let l = gtk::Label::new(Some(t));
            l.add_css_class("wc-section");
            l.set_xalign(0.0);
            l
        };
        col.append(&h("Window"));
        col.append(&grid);
        col.append(&h("Units"));
        col.append(&units);
        col.append(&h("Line thickness"));
        col.append(&thick);
        col.append(&reset);
        let pop = gtk::Popover::builder()
            .child(&col)
            .css_classes(["wc-flyout"])
            .build();

        // Weak both ways: the GraphView keeps this callback, and the entries'
        // handlers reach the GraphView.
        let fill = {
            let entries = [&xmin, &xmax, &ymin, &ymax].map(|e| e.downgrade());
            move |vp: &graphing::Viewport| {
                let f = |v: f64| format!("{}", (v * 1000.0).round() / 1000.0);
                for (e, v) in entries.iter().zip([vp.x_min, vp.x_max, vp.y_min, vp.y_max]) {
                    if let Some(e) = e.upgrade()
                        && !e.has_focus()
                    {
                        e.set_text(&f(v));
                    }
                }
            }
        };
        let fill = Rc::new(fill);
        {
            let on_change = fill.clone();
            self.graph_view
                .connect_viewport_changed(move |vp| on_change(vp));
            let (gv, fill) = (self.graph_view.downgrade(), fill.clone());
            pop.connect_show(move |_| {
                if let Some(vp) = gv.upgrade().and_then(|gv| gv.viewport()) {
                    fill(&vp);
                }
            });
        }
        for e in [&xmin, &xmax, &ymin, &ymax] {
            let (gv, a, b, c, d) = (
                self.graph_view.downgrade(),
                xmin.downgrade(),
                xmax.downgrade(),
                ymin.downgrade(),
                ymax.downgrade(),
            );
            e.connect_activate(move |_| {
                let (Some(gv), Some(a), Some(b), Some(c), Some(d)) = (
                    gv.upgrade(),
                    a.upgrade(),
                    b.upgrade(),
                    c.upgrade(),
                    d.upgrade(),
                ) else {
                    return;
                };
                let p = |e: &gtk::Entry| e.text().replace('−', "-").trim().parse::<f64>().ok();
                let ok = match (p(&a), p(&b), p(&c), p(&d)) {
                    (Some(x0), Some(x1), Some(y0), Some(y1)) => gv.set_ranges(x0, x1, y0, y1),
                    _ => false,
                };
                // Rejected ranges (unparsable, min ≥ max, or a span the graph
                // can't map) are flagged instead of silently ignored.
                for e in [&a, &b, &c, &d] {
                    if ok {
                        e.remove_css_class("error");
                    } else {
                        e.add_css_class("error");
                    }
                }
            });
        }
        let weak = Rc::downgrade(self);
        units.connect_active_name_notify(move |t| {
            if let Some(p) = weak.upgrade() {
                let unit = match t.active_name().as_deref() {
                    Some("deg") => TrigUnit::Degrees,
                    Some("grad") => TrigUnit::Grads,
                    _ => TrigUnit::Radians,
                };
                p.graph.borrow_mut().set_trig_unit(unit);
                p.graph_view.invalidate();
            }
        });
        let gv = self.graph_view.clone();
        thick.connect_active_name_notify(move |t| {
            let i: usize = t.active_name().and_then(|n| n.parse().ok()).unwrap_or(1);
            gv.set_line_width(graphing::graph::LINE_WIDTHS[i.min(3)]);
        });
        let gv = self.graph_view.clone();
        reset.connect_clicked(move |_| gv.reset_view());
        pop
    }

    fn restore(self: &Rc<Self>) {
        let saved: Vec<SavedEquation> = match std::env::var("GMNB_EQUATIONS") {
            Ok(list) => session::from_list(&list),
            Err(_) => session::restore(self.ctx.store.page_state("graphing")),
        };
        self.building.set(true);
        for eq in &saved {
            if let Some(row) = self.add_equation(&eq.text) {
                row.color.set(eq.color);
                self.paint_swatch(&row);
                let s = self.ctx.hub.scheme();
                self.graph_view
                    .set_color(row.id, s.series[eq.color % s.series.len()]);
                if let Some(style) = session::style_from_key(&eq.style) {
                    self.graph.borrow_mut().set_line_style(row.id, style);
                }
                if eq.hidden {
                    self.graph.borrow_mut().set_line_enabled(row.id, false);
                    row.swatch.set_opacity(0.35);
                }
            }
        }
        self.next_color.set(session::next_color(&saved));
        self.building.set(false);
        let rows: Vec<(EquationId, String)> = self
            .rows
            .borrow()
            .iter()
            .map(|r| (r.id, r.entry.text().to_string()))
            .collect();
        for (id, text) in rows {
            self.equation_changed(id, &text);
        }
        if self.rows.borrow().is_empty() {
            self.add_equation("");
        }
    }
}

pub struct GraphingHandle(pub Rc<GraphingPage>);

impl Page for GraphingHandle {
    fn widget(&self) -> gtk::Widget {
        self.0.root.clone().upcast()
    }

    fn activate(&self, _mode: ViewMode) {}

    fn header_end(&self) -> Vec<gtk::Widget> {
        vec![self.0.mode_toggle.clone().upcast()]
    }

    fn key_pressed(&self, kp: &KeyPress) -> bool {
        let p = &self.0;
        match input::graph_shortcut(kp) {
            Some(GraphAction::ZoomIn) => p.graph_view.zoom_in(),
            Some(GraphAction::ZoomOut) => p.graph_view.zoom_out(),
            Some(GraphAction::ResetView) => p.graph_view.reset_view(),
            Some(GraphAction::ShowGraph) => p.mode_toggle.set_active_name(Some("graph")),
            None => return false,
        }
        true
    }

    fn copy(&self) -> Option<String> {
        let p = &self.0;
        let text = p
            .focused
            .borrow()
            .as_ref()
            .and_then(|w| w.upgrade())
            .map(|e| e.text().to_string())?;
        p.ctx.copy_to_clipboard(&text);
        Some(text)
    }

    fn paste(&self, text: &str) {
        self.0.insert_text(text.trim());
    }

    fn save(&self) {
        let p = &self.0;
        let graph = p.graph.borrow();
        let saved: Vec<SavedEquation> = p
            .rows
            .borrow()
            .iter()
            .filter(|r| !r.entry.text().trim().is_empty())
            .map(|r| SavedEquation {
                text: r.entry.text().to_string(),
                color: r.color.get(),
                style: session::style_key(graph.line_style(r.id)).into(),
                hidden: !graph.is_line_enabled(r.id),
            })
            .collect();
        if let Ok(v) = serde_json::to_value(saved) {
            p.ctx.store.set_page_state("graphing", v);
        }
    }
}

impl GraphingPage {
    pub fn handle(ctx: Rc<Ctx>) -> Rc<dyn Page> {
        Rc::new(GraphingHandle(Self::new(ctx)))
    }
}
