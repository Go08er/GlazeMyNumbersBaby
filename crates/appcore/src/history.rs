//! A text field's undo history (Ctrl+Z; Ctrl+Shift+Z or Ctrl+Y to redo),
//! one model for both twins' fields: DGMNB's `TextEdit` keeps one, and so
//! does each of GMNB's GtkText fields in place of GTK's own (which an
//! assistive technology's `SetTextContents` clears, and which takes the
//! first character typed over a selection back apart from the rest).
//!
//! What it holds is what upstream's rich edit box takes back: each edit
//! whole, the text and the selection as they were before it (what a
//! replacement replaced is selected again). A run of typing is one edit;
//! a keypad key, a paste, a cut, a deletion and an assistive technology's
//! text are each one of their own. A new edit forgets what was undone. A
//! text the program writes into a field itself is not the user's edit: it
//! ends the history ([`History::clear`]), as GTK's `set_text` does.
//!
//! It knows nothing of how a field stores its text: positions are the
//! caller's own (DGMNB's are byte offsets, GTK's characters).

use std::collections::VecDeque;

use crate::input::{Key, KeyPress};

/// How many edits a field remembers.
pub const UNDO_STEPS: usize = 200;

/// What a key does to a field's history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chord {
    Undo,
    Redo,
}

/// The history's keys, as GTK's fields and upstream's RichEditBox have
/// them: Ctrl+Z undoes; Ctrl+Shift+Z and Ctrl+Y redo. Not with Alt.
pub fn chord(kp: &KeyPress) -> Option<Chord> {
    if !kp.ctrl || kp.alt {
        return None;
    }
    match kp.key {
        Key::Char('z' | 'Z') if kp.shift => Some(Chord::Redo),
        Key::Char('z' | 'Z') => Some(Chord::Undo),
        Key::Char('y' | 'Y') if !kp.shift => Some(Chord::Redo),
        _ => None,
    }
}

/// Whether `kp` types its character into a field (one that a run of typing
/// is made of, [`History::record`]): a character, without Ctrl or Alt.
pub fn types(kp: &KeyPress) -> bool {
    matches!(kp.key, Key::Char(_)) && !kp.ctrl && !kp.alt
}

/// A field's text and selection, as an undo or a redo puts them back:
/// the caret (`cursor`) and the selection's other end (`anchor`, the caret
/// itself when nothing is selected).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub text: String,
    pub cursor: usize,
    pub anchor: usize,
}

impl Snapshot {
    /// Whether something is selected.
    pub fn has_selection(&self) -> bool {
        self.cursor != self.anchor
    }
}

/// The edits Ctrl+Z goes back through and Ctrl+Shift+Z forward again.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    /// What Ctrl+Z goes back to, the latest last.
    undone: VecDeque<Snapshot>,
    /// What Ctrl+Shift+Z goes forward to again, after an undo.
    redone: Vec<Snapshot>,
    /// Where the last typed character left the caret, while typing goes
    /// on: one typed there next is taken back together with it.
    typed: Option<usize>,
}

impl History {
    pub fn new() -> History {
        History::default()
    }

    /// Records an edit that changed the field from `before` to the text
    /// `after`, the caret now at `caret`; true if it joined the edit before
    /// it. One that changed nothing is no edit. A character typed
    /// (`typing`) where the last one typed left the caret, with nothing
    /// selected, joins that one's edit: a run of typing is taken back whole.
    /// The first one typed over a selection begins a run (taken back, the
    /// selection is back). Any other edit is one of its own and ends a run.
    pub fn record(&mut self, before: Snapshot, after: &str, caret: usize, typing: bool) -> bool {
        if before.text == after {
            return false;
        }
        let joins = typing && !before.has_selection() && self.typed == Some(before.cursor);
        if !joins {
            if self.undone.len() >= UNDO_STEPS {
                self.undone.pop_front();
            }
            self.undone.push_back(before);
        }
        self.redone.clear();
        self.typed = typing.then_some(caret);
        joins
    }

    /// Takes the last edit back: what the field is to show now, given what
    /// it shows (`now`, which redo goes forward to again). None if there is
    /// none.
    pub fn undo(&mut self, now: Snapshot) -> Option<Snapshot> {
        let was = self.undone.pop_back()?;
        self.redone.push(now);
        self.typed = None;
        Some(was)
    }

    /// Makes the edit last taken back again: what the field is to show
    /// now, given what it shows (`now`). None if there is none: any new
    /// edit forgets them.
    pub fn redo(&mut self, now: Snapshot) -> Option<Snapshot> {
        let was = self.redone.pop()?;
        if self.undone.len() >= UNDO_STEPS {
            self.undone.pop_front();
        }
        self.undone.push_back(now);
        self.typed = None;
        Some(was)
    }

    /// The caret moved, or the selection changed, by itself: a character
    /// typed next begins a new run.
    pub fn end_run(&mut self) {
        self.typed = None;
    }

    /// Forgets every edit: the program wrote the field's text itself.
    pub fn clear(&mut self) {
        self.undone.clear();
        self.redone.clear();
        self.typed = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undone.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redone.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str, cursor: usize, anchor: usize) -> Snapshot {
        Snapshot {
            text: text.into(),
            cursor,
            anchor,
        }
    }

    /// A field as both apps drive it: `type_char` and `insert` replace the
    /// selection, then record.
    struct Field {
        now: Snapshot,
        history: History,
    }

    impl Field {
        fn new(text: &str) -> Field {
            Field {
                now: at(text, text.len(), text.len()),
                history: History::new(),
            }
        }

