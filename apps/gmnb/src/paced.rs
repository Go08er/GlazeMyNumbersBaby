//! What Settings paces, so that neither a theme nor a slider holds the
//! window (docs/theming.md, "Limits, and what happens when a theme
//! fails"):
//!
//! - [`Turns`]: the theme's `visible` and `preview` calls, in passes that
//!   share one budget ([`Limits::settings`]). The first pass runs at once;
//!   what doesn't fit waits for the next, an idle callback, after the
//!   window has drawn and taken its input. Whether rows are shown is asked
//!   first, then the tiles, each in order.
//! - [`OnceAFrame`]: the latest value of something that changes faster
//!   than the screen (the Background opacity being dragged), applied once
//!   a frame.
//!
//! Nothing here runs while Settings is closed. A pass is queued only by
//! Settings itself, and [`Turns::stop`] removes it when Settings closes; a
//! value waiting for its frame is applied at once when its widget goes
//! off screen ([`OnceAFrame::flush`]).
//!
//! [`Limits::settings`]: gmnb_theme::Limits::settings

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use crate::theme::Hub;

/// One call Settings has to make of the theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// Whether option row `n` is shown: its `visible`.
    Visible(usize),
    /// Tile `n`'s picture: its option's `preview`.
    Tile(usize),
}

/// What is still to be asked, each once: rows first (whether they are
/// shown moves everything under them), then tiles, each in the order
/// asked.
#[derive(Debug, Default)]
pub struct Queue {
    visible: VecDeque<usize>,
    tiles: VecDeque<usize>,
}

impl Queue {
    /// Asks for `ask`, unless it is waiting already.
    pub fn ask(&mut self, ask: Ask) {
        let (list, n) = match ask {
            Ask::Visible(n) => (&mut self.visible, n),
            Ask::Tile(n) => (&mut self.tiles, n),
        };
        if !list.contains(&n) {
            list.push_back(n);
        }
    }

    /// The next thing to ask.
    pub fn next(&mut self) -> Option<Ask> {
        self.visible
            .pop_front()
            .map(Ask::Visible)
            .or_else(|| self.tiles.pop_front().map(Ask::Tile))
    }

    pub fn is_empty(&self) -> bool {
        self.visible.is_empty() && self.tiles.is_empty()
    }

    pub fn len(&self) -> usize {
        self.visible.len() + self.tiles.len()
    }

    pub fn clear(&mut self) {
        self.visible.clear();
        self.tiles.clear();
    }
}

/// How a pass went: what it asked, and the processor time it took.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pass {
    pub asked: u32,
    pub used: Duration,
}

/// Runs one pass of `queue`: `answer` for each thing waiting, in order,
/// until `over` says the pass's budget is used (checked before each call,
/// so the first always runs and a call never starts once the budget is
/// gone; each keeps its own limit). `answer` may ask for more.
pub fn pass(queue: &RefCell<Queue>, over: impl Fn() -> bool, mut answer: impl FnMut(Ask)) -> u32 {
    let mut asked = 0;
    loop {
        if asked > 0 && over() {
            break;
        }
        // (Not borrowed while answering: an answer may ask for more.)
        let Some(next) = queue.borrow_mut().next() else {
            break;
        };
        answer(next);
        asked += 1;
    }
    asked
}

/// Settings' calls of the theme, in passes of [`Limits::settings`].
///
/// [`Limits::settings`]: gmnb_theme::Limits::settings
pub struct Turns {
    queue: RefCell<Queue>,
    /// The idle callback the next pass runs in, while one is waiting.
    idle: RefCell<Option<glib::SourceId>>,
    budget: Duration,
}

impl Turns {
    pub fn new() -> Rc<Turns> {
        Rc::new(Turns {
            queue: RefCell::default(),
            idle: RefCell::default(),
            budget: gmnb_theme::Limits::default().settings,
        })
    }

    /// Asks for `ask` in a pass to come ([`Turns::go`]).
    pub fn ask(&self, ask: Ask) {
        self.queue.borrow_mut().ask(ask);
    }

    /// Forgets everything waiting, and the pass it was waiting for.
    pub fn stop(&self) {
        self.queue.borrow_mut().clear();
        if let Some(id) = self.idle.borrow_mut().take() {
            id.remove();
        }
    }

    /// One pass now, and if anything is left, the rest in passes to come,
    /// each in an idle callback of its own. `answer` makes one call.
    /// While a pass is waiting already, what was asked waits for it:
    /// however many changes come in one turn of the main loop, they cost
    /// one pass's budget there.
    pub fn go(self: &Rc<Self>, answer: Rc<dyn Fn(Ask)>) {
        if self.idle.borrow().is_some() || !self.run(&*answer) {
            return;
        }
        let weak = Rc::downgrade(self);
        let id = glib::idle_add_local(move || {
            let Some(turns) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if turns.run(&*answer) {
                return glib::ControlFlow::Continue;
            }
            // Done: the source goes as this returns.
            turns.idle.borrow_mut().take();
            glib::ControlFlow::Break
        });
        *self.idle.borrow_mut() = Some(id);
    }

