//! Bounded clipboard reads for every paste.
//!
//! GTK's own text fields read the whole clipboard before inserting any of
//! it (GTK 4.22: `gtk_text_paste` reads it with
//! `gdk_clipboard_read_text_async`, and a text view reads it into a text
//! buffer even when it isn't editable), so a field's length limit only
//! applies after the read: pasting a 100 MB clipboard into an equation
//! took 600 MB. [`guard_all`] makes every field paste through
//! [`read_text`] instead: Ctrl+V, Shift+Insert and the context menu's Paste
//! (all the `paste-clipboard` signal) and a middle click (the primary
//! selection). Every field: GMNB's, and GTK's own in windows GTK makes, such
//! as the colour chooser's (a toplevel of its own, with a hexadecimal entry
//! and spin buttons), whenever they are made.
//!
//! The read is the offer's pipe, which closing stops. Text dragged onto a
//! field from another program is read the same way (GTK's drop target reads
//! it whole, as its paste does): from the drop's stream.
//!
//! One read is out of reach: an assistive technology's EditableText
//! `PasteText` request, which GTK answers by reading the clipboard whole
//! itself (GTK 4.22 `gtkatspieditabletext.c`), with no signal on the way.

use std::rc::Rc;

use glib::translate::{Borrowed, FromGlibPtrBorrow, IntoGlib};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};

/// Every field's and page's own limit is far below this (an equation is at
/// most 1000 characters); a longer clipboard is cut here, while reading,
/// instead of being read whole first.
pub const MAX_PASTE_BYTES: usize = 64 * 1024;

/// Marks a widget [`guard`] has seen.
const GUARDED: &str = "gmnb-bounded-paste";

/// The text formats read, best first.
const TEXT_TYPES: [&str; 2] = ["text/plain;charset=utf-8", "text/plain"];

/// The text on `clipboard`, cut to [`MAX_PASTE_BYTES`] (at a character
/// boundary) while it is read. `None` if it has no text.
pub async fn read_text(clipboard: &gdk::Clipboard) -> Option<String> {
    let (stream, _) = clipboard
        .read_future(&TEXT_TYPES, glib::Priority::DEFAULT)
        .await
        .ok()?;
    Some(read_stream(&stream).await)
}

/// The text dropped from another program, read as [`read_text`] reads a
/// clipboard.
async fn read_drop(drop: &gdk::Drop) -> Option<String> {
    let (stream, _) = drop
        .read_future(&TEXT_TYPES, glib::Priority::DEFAULT)
        .await
        .ok()?;
    Some(read_stream(&stream).await)
}

/// The text in `stream`, cut to [`MAX_PASTE_BYTES`]; the stream is closed
/// as soon as that much is read.
async fn read_stream(stream: &gio::InputStream) -> String {
    let mut bytes = Vec::new();
    while bytes.len() <= MAX_PASTE_BYTES {
        match stream
            .read_bytes_future(MAX_PASTE_BYTES + 1 - bytes.len(), glib::Priority::DEFAULT)
            .await
        {
            Ok(chunk) if !chunk.is_empty() => bytes.extend_from_slice(&chunk),
            _ => break,
        }
    }
    let _ = stream.close_future(glib::Priority::DEFAULT).await;
    cut(bytes)
}

