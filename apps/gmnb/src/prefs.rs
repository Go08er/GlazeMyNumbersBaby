//! Settings (upstream Settings.xaml: theme + about), plus the theme, its
//! options, its sounds and the window's opacity (docs/theming.md, "Where
//! Settings puts things"; docs/theming-media.md, "Settings").

use std::rc::Rc;

use adw::prelude::*;

use std::cell::RefCell;

use gmnb_theme::OptionValue;
use gmnb_theme::layer::PREVIEW_SIZE;
use gmnb_theme::theme::{OptionDef, OptionKind};

use crate::layers::tile::Tile;
use crate::paced::{self, Ask};
use crate::pages::Ctx;
use crate::settings::Persist;
use crate::theme::{parse_hex, rgba, to_hex};
use crate::window::{Window, apply_backdrop, apply_theme_setting};

/// Shown beside *Open themes folder*: installing a theme is running
/// someone's script.
const THEMES_ARE_CODE: &str = "Themes are code: each is a script that runs inside GMNB. It is \
     sandboxed and limited, and can only change how GMNB looks, but install only themes you trust.";

/// The style class of the list's row for a chosen theme that isn't
/// installed.
const NOT_INSTALLED: &str = "wc-not-installed";

pub fn show(win: &Rc<Window>) {
    let ctx = win.ctx().clone();
    let dialog = adw::PreferencesDialog::new();
    dialog.set_title("Settings");
    dialog.add_css_class("wc-prefs");

    let page = adw::PreferencesPage::new();
    page.set_title("Appearance");

    // Mode.
    let group = adw::PreferencesGroup::builder().title("Mode").build();
    let theme = adw::ToggleGroup::new();
    // In DGMNB's order (its Settings page's "Style").
    for (name, label) in [("system", "System"), ("light", "Light"), ("dark", "Dark")] {
        // Never cut short: a label of its own that doesn't ellipsize (the
        // toggle's would, to "S…", and the row's text took the room).
        let shown = gtk::Label::new(Some(label));
        theme.add(
            adw::Toggle::builder()
                .name(name)
                .label(label)
                .child(&shown)
                .build(),
        );
    }
    theme.set_active_name(Some(&ctx.store.data.borrow().theme));
    // Its parts equal, as DGMNB's.
    theme.set_homogeneous(true);
    theme.set_valign(gtk::Align::Center);
    let row = adw::ActionRow::builder()
        .title("Light or dark")
        .subtitle("Follows the system unless you choose otherwise")
        .build();
    row.add_suffix(&theme);
    // (GMNB_FOCUS=wc-light-dark: where the row is, for the tests.)
    row.add_css_class("wc-light-dark");
    group.add(&row);
    // Settings in the least window: one breakpoint for all of it, added to
    // the dialog once its setters are all in (below). libadwaita applies
    // only the last added of those whose condition holds: the sliders'
    // breakpoint, added after this one with the same condition, undid it
    // (the row's subtitle back, the toggles even, wider than the window
    // with a wide system font; round 6's CI run).
    let narrow = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 330sp").expect("a condition"),
    );
    // There the three toggles and the row's text don't fit side by side:
    // the toggles take only the room their labels need, and the row says
    // only its title (the toggles say the rest).
    narrow.add_setter(&theme, "homogeneous", Some(&false.to_value()));
    narrow.add_setter(&row, "subtitle", Some(&"".to_value()));
    {
        let ctx = ctx.clone();
        theme.connect_active_name_notify(move |t| {
            if ctx.layers.covers(t) {
                // Covered, Settings changes nothing (R15-M-02).
                let saved = ctx.store.data.borrow().theme.clone();
                if t.active_name().as_deref() != Some(saved.as_str()) {
                    t.set_active_name(Some(&saved));
                }
                return;
            }
            let name = t
                .active_name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "system".into());
            apply_theme_setting(&name);
            ctx.store.data.borrow_mut().theme = name;
            ctx.store.persist();
        });
    }
    let accent = adw::SwitchRow::builder()
        .title("Use the desktop's accent colour")
        .subtitle("The colour your desktop or shell publishes; off, GMNB uses its own blue")
        .active(ctx.store.data.borrow().system_accent)
        .build();
    {
        let ctx = ctx.clone();
        accent.connect_active_notify(move |r| {
            let saved = ctx.store.data.borrow().system_accent;
            if ctx.layers.covers(r) {
                // Covered, Settings changes nothing (R15-M-02).
                if r.is_active() != saved {
                    r.set_active(saved);
                }
                return;
            }
            ctx.hub.set_system_accent(r.is_active());
            ctx.store.data.borrow_mut().system_accent = r.is_active();
            ctx.store.persist();
        });
    }
    group.add(&accent);
    page.add(&group);

    // Theme, and the chosen theme's options.
    let themes = ThemeSettings::new(&ctx, &win.widget(), &dialog);
    page.add(&themes.group);
    page.add(&themes.options);
    // What the hub tells while Settings is open is told in Settings: the
    // window's toasts are under it (and hidden by it, narrow).
    {
        let dialog = dialog.downgrade();
        ctx.hub.set_front_tell(Some(Box::new(move |message: &str| {
            if let Some(dialog) = dialog.upgrade() {
                let toast = adw::Toast::new(message);
                // A theme's name is its own words: text, not markup.
                toast.set_use_markup(false);
                toast.set_timeout(4);
                dialog.add_toast(toast);
            }
        })));
    }
    // What became of the themes folder while Settings was closed (nothing
    // watches it then): a chosen theme whose folder went gives way to the
    // base look now, and one that came back is loaded.
    ctx.hub.sync_installed();
    themes.rebuild_list();
    themes.rebuild_options();
    // Themes dragged into the folder show up while Settings is open.
    themes.watch();
    // Sound, under the theme they belong to.
    page.add(&sound_group(&ctx));

    {
        // Window.
        let group = adw::PreferencesGroup::builder().title("Window").build();
        let percent = |v: f64| format!("{}%", (v * 100.0).round());
        let saved = f64::from(crate::settings::backdrop_alpha(
            ctx.store.data.borrow().background_opacity,
        ));
        let scale = gtk::Scale::with_range(
            gtk::Orientation::Horizontal,
            crate::settings::MIN_BACKGROUND_OPACITY,
            1.0,
            0.05,
        );
        scale.set_value(saved);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.set_valign(gtk::Align::Center);
        scale.set_width_request(160);
        // (GMNB_STEPS' `slide:` finds it by this.)
        scale.add_css_class("wc-opacity-slider");
        // The range reports its value as a fraction (0.1); say it as shown.
        scale.update_property(&[
            gtk::accessible::Property::Label("Background opacity"),
            gtk::accessible::Property::ValueText(&percent(saved)),
        ]);
        let shown = gtk::Label::new(Some(&percent(saved)));
        shown.add_css_class("numeric");
        shown.set_width_chars(4);
        shown.set_xalign(1.0);
        let opacity = adw::ActionRow::builder()
            .title("Background opacity")
            .subtitle("Lower lets your desktop show through, frosted if your compositor blurs translucent windows")
            .build();
        opacity.add_suffix(&scale);
        opacity.add_suffix(&shown);
        {
            // The window follows the slider once a frame, at the value it
            // has then: the toolkit sends a step for every move it is told
            // of, and each step calls the theme's `style`
            // (docs/theming.md, "When a theme runs").
            let follow = {
                let ctx = ctx.clone();
                let pending = Rc::default();
                paced::OnceAFrame::new(&scale, move |v: f64| {
                    let alpha = crate::settings::backdrop_alpha(v);
                    apply_backdrop(&ctx.backdrop, alpha);
                    // One step of a drag: only the properties that change
                    // with the opacity are rewritten (the live provider).
                    ctx.hub.set_opacity(alpha, true);
                    ctx.store.data.borrow_mut().background_opacity = v;
                    persist_at_rest(&ctx, &pending);
                })
            };
            let (ctx, shown) = (ctx.clone(), shown.clone());
            scale.connect_value_changed(move |s| {
                if ctx.layers.covers(s) {
                    // Covered, Settings changes nothing (R15-M-02): what
                    // was moved before it was covered stands.
                    follow.flush();
                    let saved = ctx.store.data.borrow().background_opacity;
                    let saved = f64::from(crate::settings::backdrop_alpha(saved));
                    if s.value() != saved {
                        s.set_value(saved);
                    }
                    return;
                }
                let v = s.value();
                shown.set_text(&percent(v));
                s.update_property(&[gtk::accessible::Property::ValueText(&percent(v))]);
                follow.set(s, v);
            });
        }
        group.add(&opacity);
        let from_env = crate::RENDERER_FROM_ENV.get().copied().unwrap_or(false);
        let vulkan = adw::SwitchRow::builder()
            .title("Vulkan acceleration")
            .subtitle(if from_env {
                "GSK_RENDERER is set where GMNB was started, and decides instead"
            } else {
                "Off draws in software: far less memory, plainer motion. Takes effect the next time GMNB starts"
            })
            .active(ctx.store.data.borrow().vulkan)
            .build();
        {
            let ctx = ctx.clone();
            vulkan.connect_active_notify(move |r| {
                let saved = ctx.store.data.borrow().vulkan;
                if ctx.layers.covers(r) {
                    // Covered, Settings changes nothing (R15-M-02).
                    if r.is_active() != saved {
                        r.set_active(saved);
                    }
                    return;
                }
                ctx.store.data.borrow_mut().vulkan = r.is_active();
                ctx.store.persist();
            });
        }
        group.add(&vulkan);
        page.add(&group);

        page.add(&graphing_group(&ctx));
    }

    // About.
    let group = adw::PreferencesGroup::builder().title("About").build();
    let about_row = adw::ButtonRow::builder()
        .title(format!("About {}", crate::APP_NAME))
        .end_icon_name("go-next-symbolic")
        .build();
    {
        let (win, ctx) = (win.widget(), ctx.clone());
        about_row.connect_activated(move |_| about(&win, &ctx.hub.credits()));
    }
    group.add(&about_row);
    page.add(&group);

    dialog.add(&page);
    {
        // In the least window the sliders' rows (their slider asks for 160
        // px) left their text some 40 px: "Vo-lu-me", a subtitle four letters
        // wide (R5-C-3). The sliders ask for less there; the same
        // breakpoint as the toggles' above.
        for class in ["wc-sound-volume-slider", "wc-opacity-slider"] {
            if let Some(scale) = find_by_class(page.upcast_ref(), class) {
                narrow.add_setter(&scale, "width-request", Some(&NARROW_SLIDER.to_value()));
            }
        }
        dialog.add_breakpoint(narrow);
    }
    // The theme rows' handlers live as long as the dialog does; once it
    // has closed, nothing more is asked of the theme for it, the watch on
    // the themes folder ends, and the hub tells the window again.
    dialog.connect_closed(move |_| {
        themes.stop();
        themes.unwatch();
        themes.ctx.hub.set_front_tell(None);
    });
    // A dialog's keys never reach the window's controllers.
    crate::window::own_keys(&dialog);
    dialog.present(Some(&win.widget()));
    fit_least_window(&dialog);
}

