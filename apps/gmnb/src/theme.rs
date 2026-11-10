//! The theme hub: the one place that knows the active theme and the
//! resolved style, writes it to the CSS providers, and tells subscribers.
//!
//! What a style is worked out for (`ctx`) is gathered here: light or dark
//! from libadwaita, the desktop's accent colour from the Settings portal,
//! the Background opacity, the theme's options, reduced motion and the
//! desktop's font. The base look needs no theme: no Luau machine exists
//! then. Nothing here runs at rest: no timer and no tick callback.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use gmnb_theme::theme::{self as themes, Action, FontFile, OptionDef, Origin, Source, Theme};
use gmnb_theme::types::{Paint, Rgba};
use gmnb_theme::{Ctx, Limits, Note, OptionValue, Style, Update, Writer, css, schema};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};

// ---------------------------------------------------------------------------
// TRANSITION: the legacy scheme.
//
// The surfaces not yet migrated to the style (the display, the graph, the
// keypad's and the display's drawn effects, and the stylesheet's rules
// outside the window, keys and segmented controls) still read a `Scheme`
// and the `@wc_*` colours. Both are derived from the resolved style here,
// so they follow whatever theme is active. Remove `Scheme`,
// `legacy_scheme`, `legacy_colors`, `Hub::scheme`, `Hub::subscribe` and
// `Hub::subscribe_while` once nothing reads them.
// ---------------------------------------------------------------------------

/// TRANSITION: the colours the unmigrated surfaces draw with.
#[derive(Clone, Copy, Debug)]
pub struct Scheme {
    pub dark: bool,
    /// The window's background, top and bottom.
    pub base_top: [f32; 3],
    pub base_bottom: [f32; 3],
    /// Foreground text.
    pub fg: [f32; 3],
    /// The accent as text (operators, focus, selection).
    pub accent: [f32; 3],
    /// The equals key's fill, its two ends, and the text on it.
    pub hot_a: [f32; 3],
    pub hot_b: [f32; 3],
    pub on_hot: [f32; 3],
    /// Graph equation colours.
    pub series: [[f32; 3]; 6],
    /// Whether the keypad's and the display's built-in effects (ripples,
    /// the pointer's light, the cascade; gradient digits, glow and their
    /// animations) run: only for a theme that has that draw layer, whose
    /// hooks will replace them.
    pub keypad_effects: bool,
    pub display_effects: bool,
}

pub use appcore::color::{parse_hex, to_hex};

pub fn rgba(c: [f32; 3], a: f32) -> gdk::RGBA {
    gdk::RGBA::new(c[0], c[1], c[2], a)
}

fn rgb3(c: Rgba) -> [f32; 3] {
    [c[0], c[1], c[2]]
}

/// TRANSITION: a [`Scheme`] from the style.
fn legacy_scheme(style: &Style, keypad_effects: bool, display_effects: bool) -> Scheme {
    let c = &style.tokens.colors;
    let ends = |paint: Option<&Paint>, fallback: Rgba| match paint {
        Some(Paint::Color(c)) => (*c, *c),
        Some(Paint::Gradient(g)) => (
            g.colors.first().copied().unwrap_or(fallback),
            g.colors.last().copied().unwrap_or(fallback),
        ),
        None => (fallback, fallback),
    };
    let (top, bottom) = ends(style.fill("window.background").0.last(), c.bg);
    let equals = style.control("keys.equals");
    let (hot_a, hot_b) = ends(equals.and_then(|e| e.background.0.first()), c.accent);
    Scheme {
        dark: style.dark,
        base_top: rgb3(top),
        base_bottom: rgb3(bottom),
        fg: rgb3(c.fg),
        accent: rgb3(c.accent_text),
        hot_a: rgb3(hot_a),
        hot_b: rgb3(hot_b),
        on_hot: rgb3(equals.map_or(c.on_accent, |e| e.color)),
        series: c.series.map(rgb3),
        keypad_effects,
        display_effects,
    }
}

