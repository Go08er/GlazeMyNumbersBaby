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
    /// Bumped by every [`Window::set_mode`]: a paste whose read began on
    /// another page, or before the page was set up again, is dropped.
    page_generation: Cell<u64>,
    /// `win.paste`, enabled only while the page it pastes into isn't
    /// covered (an AT client is offered only enabled actions).
    paste_action: gtk::gio::SimpleAction,
}

/// What covers the window's parts now, from [`crate::inert::Layers`]'s
/// record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cover {
    /// The navigation sidebar: only a dialog or a modal window covers it.
    pub nav: bool,
    /// The current page: also the open sidebar.
    pub page: bool,
    /// What the page's keys and pastes change ([`Page::target`]): also a
    /// layer of the page's own (the History sheet).
    pub target: bool,
    /// The sidebar is open.
    pub sidebar: bool,
}

/// Where a key goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Window(WindowAction),
    CloseSidebar,
    /// To the page's own layer over its target ([`Page::layer_key_pressed`]).
    Layer,
    /// To the page ([`Page::key_pressed`]).
    Page,
    /// Nowhere: what it would change is covered.
    Refused,
}

/// Where `kp` goes, given what is covered (R14-M-06). Alt+1…5 is the
/// navigation's: refused only while a dialog covers that too (under the
/// sidebar it is the sidebar's own shortcut; under the History sheet the
/// header's menu can switch as well). Copy and paste read and change the
/// page's target: refused while it is covered. Escape closes an open
/// sidebar nothing covers. Every other key is the page's: refused while
/// the page is covered, and only its own layer's while just its target is.
pub fn route(kp: &KeyPress, c: Cover) -> Route {
    match input::window_shortcut(kp) {
        Some(WindowAction::SwitchMode(_)) if c.nav => return Route::Refused,
        Some(WindowAction::Copy | WindowAction::Paste) if c.target => return Route::Refused,
        Some(action) => return Route::Window(action),
        None => {}
    }
    if kp.is(Named::Escape) && c.sidebar && !c.nav {
        return Route::CloseSidebar;
    }
    if c.page {
        Route::Refused
    } else if c.target {
        Route::Layer
    } else {
        Route::Page
    }
}

/// What has the keyboard's focus, as far as the keys it takes before the
/// page goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focused {
    /// A text field (graph equations, dialogs): every key but the
    /// app-wide chords.
    Text,
    /// One of the calculator's own keys, upstream's `CalculatorButton`s
    /// and bit `FlipButtons` ([`Page::is_calculator_key`]): Space presses
    /// it and the arrows move on, but Enter is still "=", as upstream's
    /// ignore it.
    CalculatorKey,
    /// Any other control ([`is_control`]): a button, a toggle or menu
    /// button, a radio button, a list row, a switch, a calendar day, a
    /// link. Its activation and navigation keys are its own: Enter on a
    /// History item recalls it rather than evaluating (R16-M-03).
    Control,
    /// Nothing that takes keys (no focus, the window, a scroll view, a
    /// label): every key is the page's.
    Other,
}

/// Whether the focused widget (`focused`) gets `kp` before the page does.
/// (Popovers, the date pickers' calendars included, hold the keyboard
/// while open: their keys never reach the window's.)
pub fn focus_takes(focused: Focused, kp: &KeyPress) -> bool {
    use input::ControlKey;
    match (focused, input::control_key(kp)) {
        (Focused::Text, _) => !input::is_global_chord(kp),
        (Focused::Control, key) => key.is_some(),
        (Focused::CalculatorKey, Some(ControlKey::Navigate)) => true,
        (Focused::CalculatorKey, Some(ControlKey::Activate)) => !kp.is(Named::Enter),
        (Focused::CalculatorKey | Focused::Other, _) => false,
    }
}