    /// One pass: whether anything is left.
    fn run(&self, answer: &dyn Fn(Ask)) -> bool {
        if self.queue.borrow().is_empty() {
            return false;
        }
        let span = gmnb_theme::Span::new(self.budget);
        let asked = pass(&self.queue, || span.is_over(), answer);
        let left = self.queue.borrow().len();
        if let Some(hub) = Hub::current() {
            hub.settings_pass(
                Pass {
                    asked,
                    used: span.used(),
                },
                left == 0,
            );
        }
        left > 0
    }
}

/// The latest value of something that changes faster than the screen
/// shows it, applied once a frame: on the frame clock of the widget it
/// belongs to, at the next frame after it changed.
pub struct OnceAFrame<T: Copy + 'static> {
    latest: Cell<Option<T>>,
    tick: RefCell<Option<gtk::TickCallbackId>>,
    apply: Box<dyn Fn(T)>,
}

impl<T: Copy + 'static> OnceAFrame<T> {
    /// For values of `widget`, which `apply` puts into effect. Off screen,
    /// a value waiting for its frame is applied at once.
    pub fn new(widget: &impl IsA<gtk::Widget>, apply: impl Fn(T) + 'static) -> Rc<Self> {
        let this = Rc::new(OnceAFrame {
            latest: Cell::new(None),
            tick: RefCell::default(),
            apply: Box::new(apply),
        });
        let weak = Rc::downgrade(&this);
        widget.connect_unmap(move |_| {
            if let Some(this) = weak.upgrade() {
                this.flush();
            }
        });
        this
    }

    /// `value` is the latest: it is applied at the next frame, with
    /// whatever else came before then dropped.
    pub fn set(self: &Rc<Self>, widget: &impl IsA<gtk::Widget>, value: T) {
        self.latest.set(Some(value));
        if self.tick.borrow().is_some() {
            return;
        }
        if !widget.is_mapped() {
            // No frames to wait for.
            self.flush();
            return;
        }
        let weak = Rc::downgrade(self);
        let id = widget.add_tick_callback(move |_, _| {
            if let Some(this) = weak.upgrade() {
                // The callback goes as this returns.
                this.tick.borrow_mut().take();
                if let Some(value) = this.latest.take() {
                    (this.apply)(value);
                }
            }
            glib::ControlFlow::Break
        });
        *self.tick.borrow_mut() = Some(id);
    }

    /// Applies at once what is waiting for its frame, if anything is.
    pub fn flush(&self) {
        if let Some(id) = self.tick.borrow_mut().take() {
            id.remove();
        }
        if let Some(value) = self.latest.take() {
            (self.apply)(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_asked_before_tiles_and_nothing_twice() {
        let mut q = Queue::default();
        for ask in [
            Ask::Tile(0),
            Ask::Tile(1),
            Ask::Visible(3),
            Ask::Tile(0),
            Ask::Visible(1),
            Ask::Visible(3),
        ] {
            q.ask(ask);
        }
        assert_eq!(q.len(), 4);
        let order: Vec<Ask> = std::iter::from_fn(|| q.next()).collect();
        assert_eq!(
            order,
            [Ask::Visible(3), Ask::Visible(1), Ask::Tile(0), Ask::Tile(1)]
        );
        assert!(q.is_empty());
    }

    /// A pass stops starting calls once its budget is used (the first
    /// always runs); the rest wait, in order, for the next pass, and
    /// what was asked during a pass is answered too.
    #[test]
    fn a_pass_stops_at_its_budget_and_the_next_carries_on() {
        let queue = RefCell::new(Queue::default());
        for n in 0..10 {
            queue.borrow_mut().ask(Ask::Tile(n));
        }
        // Each call "costs" 4 of a budget of 10: the third starts at 8.
        let spent = Cell::new(0);
        let answered = RefCell::new(Vec::new());
        let mut passes = Vec::new();
        while !queue.borrow().is_empty() {
            spent.set(0);
            let asked = pass(
                &queue,
                || spent.get() >= 10,
                |ask| {
                    spent.set(spent.get() + 4);
                    answered.borrow_mut().push(ask);
                    // The second tile, answered, asks for row 7.
                    if ask == Ask::Tile(1) {
                        queue.borrow_mut().ask(Ask::Visible(7));
                    }
                },
            );
            passes.push(asked);
        }
        assert_eq!(passes, [3, 3, 3, 2]);
        let answered = answered.into_inner();
        // (Asked during the first pass, it is that pass's third call.)
        assert_eq!(answered[2], Ask::Visible(7));
        let tiles: Vec<usize> = answered
            .iter()
            .filter_map(|a| match a {
                Ask::Tile(n) => Some(*n),
                Ask::Visible(_) => None,
            })
            .collect();
        assert_eq!(tiles, (0..10).collect::<Vec<_>>());
    }

    /// A call slower than the whole budget still runs, one a pass: the
    /// queue is never stuck behind it.
    #[test]
    fn a_call_slower_than_the_budget_runs_one_a_pass() {
        let queue = RefCell::new(Queue::default());
        for n in 0..3 {
            queue.borrow_mut().ask(Ask::Tile(n));
        }
        let mut passes = Vec::new();
        while !queue.borrow().is_empty() {
            let spent = Cell::new(0);
            passes.push(pass(&queue, || spent.get() >= 10, |_| spent.set(25)));
        }
        assert_eq!(passes, [1, 1, 1]);
    }
}
