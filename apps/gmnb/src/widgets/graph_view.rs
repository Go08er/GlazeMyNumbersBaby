//! `GraphView` — renders a `graphing::Graph` with GSK paths.
//!
//! Drawn from the style's `graph.canvas` (docs/theming.md, "Graphing"): in
//! the base look DGMNB's canvas (a surface card with a 1 px border, a
//! faint grid, the axes, DGMNB's labels, flat curves, a trace dot with a
//! halo and a value bubble); a theme can give it a gradient, a glow under
//! the curves, a dashed cross-hair, a ring round the dot. Curves can draw
//! themselves in (`motion.graph_draw`) and the view glide to a new zoom
//! (`motion.graph_zoom`). Drag to pan, scroll or pinch to zoom.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use graphing::equation::LineStyle;
use graphing::graph::EquationPlot;
use graphing::grid::Grid;
use graphing::trace::TracePoint;
use graphing::{EquationId, Graph, Viewport};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib, graphene, gsk, pango};

use std::collections::BTreeMap;

use appcore::metrics::graph as mg;
use gmnb_theme::Style;
use gmnb_theme::types::{
    Border, Fill, Font, GradientKind, Paint, Rgba, Shadow, TRANSPARENT, Text, Value, alpha,
};

use super::animations_enabled;
use super::fit::{draw_line, pango_font};
/// Graphs that sample faster than this re-plot inline (perfectly in step
/// with panning); slower ones move to a worker thread.
const INLINE_PLOT_MS: f64 = 12.0;

type ViewportFn = Box<dyn Fn(&Viewport)>;

mod imp {
    use super::*;

    pub struct GraphView {
        pub graph: RefCell<Option<Rc<RefCell<Graph>>>>,
        pub vp: Cell<Option<Viewport>>,
        pub anim: Cell<Option<(Viewport, Viewport, Instant)>>,
        /// The geometry shown, and the plot requests and worker job that
        /// replace it.
        pub jobs: RefCell<PlotJobs>,
        pub dirty: Cell<bool>,
        pub colors: RefCell<HashMap<EquationId, [f32; 3]>>,
        pub style: RefCell<Option<Rc<Style>>>,
        /// `motion.graph_draw` and `motion.graph_zoom`, in seconds (0:
        /// none).
        pub draw_secs: Cell<f32>,
        pub zoom_secs: Cell<f32>,
        pub line_width: Cell<f64>,
        pub trace_on: Cell<bool>,
        /// Asks the page to switch tracing on (keeps its toggle in step).
        pub trace_wanted: RefCell<Option<Box<dyn Fn()>>>,
        pub pointer: Cell<Option<(f64, f64)>>,
        pub trace: RefCell<Option<(EquationId, TracePoint)>>,
        pub draw_in: RefCell<HashMap<EquationId, Instant>>,
        pub drag_origin: Cell<Option<Viewport>>,
        pub zoom_origin: Cell<Option<(Viewport, f64, f64)>>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
        pub on_viewport: RefCell<Vec<ViewportFn>>,
        pub fitted: Cell<bool>,
    }