/// How wide the sliders of Settings' rows ask to be in the least window
/// (160 px elsewhere).
const NARROW_SLIDER: i32 = 80;

/// The first widget under `root` with the style class `class`.
fn find_by_class(root: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
    if root.has_css_class(class) {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(c) = child {
        if let Some(found) = find_by_class(&c, class) {
            return Some(found);
        }
        child = c.next_sibling();
    }
    None
}

/// What libadwaita's bottom sheet, which Settings is shown in when the
/// window is narrow, puts around a dialog: 6 px each side (measured, 1.9).
const SHEET_EDGES: i32 = 12;

/// libadwaita's preferences dialog asks for 360 px however narrow its
/// page could be (the width request of the breakpoint bin in its
/// template). In a bottom sheet that is 372 px, more than a new window
/// (360 px) or the least one (300 px) has: the sheet was cut off at the
/// window's edge and its rows squeezed. Settings asks for what the least
/// window leaves instead, and its rows for no more than that; wider, it
/// is laid out as before.
fn fit_least_window(dialog: &adw::PreferencesDialog) {
    let least = appcore::metrics::window::MIN_SIZE.0 as i32 - SHEET_EDGES;
    // As high as it asked to be (150 px in libadwaita 1.9).
    let mut height = -1;
    let mut todo = vec![dialog.clone().upcast::<gtk::Widget>()];
    while let Some(w) = todo.pop() {
        if w.is::<adw::BreakpointBin>() && w.width_request() > least {
            w.set_width_request(least);
            height = height.max(w.height_request());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            todo.push(c);
        }
    }
    // A dialog with breakpoints of its own (`show`) needs a least size.
    dialog.set_size_request(least, height.max(150));
}

/// Settings' Theme group and the chosen theme's options under it.
struct ThemeSettings {
    ctx: Rc<Ctx>,
    window: gtk::glib::WeakRef<adw::ApplicationWindow>,
    /// Settings itself: what its own toasts are shown in.
    dialog: gtk::glib::WeakRef<adw::PreferencesDialog>,
    group: adw::PreferencesGroup,
    list: gtk::ListBox,
    /// *Restore built-in themes*: shown while a built-in theme has no
    /// folder.
    restore: gtk::Button,
    /// The watch on the themes folder, while Settings is open.
    watch: RefCell<Option<Rc<crate::watch::Watch>>>,
    /// What the theme's author should know: its error and its notes.
    report: gtk::Label,
    /// Holds one group per `group` of the theme's options.
    options: adw::PreferencesGroup,
    options_box: gtk::Box,
    /// Each option row and its option, to show or hide as `visible` says.
    rows: RefCell<Vec<(gtk::Widget, OptionDef)>>,
    /// Each tile a `preview` draws, with its option and its choice: they
    /// are drawn again when `ctx` changes.
    tiles: RefCell<Vec<(Tile, OptionDef, String)>>,
    /// The `ctx` the tiles are drawn for: each is drawn once for each.
    tiles_ctx: RefCell<Option<gmnb_theme::Ctx>>,
    /// The calls into the theme still to make (`visible`, `preview`), in
    /// passes that never hold the window.
    turns: Rc<paced::Turns>,
    /// The theme's load the tiles' options are of ([`Hub::epoch`]): after
    /// another, they are the options of a theme that isn't loaded, and
    /// are built again, not drawn.
    tiles_epoch: std::cell::Cell<u64>,
    /// Set while the list or the options are being built: their widgets'
    /// signals then change nothing.
    building: std::cell::Cell<bool>,
    /// Settings has closed.
    closed: std::cell::Cell<bool>,
    pending: Rc<RefCell<Option<gtk::glib::SourceId>>>,
}

impl ThemeSettings {
    fn new(
        ctx: &Rc<Ctx>,
        window: &adw::ApplicationWindow,
        dialog: &adw::PreferencesDialog,
    ) -> Rc<Self> {
        let group = adw::PreferencesGroup::builder()
            .title("Theme")
            .description(THEMES_ARE_CODE)
            .build();
        // (GMNB_STEPS' `themes` reads the list by its class.)
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list", "wc-theme-list"])
            .build();
        group.add(&list);
        let report = gtk::Label::builder()
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .selectable(true)
            .margin_top(10)
            .visible(false)
            .css_classes(["caption", "dim-label"])
            .build();
        group.add(&report);
        // The buttons wrap to a second line rather than widen Settings (three
        // while Restore shows).
        let buttons = adw::WrapBox::builder()
            .child_spacing(8)
            .line_spacing(8)
            .margin_top(12)
            .build();
        let reload = gtk::Button::builder()
            .label("Reload theme")
            .tooltip_text("Run the theme's file again, after you have edited it")
            .build();
        // (GMNB_STEPS' `press` finds the buttons by their classes.)
        let open = gtk::Button::builder()
            .label("Open themes folder")
            .tooltip_text(format!(
                "Every theme is a folder here: drag new ones in. {}",
                crate::theme::user_dir().display()
            ))
            .css_classes(["wc-open-themes"])
            .build();
        let restore = gtk::Button::builder()
            .label("Restore built-in themes")
            .tooltip_text("Put back the themes GMNB comes with whose folders were deleted")
            .css_classes(["wc-restore-themes"])
            .visible(false)
            .build();
        buttons.append(&reload);
        buttons.append(&open);
        buttons.append(&restore);
        group.add(&buttons);

        let options = adw::PreferencesGroup::new();
        let options_box = gtk::Box::new(gtk::Orientation::Vertical, 24);
        options.add(&options_box);

        let this = Rc::new(ThemeSettings {
            ctx: ctx.clone(),
            window: window.downgrade(),
            dialog: dialog.downgrade(),
            group,
            list,
            restore: restore.clone(),
            watch: RefCell::default(),
            report,
            options,
            options_box,
            rows: RefCell::default(),
            tiles: RefCell::default(),
            tiles_ctx: RefCell::default(),
            turns: paced::Turns::new(),
            tiles_epoch: std::cell::Cell::new(0),
            building: std::cell::Cell::new(false),
            closed: std::cell::Cell::new(false),
            pending: Rc::default(),
        });
        {
            // The desktop's accent, light and dark: the tiles follow. One
            // subscription for them all (one a grid drew every grid's
            // tiles again, for each grid).
            let weak = Rc::downgrade(&this);
            this.ctx
                .hub
                .subscribe_style(&this.options_box, move |_, _| {
                    if let Some(t) = weak.upgrade() {
                        t.draw_tiles();
                    }
                });
        }
        {
            let weak = Rc::downgrade(&this);
            reload.connect_clicked(move |b| {
                let Some(t) = weak.upgrade() else { return };
                if t.ctx.layers.covers(b) {
                    return;
                }
                t.ctx.hub.reload();
                t.rebuild_list();
                t.rebuild_options();
            });
        }
        {
            let weak = Rc::downgrade(&this);
            open.connect_clicked(move |b| {
                let Some(t) = weak.upgrade() else { return };
                if t.ctx.layers.covers(b) {
                    return;
                }
                t.open_folder();
            });
        }
        {
            let weak = Rc::downgrade(&this);
            restore.connect_clicked(move |b| {
                let Some(t) = weak.upgrade() else { return };
                if t.ctx.layers.covers(b) {
                    return;
                }
                let failed = crate::theme::restore_built_ins(&t.ctx.store, &t.ctx.hub);
                if let Some(first) = failed.first() {
                    t.toast(&format!("A built-in theme couldn't be restored: {first}"));
                }
                t.refresh();
            });
        }
        this
    }

    /// A toast in Settings (text, not markup: a path may hold `&`), and
    /// the same on stderr.
    fn toast(&self, text: &str) {
        eprintln!("gmnb: {text}");
        if let Some(dialog) = self.dialog.upgrade() {
            let toast = adw::Toast::new(text);
            toast.set_use_markup(false);
            toast.set_timeout(10);
            dialog.add_toast(toast);
        }
    }

    /// *Open themes folder*: the folder in the app the desktop opens
    /// folders with, so that themes can be dragged into it.
    ///
    /// - Outside a sandbox: the default handler for folders
    ///   (`inode/directory`) that the user's `mimeapps.list` names, asked
    ///   of GIO itself. (GTK's own launcher, as of 4.22, hands the folder to
    ///   the desktop portal whenever the session bus has one, sandboxed or
    ///   not, and the portal looks the handler up in its own environment,
    ///   or asks.)
    /// - In the Flatpak (`/.flatpak-info`, as GTK tells): through GTK's
    ///   launcher, which hands the portal's OpenURI.OpenFile the folder; the
    ///   portal opens a folder with the host's default handler for folders
    ///   without asking. (Its OpenDirectory would show the folder inside the
    ///   one that holds it, through FileManager1, whichever file manager
    ///   owns that.)
    ///
    /// If nothing opens it, a toast says why and where the folder is.
    fn open_folder(self: &Rc<Self>) {
        use gtk::gio;
        let dir = crate::theme::user_dir();
        let failed = {
            let (weak, dir) = (Rc::downgrade(self), dir.display().to_string());
            move |why: &str| {
                if let Some(t) = weak.upgrade() {
                    t.toast(&format!(
                        "Couldn't open the themes folder: {why}. It is {dir}"
                    ));
                }
            }
        };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            failed(&e.to_string());
            return;
        }
        let file = gio::File::for_path(&dir);
        let window = self.window.upgrade();
        if std::path::Path::new("/.flatpak-info").exists() {
            gtk::FileLauncher::new(Some(&file)).launch(
                window.as_ref(),
                gio::Cancellable::NONE,
                move |opened| match opened {
                    Err(e) if !e.matches(gtk::DialogError::Dismissed) => failed(e.message()),
                    _ => {}
                },
            );
            return;
        }
        let Some(app) = gio::AppInfo::default_for_type("inode/directory", false) else {
            failed("no app is set to open folders");
            return;
        };
        let launch = window
            .map(|w| WidgetExt::display(&w))
            .or_else(gtk::gdk::Display::default)
            .map(|display| display.app_launch_context());
        let name = app.display_name().to_string();
        app.launch_uris_async(
            &[file.uri().as_str()],
            launch.as_ref(),
            gio::Cancellable::NONE,
            move |opened| {
                if let Err(e) = opened {
                    failed(&format!("{name} didn't start ({})", e.message()));
                }
            },
        );
    }

    /// Watches the themes folder while Settings is open: what is dragged
    /// in, deleted or changed there shows in the list.
    fn watch(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        let hub = Rc::downgrade(&self.ctx.hub);
        let watch = crate::watch::Watch::start(
            &crate::theme::user_dir(),
            move || hub.upgrade().map(|h| h.active()).unwrap_or_default(),
            move || {
                if let Some(t) = weak.upgrade() {
                    t.refresh();
                }
            },
        );
        self.watch.replace(Some(watch));
    }

    /// Settings closed: nothing watches the themes folder any more.
    fn unwatch(&self) {
        if let Some(watch) = self.watch.take() {
            watch.stop();
        }
    }

    /// The themes folder changed: the list as it is now; and a chosen
    /// theme whose folder went gives way to the base look (with a toast),
    /// one that came back is loaded, and the options follow.
    fn refresh(self: &Rc<Self>) {
        let changed = self.ctx.hub.sync_installed();
        self.rebuild_list();
        if changed {
            self.rebuild_options();
        }
    }

    /// The installed themes, the base look first: one row each, the
    /// chosen one marked, and those GMNB comes with marked so; and
    /// *Restore built-in themes* while one of those has no folder.
    fn rebuild_list(self: &Rc<Self>) {
        self.building.set(true);
        while let Some(row) = self.list.first_child() {
            self.list.remove(&row);
        }
        let active = self.ctx.hub.active();
        let mut first: Option<gtk::CheckButton> = None;
        let mut listed = self.ctx.hub.list();
        // A chosen theme that isn't installed stays the choice: show it.
        let missing = !listed.iter().any(|l| l.id == active);
        if missing {
            listed.push(crate::theme::Listed {
                id: active.clone(),
                name: active.clone(),
                description: String::new(),
                comes_with: false,
                error: Some("Not installed".into()),
            });
        }
        for l in listed {
            let check = gtk::CheckButton::builder()
                .valign(gtk::Align::Center)
                .active(l.id == active)
                .build();
            match &first {
                Some(first) => check.set_group(Some(first)),
                None => first = Some(check.clone()),
            }
            let subtitle = match &l.error {
                Some(e) => format!("Doesn't load: {e}"),
                None => l.description.clone(),
            };
            let row = adw::ActionRow::builder()
                .title(gtk::glib::markup_escape_text(&l.name))
                .subtitle(gtk::glib::markup_escape_text(&subtitle))
                .subtitle_lines(3)
                .activatable_widget(&check)
                .build();
            row.add_prefix(&check);
            // (GMNB_STEPS' `themes` reads each row's theme by its name.)
            row.set_widget_name(if l.id.is_empty() { "base" } else { &l.id });
            if missing && l.id == active {
                row.add_css_class(NOT_INSTALLED);
            }
            if l.comes_with {
                // On two lines: narrow, it would take the description's
                // room. (GMNB_STEPS' `themes` finds it by its class.)
                let mark = gtk::Label::new(Some("Comes with\nGMNB"));
                mark.add_css_class("caption");
                mark.add_css_class("dim-label");
                mark.add_css_class("wc-comes-with");
                mark.set_justify(gtk::Justification::Right);
                mark.set_xalign(1.0);
                mark.set_tooltip_text(Some(
                    "GMNB put this theme's folder in the themes folder. Delete the folder to \
                     remove it; Restore built-in themes brings it back",
                ));
                row.add_suffix(&mark);
            }
            check.update_property(&[gtk::accessible::Property::Label(&l.name)]);
            let weak = Rc::downgrade(self);
            let id = l.id.clone();
            check.connect_toggled(move |c| {
                let Some(t) = weak.upgrade() else { return };
                if t.building.get() || !c.is_active() {
                    return;
                }
                if t.ctx.layers.covers(c) {
                    // Covered, Settings changes nothing (R15-M-02).
                    t.rebuild_list();
                    return;
                }
                if t.ctx.hub.active() != id {
                    t.ctx.hub.set_theme(&id);
                    t.ctx.store.data.borrow_mut().active_theme = id.clone();
                    t.ctx.store.persist();
                    t.rebuild_options();
                    // The chosen theme's folder is watched wherever it
                    // falls, from now, not at the folder's next change.
                    if let Some(watch) = t.watch.borrow().as_ref() {
                        watch.sync();
                    }
                    t.drop_uninstalled_rows();
                }
            });
            self.list.append(&row);
        }
        self.restore.set_visible(self.ctx.hub.built_ins_missing());
        self.building.set(false);
        self.sync_report();
    }

    /// A theme that was chosen but isn't installed is listed so that it
    /// can be seen to be the choice ([`Self::rebuild_list`], which marks
    /// its row [`NOT_INSTALLED`]); once another is chosen its row goes
    /// (R5-C-1: it stayed, "Doesn't load: Not installed", until the folder
    /// next changed). The others' rows stay as they are, so that the
    /// keyboard's focus stays where it is, and the themes aren't listed
    /// (run) again.
    fn drop_uninstalled_rows(&self) {
        let active = self.ctx.hub.active();
        let active = if active.is_empty() { "base" } else { &active };
        let mut gone = Vec::new();
        let mut row = self.list.first_child();
        while let Some(r) = row {
            row = r.next_sibling();
            if r.has_css_class(NOT_INSTALLED) && r.widget_name().as_str() != active {
                gone.push(r);
            }
        }
        for r in gone {
            self.list.remove(&r);
        }
    }

    /// The theme's error and its notes (the latest 20), under the list.
    fn sync_report(&self) {
        let hub = &self.ctx.hub;
        let mut lines = Vec::new();
        if let Some(e) = hub.error() {
            lines.push(format!("The base look is showing: {e}"));
        }
        lines.extend(hub.notes().iter().map(|n| n.to_string()));
        self.report.set_text(&lines.join("\n"));
        self.report.set_visible(!lines.is_empty());
    }

    /// One option changed: tell the theme, save it, and show or hide the
    /// options as their `visible` now says.
    fn changed(self: &Rc<Self>, id: &str, value: OptionValue, drag: bool) {
        let theme = self.ctx.hub.active();
        // Whether each row is shown is asked again, before the option is
        // set: a style that changes with it asks for the tiles, and the
        // rows go first in that same pass, not in one of their own.
        self.want_visible();
        self.ctx.hub.set_option(id, value.clone(), drag);
        self.ctx
            .store
            .data
            .borrow_mut()
            .set_option(&theme, id, &value);
        if drag {
            persist_at_rest(&self.ctx, &self.pending);
        } else {
            self.ctx.store.persist();
        }
        self.want_tiles();
        self.ask();
        self.sync_report();
    }

    /// Has the theme draw every tile again (still pictures, for the
    /// `ctx` there is now), once for each `ctx`; not while the options
    /// are being built or changed in a batch (they are drawn after).
    fn draw_tiles(self: &Rc<Self>) {
        if self.building.get() {
            return;
        }
        self.want_tiles();
        self.ask();
    }

    /// Every tile, to be drawn for the `ctx` there is now, if it hasn't
    /// been asked for that one. Each keeps the picture it has until then.
    fn want_tiles(&self) {
        if self.building.get() || self.tiles_epoch.get() != self.ctx.hub.epoch() {
            return;
        }
        let ctx = self.ctx.hub.frame_ctx();
        if self.tiles_ctx.borrow().as_ref() == Some(&ctx) {
            return;
        }
        for n in 0..self.tiles.borrow().len() {
            self.turns.ask(Ask::Tile(n));
        }
        self.tiles_ctx.replace(Some(ctx));
    }

    /// Every row with a `visible`, to be asked again.
    fn want_visible(&self) {
        for (n, (_, option)) in self.rows.borrow().iter().enumerate() {
            if option.visible.is_some() {
                self.turns.ask(Ask::Visible(n));
            }
        }
    }

    /// Asks the theme what is waiting: as much as one pass's budget allows
    /// now, the rest in passes to come while Settings is open.
    fn ask(self: &Rc<Self>) {
        // Closed, Settings asks nothing (a change of style after it closed
        // still reaches its subscriber until the dialog has gone).
        if self.closed.get() {
            return;
        }
        let weak = Rc::downgrade(self);
        self.turns.go(Rc::new(move |ask| {
            if let Some(t) = weak.upgrade() {
                t.answer(ask);
            }
        }));
    }

    /// One call into the theme, and its answer shown.
    fn answer(&self, ask: Ask) {
        // Asked of a theme that has been replaced since: its rows are gone.
        if self.tiles_epoch.get() != self.ctx.hub.epoch() {
            return;
        }
        match ask {
            Ask::Visible(n) => {
                let row = self.rows.borrow().get(n).cloned();
                if let Some((row, option)) = row {
                    row.set_visible(self.ctx.hub.option_visible(&option));
                }
            }
            Ask::Tile(n) => {
                let tile = self.tiles.borrow().get(n).cloned();
                if let Some((tile, option, choice)) = tile {
                    tile.set_list(self.ctx.hub.preview(&option, &choice));
                }
            }
        }
    }

    /// Settings has closed: nothing more is asked.
    fn stop(&self) {
        self.closed.set(true);
        self.turns.stop();
    }

    /// The chosen theme's options, rendered from its option list: one
    /// group per `group`, in the order given.
    fn rebuild_options(self: &Rc<Self>) {
        self.building.set(true);
        while let Some(child) = self.options_box.first_child() {
            self.options_box.remove(&child);
        }
        self.rows.borrow_mut().clear();
        self.tiles.borrow_mut().clear();
        // What was asked of the rows that were is forgotten.
        self.turns.stop();
        self.tiles_ctx.replace(None);
        self.tiles_epoch.set(self.ctx.hub.epoch());
        let defs = self.ctx.hub.option_defs();
        let values = self.ctx.hub.option_values();
        self.options.set_visible(!defs.is_empty());
        let mut groups: Vec<(Option<String>, adw::PreferencesGroup)> = Vec::new();
        for def in defs {
            let at = groups.iter().position(|(name, _)| *name == def.group);
            let group = match at {
                Some(at) => groups[at].1.clone(),
                None => {
                    let title = def.group.clone().unwrap_or_else(|| "Theme options".into());
                    // A group's title is markup; the theme's is text.
                    let title = gtk::glib::markup_escape_text(&title);
                    let group = adw::PreferencesGroup::builder().title(title).build();
                    self.options_box.append(&group);
                    groups.push((def.group.clone(), group.clone()));
                    group
                }
            };
            let row = self.option_row(&def, values.get(&def.id));
            group.add(&row);
            self.rows.borrow_mut().push((row, def));
        }
        self.building.set(false);
        // Whether each row is shown, then each tile's picture: the first
        // pass now, so a theme whose hooks are quick shows them all at
        // once; until its picture comes, a tile shows its place.
        self.want_visible();
        self.want_tiles();
        self.ask();
        self.sync_report();
    }

    /// The row for one option, by its type.
    fn option_row(self: &Rc<Self>, def: &OptionDef, value: Option<&OptionValue>) -> gtk::Widget {
        let title = gtk::glib::markup_escape_text(&def.label);
        let subtitle = gtk::glib::markup_escape_text(def.description.as_deref().unwrap_or(""));
        let weak = Rc::downgrade(self);
        let id = def.id.clone();
        // Whether a control's change is the user's to apply: not while the
        // rows are being built, and not while Settings is covered
        // (R15-M-02: the caller then puts the control back).
        let row: gtk::Widget = match &def.kind {
            OptionKind::Note => {
                let label = gtk::Label::builder()
                    .label(&def.label)
                    .wrap(true)
                    .wrap_mode(gtk::pango::WrapMode::WordChar)
                    .xalign(0.0)
                    .margin_top(8)
                    .css_classes(["caption", "dim-label"])
                    .build();
                return label.upcast();
            }
            OptionKind::Toggle => {
                let on = matches!(value, Some(OptionValue::Bool(true)));
                let row = adw::SwitchRow::builder()
                    .title(title)
                    .subtitle(subtitle)
                    .active(on)
                    .build();
                row.connect_active_notify(move |r| {
                    let Some(t) = weak.upgrade() else { return };
                    if t.building.get() {
                        return;
                    }
                    let saved = matches!(
                        t.ctx.hub.option_values().get(&id),
                        Some(OptionValue::Bool(true))
                    );
                    if t.ctx.layers.covers(r) {
                        if r.is_active() != saved {
                            r.set_active(saved);
                        }
                        return;
                    }
                    if r.is_active() != saved {
                        t.changed(&id, OptionValue::Bool(r.is_active()), false);
                    }
                });
                row.upcast()
            }
            // With a preview a choice is a grid of tiles the theme draws,
            // each captioned and named by its label, one selected.
            OptionKind::Choice {
                choices,
                preview: Some(_),
            } => {
                let ids: Vec<String> = choices.iter().map(|c| c.id.clone()).collect();
                let flow = gtk::FlowBox::builder()
                    .selection_mode(gtk::SelectionMode::Single)
                    .max_children_per_line(5)
                    // Two in the least window, where three don't fit;
                    // three or more wherever they do.
                    .min_children_per_line(2)
                    .homogeneous(true)
                    .row_spacing(10)
                    .column_spacing(10)
                    .css_classes(["wc-palettes"])
                    .build();
                flow.update_property(&[gtk::accessible::Property::Label(&def.label)]);
                let (w, h) = PREVIEW_SIZE;
                for choice in choices {
                    // Drawn in the passes that follow (`want_tiles`).
                    let tile = Tile::new(w, h);
                    tile.set_halign(gtk::Align::Center);
                    self.tiles
                        .borrow_mut()
                        .push((tile.clone(), def.clone(), choice.id.clone()));
                    let label = gtk::Label::new(Some(&choice.label));
                    label.add_css_class("caption");
                    // A theme's word of 80 letters is wider than Settings:
                    // broken across lines.
                    label.set_wrap(true);
                    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                    label.set_max_width_chars(12);
                    label.set_justify(gtk::Justification::Center);
                    let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
                    b.append(&tile);
                    b.append(&label);
                    // (GMNB_FOCUS=wc-tile: Settings scrolled to the tiles.)
                    let child = gtk::FlowBoxChild::builder()
                        .child(&b)
                        .css_classes(["wc-tile"])
                        .build();
                    child.update_property(&[gtk::accessible::Property::Label(&choice.label)]);
                    flow.append(&child);
                    if matches!(value, Some(OptionValue::Text(v)) if *v == choice.id) {
                        flow.select_child(&child);
                    }
                }
                flow.connect_selected_children_changed(move |f| {
                    let Some(t) = weak.upgrade() else { return };
                    if t.building.get() {
                        return;
                    }
                    // GTK carries out an assistive technology's Selection
                    // request whether or not the grid is covered
                    // (crate::inert), and a single-selection grid can be
                    // left with none. Covered, or left empty, it shows the
                    // saved choice again and nothing is applied (R15-M-02).
                    let saved = match t.ctx.hub.option_values().get(&id) {
                        Some(OptionValue::Text(v)) => ids.iter().position(|c| c == v),
                        _ => None,
                    };
                    let chosen = f
                        .selected_children()
                        .first()
                        .map(|c| c.index() as usize)
                        .filter(|at| *at < ids.len());
                    let allowed = chosen.filter(|at| Some(*at) == saved || !t.ctx.layers.covers(f));
                    let Some(at) = allowed else {
                        if let Some(child) = saved.and_then(|i| f.child_at_index(i as i32)) {
                            f.select_child(&child);
                        }
                        return;
                    };
                    if saved != Some(at) {
                        t.changed(&id, OptionValue::Text(ids[at].clone()), false);
                    }
                });
                // (The tiles follow `ctx`: `ThemeSettings::new`.)
                let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
                if let Some(description) = def.description.as_deref().filter(|d| !d.is_empty()) {
                    column.append(
                        &gtk::Label::builder()
                            .label(description)
                            .wrap(true)
                            .wrap_mode(gtk::pango::WrapMode::WordChar)
                            .xalign(0.0)
                            .css_classes(["caption", "dim-label"])
                            .build(),
                    );
                }
                column.append(&flow);
                column.upcast()
            }
            OptionKind::Choice { choices, .. } => {
                let labels: Vec<&str> = choices.iter().map(|c| c.label.as_str()).collect();
                let ids: Vec<String> = choices.iter().map(|c| c.id.clone()).collect();
                let selected = match value {
                    Some(OptionValue::Text(v)) => ids.iter().position(|c| c == v).unwrap_or(0),
                    _ => 0,
                };
                let row = adw::ComboRow::builder()
                    .title(title)
                    .subtitle(subtitle)
                    .model(&gtk::StringList::new(&labels))
                    .selected(selected as u32)
                    .build();
                row.connect_selected_notify(move |r| {
                    let Some(t) = weak.upgrade() else { return };
                    if t.building.get() {
                        return;
                    }
                    let saved = match t.ctx.hub.option_values().get(&id) {
                        Some(OptionValue::Text(v)) => ids.iter().position(|c| c == v),
                        _ => None,
                    };
                    let chosen = r.selected() as usize;
                    if t.ctx.layers.covers(r) {
                        if let Some(saved) = saved.filter(|s| *s != chosen) {
                            r.set_selected(saved as u32);
                        }
                        return;
                    }
                    if let Some(choice) = ids.get(chosen).filter(|_| saved != Some(chosen)) {
                        t.changed(&id, OptionValue::Text(choice.clone()), false);
                    }
                });
                row.upcast()
            }
            OptionKind::Color => {
                let colour = match value {
                    Some(OptionValue::Text(v)) => parse_hex(v),
                    _ => None,
                }
                .unwrap_or([0.0; 3]);
                let dialog = gtk::ColorDialog::builder()
                    .with_alpha(false)
                    .title("Pick a colour")
                    .build();
                let button = gtk::ColorDialogButton::new(Some(dialog));
                button.set_rgba(&rgba(colour, 1.0));
                button.set_valign(gtk::Align::Center);
                // GTK names neither the colour button nor the button inside
                // it (that takes focus): only the colour it shows is read.
                let label = [gtk::accessible::Property::Label(&def.label)];
                button.update_property(&label);
                let mut child = button.first_child();
                while let Some(c) = child {
                    if c.is::<gtk::Button>() {
                        c.update_property(&label);
                    }
                    child = c.next_sibling();
                }
                // No cover check here, unlike the other controls: a picked
                // colour comes from the chooser, which covers Settings
                // until it has closed. Covered, the button itself refuses.
                button.connect_rgba_notify(move |b| {
                    let Some(t) = weak.upgrade() else { return };
                    if t.building.get() {
                        return;
                    }
                    let c = b.rgba();
                    let hex = to_hex([c.red(), c.green(), c.blue()]);
                    // A colour being dragged in the chooser: a step of many.
                    t.changed(&id, OptionValue::Text(hex), true);
                });
                let row = adw::ActionRow::builder()
                    .title(title)
                    .subtitle(subtitle)
                    .build();
                row.add_suffix(&button);
                row.upcast()
            }
            OptionKind::Slider {
                min,
                max,
                step,
                percent,
            } => {
                let (min, max, percent) = (*min, *max, *percent);
                let step = if *step > 0.0 {
                    *step
                } else {
                    (max - min) / 100.0
                };
                let now = match value {
                    Some(OptionValue::Number(n)) => *n,
                    _ => min,
                };
                let show = move |v: f64| {
                    if percent {
                        format!("{}%", (v * 100.0).round())
                    } else {
                        let digits = if step.fract() == 0.0 { 0 } else { 2 };
                        format!("{v:.digits$}")
                    }
                };
                let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, step);
                // (GMNB_STEPS' `slide:` finds it by this.)
                scale.add_css_class("wc-option-slider");
                scale.set_value(now);
                scale.set_draw_value(false);
                scale.set_hexpand(true);
                scale.set_valign(gtk::Align::Center);
                scale.set_width_request(160);
                scale.update_property(&[
                    gtk::accessible::Property::Label(&def.label),
                    gtk::accessible::Property::ValueText(&show(now)),
                ]);
                let shown = gtk::Label::new(Some(&show(now)));
                shown.add_css_class("numeric");
                shown.set_width_chars(5);
                // A range of 1e300 shows 300 digits: never wider than a
                // row can be.
                shown.set_max_width_chars(12);
                shown.set_ellipsize(gtk::pango::EllipsizeMode::End);
                shown.set_xalign(1.0);
                {
                    let shown = shown.clone();
                    scale.connect_value_changed(move |s| {
                        let Some(t) = weak.upgrade() else { return };
                        if t.building.get() {
                            return;
                        }
                        let saved = match t.ctx.hub.option_values().get(&id) {
                            Some(OptionValue::Number(n)) => *n,
                            _ => min,
                        };
                        if t.ctx.layers.covers(s) {
                            if s.value() != saved {
                                s.set_value(saved);
                            }
                            return;
                        }
                        let v = s.value();
                        shown.set_text(&show(v));
                        s.update_property(&[gtk::accessible::Property::ValueText(&show(v))]);
                        if v != saved {
                            t.changed(&id, OptionValue::Number(v), true);
                        }
                    });
                }
                let row = adw::ActionRow::builder()
                    .title(title)
                    .subtitle(subtitle)
                    .build();
                row.add_suffix(&scale);
                row.add_suffix(&shown);
                row.upcast()
            }
        };
        // Small buttons at the end of the row: each runs a function of the
        // theme's that returns the options to change.
        if let Some(action_row) = row.downcast_ref::<adw::ActionRow>() {
            for action in &def.actions {
                let button = gtk::Button::builder()
                    .label(&action.label)
                    .valign(gtk::Align::Center)
                    .css_classes(["flat", "wc-option-action"])
                    .build();
                // The button's own label, cut short rather than wider than
                // Settings (the tooltip and the name keep the whole).
                if let Some(text) = button.child().and_downcast::<gtk::Label>() {
                    text.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    text.set_max_width_chars(14);
                }
                button.update_property(&[gtk::accessible::Property::Label(&action.label)]);
                if let Some(tip) = &action.tooltip {
                    button.set_tooltip_text(Some(tip));
                }
                let (weak, action) = (Rc::downgrade(self), action.clone());
                button.connect_clicked(move |b| {
                    let Some(t) = weak.upgrade() else { return };
                    if t.ctx.layers.covers(b) {
                        return;
                    }
                    let theme = t.ctx.hub.active();
                    // A batch: the tiles are drawn once, as the options
                    // are built again below, not for each option set.
                    t.building.set(true);
                    // One style pass for all that it changes (R3-E-3: an action
                    // that changed 24 options ran the theme's 50 ms `style` 24 times).
                    let changes: Vec<_> = t.ctx.hub.run_action(&action).into_iter().collect();
                    for (id, value) in &changes {
                        t.ctx.store.data.borrow_mut().set_option(&theme, id, value);
                    }
                    t.ctx.hub.set_options(changes, false);
                    t.building.set(false);
                    t.ctx.store.persist();
                    // The rows show the new values.
                    let t2 = t.clone();
                    gtk::glib::idle_add_local_once(move || t2.rebuild_options());
                });
                action_row.add_suffix(&button);
            }
        }
        row
    }
}

