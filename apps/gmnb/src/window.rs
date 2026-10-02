//! The main window: aurora backdrop → toasts → overlay navigation sidebar →
//! header + page stack. Pages are created lazily the first time they're
//! needed and several modes can share one page (as upstream does).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use appcore::input::{self, WindowAction};
use appcore::modes::{Group, PageKind, ViewMode};
use appcore::{KeyPress, Named};
use gtk::{gdk, glib};

use crate::keymap::key_press;
use crate::pages::{self, Ctx, Page};
use crate::settings::{Persist, Store};
use crate::theme::{Hub, PaletteId};
use crate::widgets::aurora::Aurora;
use crate::widgets::icon::{PathIcon, paths};

pub struct Window {
    win: adw::ApplicationWindow,
    ctx: Rc<Ctx>,
    split: adw::OverlaySplitView,
    title: gtk::Label,
    stack: gtk::Stack,
    header_end: gtk::Box,
    nav: gtk::ListBox,
    nav_rows: RefCell<Vec<(ViewMode, gtk::ListBoxRow, PathIcon)>>,
    pages: RefCell<HashMap<PageKind, Rc<dyn Page>>>,
    mode: Cell<ViewMode>,
}

pub fn apply_theme_setting(theme: &str) {
    let sm = adw::StyleManager::default();
    sm.set_color_scheme(match theme {
        "light" => adw::ColorScheme::ForceLight,
        "dark" => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    });
}

