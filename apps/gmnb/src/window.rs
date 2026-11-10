//! The main window: backdrop → toasts → overlay navigation sidebar →
//! header + page stack. Pages are created lazily the first time they're
//! needed and several modes can share one page (as upstream does).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use appcore::input::{self, Focus, FocusKey, WindowAction};
use appcore::modes::{Group, PageKind, ViewMode};
use appcore::{KeyPress, Named};
use gtk::{gdk, glib};

use crate::keymap::key_press;
use crate::pages::{self, Ctx, Page};
use crate::settings::{Persist, Store};
use crate::theme::Hub;
use crate::widgets::backdrop::Backdrop;
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
/// sidebar nothing covers, whatever is held with it (as it closes a GTK
/// popover, a flyout included, and DGMNB's overlays). Every other key is
/// the page's: refused while the page is covered, and only its own
/// layer's while just its target is.
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

/// What the window does with a key press, before any widget has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// It goes on to the focused widget, whose GTK's bindings do the
    /// rest: a text field's typing, a control's bare Enter or Space, its
    /// arrows.
    Widget,
    /// The focused widget's activation, which the window carries out
    /// itself ([`activate`]): Enter or Space with something held (Shift,
    /// Ctrl; Alt with Enter), which are the control's as the bare keys
    /// are ([`input::control_key`]) but which GTK, binding the bare keys
    /// only, would do nothing with.
    Activate,
    /// The app's and the page's ([`Window::handle_key`]); GTK's if they
    /// have no use for it (Tab).
    Page,
    /// The app's and the page's, and nobody's if they have no use for it:
    /// Enter on a calculator key, whatever is held. The key ignores it, as
    /// upstream's do, wherever the page has no Enter of its own (the
    /// converter's and the graph's keypads): GTK would press the key.
    PageOnly,
}

/// What the window does with `kp`, given what has the focus (`focused`,
/// if anything): whose it is by the rule both apps share
/// ([`input::focus_key`]). (Popovers hold the keyboard while open, and so
/// does a dialog: their keys never reach the window's controllers. A
/// page's flyouts get the window's routing through [`Ctx::flyout`], its
/// other popovers and the dialogs their controls' own keys alone,
/// [`own_keys`].) In a flyout (`flyout`) Escape is the flyout's too: it
/// closes it, whatever is held with it (GTK's popovers close so).
pub fn step(focused: Option<Focus>, kp: &KeyPress, flyout: bool) -> Step {
    if flyout && kp.is(Named::Escape) {
        return Step::Widget;
    }
    match focused.map(|f| input::focus_key(f, kp)) {
        Some(FocusKey::Activate) if kp.shift || kp.ctrl || kp.alt => Step::Activate,
        Some(FocusKey::Activate | FocusKey::Navigate | FocusKey::Edit) => Step::Widget,
        Some(FocusKey::PageOnly) => Step::PageOnly,
        Some(FocusKey::Page) | None => Step::Page,
    }
}

/// What `w`, a window's focus, is to the keys ([`Focus`]), `key` saying
/// whether it is one of the page's own keys ([`Page::is_calculator_key`]).
fn focus_kind(w: &gtk::Widget, key: impl FnOnce(&gtk::Widget) -> bool) -> Focus {
    if w.is::<gtk::Text>() || w.ancestor(gtk::Text::static_type()).is_some() {
        Focus::Text
    } else if key(w) {
        Focus::CalculatorKey
    } else if w.is::<gtk::Range>() {
        Focus::Slider
    } else if is_control(w) {
        Focus::Control
    } else {
        Focus::Other
    }
}

/// Activates `focus` as its bare Enter or Space would ([`Step::Activate`]):
/// GTK's activation signal, the one those keys are bound to (a button's
/// click, a row's activation). A text field's Enter is bound to its own
/// `activate` signal instead, which an entry or a spin button passes on
/// (`gtk_widget_activate` does nothing for it). False if the widget has
/// neither, as one of the app's own that takes its keys itself (a
/// converter value field): the key then goes on to it.
fn activate(focus: &gtk::Widget) -> bool {
    // The key stops here, before what notes how a control was used sees
    // it: noted here, the control is known to be the keyboard's doing
    // (not assistive technology's, which makes no sound).
    if let Some(hub) = crate::theme::Hub::current() {
        hub.layers().key_activated();
    }
    let text = focus.downcast_ref::<gtk::Text>().cloned().or_else(|| {
        focus
            .ancestor(gtk::Text::static_type())
            .and_downcast::<gtk::Text>()
    });
    match text {
        Some(text) => {
            text.emit_activate();
            true
        }
        None => focus.activate(),
    }
}

/// The move the window makes itself for `kp` on a focused slider (a
/// `GtkScale`: a graph variable's, Settings'): a slider's navigation keys
/// are its own whatever is held with them ([`Focus::Slider`], as
/// upstream's `Slider` reads the key alone), but GTK binds only the bare
/// keys to their moves, binds none with Shift or Alt, and pages with Ctrl.
/// The bare key's move, then, for a key with something held; `None` for
/// the bare key, which is GTK's own, and for any other key.
pub fn slide(kp: &KeyPress) -> Option<gtk::ScrollType> {
    use gtk::ScrollType as S;
    let appcore::Key::Named(key) = kp.key else {
        return None;
    };
    // The scroll each key is bound to bare (gtkrange.c).
    let bare = match key {
        Named::Left => S::StepLeft,
        Named::Right => S::StepRight,
        Named::Up => S::StepUp,
        Named::Down => S::StepDown,
        Named::PageUp => S::PageUp,
        Named::PageDown => S::PageDown,
        Named::Home => S::Start,
        Named::End => S::End,
        _ => return None,
    };
    (kp.shift || kp.ctrl || kp.alt).then_some(bare)
}

