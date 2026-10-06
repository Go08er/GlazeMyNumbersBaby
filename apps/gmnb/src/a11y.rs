//! What assistive technology needs to operate GMNB that GTK 4.22 and
//! libadwaita 1.9 leave out.
//!
//! GTK offers an AT-SPI Action for a button, a switch or an entry, and
//! otherwise only the actions a widget has in its own action groups
//! (`gtkatspiaction.c`, `widget_handle_method`). So an activatable list
//! box row (the navigation's modes, History and Memory items, Settings'
//! "About GMNB" and every row of libadwaita's own: a switch row, the
//! About dialog's) has nothing to activate it by but the keyboard. Each
//! gets `row.activate`, offered while the row is activatable, which does
//! what Enter does.
//!
//! That happens when a widget is realized, before assistive technology
//! can reach it ([`install`]). Like every other action, it is refused
//! while what holds it is covered (`crate::inert`: insensitive).

use adw::prelude::*;
use glib::translate::{Borrowed, FromGlibPtrBorrow, IntoGlib};
use gtk::{gio, glib};

/// Marks a widget given its actions.
const DONE: &str = "gmnb-a11y-operable";

/// Gives `widget` the action `group.name`, which calls `f`; returns it
/// (enabled).
pub fn operable<W: IsA<gtk::Widget>>(
    widget: &W,
    group: &str,
    name: &str,
    f: impl Fn(&W) + 'static,
) -> gio::SimpleAction {
    let action = gio::SimpleAction::new(name, None);
    let weak = widget.downgrade();
    action.connect_activate(move |_, _| {
        if let Some(w) = weak.upgrade() {
            f(&w);
        }
    });
    let actions = gio::SimpleActionGroup::new();
    actions.add_action(&action);
    widget.insert_action_group(group, Some(&actions));
    action
}

/// A row gets the action it lacks.
fn realized(widget: &gtk::Widget) {
    // SAFETY: only ever set, and read, here, as ().
    unsafe {
        if widget.data::<()>(DONE).is_some() {
            return;
        }
    }
    if let Some(row) = widget.downcast_ref::<gtk::ListBoxRow>() {
        let action = operable(row, "row", "activate", |r| {
            r.activate();
        });
        // Offered only while the row is activatable.
        action.set_enabled(row.is_activatable());
        row.connect_activatable_notify(move |r| action.set_enabled(r.is_activatable()));
    } else {
        return;
    }
    // SAFETY: as above.
    unsafe { widget.set_data(DONE, ()) };
}

/// Watches every widget as it is realized ([`realized`]). Call once, at
/// startup, before any widget is realized.
pub fn install() {
    unsafe extern "C" fn hook(
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
            realized(&widget);
        }
        glib::ffi::GTRUE // stay installed
    }
    let widget = gtk::Widget::static_type();
    let _class = glib::Class::<gtk::Widget>::from_type(widget);
    let realize = glib::subclass::signal::SignalId::lookup("realize", widget)
        .expect("GtkWidget has a realize signal");
    // SAFETY: the hook matches GSignalEmissionHook, and keeps no data.
    unsafe {
        glib::gobject_ffi::g_signal_add_emission_hook(
            realize.into_glib(),
            0,
            Some(hook),
            std::ptr::null_mut(),
            None,
        );
    }
}
