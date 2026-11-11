//! Dev/test hook: `GMNB_STEPS`, a few things done to the window one after
//! another, as a person would do them (the app's sound test drives the
//! window with it). Like `GMNB_KEYS`, it is for the project's own scripts.
//!
//! `GMNB_STEPS="keys:7;click:137;activate:137;wait:500;keys:{alt+2}"`:
//! the steps, separated by `;`, one every 250 ms (`GMNB_STEPS_GAP_MS`),
//! from 1 s after the window comes on screen (`GMNB_STEPS_AFTER_MS`):
//!
//! - `keys:<script>`: typed, as `GMNB_KEYS` types (the keyboard's way);
//! - `click:<id>`: the shown keypad's key `<id>` clicked by the pointer,
//!   at its middle;
//! - `activate:<id>`: the same key activated as assistive technology
//!   activates it (no pointer, no key);
//! - `wait:<ms>`: a longer pause;
//! - `theme:<id>`: that theme chosen, as Settings' list chooses it (`base`:
//!   the base look), without saving the choice;
//! - `settings`: Settings opened, as its menu item opens it; `close`: the
//!   dialog showing (Settings, About) closed, then a line
//!   `GMNB_STEPS: closed`;
//! - `press:<css class>`: the first button with that class in the window
//!   or its dialog pressed (activated, as a key on it would: Settings'
//!   `wc-open-themes`, `wc-restore-themes`);
//! - `themes`: what Settings lists, a line on stderr: `themes: ` and each
//!   row's theme (`base` for the base look), `*` after one marked "Comes
//!   with GMNB"; then `restore shown` or `restore hidden`;
//! - `slide:<class>:<v>,<v>,...`: the slider with that CSS class (in
//!   Settings: `wc-opacity-slider`) set to each value in turn, all at once,
//!   as a drag the toolkit reports faster than the screen shows it,
//!   between a line `GMNB_STEPS: sliding` and a line `GMNB_STEPS: slid`;
//! - `quit`: what sound has done, a line on stderr (as `GMNB_THEME_STATS`
//!   writes it), then a line `note: ...` for each of the theme's notes
//!   (what Settings lists under the themes), and the window closed (its
//!   dialogs first: a window closes its dialog rather than itself).

use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;

use crate::widgets::keypad::Keypad;
use crate::window::Window;

enum Step {
    Keys(String),
    Click(u32),
    Activate(u32),
    Wait(u64),
    Theme(String),
    Settings,
    Close,
    Press(String),
    Themes,
    Slide(String, Vec<f64>),
    Quit,
}

fn parse(spec: &str) -> VecDeque<Step> {
    spec.split(';')
        .filter_map(|part| {
            match part.trim() {
                "quit" => return Some(Step::Quit),
                "settings" => return Some(Step::Settings),
                "close" => return Some(Step::Close),
                "themes" => return Some(Step::Themes),
                _ => {}
            }
            let (what, arg) = part.trim().split_once(':')?;
            Some(match what {
                "keys" => Step::Keys(arg.replace("\\n", "\n")),
                "click" => Step::Click(arg.trim().parse().ok()?),
                "activate" => Step::Activate(arg.trim().parse().ok()?),
                "wait" => Step::Wait(arg.trim().parse().ok()?),
                "theme" => Step::Theme(arg.trim().to_string()),
                "press" => Step::Press(arg.trim().to_string()),
                "slide" => {
                    let (class, values) = arg.split_once(':')?;
                    let values: Option<Vec<f64>> =
                        values.split(',').map(|v| v.trim().parse().ok()).collect();
                    Step::Slide(class.trim().to_string(), values?)
                }
                _ => {
                    eprintln!("GMNB_STEPS: what is {part:?}?");
                    return None;
                }
            })
        })
        .collect()
}