impl Window {
    pub fn new(app: &adw::Application) -> Rc<Self> {
        let screenshot = std::env::var_os("GMNB_SCREENSHOT").is_some();
        // Screenshots don't touch saved state unless explicitly asked to read it.
        let ephemeral = screenshot && std::env::var_os("GMNB_REAL_STORE").is_none();
        let store = Rc::new(if ephemeral {
            Store::ephemeral()
        } else {
            Store::load(crate::DATA_DIR)
        });
        if let Ok(p) = std::env::var("GMNB_PALETTE") {
            store.data.borrow_mut().palette = p;
        }
        match std::env::var("GMNB_DARK").as_deref() {
            Ok("1") => store.data.borrow_mut().theme = "dark".into(),
            Ok("0") => store.data.borrow_mut().theme = "light".into(),
            _ => {}
        }
        if let Some(size) = std::env::var("GMNB_SIZE").ok().and_then(|s| {
            let (a, b) = s.split_once('x')?;
            Some((a.parse().ok()?, b.parse().ok()?))
        }) {
            let mut d = store.data.borrow_mut();
            (d.width, d.height) = size;
        }

        let settings = store.data.borrow().clone();
        apply_theme_setting(&settings.theme);
        let custom = (
            crate::theme::parse_hex(&settings.custom_primary)
                .unwrap_or(crate::theme::DEFAULT_PRIMARY),
            crate::theme::parse_hex(&settings.custom_secondary)
                .unwrap_or(crate::theme::DEFAULT_SECONDARY),
        );
        let hub = Hub::new(
            PaletteId::from_key(&settings.palette).unwrap_or(PaletteId::Aurora),
            custom,
        );
        crate::theme::follow_portal_accent(&hub);

        let win = adw::ApplicationWindow::builder()
            .application(app)
            .title(crate::APP_NAME)
            .default_width(settings.width)
            .default_height(settings.height)
            .width_request(320)
            .height_request(480)
            .css_classes(["gmnb"])
            .build();

        let aurora = Aurora::default();
        aurora.set_animated(settings.animated_background && std::env::var("GMNB_STILL").is_err());
        let toasts = adw::ToastOverlay::new();
        let ctx = Rc::new(Ctx {
            hub: hub.clone(),
            aurora: aurora.clone(),
            toasts: toasts.clone(),
            store: store.clone(),
            compact: Default::default(),
        });

        // Header.
        let header = adw::HeaderBar::new();
        header.set_show_title(false);
        let menu = gtk::ToggleButton::builder()
            .child(&PathIcon::new(paths::MENU, 18))
            .tooltip_text("Open Navigation")
            .css_classes(["flat", "wc-icon-button"])
            .build();
        menu.update_property(&[gtk::accessible::Property::Label("Open Navigation")]);
        header.pack_start(&menu);
        let title = gtk::Label::new(None);
        title.add_css_class("wc-title");
        header.pack_start(&title);
        let header_end = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        header.pack_end(&header_end);

        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.set_transition_duration(220);

        let content = adw::ToolbarView::new();
        content.add_top_bar(&header);
        content.set_content(Some(&stack));

        // Navigation sidebar.
        let nav = gtk::ListBox::new();
        nav.add_css_class("wc-nav");
        nav.set_selection_mode(gtk::SelectionMode::Single);
        let nav_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let nav_scroll = gtk::ScrolledWindow::builder()
            .child(&nav)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        let nav_header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        nav_header.set_margin_top(6);
        nav_header.set_margin_start(6);
        nav_header.set_margin_bottom(4);
        let close_nav = gtk::Button::builder()
            .child(&PathIcon::new(paths::MENU, 18))
            .tooltip_text("Close Navigation")
            .css_classes(["flat", "wc-icon-button"])
            .build();
        nav_header.append(&close_nav);
        nav_box.append(&nav_header);
        nav_box.append(&nav_scroll);
        let settings_btn = gtk::Button::builder()
            .css_classes(["flat", "wc-nav-settings"])
            .build();
        let sb = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        sb.append(&PathIcon::new(paths::SETTINGS, 18));
        sb.append(&gtk::Label::new(Some("Settings")));
        settings_btn.set_child(Some(&sb));
        settings_btn.set_margin_start(8);
        settings_btn.set_margin_end(8);
        settings_btn.set_margin_bottom(8);
        nav_box.append(&settings_btn);
        nav_box.add_css_class("wc-sidebar");

        let split = adw::OverlaySplitView::builder()
            .collapsed(true)
            .sidebar(&nav_box)
            .content(&content)
            .max_sidebar_width(300.0)
            .sidebar_width_fraction(0.85)
            .build();
        split
            .bind_property("show-sidebar", &menu, "active")
            .bidirectional()
            .sync_create()
            .build();
        {
            let split = split.clone();
            close_nav.connect_clicked(move |_| split.set_show_sidebar(false));
        }

        toasts.set_child(Some(&split));
        {
            aurora.set_child(Some(&toasts));
            win.set_content(Some(&aurora));
            let aurora = aurora.clone();
            hub.subscribe(move |s| aurora.set_scheme(*s));
        }

        let this = Rc::new(Window {
            win: win.clone(),
            ctx,
            split: split.clone(),
            title,
            stack,
            header_end,
            nav: nav.clone(),
            nav_rows: RefCell::default(),
            pages: RefCell::default(),
            mode: Cell::new(ViewMode::Standard),
        });

        this.build_nav();
        {
            // Compact overlay (upstream "Keep on top"): minimal chrome, small
            // window. Wayland has no client-side always-on-top; compositors
            // can pin it (e.g. a niri window rule on the app id).
            let weak = Rc::downgrade(&this);
            let menu = menu.clone();
            let saved = Cell::new((0, 0));
            *this.ctx.compact.borrow_mut() = Some(Box::new(move |on| {
                let Some(w) = weak.upgrade() else { return };
                menu.set_visible(!on);
                w.title.set_visible(!on);
                if on {
                    saved.set((w.win.width(), w.win.height()));
                    w.win.unmaximize();
                    w.win.set_default_size(320, 420);
                    w.win.add_css_class("wc-compact");
                } else {
                    let (sw, sh) = saved.get();
                    if sw > 0 {
                        w.win.set_default_size(sw, sh);
                    }
                    w.win.remove_css_class("wc-compact");
                }
            }));
        }
        {
            let weak = Rc::downgrade(&this);
            nav.connect_row_activated(move |_, row| {
                if let Some(w) = weak.upgrade() {
                    let mode = w
                        .nav_rows
                        .borrow()
                        .iter()
                        .find(|(_, r, _)| r == row)
                        .map(|(m, _, _)| *m);
                    if let Some(mode) = mode {
                        w.set_mode(mode);
                        w.split.set_show_sidebar(false);
                    }
                }
            });
        }
        {
            let weak = Rc::downgrade(&this);
            settings_btn.connect_clicked(move |_| {
                if let Some(w) = weak.upgrade() {
                    w.split.set_show_sidebar(false);
                    crate::prefs::show(&w);
                }
            });
        }

        this.install_keyboard();
        {
            let weak = Rc::downgrade(&this);
            let paste = gtk::gio::SimpleAction::new("paste", None);
            paste.connect_activate(move |_, _| {
                if let Some(w) = weak.upgrade() {
                    w.paste();
                }
            });
            win.add_action(&paste);
        }
        {
            // This handler owns the Window for as long as the toplevel lives;
            // GTK drops it (breaking the cycle) when the window is destroyed.
            let this = this.clone();
            win.connect_close_request(move |win| {
                this.persist(win);
                glib::Propagation::Proceed
            });
        }

        let start = std::env::var("GMNB_MODE")
            .ok()
            .and_then(|m| ViewMode::from_key(&m))
            .or_else(|| ViewMode::from_key(&settings.mode))
            .unwrap_or(ViewMode::Standard);
        this.set_mode(start);
        if std::env::var("GMNB_NAV").as_deref() == Ok("1") {
            split.set_show_sidebar(true);
        }
        this
    }

    pub fn widget(&self) -> adw::ApplicationWindow {
        self.win.clone()
    }

    pub fn ctx(&self) -> &Rc<Ctx> {
        &self.ctx
    }