/// TRANSITION: the `@wc_*` colours of the stylesheet's unmigrated rules,
/// from the style's tokens. What 0.2 drew as glass is the tokens' second
/// surface, flat.
fn legacy_colors(style: &Style, scheme: &Scheme) -> String {
    let c = &style.tokens.colors;
    let rgb = |c: [f32; 3]| css::color([c[0], c[1], c[2], 1.0]);
    let mut out = String::new();
    for (name, value) in [
        ("wc_fg", css::color(c.fg)),
        ("wc_fg_dim", css::color(c.fg_dim)),
        ("wc_fg_faint", css::color(c.fg_faint)),
        ("wc_glass", css::color(c.surface2)),
        ("wc_glass_hi", css::color(c.surface2)),
        ("wc_glass_num", css::color(c.surface)),
        ("wc_glass_border", "transparent".to_string()),
        ("wc_shade", "transparent".to_string()),
        ("wc_accent", rgb(scheme.accent)),
        ("wc_hot_a", rgb(scheme.hot_a)),
        ("wc_hot_b", rgb(scheme.hot_b)),
        ("wc_on_hot", rgb(scheme.on_hot)),
        ("wc_base", rgb(scheme.base_bottom)),
    ] {
        out.push_str(&format!("@define-color {name} {value};\n"));
    }
    out
}

// ---------------------------------------------------------------------------
// The hub.
// ---------------------------------------------------------------------------

/// Returns false once it should be dropped (its widget is gone).
type Subscriber = Box<dyn Fn(&Scheme) -> bool>;
type StyleSubscriber = Box<dyn Fn(&Rc<Style>) -> bool>;
/// How the user is told something (a toast).
type Tell = Box<dyn Fn(&str)>;

/// What the hub starts with.
pub struct Config {
    /// The theme to use: an id, or "" for the base look.
    pub theme: String,
    /// What is saved of each theme's options.
    pub saved_options: BTreeMap<String, BTreeMap<String, OptionValue>>,
    /// Options for this run only (`GMNB_THEME_OPTIONS`, `GMNB_PALETTE`):
    /// option id and its value as text.
    pub run_options: Vec<(String, String)>,
    pub opacity: f32,
    /// "Use the desktop's accent colour".
    pub system_accent: bool,
    /// `GMNB_ACCENT`: what the desktop's accent is taken to be (`None`
    /// inside: none).
    pub accent: Option<Option<[f32; 3]>>,
    /// `GMNB_THEME_DIR`.
    pub dev_dir: Option<PathBuf>,
}

/// One theme of Settings' list.
pub struct Listed {
    /// "" for the base look.
    pub id: String,
    pub name: String,
    pub description: String,
    /// The user's folder replaces a built-in theme of the same id.
    pub your_copy: bool,
    /// Why it doesn't load, if it doesn't.
    pub error: Option<String>,
}

/// Counters for the gates (`GMNB_THEME_STATS`): what theming costs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    /// Luau machines alive (0 in the base look).
    pub machines: u32,
    /// Times `style(ctx)` ran.
    pub style_calls: u32,
    /// Loads of the main provider, and of the live one.
    pub main_loads: u32,
    pub live_loads: u32,
    /// Properties in each.
    pub main_properties: usize,
    pub live_properties: usize,
}

pub struct Hub {
    theme: RefCell<Option<Theme>>,
    /// The theme chosen: "" for the base look. It may not be loaded (not
    /// installed, or failed).
    active: RefCell<String>,
    /// Why the chosen theme isn't the one showing.
    error: RefCell<Option<String>>,
    /// What the theme's author should know, latest last.
    notes: RefCell<Vec<Note>>,
    /// `style` has failed and the user has been told.
    style_failed: Cell<bool>,
    options: RefCell<BTreeMap<String, BTreeMap<String, OptionValue>>>,
    run_options: Vec<(String, String)>,
    opacity: Cell<f32>,
    system_accent: Cell<bool>,
    accent_override: Option<Option<[f32; 3]>>,
    /// The desktop's accent as its portal reports it (read lazily, kept
    /// fresh by a portal watch). The inner `None`: it shares none.
    portal_accent: Cell<Option<Option<[f32; 3]>>>,
    portal_watch: RefCell<Option<gio::SignalSubscription>>,
    dev_dir: Option<PathBuf>,
    style: RefCell<Rc<Style>>,
    writer: RefCell<Writer>,
    main: gtk::CssProvider,
    live: gtk::CssProvider,
    windows: RefCell<Vec<glib::WeakRef<gtk::Window>>>,
    fonts: RefCell<HashSet<PathBuf>>,
    subscribers: RefCell<Vec<Subscriber>>,
    style_subscribers: RefCell<Vec<StyleSubscriber>>,
    /// How the user is told of a theme that failed (a toast).
    tell: RefCell<Option<Tell>>,
    stats: Cell<Stats>,
}

