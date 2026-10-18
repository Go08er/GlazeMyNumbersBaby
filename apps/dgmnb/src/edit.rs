//! A single-line text editing model (caret, selection, word motion, undo).
//! The UI layer draws it; this file only knows about strings. The undo
//! history is appcore's ([`History`]), the one GMNB's fields keep too.

use appcore::history::{History, Snapshot};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEdit {
    pub text: String,
    /// Caret, a byte offset on a char boundary.
    pub cursor: usize,
    /// Other end of the selection (== cursor when nothing is selected).
    pub anchor: usize,
    /// Longest text accepted, in chars (0 = unlimited).
    pub max_chars: usize,
    /// Uncommitted input-method text shown at the caret.
    pub preedit: String,
    /// The edits Ctrl+Z takes back ([`TextEdit::undo`]); positions in it
    /// are byte offsets, as here.
    history: History,
}

fn prev_boundary(s: &str, i: usize) -> usize {
    s[..i].char_indices().next_back().map_or(0, |(j, _)| j)
}

fn next_boundary(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(i, |c| i + c.len_utf8())
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.'
}

impl TextEdit {
    pub fn new(text: &str, max_chars: usize) -> TextEdit {
        let mut e = TextEdit {
            max_chars,
            ..Default::default()
        };
        e.set_text(text);
        e
    }

    /// Replaces the text from outside (a value the page shows, a session
    /// restored): not an edit of the user's, so there is nothing to undo
    /// after it, as after GTK's `set_text`. The text the field has already
    /// is left alone, history and all, as GMNB's fields leave it.
    pub fn set_text(&mut self, text: &str) {
        let line = appcore::input::single_line(text);
        let line = self.clamp(&line);
        if line == self.text {
            return;
        }
        self.text = line.to_string();
        self.cursor = self.text.len();
        self.anchor = self.cursor;
        self.history.clear();
    }

