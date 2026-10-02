//! The window: header, navigation, settings, input routing, accessibility
//! and the event loop glue. Pages live in their own modules.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use appcore::input::{self, WindowAction};
use appcore::modes::{Group, PageKind, ViewMode};
use appcore::settings::{HasPages, PageStates};
use appcore::{Key, KeyPress, Named};
use serde::{Deserialize, Serialize};
use tiny_skia::{Pixmap, PixmapMut};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::{Key as WKey, KeyLocation, ModifiersState, NamedKey};
use winit::window::{CursorIcon, ResizeDirection, Window, WindowId};

use crate::calc::{self, CalcPage};
use crate::clipboard::Clipboard;
use crate::conv::{self, ConvPage};
use crate::date::{self, DatePage};
use crate::edit::TextEdit;
use crate::gfx::{Canvas, Rect};
use crate::graph::{self, GraphPage};
use crate::text::Text;
use crate::theme::Theme;
use crate::ui::{
    self, Align, BODY, CAPTION, Frame, Hit, Icons, Input, Node, SMALL, STRONG, Scroll, Sense,
    Style, TITLE, id,
};

pub const APP_ID: &str = "io.github.Go08er.DontGlazeMyNumbersBaby";
pub const APP_NAME: &str = "DGMNB";
/// Directory name under the XDG config/cache dirs.
pub const DATA_DIR: &str = "dgmnb";

const HEADER_H: f32 = 46.0;
const RESIZE_EDGE: f32 = 6.0;
const NAV_W: f32 = 280.0;

mod glyph {
    pub const MINIMIZE: &str = "M6 12.5h12";
    pub const MAXIMIZE: &str = "M6.5 6.5h11v11h-11z";
    pub const RESTORE: &str = "M6.5 9h8.5v8.5H6.5zM9 9V6.5h8.5V15H15";
    pub const BACK: &str = "M15 6l-6 6 6 6";
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "system" | "light" | "dark"
    pub theme: String,
    /// Use the desktop's accent colour when it shares one.
    pub system_accent: bool,
    pub mode: String,
    pub width: i32,
    pub height: i32,
    pub pages: PageStates,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            system_accent: true,
            mode: "standard".into(),
            width: 360,
            height: 600,
            pages: Default::default(),
        }
    }
}

impl HasPages for Settings {
    fn pages(&self) -> &PageStates {
        &self.pages
    }
    fn pages_mut(&mut self) -> &mut PageStates {
        &mut self.pages
    }
}

pub type Store = appcore::settings::Store<Settings>;

fn persist(store: &Store) {
    if let Err(e) = store.save() {
        eprintln!("dgmnb: {e}");
    }
}

