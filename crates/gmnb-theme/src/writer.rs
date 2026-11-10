//! What the app's CSS providers hold (docs/theming.md, "The pieces").
//!
//! Two of them, both a `:root` block of custom properties:
//!
//! - **main**: every property the structure stylesheet reads, and the
//!   toolkit's own named colours. Reloading it restyles the whole tree
//!   (tens of milliseconds for a thousand properties).
//! - **live**, at a higher priority: the few properties that changed since
//!   main was last loaded. A drag (the Background opacity slider, a colour
//!   chooser) rewrites only this one.
//!
//! [`Writer::update`] decides which to load; this module knows nothing of
//! the toolkit, so the decision is tested without one. The live provider
//! is folded back into main ([`Writer::fold`]) when the changes stop: the
//! app calls it when it saves the setting that was dragged, a moment
//! after the last step.

use std::collections::{BTreeMap, BTreeSet};

use crate::css::PREFIX;

/// What to load after an update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Update {
    /// Nothing changed.
    Unchanged,
    /// Load this into the live provider; main stays.
    Live(String),
    /// Load this into the main provider, and empty the live one.
    Main(String),
}

pub struct Writer {
    /// The custom properties the structure stylesheet reads.
    used: BTreeSet<String>,
    /// What main holds.
    main: BTreeMap<String, String>,
    preamble: String,
    /// What live holds: the changes since main was loaded.
    live: BTreeMap<String, String>,
    loaded: bool,
}

/// The names of the app's custom properties `stylesheet` reads:
/// every `var(--gmnb-…)`.
pub fn used_properties(stylesheet: &str) -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    let mut rest = stylesheet;
    while let Some(at) = rest.find("var(") {
        rest = &rest[at + 4..];
        let name: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if name.starts_with(PREFIX) {
            used.insert(name);
        }
    }
    used
}

fn block(properties: &BTreeMap<String, String>) -> String {
    if properties.is_empty() {
        return String::new();
    }
    let mut s = String::with_capacity(properties.len() * 48 + 16);
    s.push_str(":root {\n");
    for (name, value) in properties {
        s.push_str("  ");
        s.push_str(name);
        s.push_str(": ");
        s.push_str(value);
        s.push_str(";\n");
    }
    s.push_str("}\n");
    s
}

impl Writer {
    /// More changed properties than this go to main: the live provider
    /// stays small.
    pub const LIVE_MAX: usize = 64;

    /// A writer for the structure stylesheet's text.
    pub fn new(stylesheet: &str) -> Writer {
        Writer {
            used: used_properties(stylesheet),
            main: BTreeMap::new(),
            preamble: String::new(),
            live: BTreeMap::new(),
            loaded: false,
        }
    }

    /// The properties the stylesheet reads.
    pub fn used(&self) -> &BTreeSet<String> {
        &self.used
    }

    /// Of `properties`, what the providers are given: the app's own
    /// properties that the stylesheet reads, and every other one (the
    /// toolkit's variables).
    fn wanted(&self, properties: &BTreeMap<String, String>) -> BTreeMap<String, String> {
        properties
            .iter()
            .filter(|(name, _)| !name.starts_with(PREFIX) || self.used.contains(*name))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// The stylesheet's properties that `properties` lacks: each one is a
    /// bug (the stylesheet names them without a fallback).
    pub fn missing(&self, properties: &BTreeMap<String, String>) -> Vec<String> {
        self.used
            .iter()
            .filter(|name| !properties.contains_key(*name))
            .cloned()
            .collect()
    }

    /// Takes a style's properties, and `preamble` (CSS that goes before
    /// them in main: named colours), and says what to load. `drag`: the
    /// change is one step of many (a slider being dragged), so a few
    /// changed properties go to the live provider; otherwise, and when
    /// more than [`Writer::LIVE_MAX`] differ from main, main is reloaded.
    pub fn update(
        &mut self,
        preamble: &str,
        properties: &BTreeMap<String, String>,
        drag: bool,
    ) -> Update {
        let wanted = self.wanted(properties);
        let same_names = self.loaded
            && self.preamble == preamble
            && wanted.len() == self.main.len()
            && wanted.keys().all(|k| self.main.contains_key(k));
        if same_names {
            // What live would have to hold: everything that differs from
            // main, not just from the step before.
            let differs: BTreeMap<String, String> = wanted
                .iter()
                .filter(|(k, v)| self.main.get(*k) != Some(*v))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if differs == self.live {
                return Update::Unchanged;
            }
            if drag && differs.len() <= Writer::LIVE_MAX {
                self.live = differs;
                return Update::Live(block(&self.live));
            }
            if differs.is_empty() {
                // Back to what main holds.
                self.live.clear();
                return Update::Live(String::new());
            }
        }
        self.main = wanted;
        self.preamble = preamble.to_string();
        self.live.clear();
        self.loaded = true;
        Update::Main(self.main_text())
    }

    /// Folds the live provider into main: the text to load into main (the
    /// live one is then emptied), or `None` if live holds nothing.
    pub fn fold(&mut self) -> Option<String> {
        if self.live.is_empty() {
            return None;
        }
        let live = std::mem::take(&mut self.live);
        self.main.extend(live);
        Some(self.main_text())
    }

    /// How many properties the live provider holds.
    pub fn live_len(&self) -> usize {
        self.live.len()
    }

    /// How many properties main holds.
    pub fn main_len(&self) -> usize {
        self.main.len()
    }

    fn main_text(&self) -> String {
        let mut s = self.preamble.clone();
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str(&block(&self.main));
        s
    }
}
