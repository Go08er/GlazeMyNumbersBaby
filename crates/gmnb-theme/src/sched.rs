//! When a layer is drawn again (docs/theming.md, "Asking for the next
//! frame"): the policy, as a state machine with no clock of its own. The
//! app tells it the time and what the window is doing, and does what it
//! answers: arm a timer, watch the display's frames, or nothing.
//!
//! One [`Clock`] per layer. Around every frame the app calls
//! [`Clock::begin`] and [`Clock::end`]; when the timer or a display frame
//! it armed comes round, [`Clock::fire`]; and when what the window is
//! doing changes, [`Clock::conditions`].
//!
//! A layer whose `draw` returns `nil` leaves nothing behind: `end`
//! answers [`Wait::None`] and no source is asked for.

/// Seconds without anything happening after which the app stops the
/// clock.
pub const SETTLE: f64 = 45.0;
/// The most `frame.dt` ever is.
pub const MAX_DT: f64 = 0.25;
/// A wait of this long or more uses a plain timer, so that a slow drift
/// doesn't keep the toolkit's frame clock running.
pub const TIMER_FROM: f64 = 1.0 / 30.0;
/// A wait shorter than this is "the next frame".
pub const NEXT_FRAME_BELOW: f64 = 1.0 / 60.0;
/// "Next frame" is the display's refresh, at most this many a second.
pub const MAX_RATE: f64 = 120.0;
/// The longest wait armed: a request for later is asked again then.
pub const MAX_WAIT: f64 = 3600.0;
/// How much early a frame may come and still count as due: a 120 Hz
/// display's frames are not exactly 1/120 s apart.
const SLACK: f64 = 0.0005;

/// What the window is doing, as far as animating goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conditions {
    /// The layer's widget is on screen.
    pub mapped: bool,
    /// The window is the one in use.
    pub active: bool,
    /// The desktop allows motion (and `GMNB_STILL` isn't set).
    pub motion: bool,
    /// Nothing has happened for [`SETTLE`] seconds.
    pub settled: bool,
}

impl Conditions {
    /// Whether requests for frames are honoured now.
    pub fn run(&self) -> bool {
        self.mapped && self.active && self.motion && !self.settled
    }
}

/// What the app arms after a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wait {
    /// Nothing: no timer, no frame callback.
    None,
    /// The display's next frames, until [`Clock::fire`] says to draw.
    Frame,
    /// A plain timer of this many seconds.
    Timer(f64),
}

/// What to do when the timer or a display frame comes round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fire {
    /// Draw the layer now.
    Draw,
    /// Too early (a display faster than [`MAX_RATE`]): keep watching.
    NotYet,
    /// The request waits (the window went idle or inactive): disarm.
    Hold,
    /// Nothing is pending: disarm.
    Idle,
}

/// What a change of [`Conditions`] asks of the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Nothing,
    /// A waiting request can go on: draw the layer once (`dt` will be 0).
    Draw,
    /// A pending request must wait: disarm.
    Disarm,
}

/// The times a frame is told.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    /// Seconds this layer has spent animating.
    pub time: f64,
    /// Seconds since the layer's previous frame, if that frame asked for
    /// another and the clock has run since; else 0. Never more than
    /// [`MAX_DT`].
    pub dt: f64,
}

/// One layer's clock.
#[derive(Clone, Debug, Default)]
pub struct Clock {
    time: f64,
    /// When the previous frame was drawn, if it asked for another and the
    /// clock has run since.
    since: Option<f64>,
    /// The pending request: when it is due.
    due: Option<f64>,
    /// The pending request waits for the conditions.
    held: bool,
}

impl Clock {
    pub fn new() -> Clock {
        Clock::default()
    }

    /// Seconds this layer has spent animating.
    pub fn time(&self) -> f64 {
        self.time
    }

    /// Whether a request is pending (armed or waiting).
    pub fn pending(&self) -> bool {
        self.due.is_some()
    }

    /// Whether the pending request waits for the conditions.
    pub fn held(&self) -> bool {
        self.due.is_some() && self.held
    }

    /// A frame is about to be drawn at `now` (seconds, from any fixed
    /// point): what it is told.
    pub fn begin(&mut self, now: f64, c: Conditions) -> Timing {
        let dt = match self.since {
            Some(then) if c.motion => (now - then).clamp(0.0, MAX_DT),
            _ => 0.0,
        };
        self.time += dt;
        Timing {
            time: self.time,
            dt,
        }
    }

