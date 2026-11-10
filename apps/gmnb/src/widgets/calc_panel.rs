//! History / Memory panel (upstream HistoryList.xaml + Memory.xaml).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use calcvm::HistoryEntry;
use gtk::glib;

use super::icon::paths;
use crate::theme::Hub;
use crate::window::{follow_icon, icon_button, scroller};

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
    hub: Rc<Hub>,
    switcher: adw::ToggleGroup,
    /// Programmer's switch: Memory alone, on, across the panel (DGMNB's).
    memory_only: adw::ToggleGroup,
    stack: gtk::Stack,
    history_list: gtk::ListBox,
    memory_list: gtk::ListBox,
    trash: gtk::Button,
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
    /// Each Memory row's own buttons (MC, M+, M−), by row.
    memory_buttons: RefCell<Vec<Vec<gtk::Button>>>,
}

/// What activating a row does, given the row.
type RowAction = Rc<dyn Fn(&gtk::ListBoxRow)>;

/// One line of a row, right-aligned, `height` px high (the text centred in
/// it as DGMNB centres it: style.css's line heights), cut at its start
/// when it is too long.
fn line_label(text: &str, class: &str, height: f32) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class(class);
    l.set_xalign(1.0);
    l.set_ellipsize(gtk::pango::EllipsizeMode::Start);
    l.set_height_request(height.round() as i32);
    l
}

/// What an empty list says, in the middle of it (DGMNB's).
fn empty_state(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("wc-empty");
    l.add_css_class("wc-plain");
    l.set_wrap(true);
    l.set_justify(gtk::Justification::Center);
    l.set_valign(gtk::Align::Center);
    l.set_vexpand(true);
    l
}

fn list() -> gtk::ListBox {
    let l = gtk::ListBox::new();
    l.set_selection_mode(gtk::SelectionMode::None);
    l.add_css_class("wc-panel-list");
    l
}

impl CalcPanel {
    /// `panel.tabs.stretch`: the History / Memory switch fills the
    /// panel's width in equal halves, or hugs its labels at the start.
    pub fn set_stretch(&self, stretch: bool) {
        for group in [&self.switcher, &self.memory_only] {
            group.set_homogeneous(stretch);
            group.set_hexpand(stretch);
            group.set_halign(if stretch {
                gtk::Align::Fill
            } else {
                gtk::Align::Start
            });
        }
    }

    /// What the style says of the panel's structure: the switch's width,
    /// whether the × and a memory item's buttons show only on hover, the
    /// History ⇄ Memory slide.
    fn apply_style(&self, style: &gmnb_theme::Style) {
        self.set_stretch(style.flag("panel.tabs.stretch"));
        let reveal = |path: &str, class: &str| {
            let hover = matches!(
                style.get(path),
                Some(gmnb_theme::types::Value::Word("hover"))
            );
            if hover {
                self.root.add_css_class(class);
            } else {
                self.root.remove_css_class(class);
            }
        };
        reveal("panel.delete_reveal", "wc-delete-on-hover");
        reveal("panel.memory_reveal", "wc-memory-on-hover");
        let ms = style.number("motion.panel").round() as u32;
        self.stack.set_transition_duration(ms);
        self.stack.set_transition_type(if ms > 0 {
            gtk::StackTransitionType::SlideLeftRight
        } else {
            gtk::StackTransitionType::None
        });
    }

