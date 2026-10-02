//! `Aurora` — a bin container that paints a slowly drifting aurora of soft
//! colour blobs, a film-grain layer, and transient "pulses" (soft blooms that
//! ripple out from wherever the user just pressed something), then its child.
//!
//! Battery etiquette: the drift only animates while the toplevel is active and
//! animations are enabled, and it redraws at ~30 fps (blobs move slowly); a
//! pulse temporarily bumps it to full frame rate.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

use crate::theme::{Scheme, rgba};

/// Drift redraw interval: blobs move ~5 px/s, so ~15 fps is visually smooth.
const IDLE_FRAME: Duration = Duration::from_millis(66);
/// Stop drifting after this long without interaction (resumes on activity).
const SETTLE_AFTER: Duration = Duration::from_secs(45);
const PULSE_LIFE: f32 = 1.15;
const GRAIN: i32 = 192;

#[derive(Clone, Copy)]
pub struct Pulse {
    x: f32,
    y: f32,
    color: [f32; 3],
    strength: f32,
    born: Instant,
}

mod imp {
    use super::*;

    pub struct Aurora {
        pub scheme: Cell<Option<Scheme>>,
        pub epoch: Cell<Instant>,
        pub frozen_t: Cell<f32>,
        pub animated: Cell<bool>,
        pub running: Cell<bool>,
        pub last_draw: Cell<Instant>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
        pub drift: RefCell<Option<glib::SourceId>>,
        pub pulses: RefCell<Vec<Pulse>>,
        pub grain: RefCell<Option<gdk::Texture>>,
        pub intensity: Cell<f32>,
        pub last_activity: Cell<Instant>,
        pub settled_at: Cell<f32>,
    }

