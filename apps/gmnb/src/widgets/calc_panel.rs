//! History / Memory panel (upstream HistoryList.xaml + Memory.xaml).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use calcvm::HistoryEntry;
use gtk::glib;

use super::icon::{PathIcon, paths};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemOp {
    Recall,
    Add,
    Subtract,
    Clear,
}

#[derive(Default)]
struct Handlers {
    history_recall: Option<Box<dyn Fn(usize)>>,
    history_delete: Option<Box<dyn Fn(usize)>>,
    history_clear: Option<Box<dyn Fn()>>,
    memory_op: Option<Box<dyn Fn(usize, MemOp)>>,
    memory_clear_all: Option<Box<dyn Fn()>>,
}

pub struct CalcPanel {
    pub root: gtk::Box,
    switcher: adw::ToggleGroup,
    stack: gtk::Stack,
    history_list: gtk::ListBox,
    memory_list: gtk::ListBox,
    trash: gtk::Button,
    memory_title: gtk::Label,
    handlers: RefCell<Handlers>,
    history_len: std::cell::Cell<usize>,
    memory_len: std::cell::Cell<usize>,
    /// The History items shown, and which showing of them this is: every
    /// [`CalcPanel::set_history`] (a History change, or a mode change,
    /// which shows another History) is a new one, and a request made on
    /// an earlier one's row is dropped ([`CalcPanel::history_row_current`]).
    history: RefCell<Vec<HistoryEntry>>,
    history_shown: std::cell::Cell<u64>,
    /// A History item's context menu while open: closed and let go of
    /// when the items are shown anew, so that no menu outlives its row.
    history_menu: RefCell<Option<gtk::Popover>>,
    /// What activating each History and Memory row does (recall it), by
    /// row, for the list's row-activated ([`CalcPanel::activate_row`]).
    history_actions: RefCell<Vec<(gtk::ListBoxRow, RowAction)>>,
    memory_actions: RefCell<Vec<(gtk::ListBoxRow, RowAction)>>,
}

/// What activating a row does, given the row.
type RowAction = Rc<dyn Fn(&gtk::ListBoxRow)>;

fn empty_state(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("wc-empty");
    l.set_wrap(true);
    l.set_justify(gtk::Justification::Center);
    l.set_margin_top(24);
    l.set_margin_start(16);
    l.set_margin_end(16);
    l
}

fn list() -> gtk::ListBox {
    let l = gtk::ListBox::new();
    l.set_selection_mode(gtk::SelectionMode::None);
    l.add_css_class("wc-panel-list");
    l
}

