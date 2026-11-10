//! Events (docs/theming.md, "Events"): what happened recently, as data a
//! draw hook reads from `frame.events`. Each layer keeps its own
//! [`Ring`]: the kinds its `events` table names, each for as long as it
//! says (at most [`MAX_LIFE`] seconds), at most [`MAX_EVENTS`] of them.
//!
//! Time is the caller's: nothing here reads a clock.

use std::collections::{BTreeMap, VecDeque};

use crate::draw::Rect;
use crate::tokens::OptionValue;

/// Events kept per layer.
pub const MAX_EVENTS: usize = 32;
/// The longest an event is kept, in seconds.
pub const MAX_LIFE: f64 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    Press,
    Result,
    Error,
    Show,
    Option,
    /// The user changed page or calculator mode.
    Mode,
}

impl EventKind {
    pub const ALL: [(&'static str, EventKind); 6] = [
        ("press", EventKind::Press),
        ("result", EventKind::Result),
        ("error", EventKind::Error),
        ("show", EventKind::Show),
        ("option", EventKind::Option),
        ("mode", EventKind::Mode),
    ];

    pub fn name(self) -> &'static str {
        EventKind::ALL
            .iter()
            .find(|(_, k)| *k == self)
            .map_or("press", |(n, _)| n)
    }
}

/// What a press came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Key,
    Button,
    Swap,
    Date,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Key => "key",
            Source::Button => "button",
            Source::Swap => "swap",
            Source::Date => "date",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    Pointer,
    Keyboard,
    /// Operated through the accessibility interface.
    Assistive,
}

impl Via {
    pub fn name(self) -> &'static str {
        match self {
            Via::Pointer => "pointer",
            Via::Keyboard => "keyboard",
            Via::Assistive => "assistive",
        }
    }
}

/// What a pressed control does, when it is one of these (what it is, its
/// [`KeyKind`], is another matter: C and sin are both function keys).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Clear,
    Backspace,
    Memory,
}

impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Action::Clear => "clear",
            Action::Backspace => "backspace",
            Action::Memory => "memory",
        }
    }
}

/// What a key is to a theme: its class, and whether it is a toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    Number,
    Operator,
    Func,
    Equals,
    Toggle,
    Other,
}

impl KeyKind {
    pub fn name(self) -> &'static str {
        match self {
            KeyKind::Number => "number",
            KeyKind::Operator => "operator",
            KeyKind::Func => "func",
            KeyKind::Equals => "equals",
            KeyKind::Toggle => "toggle",
            KeyKind::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Calculator,
    Converter,
    Date,
    Graphing,
    Settings,
}

impl Page {
    pub fn name(self) -> &'static str {
        match self {
            Page::Calculator => "calculator",
            Page::Converter => "converter",
            Page::Date => "date",
            Page::Graphing => "graphing",
            Page::Settings => "settings",
        }
    }
}

/// One thing that happened, in a layer's own coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    pub page: Page,
    /// The control, readout or keypad it came from.
    pub rect: Rect,
    /// The exact point pressed for a pointer press on a key, else the
    /// middle of `rect`.
    pub x: f32,
    pub y: f32,
    /// A press: what was pressed and how.
    pub source: Option<Source>,
    pub via: Option<Via>,
    pub action: Option<Action>,
    /// A press on a key: its number (the same every time; absent on a key
    /// the calculator doesn't number) and its kind.
    pub key: Option<(Option<u32>, KeyKind)>,
    /// An option change: which, and its new value.
    pub option: Option<(String, OptionValue)>,
}

impl Event {
    /// An event with nothing but its kind, page and place.
    pub fn at(kind: EventKind, page: Page, rect: Rect) -> Event {
        Event {
            kind,
            page,
            rect,
            x: rect[0] + rect[2] / 2.0,
            y: rect[1] + rect[3] / 2.0,
            source: None,
            via: None,
            action: None,
            key: None,
            option: None,
        }
    }
}

/// A layer's recent events.
#[derive(Clone, Debug, Default)]
pub struct Ring {
    /// How long each kind is kept; a kind not here isn't listened for.
    lives: BTreeMap<EventKind, f64>,
    /// Oldest first: when each happened, and what.
    events: VecDeque<(f64, Event)>,
}

