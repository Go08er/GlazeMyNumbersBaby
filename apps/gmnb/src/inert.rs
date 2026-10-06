//! What a dialog, a modal window or a sheet covers, assistive technology
//! can't use either.
//!
//! GTK keeps the controls under an AdwDialog, under a modal window of its
//! own (the colour chooser), under the navigation sidebar and under the
//! history sheet in the accessibility tree, and carries out requests on
//! them: libadwaita only stops the pointer and keyboard focus there (1.9
//! `adw-dialog-host.c`). So while a layer is covered ([`Layers`]) it is:
//!
//! - hidden from assistive technology: looked up afresh, it isn't there.
//!   GTK still answers a reference taken before (its objects stay on the
//!   bus while the widgets live), so also:
//! - insensitive, which GTK checks before an Action (GTK 4.22
//!   `gtkatspiaction.c`), and which [`refuse_insensitive_values`] makes it
//!   check before a Value change too (`gtkatspivalue.c` checks nothing; the
//!   change is then ignored);
//! - with its text fields not editable, which GTK checks before an
//!   EditableText change (`gtkatspieditabletext.c`; the bounded paste in
//!   `crate::paste` doesn't paste into them either).
//!
//! A Selection request (SelectChild, DeselectChild, DeselectSelectedChild,
//! SelectAll, ClearSelection) GTK carries out on a list box, a flow box or
//! a list or grid view without any check (`gtkatspiselection.c`; the other
//! widgets with that interface, a combo box, a stack switcher and a
//! notebook's tabs, GMNB has none of). Where the selection means nothing
//! (selection mode none: Settings' groups, History, Memory, the
//! converter's "About equal to") GTK ignores the request. Where it does,
//! the widget's own handler asks [`Layers::covers`] and, covered, puts the
//! selection back (R15-M-02): the palette grid in Settings (which then
//! saves nothing), the navigation list, and the list in each unit
//! drop-down's popup ([`Layers::hold_list`]). Settings' other controls ask
//! too, though GTK refuses their requests already.
//!
//! Nor do keys, pastes or the window's actions reach what is covered:
//! the window and its pages ask [`Layers::covers`], from the same record
//! (`Window::handle_key`, `Window::paste`; `win.paste` is disabled, so not
//! offered to assistive technology, while it would paste into a covered
//! page).
//!
//! It keeps its look: an insensitive widget is drawn faded, so a covered
//! layer carries [`INERT`], whose style (style.css) undoes that for
//! everything the app hadn't made insensitive itself (what it had, and all
//! that holds, carries [`OFF`] meanwhile). What remains is the soft shadow
//! libadwaita takes off a disabled toggle and switch knob.
//!
//! A covered text field's selection, which GTK drops, is given back when
//! it is uncovered.
//!
//! A text field made while its layer is covered (a precision change from
//! Settings rebuilds the graphing variables' spin buttons) is read-only
//! from the start ([`guard_new_fields`]): an AT client learns of it from
//! GTK's ChildrenChanged events, and GTK's EditableText checks only that.
//!
//! Left: moving the caret or the selection in a covered text field (Text
//! `SetSelection`, `SetCaretOffset`), which GTK does without any check
//! (GtkText through two different interfaces) and which changes no value.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use glib::translate::{Borrowed, FromGlibPtrBorrow, IntoGlib};
use gtk::glib;

/// The style class of a covered layer.
const INERT: &str = "wc-inert";
/// The style class, meanwhile, of a widget under it that the app made
/// insensitive itself (it stays faded).
const OFF: &str = "wc-off";

/// Marks a root [`Layers`] made insensitive, and a field it made read-only.
const MADE_INSENSITIVE: &str = "gmnb-inert";
const MADE_READ_ONLY: &str = "gmnb-inert-read-only";
/// A covered text field's selection, to give back (GTK drops it when the
/// field becomes insensitive).
const SELECTION: &str = "gmnb-inert-selection";
/// Marks a window [`Layers`] watches.
const WATCHED: &str = "gmnb-layers-watched";

/// A widget a layer of the window's own covers while `covers` says so.
struct Scrim {
    content: glib::WeakRef<gtk::Widget>,
    covers: Box<dyn Fn() -> bool>,
}

