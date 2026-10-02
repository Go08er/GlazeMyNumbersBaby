//! A single-line text editing model (caret, selection, word motion). The
//! UI layer draws it; this file only knows about strings.

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

    pub fn set_text(&mut self, text: &str) {
        self.text = self.clamp(text).to_string();
        self.cursor = self.text.len();
        self.anchor = self.cursor;
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
    }

    pub fn delete_selection(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }
        let (a, b) = self.selection();
        self.text.replace_range(a..b, "");
        self.cursor = a;
        self.anchor = a;
        true
    }

    /// Insert at the caret (replacing any selection); newlines and control
    /// characters are dropped and the length limit is respected.
    pub fn insert(&mut self, s: &str) {
        self.delete_selection();
        let clean: String = s.chars().filter(|c| !c.is_control()).collect();
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
        if self.delete_selection() || self.cursor == 0 {
            return;
        }
        let start = if word {
            self.word_left(self.cursor)
        } else {
            prev_boundary(&self.text, self.cursor)
        };
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
        self.anchor = start;
    }

    pub fn delete(&mut self, word: bool) {
        if self.delete_selection() || self.cursor >= self.text.len() {
            return;
        }
        let end = if word {
            self.word_right(self.cursor)
        } else {
            next_boundary(&self.text, self.cursor)
        };
        self.text.replace_range(self.cursor..end, "");
        self.anchor = self.cursor;
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
        self.cursor = 0;
        if !select {
            self.anchor = 0;
        }
    }

    pub fn end(&mut self, select: bool) {
        self.cursor = self.text.len();
        if !select {
            self.anchor = self.cursor;
        }
    }

    /// Put the caret at `offset` (snapped to a char boundary).
    pub fn place(&mut self, mut offset: usize, select: bool) {
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