/// Sound: the theme's sounds on or off, and how loud (docs/theming-media.md,
/// "Settings"). What the switch says under it follows the theme, the
/// desktop's own setting and the sound system.
fn sound_group(ctx: &Rc<Ctx>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title("Sound").build();
    let on = ctx.store.data.borrow().sound;
    let switch = adw::SwitchRow::builder()
        .title("Theme sounds")
        .active(on)
        .build();
    ctx.hub.subscribe_sound(&switch, |row, text| {
        row.set_subtitle(&gtk::glib::markup_escape_text(text))
    });
    // (tests/settings.rs finds the switch and the slider by these.)
    switch.add_css_class("wc-sound-switch");
    group.add(&switch);

    let percent = |v: f64| format!("{}%", (v * 100.0).round());
    let saved = f64::from(crate::settings::sound_volume(
        ctx.store.data.borrow().sound_volume,
    ));
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.05);
    scale.set_value(saved);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.set_valign(gtk::Align::Center);
    scale.set_width_request(160);
    scale.add_css_class("wc-sound-volume-slider");
    scale.update_property(&[
        gtk::accessible::Property::Label("Volume"),
        gtk::accessible::Property::ValueText(&percent(saved)),
    ]);
    let shown = gtk::Label::new(Some(&percent(saved)));
    shown.add_css_class("numeric");
    shown.set_width_chars(4);
    shown.set_xalign(1.0);
    let volume = adw::ActionRow::builder()
        .title("Volume")
        .subtitle("Let go, it plays the theme's key sound")
        .build();
    volume.add_suffix(&scale);
    volume.add_suffix(&shown);
    volume.set_sensitive(on);
    // (GMNB_FOCUS=wc-sound-volume: Settings scrolled to these rows.)
    volume.add_css_class("wc-sound-volume");
    group.add(&volume);
    {
        let (ctx, volume) = (ctx.clone(), volume.clone());
        switch.connect_active_notify(move |r| {
            let saved = ctx.store.data.borrow().sound;
            if ctx.layers.covers(r) {
                // Covered, Settings changes nothing (R15-M-02).
                if r.is_active() != saved {
                    r.set_active(saved);
                }
                return;
            }
            let on = r.is_active();
            volume.set_sensitive(on);
            if on != saved {
                ctx.hub.set_sound(on);
                ctx.store.data.borrow_mut().sound = on;
                ctx.store.persist();
            }
        });
    }
    {
        let (ctx, shown) = (ctx.clone(), shown.clone());
        let pending = Rc::default();
        scale.connect_value_changed(move |s| {
            if ctx.layers.covers(s) {
                // Covered, Settings changes nothing (R15-M-02).
                let saved = ctx.store.data.borrow().sound_volume;
                let saved = f64::from(crate::settings::sound_volume(saved));
                if s.value() != saved {
                    s.set_value(saved);
                }
                return;
            }
            let v = s.value();
            shown.set_text(&percent(v));
            s.update_property(&[gtk::accessible::Property::ValueText(&percent(v))]);
            ctx.hub.set_sound_volume(v as f32);
            ctx.store.data.borrow_mut().sound_volume = v;
            persist_at_rest(&ctx, &pending);
        });
    }
    {
        // Let go (the pointer, or a key that moves it): the theme's key
        // sound, at the volume now. The range takes the pointer's presses
        // for itself, so its events are watched before it has them.
        let ctx = ctx.clone();
        let watch = gtk::EventControllerLegacy::new();
        watch.set_propagation_phase(gtk::PropagationPhase::Capture);
        watch.connect_event(move |c, event| {
            use gtk::gdk::{EventType, Key};
            let let_go = match event.event_type() {
                EventType::ButtonRelease | EventType::TouchEnd => true,
                EventType::KeyRelease => {
                    event.downcast_ref::<gtk::gdk::KeyEvent>().is_some_and(|k| {
                        matches!(
                            k.keyval(),
                            Key::Left
                                | Key::Right
                                | Key::Up
                                | Key::Down
                                | Key::KP_Left
                                | Key::KP_Right
                                | Key::KP_Up
                                | Key::KP_Down
                                | Key::Page_Up
                                | Key::Page_Down
                                | Key::Home
                                | Key::End
                                | Key::minus
                                | Key::plus
                                | Key::KP_Subtract
                                | Key::KP_Add
                        )
                    })
                }
                _ => false,
            };
            if let_go
                && let Some(w) = c.widget()
                && w.is_sensitive()
                && !ctx.layers.covers(&w)
            {
                ctx.hub.preview_sound();
            }
            gtk::glib::Propagation::Proceed
        });
        scale.add_controller(watch);
    }
    group
}