fn ms(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub fn run(win: &Rc<Window>, spec: &str) {
    let steps = parse(spec);
    let (after, gap) = (
        ms("GMNB_STEPS_AFTER_MS", 1000),
        ms("GMNB_STEPS_GAP_MS", 250),
    );
    let w = win.clone();
    let start = move || {
        let w = w.clone();
        let steps = std::cell::RefCell::new(steps);
        glib::timeout_add_local_once(Duration::from_millis(after), move || next(w, steps, gap));
    };
    // From when the window is on screen (it is presented after this).
    let widget = win.widget();
    if widget.is_mapped() {
        start();
    } else {
        let once = std::cell::Cell::new(Some(start));
        widget.connect_map(move |_| {
            if let Some(start) = once.take() {
                start();
            }
        });
    }
}

fn next(win: Rc<Window>, steps: std::cell::RefCell<VecDeque<Step>>, gap: u64) {
    let Some(step) = steps.borrow_mut().pop_front() else {
        return;
    };
    let mut pause = gap;
    match step {
        Step::Keys(keys) => win.simulate_keys(&keys),
        Step::Click(id) | Step::Activate(id) => {
            let pointer = matches!(step, Step::Click(_));
            let done = shown_keypads(&win.widget().upcast())
                .iter()
                .any(|pad| pad.press_for_test(id, pointer));
            if !done {
                eprintln!("GMNB_STEPS: no key {id} on screen");
            }
        }
        Step::Wait(ms) => pause = ms,
        Step::Theme(id) => win.ctx().hub.set_theme(&id),
        Step::Settings => crate::prefs::show(&win),
        Step::Close => {
            use adw::prelude::*;
            if let Some(dialog) = win.widget().visible_dialog() {
                let _ = dialog.close();
            }
            eprintln!("GMNB_STEPS: closed");
        }
        Step::Press(class) => {
            let found = widgets(&win.widget().upcast())
                .into_iter()
                .find(|w| w.is::<gtk::Button>() && w.has_css_class(&class));
            match found {
                Some(button) => {
                    button.activate();
                }
                None => eprintln!("GMNB_STEPS: no button with class {class}"),
            }
        }
        Step::Themes => {
            let all = widgets(&win.widget().upcast());
            let list = all.iter().find(|w| w.has_css_class("wc-theme-list"));
            let rows: Vec<String> = list
                .map(|list| {
                    widgets(list)
                        .into_iter()
                        .filter(|w| w.is::<adw::ActionRow>())
                        .map(|row| {
                            let marked = widgets(&row)
                                .iter()
                                .any(|w| w.has_css_class("wc-comes-with"));
                            format!("{}{}", row.widget_name(), if marked { "*" } else { "" })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let restore = all
                .iter()
                .find(|w| w.has_css_class("wc-restore-themes"))
                .is_some_and(|b| b.is_visible());
            eprintln!(
                "themes: {}; restore {}",
                rows.join(" "),
                if restore { "shown" } else { "hidden" }
            );
        }
        Step::Slide(class, values) => {
            let scale = find(&win.widget().upcast(), |w| w.has_css_class(&class))
                .and_then(|w| w.downcast::<gtk::Range>().ok());
            match scale {
                Some(scale) => {
                    eprintln!("GMNB_STEPS: sliding");
                    for v in values {
                        scale.set_value(v);
                    }
                    eprintln!("GMNB_STEPS: slid");
                }
                None => eprintln!("GMNB_STEPS: no slider with class {class}"),
            }
        }
        Step::Quit => {
            eprintln!("sound: {}", win.ctx().hub.sound_report());
            for note in win.ctx().hub.notes() {
                eprintln!("note: {note}");
            }
            // (A window with a dialog open closes the dialog instead.)
            while let Some(dialog) = win.widget().visible_dialog() {
                dialog.force_close();
            }
            win.widget().close();
            return;
        }
    }
    glib::timeout_add_local_once(Duration::from_millis(pause), move || next(win, steps, gap));
}

/// `root` and every widget in it, parents first.
fn widgets(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    let mut todo = vec![root.clone()];
    while let Some(w) = todo.pop() {
        let mut child = w.last_child();
        while let Some(c) = child {
            child = c.prev_sibling();
            todo.push(c);
        }
        out.push(w);
    }
    out
}

/// The first widget in `root` (itself included) that `wanted` says is it,
/// depth first.
fn find(root: &gtk::Widget, wanted: impl Fn(&gtk::Widget) -> bool) -> Option<gtk::Widget> {
    let mut todo = vec![root.clone()];
    while let Some(w) = todo.pop() {
        if wanted(&w) {
            return Some(w);
        }
        let mut child = w.last_child();
        while let Some(c) = child {
            child = c.prev_sibling();
            todo.push(c);
        }
    }
    None
}

/// The keypads drawn in `root`.
fn shown_keypads(root: &gtk::Widget) -> Vec<Keypad> {
    let mut out = Vec::new();
    let mut todo = vec![root.clone()];
    while let Some(w) = todo.pop() {
        if let Some(pad) = w.downcast_ref::<Keypad>()
            && pad.is_drawable()
        {
            out.push(pad.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            todo.push(c);
        }
    }
    out
}