    /// The frame drawn at `now` returned `again`: `None` (nil), or
    /// seconds. It replaces whatever was pending.
    pub fn end(&mut self, now: f64, again: Option<f64>, c: Conditions) -> Wait {
        self.due = None;
        self.since = None;
        self.held = false;
        // Under reduced motion what a hook returns is ignored.
        let Some(asked) = again.filter(|_| c.motion) else {
            return Wait::None;
        };
        let wait = if asked.is_finite() && asked >= NEXT_FRAME_BELOW {
            asked.min(MAX_WAIT)
        } else {
            0.0
        };
        self.due = Some(now + wait.max(1.0 / MAX_RATE));
        if !c.run() {
            self.held = true;
            return Wait::None;
        }
        self.since = Some(now);
        if wait >= TIMER_FROM {
            Wait::Timer(wait)
        } else {
            Wait::Frame
        }
    }

    /// The timer, or a display frame, came round at `now`.
    pub fn fire(&mut self, now: f64, c: Conditions) -> Fire {
        let Some(due) = self.due else {
            return Fire::Idle;
        };
        if self.held {
            return Fire::Hold;
        }
        if !c.run() {
            self.held = true;
            self.since = None;
            return Fire::Hold;
        }
        if now + SLACK < due {
            Fire::NotYet
        } else {
            Fire::Draw
        }
    }

    /// What the window is doing changed to `c`.
    pub fn conditions(&mut self, c: Conditions) -> Change {
        if self.due.is_none() {
            return Change::Nothing;
        }
        match (self.held, c.run()) {
            // The wait ends: one frame with `dt` 0, and it carries on.
            (true, true) => {
                self.held = false;
                self.since = None;
                Change::Draw
            }
            (false, false) => {
                self.held = true;
                self.since = None;
                Change::Disarm
            }
            _ => Change::Nothing,
        }
    }