/// The app's name for the base look in Settings.
pub const BASE_NAME: &str = "GMNB";

/// `$XDG_DATA_HOME/gmnb/themes`: where the user's themes live.
pub fn user_dir() -> PathBuf {
    glib::user_data_dir().join(crate::DATA_DIR).join("themes")
}

impl Hub {
    pub fn new(config: Config) -> Rc<Self> {
        let provider = |priority: u32| {
            let p = gtk::CssProvider::new();
            p.connect_parsing_error(|_, section, err| {
                eprintln!("css error at {}: {err}", section.to_str());
            });
            gtk::style_context_add_provider_for_display(
                &gdk::Display::default().expect("display"),
                &p,
                priority,
            );
            p
        };
        let ctx = Ctx::default();
        let hub = Rc::new(Hub {
            theme: RefCell::new(None),
            active: RefCell::new(String::new()),
            error: RefCell::new(None),
            notes: RefCell::default(),
            style_failed: Cell::new(false),
            options: RefCell::new(config.saved_options),
            run_options: config.run_options,
            opacity: Cell::new(config.opacity),
            system_accent: Cell::new(config.system_accent),
            accent_override: config.accent,
            portal_accent: Cell::new(None),
            portal_watch: RefCell::new(None),
            dev_dir: config.dev_dir,
            style: RefCell::new(Rc::new(Style::base(&ctx))),
            writer: RefCell::new(Writer::new(crate::STYLESHEET)),
            // Above USER: shells like Noctalia/matugen write
            // ~/.config/gtk-4.0/gtk.css. The stylesheet is at USER + 1.
            main: provider(gtk::STYLE_PROVIDER_PRIORITY_USER + 2),
            live: provider(gtk::STYLE_PROVIDER_PRIORITY_USER + 3),
            windows: RefCell::default(),
            fonts: RefCell::default(),
            subscribers: RefCell::default(),
            style_subscribers: RefCell::default(),
            tell: RefCell::new(None),
            stats: Cell::default(),
        });
        let sm = adw::StyleManager::default();
        let weak = Rc::downgrade(&hub);
        sm.connect_dark_notify(move |_| {
            if let Some(hub) = weak.upgrade() {
                hub.apply(false);
            }
        });
        let weak = Rc::downgrade(&hub);
        sm.connect_accent_color_rgba_notify(move |_| {
            if let Some(hub) = weak.upgrade() {
                hub.refresh_portal_accent();
            }
        });
        if let Some(settings) = gtk::Settings::default() {
            // Reduced motion and the desktop's font are part of `ctx`.
            let weak = Rc::downgrade(&hub);
            settings.connect_gtk_enable_animations_notify(move |_| {
                if let Some(hub) = weak.upgrade() {
                    hub.apply(false);
                }
            });
            let weak = Rc::downgrade(&hub);
            settings.connect_gtk_font_name_notify(move |_| {
                if let Some(hub) = weak.upgrade() {
                    hub.apply(false);
                }
            });
        }
        hub.load(&config.theme);
        hub
    }

    // ---- ctx ---------------------------------------------------------------

    /// What the style is worked out for now.
    pub fn ctx(&self) -> Ctx {
        let settings = gtk::Settings::default();
        Ctx {
            dark: adw::StyleManager::default().is_dark(),
            accent: self.accent(),
            opacity: self.opacity.get(),
            options: self.option_values(),
            motion: settings
                .as_ref()
                .is_none_or(|s| s.is_gtk_enable_animations()),
            font: desktop_font(settings.as_ref()),
        }
    }

    /// The desktop's accent colour exactly as its portal reports it;
    /// `None` if it shares none or the user turned its use off.
    fn accent(&self) -> Option<[f32; 3]> {
        if let Some(fixed) = self.accent_override {
            return fixed;
        }
        if !self.system_accent.get() {
            return None;
        }
        if let Some(known) = self.portal_accent.get() {
            return known;
        }
        let read = read_portal_accent();
        self.portal_accent.set(Some(read));
        self.watch_portal();
        read
    }

    fn refresh_portal_accent(&self) {
        self.portal_accent.set(None);
        if self.accent_override.is_none() && self.system_accent.get() {
            self.apply(false);
        }
    }