/// Saves `ctx`'s settings once a slider has rested for a moment, not on
/// every step of a drag; and folds what the drag changed of the style
/// back into the main provider (`Hub::settle`).
fn persist_at_rest(ctx: &Rc<Ctx>, pending: &Rc<RefCell<Option<gtk::glib::SourceId>>>) {
    if let Some(id) = pending.borrow_mut().take() {
        id.remove();
    }
    let (ctx, pending2) = (ctx.clone(), pending.clone());
    let id = gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
        pending2.borrow_mut().take();
        ctx.hub.settle();
        ctx.store.persist();
    });
    *pending.borrow_mut() = Some(id);
}

/// Graphing: "Number precision", a slider from 5 to 20 significant digits
/// and then Off, with the calculators that round so labelled. Moving it
/// re-plots and re-analyses at once (the graphing page follows
/// `Ctx::precision`).
fn graphing_group(ctx: &Rc<Ctx>) -> adw::PreferencesGroup {
    use appcore::graph::{NUMBER_PRECISION_HELP, NumberPrecision as P};
    let group = adw::PreferencesGroup::builder().title("Graphing").build();
    let now = ctx.precision.get();
    let title = adw::ActionRow::builder()
        .title("Number precision")
        .subtitle(NUMBER_PRECISION_HELP)
        .activatable(false)
        .build();
    let (first, last) = (*P::POSITIONS.start(), *P::POSITIONS.end());
    let scale = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        f64::from(first),
        f64::from(last),
        1.0,
    );
    scale.set_round_digits(0);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.set_value(f64::from(now.position()));
    // Page Up and Page Down move by five, as DGMNB's slider. (GTK's own
    // page, ten steps, stops at a mark on the way on a scale that has
    // marks: from 10, Page Up went to 12 and Page Down to 5.)
    scale.connect_move_slider(|s, scroll| {
        use gtk::ScrollType as S;
        let by = match scroll {
            S::PageUp | S::PageRight | S::PageForward => 5.0,
            S::PageDown | S::PageLeft | S::PageBackward => -5.0,
            _ => return,
        };
        s.stop_signal_emission_by_name("move-slider");
        s.set_value(s.value() + by);
    });
    // Labels alternate above and below: 14 and 15, 20 and Off, are
    // neighbours.
    let mut marks: Vec<(u8, &str, gtk::PositionType)> = P::NOTCHES
        .iter()
        .zip([
            gtk::PositionType::Bottom,
            gtk::PositionType::Top,
            gtk::PositionType::Bottom,
            gtk::PositionType::Top,
        ])
        .map(|(&(digits, short, _), side)| (digits, short, side))
        .collect();
    marks.push((first, "5", gtk::PositionType::Bottom));
    marks.push((P::MAX, "20", gtk::PositionType::Top));
    marks.push((last, "Off", gtk::PositionType::Bottom));
    for (position, label, side) in marks {
        let label = gtk::glib::markup_escape_text(label);
        scale.add_mark(f64::from(position), side, Some(&label));
    }
    let shown = gtk::Label::new(Some(&now.describe()));
    shown.add_css_class("caption");
    shown.add_css_class("numeric");
    scale.update_property(&[
        gtk::accessible::Property::Label("Number precision"),
        gtk::accessible::Property::ValueText(&now.describe()),
        gtk::accessible::Property::Description(NUMBER_PRECISION_HELP),
    ]);
    {
        let (ctx, shown) = (ctx.clone(), shown.clone());
        let pending = Rc::default();
        scale.connect_value_changed(move |s| {
            if ctx.layers.covers(s) {
                // Covered, Settings changes nothing (R15-M-02).
                let saved = f64::from(ctx.precision.get().position());
                if s.value() != saved {
                    s.set_value(saved);
                }
                return;
            }
            let p = P::at_position(s.value());
            // GTK rounds only what the pointer and the keys set: a value
            // between positions (an AT client's 20.4) moves to the
            // setting's own, so the value read back is the setting's, as
            // in DGMNB (R14-L-02). That comes back here, on a position.
            let position = f64::from(p.position());
            if s.value() != position {
                s.set_value(position);
                return;
            }
            shown.set_text(&p.describe());
            s.update_property(&[gtk::accessible::Property::ValueText(&p.describe())]);
            ctx.precision.set(p);
            ctx.store.data.borrow_mut().literal_digits = p;
            persist_at_rest(&ctx, &pending);
        });
    }
    let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
    column.set_margin_top(6);
    column.set_margin_bottom(10);
    column.set_margin_start(12);
    column.set_margin_end(12);
    column.append(&scale);
    column.append(&shown);
    let slider = gtk::ListBoxRow::builder()
        .child(&column)
        .activatable(false)
        .selectable(false)
        .build();
    slider.update_property(&[gtk::accessible::Property::Label("Number precision slider")]);
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    list.append(&title);
    list.append(&slider);
    group.add(&list);
    group
}