impl CalcPanel {
    pub fn new() -> Rc<Self> {
        let switcher = adw::ToggleGroup::new();
        switcher.add(
            adw::Toggle::builder()
                .name("history")
                .label("History")
                .build(),
        );
        switcher.add(
            adw::Toggle::builder()
                .name("memory")
                .label("Memory")
                .build(),
        );
        switcher.set_active_name(Some("history"));
        switcher.add_css_class("wc-toggle-group");
        switcher.add_css_class("flat");
        switcher.set_halign(gtk::Align::Start);

        let history_list = list();
        let memory_list = list();

        let scroll = |child: &gtk::ListBox| {
            gtk::ScrolledWindow::builder()
                .child(child)
                .hscrollbar_policy(gtk::PolicyType::Never)
                .vexpand(true)
                .build()
        };
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
        stack.set_transition_duration(220);
        stack.add_named(&scroll(&history_list), Some("history"));
        stack.add_named(&scroll(&memory_list), Some("memory"));

        let trash = gtk::Button::builder()
            .child(&PathIcon::new(paths::TRASH, 18))
            .css_classes(["flat", "wc-icon-button"])
            .halign(gtk::Align::End)
            .tooltip_text("Clear all history (Ctrl+Shift+D)")
            .build();

        let memory_title = gtk::Label::new(Some("Memory"));
        memory_title.add_css_class("wc-panel-title");
        memory_title.set_xalign(0.0);
        memory_title.set_visible(false);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.add_css_class("wc-calc-panel");
        root.append(&switcher);
        root.append(&memory_title);
        root.append(&stack);
        root.append(&trash);

        let panel = Rc::new(CalcPanel {
            root,
            switcher: switcher.clone(),
            stack: stack.clone(),
            history_list,
            memory_list,
            trash: trash.clone(),
            memory_title,
            handlers: RefCell::default(),
            history_len: Default::default(),
            memory_len: Default::default(),
            history: RefCell::default(),
            history_shown: Default::default(),
            history_menu: RefCell::default(),
            history_actions: RefCell::default(),
            memory_actions: RefCell::default(),
        });

        // A row is activated through its list's row-activated, whichever
        // way: a click emits only that (not the row's own activate, which
        // Enter, Space and assistive technology emit, and which ends in
        // it too). Upstream recalls an item on a click.
        let weak = Rc::downgrade(&panel);
        panel.history_list.connect_row_activated(move |_, row| {
            if let Some(p) = weak.upgrade() {
                Self::activate_row(&p.history_actions, row);
            }
        });
        let weak = Rc::downgrade(&panel);
        panel.memory_list.connect_row_activated(move |_, row| {
            if let Some(p) = weak.upgrade() {
                Self::activate_row(&p.memory_actions, row);
            }
        });

        let weak = Rc::downgrade(&panel);
        switcher.connect_active_name_notify(move |g| {
            if let Some(p) = weak.upgrade() {
                let name = g.active_name().unwrap_or_else(|| "history".into());
                p.stack.set_visible_child_name(&name);
                p.sync_trash();
            }
        });
        let weak = Rc::downgrade(&panel);
        trash.connect_clicked(move |_| {
            let Some(p) = weak.upgrade() else { return };
            let h = p.handlers.borrow();
            if p.current_tab() == "history" {
                if let Some(f) = &h.history_clear {
                    f();
                }
            } else if let Some(f) = &h.memory_clear_all {
                f();
            }
        });
        panel
    }