/// The layers of one window and what they cover, found afresh whenever
/// one opens or closes:
/// - a modal window of the program's (the colour chooser) covers the
///   window's content and all its dialogs;
/// - otherwise an AdwDialog covers the content and the dialogs under it;
/// - a scrim of the window's own ([`Layers::scrim`]) covers its content.
pub struct Layers {
    win: glib::WeakRef<adw::ApplicationWindow>,
    /// The window's dialogs (libadwaita keeps the list only while someone
    /// holds it).
    dialogs: gtk::gio::ListModel,
    scrims: RefCell<Vec<Scrim>>,
    covered: RefCell<Vec<glib::WeakRef<gtk::Widget>>>,
    /// Told after each [`Layers::update`].
    changed: RefCell<Vec<Box<dyn Fn()>>>,
}

impl Layers {
    pub fn new(win: &adw::ApplicationWindow) -> Rc<Layers> {
        let this = Rc::new(Layers {
            win: win.downgrade(),
            dialogs: win.dialogs(),
            scrims: RefCell::default(),
            covered: RefCell::default(),
            changed: RefCell::default(),
        });
        ALL.with(|all| {
            let mut all = all.borrow_mut();
            all.retain(|l| l.strong_count() > 0);
            all.push(Rc::downgrade(&this));
        });
        let update = {
            let weak = Rc::downgrade(&this);
            move || {
                if let Some(this) = weak.upgrade() {
                    this.update();
                }
            }
        };
        {
            let update = update.clone();
            win.connect_visible_dialog_notify(move |_| update());
        }
        {
            let update = update.clone();
            this.dialogs
                .connect_items_changed(move |_, _, _, _| update());
        }
        // Windows: each new one is watched for showing, hiding and
        // becoming modal or someone's transient.
        let toplevels = gtk::Window::toplevels();
        let watch = {
            let update = update.clone();
            move |list: &gtk::gio::ListModel| {
                for w in windows(list) {
                    // SAFETY: only ever set, and read, here, as ().
                    unsafe {
                        if w.data::<()>(WATCHED).is_some() {
                            continue;
                        }
                        w.set_data(WATCHED, ());
                    }
                    for property in ["visible", "modal", "transient-for"] {
                        let update = update.clone();
                        w.connect_notify_local(Some(property), move |_, _| update());
                    }
                }
            }
        };
        watch(&toplevels);
        toplevels.connect_items_changed(move |list, _, _, _| {
            watch(list);
            update();
        });
        this
    }

    /// `content` is covered while `covers()`, re-checked whenever one of
    /// `source`'s `properties` changes (a sidebar overlaid on it, a sheet
    /// open over it).
    pub fn scrim(
        self: &Rc<Self>,
        content: &impl IsA<gtk::Widget>,
        source: &impl IsA<glib::Object>,
        properties: &[&str],
        covers: impl Fn() -> bool + 'static,
    ) {
        self.scrims.borrow_mut().push(Scrim {
            content: content.as_ref().downgrade(),
            covers: Box::new(covers),
        });
        for property in properties {
            let weak = Rc::downgrade(self);
            source.connect_notify_local(Some(property), move |_, _| {
                if let Some(this) = weak.upgrade() {
                    this.update();
                }
            });
        }
        self.update();
    }

    /// Whether `w` is covered now: it, or a widget holding it, is a covered
    /// layer. The one check of what keys, pastes and the window's actions
    /// may change, from the record [`Layers::update`] keeps (the same that
    /// makes those layers inert), not from any widget's sensitivity.
    pub fn covers(&self, w: &impl IsA<gtk::Widget>) -> bool {
        let w = w.as_ref();
        self.covered
            .borrow()
            .iter()
            .filter_map(|c| c.upgrade())
            .any(|c| c == *w || w.is_ancestor(&c))
    }

    /// Calls `f` after each change of what is covered.
    pub fn connect_changed(&self, f: impl Fn() + 'static) {
        self.changed.borrow_mut().push(Box::new(f));
    }