/// About. `credits`: what the installed themes credit (a title and a
/// text each), shown under Legal.
pub fn about(parent: &adw::ApplicationWindow, credits: &[(String, String)]) {
    let about = adw::AboutDialog::builder()
        .application_name(crate::APP_NAME)
        .application_icon(crate::APP_ID)
        .developer_name("GMNB contributors")
        .version(env!("CARGO_PKG_VERSION"))
        .license_type(gtk::License::MitX11)
        .comments(
            "GlazeMyNumbers,Baby — a Rust port of the open-source Windows Calculator: the \
             original arbitrary-precision engine and every mode, with considerably more shimmer \
             than strictly necessary.\n\n\
             Not affiliated with or endorsed by Microsoft.",
        )
        .website("https://github.com/Go08er/GlazeMyNumbersBaby")
        .issue_url("https://github.com/Go08er/GlazeMyNumbersBaby/issues")
        .copyright(
            "© Microsoft Corporation (original Calculator)\n© 2026 GMNB contributors (Rust port)",
        )
        .build();
    about.add_credit_section(
        Some("Based on"),
        &["Windows Calculator by Microsoft https://github.com/microsoft/calculator"],
    );
    about.add_legal_section(
        "Windows Calculator",
        Some("Copyright (c) Microsoft Corporation. All rights reserved."),
        gtk::License::MitX11,
        None,
    );
    // Custom licence text is Pango markup: escape the plain-text notices.
    for (title, copyright, text) in TYPEFACES {
        about.add_legal_section(
            title,
            copyright,
            gtk::License::Custom,
            Some(&gtk::glib::markup_escape_text(text)),
        );
    }
    for (title, text) in legal_credits(credits) {
        about.add_legal_section(
            &title,
            None,
            gtk::License::Custom,
            Some(&gtk::glib::markup_escape_text(&text)),
        );
    }
    about.add_legal_section(
        "CORE-MATH",
        Some("Correctly rounded maths functions, used by Graphing"),
        gtk::License::Custom,
        Some(&gtk::glib::markup_escape_text(CORE_MATH)),
    );
    about.add_legal_section("Rust crates", None, gtk::License::Custom, Some(CRATES));
    // The same, as a row: assistive technology can't follow a link in a
    // label (GTK offers it no action), but can activate a row.
    about.add_link("Third-party licences", THIRD_PARTY);
    about.add_legal_section(
        "Exchange rates",
        None,
        gtk::License::Custom,
        Some("Currency reference rates from central banks (European Central Bank and others), served by the Frankfurter API. Rates are informational and may lag the market."),
    );
    about.connect_activate_link(|about, uri| {
        let ours = uri == THIRD_PARTY;
        if ours {
            third_party_licences(about);
        }
        ours
    });
    crate::window::own_keys(&about);
    about.present(Some(parent));
}