    fn now(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        }
    }

    /// Makes `change`, remembering the text and the selection before it if
    /// it changes the text: one edit for [`TextEdit::undo`], whatever it
    /// is made of (a selection deleted and a text put in its place are
    /// one). A character typed (`typing`) where the last one typed left
    /// the caret joins that one's edit: a run of typing is taken back
    /// whole ([`History::record`]).
    fn edit(&mut self, typing: bool, change: impl FnOnce(&mut Self)) {
        let was = self.now();
        change(self);
        self.history.record(was, &self.text, self.cursor, typing);
    }

    /// Takes the last edit back (Ctrl+Z): the text as it was before it,
    /// with the selection it had (what a replacement replaced is selected
    /// again). False if there is none.
    pub fn undo(&mut self) -> bool {
        match self.history.undo(self.now()) {
            Some(was) => self.restore(was),
            None => false,
        }
    }

    /// Makes the edit last taken back again (Ctrl+Shift+Z, Ctrl+Y). False
    /// if there is none: any new edit forgets them.
    pub fn redo(&mut self) -> bool {
        match self.history.redo(self.now()) {
            Some(was) => self.restore(was),
            None => false,
        }
    }

    fn restore(&mut self, was: Snapshot) -> bool {
        self.text = was.text;
        self.cursor = was.cursor;
        self.anchor = was.anchor;
        true
    }

    fn clamp<'a>(&self, text: &'a str) -> &'a str {
        if self.max_chars == 0 {
            return text;
        }
        match text.char_indices().nth(self.max_chars) {
            Some((i, _)) => &text[..i],
            None => text,
        }
    }

    pub fn has_selection(&self) -> bool {
        self.cursor != self.anchor
    }

    pub fn selection(&self) -> (usize, usize) {
        (self.cursor.min(self.anchor), self.cursor.max(self.anchor))
    }

    pub fn selected_text(&self) -> &str {
        let (a, b) = self.selection();
        &self.text[a..b]
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
        self.history.end_run();
    }

    /// Deletes the selection (a cut): an edit of its own.
    pub fn delete_selection(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }
        self.edit(false, |e| {
            e.remove_selection();
        });
        true
    }

    fn remove_selection(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }
        let (a, b) = self.selection();
        self.text.replace_range(a..b, "");
        self.cursor = a;
        self.anchor = a;
        true
    }

    /// Insert at the caret (replacing any selection); the shared single-line
    /// rule and the length limit apply, including to input-method commits. One edit
    /// (a paste, a keypad key, an assistive technology's text).
    pub fn insert(&mut self, s: &str) {
        self.edit(false, |e| e.put(s));
    }

    /// Replaces the whole field as one user edit (an assistive
    /// technology's SetValue), rather than resetting its undo history as
    /// an application-driven [`TextEdit::set_text`] does.
    pub fn replace(&mut self, s: &str) {
        self.select_all();
        self.insert(s);
    }

    /// [`TextEdit::insert`] for what the keyboard types: a character typed
    /// straight after another is one edit with it.
    pub fn type_in(&mut self, s: &str) {
        let one = s.chars().count() == 1;
        self.edit(one, |e| e.put(s));
    }

    fn put(&mut self, s: &str) {
        self.remove_selection();
        let clean = appcore::input::single_line(s);
        let room = if self.max_chars == 0 {
            usize::MAX
        } else {
            self.max_chars.saturating_sub(self.text.chars().count())
        };
        let clean: String = clean.chars().take(room).collect();
        self.text.insert_str(self.cursor, &clean);
        self.cursor += clean.len();
        self.anchor = self.cursor;
    }

    pub fn backspace(&mut self, word: bool) {
        self.edit(false, |e| {
            if e.remove_selection() || e.cursor == 0 {
                return;
            }
            let start = if word {
                e.word_left(e.cursor)
            } else {
                prev_boundary(&e.text, e.cursor)
            };
            e.text.replace_range(start..e.cursor, "");
            e.cursor = start;
            e.anchor = start;
        });
    }

    pub fn delete(&mut self, word: bool) {
        self.edit(false, |e| {
            if e.remove_selection() || e.cursor >= e.text.len() {
                return;
            }
            let end = if word {
                e.word_right(e.cursor)
            } else {
                next_boundary(&e.text, e.cursor)
            };
            e.text.replace_range(e.cursor..end, "");
            e.anchor = e.cursor;
        });
    }

    fn word_left(&self, mut i: usize) -> usize {
        let s = &self.text;
        while i > 0 && !s[..i].chars().next_back().is_some_and(is_word) {
            i = prev_boundary(s, i);
        }
        while i > 0 && s[..i].chars().next_back().is_some_and(is_word) {
            i = prev_boundary(s, i);
        }
        i
    }

    fn word_right(&self, mut i: usize) -> usize {
        let s = &self.text;
        while i < s.len() && !s[i..].chars().next().is_some_and(is_word) {
            i = next_boundary(s, i);
        }
        while i < s.len() && s[i..].chars().next().is_some_and(is_word) {
            i = next_boundary(s, i);
        }
        i
    }

    pub fn left(&mut self, select: bool, word: bool) {
        self.history.end_run();
        if !select && self.has_selection() {
            self.cursor = self.selection().0;
        } else {
            self.cursor = if word {
                self.word_left(self.cursor)
            } else {
                prev_boundary(&self.text, self.cursor)
            };
        }
        if !select {
            self.anchor = self.cursor;
        }
    }

    pub fn right(&mut self, select: bool, word: bool) {
        self.history.end_run();
        if !select && self.has_selection() {
            self.cursor = self.selection().1;
        } else {
            self.cursor = if word {
                self.word_right(self.cursor)
            } else {
                next_boundary(&self.text, self.cursor)
            };
        }
        if !select {
            self.anchor = self.cursor;
        }
    }

    pub fn home(&mut self, select: bool) {
        self.history.end_run();
        self.cursor = 0;
        if !select {
            self.anchor = 0;
        }
    }

    pub fn end(&mut self, select: bool) {
        self.history.end_run();
        self.cursor = self.text.len();
        if !select {
            self.anchor = self.cursor;
        }
    }

    /// Put the caret at `offset` (snapped to a char boundary).
    pub fn place(&mut self, mut offset: usize, select: bool) {
        self.history.end_run();
        offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        self.cursor = offset;
        if !select {
            self.anchor = offset;
        }
    }

    /// Select the word around `offset` (double click).
    pub fn select_word(&mut self, offset: usize) {
        self.place(offset, false);
        let a = self.word_left(self.cursor);
        let b = self.word_right(a);
        self.anchor = a;
        self.cursor = b.max(self.cursor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_editing_and_selection() {
        let mut e = TextEdit::new("", 10);
        e.insert("sin(x)+1");
        assert_eq!(e.text, "sin(x)+1");
        e.left(false, false);
        e.backspace(false);
        assert_eq!(e.text, "sin(x)1");
        e.home(false);
        e.right(true, true);
        assert_eq!(e.selected_text(), "sin");
        e.insert("cos");
        assert_eq!(e.text, "cos(x)1");
        e.select_all();
        e.delete(false);
        assert!(e.text.is_empty());
    }

    #[test]
    fn limits_and_unicode() {
        let mut e = TextEdit::new("", 4);
        e.insert("π√xyz\n");
        assert_eq!(e.text, "π√xy");
        e.backspace(false);
        e.backspace(false);
        assert_eq!(e.text, "π√");
        e.left(false, false);
        e.delete(false);
        assert_eq!(e.text, "π");
        e.place(1, false); // inside π: snaps back
        assert_eq!(e.cursor, 0);
        // Direct insertions (including input-method commits) and external
        // whole replacements follow the same rule as clipboard paste.
        for text in ["\n\n y=x\ny=2", "\r\n y=x\ry=2", "\u{2028} y=x\u{2029}y=2"] {
            let mut e = TextEdit::new("before", 0);
            e.select_all();
            e.insert(text);
            assert_eq!(e.text, " y=x");
            assert!(e.undo());
            assert_eq!(e.selected_text(), "before");
            assert!(e.redo());
            assert_eq!(e.text, " y=x");
            e.set_text(text);
            assert_eq!(e.text, " y=x");
        }
        let mut e = TextEdit::new("", 0);
        e.insert("12\t34");
        assert_eq!(e.text, "12 34");
    }

    /// Undo and redo (Ctrl+Z, Ctrl+Shift+Z), as GTK's fields and
    /// upstream's RichEditBox have them: each edit is taken back whole,
    /// the selection it replaced selected again; a run of typing is one
    /// edit; a paste, a keypad key, a cut and a backspace are each their
    /// own; a new edit forgets what was undone; a text set from outside
    /// forgets everything.
    #[test]
    fn undo_and_redo() {
        let state = |e: &TextEdit| (e.text.clone(), e.selected_text().to_string());
        let mut e = TextEdit::new("y=a*x+1", 0);
        assert!(!e.undo() && !e.redo(), "nothing yet");
        // A keypad key over the whole text selected: one edit.
        e.select_all();
        e.insert("7");
        assert_eq!(e.text, "7");
        assert!(e.undo());
        assert_eq!(state(&e), ("y=a*x+1".into(), "y=a*x+1".into()));
        assert!(e.redo());
        assert_eq!(state(&e), ("7".into(), String::new()));
        assert!(!e.redo());
        assert!(e.undo());
        // Backspace over a selection, a cut, a paste over a part.
        e.backspace(false);
        assert_eq!(e.text, "");
        assert!(e.undo());
        e.end(false);
        e.left(true, false);
        assert!(e.delete_selection());
        assert_eq!(e.text, "y=a*x+");
        e.insert("22");
        assert_eq!(e.text, "y=a*x+22");
        assert!(e.undo());
        assert_eq!(e.text, "y=a*x+");
        assert!(e.undo());
        assert_eq!(state(&e), ("y=a*x+1".into(), "1".into()));
        assert!(e.redo() && e.redo() && !e.redo());
        assert_eq!(e.text, "y=a*x+22");
        // Typing is one edit while it goes on; a move ends the run, and so
        // does anything that isn't typed.
        let mut e = TextEdit::new("", 0);
        for c in ["s", "i", "n"] {
            e.type_in(c);
        }
        e.insert("(");
        e.type_in("x");
        e.type_in(")");
        e.left(false, false);
        e.type_in("2");
        assert_eq!(e.text, "sin(x2)");
        let mut back = Vec::new();
        while e.undo() {
            back.push(e.text.clone());
        }
        assert_eq!(back, ["sin(x)", "sin(", "sin", ""]);
        // A new edit forgets what was undone.
        assert!(e.redo());
        assert_eq!(e.text, "sin");
        e.type_in("h");
        assert!(!e.redo());
        assert!(e.undo());
        assert_eq!(e.text, "sin");
        // What changes nothing is no edit: nothing to take back for it.
        let mut e = TextEdit::new("x", 1);
        e.home(false);
        e.backspace(false);
        e.end(false);
        e.delete(false);
        e.insert("y");
        assert!(!e.undo(), "{}", e.text);
        // A text set from outside is not an edit, and ends the history;
        // the text the field has already changes nothing.
        e.select_all();
        e.type_in("z");
        e.set_text("z");
        assert!(e.undo() && e.redo());
        e.set_text("w");
        assert!(!e.undo() && !e.redo());
        // AT-SPI whole-field replacement is a user edit, unlike a value
        // written by the application itself.
        let mut e = TextEdit::new("w", 0);
        e.replace("new");
        assert_eq!(e.text, "new");
        assert!(e.undo());
        assert_eq!(state(&e), ("w".into(), "w".into()));
        assert!(e.redo());
        assert_eq!(e.text, "new");
    }

    #[test]
    fn word_motion() {
        let mut e = TextEdit::new("x^2 + 3.5*y", 0);
        e.backspace(true);
        assert_eq!(e.text, "x^2 + 3.5*");
        e.select_word(1);
        assert_eq!(e.selected_text(), "x");
    }
}
