//! Dev hook: what theming costs, measured in the real window
//! (`GMNB_BENCH`). Like `GMNB_SCREENSHOT`, it is for the project's own
//! scripts.
//!
//! `GMNB_BENCH="themes=base,aurora,mono,base;opacity"` runs, a moment
//! after the window is up:
//!
//! - `themes=…`: each theme in turn, timing the switch (loading the file,
//!   `style`, writing the providers) and from the switch to the end of the
//!   next painted frame;
//! - `opacity`: the Background opacity from 1 down to 0.1 and back in
//!   steps of 0.05, as the slider drags it, timing each step the same way,
//!   then the fold back into the main provider.
//!
//! It prints its figures and the hub's counters to stderr, then quits.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::glib;
use gtk::prelude::*;

use crate::window::Window;

enum Step {
    Theme(String),
    Opacity(f32),
    Settle,
}

struct Bench {
    win: Rc<Window>,
    steps: RefCell<VecDeque<Step>>,
    /// (the change itself, until painted), in milliseconds.
    opacity: RefCell<Vec<(f64, f64)>>,
    handler: RefCell<Option<glib::SignalHandlerId>>,
    started: Cell<Option<(Instant, f64)>>,
}

pub fn run(win: &Rc<Window>, spec: &str) {
    let mut steps = VecDeque::new();
    for part in spec.split(';') {
        if let Some(ids) = part.strip_prefix("themes=") {
            steps.extend(ids.split(',').map(|id| Step::Theme(id.trim().to_string())));
        } else if part.trim() == "opacity" {
            let down = (1..=18).map(|i| 1.0 - i as f32 * 0.05);
            let up = (0..18).map(|i| 0.15 + i as f32 * 0.05);
            steps.extend(down.chain(up).map(Step::Opacity));
            steps.push_back(Step::Settle);
        }
    }
    let bench = Rc::new(Bench {
        win: win.clone(),
        steps: RefCell::new(steps),
        opacity: RefCell::default(),
        handler: RefCell::new(None),
        started: Cell::new(None),
    });
    glib::timeout_add_local_once(Duration::from_millis(1500), move || bench.next());
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

impl Bench {
    fn next(self: &Rc<Self>) {
        let Some(step) = self.steps.borrow_mut().pop_front() else {
            return self.finish();
        };
        let ctx = self.win.ctx();
        let win = self.win.widget();
        let t0 = Instant::now();
        let label = match &step {
            Step::Theme(id) => {
                ctx.hub.set_theme(id);
                format!("theme {id}")
            }
            Step::Opacity(a) => {
                crate::window::apply_backdrop(&win, &ctx.backdrop, *a);
                ctx.hub.set_opacity(*a, true);
                String::new()
            }
            Step::Settle => {
                ctx.hub.settle();
                "fold the live provider back".to_string()
            }
        };
        let change = ms(t0.elapsed());
        self.started.set(Some((t0, change)));
        // The end of the next frame's paint. (A step that changed nothing
        // still gets a frame.)
        win.queue_draw();
        let Some(clock) = win.frame_clock() else {
            return self.finish();
        };
        let this = self.clone();
        let is_opacity = matches!(step, Step::Opacity(_));
        let id = clock.connect_after_paint(move |clock| {
            let Some((t0, change)) = this.started.take() else {
                return;
            };
            if let Some(id) = this.handler.take() {
                clock.disconnect(id);
            }
            let painted = ms(t0.elapsed());
            if is_opacity {
                this.opacity.borrow_mut().push((change, painted));
            } else {
                eprintln!("bench: {label}: {change:.2} ms, painted after {painted:.2} ms");
            }
            let this = this.clone();
            let pause = if is_opacity { 30 } else { 400 };
            glib::timeout_add_local_once(Duration::from_millis(pause), move || this.next());
        });
        self.handler.replace(Some(id));
    }

    fn finish(&self) {
        let steps = self.opacity.borrow();
        if !steps.is_empty() {
            let n = steps.len() as f64;
            let mean = |f: fn(&(f64, f64)) -> f64| steps.iter().map(f).sum::<f64>() / n;
            let max = |f: fn(&(f64, f64)) -> f64| steps.iter().map(f).fold(0.0, f64::max);
            eprintln!(
                "bench: opacity: {} steps; a step {:.3} ms (worst {:.3}); painted after {:.2} ms (worst {:.2})",
                steps.len(),
                mean(|s| s.0),
                max(|s| s.0),
                mean(|s| s.1),
                max(|s| s.1),
            );
        }
        eprintln!("bench: {:?}", self.win.ctx().hub.stats());
        if let Some(app) = self.win.widget().application() {
            app.quit();
        }
    }
}
