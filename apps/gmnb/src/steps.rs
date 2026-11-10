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
//! - `quit`: what sound has done, a line on stderr (as `GMNB_THEME_STATS`
//!   writes it), and the window closed.

use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use crate::widgets::keypad::Keypad;
use crate::window::Window;

enum Step {
    Keys(String),
    Click(u32),
    Activate(u32),
    Wait(u64),
    Theme(String),
    Quit,
}

fn parse(spec: &str) -> VecDeque<Step> {
    spec.split(';')
        .filter_map(|part| {
            if part.trim() == "quit" {
                return Some(Step::Quit);
            }
            let (what, arg) = part.trim().split_once(':')?;
            Some(match what {
                "keys" => Step::Keys(arg.replace("\\n", "\n")),
                "click" => Step::Click(arg.trim().parse().ok()?),
                "activate" => Step::Activate(arg.trim().parse().ok()?),
                "wait" => Step::Wait(arg.trim().parse().ok()?),
                "theme" => Step::Theme(arg.trim().to_string()),
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
        Step::Quit => {
            eprintln!("sound: {}", win.ctx().hub.sound_report());
            win.widget().close();
            return;
        }
    }
    glib::timeout_add_local_once(Duration::from_millis(pause), move || next(win, steps, gap));
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
