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
//! A label offers every action GtkLabel installs (`gtklabel.c`) until it
//! is selected for the first time, which a label that can't be never is:
//! paste, cut and delete, which do nothing in a label, and copy,
//! select-all and its menu, which need a selection or a selectable label.
//! Each label offers only what it can do, by GTK's own rule for one that
//! has been selected ([`quiet_label`]).
//!
//! Both happen when a widget is realized, before assistive technology
//! can reach it ([`install`]). Like every other action, these are refused
//! while what holds them is covered (`crate::inert`: insensitive).

use adw::prelude::*;
use glib::translate::{Borrowed, FromGlibPtrBorrow, IntoGlib};
use gtk::{gio, glib};

/// Marks a widget given its actions, or a label watched.
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

/// A row gets the action it lacks; a label offers only what it can do.
fn realized(widget: &gtk::Widget) {
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        quiet_label(label);
        return;
    }
    // SAFETY: only ever set, and read, here and in quiet_label, as ().
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

/// Enables a label's actions as GtkLabel does once it has been selected
/// (`gtk_label_update_actions`): paste, cut and delete never; copy with a
/// selection; select-all for a selectable label; its menu where it has one
/// (it is selectable, or has links); a link's actions only for a label
/// with links (GTK keeps those up to date itself). Again whenever it
/// becomes selectable or gets new text.
fn quiet_label(label: &gtk::Label) {
    fn apply(l: &gtk::Label) {
        let links = l.uses_markup() && l.label().contains("<a ");
        for action in ["clipboard.cut", "clipboard.paste", "selection.delete"] {
            l.action_set_enabled(action, false);
        }
        l.action_set_enabled("clipboard.copy", l.selection_bounds().is_some());
        l.action_set_enabled("selection.select-all", l.is_selectable());
        l.action_set_enabled("menu.popup", l.is_selectable() || links);
        if !links {
            l.action_set_enabled("link.open", false);
            l.action_set_enabled("link.copy", false);
        }
    }
    apply(label);
    // SAFETY: only ever set, and read, here and in realized, as ().
    unsafe {
        if label.data::<()>(DONE).is_some() {
            return;
        }
        label.set_data(DONE, ());
    }
    for property in ["selectable", "label", "use-markup"] {
        label.connect_notify_local(Some(property), |l, _| apply(l));
    }
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
