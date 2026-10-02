//! `GraphView` — renders a `graphing::Graph` with GSK paths.
//!
//! Glass canvas, fading grid with axis labels, neon curves (soft glow +
//! crisp core) that draw themselves in when an equation appears, shaded
//! inequality regions, a snapping trace cursor with a value bubble, and
//! animated zoom/reset. Drag to pan, scroll or pinch to zoom.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use graphing::equation::LineStyle;
use graphing::graph::EquationPlot;
use graphing::grid::Grid;
use graphing::trace::TracePoint;
use graphing::{EquationId, Graph, Viewport};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib, graphene, gsk, pango};

use super::animations_enabled;
use super::display::display_font;
use crate::theme::{Scheme, rgba};

const RADIUS: f32 = 20.0;
const ZOOM_ANIM: f32 = 0.24;
const DRAW_IN: f32 = 0.9;
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
        pub plots: RefCell<Vec<EquationPlot>>,
        pub dirty: Cell<bool>,
        pub colors: RefCell<HashMap<EquationId, [f32; 3]>>,
        pub scheme: Cell<Option<Scheme>>,
        pub line_width: Cell<f64>,
        pub trace_on: Cell<bool>,
        pub pointer: Cell<Option<(f64, f64)>>,
        pub trace: RefCell<Option<(EquationId, TracePoint)>>,
        pub draw_in: RefCell<HashMap<EquationId, Instant>>,
        pub drag_origin: Cell<Option<Viewport>>,
        pub zoom_origin: Cell<Option<(Viewport, f64, f64)>>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
        pub on_viewport: RefCell<Vec<ViewportFn>>,
        pub fitted: Cell<bool>,
        /// How long the last plot took.
        pub plot_ms: Cell<f64>,
        /// A worker plot is running / another was requested meanwhile.
        pub plot_busy: Cell<bool>,
        pub plot_again: Cell<bool>,
    }

    impl Default for GraphView {
        fn default() -> Self {
            Self {
                graph: RefCell::new(None),
                vp: Cell::new(None),
                anim: Cell::new(None),
                plots: RefCell::default(),
                dirty: Cell::new(true),
                colors: RefCell::default(),
                scheme: Cell::new(None),
                line_width: Cell::new(graphing::graph::DEFAULT_LINE_WIDTH),
                trace_on: Cell::new(false),
                pointer: Cell::new(None),
                trace: RefCell::new(None),
                draw_in: RefCell::default(),
                drag_origin: Cell::new(None),
                zoom_origin: Cell::new(None),
                tick: RefCell::new(None),
                on_viewport: RefCell::default(),
                fitted: Cell::new(false),
                plot_ms: Cell::new(0.0),
                plot_busy: Cell::new(false),
                plot_again: Cell::new(false),
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

    pub fn set_scheme(&self, scheme: Scheme) {
        self.imp().scheme.set(Some(scheme));
        self.queue_draw();
    }

    pub fn set_color(&self, id: EquationId, color: [f32; 3]) {
        self.imp().colors.borrow_mut().insert(id, color);
        self.queue_draw();
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
        if animations_enabled() {
            self.imp().draw_in.borrow_mut().insert(id, Instant::now());
            self.ensure_ticking();
        }
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
        if animations_enabled() {
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
                let p = (start.elapsed().as_secs_f32() / ZOOM_ANIM).min(1.0);
                let e = 1.0 - (1.0 - p as f64).powi(3);
                g.set_vp(lerp_vp(&from, &to, e));
                if p >= 1.0 {
                    imp.anim.set(None);
                }
            }
            imp.draw_in
                .borrow_mut()
                .retain(|_, t| t.elapsed().as_secs_f32() < DRAW_IN);
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
        if imp.plot_ms.get() < INLINE_PLOT_MS {
            let started = Instant::now();
            let plots = graph.borrow().plot_parallel(&vp);
            imp.plot_ms.set(started.elapsed().as_secs_f64() * 1e3);
            imp.plots.replace(plots);
            return;
        }
        // Heavy graph: plot on a worker and keep drawing the last result
        // (curves are in graph coordinates, so they still line up while
        // panning). One job at a time; requests made meanwhile coalesce into
        // a single re-plot of the latest state.
        if imp.plot_busy.replace(true) {
            imp.plot_again.set(true);
            return;
        }
        let graph = graph.borrow().clone();
        let weak = self.downgrade();
        glib::spawn_future_local(async move {
            let started = Instant::now();
            let plots = gio::spawn_blocking(move || graph.plot_parallel(&vp)).await;
            let Some(this) = weak.upgrade() else { return };
            let imp = this.imp();
            imp.plot_busy.set(false);
            imp.plot_ms.set(started.elapsed().as_secs_f64() * 1e3);
            if let Ok(plots) = plots {
                imp.plots.replace(plots);
            }
            if imp.plot_again.replace(false) {
                imp.dirty.set(true);
            }
            this.update_trace();
            this.queue_draw();
        });
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
            &imp.plots.borrow(),
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

    fn label_layout(&self, text: &str, size: f64) -> pango::Layout {
        let layout = self.create_pango_layout(Some(text));
        layout.set_font_description(Some(&display_font(self, size as f32, 400)));
        layout
    }

    fn draw(&self, s: &gtk::Snapshot) {
        let imp = self.imp();
        let Some(scheme) = imp.scheme.get() else {
            return;
        };
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
        let rr = gsk::RoundedRect::from_rect(bounds, RADIUS);
        let fg = scheme.fg;

        // Canvas.
        s.push_rounded_clip(&rr);
        {
            let (top, bottom) = if scheme.dark {
                (0.20, 0.30)
            } else {
                (0.55, 0.42)
            };
            let tint = if scheme.dark {
                [0.0, 0.0, 0.0]
            } else {
                [1.0, 1.0, 1.0]
            };
            s.append_linear_gradient(
                &bounds,
                &graphene::Point::new(0.0, 0.0),
                &graphene::Point::new(0.0, h),
                &[
                    gsk::ColorStop::new(0.0, rgba(tint, top)),
                    gsk::ColorStop::new(1.0, rgba(tint, bottom)),
                ],
            );
        }

        // Grid.
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
            s.append_stroke(&b.to_path(), &gsk::Stroke::new(width), &color);
        };
        let minor = rgba(fg, if scheme.dark { 0.045 } else { 0.06 });
        let major = rgba(fg, if scheme.dark { 0.10 } else { 0.12 });
        for x in &grid.x.minor {
            let sx = to_sx(*x);
            line(s, sx, 0.0, sx, h, minor, 1.0);
        }
        for y in &grid.y.minor {
            let sy = to_sy(*y);
            line(s, 0.0, sy, w, sy, minor, 1.0);
        }
        for t in &grid.x.major {
            let sx = to_sx(t.value);
            line(s, sx, 0.0, sx, h, major, 1.0);
        }
        for t in &grid.y.major {
            let sy = to_sy(t.value);
            line(s, 0.0, sy, w, sy, major, 1.0);
        }
        let axis = rgba(fg, 0.55);
        let (ax, ay) = (to_sx(0.0), to_sy(0.0));
        if (0.0..=w).contains(&ax) {
            line(s, ax, 0.0, ax, h, axis, 1.4);
        }
        if (0.0..=h).contains(&ay) {
            line(s, 0.0, ay, w, ay, axis, 1.4);
        }
        // Labels hug the axes, or the edges when an axis is off-screen.
        let label_color = rgba(fg, 0.62);
        let ly = ay.clamp(4.0, h - 18.0);
        for t in &grid.x.major {
            if t.value.abs() < 1e-12 {
                continue;
            }
            let layout = self.label_layout(&t.label, 11.0);
            let (lw, _) = layout.pixel_size();
            let sx = to_sx(t.value) - lw as f32 / 2.0;
            if sx < 4.0 || sx + lw as f32 > w - 4.0 {
                continue;
            }
            s.save();
            s.translate(&graphene::Point::new(sx, ly + 3.0));
            s.append_layout(&layout, &label_color);
            s.restore();
        }
        let lx = ax.clamp(4.0, w - 40.0);
        for t in &grid.y.major {
            if t.value.abs() < 1e-12 {
                continue;
            }
            let layout = self.label_layout(&t.label, 11.0);
            let (lw, lh) = layout.pixel_size();
            let x = if lx + 6.0 + lw as f32 > w {
                lx - 6.0 - lw as f32
            } else {
                lx + 6.0
            };
            let y = to_sy(t.value) - lh as f32 / 2.0;
            if y < 4.0 || y + lh as f32 > h - 4.0 {
                continue;
            }
            s.save();
            s.translate(&graphene::Point::new(x, to_sy(t.value) - lh as f32 / 2.0));
            s.append_layout(&layout, &label_color);
            s.restore();
        }

        // Curves.
        let graph = imp.graph.borrow().clone();
        let lw = imp.line_width.get() as f32;
        for (i, ep) in imp.plots.borrow().iter().enumerate() {
            let color = imp
                .colors
                .borrow()
                .get(&ep.id)
                .copied()
                .unwrap_or(scheme.series[i % scheme.series.len()]);
            let style = graph
                .as_ref()
                .map(|g| g.borrow().line_style(ep.id))
                .unwrap_or_default();
            let draw_p = imp
                .draw_in
                .borrow()
                .get(&ep.id)
                .map(|t| {
                    let p = (t.elapsed().as_secs_f32() / DRAW_IN).min(1.0);
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
                    &rgba(color, 0.18 * draw_p),
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
                let pattern: &[f32] = match (ep.plot.boundary_dashed, style) {
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
            if scheme.dark {
                s.push_blur(6.0);
                s.append_stroke(&path, &make(lw * 3.2), &rgba(color, 0.55));
                s.pop();
            }
            s.append_stroke(&path, &make(lw), &rgba(color, 1.0));
        }

        // Trace.
        if let Some((id, tp)) = imp.trace.borrow().as_ref() {
            let color = imp
                .colors
                .borrow()
                .get(id)
                .copied()
                .unwrap_or(scheme.accent);
            let (sx, sy) = (tp.screen_x as f32, tp.screen_y as f32);
            let hair = rgba(color, 0.45);
            let st = gsk::Stroke::new(1.0);
            st.set_dash(&[4.0, 4.0]);
            for (x0, y0, x1, y1) in [(sx, 0.0, sx, h), (0.0, sy, w, sy)] {
                let b = gsk::PathBuilder::new();
                b.move_to(x0, y0);
                b.line_to(x1, y1);
                s.append_stroke(&b.to_path(), &st, &hair);
            }
            let r = graphing::graph::trace_point_radius(imp.line_width.get()) as f32 + 2.0;
            let dot = gsk::PathBuilder::new();
            dot.add_circle(&graphene::Point::new(sx, sy), r + 3.0);
            s.append_fill(&dot.to_path(), gsk::FillRule::Winding, &rgba(color, 0.25));
            let dot = gsk::PathBuilder::new();
            dot.add_circle(&graphene::Point::new(sx, sy), r);
            s.append_fill(&dot.to_path(), gsk::FillRule::Winding, &rgba(color, 1.0));
            s.append_stroke(
                &dot.to_path(),
                &gsk::Stroke::new(1.5),
                &rgba([1.0, 1.0, 1.0], 0.9),
            );

            let text = graphing::trace::format_trace_value(tp.x, tp.y, vp.precision());
            let layout = self.label_layout(&text, 13.0);
            let (lw2, lh) = layout.pixel_size();
            let (bw, bh) = (lw2 as f32 + 18.0, lh as f32 + 10.0);
            let mut bx = sx + 14.0;
            let mut by = sy - bh - 14.0;
            if bx + bw > w - 6.0 {
                bx = sx - bw - 14.0;
            }
            if by < 6.0 {
                by = sy + 14.0;
            }
            let bubble = gsk::RoundedRect::from_rect(graphene::Rect::new(bx, by, bw, bh), 10.0);
            s.append_outset_shadow(&bubble, &rgba([0.0, 0.0, 0.0], 0.3), 0.0, 4.0, 0.0, 12.0);
            let bg = if scheme.dark {
                rgba(scheme.base_bottom, 0.92)
            } else {
                rgba([1.0, 1.0, 1.0], 0.95)
            };
            s.push_rounded_clip(&bubble);
            s.append_color(&bg, bubble.bounds());
            s.pop();
            s.save();
            s.translate(&graphene::Point::new(bx + 9.0, by + 5.0));
            s.append_layout(&layout, &rgba(fg, 0.95));
            s.restore();
        }
        s.pop();

        // Glass rim.
        let b = gsk::PathBuilder::new();
        b.add_rounded_rect(&gsk::RoundedRect::from_rect(
            bounds.inset_r(0.5, 0.5),
            RADIUS - 0.5,
        ));
        s.append_stroke(
            &b.to_path(),
            &gsk::Stroke::new(1.0),
            &rgba(fg, if scheme.dark { 0.12 } else { 0.18 }),
        );
    }
}

/// Wheel units per scroll "line" (GTK reports discrete steps as ±1).
trait WheelLine {
    const WHEEL_DELTA_LINE: f64;
}
impl WheelLine for Viewport {
    const WHEEL_DELTA_LINE: f64 = graphing::viewport::WHEEL_DELTA;
}