/// Whether `w` is a control that takes its own activation and navigation
/// keys: GTK binds Enter and Space to activate a button (a toggle, a menu
/// button's, a link), a check or radio button, a switch, a list or flow
/// box row, a drop-down; the arrows move between rows, radio buttons or
/// days, and change a slider. Widgets of the app's own drawing that act as
/// one say so by their accessible role (a converter value field is a
/// button, and takes Space and Enter itself).
fn is_control(w: &gtk::Widget) -> bool {
    use gtk::AccessibleRole as R;
    w.is::<gtk::Button>()
        || w.is::<gtk::CheckButton>()
        || w.is::<gtk::Switch>()
        || w.is::<gtk::ListBoxRow>()
        || w.is::<gtk::FlowBoxChild>()
        || w.is::<gtk::DropDown>()
        || w.is::<gtk::Expander>()
        || matches!(
            w.accessible_role(),
            R::Button
                | R::ToggleButton
                | R::Checkbox
                | R::Radio
                | R::Switch
                | R::Link
                | R::Tab
                | R::ListItem
                | R::Row
                | R::Option
                | R::TreeItem
                | R::MenuItem
                | R::MenuItemCheckbox
                | R::MenuItemRadio
                | R::ComboBox
                | R::Slider
        )
}

pub fn apply_theme_setting(theme: &str) {
    let sm = adw::StyleManager::default();
    sm.set_color_scheme(match theme {
        "light" => adw::ColorScheme::ForceLight,
        "dark" => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    });
}

/// The window class for a see-through backdrop.
pub const SEE_THROUGH: &str = "wc-see-through";