/// The most of one theme credit About → Legal sets in its label (its
/// bytes): a credit is a licence, and 12 KiB is the Apache licence.
const LEGAL_CREDIT_MAX: usize = 12 << 10;
/// The most of all the themes' credits it sets (bytes), and so the most
/// that is laid out when Legal is shown: a label's layout takes time
/// and height by its size, and every installed theme adds its credits.
const LEGAL_CREDITS_MAX: usize = 192 << 10;
/// The longest run of characters without a place to wrap that a credit's
/// label keeps: a longer one is broken (a zero-width space), as a long
/// word with no break took seconds to lay out.
const LEGAL_RUN_MAX: usize = 60;

/// The themes' credits as About → Legal sets them (a title and a text
/// each): each credit's text shortened to [`LEGAL_CREDIT_MAX`], the lot
/// to [`LEGAL_CREDITS_MAX`] (what doesn't fit is left out, with a line
/// that says so), and long runs without a break broken, so that the
/// labels lay out in a moment whatever the themes hold.
fn legal_credits(credits: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut left = LEGAL_CREDITS_MAX;
    for (n, (title, text)) in credits.iter().enumerate() {
        if left == 0 {
            let more = credits.len() - n;
            out.push((
                "More credits".to_string(),
                format!("{more} more credits of installed themes are in their folders."),
            ));
            break;
        }
        let mut shown = String::with_capacity(text.len().min(left.min(LEGAL_CREDIT_MAX)) + 16);
        let mut run = 0;
        let mut cut = false;
        for c in text.chars() {
            if shown.len() + c.len_utf8() > left.min(LEGAL_CREDIT_MAX) {
                cut = true;
                break;
            }
            if c.is_whitespace() {
                run = 0;
            } else {
                run += 1;
                if run > LEGAL_RUN_MAX {
                    shown.push('\u{200b}');
                    run = 1;
                }
            }
            shown.push(c);
        }
        left = left.saturating_sub(shown.len());
        if cut {
            shown.push_str("\n\n… (the rest is in the theme's folder)");
        }
        out.push((title.clone(), shown));
    }
    out
}