    impl Default for Aurora {
        fn default() -> Self {
            Self {
                scheme: Cell::new(None),
                epoch: Cell::new(Instant::now()),
                frozen_t: Cell::new(37.0),
                animated: Cell::new(true),
                running: Cell::new(false),
                last_draw: Cell::new(Instant::now()),
                tick: RefCell::new(None),
                drift: RefCell::new(None),
                pulses: RefCell::new(Vec::new()),
                grain: RefCell::new(None),
                intensity: Cell::new(1.0),
                last_activity: Cell::new(Instant::now()),
                settled_at: Cell::new(0.0),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Aurora {
        const NAME: &'static str = "GmnbAurora";
        type Type = super::Aurora;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BinLayout>();
            klass.set_css_name("aurora");
        }
    }

    impl ObjectImpl for Aurora {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_overflow(gtk::Overflow::Hidden);
            obj.connect_realize(|w| w.sync_running());
            obj.connect_unrealize(|w| w.stop());
            obj.connect_root_notify(|w| w.watch_toplevel());
            let motion = gtk::EventControllerMotion::new();
            motion.set_propagation_phase(gtk::PropagationPhase::Capture);
            let weak = obj.downgrade();
            motion.connect_motion(move |_, _, _| {
                if let Some(w) = weak.upgrade() {
                    w.poke();
                }
            });
            obj.add_controller(motion);
            if let Some(settings) = gtk::Settings::default() {
                let weak = obj.downgrade();
                settings.connect_gtk_enable_animations_notify(move |_| {
                    if let Some(w) = weak.upgrade() {
                        w.sync_running();
                    }
                });
            }
        }

        fn dispose(&self) {
            if let Some(id) = self.tick.take() {
                id.remove();
            }
            if let Some(id) = self.drift.take() {
                id.remove();
            }
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Aurora {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let w = obj.width() as f32;
            let h = obj.height() as f32;
            if w > 0.0
                && h > 0.0
                && let Some(scheme) = self.scheme.get()
            {
                obj.paint_background(snapshot, &scheme, w, h);
            }
            self.parent_snapshot(snapshot);
        }
    }
}

glib::wrapper! {
    pub struct Aurora(ObjectSubclass<imp::Aurora>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Aurora {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Aurora {
    pub fn set_child(&self, child: Option<&impl IsA<gtk::Widget>>) {
        while let Some(old) = self.first_child() {
            old.unparent();
        }
        if let Some(child) = child {
            child.set_parent(self);
        }
    }

    pub fn set_scheme(&self, scheme: Scheme) {
        self.imp().scheme.set(Some(scheme));
        self.imp().grain.replace(None);
        self.queue_draw();
    }

    /// Enable/disable the slow drift (pulses still play when enabled globally).
    pub fn set_animated(&self, animated: bool) {
        let imp = self.imp();
        if imp.animated.get() != animated {
            imp.frozen_t.set(self.time());
            imp.animated.set(animated);
            if animated {
                // Resume from where we froze instead of jumping.
                let frozen = Duration::from_secs_f32(imp.frozen_t.get());
                imp.epoch.set(
                    Instant::now()
                        .checked_sub(frozen)
                        .unwrap_or_else(Instant::now),
                );
            }
            self.sync_running();
            self.queue_draw();
        }
    }

    /// Strength of the blobs, 0..=1 (lets dense pages calm the background).
    pub fn set_intensity(&self, v: f32) {
        self.imp().intensity.set(v.clamp(0.0, 1.0));
        self.queue_draw();
    }

    /// Note user activity: wakes the drift if it had settled.
    pub fn poke(&self) {
        let imp = self.imp();
        let was_settled = imp.last_activity.get().elapsed() >= SETTLE_AFTER;
        imp.last_activity.set(Instant::now());
        if was_settled {
            // Continue from the frozen frame rather than jumping ahead.
            let t = imp.settled_at.get();
            imp.epoch.set(
                Instant::now()
                    .checked_sub(Duration::from_secs_f32(t))
                    .unwrap_or_else(Instant::now),
            );
            self.sync_running();
        }
    }

    /// Bloom a soft pulse of colour at (x, y) in this widget's coordinates.
    pub fn pulse(&self, x: f32, y: f32, color: [f32; 3], strength: f32) {
        self.poke();
        if !animations_enabled() {
            return;
        }
        let mut pulses = self.imp().pulses.borrow_mut();
        if pulses.len() > 12 {
            pulses.remove(0);
        }
        pulses.push(Pulse {
            x,
            y,
            color,
            strength,
            born: Instant::now(),
        });
        drop(pulses);
        self.sync_pulses();
    }

    fn time(&self) -> f32 {
        let imp = self.imp();
        if !(imp.animated.get() && animations_enabled()) {
            imp.frozen_t.get()
        } else if imp.last_activity.get().elapsed() >= SETTLE_AFTER {
            imp.settled_at.get()
        } else {
            let t = imp.epoch.get().elapsed().as_secs_f32();
            imp.settled_at.set(t);
            t
        }
    }

    fn watch_toplevel(&self) {
        if let Some(win) = self.root().and_downcast::<gtk::Window>() {
            let weak = self.downgrade();
            win.connect_is_active_notify(move |win| {
                if let Some(w) = weak.upgrade() {
                    if win.is_active() {
                        w.poke();
                    }
                    w.sync_running();
                }
            });
        }
        self.sync_running();
    }

    fn wants_ticks(&self) -> bool {
        let imp = self.imp();
        if !self.is_realized() {
            return false;
        }
        let active = self
            .root()
            .and_downcast::<gtk::Window>()
            .is_some_and(|w| w.is_active());
        let recent = imp.last_activity.get().elapsed() < SETTLE_AFTER;
        imp.animated.get() && animations_enabled() && active && recent
    }

    fn sync_running(&self) {
        let imp = self.imp();
        let want = self.wants_ticks();
        if want && !imp.running.get() {
            imp.running.set(true);
            // Slow drift: a plain timer. A frame-clock tick callback would keep
            // GTK cycling update/layout at the monitor's refresh rate.
            let weak = self.downgrade();
            let id = glib::timeout_add_local(IDLE_FRAME, move || {
                let Some(w) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if w.wants_ticks() {
                    w.queue_draw();
                    glib::ControlFlow::Continue
                } else {
                    w.imp().running.set(false);
                    w.imp().drift.replace(None);
                    w.queue_draw();
                    glib::ControlFlow::Break
                }
            });
            imp.drift.replace(Some(id));
        } else if !want && imp.running.get() {
            self.stop();
        }
        self.sync_pulses();
    }

    /// Pulses need full frame rate, but only while one is visible.
    fn sync_pulses(&self) {
        let imp = self.imp();
        if imp.pulses.borrow().is_empty() || imp.tick.borrow().is_some() {
            return;
        }
        let id = self.add_tick_callback(|w, _clock| {
            let imp = w.imp();
            imp.pulses
                .borrow_mut()
                .retain(|p| p.born.elapsed().as_secs_f32() < PULSE_LIFE);
            w.queue_draw();
            if imp.pulses.borrow().is_empty() {
                imp.tick.replace(None);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        imp.tick.replace(Some(id));
    }

    fn stop(&self) {
        let imp = self.imp();
        if let Some(id) = imp.drift.take() {
            id.remove();
        }
        imp.running.set(false);
    }

    fn grain_texture(&self, dark: bool) -> gdk::Texture {
        if let Some(t) = self.imp().grain.borrow().as_ref() {
            return t.clone();
        }
        // xorshift noise; premultiplied grey speckles at very low alpha.
        let mut state: u32 = 0x9e37_79b9;
        let mut px = Vec::with_capacity((GRAIN * GRAIN * 4) as usize);
        for _ in 0..GRAIN * GRAIN {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let n = (state & 0xff) as f32 / 255.0;
            let a = if dark { n * 0.07 } else { n * 0.05 };
            let v = if dark { 1.0 } else { 0.0 };
            let c = (v * a * 255.0) as u8;
            px.extend_from_slice(&[c, c, c, (a * 255.0) as u8]);
        }
        let bytes = glib::Bytes::from_owned(px);
        let tex: gdk::Texture = gdk::MemoryTexture::new(
            GRAIN,
            GRAIN,
            gdk::MemoryFormat::R8g8b8a8Premultiplied,
            &bytes,
            (GRAIN * 4) as usize,
        )
        .upcast();
        self.imp().grain.replace(Some(tex.clone()));
        tex
    }

    fn paint_background(&self, s: &gtk::Snapshot, scheme: &Scheme, w: f32, h: f32) {
        let bounds = graphene::Rect::new(0.0, 0.0, w, h);
        s.append_linear_gradient(
            &bounds,
            &graphene::Point::new(0.0, 0.0),
            &graphene::Point::new(w * 0.35, h),
            &[
                gsk::ColorStop::new(0.0, rgba(scheme.base_top, 1.0)),
                gsk::ColorStop::new(1.0, rgba(scheme.base_bottom, 1.0)),
            ],
        );

        let t = self.time();
        let k = self.imp().intensity.get();
        let span = w.max(h);
        // (anchor x, anchor y, drift amp, freq a, freq b, phase, radius factor)
        const BLOBS: [(f32, f32, f32, f32, f32, f32, f32); 4] = [
            (0.10, 0.08, 0.20, 0.071, 0.053, 0.0, 0.62),
            (0.98, 0.34, 0.16, 0.047, 0.067, 1.7, 0.52),
            (0.22, 0.98, 0.18, 0.059, 0.043, 3.1, 0.58),
            (0.88, 0.96, 0.14, 0.038, 0.061, 4.4, 0.46),
        ];
        for (i, (ax, ay, amp, fa, fb, ph, rf)) in BLOBS.iter().enumerate() {
            let cx = (ax + amp * (t * fa + ph).sin()) * w;
            let cy = (ay + amp * 0.8 * (t * fb + ph * 1.3).cos()) * h;
            let breathe = 1.0 + 0.08 * (t * 0.11 + i as f32).sin();
            let r = span * rf * breathe;
            let a = scheme.blob_alpha * k;
            let c = scheme.blobs[i];
            s.append_radial_gradient(
                &bounds,
                &graphene::Point::new(cx, cy),
                r,
                r * 0.82,
                0.0,
                1.0,
                &[
                    gsk::ColorStop::new(0.0, rgba(c, a)),
                    gsk::ColorStop::new(0.3, rgba(c, a * 0.62)),
                    gsk::ColorStop::new(0.62, rgba(c, a * 0.18)),
                    gsk::ColorStop::new(1.0, rgba(c, 0.0)),
                ],
            );
        }

        for p in self.imp().pulses.borrow().iter() {
            let age = p.born.elapsed().as_secs_f32() / PULSE_LIFE;
            if age >= 1.0 {
                continue;
            }
            let ease = 1.0 - (1.0 - age).powi(3);
            let r = 40.0 + ease * 340.0;
            let a = (1.0 - age).powf(1.6) * 0.30 * p.strength;
            s.append_radial_gradient(
                &bounds,
                &graphene::Point::new(p.x, p.y),
                r,
                r,
                0.0,
                1.0,
                &[
                    gsk::ColorStop::new(0.0, rgba(p.color, a * 0.6)),
                    gsk::ColorStop::new(0.55, rgba(p.color, a)),
                    gsk::ColorStop::new(1.0, rgba(p.color, 0.0)),
                ],
            );
        }

        // Vignette: depth in the dark, a soft halo in the light.
        let vr = w.hypot(h) * 0.62;
        let edge = if scheme.dark {
            rgba([0.0, 0.0, 0.0], 0.38)
        } else {
            rgba([1.0, 1.0, 1.0], 0.30)
        };
        let mut clear = edge;
        clear.set_alpha(0.0);
        s.append_radial_gradient(
            &bounds,
            &graphene::Point::new(w * 0.5, h * 0.42),
            vr,
            vr,
            0.0,
            1.0,
            &[
                gsk::ColorStop::new(0.55, clear),
                gsk::ColorStop::new(1.0, edge),
            ],
        );

        // Film grain kills gradient banding and makes it feel like a material.
        let tex = self.grain_texture(scheme.dark);
        let tile = graphene::Rect::new(0.0, 0.0, GRAIN as f32, GRAIN as f32);
        s.push_repeat(&bounds, Some(&tile));
        s.append_texture(&tex, &tile);
        s.pop();
    }
}

pub use super::animations_enabled;
