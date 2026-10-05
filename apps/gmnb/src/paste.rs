//! Bounded clipboard reads for every paste.
//!
//! GTK's own text fields read the whole clipboard before inserting any of
//! it (GTK 4.22: `gtk_text_paste` reads it with
//! `gdk_clipboard_read_text_async`, and a text view reads it into a text
//! buffer even when it isn't editable), so a field's length limit only
//! applies after the read: pasting a 100 MB clipboard into an equation
//! took 600 MB. [`guard`] makes a field paste through [`read_text`]
//! instead: Ctrl+V, Shift+Insert and the context menu's Paste (all the
//! `paste-clipboard` signal) and a middle click (the primary selection).
//!
//! On Wayland the read is the offer's pipe, which closing stops. On X11,
//! GDK's selection stream can't be stopped: it fetches an incremental
//! (INCR) transfer to its end once it has started, whatever is read from
//! it, and holds it until then (GTK 4.22 `gdkselectioninputstream-x11.c`).
//! So there another X client's selection is read by `x11paste` instead, on a
//! connection of its own on a worker thread, which asks the X server for no
//! more than the cap and abandons the transfer there. Dragging text onto a
//! field still goes through GTK's drop target.

use std::time::Duration;

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use x11paste::{Limits, Overflow, Selection};

/// Every field's and page's own limit is far below this (an equation is at
/// most 1000 characters); a longer clipboard is cut here, while reading,
/// instead of being read whole first.
pub const MAX_PASTE_BYTES: usize = 64 * 1024;

/// How long reading another X client's selection may take. The window stays
/// responsive meanwhile (the read is on a worker), but text that takes
/// longer than this, from a stuck or trickling owner, is dropped rather than
/// landing long after the paste. (DGMNB, whose window waits, allows one
/// second.)
const X11_TIMEOUT: Duration = Duration::from_secs(3);

/// Marks a widget [`guard`] has seen.
const GUARDED: &str = "gmnb-bounded-paste";

/// The text on `clipboard`, cut to [`MAX_PASTE_BYTES`] (at a character
/// boundary) while it is read. `None` if it has no text.
pub async fn read_text(clipboard: &gdk::Clipboard) -> Option<String> {
    if let Some(selection) = x11_selection(clipboard) {
        return read_x11(&clipboard.display(), selection).await;
    }
    let (stream, _) = clipboard
        .read_future(
            &["text/plain;charset=utf-8", "text/plain"],
            glib::Priority::DEFAULT,
        )
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

/// Whether `display` is GDK's X11 backend's (asked by type name, so as not
/// to link that backend's bindings).
fn is_x11(display: &gdk::Display) -> bool {
    display.type_().name() == "GdkX11Display"
}

/// The X selection `clipboard` stands for, if it is to be read from another
/// X client. GMNB's own copy, already in memory, is read from GDK.
fn x11_selection(clipboard: &gdk::Clipboard) -> Option<Selection> {
    let display = clipboard.display();
    if !is_x11(&display) || clipboard.is_local() {
        return None;
    }
    Some(if *clipboard == display.primary_clipboard() {
        Selection::Primary
    } else {
        Selection::Clipboard
    })
}

/// `selection`'s text on `display`, read by `x11paste` on a worker (it
/// blocks until it has the text or [`X11_TIMEOUT`] passes) and cut to
/// [`MAX_PASTE_BYTES`] as GDK's are.
async fn read_x11(display: &gdk::Display, selection: Selection) -> Option<String> {
    let name = display.name().to_string();
    let limits = Limits {
        max_bytes: MAX_PASTE_BYTES,
        timeout: X11_TIMEOUT,
        overflow: Overflow::Cut,
    };
    let text = gio::spawn_blocking(move || x11paste::read_text_on(&name, selection, &limits))
        .await
        .ok()??;
    Some(cut(text.into_bytes()))
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

/// Makes every text field in `root` (`root` included) paste through
/// [`read_text`]. Fields already guarded are left alone, so this can run
/// again whenever widgets may have been added.
pub fn guard(root: &impl IsA<gtk::Widget>) {
    let root = root.as_ref();
    if let Some(text) = root.downcast_ref::<gtk::Text>() {
        guard_text(text);
    } else if let Some(view) = root.downcast_ref::<gtk::TextView>() {
        guard_text_view(view);
    }
    let mut child = root.first_child();
    while let Some(c) = child {
        guard(&c);
        child = c.next_sibling();
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
}

/// The character position nearest to `x` (in `text`'s coordinates), as
/// GTK finds where a middle click pastes.
fn position_at(text: &gtk::Text, x: f64) -> i32 {
    let len = text.text().chars().count();
    let distance = |position: usize| {
        let (strong, _) = text.compute_cursor_extents(position);
        (f64::from(strong.x()) - x).abs()
    };
    (0..=len)
        .min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
        .map_or(0, |position| position as i32)
}

/// GtkText's paste (`paste_received`), with the text read by [`read_text`]:
/// replaces the selection, or inserts at the cursor; a middle click pastes
/// at `at`, which moves the cursor there unless it is in the selection.
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
        let (Some(mut pasted), true) = (pasted, text.is_editable()) else {
            text.error_bell();
            return;
        };
        if text.must_truncate_multiline()
            && let Some(end) = pasted.find(['\n', '\r'])
        {
            pasted.truncate(end);
        }
        if let Some(at) = at {
            let in_selection = text
                .selection_bounds()
                .is_some_and(|(a, b)| (a.min(b)..=a.max(b)).contains(&at));
            if !in_selection {
                text.select_region(at, at);
            }
        }
        text.delete_selection();
        let mut position = text.position();
        text.insert_text(&pasted, &mut position);
        text.set_position(position);
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
}

/// A text view's paste with the text read by [`read_text`]. One that isn't
/// editable only inserts nothing in GTK (after reading the clipboard
/// whole); here it reads nothing either.
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
        buffer.delete_selection(true, editable);
        buffer.insert_interactive_at_cursor(&pasted, editable);
        view.scroll_mark_onscreen(&buffer.get_insert());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