/// What the window does itself with a key that is `focus`'s own (`step`:
/// [`Step::Activate`], [`Step::Widget`]) and that GTK binds to nothing, or
/// to something else: the activation for Enter or Space with something
/// held ([`activate`]), a slider's move for one of its keys with something
/// held ([`slide`]). False if it did nothing: the key goes on to the
/// widget.
fn own_key(focus: &gtk::Widget, kp: &KeyPress, step: Step) -> bool {
    match step {
        Step::Activate => activate(focus),
        Step::Widget => match (focus.downcast_ref::<gtk::Range>(), slide(kp)) {
            (Some(slider), Some(to)) => {
                slider.emit_move_slider(to);
                true
            }
            _ => false,
        },
        Step::Page | Step::PageOnly => false,
    }
}

/// Gives `layer`, one the window doesn't route the keys of, the part of
/// that routing its controls need ([`own_key`]): Enter or Space with
/// something held activates the focused one, and a slider's keys move it
/// whatever is held, as in the window. For a page's popovers that aren't
/// flyouts of its own keys (the date pickers' calendars, the graph's
/// settings and line styles, the display's and a History item's menus)
/// and for the dialogs (Settings, About, the licences): GTK's own popups
/// (a drop-down's list, a colour dialog) keep GTK's bindings.
pub fn own_keys(layer: &impl IsA<gtk::Widget>) {
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(|c, key, _code, mods| {
        let focus = c.widget().and_then(|w| w.root()).and_then(|r| r.focus());
        if let (Some(kp), Some(focus)) = (key_press(key, mods), focus)
            && own_key(
                &focus,
                &kp,
                step(Some(focus_kind(&focus, |_| false)), &kp, true),
            )
        {
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    layer.add_controller(keys);
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

/// A pointer press in `widget` hides the focus ring, which only keyboard
/// use shows (as DGMNB's and upstream's): a click that focuses a key or a
/// button (focus-on-click) moves the focus without drawing it. GTK turns
/// the ring on and off by keys only (and starts with it on), so a click
/// after Tab would draw it around the clicked key. The controller runs
/// first (capture), before the button takes the focus. For the window,
/// and for each popover whose keys take the focus on a click: an
/// autohide popover holds a grab, so presses in it never reach the
/// window's controllers.
pub fn hide_ring_on_press(widget: &impl IsA<gtk::Widget>) {
    let pointer = gtk::EventControllerLegacy::new();
    pointer.set_propagation_phase(gtk::PropagationPhase::Capture);
    pointer.connect_event(|c, event| {
        if matches!(
            event.event_type(),
            gdk::EventType::ButtonPress | gdk::EventType::TouchBegin
        ) && let Some(win) = c
            .widget()
            .and_then(|w| w.root())
            .and_downcast::<gtk::Window>()
        {
            win.set_focus_visible(false);
        }
        glib::Propagation::Proceed
    });
    widget.add_controller(pointer);
}

/// Lays out the text of every label in `w` (and `w` itself, if it is one)
/// where DGMNB draws it: at a fraction of a pixel. A label rounds its
/// text's place down to a whole pixel, unless it is laid out across its
/// whole width (as when it may be ellipsized): Pango then places each
/// line, to the fraction, at the start, the middle or the end of it, by
/// the label's justification, set here from its alignment
/// (docs/theming.md, "Text in its place").
pub fn exact_text(w: &impl IsA<gtk::Widget>) {
    let w = w.as_ref();
    if let Some(label) = w.downcast_ref::<gtk::Label>() {
        let x = label.xalign();
        // (A label at its start is placed exactly as it is.)
        if x >= 0.25 && label.ellipsize() == gtk::pango::EllipsizeMode::None && !label.wraps() {
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        }
        label.set_justify(if x < 0.25 {
            gtk::Justification::Left
        } else if x > 0.75 {
            gtk::Justification::Right
        } else {
            gtk::Justification::Center
        });
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        exact_text(&c);
        child = c.next_sibling();
    }
}

/// A flat round icon button (`icon_button`): its name, which it is read
/// out as, is its tooltip too.
pub fn icon_button(glyph: &str, name: &str) -> gtk::Button {
    let b = gtk::Button::builder()
        .child(&PathIcon::new(glyph, 18))
        .tooltip_text(name)
        .css_classes(["flat", "wc-icon-button"])
        .build();
    b.update_property(&[gtk::accessible::Property::Label(name)]);
    b
}

/// Keeps an icon (or the icon a button holds) the size, and its line the
/// width, that the control at `path` gives its icon.
pub fn follow_icon(hub: &Hub, widget: &impl IsA<gtk::Widget>, path: &'static str) {
    let widget = widget.as_ref();
    let icon = widget.downcast_ref::<PathIcon>().cloned().or_else(|| {
        widget
            .first_child()
            .and_then(|c| c.downcast::<PathIcon>().ok())
    });
    if let Some(icon) = icon {
        hub.subscribe_style(&icon, move |icon, style| {
            if let Some(c) = style.control(path) {
                let size = c.icon.size;
                icon.set_style(
                    size,
                    c.icon.stroke.unwrap_or_else(|| style.icon_stroke(size)),
                );
            }
        });
    }
}

/// A navigation row's content, DGMNB's: the icon centred in a box of its
/// own at the row's padding, the label after it.
fn nav_row_content(glyph: &str, title: &str) -> (gtk::Box, PathIcon) {
    use appcore::metrics::nav::{ROW_ICON, ROW_ICON_BOX, ROW_ICON_X, ROW_LABEL_X};
    let icon = PathIcon::new(glyph, ROW_ICON as i32);
    icon.set_size_request(ROW_ICON_BOX as i32, -1);
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    let b = gtk::Box::new(
        gtk::Orientation::Horizontal,
        (ROW_LABEL_X - ROW_ICON_X - ROW_ICON_BOX) as i32,
    );
    b.append(&icon);
    b.append(&label);
    (b, icon)
}

/// A scrolled list, DGMNB's way: a thin thumb (`scrollbar`) over the
/// list's right edge whenever the list is longer than its view, as long
/// as DGMNB's (`ui.rs` `scroll_end`), rather than the toolkit's
/// scrollbar, which shows only while the pointer moves. The list scrolls
/// as before (wheel, keys, touch). A theme that hands the scrollbar back
/// (`native`) gets the toolkit's.
pub fn scroller(hub: &Hub, child: &impl IsA<gtk::Widget>) -> gtk::Overlay {
    let scroll = gtk::ScrolledWindow::builder()
        .child(child)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let thumb = gtk::Box::new(gtk::Orientation::Vertical, 0);
    thumb.add_css_class("wc-scroll-thumb");
    thumb.set_halign(gtk::Align::End);
    thumb.set_valign(gtk::Align::Start);
    thumb.set_can_target(false);
    thumb.set_visible(false);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&scroll));
    overlay.add_overlay(&thumb);
    overlay.set_vexpand(true);
    let own = Rc::new(Cell::new(true));
    let place = {
        let (thumb, own) = (thumb.downgrade(), own.clone());
        move |adj: &gtk::Adjustment| {
            let Some(thumb) = thumb.upgrade() else { return };
            let view = adj.page_size();
            let content = adj.upper() - adj.lower();
            let max = content - view;
            let shown = own.get() && view > 0.0 && max > 0.5;
            thumb.set_visible(shown);
            if shown {
                let min = f64::from(appcore::metrics::controls::SCROLLBAR_MIN_THUMB);
                let h = (view * view / content).max(min);
                let y = (view - h) * ((adj.value() - adj.lower()) / max);
                thumb.set_size_request(-1, h.round() as i32);
                thumb.set_margin_top(y.round() as i32);
            }
        }
    };
    let adj = scroll.vadjustment();
    adj.connect_changed(place.clone());
    adj.connect_value_changed(place.clone());
    let scroll = scroll.downgrade();
    hub.subscribe_style(&overlay, move |_, style| {
        let Some(scroll) = scroll.upgrade() else {
            return;
        };
        let native = style.native().contains(&"scrollbar");
        own.set(!native);
        scroll.set_vscrollbar_policy(if native {
            gtk::PolicyType::Automatic
        } else {
            gtk::PolicyType::External
        });
        place(&scroll.vadjustment());
    });
    overlay
}

/// The window's own minimize, maximize (or restore) and close buttons,
/// in DGMNB's order and with its glyphs (`header.controls`): the
/// toolkit's are switched off. A click doesn't focus them, as DGMNB's:
/// Enter after maximizing mustn't restore.
struct WindowControls {
    root: gtk::Box,
    minimize: gtk::Button,
    maximize: gtk::Button,
}

impl WindowControls {
    fn new(win: &adw::ApplicationWindow, hub: &Hub) -> Self {
        use appcore::icons::{CLOSE, WINDOW_MAXIMIZE, WINDOW_MINIMIZE, WINDOW_RESTORE};
        let root = gtk::Box::new(
            gtk::Orientation::Horizontal,
            appcore::metrics::window::HEADER_BUTTON_GAP as i32,
        );
        let button = |glyph: &str, name: &str, path: &'static str| {
            let b = icon_button(glyph, name);
            b.remove_css_class("wc-icon-button");
            b.add_css_class("wc-window-button");
            b.set_focus_on_click(false);
            follow_icon(hub, &b, path);
            root.append(&b);
            b
        };
        let minimize = button(WINDOW_MINIMIZE, "Minimize", "header.controls");
        let maximize = button(WINDOW_MAXIMIZE, "Maximize", "header.controls");
        let close = button(CLOSE, "Close", "header.close");
        close.add_css_class("wc-close");
        let weak = win.downgrade();
        minimize.connect_clicked(move |_| {
            if let Some(w) = weak.upgrade() {
                w.minimize();
            }
        });
        let weak = win.downgrade();
        maximize.connect_clicked(move |_| {
            if let Some(w) = weak.upgrade() {
                if w.is_maximized() {
                    w.unmaximize();
                } else {
                    w.maximize();
                }
            }
        });
        let weak = win.downgrade();
        close.connect_clicked(move |_| {
            if let Some(w) = weak.upgrade() {
                w.close();
            }
        });
        let max = maximize.downgrade();
        win.connect_maximized_notify(move |w| {
            let Some(b) = max.upgrade() else { return };
            let (glyph, name) = if w.is_maximized() {
                (WINDOW_RESTORE, "Restore")
            } else {
                (WINDOW_MAXIMIZE, "Maximize")
            };
            if let Some(icon) = b.child().and_downcast::<PathIcon>() {
                icon.set_path(glyph);
            }
            b.set_tooltip_text(Some(name));
            b.update_property(&[gtk::accessible::Property::Label(name)]);
        });
        WindowControls {
            root,
            minimize,
            maximize,
        }
    }

    /// Compact (keep on top) keeps Close alone, as DGMNB.
    fn set_compact(&self, on: bool) {
        self.minimize.set_visible(!on);
        self.maximize.set_visible(!on);
    }
}

/// Text is drawn as DGMNB draws it: unhinted outlines in grey levels,
/// where the font puts them. (GTK's automatic choice at scale 1 hints them
/// to the pixel grid, which moves and stretches every glyph by up to a
/// pixel: docs/theming.md, "Text in its place".) Set for the app, over the
/// desktop's own settings: a widget's own font options leave GTK's
/// automatic rendering rounding glyphs to whole pixels, which the
/// chrome's labels can't take.
pub fn match_text_rendering() {
    let Some(s) = gtk::Settings::default() else {
        return;
    };
    s.set_gtk_font_rendering(gtk::FontRendering::Manual);
    s.set_gtk_xft_antialias(1);
    s.set_gtk_xft_hinting(0);
    s.set_gtk_xft_hintstyle(Some("hintnone"));
    s.set_gtk_xft_rgba(Some("none"));
    s.set_gtk_hint_font_metrics(false);
}

pub fn apply_theme_setting(theme: &str) {
    let sm = adw::StyleManager::default();
    sm.set_color_scheme(match theme {
        "light" => adw::ColorScheme::ForceLight,
        "dark" => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    });
}

/// Show the backdrop at `alpha`: below 1 the desktop shows through it (a
/// Wayland compositor always composites windows; the window itself is
/// always a transparent surface). The style follows through the hub
/// (`Hub::set_opacity`): what goes under and round text on a see-through
/// window (the scrim, the halos) is the style's.
pub fn apply_backdrop(backdrop: &Backdrop, alpha: f32) {
    backdrop.set_alpha(alpha.min(1.0));
}

/// What the theme hub starts with: the saved settings, and what the
/// environment asks of this run (docs/theming.md, "Environment hooks"),
/// which is never saved.
fn theme_config(settings: &crate::settings::Settings) -> crate::theme::Config {
    use gmnb_theme::theme::{BUILT_IN, DEV_ID};
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let mut run_options = Vec::new();
    let mut theme = settings.active_theme.clone();
    let dev_dir = env("GMNB_THEME_DIR").map(std::path::PathBuf::from);
    if dev_dir.is_some() {
        theme = DEV_ID.to_string();
    }
    // Kept for the old scripts: Aurora in that palette.
    if let Some(palette) = env("GMNB_PALETTE") {
        theme = crate::settings::AURORA.to_string();
        run_options.push(("palette".to_string(), palette));
    }
    if let Some(id) = crate::THEME_ARG
        .get()
        .cloned()
        .flatten()
        .or_else(|| env("GMNB_THEME"))
    {
        theme = id;
    }
    if let Some(options) = env("GMNB_THEME_OPTIONS") {
        run_options.extend(crate::theme::options_from_env(&options));
    }
    // Every theme with saved options: the built-in ones, and whatever the
    // settings name.
    let ids = BUILT_IN
        .iter()
        .map(|b| b.id.to_string())
        .chain(settings.theme_options.keys().cloned())
        .chain([theme.clone()]);
    crate::theme::Config {
        saved_options: ids
            .map(|id| {
                let options = settings.options_of(&id);
                (id, options)
            })
            .collect(),
        theme,
        run_options,
        opacity: crate::settings::backdrop_alpha(settings.background_opacity),
        system_accent: settings.system_accent,
        accent: env("GMNB_ACCENT").and_then(|v| crate::theme::accent_from_env(&v)),
        dev_dir,
        sound: settings.sound,
        sound_volume: crate::settings::sound_volume(settings.sound_volume),
    }
}

impl Window {
    pub fn new(app: &adw::Application) -> Rc<Self> {
        let screenshot = std::env::var_os("GMNB_SCREENSHOT").is_some();
        // Screenshots don't touch saved state unless explicitly asked to read it.
        let ephemeral = screenshot && std::env::var_os("GMNB_REAL_STORE").is_none();
        let store = Rc::new(if ephemeral {
            let store = Store::ephemeral();
            store.data.borrow_mut().migrate();
            store
        } else {
            crate::settings::load_store()
        });
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
        match_text_rendering();
        let hub = Hub::new(theme_config(&settings));
        crate::theme::follow_portal_accent(&hub);

        // Saved (or overridden) sizes are hints: keep them to what a window
        // can be. GTK aborts on anything below -1.
        let defaults = crate::settings::Settings::default();
        let (min_width, min_height) = appcore::metrics::window::MIN_SIZE;
        let (min_width, min_height) = (min_width as i32, min_height as i32);
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
            .default_width(fit(settings.width, min_width, defaults.width))
            .default_height(fit(settings.height, min_height, defaults.height))
            .width_request(min_width)
            .height_request(min_height)
            .css_classes(["gmnb"])
            .build();
        hub.attach(&win);

        let backdrop = Backdrop::default();
        backdrop.set_hub(&hub);
        apply_backdrop(
            &backdrop,
            crate::settings::backdrop_alpha(settings.background_opacity),
        );
        let toasts = adw::ToastOverlay::new();
        let ctx = Rc::new(Ctx {
            hub: hub.clone(),
            backdrop: backdrop.clone(),
            toasts: toasts.clone(),
            store: store.clone(),
            compact: Default::default(),
            flyouts: Default::default(),
            layers: crate::inert::Layers::new(&win),
            precision: crate::pages::Followed::new(settings.literal_digits),
        });

        // Header, DGMNB's: the menu button and the page's title at the
        // start; at the end the window's own controls (the toolkit's are
        // off), and the page's buttons before them. The rest of it drags
        // the window, and a double click maximizes it (GtkWindowHandle).
        use appcore::metrics::{controls as ctl, window as wm};
        let header = adw::HeaderBar::new();
        header.set_show_title(false);
        header.set_show_start_title_buttons(false);
        header.set_show_end_title_buttons(false);
        let menu = gtk::ToggleButton::builder()
            .child(&PathIcon::new(paths::MENU, 18))
            .tooltip_text("Open Navigation")
            .css_classes(["flat", "wc-icon-button"])
            .margin_start(wm::HEADER_PAD_X as i32)
            .valign(gtk::Align::Center)
            .build();
        menu.update_property(&[gtk::accessible::Property::Label("Open Navigation")]);
        // The header's two ends, each one box of the app's own (the
        // toolkit puts its own spacing between what it is given).
        let start = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        header.pack_start(&start);
        let end = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        header.pack_end(&end);
        start.append(&menu);
        follow_icon(&hub, &menu, "header.button");
        // Compact (keep on top): the way back takes the menu's place.
        let back = icon_button(paths::BACK_TO_FULL, "Back to full view (Alt+Down)");
        back.set_margin_start(wm::HEADER_PAD_X as i32);
        back.set_valign(gtk::Align::Center);
        back.set_visible(false);
        follow_icon(&hub, &back, "header.button");
        start.append(&back);
        let title = gtk::Label::new(None);
        title.add_css_class("wc-title");
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let gap = wm::HEADER_BUTTON_STEP - ctl::ICON_BUTTON + wm::HEADER_TITLE_GAP;
        title.set_margin_start(gap as i32);
        // DGMNB's room for it, whatever the window's width: in the smallest
        // window it runs under the buttons, as DGMNB's, rather than widen it.
        let title_room = crate::widgets::width_bin::Strip::new();
        title_room.append(&title, false, gap + wm::HEADER_TITLE_W, 0.0);
        start.append(&title_room);
        let header_end = gtk::Box::new(gtk::Orientation::Horizontal, wm::HEADER_BUTTON_GAP as i32);
        header_end.set_margin_end((wm::HEADER_BUTTON_GAP + wm::HEADER_CONTROLS_GAP) as i32);
        header_end.set_valign(gtk::Align::Center);
        end.append(&header_end);
        let controls = WindowControls::new(&win, &hub);
        controls.root.set_margin_end(wm::HEADER_PAD_X as i32);
        controls.root.set_valign(gtk::Align::Center);
        end.append(&controls.root);

        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        // `motion.page` ms (none in the base look).
        hub.subscribe_style(&stack, |stack, style| {
            stack.set_transition_duration(style.number("motion.page").round() as u32);
        });

        let content = adw::ToolbarView::new();
        content.add_top_bar(&header);
        content.set_content(Some(&stack));

        // Navigation sidebar, DGMNB's drawer: a header as the window's
        // (the menu button and the app's name), the modes in their groups,
        // and Settings at the foot.
        use appcore::metrics::nav as mnav;
        let nav = gtk::ListBox::new();
        nav.add_css_class("wc-nav");
        nav.set_selection_mode(gtk::SelectionMode::Single);
        nav.set_margin_top(mnav::LIST_PAD as i32);
        nav.set_margin_bottom(mnav::LIST_PAD as i32);
        let nav_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let nav_scroll = scroller(&hub, &nav);
        let nav_header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        nav_header.set_height_request(wm::HEADER_H as i32);
        nav_header.set_margin_start(wm::HEADER_PAD_X as i32);
        let close_nav = icon_button(paths::MENU, "Close Navigation");
        close_nav.set_valign(gtk::Align::Center);
        nav_header.append(&close_nav);
        let nav_title = gtk::Label::new(Some(crate::APP_NAME));
        nav_title.add_css_class("wc-nav-title");
        nav_title.set_xalign(0.0);
        nav_title.set_margin_start((mnav::TITLE_X - ctl::ICON_BUTTON) as i32);
        nav_header.append(&nav_title);
        nav_box.append(&nav_header);
        nav_box.append(&nav_scroll);
        let settings_btn = gtk::Button::builder()
            .css_classes(["flat", "wc-nav-settings"])
            .build();
        let (content_box, settings_icon) = nav_row_content(paths::SETTINGS, "Settings");
        settings_btn.set_child(Some(&content_box));
        crate::a11y::name_button(&settings_btn, "Settings");
        nav_box.append(&settings_btn);
        nav_box.add_css_class("wc-sidebar");
        follow_icon(&hub, &settings_icon, "nav.settings");
        follow_icon(&hub, &close_nav, "icon_button");
        {
            let title = nav_title.downgrade();
            hub.subscribe_style(&nav_box, move |_, style| {
                if let Some(t) = title.upgrade() {
                    t.set_visible(style.flag("nav.show_title"));
                }
            });
        }

        // The drawer is `nav.width` wide, and leaves at least 40 px of the
        // window uncovered (DGMNB's); the split view's width follows the
        // window's (`Window::fit_nav`).
        let split = adw::OverlaySplitView::builder()
            .collapsed(true)
            .sidebar(&nav_box)
            .content(&content)
            .min_sidebar_width(0.0)
            .max_sidebar_width(f64::from(mnav::WIDTH))
            .sidebar_width_fraction(1.0)
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
        {
            // The drawer is `nav.width` wide but leaves `MIN_UNCOVERED` of
            // the window beside it.
            let sized = crate::widgets::width_bin::WidthBin::new(&split);
            let fit = {
                let split = split.downgrade();
                let hub = hub.clone();
                move |width: i32| {
                    if let Some(split) = split.upgrade() {
                        let most = hub.style().number("nav.width");
                        // (Before the first allocation the width is unknown: 0.)
                        let room = if width > 0 {
                            width as f32 - mnav::MIN_UNCOVERED
                        } else {
                            most
                        };
                        split.set_max_sidebar_width(f64::from(most.min(room).max(0.0)));
                    }
                }
            };
            let last = Rc::new(Cell::new(0));
            {
                let last = last.clone();
                let fit = fit.clone();
                sized.connect_width(move |w| {
                    last.set(w);
                    fit(w);
                });
            }
            hub.subscribe_style(&sized, move |_, _| fit(last.get()));
            toasts.set_child(Some(&sized));
        }
        // The window gets its content in `present`.
        backdrop.set_child(Some(&toasts));
        {
            // A theme that fails: the base look (or its last style), and
            // a toast saying so.
            let toasts = toasts.downgrade();
            hub.set_tell(move |message| {
                if let Some(toasts) = toasts.upgrade() {
                    let toast = adw::Toast::new(message);
                    // The theme's name and its errors are its own words:
                    // text, not markup (a toast reads markup by default).
                    toast.set_use_markup(false);
                    toast.set_timeout(4);
                    toasts.add_toast(toast);
                }
            });
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
        exact_text(&nav_box);
        exact_text(&start);
        {
            // Compact overlay (upstream "Keep on top"): minimal chrome, small
            // window. Wayland has no client-side always-on-top; compositors
            // can pin it (e.g. a niri window rule on the app id).
            // As DGMNB's: the way back at the start, Close alone at the
            // end.
            let weak = Rc::downgrade(&this);
            let menu = menu.clone();
            let back_ = back.clone();
            let saved = Cell::new((0, 0));
            *this.ctx.compact.borrow_mut() = Some(Box::new(move |on| {
                let Some(w) = weak.upgrade() else { return };
                menu.set_visible(!on);
                back_.set_visible(on);
                w.title.set_visible(!on);
                w.header_end.set_visible(!on);
                controls.set_compact(on);
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
            let weak = Rc::downgrade(&this);
            back.connect_clicked(move |_| {
                if let Some(w) = weak.upgrade() {
                    w.handle_key(&KeyPress::named(Named::Down).alt());
                }
            });
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
            self.win.set_content(Some(&self.ctx.backdrop));
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
                // In mixed case, as DGMNB's: `nav.uppercase_headers` shows
                // it in capitals without changing what is read out.
                let header = gtk::Label::new(Some(match mode.group() {
                    Group::Calculator => "Calculator",
                    Group::Converter => "Converter",
                }));
                header.add_css_class("wc-nav-header");
                header.set_xalign(0.0);
                let hr = gtk::ListBoxRow::builder()
                    .child(&header)
                    .activatable(false)
                    .selectable(false)
                    .height_request(appcore::metrics::nav::HEADER_H as i32)
                    .build();
                self.ctx.hub.subscribe_style(&header, |header, style| {
                    let attrs = gtk::pango::AttrList::new();
                    if style.flag("nav.uppercase_headers") {
                        attrs.insert(gtk::pango::AttrInt::new_text_transform(
                            gtk::pango::TextTransform::Uppercase,
                        ));
                    }
                    header.set_attributes(Some(&attrs));
                });
                // Named as upstream's NavCategoryGroup ("Calculators
                // category").
                hr.update_property(&[gtk::accessible::Property::Label(match mode.group() {
                    Group::Calculator => "Calculators category",
                    Group::Converter => "Converters category",
                })]);
                hr.add_css_class("wc-nav-section");
                self.nav.append(&hr);
            }
            let (content, icon) = nav_row_content(mode.icon(), mode.title());
            let row = gtk::ListBoxRow::builder().child(&content).build();
            row.add_css_class("wc-nav-row");
            follow_icon(&self.ctx.hub, &icon, "nav.row");
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
        // The theme's layers hear of every change after the first page.
        if self.page_generation.get() > 1 {
            self.ctx.hub.layers().mode_changed(&page.widget());
        }

        while let Some(c) = self.header_end.first_child() {
            self.header_end.remove(&c);
        }
        for w in page.header_end() {
            self.header_end.append(&w);
        }

        for (m, row, icon) in self.nav_rows.borrow().iter() {
            if *m == mode && !row.is_selected() {
                self.nav.select_row(Some(row));
                // The base look draws nothing in (`motion.icon_draw` 0).
                if self.ctx.hub.style().number("motion.icon_draw") > 0.0 {
                    icon.animate_draw();
                }
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
        self.win.add_controller(self.key_router(false));
        hide_ring_on_press(&self.win);
        // A page's flyouts (autohide popovers) hold a grab: their keys and
        // presses never reach the window's controllers, so they get their
        // own, which do the same.
        let weak = Rc::downgrade(self);
        *self.ctx.flyouts.borrow_mut() = Some(Box::new(move |flyout| {
            if let Some(w) = weak.upgrade() {
                flyout.add_controller(w.key_router(true));
            }
            hide_ring_on_press(flyout);
        }));
    }

    /// The keys' way through the window, before any widget has them
    /// (capture), as [`step`] says: a text field's, a focused control's
    /// own, else the app's and the page's (handle_key); Enter stops at a
    /// calculator key, which ignores it. In a flyout (`flyout`) the same,
    /// but for Escape, which closes it: Enter on a key there is "=", as
    /// upstream's flyout keys (CalculatorButtons) leave it to the
    /// calculator, and what is typed goes into it.
    fn key_router(self: &Rc<Self>, flyout: bool) -> gtk::EventControllerKey {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _code, mods| {
            let (Some(w), Some(kp)) = (weak.upgrade(), key_press(key, mods)) else {
                return glib::Propagation::Proceed;
            };
            // Let text entries (graph equations, dialogs) type normally, but
            // still honour the app-wide chords a text field has no use for;
            // and let a focused control have its own keys.
            let focus = gtk::prelude::GtkWindowExt::focus(&w.win);
            let focused = focus.as_ref().map(|f| w.focused(f));
            let handled = match step(focused, &kp, flyout) {
                // The focused widget's own: GTK's, but for what GTK has
                // no binding for (own_key).
                own @ (Step::Widget | Step::Activate) => {
                    focus.as_ref().is_some_and(|f| own_key(f, &kp, own))
                }
                // What the key would change may be covered: handle_key asks.
                Step::Page => w.handle_key(&kp),
                Step::PageOnly => {
                    w.handle_key(&kp);
                    true
                }
            };
            if handled {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        keys
    }

    /// What `focus`, the window's focus, is ([`Focus`]).
    fn focused(&self, focus: &gtk::Widget) -> Focus {
        focus_kind(focus, |f| {
            self.shown_page().is_some_and(|p| p.is_calculator_key(f))
        })
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
        use Focus::*;
        let at = |f, k: &KeyPress| step(Some(f), k, false);
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
            assert_eq!(at(Control, k), Step::Widget, "{k:?}");
        }
        for k in [&seven, &escape, &ctrl_h, &alt_up, &alt_2, &space.alt()] {
            assert_eq!(at(Control, k), Step::Page, "{k:?}");
        }

        assert_eq!(at(CalculatorKey, &enter), Step::PageOnly);
        for k in [&space, &down, &page_down] {
            assert_eq!(at(CalculatorKey, k), Step::Widget, "{k:?}");
        }
        assert_eq!(at(CalculatorKey, &seven), Step::Page);

        for k in [&enter, &space, &down, &seven, &escape] {
            assert_eq!(at(Other, k), Step::Page, "{k:?}");
            assert_eq!(step(None, k, false), Step::Page, "{k:?}");
            assert_eq!(at(Text, k), Step::Widget, "{k:?}");
        }
        assert_eq!(at(Text, &alt_2), Step::Page);
        assert_eq!(at(Text, &KeyPress::named(Named::Home).ctrl()), Step::Page);
    }

    /// R18-L-02: Enter and Space with something held are the focused
    /// control's as the bare keys are, as upstream's controls read the key
    /// alone: Shift, Ctrl, and for Enter Alt too. GTK binds the bare keys
    /// only, so the window activates the control itself. Alt+Space is the
    /// window's menu, nobody's here. Shift+Return on the pressed 2nd
    /// unpresses it and evaluates nothing (it gave 5).
    #[test]
    fn enter_and_space_with_something_held_activate_as_bare() {
        use Focus::*;
        let enter = KeyPress::named(Named::Enter);
        let space = KeyPress::char(' ');
        for flyout in [false, true] {
            let at = |f, k: &KeyPress| step(Some(f), k, flyout);
            for key in [enter, space] {
                for k in [key.shift(), key.ctrl(), key.ctrl().shift()] {
                    assert_eq!(at(Control, &k), Step::Activate, "{k:?}");
                    assert_eq!(at(Other, &k), Step::Page, "{k:?}");
                }
            }
            for k in [enter.alt(), enter.alt().shift(), enter.alt().ctrl()] {
                assert_eq!(at(Control, &k), Step::Activate, "{k:?}");
                assert_eq!(at(Text, &k), Step::Activate, "{k:?}");
                assert_eq!(at(CalculatorKey, &k), Step::PageOnly, "{k:?}");
            }
            for k in [space.alt(), space.alt().shift(), space.alt().ctrl()] {
                assert_eq!(at(Control, &k), Step::Page, "{k:?}");
                assert_eq!(at(CalculatorKey, &k), Step::Page, "{k:?}");
            }
            // A calculator key: Space presses it whatever is held; a text
            // field: Enter submits it whatever is held, Space is typed.
            for k in [space.shift(), space.ctrl(), space.ctrl().shift()] {
                assert_eq!(at(CalculatorKey, &k), Step::Activate, "{k:?}");
                assert_eq!(at(Text, &k), Step::Widget, "{k:?}");
            }
            for k in [enter.shift(), enter.ctrl(), enter.ctrl().shift()] {
                assert_eq!(at(Text, &k), Step::Activate, "{k:?}");
            }
            assert_eq!(at(Text, &enter), Step::Widget);
            assert_eq!(at(Control, &enter), Step::Widget);
        }
    }

    /// R17-M-04 follow-up: Enter the page doesn't take (the converter's,
    /// the graph's keypad) still never reaches a focused calculator key,
    /// which ignores it as upstream's `CalculatorButton` does; GTK would
    /// press it. Whatever is held with it (R18-L-02): upstream's reads the
    /// key alone. Every other key the page leaves goes on as before.
    #[test]
    fn a_calculator_key_ignores_enter() {
        use Focus::*;
        let at = |f, k: &KeyPress| step(Some(f), k, false);
        let enter = KeyPress::named(Named::Enter);
        for k in [
            enter,
            enter.shift(),
            enter.ctrl(),
            enter.ctrl().shift(),
            enter.alt(),
        ] {
            assert_eq!(at(CalculatorKey, &k), Step::PageOnly, "{k:?}");
        }
        for k in [
            &KeyPress::char(' '),
            &KeyPress::named(Named::Down),
            &KeyPress::char('7'),
        ] {
            assert_ne!(at(CalculatorKey, k), Step::PageOnly, "{k:?}");
        }
        for f in [Text, Control, Other] {
            assert_ne!(at(f, &enter), Step::PageOnly, "{f:?}");
        }
    }

    /// A slider's navigation keys are its own whatever is held with them,
    /// as upstream's Slider reads the key alone and DGMNB's sliders take
    /// them (`input::Focus::Slider`): Ctrl+Right steps as Right does. GTK
    /// binds only the bare keys so (none with Shift or Alt, a page with
    /// Ctrl): the window makes the bare key's move for the others, before
    /// the page has the key (Ctrl+Home is the slider's Home).
    #[test]
    fn a_sliders_keys_move_it_whatever_is_held() {
        use gtk::ScrollType as S;
        for (key, bare) in [
            (Named::Left, S::StepLeft),
            (Named::Right, S::StepRight),
            (Named::Up, S::StepUp),
            (Named::Down, S::StepDown),
            (Named::PageUp, S::PageUp),
            (Named::PageDown, S::PageDown),
            (Named::Home, S::Start),
            (Named::End, S::End),
        ] {
            let k = KeyPress::named(key);
            assert_eq!(slide(&k), None, "{k:?}: GTK's own");
            for held in [
                k.shift(),
                k.ctrl(),
                k.ctrl().shift(),
                k.alt(),
                k.alt().shift(),
            ] {
                assert_eq!(slide(&held), Some(bare), "{held:?}");
                // The window has it before the page: it makes the move.
                assert_eq!(step(Some(Focus::Slider), &held, false), Step::Widget);
            }
            assert_eq!(step(Some(Focus::Slider), &k, false), Step::Widget);
            // Another control's are the page's with Ctrl or Alt.
            assert_eq!(step(Some(Focus::Control), &k.ctrl(), false), Step::Page);
        }
        for k in [
            KeyPress::char('7').shift(),
            KeyPress::char(' ').shift(),
            KeyPress::named(Named::Enter).ctrl(),
            KeyPress::named(Named::Tab).ctrl(),
            KeyPress::named(Named::Escape).shift(),
        ] {
            assert_eq!(slide(&k), None, "{k:?}");
            assert_eq!(step(Some(Focus::Slider), &k, false), Step::Page, "{k:?}");
        }
    }

    /// Escape closes whatever is held with it (R18-L-02's audit): GTK
    /// closes a popover on Escape with any modifier, so a flyout's Escape
    /// is the flyout's with any, and the sidebar closes the same way (as
    /// DGMNB's overlays do). Only the calculator's Clear is the bare
    /// Escape's alone (`input::shortcut`).
    #[test]
    fn escape_closes_whatever_is_held() {
        let sidebar = Cover {
            nav: false,
            page: true,
            target: true,
            sidebar: true,
        };
        let escape = KeyPress::named(Named::Escape);
        for k in [
            escape,
            escape.shift(),
            escape.ctrl(),
            escape.ctrl().shift(),
            escape.alt(),
        ] {
            assert_eq!(route(&k, sidebar), Route::CloseSidebar, "{k:?}");
            for f in [Focus::CalculatorKey, Focus::Control, Focus::Text] {
                assert_eq!(step(Some(f), &k, true), Step::Widget, "{f:?} {k:?}");
            }
        }
    }

    /// In an open flyout the keys go the window's way, as upstream's
    /// shortcuts reach the calculator with a flyout open: Enter on a
    /// focused key there is "=" (the key ignores it), what is typed is the
    /// calculator's; the 2nd and hyp toggles (and Bit shift's radio
    /// buttons) take their own Enter, Space and arrows. Escape, though, is
    /// the flyout's, which closes it; in the window it is the page's.
    #[test]
    fn keys_in_a_flyout_go_the_windows_way() {
        use Focus::*;
        let enter = KeyPress::named(Named::Enter);
        let escape = KeyPress::named(Named::Escape);
        let seven = KeyPress::char('7');
        let space = KeyPress::char(' ');
        for flyout in [false, true] {
            let at = |f, k: &KeyPress| step(f, k, flyout);
            assert_eq!(at(Some(CalculatorKey), &enter), Step::PageOnly);
            assert_eq!(at(Some(CalculatorKey), &seven), Step::Page);
            assert_eq!(at(Some(CalculatorKey), &space), Step::Widget);
            assert_eq!(at(Some(Control), &enter), Step::Widget);
            assert_eq!(at(Some(Control), &seven), Step::Page);
            assert_eq!(at(None, &enter), Step::Page);
        }
        for f in [Some(CalculatorKey), Some(Control), Some(Other), None] {
            assert_eq!(step(f, &escape, true), Step::Widget, "{f:?}");
            assert_eq!(step(f, &escape, false), Step::Page, "{f:?}");
        }
    }
}