const INTER: &str = include_str!("../../dgmnb/assets/fonts/OFL-Inter.txt");
const NOTO: &str = include_str!("../../dgmnb/assets/fonts/OFL-Noto.txt");
/// The typefaces GMNB bundles itself (`crate::INTER`, and the subsets it
/// falls back to, `crate::FALLBACKS`): each one's title, copyright and
/// licence in About → Legal. (A theme's are its own credits.)
const TYPEFACES: [(&str, Option<&str>, &str); 2] = [
    (
        "Inter typeface",
        Some("Copyright (c) 2016 The Inter Project Authors"),
        INTER,
    ),
    (
        "Noto Sans typefaces (Math, Arabic, Armenian, Bengali and Khmer subsets)",
        None,
        NOTO,
    ),
];
const CORE_MATH: &str = concat!(
    include_str!("../../../crates/crmath/vendor/COPYRIGHT"),
    "\n",
    include_str!("../../../crates/crmath/vendor/LICENSE"),
);
/// The About dialog's link to [`third_party_licences`].
const THIRD_PARTY: &str = "gmnb:third-party-licences";
const CRATES: &str = "GMNB is built from many Rust crates, each under its own licence (MIT, \
     Apache 2.0, BSD and others). <a href=\"gmnb:third-party-licences\">Show their \
     licences</a>; they are also installed with GMNB, as THIRD-PARTY-LICENSES.txt.";