    pub fn current_tab(&self) -> String {
        self.switcher
            .active_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "history".into())
    }

    pub fn show_tab(&self, name: &str) {
        self.switcher.set_active_name(Some(name));
    }

    /// Programmer mode has no history upstream (the History pivot item is
    /// removed and only Memory remains).
    pub fn set_history_available(&self, available: bool) {
        self.switcher.set_visible(available);
        self.memory_title.set_visible(!available);
        if !available {
            self.show_tab("memory");
        }
    }

    fn sync_trash(&self) {
        let hist = self.current_tab() == "history";
        let n = if hist {
            self.history_len.get()
        } else {
            self.memory_len.get()
        };
        self.trash.set_sensitive(n > 0);
        self.trash.set_tooltip_text(Some(if hist {
            "Clear all history (Ctrl+Shift+D)"
        } else {
            "Clear all memory (Ctrl+L)"
        }));
    }

    pub fn connect_history_recall(&self, f: impl Fn(usize) + 'static) {
        self.handlers.borrow_mut().history_recall = Some(Box::new(f));
    }
    pub fn connect_history_delete(&self, f: impl Fn(usize) + 'static) {
        self.handlers.borrow_mut().history_delete = Some(Box::new(f));
    }
    pub fn connect_history_clear(&self, f: impl Fn() + 'static) {
        self.handlers.borrow_mut().history_clear = Some(Box::new(f));
    }
    pub fn connect_memory_op(&self, f: impl Fn(usize, MemOp) + 'static) {
        self.handlers.borrow_mut().memory_op = Some(Box::new(f));
    }
    pub fn connect_memory_clear_all(&self, f: impl Fn() + 'static) {
        self.handlers.borrow_mut().memory_clear_all = Some(Box::new(f));
    }

    /// Carries out what activating `row` does, if it is one of `actions`'
    /// rows (the items shown now): no borrow is held meanwhile, as recalling
    /// shows the items anew.
    fn activate_row(actions: &RefCell<Vec<(gtk::ListBoxRow, RowAction)>>, row: &gtk::ListBoxRow) {
        let action = actions
            .borrow()
            .iter()
            .find(|(r, _)| r == row)
            .map(|(_, a)| a.clone());
        if let Some(action) = action {
            action(row);
        }
    }

    /// Whether a request made on History row `row`, the `i`th of the
    /// showing `shown` and holding `entry`, may still be carried out: that
    /// showing is the current one (no History or mode change since, each
    /// of which shows the items anew), the same item still at `i`, and the
    /// row still in the list, shown and not covered (`crate::inert` makes
    /// a covered layer insensitive). Else it is dropped: an index alone
    /// could name another item by then (R16 Q1).
    fn history_row_current(
        &self,
        shown: u64,
        i: usize,
        entry: &HistoryEntry,
        row: &gtk::ListBoxRow,
    ) -> bool {
        self.history_shown.get() == shown
            && self.history.borrow().get(i) == Some(entry)
            && row.parent().as_ref() == Some(self.history_list.upcast_ref())
            && usize::try_from(row.index()).ok() == Some(i)
            && row.is_mapped()
            && row.is_sensitive()
    }

    pub fn set_history(self: &Rc<Self>, items: &[HistoryEntry]) {
        let grew = items.len() > self.history_len.get();
        self.history_len.set(items.len());
        let shown = self.history_shown.get().wrapping_add(1);
        self.history_shown.set(shown);
        self.history.replace(items.to_vec());
        // An open item menu goes with its row (which holds it, and must
        // not be let go of holding it).
        if let Some(menu) = self.history_menu.take() {
            menu.popdown();
            menu.unparent();
        }
        self.history_list.remove_all();
        self.history_actions.borrow_mut().clear();
        // remove_all() also drops the placeholder, so re-attach it.
        self.history_list
            .set_placeholder(Some(&empty_state("There's no history yet")));
        for (i, item) in items.iter().enumerate() {
            let expr = gtk::Label::new(Some(&item.expression));
            expr.add_css_class("wc-hist-expr");
            expr.set_xalign(1.0);
            expr.set_wrap(true);
            expr.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            let res = gtk::Label::new(Some(&item.result));
            res.add_css_class("wc-hist-result");
            res.set_xalign(1.0);
            res.set_wrap(true);
            res.set_wrap_mode(gtk::pango::WrapMode::Char);
            let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
            b.append(&expr);
            b.append(&res);
            let row = self.make_row(&b, i == 0 && grew);
            row.update_property(&[gtk::accessible::Property::Label(&format!(
                "{} {}",
                item.expression, item.result
            ))]);

            // Every request on this row names its item by this showing,
            // its place and the item itself, checked when it is carried
            // out (history_row_current): a row the list no longer holds
            // (one a menu or an assistive technology still has) does
            // nothing.
            let id = Rc::new((shown, i, item.clone()));
            let weak = Rc::downgrade(self);
            let item_id = id.clone();
            let recall: RowAction = Rc::new(move |r| {
                let (shown, i, entry) = &*item_id;
                if let Some(p) = weak.upgrade()
                    && p.history_row_current(*shown, *i, entry, r)
                    && let Some(f) = &p.handlers.borrow().history_recall
                {
                    f(*i);
                }
            });
            self.history_actions
                .borrow_mut()
                .push((row.clone(), recall));
            // Context menu → delete.
            let click = gtk::GestureClick::builder().button(3).build();
            let weak = Rc::downgrade(self);
            let anchor = row.downgrade();
            let item_id = id.clone();
            click.connect_pressed(move |_, _, x, y| {
                let (Some(p), Some(anchor)) = (weak.upgrade(), anchor.upgrade()) else {
                    return;
                };
                let del = gtk::Button::with_label("Delete");
                del.add_css_class("flat");
                let pop = gtk::Popover::builder().child(&del).has_arrow(false).build();
                pop.set_parent(&anchor);
                pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                // Weak popover and row: they hold this button.
                let (pop_, row) = (pop.downgrade(), anchor.downgrade());
                let weak = Rc::downgrade(&p);
                let item_id = item_id.clone();
                del.connect_clicked(move |_| {
                    if let Some(pop) = pop_.upgrade() {
                        pop.popdown();
                    }
                    let (shown, i, entry) = &*item_id;
                    if let (Some(p), Some(row)) = (weak.upgrade(), row.upgrade())
                        && p.history_row_current(*shown, *i, entry, &row)
                        && let Some(f) = &p.handlers.borrow().history_delete
                    {
                        f(*i);
                    }
                });
                let weak = Rc::downgrade(&p);
                pop.connect_closed(move |pop| {
                    if let Some(p) = weak.upgrade() {
                        let mut menu = p.history_menu.borrow_mut();
                        if menu.as_ref() == Some(pop) {
                            menu.take();
                        }
                    }
                    let pop = pop.clone();
                    glib::idle_add_local_once(move || pop.unparent());
                });
                if let Some(old) = p.history_menu.replace(Some(pop.clone())) {
                    old.popdown();
                    old.unparent();
                }
                pop.popup();
            });
            row.add_controller(click);
            // The context menu's Delete, which only a right click opens,
            // for assistive technology: history.delete. After the request
            // is answered, as it rebuilds this list, and only if this row
            // still holds its item then (R16 Q1).
            let weak = Rc::downgrade(self);
            crate::a11y::operable(&row, "history", "delete", move |r| {
                let (weak, row, item_id) = (weak.clone(), r.downgrade(), id.clone());
                glib::idle_add_local_once(move || {
                    let (shown, i, entry) = &*item_id;
                    if let (Some(p), Some(row)) = (weak.upgrade(), row.upgrade())
                        && p.history_row_current(*shown, *i, entry, &row)
                        && let Some(f) = &p.handlers.borrow().history_delete
                    {
                        f(*i);
                    }
                });
            });
            self.history_list.append(&row);
        }
        self.sync_trash();
    }

    pub fn set_memory(self: &Rc<Self>, items: &[String]) {
        let grew = items.len() > self.memory_len.get();
        self.memory_len.set(items.len());
        self.memory_list.remove_all();
        self.memory_actions.borrow_mut().clear();
        self.memory_list
            .set_placeholder(Some(&empty_state("There's nothing saved in memory")));
        for (i, value) in items.iter().enumerate() {
            let label = gtk::Label::new(Some(value));
            label.add_css_class("wc-hist-result");
            label.set_xalign(1.0);
            label.set_wrap(true);
            label.set_wrap_mode(gtk::pango::WrapMode::Char);
            let actions = gtk::Box::new(gtk::Orientation::Horizontal, 2);
            actions.set_halign(gtk::Align::End);
            actions.add_css_class("wc-mem-actions");
            for (text, op, tip) in [
                ("MC", MemOp::Clear, "Clear memory item"),
                ("M+", MemOp::Add, "Add to memory item"),
                ("M−", MemOp::Subtract, "Subtract from memory item"),
            ] {
                let b = gtk::Button::with_label(text);
                b.add_css_class("wc-mem");
                b.add_css_class("wc-mem-small");
                b.set_tooltip_text(Some(tip));
                let weak = Rc::downgrade(self);
                b.connect_clicked(move |_| {
                    if let Some(p) = weak.upgrade()
                        && let Some(f) = &p.handlers.borrow().memory_op
                    {
                        f(i, op);
                    }
                });
                actions.append(&b);
            }
            let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
            b.append(&label);
            b.append(&actions);
            let row = self.make_row(&b, i == 0 && grew);
            row.add_css_class("wc-mem-row");
            row.update_property(&[gtk::accessible::Property::Label(value)]);
            let weak = Rc::downgrade(self);
            let recall: RowAction = Rc::new(move |_| {
                if let Some(p) = weak.upgrade()
                    && let Some(f) = &p.handlers.borrow().memory_op
                {
                    f(i, MemOp::Recall);
                }
            });
            self.memory_actions.borrow_mut().push((row.clone(), recall));
            self.memory_list.append(&row);
        }
        self.sync_trash();
    }

    fn make_row(&self, content: &gtk::Box, animate: bool) -> gtk::ListBoxRow {
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let revealer = gtk::Revealer::builder()
            .child(content)
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(280)
            .reveal_child(!animate)
            .build();
        let row = gtk::ListBoxRow::builder()
            .child(&revealer)
            .activatable(true)
            .build();
        row.add_css_class("wc-panel-row");
        if animate {
            glib::idle_add_local_once(move || revealer.set_reveal_child(true));
        }
        row
    }
}
