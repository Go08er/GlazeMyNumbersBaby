//! Undo in GMNB's own text fields from appcore's history (DGMNB's model)
//! instead of GtkText's own (R20-M-04, R20-M-05).
//!
//! GtkText keeps a private undo history no program can add to or keep:
//! `gtk_editable_set_text`, which is what an assistive technology's
//! `SetTextContents` calls (GTK 4.22 `gtkatspieditabletext.c`), clears
//! it, and it takes the first character typed over a selection back apart
//! from the rest of the run (two Ctrl+Z, the field holding that one
//! character in between). So a field [`attach`]ed here has GTK's history
//! off (`enable-undo`), and every change of its text is recorded in an
//! [`appcore::history::History`] as one edit, whatever made it: typing,
//! an input method's commit (the keypad and paste go in that way,
//! [`crate::paste::replace_selection`]), a paste, a drop, a middle click,
//! a cut, Backspace or Delete, and an assistive technology's
//! `SetTextContents`, `InsertText`, `DeleteText`, `CutText` or `PasteText`.
//!
//! An edit is everything one GTK operation does to the text: it begins
//! with its first `insert-text` or `delete-text` (when the text and the
//! selection are read as they were) and ends with the field's one `text`
//! notification. GtkText makes each of its own operations one change (it
//! holds its notifications for it, `begin_change`), and
//! `gtk_editable_set_text` on the entry holds the entry's: an assistive
//! technology's whole replacement, a deletion and an insertion, is one.
//!
//! A character typed by a key pressed in the field is typing: a run of
//! them is one edit, the first one, over a selection, included. A key
//! that isn't typing (a move, Backspace, Ctrl+A), a click in the field
//! and an undo end a run.
//!
//! Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y ([`appcore::history::chord`]), and
//! GTK's Undo and Redo keys, take the field's own keys over from GtkText's.
//! Its context menu has no Undo of GtkText's (GTK 4.22
//! `gtk_text_get_menu_model`: Cut, Copy, Paste, Delete, Select All,
//! Insert Emoji); upstream's equation box has one (Cut, Copy, Paste, Undo,
//! Select All: `EquationInputArea.xaml`, `MathRichEditContextMenu`), so
//! the field's menu gets an Undo, in its own section after GTK's, enabled
//! when there is something to undo, as upstream's (`CanUndo`). (GTK
//! presents neither a GtkText nor anything in it to assistive technology,
//! so that menu, GTK's own entries too, is not in the accessibility tree
//! (GTK 4.22 `gtk_accessible_should_present`); Shift+F10 or the Menu key
//! opens it.)
//!
//! A text the program writes itself ([`FieldUndo::write`]: a range field
//! following the graph) is not the user's edit: it ends the history, as
//! GTK's `set_text` ends GTK's and DGMNB's `TextEdit::set_text` its own.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use appcore::history::{self, Chord, History, Snapshot};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};

/// One field's history ([`attach`]).
pub struct FieldUndo {
    entry: glib::WeakRef<gtk::Entry>,
    text: glib::WeakRef<gtk::Text>,
    history: RefCell<History>,
    /// The text and the selection as they were before the operation now
    /// changing the field, read at its first insertion or deletion.
    before: RefCell<Option<Snapshot>>,
    /// A key that types was pressed in the field and its character hasn't
    /// gone in yet: the edit it makes is typing.
    typing: Cell<bool>,
    /// Putting back what an undo or a redo goes to: not an edit.
    restoring: Cell<bool>,
    /// The program writing the field's text ([`FieldUndo::write`]).
    writing: Cell<bool>,
    /// The context menu's Undo.
    undo_action: gio::SimpleAction,
}

/// What a key pressed in a field does to its history.
#[derive(Debug, PartialEq, Eq)]
enum KeyRole {
    Undo,
    Redo,
    /// It types a character: the edit it makes is typing.
    Types,
    /// Anything else that is a key of its own: a run of typing ends.
    Other,
    /// A modifier by itself, or a key GMNB knows nothing of: nothing.
    Nothing,
}