    /// Keeps the list in `dropdown`'s popup on the drop-down's own choice
    /// while the drop-down is covered (R15-M-02). The list has a selection
    /// of its own, which GTK changes on a Selection request (module
    /// docs); the drop-down takes it only when a row is activated, an
    /// Action, refused while covered. Put back, the list shows the
    /// drop-down's choice when it next opens. The list's model is
    /// replaced with the drop-down's, so each one is watched.
    pub fn hold_list(self: &Rc<Self>, dropdown: &gtk::DropDown) {
        let Some(list) = descendant::<gtk::ListView>(dropdown.upcast_ref()) else {
            return;
        };
        let hold = {
            let (layers, dropdown) = (Rc::downgrade(self), dropdown.downgrade());
            move |list: &gtk::ListView| {
                let Some(model) = list.model().and_downcast::<gtk::SingleSelection>() else {
                    return;
                };
                let (layers, dropdown) = (layers.clone(), dropdown.clone());
                model.connect_selected_notify(move |m| {
                    let (Some(layers), Some(dropdown)) = (layers.upgrade(), dropdown.upgrade())
                    else {
                        return;
                    };
                    let want = dropdown.selected_item();
                    if layers.covers(&dropdown) && m.selected_item() != want {
                        // The list may be filtered (a search): by item.
                        let at = want.and_then(|w| {
                            (0..m.n_items()).find(|&i| m.item(i).as_ref() == Some(&w))
                        });
                        m.set_selected(at.unwrap_or(gtk::INVALID_LIST_POSITION));
                    }
                });
            }
        };
        hold(&list);
        list.connect_model_notify(move |l| hold(l));
    }

    /// What is covered now.
    fn covering(&self) -> Vec<gtk::Widget> {
        let mut out = Vec::new();
        if let Some(win) = self.win.upgrade() {
            let modal = windows(&gtk::Window::toplevels()).any(|w| {
                w != *win.upcast_ref::<gtk::Window>()
                    && w.is_visible()
                    && w.is_modal()
                    && w.transient_for().as_ref() == Some(win.upcast_ref())
            });
            let dialogs: Vec<adw::Dialog> = self.dialogs.iter::<adw::Dialog>().flatten().collect();
            if modal || !dialogs.is_empty() {
                let top = (!modal).then(|| win.visible_dialog()).flatten();
                out.extend(win.content());
                out.extend(
                    dialogs
                        .into_iter()
                        .filter(|d| Some(d) != top.as_ref())
                        .map(|d| d.upcast()),
                );
            }
        }
        for scrim in self.scrims.borrow().iter() {
            if (scrim.covers)()
                && let Some(content) = scrim.content.upgrade()
            {
                out.push(content);
            }
        }
        out
    }

    /// Brings what is inert in line with what is covered.
    pub fn update(&self) {
        let now = self.covering();
        let before: Vec<gtk::Widget> = self
            .covered
            .take()
            .iter()
            .filter_map(|w| w.upgrade())
            .collect();
        for w in before.iter().filter(|w| !now.contains(w)) {
            set_inert(w, false);
        }
        for w in now.iter().filter(|w| !before.contains(w)) {
            keep_selections(w);
            set_inert(w, true);
        }
        // Layers nest (the content, and the sidebar's content in it): each
        // widget's state follows from all of them.
        for root in before.iter().chain(&now) {
            let covered = now.iter().any(|c| root.is_ancestor(c));
            let off = std::iter::successors(root.parent(), |w| w.parent()).any(|w| app_off(&w));
            mark(root, &now, covered, off);
        }
        *self.covered.borrow_mut() = now.iter().map(|w| w.downgrade()).collect();
        if now != before {
            for f in self.changed.borrow().iter() {
                f();
            }
        }
    }
}

/// Whether the app made `w` insensitive itself.
fn app_off(w: &gtk::Widget) -> bool {
    // SAFETY: only ever set, and read, as () (set_inert).
    !w.get_sensitive() && unsafe { w.data::<()>(MADE_INSENSITIVE) }.is_none()
}

