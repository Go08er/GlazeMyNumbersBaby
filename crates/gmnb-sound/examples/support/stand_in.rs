//! A stand-in for the app between an event and its plays
//! (docs/theming-media.md, "From event to sound", steps 1 to 4): the
//! app's own decision (`gmnb_theme::sound::decide`, which the app's
//! `apps/gmnb/src/sound.rs` calls too), with a `ctx` as the app has it
//! just after the theme was loaded, and events made as the app makes
//! them.
//!
//! The render example and the tests of a theme's sounds share this file
//! (`#[path]`), so that what is rendered and what is tested are decided the
//! same way, and the same way as in the app. Nothing here makes a sound.
//!
//! What it does not do: the window's being in use, and the 30 ms rule for
//! the same event twice (step 5).
#![allow(dead_code)]

use std::collections::BTreeMap;

use gmnb_sound::{EventName, Play, Resolver};
use gmnb_theme::events::{Action, Cause, Event, EventKind, KeyKind, Page, Source, Via};
pub use gmnb_theme::sound::By;
use gmnb_theme::sound::{Choice, across, decide};
use gmnb_theme::theme::{Theme, complete_options};
use gmnb_theme::{Ctx, OptionValue, ThemeError};

/// The window the stand-in's events happen in.
pub const WIDTH: f32 = 400.0;
pub const HEIGHT: f32 = 640.0;

pub struct StandIn<'a> {
    pub theme: &'a Theme,
    pub resolver: Resolver,
    /// What the theme's hooks are told: the app's default `ctx` (light,
    /// no accent, opaque, moving, the default font), with the theme's
    /// options.
    ctx: Ctx,
    /// What the theme's author should know: what the function returned
    /// that was no play, its failures, and what else of its sounds the
    /// theme noted.
    pub notes: Vec<String>,
}

impl<'a> StandIn<'a> {
    /// For a theme as it is when just loaded: its options at their
    /// defaults.
    pub fn new(theme: &'a Theme) -> StandIn<'a> {
        let options = complete_options(&theme.def.options, &BTreeMap::new());
        StandIn {
            theme,
            resolver: Resolver::new(theme.sounds().table, 1),
            ctx: Ctx {
                options,
                ..Ctx::default()
            },
            notes: Vec::new(),
        }
    }

    /// One of the theme's options, as `ctx.options` has it from now on.
    /// (The event that tells the theme of the change is [`option`].)
    pub fn set_option(&mut self, id: &str, value: OptionValue) {
        self.ctx.options.insert(id.to_string(), value);
    }

    fn take_notes(&mut self) {
        let notes = self.theme.take_sound_notes();
        self.notes.extend(notes.iter().map(|n| n.to_string()));
    }

    /// What the theme's `sound` function says to `event`, in the time
    /// the app gives it. `None`: the theme has none, or it has been
    /// switched off.
    pub fn ask(&mut self, event: &Event) -> Option<Result<Choice, ThemeError>> {
        let asked = self
            .theme
            .sound_choice(event, across(event, WIDTH), &self.ctx);
        self.take_notes();
        asked
    }

    /// What the app would play for `event`, and who decided it.
    pub fn decide(&mut self, event: &Event) -> (Vec<Play>, By) {
        let decided = decide(self.theme, &mut self.resolver, event, WIDTH, &self.ctx);
        if let Some(e) = &decided.failed {
            self.notes.push(format!("sound(): {e}"));
        }
        self.take_notes();
        (decided.plays, decided.by)
    }
}

// ---- Events, as the app makes them ------------------------------------------------

/// A control `w` wide whose middle is `across` the window (0 its left
/// edge, 1 its right), in the keypad's part of it.
fn control(across: f32, w: f32) -> [f32; 4] {
    [across * WIDTH - w / 2.0, HEIGHT * 0.7, w, 48.0]
}

/// A keypad key pressed with the pointer: its number, its kind, what it
/// does if that is one of the three things sound has a name for, and
/// where it is across the window.
pub fn key(id: u32, kind: KeyKind, action: Option<Action>, across: f32) -> Event {
    Event {
        source: Some(Source::Key),
        via: Some(Via::Pointer),
        action,
        key: Some((Some(id), kind)),
        ..Event::at(EventKind::Press, Page::Calculator, control(across, 80.0))
    }
}

/// A button that is no key: the memory row's (`Some(Action::Memory)`),
/// F-E, the converter's swap, a date picked.
pub fn button(source: Source, action: Option<Action>, page: Page, across: f32) -> Event {
    Event {
        source: Some(source),
        via: Some(Via::Pointer),
        action,
        ..Event::at(EventKind::Press, page, control(across, 56.0))
    }
}

/// A calculation's result (`EventKind::Result`) or error, from the
/// readout at the top of `page`, after = was clicked: what the `sounds`
/// table plays.
pub fn outcome(kind: EventKind, page: Page) -> Event {
    outcome_after(kind, page, Cause::Equals)
}

/// A result or an error that a press of this kind brought about (the
/// running total + shows is `Cause::Operator`, which the table leaves
/// silent), clicked.
pub fn outcome_after(kind: EventKind, page: Page, cause: Cause) -> Event {
    Event {
        via: Some(Via::Pointer),
        cause: Some(cause),
        ..Event::at(kind, page, [0.0, 80.0, WIDTH, 96.0])
    }
}

/// The page or mode was changed.
pub fn mode(page: Page) -> Event {
    Event::at(EventKind::Mode, page, [0.0, 48.0, WIDTH, HEIGHT - 48.0])
}

/// One of the theme's options was changed, in Settings.
pub fn option(id: &str, value: OptionValue) -> Event {
    Event {
        option: Some((id.to_string(), value)),
        ..Event::at(EventKind::Option, Page::Settings, [0.0, 0.0, WIDTH, HEIGHT])
    }
}

/// An event of each name the app gives one to, on key `id` (for the names
/// that are keys), `across` the window. `Key` and `Press` are no events:
/// they are what other names fall back to.
pub fn named(name: EventName, id: u32, across: f32) -> Option<Event> {
    let page = Page::Calculator;
    Some(match name {
        EventName::Digit => key(id, KeyKind::Number, None, across),
        EventName::Operator => key(id, KeyKind::Operator, None, across),
        EventName::Func => key(id, KeyKind::Func, None, across),
        EventName::Equals => key(id, KeyKind::Equals, None, across),
        EventName::Toggle => key(id, KeyKind::Toggle, None, across),
        EventName::Clear => key(id, KeyKind::Func, Some(Action::Clear), across),
        EventName::Backspace => key(id, KeyKind::Other, Some(Action::Backspace), across),
        EventName::Memory => button(Source::Button, Some(Action::Memory), page, across),
        EventName::Button => button(Source::Button, None, page, across),
        EventName::Result => outcome(EventKind::Result, page),
        EventName::Error => outcome(EventKind::Error, page),
        EventName::Mode => mode(page),
        EventName::Key | EventName::Press => return None,
    })
}