/// `bytes` as text, at most [`MAX_PASTE_BYTES`] of them, without a
/// character cut in two.
fn cut(mut bytes: Vec<u8>) -> String {
    bytes.truncate(MAX_PASTE_BYTES);
    if let Err(e) = std::str::from_utf8(&bytes)
        && e.error_len().is_none()
    {
        bytes.truncate(e.valid_up_to());
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Makes every text field the program shows paste through [`read_text`]:
/// each GtkText and GtkTextView is guarded as it is realized, which a field
/// is before it can take a key, a click or a drop. That covers every
/// window, those GTK makes for itself included, and fields made at any
/// time, without knowing where they are. Call once, before any widget is
/// realized.
pub fn guard_all() {
    unsafe extern "C" fn realized(
        _hint: *mut glib::gobject_ffi::GSignalInvocationHint,
        n_values: u32,
        values: *const glib::gobject_ffi::GValue,
        _data: glib::ffi::gpointer,
    ) -> glib::ffi::gboolean {
        if n_values > 0 {
            // SAFETY: an emission's first value is the instance emitting,
            // here a GtkWidget (this hook is on GtkWidget::realize), alive
            // for the emission.
            let widget: Borrowed<gtk::Widget> = unsafe {
                let instance = glib::gobject_ffi::g_value_get_object(values);
                gtk::Widget::from_glib_borrow(instance.cast())
            };
            guard(&widget);
        }
        glib::ffi::GTRUE // stay installed
    }
    // A class's signals exist once the class does: maybe not yet, before
    // the first widget.
    let widget = gtk::Widget::static_type();
    let _class = glib::Class::<gtk::Widget>::from_type(widget);
    let realize = glib::subclass::signal::SignalId::lookup("realize", widget)
        .expect("GtkWidget has a realize signal");
    // SAFETY: the hook matches GSignalEmissionHook, and keeps no data.
    unsafe {
        glib::gobject_ffi::g_signal_add_emission_hook(
            realize.into_glib(),
            0,
            Some(realized),
            std::ptr::null_mut(),
            None,
        );
    }
}

/// Makes `widget` paste through [`read_text`] if it is a text field. One
/// already guarded is left alone.
fn guard(widget: &gtk::Widget) {
    if let Some(text) = widget.downcast_ref::<gtk::Text>() {
        guard_text(text);
    } else if let Some(view) = widget.downcast_ref::<gtk::TextView>() {
        guard_text_view(view);
    }
}

/// Whether `widget` is guarded now (marking it if not).
fn first_time(widget: &gtk::Widget) -> bool {
    // SAFETY: GUARDED is only ever set, and read, here, always as ().
    unsafe {
        if widget.data::<()>(GUARDED).is_some() {
            return false;
        }
        widget.set_data(GUARDED, ());
    }
    true
}

/// A middle-click gesture that runs before the widget's own (which pastes
/// the primary selection read whole) and claims every middle press, so
/// that one never sees them (a second press it saw would be its first).
/// `paste` gets the widget and the position of a single click. If
/// middle-click paste is off (`gtk-enable-primary-paste`) the press is left
/// alone: the widget doesn't paste then either.
fn middle_click(widget: &gtk::Widget, paste: impl Fn(&gtk::Widget, f64, f64) + 'static) {
    let click = gtk::GestureClick::builder()
        .button(gdk::BUTTON_MIDDLE)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    click.connect_pressed(move |gesture, n_press, x, y| {
        let Some(widget) = gesture.widget() else {
            return;
        };
        if !widget.settings().is_gtk_enable_primary_paste() {
            return;
        }
        gesture.set_state(gtk::EventSequenceState::Claimed);
        if n_press == 1 {
            paste(&widget, x, y);
        }
    });
    widget.add_controller(click);
}

/// A drop target that runs before the widget's own, which reads a drag's
/// text whole before inserting any (GTK 4.22 `GtkDropTarget`), and takes
/// the drop of text dragged from another program: read by [`read_drop`] and
/// given to `insert` with where it was dropped, or, if the widget isn't
/// editable, refused unread. Drags from GMNB itself (their text already in
/// memory) are left to the widget's own target, and so is every drag's
/// feedback until the drop (where the text would go, whether it's taken).
fn drop_target(widget: &gtk::Widget, insert: impl Fn(&gtk::Widget, String, f64, f64) + 'static) {
    let target = gtk::DropTargetAsync::new(None, gdk::DragAction::COPY | gdk::DragAction::MOVE);
    target.set_propagation_phase(gtk::PropagationPhase::Capture);
    target.connect_accept(|_, drop| {
        let formats = drop.formats();
        drop.drag().is_none()
            && TEXT_TYPES.iter().any(|t| formats.contain_mime_type(t))
            && drop_action(drop.actions()) != gdk::DragAction::empty()
    });
    // No preference: the widget's own target answers.
    target.connect_drag_enter(|_, _, _, _| gdk::DragAction::empty());
    target.connect_drag_motion(|_, _, _, _| gdk::DragAction::empty());
    let insert = Rc::new(insert);
    target.connect_drop(move |target, drop, x, y| {
        let Some(widget) = target.widget() else {
            return false;
        };
        forget_drop(&widget);
        let (dnd, weak, insert) = (drop.clone(), widget.downgrade(), insert.clone());
        let editable = is_editable(&widget);
        glib::spawn_future_local(async move {
            let dropped = match editable {
                true => read_drop(&dnd).await,
                false => None,
            };
            let action = match (dropped, weak.upgrade()) {
                (Some(text), Some(widget)) if is_editable(&widget) => {
                    insert(&widget, text, x, y);
                    drop_action(dnd.actions())
                }
                _ => gdk::DragAction::empty(),
            };
            dnd.finish(action);
        });
        true
    });
    widget.add_controller(target);
}

/// The action a drop offering `actions` is finished with, as GTK's own
/// targets pick it: a copy where the source allows one.
fn drop_action(actions: gdk::DragAction) -> gdk::DragAction {
    [gdk::DragAction::COPY, gdk::DragAction::MOVE]
        .into_iter()
        .find(|&a| actions.contains(a))
        .unwrap_or(gdk::DragAction::empty())
}

/// Lets go of the drop `widget`'s own targets are holding: they saw the
/// drag come in but not the drop, which [`drop_target`] took, so they would
/// keep the finished drop until the pointer next leaves the widget. This
/// leaves them as their own drop does.
fn forget_drop(widget: &gtk::Widget) {
    let controllers = widget.observe_controllers();
    for target in (0..controllers.n_items())
        .filter_map(|i| controllers.item(i).and_downcast::<gtk::DropTarget>())
    {
        target.reject();
    }
}

fn is_editable(widget: &gtk::Widget) -> bool {
    if let Some(text) = widget.downcast_ref::<gtk::Text>() {
        text.is_editable()
    } else {
        widget
            .downcast_ref::<gtk::TextView>()
            .is_some_and(|view| view.is_editable())
    }
}

fn guard_text(text: &gtk::Text) {
    if !first_time(text.upcast_ref()) {
        return;
    }
    // Runs before the class handler, which this stops.
    text.connect_local("paste-clipboard", false, |args| {
        let text = args[0].get::<gtk::Text>().ok()?;
        text.stop_signal_emission_by_name("paste-clipboard");
        paste_into_text(&text, text.clipboard(), None);
        None
    });
    middle_click(text.upcast_ref(), |widget, x, _| {
        let Some(text) = widget.downcast_ref::<gtk::Text>() else {
            return;
        };
        let at = position_at(text, x);
        if !text.has_focus() {
            // Not grab_focus: a field that selects all on focus would then
            // have the paste replace everything.
            text.grab_focus_without_selecting();
        }
        paste_into_text(text, text.primary_clipboard(), Some(at));
    });
    drop_target(text.upcast_ref(), |widget, dropped, x, _| {
        if let Some(text) = widget.downcast_ref::<gtk::Text>() {
            drop_into_text(text, dropped, x);
        }
    });
}

/// GtkText's drop (`gtk_text_drag_drop`), with the text read by
/// [`read_drop`]: inserted where it was dropped, or in place of the
/// selection if dropped on it, then as one edit ([`replace_selection`]).
/// The cursor stays where it was (before the text, in place of a
/// selection).
fn drop_into_text(text: &gtk::Text, dropped: String, x: f64) {
    // One line of it, as of a paste ([`paste_into_text`]).
    let dropped = appcore::input::single_line(&dropped);
    if dropped.is_empty() {
        return;
    }
    let at = position_at(text, x);
    let _one_edit = text.freeze_notify();
    match text.selection_bounds() {
        Some((a, b)) if dropped_on((a, b), at) => {
            replace_selection(text, &dropped);
            text.set_position(a.min(b));
        }
        selection => {
            // An edit like any other ([`replace_selection`], with nothing
            // selected at the place): GtkText's own insertion, asked of it
            // directly, ended the field's undo history (R19-M-03's kind).
            // The cursor and the selection stay on the text they were on.
            let (cursor, before) = (text.position(), i32::from(text.text_length()));
            text.select_region(at, at);
            replace_selection(text, &dropped);
            let added = i32::from(text.text_length()) - before;
            match selection {
                Some((a, b)) => {
                    text.select_region(after_insertion(a, at, added), after_insertion(b, at, added))
                }
                None => text.set_position(after_insertion(cursor, at, added)),
            }
        }
    }
}

/// Whether a drop at `at` is on the selection `(a, b)` (its ends included,
/// in either order): it then takes the selection's place, as in GtkText's
/// own drop.
fn dropped_on((a, b): (i32, i32), at: i32) -> bool {
    (a.min(b)..=a.max(b)).contains(&at)
}

/// Where the position `p` of a text is once `added` characters have gone
/// in at `at`: on the same text as before, so moved along if they went in
/// before it (or at it).
fn after_insertion(p: i32, at: i32, added: i32) -> i32 {
    if p >= at { p + added } else { p }
}

/// Puts `with` in place of `text`'s selection (or at its cursor, with
/// none), the cursor after it, as typing it there does: one edit, one
/// change of its text property for a caller holding its notifications
/// (`freeze_notify`), and one step of its undo. Ctrl+Z takes it back
/// whole, the text gone and what it replaced there again, selected, as
/// GTK takes back a selection typed over. (The graphing equation field
/// tells its graph of its text only once an operation is over, R17-M-03;
/// with one step of undo the field is not left empty between two Ctrl+Z
/// either, where the graph would forget the sliders the text still uses,
/// R19-M-03.)
///
/// As the text's input method does it: GtkText takes what is typed from
/// its input method context's `commit` ([`im_context`]), and makes of it
/// one undoable change, selection replaced (GTK 4.22 `gtk_text_commit_cb`,
/// `gtk_text_enter_text`). No other way in is one step: GtkText takes a
/// program's direct insertion or deletion as irreversible and clears its
/// undo history for it (`gtk_text_insert_text`, `gtk_text_delete_text`),
/// and its key-binding signals `delete-from-cursor` and `insert-at-cursor`
/// are a step each, with the field as the first left it between two
/// Ctrl+Z. Those are what is left without an input method context.
pub(crate) fn replace_selection(text: &gtk::Text, with: &str) {
    match im_context(text) {
        Some(typed) if !with.is_empty() => typed.emit_by_name::<()>("commit", &[&with]),
        _ => {
            if text.selection_bounds().is_some() {
                // With a selection, it is what goes, whatever the count.
                text.emit_delete_from_cursor(gtk::DeleteType::Chars, 1);
            }
            if !with.is_empty() {
                text.emit_insert_at_cursor(with);
            }
        }
    }
}

/// The input method context `text` types from: the one its own key
/// controller has (GtkText gives it its context so that the keys go
/// through the input method first).
fn im_context(text: &gtk::Text) -> Option<gtk::IMContext> {
    text.observe_controllers()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|c| c.downcast::<gtk::EventControllerKey>().ok())
        .find_map(|keys| keys.im_context())
}

/// The character position nearest to `x` (in `text`'s coordinates), as
/// GTK finds where a middle click pastes.
///
/// GTK's own lookup isn't public (nor its layout), so this asks GTK where
/// the cursor would be, each answer costing a walk of the text. Text
/// written one way has the cursor move one way along it, so a bisection
/// asks a logarithmic number of times; text with right-to-left characters
/// can mix directions, and there every position is asked (quadratic in
/// the field's length, as before).
fn position_at(text: &gtk::Text, x: f64) -> i32 {
    let content = text.text();
    let len = content.chars().count();
    let at = |position: usize| f64::from(text.compute_cursor_extents(position).0.x());
    let position = if content.chars().any(right_to_left) {
        let distance = |position: usize| (at(position) - x).abs();
        (0..=len)
            .min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
            .unwrap_or(0)
    } else {
        nearest_along(len, at, x)
    };
    i32::try_from(position).unwrap_or(i32::MAX)
}

/// Whether `c` is written right to left (or forces that), so that text
/// holding it may mix directions.
fn right_to_left(c: char) -> bool {
    matches!(c,
        '\u{0590}'..='\u{08FF}' // Hebrew, Arabic, Syriac, Thaana, N'Ko...
        | '\u{200F}' | '\u{202B}' | '\u{202E}' | '\u{2067}' // RLM, RLE, RLO, RLI
        | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}' // presentation forms
        | '\u{10800}'..='\u{10FFF}' | '\u{1E800}'..='\u{1EFFF}')
}