/// THIRD-PARTY-LICENSES.txt (CORE-MATH, smithay-clipboard and every Rust
/// crate) in a text view, which lays out only what's on screen: as a
/// legal section's label, its 160 KB took over half a second per layout.
fn third_party_licences(parent: &impl IsA<gtk::Widget>) {
    let buffer = gtk::TextBuffer::new(None);
    buffer.set_text(include_str!("../../../THIRD-PARTY-LICENSES.txt"));
    let view = gtk::TextView::builder()
        .buffer(&buffer)
        .editable(false)
        .cursor_visible(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(12)
        .bottom_margin(12)
        .left_margin(12)
        .right_margin(12)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .child(&view)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&scroll));
    let dialog = adw::Dialog::builder()
        .title("Third-party licences")
        .content_width(640)
        .content_height(600)
        .child(&toolbar)
        .build();
    crate::window::own_keys(&dialog);
    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// About → Legal sets every installed theme's credits in labels: up
    /// to 8 files of 64 KiB each. One long word, without a break, took
    /// 3.3 s to lay out at 64 KiB (and 8 of them in a theme, in each
    /// theme); newlines made a label 1.5 million pixels high.
    #[test]
    fn theme_credits_are_laid_out_in_a_moment() {
        use pango::prelude::*;
        let hostile = [
            "x".repeat(64 << 10),
            "\n".repeat(64 << 10),
            "lorem ipsum ".repeat(6000),
            "\u{4e2d}".repeat(21_000),
        ];
        // Four themes of eight such credits each.
        let credits: Vec<(String, String)> = (0..32)
            .map(|n| (format!("C{n}"), hostile[n % 4].clone()))
            .collect();
        let shown = legal_credits(&credits);
        let bytes: usize = shown.iter().map(|(_, t)| t.len()).sum();
        assert!(bytes <= LEGAL_CREDITS_MAX + 4096, "{bytes} bytes");
        let context = pangocairo::FontMap::default().create_context();
        let start = std::time::Instant::now();
        let mut height = 0;
        for (_, text) in &shown {
            let layout = pango::Layout::new(&context);
            layout.set_width(400 * pango::SCALE);
            layout.set_wrap(pango::WrapMode::WordChar);
            layout.set_markup(&gtk::glib::markup_escape_text(text));
            height += layout.pixel_size().1;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "laying out took {:?}",
            start.elapsed()
        );
        assert!(height < 4_000_000, "{height} px high");
        // A credit within the limits is as it was written.
        let licence = "Copyright (c) A\n\nPermission is hereby granted.\n".repeat(50);
        let kept = legal_credits(&[("L".to_string(), licence.clone())]);
        assert_eq!(kept, [("L".to_string(), licence)]);
    }

    /// Legal sections' text is Pango markup, and text that doesn't parse
    /// shows as nothing (as a font licence's "PERMISSION & CONDITIONS"
    /// once did).
    #[test]
    fn legal_text_is_markup() {
        assert!(pango::parse_markup(INTER, '\0').is_err());
        for text in [INTER, CORE_MATH] {
            let escaped = gtk::glib::markup_escape_text(text);
            let (_, plain, _) = pango::parse_markup(&escaped, '\0').unwrap();
            assert_eq!(plain, text);
        }
        // GtkLabel takes the links out before Pango sees the rest.
        let link = format!("<a href=\"{THIRD_PARTY}\">");
        assert!(CRATES.contains(&link));
        let rest = CRATES.replace(&link, "").replace("</a>", "");
        let (_, plain, _) = pango::parse_markup(&rest, '\0').unwrap();
        assert!(plain.contains("Show their licences"));
        assert!(CORE_MATH.contains("Alexei Sibidanov") && CORE_MATH.contains("Permission is"));
    }

    /// Every credit a built-in theme carries (About → Legal shows it) is
    /// in every package: its file among GMNB's licences (Flatpak, Arch,
    /// Fedora, Nix), a stanza of debian/copyright for its folder, and the
    /// licence it names in each package's licence list (and the metainfo's).
    /// Swirls' (ly's ColorMix, under the WTFPL) was in none; later still
    /// missing from the Nix package and the metainfo.
    #[test]
    fn every_built_in_credit_is_in_every_package() {
        use gmnb_theme::theme::{BUILT_IN, Source, Theme};
        let flatpak =
            include_str!("../../../packaging/flatpak/io.github.Go08er.GlazeMyNumbersBaby.json");
        let arch = include_str!("../../../packaging/arch/PKGBUILD");
        let fedora = include_str!("../../../packaging/fedora/gmnb.spec");
        let debian = include_str!("../../../packaging/debian/copyright");
        let nix = include_str!("../../../nix/package.nix");
        let metainfo =
            include_str!("../../../packaging/io.github.Go08er.GlazeMyNumbersBaby.metainfo.xml");
        let metainfo_license = metainfo
            .split("<project_license>")
            .nth(1)
            .and_then(|s| s.split("</project_license>").next())
            .unwrap();
        // GMNB's own parts: Arch's package_gmnb, Fedora's main package.
        let arch_gmnb = arch.split("package_gmnb()").nth(1).unwrap();
        let arch_gmnb = arch_gmnb.split("package_dgmnb()").next().unwrap();
        let fedora_license = fedora.lines().find(|l| l.starts_with("License:")).unwrap();
        let fedora_files = fedora.split("%files\n").nth(1).unwrap();
        let fedora_files = fedora_files.split("%files -n").next().unwrap();
        // The licences a credit can name, by the words that name them.
        const LICENCES: [(&str, &str, &str); 2] = [
            ("SIL Open Font License", "OFL-1.1", "ofl"),
            ("WTFPL", "WTFPL", "wtfpl"),
        ];
        let limits = gmnb_theme::Limits {
            load: std::time::Duration::from_secs(20),
            ..Default::default()
        };
        let mut missing = Vec::new();
        for b in &BUILT_IN {
            let theme = Theme::load(Source::built_in(b.id).unwrap(), limits.clone()).unwrap();
            for (credit, (title, text)) in theme.def.credits.iter().zip(theme.credits()) {
                let path = format!("apps/gmnb/themes/{}/{}", b.id, credit.file);
                for (package, list) in [
                    ("Flatpak", flatpak),
                    ("Arch", arch_gmnb),
                    ("Fedora", fedora_files),
                    ("Nix", nix),
                ] {
                    if !list.contains(&path) {
                        missing.push(format!("{package} doesn't install {path} ({title})"));
                    }
                }
                let stanza = debian.split("\n\n").find(|stanza| {
                    stanza.lines().next().is_some_and(|first| {
                        first
                            .strip_prefix("Files: ")
                            .and_then(|glob| glob.strip_suffix('*'))
                            .is_some_and(|folder| !folder.is_empty() && path.starts_with(folder))
                    })
                });
                for (words, spdx, nix_name) in LICENCES.iter().filter(|(w, ..)| text.contains(w)) {
                    if !metainfo_license.split_whitespace().any(|w| w == *spdx) {
                        missing.push(format!("the metainfo's licence has no {spdx} ({title})"));
                    }
                    if !nix.contains(&format!("\n      {nix_name}\n")) {
                        missing.push(format!("Nix's meta.license has no {nix_name} ({title})"));
                    }
                    // (DEP-5: "License: MIT and WTFPL", and the text of
                    // each in a paragraph of its own.)
                    let names = |s: &str| {
                        s.lines()
                            .find_map(|l| l.strip_prefix("License: "))
                            .is_some_and(|l| l.split_whitespace().any(|w| w == *spdx))
                    };
                    if !stanza.is_some_and(names)
                        || !debian.contains(&format!("\nLicense: {spdx}\n "))
                    {
                        missing.push(format!("debian/copyright: {path} under {spdx} ({words})"));
                    }
                    if !fedora_license.contains(spdx) {
                        missing.push(format!("Fedora's License has no {spdx} ({title})"));
                    }
                    if !arch_gmnb.contains(&format!("'{spdx}'")) {
                        missing.push(format!("Arch's gmnb has no {spdx} ({title})"));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "{}", missing.join("\n"));
    }

    /// THIRD-PARTY-LICENSES.txt names the crates at the versions Cargo.lock
    /// has: it is generated from the lock (`packaging/licences/generate.sh`),
    /// and a dependency update that doesn't regenerate it leaves it naming
    /// versions that no longer ship (0.3's did: rustls 0.23.45, ureq 3.4.2).
    #[test]
    fn third_party_licences_name_the_locked_versions() {
        let lock = include_str!("../../../Cargo.lock");
        let licences = include_str!("../../../THIRD-PARTY-LICENSES.txt");
        let mut locked = std::collections::HashSet::new();
        let mut name = "";
        let mut version = "";
        for line in lock.lines() {
            if let Some(n) = line.strip_prefix("name = \"") {
                name = n.trim_end_matches('"');
            } else if let Some(v) = line.strip_prefix("version = \"") {
                version = v.trim_end_matches('"');
            } else if line.starts_with("source = ") {
                locked.insert(format!("{name} {version}"));
            }
        }
        assert!(locked.len() > 300, "{} locked crates?", locked.len());
        let mut stale = Vec::new();
        let mut listed = 0;
        for paragraph in licences.split("\n\n") {
            // (After the licence's title; a long list wraps onto lines
            // indented by two spaces.)
            let Some((_, used)) = paragraph.split_once("\nUsed by: ") else {
                continue;
            };
            for entry in used.split(',') {
                let entry = entry.split_whitespace().collect::<Vec<_>>().join(" ");
                listed += 1;
                if !locked.contains(&entry) {
                    stale.push(entry);
                }
            }
        }
        assert!(listed > 200, "{listed} crates listed?");
        assert!(stale.is_empty(), "not in Cargo.lock: {stale:?}");
    }

    /// Every typeface GMNB bundles has its licence in About → Legal:
    /// Inter, and the Noto Sans subsets it falls back to (0.3 bundles
    /// them, as DGMNB does, whose About lists both).
    #[test]
    fn every_bundled_typeface_is_credited() {
        let credited = |licence: &str| TYPEFACES.iter().any(|(_, _, text)| *text == licence);
        assert!(credited(INTER), "Inter isn't credited in About → Legal");
        for (name, _) in crate::FALLBACKS {
            assert!(name.starts_with("NotoSans"), "{name}: which licence?");
            assert!(credited(NOTO), "{name} isn't credited in About → Legal");
        }
        for (_, _, text) in TYPEFACES {
            let escaped = gtk::glib::markup_escape_text(text);
            let (_, plain, _) = pango::parse_markup(&escaped, '\0').unwrap();
            assert_eq!(plain, text);
        }
    }
}