    /// Live-follow accent changes (e.g. Noctalia re-theming from a wallpaper).
    fn watch_portal(&self) {
        if self.portal_watch.borrow().is_some() {
            return;
        }
        let Ok(conn) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
            return;
        };
        // The callback can't hold the hub, so it nudges it through the
        // thread's own slot instead.
        let sub = conn.subscribe_to_signal(
            Some("org.freedesktop.portal.Desktop"),
            Some("org.freedesktop.portal.Settings"),
            Some("SettingChanged"),
            Some("/org/freedesktop/portal/desktop"),
            Some("org.freedesktop.appearance"),
            gio::DBusSignalFlags::NONE,
            |signal| {
                // Signal bodies aren't type-checked: a malformed one is
                // ignored, not indexed into.
                let key = signal.parameters.try_child_value(1);
                if key.as_ref().and_then(|k| k.str()) == Some("accent-color") {
                    PORTAL_ACCENT_CHANGED.with(|f| {
                        if let Some(f) = f.borrow().as_ref() {
                            f();
                        }
                    });
                }
            },
        );
        self.portal_watch.replace(Some(sub));
    }

    /// "Use the desktop's accent colour".
    pub fn set_system_accent(&self, on: bool) {
        if self.system_accent.replace(on) != on {
            self.apply(false);
        }
    }

    /// The Background opacity. `drag`: one step of a slider being dragged
    /// (the few properties that change go to the live provider; call
    /// [`Hub::settle`] when it rests).
    pub fn set_opacity(&self, opacity: f32, drag: bool) {
        self.opacity.set(opacity);
        self.apply(drag);
    }

    // ---- The theme ------------------------------------------------------------

    fn sources(&self) -> Vec<Source> {
        themes::discover(Some(&user_dir()), self.dev_dir.as_deref())
    }

    /// What Settings lists: the base look first, then the built-in themes,
    /// then the user's. Each theme's file is run in a throwaway sandbox
    /// for its name.
    pub fn list(&self) -> Vec<Listed> {
        let mut out = vec![Listed {
            id: String::new(),
            name: BASE_NAME.to_string(),
            description: "The base look".to_string(),
            your_copy: false,
            error: None,
        }];
        for l in themes::list(&self.sources(), &Limits::default()) {
            let (name, description, error) = match l.meta {
                Ok(meta) => (meta.name, meta.description.unwrap_or_default(), None),
                Err(e) => (l.id.clone(), String::new(), Some(e)),
            };
            out.push(Listed {
                id: l.id,
                name,
                description,
                your_copy: l.origin == Origin::UserCopy,
                error,
            });
        }
        out
    }

    /// What the installed themes credit (About → Legal): a title and a
    /// text each.
    pub fn credits(&self) -> Vec<(String, String)> {
        self.sources()
            .into_iter()
            .filter_map(|source| Theme::load(source, Limits::default()).ok())
            .flat_map(|theme| theme.credits())
            .collect()
    }

    /// The theme chosen ("" for the base look), loaded or not.
    pub fn active(&self) -> String {
        self.active.borrow().clone()
    }

    /// Why the chosen theme isn't the one showing, if it isn't.
    pub fn error(&self) -> Option<String> {
        self.error.borrow().clone()
    }

    /// What the theme's author should know: the latest 20.
    pub fn notes(&self) -> Vec<Note> {
        let notes = self.notes.borrow();
        notes[notes.len().saturating_sub(20)..].to_vec()
    }

    /// How the user is told that a theme failed.
    pub fn set_tell(&self, tell: impl Fn(&str) + 'static) {
        self.tell.replace(Some(Box::new(tell)));
    }

    fn tell(&self, message: &str) {
        eprintln!("gmnb: {message}");
        if let Some(tell) = self.tell.borrow().as_ref() {
            tell(message);
        }
    }

    /// Chooses a theme ("" or "base": the base look) and shows it.
    pub fn set_theme(&self, id: &str) {
        self.load(id);
    }

    /// Runs the chosen theme's file again (Settings' Reload).
    pub fn reload(&self) {
        let id = self.active();
        self.load(&id);
    }

    fn load(&self, id: &str) {
        let id = if id == themes::BASE_ID { "" } else { id };
        *self.active.borrow_mut() = id.to_string();
        self.error.replace(None);
        self.notes.borrow_mut().clear();
        self.style_failed.set(false);
        // Unload first: one machine at a time.
        self.theme.replace(None);
        if !id.is_empty() {
            match self.sources().into_iter().find(|s| s.id == id) {
                None => {
                    // Kept as the choice: the theme returns when its
                    // folder does.
                    self.error
                        .replace(Some(format!("The theme “{id}” isn't installed")));
                }
                Some(source) => match Theme::load(source, Limits::default()) {
                    Ok(theme) => {
                        self.register_fonts(&theme);
                        self.notes.borrow_mut().extend(theme.notes.list.clone());
                        self.theme.replace(Some(theme));
                    }
                    Err(e) => {
                        self.error.replace(Some(e.to_string()));
                        self.tell(&format!("Theme “{id}” couldn't be loaded"));
                    }
                },
            }
        }
        self.apply(false);
    }

    /// Adds the theme's font files to the app's font map (they are known
    /// by the family names inside them). A built-in theme's are written to
    /// the cache directory first.
    fn register_fonts(&self, theme: &Theme) {
        for file in theme.font_files() {
            let path = match file {
                FontFile::Path(path) => path,
                FontFile::Bytes(name, bytes) => {
                    let name = name.rsplit('/').next().unwrap_or(name);
                    match crate::cache_font(&format!("{}-{name}", theme.source.id), bytes) {
                        Some(path) => path,
                        None => continue,
                    }
                }
            };
            if self.fonts.borrow_mut().insert(path.clone()) {
                crate::add_font_file(&path);
            }
        }
    }

    /// Runs `f` with the loaded theme, if one is.
    pub fn with_theme<R>(&self, f: impl FnOnce(Option<&Theme>) -> R) -> R {
        f(self.theme.borrow().as_ref())
    }

    /// The loaded theme's options, as Settings renders them.
    pub fn option_defs(&self) -> Vec<OptionDef> {
        self.with_theme(|t| t.map_or_else(Vec::new, |t| t.def.options.clone()))
    }

    /// The loaded theme's option values: the run's, else the saved ones,
    /// else the defaults. Always complete.
    pub fn option_values(&self) -> BTreeMap<String, OptionValue> {
        let theme = self.theme.borrow();
        let Some(theme) = theme.as_ref() else {
            return BTreeMap::new();
        };
        let mut saved = self
            .options
            .borrow()
            .get(&theme.source.id)
            .cloned()
            .unwrap_or_default();
        for (id, text) in &self.run_options {
            if let Some(def) = theme.def.options.iter().find(|o| o.id == *id) {
                let value = match text.as_str() {
                    "true" => OptionValue::Bool(true),
                    "false" => OptionValue::Bool(false),
                    t => match (&def.kind, t.parse::<f64>()) {
                        (themes::OptionKind::Slider { .. }, Ok(n)) => OptionValue::Number(n),
                        _ => OptionValue::Text(t.to_string()),
                    },
                };
                saved.insert(id.clone(), value);
            }
        }
        themes::complete_options(&theme.def.options, &saved)
    }

    /// Changes one of the loaded theme's options (the caller saves it).
    /// `drag`: as for [`Hub::set_opacity`].
    pub fn set_option(&self, id: &str, value: OptionValue, drag: bool) {
        let Some(theme) = self.with_theme(|t| t.map(|t| t.source.id.clone())) else {
            return;
        };
        self.options
            .borrow_mut()
            .entry(theme)
            .or_default()
            .insert(id.to_string(), value);
        self.apply(drag);
    }

    /// Whether Settings shows `option` now.
    pub fn option_visible(&self, option: &OptionDef) -> bool {
        let ctx = self.ctx();
        self.with_theme(|t| t.is_none_or(|t| t.visible(option, &ctx)))
    }

    /// Runs an option's action: the options it changes.
    pub fn run_action(&self, action: &Action) -> BTreeMap<String, OptionValue> {
        let ctx = self.ctx();
        self.with_theme(|t| t.and_then(|t| t.run_action(action, &ctx).ok()))
            .unwrap_or_default()
    }

    // ---- The style --------------------------------------------------------------

    pub fn style(&self) -> Rc<Style> {
        self.style.borrow().clone()
    }

    /// Works the style out again for the current `ctx`, and if it changed,
    /// writes it and tells the subscribers.
    fn apply(&self, drag: bool) {
        let ctx = self.ctx();
        let mut stats = self.stats.get();
        let mut unload = None;
        let resolved = {
            let mut theme = self.theme.borrow_mut();
            match theme.as_mut() {
                None => Some(gmnb_theme::Resolved {
                    style: Style::base(&ctx),
                    notes: Default::default(),
                }),
                Some(theme) => {
                    stats.style_calls += 1;
                    let result = theme.style(&ctx).cloned();
                    for line in theme.take_log() {
                        eprintln!("gmnb theme {}: {line}", theme.source.id);
                        self.notes.borrow_mut().push(Note {
                            path: "print".into(),
                            message: line,
                        });
                    }
                    match result {
                        Ok(resolved) => Some(resolved),
                        Err(e) => {
                            if theme.is_dead() {
                                unload = Some(e.to_string());
                            } else {
                                self.notes.borrow_mut().push(Note {
                                    path: "style".into(),
                                    message: e.to_string(),
                                });
                                if !self.style_failed.replace(true) {
                                    self.tell(&format!(
                                        "Theme “{}” has a problem: {e}",
                                        theme.def.meta.name
                                    ));
                                }
                            }
                            None
                        }
                    }
                }
            }
        };
        self.stats.set(stats);
        if let Some(why) = unload {
            // The memory limit: the theme is unloaded.
            let name = self.active();
            self.theme.replace(None);
            self.error.replace(Some(why));
            self.tell(&format!(
                "Theme “{name}” was unloaded: it ran out of memory"
            ));
            return self.apply(false);
        }
        // A `style` that failed leaves the last one that worked standing
        // (the base look, if none did).
        let Some(resolved) = resolved else { return };
        {
            let mut notes = self.notes.borrow_mut();
            for note in resolved.notes.list {
                if !notes.contains(&note) {
                    notes.push(note);
                }
            }
            let extra = notes.len().saturating_sub(200);
            notes.drain(..extra);
        }
        let style = Rc::new(resolved.style);
        let changed = **self.style.borrow() != *style;
        self.style.replace(style.clone());
        if changed || self.stats.get().main_loads == 0 {
            self.write(&style, drag);
            self.sync_windows(&style);
            self.notify(&style);
        }
    }

    /// The legacy scheme of the current style (TRANSITION).
    pub fn scheme(&self) -> Scheme {
        self.scheme_of(&self.style())
    }

    fn scheme_of(&self, style: &Style) -> Scheme {
        let has = |layer: &str| {
            self.with_theme(|t| {
                t.is_some_and(|t| t.def.layers.get(layer).is_some_and(|l| l.draw.is_some()))
            })
        };
        legacy_scheme(style, has("keypad"), has("display"))
    }

    /// Writes the style to the providers: the whole of it to the main one,
    /// or, for a step of a drag that changed few properties, those to the
    /// live one.
    fn write(&self, style: &Style, drag: bool) {
        let (mut preamble, stock) = css::stock(style);
        preamble.push_str(&legacy_colors(style, &self.scheme_of(style)));
        let mut properties = css::properties(style);
        properties.extend(stock);
        let mut writer = self.writer.borrow_mut();
        let mut stats = self.stats.get();
        match writer.update(&preamble, &properties, drag) {
            Update::Unchanged => {}
            Update::Live(text) => {
                self.live.load_from_string(&text);
                stats.live_loads += 1;
            }
            Update::Main(text) => {
                self.main.load_from_string(&text);
                self.live.load_from_string("");
                stats.main_loads += 1;
            }
        }
        stats.main_properties = writer.main_len();
        stats.live_properties = writer.live_len();
        self.stats.set(stats);
    }

    /// Folds what a drag left in the live provider back into the main
    /// one. Called when the dragged setting is saved, a moment after its
    /// last step (`prefs::persist_at_rest`): theming keeps no timer of
    /// its own.
    pub fn settle(&self) {
        let folded = self.writer.borrow_mut().fold();
        if let Some(text) = folded {
            self.main.load_from_string(&text);
            self.live.load_from_string("");
            let mut stats = self.stats.get();
            stats.main_loads += 1;
            stats.main_properties = self.writer.borrow().main_len();
            stats.live_properties = 0;
            self.stats.set(stats);
        }
    }

    /// A window the style applies to: it gets a class for each family of
    /// the toolkit's widgets the app draws itself (`own-switch`), which
    /// the stylesheet's rules for that family sit under. A theme hands
    /// families back with `native`.
    pub fn attach(&self, window: &impl IsA<gtk::Window>) {
        let window = window.clone().upcast::<gtk::Window>();
        self.windows.borrow_mut().push(window.downgrade());
        self.sync_window(&window, &self.style());
    }

    fn sync_window(&self, window: &gtk::Window, style: &Style) {
        let native = style.native();
        for family in schema::NATIVE_FAMILIES {
            let class = format!("own-{family}");
            if native.contains(&family) {
                window.remove_css_class(&class);
            } else {
                window.add_css_class(&class);
            }
        }
    }

    fn sync_windows(&self, style: &Style) {
        let windows: Vec<gtk::Window> = {
            let mut all = self.windows.borrow_mut();
            all.retain(|w| w.upgrade().is_some());
            all.iter().filter_map(|w| w.upgrade()).collect()
        };
        for window in windows {
            self.sync_window(&window, style);
        }
    }

    fn notify(&self, style: &Rc<Style>) {
        // Collect first: a subscriber may itself subscribe or open UI.
        let subs = std::mem::take(&mut *self.style_subscribers.borrow_mut());
        let mut keep: Vec<StyleSubscriber> = subs.into_iter().filter(|f| f(style)).collect();
        keep.append(&mut self.style_subscribers.borrow_mut());
        *self.style_subscribers.borrow_mut() = keep;

        let scheme = self.scheme_of(style);
        let subs = std::mem::take(&mut *self.subscribers.borrow_mut());
        let mut keep: Vec<Subscriber> = subs.into_iter().filter(|f| f(&scheme)).collect();
        keep.append(&mut self.subscribers.borrow_mut());
        *self.subscribers.borrow_mut() = keep;
    }

    /// Call `f` now and whenever the style changes, while `widget` is
    /// alive.
    pub fn subscribe_style<W: IsA<glib::Object>>(
        &self,
        widget: &W,
        f: impl Fn(&W, &Rc<Style>) + 'static,
    ) {
        f(widget, &self.style());
        let weak = widget.downgrade();
        self.style_subscribers
            .borrow_mut()
            .push(Box::new(move |s| match weak.upgrade() {
                Some(w) => {
                    f(&w, s);
                    true
                }
                None => false,
            }));
    }

    /// TRANSITION: call `f` now and whenever the scheme changes.
    pub fn subscribe(&self, f: impl Fn(&Scheme) + 'static) {
        f(&self.scheme());
        self.subscribers.borrow_mut().push(Box::new(move |s| {
            f(s);
            true
        }));
    }

    /// TRANSITION: like [`Hub::subscribe`], but only while `widget` is
    /// alive.
    pub fn subscribe_while<W: IsA<glib::Object>>(
        &self,
        widget: &W,
        f: impl Fn(&W, &Scheme) + 'static,
    ) {
        f(widget, &self.scheme());
        let weak = widget.downgrade();
        self.subscribers
            .borrow_mut()
            .push(Box::new(move |s| match weak.upgrade() {
                Some(w) => {
                    f(&w, s);
                    true
                }
                None => false,
            }));
    }

    pub fn stats(&self) -> Stats {
        let mut stats = self.stats.get();
        stats.machines = u32::from(self.theme.borrow().is_some());
        stats
    }
}