/// Show the backdrop at `alpha`: below 1 it, and the window under it, are
/// see-through, where the display composites windows. Without that (X11
/// with no compositing manager) a see-through window shows black, so it
/// stays opaque.
pub fn apply_backdrop(win: &adw::ApplicationWindow, aurora: &Aurora, alpha: f32) {
    let see_through = alpha < 1.0 && WidgetExt::display(win).is_composited();
    aurora.set_backdrop_alpha(if see_through { alpha } else { 1.0 });
    if see_through == win.has_css_class(SEE_THROUGH) {
        return;
    }
    if see_through {
        win.add_css_class(SEE_THROUGH);
    } else {
        win.remove_css_class(SEE_THROUGH);
    }
    // Text drawn by hand (the display) adds or drops its halo.
    fn redraw(w: &gtk::Widget) {
        w.queue_draw();
        let mut c = w.first_child();
        while let Some(child) = c {
            redraw(&child);
            c = child.next_sibling();
        }
    }
    redraw(win.upcast_ref());
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
        if let Some(v) = std::env::var("GMNB_OPACITY")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            store.data.borrow_mut().background_opacity = v;
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

        // Saved (or overridden) sizes are hints: keep them to what a window
        // can be. GTK aborts on anything below -1.
        let defaults = crate::settings::Settings::default();
        let fit = |v: i32, min: i32, fallback: i32| {
            if v <= 0 {
                fallback
            } else {
                v.clamp(min, 16384)
            }
        };
        let win = adw::ApplicationWindow::builder()
            .application(app)
            .title(crate::APP_NAME)
            .default_width(fit(settings.width, 320, defaults.width))
            .default_height(fit(settings.height, 480, defaults.height))
            .width_request(320)
            .height_request(480)
            .css_classes(["gmnb"])
            .build();

        let aurora = Aurora::default();
        aurora.set_animated(settings.animated_background && std::env::var("GMNB_STILL").is_err());
        apply_backdrop(
            &win,
            &aurora,
            crate::settings::backdrop_alpha(settings.background_opacity),
        );
        {
            // A compositing manager can start or stop while GMNB runs (X11).
            let (w, a, st) = (win.downgrade(), aurora.downgrade(), Rc::downgrade(&store));
            WidgetExt::display(&win).connect_composited_notify(move |_| {
                if let (Some(w), Some(a), Some(st)) = (w.upgrade(), a.upgrade(), st.upgrade()) {
                    let v = st.data.borrow().background_opacity;
                    apply_backdrop(&w, &a, crate::settings::backdrop_alpha(v));
                }
            });
        }
        let toasts = adw::ToastOverlay::new();
        let ctx = Rc::new(Ctx {
            hub: hub.clone(),
            aurora: aurora.clone(),
            toasts: toasts.clone(),
            store: store.clone(),
            compact: Default::default(),
            layers: crate::inert::Layers::new(&win),
            precision: crate::pages::Followed::new(settings.literal_digits),
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

        {
            // The sidebar overlays the content while collapsed.
            let weak = split.downgrade();
            ctx.layers.scrim(
                &content,
                &split,
                &["show-sidebar", "collapsed"],
                move || {
                    weak.upgrade()
                        .is_some_and(|s| s.is_collapsed() && s.shows_sidebar())
                },
            );
        }
        toasts.set_child(Some(&split));
        {
            // The window gets its content in `present`.
            aurora.set_child(Some(&toasts));
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
            page_generation: Cell::new(0),
            paste_action: gtk::gio::SimpleAction::new("paste", None),
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
            // A row is picked by activating it; selecting one (the keys
            // move the selection) only marks it. GTK carries out an
            // assistive technology's Selection request whether or not the
            // list is covered (crate::inert): covered, or left with no
            // row, the list marks the current mode again (R15-M-02).
            let weak = Rc::downgrade(&this);
            nav.connect_row_selected(move |nav, row| {
                let Some(w) = weak.upgrade() else { return };
                // set_mode has set the mode before it selects the mode's
                // row, holding the rows (shared, so this borrow succeeds).
                let Ok(rows) = w.nav_rows.try_borrow() else {
                    return;
                };
                let current = rows
                    .iter()
                    .find(|(m, _, _)| *m == w.mode.get())
                    .map(|(_, r, _)| r.clone());
                drop(rows);
                if let Some(current) = current
                    && row != Some(&current)
                    && (row.is_none() || w.ctx.layers.covers(nav))
                {
                    nav.select_row(Some(&current));
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
            this.paste_action.connect_activate(move |_, _| {
                if let Some(w) = weak.upgrade() {
                    w.paste();
                }
            });
            win.add_action(&this.paste_action);
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
        {
            let weak = Rc::downgrade(&this);
            this.ctx.layers.connect_changed(move || {
                if let Some(w) = weak.upgrade() {
                    w.sync_actions();
                }
            });
        }
        this.sync_actions();
        if std::env::var("GMNB_NAV").as_deref() == Ok("1") {
            split.set_show_sidebar(true);
        }
        this
    }

    pub fn widget(&self) -> adw::ApplicationWindow {
        self.win.clone()
    }

    /// Show the window. Its content goes in right after the window is
    /// realized, still before the first frame: realizing creates the GPU
    /// renderer, which takes a while, and GTK meanwhile finishes loading the
    /// icon theme on its own thread. Attached any earlier, the header bar's
    /// window controls would wait for that theme on the main thread.
    pub fn present(&self) {
        self.win.present();
        if self.win.content().is_none() {
            self.win.set_content(Some(&self.ctx.aurora));
        }
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
                // Named as upstream's NavCategoryGroup ("Calculators
                // category").
                hr.update_property(&[gtk::accessible::Property::Label(match mode.group() {
                    Group::Calculator => "Calculators category",
                    Group::Converter => "Converters category",
                })]);
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
            // Named as upstream's NavCategory, the mode and its group
            // ("Standard Calculator", "Length Converter"): GTK names a row
            // only from its tooltip, which the converters have none of.
            let group = match mode.group() {
                Group::Calculator => "Calculator",
                Group::Converter => "Converter",
            };
            row.update_property(&[gtk::accessible::Property::Label(&format!(
                "{} {group}",
                mode.title()
            ))]);
            if let Some(n) = mode.alt_number() {
                row.set_tooltip_text(Some(&format!("{} (Alt+{n})", mode.title())));
                row.update_property(&[gtk::accessible::Property::KeyShortcuts(&format!(
                    "Alt+{n}"
                ))]);
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
        self.page_generation.set(self.page_generation.get() + 1);
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
        self.sync_actions();
    }

    /// The current page, if it is made already (never makes one).
    fn shown_page(&self) -> Option<Rc<dyn Page>> {
        let pages = self.pages.try_borrow().ok()?;
        pages.get(&self.mode.get().page()).cloned()
    }

    /// What covers the window's parts now (crate::inert's record).
    fn cover(&self, page: &Rc<dyn Page>) -> Cover {
        let layers = &self.ctx.layers;
        Cover {
            nav: layers.covers(&self.nav),
            page: layers.covers(&page.widget()),
            target: layers.covers(&page.target()),
            sidebar: self.split.shows_sidebar(),
        }
    }

    /// Whether the current page's target is covered (or there is none).
    fn target_covered(&self) -> bool {
        self.shown_page()
            .is_none_or(|p| self.ctx.layers.covers(&p.target()))
    }

    /// `win.paste` is enabled only while it would paste into a page
    /// nothing covers.
    fn sync_actions(&self) {
        let enabled = !self.target_covered();
        if self.paste_action.is_enabled() != enabled {
            self.paste_action.set_enabled(enabled);
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
            // still honour the app-wide chords a text field has no use for;
            // and let a focused control have its own keys (focus_takes).
            if let Some(focus) = gtk::prelude::GtkWindowExt::focus(&w.win)
                && focus_takes(w.focused(&focus), &kp)
            {
                return glib::Propagation::Proceed;
            }
            // What the key would change may be covered: handle_key asks.
            if w.handle_key(&kp) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.win.add_controller(keys);
    }

    /// What `focus`, the window's focus, is ([`Focused`]).
    fn focused(&self, focus: &gtk::Widget) -> Focused {
        if focus.is::<gtk::Text>() || focus.ancestor(gtk::Text::static_type()).is_some() {
            Focused::Text
        } else if self
            .shown_page()
            .is_some_and(|p| p.is_calculator_key(focus))
        {
            Focused::CalculatorKey
        } else if is_control(focus) {
            Focused::Control
        } else {
            Focused::Other
        }
    }

    /// A key, wherever it comes from (the keyboard, `GMNB_KEYS`), goes
    /// where [`route`] says, given what crate::inert's record says is
    /// covered: a dialog, a modal window, the sidebar or the History sheet
    /// over what it would change refuses it (R14-M-06).
    pub fn handle_key(self: &Rc<Self>, kp: &KeyPress) -> bool {
        let page = self.current_page();
        match route(kp, self.cover(&page)) {
            Route::Window(WindowAction::SwitchMode(mode)) => {
                self.set_mode(mode);
                true
            }
            Route::Window(WindowAction::Copy) => page.copy().is_some(),
            Route::Window(WindowAction::Paste) => {
                self.paste();
                true
            }
            Route::CloseSidebar => {
                self.split.set_show_sidebar(false);
                true
            }
            Route::Layer => page.layer_key_pressed(kp),
            Route::Page => page.key_pressed(kp),
            Route::Refused => false,
        }
    }

    /// Dev/screenshot helper: type a script (see `appcore::input::parse_key_script`)
    /// through the real keyboard path.
    pub fn simulate_keys(self: &Rc<Self>, text: &str) {
        for kp in input::parse_key_script(text) {
            self.handle_key(&kp);
        }
    }

    /// Pastes the clipboard into the current page (Ctrl+V, Shift+Insert,
    /// `win.paste`). Not while the page's target is covered; and the read
    /// takes a while, so the text is dropped if by then the page changed
    /// ([`Window::set_mode`]) or its target is covered (R14-M-06).
    pub fn paste(self: &Rc<Self>) {
        if self.target_covered() {
            return;
        }
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let generation = self.page_generation.get();
        let weak = Rc::downgrade(self);
        let clipboard = display.clipboard();
        glib::spawn_future_local(async move {
            let Some(text) = crate::paste::read_text(&clipboard).await else {
                return;
            };
            if let Some(w) = weak.upgrade()
                && w.page_generation.get() == generation
                && !w.target_covered()
                && let Some(page) = w.shown_page()
            {
                page.paste(&text);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// R14-M-06: a covered target takes no key, copy or paste (Settings,
    /// the sidebar, the History sheet); Alt+1…5 is refused only under a
    /// dialog; the History sheet takes its own keys; Escape closes an
    /// open sidebar only when nothing covers it.
    #[test]
    fn keys_reach_nothing_covered() {
        let open = Cover::default();
        let dialog = Cover {
            nav: true,
            page: true,
            target: true,
            sidebar: false,
        };
        let sidebar = Cover {
            nav: false,
            page: true,
            target: true,
            sidebar: true,
        };
        let sheet = Cover {
            nav: false,
            page: false,
            target: true,
            sidebar: false,
        };
        let seven = KeyPress::char('7');
        let ctrl_v = KeyPress::char('v').ctrl();
        let shift_insert = KeyPress::named(Named::Insert).shift();
        let ctrl_c = KeyPress::char('c').ctrl();
        let alt_2 = KeyPress::char('2').alt();
        let ctrl_h = KeyPress::char('h').ctrl();
        let escape = KeyPress::named(Named::Escape);
        let scientific = Route::Window(WindowAction::SwitchMode(ViewMode::Scientific));

        assert_eq!(route(&seven, open), Route::Page);
        assert_eq!(route(&ctrl_v, open), Route::Window(WindowAction::Paste));
        assert_eq!(
            route(&shift_insert, open),
            Route::Window(WindowAction::Paste)
        );
        assert_eq!(route(&ctrl_c, open), Route::Window(WindowAction::Copy));
        assert_eq!(route(&alt_2, open), scientific);
        assert_eq!(route(&escape, open), Route::Page);

        for c in [dialog, sidebar, sheet] {
            for k in [&ctrl_v, &shift_insert, &ctrl_c] {
                assert_eq!(route(k, c), Route::Refused, "{k:?} under {c:?}");
            }
        }
        assert_eq!(route(&seven, dialog), Route::Refused);
        assert_eq!(route(&seven, sidebar), Route::Refused);
        assert_eq!(route(&seven, sheet), Route::Layer);
        assert_eq!(route(&ctrl_h, sheet), Route::Layer);
        assert_eq!(route(&ctrl_h, dialog), Route::Refused);

        assert_eq!(route(&alt_2, dialog), Route::Refused);
        assert_eq!(route(&alt_2, sidebar), scientific);
        assert_eq!(route(&alt_2, sheet), scientific);

        assert_eq!(route(&escape, sidebar), Route::CloseSidebar);
        let dialog_over_sidebar = Cover {
            sidebar: true,
            ..dialog
        };
        assert_eq!(route(&escape, dialog_over_sidebar), Route::Refused);
    }

    /// R16-M-03: a focused control (a History item, a toggle, a menu
    /// button) takes Enter, Space and its navigation keys before the
    /// calculator, so Enter on a History item recalls it; a calculator key
    /// (and no focus in particular) leaves Enter to the calculator ("="),
    /// as upstream's do. Everything else typed is the calculator's
    /// wherever the focus is; a text field keeps all but the app-wide
    /// chords.
    #[test]
    fn focused_controls_take_their_own_keys() {
        use Focused::*;
        let enter = KeyPress::named(Named::Enter);
        let space = KeyPress::char(' ');
        let down = KeyPress::named(Named::Down);
        let page_down = KeyPress::named(Named::PageDown);
        let seven = KeyPress::char('7');
        let escape = KeyPress::named(Named::Escape);
        let ctrl_h = KeyPress::char('h').ctrl();
        let alt_up = KeyPress::named(Named::Up).alt();
        let alt_2 = KeyPress::char('2').alt();

        for k in [&enter, &space, &down, &page_down] {
            assert!(focus_takes(Control, k), "{k:?}");
        }
        for k in [&seven, &escape, &ctrl_h, &alt_up, &alt_2, &enter.shift()] {
            assert!(!focus_takes(Control, k), "{k:?}");
        }

        assert!(!focus_takes(CalculatorKey, &enter));
        for k in [&space, &down, &page_down] {
            assert!(focus_takes(CalculatorKey, k), "{k:?}");
        }
        assert!(!focus_takes(CalculatorKey, &seven));

        for k in [&enter, &space, &down, &seven, &escape] {
            assert!(!focus_takes(Other, k), "{k:?}");
            assert!(focus_takes(Text, k), "{k:?}");
        }
        assert!(!focus_takes(Text, &alt_2));
        assert!(!focus_takes(Text, &KeyPress::named(Named::Home).ctrl()));
    }
}