fn key_role(key: gdk::Key, mods: gdk::ModifierType) -> KeyRole {
    match key {
        gdk::Key::Undo => return KeyRole::Undo,
        gdk::Key::Redo => return KeyRole::Redo,
        _ => {}
    }
    let Some(kp) = crate::keymap::key_press(key, mods) else {
        return KeyRole::Nothing;
    };
    match history::chord(&kp) {
        Some(Chord::Undo) => KeyRole::Undo,
        Some(Chord::Redo) => KeyRole::Redo,
        None if history::types(&kp) => KeyRole::Types,
        None => KeyRole::Other,
    }
}

/// Gives `entry` a [`FieldUndo`] in place of its GtkText's own history.
/// Call once, before the field is shown (its handlers then come before
/// any added when it is realized, [`crate::paste`]'s).
pub fn attach(entry: &gtk::Entry) -> Rc<FieldUndo> {
    let text = entry
        .delegate()
        .and_downcast::<gtk::Text>()
        .expect("a GtkEntry edits through a GtkText");
    text.set_enable_undo(false);
    let undo_action = gio::SimpleAction::new("undo", None);
    undo_action.set_enabled(false);
    let field = Rc::new(FieldUndo {
        entry: entry.downgrade(),
        text: text.downgrade(),
        history: RefCell::default(),
        before: RefCell::default(),
        typing: Cell::new(false),
        restoring: Cell::new(false),
        writing: Cell::new(false),
        undo_action: undo_action.clone(),
    });

    // The state before an operation, at its first change: GtkText's
    // handlers run after these (the signals run their class handler last).
    let f = field.clone();
    text.connect_insert_text(move |_, _, _| f.begin());
    let f = field.clone();
    text.connect_delete_text(move |_, _, _| f.begin());
    // Its end: one notification of the entry's text per operation.
    let f = field.clone();
    entry.connect_notify_local(Some("text"), move |_, _| f.changed());

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let f = field.clone();
    keys.connect_key_pressed(move |_, key, _, mods| f.key(key, mods));
    entry.add_controller(keys);
    let click = gtk::GestureClick::builder()
        .button(0)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    let f = field.clone();
    click.connect_pressed(move |_, _, _, _| f.history.borrow_mut().end_run());
    entry.add_controller(click);

    // The context menu's Undo, as upstream's equation box has it. Weak:
    // the entry holds the action.
    let weak = Rc::downgrade(&field);
    undo_action.connect_activate(move |_, _| {
        if let Some(f) = weak.upgrade() {
            f.step(Chord::Undo);
        }
    });
    let actions = gio::SimpleActionGroup::new();
    actions.add_action(&undo_action);
    entry.insert_action_group("field", Some(&actions));
    let section = gio::Menu::new();
    section.append(Some("_Undo"), Some("field.undo"));
    let menu = gio::Menu::new();
    menu.append_section(None, &section);
    entry.set_extra_menu(Some(&menu));
    field
}

impl FieldUndo {
    /// The field as it is now.
    fn now(&self) -> Option<Snapshot> {
        let text = self.text.upgrade()?;
        let count = |n: i32| usize::try_from(n).unwrap_or(0);
        Some(Snapshot {
            text: text.text().into(),
            cursor: count(text.position()),
            anchor: count(text.property::<i32>("selection-bound")),
        })
    }

    /// An insertion or a deletion is about to be made: the first of an
    /// operation reads the field as it is before it. One that changes
    /// nothing (the field full) has no text notification to end it: the
    /// next idle does.
    fn begin(self: &Rc<Self>) {
        if self.before.borrow().is_none() {
            let now = self.now();
            self.before.replace(now);
            let weak = Rc::downgrade(self);
            glib::idle_add_local_once(move || {
                if let Some(f) = weak.upgrade() {
                    f.before.take();
                }
            });
        }
    }