/// Under a covered layer, a text field is read-only, and what the app made
/// insensitive itself (with all it holds) is marked [`OFF`]; elsewhere
/// neither. `covered` and `off` say so of `w`'s ancestors.
fn mark(w: &gtk::Widget, now: &[gtk::Widget], covered: bool, off: bool) {
    let covered = covered || now.contains(w);
    let off = off || app_off(w);
    if w.is::<gtk::Text>() || w.is::<gtk::TextView>() {
        set_read_only(w, covered);
    }
    if !covered && let Some(text) = w.downcast_ref::<gtk::Text>() {
        // SAFETY: only ever set, and read, as (i32, i32) (keep_selections).
        if let Some((start, end)) = unsafe { text.steal_data::<(i32, i32)>(SELECTION) } {
            text.select_region(start, end);
        }
    }
    if covered && off {
        w.add_css_class(OFF);
    } else {
        w.remove_css_class(OFF);
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        mark(&c, now, covered, off);
        child = c.next_sibling();
    }
}

/// Remembers the selection of each text field in `root` that has one, to
/// give back when it is uncovered ([`mark`]): GTK drops a text field's
/// selection when it becomes insensitive (4.22 `gtk_text_state_flags_changed`).
fn keep_selections(root: &gtk::Widget) {
    if let Some(text) = root.downcast_ref::<gtk::Text>()
        && let Some(bounds) = text.selection_bounds()
    {
        // SAFETY: only ever set, and read, as (i32, i32) (also in mark).
        unsafe { text.set_data(SELECTION, bounds) };
    }
    let mut child = root.first_child();
    while let Some(c) = child {
        keep_selections(&c);
        child = c.next_sibling();
    }
}

/// The first `T` in `root`'s tree (popovers too), `root` included.
fn descendant<T: IsA<gtk::Widget>>(root: &gtk::Widget) -> Option<T> {
    if let Some(t) = root.downcast_ref::<T>() {
        return Some(t.clone());
    }
    let mut child = root.first_child();
    while let Some(c) = child {
        if let Some(t) = descendant::<T>(&c) {
            return Some(t);
        }
        child = c.next_sibling();
    }
    None
}

/// The windows in GTK's list of toplevels (a list of widgets).
fn windows(toplevels: &gtk::gio::ListModel) -> impl Iterator<Item = gtk::Window> + '_ {
    toplevels
        .iter::<gtk::Widget>()
        .flatten()
        .filter_map(|w| w.downcast::<gtk::Window>().ok())
}

fn set_inert(root: &gtk::Widget, on: bool) {
    if on {
        root.update_state(&[gtk::accessible::State::Hidden(true)]);
        root.add_css_class(INERT);
        if root.get_sensitive() {
            // SAFETY: only ever set, and read, as () (also in update).
            unsafe { root.set_data(MADE_INSENSITIVE, ()) };
            root.set_sensitive(false);
        }
    } else {
        // SAFETY: as above.
        if unsafe { root.steal_data::<()>(MADE_INSENSITIVE) }.is_some() {
            root.set_sensitive(true);
        }
        root.remove_css_class(INERT);
        root.reset_state(gtk::AccessibleState::Hidden);
    }
}

fn set_read_only(field: &gtk::Widget, on: bool) {
    let editable = |w: &gtk::Widget| {
        w.downcast_ref::<gtk::Text>()
            .map(|t| t.is_editable())
            .or_else(|| w.downcast_ref::<gtk::TextView>().map(|v| v.is_editable()))
            .unwrap_or(false)
    };
    let set = |w: &gtk::Widget, editable: bool| {
        if let Some(t) = w.downcast_ref::<gtk::Text>() {
            t.set_editable(editable);
        } else if let Some(v) = w.downcast_ref::<gtk::TextView>() {
            v.set_editable(editable);
        }
    };
    // SAFETY: only ever set, and read, here, as ().
    unsafe {
        if on && editable(field) {
            field.set_data(MADE_READ_ONLY, ());
            set(field, false);
        } else if !on && field.steal_data::<()>(MADE_READ_ONLY).is_some() {
            set(field, true);
        }
    }
}

thread_local! {
    /// Every window's layers, for [`guard_new_fields`].
    static ALL: RefCell<Vec<Weak<Layers>>> = RefCell::default();
}