    fn build_nav(&self) {
        let mut rows = self.nav_rows.borrow_mut();
        let mut last_group = None;
        for mode in ViewMode::ALL {
            if last_group != Some(mode.group()) {
                last_group = Some(mode.group());
                let header = gtk::Label::new(Some(match mode.group() {
                    Group::Calculator => "CALCULATOR",
                    Group::Converter => "CONVERTER",
                }));
                header.add_css_class("wc-nav-header");
                header.set_xalign(0.0);
                let hr = gtk::ListBoxRow::builder()
                    .child(&header)
                    .activatable(false)
                    .selectable(false)
                    .build();
                hr.add_css_class("wc-nav-section");
                self.nav.append(&hr);
            }
            let icon = PathIcon::new(mode.icon(), 20);
            let label = gtk::Label::new(Some(mode.title()));
            label.set_xalign(0.0);
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 14);
            b.append(&icon);
            b.append(&label);
            let row = gtk::ListBoxRow::builder().child(&b).build();
            if let Some(n) = mode.alt_number() {
                row.set_tooltip_text(Some(&format!("{} (Alt+{n})", mode.title())));
            }
            self.nav.append(&row);
            rows.push((mode, row, icon));
        }
    }

    fn page(self: &Rc<Self>, kind: PageKind) -> Rc<dyn Page> {
        if let Some(p) = self.pages.borrow().get(&kind) {
            return p.clone();
        }
        let page: Rc<dyn Page> = match kind {
            PageKind::Date => pages::date::DatePage::new(self.ctx.clone()),
            PageKind::Calculator => pages::calculator::CalculatorPage::handle(self.ctx.clone()),
            PageKind::Converter => pages::converter::ConverterPage::handle(self.ctx.clone()),
            PageKind::Graphing => pages::graphing::GraphingPage::handle(self.ctx.clone()),
        };
        self.stack.add_named(&page.widget(), Some(kind.key()));
        self.pages.borrow_mut().insert(kind, page.clone());
        page
    }

    pub fn set_mode(self: &Rc<Self>, mode: ViewMode) {
        let previous = self.mode.get().page();
        if previous != mode.page()
            && let Some(old) = self.pages.borrow().get(&previous).cloned()
        {
            old.deactivate();
        }
        let page = self.page(mode.page());
        self.mode.set(mode);
        self.ctx.store.data.borrow_mut().mode = mode.key().into();
        self.title.set_text(mode.title());
        page.activate(mode);
        self.stack.set_visible_child_name(mode.page().key());

        while let Some(c) = self.header_end.first_child() {
            self.header_end.remove(&c);
        }
        for w in page.header_end() {
            self.header_end.append(&w);
        }

        for (m, row, icon) in self.nav_rows.borrow().iter() {
            if *m == mode && !row.is_selected() {
                self.nav.select_row(Some(row));
                icon.animate_draw();
            }
        }
    }

    pub fn current_page(self: &Rc<Self>) -> Rc<dyn Page> {
        self.page(self.mode.get().page())
    }

    fn install_keyboard(self: &Rc<Self>) {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _code, mods| {
            let (Some(w), Some(kp)) = (weak.upgrade(), key_press(key, mods)) else {
                return glib::Propagation::Proceed;
            };
            // Let text entries (graph equations, dialogs) type normally, but
            // still honour the app-wide chords a text field has no use for.
            if let Some(focus) = gtk::prelude::GtkWindowExt::focus(&w.win) {
                let in_text =
                    focus.is::<gtk::Text>() || focus.ancestor(gtk::Text::static_type()).is_some();
                if in_text && !input::is_global_chord(&kp) {
                    return glib::Propagation::Proceed;
                }
            }
            if w.handle_key(&kp) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.win.add_controller(keys);
    }

    pub fn handle_key(self: &Rc<Self>, kp: &KeyPress) -> bool {
        match input::window_shortcut(kp) {
            Some(WindowAction::SwitchMode(mode)) => {
                self.set_mode(mode);
                return true;
            }
            Some(WindowAction::Copy) => return self.current_page().copy().is_some(),
            Some(WindowAction::Paste) => {
                self.paste();
                return true;
            }
            None => {}
        }
        if kp.is(Named::Escape) && self.split.shows_sidebar() {
            self.split.set_show_sidebar(false);
            return true;
        }
        self.current_page().key_pressed(kp)
    }

    /// Dev/screenshot helper: type a script (see `appcore::input::parse_key_script`)
    /// through the real keyboard path.
    pub fn simulate_keys(self: &Rc<Self>, text: &str) {
        for kp in input::parse_key_script(text) {
            self.handle_key(&kp);
        }
    }

    pub fn paste(self: &Rc<Self>) {
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let weak = Rc::downgrade(self);
        let clipboard = display.clipboard();
        glib::spawn_future_local(async move {
            if let Ok(Some(text)) = clipboard.read_text_future().await
                && let Some(w) = weak.upgrade()
            {
                w.current_page().paste(&text);
            }
        });
    }

    fn persist(&self, win: &adw::ApplicationWindow) {
        for page in self.pages.borrow().values() {
            page.save();
        }
        {
            let mut d = self.ctx.store.data.borrow_mut();
            if !win.is_maximized() && !win.is_fullscreen() {
                d.width = win.width();
                d.height = win.height();
            }
        }
        self.ctx.store.persist();
    }
}