    /// The entry's text changed: an operation is over.
    fn changed(&self) {
        let before = self.before.take();
        let typing = self.typing.replace(false);
        if self.restoring.get() {
            return;
        }
        let mut history = self.history.borrow_mut();
        if self.writing.get() {
            history.clear();
        } else if let (Some(before), Some(now)) = (before, self.now()) {
            history.record(before, &now.text, now.cursor, typing);
        } else {
            history.end_run();
        }
        self.undo_action.set_enabled(history.can_undo());
    }

    fn key(self: &Rc<Self>, key: gdk::Key, mods: gdk::ModifierType) -> glib::Propagation {
        match key_role(key, mods) {
            KeyRole::Undo => self.step(Chord::Undo),
            KeyRole::Redo => self.step(Chord::Redo),
            KeyRole::Types => {
                self.typing.set(true);
                // A key whose character doesn't go in (the field full, an
                // input method composing) types nothing later either.
                let weak = Rc::downgrade(self);
                glib::idle_add_local_once(move || {
                    if let Some(f) = weak.upgrade() {
                        f.typing.set(false);
                    }
                });
                return glib::Propagation::Proceed;
            }
            KeyRole::Other => {
                self.typing.set(false);
                self.history.borrow_mut().end_run();
                return glib::Propagation::Proceed;
            }
            KeyRole::Nothing => return glib::Propagation::Proceed,
        }
        // The field's own keys: GtkText's would undo its (empty) history.
        glib::Propagation::Stop
    }

    /// Undoes or redoes the field's last edit, if there is one: its text
    /// and selection as they were, told as one change of its text.
    fn step(&self, chord: Chord) {
        let (Some(entry), Some(now)) = (self.entry.upgrade(), self.now()) else {
            return;
        };
        let was = {
            let mut history = self.history.borrow_mut();
            match chord {
                Chord::Undo => history.undo(now),
                Chord::Redo => history.redo(now),
            }
        };
        if let Some(was) = was {
            self.restoring.set(true);
            {
                let _one_change = entry.freeze_notify();
                entry.set_text(&was.text);
                let at = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
                entry.select_region(at(was.anchor), at(was.cursor));
            }
            self.restoring.set(false);
            self.before.take();
        }
        self.undo_action
            .set_enabled(self.history.borrow().can_undo());
    }

    /// Writes `text` into the field for the program (not the user's edit):
    /// the field's history ends, as after GTK's `set_text`. The text it
    /// has already is left alone, history and all.
    pub fn write(&self, text: &str) {
        let Some(entry) = self.entry.upgrade() else {
            return;
        };
        if entry.text() == text {
            return;
        }
        self.writing.set(true);
        entry.set_text(text);
        self.writing.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The field's keys: its history's chords (and GTK's Undo and Redo
    /// keys), a character typed, and the rest.
    #[test]
    fn key_roles() {
        use gdk::Key as K;
        use gdk::ModifierType as M;
        let ctrl = M::CONTROL_MASK;
        assert_eq!(key_role(K::z, ctrl), KeyRole::Undo);
        assert_eq!(key_role(K::Z, ctrl | M::SHIFT_MASK), KeyRole::Redo);
        assert_eq!(key_role(K::y, ctrl), KeyRole::Redo);
        assert_eq!(key_role(K::Undo, M::empty()), KeyRole::Undo);
        assert_eq!(key_role(K::Redo, M::empty()), KeyRole::Redo);
        assert_eq!(key_role(K::z, ctrl | M::ALT_MASK), KeyRole::Other);
        assert_eq!(key_role(K::x, M::empty()), KeyRole::Types);
        assert_eq!(key_role(K::plus, M::SHIFT_MASK), KeyRole::Types);
        assert_eq!(key_role(K::a, ctrl), KeyRole::Other);
        assert_eq!(key_role(K::BackSpace, M::empty()), KeyRole::Other);
        assert_eq!(key_role(K::Left, M::empty()), KeyRole::Other);
        assert_eq!(key_role(K::Shift_L, M::empty()), KeyRole::Nothing);
        assert_eq!(key_role(K::Control_L, M::empty()), KeyRole::Nothing);
    }
}