        fn put(&mut self, s: &str, typing: bool) -> bool {
            let before = self.now.clone();
            let (a, b) = (
                before.cursor.min(before.anchor),
                before.cursor.max(before.anchor),
            );
            let mut text = before.text.clone();
            text.replace_range(a..b, s);
            let caret = a + s.len();
            self.now = at(&text, caret, caret);
            self.history.record(before, &text, caret, typing)
        }

        fn undo(&mut self) -> bool {
            match self.history.undo(self.now.clone()) {
                Some(was) => {
                    self.now = was;
                    true
                }
                None => false,
            }
        }

        fn redo(&mut self) -> bool {
            match self.history.redo(self.now.clone()) {
                Some(was) => {
                    self.now = was;
                    true
                }
                None => false,
            }
        }

        fn select(&mut self, anchor: usize, cursor: usize) {
            self.now.anchor = anchor;
            self.now.cursor = cursor;
            self.history.end_run();
        }
    }

    /// A run of typing is one edit, the first character typed over a
    /// selection included (R20-M-04: GTK's own history took that one back
    /// apart from the rest); a keypad key or a paste after it is one of
    /// its own; a move ends the run.
    #[test]
    fn typing_is_grouped() {
        let mut f = Field::new("y=a*x+1");
        f.select(0, 7);
        assert!(!f.put("x", true), "begins a run");
        for c in ["+", "a", "*", "x", "+", "2"] {
            assert!(f.put(c, true), "{c} joins");
        }
        assert_eq!(f.now.text, "x+a*x+2");
        assert!(!f.put("7", false), "a keypad key is an edit of its own");
        assert!(!f.put("8", true), "a run doesn't go on after it");
        assert!(f.undo());
        assert_eq!(f.now.text, "x+a*x+27");
        assert!(f.undo());
        assert_eq!(f.now.text, "x+a*x+2");
        assert!(f.undo());
        assert_eq!(f.now, at("y=a*x+1", 7, 0), "one undo, selection back");
        assert!(!f.undo());
        // A move ends a run; so does an undo.
        let mut f = Field::new("");
        f.put("s", true);
        f.put("i", true);
        f.select(1, 1);
        f.select(2, 2);
        assert!(!f.put("n", true), "moved away and back: a new run");
        assert!(f.undo() && f.undo());
        assert_eq!(f.now.text, "");
        // Several characters typed at once (an input method's commit, a
        // compose sequence) are no run of single characters, and what
        // changes nothing is no edit.
        let mut f = Field::new("");
        assert!(!f.put("ab", false));
        assert!(!f.put("", true));
        assert!(f.undo());
        assert!(!f.undo());
    }

    /// Undo puts back the text and the selection as they were before the
    /// edit; redo what the field showed when it was undone; a new edit
    /// forgets what was undone; a text written by the program forgets
    /// everything.
    #[test]
    fn undo_restores_the_selection_and_a_new_edit_clears_redo() {
        let mut f = Field::new("y=a*x+1");
        f.select(6, 7); // "1", the caret at its end
        f.put("22", false);
        assert_eq!(f.now, at("y=a*x+22", 8, 8));
        f.select(2, 0); // "y=", the caret at the start
        f.put("", false);
        assert_eq!(f.now.text, "a*x+22");
        assert!(f.undo());
        assert_eq!(f.now, at("y=a*x+22", 0, 2));
        assert!(f.undo());
        assert_eq!(f.now, at("y=a*x+1", 7, 6));
        assert!(f.history.can_redo());
        assert!(f.redo());
        assert_eq!(f.now, at("y=a*x+22", 0, 2), "as it was when undone");
        assert!(f.redo() && !f.redo());
        assert_eq!(f.now, at("a*x+22", 0, 0));
        assert!(f.undo() && f.undo());
        f.put("3", true);
        assert!(!f.history.can_redo(), "a new edit forgets what was undone");
        assert!(!f.redo());
        assert_eq!(f.now.text, "y=a*x+3");
        f.history.clear();
        assert!(!f.history.can_undo() && !f.undo());
    }

    #[test]
    fn keys() {
        let ctrl = |c: char, shift: bool, alt: bool| KeyPress {
            ctrl: true,
            shift,
            alt,
            ..KeyPress::char(c)
        };
        assert_eq!(chord(&ctrl('z', false, false)), Some(Chord::Undo));
        assert_eq!(chord(&ctrl('Z', true, false)), Some(Chord::Redo));
        assert_eq!(chord(&ctrl('y', false, false)), Some(Chord::Redo));
        assert_eq!(chord(&ctrl('Y', true, false)), None);
        assert_eq!(chord(&ctrl('z', false, true)), None);
        assert_eq!(chord(&KeyPress::char('z')), None);
        assert!(types(&KeyPress::char('x')) && types(&KeyPress::char('A')));
        assert!(!types(&ctrl('x', false, false)));
        assert!(!types(&KeyPress::named(crate::Named::Backspace)));
    }

    /// At most UNDO_STEPS edits are remembered, the oldest forgotten; redo
    /// after undo keeps the bound.
    #[test]
    fn the_history_is_bounded() {
        let mut f = Field::new("");
        for i in 0..UNDO_STEPS + 5 {
            f.put(&format!("{}", i % 10), false);
        }
        assert_eq!(f.history.undone.len(), UNDO_STEPS);
        let mut steps = 0;
        while f.undo() {
            steps += 1;
        }
        assert_eq!(steps, UNDO_STEPS);
        assert_eq!(f.now.text.len(), 5, "the five oldest edits are kept");
        while f.redo() {}
        assert_eq!(f.now.text.len(), UNDO_STEPS + 5);
        assert_eq!(f.history.undone.len(), UNDO_STEPS);
        assert!(!f.history.can_redo());
    }
}