    /// Forgets everything (the theme was unloaded or reloaded).
    pub fn reset(&mut self) {
        *self = Clock::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: Conditions = Conditions {
        mapped: true,
        active: true,
        motion: true,
        settled: false,
    };

    #[test]
    fn nil_leaves_nothing_armed() {
        let mut c = Clock::new();
        assert_eq!(c.begin(10.0, RUN), Timing { time: 0.0, dt: 0.0 });
        assert_eq!(c.end(10.0, None, RUN), Wait::None);
        assert!(!c.pending());
        assert_eq!(c.fire(11.0, RUN), Fire::Idle);
        assert_eq!(c.conditions(RUN), Change::Nothing);
        // A later frame (an event, say) is told no time has passed.
        assert_eq!(c.begin(20.0, RUN), Timing { time: 0.0, dt: 0.0 });
    }

    #[test]
    fn zero_is_the_next_frame_at_most_120_a_second() {
        let mut c = Clock::new();
        c.begin(0.0, RUN);
        assert_eq!(c.end(0.0, Some(0.0), RUN), Wait::Frame);
        // A 240 Hz display: every second frame draws.
        assert_eq!(c.fire(1.0 / 240.0, RUN), Fire::NotYet);
        assert_eq!(c.fire(2.0 / 240.0, RUN), Fire::Draw);
        let t = c.begin(2.0 / 240.0, RUN);
        assert!((t.dt - 2.0 / 240.0).abs() < 1e-12 && t.time == t.dt);
        // Below 1/60 is 0; a 60 Hz display draws every frame.
        assert_eq!(c.end(1.0, Some(0.01), RUN), Wait::Frame);
        assert_eq!(c.fire(1.0 + 1.0 / 60.0, RUN), Fire::Draw);
        // Nonsense is the next frame too.
        assert_eq!(c.end(2.0, Some(-3.0), RUN), Wait::Frame);
        assert_eq!(c.end(2.0, Some(f64::NAN), RUN), Wait::Frame);
    }

    #[test]
    fn seconds_use_the_frame_clock_below_a_thirtieth_and_a_timer_from_it() {
        let mut c = Clock::new();
        c.begin(0.0, RUN);
        assert_eq!(c.end(0.0, Some(1.0 / 50.0), RUN), Wait::Frame);
        assert_eq!(c.fire(1.0 / 120.0, RUN), Fire::NotYet);
        assert_eq!(c.fire(1.0 / 50.0, RUN), Fire::Draw);
        assert_eq!(c.end(1.0, Some(1.0 / 30.0), RUN), Wait::Timer(1.0 / 30.0));
        assert_eq!(c.end(1.0, Some(0.066), RUN), Wait::Timer(0.066));
        assert_eq!(c.fire(1.066, RUN), Fire::Draw);
        assert_eq!(c.end(2.0, Some(1.0e9), RUN), Wait::Timer(MAX_WAIT));
    }

    #[test]
    fn dt_is_the_time_since_a_frame_that_asked_for_another_and_is_capped() {
        let mut c = Clock::new();
        c.begin(5.0, RUN);
        c.end(5.0, Some(1.0), RUN);
        // An event redraws the layer in the middle of its wait: the time
        // since the last frame, whatever caused this one.
        let t = c.begin(5.2, RUN);
        assert!((t.dt - 0.2).abs() < 1e-9);
        c.end(5.2, Some(1.0), RUN);
        // A stall: never more than a quarter of a second.
        let t = c.begin(9.0, RUN);
        assert_eq!(t.dt, MAX_DT);
        assert!((t.time - 0.45).abs() < 1e-9);
        // The frame asked for no other: time stands still after it.
        c.end(9.0, None, RUN);
        assert_eq!(c.begin(12.0, RUN).dt, 0.0);
        assert!((c.time() - 0.45).abs() < 1e-9);
    }

    #[test]
    fn settling_holds_the_request_and_waking_draws_once_with_no_dt() {
        let mut c = Clock::new();
        c.begin(0.0, RUN);
        assert_eq!(c.end(0.0, Some(0.066), RUN), Wait::Timer(0.066));
        // 45 s on, the timer comes round with nothing having happened.
        let settled = Conditions {
            settled: true,
            ..RUN
        };
        assert_eq!(c.fire(45.0, settled), Fire::Hold);
        assert!(c.held());
        // It stays disarmed however often it is asked.
        assert_eq!(c.fire(46.0, settled), Fire::Hold);
        assert_eq!(c.conditions(settled), Change::Nothing);
        // A frame drawn meanwhile (a resize) is told no time passes, and
        // its request waits too.
        assert_eq!(c.begin(50.0, settled).dt, 0.0);
        assert_eq!(c.end(50.0, Some(0.066), settled), Wait::None);
        assert!(c.held());
        // Something happens: one frame with dt 0, and it carries on.
        assert_eq!(c.conditions(RUN), Change::Draw);
        assert_eq!(c.begin(60.0, RUN).dt, 0.0);
        assert_eq!(c.end(60.0, Some(0.066), RUN), Wait::Timer(0.066));
        assert!((c.begin(60.066, RUN).dt - 0.066).abs() < 1e-9);
    }

    #[test]
    fn an_inactive_or_unmapped_window_disarms() {
        for off in [
            Conditions {
                active: false,
                ..RUN
            },
            Conditions {
                mapped: false,
                ..RUN
            },
        ] {
            let mut c = Clock::new();
            c.begin(0.0, RUN);
            assert_eq!(c.end(0.0, Some(0.0), RUN), Wait::Frame);
            assert_eq!(c.conditions(off), Change::Disarm);
            assert_eq!(c.conditions(off), Change::Nothing);
            assert_eq!(c.fire(1.0, off), Fire::Hold);
            assert_eq!(c.conditions(RUN), Change::Draw);
            assert_eq!(c.begin(9.0, RUN).dt, 0.0);
            // A frame that asks while the window is away waits at once.
            assert_eq!(c.end(9.0, Some(0.0), off), Wait::None);
            assert!(c.held());
        }
    }

    #[test]
    fn reduced_motion_ignores_requests_and_stops_time() {
        let still = Conditions {
            motion: false,
            ..RUN
        };
        let mut c = Clock::new();
        assert_eq!(c.begin(0.0, still).dt, 0.0);
        assert_eq!(c.end(0.0, Some(0.0), still), Wait::None);
        assert!(!c.pending());
        assert_eq!(c.conditions(RUN), Change::Nothing);
        // Motion going away in the middle of an animation.
        c.begin(1.0, RUN);
        c.end(1.0, Some(0.0), RUN);
        assert_eq!(c.conditions(still), Change::Disarm);
        assert_eq!(c.begin(1.5, still).dt, 0.0);
        assert_eq!(c.end(1.5, Some(0.0), still), Wait::None);
        assert!(!c.pending());
    }
}
