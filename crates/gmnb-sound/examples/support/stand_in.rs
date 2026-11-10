//! A stand-in for the app between an event and its plays
//! (docs/theming-media.md, "From event to sound", steps 2 to 4): the event
//! as a theme's `sound` function is given it, the `ctx` beside it, the
//! call in its 2 ms, and the `sounds` table for what the function leaves
//! to it.
//!
//! The render example and the tests of a theme's sounds share this file
//! (`#[path]`), so that what is rendered and what is tested are decided the
//! same way. Nothing here makes a sound.
//!
//! What it does not do: the 30 ms rule for the same event twice (step 5),
//! and a `ctx` in full. Its `ctx` has `dark`, `opacity`, `options`,
//! `motion` and `font`, and no `accent` or `base`.
#![allow(dead_code)]

use std::collections::BTreeMap;

use gmnb_sound::{EventName, Play, Resolver};
use gmnb_theme::events::{Action, Event, EventKind, KeyKind, Page, Source, Via};
use gmnb_theme::mlua::{self, Lua, Table};
use gmnb_theme::sound::{Choice, HOOK_TIME, across, answers, event_name, parse_choice, trigger};
use gmnb_theme::theme::{Theme, complete_options};
use gmnb_theme::{Budget, Notes, OptionValue, ThemeError};

/// The window the stand-in's events happen in.
pub const WIDTH: f32 = 400.0;
pub const HEIGHT: f32 = 640.0;

/// Who decided what an event plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    /// The theme's `sound` function.
    Function,
    /// The `sounds` table, under this name.
    Table(EventName),
    /// Nobody: the function left it to the table, which has no entry for
    /// it (or has `false`, or the event has no name).
    Nobody,
}

pub struct StandIn<'a> {
    pub theme: &'a Theme,
    pub resolver: Resolver,
    options: BTreeMap<String, OptionValue>,
    ctx: Table,
    /// What the theme's author should know: what the function returned
    /// that was no play, and its failures.
    pub notes: Vec<String>,
}

fn set_option(t: &Table, id: &str, value: &OptionValue) -> mlua::Result<()> {
    match value {
        OptionValue::Text(s) => t.raw_set(id, s.as_str()),
        OptionValue::Number(n) => t.raw_set(id, *n),
        OptionValue::Bool(b) => t.raw_set(id, *b),
    }
}

fn ctx_table(lua: &Lua, options: &BTreeMap<String, OptionValue>) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("dark", false)?;
    t.raw_set("opacity", 1.0)?;
    let o = lua.create_table()?;
    for (id, value) in options {
        set_option(&o, id, value)?;
    }
    o.set_readonly(true);
    t.raw_set("options", o)?;
    t.raw_set("motion", true)?;
    let font = lua.create_table()?;
    font.raw_set("family", "sans-serif")?;
    font.raw_set("size", 15.0)?;
    font.set_readonly(true);
    t.raw_set("font", font)?;
    t.set_readonly(true);
    Ok(t)
}

/// An event as `sound(event, ctx)` is given it: what a layer would see
/// (`gmnb-theme`'s `layer.rs`, `event_table`), in window coordinates, with
/// `age` 0 and `pan`.
fn event_table(lua: &Lua, e: &Event) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.raw_set("kind", e.kind.name())?;
    t.raw_set("age", 0.0)?;
    t.raw_set("x", e.x)?;
    t.raw_set("y", e.y)?;
    let rect = lua.create_table()?;
    for (name, v) in ["x", "y", "w", "h"].into_iter().zip(e.rect) {
        rect.raw_set(name, v)?;
    }
    rect.set_readonly(true);
    t.raw_set("rect", rect)?;
    t.raw_set("page", e.page.name())?;
    if let Some(source) = e.source {
        t.raw_set("source", source.name())?;
    }
    if let Some(via) = e.via {
        t.raw_set("via", via.name())?;
    }
    if let Some(action) = e.action {
        t.raw_set("action", action.name())?;
    }
    if let Some((id, kind)) = e.key {
        let key = lua.create_table()?;
        if let Some(id) = id {
            key.raw_set("id", id)?;
        }
        key.raw_set("kind", kind.name())?;
        key.set_readonly(true);
        t.raw_set("key", key)?;
    }
    if let Some((id, value)) = &e.option {
        t.raw_set("id", id.as_str())?;
        set_option(&t, "value", value)?;
    }
    t.raw_set("pan", across(e, WIDTH))?;
    t.set_readonly(true);
    Ok(t)
}

impl<'a> StandIn<'a> {
    /// For a theme as it is when just loaded: its options at their
    /// defaults.
    pub fn new(theme: &'a Theme) -> StandIn<'a> {
        let options = complete_options(&theme.def.options, &BTreeMap::new());
        let ctx = ctx_table(theme.engine().lua(), &options).expect("a table for ctx");
        StandIn {
            theme,
            resolver: Resolver::new(theme.sounds().table, 1),
            options,
            ctx,
            notes: Vec::new(),
        }
    }

    /// One of the theme's options, as `ctx.options` has it from now on.
    /// (The event that tells the theme of the change is [`option`].)
    pub fn set_option(&mut self, id: &str, value: OptionValue) {
        self.options.insert(id.to_string(), value);
        self.ctx = ctx_table(self.theme.engine().lua(), &self.options).expect("a table for ctx");
    }

    /// What the theme's `sound` function says to `event`, in the time
    /// the app gives it. `None`: the theme has none, or it has been
    /// switched off.
    pub fn ask(&mut self, event: &Event) -> Option<Result<Choice, ThemeError>> {
        let hook = self.theme.def.sound.as_ref()?;
        let engine = self.theme.engine();
        let table = event_table(engine.lua(), event).expect("a table for the event");
        match engine.call(hook, Budget::Custom(HOOK_TIME), (table, &self.ctx)) {
            Err(ThemeError::Disabled(_)) => None,
            Err(e) => Some(Err(e)),
            Ok(value) => {
                let mut notes = Notes::default();
                let choice = parse_choice(&value, &mut notes);
                self.notes.extend(notes.list.iter().map(|n| n.to_string()));
                Some(Ok(choice))
            }
        }
    }

    /// What the app would play for `event`, and who decided it.
    pub fn decide(&mut self, event: &Event) -> (Vec<Play>, By) {
        if !answers(event) {
            return (Vec::new(), By::Nobody);
        }
        match self.ask(event) {
            Some(Ok(Choice::Silence)) => return (Vec::new(), By::Function),
            Some(Ok(Choice::Plays(plays))) => return (plays, By::Function),
            Some(Ok(Choice::Table)) | None => {}
            // The table decides for an event the function failed on.
            Some(Err(e)) => self.notes.push(format!("sound(): {e}")),
        }
        let Some(name) = event_name(event) else {
            return (Vec::new(), By::Nobody);
        };
        let by = match self.resolver.table().find(name) {
            Some((found, _)) => By::Table(found),
            None => By::Nobody,
        };
        let play = self.resolver.resolve(name, trigger(event, WIDTH));
        (play.into_iter().collect(), by)
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
/// readout at the top of `page`.
pub fn outcome(kind: EventKind, page: Page) -> Event {
    Event::at(kind, page, [0.0, 80.0, WIDTH, 96.0])
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