// ---------------------------------------------------------------------------
// Messages and events
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    Nav(bool),
    Mode(ViewMode),
    Settings(bool),
    Licences(bool),
    Theme(&'static str),
    SystemAccent(bool),
    CopyLink(&'static str),
    Compact(bool),
    Minimize,
    Maximize,
    Close,
    Calc(calc::Msg),
    Conv(conv::Msg),
    Date(date::Msg),
    Graph(graph::Msg),
}

/// Things that wake the event loop from elsewhere.
#[derive(Debug)]
pub enum UserEvent {
    AccessKit(accesskit_winit::Event),
    /// Desktop setting changed: (key, prefers dark, accent).
    Desktop(Desktop),
    Currency(Box<Result<unitconv::CurrencySnapshot, unitconv::CurrencyError>>),
    Analysis(u64, Box<graphing::analysis::KeyGraphFeatures>),
    Plot(u64, Vec<graphing::graph::EquationPlot>, f64),
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(e: accesskit_winit::Event) -> Self {
        UserEvent::AccessKit(e)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Desktop {
    pub dark: Option<bool>,
    pub accent: Option<[f32; 3]>,
    /// The portal answered at all.
    pub portal: bool,
}

/// Markup label → plain text.
pub fn plain(s: &str) -> String {
    appcore::keys::plain_label(s)
}

/// Services pages can use while handling a message.
pub struct Cx<'a> {
    pub toasts: &'a mut Vec<(String, Instant)>,
    pub clipboard: Option<&'a Clipboard>,
    pub proxy: &'a EventLoopProxy<UserEvent>,
    pub wide: bool,
    /// Move keyboard focus (e.g. to a new equation field).
    pub focus: &'a mut Option<ui::Id>,
}

impl Cx<'_> {
    pub fn toast(&mut self, text: &str) {
        self.toasts.retain(|t| t.0 != text);
        self.toasts
            .push((text.to_string(), Instant::now() + Duration::from_secs(2)));
    }

    pub fn copy(&mut self, text: &str) {
        match self.clipboard {
            Some(c) => {
                c.copy_text(text);
                self.toast("Copied to clipboard");
            }
            None => self.toast("The clipboard isn't available"),
        }
    }

    pub fn copy_png(&mut self, png: Vec<u8>, what: &str) {
        match self.clipboard {
            Some(c) => {
                c.copy_png(png);
                self.toast(what);
            }
            None => self.toast("The clipboard isn't available"),
        }
    }

    pub fn paste(&mut self) -> Option<String> {
        self.clipboard?.paste_text()
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct Gfx {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    adapter: accesskit_winit::Adapter,
}

pub struct App {
    proxy: EventLoopProxy<UserEvent>,
    gfx: Option<Gfx>,
    clipboard: Option<Clipboard>,
    a11y: bool,
    text: Text,
    icons: Icons,
    input: Input,
    scrolls: HashMap<ui::Id, Scroll>,
    hits: Vec<Hit>,
    mods: ModifiersState,
    last_click: Option<(Instant, ui::Id)>,
    drag: Option<(ui::Id, f32, f32)>,
    /// The text field the input method is enabled for.
    ime: Option<Rect>,
    store: Store,
    desktop: Desktop,
    theme: Theme,
    mode: ViewMode,
    nav: bool,
    settings: bool,
    licences: bool,
    compact: bool,
    calc: Option<CalcPage>,
    conv: Option<ConvPage>,
    date: Option<DatePage>,
    graph: Option<GraphPage>,
    toasts: Vec<(String, Instant)>,
    dev: Dev,
}

/// Screenshot / scripting hooks (`DGMNB_*` environment variables).
#[derive(Default)]
struct Dev {
    screenshot: Option<PathBuf>,
    keys: Option<String>,
    /// `DGMNB_CLICKS="x,y;x,y"`: pointer clicks (logical px), one per frame.
    clicks: std::collections::VecDeque<(f32, f32)>,
    frames: u64,
    clicked_at: u64,
    deadline: Option<Instant>,
    autoclose: Option<Instant>,
    started: bool,
}

fn env(name: &str) -> Option<String> {
    std::env::var(format!("DGMNB_{name}")).ok()
}

impl App {
    pub fn new(proxy: EventLoopProxy<UserEvent>, desktop: Desktop) -> App {
        let screenshot = env("SCREENSHOT").map(PathBuf::from);
        let store = if screenshot.is_some() && env("REAL_STORE").is_none() {
            Store::ephemeral()
        } else {
            Store::load(DATA_DIR)
        };
        {
            let mut d = store.data.borrow_mut();
            match env("DARK").as_deref() {
                Some("1") => d.theme = "dark".into(),
                Some("0") => d.theme = "light".into(),
                _ => {}
            }
            if let Some(m) = env("MODE") {
                d.mode = m;
            }
            if let Some((w, h)) = env("SIZE").and_then(|s| {
                let (a, b) = s.split_once('x')?;
                Some((a.parse().ok()?, b.parse().ok()?))
            }) {
                (d.width, d.height) = (w, h);
            }
        }
        let mode = ViewMode::from_key(&store.data.borrow().mode).unwrap_or(ViewMode::Standard);
        let mut app = App {
            proxy,
            gfx: None,
            clipboard: None,
            a11y: false,
            text: Text::new(),
            icons: Icons::default(),
            input: Input::default(),
            scrolls: HashMap::new(),
            hits: Vec::new(),
            mods: ModifiersState::empty(),
            last_click: None,
            drag: None,
            ime: None,
            store,
            desktop,
            theme: Theme::new(false, None),
            mode,
            nav: false,
            settings: false,
            licences: false,
            compact: false,
            calc: None,
            conv: None,
            date: None,
            graph: None,
            toasts: Vec::new(),
            dev: Dev {
                keys: env("KEYS").map(|k| k.replace("\\n", "\n")),
                clicks: env("CLICKS")
                    .map(|c| {
                        c.split(';')
                            .filter_map(|p| {
                                let (x, y) = p.split_once(',')?;
                                Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                ..Dev::default()
            },
        };
        app.dev.screenshot = screenshot;
        app.retheme();
        app.set_mode(mode);
        app.nav = env("NAV").as_deref() == Some("1");
        app.settings = env("PREFS").as_deref() == Some("1");
        app
    }

    fn retheme(&mut self) {
        let d = self.store.data.borrow();
        let dark = match d.theme.as_str() {
            "light" => false,
            "dark" => true,
            _ => self.desktop.dark.unwrap_or(false),
        };
        let accent = env("ACCENT")
            .and_then(|h| appcore::color::parse_hex(&h))
            .or(if d.system_accent {
                self.desktop.accent
            } else {
                None
            });
        self.theme = Theme::new(dark, accent);
    }

    fn wide(&self) -> bool {
        self.gfx.as_ref().is_some_and(|g| {
            let s = g
                .window
                .inner_size()
                .to_logical::<f32>(g.window.scale_factor());
            s.width >= calc::WIDE
        })
    }

    fn set_mode(&mut self, mode: ViewMode) {
        self.mode = mode;
        self.nav = false;
        self.settings = false;
        if mode != ViewMode::Standard {
            self.set_compact(false);
        }
        self.input.focus = None;
        let store = &self.store;
        match mode.page() {
            PageKind::Calculator => {
                let page = self
                    .calc
                    .get_or_insert_with(|| CalcPage::new(store.page_state("calculator")));
                if let Some(m) = mode.calc_mode() {
                    page.set_mode(m);
                }
            }
            PageKind::Converter => {
                let page = self
                    .conv
                    .get_or_insert_with(|| ConvPage::new(store.page_state("converter")));
                if let Some(m) = mode.converter_mode() {
                    page.activate(m, &self.proxy);
                }
            }
            PageKind::Date => {
                self.date.get_or_insert_with(DatePage::new);
            }
            PageKind::Graphing => {
                let eqs = match env("EQUATIONS") {
                    Some(list) => appcore::graph::from_list(&list),
                    None => appcore::graph::restore(store.page_state("graphing")),
                };
                self.graph.get_or_insert_with(|| GraphPage::new(eqs));
            }
        }
        self.store.data.borrow_mut().mode = mode.key().into();
        self.redraw();
    }

    fn set_compact(&mut self, on: bool) {
        if self.compact == on {
            return;
        }
        self.compact = on;
        if let Some(g) = &self.gfx {
            let size = if on {
                LogicalSize::new(320.0, 420.0)
            } else {
                let d = self.store.data.borrow();
                LogicalSize::new(d.width.max(320) as f64, d.height.max(420) as f64)
            };
            let _ = g.window.request_inner_size(size);
        }
    }

    fn save(&mut self) {
        if let Some(p) = &self.calc {
            self.store.set_page_state("calculator", p.save());
        }
        if let Some(p) = &self.conv {
            self.store.set_page_state("converter", p.save());
        }
        if let Some(p) = &self.graph {
            self.store.set_page_state("graphing", p.save());
        }
        if let Some(g) = &self.gfx
            && !self.compact
            && !g.window.is_maximized()
        {
            let s = g
                .window
                .inner_size()
                .to_logical::<f64>(g.window.scale_factor());
            let mut d = self.store.data.borrow_mut();
            d.width = s.width.round() as i32;
            d.height = s.height.round() as i32;
        }
        persist(&self.store);
    }

    fn redraw(&self) {
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }

    fn cx_parts(
        &mut self,
    ) -> (
        Cx<'_>,
        &mut Option<CalcPage>,
        &mut Option<ConvPage>,
        &mut Option<DatePage>,
        &mut Option<GraphPage>,
    ) {
        let wide = self.wide();
        (
            Cx {
                toasts: &mut self.toasts,
                clipboard: self.clipboard.as_ref(),
                proxy: &self.proxy,
                wide,
                focus: &mut self.input.focus,
            },
            &mut self.calc,
            &mut self.conv,
            &mut self.date,
            &mut self.graph,
        )
    }

    fn update(&mut self, el: &ActiveEventLoop, msg: Msg) {
        match msg {
            Msg::Nav(open) => self.nav = open,
            Msg::Mode(m) => self.set_mode(m),
            Msg::Settings(open) => {
                self.settings = open;
                self.nav = false;
                self.licences = false;
            }
            Msg::Licences(open) => self.licences = open,
            Msg::Theme(t) => {
                self.store.data.borrow_mut().theme = t.into();
                self.retheme();
                persist(&self.store);
            }
            Msg::SystemAccent(on) => {
                self.store.data.borrow_mut().system_accent = on;
                self.retheme();
                persist(&self.store);
            }
            Msg::CopyLink(url) => {
                let (mut cx, ..) = self.cx_parts();
                cx.copy(url);
            }
            Msg::Compact(on) => self.set_compact(on),
            Msg::Minimize => {
                if let Some(g) = &self.gfx {
                    g.window.set_minimized(true);
                }
            }
            Msg::Maximize => {
                if let Some(g) = &self.gfx {
                    g.window.set_maximized(!g.window.is_maximized());
                }
            }
            Msg::Close => {
                self.save();
                el.exit();
            }
            Msg::Calc(m) => {
                let (mut cx, calc, ..) = self.cx_parts();
                if let Some(p) = calc {
                    p.update(m, &mut cx);
                }
            }
            Msg::Conv(m) => {
                let (mut cx, _, conv, ..) = self.cx_parts();
                if let Some(p) = conv {
                    p.update(m, &mut cx);
                }
            }
            Msg::Date(m) => {
                let (mut cx, _, _, date, _) = self.cx_parts();
                if let Some(p) = date {
                    p.update(m, &mut cx);
                }
            }
            Msg::Graph(graph::Msg::CopyImage) => self.copy_graph_image(),
            Msg::Graph(m) => {
                let (mut cx, _, _, _, graph) = self.cx_parts();
                if let Some(p) = graph {
                    p.update(m, &mut cx);
                }
            }
        }
        // Pages may have moved focus (new equation, unit search).
        self.sync_ime();
        self.redraw();
    }

    /// Render the graph canvas to a PNG and put it on the clipboard.
    fn copy_graph_image(&mut self) {
        let Some(g) = self.graph.as_mut() else { return };
        let r = g.canvas_rect();
        let scale = self
            .gfx
            .as_ref()
            .map_or(1.0, |g| g.window.scale_factor() as f32);
        let (w, h) = ((r.w * scale).round() as u32, (r.h * scale).round() as u32);
        let Some(mut pm) = Pixmap::new(w.max(1), h.max(1)) else {
            return;
        };
        {
            let canvas = Canvas::new(pm.as_mut(), scale, false);
            let input = Input::default();
            let mut scrolls = HashMap::new();
            let mut f = Frame::new(
                canvas,
                &mut self.text,
                &mut self.icons,
                self.theme,
                &input,
                &mut scrolls,
                false,
            );
            f.cv.fill(self.theme.surface);
            g.draw_canvas(&mut f, Rect::new(0.0, 0.0, r.w, r.h), false);
        }
        g.restore_canvas_rect(r);
        let png = pm.encode_png().unwrap_or_default();
        let (mut cx, ..) = self.cx_parts();
        cx.copy_png(png, "Graph copied to clipboard");
    }

    // ------------------------------------------------------------ drawing

    fn draw(&mut self, cv: Canvas, maximized: bool) -> (Vec<Hit>, Option<Vec<Node>>) {
        let theme = self.theme;
        let App {
            text,
            icons,
            input,
            scrolls,
            calc,
            conv,
            date,
            graph,
            ..
        } = self;
        let mut f = Frame::new(cv, text, icons, theme, input, scrolls, self.a11y);
        let (w, h) = (f.width(), f.height());
        f.cv.fill(theme.bg);
        let full = Rect::new(0.0, 0.0, w, h);
        let (header, body) = full.take_top(HEADER_H);
        let wide = w >= calc::WIDE;

        draw_header(
            &mut f,
            header,
            self.mode,
            self.settings,
            self.compact,
            maximized,
            wide,
            calc.as_ref(),
            graph.as_ref(),
        );
        if self.settings {
            draw_settings(&mut f, body, &self.store.data.borrow(), self.desktop);
        } else {
            match self.mode.page() {
                PageKind::Calculator => {
                    if let Some(p) = calc.as_mut() {
                        p.view(&mut f, body, self.compact);
                    }
                }
                PageKind::Converter => {
                    if let Some(p) = conv.as_mut() {
                        p.view(&mut f, body);
                    }
                }
                PageKind::Date => {
                    if let Some(p) = date.as_mut() {
                        p.view(&mut f, body);
                    }
                }
                PageKind::Graphing => {
                    if let Some(p) = graph.as_mut() {
                        p.view(&mut f, body);
                    }
                }
            }
            match self.mode.page() {
                PageKind::Calculator => {
                    if let Some(p) = calc.as_mut() {
                        p.overlay(&mut f, body);
                    }
                }
                PageKind::Converter => {
                    if let Some(p) = conv.as_mut() {
                        p.overlay(&mut f, body);
                    }
                }
                PageKind::Date => {
                    if let Some(p) = date.as_mut() {
                        p.overlay(&mut f, body);
                    }
                }
                PageKind::Graphing => {
                    if let Some(p) = graph.as_mut() {
                        p.overlay(&mut f, body);
                    }
                }
            }
        }
        if self.licences {
            draw_licences(&mut f, full);
        }
        if self.nav {
            draw_nav(&mut f, full, self.mode, self.settings);
        }
        // Toasts.
        if let Some((msg, _)) = self.toasts.last() {
            let line = f.layout(msg, BODY);
            let tw = line.width + 32.0;
            let r = Rect::new((w - tw) / 2.0, h - 64.0, tw, 38.0);
            f.cv.rounded(r, 19.0, theme.fg.alpha(0.92));
            f.draw_line(&line, r, Align::Center, theme.bg);
            if let Some(n) = f.node(id("toast"), accesskit::Role::Status, msg, r) {
                n.live = true;
            }
        }
        (std::mem::take(&mut f.hits), f.nodes.take())
    }

    fn render(&mut self) {
        // Take the surface out while drawing so `draw` can borrow `self`.
        let Some(mut g) = self.gfx.take() else { return };
        let size = g.window.inner_size();
        let scale = g.window.scale_factor();
        let maximized = g.window.is_maximized();
        let drawn = (|| {
            let w = NonZeroU32::new(size.width)?;
            let h = NonZeroU32::new(size.height)?;
            g.surface.resize(w, h).ok()?;
            let mut buffer = g.surface.buffer_mut().ok()?;
            // SAFETY: reinterpreting the u32 pixel buffer as bytes (same
            // memory, 4× the length; u8 has no alignment requirement).
            let bytes = unsafe {
                std::slice::from_raw_parts_mut(buffer.as_mut_ptr().cast::<u8>(), buffer.len() * 4)
            };
            let pm = PixmapMut::from_bytes(bytes, size.width, size.height)?;
            let out = self.draw(Canvas::new(pm, scale as f32, true), maximized);
            buffer.present().ok()?;
            Some(out)
        })();
        if let Some((hits, nodes)) = drawn {
            self.hits = hits;
            self.dev.frames += 1;
            if let Some(nodes) = nodes {
                let title = format!("{} — {}", APP_NAME, self.mode.title());
                let focus = self.input.focus;
                g.adapter
                    .update_if_active(|| crate::a11y::tree(&nodes, &title, focus, scale));
            }
        }
        self.gfx = Some(g);
        self.update_hover();
        self.sync_ime();
    }

    /// Render the window into a PNG (screenshot hook).
    fn screenshot(&mut self, path: &PathBuf) {
        let Some(g) = &self.gfx else { return };
        let size = g.window.inner_size();
        let scale = g.window.scale_factor() as f32;
        let Some(mut pm) = Pixmap::new(size.width.max(1), size.height.max(1)) else {
            return;
        };
        let maximized = g.window.is_maximized();
        let canvas = Canvas::new(pm.as_mut(), scale, false);
        let _ = self.draw(canvas, maximized);
        match pm.save_png(path) {
            Ok(()) => println!(
                "screenshot: {} ({}x{})",
                path.display(),
                size.width,
                size.height
            ),
            Err(e) => eprintln!("screenshot failed: {e}"),
        }
    }

    // ------------------------------------------------------------ input

    fn topmost(&self, x: f32, y: f32) -> Option<&Hit> {
        self.hits
            .iter()
            .rev()
            .find(|h| h.sense != Sense::Scroll && h.rect.contains(x, y))
    }

    fn update_hover(&mut self) {
        let hover = self
            .input
            .pointer
            .and_then(|(x, y)| self.topmost(x, y).map(|h| h.id));
        if hover != self.input.hover {
            self.input.hover = hover;
            self.redraw();
        }
        let cursor = match self.input.pointer {
            Some((x, y)) => match self.resize_dir(x, y) {
                Some(d) => CursorIcon::from(d),
                None => match self.topmost(x, y).map(|h| h.sense) {
                    Some(Sense::Text) => CursorIcon::Text,
                    Some(Sense::Drag) if self.mode == ViewMode::Graphing => CursorIcon::Grab,
                    _ => CursorIcon::Default,
                },
            },
            None => CursorIcon::Default,
        };
        if let Some(g) = &self.gfx {
            g.window.set_cursor(cursor);
        }
    }

    fn resize_dir(&self, x: f32, y: f32) -> Option<ResizeDirection> {
        let g = self.gfx.as_ref()?;
        if g.window.is_maximized() {
            return None;
        }
        let s = g
            .window
            .inner_size()
            .to_logical::<f32>(g.window.scale_factor());
        let (l, r) = (x < RESIZE_EDGE, x > s.width - RESIZE_EDGE);
        let (t, b) = (y < RESIZE_EDGE, y > s.height - RESIZE_EDGE);
        Some(match (l, r, t, b) {
            (true, _, true, _) => ResizeDirection::NorthWest,
            (_, true, true, _) => ResizeDirection::NorthEast,
            (true, _, _, true) => ResizeDirection::SouthWest,
            (_, true, _, true) => ResizeDirection::SouthEast,
            (true, ..) => ResizeDirection::West,
            (_, true, ..) => ResizeDirection::East,
            (_, _, true, _) => ResizeDirection::North,
            (_, _, _, true) => ResizeDirection::South,
            _ => return None,
        })
    }

    fn pointer_pressed(&mut self, el: &ActiveEventLoop, x: f32, y: f32) {
        self.input.focus_visible = false;
        if let Some(d) = self.resize_dir(x, y)
            && let Some(g) = &self.gfx
        {
            let _ = g.window.drag_resize_window(d);
            return;
        }
        let hit = self.topmost(x, y).cloned();
        let Some(hit) = hit else {
            // Bare header: move the window; double-click maximizes.
            if y < HEADER_H {
                let now = Instant::now();
                let double = self
                    .last_click
                    .is_some_and(|(t, i)| i == 0 && now - t < Duration::from_millis(400));
                self.last_click = Some((now, 0));
                if double {
                    self.update(el, Msg::Maximize);
                } else if let Some(g) = &self.gfx {
                    let _ = g.window.drag_window();
                }
            }
            if self.input.focus.is_some() {
                self.input.focus = None;
                self.sync_ime();
                self.redraw();
            }
            return;
        };
        self.input.pressed = Some(hit.id);
        let now = Instant::now();
        let double = self
            .last_click
            .is_some_and(|(t, i)| i == hit.id && now - t < Duration::from_millis(400));
        self.last_click = Some((now, hit.id));
        match hit.sense {
            Sense::Text => {
                self.input.focus = Some(hit.id);
                let shift = self.mods.shift_key();
                if let Some(e) = self.field(hit.id) {
                    let _ = (e, shift);
                }
                self.click_field(hit.id, x, hit.rect, shift, double);
                self.drag = Some((hit.id, x, y));
            }
            Sense::Drag => {
                self.input.focus = Some(hit.id);
                self.drag = Some((hit.id, x, y));
                self.page_drag(hit.id, x, y, 0.0, 0.0, true);
            }
            Sense::Click
                if hit.focusable && self.field(self.input.focus.unwrap_or(0)).is_some() =>
            {
                self.input.focus = None;
            }
            _ => {}
        }
        self.sync_ime();
        self.redraw();
    }

    fn pointer_released(&mut self, el: &ActiveEventLoop) {
        let pressed = self.input.pressed.take();
        self.drag = None;
        if let Some(p) = pressed
            && self.input.hover == Some(p)
            && let Some(msg) = self
                .hits
                .iter()
                .rev()
                .find(|h| h.id == p && h.sense == Sense::Click)
                .and_then(|h| h.msg.clone())
        {
            self.update(el, msg);
        }
        if let Some(p) = pressed {
            self.page_drag(p, 0.0, 0.0, 0.0, 0.0, false);
        }
        self.redraw();
    }

    fn pointer_moved(&mut self, x: f32, y: f32) {
        self.input.pointer = Some((x, y));
        if let Some((id, x0, y0)) = self.drag {
            if self.field(id).is_some() {
                if let Some(h) = self.hits.iter().find(|h| h.id == id).map(|h| h.rect) {
                    self.click_field(id, x, h, true, false);
                }
            } else {
                self.page_drag(id, x, y, x - x0, y - y0, true);
            }
            self.drag = Some((id, x, y));
            self.redraw();
        }
        if self.mode == ViewMode::Graphing
            && let Some(g) = self.graph.as_mut()
            && g.pointer(x, y)
        {
            self.redraw();
        }
        self.update_hover();
    }

    fn page_drag(&mut self, id: ui::Id, x: f32, y: f32, dx: f32, dy: f32, active: bool) {
        let rect = self.hits.iter().find(|h| h.id == id).map(|h| h.rect);
        let Some(rect) = rect else { return };
        let (mut cx, _, _, _, graph) = self.cx_parts();
        if let Some(g) = graph.as_mut() {
            g.drag(id, rect, x, y, dx, dy, active, &mut cx);
        }
    }

    fn wheel(&mut self, dy: f32, x: f32, y: f32) {
        if self.mode == ViewMode::Graphing
            && !self.settings
            && let Some(g) = self.graph.as_mut()
            && g.wheel(x, y, dy, &self.hits)
        {
            self.redraw();
            return;
        }
        let target = self
            .hits
            .iter()
            .rev()
            .find(|h| h.sense == Sense::Scroll && h.rect.contains(x, y))
            .map(|h| h.id);
        if let Some(id) = target
            && let Some(s) = self.scrolls.get_mut(&id)
        {
            s.offset = (s.offset - dy).clamp(0.0, s.max());
            self.redraw();
        }
    }

    // ------------------------------------------------------------ text fields

    fn field(&mut self, id: ui::Id) -> Option<&mut TextEdit> {
        if id == 0 || self.settings {
            return None;
        }
        match self.mode.page() {
            PageKind::Graphing => self.graph.as_mut()?.field(id),
            PageKind::Converter => self.conv.as_mut()?.field(id),
            PageKind::Date => self.date.as_mut()?.field(id),
            PageKind::Calculator => None,
        }
    }

    fn field_changed(&mut self, id: ui::Id) {
        let page = self.mode.page();
        let (mut cx, _, conv, date, graph) = self.cx_parts();
        match page {
            PageKind::Graphing => {
                if let Some(g) = graph.as_mut() {
                    g.field_changed(id, &mut cx);
                }
            }
            PageKind::Converter => {
                if let Some(c) = conv.as_mut() {
                    c.field_changed(id);
                }
            }
            PageKind::Date => {
                if let Some(d) = date.as_mut() {
                    d.field_changed(id);
                }
            }
            PageKind::Calculator => {}
        }
    }

    fn click_field(&mut self, id: ui::Id, x: f32, rect: Rect, extend: bool, double: bool) {
        let Some(text) = self.field(id).map(|e| e.text.clone()) else {
            return;
        };
        let line = self.text.layout(&text, BODY);
        let inner_x = rect.x + 10.0;
        let cursor = self.field(id).map_or(0, |e| e.cursor);
        let scroll = (line.caret_x(cursor) - (rect.w - 20.0) + 2.0).max(0.0);
        let off = line.offset_at(x - inner_x + scroll);
        if let Some(e) = self.field(id) {
            if double {
                e.select_word(off);
            } else {
                e.place(off, extend);
            }
        }
    }

    /// Enable the input method while a text field has focus (only telling
    /// the compositor when something changed).
    fn sync_ime(&mut self) {
        let focus = self.input.focus;
        let field = focus.and_then(|f| {
            self.hits
                .iter()
                .find(|h| h.id == f && h.sense == Sense::Text)
                .map(|h| h.rect)
        });
        if field == self.ime {
            return;
        }
        self.ime = field;
        let Some(g) = &self.gfx else { return };
        g.window.set_ime_allowed(field.is_some());
        if let Some(r) = field {
            g.window.set_ime_cursor_area(
                winit::dpi::LogicalPosition::new(r.x as f64, r.y as f64),
                LogicalSize::new(r.w as f64, r.h as f64),
            );
        }
    }

    /// Editing keys for the focused text field; true if consumed.
    fn edit_key(&mut self, kp: &KeyPress, text: Option<&str>) -> bool {
        enum Clip {
            None,
            Copy,
            Cut,
            Paste,
        }
        let Some(id) = self.input.focus else {
            return false;
        };
        let (ctrl, shift, alt) = (kp.ctrl, kp.shift, kp.alt);
        let mut changed = true;
        let mut clip = Clip::None;
        let selected = {
            let Some(e) = self.field(id) else {
                return false;
            };
            match kp.key {
                Key::Named(Named::Backspace) => e.backspace(ctrl),
                Key::Named(Named::Delete) => e.delete(ctrl),
                Key::Named(Named::Left) => {
                    e.left(shift, ctrl);
                    changed = false;
                }
                Key::Named(Named::Right) => {
                    e.right(shift, ctrl);
                    changed = false;
                }
                Key::Named(Named::Home) if !ctrl => {
                    e.home(shift);
                    changed = false;
                }
                Key::Named(Named::End) => {
                    e.end(shift);
                    changed = false;
                }
                Key::Char('a' | 'A') if ctrl && !alt => {
                    e.select_all();
                    changed = false;
                }
                Key::Char('c' | 'C') if ctrl && !alt => {
                    clip = Clip::Copy;
                    changed = false;
                }
                Key::Named(Named::Insert) if ctrl => {
                    clip = Clip::Copy;
                    changed = false;
                }
                Key::Char('x' | 'X') if ctrl && !alt => clip = Clip::Cut,
                Key::Char('v' | 'V') if ctrl && !alt => clip = Clip::Paste,
                Key::Named(Named::Insert) if shift => clip = Clip::Paste,
                _ if !ctrl && !alt => match text {
                    Some(t) if !t.is_empty() && !t.chars().any(char::is_control) => e.insert(t),
                    _ => return false,
                },
                _ => return false,
            }
            e.selected_text().to_string()
        };
        match clip {
            Clip::Copy | Clip::Cut if !selected.is_empty() => {
                if let Some(c) = &self.clipboard {
                    c.copy_text(&selected);
                }
                if matches!(clip, Clip::Cut)
                    && let Some(e) = self.field(id)
                {
                    e.delete_selection();
                }
            }
            Clip::Paste => {
                let pasted = self.clipboard.as_ref().and_then(Clipboard::paste_text);
                if let (Some(t), Some(e)) = (pasted, self.field(id)) {
                    e.insert(t.lines().next().unwrap_or(""));
                }
            }
            _ => {}
        }
        if changed {
            self.field_changed(id);
        }
        self.redraw();
        true
    }

    // ------------------------------------------------------------ keys

    fn key_press(&mut self, el: &ActiveEventLoop, kp: KeyPress, text: Option<String>) {
        // Text fields first (except app-wide chords).
        if self.field(self.input.focus.unwrap_or(0)).is_some()
            && !input::is_global_chord(&kp)
            && !matches!(
                kp.key,
                Key::Named(Named::Tab | Named::Escape | Named::Enter)
            )
            && self.edit_key(&kp, text.as_deref())
        {
            return;
        }
        match kp.key {
            Key::Named(Named::Tab) if !kp.ctrl && !kp.alt => {
                self.move_focus(!kp.shift);
                return;
            }
            Key::Named(Named::Escape) => {
                if self.close_overlay() {
                    self.redraw();
                    return;
                }
                if self.input.focus.is_some() && self.field(self.input.focus.unwrap_or(0)).is_some()
                {
                    self.input.focus = None;
                    self.sync_ime();
                    self.redraw();
                    return;
                }
            }
            Key::Char(' ') | Key::Named(Named::Enter) if !kp.ctrl && !kp.alt => {
                let focused = self.input.focus.and_then(|f| {
                    self.hits
                        .iter()
                        .rev()
                        .find(|h| h.id == f && h.sense == Sense::Click)
                        .cloned()
                });
                let enter = kp.key == Key::Named(Named::Enter);
                if let Some(h) = focused
                    && self.input.focus_visible
                    && !(enter && h.keypad)
                    && let Some(m) = h.msg
                {
                    self.update(el, m);
                    return;
                }
                if enter
                    && let Some(id) = self.input.focus
                    && self.field(id).is_some()
                {
                    let page = self.mode.page();
                    let (mut cx, _, conv, _, graph) = self.cx_parts();
                    match page {
                        PageKind::Graphing => {
                            if let Some(g) = graph.as_mut() {
                                g.field_activate(id, &mut cx);
                            }
                        }
                        PageKind::Converter => {
                            if let Some(c) = conv.as_mut() {
                                c.activate_search();
                                *cx.focus = None;
                            }
                        }
                        _ => *cx.focus = None,
                    }
                    self.sync_ime();
                    self.redraw();
                    return;
                }
            }
            _ => {}
        }
        if let Some(a) = input::window_shortcut(&kp) {
            match a {
                WindowAction::SwitchMode(m) => self.set_mode(m),
                WindowAction::Copy => self.copy(),
                WindowAction::Paste => self.paste(),
            }
            self.redraw();
            return;
        }
        if self.settings || self.nav {
            return;
        }
        if self.mode == ViewMode::Standard
            && let Some(on) = input::compact_shortcut(&kp)
        {
            self.set_compact(on);
            self.redraw();
            return;
        }
        let handled = {
            let mode = self.mode.page();
            let (mut cx, calc, conv, _, graph) = self.cx_parts();
            match mode {
                PageKind::Calculator => calc.as_mut().is_some_and(|p| p.key(&kp, &mut cx)),
                PageKind::Converter => conv.as_mut().is_some_and(|p| p.key(&kp)),
                PageKind::Graphing => graph.as_mut().is_some_and(|p| p.key(&kp, &mut cx)),
                PageKind::Date => false,
            }
        };
        if handled {
            self.redraw();
        }
    }

    fn close_overlay(&mut self) -> bool {
        if self.licences {
            self.licences = false;
            return true;
        }
        if self.nav {
            self.nav = false;
            return true;
        }
        match self.mode.page() {
            PageKind::Calculator => self.calc.as_mut().is_some_and(|p| p.popup.take().is_some()),
            PageKind::Converter => self.conv.as_mut().is_some_and(|p| p.close_popup()),
            PageKind::Date => self.date.as_mut().is_some_and(|p| p.close_popup()),
            PageKind::Graphing => self.graph.as_mut().is_some_and(|p| p.close_popup()),
        }
    }

    fn move_focus(&mut self, forward: bool) {
        // Only cycle within the topmost overlay, if one is open.
        let start = self
            .hits
            .iter()
            .rposition(|h| h.id == id("scrim"))
            .unwrap_or(0);
        let ids: Vec<ui::Id> = self.hits[start..]
            .iter()
            .filter(|h| h.focusable)
            .map(|h| h.id)
            .collect();
        if ids.is_empty() {
            return;
        }
        let pos = self
            .input
            .focus
            .and_then(|f| ids.iter().position(|&i| i == f));
        let next = match (pos, forward) {
            (None, true) => 0,
            (None, false) => ids.len() - 1,
            (Some(p), true) => (p + 1) % ids.len(),
            (Some(p), false) => (p + ids.len() - 1) % ids.len(),
        };
        self.input.focus = Some(ids[next]);
        self.input.focus_visible = true;
        self.sync_ime();
        self.redraw();
    }

    fn copy(&mut self) {
        let text = match self.mode.page() {
            PageKind::Calculator => self.calc.as_ref().map(CalcPage::copy_text),
            PageKind::Converter => self.conv.as_ref().map(ConvPage::copy_text),
            PageKind::Date => self.date.as_ref().and_then(DatePage::copy_text),
            PageKind::Graphing => self.graph.as_ref().and_then(GraphPage::copy_text),
        };
        if let Some(t) = text {
            let (mut cx, ..) = self.cx_parts();
            cx.copy(&t);
        }
    }

    fn paste(&mut self) {
        let Some(text) = self.clipboard.as_ref().and_then(Clipboard::paste_text) else {
            return;
        };
        let page = self.mode.page();
        let (mut cx, calc, conv, _, graph) = self.cx_parts();
        match page {
            PageKind::Calculator => {
                if let Some(p) = calc {
                    p.paste(&text);
                }
            }
            PageKind::Converter => {
                if let Some(p) = conv {
                    p.paste(&text);
                }
            }
            PageKind::Graphing => {
                if let Some(p) = graph {
                    p.paste(&text, &mut cx);
                }
            }
            PageKind::Date => {}
        }
    }

    fn a11y_action(&mut self, el: &ActiveEventLoop, req: accesskit::ActionRequest) {
        let target = req.target_node.0;
        match req.action {
            accesskit::Action::Click => {
                if let Some(m) = self
                    .hits
                    .iter()
                    .rev()
                    .find(|h| h.id == target)
                    .and_then(|h| h.msg.clone())
                {
                    self.update(el, m);
                } else if self.field(target).is_some() {
                    self.input.focus = Some(target);
                }
            }
            accesskit::Action::Focus => {
                self.input.focus = Some(target);
                self.input.focus_visible = true;
                self.sync_ime();
            }
            _ => {}
        }
        self.redraw();
    }
}

fn translate_key(ev: &winit::event::KeyEvent, mods: ModifiersState) -> Option<KeyPress> {
    let key = match &ev.logical_key {
        WKey::Named(n) => Key::Named(match n {
            NamedKey::Enter => Named::Enter,
            NamedKey::Escape => Named::Escape,
            NamedKey::Backspace => Named::Backspace,
            NamedKey::Delete => Named::Delete,
            NamedKey::Insert => Named::Insert,
            NamedKey::Tab => Named::Tab,
            NamedKey::Home => Named::Home,
            NamedKey::End => Named::End,
            NamedKey::PageUp => Named::PageUp,
            NamedKey::PageDown => Named::PageDown,
            NamedKey::ArrowUp => Named::Up,
            NamedKey::ArrowDown => Named::Down,
            NamedKey::ArrowLeft => Named::Left,
            NamedKey::ArrowRight => Named::Right,
            NamedKey::Space => Named::F(0),
            other => {
                let f = format!("{other:?}");
                let n: u8 = f.strip_prefix('F')?.parse().ok()?;
                Named::F(n)
            }
        }),
        WKey::Character(s) => Key::Char(s.chars().next()?),
        _ => return None,
    };
    // Space arrives as a named key; treat it as the character it types.
    let key = if key == Key::Named(Named::F(0)) {
        Key::Char(' ')
    } else {
        key
    };
    Some(KeyPress {
        key,
        ctrl: mods.control_key(),
        shift: mods.shift_key(),
        alt: mods.alt_key(),
        keypad: ev.location == KeyLocation::Numpad,
    })
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let (w, h) = {
            let d = self.store.data.borrow();
            (d.width.max(300) as f64, d.height.max(400) as f64)
        };
        let attrs = Window::default_attributes()
            .with_title(APP_NAME)
            .with_inner_size(LogicalSize::new(w, h))
            .with_min_inner_size(LogicalSize::new(300.0, 400.0))
            .with_decorations(false)
            .with_visible(false);
        let attrs =
            winit::platform::wayland::WindowAttributesExtWayland::with_name(attrs, APP_ID, "dgmnb");
        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("dgmnb: could not open a window: {e}");
                el.exit();
                return;
            }
        };
        let adapter =
            accesskit_winit::Adapter::with_event_loop_proxy(el, &window, self.proxy.clone());
        let surface = softbuffer::Context::new(window.clone())
            .and_then(|ctx| softbuffer::Surface::new(&ctx, window.clone()));
        let surface = match surface {
            Ok(s) => s,
            Err(e) => {
                eprintln!("dgmnb: no drawing surface: {e}");
                el.exit();
                return;
            }
        };
        {
            use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
            if let Ok(h) = window.display_handle()
                && let RawDisplayHandle::Wayland(h) = h.as_raw()
            {
                // SAFETY: the display is winit's live connection, which
                // outlives the clipboard worker (both end with the process).
                self.clipboard = unsafe { Clipboard::new(h.display.as_ptr()) };
            }
        }
        window.set_visible(true);
        self.gfx = Some(Gfx {
            window,
            surface,
            adapter,
        });
        let now = Instant::now();
        if self.dev.screenshot.is_some() {
            let delay = env("SCREENSHOT_DELAY_MS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(700);
            self.dev.deadline = Some(now + Duration::from_millis(delay));
        }
        if let Some(ms) = env("AUTOCLOSE_MS").and_then(|v| v.parse::<u64>().ok()) {
            self.dev.autoclose = Some(now + Duration::from_millis(ms));
        }
        self.redraw();
    }

    fn new_events(&mut self, el: &ActiveEventLoop, cause: StartCause) {
        if !matches!(
            cause,
            StartCause::ResumeTimeReached { .. } | StartCause::WaitCancelled { .. }
        ) {
            return;
        }
        let now = Instant::now();
        let before = self.toasts.len();
        self.toasts.retain(|t| t.1 > now);
        if self.toasts.len() != before {
            self.redraw();
        }
        if let Some(d) = self.dev.deadline
            && now >= d
        {
            self.dev.deadline = None;
            if let Some(p) = self.dev.screenshot.clone() {
                self.screenshot(&p);
                el.exit();
            }
        }
        if self.dev.autoclose.is_some_and(|d| now >= d) {
            self.save();
            el.exit();
        }
        let _ = el;
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.dev.frames > self.dev.clicked_at
            && let Some((x, y)) = self.dev.clicks.pop_front()
        {
            self.dev.clicked_at = self.dev.frames;
            self.pointer_moved(x, y);
            self.pointer_pressed(el, x, y);
            self.pointer_released(el);
            self.redraw();
        }
        if !self.dev.started && self.dev.frames > 0 {
            self.dev.started = true;
            if let Some(keys) = self.dev.keys.take() {
                for kp in input::parse_key_script(&keys) {
                    let text = kp.text().map(String::from);
                    self.key_press(el, kp, text);
                }
            }
        }
        // Sleep until the next timer (toast expiry, dev hooks) or event.
        let next = self
            .toasts
            .iter()
            .map(|t| t.1)
            .chain(self.dev.deadline)
            .chain(self.dev.autoclose)
            .min();
        el.set_control_flow(match next {
            Some(t) => ControlFlow::WaitUntil(t),
            None => ControlFlow::Wait,
        });
    }

    fn user_event(&mut self, el: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::AccessKit(e) => match e.window_event {
                accesskit_winit::WindowEvent::InitialTreeRequested => {
                    self.a11y = true;
                    self.redraw();
                }
                accesskit_winit::WindowEvent::ActionRequested(req) => self.a11y_action(el, req),
                accesskit_winit::WindowEvent::AccessibilityDeactivated => self.a11y = false,
            },
            UserEvent::Desktop(d) => {
                self.desktop = d;
                self.retheme();
                self.redraw();
            }
            UserEvent::Currency(result) => {
                if let Some(c) = self.conv.as_mut() {
                    c.currency_fetched(*result);
                }
                self.redraw();
            }
            UserEvent::Analysis(seq, features) => {
                if let Some(g) = self.graph.as_mut() {
                    g.analysis_done(seq, *features);
                }
                self.redraw();
            }
            UserEvent::Plot(seq, plots, ms) => {
                let (mut cx, _, _, _, graph) = self.cx_parts();
                if let Some(g) = graph.as_mut() {
                    g.plot_done(seq, plots, ms, &mut cx);
                }
                self.redraw();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if let Some(g) = self.gfx.as_mut() {
            g.adapter.process_event(&g.window, &event);
        }
        let scale = self.gfx.as_ref().map_or(1.0, |g| g.window.scale_factor()) as f32;
        match event {
            WindowEvent::CloseRequested => {
                self.save();
                el.exit();
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => self.redraw(),
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer_moved(position.x as f32 / scale, position.y as f32 / scale)
            }
            WindowEvent::CursorLeft { .. } => {
                self.input.pointer = None;
                if let Some(g) = self.graph.as_mut() {
                    g.pointer_left();
                }
                self.update_hover();
                self.redraw();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some((x, y)) = self.input.pointer else {
                    return;
                };
                match (button, state) {
                    (MouseButton::Left, ElementState::Pressed) => self.pointer_pressed(el, x, y),
                    (MouseButton::Left, ElementState::Released) => self.pointer_released(el),
                    (MouseButton::Right, ElementState::Pressed) => {
                        if self.mode.page() == PageKind::Calculator
                            && !self.settings
                            && let Some(c) = self.calc.as_mut()
                        {
                            let hit = self.hits.iter().rev().find(|h| h.rect.contains(x, y));
                            if hit.is_some_and(|h| h.id == id("display")) {
                                c.popup = Some(calc::Popup::DisplayMenu(x, y));
                                self.redraw();
                            }
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::Touch(t) => {
                let (x, y) = (t.location.x as f32 / scale, t.location.y as f32 / scale);
                match t.phase {
                    winit::event::TouchPhase::Started => {
                        self.pointer_moved(x, y);
                        self.pointer_pressed(el, x, y);
                    }
                    winit::event::TouchPhase::Moved => self.pointer_moved(x, y),
                    _ => {
                        self.pointer_released(el);
                        self.input.pointer = None;
                        self.update_hover();
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some((x, y)) = self.input.pointer else {
                    return;
                };
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, l) => l * 48.0,
                    MouseScrollDelta::PixelDelta(PhysicalPosition { y, .. }) => y as f32 / scale,
                };
                self.wheel(dy, x, y);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                let Some(kp) = translate_key(&event, self.mods) else {
                    return;
                };
                let text = event.text.as_ref().map(|t| t.to_string());
                self.key_press(el, kp, text);
            }
            WindowEvent::Ime(ime) => {
                let Some(id) = self.input.focus else { return };
                if let Some(e) = self.field(id) {
                    match ime {
                        Ime::Preedit(s, _) => e.preedit = s,
                        Ime::Commit(s) => {
                            e.preedit.clear();
                            e.insert(&s);
                            self.field_changed(id);
                        }
                        _ => {}
                    }
                    self.redraw();
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Chrome: header, navigation, settings
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn draw_header(
    f: &mut Frame,
    r: Rect,
    mode: ViewMode,
    settings: bool,
    compact: bool,
    maximized: bool,
    wide: bool,
    calc: Option<&CalcPage>,
    graph: Option<&GraphPage>,
) {
    let t = f.t;
    let r = r.inset_xy(6.0, 5.0);
    let mut x = r.x;
    let btn =
        |f: &mut Frame, x: &mut f32, d: &'static str, name: &str, msg: Msg, on: Option<bool>| {
            let b = Rect::new(*x, r.y, 36.0, 36.0);
            f.icon_button(id(("hdr", name)), b, d, name, msg, true, on);
            *x += 38.0;
        };
    if compact {
        btn(
            f,
            &mut x,
            appcore::icons::BACK_TO_FULL,
            "Back to full view (Alt+Down)",
            Msg::Compact(false),
            None,
        );
    } else if settings {
        btn(f, &mut x, glyph::BACK, "Back", Msg::Settings(false), None);
        f.label(
            Rect::new(x + 6.0, r.y, 200.0, 36.0),
            "Settings",
            TITLE,
            t.fg,
            Align::Start,
        );
    } else {
        btn(
            f,
            &mut x,
            appcore::icons::MENU,
            "Open navigation",
            Msg::Nav(true),
            None,
        );
        f.label(
            Rect::new(x + 6.0, r.y, 220.0, 36.0),
            mode.title(),
            TITLE,
            t.fg,
            Align::Start,
        );
    }
    // Window controls on the right, page actions before them.
    let mut rx = r.right();
    let rbtn =
        |f: &mut Frame, rx: &mut f32, d: &'static str, name: &str, msg: Msg, on: Option<bool>| {
            *rx -= 36.0;
            let b = Rect::new(*rx, r.y, 36.0, 36.0);
            f.icon_button(id(("hdr", name)), b, d, name, msg, true, on);
            *rx -= 2.0;
        };
    rbtn(f, &mut rx, appcore::icons::CLOSE, "Close", Msg::Close, None);
    if !compact {
        rbtn(
            f,
            &mut rx,
            if maximized {
                glyph::RESTORE
            } else {
                glyph::MAXIMIZE
            },
            if maximized { "Restore" } else { "Maximize" },
            Msg::Maximize,
            None,
        );
        rbtn(f, &mut rx, glyph::MINIMIZE, "Minimize", Msg::Minimize, None);
        rx -= 8.0;
    }
    if !settings && !compact {
        match mode {
            ViewMode::Standard => {
                if let Some(c) = calc
                    && c.wants_history_button(wide)
                {
                    rbtn(
                        f,
                        &mut rx,
                        appcore::icons::HISTORY,
                        "History (Ctrl+H)",
                        Msg::Calc(calc::Msg::Popup(Some(calc::Popup::Panel))),
                        None,
                    );
                }
                rbtn(
                    f,
                    &mut rx,
                    appcore::icons::KEEP_ON_TOP,
                    "Keep on top (Alt+Up)",
                    Msg::Compact(true),
                    None,
                );
            }
            ViewMode::Scientific => {
                if let Some(c) = calc
                    && c.wants_history_button(wide)
                {
                    rbtn(
                        f,
                        &mut rx,
                        appcore::icons::HISTORY,
                        "History (Ctrl+H)",
                        Msg::Calc(calc::Msg::Popup(Some(calc::Popup::Panel))),
                        None,
                    );
                }
            }
            ViewMode::Graphing => {
                if let Some(g) = graph
                    && let Some(showing_graph) = g.narrow_toggle(f.width())
                {
                    rbtn(
                        f,
                        &mut rx,
                        appcore::icons::GRAPHING,
                        "Graph (Ctrl+Home)",
                        Msg::Graph(graph::Msg::ShowGraph(true)),
                        Some(showing_graph),
                    );
                    rbtn(
                        f,
                        &mut rx,
                        appcore::icons::FUNCTION,
                        "Equations",
                        Msg::Graph(graph::Msg::ShowGraph(false)),
                        Some(!showing_graph),
                    );
                }
            }
            _ => {}
        }
    }
    // Everything else in the header drags the window.
    let _ = rx;
}

fn draw_nav(f: &mut Frame, full: Rect, mode: ViewMode, settings: bool) {
    let t = f.t;
    f.scrim(Msg::Nav(false), true);
    let panel = Rect::new(0.0, 0.0, NAV_W.min(full.w - 40.0), full.h);
    f.cv.fill_rect(panel, t.bg);
    // Blank parts of the drawer don't close it.
    f.hit(id("nav-panel"), panel, Sense::Click, None, false);
    f.cv.fill_rect(Rect::new(panel.right() - 1.0, 0.0, 1.0, full.h), t.border);
    f.group(id("nav"), accesskit::Role::Navigation, "Navigation", panel);
    let (head, rest) = panel.take_top(HEADER_H);
    let head = head.inset_xy(6.0, 5.0);
    f.icon_button(
        id("nav-close"),
        Rect::new(head.x, head.y, 36.0, 36.0),
        appcore::icons::MENU,
        "Close navigation",
        Msg::Nav(false),
        true,
        None,
    );
    f.label(
        Rect::new(head.x + 48.0, head.y, 200.0, 36.0),
        APP_NAME,
        TITLE,
        t.fg,
        Align::Start,
    );
    let (list, foot) = rest.take_bottom(52.0);
    let (foot, list) = (list, foot);
    let sid = id("nav-scroll");
    let off = f.scroll_begin(sid, list);
    let mut y = list.y - off + 4.0;
    let mut group = None;
    for m in ViewMode::ALL {
        if group != Some(m.group()) {
            group = Some(m.group());
            let label = match m.group() {
                Group::Calculator => "Calculator",
                Group::Converter => "Converter",
            };
            f.label(
                Rect::new(list.x + 18.0, y, 200.0, 28.0),
                label,
                CAPTION,
                t.fg_dim,
                Align::Start,
            );
            y += 28.0;
        }
        let row = Rect::new(list.x + 6.0, y, list.w - 12.0, 40.0);
        let sel = m == mode && !settings;
        f.row(id(("nav", m.key())), row, Msg::Mode(m), sel, m.title());
        f.icon(
            Rect::new(row.x + 8.0, row.y, 28.0, row.h),
            m.icon(),
            18.0,
            if sel { t.accent_text } else { t.fg },
        );
        f.label(
            Rect::new(row.x + 44.0, row.y, row.w - 50.0, row.h),
            m.title(),
            BODY,
            if sel { t.accent_text } else { t.fg },
            Align::Start,
        );
        y += 42.0;
    }
    let content = y + off - list.y + 4.0;
    f.scroll_end(sid, list, content);
    let row = foot.inset_xy(6.0, 6.0);
    f.row(
        id("nav-settings"),
        row,
        Msg::Settings(true),
        settings,
        "Settings",
    );
    f.icon(
        Rect::new(row.x + 8.0, row.y, 28.0, row.h),
        appcore::icons::SETTINGS,
        18.0,
        t.fg,
    );
    f.label(
        Rect::new(row.x + 44.0, row.y, row.w - 50.0, row.h),
        "Settings",
        BODY,
        t.fg,
        Align::Start,
    );
    f.end_group();
}

fn draw_settings(f: &mut Frame, body: Rect, s: &Settings, desktop: Desktop) {
    let t = f.t;
    let sid = id("settings-scroll");
    let area = body.inset_xy(0.0, 0.0);
    let off = f.scroll_begin(sid, area);
    let col_w = area.w.min(520.0) - 32.0;
    let x = area.x + (area.w - col_w) / 2.0;
    let mut y = area.y + 8.0 - off;
    let heading = |f: &mut Frame, y: &mut f32, s: &str| {
        f.label(Rect::new(x, *y, col_w, 30.0), s, STRONG, t.fg, Align::Start);
        *y += 34.0;
    };
    heading(f, &mut y, "Appearance");
    let card = Rect::new(x, y, col_w, 116.0);
    f.cv.rounded(card, 12.0, t.surface);
    f.label(
        Rect::new(x + 14.0, y + 8.0, col_w, 24.0),
        "Style",
        SMALL,
        t.fg_dim,
        Align::Start,
    );
    let seg = Rect::new(x + 12.0, y + 34.0, col_w - 24.0, 34.0);
    for (i, (key, label)) in [("system", "System"), ("light", "Light"), ("dark", "Dark")]
        .into_iter()
        .enumerate()
    {
        let c = seg.cell(1, 3, 0, i, 4.0);
        f.button(
            id(("theme", key)),
            c,
            label,
            BODY,
            Msg::Theme(key),
            true,
            Some(s.theme == key),
            true,
        );
    }
    let row = Rect::new(x + 12.0, y + 76.0, col_w - 24.0, 34.0);
    let accent_ok = desktop.accent.is_some();
    let label = if accent_ok {
        "Use the desktop's accent colour"
    } else {
        "Use the desktop's accent colour (none shared)"
    };
    f.label(
        row.inset_xy(2.0, 0.0),
        label,
        BODY,
        if accent_ok { t.fg } else { t.fg_dim },
        Align::Start,
    );
    let sw = Rect::new(row.right() - 52.0, row.y + 5.0, 46.0, 24.0);
    let on = s.system_accent && accent_ok;
    f.cv.rounded(sw, 12.0, if on { t.accent } else { t.border });
    f.cv.circle(
        if on { sw.right() - 12.0 } else { sw.x + 12.0 },
        sw.cy(),
        9.0,
        if on { t.on_accent } else { t.surface },
    );
    if accent_ok {
        let sid2 = id("accent-switch");
        f.hit(
            sid2,
            row,
            Sense::Click,
            Some(Msg::SystemAccent(!s.system_accent)),
            true,
        );
        if let Some(n) = f.node(
            sid2,
            accesskit::Role::Switch,
            "Use the desktop's accent colour",
            row,
        ) {
            n.toggled = Some(on);
            n.clickable = true;
            n.focusable = true;
        }
    }
    y += 116.0 + 20.0;

    heading(f, &mut y, "About");
    const ABOUT: &str = "The lean twin of GMNB: the Windows Calculator engine ported to Rust, drawn in software with nothing running while it waits. Not affiliated with or endorsed by Microsoft.";
    let text_h = {
        let lines = f.wrap(ABOUT, col_w - 28.0, SMALL);
        lines.len() as f32 * (SMALL.size * 1.4).round()
    };
    let card = Rect::new(x, y, col_w, 136.0 + text_h);
    f.cv.rounded(card, 12.0, t.surface);
    let inner = card.inset(14.0);
    f.label(
        Rect::new(inner.x, inner.y, inner.w, 28.0),
        "DGMNB",
        Style::new(22.0, 700.0),
        t.fg,
        Align::Start,
    );
    f.label(
        Rect::new(inner.x, inner.y + 28.0, inner.w, 22.0),
        "Don't Glaze My Numbers, Baby",
        BODY,
        t.fg_dim,
        Align::Start,
    );
    f.label(
        Rect::new(inner.x, inner.y + 50.0, inner.w, 22.0),
        &format!("Version {}", env!("CARGO_PKG_VERSION")),
        SMALL,
        t.fg_dim,
        Align::Start,
    );
    f.paragraph(inner.x, inner.y + 78.0, inner.w, ABOUT, SMALL, t.fg);
    let b = Rect::new(inner.x, inner.bottom() - 36.0, 150.0, 34.0);
    f.button(
        id("copy-site"),
        b,
        "Copy website link",
        SMALL,
        Msg::CopyLink("https://github.com/Go08er/GlazeMyNumbersBaby"),
        true,
        None,
        true,
    );
    let b2 = Rect::new(b.right() + 8.0, b.y, 110.0, 34.0);
    f.button(
        id("licences"),
        b2,
        "Licences",
        SMALL,
        Msg::Licences(true),
        true,
        None,
        true,
    );
    y += card.h + 16.0;
    f.scroll_end(sid, area, y + off - area.y);
}

const LICENCES: &str = concat!(
    "DGMNB is MIT licensed. It is a port of Microsoft's Windows Calculator.\n\n",
    include_str!("../../../LICENSE"),
    "\n\n— Inter —\n\n",
    include_str!("../assets/fonts/OFL-Inter.txt"),
    "\n\n— Noto Sans (Math, Arabic, Armenian, Bengali, Khmer subsets) —\n\n",
    include_str!("../assets/fonts/OFL-Noto.txt"),
    "\n\nExchange rates: Frankfurter (central bank reference rates)."
);

fn draw_licences(f: &mut Frame, full: Rect) {
    let t = f.t;
    f.scrim(Msg::Licences(false), true);
    let r = full.inset(18.0);
    f.card(r, 14.0);
    let (head, body) = r.inset(10.0).take_top(40.0);
    f.label(
        head.inset_xy(6.0, 0.0),
        "Licences",
        TITLE,
        t.fg,
        Align::Start,
    );
    f.icon_button(
        id("lic-close"),
        Rect::new(head.right() - 36.0, head.y + 2.0, 36.0, 36.0),
        appcore::icons::CLOSE,
        "Close",
        Msg::Licences(false),
        true,
        None,
    );
    let sid = id("lic-scroll");
    let off = f.scroll_begin(sid, body);
    let h = f.paragraph(
        body.x + 4.0,
        body.y - off,
        body.w - 16.0,
        LICENCES,
        SMALL,
        t.fg,
    );
    f.scroll_end(sid, body, h);
    if let Some(n) = f.node(id("lic-text"), accesskit::Role::Document, "Licences", body) {
        n.value = Some(LICENCES.to_string());
    }
}