/// Makes a text field realized under a covered layer read-only, as
/// [`Layers::update`] made those there when it was covered, and gives it
/// back when it is uncovered (the same mark). A field is realized before
/// assistive technology can reach it. Call once, at startup, before any
/// widget is realized.
pub fn guard_new_fields() {
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
            if (widget.is::<gtk::Text>() || widget.is::<gtk::TextView>())
                && ALL.with(|all| {
                    all.borrow()
                        .iter()
                        .filter_map(Weak::upgrade)
                        .any(|l| l.covers(&*widget))
                })
            {
                set_read_only(&widget, true);
            }
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
            Some(realized),
            std::ptr::null_mut(),
            None,
        );
    }
}

type SetValue = unsafe extern "C" fn(*mut gtk::ffi::GtkAccessibleRange, f64) -> glib::ffi::gboolean;

thread_local! {
    /// Each patched class's own vfunc.
    static SET_VALUE: RefCell<HashMap<glib::Type, SetValue>> = RefCell::default();
}

/// Makes an insensitive range (a spin button, a scale, a scroll bar...)
/// ignore a value from assistive technology, as GTK has it refuse an
/// action: GTK sets the value through the range class's
/// GtkAccessibleRange vfunc, which checks nothing (4.22
/// `gtkatspivalue.c`), so every range class's vfunc is wrapped. A class
/// copies its parent's vfuncs when it is first used, so the classes that
/// implement the interface are patched first, then every class derived
/// from them that is in use already; those used later copy the wrapper.
/// Call once, at startup, on the main thread.
///
/// The value is ignored, not refused: GTK passes the vfunc's answer to
/// GDBus as the property write's success with no error, and GDBus aborts
/// on a failure without one (GLib `invoke_set_property_in_idle_cb`). So,
/// like GTK's own vfunc for ranges whose value can't be set, it answers
/// that it did.
pub fn refuse_insensitive_values() {
    unsafe extern "C" fn set_value(
        range: *mut gtk::ffi::GtkAccessibleRange,
        value: f64,
    ) -> glib::ffi::gboolean {
        // SAFETY: GTK calls this with a live accessible range.
        let object = unsafe { glib::Object::from_glib_borrow(range.cast()) };
        if object
            .downcast_ref::<gtk::Widget>()
            .is_some_and(|w| !w.is_sensitive())
        {
            return glib::ffi::GTRUE;
        }
        let mut ty = Some(object.type_());
        let own = SET_VALUE.with(|own| {
            let own = own.borrow();
            while let Some(t) = ty {
                if let Some(f) = own.get(&t) {
                    return Some(*f);
                }
                ty = t.parent();
            }
            None
        });
        // SAFETY: the class's own vfunc, for its own instance.
        own.map_or(glib::ffi::GTRUE, |f| unsafe { f(range, value) })
    }
    fn patch(ty: glib::Type) {
        let iface = gtk::AccessibleRange::static_type().into_glib();
        // SAFETY: an initialized class's interface vtable is a
        // GtkAccessibleRangeInterface while the class implements it, and
        // stays put; this runs on the main thread, where GTK calls it.
        unsafe {
            let class = glib::gobject_ffi::g_type_class_peek(ty.into_glib());
            if class.is_null() {
                return;
            }
            let vtable = glib::gobject_ffi::g_type_interface_peek(class, iface)
                .cast::<gtk::ffi::GtkAccessibleRangeInterface>();
            if vtable.is_null() {
                return;
            }
            if let Some(f) = (*vtable).set_current_value
                && f as usize != set_value as SetValue as usize
            {
                SET_VALUE.with(|own| own.borrow_mut().insert(ty, f));
                (*vtable).set_current_value = Some(set_value);
            }
        }
        for child in ty.children().iter() {
            patch(*child);
        }
    }
    for ty in [
        gtk::Range::static_type(),
        gtk::SpinButton::static_type(),
        gtk::Scrollbar::static_type(),
        gtk::Paned::static_type(),
        gtk::ScaleButton::static_type(),
        gtk::LevelBar::static_type(),
        gtk::ProgressBar::static_type(),
    ] {
        // Initialized for good (a static type's class is never freed), so
        // that its subclasses copy the wrapper.
        drop(glib::Class::<gtk::Widget>::from_type(ty));
        patch(ty);
    }
}