/// Of the positions `0..=len`, whose x (`at`) runs one way, the first one
/// nearest to `x`, found by bisection.
fn nearest_along(len: usize, at: impl Fn(usize) -> f64, x: f64) -> usize {
    let rising = at(len) >= at(0);
    // The first position at or past x.
    let (mut lo, mut hi) = (0, len);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let past = if rising { at(mid) >= x } else { at(mid) <= x };
        if past {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let mut best = lo;
    if lo > 0 && (at(lo - 1) - x).abs() <= (at(lo) - x).abs() {
        best = lo - 1;
    }
    // Several positions at one x (inside a cluster): the first of them.
    let here = at(best);
    while best > 0 && at(best - 1) == here {
        best -= 1;
    }
    best
}

/// GtkText's paste (`paste_received`), with the text read by [`read_text`]:
/// replaces the selection, or inserts at the cursor, as one edit
/// ([`replace_selection`]); a middle click pastes at `at`, which moves the
/// cursor there unless it is in the selection.
fn paste_into_text(text: &gtk::Text, clipboard: gdk::Clipboard, at: Option<i32>) {
    if !text.is_editable() {
        text.error_bell();
        return;
    }
    let weak = text.downgrade();
    glib::spawn_future_local(async move {
        let pasted = read_text(&clipboard).await;
        let Some(text) = weak.upgrade() else {
            return;
        };
        let (Some(pasted), true) = (pasted, text.is_editable()) else {
            text.error_bell();
            return;
        };
        // A GtkText is one line: what a single-line field takes of a text
        // (appcore's rule, DGMNB's too): the first line that isn't blank,
        // tabs as spaces. GTK's own paste keeps every line, with the line
        // breaks in the field, unless the field asks for the first
        // (truncate-multiline), and then takes it blank or not.
        let pasted = appcore::input::single_line(&pasted);
        if pasted.is_empty() {
            return;
        }
        let _one_edit = text.freeze_notify();
        if let Some(at) = at {
            let in_selection = text
                .selection_bounds()
                .is_some_and(|(a, b)| (a.min(b)..=a.max(b)).contains(&at));
            if !in_selection {
                text.select_region(at, at);
            }
        }
        replace_selection(&text, &pasted);
    });
}

fn guard_text_view(view: &gtk::TextView) {
    if !first_time(view.upcast_ref()) {
        return;
    }
    view.connect_local("paste-clipboard", false, |args| {
        let view = args[0].get::<gtk::TextView>().ok()?;
        view.stop_signal_emission_by_name("paste-clipboard");
        paste_into_text_view(&view, view.clipboard(), None);
        None
    });
    middle_click(view.upcast_ref(), |widget, x, y| {
        let Some(view) = widget.downcast_ref::<gtk::TextView>() else {
            return;
        };
        let (bx, by) =
            view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
        let at = view.iter_at_location(bx, by).map(|iter| iter.offset());
        paste_into_text_view(view, view.primary_clipboard(), at);
    });
    drop_target(view.upcast_ref(), |widget, dropped, x, y| {
        if let Some(view) = widget.downcast_ref::<gtk::TextView>() {
            drop_into_text_view(view, &dropped, x, y);
        }
    });
}

/// A text view's drop (`gtk_text_view_drag_drop`), with the text read by
/// [`read_drop`]: inserted where it was dropped, if text can go there, with
/// the cursor after it.
fn drop_into_text_view(view: &gtk::TextView, dropped: &str, x: f64, y: f64) {
    let (bx, by) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
    let editable = view.is_editable();
    let Some(mut at) = view
        .iter_at_location(bx, by)
        .filter(|at| at.can_insert(editable))
    else {
        return;
    };
    let buffer = view.buffer();
    buffer.begin_user_action();
    buffer.insert_interactive(&mut at, dropped, editable);
    buffer.place_cursor(&at);
    buffer.end_user_action();
}

/// A text view's paste with the text read by [`read_text`], as one edit
/// (`paste_from_buffer`: one user action, so one undo step) told once to
/// the buffer's text property. One that isn't editable only inserts
/// nothing in GTK (after reading the clipboard whole); here it reads
/// nothing either.
fn paste_into_text_view(view: &gtk::TextView, clipboard: gdk::Clipboard, at: Option<i32>) {
    if !view.is_editable() {
        return;
    }
    let weak = view.downgrade();
    glib::spawn_future_local(async move {
        let pasted = read_text(&clipboard).await;
        let (Some(view), Some(pasted)) = (weak.upgrade(), pasted) else {
            return;
        };
        let buffer = view.buffer();
        if let Some(at) = at {
            let in_selection = buffer
                .selection_bounds()
                .is_some_and(|(start, end)| (start.offset()..=end.offset()).contains(&at));
            if !in_selection {
                buffer.place_cursor(&buffer.iter_at_offset(at));
            }
        }
        let editable = view.is_editable();
        {
            let _one_edit = buffer.freeze_notify();
            buffer.begin_user_action();
            buffer.delete_selection(true, editable);
            buffer.insert_interactive_at_cursor(&pasted, editable);
            buffer.end_user_action();
        }
        view.scroll_mark_onscreen(&buffer.get_insert());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A text dropped into a field: on the selection it takes its place;
    /// beside it, it goes in where it was dropped and the selection (or
    /// the cursor) stays on the text it was on.
    #[test]
    fn a_drop_on_the_selection_or_beside_it() {
        // "abc+def" with "abc" selected (0..3), either way round.
        for selection in [(0, 3), (3, 0)] {
            for at in 0..=3 {
                assert!(dropped_on(selection, at), "{selection:?} {at}");
            }
            for at in 4..=7 {
                assert!(!dropped_on(selection, at), "{selection:?} {at}");
            }
        }
        assert!(!dropped_on((2, 5), 1));
        // Three characters dropped at the end (7): the selection stays.
        assert_eq!((after_insertion(0, 7, 3), after_insertion(3, 7, 3)), (0, 3));
        // Dropped at 1, before "+def" selected (3..7): it moves along.
        assert_eq!(
            (after_insertion(3, 1, 3), after_insertion(7, 1, 3)),
            (6, 10)
        );
        // A cursor at the place itself ends up after the text, one before
        // it stays, and nothing added moves nothing.
        assert_eq!(after_insertion(4, 4, 2), 6);
        assert_eq!(after_insertion(3, 4, 2), 3);
        assert_eq!(after_insertion(5, 2, 0), 5);
    }

    /// Bisection finds the first nearest position, as asking every
    /// position would, in a logarithmic number of questions.
    #[test]
    fn nearest_position_by_bisection() {
        let check = |xs: &[f64]| {
            let len = xs.len() - 1;
            for i in -10..=10 * xs.len() as i32 {
                let x = f64::from(i) / 3.0 - 1.0;
                let every = (0..=len)
                    .min_by(|&a, &b| (xs[a] - x).abs().total_cmp(&(xs[b] - x).abs()))
                    .unwrap();
                assert_eq!(nearest_along(len, |p| xs[p], x), every, "{xs:?} at {x}");
            }
        };
        check(&[0.0]);
        check(&[0.0, 7.0]);
        // Uneven advances, and a cluster (two positions at one x).
        check(&[0.0, 5.0, 7.0, 7.0, 15.0, 16.0, 30.0]);
        // Right-aligned right-to-left text: x falls along it.
        check(&[30.0, 22.0, 21.0, 9.0, 9.0, 0.0]);
        // A 64 KiB field: about 2 log2(n) questions, not n.
        let asked = std::cell::Cell::new(0);
        let at = |p: usize| {
            asked.set(asked.get() + 1);
            p as f64 * 7.5
        };
        assert_eq!(nearest_along(65_536, at, 1234.0 * 7.5 + 3.0), 1234);
        assert!(asked.get() < 40, "{} questions", asked.get());
        assert!(right_to_left('ש') && right_to_left('ب') && !right_to_left('x'));
    }

    #[test]
    fn reads_are_cut_at_a_character_boundary() {
        assert_eq!(cut(b"sin(x)".to_vec()), "sin(x)");
        let long = "é".repeat(MAX_PASTE_BYTES);
        let text = cut(long.into_bytes());
        assert_eq!(text.len(), MAX_PASTE_BYTES);
        assert!(text.chars().all(|c| c == 'é'));
        // One byte over, mid-character: the half is dropped.
        let mut odd = "a".repeat(MAX_PASTE_BYTES - 1).into_bytes();
        odd.extend_from_slice("é".as_bytes());
        assert_eq!(cut(odd), "a".repeat(MAX_PASTE_BYTES - 1));
        // Invalid bytes inside still read as replacement characters.
        assert_eq!(cut(vec![b'a', 0xff, b'b']), "a\u{fffd}b");
    }
}