    pub fn new(hub: &Rc<Hub>) -> Rc<Self> {
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
        switcher.add_css_class("wc-panel-tabs");
        switcher.add_css_class("flat");
        // Across the panel in equal halves (`panel.tabs.stretch`, DGMNB's
        // way) until a style says otherwise: `set_stretch`.
        switcher.set_homogeneous(true);
        switcher.set_hexpand(true);
        switcher.set_halign(gtk::Align::Fill);
        let memory_only = adw::ToggleGroup::new();
        memory_only.add(
            adw::Toggle::builder()
                .name("memory")
                .label("Memory")
                .build(),
        );
        memory_only.set_active_name(Some("memory"));
        for class in ["wc-toggle-group", "wc-panel-tabs", "flat"] {
            memory_only.add_css_class(class);
        }
        memory_only.set_visible(false);
        // Stays on: it is the only one.
        memory_only.connect_active_name_notify(|g| {
            if g.active_name().is_none() {
                g.set_active_name(Some("memory"));
            }
        });

        let history_list = list();
        let memory_list = list();

        let stack = gtk::Stack::new();
        stack.add_named(&scroller(hub, &history_list), Some("history"));
        stack.add_named(&scroller(hub, &memory_list), Some("memory"));
        stack.set_vexpand(true);

        // The bar under the list, with the trash button at its end (shown
        // while there is something to clear, as DGMNB's).
        use appcore::metrics::panel::{BAR_H, TRASH_INSET, TRASH_W};
        let trash = icon_button(paths::TRASH, "Clear all history (Ctrl+Shift+D)");
        trash.set_halign(gtk::Align::End);
        trash.set_size_request(
            (TRASH_W - 2.0 * TRASH_INSET) as i32,
            (BAR_H - 2.0 * TRASH_INSET) as i32,
        );
        let inset = TRASH_INSET as i32;
        trash.set_margin_top(inset);
        trash.set_margin_bottom(inset);
        trash.set_margin_end(inset);
        trash.set_visible(false);
        follow_icon(hub, &trash, "icon_button");
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        bar.add_css_class("wc-panel-bar");
        bar.set_height_request(BAR_H as i32);
        bar.set_halign(gtk::Align::End);
        bar.append(&trash);

        crate::window::exact_text(&switcher);
        crate::window::exact_text(&memory_only);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("wc-calc-panel");
        root.append(&switcher);
        root.append(&memory_only);
        root.append(&stack);
        root.append(&bar);

        let panel = Rc::new(CalcPanel {
            root,
            hub: hub.clone(),
            switcher: switcher.clone(),
            memory_only,
            stack: stack.clone(),
            history_list,
            memory_list,
            trash: trash.clone(),
            handlers: RefCell::default(),
            history_len: Default::default(),
            memory_len: Default::default(),
            history: RefCell::default(),
            history_shown: Default::default(),
            history_menu: RefCell::default(),
            history_actions: RefCell::default(),
            memory_actions: RefCell::default(),
            memory_buttons: RefCell::default(),
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
        hub.subscribe_style(&panel.root, move |_, style| {
            if let Some(p) = weak.upgrade() {
                p.apply_style(style);
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

    /// Puts the keyboard focus on the History/Memory tabs (upstream's
    /// docked pivot, where its focus goes when the History is emptied);
    /// false if they aren't shown or didn't take it.
    pub fn focus_tabs(&self) -> bool {
        [&self.switcher, &self.memory_only]
            .into_iter()
            .any(|g| g.is_mapped() && g.grab_focus())
    }

    /// Where the keyboard focus is in `list`, if it is: the row's place
    /// and, if it is on one of the row's own `buttons`, which. (A row's
    /// open menu is in the row.)
    fn focus_in(
        list: &gtk::ListBox,
        buttons: &[Vec<gtk::Button>],
    ) -> Option<(usize, Option<usize>)> {
        let focus = list.root().and_then(|r| r.focus())?;
        let row = focus
            .downcast_ref::<gtk::ListBoxRow>()
            .cloned()
            .or_else(|| {
                focus
                    .ancestor(gtk::ListBoxRow::static_type())
                    .and_downcast::<gtk::ListBoxRow>()
            })?;
        if row.parent().as_ref() != Some(list.upcast_ref()) {
            return None;
        }
        let i = usize::try_from(row.index()).ok()?;
        let button = buttons.get(i).and_then(|row| {
            row.iter()
                .position(|b| b.upcast_ref::<gtk::Widget>() == &focus)
        });
        Some((i, button))
    }

    /// Takes the keyboard focus off `list`'s rows, which are about to go.
    /// Left to find out that its focus went, GTK keeps the way to it as
    /// each ancestor's focus child (it forgets them walking up from the
    /// widget that had the focus, which has no parent by then), and Tab
    /// goes by what it remembers: once the other list was the one shown,
    /// Tab passed it by.
    fn let_go(list: &gtk::ListBox) {
        if let Some(root) = list.root() {
            root.set_focus(gtk::Widget::NONE);
        }
    }

    /// Programmer mode has no history upstream (the History pivot item is
    /// removed and only Memory remains).
    pub fn set_history_available(&self, available: bool) {
        self.switcher.set_visible(available);
        self.memory_only.set_visible(!available);
        if !available {
            self.show_tab("memory");
        }
    }

    /// The trash button is there while its list has something in it.
    fn sync_trash(&self) {
        let hist = self.current_tab() == "history";
        let n = if hist {
            self.history_len.get()
        } else {
            self.memory_len.get()
        };
        self.trash.set_visible(n > 0);
        let name = if hist {
            "Clear all history (Ctrl+Shift+D)"
        } else {
            "Clear all memory (Ctrl+L)"
        };
        self.trash.set_tooltip_text(Some(name));
        self.trash
            .update_property(&[gtk::accessible::Property::Label(name)]);
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

    /// Shows `items`. A list shown anew has new rows: if the focus was on
    /// one (or in its menu), it goes to the row that follows it
    /// ([`appcore::input::item_after`]), so that no key is typed at a row that
    /// is gone (GTK sends a key to the focus, and one sent to a widget out
    /// of the window reaches nothing: the first key typed after Clear
    /// History with a row focused was lost). True if the focus was in the
    /// list, or on the button that clears it, and no item is left: the
    /// page says where it goes then.
    pub fn set_history(self: &Rc<Self>, items: &[HistoryEntry]) -> bool {
        let focused = Self::focus_in(&self.history_list, &[]);
        if focused.is_some() {
            Self::let_go(&self.history_list);
        }
        let was = self.history_len.get();
        let cleared = self.trash.is_focus() && self.current_tab() == "history";
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
        let empty = empty_state("There's no history yet");
        crate::window::exact_text(&empty);
        self.history_list.set_placeholder(Some(&empty));
        use appcore::metrics::panel as mp;
        for (i, item) in items.iter().enumerate() {
            // The expression on a line of its own, clear of the ×; the
            // result in the rest of the row (DGMNB's).
            let expr = line_label(&item.expression, "wc-hist-expr", mp::EXPRESSION_H);
            expr.set_margin_end(mp::EXPRESSION_TRIM as i32);
            let res = line_label(
                &item.result,
                "wc-hist-result",
                mp::HISTORY_ROW_H - 2.0 * mp::ROW_PAD_Y - mp::EXPRESSION_H,
            );
            let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
            b.append(&expr);
            b.append(&res);
            let delete = icon_button(paths::CLOSE, "Delete");
            delete.remove_css_class("wc-icon-button");
            delete.add_css_class("wc-hist-delete");
            delete.set_halign(gtk::Align::End);
            delete.set_valign(gtk::Align::Start);
            delete.set_margin_top(mp::DELETE_TOP as i32);
            delete.set_margin_end((mp::DELETE_RIGHT - mp::DELETE) as i32);
            follow_icon(&self.hub, &delete, "panel.delete");
            let row = self.make_row(&b, Some(&delete), i == 0 && grew);
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
            {
                let weak = Rc::downgrade(self);
                let (row, item_id) = (row.downgrade(), id.clone());
                delete.connect_clicked(move |_| {
                    let (shown, i, entry) = &*item_id;
                    if let (Some(p), Some(row)) = (weak.upgrade(), row.upgrade())
                        && p.history_row_current(*shown, *i, entry, &row)
                        && let Some(f) = &p.handlers.borrow().history_delete
                    {
                        f(*i);
                    }
                });
            }
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
                crate::window::own_keys(&pop);
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
        match focused {
            Some(_) if items.is_empty() => true,
            Some((row, _)) => {
                let to = appcore::input::item_after(row, was, items.len());
                if let Some(row) = self.history_list.row_at_index(to as i32) {
                    row.grab_focus();
                }
                false
            }
            None => cleared && items.is_empty(),
        }
    }

    /// Shows `items`; the focus follows as in [`CalcPanel::set_history`],
    /// on the same button of the row if it was on one (M+ pressed again
    /// adds again). True if the focus was in the list, or on the button
    /// that clears it, and no item is left.
    pub fn set_memory(self: &Rc<Self>, items: &[String]) -> bool {
        let focused = Self::focus_in(&self.memory_list, &self.memory_buttons.borrow());
        if focused.is_some() {
            Self::let_go(&self.memory_list);
        }
        let was = self.memory_len.get();
        let cleared = self.trash.is_focus() && self.current_tab() == "memory";
        let grew = items.len() > self.memory_len.get();
        self.memory_len.set(items.len());
        self.memory_list.remove_all();
        self.memory_actions.borrow_mut().clear();
        let mut buttons = Vec::with_capacity(items.len());
        let empty = empty_state("There's nothing saved in memory");
        crate::window::exact_text(&empty);
        self.memory_list.set_placeholder(Some(&empty));
        use appcore::metrics::panel as mp;
        for (i, value) in items.iter().enumerate() {
            let label = line_label(value, "wc-hist-result", mp::MEMORY_VALUE_H);
            // DGMNB's three cells, to the fraction of a pixel.
            let actions = super::width_bin::Cells::new(mp::MEMORY_BUTTONS_GAP);
            actions.set_halign(gtk::Align::End);
            actions.set_width_request(mp::MEMORY_BUTTONS_W as i32);
            actions.add_css_class("wc-mem-actions");
            let mut own = Vec::with_capacity(3);
            for (text, op, tip) in [
                ("MC", MemOp::Clear, "Clear memory item"),
                ("M+", MemOp::Add, "Add to memory item"),
                ("M−", MemOp::Subtract, "Subtract from memory item"),
            ] {
                let b = gtk::Button::with_label(text);
                b.add_css_class("wc-mem-small");
                b.set_tooltip_text(Some(tip));
                crate::a11y::name_button(&b, tip);
                let weak = Rc::downgrade(self);
                b.connect_clicked(move |_| {
                    if let Some(p) = weak.upgrade()
                        && let Some(f) = &p.handlers.borrow().memory_op
                    {
                        f(i, op);
                    }
                });
                actions.append(&b);
                own.push(b);
            }
            buttons.push(own);
            let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
            b.append(&label);
            b.append(&actions);
            let row = self.make_row(&b, None, i == 0 && grew);
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
        self.memory_buttons.replace(buttons);
        match focused {
            Some(_) if items.is_empty() => true,
            Some((row, button)) => {
                let to = appcore::input::item_after(row, was, items.len());
                let own = self.memory_buttons.borrow();
                match button.and_then(|j| own.get(to).and_then(|b| b.get(j))) {
                    Some(b) => {
                        b.grab_focus();
                    }
                    None => {
                        if let Some(row) = self.memory_list.row_at_index(to as i32) {
                            row.grab_focus();
                        }
                    }
                }
                false
            }
            None => cleared && items.is_empty(),
        }
    }

    /// A list row, full width (its hover and press cover all of it), its
    /// content inside the row's padding (`.wc-row-content`), and `corner`,
    /// the History ×, over its top right. A new row slides in for
    /// `motion.history_row` ms (none in the base look).
    fn make_row(
        &self,
        content: &gtk::Box,
        corner: Option<&gtk::Button>,
        animate: bool,
    ) -> gtk::ListBoxRow {
        content.add_css_class("wc-row-content");
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(content));
        if let Some(corner) = corner {
            overlay.add_overlay(corner);
        }
        let ms = self.hub.style().number("motion.history_row").round() as u32;
        let child: gtk::Widget = if animate && ms > 0 {
            let revealer = gtk::Revealer::builder()
                .child(&overlay)
                .transition_type(gtk::RevealerTransitionType::SlideDown)
                .transition_duration(ms)
                .reveal_child(false)
                .build();
            let r = revealer.clone();
            glib::idle_add_local_once(move || r.set_reveal_child(true));
            revealer.upcast()
        } else {
            overlay.upcast()
        };
        let row = gtk::ListBoxRow::builder()
            .child(&child)
            .activatable(true)
            .build();
        row.add_css_class("wc-panel-row");
        crate::window::exact_text(&row);
        row
    }
}