impl Ring {
    /// A ring for a layer's `events` table (kind names to seconds).
    pub fn new(events: &BTreeMap<String, f64>) -> Ring {
        let lives = EventKind::ALL
            .iter()
            .filter_map(|(name, kind)| {
                let life = *events.get(*name)?;
                (life > 0.0).then(|| (*kind, life.min(MAX_LIFE)))
            })
            .collect();
        Ring {
            lives,
            events: VecDeque::new(),
        }
    }

    /// Whether the layer is told of `kind`.
    pub fn listens(&self, kind: EventKind) -> bool {
        self.lives.contains_key(&kind)
    }

    /// `event` happened at `now`. Whether the layer is told of it.
    pub fn push(&mut self, now: f64, event: Event) -> bool {
        if !self.listens(event.kind) {
            return false;
        }
        self.prune(now);
        while self.events.len() >= MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back((now, event));
        true
    }

    /// Drops what has outlived its kind's life.
    fn prune(&mut self, now: f64) {
        let lives = &self.lives;
        self.events
            .retain(|(born, e)| lives.get(&e.kind).is_some_and(|life| now - born <= *life));
    }

    /// What `frame.events` holds at `now`: each event and its age, newest
    /// last.
    pub fn at(&mut self, now: f64) -> impl Iterator<Item = (f64, &Event)> {
        self.prune(now);
        self.events
            .iter()
            .map(move |(born, e)| ((now - born).max(0.0), e))
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(pairs: &[(&str, f64)]) -> Ring {
        Ring::new(&pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect())
    }

    fn press() -> Event {
        Event::at(EventKind::Press, Page::Calculator, [0.0, 0.0, 10.0, 10.0])
    }

    #[test]
    fn a_layer_is_told_only_what_it_listens_for() {
        let mut r = ring(&[("press", 1.0)]);
        assert!(r.listens(EventKind::Press) && !r.listens(EventKind::Show));
        assert!(r.push(0.0, press()));
        let show = Event::at(EventKind::Show, Page::Calculator, [0.0; 4]);
        assert!(!r.push(0.0, show));
        assert_eq!(r.at(0.5).count(), 1);
        // No table, no events.
        let mut none = Ring::default();
        assert!(!none.push(0.0, press()));
        assert!(none.is_empty());
    }

    #[test]
    fn every_kind_has_its_name() {
        let mut r = ring(&[("mode", 1.0), ("nonsense", 1.0)]);
        assert!(r.push(0.0, Event::at(EventKind::Mode, Page::Graphing, [0.0; 4])));
        assert_eq!(r.at(0.0).next().map(|(_, e)| e.kind.name()), Some("mode"));
        for (name, kind) in EventKind::ALL {
            assert_eq!(kind.name(), name);
        }
        assert_eq!(
            [
                Via::Pointer.name(),
                Via::Keyboard.name(),
                Via::Assistive.name()
            ],
            ["pointer", "keyboard", "assistive"]
        );
        assert_eq!(
            [
                Action::Clear.name(),
                Action::Backspace.name(),
                Action::Memory.name()
            ],
            ["clear", "backspace", "memory"]
        );
    }

    #[test]
    fn events_age_and_end_with_their_kinds_life() {
        let mut r = ring(&[("press", 1.0), ("result", 2.0)]);
        r.push(10.0, press());
        r.push(
            10.5,
            Event::at(EventKind::Result, Page::Calculator, [0.0; 4]),
        );
        let ages: Vec<f64> = r.at(10.75).map(|(age, _)| age).collect();
        assert_eq!(ages, [0.75, 0.25]);
        // The press is gone after its second; the result has another.
        let kinds: Vec<EventKind> = r.at(11.25).map(|(_, e)| e.kind).collect();
        assert_eq!(kinds, [EventKind::Result]);
        assert_eq!(r.at(12.6).count(), 0);
        assert!(r.is_empty());
    }

    #[test]
    fn at_most_five_seconds_and_thirty_two_events() {
        let mut r = ring(&[("press", 60.0)]);
        r.push(0.0, press());
        assert_eq!(r.at(5.0).count(), 1);
        assert_eq!(r.at(5.01).count(), 0);
        for i in 0..40 {
            let mut e = press();
            e.x = i as f32;
            r.push(100.0, e);
        }
        let kept: Vec<f32> = r.at(100.0).map(|(_, e)| e.x).collect();
        assert_eq!(kept.len(), MAX_EVENTS);
        // The oldest went first; the newest is last.
        assert_eq!((kept[0], kept[31]), (8.0, 39.0));
    }
}