/// The desktop's interface font: its family, and its size in pixels.
fn desktop_font(settings: Option<&gtk::Settings>) -> (String, f32) {
    let fallback = ("sans-serif".to_string(), 15.0);
    let Some(settings) = settings else {
        return fallback;
    };
    let Some(name) = settings.gtk_font_name() else {
        return fallback;
    };
    let desc = pango::FontDescription::from_string(&name);
    let Some(family) = desc.family().filter(|f| !f.is_empty()) else {
        return fallback;
    };
    // A size in points is scaled by the desktop's resolution (in 1/1024
    // dots an inch; unset: 96).
    let size = desc.size() as f32 / pango::SCALE as f32;
    let px = if desc.is_size_absolute() {
        size
    } else {
        let dpi = settings.gtk_xft_dpi();
        let dpi = if dpi > 0 { dpi as f32 / 1024.0 } else { 96.0 };
        size * dpi / 72.0
    };
    let px = if px.is_finite() && px > 0.0 {
        px
    } else {
        fallback.1
    };
    (family.to_string(), (px * 100.0).round() / 100.0)
}

thread_local! {
    static PORTAL_ACCENT_CHANGED: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Route portal accent changes to `hub` (main thread only).
pub fn follow_portal_accent(hub: &Rc<Hub>) {
    let weak = Rc::downgrade(hub);
    PORTAL_ACCENT_CHANGED.with(|f| {
        f.replace(Some(Box::new(move || {
            if let Some(hub) = weak.upgrade() {
                hub.refresh_portal_accent();
            }
        })));
    });
}

/// `org.freedesktop.appearance accent-color` from the Settings portal:
/// `(ddd)` in 0..1, or out of range when the desktop has none set.
fn read_portal_accent() -> Option<[f32; 3]> {
    let conn = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
    let reply = conn
        .call_sync(
            Some("org.freedesktop.portal.Desktop"),
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
            "ReadOne",
            Some(&("org.freedesktop.appearance", "accent-color").to_variant()),
            Some(glib::VariantTy::new("(v)").expect("valid type")),
            gio::DBusCallFlags::NONE,
            500,
            gio::Cancellable::NONE,
        )
        .ok()?;
    let inner = reply.child_value(0).as_variant()?;
    let (r, g, b) = inner.get::<(f64, f64, f64)>()?;
    let ok = |v: f64| (0.0..=1.0).contains(&v);
    (ok(r) && ok(g) && ok(b)).then_some([r as f32, g as f32, b as f32])
}

/// `GMNB_ACCENT`: a hex colour, or `none`.
pub fn accent_from_env(value: &str) -> Option<Option<[f32; 3]>> {
    match value.trim() {
        "none" => Some(None),
        hex => parse_hex(hex).map(Some),
    }
}

/// `GMNB_THEME_OPTIONS='palette=ember;living=false'`: option ids and their
/// values as text.
pub fn options_from_env(value: &str) -> Vec<(String, String)> {
    value
        .split(';')
        .filter_map(|pair| {
            let (id, v) = pair.split_once('=')?;
            Some((id.trim().to_string(), v.trim().to_string()))
        })
        .filter(|(id, _)| !id.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environments_hooks_parse() {
        assert_eq!(accent_from_env("none"), Some(None));
        assert_eq!(accent_from_env("#ff0000"), Some(Some([1.0, 0.0, 0.0])));
        assert_eq!(accent_from_env("25d08d").map(|a| a.is_some()), Some(true));
        assert_eq!(accent_from_env("red"), None);
        assert_eq!(
            options_from_env("palette=ember; living = false;;bad;=x"),
            [
                ("palette".to_string(), "ember".to_string()),
                ("living".to_string(), "false".to_string())
            ]
        );
    }

    /// Theming costs nothing at rest: the hub and the backdrop install no
    /// timeout, no idle and no tick callback (the keypad's and the
    /// display's are the transition's, and run only while an effect of a
    /// theme with that layer plays).
    #[test]
    fn theming_installs_no_sources() {
        for (name, source) in [
            ("theme.rs", include_str!("theme.rs")),
            ("widgets/backdrop.rs", include_str!("widgets/backdrop.rs")),
        ] {
            let code = source.split("#[cfg(test)]").next().unwrap_or("");
            for call in ["timeout_add", "idle_add", "add_tick_callback", "source_new"] {
                assert!(!code.contains(call), "{name} calls {call}");
            }
        }
    }

    /// TRANSITION: the legacy scheme follows the style, for the base look
    /// and for a theme with a gradient on its equals key.
    #[test]
    fn the_legacy_scheme_follows_the_style() {
        let ctx = Ctx {
            dark: true,
            ..Ctx::default()
        };
        let style = Style::base(&ctx);
        let s = legacy_scheme(&style, false, false);
        let c = &style.tokens.colors;
        assert_eq!(s.base_top, rgb3(c.bg));
        assert_eq!(s.base_bottom, rgb3(c.bg));
        assert_eq!((s.hot_a, s.hot_b), (rgb3(c.accent), rgb3(c.accent)));
        assert_eq!(s.on_hot, rgb3(c.on_accent));
        assert_eq!(s.accent, rgb3(c.accent_text));
        assert!(!s.keypad_effects && !s.display_effects);
        let colors = legacy_colors(&style, &s);
        assert!(
            colors.contains("@define-color wc_base rgb(29,29,32);"),
            "{colors}"
        );
        assert!(colors.contains("@define-color wc_fg_dim rgba(242,242,245,0.66);"));
    }
}