    impl Default for GraphView {
        fn default() -> Self {
            Self {
                graph: RefCell::new(None),
                vp: Cell::new(None),
                anim: Cell::new(None),
                jobs: RefCell::default(),
                dirty: Cell::new(true),
                colors: RefCell::default(),
                style: RefCell::default(),
                draw_secs: Cell::new(0.0),
                zoom_secs: Cell::new(0.0),
                line_width: Cell::new(graphing::graph::DEFAULT_LINE_WIDTH),
                trace_on: Cell::new(false),
                trace_wanted: RefCell::default(),
                pointer: Cell::new(None),
                trace: RefCell::new(None),
                draw_in: RefCell::default(),
                drag_origin: Cell::new(None),
                zoom_origin: Cell::new(None),
                tick: RefCell::new(None),
                on_viewport: RefCell::default(),
                fitted: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GraphView {
        const NAME: &'static str = "GmnbGraphView";
        type Type = super::GraphView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("graphview");
            klass.set_accessible_role(gtk::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for GraphView {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_input();
            self.obj().set_focusable(true);
            // Screenshot/dev hook: trace at a fixed pointer position "x,y".
            if let Some((x, y)) = std::env::var("GMNB_GRAPH_POINTER").ok().and_then(|v| {
                let (a, b) = v.split_once(',')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            }) {
                self.pointer.set(Some((x, y)));
                self.trace_on.set(true);
            }
            self.obj()
                .update_property(&[gtk::accessible::Property::Label("Graph")]);
        }
        fn dispose(&self) {
            if let Some(id) = self.tick.take() {
                id.remove();
            }
        }
    }

    impl WidgetImpl for GraphView {
        fn measure(&self, _o: gtk::Orientation, _f: i32) -> (i32, i32, i32, i32) {
            (160, 400, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            let obj = self.obj();
            let (w, h) = (width as f64, height as f64);
            let vp = match self.vp.get() {
                Some(vp) if self.fitted.get() => vp.with_size(w, h),
                _ => {
                    self.fitted.set(true);
                    obj.fitted_viewport(w, h)
                }
            };
            self.vp.set(Some(vp));
            self.dirty.set(true);
        }

        fn snapshot(&self, s: &gtk::Snapshot) {
            self.obj().draw(s);
            // What the graph's plots hold meets the memory backstop as a
            // theme's layers do (crate::memory): after a frame that drew it.
            if let Some(hub) = crate::theme::Hub::current() {
                hub.layers().painted();
            }
        }
    }
}

glib::wrapper! {
    pub struct GraphView(ObjectSubclass<imp::GraphView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for GraphView {
    fn default() -> Self {
        glib::Object::new()
    }
}

fn lerp_vp(a: &Viewport, b: &Viewport, t: f64) -> Viewport {
    let l = |x: f64, y: f64| x + (y - x) * t;
    Viewport::new(
        l(a.x_min, b.x_min),
        l(a.x_max, b.x_max),
        l(a.y_min, b.y_min),
        l(a.y_max, b.y_max),
        b.width,
        b.height,
    )
}

impl GraphView {
    pub fn set_graph(&self, graph: Rc<RefCell<Graph>>) {
        self.imp().graph.replace(Some(graph));
        self.invalidate();
    }

    pub fn set_style(&self, style: &Rc<Style>) {
        let imp = self.imp();
        imp.draw_secs
            .set(style.number("motion.graph_draw") / 1000.0);
        imp.zoom_secs
            .set(style.number("motion.graph_zoom") / 1000.0);
        imp.style.replace(Some(style.clone()));
        self.queue_draw();
    }

    pub fn set_color(&self, id: EquationId, color: [f32; 3]) {
        self.imp().colors.borrow_mut().insert(id, color);
        self.queue_draw();
    }

    /// Drop a removed equation's colour.
    pub fn forget_color(&self, id: EquationId) {
        self.imp().colors.borrow_mut().remove(&id);
    }

    pub fn set_line_width(&self, w: f64) {
        self.imp().line_width.set(w);
        self.queue_draw();
    }

    pub fn line_width(&self) -> f64 {
        self.imp().line_width.get()
    }

    /// Re-sample everything (equations, variables or units changed).
    pub fn invalidate(&self) {
        self.imp().dirty.set(true);
        self.queue_draw();
    }

    /// Have `id`'s curve trace itself in.
    pub fn animate_draw(&self, id: EquationId) {
        if animations_enabled() && self.imp().draw_secs.get() > 0.0 {
            self.imp().draw_in.borrow_mut().insert(id, Instant::now());
            self.ensure_ticking();
        }
    }

    /// Called when keyboard tracing needs tracing switched on.
    pub fn connect_trace_wanted(&self, f: impl Fn() + 'static) {
        self.imp().trace_wanted.replace(Some(Box::new(f)));
    }

    /// Move the trace cursor by keyboard and announce the traced value. The
    /// cursor keeps every step along the arrow's axis, even one too small to
    /// move the traced point, and moves to the curve across it.
    fn trace_key(&self, dx: f64, dy: f64) {
        let imp = self.imp();
        if !imp.trace_on.get() {
            if let Some(f) = imp.trace_wanted.borrow().as_ref() {
                f();
            }
            if !imp.trace_on.get() {
                self.set_trace(true);
            }
        }
        let (w, h) = (self.width() as f64, self.height() as f64);
        if w <= 1.0 || h <= 1.0 {
            return;
        }
        let (x, y) = imp
            .pointer
            .get()
            .filter(|&(x, y)| (0.0..w).contains(&x) && (0.0..h).contains(&y))
            .unwrap_or((w / 2.0, h / 2.0));
        let (x, y) = ((x + dx).clamp(0.0, w - 1.0), (y + dy).clamp(0.0, h - 1.0));
        imp.pointer.set(Some((x, y)));
        self.update_trace();
        let traced = imp.trace.borrow().as_ref().map(|(_, t)| *t);
        if let Some(t) = traced {
            imp.pointer.set(Some(if dx != 0.0 {
                (x, t.screen_y)
            } else {
                (t.screen_x, y)
            }));
            self.announce(
                &t.spoken_text(),
                gtk::AccessibleAnnouncementPriority::Medium,
            );
        }
        self.queue_draw();
    }

    pub fn set_trace(&self, on: bool) {
        self.imp().trace_on.set(on);
        if !on {
            self.imp().trace.replace(None);
        }
        self.update_trace();
        self.queue_draw();
    }

    pub fn viewport(&self) -> Option<Viewport> {
        self.imp().vp.get()
    }

    pub fn connect_viewport_changed(&self, f: impl Fn(&Viewport) + 'static) {
        self.imp().on_viewport.borrow_mut().push(Box::new(f));
    }

    fn notify_viewport(&self) {
        if let Some(vp) = self.imp().vp.get() {
            for f in self.imp().on_viewport.borrow().iter() {
                f(&vp);
            }
        }
    }

    fn fitted_viewport(&self, w: f64, h: f64) -> Viewport {
        match self.imp().graph.borrow().as_ref() {
            Some(g) => g.borrow().fit_viewport(w, h),
            None => Viewport::default_for_size(w, h),
        }
    }

    fn set_vp(&self, vp: Viewport) {
        self.imp().vp.set(Some(vp));
        self.imp().dirty.set(true);
        self.update_trace();
        self.queue_draw();
        self.notify_viewport();
    }

    /// Animate to `target` (or jump when animations are off).
    pub fn animate_to(&self, target: Viewport) {
        let Some(cur) = self.imp().vp.get() else {
            return;
        };
        if animations_enabled() && self.imp().zoom_secs.get() > 0.0 {
            self.imp().anim.set(Some((cur, target, Instant::now())));
            self.ensure_ticking();
        } else {
            self.set_vp(target);
        }
    }

    pub fn zoom_in(&self) {
        if let Some(mut vp) = self.imp().vp.get() {
            for _ in 0..4 {
                vp.zoom_in();
            }
            self.animate_to(vp);
        }
    }

    pub fn zoom_out(&self) {
        if let Some(mut vp) = self.imp().vp.get() {
            for _ in 0..4 {
                vp.zoom_out();
            }
            self.animate_to(vp);
        }
    }

    /// "Reset view" / best fit.
    pub fn reset_view(&self) {
        let (w, h) = (self.width() as f64, self.height() as f64);
        if w > 0.0 {
            self.animate_to(self.fitted_viewport(w, h));
        }
    }

    pub fn set_ranges(&self, x_min: f64, x_max: f64, y_min: f64, y_max: f64) -> bool {
        let Some(mut vp) = self.imp().vp.get() else {
            return false;
        };
        if vp.set_display_ranges(x_min, x_max, y_min, y_max).is_ok() {
            self.animate_to(vp);
            true
        } else {
            false
        }
    }

    fn ensure_ticking(&self) {
        if self.imp().tick.borrow().is_some() {
            return;
        }
        let id = self.add_tick_callback(|g, _| {
            let imp = g.imp();
            if let Some((from, to, start)) = imp.anim.get() {
                let secs = imp.zoom_secs.get().max(1e-3);
                let p = (start.elapsed().as_secs_f32() / secs).min(1.0);
                let e = 1.0 - (1.0 - p as f64).powi(3);
                g.set_vp(lerp_vp(&from, &to, e));
                if p >= 1.0 {
                    imp.anim.set(None);
                }
            }
            let draw = imp.draw_secs.get();
            imp.draw_in
                .borrow_mut()
                .retain(|_, t| t.elapsed().as_secs_f32() < draw);
            g.queue_draw();
            if imp.anim.get().is_none() && imp.draw_in.borrow().is_empty() {
                imp.tick.replace(None);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        self.imp().tick.replace(Some(id));
    }

    fn setup_input(&self) {
        let drag = gtk::GestureDrag::new();
        let weak = self.downgrade();
        drag.connect_drag_begin(move |_, _, _| {
            if let Some(g) = weak.upgrade() {
                g.imp().anim.set(None);
                g.imp().drag_origin.set(g.imp().vp.get());
                g.grab_focus();
            }
        });
        let weak = self.downgrade();
        drag.connect_drag_update(move |_, dx, dy| {
            if let Some(g) = weak.upgrade()
                && let Some(mut vp) = g.imp().drag_origin.get()
            {
                vp.pan_pixels(dx, dy);
                g.set_vp(vp);
            }
        });
        self.add_controller(drag);

        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let weak = self.downgrade();
        scroll.connect_scroll(move |_, _dx, dy| {
            let Some(g) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let Some(mut vp) = g.imp().vp.get() else {
                return glib::Propagation::Proceed;
            };
            let (px, py) = g
                .imp()
                .pointer
                .get()
                .unwrap_or((vp.width / 2.0, vp.height / 2.0));
            g.imp().anim.set(None);
            vp.wheel_zoom(px, py, -dy * Viewport::WHEEL_DELTA_LINE);
            g.set_vp(vp);
            glib::Propagation::Stop
        });
        self.add_controller(scroll);

        let zoom = gtk::GestureZoom::new();
        let weak = self.downgrade();
        zoom.connect_begin(move |z, _| {
            if let Some(g) = weak.upgrade()
                && let (Some(vp), Some((cx, cy))) = (g.imp().vp.get(), z.bounding_box_center())
            {
                g.imp().zoom_origin.set(Some((vp, cx, cy)));
            }
        });
        let weak = self.downgrade();
        zoom.connect_scale_changed(move |_, scale| {
            if let Some(g) = weak.upgrade()
                && let Some((mut vp, cx, cy)) = g.imp().zoom_origin.get()
            {
                vp.zoom_about_pixel(cx, cy, 1.0 / scale.max(0.05));
                g.set_vp(vp);
            }
        });
        self.add_controller(zoom);

        // Keyboard tracing, as the design's Grapher: with the graph
        // focused, the arrows move the trace cursor (5 px, 1 px with Shift).
        let keys = gtk::EventControllerKey::new();
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, key, _, state| {
            let Some(g) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if state.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK) {
                return glib::Propagation::Proceed;
            }
            let d = if state.contains(gdk::ModifierType::SHIFT_MASK) {
                1.0
            } else {
                5.0
            };
            let (dx, dy) = match key {
                gdk::Key::Left => (-d, 0.0),
                gdk::Key::Right => (d, 0.0),
                gdk::Key::Up => (0.0, -d),
                gdk::Key::Down => (0.0, d),
                _ => return glib::Propagation::Proceed,
            };
            g.trace_key(dx, dy);
            glib::Propagation::Stop
        });
        self.add_controller(keys);

        let motion = gtk::EventControllerMotion::new();
        let weak = self.downgrade();
        motion.connect_motion(move |_, x, y| {
            if let Some(g) = weak.upgrade() {
                g.imp().pointer.set(Some((x, y)));
                if g.imp().trace_on.get() {
                    g.update_trace();
                    g.queue_draw();
                }
            }
        });
        let weak = self.downgrade();
        motion.connect_leave(move |_| {
            if let Some(g) = weak.upgrade() {
                g.imp().pointer.set(None);
                g.imp().trace.replace(None);
                g.queue_draw();
            }
        });
        self.add_controller(motion);
    }

    fn replot_if_dirty(&self) {
        let imp = self.imp();
        if !imp.dirty.replace(false) {
            return;
        }
        let (Some(vp), Some(graph)) = (imp.vp.get(), imp.graph.borrow().clone()) else {
            return;
        };
        let weight = graph.borrow().plot_weight();
        let plan = imp.jobs.borrow_mut().request(weight);
        match plan {
            Plan::Inline => {
                let started = Instant::now();
                let plots = graph.borrow().plot_parallel(&vp);
                let ms = started.elapsed().as_secs_f64() * 1e3;
                imp.jobs.borrow_mut().install(plots, ms, weight);
            }
            Plan::Wait => {}
            Plan::Worker(job, cancel) => {
                // Heavy graph: plot on a worker and keep drawing the last
                // result (curves are in graph coordinates, so they still
                // line up while panning).
                let graph = graph.borrow().clone();
                let weak = self.downgrade();
                glib::spawn_future_local(async move {
                    let started = Instant::now();
                    let plots =
                        gio::spawn_blocking(move || graph.plot_parallel_cancellable(&vp, &cancel))
                            .await;
                    let Some(this) = weak.upgrade() else { return };
                    let ms = started.elapsed().as_secs_f64() * 1e3;
                    let imp = this.imp();
                    if imp.jobs.borrow_mut().finish(job, plots.ok().flatten(), ms) {
                        imp.dirty.set(true);
                    }
                    this.update_trace();
                    this.queue_draw();
                });
            }
        }
    }

    fn update_trace(&self) {
        let imp = self.imp();
        if !imp.trace_on.get() {
            return;
        }
        self.replot_if_dirty();
        let (Some(vp), Some((px, py)), Some(graph)) =
            (imp.vp.get(), imp.pointer.get(), imp.graph.borrow().clone())
        else {
            return;
        };
        let t = graph.borrow().trace(
            &vp,
            &imp.jobs.borrow().plots,
            px,
            py,
            graphing::trace::DEFAULT_TRACE_RADIUS_PX,
        );
        imp.trace.replace(t);
    }

    /// Render the current graph into a texture (for "copy graph").
    pub fn to_texture(&self) -> Option<gtk::gdk::Texture> {
        let (w, h) = (self.width() as f32, self.height() as f32);
        let snapshot = gtk::Snapshot::new();
        self.draw(&snapshot);
        let node = snapshot.to_node()?;
        let renderer = self.native()?.renderer()?;
        Some(renderer.render_texture(&node, Some(&graphene::Rect::new(0.0, 0.0, w, h))))
    }

    /// A label's line in the style's `graph.canvas.label` font.
    fn label_layout(&self, text: &str, font: &Font) -> pango::Layout {
        let layout = self.create_pango_layout(Some(text));
        let (fd, attrs) = pango_font(font, font.size);
        layout.set_font_description(Some(&fd));
        layout.set_attributes(Some(&attrs));
        layout
    }

    /// The height of `font`'s capitals (what DGMNB centres a line on).
    fn cap(&self, font: &Font) -> f32 {
        let (ink, _) = self.label_layout("H", font).extents();
        ink.height() as f32 / pango::SCALE as f32
    }

    fn draw(&self, s: &gtk::Snapshot) {
        let imp = self.imp();
        let Some(style) = imp.style.borrow().clone() else {
            return;
        };
        let look = Look::of(&style);
        let (w, h) = (self.width() as f32, self.height() as f32);
        if w <= 1.0 || h <= 1.0 {
            return;
        }
        self.replot_if_dirty();
        if imp.trace_on.get() && imp.trace.borrow().is_none() && imp.pointer.get().is_some() {
            self.update_trace();
        }
        let Some(vp) = imp.vp.get() else { return };
        let bounds = graphene::Rect::new(0.0, 0.0, w, h);
        let rr = gsk::RoundedRect::from_rect(bounds, look.radius);

        // The canvas.
        s.push_rounded_clip(&rr);
        paint_rect(s, &look.background, &bounds);

        // Grid. Lines have round ends, as DGMNB strokes them.
        let grid = Grid::for_viewport(&vp);
        let to_sx = |x: f64| vp.to_screen(x, 0.0).0 as f32;
        let to_sy = |y: f64| vp.to_screen(0.0, y).1 as f32;
        let line = |s: &gtk::Snapshot,
                    x0: f32,
                    y0: f32,
                    x1: f32,
                    y1: f32,
                    color: gtk::gdk::RGBA,
                    width: f32| {
            let b = gsk::PathBuilder::new();
            b.move_to(x0, y0);
            b.line_to(x1, y1);
            let st = gsk::Stroke::new(width);
            st.set_line_cap(gsk::LineCap::Round);
            s.append_stroke(&b.to_path(), &st, &color);
        };
        let (minor, major) = (rgba(look.grid_minor), rgba(look.grid_major));
        let gw = mg::GRID_WIDTH;
        for x in &grid.x.minor {
            let sx = to_sx(*x);
            line(s, sx, 0.0, sx, h, minor, gw);
        }
        for y in &grid.y.minor {
            let sy = to_sy(*y);
            line(s, 0.0, sy, w, sy, minor, gw);
        }
        for t in &grid.x.major {
            let sx = to_sx(t.value);
            line(s, sx, 0.0, sx, h, major, gw);
        }
        for t in &grid.y.major {
            let sy = to_sy(t.value);
            line(s, 0.0, sy, w, sy, major, gw);
        }
        let axis = rgba(look.axis.0);
        let (ax, ay) = (to_sx(0.0), to_sy(0.0));
        if (0.0..=w).contains(&ax) {
            line(s, ax, 0.0, ax, h, axis, look.axis.1);
        }
        if (0.0..=h).contains(&ay) {
            line(s, 0.0, ay, w, ay, axis, look.axis.1);
        }
        // Labels hug the axes, or the edges when an axis is off-screen
        // (DGMNB's offsets).
        let label_color = rgba(look.label.color);
        let font = &look.label.font;
        let cap = self.cap(font);
        let ly = ay.clamp(4.0, h - 18.0);
        for t in &grid.x.major {
            if t.value.abs() < 1e-12 {
                continue;
            }
            let layout = self.label_layout(&t.label, font);
            let lw = layout_width(&layout);
            let sx = to_sx(t.value) - lw / 2.0;
            if sx < 4.0 || sx + lw > w - 4.0 {
                continue;
            }
            draw_line(s, &layout, sx, (ly + 4.0 + cap).round(), &label_color);
        }
        let lx = ax.clamp(4.0, w - 40.0);
        for t in &grid.y.major {
            if t.value.abs() < 1e-12 {
                continue;
            }
            let layout = self.label_layout(&t.label, font);
            let lw = layout_width(&layout);
            let x = if lx + 6.0 + lw > w {
                lx - 6.0 - lw
            } else {
                lx + 6.0
            };
            let y = to_sy(t.value);
            if y < 10.0 || y > h - 10.0 {
                continue;
            }
            draw_line(s, &layout, x, (y + cap / 2.0).round(), &label_color);
        }

        // Curves.
        let graph = imp.graph.borrow().clone();
        let lw = imp.line_width.get() as f32;
        let series = &style.tokens.colors.series;
        let draw_secs = imp.draw_secs.get();
        for (i, ep) in imp.jobs.borrow().plots.iter().enumerate() {
            let color = imp
                .colors
                .borrow()
                .get(&ep.id)
                .copied()
                .unwrap_or_else(|| rgb3(series[i % series.len()]));
            let line_style = graph
                .as_ref()
                .map(|g| g.borrow().line_style(ep.id))
                .unwrap_or_default();
            let draw_p = imp
                .draw_in
                .borrow()
                .get(&ep.id)
                .filter(|_| draw_secs > 0.0)
                .map(|t| {
                    let p = (t.elapsed().as_secs_f32() / draw_secs).min(1.0);
                    1.0 - (1.0 - p).powi(3)
                })
                .unwrap_or(1.0);

            if !ep.plot.fill.is_empty() {
                let b = gsk::PathBuilder::new();
                for poly in &ep.plot.fill {
                    for (k, p) in poly.iter().enumerate() {
                        let (sx, sy) = vp.to_screen(p.x, p.y);
                        if k == 0 {
                            b.move_to(sx as f32, sy as f32);
                        } else {
                            b.line_to(sx as f32, sy as f32);
                        }
                    }
                    b.close();
                }
                s.append_fill(
                    &b.to_path(),
                    gsk::FillRule::Winding,
                    &rgba3(color, look.fill_opacity * draw_p),
                );
            }

            let b = gsk::PathBuilder::new();
            let mut any = false;
            for poly in &ep.plot.curves {
                for (k, p) in poly.iter().enumerate() {
                    let (sx, sy) = vp.to_screen(p.x, p.y);
                    if k == 0 {
                        b.move_to(sx as f32, sy as f32);
                    } else {
                        b.line_to(sx as f32, sy as f32);
                    }
                    any = true;
                }
            }
            if !any {
                continue;
            }
            let path = b.to_path();
            let len = if draw_p < 1.0 {
                gsk::PathMeasure::new(&path).length()
            } else {
                0.0
            };
            let make = |width: f32| {
                let st = gsk::Stroke::new(width);
                st.set_line_cap(gsk::LineCap::Round);
                st.set_line_join(gsk::LineJoin::Round);
                let pattern: &[f32] = match (ep.plot.boundary_dashed, line_style) {
                    (true, _) | (_, LineStyle::Dash) => &[8.0, 6.0],
                    (_, LineStyle::Dot) => &[0.1, 5.0],
                    (_, LineStyle::DashDot) => &[8.0, 5.0, 0.1, 5.0],
                    (_, LineStyle::DashDotDot) => &[8.0, 5.0, 0.1, 5.0, 0.1, 5.0],
                    _ => &[],
                };
                if draw_p < 1.0 {
                    st.set_dash(&[len * draw_p, len + 1.0]);
                } else if !pattern.is_empty() {
                    st.set_dash(pattern);
                }
                st
            };
            // Proven holes ((x²−1)/(x−1) at 1): the curve is cut away
            // inside an open circle.
            let hole_r = graphing::graph::trace_point_radius(imp.line_width.get()) as f32 + 1.0;
            let holes = || {
                let b = gsk::PathBuilder::new();
                for p in &ep.plot.holes {
                    let (sx, sy) = vp.to_screen(p.x, p.y);
                    b.add_circle(&graphene::Point::new(sx as f32, sy as f32), hole_r);
                }
                b.to_path()
            };
            let has_holes = !ep.plot.holes.is_empty();
            if has_holes {
                s.push_mask(gsk::MaskMode::InvertedAlpha);
                s.append_fill(
                    &holes(),
                    gsk::FillRule::Winding,
                    &rgba3([1.0, 1.0, 1.0], 1.0),
                );
                s.pop();
            }
            // A theme's glow: a blurred, wider copy under the line.
            if let Some((blur, width, opacity)) = look.glow {
                s.push_blur(f64::from(blur));
                s.append_stroke(&path, &make(lw * width), &rgba3(color, opacity));
                s.pop();
            }
            s.append_stroke(&path, &make(lw), &rgba3(color, 1.0));
            if has_holes {
                s.pop();
                s.append_stroke(
                    &holes(),
                    &gsk::Stroke::new((lw * 0.75).max(1.25)),
                    &rgba3(color, draw_p),
                );
            }
        }

        // Trace.
        if let Some((id, tp)) = imp.trace.borrow().as_ref() {
            let color = imp
                .colors
                .borrow()
                .get(id)
                .copied()
                .unwrap_or_else(|| rgb3(style.tokens.colors.accent));
            let (sx, sy) = (tp.screen_x as f32, tp.screen_y as f32);
            let hair = rgba3(color, look.hair_opacity);
            let st = gsk::Stroke::new(mg::HAIR_WIDTH);
            st.set_line_cap(gsk::LineCap::Round);
            // (GSK refuses a dash that never draws or never moves on: a
            // theme's `{0, 0}` is a solid line, as `draw:stroke` takes it.)
            if let Some((dash, gap)) = look.hair_dash.filter(|(d, g)| d + g > 0.0) {
                st.set_dash(&[dash, gap]);
            }
            for (x0, y0, x1, y1) in [(sx, 0.0, sx, h), (0.0, sy, w, sy)] {
                let b = gsk::PathBuilder::new();
                b.move_to(x0, y0);
                b.line_to(x1, y1);
                s.append_stroke(&b.to_path(), &st, &hair);
            }
            let r = graphing::graph::trace_point_radius(imp.line_width.get()) as f32
                + mg::TRACE_DOT_ADD;
            let disc = |radius: f32, c: gtk::gdk::RGBA| {
                let b = gsk::PathBuilder::new();
                b.add_circle(&graphene::Point::new(sx, sy), radius);
                s.append_fill(&b.to_path(), gsk::FillRule::Winding, &c);
            };
            if let Some((grow, halo, opacity)) = look.halo {
                let c = match halo {
                    Some(c) => rgba(alpha(c, opacity)),
                    None => rgba3(color, opacity),
                };
                disc(r + grow, c);
            }
            disc(r, rgba3(color, 1.0));
            if let Some(ring) = look.ring {
                let b = gsk::PathBuilder::new();
                b.add_circle(&graphene::Point::new(sx, sy), r);
                s.append_stroke(
                    &b.to_path(),
                    &gsk::Stroke::new(ring.width),
                    &rgba(ring.color),
                );
            }

            let bubble = &look.bubble;
            let text = tp.text();
            let layout = self.label_layout(&text, &bubble.font);
            let tw = layout_width(&layout);
            let (bw, bh) = (tw + mg::BUBBLE_PAD, mg::BUBBLE_H);
            let mut bx = sx + mg::BUBBLE_OFFSET;
            let mut by = sy - bh - mg::BUBBLE_OFFSET;
            if bx + bw > w - mg::BUBBLE_EDGE {
                bx = sx - bw - mg::BUBBLE_OFFSET;
            }
            if by < mg::BUBBLE_EDGE {
                by = sy + mg::BUBBLE_OFFSET;
            }
            let rect = graphene::Rect::new(bx, by, bw, bh);
            surface(
                s,
                &rect,
                bubble.radius,
                &Fill::color(bubble.background),
                bubble.border,
                &bubble.shadow,
            );
            let cap = self.cap(&bubble.font);
            let tx = bx + (bw - tw) / 2.0;
            draw_line(
                s,
                &layout,
                tx,
                (by + bh / 2.0 + cap / 2.0).round(),
                &rgba(bubble.color),
            );
        }
        s.pop();

        // The border, inside the edge.
        if let Some(b) = look.border {
            s.append_border(&rr, &[b.width; 4], &[rgba(b.color); 4]);
        }
    }
}

/// What the canvas is drawn with, from the style (`graph.canvas`).
struct Look {
    background: Paint,
    radius: f32,
    border: Option<Border>,
    grid_minor: Rgba,
    grid_major: Rgba,
    /// Colour and width.
    axis: (Rgba, f32),
    label: Text,
    fill_opacity: f32,
    /// Blur, width (times the line's), opacity.
    glow: Option<(f32, f32, f32)>,
    hair_opacity: f32,
    hair_dash: Option<(f32, f32)>,
    /// Grow, colour (`None`: the curve's), opacity.
    halo: Option<(f32, Option<Rgba>, f32)>,
    ring: Option<Border>,
    bubble: Bubble,
}

struct Bubble {
    background: Rgba,
    color: Rgba,
    radius: f32,
    font: Font,
    border: Option<Border>,
    shadow: Vec<Shadow>,
}

/// The parts of the record at `path`.
fn parts<'a>(style: &'a Style, path: &str) -> Option<&'a BTreeMap<&'static str, Value>> {
    match style.get(path) {
        Some(Value::Record(r)) => Some(r),
        _ => None,
    }
}

fn number(v: Option<&Value>) -> f32 {
    match v {
        Some(Value::Number(n)) => *n as f32,
        _ => 0.0,
    }
}

fn color(v: Option<&Value>) -> Rgba {
    match v {
        Some(Value::Color(c)) => *c,
        _ => TRANSPARENT,
    }
}

fn border(v: Option<&Value>) -> Option<Border> {
    match v {
        Some(Value::Border(b)) => Some(*b),
        _ => None,
    }
}

impl Look {
    fn of(style: &Style) -> Look {
        let c = &style.tokens.colors;
        let axis = parts(style, "graph.canvas.axis");
        let glow = parts(style, "graph.canvas.glow").map(|g| {
            (
                number(g.get("blur")),
                number(g.get("width")),
                number(g.get("opacity")),
            )
        });
        let hair_dash = match style.get("graph.canvas.trace.hair_dash") {
            Some(Value::Numbers(n)) if n.len() == 2 => Some((n[0] as f32, n[1] as f32)),
            _ => None,
        };
        let halo = parts(style, "graph.canvas.trace.halo").map(|h| {
            let color = match h.get("color") {
                Some(Value::Color(c)) => Some(*c),
                // "series": the curve's own.
                _ => None,
            };
            (number(h.get("grow")), color, number(h.get("opacity")))
        });
        let b = parts(style, "graph.canvas.trace.bubble");
        let bubble = Bubble {
            background: b.map_or(c.surface, |b| color(b.get("background"))),
            color: b.map_or(c.fg, |b| color(b.get("color"))),
            radius: b.map_or(0.0, |b| number(b.get("radius"))),
            font: match b.and_then(|b| b.get("font")) {
                Some(Value::Font(f)) => f.clone(),
                _ => style.tokens.text.small.clone(),
            },
            border: b.and_then(|b| border(b.get("border"))),
            shadow: match b.and_then(|b| b.get("shadow")) {
                Some(Value::Shadows(s)) => s.clone(),
                _ => Vec::new(),
            },
        };
        Look {
            background: match style.get("graph.canvas.background") {
                Some(Value::Paint(p)) => p.clone(),
                _ => Paint::Color(c.surface),
            },
            radius: style.number("graph.canvas.radius"),
            border: border(style.get("graph.canvas.border")),
            grid_minor: style.color("graph.canvas.grid_minor"),
            grid_major: style.color("graph.canvas.grid_major"),
            axis: (
                axis.map_or(c.fg, |a| color(a.get("color"))),
                axis.map_or(1.0, |a| number(a.get("width"))),
            ),
            label: match style.get("graph.canvas.label") {
                Some(Value::Text(t)) => t.clone(),
                _ => Text {
                    color: c.fg_dim,
                    font: style.tokens.text.small.clone(),
                    shadow: Vec::new(),
                    margin: Default::default(),
                },
            },
            fill_opacity: style.number("graph.canvas.fill_opacity"),
            glow,
            hair_opacity: style.number("graph.canvas.trace.hair_opacity"),
            hair_dash,
            halo,
            ring: border(style.get("graph.canvas.trace.ring")),
            bubble,
        }
    }
}

fn rgba(c: Rgba) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(c[0], c[1], c[2], c[3])
}

fn rgba3(c: [f32; 3], a: f32) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(c[0], c[1], c[2], a)
}

fn rgb3(c: Rgba) -> [f32; 3] {
    [c[0], c[1], c[2]]
}

fn layout_width(layout: &pango::Layout) -> f32 {
    let (_, logical) = layout.extents();
    logical.width() as f32 / pango::SCALE as f32
}

/// A paint over `rect`: a colour, or a gradient along the rect.
fn paint_rect(s: &gtk::Snapshot, paint: &Paint, rect: &graphene::Rect) {
    match paint {
        Paint::Color(c) => s.append_color(&rgba(*c), rect),
        Paint::Gradient(g) => {
            let stops: Vec<gsk::ColorStop> = g
                .colors
                .iter()
                .zip(&g.stops)
                .map(|(c, t)| gsk::ColorStop::new(*t, rgba(*c)))
                .collect();
            match g.kind {
                GradientKind::Linear => {
                    // CSS angles: 0deg points up, 90deg right.
                    let a = g.angle.to_radians();
                    let (dx, dy) = (a.sin(), -a.cos());
                    let (w, h) = (rect.width(), rect.height());
                    let half = (w * dx.abs() + h * dy.abs()) / 2.0;
                    let (cx, cy) = (rect.x() + w / 2.0, rect.y() + h / 2.0);
                    s.append_linear_gradient(
                        rect,
                        &graphene::Point::new(cx - dx * half, cy - dy * half),
                        &graphene::Point::new(cx + dx * half, cy + dy * half),
                        &stops,
                    );
                }
                _ => {
                    let center = graphene::Point::new(
                        rect.x() + rect.width() * g.at[0],
                        rect.y() + rect.height() * g.at[1],
                    );
                    let r = rect.width().max(rect.height());
                    s.append_radial_gradient(rect, &center, r, r, 0.0, 1.0, &stops);
                }
            }
        }
    }
}

/// A raised surface: its shadows, its fill and its border inside the
/// edge, as DGMNB's `surface` (three rings for a shadow).
fn surface(
    s: &gtk::Snapshot,
    rect: &graphene::Rect,
    radius: f32,
    fill: &Fill,
    border: Option<Border>,
    shadows: &[Shadow],
) {
    let rr = gsk::RoundedRect::from_rect(*rect, radius);
    for sh in shadows.iter().filter(|sh| !sh.inset) {
        s.append_outset_shadow(&rr, &rgba(sh.color), sh.x, sh.y, sh.spread, sh.blur);
    }
    s.push_rounded_clip(&rr);
    for paint in fill.0.iter().rev() {
        paint_rect(s, paint, rect);
    }
    s.pop();
    if let Some(b) = border {
        s.append_border(&rr, &[b.width; 4], &[rgba(b.color); 4]);
    }
}

/// The geometry a [`GraphView`] shows, and the plot requests that replace
/// it: inline when the prediction from the last plot
/// ([`graphing::graph::predicted_plot_ms`]) is quick, else on a worker,
/// one job at a time. Every request supersedes all older work, even one
/// plotted inline: it cancels the running job, and a job's plots are
/// installed only if no request came after its own (R12-M-06: an old
/// worker finishing after a quick inline plot put its old graph's curves
/// back). A request made while a job runs waits for it to report back.
#[derive(Default)]
pub struct PlotJobs {
    /// The geometry shown.
    pub plots: Vec<EquationPlot>,
    /// The latest request (requests are numbered from 1).
    latest: u64,
    /// The worker job running, if any.
    running: Option<Job>,
    /// A request is waiting for the running job to report back.
    again: bool,
    /// How long the plot shown took, and its `Graph::plot_weight`.
    ms: f64,
    weight: f64,
}

struct Job {
    /// The request it answers.
    request: u64,
    cancel: Arc<AtomicBool>,
    weight: f64,
}

/// How to answer a plot request.
pub enum Plan {
    /// Plot now, on this thread, then [`PlotJobs::install`].
    Inline,
    /// Plot on a worker, polling the flag, then [`PlotJobs::finish`] with
    /// this request's number.
    Worker(u64, Arc<AtomicBool>),
    /// Nothing yet: a job is running; [`PlotJobs::finish`] asks for this
    /// request to be made again.
    Wait,
}

impl PlotJobs {
    /// A new request, for a graph of `Graph::plot_weight` `weight`.
    pub fn request(&mut self, weight: f64) -> Plan {
        self.latest += 1;
        // Whatever runs now answers an older request.
        if let Some(job) = &self.running {
            job.cancel.store(true, Ordering::Relaxed);
        }
        // Inline only if this plot, scaled from the last by the graph's
        // weight, is quick: an edit to a heavy row goes to the worker.
        if graphing::graph::predicted_plot_ms(self.ms, self.weight, weight) < INLINE_PLOT_MS {
            self.again = false;
            return Plan::Inline;
        }
        if self.running.is_some() {
            self.again = true;
            return Plan::Wait;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.running = Some(Job {
            request: self.latest,
            cancel: cancel.clone(),
            weight,
        });
        Plan::Worker(self.latest, cancel)
    }

    /// Shows the inline plot of the latest request, which took `ms`.
    pub fn install(&mut self, plots: Vec<EquationPlot>, ms: f64, weight: f64) {
        self.plots = plots;
        self.ms = ms;
        self.weight = weight;
    }

    /// The worker job for request `request` reported back with its plots
    /// (`None`: cancelled or failed) after `ms`. They are shown only if no
    /// request came after it. True if a request is waiting: make it again.
    pub fn finish(&mut self, request: u64, plots: Option<Vec<EquationPlot>>, ms: f64) -> bool {
        let Some(job) = self.running.take_if(|j| j.request == request) else {
            return false;
        };
        if request == self.latest
            && let Some(plots) = plots
        {
            self.install(plots, ms, job.weight);
        }
        std::mem::take(&mut self.again)
    }
}

/// Wheel units per scroll "line" (GTK reports discrete steps as ±1).
trait WheelLine {
    const WHEEL_DELTA_LINE: f64;
}
impl WheelLine for Viewport {
    const WHEEL_DELTA_LINE: f64 = graphing::viewport::WHEEL_DELTA;
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphing::plot::Plot;

    /// A row too long to plot inline with no plot before it to go by.
    fn heavy() -> String {
        let terms: Vec<String> = (1..=30).map(|k| format!("sin({k}x)")).collect();
        format!("y={}", terms.join("+"))
    }

    fn geometry(plots: &[EquationPlot]) -> Vec<(EquationId, &Plot)> {
        plots.iter().map(|p| (p.id, &p.plot)).collect()
    }

    /// R12-M-06: a heavy plot goes to a worker; a quick edit while it runs
    /// is plotted inline and cancels it; the worker's report, arriving
    /// after (it finished before it saw the flag), is dropped: it plotted
    /// the old row.
    #[test]
    fn a_stale_worker_never_replaces_a_newer_inline_plot() {
        let vp = Viewport::default_for_size(760.0, 700.0);
        let mut graph = Graph::new();
        let id = graph.add_equation(&heavy());
        let mut jobs = PlotJobs::default();
        let Plan::Worker(job, cancel) = jobs.request(graph.plot_weight()) else {
            panic!("a heavy first plot is a worker's");
        };
        let old = graph.clone();

        graph.set_equation_text(id, "y=x");
        assert!(matches!(jobs.request(graph.plot_weight()), Plan::Inline));
        let new = graph.plot_parallel(&vp);
        jobs.install(new.clone(), 1.0, graph.plot_weight());
        assert!(cancel.load(Ordering::Relaxed), "the old job is cancelled");

        let stale = old
            .plot_parallel_cancellable(&vp, &AtomicBool::new(false))
            .expect("not cancelled");
        assert_ne!(geometry(&stale), geometry(&new));
        assert!(!jobs.finish(job, Some(stale), 900.0), "nothing waits");
        assert_eq!(geometry(&jobs.plots), geometry(&new));
        // The prediction is still the inline plot's.
        assert!(matches!(jobs.request(graph.plot_weight()), Plan::Inline));
    }

    /// A request made while a job runs cancels it and waits; the job's
    /// report (stale) is dropped and asks for the request again, which
    /// then runs; a report from no running job changes nothing.
    #[test]
    fn a_request_during_a_job_is_made_again_after_it() {
        let vp = Viewport::default_for_size(760.0, 700.0);
        let mut graph = Graph::new();
        graph.add_equation(&heavy());
        let weight = graph.plot_weight();
        let mut jobs = PlotJobs::default();
        let Plan::Worker(first, cancel) = jobs.request(weight) else {
            panic!("a heavy first plot is a worker's");
        };
        assert!(matches!(jobs.request(weight), Plan::Wait));
        assert!(cancel.load(Ordering::Relaxed));
        let plots = graph.plot_parallel(&vp);
        assert!(jobs.finish(first, Some(plots.clone()), 900.0));
        assert!(jobs.plots.is_empty(), "a newer request came after it");

        let Plan::Worker(second, cancel) = jobs.request(weight) else {
            panic!("the waiting request runs");
        };
        assert!(second > first && !cancel.load(Ordering::Relaxed));
        assert!(!jobs.finish(second, Some(plots.clone()), 900.0));
        assert_eq!(geometry(&jobs.plots), geometry(&plots));
        assert!(!jobs.finish(second, Some(Vec::new()), 1.0));
        assert!(!jobs.finish(first, Some(Vec::new()), 1.0));
        assert_eq!(geometry(&jobs.plots), geometry(&plots));
    }
}
